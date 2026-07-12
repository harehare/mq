use std::io::{self, Write};

use mq_lint::{Diagnostic, Severity};

/// Writes a single SARIF 2.1.0 log document covering every linted file.
///
/// See <https://docs.oasis-open.org/sarif/sarif/v2.1.0/os/sarif-v2.1.0-os.html>.
pub(super) fn write_sarif_report(w: &mut impl Write, results: &[(String, Vec<Diagnostic>)]) -> io::Result<()> {
    let sarif_results: Vec<serde_json::Value> = results
        .iter()
        .flat_map(|(file_label, diagnostics)| {
            diagnostics.iter().map(move |diagnostic| {
                let mut physical_location = serde_json::json!({
                    "artifactLocation": {"uri": file_label},
                });
                if let Some(range) = &diagnostic.range {
                    physical_location["region"] = serde_json::json!({
                        "startLine": range.start.line,
                        "startColumn": range.start.column,
                        "endLine": range.end.line,
                        "endColumn": range.end.column,
                    });
                }

                serde_json::json!({
                    "ruleId": diagnostic.rule_id().as_str(),
                    "level": sarif_level(diagnostic.severity),
                    "message": {"text": diagnostic.message()},
                    "locations": [{"physicalLocation": physical_location}],
                })
            })
        })
        .collect();

    let sarif = serde_json::json!({
        "$schema": "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/master/Schemata/sarif-schema-2.1.0.json",
        "version": "2.1.0",
        "runs": [{
            "tool": {
                "driver": {
                    "name": "mq-lint",
                    "informationUri": "https://github.com/harehare/mq",
                    "version": env!("CARGO_PKG_VERSION"),
                }
            },
            "results": sarif_results,
        }],
    });

    writeln!(w, "{}", serde_json::to_string_pretty(&sarif).map_err(io::Error::other)?)
}

/// Maps a lint [`Severity`] to a SARIF result `level` (`error`, `warning`, or `note`).
fn sarif_level(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warn => "warning",
        Severity::Perf | Severity::Style => "note",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mq_lint::{LintConfig, Linter};

    fn sample_diagnostics() -> Vec<Diagnostic> {
        let config = LintConfig::default();
        let linter = Linter::with_default_rules();
        crate::collect_diagnostics(r#".checked == true"#, &linter, &config, Severity::Style)
    }

    #[test]
    fn test_write_sarif_report_produces_valid_sarif_shape() {
        let diagnostics = sample_diagnostics();
        assert!(!diagnostics.is_empty());
        let results = vec![("test.mq".to_string(), diagnostics)];

        let mut buf = Vec::new();
        write_sarif_report(&mut buf, &results).unwrap();
        let output = String::from_utf8(buf).unwrap();
        let json: serde_json::Value = serde_json::from_str(&output).unwrap();

        assert_eq!(json["version"], "2.1.0");
        assert_eq!(json["runs"][0]["tool"]["driver"]["name"], "mq-lint");
        let result = &json["runs"][0]["results"][0];
        assert_eq!(result["ruleId"], "boolean_comparison");
        assert_eq!(result["level"], "note");
        assert_eq!(
            result["locations"][0]["physicalLocation"]["artifactLocation"]["uri"],
            "test.mq"
        );
        assert_eq!(result["locations"][0]["physicalLocation"]["region"]["startLine"], 1);
    }

    #[test]
    fn test_write_sarif_report_empty_diagnostics() {
        let results = vec![("test.mq".to_string(), Vec::new())];
        let mut buf = Vec::new();
        write_sarif_report(&mut buf, &results).unwrap();
        let json: serde_json::Value = serde_json::from_str(&String::from_utf8(buf).unwrap()).unwrap();
        assert_eq!(json["runs"][0]["results"].as_array().unwrap().len(), 0);
    }
}
