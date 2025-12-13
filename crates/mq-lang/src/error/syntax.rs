use smol_str::SmolStr;
use thiserror::Error;

use crate::{Token, module::ModuleId, selector};

#[derive(Error, Debug, PartialEq)]
pub enum SyntaxError {
    #[error("Not found env `{1}`")]
    EnvNotFound(Token, SmolStr),
    #[error("Unexpected token `{}`", if .0.is_eof() { "EOF".to_string() } else { .0.to_string() })]
    UnexpectedToken(Token),
    #[error("Unexpected EOF detected")]
    UnexpectedEOFDetected(ModuleId),
    #[error("Insufficient tokens `{}`", if .0.is_eof() { "EOF".to_string() } else { .0.to_string() })]
    InsufficientTokens(Token),
    #[error("Expected a closing parenthesis `)` but got `{}` delimiter", if .0.is_eof() { "EOF".to_string() } else { .0.to_string() })]
    ExpectedClosingParen(Token),
    #[error("Expected a closing brace `}}` but got `{}` delimiter", if .0.is_eof() { "EOF".to_string() } else { .0.to_string() })]
    ExpectedClosingBrace(Token),
    #[error("Expected a closing bracket `]` but got `{}` delimiter", if .0.is_eof() { "EOF".to_string() } else { .0.to_string() })]
    ExpectedClosingBracket(Token),
    #[error("Invalid assignment target: expected an identifier but got `{}`", if .0.is_eof() { "EOF".to_string() } else { .0.to_string() })]
    InvalidAssignmentTarget(Token),
    #[error(transparent)]
    UnknownSelector(selector::UnknownSelector),
}

impl SyntaxError {
    #[cold]
    pub fn token(&self) -> Option<&Token> {
        match self {
            SyntaxError::EnvNotFound(token, _) => Some(token),
            SyntaxError::UnexpectedToken(token) => Some(token),
            SyntaxError::UnexpectedEOFDetected(_) => None,
            SyntaxError::InsufficientTokens(token) => Some(token),
            SyntaxError::ExpectedClosingParen(token) => Some(token),
            SyntaxError::ExpectedClosingBrace(token) => Some(token),
            SyntaxError::ExpectedClosingBracket(token) => Some(token),
            SyntaxError::InvalidAssignmentTarget(token) => Some(token),
            SyntaxError::UnknownSelector(selector::UnknownSelector(token)) => Some(token),
        }
    }
}
