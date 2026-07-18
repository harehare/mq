use std::io::{self, Write};

use super::CheckDiagnostic;

/// Writes a single JSON array of diagnostics across every checked file.
pub(super) fn write_json_report(w: &mut impl Write, results: &[(String, Vec<CheckDiagnostic>)]) -> io::Result<()> {
    let entries: Vec<serde_json::Value> = results
        .iter()
        .flat_map(|(file_label, diagnostics)| {
            diagnostics.iter().map(move |diagnostic| {
                serde_json::json!({
                    "file": file_label,
                    "severity": diagnostic.severity.as_str(),
                    "code": diagnostic.code,
                    "message": diagnostic.message,
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
    use crate::format::Severity;

    #[test]
    fn test_write_json_report_produces_expected_shape() {
        let diagnostics = vec![CheckDiagnostic {
            severity: Severity::Error,
            code: "typechecker::undefined_symbol",
            message: "Undefined symbol: foo".to_string(),
            range: Some(mq_lang::Range::default()),
        }];
        let results = vec![("test.mq".to_string(), diagnostics)];

        let mut buf = Vec::new();
        write_json_report(&mut buf, &results).unwrap();
        let json: serde_json::Value = serde_json::from_str(&String::from_utf8(buf).unwrap()).unwrap();

        assert_eq!(json[0]["file"], "test.mq");
        assert_eq!(json[0]["severity"], "error");
        assert_eq!(json[0]["code"], "typechecker::undefined_symbol");
        assert_eq!(json[0]["message"], "Undefined symbol: foo");
        assert!(json[0]["range"].is_object());
    }

    #[test]
    fn test_write_json_report_empty_diagnostics() {
        let results = vec![("test.mq".to_string(), Vec::new())];
        let mut buf = Vec::new();
        write_json_report(&mut buf, &results).unwrap();
        let json: serde_json::Value = serde_json::from_str(&String::from_utf8(buf).unwrap()).unwrap();
        assert_eq!(json.as_array().unwrap().len(), 0);
    }
}
