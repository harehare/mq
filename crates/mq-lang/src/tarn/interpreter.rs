//! Tarn's bytecode dispatch loop: `run_frame_slice` and its opcode handlers, `run_frames`'s
//! explicit-frame-stack trampoline driving it, and the public `run_*` entry points.
//!
//! `errors` (the `VmError` type), `frame` (deadline/call-depth tracking and the `Locals`/stack
//! pools), `calls` (binding arguments and invoking a callee), and `selectors` (applying a
//! `Selector` to a value) hold the parts that split out cleanly; this file is what remains.
mod calls;
mod errors;
mod frame;
mod selectors;

use self::calls::{
    CallSite, CallStep, ExactCallTarget, FixedClosureCall, KnownFixedChunkCall, apply_pending, call_builtin,
    call_builtin_args, call_exact_fixed_chunk_0, call_exact_fixed_chunk_1, call_exact_fixed_chunk_2,
    call_fixed_closure_from_stack, call_known_fixed_chunk_from_stack, call_self_chunk_from_stack, call_stack_value,
    call_static_chunk_from_stack, capture_upvalues, negate_ident,
};
use self::selectors::{eval_compact_selector_expr, eval_selector_expr, eval_selector_expr_with_args, type_check};
use super::bytecode::{BinaryOp, Chunk, OpCode, SELF_SLOT, TryCatchInfo};
use super::compiler::CompiledProgram;
#[cfg(feature = "debugger")]
use super::value::Cell;
use super::value::VmClosureValue;
use super::value::{Closure, Locals, StackValue, read_cell, write_cell};
#[cfg(feature = "debugger")]
use crate::ast::TokenId;
use crate::ast::constants::builtins;
use crate::number::Number;
use crate::runtime::builtin::{self, Args};
use crate::runtime::host::HostFunctions;
use crate::runtime::runtime_value::{self, RuntimeValue};
use crate::selector::Selector;
use crate::tarn::VmEnv;
use crate::{Ident, Shared};
pub(crate) use errors::VmError;
use errors::{VmResult, error_dict, flow_break_value, flow_continue, locate};
pub(crate) use frame::ExecutionPools;
use frame::{Continuation, ExecutionContext, ExecutionLimits, Frame, TryBody};
use std::sync::LazyLock;
use std::time::Duration;

static LEN_IDENT: LazyLock<Ident> = LazyLock::new(|| Ident::new(builtins::LEN));
static GET_IDENT: LazyLock<Ident> = LazyLock::new(|| Ident::new(builtins::GET));

#[cfg(feature = "debugger")]
use super::debug_symbols::DebugSlot;
#[cfg(feature = "debugger")]
use crate::ast::node::Node;
#[cfg(feature = "debugger")]
use crate::runtime::debugger::{VmDebugBinding, VmDebugFrame};

/// A snapshot of a VM frame at a source evaluation boundary, with a shared queue for writes
/// requested while execution is paused.
#[cfg(feature = "debugger")]
#[derive(Debug, Clone)]
pub(crate) struct DebugEvent {
    pub(crate) token_id: TokenId,
    pub(crate) node: Shared<Node>,
    pub(crate) current_value: RuntimeValue,
    pub(crate) bindings: Vec<(Ident, RuntimeValue)>,
    pub(crate) vm_frame: VmDebugFrame,
    pub(crate) call_stack: Vec<Shared<Node>>,
    #[cfg(feature = "debug-trace")]
    pub(crate) operand_stack: Vec<RuntimeValue>,
}

/// Receives VM debug boundaries. Implementations can queue frame writes while the VM is paused.
#[cfg(feature = "debugger")]
pub(crate) trait DebugHook {
    fn on_boundary(&mut self, event: DebugEvent) -> VmResult<()>;

    fn on_explicit_breakpoint(&mut self, event: DebugEvent) -> VmResult<()>;
}

#[cfg(feature = "debugger")]
struct DebugRuntime<'a> {
    hook: Option<&'a mut dyn DebugHook>,
    call_stack: Vec<Shared<Node>>,
    current_node: Option<Shared<Node>>,
}

pub(crate) struct RunOptions<'a> {
    pub(crate) host_functions: &'a HostFunctions,
    pub(crate) timeout: Option<Duration>,
    pub(crate) max_call_stack_depth: u32,
    pub(crate) global_bindings: &'a [(Ident, RuntimeValue)],
}

/// A top-level local selected for capture after execution.
///
/// The compiler may retain multiple source declarations for the same name. The slot is resolved
/// when bytecode is compiled so repeated evaluations can read it directly.
pub(crate) type CaptureSlot = (Ident, u16);

/// Resolves names to their final top-level slots, preserving the compiler's last-declaration
/// lookup semantics.
pub(crate) fn capture_slots(chunk: &Chunk, names: &[Ident]) -> Vec<CaptureSlot> {
    names
        .iter()
        .filter_map(|name| {
            chunk
                .local_names
                .iter()
                .rposition(|local| local == name)
                .map(|slot| (*name, slot as u16))
        })
        .collect()
}

/// Runs a compiled program with Engine-defined globals.
pub(crate) fn run_with_globals(
    compiled: &CompiledProgram,
    input: RuntimeValue,
    host_functions: &HostFunctions,
    timeout: Option<Duration>,
    max_call_stack_depth: u32,
    global_bindings: &[(Ident, RuntimeValue)],
) -> VmResult<RuntimeValue> {
    run_with_globals_and_pools(
        compiled,
        input,
        host_functions,
        timeout,
        max_call_stack_depth,
        global_bindings,
        ExecutionPools::default(),
    )
    .0
}

/// Runs a compiled program and returns reusable execution pools.
pub(crate) fn run_with_globals_and_pools(
    compiled: &CompiledProgram,
    input: RuntimeValue,
    host_functions: &HostFunctions,
    timeout: Option<Duration>,
    max_call_stack_depth: u32,
    global_bindings: &[(Ident, RuntimeValue)],
    pools: ExecutionPools,
) -> (VmResult<RuntimeValue>, ExecutionPools) {
    let env = VmEnv::from_bindings(global_bindings);
    run_with_env_and_pools(
        compiled,
        input,
        host_functions,
        timeout,
        max_call_stack_depth,
        &env,
        pools,
    )
}

/// Runs a compiled program with a prebuilt external environment and reusable execution pools.
///
/// Callers that evaluate multiple inputs with the same globals should construct the environment
/// once and use this entry point to avoid rebuilding its lookup table per input.
pub(crate) fn run_with_env_and_pools(
    compiled: &CompiledProgram,
    input: RuntimeValue,
    host_functions: &HostFunctions,
    timeout: Option<Duration>,
    max_call_stack_depth: u32,
    env: &VmEnv,
    pools: ExecutionPools,
) -> (VmResult<RuntimeValue>, ExecutionPools) {
    #[cfg(feature = "debugger")]
    let mut debug = DebugRuntime {
        hook: None,
        call_stack: Vec::new(),
        current_node: None,
    };
    run_impl_with_env(
        compiled,
        input,
        RunOptions {
            host_functions,
            timeout,
            max_call_stack_depth,
            global_bindings: &[],
        },
        env,
        pools,
        #[cfg(feature = "debugger")]
        &mut debug,
    )
}

/// Runs with predeclared bindings and captures selected locals.
pub(crate) fn run_with_globals_capturing_locals(
    compiled: &CompiledProgram,
    input: RuntimeValue,
    bindings: &[RuntimeValue],
    options: RunOptions<'_>,
    capture_names: &[Ident],
    pools: ExecutionPools,
) -> (VmResult<RuntimeValue>, Vec<(Ident, RuntimeValue)>, ExecutionPools) {
    let env = VmEnv::from_bindings(options.global_bindings);
    run_with_env_capturing_locals(compiled, input, bindings, options, &env, capture_names, pools)
}

/// Runs with a prebuilt external environment and captures selected locals.
pub(crate) fn run_with_env_capturing_locals(
    compiled: &CompiledProgram,
    input: RuntimeValue,
    bindings: &[RuntimeValue],
    options: RunOptions<'_>,
    env: &VmEnv,
    capture_names: &[Ident],
    pools: ExecutionPools,
) -> (VmResult<RuntimeValue>, Vec<(Ident, RuntimeValue)>, ExecutionPools) {
    let capture_slots = capture_slots(&compiled.chunks[0], capture_names);
    run_with_env_capturing_slots(compiled, input, bindings, options, env, &capture_slots, pools)
}

/// Runs with precomputed top-level local slots and captures their final values.
///
/// [`capture_slots`] lets cached callers resolve names once at compilation time rather than
/// scanning the chunk's local names for every input value.
pub(crate) fn run_with_env_capturing_slots(
    compiled: &CompiledProgram,
    input: RuntimeValue,
    bindings: &[RuntimeValue],
    options: RunOptions<'_>,
    env: &VmEnv,
    capture_slots: &[CaptureSlot],
    pools: ExecutionPools,
) -> (VmResult<RuntimeValue>, Vec<(Ident, RuntimeValue)>, ExecutionPools) {
    #[cfg(feature = "debugger")]
    let mut debug = DebugRuntime {
        hook: None,
        call_stack: Vec::new(),
        current_node: None,
    };
    run_impl_capturing_locals_with_env(
        compiled,
        input,
        bindings,
        options,
        env,
        pools,
        capture_slots,
        #[cfg(feature = "debugger")]
        &mut debug,
    )
}

