#[cfg(feature = "debugger")]
use std::borrow::Cow;
use std::path::PathBuf;

use crate::eval::builtin::capability;
#[cfg(feature = "debugger")]
use crate::eval::env::Env;
#[cfg(feature = "debugger")]
use crate::module::ModuleId;
use crate::{
    ArenaId, ModuleResolver, MqResult, Range, RuntimeValue, Shared, SharedCell, TokenKind,
    module::resolver::DefaultModuleResolver, token_alloc,
};
#[cfg(feature = "debugger")]
use crate::{Debugger, DebuggerHandler};

use crate::{
    ModuleLoader, Token,
    arena::Arena,
    error::{self},
    eval::Evaluator,
    optimizer::{OptimizationLevel, Optimizer},
    parse,
};

/// A compiled mq program bundled with its original source, returned by [`Engine::compile`].
#[derive(Debug, Clone)]
pub struct CompiledProgram {
    pub(crate) source: String,
    pub(crate) program: crate::ast::Program,
}

impl CompiledProgram {
    /// Returns the original source code.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Returns the underlying AST nodes.
    pub fn program(&self) -> &crate::ast::Program {
        &self.program
    }
}

impl From<crate::ast::Program> for CompiledProgram {
    /// Wraps a raw `Program` (e.g. from `ast_from_json`) with no source context.
    fn from(program: crate::ast::Program) -> Self {
        Self {
            source: String::new(),
            program,
        }
    }
}

/// The main execution engine for the mq.
///
/// The `Engine` manages parsing, optimization, and evaluation of mq code.
/// It provides methods for configuration, loading modules, and evaluating code.
///
/// # Examples
///
/// ```rust
/// use mq_lang::DefaultEngine;
///
/// let mut engine = DefaultEngine::default();
/// engine.load_builtin_module();
///
/// let input = mq_lang::parse_text_input("hello").unwrap();
/// let result = engine.eval("add(\" world\")", input.into_iter());
/// assert_eq!(result.unwrap(), vec!["hello world".to_string().into()].into());
/// ```
#[derive(Debug, Clone)]
pub struct Engine<T: ModuleResolver = DefaultModuleResolver> {
    pub(crate) evaluator: Evaluator<T>,
    token_arena: Shared<SharedCell<Arena<Shared<Token>>>>,
    optimization_level: OptimizationLevel,
}

fn create_default_token_arena() -> Shared<SharedCell<Arena<Shared<Token>>>> {
    let token_arena = Shared::new(SharedCell::new(Arena::new(2048)));
    token_alloc(
        &token_arena,
        &Shared::new(Token {
            // Ensure at least one token for ArenaId::new(0)
            kind: TokenKind::Eof, // Dummy token
            range: Range::default(),
            module_id: ArenaId::new(0), // Dummy module_id
        }),
    );
    token_arena
}

impl<T: ModuleResolver> Default for Engine<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T: ModuleResolver> Engine<T> {
    pub fn new(module_resolver: T) -> Self {
        let token_arena = create_default_token_arena();
        Self {
            evaluator: Evaluator::new(ModuleLoader::new(module_resolver), Shared::clone(&token_arena)),
            token_arena,
            optimization_level: OptimizationLevel::default(),
        }
    }

    /// Set the optimization level for AST transformations applied before evaluation.
    pub fn set_optimization_level(&mut self, level: OptimizationLevel) {
        self.optimization_level = level;
    }

    /// Set the maximum call stack depth for function calls.
    ///
    /// This prevents infinite recursion by limiting how deep function
    /// calls can be nested. Useful for controlling resource usage.
    pub fn set_max_call_stack_depth(&mut self, max_call_stack_depth: u32) {
        self.evaluator.options.max_call_stack_depth = max_call_stack_depth;
    }

    /// Enables or disables the `http` builtin for the current process.
    ///
    /// Disabled by default. This is a process-wide setting (see
    /// [`capability`](crate::eval::builtin::capability)), not per-`Engine`.
    pub fn set_allow_net(&self, allow: bool) {
        capability::set_allow_net(allow);
    }

