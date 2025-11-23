#![cfg(feature = "debugger")]

use clap::Parser;

fn main() -> miette::Result<()> {
    mq_run::Cli::parse().run()
}
