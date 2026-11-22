//! Command-line runner for `.mq` tests.

use clap::{Args, Parser, Subcommand};
use mq_test::{CoverageFormat, MutationFormat, MutationOptions, OutputFormat, TestRunner, run_doc_examples};
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    process::ExitCode,
};

#[derive(Parser, Debug)]
#[command(name = "mq-test")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "Test runner for mq: auto-discovers test_ functions and runs them")]
#[command(args_conflicts_with_subcommands = true)]
#[command(after_help = "# Examples:\n\n\
    ## Run all tests in a specific file:\n\
    mq-test tests.mq\n\n\
    ## Run tests across multiple files:\n\
    mq-test tests.mq other_tests.mq\n\n\
    ## Run all *.mq files under a directory:\n\
    mq-test tests/\n\n\
    ## Discover and run all *.mq files in the current directory:\n\
    mq-test\n\n\
    ## Resolve `include`/`import` from an extra module directory:\n\
    mq-test -L vendor/mqmq tests.mq\n\n\
    ## Run with a line-coverage report:\n\
    mq-test coverage\n\n\
    ## Write an lcov tracefile for CI:\n\
    mq-test coverage --format lcov --output lcov.info\n\n\
    ## Write an HTML coverage report (green/red per-line highlighting):\n\
    mq-test coverage --format html --output coverage.html\n\n\
    ## Write a Markdown coverage report:\n\
    mq-test coverage --format markdown --output coverage.md\n\n\
    ## Write an HTML coverage report and open it in the browser:\n\
    mq-test coverage --format html --output coverage.html --open\n\n\
    ## Only run tests whose name contains \"parse\":\n\
    mq-test --filter parse\n\n\
    ## Only run tests tagged \"smoke\" (see `# @tags(...)` in test files):\n\
    mq-test --tag smoke\n\n\
    ## Run test files in parallel once more than 4 files are discovered:\n\
    mq-test --parallel-threshold 4\n\n\
    ## Accept the current output of every assert_snapshot(...) call as the new golden snapshot:\n\
    mq-test --update-snapshots\n\n\
    ## List discovered tests without running them:\n\
    mq-test list\n\n\
    ## List discovered tests as JSON, e.g. for an editor integration:\n\
    mq-test list --format json\n\n\
    ## Run tests and print per-test results as JSON instead of a Markdown report:\n\
    mq-test --format json\n\n\
    ## Mutation testing of the modules the tests include/import:\n\
    mq-test mutate tests.mq\n\n\
    ## Fail below a 80% mutation score and write an HTML report:\n\
    mq-test mutate --min-score 80 --format html --output mutation.html")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    #[command(flatten)]
    common: CommonArgs,

    #[command(flatten)]
    run: RunOptions,

    /// Output format of the test report.
    #[arg(long, value_enum, default_value = "text")]
    format: OutputFormat,

    /// Deprecated: use `mq-test coverage`.
    #[arg(long, hide = true)]
    coverage: bool,

    /// Deprecated: use `mq-test coverage --format`.
    #[arg(long, value_enum, default_value = "text", requires = "coverage", hide = true)]
    coverage_format: CoverageFormat,

    /// Deprecated: use `mq-test coverage --output`.
    #[arg(long, requires = "coverage", hide = true)]
    coverage_output: Option<PathBuf>,

    /// Deprecated: use `mq-test coverage --open`.
    #[arg(long, requires_all = ["coverage", "coverage_output"], hide = true)]
    open: bool,

    /// Deprecated: use `mq-test list`.
    #[arg(long, hide = true)]
    list: bool,

    /// Deprecated: use `mq-test doc`.
    #[arg(long, conflicts_with_all = ["coverage", "list"], hide = true)]
    doc: bool,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Report line coverage of the `include`d/imported modules exercised by the tests.
    /// Coverage of the test files' own lines is not tracked.
    Coverage(CoverageArgs),
    /// Mutate the modules the tests use and report the mutants the tests fail to catch.
    Mutate(MutateArgs),
    /// Discover and report tests (honoring --filter/--tag) without running them.
    List(ListArgs),
    /// Run the `# => expected` examples in the given `.md` (```mq blocks) and `.mq` (fenced
    /// blocks in comments) files instead of test functions. Local modules resolve only with -L.
    Doc(DocArgs),
}

/// Test selection shared by every subcommand that runs or lists tests.
#[derive(Args, Debug)]
struct CommonArgs {
    /// Path(s) to mq test files or directories (searched for **/*.mq).
    /// Defaults to **/*.mq in the current directory when omitted.
    files: Vec<PathBuf>,

