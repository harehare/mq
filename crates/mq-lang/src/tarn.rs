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
use crate::TokenArena;
use crate::ast::Program;
use crate::ast::node::Node;
use crate::eval::host::HostFunctions;
use crate::eval::runtime_value::RuntimeValue;
use crate::module::resolver::std_resolver::StdModuleResolver;
use crate::{ModuleLoader, ModuleResolver};
use std::fmt;
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
    pub(crate) fn into_inner_error(self, token_arena: TokenArena) -> crate::error::InnerError {
        match self {
            Error::Compile(compiler::CompileError::Module(e)) => crate::error::InnerError::from(e),
            Error::Compile(other) => crate::error::InnerError::from(compile_error_to_runtime_error(other, token_arena)),
            Error::Vm(e) => crate::error::InnerError::from(vm_error_to_runtime_error(&e, token_arena)),
        }
    }
}

/// Bytecode retained by [`crate::CompiledProgram`] for repeated VM evaluation.
#[cfg(all(feature = "tarn", not(feature = "debugger")))]
#[derive(Debug, Clone)]
pub(crate) struct CachedProgram(compiler::CompiledProgram);

/// Compiles an Engine program for reuse when its bytecode cannot depend on an external module.
#[cfg(all(feature = "tarn", not(feature = "debugger")))]
pub(crate) fn compile_cached_program<R: ModuleResolver>(
    program: &Program,
    token_arena: TokenArena,
    module_loader: ModuleLoader<R>,
) -> Result<Option<CachedProgram>, Error> {
    let compiled = compiler::compile_program_for_engine(program, token_arena, module_loader, &[])?;
    Ok(compiled.cacheable.then_some(CachedProgram(compiled)))
}

/// Runs a bytecode program cached by [`compile_cached_program`] for every input.
#[cfg(all(feature = "tarn", not(feature = "debugger")))]
pub(crate) fn run_cached_many<I>(
    compiled: &CachedProgram,
    inputs: I,
    host_functions: &HostFunctions,
    timeout: Option<Duration>,
    max_call_stack_depth: u32,
) -> Result<Vec<RuntimeValue>, Error>
where
    I: Iterator<Item = RuntimeValue>,
{
    inputs
        .map(|input| {
            run_for_input(input, |value| {
                interpreter::run(&compiled.0, value, host_functions, timeout, max_call_stack_depth)
            })
            .map_err(Error::from)
        })
        .collect()
}

fn compile_error_to_runtime_error(
    err: compiler::CompileError,
    token_arena: TokenArena,
) -> crate::error::runtime::RuntimeError {
    use crate::error::runtime::RuntimeError;
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
        compiler::CompileError::Module(_) => unreachable!("routed to InnerError::Module by into_inner_error instead"),
    }
}

pub(crate) fn vm_error_to_runtime_error(
    err: &interpreter::VmError,
    token_arena: TokenArena,
) -> crate::error::runtime::RuntimeError {
    use crate::error::runtime::RuntimeError;
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
        RuntimeValue::Markdown(node, _) => *node,
    }
}

type ProgramSlice<'a> = &'a [Shared<Node>];

fn split_at_nodes(program: &Program) -> Option<(ProgramSlice<'_>, ProgramSlice<'_>)> {
    let index = program.iter().position(|node| node.is_nodes())?;
    Some(program.split_at(index))
}

#[allow(clippy::too_many_arguments)]
fn run_nodes_aggregate<R: ModuleResolver>(
    after: &[Shared<Node>],
    values: Vec<RuntimeValue>,
    host_functions: &HostFunctions,
    timeout: Option<Duration>,
    max_call_stack_depth: u32,
    token_arena: TokenArena,
    module_loader: ModuleLoader<R>,
    global_bindings: &[(crate::Ident, RuntimeValue)],
) -> Result<Vec<RuntimeValue>, Error> {
    let compiled = compiler::compile_program_for_engine(
        &after.to_vec(),
        token_arena,
        module_loader,
        &global_bindings.iter().map(|(ident, _)| *ident).collect::<Vec<_>>(),
    )?;
    match interpreter::run_with_globals(
        &compiled,
        RuntimeValue::Array(Shared::new(values)),
        host_functions,
        timeout,
        max_call_stack_depth,
        global_bindings,
    )? {
        RuntimeValue::Array(values) => Ok(Shared::unwrap_or_clone(values)),
        value => Ok(vec![value]),
    }
}