#[cfg(feature = "debugger")]
/// Runs a program with debugger callbacks.
pub(crate) fn run_with_debug_hook_and_globals(
    compiled: &CompiledProgram,
    input: RuntimeValue,
    host_functions: &HostFunctions,
    timeout: Option<Duration>,
    max_call_stack_depth: u32,
    global_bindings: &[(Ident, RuntimeValue)],
    hook: &mut dyn DebugHook,
) -> VmResult<RuntimeValue> {
    let mut debug = DebugRuntime {
        hook: Some(hook),
        call_stack: Vec::new(),
        current_node: None,
    };
    run_impl(
        compiled,
        input,
        RunOptions {
            host_functions,
            timeout,
            max_call_stack_depth,
            global_bindings,
        },
        ExecutionPools::default(),
        &mut debug,
    )
    .0
}

/// Captures locals while reporting debugger events.
#[cfg(feature = "debugger")]
pub(crate) fn run_with_debug_hook_and_globals_capturing_locals(
    compiled: &CompiledProgram,
    input: RuntimeValue,
    bindings: &[RuntimeValue],
    options: RunOptions<'_>,
    capture_names: &[Ident],
    hook: &mut dyn DebugHook,
) -> (VmResult<RuntimeValue>, Vec<(Ident, RuntimeValue)>) {
    let mut debug = DebugRuntime {
        hook: Some(hook),
        call_stack: Vec::new(),
        current_node: None,
    };
    let env = VmEnv::from_bindings(options.global_bindings);
    let capture_slots = capture_slots(&compiled.chunks[0], capture_names);
    let (result, captured, _) = run_impl_capturing_locals_with_env(
        compiled,
        input,
        bindings,
        options,
        &env,
        ExecutionPools::default(),
        &capture_slots,
        &mut debug,
    );
    (result, captured)
}

#[cfg(feature = "debugger")]
fn run_impl(
    compiled: &CompiledProgram,
    input: RuntimeValue,
    options: RunOptions<'_>,
    pools: ExecutionPools,
    #[cfg(feature = "debugger")] debug: &mut DebugRuntime<'_>,
) -> (VmResult<RuntimeValue>, ExecutionPools) {
    let env = VmEnv::from_bindings(options.global_bindings);
    run_impl_with_env(
        compiled,
        input,
        options,
        &env,
        pools,
        #[cfg(feature = "debugger")]
        debug,
    )
}

fn run_impl_with_env(
    compiled: &CompiledProgram,
    input: RuntimeValue,
    options: RunOptions<'_>,
    env: &VmEnv,
    pools: ExecutionPools,
    #[cfg(feature = "debugger")] debug: &mut DebugRuntime<'_>,
) -> (VmResult<RuntimeValue>, ExecutionPools) {
    run_impl_with_bindings(
        compiled,
        input,
        &[],
        options,
        env,
        pools,
        #[cfg(feature = "debugger")]
        debug,
    )
}

#[cfg(feature = "debugger")]
/// Evaluates a debugger expression.
pub(crate) fn run_debug_expression(
    compiled: &CompiledProgram,
    input: RuntimeValue,
    bindings: &[RuntimeValue],
    host_functions: &HostFunctions,
) -> VmResult<RuntimeValue> {
    let mut debug = DebugRuntime {
        hook: None,
        call_stack: Vec::new(),
        current_node: None,
    };
    let env = VmEnv::from_bindings(&[]);
    run_impl_with_bindings(
        compiled,
        input,
        bindings,
        RunOptions {
            host_functions,
            timeout: None,
            max_call_stack_depth: crate::tarn::Options::default().max_call_stack_depth,
            global_bindings: &[],
        },
        &env,
        ExecutionPools::default(),
        &mut debug,
    )
    .0
}

fn run_impl_with_bindings(
    compiled: &CompiledProgram,
    input: RuntimeValue,
    initial_bindings: &[RuntimeValue],
    options: RunOptions<'_>,
    env: &VmEnv,
    pools: ExecutionPools,
    #[cfg(feature = "debugger")] debug: &mut DebugRuntime<'_>,
) -> (VmResult<RuntimeValue>, ExecutionPools) {
    let mut limits = ExecutionLimits::new(options.timeout, options.max_call_stack_depth, pools);
    let top_level_chunk = &compiled.chunks[0];
    let captures_local_slots = top_level_chunk.captures_local_slots();
    let mut locals = limits.take_locals(top_level_chunk.local_count, top_level_chunk.captured_local_slots());
    locals.set(SELF_SLOT, StackValue::Value(input));
    if initial_bindings.len() + 1 > locals.len() {
        if !captures_local_slots {
            limits.recycle_locals(locals);
        }
        return (
            Err(VmError::Corrupt("too many initial debug bindings")),
            limits.into_pools(),
        );
    }
    for (slot, value) in initial_bindings.iter().cloned().enumerate() {
        locals.set(slot as u16 + 1, StackValue::Value(value));
    }
    let mut execution = ExecutionContext {
        env,
        limits: &mut limits,
        host_functions: options.host_functions,
    };
    let result = run_chunk(
        0,
        &compiled.chunks,
        locals,
        &mut execution,
        #[cfg(feature = "debugger")]
        debug,
    )
    .map(|result| into_runtime_value(result, &compiled.chunks));
    (result, limits.into_pools())
}

/// Like [`run_impl_with_bindings`], but captures precomputed local slots' final values. Bypasses
/// `run_chunk`'s pooling wrapper to keep `locals` readable.
#[allow(clippy::too_many_arguments)] // The separate pools, capture list, and debugger are independent services.
fn run_impl_capturing_locals_with_env(
    compiled: &CompiledProgram,
    input: RuntimeValue,
    bindings: &[RuntimeValue],
    options: RunOptions<'_>,
    env: &VmEnv,
    pools: ExecutionPools,
    capture_slots: &[CaptureSlot],
    #[cfg(feature = "debugger")] debug: &mut DebugRuntime<'_>,
) -> (VmResult<RuntimeValue>, Vec<(Ident, RuntimeValue)>, ExecutionPools) {
    let mut limits = ExecutionLimits::new(options.timeout, options.max_call_stack_depth, pools);
    let chunks = &compiled.chunks;
    let top_level_chunk = &chunks[0];
    let reusable_locals = !top_level_chunk.captures_local_slots();
    let mut locals = limits.take_locals(top_level_chunk.local_count, top_level_chunk.captured_local_slots());
    locals.set(SELF_SLOT, StackValue::Value(input));
    if bindings.len() + 1 > locals.len() {
        if reusable_locals {
            limits.recycle_locals(locals);
        }
        return (
            Err(VmError::Corrupt("too many initial bindings")),
            Vec::new(),
            limits.into_pools(),
        );
    }
    for (slot, value) in bindings.iter().cloned().enumerate() {
        locals.set(slot as u16 + 1, StackValue::Value(value));
    }

    let mut execution = ExecutionContext {
        env,
        limits: &mut limits,
        host_functions: options.host_functions,
    };
    let initial = Frame::new(0, None, locals, None, reusable_locals, Continuation::Push);
    let (raw_result, locals) = run_frames(
        initial,
        chunks,
        &mut execution,
        #[cfg(feature = "debugger")]
        debug,
    );
    let captured = capture_slots
        .iter()
        .filter_map(|(name, slot)| {
            locals
                .get_checked(*slot)
                .map(|value| (*name, into_runtime_value(value, chunks)))
        })
        .collect();
    if reusable_locals {
        execution.limits.recycle_locals(locals);
    }
    let result = raw_result.map(|result| into_runtime_value(result, chunks));
    (result, captured, limits.into_pools())
}

fn into_runtime_value(v: StackValue, chunks: &Shared<Vec<Chunk>>) -> RuntimeValue {
    match v {
        StackValue::Value(rv) => rv,
        StackValue::Closure(closure) => {
            RuntimeValue::VmClosure(Shared::new(VmClosureValue::from_closure(chunks, &closure)))
        }
    }
}

fn current_self(locals: &Locals, chunks: &Shared<Vec<Chunk>>) -> RuntimeValue {
    into_runtime_value(locals.get(SELF_SLOT), chunks)
}

