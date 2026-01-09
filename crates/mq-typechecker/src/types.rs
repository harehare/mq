//! Type representations for the mq type system.

use slotmap::SlotMap;
use std::fmt;

slotmap::new_key_type! {
    /// Unique identifier for type variables
    pub struct TypeVarId;
}

/// Represents a type in the mq type system
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Type {
    /// Integer type
    Int,
    /// Floating point type
    Float,
    /// Number type (unified numeric type)
    Number,
    /// String type
    String,
    /// Boolean type
    Bool,
    /// Symbol type
    Symbol,
    /// None/null type
    None,
    /// Markdown document type
    Markdown,
    /// Array type with element type
    Array(Box<Type>),
    /// Dictionary type with key and value types
    Dict(Box<Type>, Box<Type>),
    /// Function type: arguments -> return type
    Function(Vec<Type>, Box<Type>),
    /// Type variable for inference
    Var(TypeVarId),
}

impl Type {
    /// Creates a new function type
    pub fn function(params: Vec<Type>, ret: Type) -> Self {
        Type::Function(params, Box::new(ret))
    }

    /// Creates a new array type
    pub fn array(elem: Type) -> Self {
        Type::Array(Box::new(elem))
    }

    /// Creates a new dict type
    pub fn dict(key: Type, value: Type) -> Self {
        Type::Dict(Box::new(key), Box::new(value))
    }

    /// Checks if this is a type variable
    pub fn is_var(&self) -> bool {
        matches!(self, Type::Var(_))
    }

    /// Gets the type variable ID if this is a type variable
    pub fn as_var(&self) -> Option<TypeVarId> {
        match self {
            Type::Var(id) => Some(*id),
            _ => None,
        }
    }

    /// Substitutes type variables according to the given substitution
    pub fn apply_subst(&self, subst: &Substitution) -> Type {
        match self {
            Type::Var(id) => subst.lookup(*id).map_or_else(|| self.clone(), |t| t.apply_subst(subst)),
            Type::Array(elem) => Type::Array(Box::new(elem.apply_subst(subst))),
            Type::Dict(key, value) => Type::Dict(Box::new(key.apply_subst(subst)), Box::new(value.apply_subst(subst))),
            Type::Function(params, ret) => {
                let new_params = params.iter().map(|p| p.apply_subst(subst)).collect();
                Type::Function(new_params, Box::new(ret.apply_subst(subst)))
            }
            _ => self.clone(),
        }
    }

    /// Gets all free type variables in this type
    pub fn free_vars(&self) -> Vec<TypeVarId> {
        match self {
            Type::Var(id) => vec![*id],
            Type::Array(elem) => elem.free_vars(),
            Type::Dict(key, value) => {
                let mut vars = key.free_vars();
                vars.extend(value.free_vars());
                vars
            }
            Type::Function(params, ret) => {
                let mut vars: Vec<TypeVarId> = params.iter().flat_map(|p| p.free_vars()).collect();
                vars.extend(ret.free_vars());
                vars
            }
            _ => Vec::new(),
        }
    }

    /// Checks if this type can match with another type (for overload resolution).
    ///
    /// This is a weaker check than unification - it returns true if the types
    /// could potentially be unified, but doesn't require them to be identical.
    /// Type variables always match.
    pub fn can_match(&self, other: &Type) -> bool {
        match (self, other) {
            // Type variables always match
            (Type::Var(_), _) | (_, Type::Var(_)) => true,

            // Concrete types must match exactly
            (Type::Int, Type::Int)
            | (Type::Float, Type::Float)
            | (Type::Number, Type::Number)
            | (Type::String, Type::String)
            | (Type::Bool, Type::Bool)
            | (Type::Symbol, Type::Symbol)
            | (Type::None, Type::None)
            | (Type::Markdown, Type::Markdown) => true,

            // Arrays match if their element types can match
            (Type::Array(elem1), Type::Array(elem2)) => elem1.can_match(elem2),

            // Dicts match if both key and value types can match
            (Type::Dict(k1, v1), Type::Dict(k2, v2)) => k1.can_match(k2) && v1.can_match(v2),

            // Functions match if they have the same arity and all parameter/return types can match
            (Type::Function(params1, ret1), Type::Function(params2, ret2)) => {
                params1.len() == params2.len()
                    && params1.iter().zip(params2.iter()).all(|(p1, p2)| p1.can_match(p2))
                    && ret1.can_match(ret2)
            }

            // Everything else doesn't match
            _ => false,
        }
    }

