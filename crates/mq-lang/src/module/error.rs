use thiserror::Error;

use crate::ast::error::ParseError;
use crate::lexer::error::LexerError;
use std::borrow::Cow;

#[derive(Debug, PartialEq, Error)]
pub enum ModuleError {
    #[error("Module `{0}` is already loaded")]
    AlreadyLoaded(Cow<'static, str>),
    #[error("Module `{0}` not found")]
    NotFound(Cow<'static, str>),
    #[error("IO error: {0}")]
    IOError(Cow<'static, str>),
    #[error(transparent)]
    LexerError(#[from] LexerError),
    #[error(transparent)]
    ParseError(#[from] ParseError),
    #[error("Invalid module, expected IDENT or BINDING")]
    InvalidModule,
}
