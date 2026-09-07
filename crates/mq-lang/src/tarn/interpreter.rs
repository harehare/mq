//! Tarn's bytecode dispatch loop: `run_chunk_inner_impl` and its opcode handlers, plus the
//! public `run_*` entry points that set up a top-level frame and call into it.
//!
//! `errors` (the `VmError` type), `frame` (deadline/call-depth tracking and the `Locals`/stack
//! pools), `calls` (binding arguments and invoking a callee), and `selectors` (applying a
//! `Selector` to a value) hold the parts that split out cleanly; this file is what remains.
mod calls;
mod errors;
mod frame;
mod selectors;

use self::calls::{
    CallSite, FixedClosureCall, call_builtin, call_builtin_args, call_fixed_closure_from_stack, call_stack_value,
    capture_upvalues, negate_ident,
};
use self::selectors::{eval_compact_selector_expr, eval_selector_expr, eval_selector_expr_with_args, type_check};
use super::bytecode::{BinaryOp, Chunk, OpCode, SELF_SLOT};
use super::compiler::CompiledProgram;
use super::value::VmClosureValue;
use super::value::{Cell, Closure, Locals, StackValue, read_cell, write_cell};
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
use frame::{ExecutionContext, ExecutionLimits};
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

#[cfg_attr(not(test), allow(dead_code))]
/// Runs a compiled program.
pub(crate) fn run(
    compiled: &CompiledProgram,
    input: RuntimeValue,
    host_functions: &HostFunctions,
    timeout: Option<Duration>,
    max_call_stack_depth: u32,
) -> VmResult<RuntimeValue> {
    run_with_globals(compiled, input, host_functions, timeout, max_call_stack_depth, &[])
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
        capture_names,
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
    let (result, captured, _) = run_impl_capturing_locals_with_env(
        compiled,
        input,
        bindings,
        options,
        &env,
        ExecutionPools::default(),
        capture_names,
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
    let locals = limits.take_locals(top_level_chunk.local_count, captures_local_slots);
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
        &[],
        &mut execution,
        #[cfg(feature = "debugger")]
        debug,
    )
    .map(|result| into_runtime_value(result, &compiled.chunks));
    (result, limits.into_pools())
}