#[allow(dead_code, clippy::too_many_arguments)]
pub(crate) fn compile_and_run_many<I, R: ModuleResolver>(
    program: &Program,
    inputs: I,
    host_functions: &HostFunctions,
    timeout: Option<Duration>,
    max_call_stack_depth: u32,
    token_arena: TokenArena,
    module_loader: ModuleLoader<R>,
    global_bindings: &[(crate::Ident, RuntimeValue)],
) -> Result<Vec<RuntimeValue>, Error>
where
    I: Iterator<Item = RuntimeValue>,
{
    let global_names: Vec<crate::Ident> = global_bindings.iter().map(|(ident, _)| *ident).collect();
    let Some((before, after)) = split_at_nodes(program) else {
        let compiled = compiler::compile_program_for_engine(program, token_arena, module_loader, &global_names)?;
        return inputs
            .map(|input| {
                run_for_input(input, |v| {
                    interpreter::run_with_globals(
                        &compiled,
                        v,
                        host_functions,
                        timeout,
                        max_call_stack_depth,
                        global_bindings,
                    )
                })
                .map_err(Error::from)
            })
            .collect();
    };
    let compiled = compiler::compile_program_for_engine(
        &before.to_vec(),
        Shared::clone(&token_arena),
        module_loader.clone(),
        &global_names,
    )?;
    let values = inputs
        .map(|input| {
            run_for_input(input, |v| {
                interpreter::run_with_globals(
                    &compiled,
                    v,
                    host_functions,
                    timeout,
                    max_call_stack_depth,
                    global_bindings,
                )
            })
            .map_err(Error::from)
        })
        .collect::<Result<Vec<_>, _>>()?;
    run_nodes_aggregate(
        after,
        values,
        host_functions,
        timeout,
        max_call_stack_depth,
        token_arena,
        module_loader,
        global_bindings,
    )
}

/// Runs one input through the VM while adapting boundaries to the existing debugger API.
#[cfg(feature = "debugger")]
#[allow(dead_code)] // Called by Engine's opt-in VM path before the M5 default cutover.
#[allow(clippy::too_many_arguments)]
pub(crate) fn compile_and_run_debugged<R: ModuleResolver>(
    program: &Program,
    input: RuntimeValue,
    host_functions: &HostFunctions,
    timeout: Option<Duration>,
    max_call_stack_depth: u32,
    token_arena: TokenArena,
    module_loader: ModuleLoader<R>,
    global_bindings: &[(crate::Ident, RuntimeValue)],
    debugger: Shared<SharedCell<Debugger>>,
    handler: Shared<SharedCell<Box<dyn DebuggerHandler>>>,
    source: Source,
) -> Result<RuntimeValue, Error> {
    compile_and_run_debugged_many(
        program,
        std::iter::once(input),
        host_functions,
        timeout,
        max_call_stack_depth,
        token_arena,
        module_loader,
        global_bindings,
        debugger,
        handler,
        source,
    )
    .and_then(|mut values| {
        values
            .pop()
            .ok_or(Error::Vm(interpreter::VmError::Corrupt("missing VM result")))
    })
}

