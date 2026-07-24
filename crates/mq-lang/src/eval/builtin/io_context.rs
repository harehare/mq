//! Ambient access to the [`Io`] instance active for the current evaluation.
//!
//! Builtins are plain `fn` pointers with no `self` (see [`BuiltinFunction`](super::BuiltinFunction)),
//! so they can't hold a reference to the [`Evaluator`](crate::eval::Evaluator)'s `Io`. Instead,
//! `Evaluator::eval` installs it here for the duration of the call via [`scoped`], and builtins
//! read it back via [`current`].

use std::cell::RefCell;

use crate::Shared;
use crate::io::{Io, NativeIo, SandboxedIo};

thread_local! {
    static CURRENT: RefCell<Option<Shared<dyn Io>>> = const { RefCell::new(None) };
    /// All-denied, so a builtin called outside `Evaluator::eval` (e.g. directly in a unit test)
    /// fails safe rather than silently getting full host access.
    static DEFAULT_IO: Shared<dyn Io> = Shared::new(SandboxedIo::new(NativeIo::default()));
}

#[must_use]
pub(crate) struct ScopedIo(Option<Shared<dyn Io>>);

impl Drop for ScopedIo {
    fn drop(&mut self) {
        CURRENT.with(|current| *current.borrow_mut() = self.0.take());
    }
}

/// Installs `io` as the ambient instance until the returned guard is dropped, restoring
/// whatever was active before (supports re-entrant `eval` calls).
pub(crate) fn scoped(io: Shared<dyn Io>) -> ScopedIo {
    let previous = CURRENT.with(|current| current.borrow_mut().replace(io));
    ScopedIo(previous)
}

#[cfg_attr(not(any(feature = "file-io", feature = "http")), allow(dead_code))]
pub(crate) fn current() -> Shared<dyn Io> {
    CURRENT
        .with(|current| current.borrow().clone())
        .unwrap_or_else(|| DEFAULT_IO.with(Shared::clone))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::MemIo;

    #[test]
    fn test_current_falls_back_to_denied_default_when_unscoped() {
        assert!(current().read_to_string(std::path::Path::new("/a.txt")).is_err());
    }

    #[test]
    fn test_scoped_installs_and_restores() {
        let outer = current();
        {
            let _guard = scoped(Shared::new(MemIo::default().with_file("/a.txt", "content")));
            assert_eq!(
                current().read_to_string(std::path::Path::new("/a.txt")).unwrap(),
                "content"
            );
        }
        // Restored to whatever was active before (the denied default here).
        assert!(current().read_to_string(std::path::Path::new("/a.txt")).is_err());
        let _ = outer;
    }

    #[test]
    fn test_nested_scopes_restore_correctly() {
        let _outer = scoped(Shared::new(MemIo::default().with_file("/outer.txt", "outer")));
        {
            let _inner = scoped(Shared::new(MemIo::default().with_file("/inner.txt", "inner")));
            assert!(current().read_to_string(std::path::Path::new("/outer.txt")).is_err());
            assert_eq!(
                current().read_to_string(std::path::Path::new("/inner.txt")).unwrap(),
                "inner"
            );
        }
        assert_eq!(
            current().read_to_string(std::path::Path::new("/outer.txt")).unwrap(),
            "outer"
        );
    }
}
