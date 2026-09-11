//! Execution-count profiling for Tarn bytecode.
//!
//! This module is available only with the `vm-profile` feature. It counts dispatched
//! instructions, rather than measuring elapsed time, because the act of profiling changes
//! dispatch performance. Use it to identify candidates for bytecode specialization, then use
//! the normal release benchmarks to measure a change.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fmt;

thread_local! {
    static ACTIVE_PROFILE: RefCell<Option<VmProfile>> = const { RefCell::new(None) };
}

/// A count of non-debug Tarn bytecode instructions dispatched during one profiled scope.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VmProfile {
    instruction_count: u64,
    opcode_counts: BTreeMap<&'static str, u64>,
}

impl VmProfile {
    /// Returns the total number of non-debug bytecode instructions dispatched.
    pub fn instruction_count(&self) -> u64 {
        self.instruction_count
    }

    /// Returns the number of executions of `opcode`.
    pub fn opcode_count(&self, opcode: &str) -> u64 {
        self.opcode_counts.get(opcode).copied().unwrap_or_default()
    }

    /// Returns executed opcodes ordered by descending count, then opcode name.
    pub fn most_executed(&self) -> Vec<(&'static str, u64)> {
        let mut opcodes: Vec<_> = self
            .opcode_counts
            .iter()
            .map(|(&opcode, &count)| (opcode, count))
            .collect();
        opcodes.sort_unstable_by(|(left_name, left_count), (right_name, right_count)| {
            right_count.cmp(left_count).then_with(|| left_name.cmp(right_name))
        });
        opcodes
    }

    fn record(&mut self, opcode: &'static str) {
        self.instruction_count += 1;
        *self.opcode_counts.entry(opcode).or_default() += 1;
    }
}

impl fmt::Display for VmProfile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "instructions: {}", self.instruction_count)?;
        for (opcode, count) in self.most_executed() {
            let percentage = if self.instruction_count == 0 {
                0.0
            } else {
                count as f64 / self.instruction_count as f64 * 100.0
            };
            writeln!(f, "  {opcode:<30} {count:>12} ({percentage:>5.1}%)")?;
        }
        Ok(())
    }
}

/// Activates instruction counting for the current thread until [`Self::finish`] is called.
///
/// Scopes may be nested. An inner scope temporarily replaces its parent's counters and restores
/// them when it finishes or is dropped.
pub struct VmProfileScope {
    previous: Option<VmProfile>,
    finished: bool,
}

impl VmProfileScope {
    /// Starts a new instruction-count profile on the current thread.
    pub fn start() -> Self {
        let previous = ACTIVE_PROFILE.with(|profile| profile.replace(Some(VmProfile::default())));
        Self {
            previous,
            finished: false,
        }
    }

    /// Stops this scope, restores a possible parent scope, and returns its counters.
    pub fn finish(mut self) -> VmProfile {
        let profile = ACTIVE_PROFILE.with(|active| active.replace(self.previous.take()).unwrap_or_default());
        self.finished = true;
        profile
    }
}

impl Drop for VmProfileScope {
    fn drop(&mut self) {
        if !self.finished {
            ACTIVE_PROFILE.with(|active| {
                active.replace(self.previous.take());
            });
        }
    }
}

/// Records one dispatched instruction when a profiling scope is active.
#[inline(always)]
pub(crate) fn record_opcode(opcode: &'static str) {
    ACTIVE_PROFILE.with(|profile| {
        if let Some(profile) = profile.borrow_mut().as_mut() {
            profile.record(opcode);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_scope_counts_and_sorts_opcodes() {
        let scope = VmProfileScope::start();
        record_opcode("Const");
        record_opcode("Add");
        record_opcode("Const");

        let profile = scope.finish();
        assert_eq!(profile.instruction_count(), 3);
        assert_eq!(profile.opcode_count("Const"), 2);
        assert_eq!(profile.most_executed(), vec![("Const", 2), ("Add", 1)]);
    }

    #[test]
    fn nested_scope_restores_its_parent() {
        let outer = VmProfileScope::start();
        record_opcode("Outer");
        let inner = VmProfileScope::start();
        record_opcode("Inner");
        assert_eq!(inner.finish().opcode_count("Inner"), 1);
        record_opcode("Outer");
        assert_eq!(outer.finish().opcode_count("Outer"), 2);
    }

    #[test]
    fn engine_evaluation_records_dispatched_bytecode() {
        use crate::{DefaultEngine, RuntimeValue};

        let mut engine = DefaultEngine::default();
        let scope = VmProfileScope::start();
        engine
            .eval(
                "var i = 3 | while(i > 0): i -= 1; | i",
                std::iter::once(RuntimeValue::None),
            )
            .unwrap();

        let profile = scope.finish();
        assert!(profile.instruction_count() > 0);
        assert!(profile.opcode_count("GetLocal") > 0);
        #[cfg(feature = "debugger")]
        assert!(profile.opcode_count("Return") > 0);
        #[cfg(not(feature = "debugger"))]
        assert!(profile.opcode_count("ReturnLocal") > 0);
    }
}
