use glob::glob;
use miette::{IntoDiagnostic, NamedSource};
use mq_lang::CstNodeKind;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tabled::builder::Builder;
use tabled::settings::object::{Cell, Columns, Rows};
use tabled::settings::{Alignment, Color, Style};

/// A `bench_` function discovered in a `.mq` file.
#[derive(Debug, PartialEq)]
struct DiscoveredBench {
    name: String,
    arity: usize,
}

/// Timing summary for a single bench function.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchRecord {
    pub file: String,
    pub name: String,
    pub iterations: usize,
    pub mean_ns: u128,
    pub median_ns: u128,
    pub min_ns: u128,
    pub max_ns: u128,
}

impl BenchRecord {
    fn key(&self) -> String {
        format!("{}::{}", self.file, self.name)
    }
}

/// Output format for bench results.
#[derive(clap::ValueEnum, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum OutputFormat {
    /// Rounded ASCII table.
    #[default]
    Table,
    /// Machine-readable JSON array.
    Json,
    /// Markdown table, suitable for pasting into a PR description.
    Markdown,
}

/// Discovers and times `bench_` functions from `.mq` files.
///
/// A function is a bench if its name starts with `bench_`, or is preceded by a
/// `# @bench` / `# [bench]` comment. Runs sequentially; parallel runs would add
/// scheduling noise to the timings.
pub struct BenchRunner {
    files: Vec<PathBuf>,
    iterations: usize,
    warmup: usize,
    filter: Option<String>,
    format: OutputFormat,
    output: Option<PathBuf>,
    baseline: Option<PathBuf>,
}

impl BenchRunner {
    /// Creates a `BenchRunner` for the given files.
    /// If `files` is empty, globs `**/*.mq` in the current directory.
    pub fn new(files: Vec<PathBuf>) -> Self {
        Self {
            files,
            iterations: 100,
            warmup: 3,
            filter: None,
            format: OutputFormat::default(),
            output: None,
            baseline: None,
        }
    }

    /// Sets the number of timed iterations run per bench (minimum 1).
    pub fn with_iterations(mut self, iterations: usize) -> Self {
        self.iterations = iterations.max(1);
        self
    }

    /// Sets the number of untimed warmup runs performed before timing starts.
    pub fn with_warmup(mut self, warmup: usize) -> Self {
        self.warmup = warmup;
        self
    }

    /// Only runs benches whose (display) name contains this substring, case-insensitively.
    pub fn with_filter(mut self, filter: Option<String>) -> Self {
        self.filter = filter;
        self
    }

    /// Sets the output format (table, JSON, or Markdown).
    pub fn with_format(mut self, format: OutputFormat) -> Self {
        self.format = format;
        self
    }

    /// Writes results to this file instead of stdout.
    pub fn with_output(mut self, output: Option<PathBuf>) -> Self {
        self.output = output;
        self
    }

    /// Compares results against a previous `--format json` run and reports the delta per bench.
    pub fn with_baseline(mut self, baseline: Option<PathBuf>) -> Self {
        self.baseline = baseline;
        self
    }

