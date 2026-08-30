use super::bytecode::{Chunk, OpCode, ParamBinding, ParamShape, SELF_SLOT, UpvalueSource};
use super::compiler::CompiledProgram;
#[cfg(feature = "tarn")]
use super::value::VmClosureValue;
use super::value::{Cell, Closure, StackValue, new_cell, read_cell, write_cell};
use crate::ast::TokenId;
use crate::eval::builtin::{self, Args};
use crate::eval::env::Env;
use crate::eval::host::HostFunctions;
use crate::eval::runtime_value::{self, RuntimeValue};
use crate::number::Number;
use crate::{Ident, Shared, SharedCell};
use std::collections::BTreeMap;
use std::fmt;
use std::time::{Duration, Instant};

/// Number of instructions between wall-clock deadline checks — matches
/// `Evaluator::TIMEOUT_CHECK_INTERVAL`.
const TIMEOUT_CHECK_INTERVAL: u32 = 1024;

/// Deadline and call-depth enforcement shared across every `run_chunk` invocation in one
/// execution (top-level body, user function calls, `try`/`catch` closures, default-value
/// thunks), mirroring `Evaluator::check_timeout`/`enter_scope`/`exit_scope`.
struct ExecutionLimits {
    deadline: Option<Instant>,
    timeout: Option<Duration>,
    step: u32,
    call_depth: u32,
    max_call_stack_depth: u32,
    local_pool: Vec<Vec<Cell>>,
    stack_pool: Vec<Vec<StackValue>>,
}

impl ExecutionLimits {
    fn new(timeout: Option<Duration>, max_call_stack_depth: u32) -> Self {
        Self {
            deadline: timeout.map(|t| Instant::now() + t),
            timeout,
            step: 0,
            call_depth: 0,
            max_call_stack_depth,
            local_pool: Vec::new(),
            stack_pool: Vec::new(),
        }
    }

    #[inline(always)]
    fn check(&mut self) -> VmResult<()> {
        let Some(deadline) = self.deadline else {
            return Ok(());
        };
        self.step = self.step.wrapping_add(1);
        if self.step & (TIMEOUT_CHECK_INTERVAL - 1) != 0 {
            return Ok(());
        }
        if Instant::now() >= deadline {
            Err(VmError::Timeout(self.timeout.expect("deadline implies timeout is set")))
        } else {
            Ok(())
        }
    }

    /// Call before recursing into a closure's chunk; pair with `exit_call` once it returns
    /// (on every path, including error). Bounds the VM's own recursion so runaway/deep
    /// recursion raises `VmError::RecursionError` instead of overflowing the native stack.
    #[inline(always)]
    fn enter_call(&mut self) -> VmResult<()> {
        if self.call_depth >= self.max_call_stack_depth {
            return Err(VmError::RecursionError(self.max_call_stack_depth));
        }
        self.call_depth += 1;
        Ok(())
    }

    #[inline(always)]
    fn exit_call(&mut self) {
        self.call_depth = self.call_depth.saturating_sub(1);
    }

    fn take_locals(&mut self, count: u16) -> Vec<Cell> {
        let count = count as usize;
        let locals = self
            .local_pool
            .iter()
            .position(|locals| locals.len() == count)
            .map(|index| self.local_pool.swap_remove(index))
            .unwrap_or_else(|| fresh_locals(count));
        for local in &locals {
            write_cell(local, StackValue::Value(RuntimeValue::None));
        }
        locals
    }

    fn recycle_locals(&mut self, locals: Vec<Cell>) {
        self.local_pool.push(locals);
    }

    fn take_stack(&mut self) -> Vec<StackValue> {
        self.stack_pool.pop().unwrap_or_else(|| Vec::with_capacity(8))
    }

    fn recycle_stack(&mut self, mut stack: Vec<StackValue>) {
        stack.clear();
        self.stack_pool.push(stack);
    }
}

#[cfg(feature = "debugger")]
use super::debug_symbols::DebugSlot;
#[cfg(feature = "debugger")]
use crate::ast::node::Node;

/// A read-only snapshot of a VM frame at a source evaluation boundary.
#[cfg(feature = "debugger")]
#[derive(Debug, Clone)]
// Consumed by the Engine adapter in the next M4 slice; the standalone VM runner uses no hook.
#[allow(dead_code)]
pub(crate) struct DebugEvent {
    pub(crate) token_id: TokenId,
    pub(crate) node: Shared<Node>,
    pub(crate) current_value: RuntimeValue,
    pub(crate) bindings: Vec<(Ident, RuntimeValue)>,
    pub(crate) call_stack: Vec<Shared<Node>>,
}

/// Receives VM debug boundaries. Implementations may inspect, but cannot mutate, frames.
#[cfg(feature = "debugger")]
pub(crate) trait DebugHook {
    fn on_boundary(&mut self, event: DebugEvent);
}

#[cfg(feature = "debugger")]
struct DebugRuntime<'a> {
    hook: Option<&'a mut dyn DebugHook>,
    call_stack: Vec<Shared<Node>>,
    current_node: Option<Shared<Node>>,
}

#[derive(Debug)]
pub(crate) enum VmError {
    Builtin(builtin::Error),
    Host(Ident, String),
    ZeroDivision,
    NotCallable,
    EnvNotFound(String),
    UndefinedGlobal(String),
    Corrupt(&'static str),
    ArityMismatch {
        expected: u8,
        actual: u8,
    },
    /// Internal control flow emitted by a `break` inside a nested `try` chunk.
    FlowBreak(Option<RuntimeValue>),
    /// Internal control flow emitted by a `continue` inside a nested `try` chunk.
    FlowContinue,
    DestructuringFailed,
    InvalidForeachTarget(String),
    Timeout(Duration),
    RecursionError(u32),
    Located(Box<VmError>, TokenId),
}

impl fmt::Display for VmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VmError::Builtin(e) => write!(f, "{e}"),
            VmError::Host(name, msg) => write!(f, "error in host function \"{name}\": {msg}"),
            VmError::ZeroDivision => write!(f, "division by zero"),
            VmError::NotCallable => write!(f, "value is not callable"),
            VmError::EnvNotFound(name) => write!(f, "environment variable not found: {name}"),
            VmError::UndefinedGlobal(name) => write!(f, "undefined identifier `{name}`"),
            VmError::Corrupt(what) => write!(f, "corrupt bytecode: {what}"),
            VmError::ArityMismatch { expected, actual } => {
                write!(f, "expected {expected} argument(s), got {actual}")
            }
            VmError::FlowBreak(_) => write!(f, "break outside a loop"),
            VmError::FlowContinue => write!(f, "continue outside a loop"),
            VmError::DestructuringFailed => write!(f, "destructuring pattern did not match value"),
            VmError::InvalidForeachTarget(repr) => write!(f, "invalid types for \"foreach\", got {repr}"),
            VmError::Timeout(d) => write!(f, "execution timed out after {:.3}s", d.as_secs_f64()),
            VmError::RecursionError(max) => write!(f, "maximum recursion depth exceeded ({max})"),
            VmError::Located(inner, _) => write!(f, "{inner}"),
        }
    }
}

