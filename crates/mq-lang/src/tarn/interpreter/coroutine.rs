//! Suspended generator/coroutine state: the `Vec<Frame>` + operand stack a `yield` detaches
//! from the trampoline instead of letting it recycle them, resumed by `OpCode::Resume`.
//!
//! Holds no borrows of `VmEnv`/`HostFunctions`/`ExecutionContext`. `resume` takes those fresh
//! from the caller, so a coroutine can be resumed by an unrelated later evaluation. It does keep
//! its own owned `TokenArena` (see `CoroutineState::token_arena`), since that evaluation's arena
//! is the wrong one to blame a failure on.
#[cfg(feature = "debugger")]
use super::DebugRuntime;
use super::errors::{VmError, VmResult};
use super::frame::{ExecutionContext, Frame};
use super::{DriveOutcome, into_runtime_value};
#[cfg(feature = "debugger")]
use crate::ast::node::Node;
use crate::runtime::runtime_value::{DictMap, RuntimeValue};
use crate::tarn::bytecode::Chunk;
use crate::tarn::value::{
    Cell, StackValue, WeakCell, collect_coroutine_handles_in_cell, collect_coroutine_handles_in_stack_value,
    is_weak_self_reference, read_cell, sanitize_nested_self_reference, weak_coroutine_cell, write_cell,
};
use crate::{Ident, Shared, SharedCell, TokenArena};

/// A coroutine's lifecycle.
pub(crate) enum CoroutineStatus {
    /// Bound but never resumed.
    Created,
    /// Suspended at a `yield`; resuming restarts right after it.
    Suspended,
    /// Being driven by a `next()` higher on the Rust call stack; re-entering is an error.
    Running,
    /// Ran to completion; every later `next()` returns `{ value: None, done: true }`.
    Completed,
    /// Raised an error; every later `next()` re-raises it.
    Failed(Shared<VmError>),
}

pub(crate) struct CoroutineState {
    pub(super) status: CoroutineStatus,
    pub(super) frames: Vec<Frame>,
    pub(super) operand_stack: Vec<StackValue>,
    pub(super) chunks: Shared<Vec<Chunk>>,
    /// The arena `chunks`' token IDs resolve against, from whichever evaluation created this
    /// coroutine. Kept so a failure is diagnosed against its own source even when a later
    /// `next()`/`send()` resumes it under a different evaluation's arena.
    pub(super) token_arena: TokenArena,
    /// Frames retained at suspension still count against recursion while the coroutine runs,
    /// but must not consume depth in an unrelated caller between resumes.
    pub(super) suspended_call_depth: u32,
    /// Upvalue cells downgraded to break a self-reference cycle (see
    /// `downgrade_self_references`), paired with a weak pointer to the original cell they were
    /// split from. Applied on the next resume so outer writes to the captured variable made
    /// while suspended aren't lost.
    pub(super) pending_resyncs: Vec<(Cell, WeakCell)>,
    /// Debugger state belongs to the suspended frames for the same reason as call depth.
    #[cfg(feature = "debugger")]
    pub(super) debug_call_stack: Vec<Shared<Node>>,
    #[cfg(feature = "debugger")]
    pub(super) debug_current_node: Option<Shared<Node>>,
}

/// Cloning a `RuntimeValue::Coroutine` shares this handle, so every clone drives the same
/// progress, using the same interior-mutability idiom as upvalue cells (`tarn::value::Cell`).
pub(crate) type CoroutineHandle = Shared<SharedCell<CoroutineState>>;

#[cfg(not(feature = "sync"))]
pub(crate) type CoroutineWeakHandle = std::rc::Weak<SharedCell<CoroutineState>>;
#[cfg(feature = "sync")]
pub(crate) type CoroutineWeakHandle = std::sync::Weak<SharedCell<CoroutineState>>;

pub(crate) fn downgrade_handle(handle: &CoroutineHandle) -> CoroutineWeakHandle {
    Shared::downgrade(handle)
}

