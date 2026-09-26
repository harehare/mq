//! Caches compiled bytecode across repeated evaluations of the same program (non-debugger builds).
use super::split_program::SplitProgram;
use super::{EngineRunContext, Error, engine};
use crate::ModuleResolver;
use crate::ast::Program;
use crate::runtime::runtime_value::RuntimeValue;
use crate::tarn::{VmEnvCacheKey, VmModuleCacheKey};
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
        self.split.has_available_execution_pools()
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
    })
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
    compiled.split.run_reusing(inputs, context, deadline, environment_key)
}