    /// Computes a match score for overload resolution.
    /// Higher scores indicate better matches. Returns None if types cannot match.
    ///
    /// Scoring:
    /// - Exact match: 100
    /// - Type variable: 10
    /// - Structural match (array/dict/function): sum of component scores
    pub fn match_score(&self, other: &Type) -> Option<u32> {
        if !self.can_match(other) {
            return None;
        }

        match (self, other) {
            // Exact matches get highest score
            (Type::Int, Type::Int)
            | (Type::Float, Type::Float)
            | (Type::Number, Type::Number)
            | (Type::String, Type::String)
            | (Type::Bool, Type::Bool)
            | (Type::Symbol, Type::Symbol)
            | (Type::None, Type::None)
            | (Type::Markdown, Type::Markdown) => Some(100),

            // Type variables get low score (prefer concrete types)
            (Type::Var(_), _) | (_, Type::Var(_)) => Some(10),

            // Arrays: sum element scores
            (Type::Array(elem1), Type::Array(elem2)) => elem1.match_score(elem2).map(|s| s / 2),

            // Dicts: sum key and value scores
            (Type::Dict(k1, v1), Type::Dict(k2, v2)) => {
                let key_score = k1.match_score(k2)?;
                let val_score = v1.match_score(v2)?;
                Some((key_score + val_score) / 2)
            }

            // Functions: sum all parameter scores and return score
            (Type::Function(params1, ret1), Type::Function(params2, ret2)) => {
                let param_score: u32 = params1
                    .iter()
                    .zip(params2.iter())
                    .map(|(p1, p2)| p1.match_score(p2).unwrap_or(0))
                    .sum();
                let ret_score = ret1.match_score(ret2)?;
                Some(param_score + ret_score)
            }

            _ => None,
        }
    }
}

impl Type {
    /// Formats the type as a string, resolving type variables to their readable names.
    /// This is used for better error messages.
    pub fn display_resolved(&self) -> String {
        match self {
            Type::Int => "int".to_string(),
            Type::Float => "float".to_string(),
            Type::Number => "number".to_string(),
            Type::String => "string".to_string(),
            Type::Bool => "bool".to_string(),
            Type::Symbol => "symbol".to_string(),
            Type::None => "none".to_string(),
            Type::Markdown => "markdown".to_string(),
            Type::Array(elem) => format!("[{}]", elem.display_resolved()),
            Type::Dict(key, value) => format!("{{{}: {}}}", key.display_resolved(), value.display_resolved()),
            Type::Function(params, ret) => {
                let params_str = params
                    .iter()
                    .map(|p| p.display_resolved())
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("({}) -> {}", params_str, ret.display_resolved())
            }
            Type::Var(id) => {
                // Convert TypeVarId to a readable name like 'a, 'b, 'c, etc.
                type_var_name(*id)
            }
        }
    }
}

