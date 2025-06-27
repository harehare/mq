use std::{cell::RefCell, path::PathBuf, rc::Rc};

use crate::MqResult;

use crate::{
    ModuleLoader, Token, Value,
    arena::Arena,
    error::{self, InnerError},
    eval::Evaluator,
    optimizer::Optimizer,
    parse,
};

/// Configuration options for the mq engine.
#[derive(Debug, Clone)]
pub struct Options {
    /// Whether to enable code optimization during evaluation.
    /// When enabled, performs constant folding and dead code elimination.
    pub optimize: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self { optimize: true }
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
/// use mq_lang::Engine;
///
/// let mut engine = Engine::default();
/// engine.load_builtin_module();
///
/// let input = mq_lang::parse_text_input("hello").unwrap();
/// let result = engine.eval("add(\" world\")", input.into_iter());
/// assert_eq!(result.unwrap(), vec!["hello world".to_string().into()].into());
/// ```
#[derive(Debug, Clone)]
pub struct Engine {
    pub(crate) evaluator: Evaluator,
    pub(crate) options: Options,
    token_arena: Rc<RefCell<Arena<Rc<Token>>>>,
}

impl Default for Engine {
    fn default() -> Self {
        let token_arena = Rc::new(RefCell::new(Arena::new(10000)));

        Self {
            evaluator: Evaluator::new(ModuleLoader::new(None), Rc::clone(&token_arena)),
            options: Options::default(),
            token_arena,
        }
    }
}

impl Engine {
    /// Enable or disable code optimization.
    ///
    /// When optimization is enabled, the engine performs constant folding
    /// and dead code elimination to improve execution performance.
    pub fn set_optimize(&mut self, optimize: bool) {
        self.options.optimize = optimize;
    }

    /// Configure whether to filter out None values from results.
    ///
    /// When enabled, None values are automatically removed from the output,
    /// resulting in cleaner results for most use cases.
    pub fn set_filter_none(&mut self, filter_none: bool) {
        self.evaluator.options.filter_none = filter_none;
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
    pub fn set_paths(&mut self, paths: Vec<PathBuf>) {
        self.evaluator.module_loader.search_paths = Some(paths);
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
            .load_from_file(module_name, Rc::clone(&self.token_arena))
            .map_err(|e| {
                error::Error::from_error(
                    "",
                    InnerError::Module(e),
                    self.evaluator.module_loader.clone(),
                )
            })?;

        self.evaluator.load_module(module).map_err(|e| {
            Box::new(error::Error::from_error(
                "",
                InnerError::Eval(e),
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
    /// use mq_lang::Engine;
    ///
    /// let mut engine = Engine::default();
    /// engine.load_builtin_module();
    ///
    /// let input = mq_lang::parse_text_input("hello").unwrap();
    /// let result = engine.eval("add(\" world\")", input.into_iter());
    /// assert_eq!(result.unwrap(), vec!["hello world".to_string().into()].into());
    /// ```
    ///
    pub fn eval<I: Iterator<Item = Value>>(&mut self, code: &str, input: I) -> MqResult {
        let program = parse(code, Rc::clone(&self.token_arena))?;
        let program = if self.options.optimize {
            Optimizer::new().optimize(&program)
        } else {
            program
        };
        self.evaluator
            .eval(&program, input.into_iter().map(|v| v.into()))
            .map(|values| {
                values
                    .into_iter()
                    .map(Into::into)
                    .collect::<Vec<_>>()
                    .into()
            })
            .map_err(|e| {
                Box::new(error::Error::from_error(
                    code,
                    e,
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
    use mq_test::defer;

    use super::*;

    #[test]
    fn test_engine_default() {
        let engine = Engine::default();
        assert!(engine.options.optimize);
    }

    #[test]
    fn test_set_optimize() {
        let mut engine = Engine::default();
        engine.set_optimize(false);
        assert!(!engine.options.optimize);
    }

    #[test]
    fn test_set_paths() {
        let mut engine = Engine::default();
        let paths = vec![PathBuf::from("/test/path")];
        engine.set_paths(paths.clone());
        assert_eq!(engine.evaluator.module_loader.search_paths, Some(paths));
    }

    #[test]
    fn test_set_max_call_stack_depth() {
        let mut engine = Engine::default();
        let default_depth = engine.evaluator.options.max_call_stack_depth;
        let new_depth = default_depth + 10;

        engine.set_max_call_stack_depth(new_depth);
        assert_eq!(engine.evaluator.options.max_call_stack_depth, new_depth);
    }

    #[test]
    fn test_set_filter_none() {
        let mut engine = Engine::default();
        let initial_value = engine.evaluator.options.filter_none;

        engine.set_filter_none(!initial_value);
        assert_eq!(engine.evaluator.options.filter_none, !initial_value);
    }
    #[test]
    fn test_version() {
        let version = Engine::version();
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

        let mut engine = Engine::default();
        engine.set_paths(vec![temp_dir]);

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

        let mut engine = Engine::default();
        engine.set_paths(vec![temp_dir]);

        let result = engine.load_module("error");
        assert!(result.is_err());
    }

    #[test]
    fn test_eval() {
        let mut engine = Engine::default();
        let result = engine.eval("add(1, 1)", vec!["".to_string().into()].into_iter());
        assert!(result.is_ok());
        let values = result.unwrap();
        assert_eq!(values.len(), 1);
    }
}
