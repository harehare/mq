use super::builtin;
use super::runtime_value::RuntimeValue;
use crate::ast::TokenId;
use crate::error::runtime::RuntimeError;
use crate::{Ident, SharedCell, Token, TokenArena, get_token};
use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet};
use std::error::Error;
use std::fmt::{self, Debug};

#[cfg(not(feature = "sync"))]
type Weak<T> = std::rc::Weak<T>;

#[cfg(feature = "sync")]
type Weak<T> = std::sync::Weak<T>;

#[derive(Debug, PartialEq)]
pub enum EnvError {
    InvalidDefinition(String),
    AssignToImmutable(String),
    UndefinedVariable(String),
}

impl Error for EnvError {}

impl fmt::Display for EnvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl EnvError {
    #[cold]
    pub fn to_runtime_error(&self, token_id: TokenId, token_arena: TokenArena) -> RuntimeError {
        match self {
            EnvError::InvalidDefinition(def) => {
                RuntimeError::InvalidDefinition((*get_token(token_arena, token_id)).clone(), def.to_string())
            }
            EnvError::AssignToImmutable(var) => {
                RuntimeError::AssignToImmutable((*get_token(token_arena, token_id)).clone(), var.to_string())
            }
            EnvError::UndefinedVariable(var) => {
                RuntimeError::UndefinedVariable((*get_token(token_arena, token_id)).clone(), var.to_string())
            }
        }
    }

    #[cold]
    pub fn to_runtime_error_with_token(&self, token: Token) -> RuntimeError {
        match self {
            EnvError::InvalidDefinition(def) => RuntimeError::InvalidDefinition(token, def.to_string()),
            EnvError::AssignToImmutable(var) => RuntimeError::AssignToImmutable(token, var.to_string()),
            EnvError::UndefinedVariable(var) => RuntimeError::UndefinedVariable(token, var.to_string()),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Env {
    context: FxHashMap<Ident, RuntimeValue>,
    mutable_vars: FxHashSet<Ident>,
    parent: Option<Weak<SharedCell<Env>>>,
    /// Local variable storage (Perl padlist-style optimization)
    /// Indexed access is faster than HashMap lookup
    pad: Vec<RuntimeValue>,
}

impl PartialEq for Env {
    fn eq(&self, other: &Self) -> bool {
        self.context == other.context
            && self.mutable_vars == other.mutable_vars
            && self.parent.as_ref().map(|p| p.as_ptr()) == other.parent.as_ref().map(|p| p.as_ptr())
    }
}

#[cfg(feature = "debugger")]
#[derive(Debug, Clone, PartialEq)]
pub struct Variable {
    pub name: String,
    pub value: String,
    pub type_field: String,
}

#[cfg(feature = "debugger")]
impl Variable {
    fn from(ident: Ident, value: &RuntimeValue) -> Self {
        match value {
            RuntimeValue::Array(_) => Variable {
                name: ident.to_string(),
                value: value.to_string(),
                type_field: "array".to_string(),
            },
            RuntimeValue::Boolean(_) => Variable {
                name: ident.to_string(),
                value: value.to_string(),
                type_field: "bool".to_string(),
            },
            RuntimeValue::Dict(_) => Variable {
                name: ident.to_string(),
                value: value.to_string(),
                type_field: "dict".to_string(),
            },
            RuntimeValue::String(_) => Variable {
                name: ident.to_string(),
                value: value.to_string(),
                type_field: "string".to_string(),
            },
            RuntimeValue::Symbol(_) => Variable {
                name: ident.to_string(),
                value: value.to_string(),
                type_field: "symbol".to_string(),
            },
            RuntimeValue::Number(_) => Variable {
                name: ident.to_string(),
                value: value.to_string(),
                type_field: "number".to_string(),
            },
            RuntimeValue::Markdown(_, _) => Variable {
                name: ident.to_string(),
                value: value.to_string(),
                type_field: "markdown".to_string(),
            },
            RuntimeValue::Function(params, _, _) => Variable {
                name: ident.to_string(),
                value: format!("function/{}", params.len()),
                type_field: "function".to_string(),
            },
            RuntimeValue::OpTreeFunction { params, .. } => Variable {
                name: ident.to_string(),
                value: format!("optree_function/{}", params.len()),
                type_field: "optree_function".to_string(),
            },
            RuntimeValue::NativeFunction(_) => Variable {
                name: ident.to_string(),
                value: "native function".to_string(),
                type_field: "native_function".to_string(),
            },
            RuntimeValue::Module(m) => Variable {
                name: m.name().to_string(),
                value: format!("module/{}", m.len()),
                type_field: "module".to_string(),
            },
            RuntimeValue::None => Variable {
                name: ident.to_string(),
                value: "None".to_string(),
                type_field: "none".to_string(),
            },
        }
    }
}

#[cfg(feature = "debugger")]
impl std::fmt::Display for Variable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} = {}, type: {}", self.name, self.value, self.type_field)
    }
}