    /// Enables or disables the `read_file`/`read_file_bytes` builtins for the current process.
    ///
    /// Disabled by default. This is a process-wide setting (see
    /// [`capability`](crate::eval::builtin::capability)), not per-`Engine`.
    pub fn set_allow_read(&self, allow: bool) {
        capability::set_allow_read(allow);
    }

    /// Enables or disables the `write_file` builtin for the current process.
    ///
    /// Disabled by default. This is a process-wide setting (see
    /// [`capability`](crate::eval::builtin::capability)), not per-`Engine`.
    pub fn set_allow_write(&self, allow: bool) {
        capability::set_allow_write(allow);
    }

    /// Set search paths for module loading.
    ///
    /// These paths will be searched when loading external modules
    /// via the `include` statement in mq code.
    pub fn set_search_paths(&mut self, paths: Vec<PathBuf>) {
        self.evaluator.module_loader.set_search_paths(paths);
    }

    /// Define a string variable that can be used in mq code.
    ///
    /// This allows you to inject values from the host environment
    /// into the mq execution context.
    pub fn define_string_value(&self, name: &str, value: &str) {
        self.evaluator.define_string_value(name, value);
    }

    /// Defines an arbitrary runtime value in the current environment.
    pub fn define_value(&self, name: &str, value: RuntimeValue) {
        self.evaluator.define_value(name, value);
    }

    /// Load the built-in function modules.
    ///
    /// This must be called to enable access to standard functions
    /// like `add`, `sub`, `map`, `filter`, etc.
    pub fn load_builtin_module(&mut self) {
        self.evaluator
            .load_builtin_module()
            .expect("Failed to load builtin module");
    }

    /// Import an external module by name.
    ///
    /// The module will be searched for in the configured search paths
    /// and made available for use in mq code.
    pub fn import_module(&mut self, module_name: &str) -> Result<(), Box<error::Error>> {
        let module = self
            .evaluator
            .module_loader
            .load_from_file(module_name, Shared::clone(&self.token_arena));
        let module =
            module.map_err(|e| error::Error::from_error("", e.into(), self.evaluator.module_loader.clone()))?;

        let _ = self.evaluator.import_module(module).map_err(|e| {
            Box::new(error::Error::from_error(
                "",
                e.into(),
                self.evaluator.module_loader.clone(),
            ))
        })?;

        Ok(())
    }

    /// Load an external module by name.
    ///
    /// The module will be searched for in the configured search paths
    /// and made available for use in mq code.
    pub fn load_module(&mut self, module_name: &str) -> Result<(), Box<error::Error>> {
        let module = self
            .evaluator
            .module_loader
            .load_from_file(module_name, Shared::clone(&self.token_arena));
        let module =
            module.map_err(|e| error::Error::from_error("", e.into(), self.evaluator.module_loader.clone()))?;

        self.evaluator.load_module(module).map_err(|e| {
            Box::new(error::Error::from_error(
                "",
                e.into(),
                self.evaluator.module_loader.clone(),
            ))
        })
    }

    /// The main engine for evaluating mq code.
    ///
    /// The `Engine` manages parsing, optimization, and evaluation of mq.
    /// It provides methods for configuration, loading modules, and evaluating code.
    ///
    /// # Examples
    ///
    /// ```
    /// let mut engine = mq_lang::DefaultEngine::default();
    /// engine.load_builtin_module();
    ///
    /// let input = mq_lang::parse_text_input("hello").unwrap();
    /// let result = engine.eval("add(\" world\")", input.into_iter());
    /// assert_eq!(result.unwrap(), vec!["hello world".to_string().into()].into());
    /// ```
    ///
    pub fn eval<I: Iterator<Item = RuntimeValue>>(&mut self, code: &str, input: I) -> MqResult {
        if code.is_empty() {
            return Ok(vec![].into());
        }

        let program = parse(code, Shared::clone(&self.token_arena))?;
        let program = Optimizer::with_level(self.optimization_level).optimize(program);

        #[cfg(feature = "debugger")]
        self.evaluator.module_loader.set_source_code(code.to_string());

        self.evaluator
            .eval(&program, input.into_iter())
            .map(|values| values.into())
            .map_err(|e| Box::new(error::Error::from_error(code, e, self.evaluator.module_loader.clone())))
    }

