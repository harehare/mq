//! Evaluating a recursive `def` must not leave allocations behind once the engine is dropped.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicIsize, Ordering};

struct Counting;

static LIVE: AtomicIsize = AtomicIsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        LIVE.fetch_add(1, Ordering::SeqCst);
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE.fetch_sub(1, Ordering::SeqCst);
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// Allocations still live after evaluating `code` on a fresh engine that was then dropped.
fn live_after(code: &str) -> isize {
    let before = LIVE.load(Ordering::SeqCst);
    {
        let mut engine = mq_lang::DefaultEngine::default();
        let _ = engine.eval(code, mq_lang::null_input().into_iter());
    }
    LIVE.load(Ordering::SeqCst) - before
}

#[test]
fn recursive_defs_are_freed_with_the_engine() {
    for code in [
        "in",
        "in([[1, [2]]], [1, [2]])",
        "def f(): fn(): f();; | 1",
        "def f(v): all(v, fn(x): f(x);); | f",
        "def f(n): fn(): f;; | let g = f(1) | g()",
        "def f(n, g = fn(): f;): n; | f(1)",
    ] {
        // The first run fills process-wide caches that stay allocated by design.
        live_after(code);
        assert_eq!(live_after(code), 0, "leaked evaluating {code}");
    }
}
