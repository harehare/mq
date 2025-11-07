#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use clap::Parser;

fn main() -> miette::Result<()> {
    mq_cli::Cli::parse().run()
}
