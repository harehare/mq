//! Process-wide opt-in flags gating capabilities with real-world side effects:
//! `http` (network), `read_file`/`read_file_bytes` (filesystem reads), and `write_file`
//! (filesystem writes).
//!
//! All default to `false`. A host must explicitly enable them via
//! [`set_allow_net`]/[`set_allow_read`]/[`set_allow_write`] (wired to the
//! `--allow-net`/`--allow-read`/`--allow-write` CLI flags in `mq-run`) before the corresponding
//! builtins will run; otherwise they return a runtime error explaining how to opt in. This
//! keeps the model symmetric: a third-party module fetched via HTTP import can't silently read
//! or write local files, or reach the network, without the host opting in to each capability.
//!
//! This is process-wide rather than per-[`Engine`](crate::Engine): like the `file-io` Cargo
//! feature that gates these functions at compile time, filesystem/network access is a
//! deployment-level decision, not something a single process needs to vary per query.

use std::sync::atomic::{AtomicBool, Ordering};

static NET_ALLOWED: AtomicBool = AtomicBool::new(false);
static READ_ALLOWED: AtomicBool = AtomicBool::new(false);
static WRITE_ALLOWED: AtomicBool = AtomicBool::new(false);

/// Enables or disables `http` for the current process.
pub fn set_allow_net(allow: bool) {
    NET_ALLOWED.store(allow, Ordering::Relaxed);
}

/// Enables or disables `read_file`/`read_file_bytes` for the current process.
pub fn set_allow_read(allow: bool) {
    READ_ALLOWED.store(allow, Ordering::Relaxed);
}

/// Enables or disables `write_file` for the current process.
pub fn set_allow_write(allow: bool) {
    WRITE_ALLOWED.store(allow, Ordering::Relaxed);
}

#[cfg(feature = "http")]
pub(crate) fn is_net_allowed() -> bool {
    NET_ALLOWED.load(Ordering::Relaxed)
}

#[cfg(feature = "file-io")]
pub(crate) fn is_read_allowed() -> bool {
    READ_ALLOWED.load(Ordering::Relaxed)
}

#[cfg(feature = "file-io")]
pub(crate) fn is_write_allowed() -> bool {
    WRITE_ALLOWED.load(Ordering::Relaxed)
}
