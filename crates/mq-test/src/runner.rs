use glob::glob;
use miette::{IntoDiagnostic, NamedSource};
use mq_lang::{CstNodeKind, CstTrivia, RuntimeValue};
use rayon::prelude::*;
use rustc_hash::{FxHashMap, FxHashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::coverage::{self, CoverageData, CoverageFormat, CoverageHandler, FileCoverage};
use crate::snapshot;

/// Number of iterations a `# @property(generators)` test runs when no count is given.
const DEFAULT_PROPERTY_CASES: &str = "100";

/// Parsed test annotation from a leading comment.
#[derive(Debug, PartialEq)]
enum TestAnnotation {
    Test,
    Parametrize {
        params_expr: String,
    },
    /// `# @property(count, generators)` (or `# @property(generators)`, defaulting `count`
    /// to [`DEFAULT_PROPERTY_CASES`]). `generators` must evaluate to an array of one
    /// `gen::`-style generator (see `gen.mq`) per test parameter.
    Property {
        count_expr: String,
        generators_expr: String,
    },
    Tags(Vec<String>),
}

/// Output format for `--list` and for a normal test run.
#[derive(clap::ValueEnum, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum OutputFormat {
    /// Human-readable text, suitable for a terminal.
    #[default]
    Text,
    /// Machine-readable JSON, suitable for editor/CI integrations.
    Json,
}

/// A test function discovered in a `.mq` file.
#[derive(Debug, PartialEq)]
enum DiscoveredTest {
    Simple {
        name: String,
        tags: Vec<String>,
    },
    Parametrized {
        name: String,
        params_expr: String,
        arity: usize,
        tags: Vec<String>,
    },
    /// A `# @property(count, generators)` test. Kept distinct from `Parametrized` since each
    /// generated case needs its own shrink search on failure. See
    /// `TestRunner::build_property_case_expr`.
    Property {
        name: String,
        count_expr: String,
        generators_expr: String,
        arity: usize,
        tags: Vec<String>,
    },
}

impl DiscoveredTest {
    fn name(&self) -> &str {
        match self {
            DiscoveredTest::Simple { name, .. } => name,
            DiscoveredTest::Parametrized { name, .. } => name,
            DiscoveredTest::Property { name, .. } => name,
        }
    }

    fn tags(&self) -> &[String] {
        match self {
            DiscoveredTest::Simple { tags, .. } => tags,
            DiscoveredTest::Parametrized { tags, .. } => tags,
            DiscoveredTest::Property { tags, .. } => tags,
        }
    }

    /// Machine-readable test kind, used by `mq-test --list --format json`.
    fn kind(&self) -> &'static str {
        match self {
            DiscoveredTest::Simple { .. } => "simple",
            DiscoveredTest::Parametrized { .. } => "parametrized",
            DiscoveredTest::Property { .. } => "property",
        }
    }
}

/// Discovers and runs mq test functions from `.mq` files.
///
/// A function is treated as a test if its name starts with `test_`, it is
/// preceded by `# @test` / `# [test]`, or preceded by `# @parametrize(expr)`.
/// The runner auto-generates the `run_tests(...)` call from discovered tests.
pub struct TestRunner {
    files: Vec<PathBuf>,
    coverage: bool,
    coverage_format: CoverageFormat,
    coverage_output: Option<PathBuf>,
    open: bool,
    filter: Option<String>,
    tags: Vec<String>,
    parallel_threshold: usize,
    update_snapshots: bool,
    list: bool,
    format: OutputFormat,
    module_directories: Vec<PathBuf>,
    step_handler: Option<std::sync::Arc<dyn mq_lang::DebuggerHandler>>,
}

/// Lets one handler be installed on the engine of every test file.
#[derive(Debug)]
struct SharedStepHandler(std::sync::Arc<dyn mq_lang::DebuggerHandler>);

impl mq_lang::DebuggerHandler for SharedStepHandler {
    fn on_step(&self, context: &mq_lang::DebugContext) -> mq_lang::DebuggerAction {
        self.0.on_step(context)
    }
}

