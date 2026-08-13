mod coverage;
mod highlight;
mod html;
mod runner;
mod snapshot;

use clap::Parser;
use coverage::CoverageFormat;
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
    mq-test\n\n\
    ## Run with a line-coverage report:\n\
    mq-test --coverage\n\n\
    ## Write an lcov tracefile for CI:\n\
    mq-test --coverage --coverage-format lcov --coverage-output lcov.info\n\n\
    ## Write an HTML coverage report (green/red per-line highlighting):\n\
    mq-test --coverage --coverage-format html --coverage-output coverage.html\n\n\
    ## Write a Markdown coverage report:\n\
    mq-test --coverage --coverage-format markdown --coverage-output coverage.md\n\n\
    ## Write an HTML coverage report and open it in the browser:\n\
    mq-test --coverage --coverage-format html --coverage-output coverage.html --open\n\n\
    ## Only run tests whose name contains \"parse\":\n\
    mq-test --filter parse\n\n\
    ## Only run tests tagged \"smoke\" (see `# @tags(...)` in test files):\n\
    mq-test --tag smoke\n\n\
    ## Run test files in parallel once more than 4 files are discovered:\n\
    mq-test --parallel-threshold 4\n\n\
    ## Accept the current output of every assert_snapshot(...) call as the new golden snapshot:\n\
    mq-test --update-snapshots")]
struct Cli {
    /// Path(s) to mq test files.
    /// Defaults to **/*.mq in the current directory when omitted.
    files: Vec<PathBuf>,

    /// Collect and report line coverage of the `include`d/imported modules
    /// exercised while running the tests. Coverage of the test files'
    /// own lines is not tracked.
    #[arg(long)]
    coverage: bool,

    /// Report format used when `--coverage` is enabled.
    #[arg(long, value_enum, default_value = "text", requires = "coverage")]
    coverage_format: CoverageFormat,

    /// Write the coverage report to a file instead of stdout.
    #[arg(long, requires = "coverage")]
    coverage_output: Option<PathBuf>,

    /// Open the written coverage report in the OS default application.
    /// Requires `--coverage-output`.
    #[arg(long, requires_all = ["coverage", "coverage_output"])]
    open: bool,

    /// Only run tests whose name contains this substring (case-insensitive).
    #[arg(short = 'k', long)]
    filter: Option<String>,

    /// Only run tests tagged (via `# @tags(...)`) with this tag. Repeatable;
    /// a test runs if it has at least one of the given tags.
    #[arg(long = "tag")]
    tags: Vec<String>,

    /// Number of test files to process before switching to parallel processing.
    /// Omit to always run files sequentially.
    #[arg(short = 'P', long)]
    parallel_threshold: Option<usize>,

    /// Write the current output of every `assert_snapshot(name, actual)` call as its new
    /// golden snapshot, instead of comparing against the existing one.
    #[arg(long)]
    update_snapshots: bool,
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    match runner::TestRunner::new(cli.files)
        .with_coverage(cli.coverage)
        .with_coverage_format(cli.coverage_format)
        .with_coverage_output(cli.coverage_output)
        .with_open(cli.open)
        .with_filter(cli.filter)
        .with_tags(cli.tags)
        .with_parallel_threshold(cli.parallel_threshold.unwrap_or(usize::MAX))
        .with_update_snapshots(cli.update_snapshots)
        .run()
    {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("{e:?}");
            ExitCode::FAILURE
        }
    }
}
