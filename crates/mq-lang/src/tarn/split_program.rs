//! Engine bytecode split around `nodes`, shared by the cache and `.mqc`.
#[cfg(not(feature = "debugger"))]
use super::VmEnvCacheKey;
use super::nodes_split::{
    immutable_let_names_before_nodes, is_declaration, let_names_before_nodes, program_after_nodes, split_at_nodes,
};
use super::{EngineRunContext, Error, VmEnv, compiler, interpreter, map_input_values, remaining_timeout};
use crate::ast::Program;
use crate::runtime::runtime_value::RuntimeValue;
use crate::{Ident, ModuleResolver, Shared, SharedCell};
use std::fmt;
#[cfg(not(all(target_arch = "wasm32", feature = "wasm")))]
use std::time::Instant;
#[cfg(all(target_arch = "wasm32", feature = "wasm"))]
use web_time::Instant;

/// Uninstrumented bytecode for repeated evaluation.
#[derive(Debug)]
pub(crate) struct SplitProgram {
    /// Runs per input (the part before `nodes`).
    pub(crate) program: compiler::CompiledProgram,
    /// Runs once over all results (the part after `nodes`).
    pub(crate) after: Option<compiler::CompiledProgram>,
    /// Bindings before `nodes`, carried into `after` from the last input.
    pub(crate) let_names: Vec<Ident>,
    /// Slots of `let_names` in `program`.
    let_slots: Vec<interpreter::CaptureSlot>,
    reuse: RunReuse,
}

/// State reused between evaluations; a concurrent caller allocates its own pools.
struct RunReuse {
    execution_pools: SharedCell<Option<interpreter::ExecutionPools>>,
    /// Keyed by the engine-global snapshot it was built from.
    #[cfg(not(feature = "debugger"))]
    environment: SharedCell<Option<(VmEnvCacheKey, Shared<VmEnv>)>>,
}

impl Default for RunReuse {
    fn default() -> Self {
        Self {
            execution_pools: SharedCell::new(Some(interpreter::ExecutionPools::default())),
            #[cfg(not(feature = "debugger"))]
            environment: SharedCell::new(None),
        }
    }
}

impl fmt::Debug for RunReuse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RunReuse").finish_non_exhaustive()
    }
}

impl RunReuse {
    fn take_pools(&self) -> interpreter::ExecutionPools {
        #[cfg(not(feature = "sync"))]
        let pools = self.execution_pools.borrow_mut().take();
        #[cfg(feature = "sync")]
        let pools = self.execution_pools.write().unwrap().take();
        pools.unwrap_or_default()
    }

    fn restore_pools(&self, pools: interpreter::ExecutionPools) {
        #[cfg(not(feature = "sync"))]
        let mut slot = self.execution_pools.borrow_mut();
        #[cfg(feature = "sync")]
        let mut slot = self.execution_pools.write().unwrap();
        if slot.is_none() {
            *slot = Some(pools);
        }
    }

    #[cfg(not(feature = "debugger"))]
    fn environment(
        &self,
        key: VmEnvCacheKey,
        global_bindings: &[(Ident, RuntimeValue)],
        token_arena: &crate::TokenArena,
    ) -> Shared<VmEnv> {
        #[cfg(not(feature = "sync"))]
        let mut slot = self.environment.borrow_mut();
        #[cfg(feature = "sync")]
        let mut slot = self.environment.write().unwrap();
        if let Some((cached_key, env)) = slot.as_ref()
            && *cached_key == key
        {
            return Shared::clone(env);
        }
        let env = Shared::new(VmEnv::from_bindings(global_bindings, Shared::clone(token_arena)));
        *slot = Some((key, Shared::clone(&env)));
        env
    }
}

impl SplitProgram {
    pub(crate) fn new(
        program: compiler::CompiledProgram,
        after: Option<compiler::CompiledProgram>,
        let_names: Vec<Ident>,
    ) -> Self {
        let let_slots = interpreter::capture_slots(&program.chunks[0], &let_names);
        Self {
            program,
            after,
            let_names,
            let_slots,
            reuse: RunReuse::default(),
        }
    }

