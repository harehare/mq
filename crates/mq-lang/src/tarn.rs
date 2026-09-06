//! Tarn: mq's bytecode VM, an alternative to the tree-walking evaluator (`eval.rs`).
//! Enabled via the `tarn` feature, which routes `Engine::eval`/`eval_compiled` here instead.
//!
//! VM closures have a `RuntimeValue::VmClosure` representation, so they can be stored in
//! collections and passed to native higher-order functions such as `partial`.
//!
//! This file is the front door (`Error`, `VmState`/`TarnVm`, session + run orchestration);
//! `cache`, `nodes_split`, and `disasm` hold the parts that split out cleanly.

mod bytecode;
#[cfg(not(feature = "debugger"))]
mod cache;
mod compiler;
#[cfg(feature = "debugger")]
mod debug_symbols;
#[cfg(feature = "debugger")]
mod debugger;
#[cfg(feature = "debug-trace")]
mod disasm;
mod interpreter;
mod nodes_split;
mod resolver;
pub(crate) mod value;

#[cfg(not(feature = "debugger"))]
pub(crate) use cache::CachedProgram;
#[cfg(feature = "debug-trace")]
pub(crate) use disasm::dump_bytecode;
use nodes_split::{
    ProgramSlice, immutable_let_names_before_nodes, let_names_before_nodes, program_after_nodes, split_at_nodes,
    top_level_binding_names,
};

use crate::Shared;
use crate::TokenArena;
use crate::ast::Program;
use crate::ast::node::{self as ast, AccessTarget, Expr, Literal, Node, Pattern};
use crate::engine;
use crate::error;
use crate::eval::Options;
use crate::io::{Io, NativeIo, SandboxedIo};
use crate::module::resolver::DefaultModuleResolver;
use crate::module::resolver::std_resolver::StdModuleResolver;
use crate::runtime::host::HostFunctions;
use crate::runtime::runtime_value::RuntimeValue;
use crate::{ModuleLoader, ModuleResolver};
use rustc_hash::{FxHashMap, FxHashSet};
use std::fmt;
use std::time::{Duration, Instant};

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

/// A deadline shared by all inputs in one evaluation.
fn shared_deadline(timeout: Option<Duration>) -> Option<Instant> {
    timeout.map(|timeout| Instant::now() + timeout)
}

fn remaining_timeout(deadline: Option<Instant>) -> Option<Duration> {
    deadline.map(|deadline| deadline.saturating_duration_since(Instant::now()))
}

fn compile_error_to_runtime_error(
    err: compiler::CompileError,
    token_arena: TokenArena,
) -> error::runtime::RuntimeError {
    use error::runtime::RuntimeError;
    // Fall back to the arena's dummy EOF token.
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
    let token_id = err.token_id().unwrap_or(crate::ast::TokenId::new(0));
    let token = (*crate::get_token(Shared::clone(&token_arena), token_id)).clone();
    err.to_runtime_error(token, token_id, token_arena)
}

#[cfg_attr(not(test), allow(dead_code))]
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

#[cfg_attr(not(test), allow(dead_code))]
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

