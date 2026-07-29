//! A source-agnostic diagnostic shape shared by the CST parser (used for editor/LSP
//! scenarios that need error recovery) and other error producers. Keeping this separate
//! from the AST-level `Error`/miette machinery lets consumers that don't want a full
//! `miette::Diagnostic` (e.g. `mq-lsp`) render consistent messages and hints without
//! depending on miette's rendering model.

use crate::Range;

/// A single diagnostic: a message, an optional source location, and zero or more
/// hint strings explaining how to fix the issue.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostic {
    pub message: String,
    pub range: Option<Range>,
    pub hints: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diagnostic_holds_message_range_and_hints() {
        let diagnostic = Diagnostic {
            message: "unexpected token".to_string(),
            range: None,
            hints: vec!["check for a missing operator".to_string()],
        };

        assert_eq!(diagnostic.message, "unexpected token");
        assert_eq!(diagnostic.hints.len(), 1);
    }
}
