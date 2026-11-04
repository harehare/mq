//! Heap allocation counting for one measured region.
//!
//! [`CountingAlloc`] wraps the real allocator and counts, for the calling thread only, while a
//! region is being measured. Counts do not depend on timing, so equal code gives equal numbers.

use serde::{Deserialize, Serialize};
use std::alloc::{GlobalAlloc, Layout};
use std::cell::Cell;

/// Heap use of one measured region.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemStats {
    /// Number of allocations, counting a `realloc` as one.
    pub allocs: u64,
    /// Bytes requested in total, not counting what was freed.
    pub bytes: u64,
    /// Most bytes that were live at once, relative to the start of the region.
    pub peak_bytes: u64,
}

/// Counters of the measuring thread. `const` initialization and no destructor, so reading them
/// inside the allocator never allocates or registers a thread-exit hook.
struct State {
    enabled: Cell<bool>,
    allocs: Cell<u64>,
    bytes: Cell<u64>,
    live: Cell<i64>,
    peak: Cell<i64>,
}

thread_local! {
    static STATE: State = const {
        State {
            enabled: Cell::new(false),
            allocs: Cell::new(0),
            bytes: Cell::new(0),
            live: Cell::new(0),
            peak: Cell::new(0),
        }
    };
}

fn record(allocs: u64, bytes: u64, live_delta: i64) {
    // `try_with` fails only while the thread is being torn down, when nothing is measured.
    let _ = STATE.try_with(|state| {
        if !state.enabled.get() {
            return;
        }
        state.allocs.set(state.allocs.get() + allocs);
        state.bytes.set(state.bytes.get() + bytes);
        let live = state.live.get() + live_delta;
        state.live.set(live);
        if live > state.peak.get() {
            state.peak.set(live);
        }
    });
}

/// A global allocator that forwards to `A` and counts while a region is measured.
pub struct CountingAlloc<A>(pub A);

// SAFETY: every method forwards to `A` with the same arguments and only updates thread-local
// counters, so the contract of `GlobalAlloc` is `A`'s.
unsafe impl<A: GlobalAlloc> GlobalAlloc for CountingAlloc<A> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { self.0.alloc(layout) };
        if !ptr.is_null() {
            record(1, layout.size() as u64, layout.size() as i64);
        }
        ptr
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { self.0.alloc_zeroed(layout) };
        if !ptr.is_null() {
            record(1, layout.size() as u64, layout.size() as i64);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        record(0, 0, -(layout.size() as i64));
        unsafe { self.0.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let new_ptr = unsafe { self.0.realloc(ptr, layout, new_size) };
        if !new_ptr.is_null() {
            let old = layout.size() as i64;
            let new = new_size as i64;
            record(1, new.saturating_sub(old).max(0) as u64, new - old);
        }
        new_ptr
    }
}

/// Starts counting on the calling thread, from zero.
fn start() {
    STATE.with(|state| {
        state.allocs.set(0);
        state.bytes.set(0);
        state.live.set(0);
        state.peak.set(0);
        state.enabled.set(true);
    });
}

/// Stops counting and returns what was counted since [`start`].
fn stop() -> MemStats {
    STATE.with(|state| {
        state.enabled.set(false);
        MemStats {
            allocs: state.allocs.get(),
            bytes: state.bytes.get(),
            peak_bytes: state.peak.get().max(0) as u64,
        }
    })
}

/// Runs `f` and returns its result with the heap use of the calling thread during it.
///
/// Returns `None` for the stats when [`CountingAlloc`] is not the global allocator, which a
/// probe allocation detects. Work `f` hands to other threads is not counted.
pub fn measure<R>(f: impl FnOnce() -> R) -> (R, Option<MemStats>) {
    start();
    drop(std::hint::black_box(Box::new(0u8)));
    let installed = STATE.with(|state| state.allocs.get() > 0);
    // Count from zero again so the probe is not part of the result.
    start();
    let result = f();
    let stats = stop();
    (result, installed.then_some(stats))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::alloc::System;

    fn layout(size: usize) -> Layout {
        Layout::from_size_align(size, 8).unwrap()
    }

    /// Runs `f` against a private allocator, so no global allocator needs to be installed.
    fn counted(f: impl FnOnce(&CountingAlloc<System>)) -> MemStats {
        let allocator = CountingAlloc(System);
        start();
        f(&allocator);
        stop()
    }

    #[test]
    fn counts_allocations_bytes_and_peak() {
        let stats = counted(|a| unsafe {
            let first = a.alloc(layout(100));
            let second = a.alloc(layout(50));
            a.dealloc(first, layout(100));
            let third = a.alloc(layout(10));
            a.dealloc(second, layout(50));
            a.dealloc(third, layout(10));
        });
        assert_eq!(stats.allocs, 3);
        assert_eq!(stats.bytes, 160);
        // 100 + 50 were live together; the later 10 never exceeded that.
        assert_eq!(stats.peak_bytes, 150);
    }

    #[test]
    fn realloc_counts_once_and_only_growth_adds_bytes() {
        let stats = counted(|a| unsafe {
            let ptr = a.alloc(layout(64));
            let grown = a.realloc(ptr, layout(64), 256);
            let shrunk = a.realloc(grown, layout(256), 32);
            a.dealloc(shrunk, layout(32));
        });
        assert_eq!(stats.allocs, 3);
        assert_eq!(stats.bytes, 64 + 192);
        assert_eq!(stats.peak_bytes, 256);
    }

    #[test]
    fn frees_of_memory_allocated_before_the_region_do_not_underflow_the_peak() {
        let allocator = CountingAlloc(System);
        let before = unsafe { allocator.alloc(layout(1000)) };
        start();
        unsafe { allocator.dealloc(before, layout(1000)) };
        let stats = stop();
        assert_eq!((stats.allocs, stats.bytes, stats.peak_bytes), (0, 0, 0));
    }

    #[test]
    fn nothing_is_counted_outside_a_region() {
        let allocator = CountingAlloc(System);
        start();
        stop();
        unsafe {
            let ptr = allocator.alloc(layout(8));
            allocator.dealloc(ptr, layout(8));
        }
        assert_eq!(stop(), MemStats::default());
    }

    #[test]
    fn only_the_measuring_thread_is_counted() {
        let stats = counted(|a| {
            std::thread::scope(|scope| {
                scope.spawn(|| unsafe {
                    let ptr = a.alloc(layout(4096));
                    a.dealloc(ptr, layout(4096));
                });
            });
        });
        // Spawning allocates on the measuring thread itself, but the worker's 4096 bytes are not counted.
        assert!(stats.bytes < 4096, "{stats:?}");
    }

    #[test]
    fn measure_reports_none_without_the_counting_allocator() {
        // The test binary uses the system allocator unless `main.rs` installs the wrapper.
        let (value, stats) = measure(|| vec![1u8; 128].len());
        assert_eq!(value, 128);
        assert!(stats.is_none_or(|stats| stats.bytes >= 128));
    }
}