fn markdown_child_result(value: RuntimeValue, child_node: &mq_markdown::Node) -> mq_markdown::Node {
    match value {
        RuntimeValue::None => child_node.to_fragment(),
        RuntimeValue::Function(..) | RuntimeValue::NativeFunction(_) | RuntimeValue::Module(_) => {
            mq_markdown::Node::Empty
        }
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

/// One captured top-level `let`/`var`/`def` binding for [`engine::Engine::enable_query_session`].
#[derive(Debug, Clone)]
pub(crate) struct SessionBinding {
    pub(crate) name: crate::Ident,
    pub(crate) mutable: bool,
    pub(crate) value: RuntimeValue,
}

/// VM-only state, held directly by `Engine` (independent of `Evaluator`).
#[derive(Debug)]
pub(crate) struct VmState<T: ModuleResolver = DefaultModuleResolver, IO: Io = SandboxedIo<NativeIo>> {
    pub(crate) options: Options,
    pub(crate) module_loader: ModuleLoader<T>,
    pub(crate) io: Shared<IO>,
    pub(crate) host_functions: Shared<crate::SharedCell<HostFunctions>>,
    global_bindings: Shared<crate::SharedCell<FxHashMap<crate::Ident, RuntimeValue>>>,
    pub(crate) session_enabled: bool,
    pub(crate) session_bindings: Shared<crate::SharedCell<Vec<SessionBinding>>>,
    #[cfg(feature = "debugger")]
    pub(crate) debugger: Shared<crate::SharedCell<Debugger>>,
    #[cfg(feature = "debugger")]
    pub(crate) debugger_handler: Shared<crate::SharedCell<Box<dyn DebuggerHandler>>>,
}

impl<T: ModuleResolver, IO: Io + Default> Default for VmState<T, IO> {
    fn default() -> Self {
        Self {
            options: Options::default(),
            module_loader: ModuleLoader::new(T::default()),
            io: Shared::new(IO::default()),
            host_functions: Shared::new(crate::SharedCell::new(HostFunctions::default())),
            global_bindings: Shared::new(crate::SharedCell::new(FxHashMap::default())),
            session_enabled: false,
            session_bindings: Shared::new(crate::SharedCell::new(Vec::new())),
            #[cfg_attr(feature = "sync", allow(clippy::arc_with_non_send_sync))]
            #[cfg(feature = "debugger")]
            debugger: Shared::new(crate::SharedCell::new(Debugger::new())),
            #[cfg(feature = "debugger")]
            debugger_handler: Shared::new(crate::SharedCell::new(Box::new(
                crate::runtime::debugger::DefaultDebuggerHandler,
            ))),
        }
    }
}

impl<T: ModuleResolver, IO: Io> Clone for VmState<T, IO> {
    fn clone(&self) -> Self {
        Self {
            options: self.options.clone(),
            module_loader: self.module_loader.clone(),
            io: Shared::clone(&self.io),
            host_functions: Shared::clone(&self.host_functions),
            global_bindings: Shared::clone(&self.global_bindings),
            session_enabled: self.session_enabled,
            session_bindings: Shared::clone(&self.session_bindings),
            #[cfg(feature = "debugger")]
            debugger: Shared::clone(&self.debugger),
            #[cfg(feature = "debugger")]
            debugger_handler: Shared::clone(&self.debugger_handler),
        }
    }
}

impl<T: ModuleResolver, IO: Io + Default> VmState<T, IO> {
    pub(crate) fn with_module_loader(module_loader: ModuleLoader<T>) -> Self {
        Self {
            module_loader,
            ..Default::default()
        }
    }
}

impl<T: ModuleResolver, IO: Io> VmState<T, IO> {
    pub(crate) fn with_module_loader_and_io(module_loader: ModuleLoader<T>, io: Shared<IO>) -> Self {
        Self {
            options: Options::default(),
            module_loader,
            io,
            host_functions: Shared::new(crate::SharedCell::new(HostFunctions::default())),
            global_bindings: Shared::new(crate::SharedCell::new(FxHashMap::default())),
            session_enabled: false,
            session_bindings: Shared::new(crate::SharedCell::new(Vec::new())),
            #[cfg_attr(feature = "sync", allow(clippy::arc_with_non_send_sync))]
            #[cfg(feature = "debugger")]
            debugger: Shared::new(crate::SharedCell::new(Debugger::new())),
            #[cfg(feature = "debugger")]
            debugger_handler: Shared::new(crate::SharedCell::new(Box::new(
                crate::runtime::debugger::DefaultDebuggerHandler,
            ))),
        }
    }

    pub(crate) fn define(&self, name: crate::Ident, value: RuntimeValue) {
        #[cfg(not(feature = "sync"))]
        self.global_bindings.borrow_mut().insert(name, value);
        #[cfg(feature = "sync")]
        self.global_bindings.write().unwrap().insert(name, value);
    }

    pub(crate) fn global_bindings_snapshot(&self) -> Vec<(crate::Ident, RuntimeValue)> {
        #[cfg(not(feature = "sync"))]
        let bindings = self.global_bindings.borrow();
        #[cfg(feature = "sync")]
        let bindings = self.global_bindings.read().unwrap();
        bindings.iter().map(|(ident, value)| (*ident, value.clone())).collect()
    }

    /// Warms this VM's own builtin.mq parse cache, independent of `Evaluator`.
    pub(crate) fn load_builtin_module(&mut self, token_arena: TokenArena) {
        match self.module_loader.load_builtin(token_arena) {
            Ok(_) | Err(crate::module::error::ModuleError::AlreadyLoaded(_)) => {}
            Err(e) => panic!("Failed to load builtin module: {e}"),
        }
    }
}

/// Engine-provided services needed to compile and execute a VM program.
pub(crate) struct EngineRunContext<'a, R: ModuleResolver> {
    pub(crate) host_functions: &'a HostFunctions,
    pub(crate) timeout: Option<Duration>,
    pub(crate) max_call_stack_depth: u32,
    pub(crate) token_arena: TokenArena,
    pub(crate) module_loader: ModuleLoader<R>,
    pub(crate) global_bindings: &'a [(crate::Ident, RuntimeValue)],
    /// `Some` when [`engine::Engine::enable_query_session`] is on.
    pub(crate) session: Option<&'a Shared<crate::SharedCell<Vec<SessionBinding>>>>,
    /// Module var values computed once per eval — see [`resolve_module_prelude_globals`].
    pub(crate) preresolved_module_vars: compiler::ResolvedModuleVars,
}

#[cfg(feature = "debugger")]
pub(crate) struct DebugRunContext<'a, R: ModuleResolver> {
    pub(crate) engine: EngineRunContext<'a, R>,
    pub(crate) debugger: Shared<SharedCell<Debugger>>,
    pub(crate) handler: Shared<SharedCell<Box<dyn DebuggerHandler>>>,
    pub(crate) source: Source,
}

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

/// Collects external module paths and root inline `module { .. }` blocks reachable from
/// `program`. Nested inline modules remain part of their root's probe so their initializers
/// execute in source order exactly once.
fn collect_module_prelude_targets(
    program: &Program,
    paths: &mut Vec<String>,
    inline_modules: &mut Vec<(ast::IdentWithToken, Program, Program)>,
) {
    for (index, node) in program.iter().enumerate() {
        match &*node.expr {
            Expr::Include(Literal::String(path)) => {
                if !paths.iter().any(|existing| existing == path) {
                    paths.push(path.clone());
                }
            }
            Expr::Import(Literal::String(path), _) => {
                if !paths.iter().any(|existing| existing == path) {
                    paths.push(path.clone());
                }
            }
            Expr::Module(ident, body) => {
                inline_modules.push((ident.clone(), program[..index].to_vec(), body.clone()));
                collect_module_paths(body, paths);
            }
            _ => {}
        }
    }
}

