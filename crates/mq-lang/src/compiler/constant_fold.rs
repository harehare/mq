//! Compile-time constant folding optimization.
//!
//! This module provides constant folding capabilities for the compiler,
//! allowing constant expressions to be evaluated at compile time rather
//! than runtime.

use crate::Ident;
use crate::ast::node::Literal;
use crate::eval::runtime_value::RuntimeValue;
use rustc_hash::FxHashMap;

/// Constant folder for compile-time optimization.
///
/// Tracks known constant values and attempts to fold constant expressions
/// during compilation.
#[derive(Debug, Clone, Default)]
pub struct ConstantFolder {
    /// Map of known constant values (from `let` bindings, etc.)
    #[allow(dead_code)]
    constants: FxHashMap<Ident, RuntimeValue>,
}

impl ConstantFolder {
    /// Creates a new constant folder.
    pub fn new() -> Self {
        Self {
            constants: FxHashMap::default(),
        }
    }

    /// Registers a constant value.
    ///
    /// # Arguments
    ///
    /// * `ident` - The identifier name
    /// * `value` - The constant value
    #[allow(dead_code)]
    pub fn add_constant(&mut self, ident: Ident, value: RuntimeValue) {
        self.constants.insert(ident, value);
    }

    /// Attempts to fold a literal expression into a constant value.
    ///
    /// # Arguments
    ///
    /// * `literal` - The literal expression to fold
    ///
    /// # Returns
    ///
    /// The folded constant value.
    pub fn fold_literal(&self, literal: &Literal) -> RuntimeValue {
        match literal {
            Literal::None => RuntimeValue::None,
            Literal::Bool(b) => RuntimeValue::Boolean(*b),
            Literal::String(s) => RuntimeValue::String(s.clone()),
            Literal::Symbol(i) => RuntimeValue::Symbol(*i),
            Literal::Number(n) => RuntimeValue::Number(*n),
        }
    }

    /// Attempts to resolve a constant identifier.
    ///
    /// # Arguments
    ///
    /// * `ident` - The identifier to resolve
    ///
    /// # Returns
    ///
    /// `Some(value)` if the identifier is a known constant, `None` otherwise.
    #[allow(dead_code)]
    pub fn resolve_constant(&self, ident: &Ident) -> Option<&RuntimeValue> {
        self.constants.get(ident)
    }
}
