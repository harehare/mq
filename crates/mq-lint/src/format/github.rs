use std::io::{self, Write};

use mq_lint::{Diagnostic, Severity};

/// Writes one GitHub Actions workflow-command annotation
/// (`::error`/`::warning`/`::notice`) per diagnostic.
///
/// See <https://docs.github.com/en/actions/using-workflows/workflow-commands-for-github-actions#setting-an-error-message>.
pub(super) fn write_github_report(w: &mut impl Write, results: &[(String, Vec<Diagnostic>)]) -> io::Result<()> {
    for (file_label, diagnostics) in results {
        for diagnostic in diagnostics {
            let level = match diagnostic.severity {
                Severity::Error => "error",
                Severity::Warn => "warning",
                Severity::Perf | Severity::Style => "notice",
            };

            let mut props = format!("file={}", gha_escape_property(file_label));
            if let Some(range) = &diagnostic.range {
                props.push_str(&format!(",line={},col={}", range.start.line, range.start.column));
            }
            props.push_str(&format!(
                ",title={}",
                gha_escape_property(&format!("mq-lint/{}", diagnostic.rule_id().as_str()))
            ));

            writeln!(w, "::{level} {props}::{}", gha_escape_data(&diagnostic.message()))?;
        }
    }
    Ok(())
}

/// Escapes a GitHub Actions workflow-command data value (the part after the final `::`).
fn gha_escape_data(s: &str) -> String {
    s.replace('%', "%25").replace('\r', "%0D").replace('\n', "%0A")
}

/// Escapes a GitHub Actions workflow-command property value (e.g. `file=`, `title=`).
fn gha_escape_property(s: &str) -> String {
    gha_escape_data(s).replace(':', "%3A").replace(',', "%2C")
}

#[cfg(test)]
mod tests {
    use super::*;
    use mq_lint::{Diagnostic, LintConfig, Linter};
    use rstest::rstest;

    fn sample_diagnostics() -> Vec<Diagnostic> {
        let config = LintConfig::default();
        let linter = Linter::with_default_rules();
        crate::collect_diagnostics(r#".checked == true"#, &linter, &config, Severity::Style)
    }

    #[test]
    fn test_write_github_report_emits_workflow_command_annotations() {
        let diagnostics = sample_diagnostics();
        let results = vec![("test.mq".to_string(), diagnostics)];

        let mut buf = Vec::new();
        write_github_report(&mut buf, &results).unwrap();
        let output = String::from_utf8(buf).unwrap();

        assert!(output.starts_with("::notice file=test.mq,line=1,col=1,title=mq-lint/boolean_comparison::"));
    }

    #[rstest]
    #[case(Severity::Error, "error")]
    #[case(Severity::Warn, "warning")]
    #[case(Severity::Perf, "notice")]
    #[case(Severity::Style, "notice")]
    fn test_write_github_report_severity_to_level(#[case] severity: Severity, #[case] expected_level: &str) {
        let kind = mq_lint::LintMessage::BooleanComparison {
            op: "==".to_string(),
            bool_val: "true".to_string(),
        };
        let diagnostic = Diagnostic::new(kind, severity);
        let results = vec![("test.mq".to_string(), vec![diagnostic])];

        let mut buf = Vec::new();
        write_github_report(&mut buf, &results).unwrap();
        let output = String::from_utf8(buf).unwrap();

        assert!(output.starts_with(&format!("::{expected_level} ")));
    }

    #[rstest]
    #[case("100%", "100%25")]
    #[case("line1\nline2", "line1%0Aline2")]
    #[case("a\rb", "a%0Db")]
    fn test_gha_escape_data(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(gha_escape_data(input), expected);
    }

    #[rstest]
    #[case("a,b", "a%2Cb")]
    #[case("a:b", "a%3Ab")]
    #[case("100%,x", "100%25%2Cx")]
    fn test_gha_escape_property(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(gha_escape_property(input), expected);
    }
}
