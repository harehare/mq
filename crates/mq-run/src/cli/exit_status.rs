//! Process exit codes and the `--exit-status` outcome of a run.
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

/// Classification of a result for --exit-status. Declaration order is best-last.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub(super) enum Outcome {
    NoResult,
    Falsy,
    Truthy,
}

impl Outcome {
    /// Classifies a value. An empty Markdown value is what `select()` leaves for a non-matching
    /// node, so it counts as no result rather than a falsy one.
    ///
    /// Mirrors jq: false and null are falsy; everything else (including empty string, 0, [], {})
    /// is truthy.
    pub(super) fn of(value: &mq_lang::RuntimeValue) -> Self {
        match value {
            mq_lang::RuntimeValue::Markdown(..) if value.is_empty() => Self::NoResult,
            mq_lang::RuntimeValue::Boolean(false) => Self::Falsy,
            _ if value.is_empty() => Self::Falsy,
            _ => Self::Truthy,
        }
    }

    /// The exit reason for a run whose best result is `self`, or `None` on success.
    pub(super) fn exit_reason(self) -> Option<ExitReason> {
        match self {
            Self::Truthy => None,
            Self::Falsy => Some(ExitReason::Failure),
            Self::NoResult => Some(ExitReason::NoResult),
        }
    }
}

/// The best `Outcome` seen during a run. A plain atomic (rather than a thread_local) is
/// required because batch processing can fan out across rayon worker threads.
pub(super) struct BestOutcome(AtomicU8);

impl BestOutcome {
    const fn new() -> Self {
        Self(AtomicU8::new(Outcome::NoResult as u8))
    }

    pub(super) fn record(&self, outcome: Outcome) {
        self.0.fetch_max(outcome as u8, Ordering::Relaxed);
    }

    pub(super) fn get(&self) -> Outcome {
        match self.0.load(Ordering::Relaxed) {
            r if r == Outcome::Truthy as u8 => Outcome::Truthy,
            r if r == Outcome::Falsy as u8 => Outcome::Falsy,
            _ => Outcome::NoResult,
        }
    }
}

pub(super) static BEST_OUTCOME: BestOutcome = BestOutcome::new();

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
/// `NoResult` and a falsy `Failure` are only produced under `--exit-status`.
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
