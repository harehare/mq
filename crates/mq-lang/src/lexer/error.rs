use thiserror::Error;

use crate::module::ModuleId;

use super::token::Token;

#[derive(Error, Debug, PartialEq)]
pub enum LexerError {
    #[error("Unexpected token `{0}`")]
    UnexpectedToken(Token),
    #[error("Unexpected EOF detected")]
    UnexpectedEOFDetected(ModuleId),
}

impl LexerError {
    #[cold]
    pub fn token(&self) -> Option<&Token> {
        match self {
            LexerError::UnexpectedToken(token) => Some(token),
            LexerError::UnexpectedEOFDetected(_) => None,
        }
    }
}
