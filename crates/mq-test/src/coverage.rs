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
    /// suitable for `genhtml` or CI coverage integrations.
    Lcov,
    /// Self-contained HTML report.
    Html,
    /// Machine-readable JSON report.
    Json,
    /// Markdown report, suitable for pasting into a PR description or GitHub summary.
    Markdown,
    /// suitable for Jenkins/GitLab CI coverage integrations.
    Cobertura,
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
    /// The file's own source text, used to render per-line source in the HTML report.
    content: String,
}

impl FileCoverage {
    pub(crate) fn new(
        file: PathBuf,
        executable_lines: BTreeSet<usize>,
        visited_lines: FxHashSet<usize>,
        content: String,
    ) -> Self {
        Self {
            file,
            executable_lines,
            visited_lines,
            content,
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

    fn line_status(&self, line: usize) -> LineStatus {
        if !self.executable_lines.contains(&line) {
            LineStatus::Plain
        } else if self.is_hit(line) {
            LineStatus::Covered
        } else {
            LineStatus::Uncovered
        }
    }
}

/// Coverage classification of a single source line, shared by the HTML,
/// Markdown, and JSON renderers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LineStatus {
    /// Not considered executable (declarations, structural syntax, blank lines).
    Plain,
    /// Executable and visited by the evaluator.
    Covered,
    /// Executable but never visited.
    Uncovered,
}

impl LineStatus {
    fn as_str(self) -> &'static str {
        match self {
            LineStatus::Plain => "plain",
            LineStatus::Covered => "covered",
            LineStatus::Uncovered => "uncovered",
        }
    }