impl TestRunner {
    /// Creates a `TestRunner` for the given files.
    /// If `files` is empty, globs `**/*.mq` in the current directory.
    pub fn new(files: Vec<PathBuf>) -> Self {
        Self {
            files,
            coverage: false,
            coverage_format: CoverageFormat::default(),
            coverage_output: None,
            open: false,
            filter: None,
            tags: Vec::new(),
            parallel_threshold: usize::MAX,
            update_snapshots: false,
            list: false,
            format: OutputFormat::default(),
            module_directories: Vec::new(),
            step_handler: None,
        }
    }

    /// Calls `handler` before every statement the tests execute, including those in the modules
    /// they use. Ignored when coverage is on, which needs the same hook.
    // Used by library consumers (the type checker's conformance test), not by the binary.
    #[allow(dead_code)]
    pub fn with_step_handler(mut self, handler: std::sync::Arc<dyn mq_lang::DebuggerHandler>) -> Self {
        self.step_handler = Some(handler);
        self
    }

    /// Enables line-coverage tracking of `include`d/imported modules.
    /// The test files' own lines are not tracked.
    pub fn with_coverage(mut self, coverage: bool) -> Self {
        self.coverage = coverage;
        self
    }

    /// Sets the report format used when coverage is enabled.
    pub fn with_coverage_format(mut self, format: CoverageFormat) -> Self {
        self.coverage_format = format;
        self
    }

    /// Sets an output file for the coverage report. When `None`, the report
    /// is printed to stdout.
    pub fn with_coverage_output(mut self, output: Option<PathBuf>) -> Self {
        self.coverage_output = output;
        self
    }

    /// Opens the written coverage report in the OS default application after `run()`.
    pub fn with_open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    /// Only runs tests whose (display) name contains this substring, case-insensitively.
    pub fn with_filter(mut self, filter: Option<String>) -> Self {
        self.filter = filter;
        self
    }

    /// Only runs tests tagged (via `# @tags(...)`) with at least one of the given tags.
    /// An empty list runs tests regardless of tags.
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    /// Runs test files in parallel once more than this many files are discovered.
    pub fn with_parallel_threshold(mut self, parallel_threshold: usize) -> Self {
        self.parallel_threshold = parallel_threshold;
        self
    }

    /// When `true`, `assert_snapshot(name, actual)` writes `actual` as the new golden
    /// snapshot instead of comparing against it, for every snapshot exercised by the run.
    pub fn with_update_snapshots(mut self, update_snapshots: bool) -> Self {
        self.update_snapshots = update_snapshots;
        self
    }

    /// When `true`, discovers and reports tests without running them.
    pub fn with_list(mut self, list: bool) -> Self {
        self.list = list;
        self
    }

    /// Sets the report format for `--list` and for a normal test run.
    pub fn with_format(mut self, format: OutputFormat) -> Self {
        self.format = format;
        self
    }

    /// Module search paths for `file`: its own directory first, then the `-L` directories.
    /// `None` keeps the engine defaults.
    fn search_paths(&self, file: &Path) -> Option<Vec<PathBuf>> {
        let parent = file.parent().filter(|parent| *parent != Path::new(""));
        if parent.is_none() && self.module_directories.is_empty() {
            return None;
        }
        let mut paths = vec![parent.unwrap_or(Path::new(".")).to_path_buf()];
        paths.extend(self.module_directories.iter().cloned());
        Some(paths)
    }

    /// Adds directories searched for `include`/`import`ed modules, after the
    /// directory of each test file.
    pub fn with_module_directories(mut self, dirs: Vec<PathBuf>) -> Self {
        self.module_directories = dirs;
        self
    }

