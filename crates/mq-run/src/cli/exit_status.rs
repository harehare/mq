//! Process exit codes and the `--exit-status` outcome of a run.
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

/// Whether `value` counts as a result for --exit-status. `None`, empty strings and collections,
/// and the empty node `select()` leaves for a non-matching node are not results.
pub(super) fn is_result(value: &mq_lang::RuntimeValue) -> bool {
    !value.is_empty()
}

/// Whether any run produced a result. A plain atomic (rather than a thread_local) is required
/// because batch processing can fan out across rayon worker threads.
pub(super) struct HadResult(AtomicBool);

impl HadResult {
    const fn new() -> Self {
        Self(AtomicBool::new(false))
    }

    pub(super) fn record(&self, values: &[mq_lang::RuntimeValue]) {
        if values.iter().any(is_result) {
            self.0.store(true, Ordering::Relaxed);
        }
    }

    pub(super) fn get(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

pub(super) static HAD_RESULT: HadResult = HadResult::new();

// Tracks whether --diff found any changed input, for the exit-code-1 CI check.
static HAD_DIFF: AtomicBool = AtomicBool::new(false);

/// Marks that --diff found a changed input.
pub(super) fn mark_diff() {
    HAD_DIFF.store(true, Ordering::Relaxed);
}

/// Whether --diff found any changed input.
pub(super) fn had_diff() -> bool {
    HAD_DIFF.load(Ordering::Relaxed)
}

/// Context for an input or query file that could not be read.
#[derive(Debug)]
pub(super) struct ReadFailure(pub(super) PathBuf);

impl std::fmt::Display for ReadFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "failed to read `{}`", self.0.display())
    }
}

/// Process exit codes, following jq. `Failure` is also the `--diff` "changes found" code.
/// `NoResult` is only produced under `--exit-status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub(super) enum ExitReason {
    Failure = 1,
    IoError = 2,
    SyntaxError = 3,
    NoResult = 4,
    RuntimeError = 5,
}

impl ExitReason {
    pub(super) const fn code(self) -> i32 {
        self as i32
    }
}