    /// Search modules from the directory. Repeatable.
    #[arg(short = 'L', long = "directory")]
    module_directories: Vec<PathBuf>,

    /// Only run tests whose name contains this substring (case-insensitive).
    #[arg(short = 'k', long)]
    filter: Option<String>,

    /// Only run tests tagged (via `# @tags(...)`) with this tag. Repeatable;
    /// a test runs if it has at least one of the given tags.
    #[arg(long = "tag")]
    tags: Vec<String>,
}

#[derive(Args, Debug)]
struct RunOptions {
    /// Number of test files to process before switching to parallel processing.
    /// Omit to always run files sequentially.
    #[arg(short = 'P', long)]
    parallel_threshold: Option<usize>,

    /// Write the current output of every `assert_snapshot(name, actual)` call as its new
    /// golden snapshot, instead of comparing against the existing one.
    #[arg(long)]
    update_snapshots: bool,
}

#[derive(Args, Debug)]
struct CoverageArgs {
    #[command(flatten)]
    common: CommonArgs,

    #[command(flatten)]
    run: RunOptions,

    /// Report format.
    #[arg(long, value_enum, default_value = "text")]
    format: CoverageFormat,

    /// Write the report to a file instead of stdout.
    #[arg(long)]
    output: Option<PathBuf>,

    /// Open the written report in the OS default application. Requires `--output`.
    #[arg(long, requires = "output")]
    open: bool,

    /// Output format of the test results.
    #[arg(long, value_enum, default_value = "text")]
    test_format: OutputFormat,
}

#[derive(Args, Debug)]
struct MutateArgs {
    #[command(flatten)]
    common: CommonArgs,

    /// Also mutate the bundled `builtin.mq`. Slow: run it with a narrow `--filter`.
    #[arg(long)]
    builtin: bool,

    /// Report format.
    #[arg(long, value_enum, default_value = "text")]
    format: MutationFormat,

    /// Write the report to a file instead of stdout.
    #[arg(long)]
    output: Option<PathBuf>,

    /// Number of mutants to run at once. Default: half the cores, at most four.
    #[arg(long, value_parser = clap::value_parser!(usize))]
    jobs: Option<usize>,

    /// Fail when the mutation score (percent of mutants killed) is below this value.
    #[arg(long, value_parser = clap::value_parser!(u32).range(0..=100))]
    min_score: Option<u32>,
}

#[derive(Args, Debug)]
struct ListArgs {
    #[command(flatten)]
    common: CommonArgs,

    /// Output format.
    #[arg(long, value_enum, default_value = "text")]
    format: OutputFormat,
}

#[derive(Args, Debug)]
struct DocArgs {
    /// `.md` and `.mq` files to check.
    #[arg(required = true)]
    files: Vec<PathBuf>,

    /// Search modules from the directory. Repeatable.
    #[arg(short = 'L', long = "directory")]
    module_directories: Vec<PathBuf>,
}

fn exit_code(result: miette::Result<bool>) -> ExitCode {
    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("{e:?}");
            ExitCode::FAILURE
        }
    }
}

fn runner(common: CommonArgs, run: RunOptions) -> TestRunner {
    TestRunner::new(common.files)
        .with_filter(common.filter)
        .with_tags(common.tags)
        .with_module_directories(common.module_directories)
        .with_parallel_threshold(run.parallel_threshold.unwrap_or(usize::MAX))
        .with_update_snapshots(run.update_snapshots)
}