    /// Compiles `program`, baking module `let` values in as constants.
    pub(crate) fn compile<R: ModuleResolver>(
        program: &Program,
        context: &mut EngineRunContext<'_, R>,
        deadline: Option<Instant>,
    ) -> Result<Self, Error> {
        let token_arena = Shared::clone(&context.token_arena);
        let mut global_names: Vec<Ident> = context.global_bindings.iter().map(|(name, _)| *name).collect();
        global_names.sort_unstable();
        // Resolve on the context's loader so the clone below reuses loaded modules.
        let preresolved_module_vars = super::resolve_module_prelude_globals(program, context, deadline)?;
        let module_loader = context.module_loader.clone();
        let Some((before, after)) = split_at_nodes(program) else {
            let program = compiler::compile_uninstrumented_program_for_engine(
                program,
                token_arena,
                module_loader,
                &[],
                &[],
                &global_names,
                &preresolved_module_vars,
            )?;
            return Ok(Self::new(program, None, Vec::new()));
        };
        let let_names = let_names_before_nodes(before);
        let immutable_let_names = immutable_let_names_before_nodes(before);
        // `after` re-declares these, so skip them per input.
        let per_input = if before.iter().all(|node| is_declaration(node)) {
            Program::new()
        } else {
            before.to_vec()
        };
        let program_before = compiler::compile_uninstrumented_program_for_engine(
            &per_input,
            Shared::clone(&token_arena),
            module_loader.clone(),
            &[],
            &[],
            &global_names,
            &preresolved_module_vars,
        )?;
        let program_after = compiler::compile_uninstrumented_program_for_engine(
            &program_after_nodes(before, after),
            token_arena,
            module_loader,
            &let_names,
            &immutable_let_names,
            &global_names,
            &preresolved_module_vars,
        )?;
        Ok(Self::new(program_before, Some(program_after), let_names))
    }

    /// Runs every input, then the `nodes` part over all results.
    fn run<I>(
        &self,
        inputs: I,
        context: &EngineRunContext<'_, impl ModuleResolver>,
        deadline: Option<Instant>,
        env: &VmEnv,
        pools: &mut interpreter::ExecutionPools,
    ) -> Result<Vec<RuntimeValue>, Error>
    where
        I: Iterator<Item = RuntimeValue>,
    {
        let mut values = Vec::new();
        let mut let_bindings: Vec<(Ident, RuntimeValue)> = Vec::new();
        for input in inputs {
            let result = map_input_values(input, |value| {
                let execution_pools = std::mem::take(pools);
                if self.let_names.is_empty() {
                    let (result, next_pools) = interpreter::run_with_env_and_pools(
                        &self.program,
                        value,
                        context.run_options(remaining_timeout(deadline)),
                        env,
                        execution_pools,
                    );
                    *pools = next_pools;
                    result
                } else {
                    let (result, captured, next_pools) = interpreter::run_with_env_capturing_slots(
                        &self.program,
                        value,
                        &[],
                        context.run_options(remaining_timeout(deadline)),
                        env,
                        &self.let_slots,
                        execution_pools,
                    );
                    *pools = next_pools;
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
        let Some(after) = &self.after else {
            return Ok(values);
        };
        let input = RuntimeValue::Array(Shared::new(values));
        let result = if self.let_names.is_empty() {
            let (result, next_pools) = interpreter::run_with_env_and_pools(
                after,
                input,
                context.run_options(remaining_timeout(deadline)),
                env,
                std::mem::take(pools),
            );
            *pools = next_pools;
            result
        } else {
            let let_values: Vec<RuntimeValue> = let_bindings.into_iter().map(|(_, value)| value).collect();
            let (result, _, next_pools) = interpreter::run_with_env_capturing_locals(
                after,
                input,
                &let_values,
                context.run_options(remaining_timeout(deadline)),
                env,
                &[],
                std::mem::take(pools),
            );
            *pools = next_pools;
            result
        };
        match result? {
            RuntimeValue::Array(values) => Ok(Shared::unwrap_or_clone(values)),
            value => Ok(vec![value]),
        }
    }

    /// Compiles with a deadline from the context's timeout.
    #[cfg(feature = "mqc")]
    pub(crate) fn compile_standalone<R: ModuleResolver>(
        program: &Program,
        context: &mut EngineRunContext<'_, R>,
    ) -> Result<Self, Error> {
        let deadline = super::shared_deadline(context.timeout);
        Self::compile(program, context, deadline)
    }

    /// Like `run`, reusing frame pools and the globals environment across calls.
    pub(crate) fn run_reusing<I>(
        &self,
        inputs: I,
        context: &EngineRunContext<'_, impl ModuleResolver>,
        deadline: Option<Instant>,
        #[cfg(not(feature = "debugger"))] environment_key: VmEnvCacheKey,
    ) -> Result<Vec<RuntimeValue>, Error>
    where
        I: Iterator<Item = RuntimeValue>,
    {
        let mut pools = self.reuse.take_pools();
        #[cfg(not(feature = "debugger"))]
        let env = self
            .reuse
            .environment(environment_key, context.global_bindings, &self.program.token_arena);
        #[cfg(feature = "debugger")]
        let env = VmEnv::from_bindings(context.global_bindings, Shared::clone(&self.program.token_arena));
        let result = self.run(inputs, context, deadline, &env, &mut pools);
        self.reuse.restore_pools(pools);
        result
    }

    #[cfg(all(test, not(feature = "debugger")))]
    pub(crate) fn has_available_execution_pools(&self) -> bool {
        #[cfg(not(feature = "sync"))]
        let slot = self.reuse.execution_pools.borrow();
        #[cfg(feature = "sync")]
        let slot = self.reuse.execution_pools.read().unwrap();
        slot.is_some()
    }
}