#[cfg(feature = "debugger")]
#[allow(dead_code)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn compile_and_run_debugged_many<I, R: ModuleResolver>(
    program: &Program,
    inputs: I,
    host_functions: &HostFunctions,
    timeout: Option<Duration>,
    max_call_stack_depth: u32,
    token_arena: TokenArena,
    module_loader: ModuleLoader<R>,
    global_bindings: &[(crate::Ident, RuntimeValue)],
    debugger: Shared<SharedCell<Debugger>>,
    handler: Shared<SharedCell<Box<dyn DebuggerHandler>>>,
    source: Source,
) -> Result<Vec<RuntimeValue>, Error>
where
    I: Iterator<Item = RuntimeValue>,
{
    let global_names: Vec<crate::Ident> = global_bindings.iter().map(|(ident, _)| *ident).collect();
    let Some((before, after)) = split_at_nodes(program) else {
        let compiled = compiler::compile_program_for_engine(
            program,
            Shared::clone(&token_arena),
            module_loader.clone(),
            &global_names,
        )?;
        let mut hook = debugger::VmDebuggerHook::new(
            debugger,
            handler,
            token_arena,
            source,
            module_loader.clone(),
            compiled.debug_sources.clone(),
        );
        return inputs
            .map(|input| {
                match run_for_input(input, |v| {
                    interpreter::run_with_debug_hook_and_globals(
                        &compiled,
                        v,
                        host_functions,
                        timeout,
                        max_call_stack_depth,
                        global_bindings,
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
        Shared::clone(&token_arena),
        module_loader.clone(),
        &global_names,
    )?;
    let mut hook = debugger::VmDebuggerHook::new(
        debugger,
        handler,
        Shared::clone(&token_arena),
        source,
        module_loader.clone(),
        compiled.debug_sources.clone(),
    );
    let values = inputs
        .map(|input| {
            match run_for_input(input, |v| {
                interpreter::run_with_debug_hook_and_globals(
                    &compiled,
                    v,
                    host_functions,
                    timeout,
                    max_call_stack_depth,
                    global_bindings,
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
        .collect::<Result<Vec<_>, Error>>()?;
    let aggregate_compiled =
        compiler::compile_program_for_engine(&after.to_vec(), token_arena, module_loader, &global_names)?;
    hook.set_sources(aggregate_compiled.debug_sources.clone());
    match interpreter::run_with_debug_hook_and_globals(
        &aggregate_compiled,
        RuntimeValue::Array(Shared::new(values)),
        host_functions,
        timeout,
        max_call_stack_depth,
        global_bindings,
        &mut hook,
    ) {
        Ok(RuntimeValue::Array(values)) => Ok(Shared::unwrap_or_clone(values)),
        Ok(value) => Ok(vec![value]),
        Err(error) => {
            hook.notify_error(&error);
            Err(error.into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Arena, Shared, SharedCell};
    use proptest::prelude::*;
    use rstest::rstest;

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
        assert_eq!(result.values()[0], RuntimeValue::String("a".to_string()));
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
        assert_eq!(run(code), RuntimeValue::String(expected.to_string()));
    }

    #[test]
    fn false_if_without_else_yields_none() {
        assert_eq!(run("if(false): 1;"), RuntimeValue::None);
    }

    #[test]
    fn interpolation_can_reference_self() {
        assert_eq!(
            run_with_input(r#"s"value=${self}""#, RuntimeValue::Number(42.0.into())),
            RuntimeValue::String("value=42".to_string())
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

    /// Reference output from the tree-walker for the same code/input, used to cross-check
    /// `run_for_input`'s port of `eval_markdown_node` against the real thing rather than a
    /// hand-derived expectation — `map_values`' "recurse into a container only when the
    /// query found nothing to match at this level, via `to_fragment`" behavior is subtle
    /// enough that re-deriving it by hand is easy to get wrong.
    fn tree_walk_eval(code: &str, input: RuntimeValue) -> RuntimeValue {
        let mut engine = crate::DefaultEngine::default();
        engine.load_builtin_module();
        let values = engine.eval(code, std::iter::once(input)).unwrap();
        values.values()[0].clone()
    }

    fn tree_walk_eval_many(code: &str, inputs: Vec<RuntimeValue>) -> Vec<RuntimeValue> {
        let mut engine = crate::DefaultEngine::default();
        engine.load_builtin_module();
        engine.eval(code, inputs.into_iter()).unwrap().values().clone()
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
            &HostFunctions::default(),
            None,
            crate::eval::Options::default().max_call_stack_depth,
            token_arena,
            ModuleLoader::new(StdModuleResolver),
            &[],
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
        let results = compile_and_run_debugged_many(
            &program,
            inputs.into_iter(),
            &HostFunctions::default(),
            None,
            crate::eval::Options::default().max_call_stack_depth,
            token_arena,
            ModuleLoader::new(StdModuleResolver),
            &[],
            debugger,
            handler,
            Source {
                name: None,
                code: code.to_string(),
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
            &HostFunctions::default(),
            None,
            crate::eval::Options::default().max_call_stack_depth,
            token_arena,
            ModuleLoader::new(StdModuleResolver),
            &[],
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
            &HostFunctions::default(),
            None,
            crate::eval::Options::default().max_call_stack_depth,
            token_arena,
            ModuleLoader::new(StdModuleResolver),
            &[],
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
            &HostFunctions::default(),
            None,
            crate::eval::Options::default().max_call_stack_depth,
            token_arena,
            ModuleLoader::new(StdModuleResolver),
            &[],
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
            &HostFunctions::default(),
            None,
            crate::eval::Options::default().max_call_stack_depth,
            token_arena,
            ModuleLoader::new(StdModuleResolver),
            &[],
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
        }

        impl DebuggerHandler for RecordingHandler {
            fn on_breakpoint_hit(&self, _breakpoint: &crate::Breakpoint, context: &DebugContext) -> DebuggerAction {
                if let Ok(value) = context.env.read().unwrap().resolve(crate::Ident::new("inner")) {
                    self.inner_values.lock().unwrap().push(value);
                }
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
        let handler: Shared<SharedCell<Box<dyn DebuggerHandler>>> =
            Shared::new(SharedCell::new(Box::new(RecordingHandler {
                inner_values: Arc::clone(&inner_values),
            })));
        let mut hook = debugger::VmDebuggerHook::new(
            debugger,
            handler,
            token_arena,
            Source {
                name: None,
                code: String::new(),
            },
            ModuleLoader::new(StdModuleResolver),
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
            ModuleLoader::new(StdModuleResolver),
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
            RuntimeValue::String("| a | b |\n| --- | --- |\n| 1 | 2 |".to_string())
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
}
