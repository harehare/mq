use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use mq_lang::{CstNode, CstNodeKind, DebugContext, DebuggerAction, DebuggerHandler, Shared};
use rustc_hash::FxHashSet;

/// Output format for a coverage report.
#[derive(clap::ValueEnum, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CoverageFormat {
    /// Human-readable summary printed to the terminal.
    #[default]
    Text,
    /// [lcov tracefile](https://ltp.sourceforge.net/coverage/lcov/geninfo.1.php) format,
    /// suitable for `genhtml` or CI coverage integrations.
    Lcov,
}

/// Shared, thread-safe accumulator of source lines visited during a single test-file run.
#[derive(Debug, Default, Clone)]
pub(crate) struct CoverageData(Arc<Mutex<FxHashSet<usize>>>);

impl CoverageData {
    fn record(&self, line: usize) {
        self.0.lock().unwrap().insert(line);
    }

    pub(crate) fn snapshot(&self) -> FxHashSet<usize> {
        self.0.lock().unwrap().clone()
    }
}

/// Records the line of every top-level-module expression visited by the evaluator.
///
/// Lines belonging to `include`d/imported modules are intentionally ignored;
/// only the coverage of the test file itself is tracked.
#[derive(Debug)]
pub(crate) struct CoverageHandler(pub(crate) CoverageData);

impl DebuggerHandler for CoverageHandler {
    fn on_step(&self, context: &DebugContext) -> DebuggerAction {
        if context.source.name.is_none() {
            self.0.record(context.token.range.start.line as usize);
        }
        // Keep single-stepping through every expression in the program.
        DebuggerAction::StepInto
    }
}

/// Coverage result for a single test file.
#[derive(Debug, Clone)]
pub struct FileCoverage {
    pub file: PathBuf,
    executable_lines: BTreeSet<usize>,
    visited_lines: FxHashSet<usize>,
}

impl FileCoverage {
    pub(crate) fn new(file: PathBuf, executable_lines: BTreeSet<usize>, visited_lines: FxHashSet<usize>) -> Self {
        Self {
            file,
            executable_lines,
            visited_lines,
        }
    }

    /// Number of lines considered executable (the coverage denominator).
    pub fn total_lines(&self) -> usize {
        self.executable_lines.len()
    }

    /// Number of executable lines that were visited at least once.
    pub fn covered_lines(&self) -> usize {
        self.executable_lines
            .iter()
            .filter(|line| self.visited_lines.contains(line))
            .count()
    }

    /// Executable lines that were never visited, in ascending order.
    pub fn uncovered_lines(&self) -> Vec<usize> {
        self.executable_lines
            .iter()
            .filter(|line| !self.visited_lines.contains(line))
            .copied()
            .collect()
    }

    /// Percentage of executable lines covered, in `[0.0, 100.0]`.
    /// A file with no executable lines is reported as fully covered.
    pub fn percent(&self) -> f64 {
        let total = self.total_lines();
        if total == 0 {
            100.0
        } else {
            (self.covered_lines() as f64 / total as f64) * 100.0
        }
    }

    fn is_hit(&self, line: usize) -> bool {
        self.visited_lines.contains(&line)
    }
}

/// CST node kinds that are excluded from the set of "executable" lines used
/// as the coverage denominator: either purely structural wrappers/delimiters,
/// or declaration statements (`def`, `include`, `import`) whose own line is
/// registered without going through the evaluator's per-expression debugger
/// hook, so it would otherwise be permanently reported as uncovered. Their
/// bodies/children are still collected independently.
fn is_structural(kind: &CstNodeKind) -> bool {
    matches!(
        kind,
        CstNodeKind::Module
            | CstNodeKind::Nodes
            | CstNodeKind::Block
            | CstNodeKind::End
            | CstNodeKind::Pattern
            | CstNodeKind::OrPattern
            | CstNodeKind::Token
            | CstNodeKind::Eof
            | CstNodeKind::Def
            | CstNodeKind::Include
            | CstNodeKind::Import
    )
}

fn collect_executable_lines(nodes: &[Shared<CstNode>], max_line: usize, lines: &mut BTreeSet<usize>) {
    for node in nodes {
        if !is_structural(&node.kind)
            && let Some(token) = &node.token
        {
            let line = token.range.start.line as usize;
            if line >= 1 && line <= max_line {
                lines.insert(line);
            }
        }

        match node.kind {
            // Only the function body counts toward coverage — the signature
            // (name + params) is declaration syntax, not a per-step execution.
            CstNodeKind::Def => {
                let (_, program) = node.split_cond_and_program();
                collect_executable_lines(&program, max_line, lines);
            }
            // Single-statement declarations have no body worth descending into.
            CstNodeKind::Include | CstNodeKind::Import => {}
            _ => collect_executable_lines(&node.children, max_line, lines),
        }
    }
}