impl VmError {
    #[allow(dead_code)]
    pub(crate) fn token_id(&self) -> Option<TokenId> {
        match self {
            VmError::Located(_, token_id) => Some(*token_id),
            _ => None,
        }
    }
}

fn locate(chunk: &Chunk, ip: usize, e: VmError) -> VmError {
    match chunk.token_at(ip.saturating_sub(1)) {
        Some(token_id) => VmError::Located(Box::new(e), token_id),
        None => e,
    }
}

impl std::error::Error for VmError {}

impl From<builtin::Error> for VmError {
    fn from(e: builtin::Error) -> Self {
        VmError::Builtin(e)
    }
}

type VmResult<T> = Result<T, VmError>;

/// Runtime services shared by parameter binding and default-value evaluation.
struct ParameterContext<'chunks, 'execution> {
    chunks: &'chunks Shared<Vec<Chunk>>,
    env: &'execution Shared<SharedCell<Env>>,
    limits: &'execution mut ExecutionLimits,
    host_functions: &'execution HostFunctions,
}

/// Mutable services shared by all frames of one VM evaluation.
struct ExecutionContext<'a> {
    env: &'a Shared<SharedCell<Env>>,
    limits: &'a mut ExecutionLimits,
    host_functions: &'a HostFunctions,
}

struct RunOptions<'a> {
    host_functions: &'a HostFunctions,
    timeout: Option<Duration>,
    max_call_stack_depth: u32,
    global_bindings: &'a [(Ident, RuntimeValue)],
}

struct CallSite<'a> {
    locals: &'a [Cell],
    chunk: &'a Chunk,
    ip: usize,
}

pub(crate) fn run(
    compiled: &CompiledProgram,
    input: RuntimeValue,
    host_functions: &HostFunctions,
    timeout: Option<Duration>,
    max_call_stack_depth: u32,
) -> VmResult<RuntimeValue> {
    run_with_globals(compiled, input, host_functions, timeout, max_call_stack_depth, &[])
}

pub(crate) fn run_with_globals(
    compiled: &CompiledProgram,
    input: RuntimeValue,
    host_functions: &HostFunctions,
    timeout: Option<Duration>,
    max_call_stack_depth: u32,
    global_bindings: &[(Ident, RuntimeValue)],
) -> VmResult<RuntimeValue> {
    #[cfg(feature = "debugger")]
    let mut debug = DebugRuntime {
        hook: None,
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
        #[cfg(feature = "debugger")]
        &mut debug,
    )
}

#[cfg(feature = "debugger")]
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
        &mut debug,
    )
}

fn run_impl(
    compiled: &CompiledProgram,
    input: RuntimeValue,
    options: RunOptions<'_>,
    #[cfg(feature = "debugger")] debug: &mut DebugRuntime<'_>,
) -> VmResult<RuntimeValue> {
    run_impl_with_bindings(
        compiled,
        input,
        &[],
        options,
        #[cfg(feature = "debugger")]
        debug,
    )
}

#[cfg(feature = "debugger")]
#[allow(dead_code)] // Used by the VM debugger adapter before its Engine cutover in M5.
pub(crate) fn run_debug_expression(
    compiled: &CompiledProgram,
    bindings: &[RuntimeValue],
    host_functions: &HostFunctions,
) -> VmResult<RuntimeValue> {
    let mut debug = DebugRuntime {
        hook: None,
        call_stack: Vec::new(),
        current_node: None,
    };
    run_impl_with_bindings(
        compiled,
        RuntimeValue::None,
        bindings,
        RunOptions {
            host_functions,
            timeout: None,
            max_call_stack_depth: crate::eval::Options::default().max_call_stack_depth,
            global_bindings: &[],
        },
        &mut debug,
    )
}

fn run_impl_with_bindings(
    compiled: &CompiledProgram,
    input: RuntimeValue,
    initial_bindings: &[RuntimeValue],
    options: RunOptions<'_>,
    #[cfg(feature = "debugger")] debug: &mut DebugRuntime<'_>,
) -> VmResult<RuntimeValue> {
    let mut env = Env::default();
    for (ident, value) in options.global_bindings {
        env.define(*ident, value.clone());
    }
    let placeholder_env: Shared<SharedCell<Env>> = Shared::new(SharedCell::new(env));
    let mut limits = ExecutionLimits::new(options.timeout, options.max_call_stack_depth);
    let locals = limits.take_locals(compiled.chunks[0].local_count);
    write_cell(&locals[SELF_SLOT as usize], StackValue::Value(input));
    if initial_bindings.len() + 1 > locals.len() {
        return Err(VmError::Corrupt("too many initial debug bindings"));
    }
    for (slot, value) in initial_bindings.iter().cloned().enumerate() {
        write_cell(&locals[slot + 1], StackValue::Value(value));
    }
    let mut execution = ExecutionContext {
        env: &placeholder_env,
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
    )?;
    match result {
        StackValue::Value(v) => Ok(v),
        StackValue::Closure(_) => Err(VmError::Corrupt("top-level result is a closure")),
    }
}

fn fresh_locals(count: usize) -> Vec<Cell> {
    (0..count)
        .map(|_| new_cell(StackValue::Value(RuntimeValue::None)))
        .collect()
}

