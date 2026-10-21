//! Debugger binary.

#![cfg(feature = "debugger")]

use clap::Parser;

fn main() -> std::process::ExitCode {
    let args: Vec<std::ffi::OsString> = std::env::args_os().collect();
    if let Some(code) = mq_run::Cli::run_external_subcommand(&args) {
        return code;
    }

    let cli = mq_run::Cli::parse_from(args);
    match cli.run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(err) => {
            let code = cli.error_exit_code(&err);
            if mq_run::Cli::halt_exit_code(&err).is_none() {
                cli.report_error(&err);
            }
            std::process::exit(code)
        }
    }
}