#[cfg(feature = "debugger")]
fn debug_bindings(
    chunk: &Chunk,
    locals: &Locals,
    upvalues: &[Cell],
    chunks: &Shared<Vec<Chunk>>,
) -> Option<DebugBindings> {
    let mut bindings = Vec::with_capacity(chunk.debug_symbols.bindings().len());
    let mut local_slots = Vec::new();
    let mut upvalue_slots = Vec::new();
    for (name, slot) in chunk.debug_symbols.bindings() {
        let value = match slot {
            DebugSlot::Local(slot) => locals.get_checked(*slot),
            DebugSlot::Upvalue(slot) => upvalues.get(*slot as usize).map(read_cell),
        }?;
        let binding = (*name, into_runtime_value(value, chunks));
        match slot {
            DebugSlot::Local(slot) => {
                local_slots.push(VmDebugBinding::new(*name, *slot, binding.1.clone()));
            }
            DebugSlot::Upvalue(slot) => {
                upvalue_slots.push(VmDebugBinding::new(*name, *slot, binding.1.clone()));
            }
        }
        bindings.push(binding);
    }
    Some(DebugBindings {
        bindings,
        vm_frame: VmDebugFrame::new(local_slots, upvalue_slots),
    })
}

#[cfg(feature = "debugger")]
struct DebugBindings {
    bindings: Vec<(Ident, RuntimeValue)>,
    vm_frame: VmDebugFrame,
}

#[cfg(feature = "debugger")]
fn apply_debug_updates(frame: &VmDebugFrame, locals: &mut Locals, upvalues: &[Cell]) {
    for update in frame.take_pending_updates() {
        if update.is_upvalue {
            if let Some(cell) = upvalues.get(update.slot as usize) {
                write_cell(cell, StackValue::Value(update.value));
            }
        } else if (update.slot as usize) < locals.len() {
            locals.set(update.slot, StackValue::Value(update.value));
        }
    }
}

/// For callers that don't need the bottom frame's `Locals` back afterward.
fn run_chunk(
    chunk_index: u16,
    chunks: &Shared<Vec<Chunk>>,
    locals: Locals,
    execution: &mut ExecutionContext<'_>,
    #[cfg(feature = "debugger")] debug: &mut DebugRuntime<'_>,
) -> VmResult<StackValue> {
    let reusable_locals = !chunks[chunk_index as usize].captures_local_slots();
    let initial = Frame::new(chunk_index, None, locals, None, reusable_locals, Continuation::Push);
    let (result, locals) = run_frames(
        initial,
        chunks,
        execution,
        #[cfg(feature = "debugger")]
        debug,
    );
    if reusable_locals {
        execution.limits.recycle_locals(locals);
    }
    result
}

enum FrameOutcome {
    Enter(Frame),
    /// A call followed immediately by `Return`; the callee can replace this frame.
    TailEnter(Frame),
    Complete(StackValue),
}

/// The trampoline: an explicit `Vec<Frame>` replaces Rust's own call stack, so mq call depth is
/// decoupled from Rust stack depth. Returns the bottom frame's `Locals` unrecycled.
fn run_frames(
    initial: Frame,
    root_chunks: &Shared<Vec<Chunk>>,
    execution: &mut ExecutionContext<'_>,
    #[cfg(feature = "debugger")] debug: &mut DebugRuntime<'_>,
) -> (VmResult<StackValue>, Locals) {
    let mut operand_stack = execution.limits.take_stack();
    let mut frames = execution.limits.take_frame_stack();
    let result = if execution.limits.has_deadline() {
        run_frames_impl::<true>(
            initial,
            root_chunks,
            &mut frames,
            &mut operand_stack,
            execution,
            #[cfg(feature = "debugger")]
            debug,
        )
    } else {
        run_frames_impl::<false>(
            initial,
            root_chunks,
            &mut frames,
            &mut operand_stack,
            execution,
            #[cfg(feature = "debugger")]
            debug,
        )
    };
    execution.limits.recycle_frame_stack(frames);
    execution.limits.recycle_stack(operand_stack);
    result
}

fn run_frames_impl<const CHECK_TIMEOUT: bool>(
    initial: Frame,
    root_chunks: &Shared<Vec<Chunk>>,
    frames: &mut Vec<Frame>,
    operand_stack: &mut Vec<StackValue>,
    execution: &mut ExecutionContext<'_>,
    #[cfg(feature = "debugger")] debug: &mut DebugRuntime<'_>,
) -> (VmResult<StackValue>, Locals) {
    frames.push(initial);

    'frames: loop {
        let frame = frames.last_mut().expect("the frame stack is never empty here");
        let outcome = run_frame_slice::<CHECK_TIMEOUT>(
            frame,
            root_chunks,
            operand_stack,
            execution,
            #[cfg(feature = "debugger")]
            debug,
        );

        let value = match outcome {
            Ok(FrameOutcome::Enter(mut new_frame)) => {
                new_frame.stack_base = operand_stack.len();
                if let Err(e) = execution.limits.push_frame(
                    frames,
                    new_frame,
                    #[cfg(feature = "debugger")]
                    debug,
                ) {
                    let e = locate_at_top(frames, root_chunks, e);
                    match unwind(
                        e,
                        frames,
                        root_chunks,
                        operand_stack,
                        execution,
                        #[cfg(feature = "debugger")]
                        debug,
                    ) {
                        Ok(()) => continue 'frames,
                        Err((e, locals)) => return (Err(e), locals),
                    }
                }
                continue 'frames;
            }
            Ok(FrameOutcome::TailEnter(mut new_frame)) => {
                let caller = frames.last().expect("the frame stack is never empty here");
                new_frame.stack_base = caller.stack_base;
                execution.limits.replace_top_frame(
                    frames,
                    new_frame,
                    #[cfg(feature = "debugger")]
                    debug,
                );
                continue 'frames;
            }
            Ok(FrameOutcome::Complete(value)) => value,
            Err(e) => match unwind(
                e,
                frames,
                root_chunks,
                operand_stack,
                execution,
                #[cfg(feature = "debugger")]
                debug,
            ) {
                Ok(()) => continue 'frames,
                Err((e, locals)) => return (Err(e), locals),
            },
        };

        if frames.len() == 1 {
            let finished = frames.pop().expect("just checked len() == 1");
            return (Ok(value), finished.locals);
        }
        let continuation = execution
            .limits
            .pop_frame(
                frames,
                #[cfg(feature = "debugger")]
                debug,
            )
            .expect("just checked len() > 1");
        match continuation {
            Continuation::Push | Continuation::TryBody(_) => {
                operand_stack.push(value);
            }
            Continuation::ResumeBindParams(pending) => {
                let next = match apply_pending(*pending, value, execution) {
                    Ok(next) => next,
                    Err(e) => {
                        let e = locate_at_top(frames, root_chunks, e);
                        match unwind(
                            e,
                            frames,
                            root_chunks,
                            operand_stack,
                            execution,
                            #[cfg(feature = "debugger")]
                            debug,
                        ) {
                            Ok(()) => continue 'frames,
                            Err((e, locals)) => return (Err(e), locals),
                        }
                    }
                };
                let mut next = next;
                next.stack_base = operand_stack.len();
                if let Err(e) = execution.limits.push_frame(
                    frames,
                    next,
                    #[cfg(feature = "debugger")]
                    debug,
                ) {
                    let e = locate_at_top(frames, root_chunks, e);
                    match unwind(
                        e,
                        frames,
                        root_chunks,
                        operand_stack,
                        execution,
                        #[cfg(feature = "debugger")]
                        debug,
                    ) {
                        Ok(()) => continue 'frames,
                        Err((e, locals)) => return (Err(e), locals),
                    }
                }
            }
        }
    }
}

/// `locate`s `e` at the still-suspended calling frame's own chunk/`ip`.
fn locate_at_top(frames: &[Frame], root_chunks: &Shared<Vec<Chunk>>, e: VmError) -> VmError {
    let caller = frames.last().expect("the frame stack is never empty here");
    let chunks = caller.chunks.as_ref().unwrap_or(root_chunks);
    locate(&chunks[caller.chunk_index as usize], caller.ip, e)
}