    /// Discovers and executes all test functions.
    ///
    /// A file that fails to read, parse, or evaluate is reported in place but does not
    /// stop the remaining files from running.
    ///
    /// Returns `Ok(true)` if every executed test passed and every file ran.
    pub fn run(self) -> miette::Result<bool> {
        let test_files: Vec<PathBuf> = if self.files.is_empty() {
            glob("./**/*.mq")
                .into_diagnostic()?
                .collect::<Result<Vec<_>, _>>()
                .into_diagnostic()?
        } else {
            self.files.clone()
        };

        if self.list {
            return self.run_list(&test_files);
        }

        // Merged across all test files, so a shared module gets combined coverage.
        let coverage_data = CoverageData::default();
        // Resolved module-name -> file path, filled in as each engine resolves imports.
        // Behind a `Mutex` so files can be run in parallel.
        let module_paths: Mutex<FxHashMap<String, PathBuf>> = Mutex::new(FxHashMap::default());
        let any_failed = AtomicBool::new(false);
        // Files that failed to read/parse/evaluate, with the rendered error. Printed together
        // after every file has run, so one broken file cannot bury another's report.
        let file_errors: Mutex<Vec<(PathBuf, String)>> = Mutex::new(Vec::new());
        // Files that ran but had at least one failing test.
        let failed_files: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
        // Per-file JSON test results, only populated (and only printed) when `format` is
        // `Json`, kept separate from `file_errors` because it needs the full per-test
        // detail (name/status/duration/error), not just the failing file's path.
        let json_results: Mutex<Vec<serde_json::Value>> = Mutex::new(Vec::new());

        let run_file = |file: &PathBuf| {
            let content = match fs::read_to_string(file) {
                Ok(content) => content,
                Err(e) => {
                    any_failed.store(true, Ordering::Relaxed);
                    file_errors.lock().unwrap().push((
                        file.clone(),
                        format!("# {}\n\n❌ Failed to read file: {e}\n\n---\n", file.display()),
                    ));
                    if self.format == OutputFormat::Json {
                        json_results.lock().unwrap().push(serde_json::json!({
                            "file": file.display().to_string(),
                            "error": e.to_string(),
                        }));
                    }
                    return;
                }
            };
            let tests: Vec<DiscoveredTest> = Self::discover_tests(&content)
                .into_iter()
                .filter(|test| self.matches(test))
                .collect();
            if tests.is_empty() {
                return;
            }

            let query = match self.format {
                OutputFormat::Text => Self::build_test_query(&content, &tests),
                OutputFormat::Json => Self::build_test_query_with_fn(&content, &tests, "run_tests_data"),
            };
            let mut engine = mq_lang::Engine::with_io(
                mq_lang::DefaultModuleResolver::default(),
                mq_lang::Shared::new(mq_lang::MemIo::default()),
            );
            engine.load_builtin_module();
            engine.define_string_value("TEST_FILE", file.to_string_lossy().as_ref());

            // Resolve relative `include` statements in the test file, then fall back to `-L` dirs.
            if let Some(paths) = self.search_paths(file) {
                engine.set_search_paths(paths);
            }

            {
                let snapshot_file = file.clone();
                let update_snapshots = self.update_snapshots;
                engine.register_fn(
                    "assert_snapshot",
                    move |name: String, actual: String| -> mq_lang::HostFnResult {
                        Ok(snapshot::check_snapshot(
                            &snapshot_file,
                            &name,
                            &actual,
                            update_snapshots,
                        ))
                    },
                );
            }

            if self.coverage || self.step_handler.is_some() {
                match &self.step_handler {
                    Some(handler) if !self.coverage => {
                        engine.set_debugger_handler(Box::new(SharedStepHandler(std::sync::Arc::clone(handler))));
                    }
                    _ => engine.set_debugger_handler(Box::new(CoverageHandler(coverage_data.clone()))),
                }
                let debugger = engine.debugger();
                debugger.write().unwrap().activate();
                debugger
                    .write()
                    .unwrap()
                    .set_command(mq_lang::DebuggerCommand::StepInto);
            }

            let before_modules: FxHashSet<String> = if self.coverage {
                coverage_data.snapshot().keys().cloned().collect()
            } else {
                FxHashSet::default()
            };

            let input = mq_lang::null_input();
            match engine.eval(&query, input.into_iter()) {
                Ok(result) => {
                    match self.format {
                        OutputFormat::Text => {
                            let passed = matches!(result.values().first(), Some(RuntimeValue::Boolean(true)));
                            if !passed {
                                any_failed.store(true, Ordering::Relaxed);
                                failed_files.lock().unwrap().push(file.clone());
                            }
                        }
                        OutputFormat::Json => {
                            // `run_tests_data` returns an array of `{name, status, duration,
                            // error}` dicts (see `test.mq`); `to_json_value` turns that
                            // straight into the JSON shape the extension expects.
                            let tests_json = result
                                .values()
                                .first()
                                .cloned()
                                .map(RuntimeValue::to_json_value)
                                .unwrap_or_else(|| serde_json::Value::Array(Vec::new()));
                            let file_failed = tests_json.as_array().is_some_and(|tests| {
                                tests
                                    .iter()
                                    .any(|t| t.get("status").and_then(|s| s.as_str()) == Some("failed"))
                            });
                            if file_failed {
                                any_failed.store(true, Ordering::Relaxed);
                                failed_files.lock().unwrap().push(file.clone());
                            }
                            json_results.lock().unwrap().push(serde_json::json!({
                                "file": file.display().to_string(),
                                "tests": tests_json,
                            }));
                        }
                    }

                    if self.coverage {
                        let mut module_paths = module_paths.lock().unwrap();
                        for module_name in coverage_data.snapshot().keys() {
                            if before_modules.contains(module_name) {
                                continue;
                            }
                            module_paths.entry(module_name.clone()).or_insert_with(|| {
                                engine
                                    .get_module_path(module_name)
                                    .map(PathBuf::from)
                                    .unwrap_or_else(|_| PathBuf::from(module_name))
                            });
                        }
                    }
                }
                Err(e) => {
                    any_failed.store(true, Ordering::Relaxed);
                    if self.format == OutputFormat::Json {
                        json_results.lock().unwrap().push(serde_json::json!({
                            "file": file.display().to_string(),
                            "error": e.to_string(),
                        }));
                    }
                    file_errors
                        .lock()
                        .unwrap()
                        .push((file.clone(), Self::render_file_error(file, *e)));
                }
            }
        };

        if test_files.len() > self.parallel_threshold {
            test_files.par_iter().for_each(run_file);
        } else {
            test_files.iter().for_each(run_file);
        }

        if self.coverage {
            let module_paths = module_paths.lock().unwrap();
            let mut file_coverages: Vec<FileCoverage> = coverage_data
                .snapshot()
                .into_iter()
                .map(|(module_name, hits)| {
                    let executable = coverage::executable_lines(&hits.code);
                    let file = module_paths
                        .get(&module_name)
                        .cloned()
                        .unwrap_or_else(|| PathBuf::from(&module_name));
                    FileCoverage::new(file, executable, hits.lines, hits.code)
                })
                .collect();
            file_coverages.sort_by(|a, b| a.file.cmp(&b.file));

            let report = match self.coverage_format {
                CoverageFormat::Text => coverage::format_text_report(&file_coverages),
                CoverageFormat::Lcov => coverage::format_lcov_report(&file_coverages),
                CoverageFormat::Html => coverage::format_html_report(&file_coverages),
                CoverageFormat::Markdown => coverage::format_markdown_report(&file_coverages),
                CoverageFormat::Json => coverage::format_json_report(&file_coverages),
                CoverageFormat::Cobertura => coverage::format_cobertura_report(&file_coverages),
            };

            match &self.coverage_output {
                Some(path) => {
                    fs::write(path, report).into_diagnostic()?;
                    if self.open {
                        open_in_default_app(path)?;
                    }
                }
                None => print!("{report}"),
            }
        }

        let mut file_errors = file_errors.into_inner().unwrap();
        let mut failed_files = failed_files.into_inner().unwrap();
        // Parallel runs finish in arbitrary order; sort so the output is stable.
        file_errors.sort_by(|a, b| a.0.cmp(&b.0));
        failed_files.sort();
        for (_, message) in &file_errors {
            eprintln!("{message}");
        }
        if test_files.len() > 1 && !(file_errors.is_empty() && failed_files.is_empty()) {
            eprintln!("{}", Self::format_rollup(&failed_files, &file_errors, test_files.len()));
        }

        if self.format == OutputFormat::Json {
            // Printed once at the end, after every (possibly parallel) file has finished,
            // rather than incrementally per file: a single JSON document is much easier
            // for a caller like the VS Code extension to parse than an interleaved stream.
            println!("{}", Self::format_run_json_report(json_results.into_inner().unwrap()));
        }

        Ok(!any_failed.load(Ordering::Relaxed))
    }

