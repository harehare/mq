//! Candidates nobody owns any more: those an exiting thread still had registered, and the ones
//! still alive when their engine was dropped (a returned value can outlive its engine, and with
//! `sync` a closure can be dropped on another thread). The next collection takes them.
//!
//! Without `sync` nothing crosses threads, so the pool is per thread.

use super::WeakCell;

#[cfg(feature = "sync")]
mod store {
    use super::WeakCell;
    use std::sync::{
        Mutex, PoisonError,
        atomic::{AtomicBool, Ordering},
    };

    static POOL: Mutex<Vec<WeakCell>> = Mutex::new(Vec::new());
    static NON_EMPTY: AtomicBool = AtomicBool::new(false);

    pub(super) fn extend(cells: Vec<WeakCell>) {
        POOL.lock().unwrap_or_else(PoisonError::into_inner).extend(cells);
        NON_EMPTY.store(true, Ordering::Release);
    }

    pub(super) fn drain() -> Vec<WeakCell> {
        if NON_EMPTY.swap(false, Ordering::Acquire) {
            return std::mem::take(&mut *POOL.lock().unwrap_or_else(PoisonError::into_inner));
        }
        Vec::new()
    }
}

#[cfg(not(feature = "sync"))]
mod store {
    use super::WeakCell;
    use std::cell::RefCell;

    thread_local! {
        static POOL: RefCell<Vec<WeakCell>> = const { RefCell::new(Vec::new()) };
    }

    pub(super) fn extend(cells: Vec<WeakCell>) {
        let _ = POOL.try_with(|pool| pool.borrow_mut().extend(cells));
    }

    pub(super) fn drain() -> Vec<WeakCell> {
        POOL.try_with(|pool| std::mem::take(&mut *pool.borrow_mut()))
            .unwrap_or_default()
    }
}

/// Hands `cells` over to whichever collection runs next.
pub(super) fn push(cells: Vec<WeakCell>) {
    if !cells.is_empty() {
        store::extend(cells);
    }
}

/// Takes everything abandoned so far.
pub(super) fn take() -> Vec<WeakCell> {
    store::drain()
}
