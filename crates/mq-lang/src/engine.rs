#[cfg(feature = "debugger")]
use std::borrow::Cow;
use std::path::PathBuf;

#[cfg(feature = "ast-json")]
use crate::Program;
#[cfg(feature = "debugger")]
use crate::eval::env::Env;
#[cfg(feature = "debugger")]
use crate::module::ModuleId;
#[cfg(feature = "debugger")]
use crate::{Debugger, DebuggerHandler};
use crate::{LocalFsModuleResolver, ModuleResolver, MqResult, RuntimeValue, Shared, SharedCell, token_alloc};

use crate::{
    ModuleLoader, Token,
    arena::Arena,
    error::{self},
    eval::Evaluator,
    parse,
};

/// Configuration options for the mq engine.
#[derive(Debug, Clone)]
pub struct Options {
    /// Whether to use the closure-based compiler instead of the tree-walking interpreter.
    /// When enabled, AST is compiled to closures for faster execution (10-15% speedup expected).
    /// Default: true (compiler is enabled by default)
    #[allow(dead_code)]
    pub use_compiler: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self { use_compiler: true }
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
    pub(crate) compiler: crate::compiler::Compiler,
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
            compiler: crate::compiler::Compiler::new(Shared::clone(&token_arena)),
            options: Options::default(),
            token_arena,
        }
    }

    /// Enable or disable the closure-based compiler.
    ///
    /// When enabled, AST is compiled to closures for faster execution.
    /// When disabled, the tree-walking interpreter is used (default).
    ///
    /// # Examples
    ///
    /// ```
    /// use mq_lang::DefaultEngine;
    ///
    /// let mut engine = DefaultEngine::default();
    /// engine.set_use_compiler(true);
    /// ```
    pub fn set_use_compiler(&mut self, use_compiler: bool) {
        self.options.use_compiler = use_compiler;
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

        #[cfg(feature = "debugger")]
        self.evaluator.module_loader.set_source_code(code.to_string());

        let result = if self.options.use_compiler {
            self.eval_compiled(&program, input)
        } else {
            self.evaluator
                .eval(&program, input.into_iter())
                .map(|values| values.into())
        };

        result.map_err(|e| Box::new(error::Error::from_error(code, e, self.evaluator.module_loader.clone())))
    }

    /// Evaluates a program using the closure-based compiler.
    ///
    /// This is a private method used when `use_compiler` is enabled.
    fn eval_compiled<I: Iterator<Item = RuntimeValue>>(
        &mut self,
        program: &crate::Program,
        input: I,
    ) -> Result<crate::eval::runtime_value::RuntimeValues, crate::error::InnerError> {
        use crate::ast::node::Expr;
        use crate::error::InnerError;
        use crate::eval::define;
        use crate::eval::runtime_value::RuntimeValue;

        // Pre-process: handle Def/Import/Include (same as tree-walker)
        let filtered_program: Vec<_> = program
            .iter()
            .filter_map(|node| {
                match &*node.expr {
                    Expr::Def(ident, params, program) => {
                        // Register function definition
                        define(
                            &self.evaluator.env,
                            ident.name,
                            RuntimeValue::Function(params.clone(), program.clone(), Shared::clone(&self.evaluator.env)),
                        );
                        None // Remove from program
                    }
                    Expr::Include(module_id) => {
                        // Execute include and remove from program
                        let _ = self
                            .evaluator
                            .eval_include(module_id.clone(), &Shared::clone(&self.evaluator.env));
                        None
                    }
                    Expr::Import(module_path) => {
                        // Execute import and remove from program
                        let _ = self
                            .evaluator
                            .eval_import(module_path.clone(), &Shared::clone(&self.evaluator.env));
                        None
                    }
                    _ => Some(Shared::clone(node)), // Keep in program
                }
            })
            .collect();

        // Compile the filtered program
        let (nodes_index, compiled_program) = self
            .compiler
            .compile_program(&filtered_program)
            .map_err(InnerError::Runtime)?;

        // Create call stack for recursion tracking
        let mut call_stack = Vec::with_capacity(32);

        // Get environment from evaluator
        let env = Shared::clone(&self.evaluator.env);

        // Check if program contains `nodes` expression
        if let Some(index) = nodes_index {
            // Split compiled program at nodes position
            let (before_nodes, after_nodes_with_nodes) = compiled_program.split_at(index);
            let after_nodes = &after_nodes_with_nodes[1..]; // Skip the nodes expression itself

            // Execute before_nodes part and collect results into array
            let values: Vec<RuntimeValue> = input
                .map(|runtime_value| {
                    match &runtime_value {
                        RuntimeValue::Markdown(node, _) => {
                            // For Markdown nodes, use the tree-walker's eval_markdown_node logic
                            self.eval_compiled_markdown_node(before_nodes, &mut call_stack, &env, node)
                        }
                        _ => {
                            // For non-Markdown values, execute normally
                            let mut value = runtime_value;
                            for expr in before_nodes {
                                value = expr(value, &mut call_stack, Shared::clone(&env))?;
                            }
                            Ok(value)
                        }
                    }
                })
                .collect::<Result<_, _>>()
                .map_err(InnerError::Runtime)?;

            // If there's a program after nodes, execute it with the values array
            if after_nodes.is_empty() {
                Ok(values.into())
            } else {
                // Execute after_nodes part with the values array
                let mut value = RuntimeValue::Array(values);
                for expr in after_nodes {
                    value = expr(value, &mut call_stack, Shared::clone(&env)).map_err(InnerError::Runtime)?;
                }

                // Convert result to RuntimeValues
                Ok(match value {
                    RuntimeValue::Array(values) => values.into(),
                    other => vec![other].into(),
                })
            }
        } else {
            // No nodes expression, execute normally
            let results: Vec<RuntimeValue> = input
                .map(|runtime_value| {
                    match &runtime_value {
                        RuntimeValue::Markdown(node, _) => {
                            // For Markdown nodes, use the tree-walker's eval_markdown_node logic
                            self.eval_compiled_markdown_node(&compiled_program, &mut call_stack, &env, node)
                        }
                        _ => {
                            // For non-Markdown values, execute normally
                            let mut value = runtime_value;
                            for expr in &compiled_program {
                                value = expr(value, &mut call_stack, Shared::clone(&env))?;
                            }
                            Ok(value)
                        }
                    }
                })
                .collect::<Result<_, _>>()
                .map_err(InnerError::Runtime)?;

            Ok(results.into())
        }
    }

    /// Helper method to evaluate Markdown nodes with the compiled program.
    ///
    /// This replicates the tree-walker's `eval_markdown_node` logic for the compiler.
    fn eval_compiled_markdown_node(
        &mut self,
        compiled_program: &[crate::compiler::compiled::CompiledExpr],
        call_stack: &mut Vec<()>,
        env: &Shared<SharedCell<crate::eval::env::Env>>,
        node: &mq_markdown::Node,
    ) -> Result<RuntimeValue, crate::error::runtime::RuntimeError> {
        node.map_values(&mut |child_node| {
            let mut value = RuntimeValue::Markdown(child_node.clone(), None);
            for expr in compiled_program {
                value = expr(value, call_stack, Shared::clone(env))?;
            }

            Ok(match value {
                RuntimeValue::None => child_node.to_fragment(),
                RuntimeValue::Function(_, _, _) | RuntimeValue::NativeFunction(_) | RuntimeValue::Module(_) => {
                    mq_markdown::Node::Empty
                }
                RuntimeValue::Array(_)
                | RuntimeValue::Dict(_)
                | RuntimeValue::Boolean(_)
                | RuntimeValue::Number(_)
                | RuntimeValue::String(_) => value.to_string().into(),
                RuntimeValue::Symbol(i) => i.as_str().into(),
                RuntimeValue::Markdown(node, _) => node,
            })
        })
        .map(|node| RuntimeValue::Markdown(node, None))
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
    /// use mq_lang::{DefaultEngine, AstNode, AstExpr, AstLiteral, Program, RuntimeValue, Shared};
    ///
    /// let mut engine = DefaultEngine::default();
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
    pub fn eval_ast<I: Iterator<Item = RuntimeValue>>(&mut self, program: Program, input: I) -> MqResult {
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
            compiler: crate::compiler::Compiler::new(Shared::clone(&token_arena)),
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

    #[cfg(feature = "ast-json")]
    #[test]
    fn test_eval_ast() {
        use crate::{AstExpr, AstLiteral, AstNode, Shared};

        let mut engine = DefaultEngine::default();
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
