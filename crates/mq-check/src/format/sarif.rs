use std::io::{self, Write};

use super::{CheckDiagnostic, Severity};

/// Writes a single SARIF 2.1.0 log document covering every checked file.
///
/// See <https://docs.oasis-open.org/sarif/sarif/v2.1.0/os/sarif-v2.1.0-os.html>.
pub(super) fn write_sarif_report(w: &mut impl Write, results: &[(String, Vec<CheckDiagnostic>)]) -> io::Result<()> {
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
                    "ruleId": diagnostic.code,
                    "level": sarif_level(diagnostic.severity),
                    "message": {"text": diagnostic.message},
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
                    "name": "mq-check",
                    "informationUri": "https://github.com/harehare/mq",
                    "version": env!("CARGO_PKG_VERSION"),
                }
            },
            "results": sarif_results,
        }],
    });

    writeln!(w, "{}", serde_json::to_string_pretty(&sarif).map_err(io::Error::other)?)
}

/// Maps a check [`Severity`] to a SARIF result `level` (`error` or `warning`).
fn sarif_level(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_sarif_report_produces_valid_sarif_shape() {
        let diagnostics = vec![CheckDiagnostic {
            severity: Severity::Error,
            code: "typechecker::undefined_symbol",
            message: "Undefined symbol: foo".to_string(),
            range: Some(mq_lang::Range::default()),
        }];
        let results = vec![("test.mq".to_string(), diagnostics)];

        let mut buf = Vec::new();
        write_sarif_report(&mut buf, &results).unwrap();
        let json: serde_json::Value = serde_json::from_str(&String::from_utf8(buf).unwrap()).unwrap();

        assert_eq!(json["version"], "2.1.0");
        assert_eq!(json["runs"][0]["tool"]["driver"]["name"], "mq-check");
        let result = &json["runs"][0]["results"][0];
        assert_eq!(result["ruleId"], "typechecker::undefined_symbol");
        assert_eq!(result["level"], "error");
        assert_eq!(
            result["locations"][0]["physicalLocation"]["artifactLocation"]["uri"],
            "test.mq"
        );
    }

    #[test]
    fn test_write_sarif_report_empty_diagnostics() {
        let results = vec![("test.mq".to_string(), Vec::new())];
        let mut buf = Vec::new();
        write_sarif_report(&mut buf, &results).unwrap();
        let json: serde_json::Value = serde_json::from_str(&String::from_utf8(buf).unwrap()).unwrap();
        assert_eq!(json["runs"][0]["results"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn test_sarif_level_maps_severity() {
        assert_eq!(sarif_level(Severity::Error), "error");
        assert_eq!(sarif_level(Severity::Warning), "warning");
    }
}