/// Pops frames until a `try` body catches `e` (`Ok(())`) or the stack empties (`Err`).
#[allow(
    clippy::ptr_arg,
    reason = "unwinding may truncate and push onto the shared operand stack"
)]
fn unwind(
    mut e: VmError,
    frames: &mut Vec<Frame>,
    root_chunks: &Shared<Vec<Chunk>>,
    operand_stack: &mut Vec<StackValue>,
    execution: &mut ExecutionContext<'_>,
    #[cfg(feature = "debugger")] debug: &mut DebugRuntime<'_>,
) -> Result<(), (VmError, Locals)> {
    loop {
        if frames.len() == 1 {
            let finished = frames.pop().expect("just checked len() == 1");
            return Err((e, finished.locals));
        }
        let continuation = execution
            .limits
            .pop_frame(
                frames,
                #[cfg(feature = "debugger")]
                debug,
            )
            .expect("just checked len() > 1");
        match continuation {
            Continuation::Push => continue,
            Continuation::ResumeBindParams(pending) => {
                execution.limits.recycle_pending_locals(*pending);
                continue;
            }
            Continuation::TryBody(body) => {
                let TryBody {
                    catch_closure,
                    has_binder,
                    break_acc_slot,
                    break_offset,
                    continue_offset,
                } = *body;
                if let Some(value) = flow_break_value(&e) {
                    let (Some(acc_slot), Some(offset)) = (break_acc_slot, break_offset) else {
                        continue;
                    };
                    let parent = frames.last_mut().expect("just checked len() > 1");
                    if let Some(value) = value {
                        parent.locals.set(acc_slot, StackValue::Value(value));
                    }
                    parent.ip = (parent.ip as i64 + offset as i64) as usize;
                    return Ok(());
                }
                if flow_continue(&e) {
                    let Some(offset) = continue_offset else {
                        continue;
                    };
                    let parent = frames.last_mut().expect("just checked len() > 1");
                    parent.ip = (parent.ip as i64 + offset as i64) as usize;
                    return Ok(());
                }
                let parent = frames.last().expect("just checked len() > 1");
                let catch_chunks = parent.chunks.as_ref().unwrap_or(root_chunks);
                let catch_chunk = &catch_chunks[catch_closure.chunk_index as usize];
                let mut catch_locals = execution
                    .limits
                    .take_locals(catch_chunk.local_count, catch_chunk.captured_local_slots());
                catch_locals.set(SELF_SLOT, parent.locals.get(SELF_SLOT));
                if has_binder {
                    catch_locals.set(1, StackValue::Value(error_dict(&e)));
                }
                let mut catch_frame = Frame::new(
                    catch_closure.chunk_index,
                    parent.chunks.clone(),
                    catch_locals,
                    catch_closure.upvalues.clone(),
                    !catch_chunk.captures_local_slots(),
                    Continuation::Push,
                );
                catch_frame.stack_base = operand_stack.len();
                match execution.limits.push_frame(
                    frames,
                    catch_frame,
                    #[cfg(feature = "debugger")]
                    debug,
                ) {
                    Ok(()) => return Ok(()),
                    Err(new_e) => {
                        e = locate_at_top(&*frames, root_chunks, new_e);
                        continue;
                    }
                }
            }
        }
    }
}

