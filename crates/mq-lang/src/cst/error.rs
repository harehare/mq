use std::sync::Arc;

use thiserror::Error;

use crate::Token;

#[derive(Error, Debug, PartialEq, Clone, PartialOrd, Eq, Ord)]
pub enum ParseError {
    #[error("Unexpected token `{0}`")]
    UnexpectedToken(Arc<Token>),
    #[error("Unexpected EOF detected")]
    UnexpectedEOFDetected,
    #[error("Insufficient tokens `{0}`")]
    InsufficientTokens(Arc<Token>),
}
