use thiserror::Error;

use crate::{Shared, Token, TokenKind, selector};

#[derive(Error, Debug, PartialEq, Clone, PartialOrd, Eq, Ord)]
pub enum ParseError {
    #[error("Unexpected token `{0}`")]
    UnexpectedToken(Shared<Token>),
    #[error("Unexpected EOF detected")]
    UnexpectedEOFDetected,
    #[error("Insufficient tokens `{0}`")]
    InsufficientTokens(Shared<Token>),
    #[error("Expected a closing bracket `]` but got `{0}` delimiter")]
    ExpectedClosingBracket(Shared<Token>),
    #[error(transparent)]
    UnknownSelector(selector::UnknownSelector),
    #[error("Unexpected `end` keyword — no open block to close")]
    UnmatchedEnd(Shared<Token>),
}

impl ParseError {
    /// User-facing hint text for this error, mirroring the `help()` text the AST-level
    /// `SyntaxError` produces for the equivalent case, so CST-based (LSP) and AST-based
    /// (CLI) diagnostics stay consistent.
    #[cold]
    pub fn hint(&self) -> Option<String> {
        match self {
            ParseError::UnknownSelector(sel) => Some(crate::error::selector_help(sel).into_owned()),
            ParseError::UnexpectedToken(token) if token.kind == TokenKind::Eof => Some(
                "The source could not be fully parsed from this position. Check for unsupported escape sequences (use \\u{XXXX} for Unicode), invalid characters, or unterminated string literals.".to_string(),
            ),
            ParseError::UnexpectedToken(_) => Some(
                "This token is not valid here. Check for typos, missing operators, or misplaced punctuation.".to_string(),
            ),
            ParseError::UnexpectedEOFDetected => Some(
                "Unexpected end of input. Check for missing closing brackets, parentheses, or incomplete expressions.".to_string(),
            ),
            ParseError::InsufficientTokens(_) => Some(
                "Parsing could not continue here. Check for missing arguments, operators, or mismatched delimiters.".to_string(),
            ),
            ParseError::ExpectedClosingBracket(_) => Some(
                "Expected a closing bracket ']'. Check your brackets for balance.".to_string(),
            ),
            ParseError::UnmatchedEnd(_) => Some(
                "This `end` keyword does not match any open block. Note: single-line `if` expressions do not require `end`. Check that each `end` closes a `def`, `fn`, `do`, `while`, `loop`, or `foreach` block.".to_string(),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Position, Range, arena::ArenaId};
    use rstest::rstest;

    fn make_token(kind: TokenKind) -> Shared<Token> {
        Shared::new(Token {
            range: Range {
                start: Position { line: 1, column: 1 },
                end: Position { line: 1, column: 1 },
            },
            kind,
            module_id: ArenaId::new(0),
        })
    }

    #[rstest]
    #[case::unexpected_token(ParseError::UnexpectedToken(make_token(TokenKind::Comma)), true)]
    #[case::unexpected_token_eof(ParseError::UnexpectedToken(make_token(TokenKind::Eof)), true)]
    #[case::unexpected_eof_detected(ParseError::UnexpectedEOFDetected, true)]
    #[case::insufficient_tokens(ParseError::InsufficientTokens(make_token(TokenKind::Comma)), true)]
    #[case::expected_closing_bracket(ParseError::ExpectedClosingBracket(make_token(TokenKind::Comma)), true)]
    #[case::unmatched_end(ParseError::UnmatchedEnd(make_token(TokenKind::End)), true)]
    fn test_hint_present(#[case] err: ParseError, #[case] has_hint: bool) {
        assert_eq!(err.hint().is_some(), has_hint);
    }

    #[test]
    fn test_unknown_selector_hint_suggests_similar_selector() {
        let token = Token {
            range: Range {
                start: Position { line: 1, column: 1 },
                end: Position { line: 1, column: 1 },
            },
            kind: TokenKind::Selector(smol_str::SmolStr::new(".hedaing")),
            module_id: ArenaId::new(0),
        };
        let err = ParseError::UnknownSelector(selector::UnknownSelector::new(token));
        assert_eq!(
            err.hint(),
            Some("Unknown selector `.hedaing`. A selector with a similar name exists: `.heading`.".to_string())
        );
    }
}