    /// Compiles mq code into a [`CompiledProgram`] that can be evaluated multiple times.
    ///
    /// Use this with `eval_compiled` to avoid re-parsing the same query for each input.
    pub fn compile(&mut self, code: &str) -> Result<CompiledProgram, Box<error::Error>> {
        if code.is_empty() {
            return Ok(CompiledProgram {
                source: String::new(),
                program: vec![],
            });
        }
        let program = parse(code, Shared::clone(&self.token_arena))?;
        let program = Optimizer::with_level(self.optimization_level).optimize(program);
        Ok(CompiledProgram {
            source: code.to_string(),
            program,
        })
    }

    /// Evaluates a pre-compiled program against the given input.
    ///
    /// Use with `compile` to avoid re-parsing the same query for each input file,
    /// or with a [`CompiledProgram`] constructed from a deserialized JSON AST (`ast-json` feature).
    ///
    /// # Examples
    ///
    /// ```rust
    /// let mut engine = mq_lang::DefaultEngine::default();
    /// engine.load_builtin_module();
    ///
    /// let compiled = engine.compile("add(\" world\")").unwrap();
    /// let input = mq_lang::parse_text_input("hello").unwrap();
    /// let result = engine.eval_compiled(&compiled, input.into_iter());
    /// assert_eq!(result.unwrap(), vec!["hello world".to_string().into()].into());
    /// ```
    pub fn eval_compiled<I: Iterator<Item = RuntimeValue>>(
        &mut self,
        compiled: &CompiledProgram,
        input: I,
    ) -> MqResult {
        #[cfg(feature = "debugger")]
        self.evaluator.module_loader.set_source_code(compiled.source.clone());

        self.evaluator
            .eval(&compiled.program, input)
            .map(|values| values.into())
            .map_err(|e| {
                Box::new(error::Error::from_error(
                    &compiled.source,
                    e,
                    self.evaluator.module_loader.clone(),
                ))
            })
    }

    /// Returns a reference to the debugger instance.
    ///
    /// This allows interactive debugging of mq code execution when the
    /// `debugger` feature is enabled. Use this to inspect or control
    /// the execution state for advanced debugging scenarios.
    #[cfg(feature = "debugger")]
    pub fn debugger(&self) -> Shared<SharedCell<Debugger>> {
        self.evaluator.debugger()
    }

    #[cfg(feature = "debugger")]
    pub fn set_debugger_handler(&mut self, handler: Box<dyn DebuggerHandler>) {
        self.evaluator.set_debugger_handler(handler);
    }

    #[cfg(feature = "debugger")]
    pub fn token_arena(&self) -> Shared<SharedCell<Arena<Shared<Token>>>> {
        Shared::clone(&self.token_arena)
    }

    /// Returns a reference to the underlying evaluator.
    ///
    /// This is primarily intended for advanced use cases such as debugging,
    /// where direct access to the evaluator internals is required.
    #[cfg(feature = "debugger")]
    pub fn switch_env(&self, env: Shared<SharedCell<Env>>) -> Self {
        #[cfg(not(feature = "sync"))]
        let token_arena = Shared::new(SharedCell::new(self.token_arena.borrow().clone()));
        #[cfg(feature = "sync")]
        let token_arena = Shared::new(SharedCell::new(self.token_arena.read().unwrap().clone()));

        Self {
            evaluator: Evaluator::with_env(Shared::clone(&token_arena), Shared::clone(&env)),
            token_arena: Shared::clone(&token_arena),
            optimization_level: self.optimization_level,
        }
    }