/// Resolves a closure's declared upvalue sources against the currently-running frame,
/// exactly like `MakeClosure`'s handler — shared with `bind_params`, which needs the same
/// resolution to build a default-value thunk's captures.
fn capture_upvalues(sources: &[UpvalueSource], locals: &[Cell], upvalues: &[Cell]) -> Vec<Cell> {
    sources
        .iter()
        .map(|source| match source {
            UpvalueSource::Local(slot) => Shared::clone(&locals[*slot as usize]),
            UpvalueSource::Upvalue(idx) => Shared::clone(&upvalues[*idx as usize]),
        })
        .collect()
}

/// Calls `callee` with `args`. Shared by `OpCode::CallValue` and `OpCode::MaybeAutoCall`.
fn call_stack_value(
    callee: StackValue,
    #[cfg_attr(not(feature = "tarn"), allow(unused_mut))] mut args: Vec<StackValue>,
    call_site: CallSite<'_>,
    chunks: &Shared<Vec<Chunk>>,
    execution: &mut ExecutionContext<'_>,
    #[cfg(feature = "debugger")] debug: &mut DebugRuntime<'_>,
) -> VmResult<StackValue> {
    if let StackValue::Value(RuntimeValue::NativeFunction(ident)) = callee {
        #[cfg(feature = "tarn")]
        let arg_values: Vec<RuntimeValue> = args.into_iter().map(|a| into_runtime_value(a, chunks)).collect();
        #[cfg(not(feature = "tarn"))]
        let arg_values: Vec<RuntimeValue> = args
            .into_iter()
            .map(|a| match a {
                StackValue::Value(v) => Ok(v),
                StackValue::Closure(_) => Err(locate(
                    call_site.chunk,
                    call_site.ip,
                    VmError::Corrupt("closures can't be passed to a native function value"),
                )),
            })
            .collect::<VmResult<Vec<_>>>()?;
        let self_value = current_self(call_site.locals);
        let result = call_builtin(
            &ident,
            &arg_values,
            &self_value,
            execution.env,
            execution.host_functions,
        )
        .map_err(|e| locate(call_site.chunk, call_site.ip, e))?;
        return Ok(StackValue::Value(result));
    }

    #[cfg(feature = "tarn")]
    let (callee_chunks, callee_chunk_index, callee_upvalues): (&Shared<Vec<Chunk>>, u16, &[Cell]) = match &callee {
        StackValue::Closure(closure) => (chunks, closure.chunk_index, &closure.upvalues),
        StackValue::Value(RuntimeValue::VmClosure(vc)) => {
            if !vc.bound_args.is_empty() {
                let mut combined: Vec<StackValue> = vc.bound_args.iter().cloned().map(StackValue::Value).collect();
                combined.append(&mut args);
                args = combined;
            }
            (&vc.chunks, vc.chunk_index, &vc.upvalues)
        }
        _ => return Err(locate(call_site.chunk, call_site.ip, VmError::NotCallable)),
    };
    #[cfg(not(feature = "tarn"))]
    let (callee_chunks, callee_chunk_index, callee_upvalues): (&Shared<Vec<Chunk>>, u16, &[Cell]) = match &callee {
        StackValue::Closure(closure) => (chunks, closure.chunk_index, &closure.upvalues),
        _ => return Err(locate(call_site.chunk, call_site.ip, VmError::NotCallable)),
    };
    let callee_chunk = &callee_chunks[callee_chunk_index as usize];
    let callee_locals = execution.limits.take_locals(callee_chunk.local_count);
    write_cell(
        &callee_locals[SELF_SLOT as usize],
        read_cell(&call_site.locals[SELF_SLOT as usize]),
    );
    bind_params(
        &callee_chunk.param_shape,
        args,
        &callee_locals,
        callee_upvalues,
        &mut ParameterContext {
            chunks: callee_chunks,
            env: execution.env,
            limits: execution.limits,
            host_functions: execution.host_functions,
        },
        #[cfg(feature = "debugger")]
        debug,
    )
    .map_err(|e| locate(call_site.chunk, call_site.ip, e))?;
    #[cfg(feature = "debugger")]
    let caller_node = debug.current_node.clone();
    #[cfg(feature = "debugger")]
    let pushed_call = if let Some(node) = &caller_node {
        debug.call_stack.push(Shared::clone(node));
        true
    } else {
        false
    };
    execution
        .limits
        .enter_call()
        .map_err(|e| locate(call_site.chunk, call_site.ip, e))?;
    let call_result = run_chunk(
        callee_chunk_index,
        callee_chunks,
        callee_locals,
        callee_upvalues,
        execution,
        #[cfg(feature = "debugger")]
        debug,
    );
    execution.limits.exit_call();
    #[cfg(feature = "debugger")]
    if pushed_call {
        debug.call_stack.pop();
    }
    #[cfg(feature = "debugger")]
    {
        debug.current_node = caller_node;
    }
    call_result
}

