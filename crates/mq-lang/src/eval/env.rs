use super::builtin;
use super::runtime_value::RuntimeValue;
use crate::ast::TokenId;
use crate::error::runtime::RuntimeError;
use crate::{Ident, SharedCell, Token, TokenArena, get_token};
use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;
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

/// Scopes with this many or more unique entries are promoted from SmallVec to FxHashMap.
/// Below this threshold, linear search over a stack-allocated array is faster than hashing.
const PROMOTE_THRESHOLD: usize = 6;

/// Per-scope variable storage.
///
/// `Small` is stack-allocated (up to 8 entries) and uses linear search.  It is used
/// for child scopes (function parameters, `let`/`var` bindings) where the number of
/// variables is small.
///
/// `Large` is a heap-allocated hash map.  It is used only for the global scope, which
/// accumulates many entries when `load_builtin_module()` is called (103+ definitions).
///
/// `Env` is always stored inside `Shared<SharedCell<Env>>` (i.e. on the heap), so the
/// large `Small` variant does not cause stack pressure.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
enum EnvContext {
    Small(SmallVec<[(Ident, RuntimeValue); 4]>),
    Large(Box<FxHashMap<Ident, RuntimeValue>>),
}

impl Default for EnvContext {
    fn default() -> Self {
        EnvContext::Large(Box::default())
    }
}

impl PartialEq for EnvContext {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (EnvContext::Small(a), EnvContext::Small(b)) => a == b,
            (EnvContext::Large(a), EnvContext::Large(b)) => a == b,
            _ => false,
        }
    }
}

impl EnvContext {
    #[inline]
    fn new_small() -> Self {
        EnvContext::Small(SmallVec::new())
    }

    #[inline]
    fn get(&self, ident: Ident) -> Option<&RuntimeValue> {
        match self {
            EnvContext::Small(v) => v.iter().rev().find(|(k, _)| *k == ident).map(|(_, v)| v),
            EnvContext::Large(m) => m.get(&ident),
        }
    }

    /// Upsert: update an existing binding if present, otherwise push a new one.
    ///
    /// When the scope grows beyond `PROMOTE_THRESHOLD` unique entries, the `Small`
    /// variant is automatically converted to `Large` (FxHashMap) so that subsequent
    /// lookups remain O(1) even for scopes that accumulate many bindings (e.g. a
    /// `foreach` body with many `let` variables).
    #[inline]
    fn upsert(&mut self, ident: Ident, value: RuntimeValue) {
        match self {
            EnvContext::Small(v) => {
                if let Some(entry) = v.iter_mut().find(|(k, _)| *k == ident) {
                    entry.1 = value;
                    return;
                }
                v.push((ident, value));
                if v.len() >= PROMOTE_THRESHOLD {
                    let map: FxHashMap<Ident, RuntimeValue> = std::mem::take(v).into_iter().collect();
                    *self = EnvContext::Large(Box::new(map));
                }
            }
            EnvContext::Large(m) => {
                m.insert(ident, value);
            }
        }
    }

    #[inline]
    fn contains_key(&self, ident: Ident) -> bool {
        match self {
            EnvContext::Small(v) => v.iter().any(|(k, _)| *k == ident),
            EnvContext::Large(m) => m.contains_key(&ident),
        }
    }

    fn len(&self) -> usize {
        match self {
            EnvContext::Small(v) => v.len(),
            EnvContext::Large(m) => m.len(),
        }
    }

    #[cfg(feature = "debugger")]
    fn iter_entries(&self) -> impl Iterator<Item = (Ident, &RuntimeValue)> + '_ {
        match self {
            EnvContext::Small(v) => Either::A(v.iter().map(|(k, v)| (*k, v))),
            EnvContext::Large(m) => Either::B(m.iter().map(|(k, v)| (*k, v))),
        }
    }
}

/// Helper to unify two different iterator types without boxing.
#[cfg(feature = "debugger")]
enum Either<A, B> {
    A(A),
    B(B),
}

