//! Debugger binary.

#![cfg(feature = "debugger")]

use clap::Parser;

fn main() -> std::process::ExitCode {
    let cli = mq_run::Cli::parse();
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
