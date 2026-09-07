//! Caches compiled bytecode across repeated evaluations of the same program (non-debugger builds).
use super::nodes_split::{
    immutable_let_names_before_nodes, let_names_before_nodes, program_after_nodes, split_at_nodes,
};
use super::{
    EngineRunContext, Error, compiler, engine, interpreter, remaining_timeout, resolve_module_prelude_globals,
    run_for_input,
};
use crate::ast::Program;
use crate::runtime::host::HostFunctions;
use crate::runtime::runtime_value::RuntimeValue;
use crate::tarn::{VmEnv, VmEnvCacheKey};
use crate::{ModuleLoader, ModuleResolver, Shared, SharedCell};
use std::fmt;
use std::time::Instant;

/// Bytecode retained for repeated VM evaluation.
pub(crate) struct CachedProgram {
    program: compiler::CompiledProgram,
    after: Option<compiler::CompiledProgram>,
    let_names: Vec<crate::Ident>,
    global_names: Vec<crate::Ident>,
    configuration: Vec<engine::VmModulePrelude>,
    /// Frame storage retained between non-overlapping `eval_compiled` calls.
    ///
    /// References to a cached program share this slot. A concurrent caller that finds it empty
    /// simply allocates an independent pool, so bytecode remains safely reusable.
    execution_pools: Shared<SharedCell<Option<interpreter::ExecutionPools>>>,
    /// Lookup table for the most recently used engine-global snapshot. This is independent of
    /// frame pools: concurrent callers can safely retain different environments.
    environment: Shared<SharedCell<Option<CachedEnvironment>>>,
}