    /// One-glance list of the files that went wrong, printed after every file has run.
    fn format_rollup(failed_files: &[PathBuf], file_errors: &[(PathBuf, String)], total: usize) -> String {
        let mut lines = vec![format!(
            "{} of {total} file(s) did not pass: {} with failing tests, {} failed to run",
            failed_files.len() + file_errors.len(),
            failed_files.len(),
            file_errors.len()
        )];
        lines.extend(
            failed_files
                .iter()
                .map(|f| format!("  ❌ {} (tests failed)", f.display())),
        );
        lines.extend(
            file_errors
                .iter()
                .map(|(f, _)| format!("  ⚠ {} (failed to run)", f.display())),
        );
        lines.join("\n")
    }

    /// Wraps the per-file JSON results (each already `{"file": ..., "tests": [...]}` or
    /// `{"file": ..., "error": ...}`) collected while running in one `{"files": [...]}`
    /// document. Pulled out of [`Self::run`] as a pure function so it can be tested directly.
    fn format_run_json_report(json_results: Vec<serde_json::Value>) -> String {
        let report = serde_json::json!({ "files": json_results });
        serde_json::to_string_pretty(&report).expect("test report is always serializable")
    }

    /// Discovers tests (honoring `--filter`/`--tag`) without running them, and reports them in
    /// the requested `--format`. A file that fails to read is reported to stderr and skipped;
    /// listing itself always succeeds, since "no tests found" isn't a failure the way a failed
    /// assertion is.
    fn run_list(&self, test_files: &[PathBuf]) -> miette::Result<bool> {
        let mut discovered: Vec<(PathBuf, Vec<DiscoveredTest>)> = Vec::new();

        for file in test_files {
            let content = match fs::read_to_string(file) {
                Ok(content) => content,
                Err(e) => {
                    eprintln!("# {}\n\n❌ Failed to read file: {e}\n\n---\n", file.display());
                    continue;
                }
            };
            let tests: Vec<DiscoveredTest> = Self::discover_tests(&content)
                .into_iter()
                .filter(|test| self.matches(test))
                .collect();
            if tests.is_empty() {
                continue;
            }
            discovered.push((file.clone(), tests));
        }

        println!("{}", Self::format_list_report(&discovered, self.format));

        Ok(true)
    }

