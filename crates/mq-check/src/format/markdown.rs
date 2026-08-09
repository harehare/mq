use std::io::{self, Write};

use super::CheckDiagnostic;

/// Writes a GitHub-flavored Markdown table of diagnostics across every checked file.
pub(super) fn write_markdown_report(w: &mut impl Write, results: &[(String, Vec<CheckDiagnostic>)]) -> io::Result<()> {
    let issue_count: usize = results.iter().map(|(_, diagnostics)| diagnostics.len()).sum();

    writeln!(w, "# mq-check Report")?;
    writeln!(w)?;

    if issue_count == 0 {
        writeln!(w, "No type errors found.")?;
        return Ok(());
    }

    writeln!(w, "| File | Severity | Code | Location | Message |")?;
    writeln!(w, "| --- | --- | --- | --- | --- |")?;

    for (file_label, diagnostics) in results {
        for diagnostic in diagnostics {
            let loc = match &diagnostic.range {
                Some(range) => format!("{}:{}", range.start.line, range.start.column),
                None => String::new(),
            };

            writeln!(
                w,
                "| {} | {} | `{}` | {} | {} |",
                escape_cell(file_label),
                diagnostic.severity.as_str(),
                diagnostic.code,
                loc,
                escape_cell(&diagnostic.message),
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
    use crate::format::Severity;

    #[test]
    fn test_write_markdown_report_produces_table() {
        let diagnostics = vec![CheckDiagnostic {
            severity: Severity::Error,
            code: "typechecker::undefined_symbol",
            message: "Undefined symbol: foo".to_string(),
            range: Some(mq_lang::Range::default()),
        }];
        let results = vec![("test.mq".to_string(), diagnostics)];

        let mut buf = Vec::new();
        write_markdown_report(&mut buf, &results).unwrap();
        let output = String::from_utf8(buf).unwrap();

        assert!(output.contains("# mq-check Report"));
        assert!(output.contains("| File | Severity | Code | Location | Message |"));
        assert!(output.contains("`typechecker::undefined_symbol`"));
        assert!(output.contains("Undefined symbol: foo"));
        assert!(output.contains("**Found 1 issue.**"));
    }

    #[test]
    fn test_write_markdown_report_no_issues() {
        let results = vec![("test.mq".to_string(), Vec::new())];
        let mut buf = Vec::new();
        write_markdown_report(&mut buf, &results).unwrap();
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("No type errors found."));
        assert!(!output.contains('|'));
    }

    #[test]
    fn test_escape_cell() {
        assert_eq!(escape_cell("a | b"), "a \\| b");
        assert_eq!(escape_cell("line1\nline2"), "line1 line2");
    }
}
