//! Call stack management for the compiler.
//!
//! This module provides utilities for tracking function call depth to prevent
//! stack overflow errors during recursive function calls.

/// Default maximum call stack depth.
///
/// This matches the default used by the tree-walking evaluator.
pub const DEFAULT_MAX_CALL_STACK_DEPTH: u32 = 1024;

/// A call stack for tracking function call depth.
///
/// This is a simple depth counter to prevent stack overflow. It's represented
/// as a Vec<()> where the length indicates the current depth.
pub type CallStack = Vec<()>;

/// Checks if the call stack has exceeded the maximum depth.
///
/// # Arguments
///
/// * `stack` - The current call stack
/// * `max_depth` - The maximum allowed depth
///
/// # Returns
///
/// `true` if the stack depth is within limits, `false` otherwise.
///
/// # Example
///
/// ```rust,ignore
/// let mut stack = CallStack::new();
/// assert!(check_stack_depth(&stack, 1024));
/// ```
#[allow(dead_code)]
pub fn check_stack_depth(stack: &CallStack, max_depth: u32) -> bool {
    stack.len() < max_depth as usize
}
