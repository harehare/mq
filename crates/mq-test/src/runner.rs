use glob::glob;
use miette::IntoDiagnostic;
use mq_lang::{CstNodeKind, CstTrivia};
use std::fs;
use std::path::{Path, PathBuf};

/// Parsed test annotation from a leading comment.
#[derive(Debug, PartialEq)]
enum TestAnnotation {
    Test,
    Parametrize { params_expr: String },
}

/// A test function discovered in a `.mq` file.
#[derive(Debug, PartialEq)]
enum DiscoveredTest {
    Simple(String),
    Parametrized {
        name: String,
        params_expr: String,
        arity: usize,
    },
}

/// Discovers and runs mq test functions from `.mq` files.
///
/// A function is treated as a test if its name starts with `test_`, it is
/// preceded by `# @test` / `# [test]`, or preceded by `# @parametrize(expr)`.
/// The runner auto-generates the `run_tests(...)` call from discovered tests.
pub struct TestRunner {
    files: Vec<PathBuf>,
}

impl TestRunner {
    /// Creates a `TestRunner` for the given files.
    /// If `files` is empty, globs `**/*.mq` in the current directory.
    pub fn new(files: Vec<PathBuf>) -> Self {
        Self { files }
    }

    /// Discovers and executes all test functions.
    pub fn run(self) -> miette::Result<()> {
        let test_files: Vec<PathBuf> = if self.files.is_empty() {
            glob("./**/*.mq")
                .into_diagnostic()?
                .collect::<Result<Vec<_>, _>>()
                .into_diagnostic()?
        } else {
            self.files
        };

        for file in &test_files {
            let content = fs::read_to_string(file).into_diagnostic()?;
            let tests = Self::discover_tests(&content);
            if tests.is_empty() {
                continue;
            }

            let query = Self::build_test_query(&content, &tests);
            let mut engine = mq_lang::DefaultEngine::default();
            engine.load_builtin_module();
            engine.define_string_value("TEST_FILE", file.to_string_lossy().as_ref());

            // Resolve relative `include` statements in the test file.
            if let Some(parent) = file.parent()
                && parent != Path::new("")
            {
                engine.set_search_paths(vec![parent.to_path_buf()]);
            }

            let input = mq_lang::null_input();
            engine.eval(&query, input.into_iter()).map_err(|e| *e)?;
        }

        Ok(())
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
                    tests.push(DiscoveredTest::Simple(func_name));
                }
                Some(TestAnnotation::Parametrize { params_expr }) => {
                    let arity = Self::get_arity(node);
                    tests.push(DiscoveredTest::Parametrized {
                        name: func_name,
                        params_expr,
                        arity,
                    });
                }
                None if func_name.starts_with("test_") => {
                    tests.push(DiscoveredTest::Simple(func_name));
                }
                None => {}
            }
        }

        tests
    }

    /// Parses a comment into a `TestAnnotation`.
    ///
    /// Supported forms: `@test`, `[test]`, `@parametrize(expr)`.
    /// Unknown `@name(...)` annotations are silently ignored.
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
            _ => None,
        }
    }

    fn find_test_annotation(trivia: &[CstTrivia]) -> Option<TestAnnotation> {
        trivia.iter().find_map(|t| t.comment().and_then(Self::parse_annotation))
    }

    /// Returns the number of positional parameters of a `def` node.
    fn get_arity(node: &mq_lang::Shared<mq_lang::CstNode>) -> usize {
        let (sig, _) = node.split_cond_and_program();
        // sig[0] is the function name; the rest are parameter idents.
        sig.len().saturating_sub(1)
    }

    /// Builds the `run_tests(flatten([...]))` call appended to the file content.
    ///
    /// Simple tests are `[test_case(...)]`; parametrized tests expand via
    /// `map(zip(range(...), params), fn(...))`. `flatten` merges both into one list.
    fn build_test_query(content: &str, tests: &[DiscoveredTest]) -> String {
        let cases = tests
            .iter()
            .map(|test| match test {
                DiscoveredTest::Simple(name) => {
                    let display = name.strip_prefix("test_").unwrap_or(name);
                    format!("  [test_case(\"{display}\", {name})]")
                }
                DiscoveredTest::Parametrized {
                    name,
                    params_expr,
                    arity,
                } => {
                    let display = name.strip_prefix("test_").unwrap_or(name);
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
            DiscoveredTest::Simple("test_foo".to_string()),
            DiscoveredTest::Simple("test_bar".to_string()),
        ]
    )]
    #[case(
        "# @test\ndef my_check():\n  None\nend\n\ndef not_a_test():\n  None\nend\n",
        vec![DiscoveredTest::Simple("my_check".to_string())]
    )]
    #[case(
        "# [test]\ndef another_check():\n  None\nend\n",
        vec![DiscoveredTest::Simple("another_check".to_string())]
    )]
    #[case(
        "def test_first():\n  None\nend\n\n# @test\ndef annotated():\n  None\nend\n",
        vec![
            DiscoveredTest::Simple("test_first".to_string()),
            DiscoveredTest::Simple("annotated".to_string()),
        ]
    )]
    #[case("def helper():\n  None\nend\n", vec![])]
    #[case(
        "module a:\n  def test_first():\n  None\nend\n\n# @test\ndef annotated():\n  None\nend\nend\n",
        vec![
            DiscoveredTest::Simple("test_first".to_string()),
            DiscoveredTest::Simple("annotated".to_string()),
        ]
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
        assert!(matches!(&tests[0], DiscoveredTest::Simple(n) if n == "test_simple"));
        assert!(matches!(&tests[1], DiscoveredTest::Simple(n) if n == "annotated"));
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
    #[case(vec![DiscoveredTest::Simple("test_foo".to_string())], "[test_case(\"foo\", test_foo)]")]
    #[case(vec![DiscoveredTest::Simple("test_is_array".to_string())], "[test_case(\"is_array\", test_is_array)]")]
    #[case(vec![DiscoveredTest::Simple("my_check".to_string())], "[test_case(\"my_check\", my_check)]")]
    fn test_build_test_query_simple_cases(#[case] tests: Vec<DiscoveredTest>, #[case] expected: &str) {
        let query = TestRunner::build_test_query("content", &tests);
        assert!(query.starts_with("content\n"), "query must start with original content");
        assert!(query.contains("flatten(["), "must use flatten");
        assert!(query.contains(expected), "expected {expected:?} in:\n{query}");
    }

    #[rstest]
    #[case(
        DiscoveredTest::Parametrized { name: "test_no_args".to_string(), params_expr: "[[]]".to_string(), arity: 0 },
        "test_no_args()",
        "\"no_args[\""
    )]
    #[case(
        DiscoveredTest::Parametrized { name: "test_double".to_string(), params_expr: "[[1], [2]]".to_string(), arity: 1 },
        "test_double(__ic[1][0])",
        "\"double[\""
    )]
    #[case(
        DiscoveredTest::Parametrized { name: "test_len".to_string(), params_expr: "[[\"a\", 1]]".to_string(), arity: 2 },
        "test_len(__ic[1][0], __ic[1][1])",
        "\"len[\""
    )]
    #[case(
        DiscoveredTest::Parametrized { name: "test_concat".to_string(), params_expr: "[[\"a\", \"b\", \"ab\"]]".to_string(), arity: 3 },
        "test_concat(__ic[1][0], __ic[1][1], __ic[1][2])",
        "\"concat[\""
    )]
    #[case(
        DiscoveredTest::Parametrized { name: "check_len".to_string(), params_expr: "[[1]]".to_string(), arity: 1 },
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
            DiscoveredTest::Simple("test_foo".to_string()),
            DiscoveredTest::Simple("test_bar".to_string()),
        ];
        let query = TestRunner::build_test_query("content", &tests);
        assert!(query.contains("[test_case(\"foo\", test_foo)]"));
        assert!(query.contains("[test_case(\"bar\", test_bar)]"));
        assert!(query.contains("flatten(["));
    }

    #[test]
    fn test_build_test_query_mixed() {
        let tests = vec![
            DiscoveredTest::Simple("test_foo".to_string()),
            DiscoveredTest::Parametrized {
                name: "test_len".to_string(),
                params_expr: "[[\"a\", 1]]".to_string(),
                arity: 2,
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
        let tests = vec![DiscoveredTest::Simple("test_foo".to_string())];
        let query = TestRunner::build_test_query(content, &tests);
        assert!(query.starts_with(content));
    }
}