pub(crate) fn upgrade_handle(handle: &CoroutineWeakHandle) -> Option<CoroutineHandle> {
    handle.upgrade()
}

pub(crate) fn same_handle(left: &CoroutineHandle, right: &CoroutineHandle) -> bool {
    Shared::ptr_eq(left, right)
}

impl CoroutineState {
    pub(super) fn new_handle(frame: Frame, chunks: Shared<Vec<Chunk>>, token_arena: TokenArena) -> CoroutineHandle {
        Shared::new(SharedCell::new(CoroutineState {
            status: CoroutineStatus::Created,
            frames: vec![frame],
            operand_stack: Vec::new(),
            chunks,
            token_arena,
            // A created coroutine already owns its generator frame. Count it as soon as it
            // starts running so recursively resuming child generators cannot bypass the VM's
            // recursion limit before any of them reaches a `yield`.
            suspended_call_depth: 1,
            pending_resyncs: Vec::new(),
            #[cfg(feature = "debugger")]
            debug_call_stack: Vec::new(),
            #[cfg(feature = "debugger")]
            debug_current_node: None,
        }))
    }
}

#[cfg(not(feature = "sync"))]
fn borrow_mut(handle: &CoroutineHandle) -> std::cell::RefMut<'_, CoroutineState> {
    handle.borrow_mut()
}

#[cfg(feature = "sync")]
fn borrow_mut(handle: &CoroutineHandle) -> std::sync::RwLockWriteGuard<'_, CoroutineState> {
    handle.write().unwrap()
}

#[cfg(not(feature = "sync"))]
fn borrow(handle: &CoroutineHandle) -> std::cell::Ref<'_, CoroutineState> {
    handle.borrow()
}

#[cfg(feature = "sync")]
fn borrow(handle: &CoroutineHandle) -> std::sync::RwLockReadGuard<'_, CoroutineState> {
    handle.read().unwrap()
}

/// Stable name for `handle`'s current lifecycle state, backing the `status()` builtin's symbol.
pub(crate) fn status_name(handle: &CoroutineHandle) -> &'static str {
    match borrow(handle).status {
        CoroutineStatus::Created => "created",
        CoroutineStatus::Suspended => "suspended",
        CoroutineStatus::Running => "running",
        CoroutineStatus::Completed => "completed",
        CoroutineStatus::Failed(_) => "failed",
    }
}

/// Forces `handle` straight to `Completed`, dropping its frames early. Backs `close()`.
/// `Failed` is left as-is so its error still surfaces later. Returns `false` (no change) if
/// `handle` is `Running`: its frames are live elsewhere on the Rust call stack.
pub(crate) fn close(handle: &CoroutineHandle) -> bool {
    let mut state = borrow_mut(handle);
    match state.status {
        CoroutineStatus::Running => false,
        CoroutineStatus::Completed | CoroutineStatus::Failed(_) => true,
        CoroutineStatus::Created | CoroutineStatus::Suspended => {
            state.frames = Vec::new();
            state.operand_stack = Vec::new();
            state.suspended_call_depth = 0;
            state.pending_resyncs = Vec::new();
            #[cfg(feature = "debugger")]
            {
                state.debug_call_stack = Vec::new();
                state.debug_current_node = None;
            }
            state.status = CoroutineStatus::Completed;
            true
        }
    }
}

fn done_result(value: RuntimeValue, done: bool) -> RuntimeValue {
    let mut map = DictMap::default();
    map.insert(Ident::new("value"), value);
    map.insert(Ident::new("done"), RuntimeValue::Boolean(done));
    RuntimeValue::Dict(Shared::new(map))
}

/// Carries only unchanged self-reference resync pairs.
fn carry_unchanged_resyncs(resyncs: Vec<(Cell, WeakCell)>, handle: &CoroutineHandle) -> Vec<(Cell, WeakCell)> {
    let mut carried = Vec::new();
    for (downgraded_cell, original) in resyncs {
        let Some(original) = original.upgrade() else {
            continue;
        };

        if !is_weak_self_reference(&downgraded_cell, handle) {
            write_cell(&original, read_cell(&downgraded_cell));
            continue;
        }

        let original_is_self_reference = matches!(read_cell(&original), StackValue::Value(RuntimeValue::Coroutine(value)) if same_handle(&value, handle));
        if original_is_self_reference {
            carried.push((downgraded_cell, Shared::downgrade(&original)));
        }
    }
    carried
}

