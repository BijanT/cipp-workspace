use clap::{arg, ArgAction};

use libscail::{
    background::{BackgroundContext, BackgroundTask},
    dir,
    //    workloads::{TasksetCtxBuilder, TasksetCtxInterleaving},
    dump_sys_info,
    get_user_home_dir,
    output::{Parametrize, Timestamp},
    set_kernel_printk_level,
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

#[derive(Debug, Clone, Serialize, Deserialize, Parametrize)]
struct Config {
    #[name]
    exp: String,

    #[name]
    workloads: Vec<Workload>,

    disable_thp: bool,
    disable_aslr: bool,
    flame_graph: bool,
    bwmon: bool,
    meminfo: bool,

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
        .arg(arg!(--disable_thp "Disable THP completely.").action(ArgAction::SetTrue))
        .arg(arg!(--disable_aslr "Disable ASLR.").action(ArgAction::SetTrue))
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
}

pub fn run(sub_m: &clap::ArgMatches) -> Result<(), failure::Error> {
    let username = sub_m.get_one::<String>("username").unwrap().clone();
    let host = sub_m.get_one::<String>("hostname").unwrap().clone();
    let login = Login {
        username: username.as_str(),
        hostname: host.as_str(),
        host: host.as_str(),
    };

    let disable_thp = sub_m.get_flag("disable_thp");
    let disable_aslr = sub_m.get_flag("disable_aslr");
    let flame_graph = sub_m.get_flag("flame_graph");
    let bwmon = sub_m.get_flag("bwmon");
    let meminfo = sub_m.get_flag("meminfo");

    let workloads = match sub_m.subcommand() {
        Some(("merci", sub_m)) => {
            let runs = *sub_m.get_one::<u64>("runs").unwrap_or(&10);
            vec![Workload::Merci { runs }]
        }
        Some(("gapbs_tc", sub_m)) => {
            let runs = *sub_m.get_one::<u64>("runs").unwrap_or(&10);
            vec![Workload::GapbsTc { runs }]
        }
        _ => unreachable!(),
    };

    let cfg = Config {
        exp: "cipp_exp".into(),
        workloads,
        disable_thp,
        disable_aslr,
        flame_graph,
        bwmon,
        meminfo,
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
    let flame_graph_file = dir!(&results_dir, cfg.gen_file_name("flamegraph.svg"));
    let bwmon_file = dir!(&results_dir, cfg.gen_file_name("bwmon"));
    let merci_file = dir!(&results_dir, cfg.gen_file_name("merci"));
    let gapbs_file = dir!(&results_dir, cfg.gen_file_name("gapbs"));
    let meminfo_file_stub = dir!(&results_dir, cfg.gen_file_name("meminfo"));

    let tools_dir = dir!(&user_home, crate::WKSPC_PATH, "tools/");
    let merci_dir = dir!(
        &user_home,
        crate::WORKLOADS_PATH,
        "MERCI/4_performance_evaluation/"
    );
    let gapbs_dir = dir!(&user_home, crate::WORKLOADS_PATH, "gapbs/");

    ushell.run(cmd!("mkdir -p {}", results_dir))?;
    ushell.run(cmd!(
        "echo {} > {}",
        escape_for_bash(&serde_json::to_string(&cfg)?),
        dir!(&results_dir, params_file)
    ))?;

    // For now, always initially pin memory to local NUMA node
    let mut cmd_prefixes: Vec<String> =
        vec![String::from("numactl --membind=0 "); cfg.workloads.len()];

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

    if cfg.bwmon {
        // Attach bwmon to only the first workload since it will track bw for the
        // whole system.
        cmd_prefixes[0].push_str(&format!("sudo {}/bwmon 200 {} ", tools_dir, bwmon_file));
    }

    let mut bgctx = BackgroundContext::new(&ushell);
    if cfg.meminfo {
        for name in &proc_names {
            let meminfo_file = format!("{}.{}", meminfo_file_stub, name);
            bgctx.spawn(BackgroundTask {
                name: "meminfo",
                period: 10, // Seconds
                cmd: format!(
                    "sudo {}/meminfo $(pgrep -x {} | sort -n | head -n1) 0x100000000 0x1500000000 >> {}",
                    tools_dir, name, &meminfo_file
                ),
                ensure_started: meminfo_file,
            })?;
        }
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
            Workload::Merci { runs } => {
                run_merci(&ushell, &merci_dir, runs, &cmd_prefixes[i], &merci_file)
            }
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
    cmd_prefix: &str,
    merci_file: &str,
) -> Result<SshSpawnHandle, failure::Error> {
    let handle = ushell.spawn(
        cmd!(
            "{} ./bin/eval_baseline -d amazon_Books -r {} | sudo tee {}",
            cmd_prefix,
            runs,
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
