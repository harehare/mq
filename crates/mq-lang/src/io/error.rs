use std::borrow::Cow;
use thiserror::Error;

/// Error returned by [`Io`](super::Io) operations. Internal plumbing type —
/// call sites are expected to convert it into whatever domain error type they
/// already use (e.g. `ModuleError` for module resolution, the builtin
/// evaluator's own error type) rather than surface it to `miette` directly.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum IoError {
    #[error("{0} not found")]
    NotFound(Cow<'static, str>),
    /// The operation was refused by the concrete `Io` implementation (e.g.
    /// [`SandboxedIo`](super::SandboxedIo) with the matching capability
    /// disabled).
    #[error("{0}")]
    PermissionDenied(Cow<'static, str>),
    #[error("{0}")]
    Other(Cow<'static, str>),
}
