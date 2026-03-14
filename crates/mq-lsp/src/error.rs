use tower_lsp_server::ls_types;

pub type SyntaxError = (std::string::String, mq_lang::Range);

#[derive(Debug, Clone)]
pub enum LspError {
    SyntaxError(SyntaxError),
    TypeError(mq_check::TypeError),
}

impl From<&LspError> for ls_types::Diagnostic {
    fn from(error: &LspError) -> Self {
        match error {
            LspError::SyntaxError((message, range)) => ls_types::Diagnostic::new_simple(
                ls_types::Range::new(
                    ls_types::Position {
                        line: range.start.line.saturating_sub(1),
                        character: range.start.column.saturating_sub(1) as u32,
                    },
                    ls_types::Position {
                        line: range.end.line.saturating_sub(1),
                        character: range.end.column.saturating_sub(1) as u32,
                    },
                ),
                message.to_string(),
            ),
            LspError::TypeError(type_error) => match type_error.location() {
                Some(range) => ls_types::Diagnostic::new_simple(
                    ls_types::Range::new(
                        ls_types::Position {
                            line: range.start.line.saturating_sub(1),
                            character: range.start.column.saturating_sub(1) as u32,
                        },
                        ls_types::Position {
                            line: range.end.line.saturating_sub(1),
                            character: range.end.column.saturating_sub(1) as u32,
                        },
                    ),
                    type_error.to_string(),
                ),
                None => ls_types::Diagnostic::new_simple(
                    ls_types::Range::new(
                        ls_types::Position { line: 0, character: 0 },
                        ls_types::Position { line: 0, character: 1 },
                    ),
                    type_error.to_string(),
                ),
            },
        }
    }
}