    /// Renders discovered tests in the given `format`. Pulled out of [`Self::run_list`] as a
    /// pure function (no I/O) so it can be exercised directly in tests without capturing
    /// stdout.
    fn format_list_report(discovered: &[(PathBuf, Vec<DiscoveredTest>)], format: OutputFormat) -> String {
        match format {
            OutputFormat::Text => {
                // A plain, greppable listing: one file per line, followed by its tests
                // indented with their kind and (if any) tags, meant for a human at a
                // terminal, not for parsing, so the exact spacing isn't a contract.
                discovered
                    .iter()
                    .map(|(file, tests)| {
                        let test_lines = tests
                            .iter()
                            .map(|test| {
                                let display = Self::display_name(test.name());
                                let tags = if test.tags().is_empty() {
                                    String::new()
                                } else {
                                    format!(" [{}]", test.tags().join(", "))
                                };
                                format!("  {display} ({}){tags}", test.kind())
                            })
                            .collect::<Vec<_>>()
                            .join("\n");
                        format!("{}\n{test_lines}", file.display())
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            }
            OutputFormat::Json => {
                // One JSON document describing every discovered file and its tests, meant
                // for an editor (e.g. the VS Code extension's Test Explorer integration) to
                // build a test tree from without having to run anything first.
                let files: Vec<serde_json::Value> = discovered
                    .iter()
                    .map(|(file, tests)| {
                        let tests: Vec<serde_json::Value> = tests
                            .iter()
                            .map(|test| {
                                serde_json::json!({
                                    "name": Self::display_name(test.name()),
                                    "kind": test.kind(),
                                    "tags": test.tags(),
                                })
                            })
                            .collect();
                        serde_json::json!({
                            "file": file.display().to_string(),
                            "tests": tests,
                        })
                    })
                    .collect();
                let report = serde_json::json!({ "files": files });
                serde_json::to_string_pretty(&report).expect("list report is always serializable")
            }
        }
    }

    fn render_file_error(file: &Path, mut error: mq_lang::Error) -> String {
        if error.source_code.name().is_empty() {
            error.source_code = NamedSource::new(file.display().to_string(), error.source_code.inner().clone());
        }
        format!(
            "# {}\n\n❌ Failed to run tests\n\n{:?}\n---\n",
            file.display(),
            miette::Report::new(error)
        )
    }

    fn matches(&self, test: &DiscoveredTest) -> bool {
        let name_matches = match &self.filter {
            Some(filter) => Self::display_name(test.name())
                .to_lowercase()
                .contains(&filter.to_lowercase()),
            None => true,
        };
        let tags_match = self.tags.is_empty() || test.tags().iter().any(|tag| self.tags.contains(tag));

        name_matches && tags_match
    }

    /// Strips the `test_` prefix used for the reported display name.
    fn display_name(name: &str) -> &str {
        name.strip_prefix("test_").unwrap_or(name)
    }

    fn discover_tests(content: &str) -> Vec<DiscoveredTest> {
        let (nodes, _) = mq_lang::parse_recovery(content);
        Self::discover_tests_in(&nodes)
    }

    fn discover_tests_in(nodes: &[mq_lang::Shared<mq_lang::CstNode>]) -> Vec<DiscoveredTest> {
        let mut tests = Vec::new();

        for node in nodes {
            if let CstNodeKind::Module { program, .. } = &node.kind {
                tests.extend(Self::discover_tests_in(program));
                continue;
            }

            let CstNodeKind::Def { name, .. } = &node.kind else {
                continue;
            };
            let func_name = name.to_string();

            if func_name.is_empty() {
                continue;
            }

            match Self::find_test_annotation(&node.leading_trivia) {
                Some(TestAnnotation::Test) => {
                    tests.push(DiscoveredTest::Simple {
                        name: func_name.clone(),
                        tags: Self::collect_tags(&node.leading_trivia),
                    });
                }
                Some(TestAnnotation::Parametrize { params_expr }) => {
                    let arity = Self::get_arity(node);
                    tests.push(DiscoveredTest::Parametrized {
                        name: func_name.clone(),
                        params_expr,
                        arity,
                        tags: Self::collect_tags(&node.leading_trivia),
                    });
                }
                Some(TestAnnotation::Property {
                    count_expr,
                    generators_expr,
                }) => {
                    let arity = Self::get_arity(node);
                    tests.push(DiscoveredTest::Property {
                        name: func_name.clone(),
                        count_expr,
                        generators_expr,
                        arity,
                        tags: Self::collect_tags(&node.leading_trivia),
                    });
                }
                _ if func_name.starts_with("test_") => {
                    tests.push(DiscoveredTest::Simple {
                        name: func_name.clone(),
                        tags: Self::collect_tags(&node.leading_trivia),
                    });
                }
                _ => {}
            }
        }

        tests
    }

    fn parse_annotation(comment: &str) -> Option<TestAnnotation> {
        let s = comment.trim();

        if s == "[test]" {
            return Some(TestAnnotation::Test);
        }

        let s = s.strip_prefix('@')?;

        if s == "test" {
            return Some(TestAnnotation::Test);
        }

        // Parse `name(args)`: split at the first '(' only so args may contain '('.
        let paren = s.find('(')?;
        let name = s[..paren].trim();
        let rest = s[paren + 1..].trim();
        let args = rest.strip_suffix(')')?.trim().to_string();

        match name {
            "parametrize" => Some(TestAnnotation::Parametrize { params_expr: args }),
            "property" => {
                let (count_expr, generators_expr) = match Self::split_top_level_comma(&args) {
                    Some((count, generators)) => (count.trim().to_string(), generators.trim().to_string()),
                    None => (DEFAULT_PROPERTY_CASES.to_string(), args),
                };
                Some(TestAnnotation::Property {
                    count_expr,
                    generators_expr,
                })
            }
            "tags" | "tag" => Some(TestAnnotation::Tags(
                args.split(',')
                    .map(|tag| tag.trim().to_string())
                    .filter(|tag| !tag.is_empty())
                    .collect(),
            )),
            _ => None,
        }
    }

    /// Splits `s` at its first comma that isn't nested inside `()`/`[]`/`{}`, used to pull
    /// `@property(count, generators)`'s two arguments apart even though `generators` is
    /// itself an array literal full of commas. Returns `None` if there's no such comma (the
    /// single-argument `@property(generators)` form).
    fn split_top_level_comma(s: &str) -> Option<(&str, &str)> {
        let mut depth = 0i32;
        let mut in_string = false;
        let mut chars = s.char_indices();
        while let Some((i, c)) = chars.next() {
            if in_string {
                match c {
                    '\\' => {
                        chars.next();
                    }
                    '"' => in_string = false,
                    _ => {}
                }
                continue;
            }
            match c {
                '"' => in_string = true,
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => depth -= 1,
                ',' if depth == 0 => return Some((&s[..i], &s[i + 1..])),
                _ => {}
            }
        }
        None
    }

    /// Finds the first `@test`/`[test]`/`@parametrize(...)` annotation among `trivia`,
    /// ignoring `@tags(...)` comments (collected separately via `collect_tags`).
    fn find_test_annotation(trivia: &[CstTrivia]) -> Option<TestAnnotation> {
        Self::merge_annotation_comments(trivia)
            .into_iter()
            .filter_map(|comment| Self::parse_annotation(&comment))
            .find(|annotation| !matches!(annotation, TestAnnotation::Tags(_)))
    }

    /// Collects and flattens every `@tags(...)`/`@tag(...)` comment among `trivia`.
    fn collect_tags(trivia: &[CstTrivia]) -> Vec<String> {
        Self::merge_annotation_comments(trivia)
            .into_iter()
            .filter_map(|comment| Self::parse_annotation(&comment))
            .filter_map(|annotation| match annotation {
                TestAnnotation::Tags(tags) => Some(tags),
                _ => None,
            })
            .flatten()
            .collect()
    }

    /// Joins comment lines whose `name(...)` annotation doesn't close on the same line,
    /// so e.g. `@parametrize(...)` can span multiple comment lines.
    fn merge_annotation_comments(trivia: &[CstTrivia]) -> Vec<String> {
        let lines: Vec<&str> = trivia.iter().filter_map(|t| t.comment()).collect();
        let mut merged = Vec::new();
        let mut i = 0;

        while i < lines.len() {
            let trimmed = lines[i].trim();
            let unclosed = trimmed.starts_with('@') && trimmed.contains('(') && !trimmed.ends_with(')');

            if unclosed {
                let mut end = i + 1;
                while end < lines.len() && !lines[end].trim().ends_with(')') {
                    end += 1;
                }
                if end < lines.len() {
                    end += 1;
                }
                merged.push(lines[i..end].join("\n"));
                i = end;
            } else {
                merged.push(lines[i].to_string());
                i += 1;
            }
        }

        merged
    }

    /// Builds the array-of-`test_case`s expression for a `# @property(count, generators)` test.
    /// Each iteration generates args via `gen::tuple(generators)` seeded with its index, then
    /// runs `_property_case_result` (see `test.mq`), which shrinks a failing case before
    /// reporting it. `generators_expr`'s length is checked against `arity` so a mismatch fails
    /// loudly instead of silently passing `None` for missing parameters.
    fn build_property_case_expr(
        display: &str,
        name: &str,
        count_expr: &str,
        generators_expr: &str,
        arity: usize,
    ) -> String {
        let arg_list = (0..arity)
            .map(|i| format!("__property_arg[{i}]"))
            .collect::<Vec<_>>()
            .join(", ");
        // `range` is inclusive and auto-descends, so `range(0, count - 1)` for count <= 0
        // would still yield seeds instead of an empty case list; guard it explicitly.
        format!(
            "do \
               let __property_generators = ({generators_expr}) \
               | let __property_count = ({count_expr}) \
               | if (len(__property_generators) != {arity}): \
                   error(\"@property: expected {arity} generator(s) for '{name}' ({arity} parameter(s)), got \" + to_string(len(__property_generators))) \
                 elif (__property_count <= 0): [] \
                 else: map(range(0, __property_count - 1), fn(__property_seed): \
                   do \
                     let __property_args = (gen::tuple(__property_generators))(__property_seed) \
                     | test_case( \
                         \"{display}[\" + to_string(__property_seed) + \"]\", \
                         fn(): _property_case_result(__property_args, fn(__property_arg): {name}({arg_list});) ; \
                       ) \
                   end \
                 ;) \
             end"
        )
    }

    /// Returns the number of positional parameters of a `def` node.
    fn get_arity(node: &mq_lang::Shared<mq_lang::CstNode>) -> usize {
        let (sig, _) = node.split_cond_and_program();
        // sig[0] is the function name; the rest are parameter idents.
        sig.len().saturating_sub(1)
    }

    /// Builds the `run_tests(flatten([...]))` call appended to the file content.
    fn build_test_query(content: &str, tests: &[DiscoveredTest]) -> String {
        Self::build_test_query_with_fn(content, tests, "run_tests")
    }

    /// Like [`Self::build_test_query`], but calls `report_fn` (e.g. `run_tests_data`) instead
    /// of `run_tests`, so the caller can get back structured results instead of a Markdown
    /// report printed to stdout.
    fn build_test_query_with_fn(content: &str, tests: &[DiscoveredTest], report_fn: &str) -> String {
        let cases = tests
            .iter()
            .map(|test| match test {
                DiscoveredTest::Simple { name, .. } => {
                    let display = Self::display_name(name);
                    format!("  [test_case(\"{display}\", {name})]")
                }
                DiscoveredTest::Parametrized {
                    name,
                    params_expr,
                    arity,
                    ..
                } => {
                    let display = Self::display_name(name);
                    let arg_list = (0..*arity)
                        .map(|i| format!("__ic[1][{i}]"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!(
                        "  map(\
                            zip(range(0, len({params_expr})), {params_expr}), \
                            fn(__ic): test_case(\"{display}[\" + to_string(__ic[0]) + \"]\", \
                            fn(): {name}({arg_list}) ;) ;)"
                    )
                }
                DiscoveredTest::Property {
                    name,
                    count_expr,
                    generators_expr,
                    arity,
                    ..
                } => {
                    let display = Self::display_name(name);
                    format!(
                        "  {}",
                        Self::build_property_case_expr(display, name, count_expr, generators_expr, *arity)
                    )
                }
            })
            .collect::<Vec<_>>()
            .join(",\n");

        format!("{content}\n| {report_fn}(flatten([\n{cases}\n]))")
    }
}

fn build_open_command(path: &Path, target_os: &str) -> std::process::Command {
    let mut cmd = if target_os == "macos" {
        std::process::Command::new("open")
    } else if target_os == "windows" {
        let mut cmd = std::process::Command::new("cmd");
        cmd.args(["/C", "start", ""]);
        cmd
    } else {
        std::process::Command::new("xdg-open")
    };
    cmd.arg(path);
    cmd
}

/// Launches `path` in the OS default application.
fn open_in_default_app(path: &Path) -> miette::Result<()> {
    build_open_command(path, std::env::consts::OS)
        .status()
        .into_diagnostic()?;
    Ok(())
}

#[cfg(test)]
mod tests;