    /// Diff-style prefix: GitHub colors `+`/`-` lines in ` ```diff ` blocks green/red.
    fn diff_prefix(self) -> char {
        match self {
            LineStatus::Plain => ' ',
            LineStatus::Covered => '+',
            LineStatus::Uncovered => '-',
        }
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

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Classifies a coverage percentage into a badge color tier.
fn badge_class(pct: f64) -> &'static str {
    if pct >= 80.0 {
        "high"
    } else if pct >= 50.0 {
        "mid"
    } else {
        "low"
    }
}

const HTML_STYLE: &str = r#"
:root {
  color-scheme: light dark;
  --bg: #ffffff;
  --fg: #1a1a1a;
  --muted: #6b7280;
  --border: #e5e7eb;
  --row-alt: #f9fafb;
  --covered-bg: #d9f7e3;
  --covered-fg: #1a7f37;
  --uncovered-bg: #ffe3e3;
  --uncovered-fg: #c53030;
  --badge-high-bg: #d9f7e3;
  --badge-high-fg: #1a7f37;
  --badge-mid-bg: #fff3cd;
  --badge-mid-fg: #8a6100;
  --badge-low-bg: #ffe3e3;
  --badge-low-fg: #c53030;
}
@media (prefers-color-scheme: dark) {
  :root {
    --bg: #16181d;
    --fg: #e6e6e6;
    --muted: #9aa0a6;
    --border: #30333a;
    --row-alt: #1d2026;
    --covered-bg: #123822;
    --covered-fg: #4ada91;
    --uncovered-bg: #3a1618;
    --uncovered-fg: #ff8080;
    --badge-high-bg: #123822;
    --badge-high-fg: #4ada91;
    --badge-mid-bg: #3a2f00;
    --badge-mid-fg: #ffd766;
    --badge-low-bg: #3a1618;
    --badge-low-fg: #ff8080;
  }
}
* { box-sizing: border-box; }
body {
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
  margin: 2rem auto;
  max-width: 960px;
  color: var(--fg);
  background: var(--bg);
}
h1 { font-size: 1.4rem; }
table { border-collapse: collapse; width: 100%; }
th, td { text-align: left; padding: 0.5rem 0.8rem; border-bottom: 1px solid var(--border); }
th { background: var(--row-alt); font-size: 0.8rem; text-transform: uppercase; letter-spacing: 0.03em; color: var(--muted); }
tbody tr:hover { background: var(--row-alt); }
td.uncovered { color: var(--uncovered-fg); font-family: ui-monospace, monospace; font-size: 0.85rem; }
tfoot td { font-weight: 700; border-top: 2px solid var(--border); border-bottom: none; }
a { color: inherit; }
.badge { display: inline-block; padding: 0.15rem 0.6rem; border-radius: 999px; font-weight: 600; font-variant-numeric: tabular-nums; font-size: 0.85rem; }
.badge.high { background: var(--badge-high-bg); color: var(--badge-high-fg); }
.badge.mid { background: var(--badge-mid-bg); color: var(--badge-mid-fg); }
.badge.low { background: var(--badge-low-bg); color: var(--badge-low-fg); }
details { margin-top: 1rem; border: 1px solid var(--border); border-radius: 8px; padding: 0.6rem 1rem; }
summary { cursor: pointer; font-weight: 600; }
summary:hover { color: var(--muted); }
table.source { margin-top: 0.6rem; border: none; font-family: ui-monospace, monospace; font-size: 0.85rem; }
table.source td { padding: 0.1rem 0.6rem; border-bottom: none; white-space: pre; }
table.source td.lineno { color: var(--muted); text-align: right; user-select: none; width: 1%; }
table.source tr.covered td.code { background: var(--covered-bg); color: var(--covered-fg); }
table.source tr.uncovered td.code { background: var(--uncovered-bg); color: var(--uncovered-fg); }
"#;

/// Renders the per-line source table for a single file, with executable lines
/// highlighted green (covered) or red (uncovered).
fn format_html_source(cov: &FileCoverage, anchor: &str) -> String {
    let mut lines_html = String::new();

    for (i, line) in cov.content.lines().enumerate() {
        let line_no = i + 1;
        let status = cov.line_status(line_no);

        lines_html.push_str(&format!(
            "          <tr class=\"{class}\"><td class=\"lineno\">{line_no}</td><td class=\"code\">{code}</td></tr>\n",
            class = status.as_str(),
            code = html_escape(line),
        ));
    }

    format!(
        "      <details id=\"{anchor}\">\n\
        \x20       <summary>{file} — {pct:.1}% ({covered}/{total})</summary>\n\
        \x20       <table class=\"source\">\n\
        \x20         <tbody>\n\
        {lines_html}\
        \x20         </tbody>\n\
        \x20       </table>\n\
        \x20     </details>\n",
        anchor = anchor,
        file = html_escape(&cov.file.display().to_string()),
        pct = cov.percent(),
        covered = cov.covered_lines(),
        total = cov.total_lines(),
    )
}

/// Renders a self-contained HTML coverage report: a summary table plus a
/// collapsible, green/red line-highlighted source view per file.
pub(crate) fn format_html_report(coverages: &[FileCoverage]) -> String {
    let (mut total_covered, mut total_lines) = (0, 0);
    let mut rows = String::new();
    let mut sources = String::new();

    for (i, cov) in coverages.iter().enumerate() {
        total_covered += cov.covered_lines();
        total_lines += cov.total_lines();

        let uncovered = cov.uncovered_lines();
        let uncovered_text = if uncovered.is_empty() {
            "-".to_string()
        } else {
            uncovered.iter().map(|l| l.to_string()).collect::<Vec<_>>().join(", ")
        };

        let anchor = format!("file-{i}");

        rows.push_str(&format!(
            "      <tr>\n\
            \x20       <td><a href=\"#{anchor}\">{file}</a></td>\n\
            \x20       <td><span class=\"badge {badge}\">{pct:.1}%</span></td>\n\
            \x20       <td>{covered}/{total}</td>\n\
            \x20       <td class=\"uncovered\">{uncovered_text}</td>\n\
            \x20     </tr>\n",
            file = html_escape(&cov.file.display().to_string()),
            badge = badge_class(cov.percent()),
            pct = cov.percent(),
            covered = cov.covered_lines(),
            total = cov.total_lines(),
        ));

        sources.push_str(&format_html_source(cov, &anchor));
    }

    let overall = if total_lines == 0 {
        100.0
    } else {
        (total_covered as f64 / total_lines as f64) * 100.0
    };

    format!(
        "<!doctype html>\n\
        <html lang=\"en\">\n\
        <head>\n\
        \x20 <meta charset=\"utf-8\">\n\
        \x20 <title>mq-test coverage report</title>\n\
        \x20 <style>{HTML_STYLE}</style>\n\
        </head>\n\
        <body>\n\
        \x20 <h1>Coverage report</h1>\n\
        \x20 <table>\n\
        \x20   <thead>\n\
        \x20     <tr><th>File</th><th>Coverage</th><th>Lines</th><th>Uncovered lines</th></tr>\n\
        \x20   </thead>\n\
        \x20   <tbody>\n\
        {rows}\
        \x20   </tbody>\n\
        \x20   <tfoot>\n\
        \x20     <tr><td>Total</td><td><span class=\"badge {overall_badge}\">{overall:.1}%</span></td><td>{total_covered}/{total_lines}</td><td></td></tr>\n\
        \x20   </tfoot>\n\
        \x20 </table>\n\
        {sources}\
        </body>\n\
        </html>\n",
        overall_badge = badge_class(overall),
    )
}

/// Renders a Markdown coverage report: a summary table followed by each
/// file's source in a ` ```diff ` block, so GitHub and other diff-aware
/// Markdown renderers color covered/uncovered lines green/red.
pub(crate) fn format_markdown_report(coverages: &[FileCoverage]) -> String {
    let mut out =
        String::from("# Coverage report\n\n| File | Coverage | Lines | Uncovered lines |\n| --- | --- | --- | --- |\n");
    let (mut total_covered, mut total_lines) = (0, 0);

    for cov in coverages {
        total_covered += cov.covered_lines();
        total_lines += cov.total_lines();

        let uncovered = cov.uncovered_lines();
        let uncovered_text = if uncovered.is_empty() {
            "-".to_string()
        } else {
            uncovered.iter().map(|l| l.to_string()).collect::<Vec<_>>().join(", ")
        };

        out.push_str(&format!(
            "| `{file}` | {pct:.1}% | {covered}/{total} | {uncovered_text} |\n",
            file = cov.file.display(),
            pct = cov.percent(),
            covered = cov.covered_lines(),
            total = cov.total_lines(),
        ));
    }

    let overall = if total_lines == 0 {
        100.0
    } else {
        (total_covered as f64 / total_lines as f64) * 100.0
    };
    out.push_str(&format!("\n**Total: {overall:.1}% ({total_covered}/{total_lines})**\n"));

    for cov in coverages {
        out.push_str(&format!(
            "\n## {file} — {pct:.1}% ({covered}/{total})\n\n```diff\n",
            file = cov.file.display(),
            pct = cov.percent(),
            covered = cov.covered_lines(),
            total = cov.total_lines(),
        ));

        for (i, line) in cov.content.lines().enumerate() {
            out.push(cov.line_status(i + 1).diff_prefix());
            out.push_str(line);
            out.push('\n');
        }

        out.push_str("```\n");
    }

    out
}

/// Renders a machine-readable JSON coverage report.
pub(crate) fn format_json_report(coverages: &[FileCoverage]) -> String {
    let (mut total_covered, mut total_lines) = (0, 0);
    let files: Vec<serde_json::Value> = coverages
        .iter()
        .map(|cov| {
            total_covered += cov.covered_lines();
            total_lines += cov.total_lines();

            let lines: Vec<serde_json::Value> = cov
                .content
                .lines()
                .enumerate()
                .map(|(i, line)| {
                    let line_no = i + 1;
                    serde_json::json!({
                        "line": line_no,
                        "content": line,
                        "status": cov.line_status(line_no).as_str(),
                    })
                })
                .collect();

            serde_json::json!({
                "file": cov.file.display().to_string(),
                "totalLines": cov.total_lines(),
                "coveredLines": cov.covered_lines(),
                "percent": cov.percent(),
                "uncoveredLines": cov.uncovered_lines(),
                "lines": lines,
            })
        })
        .collect();

    let overall = if total_lines == 0 {
        100.0
    } else {
        (total_covered as f64 / total_lines as f64) * 100.0
    };

    let report = serde_json::json!({
        "files": files,
        "total": {
            "totalLines": total_lines,
            "coveredLines": total_covered,
            "percent": overall,
        },
    });

    serde_json::to_string_pretty(&report).expect("coverage report is always serializable")
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Renders a [Cobertura](https://cobertura.github.io/cobertura/) XML coverage report.
///
/// Since `mq` has no notion of packages/classes, each file is reported as a
/// single class within a single package.
pub(crate) fn format_cobertura_report(coverages: &[FileCoverage]) -> String {
    let (mut total_covered, mut total_lines) = (0, 0);
    let mut packages = String::new();

    for cov in coverages {
        total_covered += cov.covered_lines();
        total_lines += cov.total_lines();

        let file = xml_escape(&cov.file.display().to_string());
        let mut lines = String::new();
        for line in &cov.executable_lines {
            let hits = if cov.is_hit(*line) { 1 } else { 0 };
            lines.push_str(&format!("          <line number=\"{line}\" hits=\"{hits}\"/>\n"));
        }

        packages.push_str(&format!(
            "  <package name=\"{file}\" line-rate=\"{rate:.4}\" branch-rate=\"1.0\">\n\
            \x20   <classes>\n\
            \x20     <class name=\"{file}\" filename=\"{file}\" line-rate=\"{rate:.4}\" branch-rate=\"1.0\">\n\
            \x20       <lines>\n\
            {lines}\
            \x20       </lines>\n\
            \x20     </class>\n\
            \x20   </classes>\n\
            \x20 </package>\n",
            rate = if cov.total_lines() == 0 {
                1.0
            } else {
                cov.covered_lines() as f64 / cov.total_lines() as f64
            },
        ));
    }

    let overall_rate = if total_lines == 0 {
        1.0
    } else {
        total_covered as f64 / total_lines as f64
    };

    format!(
        "<?xml version=\"1.0\"?>\n\
        <coverage line-rate=\"{overall_rate:.4}\" branch-rate=\"1.0\" lines-covered=\"{total_covered}\" \
        lines-valid=\"{total_lines}\" version=\"mq-test\">\n\
        \x20 <packages>\n\
        {packages}\
        \x20 </packages>\n\
        </coverage>\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

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
            "a\nb\nc\nd\n".to_string(),
        );
        assert_eq!(cov.total_lines(), 4);
        assert_eq!(cov.covered_lines(), 2);
        assert_eq!(cov.uncovered_lines(), vec![3, 4]);
        assert_eq!(cov.percent(), 50.0);
    }

    #[test]
    fn test_file_coverage_percent_no_executable_lines() {
        let cov = FileCoverage::new(
            PathBuf::from("empty.mq"),
            BTreeSet::new(),
            FxHashSet::default(),
            String::new(),
        );
        assert_eq!(cov.percent(), 100.0);
    }

    #[test]
    fn test_format_text_report_contains_summary() {
        let cov = FileCoverage::new(
            PathBuf::from("test.mq"),
            BTreeSet::from([1, 2, 3, 4]),
            [1, 2].into_iter().collect(),
            "a\nb\nc\nd\n".to_string(),
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
            "a\nb\n".to_string(),
        );
        let report = format_lcov_report(&[cov]);
        assert!(report.contains("SF:test.mq\n"));
        assert!(report.contains("DA:1,1\n"));
        assert!(report.contains("DA:2,0\n"));
        assert!(report.contains("LF:2\n"));
        assert!(report.contains("LH:1\n"));
        assert!(report.contains("end_of_record\n"));
    }

    #[test]
    fn test_format_html_report_structure() {
        let cov = FileCoverage::new(
            PathBuf::from("test.mq"),
            BTreeSet::from([1, 2, 3, 4]),
            [1, 2].into_iter().collect(),
            "a\nb\nc\nd\n".to_string(),
        );
        let report = format_html_report(&[cov]);
        assert!(report.starts_with("<!doctype html>"));
        assert!(report.contains("test.mq"));
        assert!(report.contains("50.0%"));
        assert!(report.contains("2/4"));
        assert!(report.contains("3, 4"));
        assert!(report.contains("Total"));
    }

    #[test]
    fn test_format_html_report_escapes_file_path() {
        let cov = FileCoverage::new(
            PathBuf::from("<a & b>.mq"),
            BTreeSet::new(),
            FxHashSet::default(),
            String::new(),
        );
        let report = format_html_report(&[cov]);
        assert!(report.contains("&lt;a &amp; b&gt;.mq"));
        assert!(!report.contains("<a & b>.mq"));
    }

    #[test]
    fn test_format_html_report_highlights_covered_and_uncovered_lines() {
        let cov = FileCoverage::new(
            PathBuf::from("test.mq"),
            BTreeSet::from([1, 2, 3]),
            [1, 3].into_iter().collect(),
            "covered one\nuncovered\ncovered two\n".to_string(),
        );
        let report = format_html_report(&[cov]);

        // Line 1 and 3 were visited -> covered (green); line 2 was not -> uncovered (red).
        assert!(
            report
                .contains("<tr class=\"covered\"><td class=\"lineno\">1</td><td class=\"code\">covered one</td></tr>")
        );
        assert!(
            report
                .contains("<tr class=\"uncovered\"><td class=\"lineno\">2</td><td class=\"code\">uncovered</td></tr>")
        );
        assert!(
            report
                .contains("<tr class=\"covered\"><td class=\"lineno\">3</td><td class=\"code\">covered two</td></tr>")
        );
        assert!(report.contains("tr.covered td.code { background: var(--covered-bg)"));
        assert!(report.contains("tr.uncovered td.code { background: var(--uncovered-bg)"));
        assert!(report.contains("prefers-color-scheme: dark"));
        assert!(report.contains("<span class=\"badge mid\">66.7%</span>"));
    }

    #[test]
    fn test_format_html_report_marks_non_executable_lines_plain() {
        let cov = FileCoverage::new(
            PathBuf::from("test.mq"),
            BTreeSet::from([2]),
            [2].into_iter().collect(),
            "def foo():\n  1 + 1\nend\n".to_string(),
        );
        let report = format_html_report(&[cov]);

        assert!(
            report.contains("<tr class=\"plain\"><td class=\"lineno\">1</td><td class=\"code\">def foo():</td></tr>")
        );
        assert!(report.contains("<tr class=\"covered\"><td class=\"lineno\">2</td>"));
        assert!(report.contains("<tr class=\"plain\"><td class=\"lineno\">3</td><td class=\"code\">end</td></tr>"));
    }

    #[rstest]
    #[case(100.0, "high")]
    #[case(80.0, "high")]
    #[case(79.9, "mid")]
    #[case(50.0, "mid")]
    #[case(49.9, "low")]
    #[case(0.0, "low")]
    fn test_badge_class(#[case] pct: f64, #[case] expected: &str) {
        assert_eq!(badge_class(pct), expected);
    }

    #[test]
    fn test_format_markdown_report_summary_and_totals() {
        let cov = FileCoverage::new(
            PathBuf::from("test.mq"),
            BTreeSet::from([1, 2, 3, 4]),
            [1, 2].into_iter().collect(),
            "a\nb\nc\nd\n".to_string(),
        );
        let report = format_markdown_report(&[cov]);
        assert!(report.starts_with("# Coverage report"));
        assert!(report.contains("| `test.mq` | 50.0% | 2/4 | 3, 4 |"));
        assert!(report.contains("**Total: 50.0% (2/4)**"));
    }

    #[test]
    fn test_format_markdown_report_diff_highlights_covered_and_uncovered_lines() {
        let cov = FileCoverage::new(
            PathBuf::from("test.mq"),
            BTreeSet::from([1, 2, 3]),
            [1, 3].into_iter().collect(),
            "covered one\nuncovered\ncovered two\n".to_string(),
        );
        let report = format_markdown_report(&[cov]);

        assert!(report.contains("```diff\n"));
        assert!(report.contains("+covered one\n"));
        assert!(report.contains("-uncovered\n"));
        assert!(report.contains("+covered two\n"));
    }

    #[test]
    fn test_format_markdown_report_marks_non_executable_lines_plain() {
        let cov = FileCoverage::new(
            PathBuf::from("test.mq"),
            BTreeSet::from([2]),
            [2].into_iter().collect(),
            "def foo():\n  1 + 1\nend\n".to_string(),
        );
        let report = format_markdown_report(&[cov]);

        assert!(report.contains(" def foo():\n"));
        assert!(report.contains("+  1 + 1\n"));
        assert!(report.contains(" end\n"));
    }

    #[test]
    fn test_format_json_report_structure() {
        let cov = FileCoverage::new(
            PathBuf::from("test.mq"),
            BTreeSet::from([1, 2, 3, 4]),
            [1, 2].into_iter().collect(),
            "a\nb\nc\nd\n".to_string(),
        );
        let report = format_json_report(&[cov]);
        let parsed: serde_json::Value = serde_json::from_str(&report).expect("valid json");
        assert_eq!(parsed["files"][0]["file"], "test.mq");
        assert_eq!(parsed["files"][0]["totalLines"], 4);
        assert_eq!(parsed["files"][0]["coveredLines"], 2);
        assert_eq!(parsed["files"][0]["percent"], 50.0);
        assert_eq!(parsed["files"][0]["uncoveredLines"], serde_json::json!([3, 4]));
        assert_eq!(parsed["total"]["totalLines"], 4);
        assert_eq!(parsed["total"]["coveredLines"], 2);
        assert_eq!(parsed["total"]["percent"], 50.0);
    }

    #[test]
    fn test_format_json_report_includes_per_line_status() {
        let cov = FileCoverage::new(
            PathBuf::from("test.mq"),
            BTreeSet::from([1, 2, 3]),
            [1, 3].into_iter().collect(),
            "covered one\nuncovered\ncovered two\n".to_string(),
        );
        let report = format_json_report(&[cov]);
        let parsed: serde_json::Value = serde_json::from_str(&report).expect("valid json");
        let lines = &parsed["files"][0]["lines"];
        assert_eq!(
            lines[0],
            serde_json::json!({"line": 1, "content": "covered one", "status": "covered"})
        );
        assert_eq!(
            lines[1],
            serde_json::json!({"line": 2, "content": "uncovered", "status": "uncovered"})
        );
        assert_eq!(
            lines[2],
            serde_json::json!({"line": 3, "content": "covered two", "status": "covered"})
        );
    }

    #[test]
    fn test_format_cobertura_report_structure() {
        let cov = FileCoverage::new(
            PathBuf::from("test.mq"),
            BTreeSet::from([1, 2]),
            [1].into_iter().collect(),
            "a\nb\n".to_string(),
        );
        let report = format_cobertura_report(&[cov]);
        assert!(report.starts_with("<?xml version=\"1.0\"?>"));
        assert!(
            report.contains("<coverage line-rate=\"0.5000\" branch-rate=\"1.0\" lines-covered=\"1\" lines-valid=\"2\"")
        );
        assert!(report.contains("filename=\"test.mq\""));
        assert!(report.contains("<line number=\"1\" hits=\"1\"/>"));
        assert!(report.contains("<line number=\"2\" hits=\"0\"/>"));
    }

    #[test]
    fn test_format_cobertura_report_escapes_file_path() {
        let cov = FileCoverage::new(
            PathBuf::from("<a & b>.mq"),
            BTreeSet::new(),
            FxHashSet::default(),
            String::new(),
        );
        let report = format_cobertura_report(&[cov]);
        assert!(report.contains("&lt;a &amp; b&gt;.mq"));
        assert!(!report.contains("<a & b>.mq"));
    }
}
