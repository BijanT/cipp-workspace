use clap::{arg, ArgAction};

use libscail::{
    background::{BackgroundContext, BackgroundTask},
    dir, dump_sys_info, get_user_home_dir,
    output::{Parametrize, Timestamp},
    set_kernel_printk_level, with_shell,
    workloads::{gen_perf_command_prefix, TasksetCtxBuilder, TasksetCtxInterleaving},
    Login,
};

use serde::{Deserialize, Serialize};

use spurs::{cmd, Execute, SshShell, SshSpawnHandle};
use spurs_util::escape_for_bash;

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
enum Workload {
    Merci { runs: u64 },
    GapbsTc { runs: u64 },
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
enum Strategy {
    Colloid,
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

    perf_stat: bool,
    perf_counters: Vec<String>,
    disable_thp: bool,
    disable_aslr: bool,
    flame_graph: bool,
    bwmon: bool,
    meminfo: bool,
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
        .arg(arg!(--colloid "Use Colloid").action(ArgAction::SetTrue))
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
            arg!(--quartz <QUARTZ_BW> "Use Quartz to limit the memory bandwidth (MB/s)")
                .value_parser(clap::value_parser!(u64))
                .conflicts_with("msr_throttle"),
        )
        .arg(
            arg!(--msr_throttle <REGISTER_VAL> "Write to the MSR_UNCORE_RATIO_LIMIT register to lower uncore frequency")
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
            clap::Command::new("merci_tc")
                .about("Run the MERCI and GAPBS tc workload together")
                .arg(
                    arg!([runs]
            "The number of iterations of tc to run. Default: 10")
                    .value_parser(clap::value_parser!(u64)),
                ),
        )
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
    let colloid = sub_m.get_flag("colloid");
    let flame_graph = sub_m.get_flag("flame_graph");
    let bwmon = sub_m.get_flag("bwmon");
    let meminfo = sub_m.get_flag("meminfo");
    let quartz_bw = sub_m.get_one::<u64>("quartz").copied();
    let msr_throttle = sub_m.get_flag("msr_throttle");

    let workloads = match sub_m.subcommand() {
        Some(("merci", sub_m)) => {
            let runs = *sub_m.get_one::<u64>("runs").unwrap_or(&10);
            vec![Workload::Merci { runs }]
        }
        Some(("gapbs_tc", sub_m)) => {
            let runs = *sub_m.get_one::<u64>("runs").unwrap_or(&10);
            vec![Workload::GapbsTc { runs }]
        }
        Some(("merci_tc", sub_m)) => {
            let tc_runs = *sub_m.get_one::<u64>("runs").unwrap_or(&10);
            let merci_runs = 100 * tc_runs;
            vec![
                Workload::GapbsTc { runs: tc_runs },
                Workload::Merci { runs: merci_runs },
            ]
        }
        _ => unreachable!(),
    };

    let strategy = if colloid {
        Strategy::Colloid
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
        perf_stat,
        perf_counters,
        disable_thp,
        disable_aslr,
        flame_graph,
        bwmon,
        meminfo,
        throttle,
        timestamp: Timestamp::now(),
    };

    run_inner(&login, &cfg)
}

