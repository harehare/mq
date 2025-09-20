use thiserror::Error;

use crate::{Shared, Token};

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
}
