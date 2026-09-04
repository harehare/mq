//! Caches compiled bytecode across repeated evaluations of the same program (non-debugger builds).
use super::nodes_split::{let_names_before_nodes, program_after_nodes, split_at_nodes};
use super::{Error, compiler, interpreter, remaining_timeout, run_for_input, shared_deadline};
use crate::ast::Program;
use crate::runtime::host::HostFunctions;
use crate::runtime::runtime_value::RuntimeValue;
use crate::{ModuleLoader, ModuleResolver, Shared, SharedCell, TokenArena};
use std::fmt;
use std::time::Duration;

/// Bytecode retained for repeated VM evaluation.
#[cfg(not(feature = "debugger"))]
#[derive(Clone)]
pub(crate) struct CachedProgram {
    program: compiler::CompiledProgram,
    after: Option<compiler::CompiledProgram>,
    let_names: Vec<crate::Ident>,
    configuration: Vec<String>,
    execution_pools: Shared<SharedCell<interpreter::ExecutionPools>>,
}

#[cfg(not(feature = "debugger"))]
impl fmt::Debug for CachedProgram {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CachedProgram")
            .field("program", &self.program)
            .field("after", &self.after)
            .field("configuration", &self.configuration)
            .finish_non_exhaustive()
    }
}

/// Compiles an Engine program for repeated evaluation.
#[cfg(not(feature = "debugger"))]
pub(super) fn compile_cached_program<R: ModuleResolver>(
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
#[cfg(not(feature = "debugger"))]
pub(super) fn cached_program_is_current<R: ModuleResolver>(
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
#[cfg(not(feature = "debugger"))]
pub(super) fn run_cached<I>(
    compiled: &CachedProgram,
    inputs: I,
    host_functions: &HostFunctions,
    timeout: Option<Duration>,
    max_call_stack_depth: u32,
) -> Result<Vec<RuntimeValue>, Error>
where
    I: Iterator<Item = RuntimeValue>,
{
    let deadline = shared_deadline(timeout);
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
                    remaining_timeout(deadline),
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
                        timeout: remaining_timeout(deadline),
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
        interpreter::run_with_globals(
            after,
            input,
            host_functions,
            remaining_timeout(deadline),
            max_call_stack_depth,
            &[],
        )
    } else {
        let let_values: Vec<RuntimeValue> = let_bindings.into_iter().map(|(_, value)| value).collect();
        interpreter::run_with_globals_capturing_locals(
            after,
            input,
            &let_values,
            interpreter::RunOptions {
                host_functions,
                timeout: remaining_timeout(deadline),
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

#[cfg(all(not(feature = "debugger"), not(feature = "sync")))]
fn take_execution_pools(pools: &Shared<SharedCell<interpreter::ExecutionPools>>) -> interpreter::ExecutionPools {
    std::mem::take(&mut *pools.borrow_mut())
}

#[cfg(all(not(feature = "debugger"), feature = "sync"))]
fn take_execution_pools(pools: &Shared<SharedCell<interpreter::ExecutionPools>>) -> interpreter::ExecutionPools {
    std::mem::take(&mut *pools.write().expect("execution pool lock is poisoned"))
}

#[cfg(all(not(feature = "debugger"), not(feature = "sync")))]
fn restore_execution_pools(
    pools: &Shared<SharedCell<interpreter::ExecutionPools>>,
    execution_pools: interpreter::ExecutionPools,
) {
    *pools.borrow_mut() = execution_pools;
}

#[cfg(all(not(feature = "debugger"), feature = "sync"))]
fn restore_execution_pools(
    pools: &Shared<SharedCell<interpreter::ExecutionPools>>,
    execution_pools: interpreter::ExecutionPools,
) {
    *pools.write().expect("execution pool lock is poisoned") = execution_pools;
}
