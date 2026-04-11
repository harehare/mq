mod runner;

use clap::Parser;
use std::{path::PathBuf, process::ExitCode};

#[derive(Parser, Debug)]
#[command(name = "mq-test")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "Test runner for mq — auto-discovers test_ functions and runs them")]
#[command(after_help = "# Examples:\n\n\
    ## Run all tests in a specific file:\n\
    mq-test tests.mq\n\n\
    ## Run tests across multiple files:\n\
    mq-test tests.mq other_tests.mq\n\n\
    ## Discover and run all *.mq files in the current directory:\n\
    mq-test")]
struct Cli {
    /// Path(s) to mq test files.
    /// Defaults to **/*.mq in the current directory when omitted.
    files: Vec<PathBuf>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    if let Err(e) = runner::TestRunner::new(cli.files).run() {
        eprintln!("{e:?}");
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
