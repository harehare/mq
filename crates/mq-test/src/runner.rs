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

/// Parsed test annotation from a leading comment.
#[derive(Debug, PartialEq)]
enum TestAnnotation {
    Test,
    Parametrize { params_expr: String },
    Tags(Vec<String>),
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
}

impl DiscoveredTest {
    fn name(&self) -> &str {
        match self {
            DiscoveredTest::Simple { name, .. } => name,
            DiscoveredTest::Parametrized { name, .. } => name,
        }
    }

    fn tags(&self) -> &[String] {
        match self {
            DiscoveredTest::Simple { tags, .. } => tags,
            DiscoveredTest::Parametrized { tags, .. } => tags,
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
        }
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
        // Merged across all test files, so a shared module gets combined coverage.
        let coverage_data = CoverageData::default();
        // Resolved module-name -> file path, filled in as each engine resolves imports.
        // Behind a `Mutex` so files can be run in parallel.
        let module_paths: Mutex<FxHashMap<String, PathBuf>> = Mutex::new(FxHashMap::default());
        let any_failed = AtomicBool::new(false);
        // Files that failed to read/parse/evaluate, reported as a rollup at the end.
        let file_errors: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

        let run_file = |file: &PathBuf| {
            let content = match fs::read_to_string(file) {
                Ok(content) => content,
                Err(e) => {
                    any_failed.store(true, Ordering::Relaxed);
                    file_errors.lock().unwrap().push(file.clone());
                    eprintln!("# {}\n\n❌ Failed to read file: {e}\n\n---\n", file.display());
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

            let query = Self::build_test_query(&content, &tests);
            let mut engine = mq_lang::Engine::with_io(
                mq_lang::DefaultModuleResolver::default(),
                mq_lang::Shared::new(mq_lang::MemIo::default()),
            );
            engine.load_builtin_module();
            engine.define_string_value("TEST_FILE", file.to_string_lossy().as_ref());

            // Resolve relative `include` statements in the test file.
            if let Some(parent) = file.parent()
                && parent != Path::new("")
            {
                engine.set_search_paths(vec![parent.to_path_buf()]);
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

            if self.coverage {
                engine.set_debugger_handler(Box::new(CoverageHandler(coverage_data.clone())));
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
                    let passed = matches!(result.values().first(), Some(RuntimeValue::Boolean(true)));
                    if !passed {
                        any_failed.store(true, Ordering::Relaxed);
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
                    file_errors.lock().unwrap().push(file.clone());
                    eprintln!("{}", Self::render_file_error(file, *e));
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

        let file_errors = file_errors.into_inner().unwrap();
        if !file_errors.is_empty() {
            let list = file_errors
                .iter()
                .map(|f| format!("  - {}", f.display()))
                .collect::<Vec<_>>()
                .join("\n");
            eprintln!(
                "⚠ {} of {} file(s) failed to run and were skipped:\n{list}",
                file_errors.len(),
                test_files.len()
            );
        }

        Ok(!any_failed.load(Ordering::Relaxed))
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
            if node.kind == CstNodeKind::Module {
                tests.extend(Self::discover_tests_in(&node.children));
                continue;
            }

            if node.kind != CstNodeKind::Def {
                continue;
            }

            let func_name = match node.children.first() {
                Some(child) => child.to_string(),
                None => continue,
            };

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

        // Parse `name(args)` — split at the first '(' only so args may contain '('.
        let paren = s.find('(')?;
        let name = s[..paren].trim();
        let rest = s[paren + 1..].trim();
        let args = rest.strip_suffix(')')?.trim().to_string();

        match name {
            "parametrize" => Some(TestAnnotation::Parametrize { params_expr: args }),
            "tags" | "tag" => Some(TestAnnotation::Tags(
                args.split(',')
                    .map(|tag| tag.trim().to_string())
                    .filter(|tag| !tag.is_empty())
                    .collect(),
            )),
            _ => None,
        }
    }

    /// Finds the first `@test`/`[test]`/`@parametrize(...)` annotation among `trivia`,
    /// ignoring `@tags(...)` comments (collected separately via `collect_tags`).
    fn find_test_annotation(trivia: &[CstTrivia]) -> Option<TestAnnotation> {
        trivia
            .iter()
            .filter_map(|t| t.comment().and_then(Self::parse_annotation))
            .find(|annotation| !matches!(annotation, TestAnnotation::Tags(_)))
    }

    /// Collects and flattens every `@tags(...)`/`@tag(...)` comment among `trivia`.
    fn collect_tags(trivia: &[CstTrivia]) -> Vec<String> {
        trivia
            .iter()
            .filter_map(|t| t.comment().and_then(Self::parse_annotation))
            .filter_map(|annotation| match annotation {
                TestAnnotation::Tags(tags) => Some(tags),
                _ => None,
            })
            .flatten()
            .collect()
    }

    /// Returns the number of positional parameters of a `def` node.
    fn get_arity(node: &mq_lang::Shared<mq_lang::CstNode>) -> usize {
        let (sig, _) = node.split_cond_and_program();
        // sig[0] is the function name; the rest are parameter idents.
        sig.len().saturating_sub(1)
    }

    /// Builds the `run_tests(flatten([...]))` call appended to the file content.
    fn build_test_query(content: &str, tests: &[DiscoveredTest]) -> String {
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
            })
            .collect::<Vec<_>>()
            .join(",\n");

        format!("{content}\n| run_tests(flatten([\n{cases}\n]))")
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
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case("@test", Some(TestAnnotation::Test))]
    #[case("  @test  ", Some(TestAnnotation::Test))]
    #[case("[test]", Some(TestAnnotation::Test))]
    #[case("  [test]  ", Some(TestAnnotation::Test))]
    #[case(
        "@parametrize([[1, 2], [3, 4]])",
        Some(TestAnnotation::Parametrize { params_expr: "[[1, 2], [3, 4]]".to_string() })
    )]
    #[case(
        "  @parametrize(  [[1, 2]]  )  ",
        Some(TestAnnotation::Parametrize { params_expr: "[[1, 2]]".to_string() })
    )]
    #[case(
        "@parametrize(range(0, 5))",
        Some(TestAnnotation::Parametrize { params_expr: "range(0, 5)".to_string() })
    )]
    #[case(
        "@parametrize([])",
        Some(TestAnnotation::Parametrize { params_expr: "[]".to_string() })
    )]
    #[case(
        "@tags(slow, integration)",
        Some(TestAnnotation::Tags(vec!["slow".to_string(), "integration".to_string()]))
    )]
    #[case(
        "  @tags(  slow ,  integration  )  ",
        Some(TestAnnotation::Tags(vec!["slow".to_string(), "integration".to_string()]))
    )]
    #[case("@tag(slow)", Some(TestAnnotation::Tags(vec!["slow".to_string()])))]
    #[case("@tags()", Some(TestAnnotation::Tags(vec![])))]
    #[case("@unknown(foo)", None)]
    #[case("@skip", None)]
    #[case("not an annotation", None)]
    #[case("@", None)]
    #[case("@parametrize", None)]
    fn test_parse_annotation(#[case] input: &str, #[case] expected: Option<TestAnnotation>) {
        assert_eq!(TestRunner::parse_annotation(input), expected);
    }

    fn first_def(content: &str) -> mq_lang::Shared<mq_lang::CstNode> {
        let (nodes, _) = mq_lang::parse_recovery(content);
        nodes
            .into_iter()
            .find(|n| n.kind == mq_lang::CstNodeKind::Def)
            .expect("no def node found")
    }

    #[rstest]
    #[case("def foo():\n  None\nend\n", 0)]
    #[case("def foo(x):\n  None\nend\n", 1)]
    #[case("def foo(x, y):\n  None\nend\n", 2)]
    #[case("def foo(x, y, z):\n  None\nend\n", 3)]
    #[case("def foo(a, b, c, d):\n  None\nend\n", 4)]
    fn test_get_arity(#[case] content: &str, #[case] expected: usize) {
        let node = first_def(content);
        assert_eq!(TestRunner::get_arity(&node), expected);
    }

    #[rstest]
    #[case(
        "def test_foo():\n  None\nend\n\ndef helper():\n  None\nend\n\ndef test_bar():\n  None\nend\n",
        vec![
            DiscoveredTest::Simple { name: "test_foo".to_string(), tags: vec![] },
            DiscoveredTest::Simple { name: "test_bar".to_string(), tags: vec![] },
        ]
    )]
    #[case(
        "# @test\ndef my_check():\n  None\nend\n\ndef not_a_test():\n  None\nend\n",
        vec![DiscoveredTest::Simple { name: "my_check".to_string(), tags: vec![] }]
    )]
    #[case(
        "# [test]\ndef another_check():\n  None\nend\n",
        vec![DiscoveredTest::Simple { name: "another_check".to_string(), tags: vec![] }]
    )]
    #[case(
        "def test_first():\n  None\nend\n\n# @test\ndef annotated():\n  None\nend\n",
        vec![
            DiscoveredTest::Simple { name: "test_first".to_string(), tags: vec![] },
            DiscoveredTest::Simple { name: "annotated".to_string(), tags: vec![] },
        ]
    )]
    #[case("def helper():\n  None\nend\n", vec![])]
    #[case(
        "module a:\n  def test_first():\n  None\nend\n\n# @test\ndef annotated():\n  None\nend\nend\n",
        vec![
            DiscoveredTest::Simple { name: "test_first".to_string(), tags: vec![] },
            DiscoveredTest::Simple { name: "annotated".to_string(), tags: vec![] },
        ]
    )]
    #[case(
        "# @tags(slow, integration)\n# @test\ndef my_check():\n  None\nend\n",
        vec![DiscoveredTest::Simple {
            name: "my_check".to_string(),
            tags: vec!["slow".to_string(), "integration".to_string()],
        }]
    )]
    #[case(
        "# @tags(slow)\ndef test_first():\n  None\nend\n",
        vec![DiscoveredTest::Simple { name: "test_first".to_string(), tags: vec!["slow".to_string()] }]
    )]
    fn test_discover_tests_simple(#[case] content: &str, #[case] expected: Vec<DiscoveredTest>) {
        assert_eq!(TestRunner::discover_tests(content), expected);
    }

    #[rstest]
    #[case(
        "# @parametrize([[\"hello\", 5], [\"world\", 5]])\ndef test_len(input, expected):\n  None\nend\n",
        "test_len",
        "[[\"hello\", 5], [\"world\", 5]]",
        2
    )]
    #[case(
        "# @parametrize([[1], [2], [3]])\ndef test_double(x):\n  None\nend\n",
        "test_double",
        "[[1], [2], [3]]",
        1
    )]
    #[case(
        "# @parametrize([[\"a\", \"b\", \"ab\"], [\"x\", \"y\", \"xy\"]])\ndef test_concat(a, b, expected):\n  None\nend\n",
        "test_concat",
        "[[\"a\", \"b\", \"ab\"], [\"x\", \"y\", \"xy\"]]",
        3
    )]
    #[case(
        "# @parametrize([[\"ignored\"]])\ndef test_no_args():\n  None\nend\n",
        "test_no_args",
        "[[\"ignored\"]]",
        0
    )]
    fn test_discover_tests_parametrized(
        #[case] content: &str,
        #[case] expected_name: &str,
        #[case] expected_params_expr: &str,
        #[case] expected_arity: usize,
    ) {
        let tests = TestRunner::discover_tests(content);
        assert_eq!(tests.len(), 1);
        match &tests[0] {
            DiscoveredTest::Parametrized {
                name,
                params_expr,
                arity,
                ..
            } => {
                assert_eq!(name, expected_name);
                assert_eq!(params_expr, expected_params_expr);
                assert_eq!(*arity, expected_arity);
            }
            other => panic!("expected Parametrized, got {other:?}"),
        }
    }

    #[test]
    fn test_discover_tests_multiple_parametrized() {
        let content = concat!(
            "# @parametrize([[1, 2], [3, 4]])\n",
            "def test_add(a, b):\n  None\nend\n\n",
            "# @parametrize([[\"hello\", 5]])\n",
            "def test_len(s, n):\n  None\nend\n",
        );
        let tests = TestRunner::discover_tests(content);
        assert_eq!(tests.len(), 2);
        assert!(matches!(&tests[0], DiscoveredTest::Parametrized { name, .. } if name == "test_add"));
        assert!(matches!(&tests[1], DiscoveredTest::Parametrized { name, .. } if name == "test_len"));
    }

    #[test]
    fn test_discover_tests_mixed_all_kinds() {
        let content = concat!(
            "def test_simple():\n  None\nend\n\n",
            "# @test\ndef annotated():\n  None\nend\n\n",
            "# @parametrize([[1, 2]])\ndef test_param(a, b):\n  None\nend\n",
        );
        let tests = TestRunner::discover_tests(content);
        assert_eq!(tests.len(), 3);
        assert!(matches!(&tests[0], DiscoveredTest::Simple { name, .. } if name == "test_simple"));
        assert!(matches!(&tests[1], DiscoveredTest::Simple { name, .. } if name == "annotated"));
        assert!(matches!(&tests[2], DiscoveredTest::Parametrized { name, .. } if name == "test_param"));
    }

    #[test]
    fn test_discover_tests_parametrized_in_module() {
        let content = concat!(
            "module m:\n",
            "  # @parametrize([[1, 2]])\n",
            "  def test_add(a, b):\n  None\nend\n",
            "end\n",
        );
        let tests = TestRunner::discover_tests(content);
        assert_eq!(tests.len(), 1);
        assert!(
            matches!(&tests[0], DiscoveredTest::Parametrized { name, arity, .. } if name == "test_add" && *arity == 2)
        );
    }

    #[test]
    fn test_discover_tests_ignores_unknown_annotation() {
        let content = "# @skip\ndef my_check():\n  None\nend\n";
        let tests = TestRunner::discover_tests(content);
        assert!(tests.is_empty());
    }

    #[rstest]
    #[case(vec![DiscoveredTest::Simple { name: "test_foo".to_string(), tags: vec![] }], "[test_case(\"foo\", test_foo)]")]
    #[case(vec![DiscoveredTest::Simple { name: "test_is_array".to_string(), tags: vec![] }], "[test_case(\"is_array\", test_is_array)]")]
    #[case(vec![DiscoveredTest::Simple { name: "my_check".to_string(), tags: vec![] }], "[test_case(\"my_check\", my_check)]")]
    fn test_build_test_query_simple_cases(#[case] tests: Vec<DiscoveredTest>, #[case] expected: &str) {
        let query = TestRunner::build_test_query("content", &tests);
        assert!(query.starts_with("content\n"), "query must start with original content");
        assert!(query.contains("flatten(["), "must use flatten");
        assert!(query.contains(expected), "expected {expected:?} in:\n{query}");
    }

    #[rstest]
    #[case(
        DiscoveredTest::Parametrized { name: "test_no_args".to_string(), params_expr: "[[]]".to_string(), arity: 0, tags: vec![] },
        "test_no_args()",
        "\"no_args[\""
    )]
    #[case(
        DiscoveredTest::Parametrized { name: "test_double".to_string(), params_expr: "[[1], [2]]".to_string(), arity: 1, tags: vec![] },
        "test_double(__ic[1][0])",
        "\"double[\""
    )]
    #[case(
        DiscoveredTest::Parametrized { name: "test_len".to_string(), params_expr: "[[\"a\", 1]]".to_string(), arity: 2, tags: vec![] },
        "test_len(__ic[1][0], __ic[1][1])",
        "\"len[\""
    )]
    #[case(
        DiscoveredTest::Parametrized { name: "test_concat".to_string(), params_expr: "[[\"a\", \"b\", \"ab\"]]".to_string(), arity: 3, tags: vec![] },
        "test_concat(__ic[1][0], __ic[1][1], __ic[1][2])",
        "\"concat[\""
    )]
    #[case(
        DiscoveredTest::Parametrized { name: "check_len".to_string(), params_expr: "[[1]]".to_string(), arity: 1, tags: vec![] },
        "check_len(__ic[1][0])",
        "\"check_len[\""
    )]
    fn test_build_test_query_parametrized_cases(
        #[case] test: DiscoveredTest,
        #[case] expected_call: &str,
        #[case] expected_label: &str,
    ) {
        let query = TestRunner::build_test_query("content", &[test]);
        assert!(query.contains("flatten(["), "must use flatten");
        assert!(query.contains("map("), "must use map");
        assert!(query.contains("zip(range("), "must use zip+range");
        assert!(
            query.contains(expected_call),
            "expected call {expected_call:?} in:\n{query}"
        );
        assert!(
            query.contains(expected_label),
            "expected label {expected_label:?} in:\n{query}"
        );
    }

    #[test]
    fn test_build_test_query_multiple_simple() {
        let tests = vec![
            DiscoveredTest::Simple {
                name: "test_foo".to_string(),
                tags: vec![],
            },
            DiscoveredTest::Simple {
                name: "test_bar".to_string(),
                tags: vec![],
            },
        ];
        let query = TestRunner::build_test_query("content", &tests);
        assert!(query.contains("[test_case(\"foo\", test_foo)]"));
        assert!(query.contains("[test_case(\"bar\", test_bar)]"));
        assert!(query.contains("flatten(["));
    }

    #[test]
    fn test_build_test_query_mixed() {
        let tests = vec![
            DiscoveredTest::Simple {
                name: "test_foo".to_string(),
                tags: vec![],
            },
            DiscoveredTest::Parametrized {
                name: "test_len".to_string(),
                params_expr: "[[\"a\", 1]]".to_string(),
                arity: 2,
                tags: vec![],
            },
        ];
        let query = TestRunner::build_test_query("content", &tests);
        assert!(query.contains("[test_case(\"foo\", test_foo)]"));
        assert!(query.contains("map("));
        assert!(query.contains("test_len(__ic[1][0], __ic[1][1])"));
        assert!(query.contains("flatten(["));
    }

    #[test]
    fn test_build_test_query_preserves_content() {
        let content = "include \"test\"\n|\ndef helper(): None end";
        let tests = vec![DiscoveredTest::Simple {
            name: "test_foo".to_string(),
            tags: vec![],
        }];
        let query = TestRunner::build_test_query(content, &tests);
        assert!(query.starts_with(content));
    }

    #[rstest]
    #[case("macos", "open", Vec::<&str>::new())]
    #[case("windows", "cmd", vec!["/C", "start", ""])]
    #[case("linux", "xdg-open", Vec::<&str>::new())]
    #[case("freebsd", "xdg-open", Vec::<&str>::new())]
    fn test_build_open_command(
        #[case] target_os: &str,
        #[case] expected_program: &str,
        #[case] expected_args: Vec<&str>,
    ) {
        let path = PathBuf::from("coverage.html");
        let cmd = build_open_command(&path, target_os);

        assert_eq!(cmd.get_program(), expected_program);

        let args: Vec<_> = cmd.get_args().map(|a| a.to_string_lossy().to_string()).collect();
        let mut expected: Vec<String> = expected_args.into_iter().map(String::from).collect();
        expected.push("coverage.html".to_string());
        assert_eq!(args, expected);
    }

    #[rstest]
    #[case(None, vec![], "test_foo", vec![], true)]
    #[case(Some("foo"), vec![], "test_foo", vec![], true)]
    #[case(Some("FOO"), vec![], "test_foo", vec![], true)]
    #[case(Some("bar"), vec![], "test_foo", vec![], false)]
    #[case(None, vec!["slow".to_string()], "test_foo", vec!["slow".to_string()], true)]
    #[case(None, vec!["slow".to_string()], "test_foo", vec!["fast".to_string()], false)]
    #[case(None, vec!["slow".to_string()], "test_foo", vec![], false)]
    #[case(Some("foo"), vec!["slow".to_string()], "test_foo", vec!["slow".to_string()], true)]
    #[case(Some("bar"), vec!["slow".to_string()], "test_foo", vec!["slow".to_string()], false)]
    fn test_matches(
        #[case] filter: Option<&str>,
        #[case] tags: Vec<String>,
        #[case] test_name: &str,
        #[case] test_tags: Vec<String>,
        #[case] expected: bool,
    ) {
        let runner = TestRunner::new(vec![])
            .with_filter(filter.map(str::to_string))
            .with_tags(tags);
        let test = DiscoveredTest::Simple {
            name: test_name.to_string(),
            tags: test_tags,
        };
        assert_eq!(runner.matches(&test), expected);
    }

    fn temp_project_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mq_test_{name}_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Writes a minimal test file exercising `assert_snapshot("greeting", "hello world")`,
    /// shared by the `assert_snapshot` integration tests below.
    fn write_greeting_snapshot_test_file(dir: &Path) -> PathBuf {
        let test_file = dir.join("tests.mq");
        fs::write(
            &test_file,
            "include \"test\"\n|\ndef test_greeting():\n  assert_snapshot(\"greeting\", \"hello world\")\nend\n",
        )
        .unwrap();
        test_file
    }

    #[test]
    fn test_coverage_reports_only_the_imported_module() {
        let dir = temp_project_dir("coverage_only_imported");
        fs::write(
            dir.join("lib.mq"),
            "def add(a, b):\n  a + b\nend\n\ndef unused(a):\n  a * 100\nend\n",
        )
        .unwrap();
        let test_file = dir.join("tests.mq");
        fs::write(
            &test_file,
            "include \"test\" | include \"lib\"\n|\n\ndef test_add():\n  assert_eq(add(1, 2), 3)\nend\n",
        )
        .unwrap();
        let output = dir.join("coverage.json");

        let passed = TestRunner::new(vec![test_file])
            .with_coverage(true)
            .with_coverage_format(CoverageFormat::Json)
            .with_coverage_output(Some(output.clone()))
            .run()
            .unwrap();
        assert!(passed);

        let report = fs::read_to_string(&output).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&report).unwrap();
        let files = parsed["files"].as_array().unwrap();

        // Only "lib" is reported — the test file itself and the "test"/"builtin"
        // modules it also pulls in are not the code under test.
        assert_eq!(files.len(), 1, "expected only lib.mq in report: {files:?}");
        assert!(files[0]["file"].as_str().unwrap().ends_with("lib.mq"));
        assert_eq!(files[0]["totalLines"], 2);
        assert_eq!(files[0]["coveredLines"], 1);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_read_file_write_file_use_the_in_memory_mock_not_the_real_disk() {
        let dir = temp_project_dir("mock_io");
        let test_file = dir.join("tests.mq");
        // "/definitely/not/a/real/writable/path.txt" would fail against a real, sandboxed
        // filesystem `Io` (no such directory, no write permission) — succeeding here proves
        // `write_file`/`read_file` went through the engine's in-memory mock instead.
        fs::write(
            &test_file,
            concat!(
                "include \"test\"\n",
                "|\n",
                "def test_write_then_read_file():\n",
                "  write_file(\"/definitely/not/a/real/writable/path.txt\", \"hello-mock\")\n",
                "  | assert_eq(read_file(\"/definitely/not/a/real/writable/path.txt\"), \"hello-mock\")\n",
                "end\n",
            ),
        )
        .unwrap();

        assert!(TestRunner::new(vec![test_file]).run().unwrap());

        assert!(
            !Path::new("/definitely/not/a/real/writable/path.txt").exists(),
            "write_file must not touch the real filesystem"
        );

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_mock_fetch_lets_a_test_seed_its_own_http_response() {
        let dir = temp_project_dir("mock_fetch");
        let test_file = dir.join("tests.mq");
        // No real network call is made — `mock_fetch` seeds the in-memory mock's response
        // for the URL, and `http()` reads it back, entirely from within the test function.
        fs::write(
            &test_file,
            concat!(
                "include \"test\"\n",
                "|\n",
                "def test_reads_mocked_api_response():\n",
                "  mock_fetch(\"https://api.example.com/data\", \"{\\\"ok\\\": true}\")\n",
                "  | assert_eq(http(\"get\", \"https://api.example.com/data\"), \"{\\\"ok\\\": true}\")\n",
                "end\n",
            ),
        )
        .unwrap();

        assert!(TestRunner::new(vec![test_file]).run().unwrap());

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_coverage_merges_across_test_files_sharing_a_module() {
        let dir = temp_project_dir("coverage_merge");
        fs::write(
            dir.join("lib.mq"),
            "def add(a, b):\n  a + b\nend\n\ndef sub(a, b):\n  a - b\nend\n",
        )
        .unwrap();
        let test_add = dir.join("test_add.mq");
        fs::write(
            &test_add,
            "include \"test\" | include \"lib\"\n|\n\ndef test_add():\n  assert_eq(add(1, 2), 3)\nend\n",
        )
        .unwrap();
        let test_sub = dir.join("test_sub.mq");
        fs::write(
            &test_sub,
            "include \"test\" | include \"lib\"\n|\n\ndef test_sub():\n  assert_eq(sub(5, 2), 3)\nend\n",
        )
        .unwrap();
        let output = dir.join("coverage.json");

        let passed = TestRunner::new(vec![test_add, test_sub])
            .with_coverage(true)
            .with_coverage_format(CoverageFormat::Json)
            .with_coverage_output(Some(output.clone()))
            .run()
            .unwrap();
        assert!(passed);

        let report = fs::read_to_string(&output).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&report).unwrap();
        let files = parsed["files"].as_array().unwrap();

        assert_eq!(files.len(), 1, "lib.mq must be reported once, merged: {files:?}");
        assert_eq!(files[0]["totalLines"], 2);
        assert_eq!(files[0]["coveredLines"], 2);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_run_reports_failure_without_aborting_other_files() {
        let dir = temp_project_dir("failure_no_abort");
        let failing = dir.join("failing.mq");
        fs::write(
            &failing,
            "include \"test\"\n|\ndef test_fails():\n  assert_eq(1, 2)\nend\n",
        )
        .unwrap();
        let passing = dir.join("passing.mq");
        fs::write(
            &passing,
            "include \"test\"\n|\ndef test_passes():\n  assert_eq(1, 1)\nend\n",
        )
        .unwrap();

        let passed = TestRunner::new(vec![failing, passing]).run().unwrap();
        assert!(!passed, "run() must report failure when any test fails");

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_run_continues_after_a_file_with_a_syntax_error() {
        let dir = temp_project_dir("continues_after_syntax_error");
        fs::write(dir.join("lib.mq"), "def add(a, b):\n  a + b\nend\n").unwrap();

        let broken = dir.join("a_broken.mq");
        fs::write(
            &broken,
            "include \"test\"\n|\ndef test_broken():\n  assert_eq(1, 1)\nend\nend\n",
        )
        .unwrap();

        let ok = dir.join("b_ok.mq");
        fs::write(
            &ok,
            "include \"test\" | include \"lib\"\n|\n\ndef test_ok():\n  assert_eq(add(1, 2), 3)\nend\n",
        )
        .unwrap();
        let output = dir.join("coverage.json");

        let passed = TestRunner::new(vec![broken, ok])
            .with_coverage(true)
            .with_coverage_format(CoverageFormat::Json)
            .with_coverage_output(Some(output.clone()))
            .run()
            .unwrap();
        assert!(!passed, "a broken file must fail the overall run");

        // Proves b_ok.mq still ran despite a_broken.mq's syntax error.
        let report = fs::read_to_string(&output).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&report).unwrap();
        let files = parsed["files"].as_array().unwrap();
        assert_eq!(
            files.len(),
            1,
            "b_ok.mq must still have run and exercised lib.mq: {files:?}"
        );
        assert!(files[0]["file"].as_str().unwrap().ends_with("lib.mq"));

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_render_file_error_attaches_the_file_name_when_mq_lang_left_it_blank() {
        let dir = temp_project_dir("render_file_error");
        let broken = dir.join("broken.mq");
        let content = "include \"test\"\n|\ndef test_x():\n  1\nend\nend\n";
        fs::write(&broken, content).unwrap();

        let tests = TestRunner::discover_tests(content);
        let query = TestRunner::build_test_query(content, &tests);

        let mut engine = mq_lang::Engine::with_io(
            mq_lang::DefaultModuleResolver::default(),
            mq_lang::Shared::new(mq_lang::MemIo::default()),
        );
        engine.load_builtin_module();
        let err = *engine.eval(&query, mq_lang::null_input().into_iter()).unwrap_err();
        assert_eq!(
            err.source_code.name(),
            "",
            "sanity check: mq-lang itself doesn't know the file name"
        );

        let rendered = TestRunner::render_file_error(&broken, err);
        assert!(
            rendered.contains(&broken.display().to_string()),
            "rendered error must name the failing file:\n{rendered}"
        );

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_filter_skips_non_matching_tests() {
        let dir = temp_project_dir("filter_skips");
        let test_file = dir.join("tests.mq");
        fs::write(
            &test_file,
            concat!(
                "include \"test\"\n",
                "|\n",
                "def test_add():\n  assert_eq(1 + 1, 2)\nend\n\n",
                "def test_sub():\n  assert_eq(2, 3)\nend\n",
            ),
        )
        .unwrap();

        // Only "add" is selected; the failing "sub" test is filtered out.
        let passed = TestRunner::new(vec![test_file])
            .with_filter(Some("add".to_string()))
            .run()
            .unwrap();
        assert!(passed);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_tag_filter_skips_untagged_tests() {
        let dir = temp_project_dir("tag_filter_skips");
        let test_file = dir.join("tests.mq");
        fs::write(
            &test_file,
            concat!(
                "include \"test\"\n",
                "|\n",
                "# @tags(smoke)\n",
                "def test_add():\n  assert_eq(1 + 1, 2)\nend\n\n",
                "def test_sub():\n  assert_eq(2, 3)\nend\n",
            ),
        )
        .unwrap();

        // Only the "smoke"-tagged test runs; the failing untagged test is filtered out.
        let passed = TestRunner::new(vec![test_file])
            .with_tags(vec!["smoke".to_string()])
            .run()
            .unwrap();
        assert!(passed);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_parallel_execution_runs_all_files() {
        let dir = temp_project_dir("parallel_execution");
        let files: Vec<PathBuf> = (0..5)
            .map(|i| {
                let path = dir.join(format!("test_{i}.mq"));
                fs::write(
                    &path,
                    format!("include \"test\"\n|\ndef test_case_{i}():\n  assert_eq({i}, {i})\nend\n"),
                )
                .unwrap();
                path
            })
            .collect();

        let passed = TestRunner::new(files).with_parallel_threshold(0).run().unwrap();
        assert!(passed);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_assert_snapshot_creates_then_matches_across_runs() {
        let dir = temp_project_dir("snapshot_create_then_match");
        let test_file = write_greeting_snapshot_test_file(&dir);

        // First run with --update-snapshots creates the golden file and passes.
        assert!(
            TestRunner::new(vec![test_file.clone()])
                .with_update_snapshots(true)
                .run()
                .unwrap()
        );
        assert!(dir.join("__snapshots__/tests/greeting.snap").exists());

        // A normal run now compares against it and still passes.
        assert!(TestRunner::new(vec![test_file]).run().unwrap());

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_assert_snapshot_fails_on_mismatch_and_writes_store() {
        let dir = temp_project_dir("snapshot_mismatch");
        let test_file = write_greeting_snapshot_test_file(&dir);
        fs::create_dir_all(dir.join("__snapshots__/tests")).unwrap();
        fs::write(dir.join("__snapshots__/tests/greeting.snap"), "goodbye world").unwrap();

        let passed = TestRunner::new(vec![test_file]).run().unwrap();
        assert!(!passed, "a snapshot mismatch must fail the run");
        assert!(dir.join(".mq-test-store/tests/greeting.diff.html").exists());

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_assert_snapshot_missing_golden_fails_without_update_flag() {
        let dir = temp_project_dir("snapshot_missing");
        let test_file = write_greeting_snapshot_test_file(&dir);

        let passed = TestRunner::new(vec![test_file]).run().unwrap();
        assert!(!passed, "a missing golden snapshot must fail rather than silently pass");
        assert!(!dir.join("__snapshots__/tests/greeting.snap").exists());

        fs::remove_dir_all(&dir).ok();
    }
}
