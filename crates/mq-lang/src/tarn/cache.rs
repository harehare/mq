//! Caches compiled bytecode across repeated evaluations of the same program (non-debugger builds).
use super::nodes_split::{
    immutable_let_names_before_nodes, let_names_before_nodes, program_after_nodes, split_at_nodes,
};
use super::{Error, compiler, interpreter, remaining_timeout, run_for_input, shared_deadline};
use crate::ast::Program;
use crate::runtime::host::HostFunctions;
use crate::runtime::runtime_value::RuntimeValue;
use crate::{ModuleLoader, ModuleResolver, Shared, TokenArena};
use std::fmt;
use std::time::Duration;

/// Bytecode retained for repeated VM evaluation.
#[derive(Clone)]
pub(crate) struct CachedProgram {
    program: compiler::CompiledProgram,
    after: Option<compiler::CompiledProgram>,
    let_names: Vec<crate::Ident>,
    global_names: Vec<crate::Ident>,
    configuration: Vec<String>,
}

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
pub(super) fn compile_cached_program<R: ModuleResolver>(
    program: &Program,
    token_arena: TokenArena,
    module_loader: ModuleLoader<R>,
    configuration: Vec<String>,
    global_bindings: &[(crate::Ident, RuntimeValue)],
) -> Result<CachedProgram, Error> {
    let mut global_names: Vec<crate::Ident> = global_bindings.iter().map(|(name, _)| *name).collect();
    global_names.sort_unstable();
    let (program, after, let_names) = if let Some((before, after)) = split_at_nodes(program) {
        let let_names = let_names_before_nodes(before);
        let immutable_let_names = immutable_let_names_before_nodes(before);
        (
            compiler::compile_program_for_engine(
                &before.to_vec(),
                Shared::clone(&token_arena),
                module_loader.clone(),
                &global_names,
            )?,
            Some(compiler::compile_program_for_engine_with_bindings(
                &program_after_nodes(before, after),
                token_arena,
                module_loader,
                &let_names,
                &immutable_let_names,
                &global_names,
            )?),
            let_names,
        )
    } else {
        (
            compiler::compile_program_for_engine(program, token_arena, module_loader, &global_names)?,
            None,
            Vec::new(),
        )
    };
    Ok(CachedProgram {
        program,
        after,
        let_names,
        global_names,
        configuration,
    })
}

/// Returns whether every external module compiled into this program still has identical source.
pub(super) fn cached_program_is_current<R: ModuleResolver>(
    compiled: &CachedProgram,
    module_loader: &ModuleLoader<R>,
    configuration: &[String],
    global_bindings: &[(crate::Ident, RuntimeValue)],
) -> Result<bool, Error> {
    let mut global_names: Vec<crate::Ident> = global_bindings.iter().map(|(name, _)| *name).collect();
    global_names.sort_unstable();
    if compiled.configuration != configuration || compiled.global_names != global_names {
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
pub(super) fn run_cached<I>(
    compiled: &CachedProgram,
    inputs: I,
    host_functions: &HostFunctions,
    timeout: Option<Duration>,
    max_call_stack_depth: u32,
    global_bindings: &[(crate::Ident, RuntimeValue)],
) -> Result<Vec<RuntimeValue>, Error>
where
    I: Iterator<Item = RuntimeValue>,
{
    let deadline = shared_deadline(timeout);
    // Pools contain reusable frame storage and must remain exclusive to one evaluation.
    // Cached bytecode is immutable and safely shared; frame storage is intentionally local.
    let mut pools = interpreter::ExecutionPools::default();
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
                    global_bindings,
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
                        global_bindings,
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
                return Err(Error::from(error));
            }
        }
    }
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
            global_bindings,
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
                global_bindings,
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
