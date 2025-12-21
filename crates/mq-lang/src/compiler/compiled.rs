//! Compiled expression types.
//!
//! This module defines the `CompiledExpr` type, which represents an AST expression
//! that has been compiled into a closure for faster execution.

use super::call_stack::CallStack;
use crate::error::runtime::RuntimeError;
use crate::eval::env::Env;
use crate::eval::runtime_value::RuntimeValue;
use crate::{Shared, SharedCell};

/// A compiled expression represented as a dynamically-dispatched closure.
///
/// The closure takes three parameters:
/// - `RuntimeValue`: The input value (for pipeline-style execution)
/// - `&mut CallStack`: The call stack for recursion tracking
/// - `Shared<SharedCell<Env>>`: The environment for variable resolution
///
/// Returns a `Result<RuntimeValue, RuntimeError>` containing the result or an error.
///
/// ## Example
///
/// ```rust,ignore
/// // A simple literal expression
/// let compiled: CompiledExpr = Box::new(|_input, _stack, _env| {
///     Ok(RuntimeValue::Number(Number::from(42)))
/// });
///
/// let result = compiled(RuntimeValue::None, &mut vec![], env)?;
/// assert_eq!(result, RuntimeValue::Number(Number::from(42)));
/// ```
pub type CompiledExpr = Box<
    dyn Fn(
        RuntimeValue,            // Input value (pipeline style)
        &mut CallStack,          // Call stack for recursion tracking
        Shared<SharedCell<Env>>, // Environment for variable resolution
    ) -> Result<RuntimeValue, RuntimeError>,
>;

/// A compiled program is a sequence of compiled expressions.
///
/// Each expression is evaluated in order, with the output of one expression
/// becoming the input to the next (pipeline style).
pub type CompiledProgram = Vec<CompiledExpr>;
