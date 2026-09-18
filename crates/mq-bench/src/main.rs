mod runner;

use clap::Parser;
use runner::{BenchRunner, OutputFormat};
use std::{path::PathBuf, process::ExitCode};

#[derive(Parser, Debug)]
#[command(name = "mq-bench")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "Benchmark runner for mq — times bench_ functions written in .mq files")]
#[command(after_help = "# Examples:\n\n\
    ## Run all benches in a specific file:\n\
    mq-bench benches.mq\n\n\
    ## Discover and run all *.mq files in the current directory:\n\
    mq-bench\n\n\
    ## Run more iterations with more warmup:\n\
    mq-bench benches.mq --iterations 1000 --warmup 10\n\n\
    ## Only run benches whose name contains \"parse\":\n\
    mq-bench --filter parse\n\n\
    ## Write machine-readable results for CI:\n\
    mq-bench --format json --output results.json\n\n\
    ## Paste results into a PR description:\n\
    mq-bench --format markdown\n\n\
    ## Compare against a previous run:\n\
    mq-bench --baseline results.json")]
struct Cli {
    /// Path(s) to mq bench files.
    /// Defaults to **/*.mq in the current directory when omitted.
    files: Vec<PathBuf>,

    /// Number of timed iterations run per bench.
    #[arg(short = 'n', long, default_value_t = 100)]
    iterations: usize,

    /// Number of untimed warmup runs performed before timing starts.
    #[arg(long, default_value_t = 3)]
    warmup: usize,

    /// Only run benches whose name contains this substring (case-insensitive).
    #[arg(short = 'k', long)]
    filter: Option<String>,

    /// Output format for the results.
    #[arg(long, value_enum, default_value = "table")]
    format: OutputFormat,

    /// Write results to a file instead of stdout.
    #[arg(long)]
    output: Option<PathBuf>,

    /// Compare results against a previous `--format json` run and report the delta per bench.
    #[arg(long)]
    baseline: Option<PathBuf>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    match BenchRunner::new(cli.files)
        .with_iterations(cli.iterations)
        .with_warmup(cli.warmup)
        .with_filter(cli.filter)
        .with_format(cli.format)
        .with_output(cli.output)
        .with_baseline(cli.baseline)
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