/// Collects external module paths below an inline module without adding nested inline modules
/// as separate probes.
fn collect_module_paths(program: &Program, paths: &mut Vec<String>) {
    for node in program {
        match &*node.expr {
            Expr::Include(Literal::String(path)) | Expr::Import(Literal::String(path), _) => {
                if !paths.iter().any(|existing| existing == path) {
                    paths.push(path.clone());
                }
            }
            Expr::Module(_, body) => collect_module_paths(body, paths),
            _ => {}
        }
    }
}

/// Resolves an external module and its transitive dependencies in post-order.
///
/// The shared loader retains source text while discovering the graph, so the subsequent
/// initializer probe reloads from that in-memory cache rather than resolving the module again.
/// A parent module's compiler emits its module directives before its own vars, so resolving
/// children first also prevents a parent probe from re-running them.
#[allow(clippy::too_many_arguments)]
fn resolve_external_module_prelude<R: ModuleResolver>(
    path: &str,
    token_arena: &TokenArena,
    host_functions: &HostFunctions,
    max_call_stack_depth: u32,
    global_bindings: &[(crate::Ident, RuntimeValue)],
    module_loader: &mut ModuleLoader<R>,
    seen: &mut FxHashSet<String>,
    result: &mut compiler::ResolvedModuleVars,
    inline_modules: &mut Vec<(ast::IdentWithToken, Program, Program)>,
    deadline: Option<Instant>,
) -> Result<(), Error> {
    if !seen.insert(path.to_string()) {
        return Ok(());
    }

    let module = match module_loader.load_from_file(path, Shared::clone(token_arena)) {
        Ok(module) => module,
        Err(crate::ModuleError::AlreadyLoaded(_)) => module_loader
            .reload_cached(path, Shared::clone(token_arena))
            .map_err(compiler::CompileError::Module)?,
        Err(error) => return Err(compiler::CompileError::Module(error).into()),
    };
    let mut dependencies = Vec::new();
    collect_module_prelude_targets(&module.modules, &mut dependencies, inline_modules);
    for dependency in dependencies {
        resolve_external_module_prelude(
            &dependency,
            token_arena,
            host_functions,
            max_call_stack_depth,
            global_bindings,
            module_loader,
            seen,
            result,
            inline_modules,
            deadline,
        )?;
    }

    let var_names = compiler::module_var_names(&module.vars);
    if var_names.is_empty() {
        return Ok(());
    }

    let directive_program: Program = vec![Shared::new(Node {
        token_id: crate::ast::TokenId::new(0),
        expr: Shared::new(Expr::Include(Literal::String(path.to_string()))),
    })];
    let compiled = compiler::compile_program_for_engine(
        &directive_program,
        Shared::clone(token_arena),
        module_loader.clone(),
        &[],
        result,
    )?;
    let (run_result, captured, _) = interpreter::run_with_globals_capturing_locals(
        &compiled,
        RuntimeValue::None,
        &[],
        interpreter::RunOptions {
            host_functions,
            timeout: remaining_timeout(deadline),
            max_call_stack_depth,
            global_bindings,
        },
        &var_names,
        interpreter::ExecutionPools::default(),
    );
    run_result?;
    result.by_path.insert(path.to_string(), captured);
    Ok(())
}

/// Collects every simple `let` declared by an inline module tree, retaining the qualified path
/// needed to read it from one root-module probe.
fn collect_inline_module_vars(
    module: &ast::IdentWithToken,
    body: &Program,
    parent_path: &[ast::IdentWithToken],
    vars: &mut Vec<(Vec<ast::IdentWithToken>, Shared<Node>)>,
) {
    // Inline-module aliases are exported into the surrounding compiler scope, including when
    // the declaration itself is nested (the same rule used by `compile_module_rest`).
    let mut path = parent_path.to_vec();
    path.push(module.clone());
    for node in body {
        match &*node.expr {
            Expr::Let(Pattern::Ident(_), _) => vars.push((path.clone(), Shared::clone(node))),
            Expr::Module(nested_module, nested_body) => {
                collect_inline_module_vars(nested_module, nested_body, &path, vars);
            }
            _ => {}
        }
    }
}

