//! [`Session`]: evaluates queries one after another, as a REPL does.
use super::{CompiledProgram, Engine, VmModulePrelude};
use crate::ast::{
    Program,
    node::{Expr, Literal},
};
use crate::io::{Io, NativeIo, SandboxedIo};
use crate::module::ModuleId;
use crate::runtime::builtin::io_context;
use crate::tarn::SessionBindings;
use crate::{
    ModuleResolver, MqResult, RuntimeValue, Shared, SharedCell, error, module::resolver::DefaultModuleResolver,
    parse_in_module,
};

/// Marks module ids of session queries, which the module loader never assigns.
const QUERY_ID_BIT: u32 = 1 << 31;

/// Evaluates queries one after another, carrying top-level `let`/`var`/`def` bindings and
/// `import`/`include` directives from each query to the next.
///
/// Errors show the query they happened in, named `repl#N` for the Nth query, so an error in a
/// function defined by an earlier query points at that query's source.
///
/// ```rust
/// let mut session = mq_lang::Session::new(mq_lang::DefaultEngine::default());
/// session.eval("let x = 41", mq_lang::null_input().into_iter()).unwrap();
/// let result = session.eval("x + 1", mq_lang::null_input().into_iter()).unwrap();
/// assert_eq!(result, vec![42.into()].into());
/// ```
#[derive(Debug)]
pub struct Session<T: ModuleResolver = DefaultModuleResolver, IO: Io = SandboxedIo<NativeIo>> {
    engine: Engine<T, IO>,
    bindings: SessionBindings,
    /// Source of each query, indexed by its module id.
    sources: Vec<String>,
}

impl<T: ModuleResolver, IO: Io> Session<T, IO> {
    /// Starts a session that evaluates queries on `engine`.
    pub fn new(engine: Engine<T, IO>) -> Self {
        Self {
            engine,
            bindings: Shared::new(SharedCell::new(Vec::new())),
            sources: Vec::new(),
        }
    }

    /// The engine queries run on.
    pub fn engine(&self) -> &Engine<T, IO> {
        &self.engine
    }

    /// The engine queries run on, e.g. to define values or load modules.
    pub fn engine_mut(&mut self) -> &mut Engine<T, IO> {
        &mut self.engine
    }

    /// Ends the session, returning its engine.
    pub fn into_engine(self) -> Engine<T, IO> {
        self.engine
    }

    /// Evaluates `code` with the bindings and modules of the queries before it.
    pub fn eval<I: Iterator<Item = RuntimeValue>>(&mut self, code: &str, input: I) -> MqResult {
        if code.is_empty() {
            return Ok(vec![].into());
        }

        // Scoped before parsing so bare `$VAR` resolution sees the engine's `Io`.
        let _io_guard = io_context::scoped(Shared::clone(&self.engine.vm.io) as Shared<dyn Io>);
        // Tokens stay in the engine's arena, since bindings outlive the query.
        let token_arena = Shared::clone(&self.engine.token_arena);
        self.sources.push(code.to_string());
        let module_id = ModuleId::new(QUERY_ID_BIT | (self.sources.len() - 1) as u32);
        let program = match parse_in_module(code, Shared::clone(&token_arena), module_id) {
            Ok(program) => program,
            Err(cause) => {
                let error = self.error(cause);
                self.sources.pop();
                return Err(Box::new(error));
            }
        };

        #[cfg(feature = "debugger")]
        self.engine.vm.module_loader.set_source_code(code.to_string());

        let compiled = CompiledProgram::cached(code.to_string(), program);
        let result = self
            .engine
            .run_compiled(&compiled, input, token_arena, Some(&self.bindings))
            .map_err(|cause| Box::new(self.error(cause)))?;
        if let Some(program) = compiled.program() {
            self.keep_modules(program);
        }
        Ok(result)
    }

    /// Keeps top-level `import`/`include` directives so later queries can use the modules.
    fn keep_modules(&mut self, program: &Program) {
        for node in program {
            let module = match &node.expr {
                Expr::Include(Literal::String(name)) => VmModulePrelude::Include(name.clone()),
                Expr::Import(Literal::String(name), alias) => {
                    VmModulePrelude::Import(name.clone(), alias.as_ref().map(|alias| alias.name.to_string()))
                }
                _ => continue,
            };
            if !self.engine.vm_module_prelude.contains(&module) {
                self.engine.vm_module_prelude.push(module);
            }
        }
    }

    /// Builds a diagnostic for `cause`, showing the query or module source it happened in.
    fn error(&self, cause: error::InnerError) -> error::Error {
        let current = self.sources.last().cloned().unwrap_or_default();
        let module_loader = &self.engine.vm.module_loader;
        error::Error::from_error_with(cause, |module_id| match module_id {
            Some(module_id) if module_id.raw() & QUERY_ID_BIT != 0 => {
                let index = (module_id.raw() & !QUERY_ID_BIT) as usize;
                let source = self.sources.get(index).cloned().unwrap_or_default();
                (format!("repl#{}", index + 1), source)
            }
            Some(module_id) => (
                module_loader.module_file_name(module_id),
                module_loader
                    .get_source_code(module_id, current.clone())
                    .unwrap_or_default(),
            ),
            None => (String::new(), current.clone()),
        })
    }
}
