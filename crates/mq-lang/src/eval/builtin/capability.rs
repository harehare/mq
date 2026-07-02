//! Process-wide opt-in flags gating capabilities with real-world side effects:
//! `http_get`/`http_post` (network) and `write_file` (filesystem writes).
//!
//! Both default to `false`. A host must explicitly enable them via
//! [`set_allow_net`]/[`set_allow_write`] (wired to the `--allow-net`/`--allow-write` CLI flags
//! in `mq-run`) before the corresponding builtins will run; otherwise they return a runtime
//! error explaining how to opt in.
//!
//! This is process-wide rather than per-[`Engine`](crate::Engine): like the `file-io` Cargo
//! feature that gates `read_file` at compile time, network/write access is a deployment-level
//! decision, not something a single process needs to vary per query.

use std::sync::atomic::{AtomicBool, Ordering};

static NET_ALLOWED: AtomicBool = AtomicBool::new(false);
static WRITE_ALLOWED: AtomicBool = AtomicBool::new(false);

/// Enables or disables `http_get`/`http_post` for the current process.
pub fn set_allow_net(allow: bool) {
    NET_ALLOWED.store(allow, Ordering::Relaxed);
}

/// Enables or disables `write_file` for the current process.
pub fn set_allow_write(allow: bool) {
    WRITE_ALLOWED.store(allow, Ordering::Relaxed);
}

#[cfg(feature = "http-import-ureq")]
pub(crate) fn is_net_allowed() -> bool {
    NET_ALLOWED.load(Ordering::Relaxed)
}

#[cfg(feature = "file-io")]
pub(crate) fn is_write_allowed() -> bool {
    WRITE_ALLOWED.load(Ordering::Relaxed)
}
