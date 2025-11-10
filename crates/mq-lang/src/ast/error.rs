use smol_str::SmolStr;
use thiserror::Error;

use crate::{Token, module::ModuleId};

#[derive(Error, Debug, PartialEq)]
pub enum ParseError {
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
}
