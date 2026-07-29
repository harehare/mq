//! Abstracts every environment-dependent operation the mq engine needs — file
//! read/write, environment-variable access, network fetch, and external
//! process execution — behind a single trait, so the core evaluator, parser,
//! and module resolvers never call `std::fs`/`std::env`/a network
//! client/`std::process::Command` directly. Concrete implementations are
//! injected per [`Engine`](crate::Engine) instance.
//!
//! See [`NativeIo`] for a real filesystem/environment/network/process-backed
//! implementation, and [`SandboxedIo`] for a decorator that enforces
//! per-instance read/write/net/run permissions. The ambient instance active
//! during an [`Engine::eval`](crate::Engine::eval) call is reachable from
//! builtins via [`crate::eval::builtin::io_context::current`].
//!
//! `LocalFsModuleResolver` (local module resolution) routes through this
//! trait; `HttpModuleResolver`/`UreqFetcher` (HTTP module imports) do not yet
//! — that unification, including relocating `UreqFetcher`'s on-disk
//! cache/lockfile logic into `NativeIo`, is left for a follow-up.

mod error;
#[cfg(any(test, feature = "mock-io"))]
mod mem;
mod native;
mod sandboxed;

pub use error::IoError;
pub use native::NativeIo;
pub use sandboxed::{EnvAccess, NetAccess, PathAccess, SandboxedIo};

/// `pub` here (rather than gated by the `mock-io` feature) would still not leak `MemIo`
/// externally, since the `io` module itself is private to the crate (see `mod io;` in
/// `lib.rs`) — but it's gated anyway so the "unused" build (neither `test` nor `mock-io`)
/// doesn't warn. The crate-root re-export in `lib.rs` is what actually governs external
/// visibility, since [`MemIo`] and the `mock_fetch` builtin are opt-in, testing-focused surface.
#[cfg(any(test, feature = "mock-io"))]
pub use mem::MemIo;

use std::borrow::Cow;
use std::path::{Path, PathBuf};

/// Marker supertrait carrying the `Send + Sync` bound for [`Io`] only when the
/// `sync` feature is enabled, mirroring the `Rc`/`Arc` split of
/// [`crate::Shared`]. WASM/single-threaded embeddings (e.g. `mq-wasm`'s
/// `Rc<RefCell<..>>`-backed fetch cache) cannot satisfy `Send + Sync`, so the
/// bound must not be unconditional.
#[cfg(feature = "sync")]
pub trait IoSyncBound: Send + Sync {}
#[cfg(feature = "sync")]
impl<T: Send + Sync> IoSyncBound for T {}

#[cfg(not(feature = "sync"))]
pub trait IoSyncBound {}
#[cfg(not(feature = "sync"))]
impl<T> IoSyncBound for T {}

/// Abstracts file, environment-variable, and network access for the mq
/// engine. All methods are synchronous, matching the existing sync contract
/// of [`ModuleResolver::resolve`](crate::module::resolver::ModuleResolver::resolve)
/// and the sync evaluator loop. Hosts whose only primitive is async (e.g. a
/// browser `fetch`) are expected to pre-populate a cache asynchronously and
/// answer these calls synchronously from that cache.
///
/// Implementations do not need to enforce permissions themselves unless they
/// choose to (see [`SandboxedIo`] for a decorator that does); `Io` itself is
/// purely a capability abstraction, not a policy.
pub trait Io: std::fmt::Debug + IoSyncBound + 'static {
    fn read_to_string(&self, path: &Path) -> Result<String, IoError>;
    fn read_bytes(&self, path: &Path) -> Result<Vec<u8>, IoError>;
    fn write(&self, path: &Path, content: &[u8]) -> Result<(), IoError>;
    fn exists(&self, path: &Path) -> Result<bool, IoError>;

    /// Size of the file at `path`, in bytes.
    fn file_size(&self, path: &Path) -> Result<u64, IoError>;

    /// `(path, is_dir)` pairs for the immediate entries of a directory.
    fn read_dir(&self, path: &Path) -> Result<Vec<(PathBuf, bool)>, IoError>;

    /// Falls back to the original path unchanged if canonicalization fails.
    fn canonicalize(&self, path: &Path) -> PathBuf;

    fn env_var(&self, name: &str) -> Result<String, IoError>;

    /// Callers are responsible for URL/domain policy (HTTPS-only, allowlisting,
    /// etc.) before calling this — `fetch` is a minimal transport primitive.
    fn fetch(&self, url: &str) -> Result<String, IoError>;

    /// General-purpose HTTP request (arbitrary method/body/headers), backing
    /// the `http()` builtin. `headers` values must already be validated by
    /// the caller; `fetch` above is the narrower primitive module resolution
    /// uses.
    fn http_request(
        &self,
        method: &str,
        url: &str,
        body: Option<&str>,
        headers: &[(String, String)],
    ) -> Result<String, IoError>;

    fn home_dir(&self) -> Option<PathBuf>;
    fn current_dir(&self) -> Option<PathBuf>;

    /// Runs `command` with `args` as a child process — never through a shell, so shell
    /// metacharacters in `args` are never interpreted — and returns its captured stdout as a
    /// string. A non-zero exit status is reported as an error that includes the process's
    /// stderr. Callers are responsible for policy (which commands are permitted) before calling
    /// this, same as [`fetch`](Self::fetch)'s URL/domain policy.
    fn execute(&self, command: &str, args: &[String]) -> Result<String, IoError>;

    /// Seeds the response body a subsequent `fetch`/`http_request` call for `url` returns,
    /// backing the `mock_fetch` builtin. Only meaningful against an `Io` that keeps mock
    /// state (see [`MemIo`]); the default implementation refuses, since there is no sensible
    /// way to "seed" a real filesystem/network-backed `Io`.
    fn set_fetch_response(&self, _url: &str, _body: &str) -> Result<(), IoError> {
        Err(IoError::Other(Cow::Borrowed(
            "set_fetch_response is not supported by this Io implementation",
        )))
    }
}
