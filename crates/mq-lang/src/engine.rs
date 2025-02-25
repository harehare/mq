use std::{cell::RefCell, path::PathBuf, rc::Rc};

use itertools::Itertools;

use crate::{
    AstIdentName, Module, ModuleLoader, MqResult, Token, Value,
    arena::Arena,
    error::{self, InnerError},
    eval::Evaluator,
    optimizer::Optimizer,
    parse,
};

#[derive(Debug, Clone)]
pub struct Engine {
    pub(crate) evaluator: Evaluator,
    pub(crate) optimization: bool,
    token_arena: Rc<RefCell<Arena<Rc<Token>>>>,
}

impl Default for Engine {
    fn default() -> Self {
        let token_arena = Rc::new(RefCell::new(Arena::new(100_000)));

        Self {
            evaluator: Evaluator::new(ModuleLoader::new(None), Rc::clone(&token_arena)),
            optimization: true,
            token_arena: Rc::clone(&token_arena),
        }
    }
}

impl Engine {
    pub fn set_optimize(&mut self, optimization: bool) {
        self.optimization = optimization;
    }

    pub fn set_paths(&mut self, paths: Vec<PathBuf>) {
        self.evaluator.module_loader.search_paths = Some(paths);
    }

    pub fn defined_values(&self) -> Vec<(AstIdentName, Box<Value>)> {
        self.evaluator
            .defined_runtime_values()
            .iter()
            .map(|(name, value)| (name.clone(), Box::new(Value::from(*value.clone()))))
            .collect_vec()
    }

    pub fn load_builtin_module(&mut self) -> Result<(), error::Error> {
        self.evaluator.load_builtin_module().map_err(|e| {
            error::Error::from_error(
                "",
                InnerError::Eval(e),
                self.evaluator.module_loader.clone(),
            )
        })
    }

    pub fn load_module(&mut self, module: Module) -> Result<(), Box<error::Error>> {
        self.evaluator.load_module(Some(module)).map_err(|e| {
            Box::new(error::Error::from_error(
                "",
                InnerError::Eval(e),
                self.evaluator.module_loader.clone(),
            ))
        })
    }

    pub fn eval<I: Iterator<Item = Value>>(&mut self, code: &str, input: I) -> MqResult {
        let program = parse(code, Rc::clone(&self.token_arena))?;
        let program = if self.optimization {
            Optimizer::new().optimize(&program)
        } else {
            program
        };
        self.evaluator
            .eval(&program, input.into_iter().map(|v| v.into()))
            .map(|values| values.into_iter().map(Into::into).collect_vec().into())
            .map_err(|e| error::Error::from_error(code, e, self.evaluator.module_loader.clone()))
    }

    pub const fn version() -> &'static str {
        env!("CARGO_PKG_VERSION")
    }
}
