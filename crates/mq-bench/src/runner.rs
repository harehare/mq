use crate::alloc::{self, MemStats};
use crate::stats::{Summary, mann_whitney_p};
use glob::glob;
use indicatif::{MultiProgress, ProgressBar, ProgressDrawTarget, ProgressStyle};
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

/// The engine every bench runs on: an in-memory filesystem and the default module resolver.
type BenchEngine = mq_lang::Engine<mq_lang::DefaultModuleResolver, mq_lang::MemIo>;

/// A `bench_` function discovered in a `.mq` file.
#[derive(Debug, PartialEq)]
struct DiscoveredBench {
    name: String,
    arity: usize,
}

/// Timing summary for a single bench function.
///
/// Timings are per evaluation. Fields after `error` are absent from older result files and
/// default to zero or empty.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchRecord {
    /// Source file of the benchmark.
    pub file: String,
    /// Benchmark name.
    pub name: String,
    /// Number of timed evaluations (samples times `batch`).
    pub iterations: usize,
    /// Mean time per iteration in nanoseconds.
    pub mean_ns: u128,
    /// Median time per iteration in nanoseconds.
    pub median_ns: u128,
    /// Fastest iteration in nanoseconds.
    pub min_ns: u128,
    /// Slowest iteration in nanoseconds.
    pub max_ns: u128,
    /// Error message when the bench failed; timings are zero in that case.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Evaluations timed together in one sample, so a fast bench rises above the clock's resolution.
    #[serde(default)]
    pub batch: usize,
    /// Sample standard deviation in nanoseconds.
    #[serde(default)]
    pub std_dev_ns: f64,
    /// Median absolute deviation in nanoseconds.
    #[serde(default)]
    pub mad_ns: f64,
    /// 95th percentile in nanoseconds.
    #[serde(default)]
    pub p95_ns: u128,
    /// 99th percentile in nanoseconds.
    #[serde(default)]
    pub p99_ns: u128,
    /// Median cold run in nanoseconds: parse, bytecode compile and the first evaluation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cold_ns: Option<u128>,
    /// Heap use of one evaluation. Absent when allocations cannot be counted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mem: Option<MemStats>,
    /// Every sample, per evaluation in nanoseconds, so a later run can test for a significant change.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub samples_ns: Vec<f64>,
}

impl BenchRecord {
    fn key(&self) -> String {
        format!("{}::{}", self.file, self.name)
    }

    fn failed(file: &Path, name: &str, error: String) -> Self {
        Self {
            file: file.display().to_string(),
            name: name.to_string(),
            iterations: 0,
            mean_ns: 0,
            median_ns: 0,
            min_ns: 0,
            max_ns: 0,
            error: Some(error),
            batch: 0,
            std_dev_ns: 0.0,
            mad_ns: 0.0,
            p95_ns: 0,
            p99_ns: 0,
            cold_ns: None,
            mem: None,
            samples_ns: Vec::new(),
        }
    }

    /// Timings summarized from `sampled`.
    fn from_sampled(file: &Path, name: &str, sampled: Sampled, cold_ns: Option<u128>, mem: Option<MemStats>) -> Self {
        let summary = Summary::from_samples(&sampled.samples).expect("a bench takes at least one sample");
        let ns = |value: f64| value.round() as u128;
        Self {
            file: file.display().to_string(),
            name: name.to_string(),
            iterations: sampled.samples.len() * sampled.batch,
            mean_ns: ns(summary.mean),
            median_ns: ns(summary.median),
            min_ns: ns(summary.min),
            max_ns: ns(summary.max),
            error: None,
            batch: sampled.batch,
            std_dev_ns: summary.std_dev,
            mad_ns: summary.mad,
            p95_ns: ns(summary.p95),
            p99_ns: ns(summary.p99),
            cold_ns,
            mem,
            samples_ns: sampled.samples,
        }
    }

    /// Coefficient of variation: the standard deviation relative to the mean.
    fn cv(&self) -> f64 {
        if self.mean_ns == 0 {
            0.0
        } else {
            self.std_dev_ns / self.mean_ns as f64
        }
    }
}

/// Timed samples of one measured operation.
struct Sampled {
    /// Time per evaluation of each sample, in nanoseconds.
    samples: Vec<f64>,
    /// Evaluations timed together in each sample.
    batch: usize,
}