fn run_inner<A>(login: &Login<A>, cfg: &Config) -> Result<(), failure::Error>
where
    A: std::net::ToSocketAddrs + std::fmt::Display + std::fmt::Debug + Clone,
{
    let ushell = connect_and_setup_host(login)?;
    let user_home = get_user_home_dir(&ushell)?;

    // Setup the output filename
    let results_dir = dir!(&user_home, crate::RESULTS_PATH);

    let (_output_file, params_file, _time_file, _sim_file) = cfg.gen_standard_names();
    let perf_record_file = "/tmp/perf.data";
    let perf_stat_file = dir!(&results_dir, cfg.gen_file_name("perf_stat"));
    let flame_graph_file = dir!(&results_dir, cfg.gen_file_name("flamegraph.svg"));
    let colloid_lat_file = dir!(&results_dir, cfg.gen_file_name("colloid.lat"));
    let bwmon_file = dir!(&results_dir, cfg.gen_file_name("bwmon"));
    let merci_file = dir!(&results_dir, cfg.gen_file_name("merci"));
    let gapbs_file = dir!(&results_dir, cfg.gen_file_name("gapbs"));
    let meminfo_file_stub = dir!(&results_dir, cfg.gen_file_name("meminfo"));

    let colloid_dir = dir!(&user_home, "colloid/tpp/");
    let tools_dir = dir!(&user_home, crate::WKSPC_PATH, "tools/");
    let quartz_dir = dir!(&user_home, crate::WKSPC_PATH, "quartz/");
    let merci_dir = dir!(
        &user_home,
        crate::WORKLOADS_PATH,
        "MERCI/4_performance_evaluation/"
    );
    let gapbs_dir = dir!(&user_home, crate::WORKLOADS_PATH, "gapbs/");

    let mut tctx = TasksetCtxBuilder::from_lscpu(&ushell)?
        .numa_interleaving(TasksetCtxInterleaving::Sequential)
        .skip_hyperthreads(false)
        .build();
    let num_threads = tctx.num_threads_on_socket(0);
    let cores_per_wkld = num_threads / cfg.workloads.len();

    let mut bgctx = BackgroundContext::new(&ushell);

    ushell.run(cmd!("mkdir -p {}", results_dir))?;
    ushell.run(cmd!(
        "echo {} > {}",
        escape_for_bash(&serde_json::to_string(&cfg)?),
        dir!(&results_dir, params_file)
    ))?;

    // For now, always initially pin memory to local NUMA node
    let mut cmd_prefixes: Vec<String> = vec![String::new(); cfg.workloads.len()];

    let mut pin_cores: Vec<Vec<usize>> = vec![Vec::new(); cfg.workloads.len()];
    for pc in &mut pin_cores {
        for _ in 0..cores_per_wkld {
            if let Ok(new_core) = tctx.next() {
                pc.push(new_core);
            } else {
                return Err(std::fmt::Error.into());
            }
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

    let proc_names: Vec<&str> = cfg
        .workloads
        .iter()
        .map(|&wkld| match wkld {
            Workload::Merci { .. } => "eval_baseline",
            Workload::GapbsTc { .. } => "tc",
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
            let nvmemul_ini = dir!(&quartz_dir, "nvmemul.ini");
            let tmp_nvmemul_ini = "/tmp/nvmemul.ini";
            let quartz_lib_path = format!("{}/build/src/lib/libnvmemul.so", &quartz_dir);
            let quartz_envs = format!(
                "LD_PRELOAD={} NVMEMUL_INI={} ",
                &quartz_lib_path, tmp_nvmemul_ini
            );

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

            // We only need to add the env variable to one of the workloads
            cmd_prefixes[0].push_str(&quartz_envs);
        }
        ThrottleType::Msr => {
            // TODO: This is specific to c220g2. Make it generic
            with_shell! { ushell =>
                cmd!("sudo modprobe msr"),
                cmd!("sudo wrmsr -p 0 0x620 0x1010"),
                cmd!("sudo wrmsr -p 10 0x620 0x707"),
            }
        }
        ThrottleType::Native => (),
    }

    // Use whatever tiering strategy specified
    match cfg.strategy {
        Strategy::Colloid => {
            with_shell! { ushell =>
                cmd!("sudo insmod {}/tierinit/tierinit.ko", colloid_dir),
                cmd!("sudo insmod {}/colloid-mon/colloid-mon.ko", colloid_dir),
                cmd!("swapoff -a"),
                cmd!("echo 1 | sudo tee /sys/kernel/mm/numa/demotion_enabled"),
                cmd!("echo 6 | sudo tee /proc/sys/kernel/numa_balancing"),
            }

            bgctx.spawn(BackgroundTask {
                name: "colloid_latency",
                period: 5, // Seconds
                cmd: format!("cat /sys/kernel/colloid/latency >> {}", &colloid_lat_file),
                ensure_started: colloid_lat_file,
            })?;
        }
        Strategy::Linux => {
            for prefix in &mut cmd_prefixes {
                prefix.push_str("numactl --membind=0 ");
            }
        }
    }

    for (i, cores_str) in pin_cores_strs.iter().enumerate() {
        cmd_prefixes[i].push_str(&format!("taskset -c {} ", cores_str));
    }

    if cfg.bwmon {
        // Attach bwmon to only the first workload since it will track bw for the
        // whole system.
        cmd_prefixes[0].push_str(&format!("sudo {}/bwmon 200 {} ", tools_dir, bwmon_file));
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

    let handles: Vec<_> = cfg
        .workloads
        .iter()
        .enumerate()
        .map(|(i, &wkld)| match wkld {
            Workload::Merci { runs } => run_merci(
                &ushell,
                &merci_dir,
                runs,
                cores_per_wkld,
                &cmd_prefixes[i],
                &merci_file,
            ),
            Workload::GapbsTc { runs } => {
                run_gapbs_tc(&ushell, &gapbs_dir, runs, &cmd_prefixes[i], &gapbs_file)
            }
        })
        .collect();

    // Wait for the first workload to finish then kill the rest
    for (i, handle) in handles.into_iter().enumerate() {
        if i != 0 {
            ushell.run(cmd!("sudo pkill {}", proc_names[i]))?;
        }
        match handle {
            Ok(h) => h.join().1?,
            Err(e) => return Err(e),
        };
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

fn run_merci(
    ushell: &SshShell,
    merci_dir: &str,
    runs: u64,
    cores: usize,
    cmd_prefix: &str,
    merci_file: &str,
) -> Result<SshSpawnHandle, failure::Error> {
    let handle = ushell.spawn(
        cmd!(
            "{} ./bin/eval_baseline -d amazon_Books -r {} -c {} | sudo tee {}",
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