/// Computes each module's `let` values once per eval, baked into every real compile as
/// constants instead of recompiling the initializer per input.
///
/// An inline module's vars are only reachable via qualified access, so they're probed by
/// compiling+running a throwaway `[..enclosing prefix, module { .. }, array(alias::a, ..)]`
/// program, prefixed with the preceding top-level nodes so it sees the same enclosing
/// `let`/`def` bindings the real compile would. A name still out of scope after that falls
/// back to recompiling the module inline; any other probe failure is a real initializer bug
/// and is propagated rather than retried once per input.
fn resolve_module_prelude_globals<R: ModuleResolver>(
    program: &Program,
    context: &mut EngineRunContext<'_, R>,
    deadline: Option<Instant>,
) -> Result<compiler::ResolvedModuleVars, Error> {
    let mut paths: Vec<String> = Vec::new();
    let mut inline_modules: Vec<(ast::IdentWithToken, Program, Program)> = Vec::new();
    collect_module_prelude_targets(program, &mut paths, &mut inline_modules);

    let mut result = compiler::ResolvedModuleVars::default();
    let mut seen_paths = FxHashSet::default();
    for path in paths {
        resolve_external_module_prelude(
            &path,
            &context.token_arena,
            context.host_functions,
            context.max_call_stack_depth,
            context.global_bindings,
            &mut context.module_loader,
            &mut seen_paths,
            &mut result,
            &mut inline_modules,
            deadline,
        )?;
    }

    for (ident, prefix, body) in inline_modules {
        let mut module_vars = Vec::new();
        collect_inline_module_vars(&ident, &body, &[], &mut module_vars);
        if module_vars.is_empty() {
            continue;
        }

        let probe_args: ast::Args = module_vars
            .iter()
            .map(|(path, node)| {
                let Expr::Let(Pattern::Ident(let_ident), _) = &*node.expr else {
                    unreachable!("filtered above");
                };
                Shared::new(Node {
                    token_id: crate::ast::TokenId::new(0),
                    expr: Shared::new(Expr::QualifiedAccess(
                        path.clone(),
                        AccessTarget::Ident(let_ident.clone()),
                    )),
                })
            })
            .collect();
        let mut probe_program = prefix;
        probe_program.push(Shared::new(Node {
            token_id: crate::ast::TokenId::new(0),
            expr: Shared::new(Expr::Module(ident.clone(), body)),
        }));
        probe_program.push(Shared::new(Node {
            token_id: crate::ast::TokenId::new(0),
            expr: Shared::new(Expr::Call(ast::IdentWithToken::new("array"), probe_args)),
        }));

        let probed: Result<Vec<RuntimeValue>, Error> = (|| {
            let compiled = compiler::compile_program_for_engine(
                &probe_program,
                Shared::clone(&context.token_arena),
                context.module_loader.clone(),
                &[],
                &result,
            )?;
            let value = interpreter::run_with_globals(
                &compiled,
                RuntimeValue::None,
                context.host_functions,
                remaining_timeout(deadline),
                context.max_call_stack_depth,
                context.global_bindings,
            )?;
            Ok(match value {
                RuntimeValue::Array(values) => Shared::unwrap_or_clone(values),
                other => vec![other],
            })
        })();

        match probed {
            Ok(values) if values.len() == module_vars.len() => {
                for ((_, node), value) in module_vars.iter().zip(values) {
                    result.by_token.insert(node.token_id, value);
                }
            }
            Ok(_) | Err(Error::Compile(compiler::CompileError::UndefinedIdent(..))) => {}
            Err(error) => return Err(error),
        }
    }

    Ok(result)
}

/// Everything `Engine::eval_compiled_vm` needs to run a compiled program on Tarn.
pub(crate) struct TarnVm<'a, R: ModuleResolver> {
    pub(crate) engine: EngineRunContext<'a, R>,
    #[cfg(not(feature = "debugger"))]
    pub(crate) module_prelude: &'a [engine::VmModulePrelude],
    #[cfg(feature = "debugger")]
    pub(crate) debugger: Shared<SharedCell<Debugger>>,
    #[cfg(feature = "debugger")]
    pub(crate) debugger_handler: Shared<SharedCell<Box<dyn DebuggerHandler>>>,
    #[cfg(feature = "debugger")]
    pub(crate) source: Source,
}

impl<'a, R: ModuleResolver> TarnVm<'a, R> {
    #[cfg(not(feature = "debugger"))]
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
        #[cfg_attr(feature = "debugger", allow(unused_variables))] compiled: &engine::CompiledProgram,
        program: &Program,
        input: I,
    ) -> Result<Vec<RuntimeValue>, Error>
    where
        I: Iterator<Item = RuntimeValue>,
    {
        #[cfg(not(feature = "debugger"))]
        if self.engine.session.is_none()
            && let Some(cached) = compiled.cached_vm_program()
        {
            // One deadline for the whole call: a cache-miss compile must not spend its own
            // budget separately from the run that follows it.
            let deadline = shared_deadline(self.engine.timeout);
            let cache_configuration = self.cache_configuration();
            let cached = match cached {
                Some(cached)
                    if cache::cached_program_is_current(
                        &cached,
                        &self.engine.module_loader,
                        &cache_configuration,
                        self.engine.global_bindings,
                    )? =>
                {
                    cached
                }
                _ => {
                    let mut cache_context = EngineRunContext {
                        host_functions: self.engine.host_functions,
                        timeout: self.engine.timeout,
                        max_call_stack_depth: self.engine.max_call_stack_depth,
                        token_arena: Shared::clone(&self.engine.token_arena),
                        module_loader: self.engine.module_loader.with_same_resolver(),
                        global_bindings: self.engine.global_bindings,
                        session: self.engine.session,
                        preresolved_module_vars: compiler::ResolvedModuleVars::default(),
                    };
                    cache::compile_cached_program(program, &mut cache_context, cache_configuration, deadline)?
                }
            };
            compiled.cache_vm_program(cached.clone());
            return cache::run_cached(
                &cached,
                input,
                self.engine.host_functions,
                deadline,
                self.engine.max_call_stack_depth,
                self.engine.global_bindings,
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
                        session: self.engine.session,
                        preresolved_module_vars: compiler::ResolvedModuleVars::default(),
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
                    session: self.engine.session,
                    preresolved_module_vars: compiler::ResolvedModuleVars::default(),
                },
            )
        }
    }
}

#[cfg(not(feature = "sync"))]
fn session_snapshot(session: &Shared<crate::SharedCell<Vec<SessionBinding>>>) -> Vec<SessionBinding> {
    session.borrow().clone()
}