/// Like [`run_impl_with_bindings`], but captures `capture_names`' final slot values. Bypasses
/// `run_chunk`'s pooling wrapper to keep `locals` readable; not for use on a hot path.
#[allow(clippy::too_many_arguments)] // The separate pools, capture list, and debugger are independent services.
fn run_impl_capturing_locals_with_env(
    compiled: &CompiledProgram,
    input: RuntimeValue,
    bindings: &[RuntimeValue],
    options: RunOptions<'_>,
    env: &VmEnv,
    pools: ExecutionPools,
    capture_names: &[Ident],
    #[cfg(feature = "debugger")] debug: &mut DebugRuntime<'_>,
) -> (VmResult<RuntimeValue>, Vec<(Ident, RuntimeValue)>, ExecutionPools) {
    let mut limits = ExecutionLimits::new(options.timeout, options.max_call_stack_depth, pools);
    let chunks = &compiled.chunks;
    let top_level_chunk = &chunks[0];
    let reusable_locals = !top_level_chunk.captures_local_slots();
    let locals = limits.take_locals(top_level_chunk.local_count, top_level_chunk.captures_local_slots());
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

    let mut stack = limits.take_stack();
    let mut execution = ExecutionContext {
        env,
        limits: &mut limits,
        host_functions: options.host_functions,
    };
    let raw_result = run_chunk_inner(
        0,
        chunks,
        &locals,
        &[],
        &mut stack,
        &mut execution,
        #[cfg(feature = "debugger")]
        debug,
    );
    let captured = capture_names
        .iter()
        .filter_map(|name| {
            // Last-declared slot wins, matching the compiler's reverse name resolution.
            top_level_chunk
                .local_names
                .iter()
                .rposition(|local| local == name)
                .and_then(|slot| locals.get_checked(slot as u16))
                .map(|value| (*name, into_runtime_value(value, chunks)))
        })
        .collect();
    execution.limits.recycle_stack(stack);
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
fn apply_debug_updates(frame: &VmDebugFrame, locals: &Locals, upvalues: &[Cell]) {
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

fn run_chunk(
    chunk_index: u16,
    chunks: &Shared<Vec<Chunk>>,
    locals: Locals,
    upvalues: &[Cell],
    execution: &mut ExecutionContext<'_>,
    #[cfg(feature = "debugger")] debug: &mut DebugRuntime<'_>,
) -> VmResult<StackValue> {
    let reusable_locals = !chunks[chunk_index as usize].captures_local_slots();
    let mut stack = execution.limits.take_stack();
    let result = run_chunk_inner(
        chunk_index,
        chunks,
        &locals,
        upvalues,
        &mut stack,
        execution,
        #[cfg(feature = "debugger")]
        debug,
    );
    if reusable_locals {
        execution.limits.recycle_locals(locals);
    }
    execution.limits.recycle_stack(stack);
    result
}

fn run_chunk_inner(
    chunk_index: u16,
    chunks: &Shared<Vec<Chunk>>,
    locals: &Locals,
    upvalues: &[Cell],
    stack: &mut Vec<StackValue>,
    execution: &mut ExecutionContext<'_>,
    #[cfg(feature = "debugger")] debug: &mut DebugRuntime<'_>,
) -> VmResult<StackValue> {
    if execution.limits.has_deadline() {
        run_chunk_inner_impl::<true>(
            chunk_index,
            chunks,
            locals,
            upvalues,
            stack,
            execution,
            #[cfg(feature = "debugger")]
            debug,
        )
    } else {
        run_chunk_inner_impl::<false>(
            chunk_index,
            chunks,
            locals,
            upvalues,
            stack,
            execution,
            #[cfg(feature = "debugger")]
            debug,
        )
    }
}

fn run_chunk_inner_impl<const CHECK_TIMEOUT: bool>(
    chunk_index: u16,
    chunks: &Shared<Vec<Chunk>>,
    locals: &Locals,
    upvalues: &[Cell],
    stack: &mut Vec<StackValue>,
    execution: &mut ExecutionContext<'_>,
    #[cfg(feature = "debugger")] debug: &mut DebugRuntime<'_>,
) -> VmResult<StackValue> {
    let chunk = &chunks[chunk_index as usize];
    let mut ip: usize = 0;

    macro_rules! pop {
        () => {
            stack
                .pop()
                .ok_or_else(|| locate(chunk, ip, VmError::Corrupt("stack underflow")))?
        };
    }
    macro_rules! pop_value {
        () => {{ into_runtime_value(pop!(), chunks) }};
    }
    macro_rules! bail {
        ($e:expr) => {
            return Err(locate(chunk, ip, $e))
        };
    }

    while ip < chunk.code.len() {
        if CHECK_TIMEOUT {
            execution.limits.check().map_err(|e| locate(chunk, ip, e))?;
        }
        let op = &chunk.code[ip];
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
                    upvalues: captured,
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
                let index = locals.get(*index_slot);
                let StackValue::Value(RuntimeValue::Number(index)) = index else {
                    bail!(VmError::Corrupt("ForeachNext has invalid loop state"));
                };
                let index_value = index.value();
                let (array_len, value) = locals
                    .array_len_and_element_at(*array_slot, index_value as usize)
                    .map_err(|e| locate(chunk, ip, VmError::Corrupt(e)))?;
                if index_value >= array_len as f64 {
                    ip = (ip as i64 + *exit_offset as i64) as usize;
                    continue;
                }
                let value = value.unwrap_or(RuntimeValue::None);
                locals.set(
                    *index_slot,
                    StackValue::Value(RuntimeValue::Number(Number::new(index_value + 1.0))),
                );
                locals.set(*value_slot, StackValue::Value(value.clone()));
                locals.set(SELF_SLOT, StackValue::Value(value));
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
            OpCode::CallLocal(slot, argc) => {
                let callee = locals.get(*slot);
                if let StackValue::Closure(closure) = &callee
                    && chunks[closure.chunk_index as usize]
                        .param_shape
                        .fixed_required_arity()
                        .is_some()
                {
                    let result = call_fixed_closure_from_stack(
                        FixedClosureCall {
                            closure,
                            argc: *argc,
                            remove_callee: false,
                        },
                        stack,
                        CallSite { locals, chunk, ip },
                        chunks,
                        execution,
                        #[cfg(feature = "debugger")]
                        debug,
                    )?;
                    stack.push(result);
                    continue;
                }
                // Pooled, not `Vec::with_capacity`: this path (non-fixed-arity callees —
                // variadic/optional params, `partial`-bound closures) runs often enough in
                // higher-order builtins that a fresh heap allocation per call is worth avoiding.
                let mut args = execution.limits.take_stack();
                for _ in 0..*argc {
                    args.push(pop!());
                }
                args.reverse();
                let call_result = call_stack_value(
                    callee,
                    &mut args,
                    CallSite { locals, chunk, ip },
                    chunks,
                    execution,
                    #[cfg(feature = "debugger")]
                    debug,
                );
                execution.limits.recycle_stack(args);
                stack.push(call_result?);
            }
            OpCode::CallValue(argc) => {
                let callee_index = stack
                    .len()
                    .checked_sub(*argc as usize + 1)
                    .ok_or_else(|| locate(chunk, ip, VmError::Corrupt("stack underflow in CallValue")))?;
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
                    let result = call_fixed_closure_from_stack(
                        FixedClosureCall {
                            closure: &closure,
                            argc: *argc,
                            remove_callee: true,
                        },
                        stack,
                        CallSite { locals, chunk, ip },
                        chunks,
                        execution,
                        #[cfg(feature = "debugger")]
                        debug,
                    )?;
                    stack.push(result);
                    continue;
                }
                // See the `CallLocal` non-fixed-arity path above for why this is pooled.
                let mut args = execution.limits.take_stack();
                for _ in 0..*argc {
                    args.push(pop!());
                }
                args.reverse();
                let callee = pop!();
                let call_result = call_stack_value(
                    callee,
                    &mut args,
                    CallSite { locals, chunk, ip },
                    chunks,
                    execution,
                    #[cfg(feature = "debugger")]
                    debug,
                );
                execution.limits.recycle_stack(args);
                stack.push(call_result?);
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
                    let result = call_stack_value(
                        value,
                        &mut Vec::new(),
                        CallSite { locals, chunk, ip },
                        chunks,
                        execution,
                        #[cfg(feature = "debugger")]
                        debug,
                    )?;
                    stack.push(result);
                } else {
                    stack.push(value);
                }
            }
            OpCode::TryCatch(info) => {
                let catch_closure = pop!();
                let try_closure = pop!();
                match handle_try_catch(
                    TryCatchArgs {
                        has_binder: info.has_binder,
                        break_acc_slot: info.break_acc_slot,
                        break_offset: info.break_offset,
                        continue_offset: info.continue_offset,
                        catch_closure,
                        try_closure,
                    },
                    CallSite { locals, chunk, ip },
                    chunks,
                    execution,
                    #[cfg(feature = "debugger")]
                    debug,
                )? {
                    TryCatchOutcome::Value(value) => stack.push(value),
                    TryCatchOutcome::JumpTo(offset) => {
                        ip = (ip as i64 + offset as i64) as usize;
                        continue;
                    }
                }
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
                return Ok(pop!());
            }
        }
    }

    Ok(stack.pop().unwrap_or(StackValue::Value(RuntimeValue::None)))
}