impl Env {
    pub fn with_parent(parent: Weak<SharedCell<Env>>) -> Self {
        Self {
            context: FxHashMap::with_capacity_and_hasher(100, FxBuildHasher),
            mutable_vars: FxHashSet::default(),
            parent: Some(parent),
            pad: Vec::new(),
        }
    }

    /// Creates a new environment with a pad of specified capacity
    pub fn with_pad_capacity(capacity: usize) -> Self {
        Self {
            context: FxHashMap::with_capacity_and_hasher(100, FxBuildHasher),
            mutable_vars: FxHashSet::default(),
            parent: None,
            pad: Vec::with_capacity(capacity),
        }
    }

    /// Creates a new environment with parent and pad capacity
    pub fn with_parent_and_pad(parent: Weak<SharedCell<Env>>, pad_capacity: usize) -> Self {
        Self {
            context: FxHashMap::with_capacity_and_hasher(100, FxBuildHasher),
            mutable_vars: FxHashSet::default(),
            parent: Some(parent),
            pad: Vec::with_capacity(pad_capacity),
        }
    }

    /// Sets a local variable at given index in the pad
    #[inline(always)]
    pub fn set_local(&mut self, index: u32, value: RuntimeValue) {
        let index = index as usize;
        if index >= self.pad.len() {
            self.pad.resize(index + 1, RuntimeValue::None);
        }
        self.pad[index] = value;
    }

    /// Gets a local variable by index from the pad
    #[inline(always)]
    pub fn get_local(&self, index: u32) -> Option<RuntimeValue> {
        self.pad.get(index as usize).cloned()
    }

    /// Initializes the pad with given capacity
    #[inline]
    pub fn init_pad(&mut self, capacity: usize) {
        self.pad = vec![RuntimeValue::None; capacity];
    }

    pub fn len(&self) -> usize {
        self.context.len()
    }

    #[inline(always)]
    pub fn define(&mut self, ident: Ident, runtime_value: RuntimeValue) {
        self.context.insert(ident, runtime_value);
    }

    #[inline(always)]
    pub fn resolve(&self, ident: Ident) -> Result<RuntimeValue, EnvError> {
        match self.context.get(&ident) {
            Some(o) => Ok(o.clone()),
            None => match self.parent.as_ref().and_then(|parent| parent.upgrade()) {
                Some(ref parent_env) => {
                    #[cfg(not(feature = "sync"))]
                    let env = parent_env.borrow();
                    #[cfg(feature = "sync")]
                    let env = parent_env.read().unwrap();

                    env.resolve(ident)
                }
                None => {
                    // Use optimized string-based builtin lookup
                    if ident.resolve_with(builtin::get_builtin_functions_by_str).is_some() {
                        Ok(RuntimeValue::NativeFunction(ident))
                    } else {
                        Err(EnvError::InvalidDefinition(ident.to_string()))
                    }
                }
            },
        }
    }

    /// Defines a mutable variable in the current environment
    #[inline(always)]
    pub fn define_mutable(&mut self, ident: Ident, runtime_value: RuntimeValue) {
        self.context.insert(ident, runtime_value);
        self.mutable_vars.insert(ident);
    }

    /// Assigns a value to an existing mutable variable
    pub fn assign(&mut self, ident: Ident, runtime_value: RuntimeValue) -> Result<(), EnvError> {
        // Check if variable exists in current scope
        if self.context.contains_key(&ident) {
            if self.mutable_vars.contains(&ident) {
                self.context.insert(ident, runtime_value);
                return Ok(());
            } else {
                return Err(EnvError::AssignToImmutable(ident.to_string()));
            }
        }

        // Try to assign in parent scope
        match self.parent.as_ref().and_then(|parent| parent.upgrade()) {
            Some(ref parent_env) => {
                #[cfg(not(feature = "sync"))]
                parent_env.borrow_mut().assign(ident, runtime_value)?;
                #[cfg(feature = "sync")]
                parent_env.write().unwrap().assign(ident, runtime_value)?;
                Ok(())
            }
            None => Err(EnvError::UndefinedVariable(ident.to_string())),
        }
    }

