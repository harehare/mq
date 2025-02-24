use thiserror::Error;

use super::builtin;
use super::error::EvalError;
use super::runtime_value::RuntimeValue;
use crate::arena::Arena;
use crate::{AstIdent, AstNode, Token, ast};
use rustc_hash::FxHashMap;
use std::cell::RefCell;
use std::fmt::{Debug, Display};
use std::rc::{Rc, Weak};

#[derive(Error, Debug, PartialEq)]
pub enum EnvError {
    #[error("Invalid definition for \"{0}\"")]
    InvalidDefinition(String),
}

impl EnvError {
    pub fn to_eval_error(
        &self,
        node: AstNode,
        token_arena: Rc<RefCell<Arena<Rc<Token>>>>,
    ) -> EvalError {
        match self {
            EnvError::InvalidDefinition(def) => EvalError::InvalidDefinition(
                (*token_arena.borrow()[node.token_id]).clone(),
                def.to_string(),
            ),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Env {
    context: FxHashMap<ast::IdentName, Box<RuntimeValue>>,
    parent: Option<Weak<RefCell<Env>>>,
}

impl PartialEq for Env {
    fn eq(&self, other: &Self) -> bool {
        self.context == other.context
            && self.parent.as_ref().map(|p| p.as_ptr()) == other.parent.as_ref().map(|p| p.as_ptr())
    }
}

impl Display for Env {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Env {{ context: {:?} }}", self.context)
    }
}

impl Env {
    pub fn new(parent: Option<Weak<RefCell<Env>>>) -> Self {
        Self {
            context: FxHashMap::default(),
            parent,
        }
    }

    #[inline(always)]
    pub fn define(&mut self, ident: &AstIdent, runtime_value: RuntimeValue) {
        self.context
            .insert(ident.name.clone(), Box::new(runtime_value));
    }

    #[inline(always)]
    pub fn resolve(&self, ident: &AstIdent) -> Result<Box<RuntimeValue>, EnvError> {
        match self.context.get(&ident.name) {
            Some(o) => Ok(o.clone()),
            None if builtin::BUILTIN_FUNCTIONS.contains_key(&ident.name) => {
                Ok(Box::new(RuntimeValue::NativeFunction(ident.clone())))
            }
            None => match self.parent.as_ref().and_then(|parent| parent.upgrade()) {
                Some(ref parent_env) => {
                    let env = parent_env.borrow();
                    env.resolve(ident)
                }
                None => Err(EnvError::InvalidDefinition(ident.to_string())),
            },
        }
    }

    #[inline(always)]
    pub fn defined_runtime_values(&self) -> Vec<(ast::IdentName, Box<RuntimeValue>)> {
        self.context
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }
}
