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

fn done_result(value: RuntimeValue, done: bool) -> RuntimeValue {
    let mut map = DictMap::default();
    map.insert(Ident::new("value"), value);
    map.insert(Ident::new("done"), RuntimeValue::Boolean(done));
    RuntimeValue::Dict(Shared::new(map))
}

/// Drives `handle` forward one step (`next(stream)`), returning a `{ value, done }` dict.
pub(super) fn resume<const CHECK_TIMEOUT: bool>(
    handle: &CoroutineHandle,
    execution: &mut ExecutionContext<'_>,
    #[cfg(feature = "debugger")] debug: &mut DebugRuntime<'_>,
) -> VmResult<RuntimeValue> {
    let (mut frames, mut operand_stack, chunks) = {
        let mut state = borrow_mut(handle);
        match &state.status {
            CoroutineStatus::Running => return Err(VmError::CoroutineReentrant),
            CoroutineStatus::Completed => return Ok(done_result(RuntimeValue::None, true)),
            CoroutineStatus::Failed(err) => return Err(VmError::CoroutineFailed(Shared::clone(err))),
            CoroutineStatus::Created | CoroutineStatus::Suspended => {
                let was_suspended = matches!(state.status, CoroutineStatus::Suspended);
                state.status = CoroutineStatus::Running;
                let mut operand_stack = std::mem::take(&mut state.operand_stack);
                if was_suspended {
                    // A resumed `yield` expression must leave a value on the stack, like any
                    // other statement. `next(stream)` carries none, so it resumes to `None`.
                    operand_stack.push(StackValue::Value(RuntimeValue::None));
                }
                (
                    std::mem::take(&mut state.frames),
                    operand_stack,
                    Shared::clone(&state.chunks),
                )
            }
        }
    };

    let outcome = super::drive_frames::<CHECK_TIMEOUT>(
        &chunks,
        &mut frames,
        &mut operand_stack,
        execution,
        #[cfg(feature = "debugger")]
        debug,
    );

    let mut state = borrow_mut(handle);
    match outcome {
        DriveOutcome::Suspended(value) => {
            let value = into_runtime_value(value, &chunks);
            state.frames = frames;
            state.operand_stack = operand_stack;
            state.status = CoroutineStatus::Suspended;
            drop(state);
            Ok(done_result(value, false))
        }
        DriveOutcome::Completed(value, _locals) => {
            let value = into_runtime_value(value, &chunks);
            state.status = CoroutineStatus::Completed;
            drop(state);
            Ok(done_result(value, true))
        }
        DriveOutcome::Failed(e, _locals) => {
            let stored = Shared::new(e);
            state.status = CoroutineStatus::Failed(Shared::clone(&stored));
            drop(state);
            Err(VmError::CoroutineFailed(stored))
        }
    }
}
