use tower_lsp_server::ls_types;
use tower_lsp_server::ls_types::NumberOrString;

#[derive(Debug, Clone)]
/// A diagnostic produced by the language server.
pub enum LspError {
    /// A syntax error.
    SyntaxError(mq_lang::Diagnostic),
    /// A type checking error.
    TypeError(mq_check::TypeError, Vec<mq_check::Related>),
    /// A type error the checker is not sure about, or advice about the code, shown as a warning.
    UncertainTypeError(mq_check::TypeError, Vec<mq_check::Related>),
    /// A lint warning.
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

impl LspError {
    /// The diagnostic for a document at `uri`, with the places that explain it.
    pub fn to_diagnostic(&self, uri: &ls_types::Uri) -> ls_types::Diagnostic {
        let mut diagnostic: ls_types::Diagnostic = self.into();
        if let LspError::TypeError(_, related) | LspError::UncertainTypeError(_, related) = self
            && !related.is_empty()
        {
            diagnostic.related_information = Some(
                related
                    .iter()
                    .map(|related| ls_types::DiagnosticRelatedInformation {
                        location: ls_types::Location::new(uri.clone(), lsp_range(related.range)),
                        message: related.message.clone(),
                    })
                    .collect(),
            );
        }
        diagnostic
    }
}

impl From<&LspError> for ls_types::Diagnostic {
    fn from(error: &LspError) -> Self {
        match error {
            LspError::SyntaxError(diagnostic) => {
                let range = diagnostic.range.map(lsp_range).unwrap_or_else(|| {
                    ls_types::Range::new(
                        ls_types::Position { line: 0, character: 0 },
                        ls_types::Position { line: 0, character: 1 },
                    )
                });
                let message = if diagnostic.hints.is_empty() {
                    diagnostic.message.clone()
                } else {
                    format!("{} (help: {})", diagnostic.message, diagnostic.hints.join("; "))
                };
                ls_types::Diagnostic::new_simple(range, message)
            }
            LspError::TypeError(type_error, _) | LspError::UncertainTypeError(type_error, _) => {
                let range = type_error.location().map(lsp_range).unwrap_or_else(|| {
                    ls_types::Range::new(
                        ls_types::Position { line: 0, character: 0 },
                        ls_types::Position { line: 0, character: 1 },
                    )
                });
                let message = match type_error.hint() {
                    Some(help) => format!("{type_error} (help: {help})"),
                    None => type_error.to_string(),
                };
                let severity = if matches!(error, LspError::UncertainTypeError(..)) {
                    ls_types::DiagnosticSeverity::WARNING
                } else {
                    ls_types::DiagnosticSeverity::ERROR
                };
                let mut diagnostic = ls_types::Diagnostic::new(
                    range,
                    Some(severity),
                    Some(NumberOrString::String(type_error.error_code().to_string())),
                    Some("mq-check".to_string()),
                    message,
                    None,
                    None,
                );
                if matches!(type_error, mq_check::TypeError::UnreachableCode { .. }) {
                    diagnostic.tags = Some(vec![ls_types::DiagnosticTag::UNNECESSARY]);
                }
                diagnostic
            }
            LspError::LintWarning(diagnostic) => {
                let range = diagnostic.range.map(lsp_range).unwrap_or_else(|| {
                    ls_types::Range::new(
                        ls_types::Position { line: 0, character: 0 },
                        ls_types::Position { line: 0, character: 1 },
                    )
                });
                let message = match diagnostic.help() {
                    Some(help) => format!("{} (help: {help})", diagnostic.message()),
                    None => diagnostic.message(),
                };
                ls_types::Diagnostic::new(
                    range,
                    Some(lint_severity(diagnostic.severity)),
                    Some(NumberOrString::String(diagnostic.rule_id().to_string())),
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
        let diagnostic = mq_lint::Diagnostic::new(
            mq_lint::LintMessage::UnusedVariable { name: "x".to_string() },
            mq_lint::Severity::Warn,
        )
        .with_range(mq_lang::Range {
            start: mq_lang::Position { line: 1, column: 5 },
            end: mq_lang::Position { line: 1, column: 6 },
        });

        let lsp_diagnostic: ls_types::Diagnostic = (&LspError::LintWarning(diagnostic)).into();

        assert_eq!(lsp_diagnostic.severity, Some(ls_types::DiagnosticSeverity::WARNING));
        assert_eq!(
            lsp_diagnostic.code,
            Some(NumberOrString::String("unused_variable".to_string()))
        );
        assert_eq!(lsp_diagnostic.source, Some("mq-lint".to_string()));
        assert!(lsp_diagnostic.message.contains("unused variable `x`"));
        assert!(lsp_diagnostic.message.contains("help: if this is intentional"));
        assert_eq!(lsp_diagnostic.range.start.line, 0);
        assert_eq!(lsp_diagnostic.range.start.character, 4);
    }

    #[test]
    fn lint_warning_without_range_falls_back_to_origin() {
        let diagnostic = mq_lint::Diagnostic::new(
            mq_lint::LintMessage::DuplicateMatchArm {
                pattern: "a".to_string(),
            },
            mq_lint::Severity::Error,
        );

        let lsp_diagnostic: ls_types::Diagnostic = (&LspError::LintWarning(diagnostic)).into();

        assert_eq!(lsp_diagnostic.severity, Some(ls_types::DiagnosticSeverity::ERROR));
        assert_eq!(lsp_diagnostic.range.start.line, 0);
        assert_eq!(lsp_diagnostic.range.start.character, 0);
    }
    fn range() -> Option<mq_lang::Range> {
        Some(mq_lang::Range {
            start: mq_lang::Position { line: 2, column: 3 },
            end: mq_lang::Position { line: 2, column: 8 },
        })
    }

    #[test]
    fn type_error_carries_code_source_and_help() {
        let error = mq_check::TypeError::Mismatch {
            expected: "number".to_string(),
            found: "string".to_string(),
            location: range(),
            context: Some("in argument 1 to 'abs'".to_string()),
        };

        let diagnostic: ls_types::Diagnostic = (&LspError::TypeError(error, Vec::new())).into();

        assert_eq!(diagnostic.severity, Some(ls_types::DiagnosticSeverity::ERROR));
        assert_eq!(
            diagnostic.code,
            Some(NumberOrString::String("typechecker::type_mismatch".to_string()))
        );
        assert_eq!(diagnostic.source, Some("mq-check".to_string()));
        assert_eq!(
            diagnostic.message,
            "Type mismatch: expected number, found string (help: in argument 1 to 'abs')"
        );
        assert_eq!(diagnostic.range.start.line, 1);
        assert_eq!(diagnostic.range.start.character, 2);
    }

    #[test]
    fn uncertain_type_error_is_a_warning() {
        let error = mq_check::TypeError::UndefinedField {
            field: "x".to_string(),
            record_ty: "{}".to_string(),
            location: range(),
        };

        let diagnostic: ls_types::Diagnostic = (&LspError::UncertainTypeError(error, Vec::new())).into();

        assert_eq!(diagnostic.severity, Some(ls_types::DiagnosticSeverity::WARNING));
        assert_eq!(diagnostic.message, "Undefined field `x` in record type {}");
    }

    #[test]
    fn unreachable_code_is_tagged_unnecessary() {
        let error = mq_check::TypeError::UnreachableCode {
            reason: "always false".to_string(),
            location: range(),
        };

        let diagnostic: ls_types::Diagnostic = (&LspError::UncertainTypeError(error, Vec::new())).into();

        assert_eq!(diagnostic.tags, Some(vec![ls_types::DiagnosticTag::UNNECESSARY]));
    }

    #[test]
    fn related_places_become_related_information() {
        let error = mq_check::TypeError::UndefinedField {
            field: "x".to_string(),
            record_ty: "{}".to_string(),
            location: range(),
        };
        let related = vec![mq_check::Related {
            range: range().unwrap(),
            message: "`x` is `string`".to_string(),
        }];
        let uri: ls_types::Uri = "file:///a.mq".parse().unwrap();

        let diagnostic = LspError::TypeError(error, related).to_diagnostic(&uri);

        let information = diagnostic.related_information.unwrap();
        assert_eq!(information.len(), 1);
        assert_eq!(information[0].message, "`x` is `string`");
        assert_eq!(information[0].location.uri, uri);
        assert_eq!(information[0].location.range.start.line, 1);
    }
}
