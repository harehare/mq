use clap::Parser;

#[cfg(feature = "use_mimalloc")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

// `just build` sets MQ_FORBID_DEBUGGER so `mq` fails to build if `debugger` leaks in.
const _: () = {
    if cfg!(feature = "debugger") && option_env!("MQ_FORBID_DEBUGGER").is_some() {
        panic!("the `mq` binary must not be built with the `debugger` feature, use `mq-dbg`");
    }
};

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
