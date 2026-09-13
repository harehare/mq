use clap::Parser;

#[cfg(feature = "use_mimalloc")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

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
