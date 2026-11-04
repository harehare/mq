//! Command-line runner for `bench_` functions in `.mq` files.

mod alloc;
mod runner;
mod stats;

use clap::Parser;
use runner::{BenchRunner, OutputFormat, Phase};
use std::{path::PathBuf, process::ExitCode, time::Duration};

// Same allocator as the `mq` binary, so timings match what users see. The wrapper only counts
// while a bench's heap use is being measured.
#[global_allocator]
static GLOBAL: alloc::CountingAlloc<mimalloc::MiMalloc> = alloc::CountingAlloc(mimalloc::MiMalloc);

#[derive(Parser, Debug)]
#[command(name = "mq-bench")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "Benchmark runner for mq: times bench_ functions written in .mq files")]
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
    mq-bench --baseline results.json\n\n\
    ## Fail CI when a bench is significantly more than 5% slower:\n\
    mq-bench --baseline results.json --fail-on-regression 5\n\n\
    ## Measure parse and bytecode compile cost instead of warm evaluation:\n\
    mq-bench --phase cold")]
struct Cli {
    /// Path(s) to mq bench files.
    /// Defaults to **/*.mq in the current directory when omitted.
    files: Vec<PathBuf>,

    /// Additional directory to search for imported modules. Repeatable.
    /// The bench file's own directory is always searched first; when omitted,
    /// the current directory is searched as well.
    #[arg(short = 'L', long = "directory")]
    module_directories: Option<Vec<PathBuf>>,

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
    /// The change is tested for significance when the previous run recorded its samples.
    #[arg(long)]
    baseline: Option<PathBuf>,

    /// Exit with a failure when a bench is significantly slower than the baseline by at least this many percent.
    #[arg(long, value_name = "PERCENT", requires = "baseline")]
    fail_on_regression: Option<f64>,

    /// Smallest change, in percent, reported as faster or slower than the baseline. Runs of the same
    /// code differ by a few percent, so smaller changes show as noise (≈).
    #[arg(long, value_name = "PERCENT", default_value_t = 5.0)]
    noise_threshold: f64,

    /// Minimum duration of one timed sample, in milliseconds. A fast bench runs several
    /// evaluations per sample to reach it. 0 times every evaluation on its own.
    #[arg(long, value_name = "MS", default_value_t = 1.0)]
    min_sample_time: f64,

    /// What to measure: warm evaluation (with the cold run as an extra column), warm evaluation only,
    /// or the cold run only (fresh parse, bytecode compile and first evaluation).
    #[arg(long, value_enum, default_value = "all")]
    phase: Phase,
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    match BenchRunner::new(cli.files)
        .with_module_directories(cli.module_directories.unwrap_or_default())
        .with_iterations(cli.iterations)
        .with_warmup(cli.warmup)
        .with_filter(cli.filter)
        .with_format(cli.format)
        .with_output(cli.output)
        .with_baseline(cli.baseline)
        .with_fail_on_regression(cli.fail_on_regression)
        .with_min_sample_time(Duration::from_secs_f64(cli.min_sample_time.max(0.0) / 1000.0))
        .with_phase(cli.phase)
        .with_noise_threshold(cli.noise_threshold)
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