#[cfg(feature = "debugger")]
impl<A, B, T> Iterator for Either<A, B>
where
    A: Iterator<Item = T>,
    B: Iterator<Item = T>,
{
    type Item = T;
    fn next(&mut self) -> Option<T> {
        match self {
            Either::A(a) => a.next(),
            Either::B(b) => b.next(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Env {
    context: EnvContext,
    mutable_vars: Option<FxHashSet<Ident>>,
    parent: Option<Weak<SharedCell<Env>>>,
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
            RuntimeValue::Bytes(b) => Variable {
                name: ident.to_string(),
                value: format!("bytes({})", b.len()),
                type_field: "bytes".to_string(),
            },
            RuntimeValue::None => Variable {
                name: ident.to_string(),
                value: "None".to_string(),
                type_field: "none".to_string(),
            },
            RuntimeValue::Ast(_) => Variable {
                name: ident.to_string(),
                value: "<ast>".to_string(),
                type_field: "ast".to_string(),
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

macro_rules! borrow_env {
    ($cell:expr) => {{
        #[cfg(not(feature = "sync"))]
        {
            $cell.borrow()
        }
        #[cfg(feature = "sync")]
        {
            $cell.read().unwrap()
        }
    }};
}

macro_rules! borrow_env_mut {
    ($cell:expr) => {{
        #[cfg(not(feature = "sync"))]
        {
            $cell.borrow_mut()
        }
        #[cfg(feature = "sync")]
        {
            $cell.write().unwrap()
        }
    }};
}

impl Env {
    /// Creates a child scope.  Uses `Small` (stack-allocated SmallVec) storage because
    /// function/block scopes typically hold only a handful of variables.
    pub fn with_parent(parent: Weak<SharedCell<Env>>) -> Self {
        Self {
            context: EnvContext::new_small(),
            mutable_vars: None,
            parent: Some(parent),
        }
    }

    pub fn len(&self) -> usize {
        self.context.len()
    }

    #[inline(always)]
    pub fn define(&mut self, ident: Ident, runtime_value: RuntimeValue) {
        self.context.upsert(ident, runtime_value);
    }

    #[inline(always)]
    pub fn resolve(&self, ident: Ident) -> Result<RuntimeValue, EnvError> {
        if let Some(o) = self.context.get(ident) {
            return Ok(o.clone());
        }

        let mut current_parent = self.parent.as_ref().and_then(|p| p.upgrade());

        while let Some(parent_cell) = current_parent {
            let parent_env = borrow_env!(parent_cell);

            if let Some(o) = parent_env.context.get(ident) {
                return Ok(o.clone());
            }
            current_parent = parent_env.parent.as_ref().and_then(|p| p.upgrade());
        }

        if ident.resolve_with(builtin::get_builtin_functions_by_str).is_some() {
            Ok(RuntimeValue::NativeFunction(ident))
        } else {
            Err(EnvError::InvalidDefinition(ident.to_string()))
        }
    }

    /// Defines a mutable variable in the current environment
    #[inline(always)]
    pub fn define_mutable(&mut self, ident: Ident, runtime_value: RuntimeValue) {
        self.context.upsert(ident, runtime_value);
        self.mutable_vars.get_or_insert_with(FxHashSet::default).insert(ident);
    }

    /// Assigns a value to an existing mutable variable
    pub fn assign(&mut self, ident: Ident, runtime_value: RuntimeValue) -> Result<(), EnvError> {
        if self.context.contains_key(ident) {
            if self.mutable_vars.as_ref().is_some_and(|s| s.contains(&ident)) {
                self.context.upsert(ident, runtime_value);
                return Ok(());
            } else {
                return Err(EnvError::AssignToImmutable(ident.to_string()));
            }
        }

        let mut current_parent = self.parent.as_ref().and_then(|p| p.upgrade());

        while let Some(parent_cell) = current_parent {
            let has_key;
            let is_mutable;
            let next_parent;
            {
                let parent_env = borrow_env!(parent_cell);
                has_key = parent_env.context.contains_key(ident);
                is_mutable = has_key && parent_env.mutable_vars.as_ref().is_some_and(|s| s.contains(&ident));
                next_parent = parent_env.parent.as_ref().and_then(|p| p.upgrade());
            }

            if has_key {
                if is_mutable {
                    borrow_env_mut!(parent_cell).context.upsert(ident, runtime_value);
                    return Ok(());
                } else {
                    return Err(EnvError::AssignToImmutable(ident.to_string()));
                }
            }
            current_parent = next_parent;
        }

        Err(EnvError::UndefinedVariable(ident.to_string()))
    }

    /// Checks if a variable is mutable
    pub fn is_mutable(&self, ident: Ident) -> bool {
        if self.context.contains_key(ident) {
            return self.mutable_vars.as_ref().is_some_and(|s| s.contains(&ident));
        }

        let mut current_parent = self.parent.as_ref().and_then(|p| p.upgrade());

        while let Some(parent_cell) = current_parent {
            let parent_env = borrow_env!(parent_cell);

            if parent_env.context.contains_key(ident) {
                return parent_env.mutable_vars.as_ref().is_some_and(|s| s.contains(&ident));
            }
            current_parent = parent_env.parent.as_ref().and_then(|p| p.upgrade());
        }

        false
    }

    #[cfg(feature = "debugger")]
    /// Returns a vector of local variables in the current environment.
    pub fn get_local_variables(&self) -> Vec<Variable> {
        match self.parent {
            None => vec![],
            Some(_) => self
                .context
                .iter_entries()
                .map(|(ident, value)| Variable::from(ident, value))
                .collect(),
        }
    }

    #[cfg(feature = "debugger")]
    /// Returns a vector of global variables in the current environment.
    pub fn get_global_variables(&self) -> Vec<Variable> {
        match &self.parent {
            None => self
                .context
                .iter_entries()
                .filter_map(|(ident, value)| {
                    if value.is_function() || value.is_native_function() {
                        None
                    } else {
                        Some(Variable::from(ident, value))
                    }
                })
                .collect(),
            Some(parent_weak) => {
                if let Some(parent_env) = parent_weak.upgrade() {
                    let parent_ref = borrow_env!(parent_env);
                    parent_ref.get_global_variables()
                } else {
                    self.context
                        .iter_entries()
                        .filter_map(|(ident, value)| {
                            if value.is_function() || value.is_native_function() {
                                None
                            } else {
                                Some(Variable::from(ident, value))
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
    use crate::Shared;
    use proptest::prelude::*;
    use rstest::rstest;

    use super::*;

    fn num(n: f64) -> RuntimeValue {
        RuntimeValue::Number(n.into())
    }

    fn child(parent: &Shared<SharedCell<Env>>) -> Env {
        Env::with_parent(Shared::downgrade(parent))
    }

    fn define_n_unique(env: &mut Env, n: usize) {
        for i in 0..n {
            env.define(Ident::new(&format!("v{i}")), num(i as f64));
        }
    }

    fn make_parent() -> Shared<SharedCell<Env>> {
        Shared::new(SharedCell::new(Env::default()))
    }

    #[test]
    fn child_scope_starts_as_small() {
        let p = make_parent();
        assert!(matches!(child(&p).context, EnvContext::Small(_)));
    }

    #[test]
    fn global_scope_starts_as_large() {
        assert!(matches!(Env::default().context, EnvContext::Large(_)));
    }

    #[rstest]
    #[case(1)]
    #[case(PROMOTE_THRESHOLD - 1)]
    #[case(PROMOTE_THRESHOLD)]
    #[case(PROMOTE_THRESHOLD + 3)]
    fn define_n_keys_all_resolve_correctly(#[case] n: usize) {
        let p = make_parent();
        let mut env = child(&p);
        define_n_unique(&mut env, n);
        for i in 0..n {
            assert_eq!(env.resolve(Ident::new(&format!("v{i}"))).unwrap(), num(i as f64));
        }
    }

    #[rstest]
    #[case(1)]
    #[case(PROMOTE_THRESHOLD - 1)]
    #[case(PROMOTE_THRESHOLD + 1)]
    fn rebinding_same_key_keeps_len_1(#[case] rebinds: usize) {
        let p = make_parent();
        let mut env = child(&p);
        let x = Ident::new("x");
        for i in 0..rebinds {
            env.define(x, num(i as f64));
        }
        assert_eq!(env.context.len(), 1);
        assert_eq!(env.resolve(x).unwrap(), num((rebinds - 1) as f64));
    }

    #[test]
    fn stays_small_below_threshold() {
        let p = make_parent();
        let mut env = child(&p);
        define_n_unique(&mut env, PROMOTE_THRESHOLD - 1);
        assert!(matches!(env.context, EnvContext::Small(_)));
    }

    #[rstest]
    #[case(PROMOTE_THRESHOLD)]
    #[case(PROMOTE_THRESHOLD + 1)]
    #[case(PROMOTE_THRESHOLD + 10)]
    fn promotes_at_or_above_threshold(#[case] n: usize) {
        let p = make_parent();
        let mut env = child(&p);
        define_n_unique(&mut env, n);
        assert!(matches!(env.context, EnvContext::Large(_)));
    }

    #[rstest]
    #[case(PROMOTE_THRESHOLD)]
    #[case(PROMOTE_THRESHOLD + 5)]
    fn promotion_preserves_all_values(#[case] n: usize) {
        let p = make_parent();
        let mut env = child(&p);
        define_n_unique(&mut env, n);
        assert!(matches!(env.context, EnvContext::Large(_)));
        for i in 0..n {
            assert_eq!(env.resolve(Ident::new(&format!("v{i}"))).unwrap(), num(i as f64));
        }
    }

    #[test]
    fn upsert_in_promoted_scope_does_not_grow() {
        let p = make_parent();
        let mut env = child(&p);
        define_n_unique(&mut env, PROMOTE_THRESHOLD);
        assert!(matches!(env.context, EnvContext::Large(_)));
        let len_before = env.context.len();
        env.define(Ident::new("v0"), num(999.0)); // already exists
        assert_eq!(env.context.len(), len_before);
        assert_eq!(env.resolve(Ident::new("v0")).unwrap(), num(999.0));
    }

    #[rstest]
    #[case(false)] // child stays Small
    #[case(true)] // child promoted to Large
    fn child_finds_parent_value(#[case] promote_child: bool) {
        let p = make_parent();
        {
            #[cfg(not(feature = "sync"))]
            p.borrow_mut().define(Ident::new("pg"), num(77.0));
            #[cfg(feature = "sync")]
            p.write().unwrap().define(Ident::new("pg"), num(77.0));
        }
        let mut env = child(&p);
        if promote_child {
            define_n_unique(&mut env, PROMOTE_THRESHOLD);
        }
        assert_eq!(env.resolve(Ident::new("pg")).unwrap(), num(77.0));
    }

    #[test]
    fn child_does_not_see_siblings_variable() {
        let p = make_parent();
        let mut sibling = child(&p);
        sibling.define(Ident::new("sib"), num(1.0));

        let env = child(&p);
        assert!(env.resolve(Ident::new("sib")).is_err());
    }

    #[test]
    fn three_level_scope_chain() {
        let gp = make_parent();
        {
            #[cfg(not(feature = "sync"))]
            gp.borrow_mut().define(Ident::new("g"), num(1.0));
            #[cfg(feature = "sync")]
            gp.write().unwrap().define(Ident::new("g"), num(1.0));
        }
        let p = Shared::new(SharedCell::new(child(&gp)));
        {
            #[cfg(not(feature = "sync"))]
            p.borrow_mut().define(Ident::new("p"), num(2.0));
            #[cfg(feature = "sync")]
            p.write().unwrap().define(Ident::new("p"), num(2.0));
        }
        let mut env = child(&p);
        env.define(Ident::new("c"), num(3.0));

        assert_eq!(env.resolve(Ident::new("c")).unwrap(), num(3.0));
        assert_eq!(env.resolve(Ident::new("p")).unwrap(), num(2.0));
        assert_eq!(env.resolve(Ident::new("g")).unwrap(), num(1.0));
    }

    #[test]
    fn child_shadows_parent_variable() {
        let p = make_parent();
        {
            #[cfg(not(feature = "sync"))]
            p.borrow_mut().define(Ident::new("x"), num(1.0));
            #[cfg(feature = "sync")]
            p.write().unwrap().define(Ident::new("x"), num(1.0));
        }
        let mut env = child(&p);
        env.define(Ident::new("x"), num(2.0));
        assert_eq!(env.resolve(Ident::new("x")).unwrap(), num(2.0));
    }

    #[test]
    fn undefined_var_is_error() {
        let p = make_parent();
        assert!(child(&p).resolve(Ident::new("nope")).is_err());
    }

    #[rstest]
    #[case(false)] // still Small
    #[case(true)] // after promotion to Large
    fn mutable_assign_works(#[case] promote: bool) {
        let p = make_parent();
        let mut env = child(&p);
        let x = Ident::new("x");
        env.define_mutable(x, num(10.0));
        if promote {
            define_n_unique(&mut env, PROMOTE_THRESHOLD);
        }
        env.assign(x, num(20.0)).unwrap();
        assert_eq!(env.resolve(x).unwrap(), num(20.0));
    }

    #[rstest]
    #[case(false)]
    #[case(true)]
    fn immutable_assign_is_error(#[case] promote: bool) {
        let p = make_parent();
        let mut env = child(&p);
        let x = Ident::new("x");
        env.define(x, num(1.0));
        if promote {
            define_n_unique(&mut env, PROMOTE_THRESHOLD);
        }
        assert!(env.assign(x, num(2.0)).is_err());
    }

    #[test]
    fn assign_walks_to_parent() {
        let p = make_parent();
        {
            #[cfg(not(feature = "sync"))]
            p.borrow_mut().define_mutable(Ident::new("cnt"), num(0.0));
            #[cfg(feature = "sync")]
            p.write().unwrap().define_mutable(Ident::new("cnt"), num(0.0));
        }
        let mut env = child(&p);
        env.assign(Ident::new("cnt"), num(42.0)).unwrap();

        #[cfg(not(feature = "sync"))]
        let val = p.borrow().resolve(Ident::new("cnt")).unwrap();
        #[cfg(feature = "sync")]
        let val = p.read().unwrap().resolve(Ident::new("cnt")).unwrap();

        assert_eq!(val, num(42.0));
    }

    #[rstest]
    #[case("x", true)]
    #[case("y", false)]
    fn is_mutable_matches_definition(#[case] name: &str, #[case] mutable: bool) {
        let p = make_parent();
        let mut env = child(&p);
        let id = Ident::new(name);
        if mutable {
            env.define_mutable(id, num(0.0));
        } else {
            env.define(id, num(0.0));
        }
        assert_eq!(env.is_mutable(id), mutable);
    }

    #[rstest]
    #[case(10)]
    #[case(100)]
    #[case(1000)]
    fn foreach_rebind_keeps_len_1(#[case] iters: usize) {
        let p = make_parent();
        let mut env = child(&p);
        let x = Ident::new("x");
        for i in 0..iters {
            env.define(x, num(i as f64));
        }
        assert_eq!(env.context.len(), 1);
        assert_eq!(env.resolve(x).unwrap(), num((iters - 1) as f64));
    }

    #[test]
    fn foreach_with_multiple_lets_promotes_and_stays_correct() {
        // Simulate: foreach(i, ...): let a=i | let b=a+1 | ... | a+b+c+d+e
        let p = make_parent();
        let mut env = child(&p);
        let names = ["i", "a", "b", "c", "d", "e"];
        let ids: Vec<Ident> = names.iter().map(|n| Ident::new(n)).collect();

        for iter in 0..10usize {
            // rebind loop var
            env.define(ids[0], num(iter as f64));
            // bind let vars (upsert: first time new, subsequent iterations update)
            for (j, id) in ids[1..].iter().enumerate() {
                env.define(*id, num((iter + j) as f64));
            }
        }

        // After promotion (6 unique vars >= threshold if threshold ≤ 6)
        // All values must reflect the last iteration (iter=9)
        assert_eq!(env.resolve(ids[0]).unwrap(), num(9.0));
        assert_eq!(env.context.len(), names.len());
    }

    proptest! {
        #[test]
        fn prop_rebind_keeps_len_1(iters in 1usize..=200) {
            let p = make_parent();
            let mut env = child(&p);
            let x = Ident::new("x");
            for i in 0..iters {
                env.define(x, num(i as f64));
            }
            prop_assert_eq!(env.context.len(), 1);
            prop_assert_eq!(env.resolve(x).unwrap(), num((iters - 1) as f64));
        }

        #[test]
        fn prop_unique_keys_len_equals_n(n in 1usize..=30) {
            let p = make_parent();
            let mut env = child(&p);
            define_n_unique(&mut env, n);
            prop_assert_eq!(env.context.len(), n);
        }

        #[test]
        fn prop_all_values_accessible_after_n_defines(n in 1usize..=30) {
            let p = make_parent();
            let mut env = child(&p);
            define_n_unique(&mut env, n);
            for i in 0..n {
                let val = env.resolve(Ident::new(&format!("v{i}"))).unwrap();
                prop_assert_eq!(val, num(i as f64));
            }
        }

        #[test]
        fn prop_promotion_happens_iff_n_ge_threshold(n in 0usize..=30) {
            let p = make_parent();
            let mut env = child(&p);
            define_n_unique(&mut env, n);
            let is_large = matches!(env.context, EnvContext::Large(_));
            prop_assert_eq!(is_large, n >= PROMOTE_THRESHOLD);
        }

        #[test]
        fn prop_latest_value_always_returned(updates in 2usize..=50) {
            let p = make_parent();
            let mut env = child(&p);
            let x = Ident::new("x");
            for i in 0..updates {
                env.define(x, num(i as f64));
                // After each update, resolve must return the latest value
                prop_assert_eq!(env.resolve(x).unwrap(), num(i as f64));
            }
        }

        #[test]
        fn prop_parent_value_always_accessible(
            child_vars in 0usize..=20,
            parent_val in 0.0f64..1000.0
        ) {
            let p = make_parent();
            {
                #[cfg(not(feature = "sync"))]
                p.borrow_mut().define(Ident::new("pv"), num(parent_val));
                #[cfg(feature = "sync")]
                p.write().unwrap().define(Ident::new("pv"), num(parent_val));
            }
            let mut env = child(&p);
            define_n_unique(&mut env, child_vars);
            // Regardless of how many child vars exist (including after promotion),
            // the parent value must remain accessible.
            let resolved = env.resolve(Ident::new("pv")).unwrap();
            prop_assert_eq!(resolved, num(parent_val));
        }

        #[test]
        fn prop_mutable_assign_updates_correctly(
            initial in 0.0f64..500.0,
            updated in 500.0f64..1000.0,
            extra_vars in 0usize..=15
        ) {
            let p = make_parent();
            let mut env = child(&p);
            let x = Ident::new("x");
            env.define_mutable(x, num(initial));
            define_n_unique(&mut env, extra_vars);
            env.assign(x, num(updated)).unwrap();
            prop_assert_eq!(env.resolve(x).unwrap(), num(updated));
        }

        #[test]
        fn prop_len_never_exceeds_unique_key_count(
            rebinds in 1usize..=100,
            extra_keys in 0usize..=10
        ) {
            let p = make_parent();
            let mut env = child(&p);
            let x = Ident::new("x");
            for i in 0..rebinds {
                env.define(x, num(i as f64));
            }
            define_n_unique(&mut env, extra_keys);
            // len must equal 1 (for x) + extra_keys unique vars
            prop_assert_eq!(env.context.len(), 1 + extra_keys);
        }

        #[test]
        fn prop_contains_key_consistent_with_resolve(n in 1usize..=30, query in 0usize..=35) {
            let p = make_parent();
            let mut env = child(&p);
            define_n_unique(&mut env, n);
            let key = Ident::new(&format!("v{query}"));
            let contains = env.context.contains_key(key);
            let resolved = env.resolve(key);
            // contains_key iff resolve succeeds
            prop_assert_eq!(contains, resolved.is_ok());
        }

        #[test]
        fn prop_scope_isolation(n in 1usize..=20) {
            let p = make_parent();
            let mut env = child(&p);
            define_n_unique(&mut env, n);
            // Parent must not see child's variables
            for i in 0..n {
                #[cfg(not(feature = "sync"))]
                let result = p.borrow().resolve(Ident::new(&format!("v{i}")));
                #[cfg(feature = "sync")]
                let result = p.read().unwrap().resolve(Ident::new(&format!("v{i}")));
                prop_assert!(result.is_err());
            }
        }

        #[test]
        fn prop_assign_undefined_is_error(extra in 0usize..=15) {
            let p = make_parent();
            let mut env = child(&p);
            define_n_unique(&mut env, extra);
            let result = env.assign(Ident::new("does_not_exist"), num(1.0));
            prop_assert!(result.is_err());
        }

        #[test]
        fn prop_global_scope_rebind_keeps_len_1(rebinds in 1usize..=200) {
            let mut env = Env::default();
            let x = Ident::new("x");
            for i in 0..rebinds {
                env.define(x, num(i as f64));
            }
            prop_assert_eq!(env.context.len(), 1);
            prop_assert_eq!(env.resolve(x).unwrap(), num((rebinds - 1) as f64));
        }

        /// `is_mutable` predicts whether `assign` will succeed or return `AssignToImmutable`.
        #[test]
        fn prop_is_mutable_consistent_with_assign(
            extra in 0usize..=15,
            is_mutable in any::<bool>()
        ) {
            let p = make_parent();
            let mut env = child(&p);
            let x = Ident::new("target");
            if is_mutable {
                env.define_mutable(x, num(0.0));
            } else {
                env.define(x, num(0.0));
            }
            define_n_unique(&mut env, extra);
            let result = env.assign(x, num(1.0));
            if is_mutable {
                prop_assert!(result.is_ok(), "mutable var must accept assign");
            } else {
                prop_assert_eq!(result.unwrap_err(), EnvError::AssignToImmutable("target".to_string()));
            }
        }

        /// Defining x in a child scope must not change the parent's binding of x.
        #[test]
        fn prop_shadow_preserves_parent_value(
            parent_val in 0.0f64..500.0,
            child_val in 500.0f64..1000.0,
            extra in 0usize..=15
        ) {
            let p = make_parent();
            {
                #[cfg(not(feature = "sync"))]
                p.borrow_mut().define(Ident::new("x"), num(parent_val));
                #[cfg(feature = "sync")]
                p.write().unwrap().define(Ident::new("x"), num(parent_val));
            }
            let mut env = child(&p);
            env.define(Ident::new("x"), num(child_val));
            define_n_unique(&mut env, extra);
            // Parent's x must be unchanged
            #[cfg(not(feature = "sync"))]
            let pv = p.borrow().resolve(Ident::new("x")).unwrap();
            #[cfg(feature = "sync")]
            let pv = p.read().unwrap().resolve(Ident::new("x")).unwrap();
            prop_assert_eq!(pv, num(parent_val));
            // Child's x must be the shadowed value
            prop_assert_eq!(env.resolve(Ident::new("x")).unwrap(), num(child_val));
        }

        /// After promotion, rebinding an existing key must not increase len.
        #[test]
        fn prop_promotion_then_rebind_does_not_grow(
            rebinds in 1usize..=50
        ) {
            let p = make_parent();
            let mut env = child(&p);
            // Trigger promotion
            define_n_unique(&mut env, PROMOTE_THRESHOLD);
            prop_assert!(matches!(env.context, EnvContext::Large(_)));
            let len_at_promotion = env.context.len();
            // Rebind all existing keys multiple times
            for _ in 0..rebinds {
                for i in 0..PROMOTE_THRESHOLD {
                    env.define(Ident::new(&format!("v{i}")), num(i as f64 + 1.0));
                }
            }
            prop_assert_eq!(env.context.len(), len_at_promotion);
        }

        /// After assign to a parent mutable var, the child can resolve the new value.
        #[test]
        fn prop_assign_to_parent_visible_from_child(
            initial in 0.0f64..500.0,
            updated in 500.0f64..1000.0,
            child_extra in 0usize..=10
        ) {
            let p = make_parent();
            {
                #[cfg(not(feature = "sync"))]
                p.borrow_mut().define_mutable(Ident::new("shared"), num(initial));
                #[cfg(feature = "sync")]
                p.write().unwrap().define_mutable(Ident::new("shared"), num(initial));
            }
            let mut env = child(&p);
            define_n_unique(&mut env, child_extra);
            // Assign via child walks up and updates parent
            env.assign(Ident::new("shared"), num(updated)).unwrap();
            // Child resolve must see the updated value
            prop_assert_eq!(env.resolve(Ident::new("shared")).unwrap(), num(updated));
        }

        /// Mixed sequence of rebinds and new-key inserts: len always equals the count
        /// of distinct keys that have been defined.
        #[test]
        fn prop_mixed_operations_len_equals_unique_keys(
            unique_keys in 1usize..=20,
            rebind_rounds in 0usize..=5
        ) {
            let p = make_parent();
            let mut env = child(&p);
            // First pass: all unique keys
            for i in 0..unique_keys {
                env.define(Ident::new(&format!("k{i}")), num(i as f64));
            }
            prop_assert_eq!(env.context.len(), unique_keys);
            // Additional rebind rounds must not change len
            for round in 0..rebind_rounds {
                for i in 0..unique_keys {
                    env.define(Ident::new(&format!("k{i}")), num((round * 100 + i) as f64));
                }
                prop_assert_eq!(env.context.len(), unique_keys,
                    "len must stay at {} after rebind round {}", unique_keys, round);
            }
        }

        /// If a key is undefined in the entire chain, `assign` returns `UndefinedVariable`,
        /// never `AssignToImmutable`.
        #[test]
        fn prop_undefined_assign_error_kind(extra in 0usize..=10) {
            let p = make_parent();
            let mut env = child(&p);
            define_n_unique(&mut env, extra);
            let err = env.assign(Ident::new("ghost"), num(1.0)).unwrap_err();
            prop_assert_eq!(err, EnvError::UndefinedVariable("ghost".to_string()));
        }

        /// `contains_key` returns true for every key inserted, regardless of how many
        /// rebinds or whether promotion occurred.
        #[test]
        fn prop_contains_key_after_rebind_and_promotion(
            unique in 1usize..=25,
            rebinds in 0usize..=10
        ) {
            let p = make_parent();
            let mut env = child(&p);
            for i in 0..unique {
                env.define(Ident::new(&format!("k{i}")), num(i as f64));
            }
            for r in 0..rebinds {
                for i in 0..unique {
                    env.define(Ident::new(&format!("k{i}")), num((r * unique + i) as f64));
                }
            }
            for i in 0..unique {
                let key = Ident::new(&format!("k{i}"));
                prop_assert!(env.context.contains_key(key));
            }
        }
    }

    #[rstest]
    #[case(false, false)]
    #[case(true, false)]
    #[case(false, true)]
    #[case(true, true)]
    fn is_mutable_correct_after_optional_promotion(#[case] is_mutable: bool, #[case] promote: bool) {
        let p = make_parent();
        let mut env = child(&p);
        let x = Ident::new("x");
        if is_mutable {
            env.define_mutable(x, num(0.0));
        } else {
            env.define(x, num(0.0));
        }
        if promote {
            define_n_unique(&mut env, PROMOTE_THRESHOLD);
        }
        assert_eq!(env.is_mutable(x), is_mutable);
    }

    #[rstest]
    #[case(EnvError::UndefinedVariable("no_such".to_string()))]
    fn assign_undefined_var_returns_error(#[case] expected: EnvError) {
        let p = make_parent();
        let mut env = child(&p);
        let err = env.assign(Ident::new("no_such"), num(1.0)).unwrap_err();
        assert_eq!(err, expected);
    }

    #[test]
    fn cross_variant_partial_eq_is_false() {
        let p = make_parent();
        let mut small_env = child(&p);
        small_env.define(Ident::new("x"), num(1.0));
        assert!(matches!(small_env.context, EnvContext::Small(_)));

        let mut large_env = Env::default(); // starts as Large
        large_env.define(Ident::new("x"), num(1.0));
        assert!(matches!(large_env.context, EnvContext::Large(_)));

        assert_ne!(small_env.context, large_env.context);
    }

    #[test]
    fn global_scope_rebind_keeps_len_1() {
        let mut env = Env::default();
        let x = Ident::new("x");
        for i in 0..100 {
            env.define(x, num(i as f64));
        }
        assert_eq!(env.context.len(), 1);
        assert_eq!(env.resolve(x).unwrap(), num(99.0));
    }

    #[test]
    fn assign_unknown_in_chain_is_undefined_error() {
        let gp = make_parent();
        let p = Shared::new(SharedCell::new(child(&gp)));
        let mut env = child(&p);
        let err = env.assign(Ident::new("ghost"), num(1.0)).unwrap_err();
        assert_eq!(err, EnvError::UndefinedVariable("ghost".to_string()));
    }

    #[test]
    fn test_env_define_and_resolve() {
        let mut env = Env::default();
        let ident = Ident::new("x");
        let value = RuntimeValue::Number(42.0.into());
        env.define(ident, value.clone());
        assert_eq!(env.resolve(ident).unwrap(), value);
    }

    #[test]
    fn test_env_resolve_from_parent() {
        let parent_env = make_parent();
        let mut child_env = child(&parent_env);

        let parent_ident = Ident::new("parent_var");
        let parent_value = num(100.0);

        #[cfg(not(feature = "sync"))]
        parent_env.borrow_mut().define(parent_ident, parent_value.clone());
        #[cfg(feature = "sync")]
        parent_env.write().unwrap().define(parent_ident, parent_value.clone());

        child_env.define(Ident::new("child_var"), num(200.0));

        assert_eq!(child_env.resolve(Ident::new("child_var")).unwrap(), num(200.0));
        assert_eq!(child_env.resolve(parent_ident).unwrap(), parent_value);
        #[cfg(not(feature = "sync"))]
        assert!(parent_env.borrow().resolve(Ident::new("child_var")).is_err());
        #[cfg(feature = "sync")]
        assert!(parent_env.read().unwrap().resolve(Ident::new("child_var")).is_err());
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
    fn test_variable_from_and_display(#[case] vars: Vec<(&str, RuntimeValue)>, #[case] expected: Vec<Variable>) {
        for (i, (name, value)) in vars.iter().enumerate() {
            let ident = Ident::new(name);
            let var = Variable::from(ident, value);
            assert_eq!(var, expected[i]);
            let display = format!("{}", var);
            assert!(display.contains(&var.name));
        }
    }

    #[rstest]
    #[case("mutable_var", Some(true), None, None, true)]
    #[case("immutable_var", Some(false), None, None, false)]
    #[case("non_existent", None, None, None, false)]
    #[case("var", None, Some(true), None, true)]
    #[case("var", None, Some(false), None, false)]
    #[case("var", Some(false), Some(true), None, false)]
    #[case("var", None, None, Some(true), true)]
    fn test_is_mutable(
        #[case] var_name: &str,
        #[case] define_in_current: Option<bool>,
        #[case] define_in_parent: Option<bool>,
        #[case] define_in_grandparent: Option<bool>,
        #[case] expected_mutable: bool,
    ) {
        let grandparent_env = define_in_grandparent.map(|_| make_parent());

        let parent_env = if define_in_parent.is_some() || grandparent_env.is_some() {
            if let Some(ref gp) = grandparent_env {
                Some(Shared::new(SharedCell::new(child(gp))))
            } else {
                Some(make_parent())
            }
        } else {
            None
        };

        let mut env = if let Some(ref parent) = parent_env {
            child(parent)
        } else {
            Env::default()
        };

        let var = Ident::new(var_name);

        if let Some(is_mutable) = define_in_grandparent
            && let Some(ref gp) = grandparent_env
        {
            #[cfg(not(feature = "sync"))]
            if is_mutable {
                gp.borrow_mut().define_mutable(var, num(1.0));
            } else {
                gp.borrow_mut().define(var, num(1.0));
            }
            #[cfg(feature = "sync")]
            if is_mutable {
                gp.write().unwrap().define_mutable(var, num(1.0));
            } else {
                gp.write().unwrap().define(var, num(1.0));
            }
        }

        if let Some(is_mutable) = define_in_parent
            && let Some(ref parent) = parent_env
        {
            #[cfg(not(feature = "sync"))]
            if is_mutable {
                parent.borrow_mut().define_mutable(var, num(100.0));
            } else {
                parent.borrow_mut().define(var, num(100.0));
            }
            #[cfg(feature = "sync")]
            if is_mutable {
                parent.write().unwrap().define_mutable(var, num(100.0));
            } else {
                parent.write().unwrap().define(var, num(100.0));
            }
        }

        if let Some(is_mutable) = define_in_current {
            if is_mutable {
                env.define_mutable(var, num(200.0));
            } else {
                env.define(var, num(200.0));
            }
        }

        assert_eq!(env.is_mutable(var), expected_mutable);
    }
}
