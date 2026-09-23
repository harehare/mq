//! Per-execution frame state: deadline/call-depth tracking and the `Locals`/operand-stack
//! pools that let repeated calls reuse allocations instead of hitting the allocator.
use super::errors::{VmError, VmResult};
use crate::Shared;
use crate::runtime::host::HostFunctions;
use crate::tarn::VmEnv;
use crate::tarn::bytecode::Chunk;
use crate::tarn::value::{Cell, Closure, Locals, StackValue};
use std::collections::VecDeque;
use std::time::Duration;
#[cfg(not(all(target_arch = "wasm32", feature = "wasm")))]
use std::time::Instant;
#[cfg(all(target_arch = "wasm32", feature = "wasm"))]
use web_time::Instant;

#[cfg(feature = "debugger")]
use super::DebugRuntime;
#[cfg(feature = "debugger")]
use crate::ast::node::Node;

/// One call's state, held on the trampoline's frame stack instead of a native Rust call frame.
pub(super) struct Frame {
    pub(super) chunk_index: u16,
    pub(super) chunks: Option<Shared<Vec<Chunk>>>,
    pub(super) locals: Locals,
    pub(super) upvalues: Option<Shared<Vec<Cell>>>,
    /// First operand-stack slot owned by this frame. The operand stack itself is shared by the
    /// whole execution, so entering a call only records this boundary. Checked when pushed.
    pub(super) stack_base: u32,
    /// The verifier bounds chunk length and jump targets to this representation.
    pub(super) ip: u32,
    pub(super) reusable_locals: bool,
    pub(super) on_complete: Continuation,
    /// Number of logical call-stack slots represented by this physical frame.
    ///
    /// A tail call replaces its caller's physical frame, but it still consumes a logical
    /// call-stack slot so `max_call_stack_depth` remains a recursion safeguard.
    pub(super) call_depth_cost: u32,
    #[cfg(feature = "debugger")]
    pub(super) caller_node: Option<Shared<Node>>,
    #[cfg(feature = "debugger")]
    pub(super) pushed_call_stack_entry: bool,
}

impl Frame {
    pub(super) fn new(
        chunk_index: u16,
        chunks: Option<Shared<Vec<Chunk>>>,
        locals: Locals,
        upvalues: Option<Shared<Vec<Cell>>>,
        reusable_locals: bool,
        on_complete: Continuation,
    ) -> Self {
        Self {
            chunk_index,
            chunks,
            locals,
            upvalues,
            stack_base: 0,
            ip: 0,
            reusable_locals,
            on_complete,
            call_depth_cost: 0,
            #[cfg(feature = "debugger")]
            caller_node: None,
            #[cfg(feature = "debugger")]
            pushed_call_stack_entry: false,
        }
    }
}

/// What to do with a frame's outcome, replacing a recursive call's implicit return.
///
/// Only exceptional return paths need a payload. Keeping their discriminant inside the box
/// makes this enum pointer-sized, and ordinary calls still require no allocation.
pub(super) enum Continuation {
    /// Normal call return / `catch` body: push the value on success; propagate on error.
    Push,
    /// A return that needs additional state for exception handling or parameter binding.
    Deferred(Box<DeferredContinuation>),
}

/// State needed only while executing a `try` body or a default-parameter expression.
pub(super) enum DeferredContinuation {
    /// `try` body: success behaves like `Push`. `break`/`continue` rewrites the parent's
    /// accumulator/`ip` directly; any other error spawns a `catch` frame.
    TryBody(TryBody),
    /// Default-parameter value expression: store the result and resume binding on success;
    /// recycle the unbound callee's locals and propagate on error.
    ResumeBindParams(PendingCall),
}

impl Continuation {
    /// Returns whether this continuation can catch an error while unwinding.
    pub(super) fn is_try_body(&self) -> bool {
        matches!(self, Self::Deferred(deferred) if matches!(**deferred, DeferredContinuation::TryBody(_)))
    }
}

pub(super) struct TryBody {
    pub(super) catch_closure: Shared<Closure>,
    pub(super) has_binder: bool,
    pub(super) break_acc_slot: Option<u16>,
    pub(super) break_completed_iteration_slot: Option<u16>,
    pub(super) break_offset: Option<i32>,
    pub(super) continue_offset: Option<i32>,
}

/// A callee whose parameter binding suspended on a default-value expression.
pub(super) struct PendingCall {
    pub(super) callee_locals: Locals,
    pub(super) callee_locals_reusable: bool,
    pub(super) callee_upvalues: Option<Shared<Vec<Cell>>>,
    pub(super) callee_chunk_index: u16,
    pub(super) callee_chunks: Shared<Vec<Chunk>>,
    pub(super) remaining_args: VecDeque<StackValue>,
    pub(super) target_slot: u16,
    pub(super) next_index: usize,
    pub(super) arg_count: usize,
    pub(super) param_count: usize,
}