fn bind_params(
    shape: &ParamShape,
    args: Vec<StackValue>,
    callee_locals: &[Cell],
    enclosing_upvalues: &[Cell],
    context: &mut ParameterContext<'_, '_>,
    #[cfg(feature = "debugger")] debug: &mut DebugRuntime<'_>,
) -> VmResult<()> {
    let arg_count = args.len();
    let param_count = shape.bindings.len();
    let use_self_param = parameter_uses_implicit_self(shape, arg_count)?;

    let mut bindings = shape.bindings.iter();
    let mut args = args.into_iter();

    if use_self_param && let Some(binding) = bindings.next() {
        let self_value = current_self(callee_locals);
        write_cell(&callee_locals[binding.slot() as usize], StackValue::Value(self_value));
    }

    for binding in bindings {
        match binding {
            ParamBinding::Variadic(slot) => {
                #[cfg(feature = "tarn")]
                let collected: Vec<RuntimeValue> = args
                    .by_ref()
                    .map(|arg| into_runtime_value(arg, context.chunks))
                    .collect();
                #[cfg(not(feature = "tarn"))]
                let collected: Vec<RuntimeValue> = {
                    let mut collected = Vec::new();
                    for arg in args.by_ref() {
                        match arg {
                            StackValue::Value(v) => collected.push(v),
                            StackValue::Closure(_) => {
                                return Err(VmError::Corrupt(
                                    "closures can't be collected into a variadic parameter",
                                ));
                            }
                        }
                    }
                    collected
                };
                write_cell(
                    &callee_locals[*slot as usize],
                    StackValue::Value(RuntimeValue::Array(Shared::new(collected))),
                );
            }
            ParamBinding::Required(slot) => {
                let Some(value) = args.next() else {
                    return Err(VmError::ArityMismatch {
                        expected: param_count as u8,
                        actual: arg_count as u8,
                    });
                };
                write_cell(&callee_locals[*slot as usize], value);
            }
            ParamBinding::Optional(slot, default_chunk, default_upvalues) => {
                if let Some(value) = args.next() {
                    write_cell(&callee_locals[*slot as usize], value);
                } else {
                    let captured = capture_upvalues(default_upvalues, callee_locals, enclosing_upvalues);
                    let default_locals = context
                        .limits
                        .take_locals(context.chunks[*default_chunk as usize].local_count);
                    write_cell(
                        &default_locals[SELF_SLOT as usize],
                        read_cell(&callee_locals[SELF_SLOT as usize]),
                    );
                    let value = {
                        let mut execution = ExecutionContext {
                            env: context.env,
                            limits: context.limits,
                            host_functions: context.host_functions,
                        };
                        run_chunk(
                            *default_chunk,
                            context.chunks,
                            default_locals,
                            &captured,
                            &mut execution,
                            #[cfg(feature = "debugger")]
                            debug,
                        )?
                    };
                    write_cell(&callee_locals[*slot as usize], value);
                }
            }
        }
    }
    Ok(())
}

/// Decides whether a call binds the pipeline value as its first parameter.
///
/// This is the VM equivalent of the evaluator's one-argument-short implicit `.` rule.
/// Keeping arity validation here makes the parameter-binding loop below purely mechanical.
fn parameter_uses_implicit_self(shape: &ParamShape, arg_count: usize) -> VmResult<bool> {
    let parameter_count = shape.bindings.len();
    let accepts_explicit_args = arg_count >= shape.required && (shape.has_variadic || arg_count <= parameter_count);
    if accepts_explicit_args {
        return Ok(false);
    }

    let accepts_implicit_self = arg_count.saturating_add(1) >= shape.required && arg_count < parameter_count;
    if accepts_implicit_self {
        return Ok(true);
    }

    Err(VmError::ArityMismatch {
        expected: if shape.has_variadic {
            shape.required as u8
        } else {
            parameter_count as u8
        },
        actual: arg_count as u8,
    })
}

#[cfg(feature = "tarn")]
fn into_runtime_value(v: StackValue, chunks: &Shared<Vec<Chunk>>) -> RuntimeValue {
    match v {
        StackValue::Value(rv) => rv,
        StackValue::Closure(closure) => RuntimeValue::VmClosure(VmClosureValue::from_closure(chunks, &closure)),
    }
}

#[cfg(not(feature = "tarn"))]
fn into_runtime_value(v: StackValue, _chunks: &Shared<Vec<Chunk>>) -> RuntimeValue {
    match v {
        StackValue::Value(rv) => rv,
        StackValue::Closure(_) => RuntimeValue::None,
    }
}

fn current_self(locals: &[Cell]) -> RuntimeValue {
    match read_cell(&locals[SELF_SLOT as usize]) {
        StackValue::Value(v) => v,
        StackValue::Closure(_) => RuntimeValue::None,
    }
}

/// Makes statically-resolved slots visible to the legacy variable builtins. This is deliberately
/// limited to `get_variable`/`set_variable`: publishing every frame's locals to the shared
/// compatibility environment would leak catch binders after their lexical scope ends.
fn sync_dynamic_env(
    chunk: &Chunk,
    locals: &[Cell],
    upvalues: &[Cell],
    chunks: &Shared<Vec<Chunk>>,
    env: &Shared<SharedCell<Env>>,
) {
    #[cfg(not(feature = "sync"))]
    let mut env = env.borrow_mut();
    #[cfg(feature = "sync")]
    let mut env = env.write().unwrap();

    for (slot, name) in chunk.local_names.iter().enumerate() {
        if *name != Ident::default()
            && let Some(cell) = locals.get(slot)
        {
            env.define(*name, into_runtime_value(read_cell(cell), chunks));
        }
    }
    for (slot, name) in chunk.upvalue_names.iter().enumerate() {
        if *name != Ident::default()
            && let Some(cell) = upvalues.get(slot)
        {
            env.define(*name, into_runtime_value(read_cell(cell), chunks));
        }
    }
}