#[cfg(feature = "sync")]
fn session_snapshot(session: &Shared<crate::SharedCell<Vec<SessionBinding>>>) -> Vec<SessionBinding> {
    session.read().unwrap().clone()
}

#[cfg(not(feature = "sync"))]
fn store_session(session: &Shared<crate::SharedCell<Vec<SessionBinding>>>, bindings: Vec<SessionBinding>) {
    *session.borrow_mut() = bindings;
}

#[cfg(feature = "sync")]
fn store_session(session: &Shared<crate::SharedCell<Vec<SessionBinding>>>, bindings: Vec<SessionBinding>) {
    *session.write().unwrap() = bindings;
}

/// Seed data derived from a session's current bindings, for recompiling and re-running.
struct SessionSeed {
    seed_names: Vec<crate::Ident>,
    seed_immutable: Vec<crate::Ident>,
    seed_values: Vec<RuntimeValue>,
    /// `seed_names` plus any new top-level names `program` itself declares.
    capture_names: Vec<crate::Ident>,
}

fn session_seed(session: &Shared<crate::SharedCell<Vec<SessionBinding>>>, program: &Program) -> SessionSeed {
    let existing = session_snapshot(session);
    let seed_names: Vec<crate::Ident> = existing.iter().map(|binding| binding.name).collect();
    let seed_immutable: Vec<crate::Ident> = existing
        .iter()
        .filter(|binding| !binding.mutable)
        .map(|binding| binding.name)
        .collect();
    let seed_values: Vec<RuntimeValue> = existing.iter().map(|binding| binding.value.clone()).collect();

    let mut capture_names = seed_names.clone();
    for name in top_level_binding_names(program) {
        if !capture_names.contains(&name) {
            capture_names.push(name);
        }
    }

    SessionSeed {
        seed_names,
        seed_immutable,
        seed_values,
        capture_names,
    }
}

/// Converts captured top-level values back into [`SessionBinding`]s.
fn session_bindings_from_captured(
    compiled: &compiler::CompiledProgram,
    captured: Vec<(crate::Ident, RuntimeValue)>,
) -> Vec<SessionBinding> {
    let local_names = &compiled.chunks[0].local_names;
    let local_mutable = &compiled.chunks[0].local_mutable;
    captured
        .into_iter()
        .map(|(name, value)| {
            let mutable = local_names
                .iter()
                .position(|local| *local == name)
                .and_then(|slot| local_mutable.get(slot))
                .copied()
                .unwrap_or(true);
            SessionBinding { name, mutable, value }
        })
        .collect()
}

fn extend_unique(names: &mut Vec<crate::Ident>, additional: impl IntoIterator<Item = crate::Ident>) {
    for name in additional {
        if !names.contains(&name) {
            names.push(name);
        }
    }
}

fn session_nodes_immutable_names(seed: &SessionSeed, before: ProgramSlice<'_>) -> Vec<crate::Ident> {
    let declared_before = let_names_before_nodes(before);
    let mut names: Vec<crate::Ident> = seed
        .seed_immutable
        .iter()
        .copied()
        .filter(|name| !declared_before.contains(name))
        .collect();
    extend_unique(&mut names, immutable_let_names_before_nodes(before));
    names
}

/// Runs a `nodes` program while preserving bindings from a query session.
fn run_nodes_with_session<I, R: ModuleResolver>(
    before: ProgramSlice<'_>,
    after: ProgramSlice<'_>,
    inputs: I,
    context: &EngineRunContext<'_, R>,
    session: &Shared<crate::SharedCell<Vec<SessionBinding>>>,
    global_names: &[crate::Ident],
    deadline: Option<Instant>,
) -> Result<Vec<RuntimeValue>, Error>
where
    I: Iterator<Item = RuntimeValue>,
{
    let whole_program: Program = before.iter().chain(after.iter()).cloned().collect();
    let seed = session_seed(session, &whole_program);
    let mut before_names = seed.seed_names.clone();
    extend_unique(&mut before_names, let_names_before_nodes(before));
    let before_compiled = compiler::compile_program_for_engine_with_bindings(
        &before.to_vec(),
        Shared::clone(&context.token_arena),
        context.module_loader.clone(),
        &seed.seed_names,
        &seed.seed_immutable,
        global_names,
        &context.preresolved_module_vars,
    )?;

    let mut values = Vec::new();
    let mut before_bindings = Vec::new();
    let mut current_values = seed.seed_values.clone();
    for input in inputs {
        let result = run_for_input(input, |value| {
            let (result, captured, _) = interpreter::run_with_globals_capturing_locals(
                &before_compiled,
                value,
                &current_values,
                interpreter::RunOptions {
                    host_functions: context.host_functions,
                    timeout: remaining_timeout(deadline),
                    max_call_stack_depth: context.max_call_stack_depth,
                    global_bindings: context.global_bindings,
                },
                &before_names,
                interpreter::ExecutionPools::default(),
            );
            if result.is_ok() {
                current_values = captured.iter().map(|(_, value)| value.clone()).collect();
                before_bindings = captured;
            }
            result
        })
        .map_err(Error::from)?;
        values.push(result);
    }

    let aggregate_program = program_after_nodes(before, after);
    let aggregate_compiled = compiler::compile_program_for_engine_with_bindings(
        &aggregate_program,
        Shared::clone(&context.token_arena),
        context.module_loader.clone(),
        &before_names,
        &session_nodes_immutable_names(&seed, before),
        global_names,
        &context.preresolved_module_vars,
    )?;
    let aggregate_values = before_bindings.into_iter().map(|(_, value)| value).collect::<Vec<_>>();
    let (result, captured, _) = interpreter::run_with_globals_capturing_locals(
        &aggregate_compiled,
        RuntimeValue::Array(Shared::new(values)),
        &aggregate_values,
        interpreter::RunOptions {
            host_functions: context.host_functions,
            timeout: remaining_timeout(deadline),
            max_call_stack_depth: context.max_call_stack_depth,
            global_bindings: context.global_bindings,
        },
        &seed.capture_names,
        interpreter::ExecutionPools::default(),
    );
    let value = result?;
    store_session(session, session_bindings_from_captured(&aggregate_compiled, captured));
    match value {
        RuntimeValue::Array(values) => Ok(Shared::unwrap_or_clone(values)),
        value => Ok(vec![value]),
    }
}

