use clap::Parser;

#[cfg(feature = "use_mimalloc")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> miette::Result<()> {
    mq_cli::Cli::parse().run()
}