/// Drives `handle` forward one step (`next(stream)`/`send(stream, value)`), returning a
/// `{ value, done }` dict.
pub(super) fn resume<const CHECK_TIMEOUT: bool>(
    handle: &CoroutineHandle,
    resume_value: Option<RuntimeValue>,
    execution: &mut ExecutionContext<'_>,
    #[cfg(feature = "debugger")] debug: &mut DebugRuntime<'_>,
) -> VmResult<RuntimeValue> {
    let (mut frames, mut operand_stack, chunks, caller_depth, pending_resyncs) = {
        let mut state = borrow_mut(handle);
        match &state.status {
            CoroutineStatus::Running => return Err(VmError::CoroutineReentrant),
            CoroutineStatus::Completed => return Ok(done_result(RuntimeValue::None, true)),
            CoroutineStatus::Failed(err) => {
                return Err(VmError::CoroutineFailed(
                    Shared::clone(err),
                    Shared::clone(&state.token_arena),
                ));
            }
            CoroutineStatus::Created | CoroutineStatus::Suspended => {
                let caller_depth = execution
                    .limits
                    .enter_suspended_call_depth(state.suspended_call_depth)?;
                let was_suspended = matches!(state.status, CoroutineStatus::Suspended);
                state.status = CoroutineStatus::Running;
                let mut operand_stack = std::mem::take(&mut state.operand_stack);
                if was_suspended {
                    // The resumed `yield` expression's value; `next()` passes `None`.
                    operand_stack.push(StackValue::Value(resume_value.unwrap_or(RuntimeValue::None)));
                }
                (
                    std::mem::take(&mut state.frames),
                    operand_stack,
                    Shared::clone(&state.chunks),
                    caller_depth,
                    std::mem::take(&mut state.pending_resyncs),
                )
            }
        }
    };

    // Pull in whatever the outer scope wrote to a captured self-reference while this coroutine
    // was suspended, so the resumed body observes it instead of the stale downgraded snapshot.
    // A pull that turns up this same coroutine again (the outer binding still holds it, e.g. it
    // was never resumed before this write-time downgrade) is written back downgraded rather than
    // literally, and the pairing is kept in `carried_resyncs` for the next resume: applying it
    // only once would let `original` diverge from `downgraded_cell` forever the moment this
    // resume's own suspension-time scan finds nothing left to downgrade there.
    let mut carried_resyncs = Vec::new();
    for (downgraded_cell, original) in pending_resyncs {
        if let Some(original) = original.upgrade() {
            let mut value = read_cell(&original);
            let is_direct_self_reference =
                matches!(&value, StackValue::Value(RuntimeValue::Coroutine(h)) if same_handle(h, handle));
            value.downgrade_coroutine_reference(handle);
            write_cell(&downgraded_cell, value);
            if is_direct_self_reference {
                carried_resyncs.push((downgraded_cell, Shared::downgrade(&original)));
            }
        }
    }

    #[cfg(feature = "debugger")]
    let (caller_call_stack, caller_current_node) = {
        let mut state = borrow_mut(handle);
        (
            std::mem::replace(&mut debug.call_stack, std::mem::take(&mut state.debug_call_stack)),
            std::mem::replace(&mut debug.current_node, state.debug_current_node.take()),
        )
    };

    let outcome = super::drive_frames::<CHECK_TIMEOUT>(
        &chunks,
        &mut frames,
        &mut operand_stack,
        execution,
        #[cfg(feature = "debugger")]
        debug,
    );
    let suspended_call_depth = execution.limits.leave_suspended_call_depth(caller_depth);

    // Keep only resyncs unchanged by this invocation.
    let carried_resyncs = carry_unchanged_resyncs(carried_resyncs, handle);

    let mut state = borrow_mut(handle);
    #[cfg(feature = "debugger")]
    {
        state.debug_call_stack = std::mem::replace(&mut debug.call_stack, caller_call_stack);
        state.debug_current_node = std::mem::replace(&mut debug.current_node, caller_current_node);
    }
    match outcome {
        DriveOutcome::Suspended(value) => {
            let value = into_runtime_value(value, &chunks);
            // Break captured self-reference cycles while this state is suspended.
            let mut resyncs = downgrade_self_references(&mut frames, &mut operand_stack, handle);
            for peer in mutually_capturing_peers(handle, &frames, &operand_stack) {
                resyncs.extend(downgrade_self_references(&mut frames, &mut operand_stack, &peer));
            }
            state.frames = frames;
            state.operand_stack = operand_stack;
            state.pending_resyncs = carried_resyncs;
            state.pending_resyncs.extend(resyncs);
            state.suspended_call_depth = suspended_call_depth;
            state.status = CoroutineStatus::Suspended;
            drop(state);
            Ok(done_result(value, false))
        }
        DriveOutcome::Completed(_, _locals) => {
            debug_assert_eq!(
                suspended_call_depth, 1,
                "completed coroutine must release its initial generator frame depth"
            );
            state.status = CoroutineStatus::Completed;
            drop(state);
            // `next()` signals exhaustion rather than returning a generator function's
            // ordinary return value. This is the public generator contract; subsequent calls
            // take the same already-completed path above.
            Ok(done_result(RuntimeValue::None, true))
        }
        DriveOutcome::Failed(e, _locals) => {
            debug_assert_eq!(
                suspended_call_depth, 1,
                "failed coroutine must release its initial generator frame depth"
            );
            let stored = Shared::new(e);
            state.status = CoroutineStatus::Failed(Shared::clone(&stored));
            let token_arena = Shared::clone(&state.token_arena);
            drop(state);
            Err(VmError::CoroutineFailed(stored, token_arena))
        }
    }
}

