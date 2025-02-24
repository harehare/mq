use std::sync::Arc;

use thiserror::Error;

use crate::Token;

type ErrorToken = Arc<Token>;

#[derive(Error, Debug, PartialEq, Clone)]
pub enum ParseError {
    #[error("Unexpected token `{0}`")]
    UnexpectedToken(ErrorToken),
    #[error("Unexpected EOF detected")]
    UnexpectedEOFDetected,
    #[error("Insufficient tokens `{0}`")]
    InsufficientTokens(ErrorToken),
}