/// Instructions between deadline checks.
const TIMEOUT_CHECK_INTERVAL: u32 = 1024;

/// Per-execution deadline and call-depth state.
pub(super) struct ExecutionLimits {
    deadline: Option<Instant>,
    timeout: Option<Duration>,
    step: u32,
    call_depth: u32,
    max_call_stack_depth: u32,
    pools: ExecutionPools,
}

/// Reusable non-capturing frame storage.
#[derive(Default)]
pub(crate) struct ExecutionPools {
    local_pool: Vec<Vec<Locals>>,
    pooled_local_slots: usize,
    stack_pool: Vec<Vec<StackValue>>,
    frame_stack: Vec<Frame>,
}

const MAX_POOLED_LOCAL_COUNT: usize = 256;
const MAX_POOLED_LOCAL_SLOTS: usize = 4096;
const MAX_POOLED_STACK_CAPACITY: usize = 4096;
const INITIAL_FRAME_STACK_CAPACITY: usize = 32;
const MAX_POOLED_FRAME_STACK_CAPACITY: usize = 4096;

impl ExecutionLimits {
    pub(super) fn new(timeout: Option<Duration>, max_call_stack_depth: u32, pools: ExecutionPools) -> Self {
        Self {
            deadline: timeout.and_then(|t| Instant::now().checked_add(t)),
            timeout,
            // Check before the first instruction as well as at the regular interval. Without
            // this, a zero or already-expired timeout could still run a short query to
            // completion because it never reached `TIMEOUT_CHECK_INTERVAL` instructions.
            step: TIMEOUT_CHECK_INTERVAL - 1,
            call_depth: 0,
            max_call_stack_depth,
            pools,
        }
    }

    pub(super) fn into_pools(self) -> ExecutionPools {
        self.pools
    }

    #[inline(always)]
    pub(super) fn check(&mut self) -> VmResult<()> {
        let Some(deadline) = self.deadline else {
            return Ok(());
        };
        self.step = self.step.wrapping_add(1);
        if self.step & (TIMEOUT_CHECK_INTERVAL - 1) != 0 {
            return Ok(());
        }
        if Instant::now() >= deadline {
            Err(VmError::Timeout(self.timeout.unwrap_or_default()))
        } else {
            Ok(())
        }
    }

    #[inline(always)]
    pub(super) fn has_deadline(&self) -> bool {
        self.deadline.is_some()
    }

    /// Adds a suspended coroutine's frame depth to the active caller depth.
    ///
    /// Suspended frames are not currently in the trampoline, but they still count toward the
    /// recursion limit while the coroutine is being resumed. Returns the caller depth needed to
    /// restore this execution after the coroutine yields, completes, or fails.
    pub(super) fn enter_suspended_call_depth(&mut self, suspended_depth: u32) -> VmResult<u32> {
        let caller_depth = self.call_depth;
        let Some(combined_depth) = caller_depth.checked_add(suspended_depth) else {
            return Err(VmError::RecursionError(self.max_call_stack_depth));
        };
        if combined_depth > self.max_call_stack_depth {
            return Err(VmError::RecursionError(self.max_call_stack_depth));
        }
        self.call_depth = combined_depth;
        Ok(caller_depth)
    }

    /// Detaches a coroutine's frame depth and restores its caller's active depth.
    pub(super) fn leave_suspended_call_depth(&mut self, caller_depth: u32) -> u32 {
        let suspended_depth = self
            .call_depth
            .checked_sub(caller_depth)
            .expect("coroutine call depth must include its caller depth");
        self.call_depth = caller_depth;
        suspended_depth
    }

    /// Pushes a frame, enforcing `max_call_stack_depth`.
    #[cfg_attr(not(feature = "debugger"), allow(unused_mut))]
    pub(super) fn push_frame(
        &mut self,
        frames: &mut Vec<Frame>,
        mut frame: Frame,
        stack_base: usize,
        #[cfg(feature = "debugger")] debug: &mut DebugRuntime<'_>,
    ) -> VmResult<()> {
        if self.call_depth >= self.max_call_stack_depth {
            return self.reject_frame(frame, VmError::RecursionError(self.max_call_stack_depth));
        }
        frame.stack_base = match u32::try_from(stack_base) {
            Ok(base) => base,
            Err(_) => return self.reject_frame(frame, VmError::OperandStackLimit),
        };
        self.call_depth += 1;
        frame.call_depth_cost = 1;
        #[cfg(feature = "debugger")]
        {
            let caller_node = debug.current_node.clone();
            frame.pushed_call_stack_entry = if let Some(node) = &caller_node {
                debug.call_stack.push(Shared::clone(node));
                true
            } else {
                false
            };
            frame.caller_node = caller_node;
        }
        frames.push(frame);
        Ok(())
    }

