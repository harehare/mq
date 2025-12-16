use thiserror::Error;

use crate::Token;
use crate::error::syntax::SyntaxError;
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
    SyntaxError(#[from] SyntaxError),
    #[error("Invalid module, expected IDENT or BINDING")]
    InvalidModule,
}

impl ModuleError {
    #[cold]
    pub fn token(&self) -> Option<&Token> {
        match self {
            ModuleError::AlreadyLoaded(_) => None,
            ModuleError::NotFound(_) => None,
            ModuleError::IOError(_) => None,
            ModuleError::SyntaxError(err) => err.token(),
            ModuleError::InvalidModule => None,
        }
    }
}
