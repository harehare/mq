#[cfg(feature = "debugger")]
use std::borrow::Cow;
use std::path::PathBuf;

#[cfg(feature = "debugger")]
use crate::eval::env::Env;
#[cfg(feature = "debugger")]
use crate::module::ModuleId;
use crate::optimizer::OptimizationLevel;
#[cfg(feature = "debugger")]
use crate::{Debugger, DebuggerHandler};
use crate::{LocalFsModuleResolver, ModuleResolver, MqResult, RuntimeValue, Shared, SharedCell, token_alloc};

use crate::{
    ModuleLoader, Token,
    arena::Arena,
    error::{self},
    eval::Evaluator,
    optimizer::Optimizer,
    parse,
};

/// Configuration options for the mq engine.
#[derive(Debug, Clone)]
pub struct Options {
    /// Whether to enable code optimization during evaluation.
    /// When enabled, performs constant folding and dead code elimination.
    pub optimization_level: OptimizationLevel,
}

impl Default for Options {
    fn default() -> Self {
        #[cfg(not(feature = "debugger"))]
        {
            Self {
                optimization_level: OptimizationLevel::Full,
            }
        }
        #[cfg(feature = "debugger")]
        {
            Self {
                optimization_level: OptimizationLevel::None,
            }
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
pub struct Engine<T: ModuleResolver = LocalFsModuleResolver> {
    pub(crate) evaluator: Evaluator<T>,
    pub(crate) options: Options,
    token_arena: Shared<SharedCell<Arena<Shared<Token>>>>,
}

fn create_default_token_arena() -> Shared<SharedCell<Arena<Shared<Token>>>> {
    let token_arena = Shared::new(SharedCell::new(Arena::new(10240)));
    token_alloc(
        &token_arena,
        &Shared::new(Token {
            // Ensure at least one token for ArenaId::new(0)
            kind: crate::TokenKind::Eof, // Dummy token
            range: crate::range::Range::default(),
            module_id: crate::arena::ArenaId::new(0), // Dummy module_id
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
            options: Options::default(),
            token_arena,
        }
    }

    /// Enable or disable code optimization.
    ///
    /// When optimization is enabled, the engine performs constant folding
    /// and dead code elimination to improve execution performance.
    pub fn set_optimization_level(&mut self, level: OptimizationLevel) {
        self.options.optimization_level = level;
    }

    /// Set the maximum call stack depth for function calls.
    ///
    /// This prevents infinite recursion by limiting how deep function
    /// calls can be nested. Useful for controlling resource usage.
    pub fn set_max_call_stack_depth(&mut self, max_call_stack_depth: u32) {
        self.evaluator.options.max_call_stack_depth = max_call_stack_depth;
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

    /// Load the built-in function modules.
    ///
    /// This must be called to enable access to standard functions
    /// like `add`, `sub`, `map`, `filter`, etc.
    pub fn load_builtin_module(&mut self) {
        self.evaluator
            .load_builtin_module()
            .expect("Failed to load builtin module");
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

    /// Evaluates mq code with a specified optimization level.
    ///
    /// This method allows you to override the engine's current optimization level
    /// for a single evaluation. Useful for benchmarking or testing different
    /// optimization strategies.
    ///
    /// # Examples
    ///
    /// ```
    /// use mq_lang::{Engine, OptimizationLevel};
    ///
    /// let mut engine: Engine = Engine::default();
    /// engine.load_builtin_module();
    ///
    /// let input = mq_lang::parse_text_input("hello").unwrap();
    /// let result = engine.eval_with_level("add(\" world\")", input.into_iter(), OptimizationLevel::None);
    /// assert_eq!(result.unwrap(), vec!["hello world".to_string().into()].into());
    /// ```
    #[inline]
    pub fn eval_with_level<I: Iterator<Item = RuntimeValue>>(
        &mut self,
        code: &str,
        input: I,
        level: OptimizationLevel,
    ) -> MqResult {
        let mut program = parse(code, Shared::clone(&self.token_arena))?;
        Optimizer::with_level(level).optimize(&mut program);

        #[cfg(feature = "debugger")]
        self.evaluator.module_loader.set_source_code(code.to_string());

        self.evaluator
            .eval(&program, input.into_iter())
            .map(|values| values.into())
            .map_err(|e| Box::new(error::Error::from_error(code, e, self.evaluator.module_loader.clone())))
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
        self.eval_with_level(code, input, self.options.optimization_level)
    }

    /// Evaluates a pre-parsed AST (Program).
    ///
    /// This is similar to `eval`, but takes an AST directly, skipping parsing.
    /// The AST is typically obtained from deserializing a JSON AST.
    ///
    /// # Examples
    ///
    /// ```rust
    /// #[cfg(feature = "ast-json")]
    /// use mq_lang::{Engine, AstNode, AstExpr, AstLiteral, Program, RuntimeValue, Shared};
    ///
    /// let mut engine: Engine = Engine::default();
    /// engine.load_builtin_module();
    ///
    /// let json = r#"[
    ///   {
    ///     "expr": {
    ///       "Literal": {"String": "hello"}
    ///     }
    ///   }
    /// ]"#;
    /// let program: mq_lang::Program = serde_json::from_str(json).unwrap();
    /// let result = engine.eval_ast(program, mq_lang::null_input().into_iter());
    /// assert_eq!(result.unwrap(), vec!["hello".to_string().into()].into());
    /// ```
    #[cfg(feature = "ast-json")]
    pub fn eval_ast<I: Iterator<Item = RuntimeValue>>(
        &mut self,
        mut program: crate::ast::Program,
        input: I,
    ) -> MqResult {
        Optimizer::with_level(self.options.optimization_level).optimize(&mut program);

        self.evaluator
            .eval(&program, input.into_iter())
            .map(|values| values.into())
            .map_err(|e| Box::new(error::Error::from_error("", e, self.evaluator.module_loader.clone())))
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
            options: self.options.clone(),
            token_arena: Shared::clone(&token_arena),
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

    pub const fn version() -> &'static str {
        env!("CARGO_PKG_VERSION")
    }
}

#[cfg(test)]
mod tests {
    use crate::DefaultEngine;

    use super::*;
    use mq_test::defer;

    #[test]
    fn test_set_paths() {
        let mut engine: Engine = Engine::default();
        let paths = vec![PathBuf::from("/test/path")];
        engine.set_search_paths(paths.clone());
        assert_eq!(engine.evaluator.module_loader.search_paths(), paths);
    }

    #[test]
    fn test_set_max_call_stack_depth() {
        let mut engine: Engine = Engine::default();
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
        let (temp_dir, temp_file_path) = mq_test::create_file("test_module.mq", "def func1(): 42;");
        let temp_file_path_clone = temp_file_path.clone();

        defer! {
            if temp_file_path_clone.exists() {
                std::fs::remove_file(&temp_file_path_clone).expect("Failed to delete temp file");
            }
        }

        let mut engine: Engine = Engine::default();
        engine.set_search_paths(vec![temp_dir]);

        let result = engine.load_module("test_module");
        assert!(result.is_ok());
    }

    #[test]
    fn test_error_load_module() {
        let (temp_dir, temp_file_path) = mq_test::create_file("error.mq", "error");
        let temp_file_path_clone = temp_file_path.clone();

        defer! {
            if temp_file_path_clone.exists() {
                std::fs::remove_file(&temp_file_path_clone).expect("Failed to delete temp file");
            }
        }

        let mut engine: Engine = Engine::default();
        engine.set_search_paths(vec![temp_dir]);

        let result = engine.load_module("error");
        assert!(result.is_err());
    }

    #[test]
    fn test_eval() {
        let mut engine: Engine = Engine::default();
        let result = engine.eval("add(1, 1)", vec!["".to_string().into()].into_iter());
        assert!(result.is_ok());
        let values = result.unwrap();
        assert_eq!(values.len(), 1);
    }

    #[cfg(feature = "ast-json")]
    #[test]
    fn test_eval_ast() {
        use crate::{AstExpr, AstLiteral, AstNode};

        let mut engine: Engine = Engine::default();
        engine.load_builtin_module();

        let program = vec![Shared::new(AstNode {
            token_id: crate::arena::ArenaId::new(1),
            expr: Shared::new(AstExpr::Literal(AstLiteral::String("hello".to_string()))),
        })];

        let result = engine.eval_ast(program, crate::null_input().into_iter());
        assert!(result.is_ok());
        let values = result.unwrap();
        assert_eq!(values.len(), 1);
        assert_eq!(values[0], "hello".to_string().into());
    }

    #[cfg(feature = "sync")]
    #[test]
    fn test_engine_thread_usage_with_sync_feature() {
        use std::sync::{Arc, Mutex};

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
        use crate::{SharedCell, null_input};

        let engine: Engine = Engine::default();
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

        let mut engine: Engine = Engine::default();
        engine.load_builtin_module();

        let module_id = ModuleId::new(0);
        let result = engine.get_source_code_for_debug(module_id);

        assert!(result.is_ok());
    }
}
