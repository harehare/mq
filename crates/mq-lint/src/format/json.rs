use std::io::{self, Write};

use mq_lint::Diagnostic;

/// Writes a single JSON array of diagnostics across every linted file.
pub(super) fn write_json_report(w: &mut impl Write, results: &[(String, String, Vec<Diagnostic>)]) -> io::Result<()> {
    let entries: Vec<serde_json::Value> = results
        .iter()
        .flat_map(|(file_label, _code, diagnostics)| {
            diagnostics.iter().map(move |diagnostic| {
                serde_json::json!({
                    "file": file_label,
                    "severity": diagnostic.severity.to_string(),
                    "rule": diagnostic.rule_id().as_str(),
                    "message": diagnostic.message(),
                    "help": diagnostic.help(),
                    "range": diagnostic.range.map(|range| serde_json::json!({
                        "startLine": range.start.line,
                        "startColumn": range.start.column,
                        "endLine": range.end.line,
                        "endColumn": range.end.column,
                    })),
                })
            })
        })
        .collect();

    writeln!(
        w,
        "{}",
        serde_json::to_string_pretty(&entries).map_err(io::Error::other)?
    )
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
    fn test_write_json_report_produces_expected_shape() {
        let diagnostics = sample_diagnostics();
        let results = vec![("test.mq".to_string(), String::new(), diagnostics)];

        let mut buf = Vec::new();
        write_json_report(&mut buf, &results).unwrap();
        let json: serde_json::Value = serde_json::from_str(&String::from_utf8(buf).unwrap()).unwrap();

        assert_eq!(json[0]["file"], "test.mq");
        assert_eq!(json[0]["rule"], "boolean_comparison");
        assert!(json[0]["message"].is_string());
        assert!(json[0]["range"].is_object());
    }

    #[test]
    fn test_write_json_report_empty_diagnostics() {
        let results = vec![("test.mq".to_string(), String::new(), Vec::new())];
        let mut buf = Vec::new();
        write_json_report(&mut buf, &results).unwrap();
        let json: serde_json::Value = serde_json::from_str(&String::from_utf8(buf).unwrap()).unwrap();
        assert_eq!(json.as_array().unwrap().len(), 0);
    }
}
