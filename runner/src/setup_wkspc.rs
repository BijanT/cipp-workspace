/// Configure the freshly acquired cloudlab machine and install dependencies
use crate::WKSPC_PATH;
use clap::{arg, ArgAction};

use libscail::{clone_git_repo, dir, get_user_home_dir, with_shell, GitRepo, Login};

use spurs::{cmd, Execute, SshShell};

pub fn cli_options() -> clap::Command {
    clap::Command::new("setup_wkspc")
        .about("Setup a new _ubuntu_ machine. Requires `sudo`.")
        .arg_required_else_help(true)
        .disable_version_flag(true)
        .arg(arg!(<hostname>
         "The domain name and ssh port of the remote (e.g. c240g2-031321.wisc.cloudlab.us:22)"))
        .arg(arg!(<username> "The username of the remote (e.g. bijan)"))
        .arg(arg!(--host_dep "(Optional) If passed, install host dependencies")
            .action(ArgAction::SetTrue))
        .arg(arg!(--resize_root
         "(Optional) resize the root partition to take up the while device, \
         destroying any other partitions on the device. This is useful on cloudlab, \
         where the root partition is 16GB by default.")
            .action(ArgAction::SetTrue))
        .arg(arg!(--clone_wkspc
         "(Optional) If passed, clone the workspace on the remote (or update if already cloned). \
         If the method uses HTTPS to access a private repository, the --secret option must also \
         be passed giving the GitBub personal access token or password.")
            .action(ArgAction::SetTrue))
        .arg(arg!(--git_user <GIT_USER> "(Optional) The git username to clone with.")
            .requires("clone_wkspc"))
        .arg(arg!(--wkspc_branch <BRANCH>
         "(Optional) If passed, clone the specified branch name. If not passed, the default is used. \
         rewuires --clone_wkspc.")
            .requires("clone_wkspc"))
        .arg(arg!(--secret <SECRET>
         "(Optional) If we should clone the workspace, this is the Github personal access \
         taken or password for cloning the repo."))
        .arg(arg!(--host_bmks
         "(Optional) If passed, build host benchmarks. This also makes them available to the guest.")
            .action(ArgAction::SetTrue))
        .arg(arg!(--skip_slow
         "(Optional) If passed, skip some setup steps that take a long time to speed up basic setup.")
            .action(ArgAction::SetTrue))
}

struct SetupConfig<'a, A>
where
    A: std::net::ToSocketAddrs + std::fmt::Display + std::fmt::Debug + Clone,
{
    /// Login credentials for the host.
    login: Login<'a, 'a, A>,

    /// Install the host dependencies, rename poweroff
    host_dep: bool,

    /// Resize the root partition to take up the whole device
    resize_root: bool,

    /// Should we clone/update the workspace?
    clone_wkspc: bool,
    /// Git username to clone with
    git_user: Option<&'a str>,
    /// What branch of the workspace should we use?
    wkspc_branch: Option<&'a str>,
    /// The PAT or password to clone/update the workspace with, if needed.
    secret: Option<&'a str>,

    /// Should we build host benchmarks>
    host_bmks: bool,
    /// Should we skip some steps to speedup the setup?
    skip_slow: bool,
}

pub fn run(sub_m: &clap::ArgMatches) -> Result<(), failure::Error> {
    let login = Login {
        username: sub_m.get_one::<String>("username").unwrap(),
        hostname: sub_m.get_one::<String>("hostname").unwrap(),
        host: sub_m.get_one::<String>("hostname").unwrap(),
    };

    let host_dep = sub_m.get_flag("host_dep");

    let resize_root = sub_m.get_flag("resize_root");

    let clone_wkspc = sub_m.get_flag("clone_wkspc");
    let git_user = sub_m.get_one::<String>("git_user").map(|s| s.as_str());
    let wkspc_branch = sub_m.get_one::<String>("wkspc_branch").map(|s| s.as_str());
    let secret = sub_m.get_one::<String>("secret").map(|s| s.as_str());

    let host_bmks = sub_m.get_flag("host_bmks");
    let skip_slow = sub_m.get_flag("skip_slow");

    let cfg = SetupConfig {
        login,
        host_dep,
        resize_root,
        clone_wkspc,
        git_user,
        wkspc_branch,
        secret,
        host_bmks,
        skip_slow,
    };

    run_inner(cfg)?;

    Ok(())
}

