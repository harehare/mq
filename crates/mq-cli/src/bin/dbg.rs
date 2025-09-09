#![cfg(feature = "debugger")]

use clap::Parser;

fn main() -> miette::Result<()> {
    mq_cli::Cli::parse().run()
}