fn run_frame_slice<const CHECK_TIMEOUT: bool>(
    frame: &mut Frame,
    root_chunks: &Shared<Vec<Chunk>>,
    stack: &mut Vec<StackValue>,
    execution: &mut ExecutionContext<'_>,
    #[cfg(feature = "debugger")] debug: &mut DebugRuntime<'_>,
) -> VmResult<FrameOutcome> {
    let chunks = frame.chunks.as_ref().unwrap_or(root_chunks);
    let chunk = &chunks[frame.chunk_index as usize];
    let locals = &mut frame.locals;
    let upvalues = frame.upvalues.as_deref().map_or_else(|| &[][..], Vec::as_slice);
    let mut ip = frame.ip;

    macro_rules! pop {
        () => {{
            if stack.len() <= frame.stack_base {
                return Err(locate(chunk, ip, VmError::Corrupt("stack underflow")));
            }
            // SAFETY: the length check above proves the stack is non-empty.
            unsafe { stack.pop().unwrap_unchecked() }
        }};
    }
    macro_rules! pop_value {
        () => {{ into_runtime_value(pop!(), chunks) }};
    }
    macro_rules! bail {
        ($e:expr) => {
            return Err(locate(chunk, ip, $e))
        };
    }

    let mut tail_call_candidate = false;
    let outcome = 'dispatch: loop {
        if ip >= chunk.code.len() {
            let value = if stack.len() > frame.stack_base {
                // SAFETY: the length check above proves the stack is non-empty.
                unsafe { stack.pop().unwrap_unchecked() }
            } else {
                StackValue::Value(RuntimeValue::None)
            };
            break 'dispatch FrameOutcome::Complete(value);
        }
        if CHECK_TIMEOUT {
            execution.limits.check().map_err(|e| locate(chunk, ip, e))?;
        }
        let op = &chunk.code[ip];
        // Only `CallSelf` is eligible today. Other call instructions can enter a
        // default-parameter binder or carry call-depth behavior that must remain observable.
        tail_call_candidate = matches!(
            op,
            OpCode::CallSelf(..)
                | OpCode::CallSelfExact(..)
                | OpCode::CallSelfExact0
                | OpCode::CallSelfExact1
                | OpCode::CallSelfExact2
                | OpCode::CallSelfImplicitSelf(..)
        );
        ip += 1;

        match op {
            #[cfg(feature = "debugger")]
            OpCode::StmtBoundary(token_id) => {
                let node = chunk
                    .debug_nodes
                    .iter()
                    .rfind(|(candidate, _)| *candidate == *token_id)
                    .map(|(_, node)| Shared::clone(node))
                    .ok_or_else(|| locate(chunk, ip, VmError::Corrupt("missing debug node")))?;
                debug.current_node = Some(Shared::clone(&node));

                let DebugBindings { bindings, vm_frame } = debug_bindings(chunk, locals, upvalues, chunks)
                    .ok_or_else(|| locate(chunk, ip, VmError::Corrupt("debug slot out of bounds")))?;
                if let Some(hook) = debug.hook.as_deref_mut() {
                    hook.on_boundary(DebugEvent {
                        token_id: *token_id,
                        node,
                        current_value: current_self(locals, chunks),
                        bindings,
                        vm_frame: vm_frame.clone(),
                        call_stack: debug.call_stack.clone(),
                        #[cfg(feature = "debug-trace")]
                        operand_stack: stack
                            .iter()
                            .cloned()
                            .map(|value| into_runtime_value(value, chunks))
                            .collect(),
                    })
                    .map_err(|error| locate(chunk, ip, error))?;
                }
                apply_debug_updates(&vm_frame, locals, upvalues);
            }
            #[cfg(feature = "debugger")]
            OpCode::Breakpoint(token_id) => {
                let node = chunk
                    .debug_nodes
                    .iter()
                    .rfind(|(candidate, _)| *candidate == *token_id)
                    .map(|(_, node)| Shared::clone(node))
                    .ok_or_else(|| locate(chunk, ip, VmError::Corrupt("missing debug node")))?;
                debug.current_node = Some(Shared::clone(&node));

                let DebugBindings { bindings, vm_frame } = debug_bindings(chunk, locals, upvalues, chunks)
                    .ok_or_else(|| locate(chunk, ip, VmError::Corrupt("debug slot out of bounds")))?;
                if let Some(hook) = debug.hook.as_deref_mut() {
                    hook.on_explicit_breakpoint(DebugEvent {
                        token_id: *token_id,
                        node,
                        current_value: current_self(locals, chunks),
                        bindings,
                        vm_frame: vm_frame.clone(),
                        call_stack: debug.call_stack.clone(),
                        #[cfg(feature = "debug-trace")]
                        operand_stack: stack
                            .iter()
                            .cloned()
                            .map(|value| into_runtime_value(value, chunks))
                            .collect(),
                    })
                    .map_err(|error| locate(chunk, ip, error))?;
                }
                apply_debug_updates(&vm_frame, locals, upvalues);
            }
            OpCode::Const(idx) => {
                // SAFETY: `verify_chunks` validates every constant index before execution.
                let value = unsafe { chunk.constants.get_unchecked(*idx as usize) }.clone();
                stack.push(StackValue::Value(value));
            }
            OpCode::PushNone => stack.push(StackValue::Value(RuntimeValue::None)),
            OpCode::GetLocal(slot) => {
                // SAFETY: `verify_chunks` validates every local slot before execution.
                stack.push(unsafe { locals.get_unchecked(*slot) });
            }
            OpCode::SetLocal(slot) => {
                let v = pop!();
                // SAFETY: `verify_chunks` validates every local slot before execution.
                unsafe { locals.set_unchecked(*slot, v) };
            }
            OpCode::TeeLocal(slot) => {
                let top = stack
                    .last()
                    .ok_or_else(|| locate(chunk, ip, VmError::Corrupt("stack underflow in TeeLocal")))?
                    .clone();
                // SAFETY: `verify_chunks` validates every local slot before execution.
                unsafe { locals.set_unchecked(*slot, top) };
            }
            OpCode::CopyLocal { source, destination } => {
                // SAFETY: `verify_chunks` validates both local slots before execution.
                let value = unsafe { locals.get_unchecked(*source) };
                // SAFETY: `verify_chunks` validates both local slots before execution.
                unsafe { locals.set_unchecked(*destination, value) };
            }
            OpCode::GetUpvalue(idx) => stack.push(read_cell(&upvalues[*idx as usize])),
            OpCode::SetUpvalue(idx) => {
                let v = pop!();
                write_cell(&upvalues[*idx as usize], v);
            }
            OpCode::MakeClosure(payload) => {
                let (target_chunk, sources) = payload.as_ref();
                let captured = capture_upvalues(sources, locals, upvalues);
                stack.push(StackValue::Closure(Shared::new(Closure {
                    chunk_index: *target_chunk,
                    upvalues: (!captured.is_empty()).then(|| Shared::new(captured)),
                })));
            }
            OpCode::MakeStaticClosure(index) => {
                let closure = chunk
                    .static_closures
                    .get(*index as usize)
                    .ok_or_else(|| locate(chunk, ip, VmError::Corrupt("static closure index out of bounds")))?;
                stack.push(StackValue::Closure(Shared::clone(closure)));
            }
            OpCode::Pop => {
                pop!();
            }
            OpCode::Dup => {
                let top = stack
                    .last()
                    .ok_or_else(|| locate(chunk, ip, VmError::Corrupt("stack underflow in Dup")))?
                    .clone();
                stack.push(top);
            }
            OpCode::Jump(offset) => {
                ip = (ip as i64 + *offset as i64) as usize;
            }
            OpCode::JumpIfFalse(offset) => {
                let cond = pop_value!();
                if !cond.is_truthy() {
                    ip = (ip as i64 + *offset as i64) as usize;
                }
            }
            OpCode::Add | OpCode::Sub | OpCode::Mul | OpCode::Div | OpCode::Mod => {
                let b = pop_value!();
                let a = pop_value!();
                let Some(operation) = binary_op_from_opcode(op) else {
                    bail!(VmError::Corrupt("missing arithmetic binary operation"));
                };
                stack.push(StackValue::Value(
                    binop(operation, a, b, locals, chunks, execution.env, execution.host_functions)
                        .map_err(|e| locate(chunk, ip, e))?,
                ));
            }
            OpCode::Eq | OpCode::Ne | OpCode::Lt | OpCode::Le | OpCode::Gt | OpCode::Ge => {
                let b = pop_value!();
                let a = pop_value!();
                let Some(operation) = binary_op_from_opcode(op) else {
                    bail!(VmError::Corrupt("missing comparison binary operation"));
                };
                stack.push(StackValue::Value(
                    cmp_op(operation, a, b, locals, chunks, execution.env, execution.host_functions)
                        .map_err(|e| locate(chunk, ip, e))?,
                ));
            }
            OpCode::BinaryLocalLocal { op, left, right } => {
                let a = local_runtime_value(locals, *left, chunks)?;
                let b = local_runtime_value(locals, *right, chunks)?;
                stack.push(StackValue::Value(
                    eval_binary_op(*op, a, b, locals, chunks, execution.env, execution.host_functions)
                        .map_err(|e| locate(chunk, ip, e))?,
                ));
            }
            OpCode::BinaryLocalConst { op, local, constant } => {
                let a = local_runtime_value(locals, *local, chunks)?;
                // SAFETY: `verify_chunks` validates every constant index before execution.
                let b = unsafe { chunk.constants.get_unchecked(*constant as usize) }.clone();
                stack.push(StackValue::Value(
                    eval_binary_op(*op, a, b, locals, chunks, execution.env, execution.host_functions)
                        .map_err(|e| locate(chunk, ip, e))?,
                ));
            }
            OpCode::Neg => {
                let a = pop_value!();
                stack.push(StackValue::Value(match a {
                    RuntimeValue::Number(n) => RuntimeValue::Number(Number::new(-n.value())),
                    other => call_builtin(
                        negate_ident(),
                        &[other],
                        &current_self(locals, chunks),
                        execution.env,
                        execution.host_functions,
                    )
                    .map_err(|e| locate(chunk, ip, e))?,
                }));
            }
            OpCode::Not => {
                let value = pop_value!();
                stack.push(StackValue::Value(RuntimeValue::Boolean(!value.is_truthy())));
            }
            OpCode::ArrayNew => {
                stack.push(StackValue::Value(RuntimeValue::empty_array()));
            }
            OpCode::ArrayPush | OpCode::ToForeachIterable | OpCode::ArrayLen | OpCode::ArrayGetAt => {
                array_misc_op(op, stack, chunks, chunk, ip)?;
            }
            OpCode::ArraySpread => {
                let source = pop_value!();
                let arr = pop_value!();
                stack.push(StackValue::Value(array_spread(arr, source, chunk, ip)?));
            }
            OpCode::DictSpread => {
                let source = pop_value!();
                let arr = pop_value!();
                stack.push(StackValue::Value(dict_spread(arr, source, chunk, ip)?));
            }
            OpCode::ArrayLenLocal(slot) => {
                let value = local_runtime_value(locals, *slot, chunks)?;
                let result = match value {
                    RuntimeValue::Array(array) => RuntimeValue::Number(Number::new(array.len() as f64)),
                    value => call_builtin(
                        &LEN_IDENT,
                        &[value],
                        &current_self(locals, chunks),
                        execution.env,
                        execution.host_functions,
                    )
                    .map_err(|e| locate(chunk, ip, e))?,
                };
                stack.push(StackValue::Value(result));
            }
            OpCode::ArrayGetLocalAt { array_slot, index_slot } => {
                let array = local_runtime_value(locals, *array_slot, chunks)?;
                let index = local_runtime_value(locals, *index_slot, chunks)?;
                let result = match (array, index) {
                    (RuntimeValue::Array(array), RuntimeValue::Number(index)) => {
                        let len = array.len();
                        let index = index.value() as isize;
                        let index = if index < 0 {
                            (len as isize + index).max(0) as usize
                        } else {
                            index as usize
                        };
                        array.get(index).cloned().unwrap_or(RuntimeValue::None)
                    }
                    (array, index) => call_builtin(
                        &GET_IDENT,
                        &[array, index],
                        &current_self(locals, chunks),
                        execution.env,
                        execution.host_functions,
                    )
                    .map_err(|e| locate(chunk, ip, e))?,
                };
                stack.push(StackValue::Value(result));
            }
            OpCode::DictGetLocalOrFail {
                subject_slot,
                key,
                value_slot,
            } => {
                let subject = local_runtime_value(locals, *subject_slot, chunks)?;
                let found = match subject {
                    RuntimeValue::Dict(map) => map.get(key).cloned(),
                    _ => None,
                };
                match found {
                    Some(value) => {
                        // SAFETY: `verify_chunks` validates every local slot before execution.
                        unsafe { locals.set_unchecked(*value_slot, StackValue::Value(value)) };
                        stack.push(StackValue::Value(RuntimeValue::Boolean(true)));
                    }
                    None => stack.push(StackValue::Value(RuntimeValue::Boolean(false))),
                }
            }
            OpCode::ForeachNext {
                array_slot,
                index_slot,
                value_slot,
                exit_offset,
            } => {
                // SAFETY: `verify_chunks` validates every local slot before execution.
                let value = unsafe { locals.foreach_next(*array_slot, *index_slot, *value_slot, SELF_SLOT) }
                    .map_err(|e| locate(chunk, ip, VmError::Corrupt(e)))?;
                if value.is_none() {
                    ip = (ip as i64 + *exit_offset as i64) as usize;
                    continue;
                }
            }
            OpCode::ForeachCollect(slot) => {
                let value = pop_value!();
                locals
                    .append_to_array_at(*slot, value)
                    .map_err(|e| locate(chunk, ip, VmError::Corrupt(e)))?;
            }
            OpCode::ArraySliceFrom => {
                array_misc_op(op, stack, chunks, chunk, ip)?;
            }
            OpCode::TypeCheck(type_name) => {
                let v = pop_value!();
                let type_str = type_name.as_str();
                let matches = type_check(&v, type_str.as_str());
                stack.push(StackValue::Value(RuntimeValue::Boolean(matches)));
            }
            OpCode::SelectorMatch(_) | OpCode::SelectorMatchWithArgs(_) => {
                selector_op(op, stack, chunks, chunk, ip)?;
            }
            OpCode::SelectorMatchKind(kind) => {
                let subject = pop_value!();
                stack.push(StackValue::Value(eval_compact_selector_expr(
                    &subject,
                    kind.as_selector(),
                )));
            }
            OpCode::SelectorMatchHeading(level) => {
                let subject = pop_value!();
                stack.push(StackValue::Value(eval_compact_selector_expr(
                    &subject,
                    Selector::Heading((*level != 0).then_some(*level)),
                )));
            }
            OpCode::GetEnvVar(name_idx) => {
                // SAFETY: `verify_chunks` validates every constant index before execution.
                let RuntimeValue::String(name) = (unsafe { chunk.constants.get_unchecked(*name_idx as usize) }) else {
                    bail!(VmError::Corrupt("GetEnvVar constant is not a string"));
                };
                let value = builtin::io_context::current()
                    .env_var(name)
                    .map_err(|_| locate(chunk, ip, VmError::EnvNotFound(name.to_string())))?;
                stack.push(StackValue::Value(RuntimeValue::String(value.into())));
            }
            OpCode::GetExternalGlobal(ident) => {
                stack.push(StackValue::Value(get_external_global(
                    *ident,
                    chunk,
                    ip,
                    execution.env,
                )?));
            }
            OpCode::InterpString(n) => {
                let start = stack
                    .len()
                    .checked_sub(*n as usize)
                    .ok_or_else(|| locate(chunk, ip, VmError::Corrupt("stack underflow in InterpString")))?;
                if start < frame.stack_base {
                    bail!(VmError::Corrupt("stack underflow in InterpString"));
                }
                let value = interp_string(&stack[start..], chunks);
                stack.truncate(start);
                stack.push(StackValue::Value(value));
            }
            OpCode::CallBuiltin(ident, argc) => {
                let mut args = Args::with_capacity(*argc as usize);
                for _ in 0..*argc {
                    args.push(pop_value!());
                }
                args.reverse();
                let result = call_builtin_args(
                    ident,
                    args,
                    &current_self(locals, chunks),
                    execution.env,
                    execution.host_functions,
                )
                .map_err(|e| locate(chunk, ip, e))?;
                stack.push(StackValue::Value(result));
            }
            OpCode::CallStatic(chunk_index, argc) => {
                let new_frame = call_static_chunk_from_stack(
                    *chunk_index,
                    *argc,
                    stack,
                    CallSite {
                        locals,
                        chunk,
                        ip,
                        frame_chunks: frame.chunks.clone(),
                    },
                    chunks,
                    execution,
                )?;
                break 'dispatch FrameOutcome::Enter(new_frame);
            }
            OpCode::CallStaticExact0(target) => {
                let new_frame = call_exact_fixed_chunk_0(
                    ExactCallTarget {
                        chunk_index: target.chunk_index,
                        local_count: target.local_count,
                        captured_local_slots: &[],
                    },
                    None,
                    CallSite {
                        locals,
                        chunk,
                        ip,
                        frame_chunks: frame.chunks.clone(),
                    },
                    execution,
                );
                break 'dispatch FrameOutcome::Enter(new_frame);
            }
            OpCode::CallStaticExact1(target) => {
                let new_frame = call_exact_fixed_chunk_1(
                    ExactCallTarget {
                        chunk_index: target.chunk_index,
                        local_count: target.local_count,
                        captured_local_slots: &[],
                    },
                    None,
                    stack,
                    CallSite {
                        locals,
                        chunk,
                        ip,
                        frame_chunks: frame.chunks.clone(),
                    },
                    execution,
                )?;
                break 'dispatch FrameOutcome::Enter(new_frame);
            }
            OpCode::CallStaticExact2(target) => {
                let new_frame = call_exact_fixed_chunk_2(
                    ExactCallTarget {
                        chunk_index: target.chunk_index,
                        local_count: target.local_count,
                        captured_local_slots: &[],
                    },
                    None,
                    stack,
                    CallSite {
                        locals,
                        chunk,
                        ip,
                        frame_chunks: frame.chunks.clone(),
                    },
                    execution,
                )?;
                break 'dispatch FrameOutcome::Enter(new_frame);
            }
            OpCode::CallStaticExact(chunk_index, argc) | OpCode::CallStaticImplicitSelf(chunk_index, argc) => {
                let new_frame = call_known_fixed_chunk_from_stack(
                    KnownFixedChunkCall {
                        chunk_index: *chunk_index,
                        upvalues: None,
                        argc: *argc,
                        uses_implicit_self: matches!(op, OpCode::CallStaticImplicitSelf(..)),
                        remove_callee: false,
                    },
                    stack,
                    CallSite {
                        locals,
                        chunk,
                        ip,
                        frame_chunks: frame.chunks.clone(),
                    },
                    chunks,
                    execution,
                )?;
                break 'dispatch FrameOutcome::Enter(new_frame);
            }
            OpCode::CallSelf(argc) => {
                let new_frame = call_self_chunk_from_stack(
                    frame.chunk_index,
                    frame.upvalues.clone(),
                    *argc,
                    stack,
                    CallSite {
                        locals,
                        chunk,
                        ip,
                        frame_chunks: frame.chunks.clone(),
                    },
                    chunks,
                    execution,
                )?;
                break 'dispatch FrameOutcome::Enter(new_frame);
            }
            OpCode::CallSelfExact0 => {
                let new_frame = call_exact_fixed_chunk_0(
                    ExactCallTarget {
                        chunk_index: frame.chunk_index,
                        local_count: chunk.local_count,
                        captured_local_slots: chunk.captured_local_slots(),
                    },
                    frame.upvalues.clone(),
                    CallSite {
                        locals,
                        chunk,
                        ip,
                        frame_chunks: frame.chunks.clone(),
                    },
                    execution,
                );
                break 'dispatch FrameOutcome::Enter(new_frame);
            }
            OpCode::CallSelfExact1 => {
                let new_frame = call_exact_fixed_chunk_1(
                    ExactCallTarget {
                        chunk_index: frame.chunk_index,
                        local_count: chunk.local_count,
                        captured_local_slots: chunk.captured_local_slots(),
                    },
                    frame.upvalues.clone(),
                    stack,
                    CallSite {
                        locals,
                        chunk,
                        ip,
                        frame_chunks: frame.chunks.clone(),
                    },
                    execution,
                )?;
                break 'dispatch FrameOutcome::Enter(new_frame);
            }
            OpCode::CallSelfExact2 => {
                let new_frame = call_exact_fixed_chunk_2(
                    ExactCallTarget {
                        chunk_index: frame.chunk_index,
                        local_count: chunk.local_count,
                        captured_local_slots: chunk.captured_local_slots(),
                    },
                    frame.upvalues.clone(),
                    stack,
                    CallSite {
                        locals,
                        chunk,
                        ip,
                        frame_chunks: frame.chunks.clone(),
                    },
                    execution,
                )?;
                break 'dispatch FrameOutcome::Enter(new_frame);
            }
            OpCode::CallSelfExact(argc) | OpCode::CallSelfImplicitSelf(argc) => {
                let new_frame = call_known_fixed_chunk_from_stack(
                    KnownFixedChunkCall {
                        chunk_index: frame.chunk_index,
                        upvalues: frame.upvalues.clone(),
                        argc: *argc,
                        uses_implicit_self: matches!(op, OpCode::CallSelfImplicitSelf(..)),
                        remove_callee: false,
                    },
                    stack,
                    CallSite {
                        locals,
                        chunk,
                        ip,
                        frame_chunks: frame.chunks.clone(),
                    },
                    chunks,
                    execution,
                )?;
                break 'dispatch FrameOutcome::Enter(new_frame);
            }
            OpCode::CallLocal(slot, argc) => {
                let callee = locals.get(*slot);
                if let StackValue::Closure(closure) = &callee
                    && chunks[closure.chunk_index as usize]
                        .param_shape
                        .fixed_required_arity()
                        .is_some()
                {
                    let new_frame = call_fixed_closure_from_stack(
                        FixedClosureCall {
                            closure,
                            argc: *argc,
                            remove_callee: false,
                        },
                        stack,
                        CallSite {
                            locals,
                            chunk,
                            ip,
                            frame_chunks: frame.chunks.clone(),
                        },
                        chunks,
                        execution,
                    )?;
                    break 'dispatch FrameOutcome::Enter(new_frame);
                }
                // Pooled, not `Vec::with_capacity`: this path (non-fixed-arity callees —
                // variadic/optional params, `partial`-bound closures) runs often enough in
                // higher-order builtins that a fresh heap allocation per call is worth avoiding.
                let mut args = execution.limits.take_stack();
                for _ in 0..*argc {
                    args.push(pop!());
                }
                args.reverse();
                let step = call_stack_value(
                    callee,
                    &mut args,
                    CallSite {
                        locals,
                        chunk,
                        ip,
                        frame_chunks: frame.chunks.clone(),
                    },
                    chunks,
                    execution,
                );
                execution.limits.recycle_stack(args);
                match step? {
                    CallStep::Value(v) => stack.push(v),
                    CallStep::Enter(new_frame) => break 'dispatch FrameOutcome::Enter(new_frame),
                }
            }
            OpCode::CallUpvalue(index, argc) => {
                // SAFETY: `verify_chunks` validates every upvalue index before execution.
                let callee = read_cell(unsafe { upvalues.get_unchecked(*index as usize) });
                if let StackValue::Closure(closure) = &callee
                    && chunks[closure.chunk_index as usize]
                        .param_shape
                        .fixed_required_arity()
                        .is_some()
                {
                    let new_frame = call_fixed_closure_from_stack(
                        FixedClosureCall {
                            closure,
                            argc: *argc,
                            remove_callee: false,
                        },
                        stack,
                        CallSite {
                            locals,
                            chunk,
                            ip,
                            frame_chunks: frame.chunks.clone(),
                        },
                        chunks,
                        execution,
                    )?;
                    break 'dispatch FrameOutcome::Enter(new_frame);
                }
                let mut args = execution.limits.take_stack();
                for _ in 0..*argc {
                    args.push(pop!());
                }
                args.reverse();
                let step = call_stack_value(
                    callee,
                    &mut args,
                    CallSite {
                        locals,
                        chunk,
                        ip,
                        frame_chunks: frame.chunks.clone(),
                    },
                    chunks,
                    execution,
                );
                execution.limits.recycle_stack(args);
                match step? {
                    CallStep::Value(v) => stack.push(v),
                    CallStep::Enter(new_frame) => break 'dispatch FrameOutcome::Enter(new_frame),
                }
            }
            OpCode::CallValue(argc) => {
                let callee_index = stack
                    .len()
                    .checked_sub(*argc as usize + 1)
                    .ok_or_else(|| locate(chunk, ip, VmError::Corrupt("stack underflow in CallValue")))?;
                if callee_index < frame.stack_base {
                    bail!(VmError::Corrupt("stack underflow in CallValue"));
                }
                if let StackValue::Closure(closure) = &stack[callee_index]
                    && chunks[closure.chunk_index as usize]
                        .param_shape
                        .fixed_required_arity()
                        .is_some()
                {
                    // Keep the callee below its arguments until the fixed-call binder has
                    // popped them. This avoids `Vec::remove(callee_index)`, which shifts
                    // every argument and is especially costly for large calls.
                    let closure = Shared::clone(closure);
                    let new_frame = call_fixed_closure_from_stack(
                        FixedClosureCall {
                            closure: &closure,
                            argc: *argc,
                            remove_callee: true,
                        },
                        stack,
                        CallSite {
                            locals,
                            chunk,
                            ip,
                            frame_chunks: frame.chunks.clone(),
                        },
                        chunks,
                        execution,
                    )?;
                    break 'dispatch FrameOutcome::Enter(new_frame);
                }
                // See the `CallLocal` non-fixed-arity path above for why this is pooled.
                let mut args = execution.limits.take_stack();
                for _ in 0..*argc {
                    args.push(pop!());
                }
                args.reverse();
                let callee = pop!();
                let step = call_stack_value(
                    callee,
                    &mut args,
                    CallSite {
                        locals,
                        chunk,
                        ip,
                        frame_chunks: frame.chunks.clone(),
                    },
                    chunks,
                    execution,
                );
                execution.limits.recycle_stack(args);
                match step? {
                    CallStep::Value(v) => stack.push(v),
                    CallStep::Enter(new_frame) => break 'dispatch FrameOutcome::Enter(new_frame),
                }
            }
            OpCode::MaybeAutoCall => {
                let value = pop!();
                let eligible = match &value {
                    StackValue::Closure(closure) => chunks[closure.chunk_index as usize].param_shape.required <= 1,
                    StackValue::Value(RuntimeValue::VmClosure(vc)) => {
                        vc.chunks[vc.chunk_index as usize]
                            .param_shape
                            .required
                            .saturating_sub(vc.bound_args.len())
                            <= 1
                    }
                    StackValue::Value(RuntimeValue::NativeFunction(ident)) => builtin::get_builtin_functions(ident)
                        .is_some_and(|f| f.num_params.is_valid(0) || f.num_params.is_missing_one_params(0)),
                    _ => false,
                };
                if eligible {
                    match call_stack_value(
                        value,
                        &mut Vec::new(),
                        CallSite {
                            locals,
                            chunk,
                            ip,
                            frame_chunks: frame.chunks.clone(),
                        },
                        chunks,
                        execution,
                    )? {
                        CallStep::Value(v) => stack.push(v),
                        CallStep::Enter(new_frame) => break 'dispatch FrameOutcome::Enter(new_frame),
                    }
                } else {
                    stack.push(value);
                }
            }
            OpCode::TryCatch(info) => {
                let catch_closure = pop!();
                let try_closure = pop!();
                let new_frame = begin_try_catch(
                    info,
                    catch_closure,
                    try_closure,
                    CallSite {
                        locals,
                        chunk,
                        ip,
                        frame_chunks: frame.chunks.clone(),
                    },
                    chunks,
                    execution,
                )?;
                break 'dispatch FrameOutcome::Enter(new_frame);
            }
            OpCode::FlowBreak(has_value) => {
                let value = if *has_value { Some(pop_value!()) } else { None };
                bail!(VmError::FlowBreak(value));
            }
            OpCode::FlowContinue => bail!(VmError::FlowContinue),
            OpCode::RaiseDestructuringFailed => {
                bail!(VmError::DestructuringFailed);
            }
            OpCode::Return => {
                let v = pop!();
                break 'dispatch FrameOutcome::Complete(v);
            }
        }
    };
    let outcome = if tail_call_candidate
        && matches!(outcome, FrameOutcome::Enter(_))
        && matches!(chunk.code.get(ip), Some(OpCode::Return))
    {
        let FrameOutcome::Enter(frame) = outcome else {
            unreachable!("guard above requires an entering call frame");
        };
        FrameOutcome::TailEnter(frame)
    } else {
        outcome
    };
    frame.ip = ip;
    if let FrameOutcome::Complete(_) = &outcome {
        stack.truncate(frame.stack_base);
    }
    Ok(outcome)
}