/// Runs a program while preserving top-level bindings in `session`.
fn run_with_session<I, R: ModuleResolver>(
    program: &Program,
    inputs: I,
    context: &EngineRunContext<'_, R>,
    session: &Shared<crate::SharedCell<Vec<SessionBinding>>>,
    global_names: &[crate::Ident],
    deadline: Option<Instant>,
) -> Result<Vec<RuntimeValue>, Error>
where
    I: Iterator<Item = RuntimeValue>,
{
    if let Some((before, after)) = split_at_nodes(program) {
        return run_nodes_with_session(before, after, inputs, context, session, global_names, deadline);
    }
    let seed = session_seed(session, program);
    let compiled = compiler::compile_program_for_engine_with_bindings(
        program,
        context.token_arena.clone(),
        context.module_loader.clone(),
        &seed.seed_names,
        &seed.seed_immutable,
        global_names,
        &context.preresolved_module_vars,
    )?;

    let mut values = Vec::new();
    let mut current_values = seed.seed_values.clone();
    let mut captured: Option<Vec<(crate::Ident, RuntimeValue)>> = None;
    for input in inputs {
        let result = run_for_input(input, |v| {
            let (result, newly_captured, _) = interpreter::run_with_globals_capturing_locals(
                &compiled,
                v,
                &current_values,
                interpreter::RunOptions {
                    host_functions: context.host_functions,
                    timeout: remaining_timeout(deadline),
                    max_call_stack_depth: context.max_call_stack_depth,
                    global_bindings: context.global_bindings,
                },
                &seed.capture_names,
                interpreter::ExecutionPools::default(),
            );
            if result.is_ok() {
                current_values = newly_captured.iter().map(|(_, value)| value.clone()).collect();
                captured = Some(newly_captured);
            }
            result
        })
        .map_err(Error::from)?;
        values.push(result);
    }

    if let Some(captured) = captured {
        store_session(session, session_bindings_from_captured(&compiled, captured));
    }
    Ok(values)
}

/// Debugger counterpart to [`run_with_session`].
#[cfg(feature = "debugger")]
fn run_nodes_with_session_debugged<I, R: ModuleResolver>(
    before: ProgramSlice<'_>,
    after: ProgramSlice<'_>,
    inputs: I,
    context: DebugRunContext<'_, R>,
    session: &Shared<crate::SharedCell<Vec<SessionBinding>>>,
    global_names: &[crate::Ident],
    deadline: Option<Instant>,
) -> Result<Vec<RuntimeValue>, Error>
where
    I: Iterator<Item = RuntimeValue>,
{
    let DebugRunContext {
        engine,
        debugger: debugger_state,
        handler,
        source,
    } = context;
    let whole_program: Program = before.iter().chain(after.iter()).cloned().collect();
    let seed = session_seed(session, &whole_program);
    let mut before_names = seed.seed_names.clone();
    extend_unique(&mut before_names, let_names_before_nodes(before));
    let before_compiled = compiler::compile_program_for_engine_with_bindings(
        &before.to_vec(),
        Shared::clone(&engine.token_arena),
        engine.module_loader.clone(),
        &seed.seed_names,
        &seed.seed_immutable,
        global_names,
        &engine.preresolved_module_vars,
    )?;
    let mut hook = debugger::VmDebuggerHook::new(
        debugger_state,
        handler,
        Shared::clone(&engine.token_arena),
        source,
        before_compiled.debug_sources.clone(),
        engine.module_loader.with_same_resolver(),
        engine.host_functions.clone(),
    );

    let mut values = Vec::new();
    let mut before_bindings = Vec::new();
    let mut current_values = seed.seed_values.clone();
    for input in inputs {
        let result = run_for_input(input, |value| {
            let (result, captured) = interpreter::run_with_debug_hook_and_globals_capturing_locals(
                &before_compiled,
                value,
                &current_values,
                interpreter::RunOptions {
                    host_functions: engine.host_functions,
                    timeout: remaining_timeout(deadline),
                    max_call_stack_depth: engine.max_call_stack_depth,
                    global_bindings: engine.global_bindings,
                },
                &before_names,
                &mut hook,
            );
            if result.is_ok() {
                current_values = captured.iter().map(|(_, value)| value.clone()).collect();
                before_bindings = captured;
            }
            result
        });
        match result {
            Ok(value) => values.push(value),
            Err(error) => {
                hook.notify_error(&error);
                return Err(error.into());
            }
        }
    }

    let aggregate_program = program_after_nodes(before, after);
    let aggregate_compiled = compiler::compile_program_for_engine_with_bindings(
        &aggregate_program,
        Shared::clone(&engine.token_arena),
        engine.module_loader.clone(),
        &before_names,
        &session_nodes_immutable_names(&seed, before),
        global_names,
        &engine.preresolved_module_vars,
    )?;
    hook.set_sources(aggregate_compiled.debug_sources.clone());
    let aggregate_values = before_bindings.into_iter().map(|(_, value)| value).collect::<Vec<_>>();
    let (result, captured) = interpreter::run_with_debug_hook_and_globals_capturing_locals(
        &aggregate_compiled,
        RuntimeValue::Array(Shared::new(values)),
        &aggregate_values,
        interpreter::RunOptions {
            host_functions: engine.host_functions,
            timeout: remaining_timeout(deadline),
            max_call_stack_depth: engine.max_call_stack_depth,
            global_bindings: engine.global_bindings,
        },
        &seed.capture_names,
        &mut hook,
    );
    match result {
        Ok(RuntimeValue::Array(values)) => {
            store_session(session, session_bindings_from_captured(&aggregate_compiled, captured));
            Ok(Shared::unwrap_or_clone(values))
        }
        Ok(value) => {
            store_session(session, session_bindings_from_captured(&aggregate_compiled, captured));
            Ok(vec![value])
        }
        Err(error) => {
            hook.notify_error(&error);
            Err(error.into())
        }
    }
}

