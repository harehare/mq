use thiserror::Error;

use super::token::Token;

#[derive(Error, Debug, PartialEq)]
pub enum LexerError {
    #[error("Unexpected token `{0}`")]
    UnexpectedToken(Token),
}