/// Determines the set of "executable" lines in `content` (bounded to the file's
/// own line count, since `content` may have a generated test harness appended).
pub(crate) fn executable_lines(content: &str) -> BTreeSet<usize> {
    let max_line = content.lines().count().max(1);
    let (nodes, _) = mq_lang::parse_recovery(content);
    let mut lines = BTreeSet::new();
    collect_executable_lines(&nodes, max_line, &mut lines);
    lines
}

/// Renders a human-readable coverage summary.
pub(crate) fn format_text_report(coverages: &[FileCoverage]) -> String {
    let mut out = String::from("\nCoverage report:\n");
    let (mut total_covered, mut total_lines) = (0, 0);

    for cov in coverages {
        total_covered += cov.covered_lines();
        total_lines += cov.total_lines();

        out.push_str(&format!(
            "  {:<50} {:>6.1}% ({}/{})\n",
            cov.file.display(),
            cov.percent(),
            cov.covered_lines(),
            cov.total_lines()
        ));

        let uncovered = cov.uncovered_lines();
        if !uncovered.is_empty() {
            let lines = uncovered.iter().map(|l| l.to_string()).collect::<Vec<_>>().join(", ");
            out.push_str(&format!("      uncovered lines: {lines}\n"));
        }
    }

    let overall = if total_lines == 0 {
        100.0
    } else {
        (total_covered as f64 / total_lines as f64) * 100.0
    };

    out.push_str(&format!("\n  Total: {overall:.1}% ({total_covered}/{total_lines})\n"));
    out
}

/// Renders an lcov tracefile.
pub(crate) fn format_lcov_report(coverages: &[FileCoverage]) -> String {
    let mut out = String::new();

    for cov in coverages {
        out.push_str(&format!("SF:{}\n", cov.file.display()));
        for line in &cov.executable_lines {
            let hit = if cov.is_hit(*line) { 1 } else { 0 };
            out.push_str(&format!("DA:{line},{hit}\n"));
        }
        out.push_str(&format!("LF:{}\n", cov.total_lines()));
        out.push_str(&format!("LH:{}\n", cov.covered_lines()));
        out.push_str("end_of_record\n");
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_executable_lines_excludes_structural_nodes() {
        let content = "def foo():\n  1 + 1\nend\n";
        let lines = executable_lines(content);
        // Line 1 (`def foo():`) is a declaration, not a per-step execution.
        // Line 2 (`1 + 1`) is the body. Line 3 (`end`) is structural.
        assert!(!lines.contains(&1), "def line should not be executable: {lines:?}");
        assert!(lines.contains(&2), "body line should be executable: {lines:?}");
        assert!(!lines.contains(&3), "end line should not be executable: {lines:?}");
    }

    #[test]
    fn test_executable_lines_bounded_by_content_line_count() {
        // Callers must pass the original file content — not the query with the
        // generated `run_tests(...)` harness appended — so that lines beyond
        // the file's own line count are never reported.
        let content = "def foo():\n  1 + 1\nend\n";
        let lines = executable_lines(content);
        let max_line = content.lines().count();
        assert!(
            lines.iter().all(|&l| l <= max_line),
            "lines must be bounded by content's own line count: {lines:?}"
        );
    }

    #[test]
    fn test_file_coverage_percent() {
        let cov = FileCoverage::new(
            PathBuf::from("test.mq"),
            BTreeSet::from([1, 2, 3, 4]),
            [1, 2].into_iter().collect(),
        );
        assert_eq!(cov.total_lines(), 4);
        assert_eq!(cov.covered_lines(), 2);
        assert_eq!(cov.uncovered_lines(), vec![3, 4]);
        assert_eq!(cov.percent(), 50.0);
    }

    #[test]
    fn test_file_coverage_percent_no_executable_lines() {
        let cov = FileCoverage::new(PathBuf::from("empty.mq"), BTreeSet::new(), FxHashSet::default());
        assert_eq!(cov.percent(), 100.0);
    }

    #[test]
    fn test_format_text_report_contains_summary() {
        let cov = FileCoverage::new(
            PathBuf::from("test.mq"),
            BTreeSet::from([1, 2, 3, 4]),
            [1, 2].into_iter().collect(),
        );
        let report = format_text_report(&[cov]);
        assert!(report.contains("test.mq"));
        assert!(report.contains("50.0%"));
        assert!(report.contains("uncovered lines: 3, 4"));
        assert!(report.contains("Total: 50.0% (2/4)"));
    }

    #[test]
    fn test_format_lcov_report_structure() {
        let cov = FileCoverage::new(
            PathBuf::from("test.mq"),
            BTreeSet::from([1, 2]),
            [1].into_iter().collect(),
        );
        let report = format_lcov_report(&[cov]);
        assert!(report.contains("SF:test.mq\n"));
        assert!(report.contains("DA:1,1\n"));
        assert!(report.contains("DA:2,0\n"));
        assert!(report.contains("LF:2\n"));
        assert!(report.contains("LH:1\n"));
        assert!(report.contains("end_of_record\n"));
    }
}
