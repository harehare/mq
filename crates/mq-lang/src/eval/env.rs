use thiserror::Error;

use super::builtin;
use super::error::EvalError;
use super::runtime_value::RuntimeValue;
use crate::{arena::Arena, ast::node::NodeId, AstIdent, Token, ast, ast::node::AstArena}; // Added NodeId, AstArena
use rustc_hash::{FxBuildHasher, FxHashMap};
use std::cell::RefCell;
use std::fmt::{Debug, Display};
use std::rc::{Rc, Weak};

#[derive(Error, Debug, PartialEq)]
pub enum EnvError {
    #[error("Invalid definition for \"{0}\"")]
    InvalidDefinition(String),
}

impl EnvError {
    // This method will be updated when EvalError and the calling context in eval.rs are refactored.
    // For now, its signature refers to the old AstNode.
    pub fn to_eval_error<'ast>( // Added 'ast due to AstArena, though AstNode is old type
        &self,
        node_id: NodeId, // Changed from AstNode to NodeId
        ast_arena: &'ast AstArena<'ast>, // Added AstArena
        token_arena: Rc<RefCell<Arena<Rc<Token>>>>,
    ) -> EvalError { // EvalError itself might need 'ast if it stores RuntimeValue<'ast>
        match self {
            EnvError::InvalidDefinition(def) => {
                let token_id = ast_arena[node_id].token_id; // Get token_id from NodeData
                EvalError::InvalidDefinition(
                    (*token_arena.borrow()[token_id]).clone(),
                    def.to_string(),
                )
            }
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Env<'ast> { // Add 'ast lifetime
    context: FxHashMap<ast::IdentName, RuntimeValue<'ast>>, // RuntimeValue now has 'ast
    parent: Option<Weak<RefCell<Env<'ast>>>>, // Env in Weak also needs 'ast
}

impl<'ast> PartialEq for Env<'ast> { // Add 'ast
    fn eq(&self, other: &Self) -> bool {
        self.context == other.context
            && self.parent.as_ref().map(|p| p.as_ptr()) == other.parent.as_ref().map(|p| p.as_ptr())
    }
}

impl<'ast> Display for Env<'ast> { // Add 'ast
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Debug print for context might be verbose, consider summarizing or selective printing.
        write!(f, "Env {{ context: {:?}, parent: {} }}", self.context, self.parent.is_some())
    }
}

impl<'ast> Env<'ast> { // Add 'ast
    pub fn with_parent(parent: Weak<RefCell<Env<'ast>>>) -> Self { // Env needs 'ast
        Self {
            context: FxHashMap::with_capacity_and_hasher(100, FxBuildHasher),
            parent: Some(parent),
        }
    }

    #[inline(always)]
    pub fn define(&mut self, ident: &AstIdent, runtime_value: RuntimeValue<'ast>) { // RuntimeValue needs 'ast
        self.context.insert(ident.name.clone(), runtime_value);
    }

    #[inline(always)]
    pub fn resolve(&self, ident: &AstIdent) -> Result<RuntimeValue<'ast>, EnvError> { // RuntimeValue needs 'ast
        match self.context.get(&ident.name) {
            Some(o) => Ok(o.clone()),
            None if builtin::BUILTIN_FUNCTIONS.contains_key(&ident.name) => {
                Ok(RuntimeValue::NativeFunction(ident.clone()))
            }
            None => match self.parent.as_ref().and_then(|parent| parent.upgrade()) {
                Some(ref parent_env) => {
                    let env = parent_env.borrow();
                    env.resolve(ident) // Recursive call returns RuntimeValue<'ast>
                }
                None => Err(EnvError::InvalidDefinition(ident.to_string())),
            },
        }
    }
}
#[cfg(test)]
// #[ignore] // Removing ignore to enable tests
mod tests {
    use crate::{AstIdentName, number::Number}; // Added Number for RuntimeValue construction

    use super::*;

    #[test]
    fn test_env_define_and_resolve() {
        // Env::default() will create Env<'static> here.
        let mut env: Env = Env::default(); 
        let ident = AstIdent {
            name: AstIdentName::from("x"),
            token: None,
        };
        // RuntimeValue::Number will be RuntimeValue<'static>::Number
        let value: RuntimeValue = RuntimeValue::Number(Number::from(42.0)); 
        env.define(&ident, value.clone());

        let resolved = env.resolve(&ident).unwrap();
        assert_eq!(resolved, value);
    }

    #[test]
    fn test_env_resolve_from_parent() {
        // parent_env will be Env<'static>
        let parent_env = Rc::new(RefCell::new(Env::default())); 
        // child_env will also be Env<'static> due to parent.
        let mut child_env = Env::with_parent(Rc::downgrade(&parent_env)); 

        let parent_ident = AstIdent {
            name: AstIdentName::from("parent_var"),
            token: None,
        };
        let parent_value: RuntimeValue = RuntimeValue::Number(Number::from(100.0));
        parent_env
            .borrow_mut()
            .define(&parent_ident, parent_value.clone());

        let child_ident = AstIdent {
            name: AstIdentName::from("child_var"),
            token: None,
        };
        let child_value: RuntimeValue = RuntimeValue::Number(Number::from(200.0));
        child_env.define(&child_ident, child_value.clone());

        assert_eq!(child_env.resolve(&child_ident).unwrap(), child_value);
        assert_eq!(child_env.resolve(&parent_ident).unwrap(), parent_value);

        let result = parent_env.borrow().resolve(&child_ident);
        assert!(result.is_err());
    }

    #[test]
    fn test_env_shadow_parent_variable() {
        let parent_env = Rc::new(RefCell::new(Env::default()));
        let mut child_env = Env::with_parent(Rc::downgrade(&parent_env));

        let ident = AstIdent {
            name: AstIdentName::from("x"),
            token: None,
        };

        let parent_value: RuntimeValue = RuntimeValue::Number(Number::from(100.0));
        parent_env.borrow_mut().define(&ident, parent_value.clone()); // Clone for potential re-use if needed

        let child_value: RuntimeValue = RuntimeValue::Number(Number::from(200.0));
        child_env.define(&ident, child_value.clone());

        assert_eq!(child_env.resolve(&ident).unwrap(), child_value);
    }
}
