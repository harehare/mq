//! Tarn: mq's bytecode VM, an alternative to the tree-walking evaluator (`eval.rs`).
//! Enabled via the `tarn` feature, which routes `Engine::eval`/`eval_compiled` here instead.
//!
//! VM closures have a `RuntimeValue::VmClosure` representation, so they can be stored in
//! collections and passed to native higher-order functions such as `partial`.

mod bytecode;
mod compiler;
#[cfg(feature = "debugger")]
mod debug_symbols;
#[cfg(feature = "debugger")]
mod debugger;
mod interpreter;
mod resolver;
pub(crate) mod value;

use crate::Shared;
#[cfg(all(feature = "tarn", not(feature = "debugger")))]
use crate::SharedCell;
use crate::TokenArena;
use crate::ast::Program;
use crate::ast::node::{Expr, Node, Pattern};
use crate::engine;
use crate::error;
use crate::eval::host::HostFunctions;
use crate::eval::runtime_value::RuntimeValue;
#[cfg(feature = "debug-trace")]
use crate::get_token;
use crate::module::resolver::std_resolver::StdModuleResolver;
use crate::{ModuleLoader, ModuleResolver};
use std::fmt;
#[cfg(feature = "debug-trace")]
use std::fmt::Write as _;
use std::time::Duration;

#[cfg(feature = "debugger")]
use crate::{Debugger, DebuggerHandler, SharedCell, Source};

#[derive(Debug)]
pub(crate) enum Error {
    Compile(compiler::CompileError),
    Vm(interpreter::VmError),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Compile(e) => write!(f, "{e}"),
            Error::Vm(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<compiler::CompileError> for Error {
    fn from(e: compiler::CompileError) -> Self {
        Error::Compile(e)
    }
}

impl From<interpreter::VmError> for Error {
    fn from(e: interpreter::VmError) -> Self {
        Error::Vm(e)
    }
}

impl Error {
    pub(crate) fn into_inner_error(self, token_arena: TokenArena) -> error::InnerError {
        match self {
            Error::Compile(compiler::CompileError::Module(e)) => error::InnerError::from(e),
            Error::Compile(other) => error::InnerError::from(compile_error_to_runtime_error(other, token_arena)),
            Error::Vm(e) => error::InnerError::from(vm_error_to_runtime_error(&e, token_arena)),
        }
    }
}

/// Compiles a program exactly as the Engine VM path would and renders its bytecode for diagnosis.
#[cfg(feature = "debug-trace")]
pub(crate) fn dump_bytecode<R: ModuleResolver>(
    program: &Program,
    token_arena: TokenArena,
    module_loader: ModuleLoader<R>,
    global_bindings: &[(crate::Ident, RuntimeValue)],
) -> Result<String, Error> {
    let global_names: Vec<crate::Ident> = global_bindings.iter().map(|(ident, _)| *ident).collect();
    let mut output = String::new();

    let Some((before, after)) = split_at_nodes(program) else {
        let compiled =
            compiler::compile_program_for_engine(program, token_arena.clone(), module_loader, &global_names)?;
        format_compiled_bytecode(&mut output, "main", &compiled, &token_arena);
        return Ok(output);
    };

    let input_compiled = compiler::compile_program_for_engine(
        &before.to_vec(),
        token_arena.clone(),
        module_loader.clone(),
        &global_names,
    )?;
    format_compiled_bytecode(&mut output, "per-input", &input_compiled, &token_arena);

    let aggregate_compiled = compiler::compile_program_for_engine(
        &program_after_nodes(before, after),
        token_arena.clone(),
        module_loader,
        &global_names,
    )?;
    output.push('\n');
    format_compiled_bytecode(&mut output, "nodes aggregate", &aggregate_compiled, &token_arena);
    Ok(output)
}

#[cfg(feature = "debug-trace")]
fn format_compiled_bytecode(
    output: &mut String,
    phase: &str,
    compiled: &compiler::CompiledProgram,
    token_arena: &TokenArena,
) {
    let _ = writeln!(output, "Tarn VM bytecode");
    let _ = writeln!(output, "  phase: {phase}");
    let _ = writeln!(output, "  chunks: {}", compiled.chunks.len());
    for (chunk_index, chunk) in compiled.chunks.iter().enumerate() {
        let _ = writeln!(output, "\nChunk {chunk_index}");
        let _ = writeln!(output, "  frame");
        let _ = writeln!(
            output,
            "    local slots: {} ({})",
            chunk.local_count,
            format_slot_names(&chunk.local_names)
        );
        let _ = writeln!(
            output,
            "    upvalues: {} ({})",
            chunk.upvalue_names.len(),
            format_slot_names(&chunk.upvalue_names)
        );
        let _ = writeln!(output, "  instructions");
        for (pc, opcode) in chunk.code.iter().enumerate() {
            let location = chunk.token_at(pc).map(|token_id| {
                let token = get_token(Shared::clone(token_arena), token_id);
                format!(" @ {}:{}", token.range.start.line + 1, token.range.start.column + 1)
            });
            let _ = writeln!(
                output,
                "    {pc:04}  {}{}",
                format_opcode(opcode),
                location.unwrap_or_default()
            );
        }
        if !chunk.constants.is_empty() {
            let _ = writeln!(output, "  constants");
            for (index, value) in chunk.constants.iter().enumerate() {
                let _ = writeln!(output, "    [{index}] {}", format_value(value));
            }
        }
    }
}

#[cfg(feature = "debug-trace")]
fn format_slot_names(names: &[crate::Ident]) -> String {
    names
        .iter()
        .enumerate()
        .map(|(slot, name)| {
            let name = name.to_string();
            if name.is_empty() {
                format!("{slot}:self")
            } else {
                format!("{slot}:{name}")
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(feature = "debug-trace")]
fn format_opcode(opcode: &bytecode::OpCode) -> String {
    match opcode {
        #[cfg(feature = "debugger")]
        bytecode::OpCode::StmtBoundary(_) => "StmtBoundary".to_string(),
        bytecode::OpCode::Const(index) => format!("Const {index}"),
        bytecode::OpCode::GetLocal(slot) => format!("GetLocal {slot}"),
        bytecode::OpCode::SetLocal(slot) => format!("SetLocal {slot}"),
        bytecode::OpCode::GetUpvalue(slot) => format!("GetUpvalue {slot}"),
        bytecode::OpCode::SetUpvalue(slot) => format!("SetUpvalue {slot}"),
        bytecode::OpCode::MakeStaticClosure(chunk) => format!("MakeStaticClosure chunk {chunk}"),
        bytecode::OpCode::CallBuiltin(name, argc) => format!("CallBuiltin {name}, argc={argc}"),
        bytecode::OpCode::CallLocal(slot, argc) => format!("CallLocal {slot}, argc={argc}"),
        bytecode::OpCode::CallValue(argc) => format!("CallValue argc={argc}"),
        other => format!("{other:?}"),
    }
}

#[cfg(feature = "debug-trace")]
fn format_value(value: &RuntimeValue) -> String {
    const MAX_CHARS: usize = 96;

    let rendered = value.to_string();
    let mut chars = rendered.chars();
    let preview: String = chars.by_ref().take(MAX_CHARS).collect();
    if chars.next().is_some() {
        format!("{preview}…")
    } else {
        preview
    }
}

/// Bytecode retained by [`crate::CompiledProgram`] for repeated VM evaluation. `after` is the
/// program compiled from the trailing half of a `nodes` split, run once against every input's
/// aggregated `before` result rather than per input; `None` for a program with no `nodes`.
#[cfg(all(feature = "tarn", not(feature = "debugger")))]
#[derive(Clone)]
pub(crate) struct CachedProgram {
    program: compiler::CompiledProgram,
    after: Option<compiler::CompiledProgram>,
    let_names: Vec<crate::Ident>,
    configuration: Vec<String>,
    execution_pools: Shared<SharedCell<interpreter::ExecutionPools>>,
}

#[cfg(all(feature = "tarn", not(feature = "debugger")))]
impl fmt::Debug for CachedProgram {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CachedProgram")
            .field("program", &self.program)
            .field("after", &self.after)
            .field("configuration", &self.configuration)
            .finish_non_exhaustive()
    }
}

/// Compiles an Engine program for repeated evaluation, recording any external module sources
/// that must remain unchanged before the bytecode is reused.
#[cfg(all(feature = "tarn", not(feature = "debugger")))]
fn compile_cached_program<R: ModuleResolver>(
    program: &Program,
    token_arena: TokenArena,
    module_loader: ModuleLoader<R>,
    configuration: Vec<String>,
) -> Result<CachedProgram, Error> {
    let (program, after, let_names) = if let Some((before, after)) = split_at_nodes(program) {
        let let_names = let_names_before_nodes(before);
        (
            compiler::compile_program_for_engine(
                &before.to_vec(),
                Shared::clone(&token_arena),
                module_loader.clone(),
                &[],
            )?,
            Some(compiler::compile_program_for_engine_with_bindings(
                &program_after_nodes(before, after),
                token_arena,
                module_loader,
                &let_names,
                &[],
            )?),
            let_names,
        )
    } else {
        (
            compiler::compile_program_for_engine(program, token_arena, module_loader, &[])?,
            None,
            Vec::new(),
        )
    };
    Ok(CachedProgram {
        program,
        after,
        let_names,
        configuration,
        execution_pools: Shared::new(SharedCell::new(interpreter::ExecutionPools::default())),
    })
}

/// Returns whether every external module compiled into this program still has identical source.
#[cfg(all(feature = "tarn", not(feature = "debugger")))]
fn cached_program_is_current<R: ModuleResolver>(
    compiled: &CachedProgram,
    module_loader: &ModuleLoader<R>,
    configuration: &[String],
) -> Result<bool, Error> {
    if compiled.configuration != configuration {
        return Ok(false);
    }
    let before_current = module_loader
        .dependencies_are_current(&compiled.program.module_dependencies)
        .map_err(compiler::CompileError::Module)?;
    let after_current = match &compiled.after {
        Some(after) => module_loader
            .dependencies_are_current(&after.module_dependencies)
            .map_err(compiler::CompileError::Module)?,
        None => true,
    };
    Ok(before_current && after_current)
}

/// Runs a bytecode program cached by [`compile_cached_program`] for every input.
#[cfg(all(feature = "tarn", not(feature = "debugger")))]
fn run_cached<I>(
    compiled: &CachedProgram,
    inputs: I,
    host_functions: &HostFunctions,
    timeout: Option<Duration>,
    max_call_stack_depth: u32,
) -> Result<Vec<RuntimeValue>, Error>
where
    I: Iterator<Item = RuntimeValue>,
{
    let mut pools = take_execution_pools(&compiled.execution_pools);
    let mut values = Vec::new();
    let mut let_bindings: Vec<(crate::Ident, RuntimeValue)> = Vec::new();
    for input in inputs {
        let result = run_for_input(input, |value| {
            let execution_pools = std::mem::take(&mut pools);
            if compiled.let_names.is_empty() {
                let (result, next_pools) = interpreter::run_with_globals_and_pools(
                    &compiled.program,
                    value,
                    host_functions,
                    timeout,
                    max_call_stack_depth,
                    &[],
                    execution_pools,
                );
                pools = next_pools;
                result
            } else {
                let (result, captured, next_pools) = interpreter::run_with_globals_capturing_locals(
                    &compiled.program,
                    value,
                    &[],
                    interpreter::RunOptions {
                        host_functions,
                        timeout,
                        max_call_stack_depth,
                        global_bindings: &[],
                    },
                    &compiled.let_names,
                    execution_pools,
                );
                pools = next_pools;
                if result.is_ok() {
                    let_bindings = captured;
                }
                result
            }
        });
        match result {
            Ok(value) => values.push(value),
            Err(error) => {
                restore_execution_pools(&compiled.execution_pools, pools);
                return Err(Error::from(error));
            }
        }
    }
    restore_execution_pools(&compiled.execution_pools, pools);
    let Some(after) = &compiled.after else {
        return Ok(values);
    };
    let input = RuntimeValue::Array(Shared::new(values));
    let result = if compiled.let_names.is_empty() {
        interpreter::run_with_globals(after, input, host_functions, timeout, max_call_stack_depth, &[])
    } else {
        let let_values: Vec<RuntimeValue> = let_bindings.into_iter().map(|(_, value)| value).collect();
        interpreter::run_with_globals_capturing_locals(
            after,
            input,
            &let_values,
            interpreter::RunOptions {
                host_functions,
                timeout,
                max_call_stack_depth,
                global_bindings: &[],
            },
            &[],
            interpreter::ExecutionPools::default(),
        )
        .0
    };
    match result? {
        RuntimeValue::Array(values) => Ok(Shared::unwrap_or_clone(values)),
        value => Ok(vec![value]),
    }
}

#[cfg(all(feature = "tarn", not(feature = "debugger"), not(feature = "sync")))]
fn take_execution_pools(pools: &Shared<SharedCell<interpreter::ExecutionPools>>) -> interpreter::ExecutionPools {
    std::mem::take(&mut *pools.borrow_mut())
}

#[cfg(all(feature = "tarn", not(feature = "debugger"), feature = "sync"))]
fn take_execution_pools(pools: &Shared<SharedCell<interpreter::ExecutionPools>>) -> interpreter::ExecutionPools {
    std::mem::take(&mut *pools.write().expect("execution pool lock is poisoned"))
}

#[cfg(all(feature = "tarn", not(feature = "debugger"), not(feature = "sync")))]
fn restore_execution_pools(
    pools: &Shared<SharedCell<interpreter::ExecutionPools>>,
    execution_pools: interpreter::ExecutionPools,
) {
    *pools.borrow_mut() = execution_pools;
}

#[cfg(all(feature = "tarn", not(feature = "debugger"), feature = "sync"))]
fn restore_execution_pools(
    pools: &Shared<SharedCell<interpreter::ExecutionPools>>,
    execution_pools: interpreter::ExecutionPools,
) {
    *pools.write().expect("execution pool lock is poisoned") = execution_pools;
}

fn compile_error_to_runtime_error(
    err: compiler::CompileError,
    token_arena: TokenArena,
) -> error::runtime::RuntimeError {
    use error::runtime::RuntimeError;
    // `TokenId::new(0)` (the dummy EOF token every arena starts with) is a defensive
    // fallback for `CompileError::Module`, which `into_inner_error` never actually routes
    // here — every other variant always carries a real token.
    let token_id = err.token_id().unwrap_or(crate::ast::TokenId::new(0));
    let token = (*crate::get_token(token_arena, token_id)).clone();
    match err {
        compiler::CompileError::UndefinedIdent(name, _) => RuntimeError::UndefinedReference(token, name, Box::new([])),
        compiler::CompileError::Unsupported(what, _) => RuntimeError::Runtime(token, format!("unsupported: {what}")),
        compiler::CompileError::UnsupportedExpr(what, _) => {
            RuntimeError::Runtime(token, format!("unsupported expression: {what}"))
        }
        compiler::CompileError::AssignToImmutable(name, _) => RuntimeError::AssignToImmutable(token, name),
        compiler::CompileError::InvalidBytecode(message) => RuntimeError::Runtime(token, message),
        compiler::CompileError::Module(_) => unreachable!("routed to InnerError::Module by into_inner_error instead"),
    }
}

pub(crate) fn vm_error_to_runtime_error(
    err: &interpreter::VmError,
    token_arena: TokenArena,
) -> error::runtime::RuntimeError {
    use error::runtime::RuntimeError;
    use interpreter::VmError;
    match err {
        VmError::Located(inner, token_id) => {
            let token_id = *token_id;
            let token = (*crate::get_token(Shared::clone(&token_arena), token_id)).clone();
            match &**inner {
                VmError::Builtin(e) => e.to_runtime_error(token_id, token_arena),
                VmError::Host(name, msg) => RuntimeError::HostFunctionError(
                    token,
                    name.to_string().into_boxed_str(),
                    msg.clone().into_boxed_str(),
                ),
                VmError::ZeroDivision => RuntimeError::ZeroDivision(token),
                VmError::NotCallable => RuntimeError::InvalidDefinition(token, "value is not callable".to_string()),
                VmError::EnvNotFound(name) => RuntimeError::EnvNotFound(token, name.clone().into()),
                VmError::UndefinedGlobal(name) => RuntimeError::UndefinedReference(token, name.clone(), Box::new([])),
                VmError::ArityMismatch { expected, actual } => RuntimeError::InvalidNumberOfArguments {
                    token,
                    // The callee's name isn't threaded through `CallValue` — see
                    // `VmError::ArityMismatch`'s own doc comment.
                    name: String::new(),
                    expected: *expected,
                    actual: *actual,
                },
                VmError::FlowBreak(_) => RuntimeError::Runtime(token, "break outside a loop".to_string()),
                VmError::FlowContinue => RuntimeError::Runtime(token, "continue outside a loop".to_string()),
                VmError::DestructuringFailed => RuntimeError::DestructuringFailed(token),
                VmError::InvalidForeachTarget(repr) => RuntimeError::InvalidTypes {
                    token,
                    name: crate::TokenKind::Foreach.to_string(),
                    args: vec![repr.clone().into()],
                },
                VmError::Timeout(d) => RuntimeError::Timeout(*d),
                VmError::RecursionError(max) => RuntimeError::RecursionError(*max),
                VmError::Corrupt(what) => RuntimeError::Runtime(token, format!("corrupt bytecode: {what}")),
                nested @ VmError::Located(..) => vm_error_to_runtime_error(nested, token_arena),
            }
        }
        // Shouldn't happen (see doc comment above) — fall back to the arena's dummy token.
        other => {
            let token = (*crate::get_token(token_arena, crate::ast::TokenId::new(0))).clone();
            RuntimeError::Runtime(token, other.to_string())
        }
    }
}

pub(crate) fn compile_and_run(program: &Program, token_arena: TokenArena) -> Result<RuntimeValue, Error> {
    compile_and_run_full(
        program,
        RuntimeValue::None,
        &HostFunctions::default(),
        None,
        token_arena,
    )
}

#[cfg(test)]
pub(crate) fn compile_and_run_with_input(
    program: &Program,
    input: RuntimeValue,
    token_arena: TokenArena,
) -> Result<RuntimeValue, Error> {
    compile_and_run_full(program, input, &HostFunctions::default(), None, token_arena)
}

pub(crate) fn compile_and_run_full(
    program: &Program,
    input: RuntimeValue,
    host_functions: &HostFunctions,
    timeout: Option<Duration>,
    token_arena: TokenArena,
) -> Result<RuntimeValue, Error> {
    let compiled = compiler::compile_program(program, token_arena, ModuleLoader::new(StdModuleResolver))?;
    Ok(interpreter::run(
        &compiled,
        input,
        host_functions,
        timeout,
        crate::eval::Options::default().max_call_stack_depth,
    )?)
}

fn run_for_input<F>(input: RuntimeValue, mut run_one: F) -> Result<RuntimeValue, interpreter::VmError>
where
    F: FnMut(RuntimeValue) -> Result<RuntimeValue, interpreter::VmError>,
{
    match input {
        RuntimeValue::Markdown(node, _) => node
            .map_values(
                &mut |child_node: &mq_markdown::Node| -> Result<mq_markdown::Node, interpreter::VmError> {
                    let value = run_one(RuntimeValue::new_markdown(child_node.clone()))?;
                    Ok(markdown_child_result(value, child_node))
                },
            )
            .map(RuntimeValue::new_markdown),
        other => run_one(other),
    }
}

/// Converts one child node's query result back into the markdown tree it came from —
/// mirrors `eval::Evaluator::eval_markdown_node`'s conversion table exactly.
fn markdown_child_result(value: RuntimeValue, child_node: &mq_markdown::Node) -> mq_markdown::Node {
    match value {
        RuntimeValue::None => child_node.to_fragment(),
        RuntimeValue::Function(..) | RuntimeValue::NativeFunction(_) | RuntimeValue::Module(_) => {
            mq_markdown::Node::Empty
        }
        #[cfg(feature = "tarn")]
        RuntimeValue::VmClosure(_) => mq_markdown::Node::Empty,
        RuntimeValue::Array(arr) => arr
            .iter()
            .filter_map(|v| if v.is_none() { None } else { Some(v.to_string()) })
            .collect::<Vec<_>>()
            .join("\n")
            .into(),
        RuntimeValue::Dict(_)
        | RuntimeValue::Boolean(_)
        | RuntimeValue::Number(_)
        | RuntimeValue::String(_)
        | RuntimeValue::Bytes(_) => value.to_string().into(),
        RuntimeValue::Symbol(i) => i.as_str().into(),
        RuntimeValue::Markdown(node, _) => Shared::unwrap_or_clone(node),
    }
}

type ProgramSlice<'a> = &'a [Shared<Node>];

/// Engine-provided services needed to compile and execute a VM program.
pub(crate) struct EngineRunContext<'a, R: ModuleResolver> {
    pub(crate) host_functions: &'a HostFunctions,
    pub(crate) timeout: Option<Duration>,
    pub(crate) max_call_stack_depth: u32,
    pub(crate) token_arena: TokenArena,
    pub(crate) module_loader: ModuleLoader<R>,
    pub(crate) global_bindings: &'a [(crate::Ident, RuntimeValue)],
}

#[cfg(feature = "debugger")]
pub(crate) struct DebugRunContext<'a, R: ModuleResolver> {
    pub(crate) engine: EngineRunContext<'a, R>,
    pub(crate) debugger: Shared<SharedCell<Debugger>>,
    pub(crate) handler: Shared<SharedCell<Box<dyn DebuggerHandler>>>,
    pub(crate) source: Source,
}

/// Replays Engine-loaded modules as directives ahead of `program`. `None` when there's nothing
/// to replay, so the caller keeps using `program` unchanged.
pub(crate) fn build_program(
    program: &Program,
    token_arena: TokenArena,
    module_prelude: &[engine::VmModulePrelude],
) -> Result<Option<Program>, Box<error::Error>> {
    if module_prelude.is_empty() && !program.iter().any(|node| node.is_nodes()) {
        return Ok(None);
    }
    let mut prelude_program = Program::new();
    for module in module_prelude {
        let directive = match module {
            engine::VmModulePrelude::Include(name) => format!("include {name:?}"),
            engine::VmModulePrelude::Import(name) => format!("import {name:?}"),
        };
        prelude_program.extend(crate::parse(&directive, Shared::clone(&token_arena))?);
    }
    let mut result = prelude_program.clone();
    if let Some(nodes_index) = program.iter().position(|node| node.is_nodes()) {
        result.extend(program[..=nodes_index].iter().cloned());
        result.extend(prelude_program);
        result.extend(program[nodes_index + 1..].iter().cloned());
    } else {
        result.extend(program.iter().cloned());
    }
    Ok(Some(result))
}

/// Everything `Engine::eval_compiled_vm` needs to run a compiled program on Tarn.
pub(crate) struct TarnVm<'a, R: ModuleResolver> {
    pub(crate) engine: EngineRunContext<'a, R>,
    #[cfg(all(feature = "tarn", not(feature = "debugger")))]
    pub(crate) module_prelude: &'a [engine::VmModulePrelude],
    #[cfg(feature = "debugger")]
    pub(crate) debugger: Shared<SharedCell<Debugger>>,
    #[cfg(feature = "debugger")]
    pub(crate) debugger_handler: Shared<SharedCell<Box<dyn DebuggerHandler>>>,
    #[cfg(feature = "debugger")]
    pub(crate) source: Source,
}

impl<'a, R: ModuleResolver> TarnVm<'a, R> {
    #[cfg(all(feature = "tarn", not(feature = "debugger")))]
    fn cache_configuration(&self) -> Vec<String> {
        self.module_prelude
            .iter()
            .map(|module| match module {
                engine::VmModulePrelude::Include(name) => format!("include:{name}"),
                engine::VmModulePrelude::Import(name) => format!("import:{name}"),
            })
            .collect()
    }

