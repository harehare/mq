use std::{cell::RefCell, path::PathBuf, rc::Rc};

use crate::MqResult;

use crate::{
    AstIdentName, ModuleLoader, Token, Value,
    arena::Arena,
    error::{self, InnerError},
    eval::Evaluator,
    optimizer::Optimizer,
    parse,
};

#[derive(Debug, Clone)]
pub struct Options {
    pub optimize: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self { optimize: true }
    }
}

#[derive(Debug, Clone)]
pub struct Engine {
    pub(crate) evaluator: Evaluator,
    pub(crate) options: Options,
    token_arena: Rc<RefCell<Arena<Rc<Token>>>>,
}

impl Default for Engine {
    fn default() -> Self {
        let token_arena = Rc::new(RefCell::new(Arena::new(100_000)));

        Self {
            evaluator: Evaluator::new(ModuleLoader::new(None), Rc::clone(&token_arena)),
            options: Options::default(),
            token_arena: Rc::clone(&token_arena),
        }
    }
}

impl Engine {
    pub fn set_optimize(&mut self, optimize: bool) {
        self.options.optimize = optimize;
    }

    pub fn set_filter_none(&mut self, filter_none: bool) {
        self.evaluator.options.filter_none = filter_none;
    }

    pub fn set_paths(&mut self, paths: Vec<PathBuf>) {
        self.evaluator.module_loader.search_paths = Some(paths);
    }

    pub fn defined_values(&self) -> Vec<(AstIdentName, Box<Value>)> {
        self.evaluator
            .defined_runtime_values()
            .iter()
            .map(|(name, value)| (name.clone(), Box::new(Value::from(*value.clone()))))
            .collect::<Vec<_>>()
    }

    pub fn define_string_value(&self, name: &str, value: &str) {
        self.evaluator.define_string_value(name, value);
    }

    #[allow(clippy::result_large_err)]
    pub fn load_builtin_module(&mut self) -> Result<(), error::Error> {
        self.evaluator.load_builtin_module().map_err(|e| {
            error::Error::from_error(
                "",
                InnerError::Eval(e),
                self.evaluator.module_loader.clone(),
            )
        })
    }

    #[allow(clippy::result_large_err)]
    pub fn load_module(&mut self, module_name: &str) -> Result<(), error::Error> {
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
            error::Error::from_error(
                "",
                InnerError::Eval(e),
                self.evaluator.module_loader.clone(),
            )
        })
    }

    #[allow(clippy::result_large_err)]
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
            .map_err(|e| error::Error::from_error(code, e, self.evaluator.module_loader.clone()))
    }

    pub const fn version() -> &'static str {
        env!("CARGO_PKG_VERSION")
    }
}
#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::io::Write;

    use super::*;

    #[test]
    fn test_engine_default() {
        let engine = Engine::default();
        assert_eq!(engine.options.optimize, true);
    }

    #[test]
    fn test_set_optimize() {
        let mut engine = Engine::default();
        engine.set_optimize(false);
        assert_eq!(engine.options.optimize, false);
    }

    #[test]
    fn test_set_paths() {
        let mut engine = Engine::default();
        let paths = vec![PathBuf::from("/test/path")];
        engine.set_paths(paths.clone());
        assert_eq!(engine.evaluator.module_loader.search_paths, Some(paths));
    }

    #[test]
    fn test_define_string_value() {
        let engine = Engine::default();
        engine.define_string_value("test_var", "test_value");
        let values = engine.defined_values();
        assert!(values.iter().any(|(name, _)| name.as_str() == "test_var"));
    }

    #[test]
    fn test_version() {
        let version = Engine::version();
        assert!(!version.is_empty());
    }

    #[test]
    fn test_load_builtin_module() {
        let mut engine = Engine::default();
        let result = engine.load_builtin_module();
        assert!(result.is_ok());
    }

    #[test]
    fn test_load_module() {
        let temp_dir = std::env::temp_dir();
        let module_path = temp_dir.join("test_module.mq");
        let mut file = File::create(&module_path).unwrap();
        write!(file, "def func1(): 42;").unwrap();

        let mut engine = Engine::default();
        engine.set_paths(vec![temp_dir]);

        let result = engine.load_module("test_module");
        assert!(result.is_ok());

        let values = engine.defined_values();
        assert!(values.iter().any(|(name, _)| name.as_str() == "func1"));
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
