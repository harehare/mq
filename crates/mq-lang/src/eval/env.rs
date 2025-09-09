use thiserror::Error;

use super::builtin;
use super::error::EvalError;
use super::runtime_value::RuntimeValue;
use crate::arena::Arena;
use crate::{AstIdent, AstNode, Token, ast};
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

#[derive(Debug, Clone, Default)]
pub struct Env {
    context: FxHashMap<ast::IdentName, RuntimeValue>,
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
        let values = self
            .context
            .iter()
            .map(|(ident, v)| match v {
                RuntimeValue::Function(params, _, _) => format!("{ident}/{}", params.len()),
                RuntimeValue::NativeFunction(_) => format!("{ident} (native function)"),
                _ => format!("{ident} = {v}"),
            })
            .collect::<Vec<String>>()
            .join("\n");

        write!(f, "{}", values)
    }
}

impl Env {
    pub fn with_parent(parent: Weak<RefCell<Env>>) -> Self {
        Self {
            context: FxHashMap::with_capacity_and_hasher(100, FxBuildHasher),
            parent: Some(parent),
        }
    }

    #[inline(always)]
    pub fn define(&mut self, ident: &AstIdent, runtime_value: RuntimeValue) {
        self.context.insert(ident.name.clone(), runtime_value);
    }

    #[inline(always)]
    pub fn resolve(&self, ident: &AstIdent) -> Result<RuntimeValue, EnvError> {
        match self.context.get(&ident.name) {
            Some(o) => Ok(o.clone()),
            None => match self.parent.as_ref().and_then(|parent| parent.upgrade()) {
                Some(ref parent_env) => {
                    let env = parent_env.borrow();
                    env.resolve(ident)
                }
                None => {
                    if builtin::BUILTIN_FUNCTIONS.contains_key(&ident.name) {
                        Ok(RuntimeValue::NativeFunction(ident.clone()))
                    } else {
                        Err(EnvError::InvalidDefinition(ident.to_string()))
                    }
                }
            },
        }
    }
}
#[cfg(test)]
mod tests {
    use crate::AstIdentName;

    use super::*;

    #[test]
    fn test_env_define_and_resolve() {
        let mut env = Env::default();
        let ident = AstIdent {
            name: AstIdentName::from("x"),
            token: None,
        };
        let value = RuntimeValue::Number(42.0.into());
        env.define(&ident, value.clone());

        let resolved = env.resolve(&ident).unwrap();
        assert_eq!(resolved, value);
    }

    #[test]
    fn test_env_resolve_from_parent() {
        let parent_env = Rc::new(RefCell::new(Env::default()));
        let mut child_env = Env::with_parent(Rc::downgrade(&parent_env));

        let parent_ident = AstIdent {
            name: AstIdentName::from("parent_var"),
            token: None,
        };
        let parent_value = RuntimeValue::Number(100.0.into());
        parent_env
            .borrow_mut()
            .define(&parent_ident, parent_value.clone());

        let child_ident = AstIdent {
            name: AstIdentName::from("child_var"),
            token: None,
        };
        let child_value = RuntimeValue::Number(200.0.into());
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

        let parent_value = RuntimeValue::Number(100.0.into());
        parent_env.borrow_mut().define(&ident, parent_value);

        let child_value = RuntimeValue::Number(200.0.into());
        child_env.define(&ident, child_value.clone());

        assert_eq!(child_env.resolve(&ident).unwrap(), child_value);
    }
}