    /// Runs `program` against `input`, using cached bytecode when valid (non-debugger builds).
    pub(crate) fn run<I>(
        &self,
        #[cfg_attr(any(not(feature = "tarn"), feature = "debugger"), allow(unused_variables))]
        compiled: &engine::CompiledProgram,
        program: &Program,
        input: I,
    ) -> Result<Vec<RuntimeValue>, Error>
    where
        I: Iterator<Item = RuntimeValue>,
    {
        #[cfg(all(feature = "tarn", not(feature = "debugger")))]
        if self.engine.global_bindings.is_empty()
            && let Some(cached) = compiled.cached_vm_program()
        {
            let cache_configuration = self.cache_configuration();
            let cached = match cached {
                Some(cached)
                    if cached_program_is_current(&cached, &self.engine.module_loader, &cache_configuration)? =>
                {
                    cached
                }
                _ => compile_cached_program(
                    program,
                    Shared::clone(&self.engine.token_arena),
                    self.engine.module_loader.with_same_resolver(),
                    cache_configuration,
                )?,
            };
            compiled.cache_vm_program(cached.clone());
            return run_cached(
                &cached,
                input,
                self.engine.host_functions,
                self.engine.timeout,
                self.engine.max_call_stack_depth,
            );
        }
        #[cfg(feature = "debugger")]
        {
            compile_and_run_debugged(
                program,
                input,
                DebugRunContext {
                    engine: EngineRunContext {
                        host_functions: self.engine.host_functions,
                        timeout: self.engine.timeout,
                        max_call_stack_depth: self.engine.max_call_stack_depth,
                        token_arena: Shared::clone(&self.engine.token_arena),
                        module_loader: self.engine.module_loader.with_same_resolver(),
                        global_bindings: self.engine.global_bindings,
                    },
                    debugger: Shared::clone(&self.debugger),
                    handler: Shared::clone(&self.debugger_handler),
                    source: self.source.clone(),
                },
            )
        }
        #[cfg(not(feature = "debugger"))]
        {
            compile_and_run_many(
                program,
                input,
                EngineRunContext {
                    host_functions: self.engine.host_functions,
                    timeout: self.engine.timeout,
                    max_call_stack_depth: self.engine.max_call_stack_depth,
                    token_arena: Shared::clone(&self.engine.token_arena),
                    module_loader: self.engine.module_loader.with_same_resolver(),
                    global_bindings: self.engine.global_bindings,
                },
            )
        }
    }
}

fn split_at_nodes(program: &Program) -> Option<(ProgramSlice<'_>, ProgramSlice<'_>)> {
    let index = program.iter().position(|node| node.is_nodes())?;
    Some(program.split_at(index))
}

/// Prepends `def`/`import`/`include`/`module` from before a `nodes` split into the
/// after-program, since Tarn compiles `before`/`after` separately.
fn program_after_nodes(before: ProgramSlice<'_>, after: ProgramSlice<'_>) -> Program {
    before
        .iter()
        .filter(|node| {
            matches!(
                *node.expr,
                Expr::Def(..) | Expr::Include(..) | Expr::Import(..) | Expr::Module(..)
            )
        })
        .cloned()
        .chain(after.iter().cloned())
        .collect()
}

/// Top-level `let`/`var` names declared before a `nodes` split (last input's value wins).
fn let_names_before_nodes(before: ProgramSlice<'_>) -> Vec<crate::Ident> {
    before
        .iter()
        .filter_map(|node| match &*node.expr {
            Expr::Let(Pattern::Ident(ident), _) | Expr::Var(Pattern::Ident(ident), _) => Some(ident.name),
            _ => None,
        })
        .collect()
}

fn run_nodes_aggregate<R: ModuleResolver>(
    before: ProgramSlice<'_>,
    after: ProgramSlice<'_>,
    values: Vec<RuntimeValue>,
    let_bindings: &[(crate::Ident, RuntimeValue)],
    context: &EngineRunContext<'_, R>,
) -> Result<Vec<RuntimeValue>, Error> {
    let let_names: Vec<crate::Ident> = let_bindings.iter().map(|(ident, _)| *ident).collect();
    let global_names: Vec<crate::Ident> = context.global_bindings.iter().map(|(ident, _)| *ident).collect();
    let program = program_after_nodes(before, after);
    let input = RuntimeValue::Array(Shared::new(values));
    let result = if let_names.is_empty() {
        let compiled = compiler::compile_program_for_engine(
            &program,
            Shared::clone(&context.token_arena),
            context.module_loader.clone(),
            &global_names,
        )?;
        interpreter::run_with_globals(
            &compiled,
            input,
            context.host_functions,
            context.timeout,
            context.max_call_stack_depth,
            context.global_bindings,
        )
    } else {
        let let_values: Vec<RuntimeValue> = let_bindings.iter().map(|(_, value)| value.clone()).collect();
        let compiled = compiler::compile_program_for_engine_with_bindings(
            &program,
            Shared::clone(&context.token_arena),
            context.module_loader.clone(),
            &let_names,
            &global_names,
        )?;
        interpreter::run_with_globals_capturing_locals(
            &compiled,
            input,
            &let_values,
            interpreter::RunOptions {
                host_functions: context.host_functions,
                timeout: context.timeout,
                max_call_stack_depth: context.max_call_stack_depth,
                global_bindings: context.global_bindings,
            },
            &[],
            interpreter::ExecutionPools::default(),
        )
        .0
    };
    match result? {
        RuntimeValue::Array(values) => Ok(Shared::unwrap_or_clone(values)),
        value => Ok(vec![value]),
    }
}

// Used when the `debugger` feature is off; `compile_and_run_debugged` takes over below
// when it's on, which is genuinely dead under `--all-features`.
#[cfg_attr(feature = "debugger", allow(dead_code))]
fn compile_and_run_many<I, R: ModuleResolver>(
    program: &Program,
    inputs: I,
    context: EngineRunContext<'_, R>,
) -> Result<Vec<RuntimeValue>, Error>
where
    I: Iterator<Item = RuntimeValue>,
{
    let global_names: Vec<crate::Ident> = context.global_bindings.iter().map(|(ident, _)| *ident).collect();
    let Some((before, after)) = split_at_nodes(program) else {
        let compiled =
            compiler::compile_program_for_engine(program, context.token_arena, context.module_loader, &global_names)?;
        return inputs
            .map(|input| {
                run_for_input(input, |v| {
                    interpreter::run_with_globals(
                        &compiled,
                        v,
                        context.host_functions,
                        context.timeout,
                        context.max_call_stack_depth,
                        context.global_bindings,
                    )
                })
                .map_err(Error::from)
            })
            .collect();
    };
    let compiled = compiler::compile_program_for_engine(
        &before.to_vec(),
        Shared::clone(&context.token_arena),
        context.module_loader.clone(),
        &global_names,
    )?;
    let let_names = let_names_before_nodes(before);
    let mut let_bindings: Vec<(crate::Ident, RuntimeValue)> = Vec::new();
    let values = if let_names.is_empty() {
        inputs
            .map(|input| {
                run_for_input(input, |v| {
                    interpreter::run_with_globals(
                        &compiled,
                        v,
                        context.host_functions,
                        context.timeout,
                        context.max_call_stack_depth,
                        context.global_bindings,
                    )
                })
                .map_err(Error::from)
            })
            .collect::<Result<Vec<_>, _>>()?
    } else {
        inputs
            .map(|input| {
                run_for_input(input, |v| {
                    let (result, captured, _) = interpreter::run_with_globals_capturing_locals(
                        &compiled,
                        v,
                        &[],
                        interpreter::RunOptions {
                            host_functions: context.host_functions,
                            timeout: context.timeout,
                            max_call_stack_depth: context.max_call_stack_depth,
                            global_bindings: context.global_bindings,
                        },
                        &let_names,
                        interpreter::ExecutionPools::default(),
                    );
                    if result.is_ok() {
                        let_bindings = captured;
                    }
                    result
                })
                .map_err(Error::from)
            })
            .collect::<Result<Vec<_>, _>>()?
    };
    run_nodes_aggregate(before, after, values, &let_bindings, &context)
}

#[cfg(feature = "debugger")]
fn compile_and_run_debugged<I, R: ModuleResolver>(
    program: &Program,
    inputs: I,
    context: DebugRunContext<'_, R>,
) -> Result<Vec<RuntimeValue>, Error>
where
    I: Iterator<Item = RuntimeValue>,
{
    let global_names: Vec<crate::Ident> = context.engine.global_bindings.iter().map(|(ident, _)| *ident).collect();
    let Some((before, after)) = split_at_nodes(program) else {
        let compiled = compiler::compile_program_for_engine(
            program,
            Shared::clone(&context.engine.token_arena),
            context.engine.module_loader.clone(),
            &global_names,
        )?;
        let mut hook = debugger::VmDebuggerHook::new(
            context.debugger,
            context.handler,
            context.engine.token_arena,
            context.source,
            compiled.debug_sources.clone(),
        );
        return inputs
            .map(|input| {
                match run_for_input(input, |v| {
                    interpreter::run_with_debug_hook_and_globals(
                        &compiled,
                        v,
                        context.engine.host_functions,
                        context.engine.timeout,
                        context.engine.max_call_stack_depth,
                        context.engine.global_bindings,
                        &mut hook,
                    )
                }) {
                    Ok(value) => Ok(value),
                    Err(error) => {
                        hook.notify_error(&error);
                        Err(error.into())
                    }
                }
            })
            .collect();
    };
    let compiled = compiler::compile_program_for_engine(
        &before.to_vec(),
        Shared::clone(&context.engine.token_arena),
        context.engine.module_loader.clone(),
        &global_names,
    )?;
    let mut hook = debugger::VmDebuggerHook::new(
        context.debugger,
        context.handler,
        Shared::clone(&context.engine.token_arena),
        context.source,
        compiled.debug_sources.clone(),
    );
    let let_names = let_names_before_nodes(before);
    let mut let_bindings: Vec<(crate::Ident, RuntimeValue)> = Vec::new();
    let values = if let_names.is_empty() {
        inputs
            .map(|input| {
                match run_for_input(input, |v| {
                    interpreter::run_with_debug_hook_and_globals(
                        &compiled,
                        v,
                        context.engine.host_functions,
                        context.engine.timeout,
                        context.engine.max_call_stack_depth,
                        context.engine.global_bindings,
                        &mut hook,
                    )
                }) {
                    Ok(value) => Ok(value),
                    Err(error) => {
                        hook.notify_error(&error);
                        Err(Error::from(error))
                    }
                }
            })
            .collect::<Result<Vec<_>, Error>>()?
    } else {
        inputs
            .map(|input| {
                match run_for_input(input, |v| {
                    let (result, captured) = interpreter::run_with_debug_hook_and_globals_capturing_locals(
                        &compiled,
                        v,
                        &[],
                        interpreter::RunOptions {
                            host_functions: context.engine.host_functions,
                            timeout: context.engine.timeout,
                            max_call_stack_depth: context.engine.max_call_stack_depth,
                            global_bindings: context.engine.global_bindings,
                        },
                        &let_names,
                        &mut hook,
                    );
                    if result.is_ok() {
                        let_bindings = captured;
                    }
                    result
                }) {
                    Ok(value) => Ok(value),
                    Err(error) => {
                        hook.notify_error(&error);
                        Err(Error::from(error))
                    }
                }
            })
            .collect::<Result<Vec<_>, Error>>()?
    };
    let let_names: Vec<crate::Ident> = let_bindings.iter().map(|(ident, _)| *ident).collect();
    let program = program_after_nodes(before, after);
    let input = RuntimeValue::Array(Shared::new(values));
    let result = if let_names.is_empty() {
        let aggregate_compiled = compiler::compile_program_for_engine(
            &program,
            context.engine.token_arena,
            context.engine.module_loader,
            &global_names,
        )?;
        hook.set_sources(aggregate_compiled.debug_sources.clone());
        interpreter::run_with_debug_hook_and_globals(
            &aggregate_compiled,
            input,
            context.engine.host_functions,
            context.engine.timeout,
            context.engine.max_call_stack_depth,
            context.engine.global_bindings,
            &mut hook,
        )
    } else {
        let let_values: Vec<RuntimeValue> = let_bindings.iter().map(|(_, value)| value.clone()).collect();
        let aggregate_compiled = compiler::compile_program_for_engine_with_bindings(
            &program,
            context.engine.token_arena,
            context.engine.module_loader,
            &let_names,
            &global_names,
        )?;
        hook.set_sources(aggregate_compiled.debug_sources.clone());
        interpreter::run_with_debug_hook_and_globals_capturing_locals(
            &aggregate_compiled,
            input,
            &let_values,
            interpreter::RunOptions {
                host_functions: context.engine.host_functions,
                timeout: context.engine.timeout,
                max_call_stack_depth: context.engine.max_call_stack_depth,
                global_bindings: context.engine.global_bindings,
            },
            &[],
            &mut hook,
        )
        .0
    };
    match result {
        Ok(RuntimeValue::Array(values)) => Ok(Shared::unwrap_or_clone(values)),
        Ok(value) => Ok(vec![value]),
        Err(error) => {
            hook.notify_error(&error);
            Err(error.into())
        }
    }
}

/// Compiles and runs `code` with `bindings` predeclared as top-level slots — the VM
/// counterpart to `switch_env`, for a debug expression evaluated against a paused frame.
#[cfg(all(feature = "debugger", feature = "tarn"))]
pub(crate) fn eval_debug_expression<R: ModuleResolver>(
    program: &Program,
    token_arena: TokenArena,
    module_loader: ModuleLoader<R>,
    bindings: &[(crate::Ident, RuntimeValue)],
    host_functions: &HostFunctions,
) -> Result<RuntimeValue, Error> {
    let names: Vec<crate::Ident> = bindings.iter().map(|(name, _)| *name).collect();
    let values: Vec<RuntimeValue> = bindings.iter().map(|(_, value)| value.clone()).collect();
    let compiled = compiler::compile_debug_expression(program, token_arena, module_loader, &names)?;
    Ok(interpreter::run_debug_expression(&compiled, &values, host_functions)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Selector;
    use crate::ast::node::{self as ast, Args, MatchArm, Param, Pattern};
    use crate::error::runtime::RuntimeError;
    use crate::number::{INFINITE, NAN, Number};
    use crate::range::Range;
    use crate::{
        AstExpr, AstNode, DefaultModuleLoader, Ident, IdentWithToken, Program, Token, TokenKind, arena::Arena,
        error::InnerError, token_alloc,
    };
    use crate::{Shared, SharedCell};
    use proptest::prelude::*;
    use rstest::rstest;
    use smallvec::{SmallVec, smallvec};
    use std::f64::consts::PI;

    #[rstest]
    #[case::selector_chain(".h1 | .text")]
    #[case::builtin_calls("upcase(.) | trim(.)")]
    #[cfg(not(feature = "debugger"))]
    fn non_tail_pipe_stage_fuses_into_tee_local(#[case] code: &str) {
        use super::bytecode::OpCode;

        let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
        let program = crate::parse(code, Shared::clone(&token_arena)).unwrap();
        let compiled = compiler::compile_program(&program, token_arena, ModuleLoader::new(StdModuleResolver)).unwrap();

        assert!(
            compiled.chunks[0]
                .code
                .iter()
                .any(|op| matches!(op, OpCode::TeeLocal(_))),
            "{code:?} should fuse into TeeLocal, got {:?}",
            compiled.chunks[0].code
        );
        assert!(
            !compiled.chunks[0]
                .code
                .iter()
                .any(|op| matches!(op, OpCode::SetLocal(_)))
        );
    }

    #[test]
    fn tee_local_fusion_preserves_pipe_result() {
        assert_eq!(
            run_with_prelude(r#""  hello  " | trim() | upcase()"#),
            RuntimeValue::String(Shared::new("HELLO".to_string()))
        );
    }

    #[rstest::fixture]
    fn token_arena() -> Shared<SharedCell<Arena<Shared<Token>>>> {
        let token_arena = Shared::new(SharedCell::new(Arena::new(10)));
        token_alloc(
            &token_arena,
            &Shared::new(Token {
                kind: TokenKind::Eof,
                range: Range::default(),
                module_id: 1.into(),
            }),
        );
        token_arena
    }

    fn ast_node(expr: AstExpr) -> Shared<AstNode> {
        Shared::new(AstNode {
            token_id: 0.into(),
            expr: Shared::new(expr),
        })
    }

    fn ast_call(name: &str, args: Args) -> Shared<AstNode> {
        Shared::new(AstNode {
            token_id: 0.into(),
            expr: Shared::new(ast::Expr::Call(IdentWithToken::new(name), args)),
        })
    }

    // Keep the evaluator's AST table intact while executing every same case as a VM test.
    // The shared macro prevents the two engines' case lists from drifting apart.
    crate::eval_table_cases!(
        evaluator_table_cases_run_on_vm,
        token_arena,
        runtime_values,
        program,
        expected,
        {
            let host_functions = HostFunctions::default();
            let vm_result = compile_and_run_many(
                &program,
                runtime_values.into_iter(),
                EngineRunContext {
                    host_functions: &host_functions,
                    // Hand-built AST cases must never leave the VM test worker running forever.
                    timeout: Some(std::time::Duration::from_secs(1)),
                    max_call_stack_depth: crate::eval::Options::default().max_call_stack_depth,
                    token_arena,
                    module_loader: DefaultModuleLoader::default(),
                    global_bindings: &[],
                },
            );

            match expected {
                Ok(expected_values) => assert_eq!(
                    vm_result.expect("VM should accept a successful evaluator table case"),
                    expected_values,
                ),
                Err(_) => assert!(vm_result.is_err(), "VM should reject an evaluator error case"),
            }
        }
    );

    fn run(code: &str) -> RuntimeValue {
        let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
        let program = crate::parse(code, Shared::clone(&token_arena)).unwrap();
        compile_and_run(&program, token_arena).unwrap()
    }

    fn run_with_input(code: &str, input: RuntimeValue) -> RuntimeValue {
        let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
        let program = crate::parse(code, Shared::clone(&token_arena)).unwrap();
        compile_and_run_with_input(&program, input, token_arena).unwrap()
    }

    fn run_with_prelude(code: &str) -> RuntimeValue {
        let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
        let program = crate::parse(code, Shared::clone(&token_arena)).unwrap();
        let compiled =
            compiler::compile_program_with_builtin_prelude(&program, token_arena, ModuleLoader::new(StdModuleResolver))
                .unwrap();
        match interpreter::run(
            &compiled,
            RuntimeValue::None,
            &HostFunctions::default(),
            None,
            crate::eval::Options::default().max_call_stack_depth,
        ) {
            Ok(v) => v,
            Err(e) => panic!("{e}"),
        }
    }

    #[test]
    fn non_capturing_closures_use_chunk_static_storage() {
        use super::bytecode::OpCode;

        let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
        let program = crate::parse("let f = fn(x): x + 1; | f(2)", Shared::clone(&token_arena)).unwrap();
        let compiled = compiler::compile_program(&program, token_arena, ModuleLoader::new(StdModuleResolver)).unwrap();

        assert_eq!(compiled.chunks[0].static_closures.len(), 1);
        assert!(
            compiled.chunks[0]
                .code
                .iter()
                .any(|op| matches!(op, OpCode::MakeStaticClosure(0)))
        );
        assert!(
            compiled.chunks[0]
                .code
                .iter()
                .any(|op| matches!(op, OpCode::CallLocal(_, 1)))
        );
        assert_eq!(compiled.chunks[1].param_shape.fixed_required_arity(), Some(1));
    }

    #[test]
    fn local_binary_expressions_use_compact_bytecode() {
        use super::bytecode::OpCode;

        let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
        let program = crate::parse("let f = fn(x): x * 2; | f(2)", Shared::clone(&token_arena)).unwrap();
        let compiled = compiler::compile_program(&program, token_arena, ModuleLoader::new(StdModuleResolver)).unwrap();

        assert!(
            compiled.chunks[1]
                .code
                .iter()
                .any(|op| matches!(op, OpCode::BinaryLocalConst { .. }))
        );
    }

    #[test]
    fn top_level_def_calls_use_call_local() {
        use super::bytecode::OpCode;
        let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
        let program = crate::parse(
            r#"def identity(x): x; | foreach(i, range(0, 1000, 1)): identity(i);"#,
            Shared::clone(&token_arena),
        )
        .unwrap();
        let compiled = compiler::compile_program(&program, token_arena, ModuleLoader::new(StdModuleResolver)).unwrap();
        assert!(
            compiled
                .chunks
                .iter()
                .any(|c| c.code.iter().any(|op| matches!(op, OpCode::CallLocal(_, _)))),
            "top-level def call should compile to CallLocal, not the slower CallValue path"
        );
    }

    #[test]
    fn local_array_accesses_use_compact_bytecode() {
        use super::bytecode::OpCode;

        let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
        let program = crate::parse(
            "let values = [1, 2] | let index = 0 | len(values) + get(values, index)",
            Shared::clone(&token_arena),
        )
        .unwrap();
        let compiled = compiler::compile_program(&program, token_arena, ModuleLoader::new(StdModuleResolver)).unwrap();

        assert!(
            compiled.chunks[0]
                .code
                .iter()
                .any(|op| matches!(op, OpCode::ArrayLenLocal(_)))
        );
        assert!(
            compiled.chunks[0]
                .code
                .iter()
                .any(|op| matches!(op, OpCode::ArrayGetLocalAt { .. }))
        );
        assert_eq!(
            run("let values = [1] | let index = 0 | get(values, index) | get(values, index)"),
            RuntimeValue::Number(1.into())
        );
    }

    #[test]
    fn foreach_uses_the_specialized_iteration_opcode() {
        use super::bytecode::OpCode;

        let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
        let program = crate::parse("foreach(x, [1, 2]): x;", Shared::clone(&token_arena)).unwrap();
        let compiled = compiler::compile_program(&program, token_arena, ModuleLoader::new(StdModuleResolver)).unwrap();

        assert!(
            compiled.chunks[0]
                .code
                .iter()
                .any(|op| matches!(op, OpCode::ForeachNext { .. }))
        );
        assert!(
            compiled.chunks[0]
                .code
                .iter()
                .any(|op| matches!(op, OpCode::ForeachCollect(_)))
        );
        assert!(
            !compiled.chunks[0]
                .code
                .iter()
                .any(|op| matches!(op, OpCode::ArrayLen | OpCode::ArrayGetAt))
        );
    }

    #[test]
    fn capturing_closures_keep_dynamic_capture_sources() {
        use super::bytecode::OpCode;

        let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
        let program = crate::parse("let x = 1 | let f = fn(): x; | f()", Shared::clone(&token_arena)).unwrap();
        let compiled = compiler::compile_program(&program, token_arena, ModuleLoader::new(StdModuleResolver)).unwrap();

        assert!(
            compiled.chunks[0]
                .code
                .iter()
                .any(|op| matches!(op, OpCode::MakeClosure(_)))
        );
    }

    #[test]
    fn mutable_local_calls_keep_the_generic_value_call_path() {
        use super::bytecode::OpCode;

        let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
        let program = crate::parse("var f = fn(x): x + 1; | f(2)", Shared::clone(&token_arena)).unwrap();
        let compiled = compiler::compile_program(&program, token_arena, ModuleLoader::new(StdModuleResolver)).unwrap();

        assert!(
            compiled.chunks[0]
                .code
                .iter()
                .any(|op| matches!(op, OpCode::CallValue(1)))
        );
        assert!(
            !compiled.chunks[0]
                .code
                .iter()
                .any(|op| matches!(op, OpCode::CallLocal(_, _)))
        );
    }

    #[test]
    fn dynamic_fixed_call_keeps_the_callee_value_from_before_argument_evaluation() {
        assert_eq!(
            run("var f = fn(x): x + 1; | let update = fn(): f = fn(x): x + 2; | 0; | f(update())"),
            RuntimeValue::Number(1.into())
        );
    }

    #[test]
    fn engine_compiler_loads_only_reachable_soft_builtins() {
        let selected_arena = Shared::new(SharedCell::new(Arena::new(100)));
        let selected_program =
            crate::parse("range(0, 3, 1) | map(fn(x): x * 2;)", Shared::clone(&selected_arena)).unwrap();
        let selected = compiler::compile_program_for_engine(
            &selected_program,
            selected_arena,
            ModuleLoader::new(StdModuleResolver),
            &[],
        )
        .unwrap();

        let full_arena = Shared::new(SharedCell::new(Arena::new(100)));
        let full_program = crate::parse("range(0, 3, 1) | map(fn(x): x * 2;)", Shared::clone(&full_arena)).unwrap();
        let full = compiler::compile_program_with_builtin_prelude(
            &full_program,
            full_arena,
            ModuleLoader::new(StdModuleResolver),
        )
        .unwrap();

        assert!(selected.chunks.len() < full.chunks.len());
    }

    #[test]
    fn engine_compiler_skips_soft_builtins_shadowed_by_user_definitions() {
        let shadowed_arena = Shared::new(SharedCell::new(Arena::new(100)));
        let shadowed_program = crate::parse(
            "def identity(x): x; | foreach(i, range(0, 3, 1)): identity(i);",
            Shared::clone(&shadowed_arena),
        )
        .unwrap();
        let shadowed = compiler::compile_program_for_engine(
            &shadowed_program,
            shadowed_arena,
            ModuleLoader::new(StdModuleResolver),
            &[],
        )
        .unwrap();

        let plain_arena = Shared::new(SharedCell::new(Arena::new(100)));
        let plain_program = crate::parse(
            "def local_identity(x): x; | foreach(i, range(0, 3, 1)): local_identity(i);",
            Shared::clone(&plain_arena),
        )
        .unwrap();
        let plain = compiler::compile_program_for_engine(
            &plain_program,
            plain_arena,
            ModuleLoader::new(StdModuleResolver),
            &[],
        )
        .unwrap();

        // The soft builtin with the same name must not be compiled just because it exists
        // in `builtin.mq`; both equivalent user functions should yield the same chunks.
        assert_eq!(shadowed.chunks.len(), plain.chunks.len());
    }

    #[test]
    fn engine_compiler_loads_only_reachable_module_exports() {
        let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
        let program = crate::parse("include \"csv\" | csv_parse(true)", Shared::clone(&token_arena)).unwrap();
        let compiled =
            compiler::compile_program_for_engine(&program, token_arena, ModuleLoader::new(StdModuleResolver), &[])
                .unwrap();

        // The top-level chunk and `csv_parse`; the remaining CSV exports are not reachable.
        assert_eq!(compiled.chunks.len(), 2);
    }

    #[test]
    fn engine_compiler_reachable_prelude_cache_is_correct_across_different_queries() {
        fn compile_and_run_for_engine(code: &str) -> RuntimeValue {
            let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
            let program = crate::parse(code, Shared::clone(&token_arena)).unwrap();
            let compiled =
                compiler::compile_program_for_engine(&program, token_arena, ModuleLoader::new(StdModuleResolver), &[])
                    .unwrap();
            interpreter::run(
                &compiled,
                RuntimeValue::None,
                &HostFunctions::default(),
                None,
                crate::eval::Options::default().max_call_stack_depth,
            )
            .unwrap()
        }

        // Interleaves queries reaching disjoint soft-builtin sets, so a stale
        // `builtin_dependency_graph` cache entry from one would break another.
        assert_eq!(compile_and_run_for_engine("is_array(1)"), RuntimeValue::Boolean(false));
        assert_eq!(
            compile_and_run_for_engine("first([1, 2, 3])"),
            RuntimeValue::Number(1.into())
        );
        assert_eq!(compile_and_run_for_engine("is_array([1])"), RuntimeValue::Boolean(true));
    }

    #[test]
    #[cfg(not(feature = "debugger"))]
    fn non_tail_let_and_var_do_not_round_trip_self_through_local_zero() {
        use super::bytecode::{OpCode, SELF_SLOT};

        for code in [
            "let a = 1 | let b = 2 | a + b",
            "var a = 1 | var b = 2 | a + b",
            "[1, 2] | let [a, b] = [10, 20] | a + b",
            "def f(x): let a = x * 2 | let b = a + 1 | b; | f(5)",
        ] {
            let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
            let program = crate::parse(code, Shared::clone(&token_arena)).unwrap();
            let compiled =
                compiler::compile_program(&program, token_arena, ModuleLoader::new(StdModuleResolver)).unwrap();

            let has_self_round_trip = compiled.chunks.iter().any(|chunk| {
                chunk.code.windows(2).any(|pair| {
                    matches!(
                        pair,
                        [OpCode::GetLocal(a), OpCode::SetLocal(b)] if *a == SELF_SLOT && *b == SELF_SLOT
                    )
                })
            });
            assert!(
                !has_self_round_trip,
                "{code:?} should not round-trip self through GetLocal(SELF_SLOT)/SetLocal(SELF_SLOT)"
            );
        }
    }

    #[test]
    fn non_tail_let_and_var_preserve_self_across_bindings() {
        assert_eq!(
            run(r#""hello" | let a = 1 | var b = 2 | len()"#),
            RuntimeValue::Number(5.into())
        );
        assert_eq!(
            run("[1, 2, 3] | let [a, b] = [10, 20] | len()"),
            RuntimeValue::Number(3.into())
        );
        assert_eq!(
            run("def f(x): let a = x * 2 | let b = a + 1 | b; | f(5)"),
            RuntimeValue::Number(11.into())
        );
    }

    #[test]
    fn let_shadowing_a_loop_local_reuses_its_slot_instead_of_hanging() {
        let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
        let program = crate::parse(
            "let x = 0 | loop: let x = x + 1 | if(x > 5): break else: x;;",
            Shared::clone(&token_arena),
        )
        .unwrap();
        let result = compile_and_run_full(
            &program,
            RuntimeValue::None,
            &HostFunctions::default(),
            Some(std::time::Duration::from_secs(5)),
            token_arena,
        )
        .unwrap();
        assert_eq!(result, RuntimeValue::Number(5.0.into()));
    }

    #[test]
    fn let_shadowing_mutates_a_prior_closures_capture() {
        let result = run("let x = 1 | let f = fn(): x; | let x = 2 | f()");
        assert_eq!(result, RuntimeValue::Number(2.0.into()));
    }

    #[cfg(feature = "tarn")]
    #[test]
    fn vm_closure_stored_in_a_dict_is_callable_once_retrieved() {
        let result = run_with_prelude(r#"def f(): 1; | let d = {"name": "x", "func": f} | d["func"]()"#);
        assert_eq!(result, RuntimeValue::Number(1.0.into()));
    }

    #[cfg(feature = "tarn")]
    #[test]
    fn partial_works_on_a_vm_closure() {
        let result = run_with_prelude("def add(x, y): x + y; | let add5 = partial(add, 5) | add5(3)");
        assert_eq!(result, RuntimeValue::Number(8.0.into()));
    }

    #[rstest]
    #[case("1 + 2", 3.0)]
    #[case("10 - 4", 6.0)]
    #[case("3 * 4", 12.0)]
    #[case("10 / 4", 2.5)]
    #[case("10 % 3", 1.0)]
    #[case("-5 + 2", -3.0)]
    fn arithmetic(#[case] code: &str, #[case] expected: f64) {
        assert_eq!(run(code), RuntimeValue::Number(expected.into()));
    }

    #[test]
    fn paren_free_prelude_builtin_still_resolves_after_load_builtin_module() {
        let mut engine = crate::DefaultEngine::default();
        engine.load_builtin_module();
        let result = engine
            .eval(r#"sort(["b", "a", "c"]) | first"#, std::iter::once(RuntimeValue::None))
            .unwrap();
        assert_eq!(result.values()[0], RuntimeValue::String(Shared::new("a".to_string())));
    }

    #[rstest]
    #[case("1 < 2", true)]
    #[case("2 < 1", false)]
    #[case("2 <= 2", true)]
    #[case("3 > 2", true)]
    #[case("2 == 2", true)]
    #[case("2 != 3", true)]
    fn comparisons(#[case] code: &str, #[case] expected: bool) {
        assert_eq!(run(code), RuntimeValue::Boolean(expected));
    }

    #[rstest]
    #[case::if_true("if (1 < 2): 10 else: 20", 10.0)]
    #[case::if_false("if (2 < 1): 10 else: 20", 20.0)]
    #[case::let_binding("let x = 5 | x + 1", 6.0)]
    #[case::while_loop("var i = 0 | while (i < 5): i = i + 1; | i", 5.0)]
    #[case::recursive_fibonacci(
        "def fibonacci(x): if (x < 2): x else: fibonacci(x - 1) + fibonacci(x - 2); | fibonacci(10)",
        55.0
    )]
    #[case::closure_captures_outer_local("let x = 10 | let f = fn(y): x + y; | f(5)", 15.0)]
    #[case::nested_closure_captures_through_two_levels(
        "let x = 1 | let make = fn(y): fn(z): x + y + z;; | let add_y = make(2) | add_y(3)",
        6.0
    )]
    #[case::while_break("var x = 0 | while(x < 10): x += 1 | if(x == 3): break else: x;", 2.0)]
    #[case::while_continue("var x = 0 | while(x < 4): x += 1 | if(x == 3): continue else: x;", 4.0)]
    #[case::loop_break_with_value("loop: break: 42;", 42.0)]
    #[case::builtin_shadow_recursion_calls_builtin("def add(a, b): add(a, b) + 1; | add(1, 2)", 4.0)]
    #[case::foreach_sums_elements("var total = 0 | foreach(x, array(1, 2, 3, 4)): total = total + x; | total", 10.0)]
    #[case::foreach_continue_skips_element(
        "var total = 0 | foreach(x, array(1, 2, 3, 4)): if (x == 2): continue else: total = total + x; | total",
        8.0
    )]
    #[case::foreach_break_stops_early(
        "var total = 0 | foreach(x, array(1, 2, 3, 4)): if (x == 3): break else: total = total + x; | total",
        3.0
    )]
    #[case::foreach_collects_results_array("len(foreach(x, array(10, 20, 30)): x * 2;)", 3.0)]
    #[case::foreach_results_array_last_elem("get(foreach(x, array(10, 20, 30)): x * 2;, 2)", 60.0)]
    #[case::foreach_break_with_value_bypasses_array(
        "foreach(x, array(1, 2, 3)): if (x == 2): break: 999 else: x;",
        999.0
    )]
    #[case::try_without_error("try: 5 catch: 99;", 5.0)]
    #[case::try_bare_catch_on_error("try: 1 / 0 catch: 99;", 99.0)]
    #[case::try_catch_binder_sees_message("try: 1 / 0 catch(e): len(get(e, \"message\"));", 16.0)]
    #[case::match_ident_binding_with_guard("match (5) do | x if (x > 3): x * 10 | _: 0 end", 50.0)]
    #[case::match_wildcard_fallback("match (99) do | 1: 100 | _: 0 end", 0.0)]
    #[case::array_spread_len("len([1, 2, ...[3, 4, 5], 6])", 6.0)]
    #[case::array_spread_in_foreach_iterates_all("len(foreach(x, [0, ...[1, 2, 3]]): x + 1;)", 4.0)]
    #[case::match_array_rest_pattern("match([1, 2, 3, 4]) do | [first, ..rest]: len(rest) end", 3.0)]
    #[case::match_array_rest_binds_first("match([10, 20, 30]) do | [first, ..rest]: first end", 10.0)]
    #[case::match_array_exact_pattern("match([1, 2]) do | [a, b]: a + b | _: -1 end", 3.0)]
    #[case::match_array_exact_wrong_len("match([1, 2, 3]) do | [a, b]: a + b | _: -1 end", -1.0)]
    #[case::match_or_pattern_hit("match(2) do | 1 || 2 || 3: 1 | _: 0 end", 1.0)]
    #[case::match_or_pattern_miss("match(9) do | 1 || 2 || 3: 1 | _: 0 end", 0.0)]
    #[case::match_dict_pattern(r#"match({"x": 10}) do | {x: v}: v end"#, 10.0)]
    #[case::match_dict_pattern_missing_key(r#"match({"x": 10}) do | {y: v}: v | _: -1 end"#, -1.0)]
    #[case::default_param_used_when_supplied("def add(a, b = 10): a + b; | add(1, 2)", 3.0)]
    #[case::default_param_used_when_omitted("def add(a, b = 10): a + b; | add(1)", 11.0)]
    #[case::multiple_default_params_all_omitted("def msg(a, b = 2, c = 3): a + b + c; | msg(1)", 6.0)]
    #[case::multiple_default_params_some_supplied("def msg(a, b = 2, c = 3): a + b + c; | msg(1, 20)", 24.0)]
    #[case::default_param_can_reference_earlier_param("def f(a, b = a + 1): a + b; | f(10)", 21.0)]
    #[case::variadic_param_collects_remaining_args("def sum_all(*xs): len(xs); | sum_all(1, 2, 3, 4)", 4.0)]
    #[case::variadic_param_collects_nothing_when_absent("def sum_all(*xs): len(xs); | sum_all()", 0.0)]
    #[case::required_and_variadic_together("def f(first, *rest): first + len(rest); | f(100, 1, 2, 3)", 103.0)]
    #[case::implicit_self_fills_missing_required_arg("def double(x): x * 2; | 21 | double()", 42.0)]
    #[case::let_array_destruct("let [a, b] = [1, 2] | add(a, b)", 3.0)]
    #[case::let_array_wildcard("let [_, b] = [1, 2] | b", 2.0)]
    #[case::let_array_rest("let [first, ..rest] = [1, 2, 3] | len(rest)", 2.0)]
    #[case::var_array_destruct_then_reassign("var [a, b] = [1, 2] | a = 10 | a + b", 12.0)]
    #[case::module_inline_function("module math: def mysum(a, b): a + b; end | math::mysum(1, 2)", 3.0)]
    #[case::module_extension(
        "module math: def mysum(a, b): a + b; end module math: def mymul(a, b): a * b; end | math::mysum(2, 3) + math::mymul(2, 3)",
        11.0
    )]
    #[case::module_inline_let("module constants: let pi = 314 end | constants::pi", 314.0)]
    #[case::qualified_access_from_a_nested_closure(
        "module math: def mysum(a, b): a + b; end | let f = fn(x): math::mysum(x, 1); | f(41)",
        42.0
    )]
    #[case::qualified_access_from_a_doubly_nested_closure(
        "module math: def mysum(a, b): a + b; end | let f = fn(x): fn(y): math::mysum(x, y);; | let g = f(40) | g(2)",
        42.0
    )]
    fn programs_yield_number(#[case] code: &str, #[case] expected: f64) {
        assert_eq!(run(code), RuntimeValue::Number(expected.into()));
    }

    #[cfg(feature = "tarn")]
    #[rstest]
    #[case::arithmetic_and_assignment("var total = 1 | foreach(x, [2, 3, 4]): total += x; | total")]
    #[case::recursive_closure("let make = fn(x): fn(y): x + y;; | let add_two = make(2) | add_two(40)")]
    #[case::defaults_and_variadic("def f(first, rest = 2, *tail): first + rest + len(tail); | f(10, 20, 1, 2)")]
    #[case::try_continue("var total = 0 | foreach(x, array(1, 2, 3)) do try: continue catch: 0 end | total")]
    #[case::match_and_destructuring(
        "let [first, ..rest] = [10, 20, 30] | match(rest) do | [a, b]: first + a + b | _: 0 end"
    )]
    #[case::module_resolution("module math: def twice(x): x * 2; end | math::twice(21)")]
    #[case::string_interpolation(r#"let name = "mq" | s"hello, ${name}!""#)]
    #[case::array_spread("len([0, ...[1, 2, 3], 4])")]
    #[case::builtin_map(r#"range(0, 20, 1) | map(fn(x): x * 2;)"#)]
    #[case::builtin_filter(r#"range(0, 20, 1) | filter(fn(x): x % 3 == 0;)"#)]
    #[case::builtin_fold(r#"def sum(acc, x): add(acc, x); | fold(range(0, 20, 1), 0, sum)"#)]
    #[case::builtin_chain(
        r#"range(0, 50, 1) | filter(fn(x): x % 2 == 0;) | map(fn(x): x * 3;) | filter(fn(x): x > 10;)"#
    )]
    #[case::foreach_closures_share_the_loop_variables_captured_cell(
        "let fns = foreach(i, [1, 2, 3]): fn(): i;; | fns | map(fn(f): f();)"
    )]
    #[case::three_level_nested_closure(
        "let x = 1 | let make = fn(y): fn(z): fn(w): x + y + z + w;;; | let step1 = make(2) | let step2 = step1(3) | step2(4)"
    )]
    #[case::sibling_closures_capture_independent_lets(
        "let a = 1 | let b = 2 | let f = fn(): a; | let g = fn(): b; | f() + g()"
    )]
    #[case::closure_over_var_sees_later_mutation("var x = 1 | let f = fn(): x; | x = 99 | f()")]
    #[case::implicit_self_visible_inside_a_closure_created_by_a_def(
        "def outer(): let g = fn(): . + 1; | g(); | 41 | outer()"
    )]
    #[case::implicit_self_fill_combined_with_variadic("def f(first, *rest): first + len(rest); | 5 | f()")]
    #[case::shadowing_across_if_branches_does_not_leak_to_the_outer_binding(
        "let x = 1 | let inner_a = fn(): let x = 2 | x; | let inner_b = fn(): let x = 3 | x; | let r = if(true): inner_a() else: inner_b() | r + x"
    )]
    #[case::shadowing_in_sibling_closures_does_not_leak_between_them(
        "let f = fn(): let x = 1 | x; | let g = fn(): let x = 2 | x; | f() + g()"
    )]
    fn compiled_engine_matches_tree_walker(#[case] code: &str) {
        assert_vm_matches_tree_walker(code, vec![RuntimeValue::None]);
    }

    // Documents real (matching) behavior, not an aspirational "fresh cell per iteration"
    // semantics: both engines reuse the same captured binding for `foreach`'s loop variable
    // across iterations, so every closure created inside the loop sees the final value.
    #[cfg(feature = "tarn")]
    #[test]
    fn foreach_closures_share_the_loop_variables_captured_cell_exact_values() {
        assert_eq!(
            run_with_prelude("let fns = foreach(i, [1, 2, 3]): fn(): i;; | fns | map(fn(f): f();)"),
            RuntimeValue::Array(
                vec![
                    RuntimeValue::Number(3.0.into()),
                    RuntimeValue::Number(3.0.into()),
                    RuntimeValue::Number(3.0.into()),
                ]
                .into()
            )
        );
    }

    #[cfg(feature = "tarn")]
    #[rstest]
    #[case::self_and_pipe(". + 1 | . * 2", RuntimeValue::Number(42.0.into()))]
    #[case::multiple_inputs(". * .", RuntimeValue::Number(7.0.into()))]
    #[case::markdown_selector(".h1", heading(1))]
    fn compiled_engine_matches_tree_walker_with_input(#[case] code: &str, #[case] input: RuntimeValue) {
        assert_vm_matches_tree_walker(code, vec![input]);
    }

    #[cfg(feature = "tarn")]
    #[test]
    fn compiled_engine_matches_tree_walker_for_nodes_aggregation() {
        assert_vm_matches_tree_walker(
            ". * 10 | nodes | len()",
            vec![
                RuntimeValue::Number(1.0.into()),
                RuntimeValue::Number(2.0.into()),
                RuntimeValue::Number(3.0.into()),
            ],
        );
    }

    #[rstest]
    #[case::self_is_the_input_value(".", 42.0)]
    #[case::self_threads_through_pipe(". + 1 | . * 2", 86.0)]
    #[case::self_inside_function_call_is_caller_self("def f(): . + 1; | 42 | f()", 43.0)]
    #[case::let_preserves_pipeline_value("let saved = 10 | .", 42.0)]
    #[case::assignment_preserves_pipeline_value("var saved = 10 | saved = 20 | .", 42.0)]
    #[case::as_preserves_pipeline_value(". + 1 as saved | .", 42.0)]
    #[case::top_level_def_preserves_pipeline_value("def identity(): .; | identity()", 42.0)]
    #[case::unless_runs_for_falsy_condition("unless(false): . + 1;", 43.0)]
    #[case::until_runs_until_condition_is_true("until(. >= 45): . + 1;", 45.0)]
    fn programs_with_number_input_yield_number(#[case] code: &str, #[case] expected: f64) {
        assert_eq!(
            run_with_input(code, RuntimeValue::Number(42.0.into())),
            RuntimeValue::Number(expected.into())
        );
    }

    #[rstest]
    #[case::builtin_fallback_string_concat(r#""a" + "b""#, "ab")]
    #[case::while_break_with_value(
        "var x = 0 | while(x < 10): x += 1 | if(x == 5): break: \"found\" else: x;",
        "found"
    )]
    #[case::string_interpolation_expr(r#"let name = "World" | s"Hello, ${name}!""#, "Hello, World!")]
    #[case::string_interpolation_number(r#"let n = 1 + 2 | s"sum=${n}""#, "sum=3")]
    #[case::match_literal_arm("match (2) do | 1: \"one\" | 2: \"two\" | _: \"other\" end", "two")]
    #[case::match_type_pattern(
        "match (array(1, 2, 3)) do | :array: \"is_array\" | :number: \"is_number\" | _: \"other\" end",
        "is_array"
    )]
    #[case::let_dict_destruct(r#"let {name: n} = {"name": "Alice"} | n"#, "Alice")]
    fn programs_yield_string(#[case] code: &str, #[case] expected: &str) {
        assert_eq!(run(code), RuntimeValue::String(Shared::new(expected.to_string())));
    }

    #[test]
    fn false_if_without_else_yields_none() {
        assert_eq!(run("if(false): 1;"), RuntimeValue::None);
    }

    #[test]
    fn interpolation_can_reference_self() {
        assert_eq!(
            run_with_input(r#"s"value=${self}""#, RuntimeValue::Number(42.0.into())),
            RuntimeValue::String(Shared::new("value=42".to_string()))
        );
    }

    fn heading(depth: u8) -> RuntimeValue {
        RuntimeValue::new_markdown(mq_markdown::Node::Heading(mq_markdown::Heading {
            values: vec![],
            position: None,
            depth,
        }))
    }

    #[test]
    fn selector_matches_heading_of_the_right_depth() {
        assert_ne!(run_with_input(".h1", heading(1)), RuntimeValue::None);
    }

    #[test]
    fn selector_does_not_match_heading_of_a_different_depth() {
        assert_eq!(run_with_input(".h1", heading(2)), RuntimeValue::None);
    }

    #[test]
    fn selector_does_not_match_non_markdown_input() {
        assert_eq!(
            run_with_input(".h1", RuntimeValue::Number(1.0.into())),
            RuntimeValue::None
        );
    }

    fn text_node(value: &str) -> mq_markdown::Node {
        mq_markdown::Node::Text(mq_markdown::Text {
            value: value.to_string(),
            position: None,
        })
    }

    /// Reference output from the tree-walker for the same code/input.
    ///
    /// Calling `Engine::eval` here would select the VM when the `tarn` feature is enabled,
    /// so invoke the evaluator directly to keep this a genuine differential test.
    fn tree_walk_eval(code: &str, input: RuntimeValue) -> RuntimeValue {
        tree_walk_eval_many(code, vec![input]).remove(0)
    }

    fn tree_walk_eval_many(code: &str, inputs: Vec<RuntimeValue>) -> Vec<RuntimeValue> {
        let mut engine = crate::DefaultEngine::default();
        engine.evaluator.load_builtin_module_full().unwrap();
        let compiled = engine.compile(code).unwrap();
        engine.evaluator.eval(compiled.program(), inputs.into_iter()).unwrap()
    }

    #[cfg(feature = "tarn")]
    fn vm_engine_eval_many(code: &str, inputs: Vec<RuntimeValue>) -> Vec<RuntimeValue> {
        let mut engine = crate::DefaultEngine::default();
        engine.load_builtin_module();
        let compiled = engine.compile(code).unwrap();
        engine
            .eval_compiled(&compiled, inputs.into_iter())
            .unwrap()
            .values()
            .clone()
    }

    #[cfg(feature = "tarn")]
    fn assert_vm_matches_tree_walker(code: &str, inputs: Vec<RuntimeValue>) {
        assert_eq!(
            vm_engine_eval_many(code, inputs.clone()),
            tree_walk_eval_many(code, inputs),
            "VM and tree-walker disagreed for: {code}"
        );
    }

    #[test]
    fn nodes_aggregates_per_input_results_into_one_run() {
        // `nodes` (see `split_at_nodes`/`run_nodes_aggregate`) collects every input's
        // per-input result into one array and runs the rest of the program against that
        // array once, rather than once per input — `len()` here only makes sense read that
        // way (3 individual numbers each have no `len`, but an array of 3 does).
        let code = "nodes | len()";
        let inputs = vec![
            RuntimeValue::Number(1.0.into()),
            RuntimeValue::Number(2.0.into()),
            RuntimeValue::Number(3.0.into()),
        ];
        let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
        let program = crate::parse(code, Shared::clone(&token_arena)).unwrap();
        let results = compile_and_run_many(
            &program,
            inputs.clone().into_iter(),
            EngineRunContext {
                host_functions: &HostFunctions::default(),
                timeout: None,
                max_call_stack_depth: crate::eval::Options::default().max_call_stack_depth,
                token_arena,
                module_loader: ModuleLoader::new(StdModuleResolver),
                global_bindings: &[],
            },
        )
        .unwrap();
        assert_eq!(results, tree_walk_eval_many(code, inputs));
        assert_eq!(results, vec![RuntimeValue::Number(3.0.into())]);
    }

    #[cfg(feature = "debugger")]
    #[test]
    fn nodes_split_also_works_through_the_debugger_hooked_entry_point() {
        let code = "nodes | len()";
        let inputs = vec![
            RuntimeValue::Number(1.0.into()),
            RuntimeValue::Number(2.0.into()),
            RuntimeValue::Number(3.0.into()),
        ];
        let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
        let program = crate::parse(code, Shared::clone(&token_arena)).unwrap();
        let debugger = Shared::new(SharedCell::new(crate::Debugger::new()));
        let handler: Shared<SharedCell<Box<dyn DebuggerHandler>>> =
            Shared::new(SharedCell::new(Box::new(crate::eval::debugger::DefaultDebuggerHandler)));
        let results = compile_and_run_debugged(
            &program,
            inputs.into_iter(),
            DebugRunContext {
                engine: EngineRunContext {
                    host_functions: &HostFunctions::default(),
                    timeout: None,
                    max_call_stack_depth: crate::eval::Options::default().max_call_stack_depth,
                    token_arena,
                    module_loader: ModuleLoader::new(StdModuleResolver),
                    global_bindings: &[],
                },
                debugger,
                handler,
                source: Source {
                    name: None,
                    code: code.to_string(),
                },
            },
        )
        .unwrap();
        assert_eq!(results, vec![RuntimeValue::Number(3.0.into())]);
    }

    #[test]
    fn nodes_runs_the_pre_nodes_portion_once_per_input_first() {
        let code = ". * 10 | nodes | len()";
        let inputs = vec![RuntimeValue::Number(1.0.into()), RuntimeValue::Number(2.0.into())];
        let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
        let program = crate::parse(code, Shared::clone(&token_arena)).unwrap();
        let results = compile_and_run_many(
            &program,
            inputs.clone().into_iter(),
            EngineRunContext {
                host_functions: &HostFunctions::default(),
                timeout: None,
                max_call_stack_depth: crate::eval::Options::default().max_call_stack_depth,
                token_arena,
                module_loader: ModuleLoader::new(StdModuleResolver),
                global_bindings: &[],
            },
        )
        .unwrap();
        assert_eq!(results, tree_walk_eval_many(code, inputs));
        assert_eq!(results, vec![RuntimeValue::Number(2.0.into())]);
    }

    #[test]
    fn markdown_fragment_input_that_matches_at_the_top_runs_only_once() {
        let fragment = mq_markdown::Node::Fragment(mq_markdown::Fragment {
            values: vec![text_node("a"), text_node("b")],
        });
        let code = r#"s"[${to_string(.)}]""#;
        let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
        let program = crate::parse(code, Shared::clone(&token_arena)).unwrap();
        let results = compile_and_run_many(
            &program,
            std::iter::once(RuntimeValue::new_markdown(fragment.clone())),
            EngineRunContext {
                host_functions: &HostFunctions::default(),
                timeout: None,
                max_call_stack_depth: crate::eval::Options::default().max_call_stack_depth,
                token_arena,
                module_loader: ModuleLoader::new(StdModuleResolver),
                global_bindings: &[],
            },
        )
        .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], tree_walk_eval(code, RuntimeValue::new_markdown(fragment)));
        assert_eq!(results[0].to_string(), "[a\nb]");
    }

    #[test]
    fn markdown_selector_recurses_into_a_non_matching_container_to_find_matches_below() {
        let matching_child = mq_markdown::Node::Heading(mq_markdown::Heading {
            values: vec![],
            position: None,
            depth: 1,
        });
        let outer = mq_markdown::Node::Heading(mq_markdown::Heading {
            values: vec![matching_child, text_node("no match anywhere")],
            position: None,
            depth: 2,
        });
        let code = ".h1";
        let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
        let program = crate::parse(code, Shared::clone(&token_arena)).unwrap();
        let results = compile_and_run_many(
            &program,
            std::iter::once(RuntimeValue::new_markdown(outer.clone())),
            EngineRunContext {
                host_functions: &HostFunctions::default(),
                timeout: None,
                max_call_stack_depth: crate::eval::Options::default().max_call_stack_depth,
                token_arena,
                module_loader: ModuleLoader::new(StdModuleResolver),
                global_bindings: &[],
            },
        )
        .unwrap();
        assert_eq!(results[0], tree_walk_eval(code, RuntimeValue::new_markdown(outer)));
    }

    #[test]
    fn non_fragment_markdown_input_still_runs_the_query_once() {
        let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
        let program = crate::parse(".h1", Shared::clone(&token_arena)).unwrap();
        let results = compile_and_run_many(
            &program,
            std::iter::once(heading(1)),
            EngineRunContext {
                host_functions: &HostFunctions::default(),
                timeout: None,
                max_call_stack_depth: crate::eval::Options::default().max_call_stack_depth,
                token_arena,
                module_loader: ModuleLoader::new(StdModuleResolver),
                global_bindings: &[],
            },
        )
        .unwrap();
        assert_ne!(results[0], RuntimeValue::None);
    }

    #[test]
    fn runtime_error_carries_a_source_token() {
        let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
        let program = crate::parse("1 | 1 / 0", Shared::clone(&token_arena)).unwrap();
        let err = compile_and_run(&program, token_arena).unwrap_err();
        let Error::Vm(vm_err) = err else {
            panic!("expected a VM error, got {err}");
        };
        assert!(
            vm_err.token_id().is_some(),
            "division-by-zero error should carry a source token"
        );
    }

    #[test]
    fn timeout_enabled_execution_still_checks_the_deadline() {
        let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
        let program = crate::parse("loop: 1;", Shared::clone(&token_arena)).unwrap();
        let err = compile_and_run_full(
            &program,
            RuntimeValue::None,
            &HostFunctions::default(),
            Some(std::time::Duration::ZERO),
            token_arena,
        )
        .unwrap_err();

        assert!(matches!(
            err,
            Error::Vm(interpreter::VmError::Located(inner, _))
                if matches!(*inner, interpreter::VmError::Timeout(_))
        ));
    }

    #[cfg(feature = "debugger")]
    #[test]
    fn debugger_metadata_tracks_boundaries_and_static_slots() {
        use super::bytecode::OpCode;
        use super::debug_symbols::DebugSlot;

        let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
        let program = crate::parse(
            "let outer = 10 | let f = fn(inner): outer + inner; | f(2)",
            Shared::clone(&token_arena),
        )
        .unwrap();
        let compiled = compiler::compile_program(&program, token_arena, ModuleLoader::new(StdModuleResolver)).unwrap();

        assert!(
            compiled.chunks[0]
                .code
                .iter()
                .any(|op| matches!(op, OpCode::StmtBoundary(_)))
        );
        assert_eq!(
            compiled.chunks[0]
                .debug_symbols
                .bindings()
                .iter()
                .find(|(name, _)| *name == crate::Ident::new("outer"))
                .map(|(_, slot)| *slot),
            Some(DebugSlot::Local(1))
        );
        assert!(
            compiled.chunks[1]
                .debug_symbols
                .bindings()
                .contains(&(crate::Ident::new("outer"), DebugSlot::Upvalue(0)))
        );
        assert!(
            compiled.chunks[1]
                .debug_symbols
                .bindings()
                .contains(&(crate::Ident::new("inner"), DebugSlot::Local(1)))
        );
    }

    #[cfg(feature = "debugger")]
    #[test]
    fn debugger_hook_receives_live_bindings_and_call_stack() {
        use super::interpreter::{DebugEvent, DebugHook};

        #[derive(Default)]
        struct Recorder(Vec<DebugEvent>);

        impl DebugHook for Recorder {
            fn on_boundary(&mut self, event: DebugEvent) {
                self.0.push(event);
            }
        }

        let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
        let program = crate::parse(
            "let outer = 10 | let f = fn(inner): outer + inner; | f(2)",
            Shared::clone(&token_arena),
        )
        .unwrap();
        let compiled = compiler::compile_program(&program, token_arena, ModuleLoader::new(StdModuleResolver)).unwrap();
        let mut recorder = Recorder::default();

        let result = interpreter::run_with_debug_hook_and_globals(
            &compiled,
            RuntimeValue::None,
            &HostFunctions::default(),
            None,
            crate::eval::Options::default().max_call_stack_depth,
            &[],
            &mut recorder,
        )
        .unwrap();

        assert_eq!(result, RuntimeValue::Number(12.0.into()));
        let function_event = recorder
            .0
            .iter()
            .find(|event| {
                event
                    .bindings
                    .contains(&(crate::Ident::new("inner"), RuntimeValue::Number(2.0.into())))
            })
            .expect("function body should emit a boundary with its parameter");
        assert!(
            function_event
                .bindings
                .contains(&(crate::Ident::new("outer"), RuntimeValue::Number(10.0.into())))
        );
        assert_eq!(function_event.call_stack.len(), 1);
        assert_eq!(function_event.token_id, function_event.node.token_id);
        assert_eq!(function_event.current_value, RuntimeValue::None);
    }

    #[cfg(feature = "debugger")]
    #[test]
    fn vm_debugger_hook_adapts_breakpoints_to_existing_handler() {
        use crate::{DebugContext, DebuggerAction, DebuggerHandler, Source, get_token};
        use std::sync::{Arc, Mutex};

        #[derive(Debug)]
        struct RecordingHandler {
            inner_values: Arc<Mutex<Vec<RuntimeValue>>>,
            #[cfg(feature = "debug-trace")]
            operand_stacks: Arc<Mutex<Vec<Vec<RuntimeValue>>>>,
        }

        impl DebuggerHandler for RecordingHandler {
            fn on_breakpoint_hit(&self, _breakpoint: &crate::Breakpoint, context: &DebugContext) -> DebuggerAction {
                if let Ok(value) = context.env.read().unwrap().resolve(crate::Ident::new("inner")) {
                    self.inner_values.lock().unwrap().push(value);
                }
                #[cfg(feature = "debug-trace")]
                self.operand_stacks.lock().unwrap().push(context.operand_stack.clone());
                DebuggerAction::Continue
            }
        }

        let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
        let program = crate::parse(
            "let outer = 10 |\nlet f = fn(inner):\nouter + inner; |\nf(2)",
            Shared::clone(&token_arena),
        )
        .unwrap();
        let compiled = compiler::compile_program(
            &program,
            Shared::clone(&token_arena),
            ModuleLoader::new(StdModuleResolver),
        )
        .unwrap();
        let function_token_id = compiled.chunks[1].debug_nodes[0].0;
        let function_line = get_token(Shared::clone(&token_arena), function_token_id)
            .range
            .start
            .line as usize;

        let debugger = Shared::new(SharedCell::new(crate::Debugger::new()));
        debugger.write().unwrap().activate();
        debugger.write().unwrap().add_breakpoint_with_options(
            function_line,
            None,
            None,
            Some("inner == 2 && outer == 10".to_string()),
            None,
            None,
        );
        let inner_values = Arc::new(Mutex::new(Vec::new()));
        #[cfg(feature = "debug-trace")]
        let operand_stacks = Arc::new(Mutex::new(Vec::new()));
        let handler: Shared<SharedCell<Box<dyn DebuggerHandler>>> =
            Shared::new(SharedCell::new(Box::new(RecordingHandler {
                inner_values: Arc::clone(&inner_values),
                #[cfg(feature = "debug-trace")]
                operand_stacks: Arc::clone(&operand_stacks),
            })));
        let mut hook = debugger::VmDebuggerHook::new(
            debugger,
            handler,
            token_arena,
            Source {
                name: None,
                code: String::new(),
            },
            Default::default(),
        );

        interpreter::run_with_debug_hook_and_globals(
            &compiled,
            RuntimeValue::None,
            &HostFunctions::default(),
            None,
            crate::eval::Options::default().max_call_stack_depth,
            &[],
            &mut hook,
        )
        .unwrap();

        assert!(inner_values.lock().unwrap().contains(&RuntimeValue::Number(2.0.into())));
        #[cfg(feature = "debug-trace")]
        assert!(!operand_stacks.lock().unwrap().is_empty());
    }

    #[cfg(feature = "debugger")]
    #[test]
    fn vm_debugger_hook_evaluates_hit_conditions_and_logpoints() {
        use crate::{DebugContext, DebuggerHandler, Source, get_token};
        use std::sync::{Arc, Mutex};

        #[derive(Debug)]
        struct LogHandler(Arc<Mutex<Vec<String>>>);

        impl DebuggerHandler for LogHandler {
            fn on_log_point(&self, _breakpoint: &crate::Breakpoint, message: &str, _context: &DebugContext) {
                self.0.lock().unwrap().push(message.to_string());
            }
        }

        let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
        let program = crate::parse(
            "let outer = 10 |\nlet f = fn(inner):\nouter + inner; |\nf(2)",
            Shared::clone(&token_arena),
        )
        .unwrap();
        let compiled = compiler::compile_program(
            &program,
            Shared::clone(&token_arena),
            ModuleLoader::new(StdModuleResolver),
        )
        .unwrap();
        let function_token_id = compiled.chunks[1].debug_nodes[0].0;
        let function_line = get_token(Shared::clone(&token_arena), function_token_id)
            .range
            .start
            .line as usize;

        let debugger = Shared::new(SharedCell::new(crate::Debugger::new()));
        debugger.write().unwrap().activate();
        debugger.write().unwrap().add_breakpoint_with_options(
            function_line,
            None,
            None,
            None,
            Some("1".to_string()),
            Some("inner=${inner}, outer=${outer}".to_string()),
        );
        let messages = Arc::new(Mutex::new(Vec::new()));
        let handler: Shared<SharedCell<Box<dyn DebuggerHandler>>> =
            Shared::new(SharedCell::new(Box::new(LogHandler(Arc::clone(&messages)))));
        let mut hook = debugger::VmDebuggerHook::new(
            debugger,
            handler,
            token_arena,
            Source {
                name: None,
                code: String::new(),
            },
            Default::default(),
        );

        interpreter::run_with_debug_hook_and_globals(
            &compiled,
            RuntimeValue::None,
            &HostFunctions::default(),
            None,
            crate::eval::Options::default().max_call_stack_depth,
            &[],
            &mut hook,
        )
        .unwrap();

        assert!(
            messages
                .lock()
                .unwrap()
                .iter()
                .any(|message| message == "inner=2, outer=10")
        );
    }

    #[test]
    fn host_function_is_called_when_not_a_builtin_or_local() {
        let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
        let program = crate::parse("double(21)", Shared::clone(&token_arena)).unwrap();
        let mut host_functions = HostFunctions::default();
        host_functions.insert("double", |args: &[RuntimeValue]| {
            let RuntimeValue::Number(n) = &args[0] else {
                return Err("expected a number".into());
            };
            Ok(RuntimeValue::Number((n.value() * 2.0).into()))
        });
        let result = compile_and_run_full(&program, RuntimeValue::None, &host_functions, None, token_arena).unwrap();
        assert_eq!(result, RuntimeValue::Number(42.0.into()));
    }

    #[test]
    fn undefined_call_with_no_matching_host_function_errors() {
        let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
        let program = crate::parse("totally_undefined_name(1)", Shared::clone(&token_arena)).unwrap();
        let err = compile_and_run(&program, token_arena).unwrap_err();
        assert!(matches!(err, Error::Vm(_)));
    }

    #[rstest]
    #[case::missing_required("def add(a, b): a + b; | add()", 2, 0)]
    #[case::too_many_required("def add(a, b): a + b; | add(1, 2, 3)", 2, 3)]
    #[case::too_many_optional("def add(a, b = 1): a + b; | add(1, 2, 3)", 2, 3)]
    #[case::too_many_zero_arity("def constant(): 1; | constant(1)", 0, 1)]
    fn invalid_function_arity_reports_the_declared_bounds(
        #[case] code: &str,
        #[case] expected: u8,
        #[case] actual: u8,
    ) {
        let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
        let program = crate::parse(code, Shared::clone(&token_arena)).unwrap();
        let err = compile_and_run(&program, token_arena).unwrap_err();
        assert!(
            matches!(err, Error::Vm(interpreter::VmError::Located(inner, _)) if matches!(*inner, interpreter::VmError::ArityMismatch { expected: got_expected, actual: got_actual } if got_expected == expected && got_actual == actual))
        );
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        #[test]
        fn generated_programs_preserve_arithmetic_closure_and_foreach_semantics(
            left in -100i16..100,
            right in -100i16..100,
            scale in -10i16..10,
            values in proptest::collection::vec(-50i16..50, 0..12),
        ) {
            let arithmetic = format!("({left} + {right}) * {scale}");
            let arithmetic_expected = (i32::from(left) + i32::from(right)) * i32::from(scale);
            prop_assert_eq!(run(&arithmetic), RuntimeValue::Number(f64::from(arithmetic_expected).into()));

            let closure = format!("let base = {left} | let add_base = fn(value): base + value; | add_base({right})");
            let closure_expected = i32::from(left) + i32::from(right);
            prop_assert_eq!(run(&closure), RuntimeValue::Number(f64::from(closure_expected).into()));

            let elements = values.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ");
            let foreach = format!("var total = 0 | foreach(value, [{elements}]): total += value; | total");
            let foreach_expected: i32 = values.iter().map(|value| i32::from(*value)).sum();
            prop_assert_eq!(run(&foreach), RuntimeValue::Number(f64::from(foreach_expected).into()));
        }
    }

    #[cfg(feature = "tarn")]
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(96))]

        #[test]
        fn generated_compiled_programs_match_the_tree_walker(
            initial in -100i16..100,
            scale in -10i16..10,
            offset in -100i16..100,
            input in -100i16..100,
            values in proptest::collection::vec(-50i16..50, 0..12),
        ) {
            let elements = values.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ");
            let code = format!(
                "var total = {initial} | foreach(value, [{elements}]): total += value * {scale}; | let finish = fn(extra): total + extra + {offset}; | finish(.)"
            );
            assert_vm_matches_tree_walker(&code, vec![RuntimeValue::Number(f64::from(input).into())]);
        }
    }

    #[cfg(feature = "tarn")]
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        #[test]
        fn generated_nested_closures_match_the_tree_walker(
            x in -50i16..50,
            y in -50i16..50,
            z in -50i16..50,
            w in -50i16..50,
        ) {
            let code = format!(
                "let x = {x} | let make = fn(y): fn(z): x + y + z + {w};; | let step = make({y}) | step({z})"
            );
            assert_vm_matches_tree_walker(&code, vec![RuntimeValue::None]);
        }

        #[test]
        fn generated_foreach_closures_match_the_tree_walker(
            values in proptest::collection::vec(-30i16..30, 1..8),
        ) {
            let elements = values.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ");
            let code =
                format!("let fns = foreach(i, [{elements}]): fn(): i * 2;; | fns | map(fn(f): f();)");
            assert_vm_matches_tree_walker(&code, vec![RuntimeValue::None]);
        }
    }

    #[test]
    fn break_crossing_a_try_boundary_exits_the_enclosing_loop() {
        let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
        let program = crate::parse("while(true): try: break catch: 1;;", Shared::clone(&token_arena)).unwrap();
        assert_eq!(compile_and_run(&program, token_arena).unwrap(), RuntimeValue::None);
    }

    #[rstest]
    #[case::while_loop("var x = 0 | while(x < 4) do x += 1 | try: continue catch: 0 end | x", 4.0)]
    #[case::foreach_loop(
        "var total = 0 | foreach(x, array(1, 2, 3, 4)) do try: continue catch: 0 end | total",
        0.0
    )]
    fn continue_crossing_try_boundary_reaches_the_enclosing_loop(#[case] code: &str, #[case] expected: f64) {
        assert_eq!(run(code), RuntimeValue::Number(expected.into()));
    }

    #[test]
    fn let_destructuring_mismatch_errors() {
        let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
        let program = crate::parse("let [a, b] = [1] | a + b", Shared::clone(&token_arena)).unwrap();
        let err = compile_and_run(&program, token_arena).unwrap_err();
        assert!(
            matches!(err, Error::Vm(interpreter::VmError::Located(inner, _)) if matches!(*inner, interpreter::VmError::DestructuringFailed))
        );
    }

    #[test]
    fn assigning_to_a_let_bound_name_is_a_compile_error() {
        let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
        let program = crate::parse("let x = 1 | x = 2", Shared::clone(&token_arena)).unwrap();
        let err = compile_and_run(&program, token_arena).unwrap_err();
        assert!(matches!(
            err,
            Error::Compile(compiler::CompileError::AssignToImmutable(..))
        ));
    }

    #[test]
    fn assigning_to_a_var_bound_name_still_works() {
        assert_eq!(run("var x = 1 | x = 2 | x"), RuntimeValue::Number(2.0.into()));
    }

    #[test]
    fn module_hoisting_resolves_regardless_of_source_order() {
        assert_eq!(
            run("def call_math(): math::mysum(10, 5); | call_math() | module math: def mysum(a, b): a + b; end"),
            RuntimeValue::Number(15.0.into())
        );
    }

    #[test]
    fn top_level_let_is_visible_to_a_def_body_compiled_before_it_runs() {
        assert_eq!(run("def f(): x; | let x = 42 | f()"), RuntimeValue::Number(42.0.into()));
    }

    #[test]
    fn include_flattens_a_standard_module_into_the_current_scope() {
        assert_eq!(
            run_with_prelude(r#"include "csv" | csv_needs_quote("trailing space ", ",")"#),
            RuntimeValue::Boolean(true)
        );
        assert_eq!(
            run_with_prelude(r#"include "csv" | csv_needs_quote("plain", ",")"#),
            RuntimeValue::Boolean(false)
        );
    }

    #[test]
    fn include_hoisting_resolves_regardless_of_source_order() {
        assert_eq!(
            run_with_prelude(r#"def f(): csv_needs_quote("a,b", ","); | f() | include "csv""#),
            RuntimeValue::Boolean(true)
        );
    }

    #[test]
    fn import_exposes_functions_only_via_qualified_access() {
        assert_eq!(
            run_with_prelude(r#"import "csv" as csv | csv::csv_needs_quote("a,b", ",")"#),
            RuntimeValue::Boolean(true)
        );
    }

    #[test]
    fn import_default_alias_is_the_module_name() {
        assert_eq!(
            run_with_prelude(r#"import "csv" | csv::csv_needs_quote("a,b", ",")"#),
            RuntimeValue::Boolean(true)
        );
    }

    #[test]
    fn import_hoisting_resolves_regardless_of_source_order() {
        assert_eq!(
            run_with_prelude(r#"def f(): csv::csv_needs_quote("a,b", ","); | f() | import "csv" as csv"#),
            RuntimeValue::Boolean(true)
        );
    }

    #[test]
    fn calling_a_bare_native_function_reference_dispatches_to_the_builtin() {
        assert_eq!(
            run_with_prelude(r#"import "csv" as csv | csv::csv_to_markdown_table([["a", "b"], [1, 2]])"#),
            RuntimeValue::String(Shared::new("| a | b |\n| --- | --- |\n| 1 | 2 |".to_string()))
        );
    }

    #[test]
    fn imported_function_name_is_not_reachable_unqualified() {
        let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
        let program = crate::parse(
            r#"import "csv" as csv | csv_needs_quote("a,b", ",")"#,
            Shared::clone(&token_arena),
        )
        .unwrap();
        let err = compile_and_run(&program, token_arena).unwrap_err();
        assert!(matches!(err, Error::Vm(_)));
    }

    /// Real `.mq` file with a top-level `let`, since no bundled stdlib module has one.
    #[rstest::fixture]
    fn module_with_vars() -> tempfile::TempDir {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("mod1.mq"), "def helper(x): x + 1; let base = 10").unwrap();
        dir
    }

    fn run_with_local_module(dir: &tempfile::TempDir, code: &str) -> RuntimeValue {
        let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
        let program = crate::parse(code, Shared::clone(&token_arena)).unwrap();
        let resolver = crate::module::resolver::local_fs_resolver::LocalFsModuleResolver::new(Some(vec![
            dir.path().to_path_buf(),
        ]));
        let compiled = compiler::compile_program(&program, token_arena, ModuleLoader::new(resolver)).unwrap();
        interpreter::run(
            &compiled,
            RuntimeValue::None,
            &HostFunctions::default(),
            None,
            crate::eval::Options::default().max_call_stack_depth,
        )
        .unwrap()
    }

    /// `include` binds vars unqualified; `import` (top-level or nested) qualifies them too.
    #[rstest]
    #[case::top_level_include(r#"include "mod1" | helper(base)"#)]
    #[case::top_level_import(r#"import "mod1" as m | m::helper(m::base)"#)]
    #[case::top_level_import_default_alias(r#"import "mod1" | mod1::helper(mod1::base)"#)]
    #[case::import_nested_in_inline_module(r#"module outer: import "mod1" as m end | m::helper(m::base)"#)]
    fn module_vars_binding_is_correct(module_with_vars: tempfile::TempDir, #[case] code: &str) {
        assert_eq!(
            run_with_local_module(&module_with_vars, code),
            RuntimeValue::Number(11.into())
        );
    }

    /// `import`'s vars must not leak unqualified into the importing scope.
    #[rstest]
    #[case::top_level_import(r#"import "mod1" as m | base"#)]
    #[case::import_nested_in_inline_module(r#"module outer: import "mod1" as m end | base"#)]
    fn import_does_not_leak_the_bare_var_name(module_with_vars: tempfile::TempDir, #[case] code: &str) {
        let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
        let program = crate::parse(code, Shared::clone(&token_arena)).unwrap();
        let resolver = crate::module::resolver::local_fs_resolver::LocalFsModuleResolver::new(Some(vec![
            module_with_vars.path().to_path_buf(),
        ]));
        let err = compiler::compile_program(&program, token_arena, ModuleLoader::new(resolver)).unwrap_err();
        assert!(matches!(err, compiler::CompileError::UndefinedIdent(..)));
    }

    /// A module's vars never change `self`, whichever path binds them.
    #[rstest]
    #[case::top_level_include(r#"77 | include "mod1""#)]
    #[case::top_level_import(r#"77 | import "mod1" as m"#)]
    #[case::import_nested_in_inline_module(r#"77 | module outer: import "mod1" as m end"#)]
    fn module_vars_binding_preserves_self(module_with_vars: tempfile::TempDir, #[case] code: &str) {
        assert_eq!(
            run_with_local_module(&module_with_vars, code),
            RuntimeValue::Number(77.into())
        );
    }

    /// Non-tail module-var binding must not push and then discard `self`.
    #[rstest]
    #[case::top_level_include_non_tail(r#"include "mod1" | helper(base)"#)]
    #[case::top_level_import_non_tail(r#"import "mod1" as m | m::helper(m::base)"#)]
    #[case::import_nested_in_inline_module(r#"module outer: import "mod1" as m end | m::helper(m::base)"#)]
    #[cfg(not(feature = "debugger"))]
    fn module_vars_binding_does_not_push_and_discard_self(module_with_vars: tempfile::TempDir, #[case] code: &str) {
        use super::bytecode::{OpCode, SELF_SLOT};

        let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
        let program = crate::parse(code, Shared::clone(&token_arena)).unwrap();
        let resolver = crate::module::resolver::local_fs_resolver::LocalFsModuleResolver::new(Some(vec![
            module_with_vars.path().to_path_buf(),
        ]));
        let compiled = compiler::compile_program(&program, token_arena, ModuleLoader::new(resolver)).unwrap();

        let has_wasted_self_push = compiled.chunks.iter().any(|chunk| {
            chunk.code.windows(2).any(|pair| match pair {
                [OpCode::GetLocal(a), OpCode::Pop] => *a == SELF_SLOT,
                [OpCode::GetLocal(a), OpCode::SetLocal(b)] => *a == SELF_SLOT && *b == SELF_SLOT,
                _ => false,
            })
        });
        assert!(
            !has_wasted_self_push,
            "{code:?} should not push self just to discard it"
        );
    }
}
