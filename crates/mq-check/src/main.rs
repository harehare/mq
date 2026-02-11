use std::path::PathBuf;

use clap::Parser;
use mq_check::check_files;

/// Check syntax errors in mq files
#[derive(Parser, Debug)]
#[command(name = "mq-check")]
struct Cli {
    /// Path to the mq file to check
    files: Vec<PathBuf>,
}

fn main() -> miette::Result<()> {
    let cli = Cli::parse();
    check_files(&cli.files)
}
