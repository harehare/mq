use thiserror::Error;

use crate::eval::module::ModuleId;

use super::token::Token;

#[derive(Error, Debug, PartialEq)]
pub enum LexerError {
    #[error("Unexpected token `{0}`")]
    UnexpectedToken(Token),
    #[error("Unexpected EOF detected")]
    UnexpectedEOFDetected(ModuleId),
}
