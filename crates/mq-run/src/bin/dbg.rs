#![cfg(feature = "debugger")]

use clap::Parser;

fn main() -> std::process::ExitCode {
    let cli = mq_run::Cli::parse();
    match cli.run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(err) => {
            if let Some(code) = mq_run::Cli::halt_exit_code(&err) {
                std::process::exit(code);
            }
            cli.report_error(&err);
            std::process::ExitCode::FAILURE
        }
    }
}
