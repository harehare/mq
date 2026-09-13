#![cfg(feature = "debugger")]

use clap::Parser;

fn main() -> std::process::ExitCode {
    let cli = mq_run::Cli::parse();
    match cli.run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(err) => {
            cli.report_error(&err);
            std::process::ExitCode::FAILURE
        }
    }
}