    /// Discovers and times all bench functions.
    ///
    /// A file that fails to read, compile, or evaluate is reported in place but does not
    /// stop the remaining files from running.
    ///
    /// Returns `Ok(true)` if every discovered bench ran to completion.
    pub fn run(self) -> miette::Result<bool> {
        let bench_files: Vec<PathBuf> = if self.files.is_empty() {
            glob("./**/*.mq")
                .into_diagnostic()?
                .collect::<Result<Vec<_>, _>>()
                .into_diagnostic()?
        } else {
            self.files.clone()
        };

        let mut records: Vec<BenchRecord> = Vec::new();
        let mut any_failed = false;

        for file in &bench_files {
            let content = match fs::read_to_string(file) {
                Ok(content) => content,
                Err(e) => {
                    any_failed = true;
                    eprintln!("# {}\n\n❌ Failed to read file: {e}\n\n---\n", file.display());
                    continue;
                }
            };

            let benches: Vec<DiscoveredBench> = Self::discover_benches(&content)
                .into_iter()
                .filter(|bench| self.matches(bench))
                .collect();

            for bench in &benches {
                if bench.arity != 0 {
                    eprintln!(
                        "⚠ skipping {} in {}: bench functions must take no parameters (found {})",
                        bench.name,
                        file.display(),
                        bench.arity
                    );
                    continue;
                }

                match self.time_bench(file, &content, bench) {
                    Ok(record) => records.push(record),
                    Err(e) => {
                        any_failed = true;
                        eprintln!("{}", Self::render_file_error(file, *e));
                    }
                }
            }
        }

        let baseline = self.load_baseline()?;
        let output = match self.format {
            OutputFormat::Json => format!("{}\n", serde_json::to_string_pretty(&records).into_diagnostic()?),
            OutputFormat::Table => Self::render_table(&records, baseline.as_ref(), self.use_color()),
            OutputFormat::Markdown => Self::render_markdown(&records, baseline.as_ref()),
        };

        match &self.output {
            Some(path) => fs::write(path, output).into_diagnostic()?,
            None => print!("{output}"),
        }

        Ok(!any_failed)
    }

    /// Compiles `bench`'s call once, warms it up, then times `iterations` sequential calls.
    fn time_bench(
        &self,
        file: &Path,
        content: &str,
        bench: &DiscoveredBench,
    ) -> Result<BenchRecord, Box<mq_lang::Error>> {
        let query = format!("{content}\n| {}()", bench.name);
        let mut engine = mq_lang::Engine::with_io(
            mq_lang::DefaultModuleResolver::default(),
            mq_lang::Shared::new(mq_lang::MemIo::default()),
        );
        engine.load_builtin_module();

        if let Some(parent) = file.parent()
            && parent != Path::new("")
        {
            engine.set_search_paths(vec![parent.to_path_buf()]);
        }

        let compiled = engine.compile(&query)?;

        for _ in 0..self.warmup {
            engine.eval_compiled(&compiled, mq_lang::null_input().into_iter())?;
        }

        let mut durations: Vec<Duration> = Vec::with_capacity(self.iterations);
        for _ in 0..self.iterations {
            let start = Instant::now();
            engine.eval_compiled(&compiled, mq_lang::null_input().into_iter())?;
            durations.push(start.elapsed());
        }

        durations.sort();
        let sum_ns: u128 = durations.iter().map(|d| d.as_nanos()).sum();

        Ok(BenchRecord {
            file: file.display().to_string(),
            name: bench.name.clone(),
            iterations: durations.len(),
            mean_ns: sum_ns / durations.len() as u128,
            median_ns: durations[durations.len() / 2].as_nanos(),
            min_ns: durations.first().unwrap().as_nanos(),
            max_ns: durations.last().unwrap().as_nanos(),
        })
    }

    fn load_baseline(&self) -> miette::Result<Option<FxHashMap<String, BenchRecord>>> {
        let Some(path) = &self.baseline else {
            return Ok(None);
        };
        let content = fs::read_to_string(path).into_diagnostic()?;
        let records: Vec<BenchRecord> = serde_json::from_str(&content).into_diagnostic()?;
        Ok(Some(records.into_iter().map(|r| (r.key(), r)).collect()))
    }

    fn render_file_error(file: &Path, mut error: mq_lang::Error) -> String {
        if error.source_code.name().is_empty() {
            error.source_code = NamedSource::new(file.display().to_string(), error.source_code.inner().clone());
        }
        format!(
            "# {}\n\n❌ Failed to run bench\n\n{:?}\n---\n",
            file.display(),
            miette::Report::new(error)
        )
    }

    fn matches(&self, bench: &DiscoveredBench) -> bool {
        match &self.filter {
            Some(filter) => Self::display_name(&bench.name)
                .to_lowercase()
                .contains(&filter.to_lowercase()),
            None => true,
        }
    }

    /// Strips the `bench_` prefix used for the reported display name.
    fn display_name(name: &str) -> &str {
        name.strip_prefix("bench_").unwrap_or(name)
    }

    fn discover_benches(content: &str) -> Vec<DiscoveredBench> {
        let (nodes, _) = mq_lang::parse_recovery(content);
        Self::discover_benches_in(&nodes)
    }