/// Debugger counterpart to [`run_with_session`].
#[cfg(feature = "debugger")]
fn run_with_session_debugged<I, R: ModuleResolver>(
    program: &Program,
    inputs: I,
    context: DebugRunContext<'_, R>,
    session: &Shared<crate::SharedCell<Vec<SessionBinding>>>,
    global_names: &[crate::Ident],
    deadline: Option<Instant>,
) -> Result<Vec<RuntimeValue>, Error>
where
    I: Iterator<Item = RuntimeValue>,
{
    if let Some((before, after)) = split_at_nodes(program) {
        return run_nodes_with_session_debugged(before, after, inputs, context, session, global_names, deadline);
    }
    let seed = session_seed(session, program);
    let compiled = compiler::compile_program_for_engine_with_bindings(
        program,
        Shared::clone(&context.engine.token_arena),
        context.engine.module_loader.clone(),
        &seed.seed_names,
        &seed.seed_immutable,
        global_names,
        &context.engine.preresolved_module_vars,
    )?;
    let mut hook = debugger::VmDebuggerHook::new(
        context.debugger,
        context.handler,
        Shared::clone(&context.engine.token_arena),
        context.source,
        compiled.debug_sources.clone(),
        context.engine.module_loader.with_same_resolver(),
        context.engine.host_functions.clone(),
    );

    let mut values = Vec::new();
    let mut current_values = seed.seed_values.clone();
    let mut captured: Option<Vec<(crate::Ident, RuntimeValue)>> = None;
    for input in inputs {
        let result = run_for_input(input, |v| {
            let (result, newly_captured) = interpreter::run_with_debug_hook_and_globals_capturing_locals(
                &compiled,
                v,
                &current_values,
                interpreter::RunOptions {
                    host_functions: context.engine.host_functions,
                    timeout: remaining_timeout(deadline),
                    max_call_stack_depth: context.engine.max_call_stack_depth,
                    global_bindings: context.engine.global_bindings,
                },
                &seed.capture_names,
                &mut hook,
            );
            if result.is_ok() {
                current_values = newly_captured.iter().map(|(_, value)| value.clone()).collect();
                captured = Some(newly_captured);
            }
            result
        });
        match result {
            Ok(value) => values.push(value),
            Err(error) => {
                hook.notify_error(&error);
                return Err(error.into());
            }
        }
    }

    if let Some(captured) = captured {
        store_session(session, session_bindings_from_captured(&compiled, captured));
    }
    Ok(values)
}

