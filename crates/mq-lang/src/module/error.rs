use thiserror::Error;

use crate::Token;
use crate::error::syntax::SyntaxError;
use std::borrow::Cow;

/// Errors that can occur while loading or resolving mq modules.
#[derive(Debug, PartialEq, Error)]
#[non_exhaustive]
pub enum ModuleError {
    /// The module was already loaded.
    #[error("Module `{0}` is already loaded")]
    AlreadyLoaded(Cow<'static, str>),
    /// The module could not be found.
    #[error("Module `{0}` not found")]
    NotFound(Cow<'static, str>),
    /// Reading the module failed.
    #[error("IO error: {0}")]
    IOError(Cow<'static, str>),
    /// The module has a syntax error.
    #[error(transparent)]
    SyntaxError(#[from] SyntaxError),
    /// The module imports itself, directly or through other modules.
    #[error("Circular module dependency: {chain}")]
    CircularDependency {
        /// The modules in the cycle, e.g. `a -> b -> a`.
        chain: Cow<'static, str>,
        /// The directive that closes the cycle.
        token: Token,
    },
    /// The module declaration is malformed.
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
    /// Returns the token the error points at, if any.
    #[cold]
    pub fn token(&self) -> Option<&Token> {
        match self {
            ModuleError::AlreadyLoaded(_) => None,
            ModuleError::NotFound(_) => None,
            ModuleError::IOError(_) => None,
            ModuleError::SyntaxError(err) => err.token(),
            ModuleError::InvalidModule => None,
            ModuleError::CircularDependency { token, .. } => Some(token),
            #[cfg(feature = "http-import")]
            ModuleError::HttpImportNotAllowed(_) => None,
        }
    }
}