    #[cfg(feature = "debugger")]
    pub fn get_module_name(&self, module_id: ModuleId) -> Cow<'static, str> {
        self.evaluator.module_loader.module_name(module_id)
    }

    #[cfg(feature = "debugger")]
    pub fn get_source_code_for_debug(&self, module_id: ModuleId) -> Result<String, Box<error::Error>> {
        let source_code = self.evaluator.module_loader.get_source_code_for_debug(module_id);

        source_code.map_err(|e| {
            Box::new(error::Error::from_error(
                "",
                e.into(),
                self.evaluator.module_loader.clone(),
            ))
        })
    }

    /// Resolves `module_name` to the path its resolver loaded it from.
    pub fn get_module_path(&self, module_name: &str) -> Result<String, Box<error::Error>> {
        self.evaluator.module_loader.get_module_path(module_name).map_err(|e| {
            Box::new(error::Error::from_error(
                "",
                e.into(),
                self.evaluator.module_loader.clone(),
            ))
        })
    }

    pub const fn version() -> &'static str {
        env!("CARGO_PKG_VERSION")
    }
}

#[cfg(feature = "http-import-ureq")]
impl Engine<DefaultModuleResolver> {
    /// Replaces the HTTP resolver's domain allowlist.
    ///
    /// An empty list restricts access to the built-in default domain
    /// (`raw.githubusercontent.com/harehare`) only; it does not open up all URLs.
    pub fn set_http_allowed_domains(&mut self, domains: Vec<String>) {
        self.evaluator.module_loader.set_http_allowed_domains(domains);
    }

    /// Clears all locally-cached HTTP module files.
    ///
    /// Call this once before processing to force a re-fetch of all cached modules
    /// on the next resolve (e.g. when `--refresh-modules` is passed on the CLI).
    pub fn clear_http_cache(&self) -> Result<(), crate::module::error::ModuleError> {
        self.evaluator.module_loader.clear_http_cache()
    }

    /// Clears all HTTP module cache including versioned modules and lock files.
    ///
    /// Use this when `--clear-cache` is passed on the CLI to wipe everything.
    pub fn clear_http_cache_all(&self) -> Result<(), crate::module::error::ModuleError> {
        self.evaluator.module_loader.clear_http_cache_all()
    }

    /// Enables or disables the `mq.lock` integrity check for HTTP imports (on by default).
    pub fn set_lockfile_enabled(&mut self, enabled: bool) {
        self.evaluator.module_loader.set_lockfile_enabled(enabled);
    }

    /// Sets the path used for `mq.lock`.
    pub fn set_lockfile_path(&mut self, path: std::path::PathBuf) {
        self.evaluator.module_loader.set_lockfile_path(path);
    }
}

#[cfg(test)]
mod tests {
    use super::CompiledProgram;
    use crate::DefaultEngine;
    use rstest::rstest;
    use scopeguard::defer;
    use std::io::Write;
    use std::{fs::File, path::PathBuf};

    fn create_file(name: &str, content: &str) -> (PathBuf, PathBuf) {
        let temp_dir = std::env::temp_dir();
        let temp_file_path = temp_dir.join(name);
        let mut file = File::create(&temp_file_path).expect("Failed to create temp file");
        file.write_all(content.as_bytes())
            .expect("Failed to write to temp file");

        (temp_dir, temp_file_path)
    }

    #[test]
    fn test_set_paths() {
        let mut engine = DefaultEngine::default();
        let paths = vec![PathBuf::from("/test/path")];
        engine.set_search_paths(paths.clone());
        assert_eq!(engine.evaluator.module_loader.search_paths(), paths);
    }

    #[test]
    fn test_set_max_call_stack_depth() {
        let mut engine = DefaultEngine::default();
        let default_depth = engine.evaluator.options.max_call_stack_depth;
        let new_depth = default_depth + 10;

        engine.set_max_call_stack_depth(new_depth);
        assert_eq!(engine.evaluator.options.max_call_stack_depth, new_depth);
    }

