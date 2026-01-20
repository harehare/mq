//! Slot-based variable resolution mapping.
//!
//! This module provides data structures for mapping AST nodes to pre-computed
//! slot indices, enabling fast variable access without name-based lookups.

use crate::Ident;
use crate::ast::TokenId;
use rustc_hash::FxHashMap;

/// Resolution result for a variable reference.
///
/// Determines how a variable should be accessed at runtime.
// TODO: Remove allow(dead_code) once the slot-based evaluation is integrated
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolution {
    /// Local variable in the current stack frame.
    /// Access via `env.slots[slot]`.
    Local {
        /// Slot index in the current frame.
        slot: u16,
    },
    /// Captured variable from an enclosing scope (closure).
    /// Access via `env.captures[index]`.
    Capture {
        /// Index in the captures array.
        index: u16,
    },
    /// Built-in native function.
    /// No slot needed, resolved by name at call time.
    Builtin,
}

/// Information about a captured variable.
///
/// Used to build the captures array when creating a closure.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureInfo {
    /// Index in the captures array of the function being defined.
    pub capture_index: u16,
    /// Original variable name (for debugging/error messages).
    pub name: Ident,
    /// If true, the variable is captured from the parent's captures array.
    /// If false, it's captured from the parent's local slots.
    pub from_parent_capture: bool,
    /// Index in the parent scope (either slot index or capture index).
    pub parent_index: u16,
    /// Whether the captured variable is mutable.
    pub is_mutable: bool,
}

/// Scope information for a function or block.
///
/// Contains metadata needed to create the runtime environment.
#[allow(dead_code)]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScopeInfo {
    /// Number of local variable slots needed.
    pub slot_count: u16,
    /// Variables captured from enclosing scopes.
    pub captures: Vec<CaptureInfo>,
}

/// Mapping from AST nodes to resolution information.
///
/// Created by the resolver phase and used during evaluation.
#[allow(dead_code)]
#[derive(Debug, Default)]
pub struct SlotMap {
    /// Resolution for variable references and definitions.
    /// Key: TokenId of the AST node (Ident, Let, Var, Assign, etc.)
    pub resolutions: FxHashMap<TokenId, Resolution>,

    /// Scope information for functions and blocks.
    /// Key: TokenId of the function definition (Def, Fn) node.
    pub scope_info: FxHashMap<TokenId, ScopeInfo>,
}

#[allow(dead_code)]
impl SlotMap {
    /// Creates a new empty slot map.
    pub fn new() -> Self {
        Self::default()
    }

    /// Gets the resolution for a variable reference.
    #[inline]
    pub fn get_resolution(&self, token_id: TokenId) -> Option<&Resolution> {
        self.resolutions.get(&token_id)
    }

    /// Gets the scope info for a function.
    #[inline]
    pub fn get_scope_info(&self, token_id: TokenId) -> Option<&ScopeInfo> {
        self.scope_info.get(&token_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arena::ArenaId;

    #[test]
    fn test_resolution_local() {
        let res = Resolution::Local { slot: 5 };
        assert_eq!(res, Resolution::Local { slot: 5 });
    }

    #[test]
    fn test_resolution_capture() {
        let res = Resolution::Capture { index: 3 };
        assert_eq!(res, Resolution::Capture { index: 3 });
    }

    #[test]
    fn test_slot_map_operations() {
        let mut map = SlotMap::new();
        let token_id = ArenaId::new(1);

        map.resolutions.insert(token_id, Resolution::Local { slot: 0 });
        assert_eq!(map.get_resolution(token_id), Some(&Resolution::Local { slot: 0 }));

        let scope_info = ScopeInfo {
            slot_count: 3,
            captures: vec![],
        };
        map.scope_info.insert(token_id, scope_info.clone());
        assert_eq!(map.get_scope_info(token_id), Some(&scope_info));
    }

    #[test]
    fn test_capture_info() {
        let capture = CaptureInfo {
            capture_index: 0,
            name: Ident::new("x"),
            from_parent_capture: false,
            parent_index: 2,
            is_mutable: false,
        };
        assert_eq!(capture.name, Ident::new("x"));
        assert!(!capture.from_parent_capture);
    }
}