/// `try`/`catch` is rare; kept out of `run_frame_slice`. Builds the try body's `Frame` — `unwind`
/// handles the rest (routing success, or dispatching to `catch`/a loop jump on error).
#[cold]
#[inline(never)]
fn begin_try_catch(
    info: &TryCatchInfo,
    catch_closure: StackValue,
    try_closure: StackValue,
    call_site: CallSite<'_>,
    chunks: &Shared<Vec<Chunk>>,
    execution: &mut ExecutionContext<'_>,
) -> VmResult<Frame> {
    let CallSite {
        locals,
        chunk,
        ip,
        frame_chunks,
    } = call_site;
    let StackValue::Closure(catch_closure) = catch_closure else {
        return Err(locate(
            chunk,
            ip,
            VmError::Corrupt("TryCatch catch operand is not a closure"),
        ));
    };
    let StackValue::Closure(try_closure) = try_closure else {
        return Err(locate(
            chunk,
            ip,
            VmError::Corrupt("TryCatch try operand is not a closure"),
        ));
    };
    let try_chunk = &chunks[try_closure.chunk_index as usize];
    let mut try_locals = execution
        .limits
        .take_locals(try_chunk.local_count, try_chunk.captured_local_slots());
    try_locals.set(SELF_SLOT, locals.get(SELF_SLOT));
    Ok(Frame::new(
        try_closure.chunk_index,
        frame_chunks,
        try_locals,
        try_closure.upvalues.clone(),
        !try_chunk.captures_local_slots(),
        Continuation::TryBody(Box::new(TryBody {
            catch_closure,
            has_binder: info.has_binder,
            break_acc_slot: info.break_acc_slot,
            break_offset: info.break_offset,
            continue_offset: info.continue_offset,
        })),
    ))
}