fn run_doc(files: &[PathBuf], module_directories: &[PathBuf]) -> ExitCode {
    if run_doc_examples(files, module_directories) {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn deprecated(flag: &str, replacement: &str) {
    eprintln!("warning: `{flag}` is deprecated, use `{replacement}`");
}

const SUBCOMMANDS: [&str; 4] = ["coverage", "mutate", "list", "doc"];

/// A lone bare argument naming both a subcommand and an existing directory selects the
/// directory, as before subcommands existed. Pass `./name` or `name .` to be explicit.
fn normalize_args(mut args: Vec<OsString>, is_dir: impl Fn(&str) -> bool) -> Vec<OsString> {
    if args.len() == 2
        && let Some(name) = args[1].to_str()
        && SUBCOMMANDS.contains(&name)
        && is_dir(name)
    {
        args[1] = OsString::from(format!("./{name}"));
    }
    args
}

fn main() -> ExitCode {
    let cli = Cli::parse_from(normalize_args(std::env::args_os().collect(), |name| {
        Path::new(name).is_dir()
    }));

    match cli.command {
        Some(Command::Coverage(args)) => exit_code(
            runner(args.common, args.run)
                .with_coverage(true)
                .with_coverage_format(args.format)
                .with_coverage_output(args.output)
                .with_open(args.open)
                .with_format(args.test_format)
                .run(),
        ),
        Some(Command::Mutate(args)) => exit_code(
            TestRunner::new(args.common.files)
                .with_filter(args.common.filter)
                .with_tags(args.common.tags)
                .with_module_directories(args.common.module_directories)
                .with_mutation(MutationOptions {
                    builtin: args.builtin,
                    format: args.format,
                    output: args.output,
                    jobs: args.jobs,
                    min_score: args.min_score,
                })
                .run(),
        ),
        Some(Command::List(args)) => exit_code(
            runner(
                args.common,
                RunOptions {
                    parallel_threshold: None,
                    update_snapshots: false,
                },
            )
            .with_list(true)
            .with_format(args.format)
            .run(),
        ),
        Some(Command::Doc(args)) => run_doc(&args.files, &args.module_directories),
        None if cli.doc => {
            deprecated("--doc", "mq-test doc");
            if cli.common.files.is_empty() {
                eprintln!("{:?}", miette::miette!("--doc needs at least one file"));
                return ExitCode::FAILURE;
            }
            run_doc(&cli.common.files, &cli.common.module_directories)
        }
        None => {
            if cli.coverage {
                deprecated("--coverage", "mq-test coverage");
            }
            if cli.list {
                deprecated("--list", "mq-test list");
            }
            exit_code(
                runner(cli.common, cli.run)
                    .with_coverage(cli.coverage)
                    .with_coverage_format(cli.coverage_format)
                    .with_coverage_output(cli.coverage_output)
                    .with_open(cli.open)
                    .with_list(cli.list)
                    .with_format(cli.format)
                    .run(),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    fn args(v: &[&str]) -> Vec<OsString> {
        v.iter().map(OsString::from).collect()
    }

    #[rstest]
    #[case(&["mq-test", "list"], true, &["mq-test", "./list"])]
    #[case(&["mq-test", "coverage"], true, &["mq-test", "./coverage"])]
    #[case(&["mq-test", "mutate"], true, &["mq-test", "./mutate"])]
    #[case(&["mq-test", "doc"], true, &["mq-test", "./doc"])]
    #[case(&["mq-test", "list"], false, &["mq-test", "list"])]
    #[case(&["mq-test", "list", "."], true, &["mq-test", "list", "."])]
    #[case(&["mq-test", "list", "--format", "json"], true, &["mq-test", "list", "--format", "json"])]
    #[case(&["mq-test", "--filter", "list"], true, &["mq-test", "--filter", "list"])]
    #[case(&["mq-test", "tests"], true, &["mq-test", "tests"])]
    #[case(&["mq-test"], true, &["mq-test"])]
    fn test_normalize_args(#[case] input: &[&str], #[case] is_dir: bool, #[case] expected: &[&str]) {
        assert_eq!(normalize_args(args(input), |_| is_dir), args(expected));
    }

    #[rstest]
    #[case(&["mq-test", "./list"], false)]
    #[case(&["mq-test", "list", "."], true)]
    fn test_parsed_command(#[case] argv: &[&str], #[case] is_subcommand: bool) {
        assert_eq!(Cli::try_parse_from(argv).unwrap().command.is_some(), is_subcommand);
    }

    #[rstest]
    #[case(&["mq-test", "coverage"], CoverageFormat::Text, OutputFormat::Text)]
    #[case(&["mq-test", "coverage", "--format", "lcov"], CoverageFormat::Lcov, OutputFormat::Text)]
    #[case(&["mq-test", "coverage", "--test-format", "json"], CoverageFormat::Text, OutputFormat::Json)]
    #[case(
        &["mq-test", "coverage", "--format", "lcov", "--output", "l.info", "--test-format", "json"],
        CoverageFormat::Lcov,
        OutputFormat::Json
    )]
    fn test_coverage_formats_are_independent(
        #[case] argv: &[&str],
        #[case] coverage: CoverageFormat,
        #[case] test: OutputFormat,
    ) {
        let Some(Command::Coverage(a)) = Cli::try_parse_from(argv).unwrap().command else {
            panic!("expected coverage subcommand");
        };
        assert_eq!(a.format, coverage);
        assert_eq!(a.test_format, test);
    }
}
