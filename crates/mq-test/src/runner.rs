use glob::glob;
use miette::IntoDiagnostic;
use mq_lang::{CstNodeKind, CstTrivia};
use std::fs;
use std::path::{Path, PathBuf};

/// Discovers and runs mq test functions from `.mq` files.
///
/// A function is treated as a test if:
/// - Its name starts with `test_`, OR
/// - It is immediately preceded by a `# @test` annotation comment.
///
/// Test discovery uses the CST so that both conditions are resolved accurately
/// without any line-scanning heuristics.
///
/// The runner auto-generates the `run_tests([...])` call so test files do not
/// need to maintain a manual list of test cases.
pub struct TestRunner {
    files: Vec<PathBuf>,
}

impl TestRunner {
    /// Create a new `TestRunner` for the given files.
    ///
    /// If `files` is empty the runner will glob `**/*.mq` in the current
    /// working directory.
    pub fn new(files: Vec<PathBuf>) -> Self {
        Self { files }
    }

    /// Discover and execute all test functions.
    ///
    /// Files that contain no test functions are skipped silently.
    /// Returns an error (and stops) if any test fails or if a file cannot be
    /// read / executed.
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
            let test_names = Self::discover_test_functions(&content);
            if test_names.is_empty() {
                continue;
            }

            let query = Self::build_test_query(&content, &test_names);
            let mut engine = mq_lang::DefaultEngine::default();
            engine.load_builtin_module();

            // Add the file's directory to the search path so relative `include`
            // statements in the test file resolve correctly.
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

    /// Parse `content` via the CST and return the names of all test functions.
    ///
    /// A top-level `def` node is included when:
    /// - Its name starts with `test_`, OR
    /// - Its `leading_trivia` contains a comment whose text (trimmed) is `@test`.
    fn discover_test_functions(content: &str) -> Vec<String> {
        let (nodes, _) = mq_lang::parse_recovery(content);
        let mut names = Vec::new();

        for node in &nodes {
            if node.kind != CstNodeKind::Def {
                continue;
            }

            // The function name is children[0] — same convention as the formatter.
            let func_name = match node.children.first() {
                Some(child) => child.to_string(),
                None => continue,
            };

            if func_name.is_empty() {
                continue;
            }

            if func_name.starts_with("test_") || Self::has_test_annotation(&node.leading_trivia) {
                names.push(func_name);
            }
        }

        names
    }

    /// Returns `true` if `trivia` contains a `# @test` annotation comment.
    fn has_test_annotation(trivia: &[CstTrivia]) -> bool {
        trivia
            .iter()
            .any(|t| t.comment().is_some_and(|c| c.trim() == "@test"))
    }

    /// Append an auto-generated `run_tests([…])` call to the file content.
    ///
    /// Display names strip the `test_` prefix to match the existing manual
    /// convention (e.g. `test_is_array` → `"is_array"`).
    fn build_test_query(content: &str, test_names: &[String]) -> String {
        let cases = test_names
            .iter()
            .map(|name| {
                let display = name.strip_prefix("test_").unwrap_or(name);
                format!("  test_case(\"{display}\", {name})")
            })
            .collect::<Vec<_>>()
            .join(",\n");

        format!("{content}\n| run_tests([\n{cases}\n])")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discover_test_prefix() {
        let content = r#"
def test_foo():
  None
end

def helper():
  None
end

def test_bar():
  None
end
"#;
        let names = TestRunner::discover_test_functions(content);
        assert_eq!(names, vec!["test_foo", "test_bar"]);
    }

    #[test]
    fn test_discover_annotation() {
        let content = "# @test\ndef my_check():\n  None\nend\n\ndef not_a_test():\n  None\nend\n";
        let names = TestRunner::discover_test_functions(content);
        assert_eq!(names, vec!["my_check"]);
    }

    #[test]
    fn test_build_test_query_strips_prefix() {
        let content = "include \"test\"\n|";
        let names = vec!["test_foo".to_string(), "test_bar".to_string()];
        let query = TestRunner::build_test_query(content, &names);
        assert!(query.contains("test_case(\"foo\", test_foo)"));
        assert!(query.contains("test_case(\"bar\", test_bar)"));
    }

    #[test]
    fn test_build_test_query_annotation_no_strip() {
        let content = "include \"test\"\n|";
        let names = vec!["my_check".to_string()];
        let query = TestRunner::build_test_query(content, &names);
        assert!(query.contains("test_case(\"my_check\", my_check)"));
    }
}
