use super::bytecode::{BinaryOp, Chunk, OpCode, ParamBinding, ParamShape, SELF_SLOT, UpvalueSource};
use super::compiler::CompiledProgram;
use super::value::VmClosureValue;
use super::value::{Cell, Closure, Locals, StackValue, read_cell, write_cell};
use crate::ast::TokenId;
use crate::ast::constants::builtins;
use crate::number::Number;
use crate::runtime::builtin::{self, Args};
use crate::runtime::env::Env;
use crate::runtime::host::HostFunctions;
use crate::runtime::runtime_value::{self, RuntimeValue};
use crate::selector::Selector;
use crate::{Ident, Shared, SharedCell};
use std::collections::BTreeMap;
use std::fmt;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

/// Instructions between deadline checks.
const TIMEOUT_CHECK_INTERVAL: u32 = 1024;

static LEN_IDENT: LazyLock<Ident> = LazyLock::new(|| Ident::new(builtins::LEN));
static GET_IDENT: LazyLock<Ident> = LazyLock::new(|| Ident::new(builtins::GET));

/// Per-execution deadline and call-depth state.
struct ExecutionLimits {
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
    stack_pool: Vec<Vec<StackValue>>,
}

const MAX_POOLED_LOCAL_COUNT: usize = 256;

impl ExecutionLimits {
    fn new(timeout: Option<Duration>, max_call_stack_depth: u32, pools: ExecutionPools) -> Self {
        Self {
            deadline: timeout.map(|t| Instant::now() + t),
            timeout,
            step: 0,
            call_depth: 0,
            max_call_stack_depth,
            pools,
        }
    }

    fn into_pools(self) -> ExecutionPools {
        self.pools
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

    #[inline(always)]
    fn has_deadline(&self) -> bool {
        self.deadline.is_some()
    }

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

    fn take_locals(&mut self, count: u16, captures: bool) -> Locals {
        self.take_locals_with_initialized_prefix(count, 0, captures)
    }

    fn take_locals_with_initialized_prefix(&mut self, count: u16, initialized: usize, captures: bool) -> Locals {
        let locals = if captures {
            None
        } else {
            self.pools
                .local_pool
                .get_mut(count as usize)
                .and_then(|bucket| bucket.pop())
        };
        let locals = locals.unwrap_or_else(|| fresh_locals(count as usize, captures));
        locals.reset_from(initialized.min(count as usize));
        locals
    }

    fn recycle_locals(&mut self, locals: Locals) {
        const MAX_RETAINED_PER_LENGTH: usize = 8;
        let count = locals.len();
        if count >= MAX_POOLED_LOCAL_COUNT {
            return;
        }
        if count >= self.pools.local_pool.len() {
            self.pools.local_pool.resize_with(count + 1, Vec::new);
        }
        let bucket = &mut self.pools.local_pool[count];
        if bucket.len() < MAX_RETAINED_PER_LENGTH {
            bucket.push(locals);
        }
    }

    fn take_stack(&mut self) -> Vec<StackValue> {
        self.pools.stack_pool.pop().unwrap_or_else(|| Vec::with_capacity(8))
    }

    fn recycle_stack(&mut self, mut stack: Vec<StackValue>) {
        const MAX_RETAINED_FRAMES: usize = 32;
        stack.clear();
        if self.pools.stack_pool.len() < MAX_RETAINED_FRAMES {
            self.pools.stack_pool.push(stack);
        }
    }
}

#[cfg(feature = "debugger")]
use super::debug_symbols::DebugSlot;
#[cfg(feature = "debugger")]
use crate::ast::node::Node;

/// A read-only snapshot of a VM frame at a source evaluation boundary.
#[cfg(feature = "debugger")]
#[derive(Debug, Clone)]
pub(crate) struct DebugEvent {
    pub(crate) token_id: TokenId,
    pub(crate) node: Shared<Node>,
    pub(crate) current_value: RuntimeValue,
    pub(crate) bindings: Vec<(Ident, RuntimeValue)>,
    pub(crate) local_bindings: Vec<(Ident, RuntimeValue)>,
    pub(crate) upvalue_bindings: Vec<(Ident, RuntimeValue)>,
    pub(crate) call_stack: Vec<Shared<Node>>,
    #[cfg(feature = "debug-trace")]
    pub(crate) operand_stack: Vec<RuntimeValue>,
}

/// Receives VM debug boundaries. Implementations may inspect, but cannot mutate, frames.
#[cfg(feature = "debugger")]
pub(crate) trait DebugHook {
    fn on_boundary(&mut self, event: DebugEvent);

