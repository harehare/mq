//! [`Session`]: evaluates queries one after another, as a REPL does.
use super::{CompiledProgram, Engine, VmModulePrelude};
use crate::ast::node::{Expr, Literal, Node};
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
    bindings: SessionBindings,
    /// Source of each query, indexed by its module id.
    sources: Vec<String>,
    /// Declared last, so the engine collects after the bindings are released.
    engine: Engine<T, IO>,
}

impl<T: ModuleResolver, IO: Io> Session<T, IO> {
    /// Starts a session that evaluates queries on `engine`.
    pub fn new(mut engine: Engine<T, IO>) -> Self {
        engine.vm.module_loader.keep_modules_parsed();
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
    fn keep_modules(&mut self, program: &[Shared<Node>]) {
        for node in program {
            let module = match &node.expr {
                Expr::Include(Literal::String(name)) => VmModulePrelude::Include(name.to_string()),
                Expr::Import(Literal::String(name), alias) => {
                    VmModulePrelude::Import(name.to_string(), alias.as_ref().map(|alias| alias.name.to_string()))
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

#[cfg(test)]
mod import_cache_tests {
    use super::*;
    use rstest::rstest;
    use std::path::Path;

    fn write(dir: &Path, name: &str, code: &str) {
        std::fs::write(dir.join(format!("{name}.mq")), code).unwrap();
    }

    fn session_in(dir: &Path) -> Session {
        let mut engine = crate::DefaultEngine::default();
        engine.set_search_paths(vec![dir.to_path_buf()]);
        engine.load_builtin_module();
        Session::new(engine)
    }

    fn run(session: &mut Session, code: &str) -> MqResult {
        session.eval(code, crate::null_input().into_iter())
    }

    fn tokens(session: &Session) -> usize {
        #[cfg(not(feature = "sync"))]
        let arena = session.engine.token_arena.borrow();
        #[cfg(feature = "sync")]
        let arena = session.engine.token_arena.read().unwrap();
        arena.len()
    }

    #[test]
    fn evals_after_an_import_do_not_reparse_the_module() {
        let mut engine = crate::DefaultEngine::default();
        engine.load_builtin_module();
        let mut session = Session::new(engine);
        run(&mut session, "import \"csv\"").unwrap();
        run(&mut session, "1 + 1").unwrap();
        let before = tokens(&session);
        for _ in 0..100 {
            run(&mut session, "1 + 1").unwrap();
        }
        assert!(tokens(&session) - before < 100 * 16);
    }

    #[test]
    fn plain_eval_does_not_keep_modules_parsed() {
        let mut engine = crate::DefaultEngine::default();
        engine.load_builtin_module();
        let before = tokens_of(&engine);
        for _ in 0..20 {
            engine
                .eval("import \"csv\" | 1 + 1", crate::null_input().into_iter())
                .unwrap();
        }
        assert_eq!(tokens_of(&engine), before);
    }

    fn tokens_of(engine: &crate::DefaultEngine) -> usize {
        #[cfg(not(feature = "sync"))]
        let arena = engine.token_arena.borrow();
        #[cfg(feature = "sync")]
        let arena = engine.token_arena.read().unwrap();
        arena.len()
    }

    #[rstest]
    #[case::import("import \"m\"", "m::double(4)")]
    #[case::alias("import \"m\" as k", "k::double(4)")]
    #[case::include("include \"m\"", "double(4)")]
    fn module_functions_stay_correct_across_evals(#[case] directive: &str, #[case] call: &str) {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "m", "def double(x): x * 2;");
        let mut session = session_in(dir.path());
        run(&mut session, directive).unwrap();
        for _ in 0..3 {
            assert_eq!(run(&mut session, call).unwrap(), vec![8.into()].into());
        }
    }

    #[test]
    fn several_modules_and_nested_imports_work_together() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "base", "def inc(x): x + 1;");
        write(
            dir.path(),
            "outer",
            "import \"base\"\ndef inc2(x): base::inc(base::inc(x));",
        );
        write(dir.path(), "other", "def twice(x): x * 2;");
        let mut session = session_in(dir.path());
        run(&mut session, "import \"outer\"").unwrap();
        run(&mut session, "import \"other\"").unwrap();
        for _ in 0..3 {
            assert_eq!(
                run(&mut session, "other::twice(outer::inc2(1))").unwrap(),
                vec![6.into()].into()
            );
        }
    }

    #[test]
    fn module_let_variables_survive_repeated_evals() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "m", "let base = 10\n| def add(x): x + base;");
        let mut session = session_in(dir.path());
        run(&mut session, "import \"m\"").unwrap();
        for _ in 0..3 {
            assert_eq!(run(&mut session, "m::add(1)").unwrap(), vec![11.into()].into());
        }
    }

    #[test]
    fn an_error_in_a_cached_module_names_the_module_file() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "m", "def boom(x): x / 0;");
        let mut session = session_in(dir.path());
        run(&mut session, "import \"m\"").unwrap();
        let first = format!("{:?}", run(&mut session, "m::boom(1)").unwrap_err());
        let second = format!("{:?}", run(&mut session, "m::boom(1)").unwrap_err());
        assert!(first.contains("name: \"m.mq\""));
        assert!(second.contains("name: \"m.mq\""));
    }

    #[test]
    fn changing_search_paths_loads_the_module_from_the_new_path() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        write(first.path(), "m", "def which(): 1;");
        write(second.path(), "m", "def which(): 2;");
        let mut session = session_in(first.path());
        run(&mut session, "import \"m\"").unwrap();
        assert_eq!(run(&mut session, "m::which()").unwrap(), vec![1.into()].into());
        session.engine_mut().set_search_paths(vec![second.path().to_path_buf()]);
        assert_eq!(run(&mut session, "m::which()").unwrap(), vec![2.into()].into());
    }
}
