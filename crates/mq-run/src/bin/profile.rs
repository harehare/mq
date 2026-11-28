//! Profiler binary: `mq` with `--profile`, `--profile-lines` and `--profile-lines-html`.

#![cfg(feature = "profile")]

use clap::Parser;

#[cfg(feature = "use_mimalloc")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> std::process::ExitCode {
    let args: Vec<std::ffi::OsString> = std::env::args_os().collect();
    if let Some(code) = mq_run::Cli::run_external_subcommand(&args) {
        return code;
    }

    let cli = match mq_run::Cli::parse_from(args).with_env_defaults() {
        Ok(cli) => cli,
        Err(err) => {
            eprintln!("{err:?}");
            return std::process::ExitCode::from(2);
        }
    };
    match cli.run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(err) => {
            let code = mq_run::Cli::error_exit_code(&err);
            if mq_run::Cli::halt_exit_code(&err).is_none() {
                cli.report_error(&err);
            }
            std::process::exit(code)
        }
    }
}
