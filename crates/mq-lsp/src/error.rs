use tower_lsp_server::ls_types;
use tower_lsp_server::ls_types::NumberOrString;

pub type SyntaxError = (std::string::String, mq_lang::Range);

#[derive(Debug, Clone)]
pub enum LspError {
    SyntaxError(SyntaxError),
    TypeError(mq_check::TypeError),
    LintWarning(mq_lint::Diagnostic),
}

fn lsp_range(range: mq_lang::Range) -> ls_types::Range {
    ls_types::Range::new(
        ls_types::Position {
            line: range.start.line.saturating_sub(1),
            character: range.start.column.saturating_sub(1) as u32,
        },
        ls_types::Position {
            line: range.end.line.saturating_sub(1),
            character: range.end.column.saturating_sub(1) as u32,
        },
    )
}

fn lint_severity(severity: mq_lint::Severity) -> ls_types::DiagnosticSeverity {
    match severity {
        mq_lint::Severity::Error => ls_types::DiagnosticSeverity::ERROR,
        mq_lint::Severity::Warn => ls_types::DiagnosticSeverity::WARNING,
        mq_lint::Severity::Perf => ls_types::DiagnosticSeverity::INFORMATION,
        mq_lint::Severity::Style => ls_types::DiagnosticSeverity::HINT,
    }
}

impl From<&LspError> for ls_types::Diagnostic {
    fn from(error: &LspError) -> Self {
        match error {
            LspError::SyntaxError((message, range)) => {
                ls_types::Diagnostic::new_simple(lsp_range(*range), message.to_string())
            }
            LspError::TypeError(type_error) => match type_error.location() {
                Some(range) => ls_types::Diagnostic::new_simple(lsp_range(range), type_error.to_string()),
                None => ls_types::Diagnostic::new_simple(
                    ls_types::Range::new(
                        ls_types::Position { line: 0, character: 0 },
                        ls_types::Position { line: 0, character: 1 },
                    ),
                    type_error.to_string(),
                ),
            },
            LspError::LintWarning(diagnostic) => {
                let range = diagnostic.range.map(lsp_range).unwrap_or_else(|| {
                    ls_types::Range::new(
                        ls_types::Position { line: 0, character: 0 },
                        ls_types::Position { line: 0, character: 1 },
                    )
                });
                let message = match &diagnostic.help {
                    Some(help) => format!("{} (help: {help})", diagnostic.message),
                    None => diagnostic.message.clone(),
                };
                ls_types::Diagnostic::new(
                    range,
                    Some(lint_severity(diagnostic.severity)),
                    Some(NumberOrString::String(diagnostic.rule_id.to_string())),
                    Some("mq-lint".to_string()),
                    message,
                    None,
                    None,
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lint_warning_carries_rule_id_source_and_severity() {
        let diagnostic = mq_lint::Diagnostic::new("unused_variable", mq_lint::Severity::Warn, "unused variable `x`")
            .with_range(mq_lang::Range {
                start: mq_lang::Position { line: 1, column: 5 },
                end: mq_lang::Position { line: 1, column: 6 },
            })
            .with_help("prefix with `_` if intentional");

        let lsp_diagnostic: ls_types::Diagnostic = (&LspError::LintWarning(diagnostic)).into();

        assert_eq!(lsp_diagnostic.severity, Some(ls_types::DiagnosticSeverity::WARNING));
        assert_eq!(
            lsp_diagnostic.code,
            Some(NumberOrString::String("unused_variable".to_string()))
        );
        assert_eq!(lsp_diagnostic.source, Some("mq-lint".to_string()));
        assert!(lsp_diagnostic.message.contains("unused variable `x`"));
        assert!(lsp_diagnostic.message.contains("help: prefix with `_` if intentional"));
        assert_eq!(lsp_diagnostic.range.start.line, 0);
        assert_eq!(lsp_diagnostic.range.start.character, 4);
    }

    #[test]
    fn lint_warning_without_range_falls_back_to_origin() {
        let diagnostic =
            mq_lint::Diagnostic::new("duplicate_match_arm", mq_lint::Severity::Error, "duplicate match arm");

        let lsp_diagnostic: ls_types::Diagnostic = (&LspError::LintWarning(diagnostic)).into();

        assert_eq!(lsp_diagnostic.severity, Some(ls_types::DiagnosticSeverity::ERROR));
        assert_eq!(lsp_diagnostic.range.start.line, 0);
        assert_eq!(lsp_diagnostic.range.start.character, 0);
    }
}