fn run_inner<A>(cfg: SetupConfig<'_, A>) -> Result<(), failure::Error>
where
    A: std::net::ToSocketAddrs + std::fmt::Display + std::fmt::Debug + Clone,
{
    // Connect to the remote
    let ushell = SshShell::with_any_key(cfg.login.username, &cfg.login.host)?;

    if cfg.host_dep {
        install_host_dependencies(&ushell)?;
        libscail::install_rust(&ushell)?;
    }

    if cfg.resize_root {
        set_up_host_devices(&ushell)?;
    }

    if cfg.clone_wkspc {
        clone_research_workspace(&ushell, &cfg)?;
    }

    if cfg.host_bmks {
        build_host_benchmarks(&ushell, &cfg)?;
    }

    ushell.run(cmd!("echo DONE"))?;

    Ok(())
}

fn install_host_dependencies(ushell: &SshShell) -> Result<(), failure::Error> {
    // Make sure we're up to do
    ushell.run(cmd!("sudo apt update; sudo apt upgrade -y"))?;

    with_shell! { ushell =>
        spurs_util::ubuntu::apt_install(&[
            "build-essential",
            "libssl-dev",
            "libelf-dev",
            "libdw-dev",
            "libncurses-dev",
            "libevent-dev",
            "dwarves",
            "libpci-dev",
            "numactl",
            "linux-tools-common",
            "openjdk-8-jdk",
            "fuse",
            "redis-server",
            "python2",
            "python3",
            "python3-pip",
            "cmake",
            "gfortran",
            "curl",
            "bpfcc-tools",
            "libhugetlbfs-bin",
            "maven",
            "mpich",
            "libicu-dev",
            "libreadline-dev",
            "autoconf",
            "pkgconf",
            "debhelper",
            "bison",
            "flex",
            "libtool",
            "systemtap-sdt-dev",
            "libunwind-dev",
            "libslang2-dev",
            "libperl-dev",
            "python-dev-is-python3",
            "libzstd-dev",
            "libcap-dev",
            "libnuma-dev",
            "libbabeltrace-dev",
            "libtraceevent-dev",
            "libpfm4-dev",
            "cgroup-tools",
            "gnuplot",
            "intel-cmt-cat",
            "htop",
            "msr-tools",
            "libconfig-dev",
            "uthash-dev",
            "tcllib",
        ]),
    };

    // CLone FlameGraph
    let flamegraph_repo = GitRepo::HttpsPublic {
        repo: "github.com/brendangregg/FlameGraph.git",
    };
    clone_git_repo(ushell, flamegraph_repo, None, None, &[])?;

    Ok(())
}

fn clone_research_workspace<A>(
    ushell: &SshShell,
    cfg: &SetupConfig<'_, A>,
) -> Result<(), failure::Error>
where
    A: std::net::ToSocketAddrs + std::fmt::Display + std::fmt::Debug + Clone,
{
    const SUBMODULES: &[&str] = &["libscail", "numactl"];
    let user_home = get_user_home_dir(ushell)?;
    let wkspc_dir = format!("{}/{}", user_home, WKSPC_PATH);
    let user = &cfg.git_user.unwrap_or("");
    let secret = cfg.secret.unwrap();
    let branch = cfg.wkspc_branch.unwrap_or("main");
    let wkspc_repo = GitRepo::HttpsPrivate {
        repo: "github.com/BijanT/cipp-workspace.git",
        username: user,
        secret,
    };
    let damo_repo = GitRepo::HttpsPrivate {
        repo: "github.com/BijanT/cipp_damo.git",
        username: user,
        secret,
    };
    let workloads_repo = GitRepo::HttpsPublic {
        repo: "github.com/BijanT/scail_workloads.git",
    };
    let colloid_repo = GitRepo::HttpsPublic {
        repo: "github.com/BijanT/colloid.git",
    };

    clone_git_repo(
        ushell,
        wkspc_repo,
        Some(WKSPC_PATH),
        Some(branch),
        SUBMODULES,
    )?;

    clone_git_repo(ushell, damo_repo, Some("damo"), Some("main"), &[])?;

    clone_git_repo(
        ushell,
        workloads_repo,
        Some("workloads"),
        Some("main"),
        &["gapbs", "redis", "YCSB", "CloverLeaf"],
    )?;

    clone_git_repo(ushell, colloid_repo, None, None, &["hemem"])?;

    // Build the workspace tools
    ushell.run(cmd!("cd tools/; make ARCH=haswell;").cwd(&wkspc_dir))?;
    ushell.run(cmd!("cd numactl; ./autogen.sh; ./configure; make").cwd(&wkspc_dir))?;

    Ok(())
}

fn build_host_benchmarks<A>(
    ushell: &SshShell,
    cfg: &SetupConfig<'_, A>
) -> Result<(), failure::Error>
where
    A: std::net::ToSocketAddrs + std::fmt::Display + std::fmt::Debug + Clone,
{
    let user_home = get_user_home_dir(&ushell)?;
    let workloads_dir = dir!(&user_home, crate::WORKLOADS_PATH);
    let quartz_build_dir = dir!(&user_home, crate::WKSPC_PATH, "quartz/build");
    let merci_dir = dir!(&workloads_dir, "MERCI");
    let gapbs_dir = dir!(&workloads_dir, "gapbs");
    let redis_dir = dir!(&workloads_dir, "redis");
    let ycsb_dir = dir!(&workloads_dir, "YCSB");
    let gups_dir = dir!(&workloads_dir, "gups_hemem");
    let clover_dir = dir!(&workloads_dir, "CloverLeaf");
    let stream_dir = dir!(&workloads_dir, "stream");

    ushell.run(cmd!("make").cwd(&gapbs_dir))?;
    if !cfg.skip_slow {
        ushell.run(cmd!("./setup_merci_books.sh").cwd(&merci_dir))?;
        ushell.run(cmd!("make bench-graphs").cwd(&gapbs_dir))?;
    }
    ushell.run(cmd!("make").cwd(&redis_dir))?;
    ushell.run(cmd!("mvn -pl site.ycsb:redis-binding -am clean package").cwd(&ycsb_dir))?;
    ushell.run(cmd!("make").cwd(&gups_dir))?;

    ushell.run(cmd!("cmake -Bbuild -H. -DMODEL=omp").cwd(&clover_dir))?;
    ushell.run(cmd!("cmake --build build").cwd(&clover_dir))?;

    ushell.run(cmd!("make").cwd(&stream_dir))?;

    ushell.run(cmd!("mkdir -p {}", &quartz_build_dir))?;
    ushell.run(cmd!("cmake ..").cwd(&quartz_build_dir))?;

    Ok(())
}

fn set_up_host_devices(ushell: &SshShell) -> Result<(), failure::Error> {
    // Remove any existing swap partitions from /etc/fstab because we plan to do all of our own
    // mounting and useounting. Moreover, if fstab contains a swap partition that we destroy during
    // setup, systemd will sit around trying to find it and adding minutes to every reboot.a
    ushell.run(cmd!(
        r#"sudo sed -i 's/^.*swap.*$/#& # COMMENTED OUT BY setup_wkspc/' /etc/fstab"#
    ))?;

    libscail::resize_root_partition(ushell)?;

    Ok(())
}