    fn on_explicit_breakpoint(&mut self, event: DebugEvent);
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
    #[cfg_attr(not(test), allow(dead_code))]
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

pub(crate) struct RunOptions<'a> {
    pub(crate) host_functions: &'a HostFunctions,
    pub(crate) timeout: Option<Duration>,
    pub(crate) max_call_stack_depth: u32,
    pub(crate) global_bindings: &'a [(Ident, RuntimeValue)],
}

struct CallSite<'a> {
    locals: &'a Locals,
    chunk: &'a Chunk,
    ip: usize,
}

/// Static properties of a direct fixed-arity closure call.
struct FixedClosureCall<'a> {
    closure: &'a Closure,
    argc: u8,
    remove_callee: bool,
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
    #[cfg(feature = "debugger")]
    let mut debug = DebugRuntime {
        hook: None,
        call_stack: Vec::new(),
        current_node: None,
    };
    run_impl_capturing_locals(
        compiled,
        input,
        bindings,
        options,
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
    let (result, captured, _) = run_impl_capturing_locals(
        compiled,
        input,
        bindings,
        options,
        ExecutionPools::default(),
        capture_names,
        &mut debug,
    );
    (result, captured)
}

fn run_impl(
    compiled: &CompiledProgram,
    input: RuntimeValue,
    options: RunOptions<'_>,
    pools: ExecutionPools,
    #[cfg(feature = "debugger")] debug: &mut DebugRuntime<'_>,
) -> (VmResult<RuntimeValue>, ExecutionPools) {
    run_impl_with_bindings(
        compiled,
        input,
        &[],
        options,
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
    run_impl_with_bindings(
        compiled,
        input,
        bindings,
        RunOptions {
            host_functions,
            timeout: None,
            max_call_stack_depth: crate::eval::Options::default().max_call_stack_depth,
            global_bindings: &[],
        },
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
    pools: ExecutionPools,
    #[cfg(feature = "debugger")] debug: &mut DebugRuntime<'_>,
) -> (VmResult<RuntimeValue>, ExecutionPools) {
    let mut env = Env::default();
    for (ident, value) in options.global_bindings {
        env.define(*ident, value.clone());
    }
    let placeholder_env: Shared<SharedCell<Env>> = Shared::new(SharedCell::new(env));
    let mut limits = ExecutionLimits::new(options.timeout, options.max_call_stack_depth, pools);
    let top_level_chunk = &compiled.chunks[0];
    let locals = limits.take_locals(top_level_chunk.local_count, top_level_chunk.captures_local_slots());
    locals.set(SELF_SLOT, StackValue::Value(input));
    if initial_bindings.len() + 1 > locals.len() {
        limits.recycle_locals(locals);
        return (
            Err(VmError::Corrupt("too many initial debug bindings")),
            limits.into_pools(),
        );
    }
    for (slot, value) in initial_bindings.iter().cloned().enumerate() {
        locals.set(slot as u16 + 1, StackValue::Value(value));
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
    )
    .and_then(|result| match result {
        StackValue::Value(v) => Ok(v),
        StackValue::Closure(_) => Err(VmError::Corrupt("top-level result is a closure")),
    });
    (result, limits.into_pools())
}

/// Like [`run_impl_with_bindings`], but captures `capture_names`' final slot values. Bypasses
/// `run_chunk`'s pooling wrapper to keep `locals` readable; not for use on a hot path.
fn run_impl_capturing_locals(
    compiled: &CompiledProgram,
    input: RuntimeValue,
    bindings: &[RuntimeValue],
    options: RunOptions<'_>,
    pools: ExecutionPools,
    capture_names: &[Ident],
    #[cfg(feature = "debugger")] debug: &mut DebugRuntime<'_>,
) -> (VmResult<RuntimeValue>, Vec<(Ident, RuntimeValue)>, ExecutionPools) {
    let mut env = Env::default();
    for (ident, value) in options.global_bindings {
        env.define(*ident, value.clone());
    }
    let placeholder_env: Shared<SharedCell<Env>> = Shared::new(SharedCell::new(env));
    let mut limits = ExecutionLimits::new(options.timeout, options.max_call_stack_depth, pools);
    let chunks = &compiled.chunks;
    let top_level_chunk = &chunks[0];
    let reusable_locals = !top_level_chunk.captures_local_slots();
    let locals = limits.take_locals(top_level_chunk.local_count, top_level_chunk.captures_local_slots());
    locals.set(SELF_SLOT, StackValue::Value(input));
    if bindings.len() + 1 > locals.len() {
        limits.recycle_locals(locals);
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
        env: &placeholder_env,
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
            top_level_chunk
                .local_names
                .iter()
                .position(|local| local == name)
                .and_then(|slot| locals.get_checked(slot as u16))
                .map(|value| (*name, into_runtime_value(value, chunks)))
        })
        .collect();
    execution.limits.recycle_stack(stack);
    if reusable_locals {
        execution.limits.recycle_locals(locals);
    }
    let result = raw_result.and_then(|result| match result {
        StackValue::Value(v) => Ok(v),
        StackValue::Closure(_) => Err(VmError::Corrupt("top-level result is a closure")),
    });
    (result, captured, limits.into_pools())
}

fn fresh_locals(count: usize, captures: bool) -> Locals {
    if captures {
        Locals::boxed(count)
    } else {
        Locals::flat(count)
    }
}

fn capture_upvalues(sources: &[UpvalueSource], locals: &Locals, upvalues: &[Cell]) -> Vec<Cell> {
    sources
        .iter()
        .map(|source| match source {
            UpvalueSource::Local(slot) => Shared::clone(locals.cell(*slot)),
            UpvalueSource::Upvalue(idx) => Shared::clone(&upvalues[*idx as usize]),
        })
        .collect()
}

fn call_stack_value(
    callee: StackValue,
    mut args: Vec<StackValue>,
    call_site: CallSite<'_>,
    chunks: &Shared<Vec<Chunk>>,
    execution: &mut ExecutionContext<'_>,
    #[cfg(feature = "debugger")] debug: &mut DebugRuntime<'_>,
) -> VmResult<StackValue> {
    if let StackValue::Value(RuntimeValue::NativeFunction(ident)) = callee {
        let arg_values: Vec<RuntimeValue> = args.into_iter().map(|a| into_runtime_value(a, chunks)).collect();
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
    let callee_chunk = &callee_chunks[callee_chunk_index as usize];
    let callee_locals = execution
        .limits
        .take_locals(callee_chunk.local_count, callee_chunk.captures_local_slots());
    callee_locals.set(SELF_SLOT, call_site.locals.get(SELF_SLOT));
    execution
        .limits
        .enter_call()
        .map_err(|e| locate(call_site.chunk, call_site.ip, e))?;
    if let Err(e) = bind_params(
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
    ) {
        execution.limits.exit_call();
        return Err(locate(call_site.chunk, call_site.ip, e));
    }
    #[cfg(feature = "debugger")]
    let caller_node = debug.current_node.clone();
    #[cfg(feature = "debugger")]
    let pushed_call = if let Some(node) = &caller_node {
        debug.call_stack.push(Shared::clone(node));
        true
    } else {
        false
    };
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

fn call_fixed_closure_from_stack(
    call: FixedClosureCall<'_>,
    stack: &mut Vec<StackValue>,
    call_site: CallSite<'_>,
    chunks: &Shared<Vec<Chunk>>,
    execution: &mut ExecutionContext<'_>,
    #[cfg(feature = "debugger")] debug: &mut DebugRuntime<'_>,
) -> VmResult<StackValue> {
    let closure = call.closure;
    let callee_chunk = &chunks[closure.chunk_index as usize];
    let Some(arity) = callee_chunk.param_shape.fixed_required_arity() else {
        return Err(locate(
            call_site.chunk,
            call_site.ip,
            VmError::Corrupt("fixed call has non-fixed parameters"),
        ));
    };
    let argc = call.argc as usize;
    let uses_implicit_self = arity > 0 && argc + 1 == arity;
    if argc != arity && !uses_implicit_self {
        return Err(locate(
            call_site.chunk,
            call_site.ip,
            VmError::ArityMismatch {
                expected: arity as u8,
                actual: argc as u8,
            },
        ));
    }
    if stack.len() < argc + usize::from(call.remove_callee) {
        return Err(locate(
            call_site.chunk,
            call_site.ip,
            VmError::Corrupt("stack underflow in fixed closure call"),
        ));
    }

    let initialized_slots = SELF_SLOT as usize + 1 + arity;
    let callee_locals = execution.limits.take_locals_with_initialized_prefix(
        callee_chunk.local_count,
        initialized_slots,
        callee_chunk.captures_local_slots(),
    );
    let self_value = call_site.locals.get(SELF_SLOT);
    let first_arg_slot = if uses_implicit_self {
        callee_locals.set(SELF_SLOT, self_value.clone());
        callee_locals.set(SELF_SLOT + 1, self_value);
        SELF_SLOT as usize + 2
    } else {
        callee_locals.set(SELF_SLOT, self_value);
        SELF_SLOT as usize + 1
    };
    for offset in (0..argc).rev() {
        let Some(value) = stack.pop() else {
            return Err(locate(
                call_site.chunk,
                call_site.ip,
                VmError::Corrupt("stack underflow while binding fixed-call arguments"),
            ));
        };
        callee_locals.set((first_arg_slot + offset) as u16, value);
    }
    if call.remove_callee && stack.pop().is_none() {
        return Err(locate(
            call_site.chunk,
            call_site.ip,
            VmError::Corrupt("stack underflow while removing fixed-call callee"),
        ));
    }

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
        closure.chunk_index,
        chunks,
        callee_locals,
        &closure.upvalues,
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
    callee_locals: &Locals,
    enclosing_upvalues: &[Cell],
    context: &mut ParameterContext<'_, '_>,
    #[cfg(feature = "debugger")] debug: &mut DebugRuntime<'_>,
) -> VmResult<()> {
    if let Some(arity) = shape.fixed_required_arity() {
        return bind_fixed_required_params(arity, args, callee_locals);
    }

    let arg_count = args.len();
    let param_count = shape.bindings.len();
    let use_self_param = parameter_uses_implicit_self(shape, arg_count)?;

    let mut bindings = shape.bindings.iter();
    let mut args = args.into_iter();

    if use_self_param && let Some(binding) = bindings.next() {
        let self_value = current_self(callee_locals);
        callee_locals.set(binding.slot(), StackValue::Value(self_value));
    }

    for binding in bindings {
        match binding {
            ParamBinding::Variadic(slot) => {
                let collected: Vec<RuntimeValue> = args
                    .by_ref()
                    .map(|arg| into_runtime_value(arg, context.chunks))
                    .collect();
                callee_locals.set(*slot, StackValue::Value(RuntimeValue::Array(Shared::new(collected))));
            }
            ParamBinding::Required(slot) => {
                let Some(value) = args.next() else {
                    return Err(VmError::ArityMismatch {
                        expected: param_count as u8,
                        actual: arg_count as u8,
                    });
                };
                callee_locals.set(*slot, value);
            }
            ParamBinding::Optional(slot, default_chunk, default_upvalues) => {
                if let Some(value) = args.next() {
                    callee_locals.set(*slot, value);
                } else {
                    let captured = capture_upvalues(default_upvalues, callee_locals, enclosing_upvalues);
                    let default_chunk_ref = &context.chunks[*default_chunk as usize];
                    let default_locals = context
                        .limits
                        .take_locals(default_chunk_ref.local_count, default_chunk_ref.captures_local_slots());
                    default_locals.set(SELF_SLOT, callee_locals.get(SELF_SLOT));
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
                    callee_locals.set(*slot, value);
                }
            }
        }
    }
    Ok(())
}

fn bind_fixed_required_params(arity: usize, args: Vec<StackValue>, callee_locals: &Locals) -> VmResult<()> {
    let arg_count = args.len();
    let first_arg_slot = if arg_count == arity {
        SELF_SLOT as usize + 1
    } else if arity > 0 && arg_count + 1 == arity {
        callee_locals.set(SELF_SLOT + 1, StackValue::Value(current_self(callee_locals)));
        SELF_SLOT as usize + 2
    } else {
        return Err(VmError::ArityMismatch {
            expected: arity as u8,
            actual: arg_count as u8,
        });
    };

    for (offset, value) in args.into_iter().enumerate() {
        callee_locals.set((first_arg_slot + offset) as u16, value);
    }
    Ok(())
}

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

fn into_runtime_value(v: StackValue, chunks: &Shared<Vec<Chunk>>) -> RuntimeValue {
    match v {
        StackValue::Value(rv) => rv,
        StackValue::Closure(closure) => {
            RuntimeValue::VmClosure(Shared::new(VmClosureValue::from_closure(chunks, &closure)))
        }
    }
}

fn current_self(locals: &Locals) -> RuntimeValue {
    match locals.get(SELF_SLOT) {
        StackValue::Value(v) => v,
        StackValue::Closure(_) => RuntimeValue::None,
    }
}

#[cfg(feature = "debugger")]
fn debug_bindings(chunk: &Chunk, locals: &Locals, upvalues: &[Cell]) -> Option<DebugBindings> {
    let mut bindings = Vec::with_capacity(chunk.debug_symbols.bindings().len());
    let mut local_bindings = Vec::new();
    let mut upvalue_bindings = Vec::new();
    for (name, slot) in chunk.debug_symbols.bindings() {
        let value = match slot {
            DebugSlot::Local(slot) => locals.get_checked(*slot),
            DebugSlot::Upvalue(slot) => upvalues.get(*slot as usize).map(read_cell),
        }?;
        if let StackValue::Value(value) = value {
            let binding = (*name, value);
            match slot {
                DebugSlot::Local(_) => local_bindings.push(binding.clone()),
                DebugSlot::Upvalue(_) => upvalue_bindings.push(binding.clone()),
            }
            bindings.push(binding);
        }
    }
    Some(DebugBindings {
        bindings,
        local_bindings,
        upvalue_bindings,
    })
}

#[cfg(feature = "debugger")]
struct DebugBindings {
    bindings: Vec<(Ident, RuntimeValue)>,
    local_bindings: Vec<(Ident, RuntimeValue)>,
    upvalue_bindings: Vec<(Ident, RuntimeValue)>,
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

                let DebugBindings {
                    bindings,
                    local_bindings,
                    upvalue_bindings,
                } = debug_bindings(chunk, locals, upvalues)
                    .ok_or_else(|| locate(chunk, ip, VmError::Corrupt("debug slot out of bounds")))?;
                if let Some(hook) = debug.hook.as_deref_mut() {
                    hook.on_boundary(DebugEvent {
                        token_id: *token_id,
                        node,
                        current_value: current_self(locals),
                        bindings,
                        local_bindings,
                        upvalue_bindings,
                        call_stack: debug.call_stack.clone(),
                        #[cfg(feature = "debug-trace")]
                        operand_stack: stack
                            .iter()
                            .cloned()
                            .map(|value| into_runtime_value(value, chunks))
                            .collect(),
                    });
                }
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

                let DebugBindings {
                    bindings,
                    local_bindings,
                    upvalue_bindings,
                } = debug_bindings(chunk, locals, upvalues)
                    .ok_or_else(|| locate(chunk, ip, VmError::Corrupt("debug slot out of bounds")))?;
                if let Some(hook) = debug.hook.as_deref_mut() {
                    hook.on_explicit_breakpoint(DebugEvent {
                        token_id: *token_id,
                        node,
                        current_value: current_self(locals),
                        bindings,
                        local_bindings,
                        upvalue_bindings,
                        call_stack: debug.call_stack.clone(),
                        #[cfg(feature = "debug-trace")]
                        operand_stack: stack
                            .iter()
                            .cloned()
                            .map(|value| into_runtime_value(value, chunks))
                            .collect(),
                    });
                }
            }
            OpCode::Const(idx) => stack.push(StackValue::Value(
                unsafe { chunk.constants.get_unchecked(*idx as usize) }.clone(),
            )),
            OpCode::PushNone => stack.push(StackValue::Value(RuntimeValue::None)),
            OpCode::GetLocal(slot) => stack.push(unsafe { locals.get_unchecked(*slot) }),
            OpCode::SetLocal(slot) => {
                let v = pop!();
                unsafe { locals.set_unchecked(*slot, v) };
            }
            OpCode::TeeLocal(slot) => {
                let top = stack
                    .last()
                    .ok_or_else(|| locate(chunk, ip, VmError::Corrupt("stack underflow in TeeLocal")))?
                    .clone();
                unsafe { locals.set_unchecked(*slot, top) };
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
                    binop(operation, a, b, locals, execution.env, execution.host_functions)
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
                    cmp_op(operation, a, b, locals, execution.env, execution.host_functions)
                        .map_err(|e| locate(chunk, ip, e))?,
                ));
            }
            OpCode::BinaryLocalLocal { op, left, right } => {
                let a = local_runtime_value(locals, *left, chunks)?;
                let b = local_runtime_value(locals, *right, chunks)?;
                stack.push(StackValue::Value(
                    eval_binary_op(*op, a, b, locals, execution.env, execution.host_functions)
                        .map_err(|e| locate(chunk, ip, e))?,
                ));
            }
            OpCode::BinaryLocalConst { op, local, constant } => {
                let a = local_runtime_value(locals, *local, chunks)?;
                let b = unsafe { chunk.constants.get_unchecked(*constant as usize) }.clone();
                stack.push(StackValue::Value(
                    eval_binary_op(*op, a, b, locals, execution.env, execution.host_functions)
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
                        &current_self(locals),
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
                        &current_self(locals),
                        execution.env,
                        execution.host_functions,
                    )
                    .map_err(|e| locate(chunk, ip, e))?,
                };
                stack.push(StackValue::Value(result));
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
                    &current_self(locals),
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
                let mut args = Vec::with_capacity(*argc as usize);
                for _ in 0..*argc {
                    args.push(pop!());
                }
                args.reverse();
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
                    let StackValue::Closure(closure) = stack.remove(callee_index) else {
                        unreachable!("callee was checked as a closure above");
                    };
                    let result = call_fixed_closure_from_stack(
                        FixedClosureCall {
                            closure: &closure,
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
    match run_chunk(
        try_closure.chunk_index,
        chunks,
        try_locals,
        &try_closure.upvalues,
        execution,
        #[cfg(feature = "debugger")]
        debug,
    ) {
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
            let value = run_chunk(
                catch_closure.chunk_index,
                chunks,
                catch_locals,
                &catch_closure.upvalues,
                execution,
                #[cfg(feature = "debugger")]
                debug,
            )?;
            Ok(TryCatchOutcome::Value(value))
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

/// Rare `:type` matching, kept out of `run_chunk_inner_impl` (see `handle_try_catch`).
#[cold]
#[inline(never)]
fn type_check(v: &RuntimeValue, type_str: &str) -> bool {
    match type_str {
        "string" => matches!(v, RuntimeValue::String(_)),
        "number" => matches!(v, RuntimeValue::Number(_)),
        "bool" => matches!(v, RuntimeValue::Boolean(_)),
        "array" => matches!(v, RuntimeValue::Array(_)),
        "dict" => matches!(v, RuntimeValue::Dict(_)),
        "bytes" => matches!(v, RuntimeValue::Bytes(_)),
        "markdown" => matches!(v, RuntimeValue::Markdown(_, _)),
        "function" => matches!(v, RuntimeValue::Function(_)),
        "symbol" => matches!(v, RuntimeValue::Symbol(_)),
        "none" => matches!(v, RuntimeValue::None),
        _ => match v {
            RuntimeValue::Markdown(node, _) => crate::selector::Selector::from_selector_str(&format!(".{type_str}"))
                .filter(|selector| !selector.is_attribute_selector())
                .is_some_and(|selector| builtin::eval_selector(node, &selector) != RuntimeValue::NONE),
            _ => false,
        },
    }
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
fn get_external_global(
    ident: Ident,
    chunk: &Chunk,
    ip: usize,
    env: &Shared<SharedCell<Env>>,
) -> VmResult<RuntimeValue> {
    #[cfg(not(feature = "sync"))]
    let resolved = env.borrow().resolve(ident);
    #[cfg(feature = "sync")]
    let resolved = env.read().unwrap().resolve(ident);
    resolved.map_err(|_| locate(chunk, ip, VmError::UndefinedGlobal(ident.to_string())))
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
    env: &Shared<SharedCell<Env>>,
    host_functions: &HostFunctions,
) -> VmResult<RuntimeValue> {
    if matches!(
        op,
        BinaryOp::Eq | BinaryOp::Ne | BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge
    ) {
        return cmp_op(op, a, b, locals, env, host_functions);
    }
    binop(op, a, b, locals, env, host_functions)
}

fn binop(
    op: BinaryOp,
    a: RuntimeValue,
    b: RuntimeValue,
    locals: &Locals,
    env: &Shared<SharedCell<Env>>,
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
        &current_self(locals),
        env,
        host_functions,
    )
}

fn cmp_op(
    op: BinaryOp,
    a: RuntimeValue,
    b: RuntimeValue,
    locals: &Locals,
    env: &Shared<SharedCell<Env>>,
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
        &current_self(locals),
        env,
        host_functions,
    )
}

fn call_builtin(
    ident: &crate::Ident,
    args: &[RuntimeValue],
    self_value: &RuntimeValue,
    env: &Shared<SharedCell<Env>>,
    host_functions: &HostFunctions,
) -> VmResult<RuntimeValue> {
    call_builtin_args(ident, args.iter().cloned().collect(), self_value, env, host_functions)
}

fn call_builtin_args(
    ident: &crate::Ident,
    args: Args,
    self_value: &RuntimeValue,
    env: &Shared<SharedCell<Env>>,
    host_functions: &HostFunctions,
) -> VmResult<RuntimeValue> {
    let host_args = host_functions.get(ident).map(|_| args.clone());
    match builtin::eval_builtin(self_value, ident, args, env) {
        Ok(v) => Ok(v),
        Err(builtin::Error::NotDefined(_, _)) => match host_functions.get(ident) {
            Some(host_fn) => {
                let Some(host_args) = host_args.as_deref() else {
                    return Err(VmError::Corrupt("host function arguments were not retained"));
                };
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| host_fn.call(host_args)))
                    .unwrap_or_else(|payload| {
                        Err(crate::runtime::host::HostFunctionError::new(format!(
                            "panic: {}",
                            crate::runtime::host::panic_message(&*payload)
                        )))
                    })
                    .map_err(|e| VmError::Host(*ident, e.message().to_string()))
            }
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

#[inline]
fn eval_compact_selector_expr(value: &RuntimeValue, selector: Selector) -> RuntimeValue {
    match value {
        RuntimeValue::Markdown(node, _) => builtin::eval_selector(node, &selector),
        _ => eval_selector_expr(value, &selector),
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
    map.insert(
        Ident::new("message"),
        RuntimeValue::String(Shared::new(error_message(e))),
    );
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

#[cfg(test)]
mod tests {
    use super::*;

    fn number(value: i64) -> StackValue {
        StackValue::Value(RuntimeValue::Number(value.into()))
    }

    fn value_at(locals: &Locals, slot: u16) -> RuntimeValue {
        match locals.get(slot) {
            StackValue::Value(value) => value,
            StackValue::Closure(_) => panic!("expected a runtime value"),
        }
    }

    #[test]
    fn fixed_required_binder_handles_explicit_and_implicit_self_arguments() {
        let explicit_locals = Locals::boxed(3);
        bind_fixed_required_params(2, vec![number(3), number(4)], &explicit_locals).unwrap();
        assert_eq!(value_at(&explicit_locals, 1), RuntimeValue::Number(3.into()));
        assert_eq!(value_at(&explicit_locals, 2), RuntimeValue::Number(4.into()));

        let implicit_locals = Locals::boxed(3);
        implicit_locals.set(0, number(10));
        bind_fixed_required_params(2, vec![number(4)], &implicit_locals).unwrap();
        assert_eq!(value_at(&implicit_locals, 1), RuntimeValue::Number(10.into()));
        assert_eq!(value_at(&implicit_locals, 2), RuntimeValue::Number(4.into()));
    }

    #[test]
    fn fixed_required_binder_rejects_invalid_arity() {
        let locals = Locals::boxed(1);
        assert!(matches!(
            bind_fixed_required_params(0, vec![number(1)], &locals),
            Err(VmError::ArityMismatch { expected: 0, actual: 1 })
        ));
    }
}
