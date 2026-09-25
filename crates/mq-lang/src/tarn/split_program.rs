//! Engine bytecode split around `nodes`, shared by the cache and `.mqc`.
use super::nodes_split::{
    immutable_let_names_before_nodes, is_declaration, let_names_before_nodes, program_after_nodes, split_at_nodes,
};
use super::{EngineRunContext, Error, VmEnv, compiler, interpreter, map_input_values, remaining_timeout};
use crate::ast::Program;
use crate::runtime::runtime_value::RuntimeValue;
use crate::{Ident, ModuleResolver, Shared};
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
    pub(crate) fn run<I>(
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

    /// Runs without Engine cache state, as for a loaded `.mqc` file.
    #[cfg(feature = "mqc")]
    pub(crate) fn run_standalone<I>(
        &self,
        inputs: I,
        context: &EngineRunContext<'_, impl ModuleResolver>,
    ) -> Result<Vec<RuntimeValue>, Error>
    where
        I: Iterator<Item = RuntimeValue>,
    {
        let deadline = super::shared_deadline(context.timeout);
        let env = VmEnv::from_bindings(context.global_bindings, Shared::clone(&self.program.token_arena));
        let mut pools = interpreter::ExecutionPools::default();
        self.run(inputs, context, deadline, &env, &mut pools)
    }
}
