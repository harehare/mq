use thiserror::Error;

use crate::Token;
use crate::error::syntax::SyntaxError;
use std::borrow::Cow;

/// Errors that can occur while loading or resolving mq modules.
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
    /// HTTP imports are only permitted at the top level; modules may not fetch remote dependencies.
    #[cfg(feature = "http-import")]
    #[error(
        "HTTP import of `{0}` is not allowed inside an imported module; HTTP imports are only permitted at the top level"
    )]
    HttpImportNotAllowed(Cow<'static, str>),
}

impl ModuleError {
    /// Returns the token associated with a syntax error, if any.
    #[cold]
    pub fn token(&self) -> Option<&Token> {
        match self {
            ModuleError::AlreadyLoaded(_) => None,
            ModuleError::NotFound(_) => None,
            ModuleError::IOError(_) => None,
            ModuleError::SyntaxError(err) => err.token(),
            ModuleError::InvalidModule => None,
            #[cfg(feature = "http-import")]
            ModuleError::HttpImportNotAllowed(_) => None,
        }
    }
}
