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
    #[case("[test]", Some(TestAnnotation::Test))]
    #[case(
        "@parametrize([[1, 2], [3, 4]])",
        Some(TestAnnotation::Parametrize { params_expr: "[[1, 2], [3, 4]]".to_string() })
    )]
    #[case("@unknown(foo)", None)]
    #[case("not an annotation", None)]
    fn test_parse_annotation(#[case] input: &str, #[case] expected: Option<TestAnnotation>) {
        assert_eq!(TestRunner::parse_annotation(input), expected);
    }

    #[rstest]
    #[case(
        "def test_foo():\n  None\nend\n\ndef helper():\n  None\nend\n\ndef test_bar():\n  None\nend\n",
        vec![DiscoveredTest::Simple("test_foo".to_string()), DiscoveredTest::Simple("test_bar".to_string())]
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
        vec![DiscoveredTest::Simple("test_first".to_string()), DiscoveredTest::Simple("annotated".to_string())]
    )]
    #[case("def helper():\n  None\nend\n", vec![])]
    #[case(
        "module a:\n  def test_first():\n  None\nend\n\n# @test\ndef annotated():\n  None\nend\nend\n",
        vec![DiscoveredTest::Simple("test_first".to_string()), DiscoveredTest::Simple("annotated".to_string())]
    )]
    fn test_discover_tests_simple(#[case] content: &str, #[case] expected: Vec<DiscoveredTest>) {
        assert_eq!(TestRunner::discover_tests(content), expected);
    }

    #[test]
    fn test_discover_tests_parametrized() {
        let content = "# @parametrize([[\"hello\", 5], [\"world\", 5]])\ndef test_len(input, expected):\n  None\nend\n";
        let tests = TestRunner::discover_tests(content);
        assert_eq!(tests.len(), 1);
        match &tests[0] {
            DiscoveredTest::Parametrized {
                name,
                params_expr,
                arity,
            } => {
                assert_eq!(name, "test_len");
                assert_eq!(params_expr, "[[\"hello\", 5], [\"world\", 5]]");
                assert_eq!(*arity, 2);
            }
            other => panic!("expected Parametrized, got {other:?}"),
        }
    }

    #[rstest]
    #[case(
        "include \"test\"\n|",
        vec![DiscoveredTest::Simple("test_foo".to_string()), DiscoveredTest::Simple("test_bar".to_string())],
        vec![("[test_case(\"foo\", test_foo)]", true), ("[test_case(\"bar\", test_bar)]", true)]
    )]
    #[case(
        "include \"test\"\n|",
        vec![DiscoveredTest::Simple("my_check".to_string())],
        vec![("[test_case(\"my_check\", my_check)]", true)]
    )]
    fn test_build_test_query_simple(
        #[case] content: &str,
        #[case] tests: Vec<DiscoveredTest>,
        #[case] expected_snippets: Vec<(&str, bool)>,
    ) {
        let query = TestRunner::build_test_query(content, &tests);
        assert!(query.contains("flatten(["));
        for (snippet, should_contain) in expected_snippets {
            assert_eq!(
                query.contains(snippet),
                should_contain,
                "snippet {snippet:?} in query:\n{query}"
            );
        }
    }

    #[test]
    fn test_build_test_query_parametrized() {
        let tests = vec![DiscoveredTest::Parametrized {
            name: "test_len".to_string(),
            params_expr: "[[\"hello\", 5], [\"world\", 5]]".to_string(),
            arity: 2,
        }];
        let query = TestRunner::build_test_query("include \"test\"\n|", &tests);
        assert!(query.contains("flatten(["));
        assert!(query.contains("map("));
        assert!(
            query.contains("zip(range(0, len([[\"hello\", 5], [\"world\", 5]])), [[\"hello\", 5], [\"world\", 5]])")
        );
        assert!(query.contains("test_len(__ic[1][0], __ic[1][1])"));
        assert!(query.contains("\"len[\""));
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
        let query = TestRunner::build_test_query("include \"test\"\n|", &tests);
        assert!(query.contains("[test_case(\"foo\", test_foo)]"));
        assert!(query.contains("map("));
        assert!(query.contains("flatten(["));
    }
}