fn run_chunk(
    chunk_index: u16,
    chunks: &Shared<Vec<Chunk>>,
    locals: Vec<Cell>,
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
    locals: &[Cell],
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
        () => {{
            #[cfg(feature = "tarn")]
            {
                into_runtime_value(pop!(), chunks)
            }
            #[cfg(not(feature = "tarn"))]
            {
                match pop!() {
                    StackValue::Value(v) => v,
                    StackValue::Closure(_) => {
                        return Err(locate(chunk, ip, VmError::Corrupt("expected a value, got a closure")));
                    }
                }
            }
        }};
    }
    macro_rules! bail {
        ($e:expr) => {
            return Err(locate(chunk, ip, $e))
        };
    }

    while ip < chunk.code.len() {
        execution.limits.check().map_err(|e| locate(chunk, ip, e))?;
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

                let mut bindings = Vec::with_capacity(chunk.debug_symbols.bindings().len());
                for (name, slot) in chunk.debug_symbols.bindings() {
                    let cell = match slot {
                        DebugSlot::Local(slot) => locals.get(*slot as usize),
                        DebugSlot::Upvalue(slot) => upvalues.get(*slot as usize),
                    }
                    .ok_or_else(|| locate(chunk, ip, VmError::Corrupt("debug slot out of bounds")))?;
                    if let StackValue::Value(value) = read_cell(cell) {
                        bindings.push((*name, value));
                    }
                }
                if let Some(hook) = debug.hook.as_deref_mut() {
                    hook.on_boundary(DebugEvent {
                        token_id: *token_id,
                        node,
                        current_value: current_self(locals),
                        bindings,
                        call_stack: debug.call_stack.clone(),
                    });
                }
            }
            OpCode::Const(idx) => stack.push(StackValue::Value(chunk.constants[*idx as usize].clone())),
            OpCode::PushNone => stack.push(StackValue::Value(RuntimeValue::None)),
            OpCode::GetLocal(slot) => stack.push(read_cell(&locals[*slot as usize])),
            OpCode::SetLocal(slot) => {
                let v = pop!();
                write_cell(&locals[*slot as usize], v);
            }
            OpCode::GetUpvalue(idx) => stack.push(read_cell(&upvalues[*idx as usize])),
            OpCode::SetUpvalue(idx) => {
                let v = pop!();
                write_cell(&upvalues[*idx as usize], v);
            }
            OpCode::MakeClosure(target_chunk, sources) => {
                let captured = capture_upvalues(sources, locals, upvalues);
                stack.push(StackValue::Closure(Shared::new(Closure {
                    chunk_index: *target_chunk,
                    upvalues: captured,
                })));
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
            OpCode::Swap => {
                let len = stack.len();
                if len < 2 {
                    bail!(VmError::Corrupt("stack underflow in Swap"));
                }
                stack.swap(len - 1, len - 2);
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
                stack.push(StackValue::Value(
                    binop(op, a, b, locals, execution.env, execution.host_functions)
                        .map_err(|e| locate(chunk, ip, e))?,
                ));
            }
            OpCode::Eq | OpCode::Ne | OpCode::Lt | OpCode::Le | OpCode::Gt | OpCode::Ge => {
                let b = pop_value!();
                let a = pop_value!();
                stack.push(StackValue::Value(
                    cmp_op(op, a, b, locals, execution.env, execution.host_functions)
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
                        &current_self(locals),
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
            OpCode::ArrayPush => {
                let elem = pop_value!();
                let mut arr = pop_value!();
                let RuntimeValue::Array(array) = &mut arr else {
                    bail!(VmError::Corrupt("ArrayPush on a non-array"));
                };
                runtime_value::array_mut(array).push(elem);
                stack.push(StackValue::Value(arr));
            }
            OpCode::ArraySpread => {
                let source = pop_value!();
                let mut arr = pop_value!();
                match source {
                    RuntimeValue::Array(source) => {
                        let RuntimeValue::Array(array) = &mut arr else {
                            bail!(VmError::Corrupt("ArraySpread accumulator is not an array"));
                        };
                        runtime_value::array_mut(array).extend(Shared::unwrap_or_clone(source));
                    }
                    RuntimeValue::None => {}
                    other => bail!(VmError::Builtin(builtin::Error::InvalidTypes(
                        builtins::ARRAY.to_string(),
                        vec![other]
                    ))),
                }
                stack.push(StackValue::Value(arr));
            }
            OpCode::DictSpread => {
                let source = pop_value!();
                let mut arr = pop_value!();
                match source {
                    RuntimeValue::Dict(map) => {
                        let RuntimeValue::Array(array) = &mut arr else {
                            bail!(VmError::Corrupt("DictSpread accumulator is not an array"));
                        };
                        runtime_value::array_mut(array).extend(
                            Shared::unwrap_or_clone(map)
                                .into_iter()
                                .map(|(k, v)| RuntimeValue::Array(Shared::new(vec![RuntimeValue::Symbol(k), v]))),
                        );
                    }
                    RuntimeValue::None => {}
                    other => bail!(VmError::Builtin(builtin::Error::InvalidTypes(
                        builtins::DICT.to_string(),
                        vec![other]
                    ))),
                }
                stack.push(StackValue::Value(arr));
            }
            OpCode::ToForeachIterable => {
                let v = pop_value!();
                let normalized = match v {
                    array @ RuntimeValue::Array(_) => array,
                    RuntimeValue::String(s) => RuntimeValue::Array(Shared::new(
                        s.chars().map(|c| RuntimeValue::String(c.to_string())).collect(),
                    )),
                    other => bail!(VmError::InvalidForeachTarget(other.to_string())),
                };
                stack.push(StackValue::Value(normalized));
            }
            OpCode::ArrayLen => {
                let v = pop_value!();
                let RuntimeValue::Array(arr) = v else {
                    bail!(VmError::Corrupt("ArrayLen on a non-array"));
                };
                stack.push(StackValue::Value(RuntimeValue::Number(Number::new(arr.len() as f64))));
            }
            OpCode::ArrayGetAt => {
                let idx = pop_value!();
                let v = pop_value!();
                let (RuntimeValue::Array(arr), RuntimeValue::Number(idx)) = (v, idx) else {
                    bail!(VmError::Corrupt("ArrayGetAt on a non-array/non-number"));
                };
                let elem = arr.get(idx.value() as usize).cloned().unwrap_or(RuntimeValue::None);
                stack.push(StackValue::Value(elem));
            }
            OpCode::ArraySliceFrom => {
                let idx = pop_value!();
                let v = pop_value!();
                let (RuntimeValue::Array(arr), RuntimeValue::Number(idx)) = (v, idx) else {
                    bail!(VmError::Corrupt("ArraySliceFrom on a non-array/non-number"));
                };
                let start = (idx.value() as usize).min(arr.len());
                stack.push(StackValue::Value(RuntimeValue::Array(Shared::new(
                    arr[start..].to_vec(),
                ))));
            }
            OpCode::TypeCheck(type_name) => {
                let v = pop_value!();
                let type_str = type_name.as_str();
                let matches = match type_str.as_str() {
                    "string" => matches!(v, RuntimeValue::String(_)),
                    "number" => matches!(v, RuntimeValue::Number(_)),
                    "bool" => matches!(v, RuntimeValue::Boolean(_)),
                    "array" => matches!(v, RuntimeValue::Array(_)),
                    "dict" => matches!(v, RuntimeValue::Dict(_)),
                    "bytes" => matches!(v, RuntimeValue::Bytes(_)),
                    "markdown" => matches!(v, RuntimeValue::Markdown(_, _)),
                    "function" => matches!(v, RuntimeValue::Function(_, _, _)),
                    "symbol" => matches!(v, RuntimeValue::Symbol(_)),
                    "none" => matches!(v, RuntimeValue::None),
                    _ => match &v {
                        RuntimeValue::Markdown(node, _) => {
                            crate::selector::Selector::from_selector_str(&format!(".{type_str}"))
                                .filter(|selector| !selector.is_attribute_selector())
                                .is_some_and(|selector| builtin::eval_selector(node, &selector) != RuntimeValue::NONE)
                        }
                        _ => false,
                    },
                };
                stack.push(StackValue::Value(RuntimeValue::Boolean(matches)));
            }
            OpCode::SelectorMatch(selector) => {
                let subject = pop_value!();
                let result = eval_selector_expr(&subject, selector);
                stack.push(StackValue::Value(result));
            }
            OpCode::SelectorMatchWithArgs(selector, argc) => {
                let mut args = Vec::with_capacity(*argc as usize);
                for _ in 0..*argc {
                    args.push(pop_value!());
                }
                args.reverse();
                let subject = pop_value!();
                let result = eval_selector_expr_with_args(&subject, selector, &args);
                stack.push(StackValue::Value(result));
            }
            OpCode::GetEnvVar(name_idx) => {
                let RuntimeValue::String(name) = &chunk.constants[*name_idx as usize] else {
                    bail!(VmError::Corrupt("GetEnvVar constant is not a string"));
                };
                let value = builtin::io_context::current()
                    .env_var(name)
                    .map_err(|_| locate(chunk, ip, VmError::EnvNotFound(name.clone())))?;
                stack.push(StackValue::Value(RuntimeValue::String(value)));
            }
            OpCode::GetExternalGlobal(ident) => {
                #[cfg(not(feature = "sync"))]
                let resolved = execution.env.borrow().resolve(*ident);
                #[cfg(feature = "sync")]
                let resolved = execution.env.read().unwrap().resolve(*ident);
                let value = resolved.map_err(|_| locate(chunk, ip, VmError::UndefinedGlobal(ident.to_string())))?;
                stack.push(StackValue::Value(value));
            }
            OpCode::InterpString(n) => {
                let mut parts = Vec::with_capacity(*n as usize);
                for _ in 0..*n {
                    parts.push(pop_value!());
                }
                parts.reverse();
                let joined = parts.iter().map(|v| v.to_string()).collect::<String>();
                stack.push(StackValue::Value(RuntimeValue::String(joined)));
            }
            OpCode::CallBuiltin(ident, argc) => {
                let mut args = Vec::with_capacity(*argc as usize);
                for _ in 0..*argc {
                    args.push(pop_value!());
                }
                args.reverse();
                if *ident == Ident::new("get_variable") || *ident == Ident::new("set_variable") {
                    sync_dynamic_env(chunk, locals, upvalues, chunks, execution.env);
                }
                stack.push(StackValue::Value(
                    call_builtin(
                        ident,
                        &args,
                        &current_self(locals),
                        execution.env,
                        execution.host_functions,
                    )
                    .map_err(|e| locate(chunk, ip, e))?,
                ));
            }
            OpCode::CallValue(argc) => {
                let mut args = Vec::with_capacity(*argc as usize);
                for _ in 0..*argc {
                    args.push(pop!());
                }
                args.reverse();
                let callee = pop!();
                let result = call_stack_value(
                    callee,
                    args,
                    CallSite { locals, chunk, ip },
                    chunks,
                    execution,
                    #[cfg(feature = "debugger")]
                    debug,
                )?;
                stack.push(result);
            }
            OpCode::MaybeAutoCall => {
                let value = pop!();
                let eligible = match &value {
                    StackValue::Closure(closure) => chunks[closure.chunk_index as usize].param_shape.required <= 1,
                    #[cfg(feature = "tarn")]
                    StackValue::Value(RuntimeValue::VmClosure(vc)) => {
                        vc.chunks[vc.chunk_index as usize].param_shape.required <= 1
                    }
                    StackValue::Value(RuntimeValue::NativeFunction(ident)) => builtin::get_builtin_functions(ident)
                        .is_some_and(|f| f.num_params.is_valid(0) || f.num_params.is_missing_one_params(0)),
                    _ => false,
                };
                if eligible {
                    let result = call_stack_value(
                        value,
                        Vec::new(),
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
            OpCode::TryCatch {
                has_binder,
                break_acc_slot,
                break_offset,
                continue_offset,
            } => {
                let StackValue::Closure(catch_closure) = pop!() else {
                    bail!(VmError::Corrupt("TryCatch catch operand is not a closure"));
                };
                let StackValue::Closure(try_closure) = pop!() else {
                    bail!(VmError::Corrupt("TryCatch try operand is not a closure"));
                };
                let try_locals = execution
                    .limits
                    .take_locals(chunks[try_closure.chunk_index as usize].local_count);
                write_cell(&try_locals[SELF_SLOT as usize], read_cell(&locals[SELF_SLOT as usize]));
                match run_chunk(
                    try_closure.chunk_index,
                    chunks,
                    try_locals,
                    &try_closure.upvalues,
                    execution,
                    #[cfg(feature = "debugger")]
                    debug,
                ) {
                    Ok(value) => stack.push(value),
                    Err(e) => {
                        if let Some(value) = flow_break_value(&e) {
                            let (Some(acc_slot), Some(offset)) = (break_acc_slot, break_offset) else {
                                return Err(e);
                            };
                            if let Some(value) = value {
                                write_cell(&locals[*acc_slot as usize], StackValue::Value(value));
                            }
                            ip = (ip as i64 + *offset as i64) as usize;
                            continue;
                        }
                        if flow_continue(&e) {
                            let Some(offset) = continue_offset else {
                                return Err(e);
                            };
                            ip = (ip as i64 + *offset as i64) as usize;
                            continue;
                        }
                        let catch_locals = execution
                            .limits
                            .take_locals(chunks[catch_closure.chunk_index as usize].local_count);
                        write_cell(
                            &catch_locals[SELF_SLOT as usize],
                            read_cell(&locals[SELF_SLOT as usize]),
                        );
                        if *has_binder {
                            write_cell(&catch_locals[1], StackValue::Value(error_dict(&e)));
                        }
                        stack.push(run_chunk(
                            catch_closure.chunk_index,
                            chunks,
                            catch_locals,
                            &catch_closure.upvalues,
                            execution,
                            #[cfg(feature = "debugger")]
                            debug,
                        )?);
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

fn binop(
    op: &OpCode,
    a: RuntimeValue,
    b: RuntimeValue,
    locals: &[Cell],
    env: &Shared<SharedCell<Env>>,
    host_functions: &HostFunctions,
) -> VmResult<RuntimeValue> {
    if let (RuntimeValue::Number(n1), RuntimeValue::Number(n2)) = (&a, &b) {
        return Ok(RuntimeValue::Number(match op {
            OpCode::Add => *n1 + *n2,
            OpCode::Sub => *n1 - *n2,
            OpCode::Mul => *n1 * *n2,
            OpCode::Div => {
                if n2.is_zero() {
                    return Err(VmError::ZeroDivision);
                }
                *n1 / *n2
            }
            OpCode::Mod => *n1 % *n2,
            _ => return Err(VmError::Corrupt("non-arithmetic opcode in binop")),
        }));
    }
    let ident = match op {
        OpCode::Add => builtins::ADD,
        OpCode::Sub => builtins::SUB,
        OpCode::Mul => builtins::MUL,
        OpCode::Div => builtins::DIV,
        OpCode::Mod => builtins::MOD,
        _ => return Err(VmError::Corrupt("non-arithmetic opcode in binop")),
    };
    call_builtin(
        &crate::Ident::new(ident),
        &[a, b],
        &current_self(locals),
        env,
        host_functions,
    )
}

fn cmp_op(
    op: &OpCode,
    a: RuntimeValue,
    b: RuntimeValue,
    locals: &[Cell],
    env: &Shared<SharedCell<Env>>,
    host_functions: &HostFunctions,
) -> VmResult<RuntimeValue> {
    if let (RuntimeValue::Number(n1), RuntimeValue::Number(n2)) = (&a, &b) {
        return Ok(RuntimeValue::Boolean(match op {
            OpCode::Eq => n1 == n2,
            OpCode::Ne => n1 != n2,
            OpCode::Lt => n1 < n2,
            OpCode::Le => n1 <= n2,
            OpCode::Gt => n1 > n2,
            OpCode::Ge => n1 >= n2,
            _ => return Err(VmError::Corrupt("non-comparison opcode in cmp_op")),
        }));
    }
    let ident = match op {
        OpCode::Eq => builtins::EQ,
        OpCode::Ne => builtins::NE,
        OpCode::Lt => builtins::LT,
        OpCode::Le => builtins::LTE,
        OpCode::Gt => builtins::GT,
        OpCode::Ge => builtins::GTE,
        _ => return Err(VmError::Corrupt("non-comparison opcode in cmp_op")),
    };
    call_builtin(
        &crate::Ident::new(ident),
        &[a, b],
        &current_self(locals),
        env,
        host_functions,
    )
}

/// Calls a builtin by name, falling back to a registered host function — matching the
/// tree-walker's `Env::resolve`-fails → `host_functions` → `eval_builtin` priority — when
/// `ident` isn't a real builtin (`Error::NotDefined`).
fn call_builtin(
    ident: &crate::Ident,
    args: &[RuntimeValue],
    self_value: &RuntimeValue,
    env: &Shared<SharedCell<Env>>,
    host_functions: &HostFunctions,
) -> VmResult<RuntimeValue> {
    let call_args: Args = args.iter().cloned().collect();
    match builtin::eval_builtin(self_value, ident, call_args, env) {
        Ok(v) => Ok(v),
        Err(builtin::Error::NotDefined(_, _)) => match host_functions.get(ident) {
            // Catches a panic at the boundary, matching the tree-walker's `eval_host_fn` —
            // a misbehaving host closure can't unwind through the VM.
            Some(host_fn) => std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| host_fn.call(args)))
                .unwrap_or_else(|payload| {
                    Err(crate::eval::host::HostFunctionError::new(format!(
                        "panic: {}",
                        crate::eval::host::panic_message(&*payload)
                    )))
                })
                .map_err(|e| VmError::Host(*ident, e.message().to_string())),
            None => Err(VmError::Builtin(builtin::Error::NotDefined(
                ident.to_string(),
                Vec::new(),
            ))),
        },
        Err(e) => Err(VmError::Builtin(e)),
    }
}

fn negate_ident() -> &'static crate::Ident {
    use std::sync::LazyLock;
    static NEGATE: LazyLock<crate::Ident> = LazyLock::new(|| crate::Ident::new(builtins::NEGATE));
    &NEGATE
}

fn type_ident() -> &'static crate::Ident {
    use std::sync::LazyLock;
    static TYPE: LazyLock<crate::Ident> = LazyLock::new(|| crate::Ident::new("type"));
    &TYPE
}

/// Applies `selector` to `value`, ported from `Evaluator::eval_selector_expr`: an `Array`
/// maps per-element and flattens; a `Dict` recurses into every value except `type`.
fn eval_selector_expr(value: &RuntimeValue, selector: &crate::selector::Selector) -> RuntimeValue {
    use crate::selector::Selector;
    if let Selector::Property(property_name) = selector {
        return eval_property_selector_expr(value, property_name);
    }
    match value {
        RuntimeValue::Markdown(node, _) => builtin::eval_selector(node, selector),
        RuntimeValue::Array(values) => {
            if let Selector::List(Some(idx), None) = selector {
                return values.get(*idx).cloned().unwrap_or(RuntimeValue::None);
            }
            let values = values
                .iter()
                .flat_map(|v| match v {
                    RuntimeValue::Markdown(node, _) => match builtin::eval_selector(node, selector) {
                        RuntimeValue::Array(arr) => Shared::unwrap_or_clone(arr),
                        other => vec![other],
                    },
                    _ if matches!(selector, Selector::List(None, None)) => vec![v.clone()],
                    RuntimeValue::Dict(_) => match eval_selector_expr(v, selector) {
                        RuntimeValue::Array(arr) if matches!(selector, Selector::Recursive) => {
                            Shared::unwrap_or_clone(arr)
                        }
                        other => vec![other],
                    },
                    _ => vec![RuntimeValue::None],
                })
                .collect::<Vec<_>>();
            RuntimeValue::Array(Shared::new(values))
        }
        RuntimeValue::Dict(map) => {
            if matches!(selector, Selector::List(None, None)) {
                return RuntimeValue::Array(Shared::new(map.values().cloned().collect()));
            }
            if matches!(selector, Selector::Recursive) {
                return RuntimeValue::Array(Shared::new(collect_recursive(value)));
            }
            let new_map: BTreeMap<_, _> = map
                .iter()
                .map(|(k, v)| {
                    let new_v = if k == type_ident() {
                        v.clone()
                    } else {
                        eval_selector_expr(v, selector)
                    };
                    (*k, new_v)
                })
                .collect();
            if new_map.is_empty() {
                RuntimeValue::None
            } else {
                RuntimeValue::Dict(Shared::new(new_map))
            }
        }
        _ => RuntimeValue::None,
    }
}

fn eval_selector_expr_with_args(
    value: &RuntimeValue,
    selector: &crate::selector::Selector,
    args: &[RuntimeValue],
) -> RuntimeValue {
    use crate::selector::Selector;
    match value {
        RuntimeValue::Markdown(node, _) => builtin::eval_selector_with_args(node, selector, args),
        RuntimeValue::Array(values) => {
            if let Selector::List(Some(idx), None) = selector {
                return values.get(*idx).cloned().unwrap_or(RuntimeValue::None);
            }
            let values = values
                .iter()
                .flat_map(|v| match v {
                    RuntimeValue::Markdown(node, _) => match builtin::eval_selector_with_args(node, selector, args) {
                        RuntimeValue::Array(arr) => Shared::unwrap_or_clone(arr),
                        other => vec![other],
                    },
                    _ if matches!(selector, Selector::List(None, None)) && args.is_empty() => vec![v.clone()],
                    RuntimeValue::Dict(_) => vec![eval_selector_expr_with_args(v, selector, args)],
                    _ => vec![RuntimeValue::None],
                })
                .collect::<Vec<_>>();
            RuntimeValue::Array(Shared::new(values))
        }
        RuntimeValue::Dict(map) => {
            let new_map: BTreeMap<_, _> = map
                .iter()
                .map(|(k, v)| {
                    let new_v = if k == type_ident() {
                        v.clone()
                    } else {
                        eval_selector_expr_with_args(v, selector, args)
                    };
                    (*k, new_v)
                })
                .collect();
            if new_map.is_empty() {
                RuntimeValue::None
            } else {
                RuntimeValue::Dict(Shared::new(new_map))
            }
        }
        _ => RuntimeValue::None,
    }
}

/// Property-selector counterpart to `eval_selector_expr` (`Selector::Property` routes here) —
/// ported from `Evaluator::eval_property_selector_expr`.
fn eval_property_selector_expr(value: &RuntimeValue, property_name: &Ident) -> RuntimeValue {
    match value {
        RuntimeValue::Array(values) => RuntimeValue::Array(Shared::new(
            values
                .iter()
                .map(|v| match v {
                    RuntimeValue::Dict(_) => eval_property_selector_expr(v, property_name),
                    _ => RuntimeValue::None,
                })
                .collect(),
        )),
        RuntimeValue::Dict(map) => map.get(property_name).cloned().unwrap_or(RuntimeValue::None),
        _ => RuntimeValue::None,
    }
}

/// Collects `value` itself and all nested values recursively (depth-first) — ported from
/// `Evaluator::collect_recursive`, backing `Selector::Recursive` on an Array/Dict.
fn collect_recursive(value: &RuntimeValue) -> Vec<RuntimeValue> {
    let mut result = vec![value.clone()];
    match value {
        RuntimeValue::Array(items) => {
            for item in items.iter() {
                result.extend(collect_recursive(item));
            }
        }
        RuntimeValue::Dict(map) => {
            for v in map.values() {
                result.extend(collect_recursive(v));
            }
        }
        _ => {}
    }
    result
}

fn error_dict(e: &VmError) -> RuntimeValue {
    let mut map = BTreeMap::new();
    map.insert(Ident::new("message"), RuntimeValue::String(error_message(e)));
    RuntimeValue::Dict(Shared::new(map))
}

fn error_message(e: &VmError) -> String {
    match e {
        VmError::Located(inner, _) => error_message(inner),
        VmError::Builtin(inner) => builtin_error_message(inner),
        other => other.to_string(),
    }
}

fn flow_break_value(e: &VmError) -> Option<Option<RuntimeValue>> {
    match e {
        VmError::FlowBreak(value) => Some(value.clone()),
        VmError::Located(inner, _) => flow_break_value(inner),
        _ => None,
    }
}

fn flow_continue(e: &VmError) -> bool {
    match e {
        VmError::FlowContinue => true,
        VmError::Located(inner, _) => flow_continue(inner),
        _ => false,
    }
}

fn builtin_error_message(e: &builtin::Error) -> String {
    match e {
        builtin::Error::UserDefined(message) => message.clone(),
        builtin::Error::InvalidBase64String(_) => "Invalid base64 string".to_string(),
        builtin::Error::NotDefined(name, _) | builtin::Error::UndefinedReference(name, _) => {
            format!("\"{name}\" is not defined")
        }
        builtin::Error::InvalidDateTimeFormat(msg) => format!("Unable to format date time, {msg}"),
        builtin::Error::InvalidTypes(name, args) => {
            let args = args.iter().map(|o| o.name().to_string()).collect::<Vec<_>>().join(", ");
            format!("Invalid types for \"{name}\", got {args}")
        }
        builtin::Error::InvalidNumberOfArguments(name, expected, actual) => {
            format!("Invalid number of arguments in \"{name}\", expected {expected}, got {actual}")
        }
        builtin::Error::InvalidRegularExpression(regex) => format!("Invalid regular expression \"{regex}\""),
        builtin::Error::Runtime(msg) => format!("Runtime error: {msg}"),
        builtin::Error::ZeroDivision => "Division by zero".to_string(),
        builtin::Error::AssignToImmutable(name) => format!("Cannot assign to immutable variable \"{name}\""),
        builtin::Error::UndefinedVariable(name) => format!("Undefined variable \"{name}\""),
        builtin::Error::InvalidConvert(format) => format!("Invalid convert: {format}"),
    }
}

use crate::ast::constants::builtins;
