use clap::{arg, ArgAction};

use libscail::{
    background::{BackgroundContext, BackgroundTask},
    dir, dump_sys_info, get_user_home_dir,
    output::{Parametrize, Timestamp},
    set_kernel_printk_level, with_shell,
    workloads::{gen_perf_command_prefix, RedisWorkloadConfig, TasksetCtxBuilder, TasksetCtxInterleaving,
    YcsbConfig, YcsbDistribution, YcsbSession, YcsbSystem, YcsbWorkload},
    Login, ScailError,
};

use serde::{Deserialize, Serialize};

use spurs::{cmd, Execute, SshShell, SshSpawnHandle};
use spurs_util::escape_for_bash;

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
enum Workload {
    Merci {
        runs: u64,
        cores: Option<usize>,
        delay: Option<u64>,
    },
    GapbsTc {
        runs: u64,
    },
    GapbsPr {
        runs: u64,
    },
    Gups {
        threads: usize,
        exp: usize,
        hot_exp: usize,
        num_updates: usize,
    },
    CloverLeaf {
        threads: usize,
        delay: Option<usize>,
    },
    Redis {
        server_size_mb: usize,
        op_count: usize,
        load_before_wklds: bool,
    },
    Stream,
    SpecBwaves {
        threads: usize,
    },
    SpecLbm {
        threads: usize,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
enum Strategy {
    Tpp,
    Colloid,
    Bwmfs { ratios: Vec<(usize, usize)> },
    Numactl { local: usize, remote: usize },
    Cipp { total_bw: bool },
    Linux,
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
enum ThrottleType {
    Quartz { bw: u64 },
    Msr,
    Native,
}

#[derive(Debug, Clone, Serialize, Deserialize, Parametrize)]
struct Config {
    #[name]
    exp: String,

    #[name]
    workloads: Vec<Workload>,
    #[name]
    strategy: Strategy,

    kill_after_first_done: bool,
    perf_stat: bool,
    perf_counters: Vec<String>,
    disable_thp: bool,
    disable_aslr: bool,
    flame_graph: bool,
    bwmon: bool,
    meminfo: bool,
    memlat: bool,
    time: bool,
    throttle: ThrottleType,

    #[timestamp]
    timestamp: Timestamp,
}

pub fn cli_options() -> clap::Command {
    clap::Command::new("cipp_exp")
        .about("Run an experiment for cipp")
        .arg_required_else_help(true)
        .disable_version_flag(true)
        .arg(arg!(<hostname> "The domain name of the remote"))
        .arg(arg!(<username> "The username on the remote"))
        .arg(arg!(--perf_stat "Record counters with perf stat").action(ArgAction::SetTrue))
        .arg(
            arg!(--perf_counter "Which counters to record with perf stat")
                .action(ArgAction::Append)
                .requires("perf_stat"),
        )
        .arg(arg!(--disable_thp "Disable THP completely.").action(ArgAction::SetTrue))
        .arg(arg!(--disable_aslr "Disable ASLR.").action(ArgAction::SetTrue))
        .arg(arg!(--tpp "Use TPP").action(ArgAction::SetTrue))
        .arg(arg!(--colloid "Use Colloid").action(ArgAction::SetTrue).conflicts_with("tpp"))
        .arg(arg!(--bwmfs <RATIO> "Use BWMFS with the specified local:remote ratio")
            .action(ArgAction::Append).conflicts_with("colloid").conflicts_with("tpp"))
        .arg(arg!(--numactl <RATIO> "Use numactl weighted interleave with the specified local:remote ratio")
            .conflicts_with("colloid").conflicts_with("tpp").conflicts_with("bwmfs"))
        .arg(arg!(--cipp "Use CIPP")
            .action(ArgAction::SetTrue).conflicts_with("colloid").conflicts_with("tpp").conflicts_with("bwmfs").conflicts_with("numactl"))
        .arg(arg!(--cipp_total_bw "Use the total BW varient of CIPP")
            .action(ArgAction::SetTrue).requires("cipp"))
        .arg(arg!(--memlat "Use memlat with Colloid")
            .action(ArgAction::SetTrue).requires("colloid"))
        .arg(
            arg!(--flame_graph "Generate a flame graph of the workload.")
                .action(ArgAction::SetTrue),
        )
        .arg(
            arg!(--bwmon "Record memory bandwidth during the experiment")
                .action(ArgAction::SetTrue),
        )
        .arg(
            arg!(--meminfo "Periodically print the local/remote memory breakdown")
                .action(ArgAction::SetTrue),
        )
        .arg(
            arg!(--time "Run the workloads with GNU time")
                .action(ArgAction::SetTrue)
        )
        .arg(
            arg!(--quartz <QUARTZ_BW> "Use Quartz to limit the memory bandwidth (MB/s)")
                .value_parser(clap::value_parser!(u64))
                .conflicts_with("msr_throttle"),
        )
        .arg(
            arg!(--msr_throttle "Write to the MSR_UNCORE_RATIO_LIMIT register to lower uncore frequency")
                .action(ArgAction::SetTrue)
                .conflicts_with("quartz"),
        )
        .subcommand(
            clap::Command::new("merci")
                .about("Run the MERCI workload")
                .arg(
                    arg!([runs]
            "The number of iterations of MERCI to run. Default: 10")
                    .value_parser(clap::value_parser!(u64)),
                )
                .arg(
                    arg!(--threads <THREADS> "The number of threads to run with")
                    .value_parser(clap::value_parser!(usize))
                ),
        )
        .subcommand(
            clap::Command::new("gapbs_tc")
                .about("Run the GAPBS tc workload")
                .arg(
                    arg!([runs]
            "The number of iterations of tc to run. Default: 10")
                    .value_parser(clap::value_parser!(u64)),
                ),
        )
        .subcommand(
            clap::Command::new("gapbs_pr")
                .about("Run the GAPBS pr workloads")
                .arg(
                    arg!([runs] "The number of iterations of pr to run. Default: 10")
                    .value_parser(clap::value_parser!(u64))
                ),
        )
        .subcommand(
            clap::Command::new("gups")
                .about("Run the GUPS workload")
                .arg(
                    arg!(--threads <THREADS> "The number of threads to run with")
                        .value_parser(clap::value_parser!(usize))
                )
                .arg(
                    arg!(--exp <exp> "The log of the size of the workload")
                        .value_parser(clap::value_parser!(usize))
                        .required(true)
                )
                .arg(
                    arg!(--hot_exp <hot_exp> "The log of the size of the hot region, if there is one")
                        .value_parser(clap::value_parser!(usize))
                        .required(true)
                )
                .arg(
                    arg!(--updates <updates> "The number of updates to do. Default is 2^exp / 8")
                )
        )
        .subcommand(
            clap::Command::new("clover")
                .about("Run the CloverLeaf workload")
                .arg(
                    arg!(--threads <THREADS> "The number of threads to run with")
                        .value_parser(clap::value_parser!(usize))
                )
        )
        .subcommand(
            clap::Command::new("redis")
                .about("Run the redis workload")
                .arg(
                    arg!(--server_size <SERVER_SIZE> "The size of the server in GB")
                        .value_parser(clap::value_parser!(usize))
                )
                .arg(
                    arg!(--op_count <OP_COUNT> "The number of read operations to use")
                        .value_parser(clap::value_parser!(usize))
                )
        )
        .subcommand(
            clap::Command::new("stream")
                .about("Run the STREAM microbenchmark")
        )
        .subcommand(
            clap::Command::new("bwaves")
                .about("Run the SPEC 2017 bwaves_s benchmark.")
                .arg(
                    arg!(--threads <THREADS> "The number of threads to run with")
                        .value_parser(clap::value_parser!(usize))
                )
        )
        .subcommand(
            clap::Command::new("lbm")
                .about("Run the SPEC 2017 lbm_s benchmark.")
                .arg(
                    arg!(--threads <THREADS> "The number of threads to run with")
                        .value_parser(clap::value_parser!(usize))
                )
        )
        .subcommand(
            clap::Command::new("merci_tc")
                .about("Run the MERCI and GAPBS tc workload together")
                .arg(
                    arg!([runs]
            "The number of iterations of tc to run. Default: 10")
                    .value_parser(clap::value_parser!(u64)),
                ),
        )
        .subcommand(
            clap::Command::new("double_merci")
                .about("Run two instances of the MERCI workload together, with one offset before the other")
                .arg(
                    arg!([runs]
            "The number of iterations of MERCI to run. Default: 10")
                    .value_parser(clap::value_parser!(u64))
                )
                .arg(
                    arg!([cores]
            "The number of cores to run each instance of MERCI.")
                    .value_parser(clap::value_parser!(usize))
                )
                .arg(
                    arg!(--delay <DELAY> "The delay, in seconds, to start the second instance after the first")
                    .value_parser(clap::value_parser!(u64))
                )
        )
        .subcommand(
            clap::Command::new("double_clover")
                .about("Run two instances of the CloverLeaf workload together, with one offset before the other")
                .arg(
                    arg!(--threads <THREADS> "The number of threads to run with")
                        .value_parser(clap::value_parser!(usize))
                )
                .arg(
                    arg!(--delay <DELAY> "The delay, in seconds, to start the second instance after the first")
                    .value_parser(clap::value_parser!(usize))
                )
        )
        .subcommand(
            clap::Command::new("gups_redis")
                .about("Run single threaded GUPS and Redis together")
                .arg(
                    arg!(--server_size <SERVER_SIZE> "The size of the server in GB")
                        .value_parser(clap::value_parser!(usize))
                )
                .arg(
                    arg!(--op_count <OP_COUNT> "The number of read operations to use")
                        .value_parser(clap::value_parser!(usize))
                )
                .arg(
                    arg!(--exp <exp> "The log of the size of the workload")
                        .value_parser(clap::value_parser!(usize))
                        .required(true)
                )
                .arg(
                    arg!(--hot_exp <hot_exp> "The log of the size of the hot region, if there is one")
                        .value_parser(clap::value_parser!(usize))
                        .required(true)
                )
        )
}

fn get_remote_start_addr(ushell: &SshShell) -> Result<usize, ScailError> {
    let mut found_node1 = false;
    let zoneinfo_output = ushell.run(cmd!("cat /proc/zoneinfo"))?.stdout;
    for line in zoneinfo_output.lines() {
        if line == "Node 1, zone   Normal" {
            found_node1 = true;
            continue;
        }
        if !found_node1 {
            continue;
        }

        if line.contains("start_pfn:") {
            let mut split = line.trim().split(':');
            split.next();
            let pfn = split
                .next()
                .unwrap()
                .trim()
                .parse::<usize>()
                .expect("Expected integer");

            // Convert pfn to address
            return Ok(pfn * 4096);
        }
    }

    Err(ScailError::InvalidValueError { msg: "Could not find remote memory start".to_string() })
}

pub fn run(sub_m: &clap::ArgMatches) -> Result<(), failure::Error> {
    let username = sub_m.get_one::<String>("username").unwrap().clone();
    let host = sub_m.get_one::<String>("hostname").unwrap().clone();
    let login = Login {
        username: username.as_str(),
        hostname: host.as_str(),
        host: host.as_str(),
    };

    let perf_stat = sub_m.get_flag("perf_stat");
    let perf_counters = sub_m.get_many("perf_counter").map_or(
        Vec::new(),
        |counters: clap::parser::ValuesRef<'_, String>| counters.map(Into::into).collect(),
    );
    let disable_thp = sub_m.get_flag("disable_thp");
    let disable_aslr = sub_m.get_flag("disable_aslr");
    let tpp = sub_m.get_flag("tpp");
    let colloid = sub_m.get_flag("colloid");
    let memlat = sub_m.get_flag("memlat");
    let parse_ratio = |r: &String| {
        let expect_msg =
            "--bwmfs or --numactl should be of the format <local weight>:<remote weight>";
        let mut split = r.split(":");
        let local = split
            .next()
            .expect(expect_msg)
            .parse::<usize>()
            .expect(expect_msg);
        let remote = split
            .next()
            .expect(expect_msg)
            .parse::<usize>()
            .expect(expect_msg);

        if split.count() != 0 {
            panic!("{}", expect_msg);
        }

        (local, remote)
    };
    let bwmfs_ratios = sub_m
        .get_many("bwmfs")
        .map_or(Vec::new(), |ratios: clap::parser::ValuesRef<'_, String>| {
            ratios.map(parse_ratio).collect()
        });
    let numactl_ratio = sub_m.get_one("numactl").map(parse_ratio);
    let mut kill_after_first_done = true;
    let cipp = sub_m.get_flag("cipp");
    let cipp_total_bw = sub_m.get_flag("cipp_total_bw");
    let flame_graph = sub_m.get_flag("flame_graph");
    let bwmon = sub_m.get_flag("bwmon");
    let meminfo = sub_m.get_flag("meminfo");
    let time = sub_m.get_flag("time");
    let quartz_bw = sub_m.get_one::<u64>("quartz").copied();
    let msr_throttle = sub_m.get_flag("msr_throttle");

    let workloads = match sub_m.subcommand() {
        Some(("merci", sub_m)) => {
            let runs = *sub_m.get_one::<u64>("runs").unwrap_or(&10);
            let cores = sub_m.get_one::<usize>("threads").copied();
            vec![Workload::Merci {
                runs,
                cores,
                delay: None,
            }]
        }
        Some(("gapbs_tc", sub_m)) => {
            let runs = *sub_m.get_one::<u64>("runs").unwrap_or(&10);
            vec![Workload::GapbsTc { runs }]
        }
        Some(("gapbs_pr", sub_m)) => {
            let runs = *sub_m.get_one::<u64>("runs").unwrap_or(&10);
            vec![Workload::GapbsPr { runs }]
        }
        Some(("gups", sub_m)) => {
            let threads = *sub_m.get_one::<usize>("threads").unwrap_or(&1);
            let exp = *sub_m.get_one::<usize>("exp").unwrap();
            let hot_exp = *sub_m.get_one::<usize>("hot_exp").unwrap();
            let num_updates = sub_m.get_one::<usize>("updates").copied().unwrap_or((1 << exp) / 8);

            vec![Workload::Gups {threads, exp, hot_exp, num_updates}]
        }
        Some(("clover", sub_m)) => {
            let threads = *sub_m.get_one::<usize>("threads").unwrap_or(&10);

            vec![Workload::CloverLeaf { threads, delay: None }]
        }
        Some(("redis", sub_m)) => {
            let server_size_mb = *sub_m.get_one::<usize>("server_size").unwrap() << 10;
            let op_count = *sub_m.get_one::<usize>("op_count").unwrap();

            vec![Workload::Redis { server_size_mb, op_count, load_before_wklds: false }]
        }
        Some(("stream", _)) => {
            vec![Workload::Stream]
        }
        Some(("bwaves", sub_m)) => {
            let threads = *sub_m.get_one::<usize>("threads").unwrap_or(&10);

            vec![Workload::SpecBwaves { threads } ]
        }
        Some(("lbm", sub_m)) => {
            let threads = *sub_m.get_one::<usize>("threads").unwrap_or(&10);

            vec![Workload::SpecLbm { threads } ]
        }
        Some(("merci_tc", sub_m)) => {
            let tc_runs = *sub_m.get_one::<u64>("runs").unwrap_or(&10);
            let merci_runs = 100 * tc_runs;
            vec![
                Workload::GapbsTc { runs: tc_runs },
                Workload::Merci {
                    runs: merci_runs,
                    cores: None,
                    delay: None,
                },
            ]
        }
        Some(("double_merci", sub_m)) => {
            let runs = *sub_m.get_one::<u64>("runs").unwrap_or(&10);
            let cores = sub_m.get_one::<usize>("cores").copied();
            let delay = sub_m.get_one::<u64>("delay").copied();
            kill_after_first_done = false;
            vec![
                Workload::Merci {
                    runs,
                    cores,
                    delay: None,
                },
                Workload::Merci { runs, cores, delay },
            ]
        }
        Some(("double_clover", sub_m)) => {
            let threads = *sub_m.get_one::<usize>("threads").unwrap_or(&10);
            let delay = sub_m.get_one::<usize>("delay").copied();

            kill_after_first_done = false;
            vec![
                Workload::CloverLeaf { threads, delay: None },
                Workload::CloverLeaf { threads, delay },
            ]
        }
        Some(("gups_redis", sub_m)) => {
            let threads = 2;
            let exp = *sub_m.get_one::<usize>("exp").unwrap();
            let hot_exp = *sub_m.get_one::<usize>("hot_exp").unwrap();
            let num_updates = 5000000000;
            let server_size_mb = *sub_m.get_one::<usize>("server_size").unwrap() << 10;
            let op_count = *sub_m.get_one::<usize>("op_count").unwrap();

            kill_after_first_done = false;
            vec![
                Workload::Gups { threads, exp, hot_exp, num_updates },
                Workload::Redis { server_size_mb, op_count, load_before_wklds: false },
            ]
        }
        _ => unreachable!(),
    };

    let strategy = if tpp {
        Strategy::Tpp
    } else if colloid {
        Strategy::Colloid
    } else if bwmfs_ratios.len() != 0 {
        // Must have one ratio for each workload
        if bwmfs_ratios.len() != workloads.len() {
            panic!(
                "Must have exactly one BWMFS ratio for each workload ({})",
                workloads.len()
            );
        }

        Strategy::Bwmfs {
            ratios: bwmfs_ratios,
        }
    } else if let Some((local, remote)) = numactl_ratio {
        Strategy::Numactl { local, remote }
    } else if cipp {
        Strategy::Cipp { total_bw: cipp_total_bw }
    } else {
        Strategy::Linux
    };

    let throttle = if let Some(bw) = quartz_bw {
        ThrottleType::Quartz { bw }
    } else if msr_throttle {
        ThrottleType::Msr
    } else {
        ThrottleType::Native
    };

    let cfg = Config {
        exp: "cipp_exp".into(),
        workloads,
        strategy,
        kill_after_first_done,
        perf_stat,
        perf_counters,
        disable_thp,
        disable_aslr,
        flame_graph,
        bwmon,
        meminfo,
        memlat,
        time,
        throttle,
        timestamp: Timestamp::now(),
    };

    run_inner(&login, &cfg)
}

fn run_inner<A>(login: &Login<A>, cfg: &Config) -> Result<(), failure::Error>
where
    A: std::net::ToSocketAddrs + std::fmt::Display + std::fmt::Debug + Clone,
{
    let ushell = SshShell::with_any_key(login.username, &login.host)?;
    let user_home = get_user_home_dir(&ushell)?;

    // Setup the output filename
    let results_dir = dir!(&user_home, crate::RESULTS_PATH);

    let (_output_file, params_file, _time_file, _sim_file) = cfg.gen_standard_names();
    let perf_record_file = "/tmp/perf.data";
    let perf_stat_file = dir!(&results_dir, cfg.gen_file_name("perf_stat"));
    let flame_graph_file = dir!(&results_dir, cfg.gen_file_name("flamegraph.svg"));
    let colloid_lat_file = dir!(&results_dir, cfg.gen_file_name("colloid.lat"));
    let cipp_file = dir!(&results_dir, cfg.gen_file_name("cipp"));
    let bwmon_file = dir!(&results_dir, cfg.gen_file_name("bwmon"));
    let merci_file = dir!(&results_dir, cfg.gen_file_name("merci"));
    let gapbs_file = dir!(&results_dir, cfg.gen_file_name("gapbs"));
    let gups_file = dir!(&results_dir, cfg.gen_file_name("gups"));
    let clover_file = dir!(&results_dir, cfg.gen_file_name("clover"));
    let ycsb_file = dir!(&results_dir, cfg.gen_file_name("ycsb"));
    let stream_file = dir!(&results_dir, cfg.gen_file_name("stream"));
    let spec_file = dir!(&results_dir, cfg.gen_file_name("spec"));
    let vmstat_file = dir!(&results_dir, cfg.gen_file_name("vmstat"));
    let pgmigrate_file = dir!(&results_dir, cfg.gen_file_name("pgmigrate"));
    let damo_status_file = dir!(&results_dir, cfg.gen_file_name("damo_status"));
    let meminfo_file_stub = dir!(&results_dir, cfg.gen_file_name("meminfo"));
    let time_file_stub = dir!(&results_dir, cfg.gen_file_name("time"));

    let colloid_dir = dir!(&user_home, crate::KERNEL_PATH);
    let tools_dir = dir!(&user_home, crate::WKSPC_PATH, "tools/");
    let numactl_dir = dir!(&user_home, crate::WKSPC_PATH, "numactl/");
    let quartz_dir = dir!(&user_home, crate::WKSPC_PATH, "quartz/");
    let damo_dir = dir!(&user_home, "damo");
    let merci_dir = dir!(
        &user_home,
        crate::WORKLOADS_PATH,
        "MERCI/4_performance_evaluation/"
    );
    let gapbs_dir = dir!(&user_home, crate::WORKLOADS_PATH, "gapbs/");
    let gups_dir = dir!(&user_home, crate::WORKLOADS_PATH, "gups_hemem/");
    let clover_dir = dir!(&user_home, crate::WORKLOADS_PATH, "CloverLeaf/");
    let redis_dir = dir!(&user_home, crate::WORKLOADS_PATH, "redis/src/");
    let redis_conf = dir!(&user_home, crate::WKSPC_PATH, "redis.conf");
    let ycsb_dir = dir!(&user_home, crate::WORKLOADS_PATH, "YCSB/");
    let stream_dir = dir!(&user_home, crate::WORKLOADS_PATH, "stream/");
    let spec_dir = dir!(&user_home, crate::WORKLOADS_PATH, "spec2017/");
    let kernel_dir = dir!(&user_home, crate::KERNEL_PATH);

    isolate_remote_cores(&ushell)?;

    // Reboot to use new grubcfg with isolated cores
    let ushell = connect_and_setup_host(login)?;

    let remote_mem_start = get_remote_start_addr(&ushell)?;
    let mut tctx = TasksetCtxBuilder::from_lscpu(&ushell)?
        .numa_interleaving(TasksetCtxInterleaving::Sequential)
        .skip_hyperthreads(false)
        .group_hyperthreads(true)
        .build();
    let num_threads = tctx.num_threads_on_socket(0);
    let max_cores_per_wkld = num_threads / cfg.workloads.len();

    let mut bgctx = BackgroundContext::new(&ushell);

    ushell.run(cmd!("mkdir -p {}", results_dir))?;
    ushell.run(cmd!(
        "echo {} > {}",
        escape_for_bash(&serde_json::to_string(&cfg)?),
        dir!(&results_dir, params_file)
    ))?;

    // For now, always initially pin memory to local NUMA node
    let mut cmd_prefixes: Vec<String> = vec![String::new(); cfg.workloads.len()];

    // Determine how many threads/cores each workload should have
    let cores_per_wkld: Vec<usize> = cfg
        .workloads
        .iter()
        .map(|&wkld| match wkld {
            Workload::Merci { cores, .. } => cores.unwrap_or(max_cores_per_wkld),
            Workload::GapbsTc { .. } => max_cores_per_wkld,
            Workload::GapbsPr { .. } => max_cores_per_wkld,
            Workload::Gups { threads, .. } => threads,
            Workload::CloverLeaf { threads, .. } => threads,
            // One pair of threads (one core) for redis and YCSB
            Workload::Redis { .. } => 4,
            Workload::Stream => max_cores_per_wkld,
            Workload::SpecBwaves { threads } => threads,
            Workload::SpecLbm { threads } => threads,
        })
        .collect();

    // Assign threads to each workload
    let mut tot_wkld_threads: usize = 0;
    let mut pin_cores: Vec<Vec<usize>> = vec![Vec::new(); cfg.workloads.len()];
    for (i, num_cores) in cores_per_wkld.iter().enumerate() {
        tot_wkld_threads += num_cores;
        for _ in 0..*num_cores {
            if let Ok(new_core) = tctx.next() {
                pin_cores[i].push(new_core);
            } else {
                return Err(std::fmt::Error.into());
            }
        }
    }

    // Collect the rest of the threads in the first numa node
    let mut extra_cores: Vec<usize> = Vec::new();
    for _ in 0..(num_threads - tot_wkld_threads) {
        if let Ok(new_core) = tctx.next() {
            extra_cores.push(new_core);
        } else {
            return Err(std::fmt::Error.into());
        }
    }

    let pin_cores_strs: Vec<String> = pin_cores
        .iter()
        .map(|cores| {
            cores
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(",")
        })
        .collect();
    let extra_cores_str = extra_cores
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let all_cores_str = if extra_cores_str.len() == 0 {
        pin_cores_strs.join(",")
    } else {
        pin_cores_strs.join(",") + "," + &extra_cores_str
    };

    // All of the local NUMA cores should be taken by now. Get a core from
    // the remote NUMA for monitoring processes.
    let remote_core = tctx.next().unwrap();

    let proc_names: Vec<&str> = cfg
        .workloads
        .iter()
        .map(|&wkld| match wkld {
            Workload::Merci { .. } => "eval_baseline",
            Workload::GapbsTc { .. } => "tc",
            Workload::GapbsPr { .. } => "pr",
            Workload::Gups { .. } => "gups-hotset-mov",
            Workload::CloverLeaf { .. } => "omp-cloverleaf",
            Workload::Redis { .. } => "redis-server",
            Workload::Stream => "stream",
            Workload::SpecBwaves { .. } => "speed_bwaves_ba",
            Workload::SpecLbm { .. } => "lbm_s_base.mark",
        })
        .collect();

    let (
        transparent_hugepage_enabled,
        transparent_hugepage_defrag,
        transparent_hugepage_khugepaged_defrag,
    ) = if cfg.disable_thp {
        ("never", "never", 0)
    } else {
        ("always", "always", 1)
    };
    libscail::turn_on_thp(
        &ushell,
        transparent_hugepage_enabled,
        transparent_hugepage_defrag,
        transparent_hugepage_khugepaged_defrag,
        1000,
        1000,
    )?;

    if cfg.disable_aslr {
        libscail::disable_aslr(&ushell)?;
    } else {
        libscail::enable_aslr(&ushell)?;
    }

    ushell.run(cmd!(
        "echo performance | sudo tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor"
    ))?;

    match cfg.throttle {
        ThrottleType::Quartz { bw } => {
            let quartz_build_dir = dir!(&quartz_dir, "build");
            let nvmemul_ini = dir!(&quartz_dir, "nvmemul.ini");
            let tmp_nvmemul_ini = "/tmp/nvmemul.ini";
            let quartz_lib_path = format!("{}/build/src/lib/libnvmemul.so", &quartz_dir);
            let quartz_envs = format!(
                "LD_PRELOAD={} NVMEMUL_INI={} ",
                &quartz_lib_path, tmp_nvmemul_ini
            );

            // Make sure Quartz is built.
            // We can't do this in setup_wkspc because it requires the kernel being installed
            ushell.run(cmd!("make all").cwd(&quartz_build_dir))?;

            // Need to escaped the slashes for sed
            let escaped_user_home = user_home.replace("/", "\\/");

            ushell.run(cmd!("cp {} {}", nvmemul_ini, tmp_nvmemul_ini))?;
            ushell.run(cmd!(
                "sed -i 's/read = .*/read = {}/' {}",
                bw,
                &tmp_nvmemul_ini
            ))?;
            ushell.run(cmd!(
                "sed -i 's/write = .*/write = {}/' {}",
                bw,
                &tmp_nvmemul_ini
            ))?;
            // Quartz caches the throttle register to bandwidth map and register
            // addresses in the below files, so put them somewhere they will persist
            // between reboots
            ushell.run(cmd!(
                "sed -i 's/model = .*/model = \\\"{}\\/bandwidth_model\\\"/' {}",
                &escaped_user_home,
                &tmp_nvmemul_ini
            ))?;
            ushell.run(cmd!(
                "sed -i 's/mc_pci = .*/mc_pci = \\\"{}\\/mc_pci_bus\\\"/' {}",
                &escaped_user_home,
                &tmp_nvmemul_ini
            ))?;

            // Log that we set up the ini file correctly
            ushell.run(cmd!("cat {}", &tmp_nvmemul_ini))?;

            // Load the kernel module
            ushell.run(cmd!("sudo {}/scripts/setupdev.sh load", &quartz_dir))?;
            // Gotta do some permission stuff
            ushell.run(cmd!("echo 2 | sudo tee /sys/devices/cpu/rdpmc"))?;

            // Have to prerun Quartz twice to make sure the register and bandwidth
            // map files are populated
            ushell.run(cmd!("{} sleep 1", quartz_envs))?;
            ushell.run(cmd!("{} sleep 1", quartz_envs))?;

            // Throttle bandwidth with quartz for 2 hours
            // This should be enough for our workloads.
            ushell.spawn(cmd!("{} sleep 7200", quartz_envs))?;
        }
        ThrottleType::Msr => {
            // TODO: This is specific to c220g2. Make it generic
            with_shell! { ushell =>
                cmd!("sudo modprobe msr"),
                //cmd!("sudo wrmsr -p 0 0x620 0x1e1e"),
                cmd!("sudo wrmsr -p 10 0x620 0x606"),
            }
        }
        ThrottleType::Native => (),
    }

    if cfg.time {
        for (i, name) in proc_names.iter().enumerate() {
            let time_file = format!("{}.{}", time_file_stub, name);
            // Have to use full path because "time" is also a shell
            // command, which takes priority
            cmd_prefixes[i].push_str(&format!("/usr/bin/time -o {} ", time_file));
        }
    }

    // Record memory access latencies as the workload runs
    if cfg.memlat {
        ushell.run(cmd!("make").cwd(dir!(&colloid_dir, "colloid-perf")))?;
        let remote_mem_pfn_start = remote_mem_start / 4096;
        ushell.run(cmd!("sudo insmod {}/colloid-perf/colloid-perf.ko", colloid_dir))?;
        ushell.spawn(cmd!("sudo taskset -c {} {}/memlat {} 10 {}", remote_core, &tools_dir,
            remote_mem_pfn_start, &colloid_lat_file))?;
    } else {
        ushell.run(cmd!("make").cwd(dir!(&colloid_dir, "colloid-mon")))?;
        ushell.run(cmd!("sudo insmod {}/colloid-mon/colloid-mon.ko", colloid_dir))?;

        bgctx.spawn(BackgroundTask {
            name: "colloid_latency",
            period: 1, // Seconds
            cmd: format!("cat /sys/kernel/colloid/latency >> {}", &colloid_lat_file),
            ensure_started: colloid_lat_file,
        })?;
    }

    // Use whatever tiering strategy specified
    match &cfg.strategy {
        Strategy::Tpp => {
            ushell.run(cmd!("make").cwd(dir!(&colloid_dir, "tierinit")))?;

            with_shell! { ushell =>
                cmd!("sudo insmod {}/tierinit/tierinit.ko", colloid_dir),
                cmd!("swapoff -a"),
                cmd!("echo 1 | sudo tee /sys/kernel/mm/numa/demotion_enabled"),
                cmd!("echo 2 | sudo tee /proc/sys/kernel/numa_balancing"),
            }
        }
        Strategy::Colloid => {
            ushell.run(cmd!("make").cwd(dir!(&colloid_dir, "tierinit")))?;

            with_shell! { ushell =>
                cmd!("sudo insmod {}/tierinit/tierinit.ko", colloid_dir),
                cmd!("swapoff -a"),
                cmd!("echo 1 | sudo tee /sys/kernel/mm/numa/demotion_enabled"),
                cmd!("echo 6 | sudo tee /proc/sys/kernel/numa_balancing"),
            }
        }
        Strategy::Bwmfs { ratios } => {
            let bandwidthmfs_dir = dir!(kernel_dir, "BandwidthMMFS");

            ushell.run(cmd!("make").cwd(&bandwidthmfs_dir))?;
            ushell.run(cmd!("sudo insmod {}/bandwidth.ko", &bandwidthmfs_dir))?;
            ushell.run(cmd!("echo 1 | sudo tee /sys/kernel/mm/fbmm/state"))?;

            for (i, (local, remote)) in ratios.iter().enumerate() {
                let mount_dir = dir!(&user_home, format!("bwmfs{}", i + 1));

                ushell.run(cmd!("mkdir -p {}", mount_dir))?;
                ushell.run(cmd!(
                    "sudo mount -t BandwidthMMFS BandwidthMMFS {}",
                    mount_dir
                ))?;
                ushell.run(cmd!("sudo chown -R $USER {}", mount_dir))?;

                ushell.run(cmd!(
                    "echo {} | sudo tee /sys/fs/bwmmfs{}/node0/weight",
                    local,
                    i + 1
                ))?;
                ushell.run(cmd!(
                    "echo {} | sudo tee /sys/fs/bwmmfs{}/node1/weight",
                    remote,
                    i + 1
                ))?;

                cmd_prefixes[i].push_str(&format!("{}/fbmm_wrapper {} ", &tools_dir, mount_dir));
            }
        }
        Strategy::Numactl { local, remote } => {
            ushell.run(cmd!(
                "echo {} | sudo tee /sys/kernel/mm/mempolicy/weighted_interleave/node0",
                local,
            ))?;
            ushell.run(cmd!(
                "echo {} | sudo tee /sys/kernel/mm/mempolicy/weighted_interleave/node1",
                remote,
            ))?;

            for prefix in &mut cmd_prefixes {
                prefix.push_str(&format!("{}/numactl -w 0,1 ", &numactl_dir));
            }
        }
        Strategy::Cipp { total_bw } => {
            let damo_yaml_file = dir!(&user_home, "cipp.yaml");
            let cipp_exe = if *total_bw { "cipp_total_bw" } else { "cipp" };

            ushell.run(cmd!(
                "sudo {}/gen_interleave.py -o {} -a {}",
                &damo_dir,
                &damo_yaml_file,
                remote_mem_start,
            ))?;
            ushell.run(cmd!("sudo {}/damo start {}", &damo_dir, &damo_yaml_file))?;
            ushell.run(cmd!("sudo taskset -cp {} $(pgrep kdamond)", &all_cores_str))?;

            ushell.run(cmd!(
                "echo 100 | sudo tee /sys/kernel/mm/mempolicy/weighted_interleave/node0"
            ))?;
            ushell.run(cmd!(
                "echo 0 | sudo tee /sys/kernel/mm/mempolicy/weighted_interleave/node1"
            ))?;

            ushell.run(cmd!("echo 0 | sudo tee /proc/sys/kernel/numa_balancing"))?;

            ushell.spawn(cmd!(
                "sudo {}/{} 100 6000 30000 > {}",
                &tools_dir,
                cipp_exe,
                &cipp_file
            ))?;

            for prefix in &mut cmd_prefixes {
                prefix.push_str(&format!("{}/numactl -w 0,1 ", &numactl_dir));
            }
        }
        Strategy::Linux => {
            for prefix in &mut cmd_prefixes {
                prefix.push_str("numactl --preferred=0 ");
            }
        }
    }

    for (i, cores_str) in pin_cores_strs.iter().enumerate() {
        // The Redis code in libscail does its own pinning, so ignore it here
        if matches!(&cfg.workloads[i], Workload::Redis { .. }) {
            continue;
        }
        cmd_prefixes[i].push_str(&format!("taskset -c {} ", cores_str));
    }

    if cfg.meminfo {
        for name in &proc_names {
            let meminfo_file = format!("{}.{}", meminfo_file_stub, name);
            bgctx.spawn(BackgroundTask {
                name: "meminfo",
                period: 5, // Seconds
                // TODO: Below is currently hardcoded local memory range for c220g2.
                // We should make this more general
                cmd: format!(
                    "sudo {}/meminfo $(pgrep -x {} | sort -n | head -n1) 0x100000000 0x1480000000 >> {}",
                    tools_dir, name, &meminfo_file
                ),
                ensure_started: meminfo_file,
            })?;
        }
    }

    if cfg.perf_stat {
        // TODO: Have this be per workload, like meminfo
        cmd_prefixes[0].push_str(&gen_perf_command_prefix(
            perf_stat_file,
            &cfg.perf_counters,
            "",
        ));
    }

    if cfg.flame_graph {
        cmd_prefixes[0].push_str(&format!(
            "sudo perf record -a -g -F 1999 -o {} ",
            &perf_record_file
        ));
    }

    // Keep track of how many pages are migrated
    bgctx.spawn(BackgroundTask {
        name: "pgmigrate",
        period: 1, // Seconds
        cmd: format!("cat /proc/vmstat | grep \"\\(pgmigrate_success\\|pgdemote\\)\" >> {}", &pgmigrate_file),
        ensure_started: pgmigrate_file,
    })?;

    // For YCSB workloads, we should start and load data onto the servers
    // before running the workloads, since that will take several minutes
    let mut ycsb_sessions: Vec<Option<YcsbSession<fn(&SshShell) -> Result<(), ScailError>>>> = cfg
        .workloads
        .iter()
        .enumerate()
        .map(|(i, &wkld)| match wkld {
            Workload::Redis { server_size_mb, op_count, load_before_wklds } => {
                // Found empirically
                const RECORD_SIZE_KB: usize = 21;

                let record_count = (server_size_mb << 10) / RECORD_SIZE_KB;
                let redis_cfg = RedisWorkloadConfig {
                    redis_dir: &redis_dir,
                    nullfs: None,
                    redis_conf: &redis_conf,
                    server_size_mb,
                    wk_size_gb: server_size_mb >> 10,
                    output_file: None,
                    server_pin_core: Some(pin_cores[i][0]),
                    cmd_prefix: Some(&cmd_prefixes[i]),
                    pintool: None,
                };
                let ycsb_cfg = YcsbConfig {
                    workload: YcsbWorkload::Custom {
                        record_count,
                        op_count,
                        distribution: YcsbDistribution::Zipfian,
                        read_prop: 1.0,
                        update_prop: 0.0,
                        insert_prop: 0.0,
                    },
                    system: YcsbSystem::<fn(&SshShell) -> Result<(), ScailError>>::Redis(redis_cfg),
                    client_pin_core: Some(pin_cores[i][2]),
                    ycsb_path: &ycsb_dir,
                    ycsb_result_file: Some(&ycsb_file),
                };

                let mut ycsb = YcsbSession::new(ycsb_cfg);

                if load_before_wklds {
                    ycsb.start_and_load(&ushell).ok()?;
                }

                Some(ycsb)
            },
            _ => None,
        })
        .collect();

    let handles: Vec<_> = cfg
        .workloads
        .iter()
        .enumerate()
        .map(|(i, &wkld)| match wkld {
            Workload::Merci { runs, cores, delay } => {
                let cores = cores.unwrap_or(max_cores_per_wkld);
                run_merci(
                    &ushell,
                    &merci_dir,
                    runs,
                    cores,
                    delay,
                    &cmd_prefixes[i],
                    &merci_file,
                )
            }
            Workload::GapbsTc { runs } => {
                run_gapbs_tc(&ushell, &gapbs_dir, runs, &cmd_prefixes[i], &gapbs_file)
            }
            Workload::GapbsPr { runs } => {
                run_gapbs_pr(&ushell, &gapbs_dir, runs, &cmd_prefixes[i], &gapbs_file)
            }
            Workload::Gups { threads, exp, hot_exp, num_updates } => {
                run_gups(
                    &ushell,
                    &gups_dir,
                    threads,
                    exp,
                    hot_exp,
                    num_updates,
                    &cmd_prefixes[i],
                    &gups_file,
                )
            }
            Workload::CloverLeaf { delay, .. } => {
                run_clover(
                    &ushell,
                    &clover_dir,
                    delay,
                    &cmd_prefixes[i],
                    &clover_file,
                )
            }
            Workload::Redis { load_before_wklds, .. } => {
                match &mut ycsb_sessions[i] {
                    Some(ycsb) => {
                        if !load_before_wklds {
                            ushell.run(cmd!("sleep 60"))?;
                            ycsb.start_and_load(&ushell)?;
                        }

                        Ok(ycsb.run_handle(&ushell)?)
                    },
                    None => Err(ScailError::InvalidValueError { msg: "YCSB Session does not exist for Reids".to_string() }.into()),
                }
            }
            Workload::Stream => {
                run_stream(
                    &ushell,
                    &stream_dir,
                    &cmd_prefixes[i],
                    &stream_file,
                )
            }
            Workload::SpecBwaves { threads } => {
                run_spec(
                    &ushell,
                    &spec_dir,
                    "bwaves_s",
                    threads,
                    &cmd_prefixes[i],
                    &spec_file,
                )
            }
            Workload::SpecLbm { threads } => {
                run_spec(
                    &ushell,
                    &spec_dir,
                    "lbm_s",
                    threads,
                    &cmd_prefixes[i],
                    &spec_file,
                )
            }
        })
        .collect();

    if cfg.bwmon {
        // Attach bwmon to only the first workload since it will track bw for the
        // whole system.
        // TODO: have while loop that waits for app to start
        ushell.run(cmd!("sleep 10"))?;
        ushell.spawn(cmd!(
            "sudo {}/bwmon 100 {} $(pgrep -x {})",
            tools_dir,
            bwmon_file,
            proc_names[0]
        ))?;
    }

    // Wait for the first workload to finish then kill the rest
    for (i, handle) in handles.into_iter().enumerate() {
        if cfg.kill_after_first_done && i != 0 {
            ushell.run(cmd!("sudo pkill {}", proc_names[i]).allow_error())?;
        }
        match handle {
            Ok(h) => h.join().1?,
            Err(e) => return Err(e),
        };
    }

    ushell.run(cmd!("cat /proc/vmstat | tee {}", &vmstat_file))?;

    if let Strategy::Cipp { .. } = &cfg.strategy {
        ushell.run(cmd!("sudo {}/damo status | tee {}", &damo_dir, &damo_status_file))?;
    }

    if cfg.flame_graph {
        ushell.run(cmd!(
            "sudo perf script -i {} | ./FlameGraph/stackcollapse-perf.pl > /tmp/flamegraph",
            &perf_record_file,
        ))?;
        ushell.run(cmd!(
            "./FlameGraph/flamegraph.pl /tmp/flamegraph > {}",
            &flame_graph_file
        ))?;
    }

    bgctx.notify_and_join_all()?;

    println!("RESULTS: {}", dir!(&results_dir, cfg.gen_file_name("")));
    Ok(())
}

fn connect_and_setup_host<A>(login: &Login<A>) -> Result<SshShell, failure::Error>
where
    A: std::net::ToSocketAddrs + std::fmt::Display + std::fmt::Debug + Clone,
{
    let ushell = SshShell::with_any_key(login.username, &login.host)?;
    //    spurs_util::reboot(&mut ushell, /* dry_run */ false)?;
    let _ = ushell.run(cmd!("sudo reboot"));
    // It sometimes takes a few seconds for the reboot to actually happen,
    // so make sure we wait a bit for it.
    std::thread::sleep(std::time::Duration::from_secs(5));

    // Keep trying to connect until we succeed
    let ushell = {
        let mut shell;
        loop {
            println!("Attempting to reconnect...");
            shell = match SshShell::with_any_key(login.username, &login.host) {
                Ok(shell) => shell,
                Err(_) => {
                    std::thread::sleep(std::time::Duration::from_secs(10));
                    continue;
                }
            };
            match shell.run(cmd!("whoami")) {
                Ok(_) => break,
                Err(_) => {
                    std::thread::sleep(std::time::Duration::from_secs(10));
                    continue;
                }
            }
        }

        shell
    };

    dump_sys_info(&ushell)?;

    ushell.run(cmd!(
        "sudo LD_LIBRARY_PATH=/usr/lib64/ cpupower frequency-set -g performance",
    ))?;
    ushell.run(cmd!("lscpu"))?;
    set_kernel_printk_level(&ushell, 5)?;

    Ok(ushell)
}

fn isolate_remote_cores(ushell: &SshShell) -> Result<(), failure::Error> {
    let remote_threads = get_socket_threads(ushell, 1)?;
    let disable_cores_str = remote_threads
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(",");

    ushell.run(cmd!("cat /etc/default/grub"))?;
    // Remote the old isolcpus
    ushell.run(cmd!(
        r#"sed -E 's/ isolcpus=[0-9]+(,[0-9]+)*//g' \
        /etc/default/grub | tee /tmp/grub"#
    ))?;
    // Add new isolcpus
    ushell.run(cmd!(
        r#"sed 's/GRUB_CMDLINE_LINUX="\(.*\)"/GRUB_CMDLINE_LINUX="\1 isolcpus={}"/' \
    /tmp/grub | tee /tmp/grub2"#,
        disable_cores_str
    ))?;

    ushell.run(cmd!("sudo mv /tmp/grub2 /etc/default/grub"))?;
    ushell.run(cmd!("sudo update-grub2"))?;

    Ok(())
}

fn get_socket_threads(
    ushell: &SshShell,
    target_socket: usize,
) -> Result<Vec<usize>, failure::Error> {
    let mut threads: Vec<usize> = Vec::new();
    let lscpu_output = ushell.run(cmd!("lscpu -p"))?.stdout;

    for line in lscpu_output.lines() {
        if line.contains('#') {
            continue;
        }
        let mut split = line.trim().split(",");
        let thread = split
            .next()
            .unwrap()
            .parse::<usize>()
            .expect("Expected integer");
        let _core = split
            .next()
            .unwrap()
            .parse::<usize>()
            .expect("Expected integer");
        let socket = split
            .next()
            .unwrap()
            .parse::<usize>()
            .expect("Expected integer");

        if socket == target_socket {
            threads.push(thread);
        }
    }

    Ok(threads)
}

fn run_merci(
    ushell: &SshShell,
    merci_dir: &str,
    runs: u64,
    cores: usize,
    delay: Option<u64>,
    cmd_prefix: &str,
    merci_file: &str,
) -> Result<SshSpawnHandle, failure::Error> {
    let sleep = if let Some(d) = delay {
        format!("sleep {};", d)
    } else {
        String::new()
    };

    let handle = ushell.spawn(
        cmd!(
            "{} {} ./bin/eval_baseline -d amazon_Books -r {} -c {} | sudo tee -a {}",
            sleep,
            cmd_prefix,
            runs,
            cores,
            merci_file
        )
        .cwd(merci_dir),
    )?;

    Ok(handle)
}

fn run_gapbs_tc(
    ushell: &SshShell,
    gapbs_dir: &str,
    runs: u64,
    cmd_prefix: &str,
    gapbs_file: &str,
) -> Result<SshSpawnHandle, failure::Error> {
    let handle = ushell.spawn(
        cmd!(
            "{} ./tc -f benchmark/graphs/twitterU.sg -n {} | sudo tee {}",
            cmd_prefix,
            runs,
            gapbs_file
        )
        .cwd(gapbs_dir),
    )?;

    Ok(handle)
}

fn run_gapbs_pr(
    ushell: &SshShell,
    gapbs_dir: &str,
    runs: u64,
    cmd_prefix: &str,
    gapbs_file: &str,
) -> Result<SshSpawnHandle, failure::Error> {
    let handle = ushell.spawn(
        cmd!(
            "{} ./pr -g 26 -n {} | sudo tee {}",
            cmd_prefix,
            runs,
            gapbs_file
        )
        .cwd(gapbs_dir),
    )?;

    Ok(handle)
}

fn run_gups(
    ushell: &SshShell,
    gups_dir: &str,
    threads: usize,
    exp: usize,
    hot_exp: usize,
    num_updates: usize,
    cmd_prefix: &str,
    gups_file: &str,
) -> Result<SshSpawnHandle, failure::Error> {
    let handle = ushell.spawn(
        cmd!(
            "{} ./gups-hotset-move {} {} {} 8 {} n | tee {}",
            cmd_prefix,
            threads,
            num_updates,
            exp,
            hot_exp,
            gups_file
        )
        .cwd(gups_dir)
    )?;

    Ok(handle)
}

fn run_clover(
    ushell: &SshShell,
    clover_dir: &str,
    delay: Option<usize>,
    cmd_prefix: &str,
    clover_file: &str,
) -> Result<SshSpawnHandle, failure::Error> {
    let (sleep, outfile) = if let Some(d) = delay {
        (format!("sleep {};", d), format!("{}2", clover_file))
    } else {
        (String::new(), clover_file.to_string())
    };

    let handle = ushell.spawn(
        cmd!(
            "{} {} ./build/omp-cloverleaf --file ./InputDecks/clover_bm64_300.in | tee {}",
            sleep,
            cmd_prefix,
            outfile,
        )
        .cwd(clover_dir)
    )?;

    Ok(handle)
}

fn run_stream(
    ushell: &SshShell,
    stream_dir: &str,
    cmd_prefix: &str,
    stream_file: &str,
) -> Result<SshSpawnHandle, failure::Error> {
    let handle = ushell.spawn(
        cmd!(
            "{} ./stream | tee {}",
            cmd_prefix,
            stream_file
        )
        .cwd(stream_dir)
    )?;

    Ok(handle)
}

fn run_spec(
    ushell: &SshShell,
    spec_dir: &str,
    workload: &str,
    threads: usize,
    cmd_prefix: &str,
    spec_file: &str,
) -> Result<SshSpawnHandle, failure::Error> {
    let spec_stub = "runcpu --action=run --noreportable --iterations 1 --nobuild \
        --size ref --tune base --config spec-linux-x86.cfg";

    let handle = ushell.spawn(
        cmd!(
            "source shrc && {} {} --threads={} {} | tee {}",
            cmd_prefix,
            spec_stub,
            threads,
            workload,
            spec_file,
        )
        .cwd(spec_dir)
    )?;

    Ok(handle)
}
