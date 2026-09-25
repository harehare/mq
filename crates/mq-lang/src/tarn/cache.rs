//! Caches compiled bytecode across repeated evaluations of the same program (non-debugger builds).
use super::split_program::SplitProgram;
use super::{EngineRunContext, Error, engine, interpreter};
use crate::ast::Program;
use crate::runtime::runtime_value::RuntimeValue;
use crate::tarn::{VmEnv, VmEnvCacheKey, VmModuleCacheKey};
use crate::{ModuleResolver, Shared, SharedCell};
use std::fmt;
#[cfg(not(all(target_arch = "wasm32", feature = "wasm")))]
use std::time::Instant;
#[cfg(all(target_arch = "wasm32", feature = "wasm"))]
use web_time::Instant;

/// Bytecode retained for repeated VM evaluation.
pub(crate) struct CachedProgram {
    split: SplitProgram,
    configuration: Vec<engine::VmModulePrelude>,
    /// Global snapshot used to bake module `let` initializers into constants; must still match
    /// for the cache to stay valid, since a global's value can change under the same name.
    baked_globals_key: VmEnvCacheKey,
    /// Baked module `let`s read engine globals.
    bakes_globals: bool,
    /// Cached bytecode includes module definitions, which are frozen per Engine.
    module_cache_key: VmModuleCacheKey,
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
            .field("split", &self.split)
            .field("configuration", &self.configuration)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
impl CachedProgram {
    /// Whether the program run once per input instantiates any closure at top level.
    pub(crate) fn per_input_program_makes_closures(&self) -> bool {
        use super::bytecode::OpCode;
        self.split.program.chunks[0]
            .code
            .iter()
            .any(|op| matches!(op, OpCode::MakeClosure(_) | OpCode::MakeStaticClosure(_)))
    }

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
    baked_globals_key: VmEnvCacheKey,
    module_cache_key: VmModuleCacheKey,
) -> Result<CachedProgram, Error> {
    Ok(CachedProgram {
        split: SplitProgram::compile(program, context, deadline)?,
        configuration,
        baked_globals_key,
        bakes_globals: preresolved_module_vars.reads_globals,
        module_cache_key,
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
        let env = Shared::new(VmEnv::from_bindings(
            global_bindings,
            Shared::clone(&compiled.split.program.token_arena),
        ));
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
        let env = Shared::new(VmEnv::from_bindings(
            global_bindings,
            Shared::clone(&compiled.split.program.token_arena),
        ));
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

/// Returns whether bytecode was compiled with the same Engine configuration and frozen modules.
pub(super) fn cached_program_is_current(
    compiled: &CachedProgram,
    configuration: &[engine::VmModulePrelude],
    environment_key: VmEnvCacheKey,
    module_cache_key: VmModuleCacheKey,
) -> bool {
    // Global values matter only when baked into module `let`s.
    if compiled.configuration != configuration
        || compiled.baked_globals_key.source != environment_key.source
        || compiled.baked_globals_key.names_revision != environment_key.names_revision
        || (compiled.bakes_globals && compiled.baked_globals_key.revision != environment_key.revision)
        || compiled.module_cache_key != module_cache_key
    {
        return false;
    }
    true
}

/// Runs a bytecode program cached by [`compile_cached_program`] for every input.
///
/// `deadline` must be the same deadline used for compiling `compiled` (on a cache miss) so the
/// whole `Engine::eval_compiled` call shares one wall-clock budget instead of each stage getting
/// its own fresh `timeout` window.
pub(super) fn run_cached<I>(
    compiled: &CachedProgram,
    inputs: I,
    context: &EngineRunContext<'_, impl ModuleResolver>,
    deadline: Option<Instant>,
    environment_key: VmEnvCacheKey,
) -> Result<Vec<RuntimeValue>, Error>
where
    I: Iterator<Item = RuntimeValue>,
{
    // Pools contain mutable frame storage, so a caller takes exclusive ownership for the
    // duration of its evaluation and restores it on every exit path.
    let mut pools = take_execution_pools(compiled);
    // Reuse the map until this engine changes its globals. This also covers line-oriented
    // callers, which invoke `eval_compiled` once per row.
    let env = cached_environment(compiled, environment_key, context.global_bindings);
    let result = compiled.split.run(inputs, context, deadline, &env, &mut pools);
    restore_execution_pools(compiled, pools);
    result
}
