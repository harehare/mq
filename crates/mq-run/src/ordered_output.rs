//! Hands out batch inputs in order and releases each one's output in the same
//! order, so parallel workers print exactly what a sequential run would.

use std::sync::{Condvar, Mutex};

struct State {
    next_claim: usize,
    next_write: usize,
    aborted: bool,
    error: Option<miette::Error>,
}

/// Coordinates workers over `len` inputs. At most `window` inputs are in flight
/// past the oldest unwritten one, which bounds the buffered output.
pub(crate) struct OrderedGate {
    len: usize,
    window: usize,
    state: Mutex<State>,
    changed: Condvar,
}

/// Aborts the gate if the holder unwinds, so waiting workers do not hang.
pub(crate) struct AbortOnPanic<'a>(pub(crate) &'a OrderedGate);

impl Drop for AbortOnPanic<'_> {
    fn drop(&mut self) {
        if std::thread::panicking() {
            self.0.abort(None);
        }
    }
}

impl OrderedGate {
    pub(crate) fn new(len: usize, window: usize) -> Self {
        Self {
            len,
            window: window.max(1),
            state: Mutex::new(State {
                next_claim: 0,
                next_write: 0,
                aborted: false,
                error: None,
            }),
            changed: Condvar::new(),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Claims the next input index, waiting while the window is full.
    pub(crate) fn claim(&self) -> Option<usize> {
        let mut state = self.lock();
        loop {
            if state.aborted || state.next_claim >= self.len {
                return None;
            }
            if state.next_claim < state.next_write + self.window {
                state.next_claim += 1;
                return Some(state.next_claim - 1);
            }
            state = self.changed.wait(state).unwrap_or_else(|e| e.into_inner());
        }
    }

    /// Waits for `index`'s turn, then runs `write` and passes the turn on.
    /// Returns without running it if an earlier input failed.
    pub(crate) fn write_in_order(&self, index: usize, write: impl FnOnce() -> miette::Result<()>) {
        let mut state = self.lock();
        while !state.aborted && state.next_write != index {
            state = self.changed.wait(state).unwrap_or_else(|e| e.into_inner());
        }
        if state.aborted {
            return;
        }
        drop(state);

        let result = write();

        let mut state = self.lock();
        match result {
            Ok(()) => state.next_write += 1,
            Err(error) => {
                state.aborted = true;
                state.error.get_or_insert(error);
            }
        }
        self.changed.notify_all();
    }

    /// Stops all workers once earlier outputs are written, keeping the first error.
    pub(crate) fn fail(&self, index: usize, error: miette::Error) {
        self.write_in_order(index, || Err(error));
    }

    /// Stops all workers immediately.
    pub(crate) fn abort(&self, error: Option<miette::Error>) {
        let mut state = self.lock();
        state.aborted = true;
        if let Some(error) = error {
            state.error.get_or_insert(error);
        }
        self.changed.notify_all();
    }

    pub(crate) fn finish(self) -> miette::Result<()> {
        match self.state.into_inner().unwrap_or_else(|e| e.into_inner()).error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn writes_in_input_order_across_threads() {
        let gate = OrderedGate::new(200, 4);
        let out = Mutex::new(Vec::new());
        std::thread::scope(|scope| {
            for _ in 0..8 {
                scope.spawn(|| {
                    while let Some(i) = gate.claim() {
                        if i % 7 == 0 {
                            std::thread::sleep(std::time::Duration::from_millis(1));
                        }
                        gate.write_in_order(i, || {
                            out.lock().unwrap().push(i);
                            Ok(())
                        });
                    }
                });
            }
        });
        assert!(gate.finish().is_ok());
        assert_eq!(out.into_inner().unwrap(), (0..200).collect::<Vec<_>>());
    }

    #[test]
    fn stops_at_first_failure_after_earlier_outputs() {
        let gate = OrderedGate::new(50, 4);
        let out = Mutex::new(Vec::new());
        std::thread::scope(|scope| {
            for _ in 0..4 {
                scope.spawn(|| {
                    while let Some(i) = gate.claim() {
                        if i == 10 {
                            gate.fail(i, miette::miette!("boom"));
                        } else {
                            gate.write_in_order(i, || {
                                out.lock().unwrap().push(i);
                                Ok(())
                            });
                        }
                    }
                });
            }
        });
        assert!(gate.finish().is_err());
        assert_eq!(out.into_inner().unwrap(), (0..10).collect::<Vec<_>>());
    }
}