    /// Recycles a frame's locals and turns it into `error`. Kept out of line so the
    /// `push_frame` success path stays small; both of its rejection cases are rare.
    #[cold]
    #[inline(never)]
    fn reject_frame(&mut self, frame: Frame, error: VmError) -> VmResult<()> {
        if frame.reusable_locals {
            self.recycle_locals(frame.locals);
        }
        Err(error)
    }

    /// Pops the top frame, recycling its locals, and returns its `Continuation`.
    pub(super) fn pop_frame(
        &mut self,
        frames: &mut Vec<Frame>,
        #[cfg(feature = "debugger")] debug: &mut DebugRuntime<'_>,
    ) -> Option<Continuation> {
        let frame = frames.pop()?;
        self.call_depth = self.call_depth.saturating_sub(frame.call_depth_cost);
        if frame.reusable_locals {
            self.recycle_locals(frame.locals);
        }
        #[cfg(feature = "debugger")]
        {
            if frame.pushed_call_stack_entry {
                debug.call_stack.pop();
            }
            debug.current_node = frame.caller_node;
        }
        Some(frame.on_complete)
    }

    /// Replaces the active frame with its tail-call callee while retaining its allocation.
    ///
    /// The replacement consumes another *logical* call-stack slot even though it reuses the
    /// caller's physical frame. This keeps tail recursion subject to `max_call_stack_depth`.
    pub(super) fn replace_top_frame(
        &mut self,
        frames: &mut Vec<Frame>,
        mut replacement: Frame,
        #[cfg(feature = "debugger")] debug: &mut DebugRuntime<'_>,
    ) -> VmResult<()> {
        if self.call_depth >= self.max_call_stack_depth {
            if replacement.reusable_locals {
                self.recycle_locals(replacement.locals);
            }
            return Err(VmError::RecursionError(self.max_call_stack_depth));
        }
        let frame = frames.pop().expect("tail call requires an active frame");
        replacement.on_complete = frame.on_complete;
        replacement.call_depth_cost = frame
            .call_depth_cost
            .checked_add(1)
            .expect("call depth is bounded by u32::MAX");
        self.call_depth += 1;
        if frame.reusable_locals {
            self.recycle_locals(frame.locals);
        }
        #[cfg(feature = "debugger")]
        {
            if frame.pushed_call_stack_entry {
                debug.call_stack.pop();
            }
            debug.current_node = frame.caller_node;
            let caller_node = debug.current_node.clone();
            replacement.pushed_call_stack_entry = if let Some(node) = &caller_node {
                debug.call_stack.push(Shared::clone(node));
                true
            } else {
                false
            };
            replacement.caller_node = caller_node;
        }
        frames.push(replacement);
        Ok(())
    }

    /// Releases a not-yet-running callee's locals when binding its parameters fails.
    pub(super) fn recycle_pending_locals(&mut self, pending: PendingCall) {
        if pending.callee_locals_reusable {
            self.recycle_locals(pending.callee_locals);
        }
    }

    pub(super) fn take_locals(&mut self, count: u16, captured_slots: &[u16]) -> Locals {
        self.take_locals_with_initialized_prefix(count, 0, captured_slots)
    }

    pub(super) fn take_locals_with_initialized_prefix(
        &mut self,
        count: u16,
        initialized: usize,
        captured_slots: &[u16],
    ) -> Locals {
        let locals = if !captured_slots.is_empty() {
            None
        } else {
            self.pools
                .local_pool
                .get_mut(count as usize)
                .and_then(|bucket| bucket.pop())
        };
        if locals.is_some() {
            self.pools.pooled_local_slots = self.pools.pooled_local_slots.saturating_sub(count as usize);
        }
        let mut locals = locals.unwrap_or_else(|| fresh_locals(count as usize, captured_slots));
        locals.reset_from(initialized.min(count as usize));
        locals
    }

