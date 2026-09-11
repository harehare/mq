//! Suspended generator/coroutine state: the `Vec<Frame>` + operand stack a `yield` detaches
//! from the trampoline instead of letting it recycle them, resumed by `OpCode::Resume`.
//!
//! Holds no borrows of `VmEnv`/`HostFunctions`/`ExecutionContext` — `resume` takes those fresh
//! from the caller, so a coroutine can be resumed by an unrelated later evaluation.
#[cfg(feature = "debugger")]
use super::DebugRuntime;
use super::errors::{VmError, VmResult};
use super::frame::{ExecutionContext, Frame};
use super::{DriveOutcome, into_runtime_value};
#[cfg(feature = "debugger")]
use crate::ast::node::Node;
use crate::runtime::runtime_value::{DictMap, RuntimeValue};
use crate::tarn::bytecode::Chunk;
use crate::tarn::value::StackValue;
use crate::{Ident, Shared, SharedCell};

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
    /// Frames retained at suspension still count against recursion while the coroutine runs,
    /// but must not consume depth in an unrelated caller between resumes.
    pub(super) suspended_call_depth: u32,
    /// Debugger state belongs to the suspended frames for the same reason as call depth.
    #[cfg(feature = "debugger")]
    pub(super) debug_call_stack: Vec<Shared<Node>>,
    #[cfg(feature = "debugger")]
    pub(super) debug_current_node: Option<Shared<Node>>,
}

/// Cloning a `RuntimeValue::Coroutine` shares this handle, so every clone drives the same
/// progress — the same interior-mutability idiom as upvalue cells (`tarn::value::Cell`).
pub(crate) type CoroutineHandle = Shared<SharedCell<CoroutineState>>;

impl CoroutineState {
    pub(super) fn new_handle(frame: Frame, chunks: Shared<Vec<Chunk>>) -> CoroutineHandle {
        Shared::new(SharedCell::new(CoroutineState {
            status: CoroutineStatus::Created,
            frames: vec![frame],
            operand_stack: Vec::new(),
            chunks,
            suspended_call_depth: 0,
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

/// Drives `handle` forward one step (`next(stream)`/`send(stream, value)`), returning a
/// `{ value, done }` dict.
pub(super) fn resume<const CHECK_TIMEOUT: bool>(
    handle: &CoroutineHandle,
    resume_value: Option<RuntimeValue>,
    execution: &mut ExecutionContext<'_>,
    #[cfg(feature = "debugger")] debug: &mut DebugRuntime<'_>,
) -> VmResult<RuntimeValue> {
    let (mut frames, mut operand_stack, chunks, caller_depth) = {
        let mut state = borrow_mut(handle);
        match &state.status {
            CoroutineStatus::Running => return Err(VmError::CoroutineReentrant),
            CoroutineStatus::Completed => return Ok(done_result(RuntimeValue::None, true)),
            CoroutineStatus::Failed(err) => return Err(VmError::CoroutineFailed(Shared::clone(err))),
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
                )
            }
        }
    };

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

    let mut state = borrow_mut(handle);
    #[cfg(feature = "debugger")]
    {
        state.debug_call_stack = std::mem::replace(&mut debug.call_stack, caller_call_stack);
        state.debug_current_node = std::mem::replace(&mut debug.current_node, caller_current_node);
    }
    match outcome {
        DriveOutcome::Suspended(value) => {
            let value = into_runtime_value(value, &chunks);
            state.frames = frames;
            state.operand_stack = operand_stack;
            state.suspended_call_depth = suspended_call_depth;
            state.status = CoroutineStatus::Suspended;
            drop(state);
            Ok(done_result(value, false))
        }
        DriveOutcome::Completed(_, _locals) => {
            debug_assert_eq!(
                suspended_call_depth, 0,
                "completed coroutine must release all call depth"
            );
            state.status = CoroutineStatus::Completed;
            drop(state);
            // `next()` signals exhaustion rather than returning a generator function's
            // ordinary return value. This is the public generator contract; subsequent calls
            // take the same already-completed path above.
            Ok(done_result(RuntimeValue::None, true))
        }
        DriveOutcome::Failed(e, _locals) => {
            debug_assert_eq!(suspended_call_depth, 0, "failed coroutine must release all call depth");
            let stored = Shared::new(e);
            state.status = CoroutineStatus::Failed(Shared::clone(&stored));
            drop(state);
            Err(VmError::CoroutineFailed(stored))
        }
    }
}