struct TryCatchArgs {
    has_binder: bool,
    break_acc_slot: Option<u16>,
    break_offset: Option<i32>,
    continue_offset: Option<i32>,
    catch_closure: StackValue,
    try_closure: StackValue,
}

enum TryCatchOutcome {
    Value(StackValue),
    /// Loop control (`break`/`continue`) raised inside the try chunk bypasses the catch
    /// and jumps to the enclosing loop's patched target — signaled back to the dispatch
    /// loop instead of jumping directly, since this function doesn't own `ip`.
    JumpTo(i32),
}

/// `try`/`catch` is rare and large; kept out of `run_chunk_inner_impl` to keep it small.
#[cold]
#[inline(never)]
fn handle_try_catch(
    args: TryCatchArgs,
    call_site: CallSite<'_>,
    chunks: &Shared<Vec<Chunk>>,
    execution: &mut ExecutionContext<'_>,
    #[cfg(feature = "debugger")] debug: &mut DebugRuntime<'_>,
) -> VmResult<TryCatchOutcome> {
    let CallSite { locals, chunk, ip } = call_site;
    let StackValue::Closure(catch_closure) = args.catch_closure else {
        return Err(locate(
            chunk,
            ip,
            VmError::Corrupt("TryCatch catch operand is not a closure"),
        ));
    };
    let StackValue::Closure(try_closure) = args.try_closure else {
        return Err(locate(
            chunk,
            ip,
            VmError::Corrupt("TryCatch try operand is not a closure"),
        ));
    };
    let try_chunk = &chunks[try_closure.chunk_index as usize];
    let try_locals = execution
        .limits
        .take_locals(try_chunk.local_count, try_chunk.captures_local_slots());
    try_locals.set(SELF_SLOT, locals.get(SELF_SLOT));
    if let Err(error) = execution.limits.enter_call() {
        if !try_chunk.captures_local_slots() {
            execution.limits.recycle_locals(try_locals);
        }
        return Err(locate(chunk, ip, error));
    }
    let try_result = run_chunk(
        try_closure.chunk_index,
        chunks,
        try_locals,
        &try_closure.upvalues,
        execution,
        #[cfg(feature = "debugger")]
        debug,
    );
    execution.limits.exit_call();
    match try_result {
        Ok(value) => Ok(TryCatchOutcome::Value(value)),
        Err(e) => {
            if let Some(value) = flow_break_value(&e) {
                let (Some(acc_slot), Some(offset)) = (args.break_acc_slot, args.break_offset) else {
                    return Err(e);
                };
                if let Some(value) = value {
                    locals.set(acc_slot, StackValue::Value(value));
                }
                return Ok(TryCatchOutcome::JumpTo(offset));
            }
            if flow_continue(&e) {
                let Some(offset) = args.continue_offset else {
                    return Err(e);
                };
                return Ok(TryCatchOutcome::JumpTo(offset));
            }
            let catch_chunk = &chunks[catch_closure.chunk_index as usize];
            let catch_locals = execution
                .limits
                .take_locals(catch_chunk.local_count, catch_chunk.captures_local_slots());
            catch_locals.set(SELF_SLOT, locals.get(SELF_SLOT));
            if args.has_binder {
                catch_locals.set(1, StackValue::Value(error_dict(&e)));
            }
            if let Err(error) = execution.limits.enter_call() {
                if !catch_chunk.captures_local_slots() {
                    execution.limits.recycle_locals(catch_locals);
                }
                return Err(locate(chunk, ip, error));
            }
            let catch_result = run_chunk(
                catch_closure.chunk_index,
                chunks,
                catch_locals,
                &catch_closure.upvalues,
                execution,
                #[cfg(feature = "debugger")]
                debug,
            );
            execution.limits.exit_call();
            Ok(TryCatchOutcome::Value(catch_result?))
        }
    }
}

/// Rare spread-syntax opcodes, kept out of `run_chunk_inner_impl` (see `handle_try_catch`).
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

/// Rare array opcodes, kept out of `run_chunk_inner_impl` (see `handle_try_catch`).
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
            let mut args = Vec::with_capacity(*argc as usize);
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