    #[test]
    fn test_version() {
        let version = DefaultEngine::version();
        assert!(!version.is_empty());
    }

    #[test]
    fn test_load_module() {
        let (temp_dir, temp_file_path) = create_file("test_module.mq", "def func1(): 42;");
        let temp_file_path_clone = temp_file_path.clone();

        defer! {
            if temp_file_path_clone.exists() {
                std::fs::remove_file(&temp_file_path_clone).expect("Failed to delete temp file");
            }
        }

        let mut engine = DefaultEngine::default();
        engine.set_search_paths(vec![temp_dir]);

        let result = engine.load_module("test_module");
        assert!(result.is_ok());
    }

    #[test]
    fn test_error_load_module() {
        let (temp_dir, temp_file_path) = create_file("error.mq", "error");
        let temp_file_path_clone = temp_file_path.clone();

        defer! {
            if temp_file_path_clone.exists() {
                std::fs::remove_file(&temp_file_path_clone).expect("Failed to delete temp file");
            }
        }

        let mut engine = DefaultEngine::default();
        engine.set_search_paths(vec![temp_dir]);

        let result = engine.load_module("error");
        assert!(result.is_err());
    }

    #[test]
    fn test_eval() {
        let mut engine = DefaultEngine::default();
        let result = engine.eval("add(1, 1)", vec!["".to_string().into()].into_iter());
        assert!(result.is_ok());
        let values = result.unwrap();
        assert_eq!(values.len(), 1);
    }

    #[rstest]
    #[case("add(1, 1)", "add(1, 1)")]
    #[case(".", ".")]
    #[case("length(.)", "length(.)")]
    fn test_compiled_program_source(#[case] query: &str, #[case] expected: &str) {
        let mut engine = DefaultEngine::default();
        let compiled = engine.compile(query).unwrap();
        assert_eq!(compiled.source(), expected);
        assert!(!compiled.program().is_empty());
        assert_eq!(compiled.clone().source(), expected);
    }

    #[rstest]
    #[case("")]
    fn test_compile_empty_code(#[case] query: &str) {
        let mut engine = DefaultEngine::default();
        let compiled = engine.compile(query).unwrap();
        assert_eq!(compiled.source(), "");
        assert!(compiled.program().is_empty());
    }

    // --- builtin cache tests ---

    /// Two sequential engines calling the same builtin functions must produce identical results,
    /// whether the builtin module was loaded from a fresh parse or replayed from the cache.
    #[rstest]
    #[case("add(1, 2)", vec!["".to_string().into()], vec![3.into()])]
    #[case("not(false)", vec!["".to_string().into()], vec![true.into()])]
    #[case("to_string(42)", vec!["".to_string().into()], vec!["42".to_string().into()])]
    fn test_builtin_cache_sequential_engines_consistent(
        #[case] query: &str,
        #[case] input: Vec<crate::RuntimeValue>,
        #[case] expected: Vec<crate::RuntimeValue>,
    ) {
        let mut engine1 = DefaultEngine::default();
        engine1.load_builtin_module();
        let result1 = engine1.eval(query, input.clone().into_iter()).unwrap();

        let mut engine2 = DefaultEngine::default();
        engine2.load_builtin_module();
        let result2 = engine2.eval(query, input.into_iter()).unwrap();

        assert_eq!(result1.values(), &expected);
        assert_eq!(result2.values(), &expected);
    }

    /// Compiling and evaluating a builtin function call on a second engine (cache path) must
    /// produce the correct result — verifying that token_ids in the compiled program are valid
    /// when the builtin tokens were injected from cache rather than freshly parsed.
    #[rstest]
    #[case("add(1, 2)", vec!["".to_string().into()], vec![3.into()])]
    #[case("not(false)", vec!["".to_string().into()], vec![true.into()])]
    #[case("len(\"hello\")", vec!["".to_string().into()], vec![5.into()])]
    fn test_builtin_cache_eval_compiled_token_ids_valid(
        #[case] query: &str,
        #[case] input: Vec<crate::RuntimeValue>,
        #[case] expected: Vec<crate::RuntimeValue>,
    ) {
        let mut engine1 = DefaultEngine::default();
        engine1.load_builtin_module();

        let mut engine2 = DefaultEngine::default();
        engine2.load_builtin_module();
        let compiled = engine2.compile(query).unwrap();
        let result = engine2.eval_compiled(&compiled, input.into_iter()).unwrap();
        assert_eq!(result.values(), &expected);
    }