    fn discover_benches_in(nodes: &[mq_lang::Shared<mq_lang::CstNode>]) -> Vec<DiscoveredBench> {
        let mut benches = Vec::new();

        for node in nodes {
            if let CstNodeKind::Module { program, .. } = &node.kind {
                benches.extend(Self::discover_benches_in(program));
                continue;
            }

            let CstNodeKind::Def { name, .. } = &node.kind else {
                continue;
            };
            let func_name = name.to_string();

            if func_name.is_empty() {
                continue;
            }

            let annotated = node.leading_trivia.iter().filter_map(|t| t.comment()).any(|comment| {
                let comment = comment.trim();
                comment == "@bench" || comment == "[bench]"
            });

            if annotated || func_name.starts_with("bench_") {
                benches.push(DiscoveredBench {
                    name: func_name,
                    arity: Self::get_arity(node),
                });
            }
        }

        benches
    }

    /// Returns the number of positional parameters of a `def` node.
    fn get_arity(node: &mq_lang::Shared<mq_lang::CstNode>) -> usize {
        let (sig, _) = node.split_cond_and_program();
        // sig[0] is the function name; the rest are parameter idents.
        sig.len().saturating_sub(1)
    }

    /// Builds the shared header/row strings for the table and Markdown renderers.
    fn result_rows(
        records: &[BenchRecord],
        baseline: Option<&FxHashMap<String, BenchRecord>>,
    ) -> (Vec<String>, Vec<Vec<String>>) {
        let mut header: Vec<String> = ["File", "Bench", "Iterations", "Mean", "Median", "Min", "Max"]
            .into_iter()
            .map(String::from)
            .collect();
        if baseline.is_some() {
            header.push("Δ vs baseline".to_string());
        }

        let rows = records
            .iter()
            .map(|record| {
                let mut row = vec![
                    record.file.clone(),
                    Self::display_name(&record.name).to_string(),
                    record.iterations.to_string(),
                    Self::format_ns(record.mean_ns),
                    Self::format_ns(record.median_ns),
                    Self::format_ns(record.min_ns),
                    Self::format_ns(record.max_ns),
                ];

                if let Some(baseline) = baseline {
                    let delta = match baseline.get(&record.key()) {
                        Some(base) => {
                            let pct = (record.mean_ns as f64 - base.mean_ns as f64) / base.mean_ns as f64 * 100.0;
                            Self::format_delta(pct)
                        }
                        None => "—".to_string(),
                    };
                    row.push(delta);
                }

                row
            })
            .collect();

        (header, rows)
    }

    /// Deltas within this magnitude are marked `≈` (run-to-run noise) rather than ▲/▼.
    const NOISE_THRESHOLD_PCT: f64 = 2.0;

    fn format_delta(pct: f64) -> String {
        let symbol = if pct.abs() < Self::NOISE_THRESHOLD_PCT {
            "≈"
        } else if pct > 0.0 {
            "▲"
        } else {
            "▼"
        };
        format!("{symbol} {pct:+.1}%")
    }

    /// Whether to colorize `render_table`'s output: only when it's going to a real terminal
    /// (never into `--output`/a redirect, which would otherwise embed raw ANSI codes).
    fn use_color(&self) -> bool {
        self.output.is_none() && std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none()
    }

    fn render_table(
        records: &[BenchRecord],
        baseline: Option<&FxHashMap<String, BenchRecord>>,
        colorize: bool,
    ) -> String {
        if records.is_empty() {
            return "No benchmarks found.\n".to_string();
        }

        let (header, rows) = Self::result_rows(records, baseline);
        let delta_col = header.len() - 1;

        let mut builder = Builder::default();
        builder.push_record(header.clone());
        for row in &rows {
            builder.push_record(row.clone());
        }

        let mut table = builder.build();
        table
            .with(Style::rounded())
            .modify(Columns::new(2..header.len()), Alignment::right());

        if colorize {
            table.modify(Rows::first(), Color::FG_CYAN);
            if baseline.is_some() {
                for (i, row) in rows.iter().enumerate() {
                    let color = match row[delta_col].chars().next() {
                        Some('▲') => Color::FG_RED,
                        Some('▼') => Color::FG_GREEN,
                        _ => Color::FG_BRIGHT_BLACK,
                    };
                    table.modify(Cell::new(i + 1, delta_col), color);
                }
            }
        }

        table.to_string() + "\n"
    }