/// Rare spread-syntax opcodes, kept out of `run_frame_slice`.
#[cold]
#[inline(never)]
fn array_spread(mut arr: RuntimeValue, source: RuntimeValue, chunk: &Chunk, ip: usize) -> VmResult<RuntimeValue> {
    match source {
        RuntimeValue::Array(source) => {
            let RuntimeValue::Array(array) = &mut arr else {
                return Err(locate(
                    chunk,
                    ip,
                    VmError::Corrupt("ArraySpread accumulator is not an array"),
                ));
            };
            runtime_value::array_mut(array).extend(Shared::unwrap_or_clone(source));
        }
        RuntimeValue::None => {}
        other => {
            return Err(locate(
                chunk,
                ip,
                VmError::Builtin(builtin::Error::InvalidTypes(builtins::ARRAY.to_string(), vec![other])),
            ));
        }
    }
    Ok(arr)
}

#[cold]
#[inline(never)]
fn dict_spread(mut arr: RuntimeValue, source: RuntimeValue, chunk: &Chunk, ip: usize) -> VmResult<RuntimeValue> {
    match source {
        RuntimeValue::Dict(map) => {
            let RuntimeValue::Array(array) = &mut arr else {
                return Err(locate(
                    chunk,
                    ip,
                    VmError::Corrupt("DictSpread accumulator is not an array"),
                ));
            };
            runtime_value::array_mut(array).extend(
                Shared::unwrap_or_clone(map)
                    .into_iter()
                    .map(|(k, v)| RuntimeValue::Array(Shared::new(vec![RuntimeValue::Symbol(k), v]))),
            );
        }
        RuntimeValue::None => {}
        other => {
            return Err(locate(
                chunk,
                ip,
                VmError::Builtin(builtin::Error::InvalidTypes(builtins::DICT.to_string(), vec![other])),
            ));
        }
    }
    Ok(arr)
}

/// Standalone equivalent of the `pop_value!` macro, for the cold handlers below.
fn pop_value_from(
    stack: &mut Vec<StackValue>,
    chunks: &Shared<Vec<Chunk>>,
    chunk: &Chunk,
    ip: usize,
) -> VmResult<RuntimeValue> {
    let v = stack
        .pop()
        .ok_or_else(|| locate(chunk, ip, VmError::Corrupt("stack underflow")))?;
    Ok(into_runtime_value(v, chunks))
}