/// Breaks self-reference cycles in `frames`/`operand_stack` before a coroutine suspends.
///
/// A captured cell that holds this coroutine's own handle, directly or nested in an array/dict,
/// can't be edited in place: it shares its allocation with the caller's binding, and mutating it
/// would corrupt data the caller reads later. Instead the frame's reference is swapped for a new
/// cell holding a sanitized copy (a weak-and-upgradable handle for the direct case, see
/// `weak_coroutine_cell`; the nested self-reference cleared to `None` otherwise, see
/// `sanitize_nested_self_reference`), and the swap is recorded so the next resume can pull in
/// whatever the caller wrote to the original cell in the meantime (see `resume`'s
/// `pending_resyncs` handling).
fn downgrade_self_references(
    frames: &mut [Frame],
    operand_stack: &mut [StackValue],
    handle: &CoroutineHandle,
) -> Vec<(Cell, WeakCell)> {
    let mut resyncs = Vec::new();
    for frame in frames {
        resyncs.extend(frame.locals.downgrade_coroutine_references(handle));
        if let Some(upvalues) = &mut frame.upvalues {
            for upvalue in Shared::make_mut(upvalues) {
                if let Some(weak_cell) = weak_coroutine_cell(upvalue, handle) {
                    resyncs.push((Shared::clone(&weak_cell), Shared::downgrade(upvalue)));
                    *upvalue = weak_cell;
                } else if let Some(sanitized) = sanitize_nested_self_reference(upvalue, handle) {
                    resyncs.push((Shared::clone(&sanitized), Shared::downgrade(upvalue)));
                    *upvalue = sanitized;
                }
            }
        }
    }
    for value in operand_stack {
        value.downgrade_coroutine_reference(handle);
    }
    resyncs
}