    fn render_markdown(records: &[BenchRecord], baseline: Option<&FxHashMap<String, BenchRecord>>) -> String {
        if records.is_empty() {
            return "No benchmarks found.\n".to_string();
        }

        let (header, rows) = Self::result_rows(records, baseline);
        let sep = vec!["---".to_string(); header.len()];
        [header, sep]
            .into_iter()
            .chain(rows)
            .map(|row| format!("| {} |\n", row.join(" | ")))
            .collect()
    }

    fn format_ns(ns: u128) -> String {
        format!("{:?}", Duration::from_nanos(ns.min(u64::MAX as u128) as u64))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case("def bench_foo():\n  1\nend\n", vec![DiscoveredBench { name: "bench_foo".to_string(), arity: 0 }])]
    #[case(
        "# @bench\ndef my_bench():\n  1\nend\n",
        vec![DiscoveredBench { name: "my_bench".to_string(), arity: 0 }]
    )]
    #[case(
        "# [bench]\ndef another_bench():\n  1\nend\n",
        vec![DiscoveredBench { name: "another_bench".to_string(), arity: 0 }]
    )]
    #[case("def helper():\n  1\nend\n", vec![])]
    #[case(
        "def bench_with_args(x):\n  x\nend\n",
        vec![DiscoveredBench { name: "bench_with_args".to_string(), arity: 1 }]
    )]
    #[case(
        "module m:\n  def bench_in_module():\n  1\nend\nend\n",
        vec![DiscoveredBench { name: "bench_in_module".to_string(), arity: 0 }]
    )]
    fn test_discover_benches(#[case] content: &str, #[case] expected: Vec<DiscoveredBench>) {
        assert_eq!(BenchRunner::discover_benches(content), expected);
    }

    #[rstest]
    #[case(None, "bench_foo", true)]
    #[case(Some("foo"), "bench_foo", true)]
    #[case(Some("FOO"), "bench_foo", true)]
    #[case(Some("bar"), "bench_foo", false)]
    fn test_matches(#[case] filter: Option<&str>, #[case] name: &str, #[case] expected: bool) {
        let runner = BenchRunner::new(vec![]).with_filter(filter.map(str::to_string));
        let bench = DiscoveredBench {
            name: name.to_string(),
            arity: 0,
        };
        assert_eq!(runner.matches(&bench), expected);
    }

    #[test]
    fn test_run_times_a_simple_bench() {
        let dir = std::env::temp_dir().join(format!("mq_bench_simple_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let bench_file = dir.join("bench.mq");
        fs::write(&bench_file, "def bench_add():\n  1 + 1\nend\n").unwrap();

        let passed = BenchRunner::new(vec![bench_file])
            .with_iterations(5)
            .with_warmup(1)
            .run()
            .unwrap();
        assert!(passed);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_run_reports_a_failing_bench_without_stopping_the_run() {
        let dir = std::env::temp_dir().join(format!("mq_bench_failing_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let bench_file = dir.join("bench.mq");
        fs::write(&bench_file, "def bench_fails():\n  error(\"boom\")\nend\n").unwrap();

        let passed = BenchRunner::new(vec![bench_file])
            .with_iterations(2)
            .with_warmup(1)
            .run()
            .unwrap();
        assert!(!passed);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_run_skips_benches_that_take_parameters() {
        let dir = std::env::temp_dir().join(format!("mq_bench_arity_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let bench_file = dir.join("bench.mq");
        fs::write(&bench_file, "def bench_needs_arg(x):\n  x\nend\n").unwrap();

        // Skipped, not failed — nothing errored, so the run still reports success.
        let passed = BenchRunner::new(vec![bench_file]).run().unwrap();
        assert!(passed);

        fs::remove_dir_all(&dir).ok();
    }
}
