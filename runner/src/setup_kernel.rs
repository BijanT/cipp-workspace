use clap::{arg, ArgAction};

use libscail::{
    dir, get_git_hash, get_user_home_dir, GitRepo, KernelBaseConfigSource, KernelConfig,
    KernelPkgType, KernelSrc, Login,
};

use spurs::{cmd, Execute, SshShell};

pub fn cli_options() -> clap::Command {
    clap::Command::new("setup_kernel")
        .about("Sets up the given _ubuntu_ machine with the given kernel. Requires `sudo`.")
        .arg_required_else_help(true)
        .arg(arg!(<hostname>
         "The domain name and ssh port of the remote (e.g. c240g2-031321.wisc.cloudlab.us:22)"))
        .arg(arg!(<username> "The username of the remote (e.g. bijan)"))
        .arg(arg!(--repo <REPO> "The git repo where the kernel is stored.").required(true))
        .arg(arg!(--branch <BRANCH> "The branch of the repo to clone. Defaults to \"main\""))
        .arg(arg!(--git_user <GIT_USER>
         "The username of the GitHub account to use to clone the kernel"))
        .arg(arg!(--secret <SECRET> "The GitHub access token to use").requires("git_user"))
        .arg(
            arg!(--install_perf "(Optional) Install the perf corresponding to this kernel")
                .action(ArgAction::SetTrue),
        )
        .arg(
            arg!([configs] ...
         "Space separated list of Linux kernel configuration options, prefixed by \
         + to enable and - to disable. For example, +CONFIG_ZSWAP or \
         -CONFIG_PAGE_TABLE_ISOLATION")
            .allow_hyphen_values(true)
            .trailing_var_arg(true),
        )
}

pub fn run(sub_m: &clap::ArgMatches) -> Result<(), failure::Error> {
    let login = Login {
        username: sub_m.get_one::<String>("username").unwrap(),
        hostname: sub_m.get_one::<String>("hostname").unwrap(),
        host: sub_m.get_one::<String>("hostname").unwrap(),
    };

    let repo = sub_m.get_one::<String>("repo").unwrap().as_str();
    let branch = sub_m.get_one::<String>("branch").map_or("main", |v| v);
    let git_user = sub_m.get_one::<String>("git_user");
    let secret = sub_m.get_one::<String>("secret").map(|s| s.as_str());
    let install_perf = sub_m.get_flag("install_perf");

    let git_repo = if let Some(_secret) = &secret {
        GitRepo::HttpsPrivate {
            repo,
            username: git_user.unwrap(),
            secret: secret.unwrap(),
        }
    } else {
        GitRepo::HttpsPublic { repo }
    };

    let kernel_config: Vec<_> = sub_m
        .get_many::<String>("configs")
        .map(|values| {
            values
                .map(|arg| parse_config_option(arg).unwrap())
                .collect()
        })
        .unwrap_or_default();

    let ushell = SshShell::with_any_key(login.username, login.host)?;

    let user_home = get_user_home_dir(&ushell)?;
    let kernel_path = dir!(&user_home, crate::KERNEL_PATH);
    let perf_path = dir!(&kernel_path, "tools/perf/");

    libscail::clone_git_repo(&ushell, git_repo, Some(&kernel_path), Some(branch), &[])?;

    // Get the base config
    let config = ushell
        .run(cmd!("ls -1 /boot/config-* | head -n1").use_bash())?
        .stdout;
    let config = config.trim();
    let git_hash = get_git_hash(&ushell, &kernel_path)?;
    let kernel_localversion = libscail::gen_local_version(branch, &git_hash);

    let libscail::KernelBuildArtifacts {
        source_path: _,
        kbuild_path: _,
        pkg_path: kernel_deb,
        headers_pkg_path: kernel_headers_deb,
    } = libscail::build_kernel(
        &ushell,
        KernelSrc::Git {
            repo_path: kernel_path.clone(),
            commitish: (&branch).to_string(),
        },
        KernelConfig {
            base_config: KernelBaseConfigSource::Path(config.into()),
            extra_options: &kernel_config,
        },
        Some(&kernel_localversion),
        KernelPkgType::Deb,
        None,
        true,
    )?;

    ushell.run(cmd!("sudo dpkg -i {} {}", kernel_deb, kernel_headers_deb).cwd(&kernel_path))?;
    ushell.run(cmd!("sudo grub-set-default 0"))?;

    if install_perf {
        // Build perf
        ushell.run(cmd!("make").cwd(&perf_path))?;

        // Put the new perf in place
        ushell.run(cmd!("sudo rm -f /usr/bin/perf"))?;
        ushell.run(cmd!("sudo ln -s {}/perf /usr/bin/perf", &perf_path))?;
    }

    Ok(())
}

fn parse_config_option(opt: &str) -> Result<(&str, bool), failure::Error> {
    fn check(s: &str) -> Result<&str, failure::Error> {
        if s.is_empty() {
            Err(failure::format_err!("Empty string is not a valid option"))
        } else {
            for c in s.chars() {
                if !c.is_ascii_alphanumeric() && c != '_' {
                    return Err(failure::format_err!("Invalid config name \"{}\"", s));
                }
            }
            Ok(s)
        }
    }

    if opt.is_empty() {
        Err(failure::format_err!("Empty string is not a valid option"))
    } else {
        match &opt[0..1] {
            "+" => Ok((check(&opt[1..])?, true)),
            "-" => Ok((check(&opt[1..])?, false)),
            _ => Err(failure::format_err!(
                "Kernel config option must be prefixed with + or -"
            )),
        }
    }
}
