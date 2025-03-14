use compact_str::CompactString;
use thiserror::Error;

use crate::{Token, eval::module::ModuleId};

#[derive(Error, Debug, PartialEq)]
pub enum ParseError {
    #[error("Not found env `{1}`")]
    EnvNotFound(Token, CompactString),
    #[error("Unexpected token `{0}`")]
    UnexpectedToken(Token),
    #[error("Unexpected EOF detected")]
    UnexpectedEOFDetected(ModuleId),
    #[error("Insufficient tokens `{0}`")]
    InsufficientTokens(Token),
}
