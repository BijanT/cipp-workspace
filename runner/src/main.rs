mod setup_kernel;
mod setup_wkspc;

use clap::{arg};

const KERNEL_PATH: &str = "kernel/";

fn run() -> Result<(), failure::Error> {
    let matches = clap::Command::new("runner")
        .arg(arg!(--print_results_path "Obselete"))
        .subcommand(crate::setup_wkspc::cli_options())
        .subcommand(crate::setup_kernel::cli_options())
        .subcommand_required(true)
        .disable_version_flag(true)
        .get_matches();

    match matches.subcommand() {
        Some(("setup_wkspc", sub_m)) => crate::setup_wkspc::run(sub_m),
        Some(("setup_kernel", sub_m)) => crate::setup_kernel::run(sub_m),
        _ => {
            unreachable!();
        }
    }
}

fn main() {
    use console::style;

    env_logger::init();

    std::env::set_var("RUST_BACKTRACE", "1");

    // If an error was returned, try to print something helpful
    if let Err(err) = run() {
        const MESSAGE: &str = r#"== ERROR ==================================================================================
        `runner` encountered an error. The command log above may offer clues. If the error pertains to SSH,
        you may be able to get useful information by setting the RUST_LOG=debug enviroent variable. It is
        recommended that you use `debug` builds of `runner`, rather than `release`, as the performance of
        `runner` is not that important and is almost always dominated by the experiment being run.
        "#;

        println!("{}", style(MESSAGE).red().bold());

        // Errors from SSH commands
        if err.downcast_ref::<spurs::SshError>().is_some() {
            println!("En error occurred while attempting to run a command over SSH");
        }

        // Print error and backtrace
        println!(
            "`runner` encountered the following error:\n{}\n{}",
            err.as_fail(),
            err.backtrace(),
        );

        std::process::exit(101);
    }
}