    /// Runtime errors on a cache-using engine must carry the correct source_code.
    #[rstest]
    #[case("undefined_fn()", "undefined_fn()")]
    #[case("unknown_call(1, 2)", "unknown_call(1, 2)")]
    fn test_builtin_cache_runtime_error_preserves_source(#[case] query: &str, #[case] expected_source: &str) {
        let mut engine1 = DefaultEngine::default();
        engine1.load_builtin_module();

        let mut engine2 = DefaultEngine::default();
        engine2.load_builtin_module();
        let compiled = engine2.compile(query).unwrap();
        let err = engine2
            .eval_compiled(&compiled, crate::null_input().into_iter())
            .unwrap_err();
        assert_eq!(err.source_code.inner(), expected_source);
    }

    /// The error location (token offset + span) must point to the erroring identifier in
    /// source_code.  If cached tokens were injected at shifted positions the offset would
    /// land on the wrong character.
    #[rstest]
    #[case("undefined_fn()", "undefined_fn")]
    #[case("1 | undefined_fn()", "undefined_fn")]
    #[case("add(1) | unknown_fn()", "unknown_fn")]
    fn test_builtin_cache_runtime_error_token_location_correct(#[case] query: &str, #[case] expected_ident: &str) {
        let mut engine1 = DefaultEngine::default();
        engine1.load_builtin_module();

        let mut engine2 = DefaultEngine::default();
        engine2.load_builtin_module();
        let compiled = engine2.compile(query).unwrap();
        let err = engine2
            .eval_compiled(&compiled, crate::null_input().into_iter())
            .unwrap_err();

        let offset = err.location.offset();
        let len = err.location.len();
        assert_eq!(
            &err.source_code.inner()[offset..offset + len],
            expected_ident,
            "location must point to the erroring identifier, not a shifted position"
        );
        assert_eq!(offset, query.find(expected_ident).unwrap());
    }

    /// Two sequential engines (one possibly fresh-parse, one cache) must produce identical
    /// error locations — confirming that token_id indices are not shifted by cache replay.
    #[rstest]
    #[case("undefined_fn()")]
    #[case("1 | undefined_fn()")]
    #[case("add(1) | unknown_fn()")]
    fn test_builtin_cache_and_fresh_parse_error_location_identical(#[case] query: &str) {
        let mut engine1 = DefaultEngine::default();
        engine1.load_builtin_module();
        let compiled1 = engine1.compile(query).unwrap();
        let err1 = engine1
            .eval_compiled(&compiled1, crate::null_input().into_iter())
            .unwrap_err();

        let mut engine2 = DefaultEngine::default();
        engine2.load_builtin_module();
        let compiled2 = engine2.compile(query).unwrap();
        let err2 = engine2
            .eval_compiled(&compiled2, crate::null_input().into_iter())
            .unwrap_err();

        assert_eq!(
            err1.location, err2.location,
            "error location must be identical regardless of whether builtin cache was used"
        );
    }

    // --- CompiledProgram unit tests ---

    #[test]
    fn test_compiled_program_from_has_empty_source() {
        let compiled = CompiledProgram::from(vec![]);
        assert_eq!(compiled.source(), "");
        assert!(compiled.program().is_empty());
    }