    pub(super) fn recycle_locals(&mut self, locals: Locals) {
        const MAX_RETAINED_PER_LENGTH: usize = 8;
        let count = locals.len();
        if count >= MAX_POOLED_LOCAL_COUNT {
            return;
        }
        if count >= self.pools.local_pool.len() {
            self.pools.local_pool.resize_with(count + 1, Vec::new);
        }
        let bucket = &mut self.pools.local_pool[count];
        if bucket.len() < MAX_RETAINED_PER_LENGTH
            && self.pools.pooled_local_slots.saturating_add(count) <= MAX_POOLED_LOCAL_SLOTS
        {
            bucket.push(locals);
            self.pools.pooled_local_slots += count;
        }
    }

    pub(super) fn take_stack(&mut self) -> Vec<StackValue> {
        self.pools.stack_pool.pop().unwrap_or_else(|| Vec::with_capacity(8))
    }

    pub(super) fn recycle_stack(&mut self, mut stack: Vec<StackValue>) {
        const MAX_RETAINED_FRAMES: usize = 32;
        stack.clear();
        if stack.capacity() <= MAX_POOLED_STACK_CAPACITY && self.pools.stack_pool.len() < MAX_RETAINED_FRAMES {
            self.pools.stack_pool.push(stack);
        }
    }

    /// Takes the reusable trampoline frame stack for an evaluation.
    pub(super) fn take_frame_stack(&mut self) -> Vec<Frame> {
        let mut frames = std::mem::take(&mut self.pools.frame_stack);
        if frames.capacity() == 0 {
            frames.reserve(INITIAL_FRAME_STACK_CAPACITY);
        }
        frames
    }

    /// Retains an empty trampoline frame stack for the next non-overlapping evaluation.
    pub(super) fn recycle_frame_stack(&mut self, mut frames: Vec<Frame>) {
        debug_assert!(frames.is_empty(), "all VM frames must be popped before recycling");
        frames.clear();
        if frames.capacity() <= MAX_POOLED_FRAME_STACK_CAPACITY {
            self.pools.frame_stack = frames;
        }
    }
}

#[cfg(test)]
impl ExecutionPools {
    /// Returns the number of reusable local frames retained by this pool.
    pub(crate) fn pooled_local_frame_count(&self) -> usize {
        self.local_pool.iter().map(Vec::len).sum()
    }
}

fn fresh_locals(count: usize, captured_slots: &[u16]) -> Locals {
    if captured_slots.is_empty() {
        Locals::flat(count)
    } else {
        Locals::for_captured_slots(count, captured_slots)
    }
}

/// Mutable services shared by all frames of one VM evaluation.
pub(super) struct ExecutionContext<'a> {
    pub(super) env: &'a VmEnv,
    /// Arena the currently active chunk pool resolves tokens against; `coroutine::resume`
    /// overrides this to the resumed coroutine's own arena while driving its frames.
    pub(super) token_arena: crate::TokenArena,
    pub(super) limits: &'a mut ExecutionLimits,
    pub(super) host_functions: &'a HostFunctions,
    /// Read only on the uncaught-error path by `unwind_frames`.
    pub(super) capture_stack_trace: bool,
}

#[cfg(test)]
mod size_tests {
    use super::*;

    #[test]
    fn locals_stays_within_one_slice_plus_tag() {
        assert!(
            std::mem::size_of::<Locals>() <= 3 * std::mem::size_of::<usize>(),
            "Locals grew to {} bytes; box rare variants instead of widening it",
            std::mem::size_of::<Locals>()
        );
    }

    #[test]
    fn continuation_stays_pointer_sized() {
        assert_eq!(std::mem::size_of::<Continuation>(), std::mem::size_of::<usize>());
    }

    #[test]
    fn oversized_timeout_does_not_panic_when_creating_execution_limits() {
        let mut limits = ExecutionLimits::new(Some(Duration::MAX), 10, ExecutionPools::default());
        assert!(limits.check().is_ok());
    }

    #[cfg(not(feature = "debugger"))]
    #[test]
    fn frame_stays_compact() {
        assert!(
            std::mem::size_of::<Frame>() <= 64,
            "Frame grew to {} bytes; every call pushes and pops one",
            std::mem::size_of::<Frame>()
        );
    }

    #[cfg(all(target_pointer_width = "64", not(feature = "debugger")))]
    #[test]
    fn oversized_operand_stack_rejects_frame_and_recycles_locals() {
        let mut limits = ExecutionLimits::new(None, 10, ExecutionPools::default());
        let mut frames = Vec::new();
        let frame = Frame::new(0, None, Locals::flat(1), None, true, Continuation::Push);

        assert!(matches!(
            limits.push_frame(&mut frames, frame, u32::MAX as usize + 1),
            Err(VmError::OperandStackLimit)
        ));
        assert!(frames.is_empty());
        assert_eq!(limits.call_depth, 0);
        assert_eq!(limits.pools.pooled_local_frame_count(), 1);
    }
}