/// What a bench run measures.
#[derive(clap::ValueEnum, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Phase {
    /// Warm evaluation of the compiled program, plus its cold run as a separate column.
    #[default]
    All,
    /// Warm evaluation only.
    Eval,
    /// Cold run only: a fresh parse and bytecode compile followed by the first evaluation.
    Cold,
}

/// How a bench compares with its baseline.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Comparison {
    /// Change of the median, in percent. Positive is slower.
    delta_pct: f64,
    /// Two-sided Mann-Whitney p-value, when both runs recorded their samples.
    p_value: Option<f64>,
    verdict: Verdict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verdict {
    Slower,
    Faster,
    /// Within run-to-run noise.
    Same,
}

impl Comparison {
    /// Default for the smallest change that counts as a difference, in percent.
    ///
    /// Two runs of the same code, one after another, differ by several percent (clock speed,
    /// temperature, background load), and a test over the samples cannot see that offset.
    const DEFAULT_THRESHOLD_PCT: f64 = 5.0;
    /// Significance level of the test.
    const ALPHA: f64 = 0.05;

    /// Compares `record` with `base`. A change counts only when it is at least `threshold_pct`
    /// and, when both runs kept their samples, also statistically significant and larger than
    /// the spread of either run.
    fn new(record: &BenchRecord, base: &BenchRecord, threshold_pct: f64) -> Option<Self> {
        if record.error.is_some() || base.error.is_some() || base.median_ns == 0 {
            return None;
        }
        let delta_pct = (record.median_ns as f64 - base.median_ns as f64) / base.median_ns as f64 * 100.0;
        let p_value = mann_whitney_p(&record.samples_ns, &base.samples_ns);
        let different = match p_value {
            // Samples within a run drift together, so the test alone overstates significance.
            // A real change must also exceed the spread of either run.
            Some(p) => {
                let noise_pct = record.cv().max(base.cv()) * 100.0;
                p < Self::ALPHA && delta_pct.abs() >= threshold_pct.max(noise_pct)
            }
            None => delta_pct.abs() >= threshold_pct,
        };
        let verdict = match (different, delta_pct > 0.0) {
            (false, _) => Verdict::Same,
            (true, true) => Verdict::Slower,
            (true, false) => Verdict::Faster,
        };
        Some(Self {
            delta_pct,
            p_value,
            verdict,
        })
    }

