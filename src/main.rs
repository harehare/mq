use clap::Parser;

mod cli;

fn main() -> miette::Result<()> {
    let cli = cli::Cli::parse();

    env_logger::Builder::new()
        .filter_level(cli.verbose.log_level_filter())
        .init();
    log::debug!("cli: {cli:?}");

    cli.run()
}