/// Coroutine handles `frames`/`operand_stack` directly hold (not following into any of those
/// handles' own captured state).
fn direct_coroutine_neighbors(frames: &[Frame], operand_stack: &[StackValue]) -> Vec<CoroutineHandle> {
    let mut out = Vec::new();
    for frame in frames {
        frame.locals.collect_coroutine_handles(&mut out);
        if let Some(upvalues) = &frame.upvalues {
            for cell in upvalues.iter() {
                collect_coroutine_handles_in_cell(cell, &mut out);
            }
        }
    }
    for value in operand_stack {
        collect_coroutine_handles_in_stack_value(value, &mut out);
    }
    out
}

/// Whether `peer`'s own current frames directly hold `target`.
fn peer_directly_references(peer: &CoroutineHandle, target: &CoroutineHandle) -> bool {
    let state = borrow(peer);
    direct_coroutine_neighbors(&state.frames, &state.operand_stack)
        .iter()
        .any(|h| same_handle(h, target))
}

/// Handles `handle` directly captures that also directly capture `handle` back: a 2-coroutine
/// cycle closing right here (e.g. `a = ga()`, `b = gb()` where `ga` captures `b` and `gb`
/// captures `a`). Breaking one edge of a 2-cycle fully eliminates it, so this covers the common
/// "two generators capture each other" case without needing full graph reachability. Longer
/// chains (`A -> B -> C -> A`) aren't detected.
fn mutually_capturing_peers(
    handle: &CoroutineHandle,
    frames: &[Frame],
    operand_stack: &[StackValue],
) -> Vec<CoroutineHandle> {
    direct_coroutine_neighbors(frames, operand_stack)
        .into_iter()
        .filter(|peer| !same_handle(peer, handle) && peer_directly_references(peer, handle))
        .collect()
}

/// Breaks a self-reference cycle the moment it's created, instead of waiting for `handle`'s
/// first `Yield`: e.g. `s = g()` where `g`'s body captures `s` gives `handle`'s own frame an
/// upvalue cell now holding `handle` itself, before the coroutine has ever suspended.
/// `downgrade_self_references` only runs from `resume`'s `Suspended` arm, so an unstarted (or
/// currently-suspended, if the caller writes into it again) coroutine would otherwise never reach
/// that cleanup and leak the cycle for good. No-op while `Running`, `Completed`, or `Failed`:
/// those either have their frames borrowed elsewhere or already released them.
pub(crate) fn downgrade_self_references_before_resume(handle: &CoroutineHandle) {
    let mut state = borrow_mut(handle);
    if !matches!(state.status, CoroutineStatus::Created | CoroutineStatus::Suspended) {
        return;
    }
    let mut frames = std::mem::take(&mut state.frames);
    let mut operand_stack = std::mem::take(&mut state.operand_stack);
    let mut resyncs = downgrade_self_references(&mut frames, &mut operand_stack, handle);
    for peer in mutually_capturing_peers(handle, &frames, &operand_stack) {
        resyncs.extend(downgrade_self_references(&mut frames, &mut operand_stack, &peer));
    }
    state.frames = frames;
    state.operand_stack = operand_stack;
    state.pending_resyncs.extend(resyncs);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arena::Arena;
    use crate::tarn::interpreter::frame::Continuation;
    use crate::tarn::value::{Locals, append_to_array_cell, new_cell};

    #[test]
    fn appending_an_unstarted_coroutine_to_its_captured_array_releases_it() {
        let holder = new_cell(StackValue::Value(RuntimeValue::Array(Shared::new(Vec::new()))));
        let frame = Frame::new(
            0,
            None,
            Locals::flat(0),
            Some(Shared::new(vec![Shared::clone(&holder)])),
            false,
            Continuation::Push,
        );
        let handle = CoroutineState::new_handle(
            frame,
            Shared::new(vec![Chunk::default()]),
            Shared::new(SharedCell::new(Arena::new(1))),
        );
        let weak = Shared::downgrade(&handle);

        append_to_array_cell(&holder, RuntimeValue::Coroutine(Shared::clone(&handle))).unwrap();
        drop(handle);
        drop(holder);

        assert!(weak.upgrade().is_none());
    }
}