struct CachedEnvironment {
    key: VmEnvCacheKey,
    env: Shared<VmEnv>,
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

#[cfg(test)]
impl CachedProgram {
    pub(crate) fn has_available_execution_pools(&self) -> bool {
        #[cfg(not(feature = "sync"))]
        {
            self.execution_pools.borrow().is_some()
        }
        #[cfg(feature = "sync")]
        {
            self.execution_pools.read().unwrap().is_some()
        }
    }
}

/// Compiles an Engine program for repeated evaluation.
///
/// Only runs on a cache miss, so module var initializers resolve here exactly once; a cache
/// hit reuses the bytecode (and its already-baked constants) without calling this again.
pub(super) fn compile_cached_program<R: ModuleResolver>(
    program: &Program,
    context: &mut EngineRunContext<'_, R>,
    configuration: Vec<engine::VmModulePrelude>,
    deadline: Option<Instant>,
) -> Result<CachedProgram, Error> {
    let token_arena = Shared::clone(&context.token_arena);
    let global_bindings = context.global_bindings;
    let mut global_names: Vec<crate::Ident> = global_bindings.iter().map(|(name, _)| *name).collect();
    global_names.sort_unstable();
    // Resolve on `context.module_loader` itself so the clone below inherits already-loaded
    // (and AST-cached) modules instead of the real compile loading them again.
    let preresolved_module_vars = resolve_module_prelude_globals(program, context, deadline)?;
    let module_loader = context.module_loader.clone();
    let (program, after, let_names) = if let Some((before, after)) = split_at_nodes(program) {
        let let_names = let_names_before_nodes(before);
        let immutable_let_names = immutable_let_names_before_nodes(before);
        (
            compiler::compile_program_for_engine(
                &before.to_vec(),
                Shared::clone(&token_arena),
                module_loader.clone(),
                &global_names,
                &preresolved_module_vars,
            )?,
            Some(compiler::compile_program_for_engine_with_bindings(
                &program_after_nodes(before, after),
                token_arena,
                module_loader,
                &let_names,
                &immutable_let_names,
                &global_names,
                &preresolved_module_vars,
            )?),
            let_names,
        )
    } else {
        (
            compiler::compile_program_for_engine(
                program,
                token_arena,
                module_loader,
                &global_names,
                &preresolved_module_vars,
            )?,
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
        execution_pools: Shared::new(SharedCell::new(Some(interpreter::ExecutionPools::default()))),
        environment: Shared::new(SharedCell::new(None)),
    })
}

fn cached_environment(
    compiled: &CachedProgram,
    key: VmEnvCacheKey,
    global_bindings: &[(crate::Ident, RuntimeValue)],
) -> Shared<VmEnv> {
    #[cfg(not(feature = "sync"))]
    {
        let mut slot = compiled.environment.borrow_mut();
        if let Some(environment) = slot.as_ref()
            && environment.key == key
        {
            return Shared::clone(&environment.env);
        }
        let env = Shared::new(VmEnv::from_bindings(global_bindings));
        *slot = Some(CachedEnvironment {
            key,
            env: Shared::clone(&env),
        });
        env
    }
    #[cfg(feature = "sync")]
    {
        let mut slot = compiled.environment.write().unwrap();
        if let Some(environment) = slot.as_ref()
            && environment.key == key
        {
            return Shared::clone(&environment.env);
        }
        let env = Shared::new(VmEnv::from_bindings(global_bindings));
        *slot = Some(CachedEnvironment {
            key,
            env: Shared::clone(&env),
        });
        env
    }
}

fn take_execution_pools(compiled: &CachedProgram) -> interpreter::ExecutionPools {
    #[cfg(not(feature = "sync"))]
    {
        compiled.execution_pools.borrow_mut().take().unwrap_or_default()
    }
    #[cfg(feature = "sync")]
    {
        compiled.execution_pools.write().unwrap().take().unwrap_or_default()
    }
}

fn restore_execution_pools(compiled: &CachedProgram, pools: interpreter::ExecutionPools) {
    #[cfg(not(feature = "sync"))]
    {
        let mut slot = compiled.execution_pools.borrow_mut();
        if slot.is_none() {
            *slot = Some(pools);
        }
    }
    #[cfg(feature = "sync")]
    {
        let mut slot = compiled.execution_pools.write().unwrap();
        if slot.is_none() {
            *slot = Some(pools);
        }
    }
}

/// Returns whether every external module compiled into this program still has identical source.
pub(super) fn cached_program_is_current<R: ModuleResolver>(
    compiled: &CachedProgram,
    module_loader: &ModuleLoader<R>,
    configuration: &[engine::VmModulePrelude],
    global_bindings: &[(crate::Ident, RuntimeValue)],
) -> Result<bool, Error> {
    // `run_cached` reaches this check for every `eval_compiled` call. The compiled names are
    // already sorted, so avoid allocating and sorting another list just to compare a set of
    // names. Values intentionally do not participate: `GetExternalGlobal` reads the current
    // value from the per-evaluation environment.
    let globals_match = compiled.global_names.len() == global_bindings.len()
        && global_bindings
            .iter()
            .all(|(name, _)| compiled.global_names.binary_search(name).is_ok());
    if compiled.configuration != configuration || !globals_match {
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
///
/// `deadline` must be the same deadline used for compiling `compiled` (on a cache miss) so the
/// whole `Engine::eval_compiled` call shares one wall-clock budget instead of each stage getting
/// its own fresh `timeout` window.
pub(super) fn run_cached<I>(
    compiled: &CachedProgram,
    inputs: I,
    host_functions: &HostFunctions,
    deadline: Option<Instant>,
    max_call_stack_depth: u32,
    global_bindings: &[(crate::Ident, RuntimeValue)],
    environment_key: VmEnvCacheKey,
) -> Result<Vec<RuntimeValue>, Error>
where
    I: Iterator<Item = RuntimeValue>,
{
    // Pools contain mutable frame storage, so a caller takes exclusive ownership for the
    // duration of its evaluation and restores it on every exit path.
    let mut pools = take_execution_pools(compiled);
    let result = (|| {
        // Reuse the map until this engine changes its globals. This also covers line-oriented
        // callers, which invoke `eval_compiled` once per row.
        let env = cached_environment(compiled, environment_key, global_bindings);
        let mut values = Vec::new();
        let mut let_bindings: Vec<(crate::Ident, RuntimeValue)> = Vec::new();
        for input in inputs {
            let result = run_for_input(input, |value| {
                let execution_pools = std::mem::take(&mut pools);
                if compiled.let_names.is_empty() {
                    let (result, next_pools) = interpreter::run_with_env_and_pools(
                        &compiled.program,
                        value,
                        host_functions,
                        remaining_timeout(deadline),
                        max_call_stack_depth,
                        &env,
                        execution_pools,
                    );
                    pools = next_pools;
                    result
                } else {
                    let (result, captured, next_pools) = interpreter::run_with_env_capturing_locals(
                        &compiled.program,
                        value,
                        &[],
                        interpreter::RunOptions {
                            host_functions,
                            timeout: remaining_timeout(deadline),
                            max_call_stack_depth,
                            global_bindings,
                        },
                        &env,
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
                Err(error) => return Err(Error::from(error)),
            }
        }
        let Some(after) = &compiled.after else {
            return Ok(values);
        };
        let input = RuntimeValue::Array(Shared::new(values));
        let result = if compiled.let_names.is_empty() {
            let (result, next_pools) = interpreter::run_with_env_and_pools(
                after,
                input,
                host_functions,
                remaining_timeout(deadline),
                max_call_stack_depth,
                &env,
                std::mem::take(&mut pools),
            );
            pools = next_pools;
            result
        } else {
            let let_values: Vec<RuntimeValue> = let_bindings.into_iter().map(|(_, value)| value).collect();
            let (result, _, next_pools) = interpreter::run_with_env_capturing_locals(
                after,
                input,
                &let_values,
                interpreter::RunOptions {
                    host_functions,
                    timeout: remaining_timeout(deadline),
                    max_call_stack_depth,
                    global_bindings,
                },
                &env,
                &[],
                std::mem::take(&mut pools),
            );
            pools = next_pools;
            result
        };
        match result? {
            RuntimeValue::Array(values) => Ok(Shared::unwrap_or_clone(values)),
            value => Ok(vec![value]),
        }
    })();
    restore_execution_pools(compiled, pools);
    result
}