/// Converts a TypeVarId to a readable name.
/// For simplicity, we just use a short representation of the debug format.
fn type_var_name(id: TypeVarId) -> String {
    let id_str = format!("{:?}", id);
    // Extract a simple representation from the debug format
    // TypeVarId debug format is like "TypeVarId(1v0)" or similar
    if let Some(inner) = id_str.strip_prefix("TypeVarId(").and_then(|s| s.strip_suffix(")")) {
        format!("'{}", inner)
    } else {
        // Fallback: just use the whole debug representation
        format!("'{}", id_str)
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::Int => write!(f, "int"),
            Type::Float => write!(f, "float"),
            Type::Number => write!(f, "number"),
            Type::String => write!(f, "string"),
            Type::Bool => write!(f, "bool"),
            Type::Symbol => write!(f, "symbol"),
            Type::None => write!(f, "none"),
            Type::Markdown => write!(f, "markdown"),
            Type::Array(elem) => write!(f, "[{}]", elem),
            Type::Dict(key, value) => write!(f, "{{{}: {}}}", key, value),
            Type::Function(params, ret) => {
                write!(f, "(")?;
                for (i, param) in params.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", param)?;
                }
                write!(f, ") -> {}", ret)
            }
            Type::Var(id) => write!(f, "{}", type_var_name(*id)),
        }
    }
}

/// Type scheme for polymorphic types (generalized types)
///
/// A type scheme represents a polymorphic type by quantifying over type variables.
/// For example: forall a b. (a -> b) -> [a] -> [b]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeScheme {
    /// Quantified type variables
    pub quantified: Vec<TypeVarId>,
    /// The actual type
    pub ty: Type,
}

impl TypeScheme {
    /// Creates a monomorphic type scheme (no quantified variables)
    pub fn mono(ty: Type) -> Self {
        Self {
            quantified: Vec::new(),
            ty,
        }
    }

    /// Creates a polymorphic type scheme
    pub fn poly(quantified: Vec<TypeVarId>, ty: Type) -> Self {
        Self { quantified, ty }
    }

    /// Instantiates this type scheme with fresh type variables
    pub fn instantiate(&self, ctx: &mut TypeVarContext) -> Type {
        if self.quantified.is_empty() {
            return self.ty.clone();
        }

        // Create fresh type variables for each quantified variable
        let mut subst = Substitution::empty();
        for var_id in &self.quantified {
            let fresh = ctx.fresh();
            subst.insert(*var_id, Type::Var(fresh));
        }

        self.ty.apply_subst(&subst)
    }

    /// Generalizes a type into a type scheme
    pub fn generalize(ty: Type, env_vars: &[TypeVarId]) -> Self {
        let ty_vars = ty.free_vars();
        let quantified: Vec<TypeVarId> = ty_vars.into_iter().filter(|v| !env_vars.contains(v)).collect();
        Self::poly(quantified, ty)
    }
}

impl fmt::Display for TypeScheme {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.quantified.is_empty() {
            write!(f, "{}", self.ty)
        } else {
            write!(f, "forall ")?;
            for (i, var) in self.quantified.iter().enumerate() {
                if i > 0 {
                    write!(f, " ")?;
                }
                write!(f, "{}", type_var_name(*var))?;
            }
            write!(f, ". {}", self.ty)
        }
    }
}

/// Type variable context for generating fresh type variables
pub struct TypeVarContext {
    vars: SlotMap<TypeVarId, Option<Type>>,
}

impl TypeVarContext {
    /// Creates a new type variable context
    pub fn new() -> Self {
        Self {
            vars: SlotMap::with_key(),
        }
    }

    /// Generates a fresh type variable
    pub fn fresh(&mut self) -> TypeVarId {
        self.vars.insert(None)
    }

    /// Gets the resolved type for a type variable
    pub fn get(&self, var: TypeVarId) -> Option<&Type> {
        self.vars.get(var).and_then(|opt| opt.as_ref())
    }

    /// Sets the resolved type for a type variable
    pub fn set(&mut self, var: TypeVarId, ty: Type) {
        if let Some(slot) = self.vars.get_mut(var) {
            *slot = Some(ty);
        }
    }

    /// Checks if a type variable is resolved
    pub fn is_resolved(&self, var: TypeVarId) -> bool {
        self.vars.get(var).and_then(|opt| opt.as_ref()).is_some()
    }
}

impl Default for TypeVarContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Type substitution mapping type variables to types
#[derive(Debug, Clone, Default)]
pub struct Substitution {
    map: std::collections::HashMap<TypeVarId, Type>,
}

