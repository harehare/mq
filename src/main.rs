use clap::Parser;

mod cli;

fn main() -> miette::Result<()> {
    cli::Cli::parse().run()
}
