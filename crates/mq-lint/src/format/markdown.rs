use std::io::{self, Write};

use mq_lint::Diagnostic;

/// Writes a GitHub-flavored Markdown table of diagnostics across every linted file.
pub(super) fn write_markdown_report(
    w: &mut impl Write,
    results: &[(String, String, Vec<Diagnostic>)],
) -> io::Result<()> {
    let issue_count: usize = results.iter().map(|(_, _code, diagnostics)| diagnostics.len()).sum();

    writeln!(w, "# mq-lint Report")?;
    writeln!(w)?;

    if issue_count == 0 {
        writeln!(w, "No lint issues found.")?;
        return Ok(());
    }

    writeln!(w, "| File | Severity | Rule | Location | Message |")?;
    writeln!(w, "| --- | --- | --- | --- | --- |")?;

    for (file_label, _code, diagnostics) in results {
        for diagnostic in diagnostics {
            let loc = match &diagnostic.range {
                Some(range) => format!("{}:{}", range.start.line, range.start.column),
                None => String::new(),
            };
            let mut message = escape_cell(&diagnostic.message());
            if let Some(help) = diagnostic.help() {
                message.push_str(" — help: ");
                message.push_str(&escape_cell(&help));
            }

            writeln!(
                w,
                "| {} | {} | `{}` | {} | {} |",
                escape_cell(file_label),
                diagnostic.severity,
                diagnostic.rule_id().as_str(),
                loc,
                message,
            )?;
        }
    }

    writeln!(w)?;
    writeln!(
        w,
        "**Found {issue_count} issue{}.**",
        if issue_count == 1 { "" } else { "s" }
    )
}

/// Escapes pipes and newlines, which would otherwise break a Markdown table cell.
fn escape_cell(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use mq_lint::{LintConfig, Linter};

    fn sample_diagnostics() -> Vec<Diagnostic> {
        let config = LintConfig::default();
        let linter = Linter::with_default_rules();
        crate::collect_diagnostics(r#".checked == true"#, &linter, &config, mq_lint::Severity::Style)
    }

    #[test]
    fn test_write_markdown_report_produces_table() {
        let diagnostics = sample_diagnostics();
        let results = vec![("test.mq".to_string(), String::new(), diagnostics)];

        let mut buf = Vec::new();
        write_markdown_report(&mut buf, &results).unwrap();
        let output = String::from_utf8(buf).unwrap();

        assert!(output.contains("# mq-lint Report"));
        assert!(output.contains("| File | Severity | Rule | Location | Message |"));
        assert!(output.contains("test.mq"));
        assert!(output.contains("`boolean_comparison`"));
        assert!(output.contains("**Found 1 issue.**"));
    }

    #[test]
    fn test_write_markdown_report_no_issues() {
        let results = vec![("test.mq".to_string(), String::new(), Vec::new())];
        let mut buf = Vec::new();
        write_markdown_report(&mut buf, &results).unwrap();
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("No lint issues found."));
        assert!(!output.contains('|'));
    }

    #[test]
    fn test_escape_cell() {
        assert_eq!(escape_cell("a | b"), "a \\| b");
        assert_eq!(escape_cell("line1\nline2"), "line1 line2");
    }
}