    fn render(&self) -> String {
        let symbol = match self.verdict {
            Verdict::Slower => "▲",
            Verdict::Faster => "▼",
            Verdict::Same => "≈",
        };
        match self.p_value {
            Some(p) if p < 0.001 => format!("{symbol} {:+.1}% (p<0.001)", self.delta_pct),
            Some(p) => format!("{symbol} {:+.1}% (p={p:.3})", self.delta_pct),
            None => format!("{symbol} {:+.1}%", self.delta_pct),
        }
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
    module_directories: Vec<PathBuf>,
    min_sample_time: Duration,
    phase: Phase,
    fail_on_regression: Option<f64>,
    noise_threshold_pct: f64,
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
            module_directories: Vec::new(),
            min_sample_time: Duration::from_millis(1),
            phase: Phase::default(),
            fail_on_regression: None,
            noise_threshold_pct: Comparison::DEFAULT_THRESHOLD_PCT,
        }
    }

    /// Adds extra module search directories, tried after the bench file's own directory.
    /// When empty, the current working directory is searched instead.
    pub fn with_module_directories(mut self, dirs: Vec<PathBuf>) -> Self {
        self.module_directories = dirs;
        self
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

    /// Only runs benches whose name contains this substring, case-insensitively.
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

    /// Sets how long one sample should take at least. A fast bench runs several evaluations per
    /// sample to reach it; zero times every evaluation on its own.
    pub fn with_min_sample_time(mut self, min_sample_time: Duration) -> Self {
        self.min_sample_time = min_sample_time;
        self
    }

    /// Chooses what is measured.
    pub fn with_phase(mut self, phase: Phase) -> Self {
        self.phase = phase;
        self
    }

    /// Sets the smallest change, in percent, reported as faster or slower than the baseline.
    pub fn with_noise_threshold(mut self, percent: f64) -> Self {
        self.noise_threshold_pct = percent.max(0.0);
        self
    }

    /// Fails the run when a bench is significantly slower than its baseline by at least this many percent.
    pub fn with_fail_on_regression(mut self, percent: Option<f64>) -> Self {
        self.fail_on_regression = percent;
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
        // Printed after the progress bars are cleared: `MultiProgress::println` is a no-op
        // when stderr is not a terminal.
        let mut failures: Vec<String> = Vec::new();

        let mut plan: Vec<(&Path, String, Vec<DiscoveredBench>)> = Vec::new();
        for file in &bench_files {
            let content = match fs::read_to_string(file) {
                Ok(content) => content,
                Err(e) => {
                    any_failed = true;
                    failures.push(format!("# {}\n\n❌ Failed to read file: {e}\n\n---\n", file.display()));
                    continue;
                }
            };

            let (benches, skipped): (Vec<_>, Vec<_>) = Self::discover_benches(&content)
                .into_iter()
                .filter(|bench| self.matches(bench))
                .partition(|bench| bench.arity == 0);

            for bench in &skipped {
                eprintln!(
                    "⚠ skipping {} in {}: bench functions must take no parameters (found {})",
                    bench.name,
                    file.display(),
                    bench.arity
                );
            }
            plan.push((file, content, benches));
        }

        // Draws only when stderr is a terminal, so redirected output and `--format json` stay clean.
        // Both bars live for the whole run and share a layout, so the lines neither shift nor flicker.
        let multi = MultiProgress::with_draw_target(ProgressDrawTarget::stderr_with_hz(10));
        let total: usize = plan.iter().map(|(_, _, benches)| benches.len()).sum();
        let bar = multi.add(
            ProgressBar::new((self.warmup + self.iterations) as u64)
                .with_style(Self::progress_style("{bar:30.green/white} {pos}/{len} {msg}")),
        );
        let overall = multi.add(ProgressBar::new(total as u64).with_style(Self::progress_style(
            "{bar:30.cyan/blue} {pos}/{len} benches [{elapsed_precise}]",
        )));

        for (file, content, benches) in &plan {
            for bench in benches {
                bar.reset();
                bar.set_message(bench.name.clone());

                let result = self.time_bench(file, content, bench, &bar);
                overall.inc(1);

                match result {
                    Ok(record) => records.push(record),
                    Err(e) => {
                        any_failed = true;
                        records.push(BenchRecord::failed(file, &bench.name, e.to_string()));
                        failures.push(Self::render_file_error(file, &bench.name, *e));
                    }
                }
            }
        }
        bar.finish_and_clear();
        overall.finish_and_clear();

        for failure in &failures {
            eprintln!("{failure}");
        }

        for record in records
            .iter()
            .filter(|record| record.error.is_none() && record.cv() > Self::HIGH_VARIANCE_CV)
        {
            eprintln!(
                "⚠ {}: high variance (CV {:.0}%), so differences of that size are not meaningful; \
                 try more --iterations, a larger --min-sample-time, or a quieter machine",
                record.name,
                record.cv() * 100.0
            );
        }

        let baseline = self.load_baseline()?;
        if let (Some(limit), Some(baseline)) = (self.fail_on_regression, baseline.as_ref()) {
            for record in &records {
                let comparison = baseline
                    .get(&record.key())
                    .and_then(|base| Comparison::new(record, base, self.noise_threshold_pct));
                if let Some(comparison) = comparison
                    && comparison.verdict == Verdict::Slower
                    && comparison.delta_pct >= limit
                {
                    any_failed = true;
                    eprintln!(
                        "❌ regression in {}: {} (limit {limit}%)",
                        record.name,
                        comparison.render()
                    );
                }
            }
        }

        let output = match self.format {
            OutputFormat::Json => format!("{}\n", serde_json::to_string_pretty(&records).into_diagnostic()?),
            OutputFormat::Table => {
                Self::render_table(&records, baseline.as_ref(), self.noise_threshold_pct, self.use_color())
            }
            OutputFormat::Markdown => Self::render_markdown(&records, baseline.as_ref(), self.noise_threshold_pct),
        };

        match &self.output {
            Some(path) => fs::write(path, output).into_diagnostic()?,
            None => print!("{output}"),
        }

        Ok(!any_failed)
    }

    /// Runs with a coefficient of variation above this are reported as noisy.
    const HIGH_VARIANCE_CV: f64 = 0.10;

    fn progress_style(template: &str) -> ProgressStyle {
        ProgressStyle::with_template(template)
            .unwrap_or_else(|_| ProgressStyle::default_bar())
            .progress_chars("█▉▊▋▌▍▎▏ ")
    }

    /// Cold runs timed per bench: each parses and compiles from scratch, so few are enough.
    const COLD_SAMPLES: usize = 20;
    /// Upper bound on evaluations per sample, so a near-instant bench cannot run for ages.
    const MAX_BATCH: usize = 1_000_000;

    /// Times `bench`'s call. The warm phase compiles once, warms up, then times `iterations`
    /// samples; the cold phase times a fresh parse, bytecode compile and first evaluation.
    /// `progress` is advanced outside the timed region, in batches to keep its overhead low.
    fn time_bench(
        &self,
        file: &Path,
        content: &str,
        bench: &DiscoveredBench,
        progress: &ProgressBar,
    ) -> Result<BenchRecord, Box<mq_lang::Error>> {
        let query = format!("{content}\n| {}()", bench.name);
        let mut engine: BenchEngine = mq_lang::Engine::with_io(
            mq_lang::DefaultModuleResolver::default(),
            mq_lang::Shared::new(mq_lang::MemIo::default()),
        );
        engine.load_builtin_module();

        let mut search_paths = Vec::new();
        if let Some(parent) = file.parent()
            && parent != Path::new("")
        {
            search_paths.push(parent.to_path_buf());
        }
        if self.module_directories.is_empty() {
            search_paths.push(PathBuf::from("."));
        } else {
            search_paths.extend(self.module_directories.iter().cloned());
        }
        engine.set_search_paths(search_paths);

        let cold = |engine: &mut BenchEngine| -> Result<Sampled, Box<mq_lang::Error>> {
            Self::sample(1, Self::COLD_SAMPLES.min(self.iterations), Duration::ZERO, None, || {
                let compiled = engine.compile(&query)?;
                engine.eval_compiled(&compiled, mq_lang::null_input().into_iter())?;
                Ok(())
            })
        };

        if self.phase == Phase::Cold {
            let sampled = cold(&mut engine)?;
            // Fresh compile and first run once more, counted.
            let (counted, mem) = alloc::measure(|| {
                let compiled = engine.compile(&query)?;
                engine.eval_compiled(&compiled, mq_lang::null_input().into_iter())
            });
            counted?;
            return Ok(BenchRecord::from_sampled(file, &bench.name, sampled, None, mem));
        }

        let compiled = engine.compile(&query)?;
        let warm = Self::sample(
            self.warmup,
            self.iterations,
            self.min_sample_time,
            Some(progress),
            || {
                engine.eval_compiled(&compiled, mq_lang::null_input().into_iter())?;
                Ok(())
            },
        )?;
        // Counted apart from the timed samples, so counting never slows them down.
        let (counted, mem) = alloc::measure(|| engine.eval_compiled(&compiled, mq_lang::null_input().into_iter()));
        counted?;

        let cold_ns = match self.phase {
            Phase::All => {
                let samples = cold(&mut engine)?.samples;
                Summary::from_samples(&samples).map(|summary| summary.median.round() as u128)
            }
            _ => None,
        };
        Ok(BenchRecord::from_sampled(file, &bench.name, warm, cold_ns, mem))
    }

    /// Runs `run` `warmup` times untimed, then times `iterations` samples.
    ///
    /// A sample runs `run` as many times as it takes to last `min_sample_time`, judged from one
    /// calibration run, and records the time per run. A fast bench is otherwise lost in the
    /// clock's resolution and in call overhead.
    fn sample(
        warmup: usize,
        iterations: usize,
        min_sample_time: Duration,
        progress: Option<&ProgressBar>,
        mut run: impl FnMut() -> Result<(), Box<mq_lang::Error>>,
    ) -> Result<Sampled, Box<mq_lang::Error>> {
        let batch = ((warmup + iterations) / 100).max(1);
        let mut done = 0usize;
        let mut tick = |n: usize| {
            done += n;
            if done >= batch {
                if let Some(progress) = progress {
                    progress.inc(done as u64);
                }
                done = 0;
            }
        };

        for _ in 0..warmup {
            run()?;
            tick(1);
        }

        let per_sample = if min_sample_time.is_zero() {
            1
        } else {
            let start = Instant::now();
            run()?;
            let one = start.elapsed().max(Duration::from_nanos(1));
            (min_sample_time.as_nanos().div_ceil(one.as_nanos()) as usize).clamp(1, Self::MAX_BATCH)
        };

        let mut samples = Vec::with_capacity(iterations);
        for _ in 0..iterations {
            let start = Instant::now();
            for _ in 0..per_sample {
                run()?;
            }
            samples.push(start.elapsed().as_nanos() as f64 / per_sample as f64);
            tick(1);
        }

        Ok(Sampled {
            samples,
            batch: per_sample,
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

    fn render_file_error(file: &Path, bench: &str, mut error: mq_lang::Error) -> String {
        if error.source_code.name().is_empty() {
            error.source_code = NamedSource::new(file.display().to_string(), error.source_code.inner().clone());
        }
        format!(
            "# {}\n\n❌ Failed to run {bench}\n\n{:?}\n---\n",
            file.display(),
            miette::Report::new(error)
        )
    }

    fn matches(&self, bench: &DiscoveredBench) -> bool {
        match &self.filter {
            Some(filter) => bench.name.to_lowercase().contains(&filter.to_lowercase()),
            None => true,
        }
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
        threshold_pct: f64,
    ) -> (Vec<String>, Vec<Vec<String>>) {
        let show_cold = records.iter().any(|record| record.cold_ns.is_some());
        let show_mem = records.iter().any(|record| record.mem.is_some());
        let mut header: Vec<String> = [
            "File",
            "Bench",
            "Iterations",
            "Mean",
            "Median",
            "Min",
            "p95",
            "Max",
            "CV",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        if show_cold {
            header.push("Cold run".to_string());
        }
        if show_mem {
            header.extend(["Allocs", "Alloc bytes", "Peak"].map(String::from));
        }
        if baseline.is_some() {
            header.push("Δ vs baseline".to_string());
            if show_mem {
                header.push("Δ alloc bytes".to_string());
            }
        }

        let header_len = header.len() - usize::from(baseline.is_some()) * (1 + usize::from(show_mem));
        let rows = records
            .iter()
            .map(|record| {
                let mut row = if record.error.is_some() {
                    let mut row = vec![record.file.clone(), record.name.clone(), "-".to_string()];
                    row.push("failed".to_string());
                    row.resize(header_len, "-".to_string());
                    row
                } else {
                    let iterations = if record.batch > 1 {
                        format!("{}×{}", record.iterations / record.batch, record.batch)
                    } else {
                        record.iterations.to_string()
                    };
                    vec![
                        record.file.clone(),
                        record.name.clone(),
                        iterations,
                        Self::format_ns(record.mean_ns),
                        Self::format_ns(record.median_ns),
                        Self::format_ns(record.min_ns),
                        Self::format_ns(record.p95_ns),
                        Self::format_ns(record.max_ns),
                        format!("{:.1}%", record.cv() * 100.0),
                    ]
                };

                if show_cold {
                    row.push(record.cold_ns.map_or_else(|| "-".to_string(), Self::format_ns));
                }
                if show_mem {
                    match (record.error.is_none(), record.mem) {
                        (true, Some(mem)) => row.extend([
                            mem.allocs.to_string(),
                            Self::format_bytes(mem.bytes),
                            Self::format_bytes(mem.peak_bytes),
                        ]),
                        _ => row.extend(std::iter::repeat_n("-".to_string(), 3)),
                    }
                }
                if let Some(baseline) = baseline {
                    let delta = baseline
                        .get(&record.key())
                        .and_then(|base| Comparison::new(record, base, threshold_pct))
                        .map_or_else(|| "—".to_string(), |comparison| comparison.render());
                    row.push(delta);
                    if show_mem {
                        row.push(Self::mem_delta(record, baseline.get(&record.key())));
                    }
                }

                row
            })
            .collect();

        (header, rows)
    }

    /// Whether to colorize `render_table`'s output: only when it's going to a real terminal
    /// (never into `--output`/a redirect, which would otherwise embed raw ANSI codes).
    fn use_color(&self) -> bool {
        self.output.is_none() && std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none()
    }

    fn render_table(
        records: &[BenchRecord],
        baseline: Option<&FxHashMap<String, BenchRecord>>,
        threshold_pct: f64,
        colorize: bool,
    ) -> String {
        if records.is_empty() {
            return "No benchmarks found.\n".to_string();
        }

        let (header, rows) = Self::result_rows(records, baseline, threshold_pct);
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

    fn render_markdown(
        records: &[BenchRecord],
        baseline: Option<&FxHashMap<String, BenchRecord>>,
        threshold_pct: f64,
    ) -> String {
        if records.is_empty() {
            return "No benchmarks found.\n".to_string();
        }

        let (header, rows) = Self::result_rows(records, baseline, threshold_pct);
        let sep = vec!["---".to_string(); header.len()];
        [header, sep]
            .into_iter()
            .chain(rows)
            .map(|row| format!("| {} |\n", row.join(" | ")))
            .collect()
    }

    /// Change of the bytes allocated against the baseline. Counts are exact, so any change shows.
    fn mem_delta(record: &BenchRecord, base: Option<&BenchRecord>) -> String {
        let (Some(now), Some(before)) = (record.mem, base.and_then(|base| base.mem)) else {
            return "—".to_string();
        };
        if now.bytes == before.bytes {
            return "= 0%".to_string();
        }
        if before.bytes == 0 {
            return "▲ new".to_string();
        }
        let pct = (now.bytes as f64 - before.bytes as f64) / before.bytes as f64 * 100.0;
        format!("{} {pct:+.1}%", if pct > 0.0 { "▲" } else { "▼" })
    }

    fn format_bytes(bytes: u64) -> String {
        const UNITS: [&str; 4] = ["B", "KiB", "MiB", "GiB"];
        let mut value = bytes as f64;
        let mut unit = 0;
        while value >= 1024.0 && unit < UNITS.len() - 1 {
            value /= 1024.0;
            unit += 1;
        }
        if unit == 0 {
            format!("{bytes} B")
        } else {
            format!("{value:.1} {}", UNITS[unit])
        }
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

    /// A record whose samples are `samples`, per evaluation in nanoseconds.
    fn record_with(samples: &[f64]) -> BenchRecord {
        let sampled = Sampled {
            samples: samples.to_vec(),
            batch: 1,
        };
        BenchRecord::from_sampled(Path::new("b.mq"), "bench_x", sampled, None, None)
    }

    fn series(center: f64, spread: f64) -> Vec<f64> {
        (0..30).map(|i| center + (i % 6) as f64 * spread).collect()
    }

    #[rstest]
    #[case::clearly_slower(series(200.0, 1.0), series(100.0, 1.0), Verdict::Slower)]
    #[case::clearly_faster(series(100.0, 1.0), series(200.0, 1.0), Verdict::Faster)]
    #[case::identical(series(100.0, 1.0), series(100.0, 1.0), Verdict::Same)]
    // Medians differ by 20% but the spread swallows it: not significant.
    #[case::noise_wider_than_shift(vec![100.0, 140.0, 80.0, 160.0, 90.0, 150.0, 70.0, 130.0], vec![120.0, 85.0, 150.0, 75.0, 140.0, 95.0, 160.0, 110.0], Verdict::Same)]
    fn comparison_uses_a_significance_test(#[case] now: Vec<f64>, #[case] before: Vec<f64>, #[case] expected: Verdict) {
        let comparison = Comparison::new(&record_with(&now), &record_with(&before), 2.0).unwrap();
        assert_eq!(comparison.verdict, expected, "{comparison:?}");
        assert!(comparison.p_value.is_some());
    }

    #[rstest]
    #[case::small_change_is_noise(101.0, 100.0, Verdict::Same)]
    #[case::large_slowdown(130.0, 100.0, Verdict::Slower)]
    #[case::large_speedup(70.0, 100.0, Verdict::Faster)]
    fn comparison_falls_back_to_a_threshold_without_samples(
        #[case] now: f64,
        #[case] before: f64,
        #[case] expected: Verdict,
    ) {
        let strip = |ns: f64| BenchRecord {
            samples_ns: Vec::new(),
            ..record_with(&[ns, ns])
        };
        let comparison = Comparison::new(&strip(now), &strip(before), 2.0).unwrap();
        assert_eq!(comparison.verdict, expected);
        assert_eq!(comparison.p_value, None);
    }

    #[test]
    fn a_significant_shift_inside_the_runs_own_spread_is_noise() {
        let spread =
            |center: f64| -> Vec<f64> { (0..10).flat_map(|_| [center - 10.0, center, center + 10.0]).collect() };
        let comparison = Comparison::new(&record_with(&spread(107.0)), &record_with(&spread(100.0)), 2.0).unwrap();
        // Rank-wise the shift is unmistakable, yet 7% is less than the roughly 8% spread of each run.
        assert!(comparison.p_value.unwrap() < Comparison::ALPHA, "{comparison:?}");
        assert_eq!(comparison.verdict, Verdict::Same);
    }

    #[rstest]
    #[case::below_threshold(10.0, Verdict::Same)]
    #[case::above_threshold(3.0, Verdict::Slower)]
    fn the_threshold_sets_the_smallest_reported_change(#[case] threshold_pct: f64, #[case] expected: Verdict) {
        // A steady 5% slowdown: significant, far outside the (tiny) spread.
        let comparison = Comparison::new(
            &record_with(&series(105.0, 0.1)),
            &record_with(&series(100.0, 0.1)),
            threshold_pct,
        )
        .unwrap();
        assert_eq!(comparison.verdict, expected, "{comparison:?}");
    }

    #[test]
    fn comparison_skips_failed_runs() {
        let ok = record_with(&series(100.0, 1.0));
        let failed = BenchRecord::failed(Path::new("b.mq"), "bench_x", "boom".to_string());
        assert!(Comparison::new(&failed, &ok, 2.0).is_none());
        assert!(Comparison::new(&ok, &failed, 2.0).is_none());
    }

    #[rstest]
    #[case::slower(Verdict::Slower, 12.34, Some(0.0001), "▲ +12.3% (p<0.001)")]
    #[case::faster(Verdict::Faster, -50.0, Some(0.0123), "▼ -50.0% (p=0.012)")]
    #[case::same_without_p(Verdict::Same, 0.5, None, "≈ +0.5%")]
    fn comparison_renders_the_verdict_and_p_value(
        #[case] verdict: Verdict,
        #[case] delta_pct: f64,
        #[case] p_value: Option<f64>,
        #[case] expected: &str,
    ) {
        let comparison = Comparison {
            delta_pct,
            p_value,
            verdict,
        };
        assert_eq!(comparison.render(), expected);
    }

    #[test]
    fn results_from_before_the_extra_fields_still_load() {
        let json =
            r#"[{"file":"a.mq","name":"bench_a","iterations":10,"mean_ns":5,"median_ns":5,"min_ns":4,"max_ns":9}]"#;
        let records: Vec<BenchRecord> = serde_json::from_str(json).unwrap();
        assert_eq!(
            (records[0].batch, records[0].p95_ns, records[0].samples_ns.len()),
            (0, 0, 0)
        );
        assert_eq!(records[0].cold_ns, None);
    }

    #[test]
    fn sample_batches_a_fast_operation_to_reach_the_minimum_time() {
        let mut calls = 0usize;
        let sampled = BenchRunner::sample(0, 3, Duration::from_millis(2), None, || {
            calls += 1;
            Ok(())
        })
        .unwrap();
        assert!(sampled.batch > 1, "batch = {}", sampled.batch);
        assert_eq!(sampled.samples.len(), 3);
        // One calibration run plus `samples * batch` timed runs.
        assert_eq!(calls, 1 + 3 * sampled.batch);
    }

    #[test]
    fn sample_runs_each_evaluation_alone_without_a_minimum_time() {
        let mut calls = 0usize;
        let sampled = BenchRunner::sample(2, 4, Duration::ZERO, None, || {
            calls += 1;
            Ok(())
        })
        .unwrap();
        assert_eq!(sampled.batch, 1);
        assert_eq!(calls, 2 + 4);
    }

    fn bench_file(tag: &str, source: &str) -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!("mq_bench_{tag}_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("bench.mq");
        fs::write(&file, source).unwrap();
        (dir, file)
    }

    #[rstest]
    #[case::all(Phase::All, true)]
    #[case::eval(Phase::Eval, false)]
    fn phase_decides_whether_the_cold_run_is_reported(#[case] phase: Phase, #[case] has_cold: bool) {
        let (dir, file) = bench_file(&format!("phase_{phase:?}"), "def bench_add():\n  1 + 1\nend\n");
        let out = dir.join("out.json");
        let passed = BenchRunner::new(vec![file])
            .with_iterations(5)
            .with_warmup(1)
            .with_phase(phase)
            .with_format(OutputFormat::Json)
            .with_output(Some(out.clone()))
            .run()
            .unwrap();
        assert!(passed);

        let records: Vec<BenchRecord> = serde_json::from_str(&fs::read_to_string(&out).unwrap()).unwrap();
        assert_eq!(records[0].cold_ns.is_some(), has_cold);
        assert_eq!(records[0].samples_ns.len(), 5);
        assert!(records[0].p95_ns >= records[0].median_ns);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn cold_phase_times_fresh_compiles() {
        let (dir, file) = bench_file("cold", "def bench_add():\n  1 + 1\nend\n");
        let out = dir.join("out.json");
        let passed = BenchRunner::new(vec![file])
            .with_iterations(100)
            .with_phase(Phase::Cold)
            .with_format(OutputFormat::Json)
            .with_output(Some(out.clone()))
            .run()
            .unwrap();
        assert!(passed);

        let records: Vec<BenchRecord> = serde_json::from_str(&fs::read_to_string(&out).unwrap()).unwrap();
        // Capped at `COLD_SAMPLES`, each one unbatched.
        assert_eq!(
            (records[0].samples_ns.len(), records[0].batch),
            (BenchRunner::COLD_SAMPLES, 1)
        );
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn fail_on_regression_fails_only_for_a_significant_slowdown() {
        let (dir, file) = bench_file("regression", "def bench_add():\n  1 + 1\nend\n");
        let baseline_path = dir.join("baseline.json");
        let run = |baseline: &[f64], limit: f64| {
            let mut base = record_with(baseline);
            base.file = file.display().to_string();
            base.name = "bench_add".to_string();
            fs::write(&baseline_path, serde_json::to_string(&[base]).unwrap()).unwrap();
            BenchRunner::new(vec![file.clone()])
                .with_iterations(30)
                .with_warmup(1)
                .with_baseline(Some(baseline_path.clone()))
                .with_fail_on_regression(Some(limit))
                .with_output(Some(dir.join("out.txt")))
                .run()
                .unwrap()
        };

        // A baseline of 1ns per evaluation makes the real run a huge slowdown.
        assert!(!run(&series(1.0, 0.01), 5.0));
        // A baseline of 10s per evaluation makes it faster, which is never a regression.
        assert!(run(&series(1e10, 1.0), 5.0));
        fs::remove_dir_all(&dir).ok();
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

        let record = BenchRecord::failed(Path::new("bench.mq"), "bench_fails", "boom".to_string());
        let (_, rows) = BenchRunner::result_rows(&[record], None, 5.0);
        assert_eq!(rows[0][3], "failed");

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_run_skips_benches_that_take_parameters() {
        let dir = std::env::temp_dir().join(format!("mq_bench_arity_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let bench_file = dir.join("bench.mq");
        fs::write(&bench_file, "def bench_needs_arg(x):\n  x\nend\n").unwrap();

        // Skipped, not failed: nothing errored, so the run still reports success.
        let passed = BenchRunner::new(vec![bench_file]).run().unwrap();
        assert!(passed);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_run_resolves_modules_from_module_directories() {
        let dir = std::env::temp_dir().join(format!("mq_bench_modules_{}", std::process::id()));
        let lib_dir = dir.join("lib");
        let bench_dir = dir.join("bench");
        fs::create_dir_all(&lib_dir).unwrap();
        fs::create_dir_all(&bench_dir).unwrap();
        fs::write(lib_dir.join("helper.mq"), "def helper():\n  1\nend\n").unwrap();
        let bench_file = bench_dir.join("bench.mq");
        fs::write(
            &bench_file,
            "import \"helper\"\n| def bench_helper():\n  helper::helper()\nend\n",
        )
        .unwrap();

        let passed = BenchRunner::new(vec![bench_file])
            .with_iterations(2)
            .with_warmup(0)
            .with_module_directories(vec![lib_dir])
            .run()
            .unwrap();
        assert!(passed);

        fs::remove_dir_all(&dir).ok();
    }
}