impl Substitution {
    /// Creates an empty substitution
    pub fn empty() -> Self {
        Self {
            map: std::collections::HashMap::new(),
        }
    }

    /// Inserts a substitution
    pub fn insert(&mut self, var: TypeVarId, ty: Type) {
        self.map.insert(var, ty);
    }

    /// Looks up a type variable in the substitution
    pub fn lookup(&self, var: TypeVarId) -> Option<&Type> {
        self.map.get(&var)
    }

    /// Composes two substitutions
    pub fn compose(&self, other: &Substitution) -> Substitution {
        let mut result = Substitution::empty();

        // Apply other to all types in self
        for (var, ty) in &self.map {
            result.insert(*var, ty.apply_subst(other));
        }

        // Add mappings from other that aren't in self
        for (var, ty) in &other.map {
            if !self.map.contains_key(var) {
                result.insert(*var, ty.clone());
            }
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_display() {
        assert_eq!(Type::Number.to_string(), "number");
        assert_eq!(Type::String.to_string(), "string");
        assert_eq!(Type::array(Type::Number).to_string(), "[number]");
        assert_eq!(
            Type::function(vec![Type::Number, Type::String], Type::Bool).to_string(),
            "(number, string) -> bool"
        );
    }

    #[test]
    fn test_type_var_context() {
        let mut ctx = TypeVarContext::new();
        let var1 = ctx.fresh();
        let var2 = ctx.fresh();
        assert_ne!(var1, var2);
    }

    #[test]
    fn test_substitution() {
        let mut ctx = TypeVarContext::new();
        let var = ctx.fresh();
        let ty = Type::Var(var);

        let mut subst = Substitution::empty();
        subst.insert(var, Type::Number);

        let result = ty.apply_subst(&subst);
        assert_eq!(result, Type::Number);
    }

    #[test]
    fn test_type_scheme_instantiate() {
        let mut ctx = TypeVarContext::new();
        let var = ctx.fresh();

        let scheme = TypeScheme::poly(vec![var], Type::Var(var));
        let inst1 = scheme.instantiate(&mut ctx);
        let inst2 = scheme.instantiate(&mut ctx);

        // Each instantiation should create fresh variables
        assert_ne!(inst1, inst2);
    }

    #[test]
    fn test_can_match_concrete_types() {
        assert!(Type::Number.can_match(&Type::Number));
        assert!(Type::String.can_match(&Type::String));
        assert!(!Type::Number.can_match(&Type::String));
    }

    #[test]
    fn test_can_match_type_variables() {
        let mut ctx = TypeVarContext::new();
        let var = ctx.fresh();

        // Type variables can match anything
        assert!(Type::Var(var).can_match(&Type::Number));
        assert!(Type::Number.can_match(&Type::Var(var)));
        assert!(Type::Var(var).can_match(&Type::String));
    }

    #[test]
    fn test_can_match_arrays() {
        let arr_num = Type::array(Type::Number);
        let arr_str = Type::array(Type::String);

        assert!(arr_num.can_match(&arr_num));
        assert!(!arr_num.can_match(&arr_str));
    }

    #[test]
    fn test_can_match_functions() {
        let func1 = Type::function(vec![Type::Number], Type::String);
        let func2 = Type::function(vec![Type::Number], Type::String);
        let func3 = Type::function(vec![Type::String], Type::String);

        assert!(func1.can_match(&func2));
        assert!(!func1.can_match(&func3));
    }

    #[test]
    fn test_match_score() {
        // Exact matches get highest score
        assert_eq!(Type::Number.match_score(&Type::Number), Some(100));
        assert_eq!(Type::String.match_score(&Type::String), Some(100));

        // Type variables get lower score
        let mut ctx = TypeVarContext::new();
        let var = ctx.fresh();
        assert_eq!(Type::Var(var).match_score(&Type::Number), Some(10));

        // Incompatible types return None
        assert_eq!(Type::Number.match_score(&Type::String), None);
    }
}