fn run_nodes_aggregate<R: ModuleResolver>(
    before: ProgramSlice<'_>,
    after: ProgramSlice<'_>,
    values: Vec<RuntimeValue>,
    let_bindings: &[(crate::Ident, RuntimeValue)],
    context: &EngineRunContext<'_, R>,
    // From the caller's shared deadline, not `context.timeout`.
    timeout: Option<Duration>,
) -> Result<Vec<RuntimeValue>, Error> {
    let let_names: Vec<crate::Ident> = let_bindings.iter().map(|(ident, _)| *ident).collect();
    let immutable_let_names = immutable_let_names_before_nodes(before);
    let global_names: Vec<crate::Ident> = context.global_bindings.iter().map(|(ident, _)| *ident).collect();
    let program = program_after_nodes(before, after);
    let input = RuntimeValue::Array(Shared::new(values));
    let result = if let_names.is_empty() {
        let compiled = compiler::compile_program_for_engine(
            &program,
            Shared::clone(&context.token_arena),
            context.module_loader.clone(),
            &global_names,
            &context.preresolved_module_vars,
        )?;
        interpreter::run_with_globals(
            &compiled,
            input,
            context.host_functions,
            timeout,
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
            &immutable_let_names,
            &global_names,
            &context.preresolved_module_vars,
        )?;
        interpreter::run_with_globals_capturing_locals(
            &compiled,
            input,
            &let_values,
            interpreter::RunOptions {
                host_functions: context.host_functions,
                timeout,
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
    mut context: EngineRunContext<'_, R>,
) -> Result<Vec<RuntimeValue>, Error>
where
    I: Iterator<Item = RuntimeValue>,
{
    let deadline = shared_deadline(context.timeout);
    context.preresolved_module_vars = resolve_module_prelude_globals(program, &mut context, deadline)?;
    let global_names: Vec<crate::Ident> = context.global_bindings.iter().map(|(ident, _)| *ident).collect();
    if let Some(session) = context.session {
        return run_with_session(program, inputs, &context, session, &global_names, deadline);
    }
    let Some((before, after)) = split_at_nodes(program) else {
        let compiled = compiler::compile_program_for_engine(
            program,
            context.token_arena,
            context.module_loader,
            &global_names,
            &context.preresolved_module_vars,
        )?;
        return inputs
            .map(|input| {
                run_for_input(input, |v| {
                    interpreter::run_with_globals(
                        &compiled,
                        v,
                        context.host_functions,
                        remaining_timeout(deadline),
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
        &context.preresolved_module_vars,
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
                        remaining_timeout(deadline),
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
                            timeout: remaining_timeout(deadline),
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
    run_nodes_aggregate(
        before,
        after,
        values,
        &let_bindings,
        &context,
        remaining_timeout(deadline),
    )
}

#[cfg(feature = "debugger")]
fn compile_and_run_debugged<I, R: ModuleResolver>(
    program: &Program,
    inputs: I,
    mut context: DebugRunContext<'_, R>,
) -> Result<Vec<RuntimeValue>, Error>
where
    I: Iterator<Item = RuntimeValue>,
{
    let deadline = shared_deadline(context.engine.timeout);
    context.engine.preresolved_module_vars = resolve_module_prelude_globals(program, &mut context.engine, deadline)?;
    let global_names: Vec<crate::Ident> = context.engine.global_bindings.iter().map(|(ident, _)| *ident).collect();
    if let Some(session) = context.engine.session {
        return run_with_session_debugged(program, inputs, context, session, &global_names, deadline);
    }
    let Some((before, after)) = split_at_nodes(program) else {
        let compiled = compiler::compile_program_for_engine(
            program,
            Shared::clone(&context.engine.token_arena),
            context.engine.module_loader.clone(),
            &global_names,
            &context.engine.preresolved_module_vars,
        )?;
        let mut hook = debugger::VmDebuggerHook::new(
            context.debugger,
            context.handler,
            Shared::clone(&context.engine.token_arena),
            context.source,
            compiled.debug_sources.clone(),
            context.engine.module_loader.with_same_resolver(),
            context.engine.host_functions.clone(),
        );
        return inputs
            .map(|input| {
                match run_for_input(input, |v| {
                    interpreter::run_with_debug_hook_and_globals(
                        &compiled,
                        v,
                        context.engine.host_functions,
                        remaining_timeout(deadline),
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
        &context.engine.preresolved_module_vars,
    )?;
    let mut hook = debugger::VmDebuggerHook::new(
        context.debugger,
        context.handler,
        Shared::clone(&context.engine.token_arena),
        context.source,
        compiled.debug_sources.clone(),
        context.engine.module_loader.with_same_resolver(),
        context.engine.host_functions.clone(),
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
                        remaining_timeout(deadline),
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
                            timeout: remaining_timeout(deadline),
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
    let immutable_let_names = immutable_let_names_before_nodes(before);
    let program = program_after_nodes(before, after);
    let input = RuntimeValue::Array(Shared::new(values));
    let result = if let_names.is_empty() {
        let aggregate_compiled = compiler::compile_program_for_engine(
            &program,
            context.engine.token_arena,
            context.engine.module_loader,
            &global_names,
            &context.engine.preresolved_module_vars,
        )?;
        hook.set_sources(aggregate_compiled.debug_sources.clone());
        interpreter::run_with_debug_hook_and_globals(
            &aggregate_compiled,
            input,
            context.engine.host_functions,
            remaining_timeout(deadline),
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
            &immutable_let_names,
            &global_names,
            &context.engine.preresolved_module_vars,
        )?;
        hook.set_sources(aggregate_compiled.debug_sources.clone());
        interpreter::run_with_debug_hook_and_globals_capturing_locals(
            &aggregate_compiled,
            input,
            &let_values,
            interpreter::RunOptions {
                host_functions: context.engine.host_functions,
                timeout: remaining_timeout(deadline),
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

/// Evaluates a debugger expression against paused-frame bindings.
#[cfg(feature = "debugger")]
pub(crate) fn eval_debug_expression<R: ModuleResolver>(
    program: &Program,
    token_arena: TokenArena,
    module_loader: ModuleLoader<R>,
    input: RuntimeValue,
    bindings: &[(crate::Ident, RuntimeValue)],
    host_functions: &HostFunctions,
) -> Result<RuntimeValue, Error> {
    let names: Vec<crate::Ident> = bindings.iter().map(|(name, _)| *name).collect();
    let values: Vec<RuntimeValue> = bindings.iter().map(|(_, value)| value.clone()).collect();
    let compiled = compiler::compile_debug_expression(program, token_arena, module_loader, &names)?;
    Ok(interpreter::run_debug_expression(
        &compiled,
        input,
        &values,
        host_functions,
    )?)
}

#[cfg(test)]
mod tests;