    #[rstest]
    #[case("add(1, 1)", vec!["".to_string().into()], vec![2.into()])]
    #[case("add(\" world\")", vec!["hello".to_string().into()], vec!["hello world".to_string().into()])]
    #[case("add(\" world\")", vec!["hi".to_string().into()], vec!["hi world".to_string().into()])]
    fn test_eval_compiled(
        #[case] query: &str,
        #[case] input: Vec<crate::RuntimeValue>,
        #[case] expected: Vec<crate::RuntimeValue>,
    ) {
        let mut engine = DefaultEngine::default();
        engine.load_builtin_module();
        let compiled = engine.compile(query).unwrap();
        let result = engine.eval_compiled(&compiled, input.into_iter());
        assert!(result.is_ok());
        assert_eq!(result.unwrap().values(), &expected);
    }

    #[rstest]
    #[case("undefined_fn()", "undefined_fn()")]
    #[case("unknown()", "unknown()")]
    fn test_eval_compiled_runtime_error_preserves_source(#[case] query: &str, #[case] expected_source: &str) {
        let mut engine = DefaultEngine::default();
        let compiled = engine.compile(query).unwrap();
        let err = engine
            .eval_compiled(&compiled, crate::null_input().into_iter())
            .unwrap_err();
        assert_eq!(err.source_code.inner(), expected_source);
    }

    #[rstest]
    #[case("undefined_fn()")]
    #[case("unknown()")]
    fn test_eval_compiled_from_program_has_empty_source_in_error(#[case] query: &str) {
        let mut engine = DefaultEngine::default();
        let original = engine.compile(query).unwrap();
        let no_source = CompiledProgram::from(original.program().clone());
        assert_eq!(no_source.source(), "");
        let err = engine
            .eval_compiled(&no_source, crate::null_input().into_iter())
            .unwrap_err();
        assert_eq!(err.source_code.inner(), "");
    }

    #[test]
    fn test_eval_compiled_with_ast() {
        use crate::{AstExpr, AstLiteral, AstNode, Shared};

        let mut engine = DefaultEngine::default();
        engine.load_builtin_module();

        let program = vec![Shared::new(AstNode {
            token_id: crate::arena::ArenaId::new(1),
            expr: Shared::new(AstExpr::Literal(AstLiteral::String("hello".to_string()))),
        })];

        let compiled = CompiledProgram::from(program);
        let result = engine.eval_compiled(&compiled, crate::null_input().into_iter());
        assert!(result.is_ok());
        let values = result.unwrap();
        assert_eq!(values.len(), 1);
        assert_eq!(values[0], "hello".to_string().into());
    }

    #[cfg(feature = "sync")]
    #[test]
    fn test_engine_thread_usage_with_sync_feature() {
        use std::sync::{Arc, Mutex};

        use crate::Engine;

        let engine: Arc<Mutex<Engine>> = Arc::new(Mutex::new(Engine::default()));
        let engine_clone = Arc::clone(&engine);

        let handle = std::thread::spawn(move || {
            let mut engine = engine_clone.lock().unwrap();
            let result = engine.eval("2 + 3", vec!["".to_string().into()].into_iter());
            assert!(result.is_ok());
            let values = result.unwrap();
            assert_eq!(values.len(), 1);
            assert_eq!(values[0], 5.into());
        });

        handle.join().expect("Threaded engine usage failed");
    }

    #[cfg(feature = "debugger")]
    #[test]
    fn test_switch_env() {
        use crate::eval::env::Env;
        use crate::{RuntimeValue, Shared, SharedCell, null_input};

        let engine = DefaultEngine::default();
        let env = Shared::new(SharedCell::new(Env::default()));

        env.write().unwrap().define("runtime".into(), RuntimeValue::NONE);

        let mut new_engine = engine.switch_env(env);

        assert_eq!(
            new_engine.eval("runtime", null_input().into_iter()).unwrap()[0],
            RuntimeValue::NONE
        );
    }

    #[cfg(feature = "debugger")]
    #[test]
    fn test_get_source_code_for_debug() {
        use crate::module::ModuleId;

        let mut engine = DefaultEngine::default();
        engine.load_builtin_module();

        let module_id = ModuleId::new(0);
        let result = engine.get_source_code_for_debug(module_id);

        assert!(result.is_ok());
    }
}