    /// Checks if a variable is mutable
    pub fn is_mutable(&self, ident: Ident) -> bool {
        if self.mutable_vars.contains(&ident) {
            return true;
        }

        // Check parent scope
        self.parent
            .as_ref()
            .and_then(|parent| parent.upgrade())
            .map(|parent_env| {
                #[cfg(not(feature = "sync"))]
                let result = parent_env.borrow().is_mutable(ident);
                #[cfg(feature = "sync")]
                let result = parent_env.read().unwrap().is_mutable(ident);
                result
            })
            .unwrap_or(false)
    }

    #[cfg(feature = "debugger")]
    /// Returns a vector of local variables in the current environment.
    pub fn get_local_variables(&self) -> Vec<Variable> {
        match self.parent {
            None => vec![],
            Some(_) => self
                .context
                .iter()
                .map(|(ident, value)| Variable::from(*ident, value))
                .collect(),
        }
    }

    #[cfg(feature = "debugger")]
    /// Returns a vector of global variables in the current environment.
    pub fn get_global_variables(&self) -> Vec<Variable> {
        match &self.parent {
            None => self
                .context
                .iter()
                .filter_map(|(ident, value)| {
                    if value.is_function() || value.is_native_function() {
                        None
                    } else {
                        Some(Variable::from(*ident, value))
                    }
                })
                .collect(),
            Some(parent_weak) => {
                if let Some(parent_env) = parent_weak.upgrade() {
                    #[cfg(not(feature = "sync"))]
                    let parent_ref = parent_env.borrow();
                    #[cfg(feature = "sync")]
                    let parent_ref = parent_env.read().unwrap();

                    parent_ref.get_global_variables()
                } else {
                    // If parent is dropped, treat as root
                    self.context
                        .iter()
                        .filter_map(|(ident, value)| {
                            if value.is_function() || value.is_native_function() {
                                None
                            } else {
                                Some(Variable::from(*ident, value))
                            }
                        })
                        .collect()
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "debugger")]
    use std::collections::BTreeMap;

    use crate::Shared;

    use super::*;

    #[cfg(feature = "debugger")]
    use rstest::rstest;

    #[test]
    fn test_env_define_and_resolve() {
        let mut env = Env::default();
        let ident = Ident::new("x");
        let value = RuntimeValue::Number(42.0.into());
        env.define(ident, value.clone());

        let resolved = env.resolve(ident).unwrap();
        assert_eq!(resolved, value);
    }

    #[test]
    fn test_env_resolve_from_parent() {
        let parent_env = Shared::new(SharedCell::new(Env::default()));
        let mut child_env = Env::with_parent(Shared::downgrade(&parent_env));

        let parent_ident = Ident::new("parent_var");
        let parent_value = RuntimeValue::Number(100.0.into());

        #[cfg(not(feature = "sync"))]
        parent_env.borrow_mut().define(parent_ident, parent_value.clone());

        #[cfg(feature = "sync")]
        parent_env.write().unwrap().define(parent_ident, parent_value.clone());

        let child_ident = Ident::new("child_var");
        let child_value = RuntimeValue::Number(200.0.into());
        child_env.define(child_ident, child_value.clone());

        assert_eq!(child_env.resolve(child_ident).unwrap(), child_value);
        assert_eq!(child_env.resolve(parent_ident).unwrap(), parent_value);

        #[cfg(not(feature = "sync"))]
        let result = parent_env.borrow().resolve(child_ident);
        #[cfg(feature = "sync")]
        let result = parent_env.read().unwrap().resolve(child_ident);

        assert!(result.is_err());
    }

    #[test]
    fn test_env_shadow_parent_variable() {
        let parent_env = Shared::new(SharedCell::new(Env::default()));
        let mut child_env = Env::with_parent(Shared::downgrade(&parent_env));

        let ident = Ident::new("x");
        let parent_value = RuntimeValue::Number(100.0.into());

        #[cfg(not(feature = "sync"))]
        parent_env.borrow_mut().define(ident, parent_value);
        #[cfg(feature = "sync")]
        parent_env.write().unwrap().define(ident, parent_value);

        let child_value = RuntimeValue::Number(200.0.into());
        child_env.define(ident, child_value.clone());

        assert_eq!(child_env.resolve(ident).unwrap(), child_value);
    }

    #[cfg(feature = "debugger")]
    #[rstest]
    #[case(
        vec![("a", RuntimeValue::Number(1.0.into())), ("b", RuntimeValue::Boolean(true))],
        vec![
            Variable { name: "a".to_string(), value: "1".to_string(), type_field: "number".to_string() },
            Variable { name: "b".to_string(), value: "true".to_string(), type_field: "bool".to_string() }
        ]
    )]
    #[case(
        vec![("x", RuntimeValue::String("hello".into())), ("y", RuntimeValue::None)],
        vec![
            Variable { name: "x".to_string(), value: "hello".to_string(), type_field: "string".to_string() },
            Variable { name: "y".to_string(), value: "None".to_string(), type_field: "none".to_string() }
        ]
    )]
    #[case(
        vec![("x", RuntimeValue::Boolean(true)), ("y", RuntimeValue::None)],
        vec![
            Variable { name: "x".to_string(), value: "true".to_string(), type_field: "bool".to_string() },
            Variable { name: "y".to_string(), value: "None".to_string(), type_field: "none".to_string() }
        ]
    )]
    #[case(
        vec![
            ("arr", RuntimeValue::Array(vec![RuntimeValue::Number(1.0.into()), RuntimeValue::Number(2.0.into())])),
            ("dict", {
                let mut map = BTreeMap::new();

                map.insert("k1".into(), RuntimeValue::String("v1".into()));
                map.insert("k2".into(), RuntimeValue::Number(3.0.into()));

                RuntimeValue::Dict(map)
            })
        ],
        vec![
            Variable { name: "arr".to_string(), value: "[1, 2]".to_string(), type_field: "array".to_string() },
            Variable { name: "dict".to_string(), value: "{\"k1\": \"v1\", \"k2\": 3}".to_string(), type_field: "dict".to_string() }
        ]
    )]
    fn test_variable_from_and_display(#[case] vars: Vec<(&str, RuntimeValue)>, #[case] expected: Vec<Variable>) {
        for (i, (name, value)) in vars.iter().enumerate() {
            let ident = Ident::new(name);
            let var = Variable::from(ident, value);
            assert_eq!(var, expected[i]);

            let display = format!("{}", var);
            assert!(display.contains(&var.name));
            assert!(display.contains(&var.value));
            assert!(display.contains(&var.type_field));
        }
    }

    #[cfg(feature = "debugger")]
    #[rstest]
    fn test_get_local_variables() {
        let mut env = Env::default();
        env.define(Ident::new("foo"), RuntimeValue::Number(10.0.into()));
        env.define(Ident::new("bar"), RuntimeValue::Boolean(false));
        // No parent: should return empty
        assert_eq!(env.get_local_variables().len(), 0);

        // With parent: should return local variables
        let parent_env = Shared::new(SharedCell::new(Env::default()));
        let mut child_env = Env::with_parent(Shared::downgrade(&parent_env));
        child_env.define(Ident::new("baz"), RuntimeValue::String("abc".into()));
        let locals = child_env.get_local_variables();
        assert_eq!(locals.len(), 1);
        assert_eq!(locals[0].name, "baz");
        assert_eq!(locals[0].type_field, "string");
    }

    #[cfg(feature = "debugger")]
    #[rstest]
    fn test_get_global_variables() {
        use smallvec::smallvec;

        let mut env = Env::default();
        env.define(Ident::new("foo"), RuntimeValue::Number(1.0.into()));
        env.define(Ident::new("bar"), RuntimeValue::Boolean(true));
        env.define(
            Ident::new("func"),
            RuntimeValue::Function(smallvec![], vec![], Shared::new(SharedCell::new(Env::default()))),
        );
        env.define(Ident::new("native"), RuntimeValue::NativeFunction(Ident::new("native")));
        // Only non-function, non-native should be returned
        let globals = env.get_global_variables();
        assert!(globals.iter().any(|v| v.name == "foo" && v.type_field == "number"));
        assert!(globals.iter().any(|v| v.name == "bar" && v.type_field == "bool"));
        assert!(!globals.iter().any(|v| v.name == "func"));
        assert!(!globals.iter().any(|v| v.name == "native"));

        // With parent: should return parent's globals
        let parent_env = Shared::new(SharedCell::new(Env::default()));
        #[cfg(not(feature = "sync"))]
        {
            parent_env
                .borrow_mut()
                .define(Ident::new("p"), RuntimeValue::Number(99.0.into()));
        }
        #[cfg(feature = "sync")]
        {
            parent_env
                .write()
                .unwrap()
                .define(Ident::new("p"), RuntimeValue::Number(99.0.into()));
        }
        let child_env = Env::with_parent(Shared::downgrade(&parent_env));
        let globals = child_env.get_global_variables();

        assert!(globals.iter().any(|v| v.name == "p" && v.type_field == "number"));
    }
}