/// Rare array opcodes, kept out of `run_frame_slice`.
#[cold]
#[inline(never)]
fn array_misc_op(
    op: &OpCode,
    stack: &mut Vec<StackValue>,
    chunks: &Shared<Vec<Chunk>>,
    chunk: &Chunk,
    ip: usize,
) -> VmResult<()> {
    match op {
        OpCode::ArrayPush => {
            let elem = pop_value_from(stack, chunks, chunk, ip)?;
            let mut arr = pop_value_from(stack, chunks, chunk, ip)?;
            let RuntimeValue::Array(array) = &mut arr else {
                return Err(locate(chunk, ip, VmError::Corrupt("ArrayPush on a non-array")));
            };
            runtime_value::array_mut(array).push(elem);
            stack.push(StackValue::Value(arr));
        }
        OpCode::ToForeachIterable => {
            let v = pop_value_from(stack, chunks, chunk, ip)?;
            let normalized = match v {
                array @ RuntimeValue::Array(_) => array,
                RuntimeValue::String(s) => RuntimeValue::Array(Shared::new(
                    s.chars()
                        .map(|c| RuntimeValue::String(Shared::new(c.to_string())))
                        .collect(),
                )),
                other => return Err(locate(chunk, ip, VmError::InvalidForeachTarget(other.to_string()))),
            };
            stack.push(StackValue::Value(normalized));
        }
        OpCode::ArrayLen => {
            let v = pop_value_from(stack, chunks, chunk, ip)?;
            let RuntimeValue::Array(arr) = v else {
                return Err(locate(chunk, ip, VmError::Corrupt("ArrayLen on a non-array")));
            };
            stack.push(StackValue::Value(RuntimeValue::Number(Number::new(arr.len() as f64))));
        }
        OpCode::ArrayGetAt => {
            let idx = pop_value_from(stack, chunks, chunk, ip)?;
            let v = pop_value_from(stack, chunks, chunk, ip)?;
            let (RuntimeValue::Array(arr), RuntimeValue::Number(idx)) = (v, idx) else {
                return Err(locate(
                    chunk,
                    ip,
                    VmError::Corrupt("ArrayGetAt on a non-array/non-number"),
                ));
            };
            let elem = arr.get(idx.value() as usize).cloned().unwrap_or(RuntimeValue::None);
            stack.push(StackValue::Value(elem));
        }
        OpCode::ArraySliceFrom => {
            let idx = pop_value_from(stack, chunks, chunk, ip)?;
            let v = pop_value_from(stack, chunks, chunk, ip)?;
            let (RuntimeValue::Array(arr), RuntimeValue::Number(idx)) = (v, idx) else {
                return Err(locate(
                    chunk,
                    ip,
                    VmError::Corrupt("ArraySliceFrom on a non-array/non-number"),
                ));
            };
            let start = (idx.value() as usize).min(arr.len());
            stack.push(StackValue::Value(RuntimeValue::Array(Shared::new(
                arr[start..].to_vec(),
            ))));
        }
        _ => unreachable!("array_misc_op called with a non-array-misc opcode"),
    }
    Ok(())
}

fn selector_op(
    op: &OpCode,
    stack: &mut Vec<StackValue>,
    chunks: &Shared<Vec<Chunk>>,
    chunk: &Chunk,
    ip: usize,
) -> VmResult<()> {
    match op {
        OpCode::SelectorMatch(selector) => {
            let subject = pop_value_from(stack, chunks, chunk, ip)?;
            stack.push(StackValue::Value(eval_selector_expr(&subject, selector)));
        }
        OpCode::SelectorMatchWithArgs(payload) => {
            let (selector, argc) = payload.as_ref();
            // Selector arguments are usually one or two values. `Args` keeps those inline,
            // avoiding a heap allocation for every parameterized selector evaluation.
            let mut args = Args::with_capacity(*argc as usize);
            for _ in 0..*argc {
                args.push(pop_value_from(stack, chunks, chunk, ip)?);
            }
            args.reverse();
            let subject = pop_value_from(stack, chunks, chunk, ip)?;
            stack.push(StackValue::Value(eval_selector_expr_with_args(
                &subject, selector, &args,
            )));
        }
        _ => unreachable!("selector_op called with a non-selector opcode"),
    }
    Ok(())
}

#[cold]
#[inline(never)]
fn get_external_global(ident: Ident, chunk: &Chunk, ip: usize, env: &VmEnv) -> VmResult<RuntimeValue> {
    env.get(ident)
        .ok_or_else(|| locate(chunk, ip, VmError::UndefinedGlobal(ident.to_string())))
}

fn interp_string(parts: &[StackValue], chunks: &Shared<Vec<Chunk>>) -> RuntimeValue {
    use std::fmt::Write;
    let capacity = parts
        .iter()
        .map(|part| match part {
            StackValue::Value(RuntimeValue::String(value)) => value.len(),
            _ => 32,
        })
        .sum();
    let mut result = String::with_capacity(capacity);
    for part in parts.iter() {
        match part {
            // Skips the Display/Formatter machinery for the most common part shape (a
            // literal text fragment between `${...}`s).
            StackValue::Value(RuntimeValue::String(s)) => result.push_str(s),
            StackValue::Value(value) => {
                let _ = write!(result, "{value}");
            }
            StackValue::Closure(closure) => {
                let value = into_runtime_value(StackValue::Closure(Shared::clone(closure)), chunks);
                let _ = write!(result, "{value}");
            }
        }
    }
    RuntimeValue::String(result.into())
}

fn binary_op_from_opcode(op: &OpCode) -> Option<BinaryOp> {
    match op {
        OpCode::Add => Some(BinaryOp::Add),
        OpCode::Sub => Some(BinaryOp::Sub),
        OpCode::Mul => Some(BinaryOp::Mul),
        OpCode::Div => Some(BinaryOp::Div),
        OpCode::Mod => Some(BinaryOp::Mod),
        OpCode::Eq => Some(BinaryOp::Eq),
        OpCode::Ne => Some(BinaryOp::Ne),
        OpCode::Lt => Some(BinaryOp::Lt),
        OpCode::Le => Some(BinaryOp::Le),
        OpCode::Gt => Some(BinaryOp::Gt),
        OpCode::Ge => Some(BinaryOp::Ge),
        _ => None,
    }
}

fn local_runtime_value(locals: &Locals, slot: u16, chunks: &Shared<Vec<Chunk>>) -> VmResult<RuntimeValue> {
    // SAFETY: verify_chunks bounds-checks every caller's opcode slot.
    let value = unsafe { locals.get_unchecked(slot) };
    Ok(into_runtime_value(value, chunks))
}

fn eval_binary_op(
    op: BinaryOp,
    a: RuntimeValue,
    b: RuntimeValue,
    locals: &Locals,
    chunks: &Shared<Vec<Chunk>>,
    env: &VmEnv,
    host_functions: &HostFunctions,
) -> VmResult<RuntimeValue> {
    if matches!(
        op,
        BinaryOp::Eq | BinaryOp::Ne | BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge
    ) {
        return cmp_op(op, a, b, locals, chunks, env, host_functions);
    }
    binop(op, a, b, locals, chunks, env, host_functions)
}

fn binop(
    op: BinaryOp,
    a: RuntimeValue,
    b: RuntimeValue,
    locals: &Locals,
    chunks: &Shared<Vec<Chunk>>,
    env: &VmEnv,
    host_functions: &HostFunctions,
) -> VmResult<RuntimeValue> {
    if let (RuntimeValue::Number(n1), RuntimeValue::Number(n2)) = (&a, &b) {
        return Ok(RuntimeValue::Number(match op {
            BinaryOp::Add => *n1 + *n2,
            BinaryOp::Sub => *n1 - *n2,
            BinaryOp::Mul => *n1 * *n2,
            BinaryOp::Div => {
                if n2.is_zero() {
                    return Err(VmError::ZeroDivision);
                }
                *n1 / *n2
            }
            BinaryOp::Mod => *n1 % *n2,
            _ => return Err(VmError::Corrupt("non-arithmetic opcode in binop")),
        }));
    }
    let ident = match op {
        BinaryOp::Add => builtins::ADD,
        BinaryOp::Sub => builtins::SUB,
        BinaryOp::Mul => builtins::MUL,
        BinaryOp::Div => builtins::DIV,
        BinaryOp::Mod => builtins::MOD,
        _ => return Err(VmError::Corrupt("non-arithmetic opcode in binop")),
    };
    call_builtin(
        &crate::Ident::new(ident),
        &[a, b],
        &current_self(locals, chunks),
        env,
        host_functions,
    )
}

fn cmp_op(
    op: BinaryOp,
    a: RuntimeValue,
    b: RuntimeValue,
    locals: &Locals,
    chunks: &Shared<Vec<Chunk>>,
    env: &VmEnv,
    host_functions: &HostFunctions,
) -> VmResult<RuntimeValue> {
    if let (RuntimeValue::Number(n1), RuntimeValue::Number(n2)) = (&a, &b) {
        return Ok(RuntimeValue::Boolean(match op {
            BinaryOp::Eq => n1 == n2,
            BinaryOp::Ne => n1 != n2,
            BinaryOp::Lt => n1 < n2,
            BinaryOp::Le => n1 <= n2,
            BinaryOp::Gt => n1 > n2,
            BinaryOp::Ge => n1 >= n2,
            _ => return Err(VmError::Corrupt("non-comparison opcode in cmp_op")),
        }));
    }
    let ident = match op {
        BinaryOp::Eq => builtins::EQ,
        BinaryOp::Ne => builtins::NE,
        BinaryOp::Lt => builtins::LT,
        BinaryOp::Le => builtins::LTE,
        BinaryOp::Gt => builtins::GT,
        BinaryOp::Ge => builtins::GTE,
        _ => return Err(VmError::Corrupt("non-comparison opcode in cmp_op")),
    };
    call_builtin(
        &crate::Ident::new(ident),
        &[a, b],
        &current_self(locals, chunks),
        env,
        host_functions,
    )
}
