//! Variable resolution phase for slot-based access.
//!
//! This module analyzes AST nodes to compute slot indices for variables,
//! eliminating the need for name-based lookups at runtime.

use super::builtin;
use super::slot_map::{CaptureInfo, Resolution, ScopeInfo, SlotMap};
use crate::ast::node::{Expr, Node, Pattern, StringSegment};
use crate::{Ident, Program, Shared};
use rustc_hash::FxHashMap;

/// Information about a local variable in a scope.
#[allow(dead_code)]
#[derive(Debug, Clone)]
struct LocalInfo {
    /// Slot index in the current frame.
    slot: u16,
    /// Whether the variable is mutable (var vs let).
    is_mutable: bool,
}

/// A scope during resolution.
#[allow(dead_code)]
#[derive(Debug)]
struct ResolverScope {
    /// Local variables: name -> slot info.
    locals: FxHashMap<Ident, LocalInfo>,
    /// Next slot index to allocate.
    next_slot: u16,
    /// Captured variables for this scope.
    captures: Vec<CaptureInfo>,
    /// Index of parent scope in the scopes stack.
    parent_index: Option<usize>,
    /// Whether this is a function boundary (affects capture semantics).
    is_function_boundary: bool,
}

impl ResolverScope {
    fn new(parent_index: Option<usize>, is_function_boundary: bool) -> Self {
        Self {
            locals: FxHashMap::default(),
            next_slot: 0,
            captures: Vec::new(),
            parent_index,
            is_function_boundary,
        }
    }
}

/// Resolver that analyzes AST to produce a SlotMap.
pub struct Resolver {
    /// Stack of scopes (innermost last).
    scopes: Vec<ResolverScope>,
    /// The resulting slot map.
    slot_map: SlotMap,
}

impl Default for Resolver {
    fn default() -> Self {
        Self::new()
    }
}

impl Resolver {
    /// Creates a new resolver with a global scope.
    pub fn new() -> Self {
        Self {
            scopes: vec![ResolverScope::new(None, true)],
            slot_map: SlotMap::new(),
        }
    }

    /// Resolves a program and returns the slot map.
    pub fn resolve(mut self, program: &Program) -> SlotMap {
        for node in program {
            self.resolve_node(node);
        }

        // Store global scope info
        let global_scope = self.scopes.pop().unwrap();
        // We don't store global scope info as it's handled specially

        // Return slot map with global slot count for reference
        let _ = global_scope; // Suppress unused warning
        self.slot_map
    }

    /// Gets the current (innermost) scope.
    fn current_scope(&self) -> &ResolverScope {
        self.scopes.last().unwrap()
    }

    /// Gets the current (innermost) scope mutably.
    fn current_scope_mut(&mut self) -> &mut ResolverScope {
        self.scopes.last_mut().unwrap()
    }

    /// Pushes a new scope onto the stack.
    fn push_scope(&mut self, is_function_boundary: bool) {
        let parent_index = self.scopes.len() - 1;
        self.scopes
            .push(ResolverScope::new(Some(parent_index), is_function_boundary));
    }

    /// Pops the current scope and returns its info.
    fn pop_scope(&mut self) -> ScopeInfo {
        let scope = self.scopes.pop().unwrap();
        ScopeInfo {
            slot_count: scope.next_slot,
            captures: scope.captures,
        }
    }

    /// Declares a variable in the current scope.
    fn declare(&mut self, name: Ident, is_mutable: bool) -> u16 {
        let slot = self.current_scope().next_slot;
        self.current_scope_mut().next_slot += 1;
        self.current_scope_mut()
            .locals
            .insert(name, LocalInfo { slot, is_mutable });
        slot
    }

    /// Resolves a variable reference.
    fn resolve_variable(&mut self, name: Ident) -> Resolution {
        let scope_count = self.scopes.len();

        // Search from innermost to outermost scope
        for depth in 0..scope_count {
            let scope_index = scope_count - 1 - depth;
            let scope = &self.scopes[scope_index];

            if let Some(local_info) = scope.locals.get(&name) {
                if depth == 0 {
                    // Found in current scope
                    return Resolution::Local { slot: local_info.slot };
                } else {
                    // Found in outer scope - need to capture
                    return self.create_capture(name, scope_index, local_info.slot, local_info.is_mutable, false);
                }
            }

            // Check if already captured in this scope
            if depth > 0
                && let Some(cap_idx) = scope.captures.iter().position(|c| c.name == name)
            {
                let is_mutable = scope.captures[cap_idx].is_mutable;
                return self.create_capture(name, scope_index, cap_idx as u16, is_mutable, true);
            }
        }

        // Check if it's a builtin function
        if name.resolve_with(builtin::get_builtin_functions_by_str).is_some() {
            return Resolution::Builtin;
        }

        // Not found - will be an error at runtime
        // For now, treat as builtin to allow forward references to global functions
        Resolution::Builtin
    }

    /// Creates a capture for a variable from an outer scope.
    fn create_capture(
        &mut self,
        name: Ident,
        source_scope_index: usize,
        source_index: u16,
        is_mutable: bool,
        from_capture: bool,
    ) -> Resolution {
        // We need to capture through all function boundaries between current and source
        let current_scope_index = self.scopes.len() - 1;

        // Already captured in current scope?
        if let Some(idx) = self.scopes[current_scope_index]
            .captures
            .iter()
            .position(|c| c.name == name)
        {
            return Resolution::Capture { index: idx as u16 };
        }

        // Create capture chain from source to current
        let mut prev_index = source_index;
        let mut prev_from_capture = from_capture;

        for i in (source_scope_index + 1)..=current_scope_index {
            let scope = &self.scopes[i];

            // Check if already captured at this level
            if let Some(idx) = scope.captures.iter().position(|c| c.name == name) {
                prev_index = idx as u16;
                prev_from_capture = true;
                continue;
            }

            // Need to add capture at this level
            let capture_index = self.scopes[i].captures.len() as u16;
            self.scopes[i].captures.push(CaptureInfo {
                capture_index,
                name,
                from_parent_capture: prev_from_capture,
                parent_index: prev_index,
                is_mutable,
            });

            prev_index = capture_index;
            prev_from_capture = true;
        }

        Resolution::Capture { index: prev_index }
    }

    /// Resolves a node.
    fn resolve_node(&mut self, node: &Shared<Node>) {
        match &*node.expr {
            // Variable reference
            Expr::Ident(ident) => {
                let resolution = self.resolve_variable(ident.name);
                self.slot_map.resolutions.insert(node.token_id, resolution);
            }

            // Variable declaration (immutable)
            Expr::Let(ident, value) => {
                // Resolve value first (before declaring the variable)
                self.resolve_node(value);
                let slot = self.declare(ident.name, false);
                self.slot_map
                    .resolutions
                    .insert(node.token_id, Resolution::Local { slot });
            }

            // Variable declaration (mutable)
            Expr::Var(ident, value) => {
                self.resolve_node(value);
                let slot = self.declare(ident.name, true);
                self.slot_map
                    .resolutions
                    .insert(node.token_id, Resolution::Local { slot });
            }

            // Variable assignment
            Expr::Assign(ident, value) => {
                self.resolve_node(value);
                let resolution = self.resolve_variable(ident.name);
                self.slot_map.resolutions.insert(node.token_id, resolution);
            }

            // Function definition
            Expr::Def(ident, params, body) => {
                // Declare function name in current scope first (for recursion)
                let fn_slot = self.declare(ident.name, false);
                self.slot_map
                    .resolutions
                    .insert(node.token_id, Resolution::Local { slot: fn_slot });

                // Push function scope
                self.push_scope(true);

                // Declare parameters
                for param in params {
                    self.declare(param.ident.name, false);
                    if let Some(default) = &param.default {
                        self.resolve_node(default);
                    }
                }

                // Resolve body
                for body_node in body {
                    self.resolve_node(body_node);
                }

                // Pop scope and store info
                let scope_info = self.pop_scope();
                self.slot_map.scope_info.insert(node.token_id, scope_info);
            }

            // Anonymous function
            Expr::Fn(params, body) => {
                self.push_scope(true);

                for param in params {
                    self.declare(param.ident.name, false);
                    if let Some(default) = &param.default {
                        self.resolve_node(default);
                    }
                }

                for body_node in body {
                    self.resolve_node(body_node);
                }

                let scope_info = self.pop_scope();
                self.slot_map.scope_info.insert(node.token_id, scope_info);
            }

            // Function call
            Expr::Call(ident, args) => {
                // Resolve the function name
                let resolution = self.resolve_variable(ident.name);
                self.slot_map.resolutions.insert(node.token_id, resolution);

                // Resolve arguments
                for arg in args {
                    self.resolve_node(arg);
                }
            }

            // Dynamic function call
            Expr::CallDynamic(callable, args) => {
                self.resolve_node(callable);
                for arg in args {
                    self.resolve_node(arg);
                }
            }

            // Block
            Expr::Block(nodes) => {
                // Blocks share the parent scope (no new scope boundary)
                for inner_node in nodes {
                    self.resolve_node(inner_node);
                }
            }

            // While loop
            Expr::While(cond, body) => {
                self.resolve_node(cond);
                for body_node in body {
                    self.resolve_node(body_node);
                }
            }

            // Loop
            Expr::Loop(body) => {
                for body_node in body {
                    self.resolve_node(body_node);
                }
            }

            // Foreach
            Expr::Foreach(ident, iterable, body) => {
                self.resolve_node(iterable);

                // Create scope for loop variable
                self.push_scope(false);
                let slot = self.declare(ident.name, false);
                self.slot_map
                    .resolutions
                    .insert(node.token_id, Resolution::Local { slot });

                for body_node in body {
                    self.resolve_node(body_node);
                }

                let _ = self.pop_scope();
            }

            // If expression
            Expr::If(branches) => {
                for (cond, body) in branches {
                    if let Some(cond_node) = cond {
                        self.resolve_node(cond_node);
                    }
                    self.resolve_node(body);
                }
            }

            // Match expression
            Expr::Match(value, arms) => {
                self.resolve_node(value);

                for arm in arms {
                    // Create scope for pattern bindings
                    self.push_scope(false);

                    self.resolve_pattern(&arm.pattern);

                    if let Some(guard) = &arm.guard {
                        self.resolve_node(guard);
                    }

                    self.resolve_node(&arm.body);

                    let _ = self.pop_scope();
                }
            }

            // Logical operators
            Expr::And(left, right) | Expr::Or(left, right) => {
                self.resolve_node(left);
                self.resolve_node(right);
            }

            // Try-catch
            Expr::Try(try_expr, catch_expr) => {
                self.resolve_node(try_expr);
                self.resolve_node(catch_expr);
            }

            // Parenthesized expression
            Expr::Paren(inner) => {
                self.resolve_node(inner);
            }

            // Quote/Unquote
            Expr::Quote(inner) | Expr::Unquote(inner) => {
                self.resolve_node(inner);
            }

            // Interpolated string
            Expr::InterpolatedString(segments) => {
                for segment in segments {
                    if let StringSegment::Expr(expr) = segment {
                        self.resolve_node(expr);
                    }
                }
            }

            // Module definition
            Expr::Module(ident, body) => {
                let mod_slot = self.declare(ident.name, false);
                self.slot_map
                    .resolutions
                    .insert(node.token_id, Resolution::Local { slot: mod_slot });

                self.push_scope(true);

                for body_node in body {
                    self.resolve_node(body_node);
                }

                let scope_info = self.pop_scope();
                self.slot_map.scope_info.insert(node.token_id, scope_info);
            }

            // Qualified access (Module::func)
            Expr::QualifiedAccess(path, target) => {
                // Resolve the module path
                if let Some(first) = path.first() {
                    let resolution = self.resolve_variable(first.name);
                    self.slot_map.resolutions.insert(node.token_id, resolution);
                }

                // Resolve arguments if it's a call
                match target {
                    crate::ast::node::AccessTarget::Call(_, args) => {
                        for arg in args {
                            self.resolve_node(arg);
                        }
                    }
                    crate::ast::node::AccessTarget::Ident(_) => {}
                }
            }

            // Macro definition (treated like function)
            Expr::Macro(ident, params, body) => {
                let macro_slot = self.declare(ident.name, false);
                self.slot_map
                    .resolutions
                    .insert(node.token_id, Resolution::Local { slot: macro_slot });

                self.push_scope(true);

                for param in params {
                    self.declare(param.ident.name, false);
                }

                self.resolve_node(body);

                let scope_info = self.pop_scope();
                self.slot_map.scope_info.insert(node.token_id, scope_info);
            }

            // Break with optional value
            Expr::Break(Some(value)) => {
                self.resolve_node(value);
            }

            // Literals and other leaf nodes - no resolution needed
            Expr::Literal(_)
            | Expr::Selector(_)
            | Expr::Include(_)
            | Expr::Import(_)
            | Expr::Self_
            | Expr::Nodes
            | Expr::Break(None)
            | Expr::Continue => {}
        }
    }

    /// Resolves pattern bindings in match arms.
    fn resolve_pattern(&mut self, pattern: &Pattern) {
        match pattern {
            Pattern::Ident(ident) => {
                // Bind the identifier in current scope
                self.declare(ident.name, false);
            }
            Pattern::Array(patterns) => {
                for p in patterns {
                    self.resolve_pattern(p);
                }
            }
            Pattern::ArrayRest(patterns, rest_ident) => {
                for p in patterns {
                    self.resolve_pattern(p);
                }
                self.declare(rest_ident.name, false);
            }
            Pattern::Dict(entries) => {
                for (_, p) in entries {
                    self.resolve_pattern(p);
                }
            }
            // Literals, wildcards, and type patterns don't bind variables
            Pattern::Literal(_) | Pattern::Wildcard | Pattern::Type(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arena::ArenaId;
    use crate::ast::node::{IdentWithToken, Literal, Param};
    use smallvec::smallvec;

    fn make_node(token_id: u32, expr: Expr) -> Shared<Node> {
        Shared::new(Node {
            token_id: ArenaId::new(token_id),
            expr: Shared::new(expr),
        })
    }

    fn make_ident(name: &str) -> IdentWithToken {
        IdentWithToken::new(name)
    }

    #[test]
    fn test_simple_let() {
        // let x = 42
        let program = vec![make_node(
            0,
            Expr::Let(
                make_ident("x"),
                make_node(1, Expr::Literal(Literal::Number(42.0.into()))),
            ),
        )];

        let resolver = Resolver::new();
        let slot_map = resolver.resolve(&program);

        assert_eq!(
            slot_map.get_resolution(ArenaId::new(0)),
            Some(&Resolution::Local { slot: 0 })
        );
    }

    #[test]
    fn test_variable_reference() {
        // let x = 1; x
        let program = vec![
            make_node(
                0,
                Expr::Let(
                    make_ident("x"),
                    make_node(1, Expr::Literal(Literal::Number(1.0.into()))),
                ),
            ),
            make_node(2, Expr::Ident(make_ident("x"))),
        ];

        let resolver = Resolver::new();
        let slot_map = resolver.resolve(&program);

        // x is declared at slot 0
        assert_eq!(
            slot_map.get_resolution(ArenaId::new(0)),
            Some(&Resolution::Local { slot: 0 })
        );
        // x reference should resolve to slot 0
        assert_eq!(
            slot_map.get_resolution(ArenaId::new(2)),
            Some(&Resolution::Local { slot: 0 })
        );
    }

    #[test]
    fn test_function_with_params() {
        // def add(a, b): a + b
        let program = vec![make_node(
            0,
            Expr::Def(
                make_ident("add"),
                smallvec![Param::new(make_ident("a")), Param::new(make_ident("b")),],
                vec![make_node(1, Expr::Ident(make_ident("a")))],
            ),
        )];

        let resolver = Resolver::new();
        let slot_map = resolver.resolve(&program);

        // Function 'add' is at slot 0 in global scope
        assert_eq!(
            slot_map.get_resolution(ArenaId::new(0)),
            Some(&Resolution::Local { slot: 0 })
        );

        // Scope info for the function
        let scope_info = slot_map.get_scope_info(ArenaId::new(0)).unwrap();
        assert_eq!(scope_info.slot_count, 2); // a, b
        assert!(scope_info.captures.is_empty());

        // 'a' inside function should be local slot 0
        assert_eq!(
            slot_map.get_resolution(ArenaId::new(1)),
            Some(&Resolution::Local { slot: 0 })
        );
    }

    #[test]
    fn test_closure_capture() {
        // let x = 1; def f(): x
        let program = vec![
            make_node(
                0,
                Expr::Let(
                    make_ident("x"),
                    make_node(1, Expr::Literal(Literal::Number(1.0.into()))),
                ),
            ),
            make_node(
                2,
                Expr::Def(
                    make_ident("f"),
                    smallvec![],
                    vec![make_node(3, Expr::Ident(make_ident("x")))],
                ),
            ),
        ];

        let resolver = Resolver::new();
        let slot_map = resolver.resolve(&program);

        // x in global scope is at slot 0
        assert_eq!(
            slot_map.get_resolution(ArenaId::new(0)),
            Some(&Resolution::Local { slot: 0 })
        );

        // Function 'f' captures x
        let scope_info = slot_map.get_scope_info(ArenaId::new(2)).unwrap();
        assert_eq!(scope_info.captures.len(), 1);
        assert_eq!(scope_info.captures[0].name, Ident::new("x"));
        assert_eq!(scope_info.captures[0].parent_index, 0);
        assert!(!scope_info.captures[0].from_parent_capture);

        // x reference inside f should be a capture
        assert_eq!(
            slot_map.get_resolution(ArenaId::new(3)),
            Some(&Resolution::Capture { index: 0 })
        );
    }

    #[test]
    fn test_builtin_function() {
        // abs(-1)
        let program = vec![make_node(
            0,
            Expr::Call(
                make_ident("abs"),
                smallvec![make_node(1, Expr::Literal(Literal::Number((-1.0).into())))],
            ),
        )];

        let resolver = Resolver::new();
        let slot_map = resolver.resolve(&program);

        // 'abs' should resolve to Builtin
        assert_eq!(slot_map.get_resolution(ArenaId::new(0)), Some(&Resolution::Builtin));
    }

    #[test]
    fn test_mutable_variable() {
        // var x = 1; x = 2
        let program = vec![
            make_node(
                0,
                Expr::Var(
                    make_ident("x"),
                    make_node(1, Expr::Literal(Literal::Number(1.0.into()))),
                ),
            ),
            make_node(
                2,
                Expr::Assign(
                    make_ident("x"),
                    make_node(3, Expr::Literal(Literal::Number(2.0.into()))),
                ),
            ),
        ];

        let resolver = Resolver::new();
        let slot_map = resolver.resolve(&program);

        // var x at slot 0
        assert_eq!(
            slot_map.get_resolution(ArenaId::new(0)),
            Some(&Resolution::Local { slot: 0 })
        );

        // x = 2 assigns to slot 0
        assert_eq!(
            slot_map.get_resolution(ArenaId::new(2)),
            Some(&Resolution::Local { slot: 0 })
        );
    }

    #[test]
    fn test_foreach_binding() {
        // foreach i in [1,2,3]: i
        let program = vec![make_node(
            0,
            Expr::Foreach(
                make_ident("i"),
                make_node(1, Expr::Literal(Literal::Number(1.0.into()))), // simplified
                vec![make_node(2, Expr::Ident(make_ident("i")))],
            ),
        )];

        let resolver = Resolver::new();
        let slot_map = resolver.resolve(&program);

        // 'i' is the loop variable
        assert_eq!(
            slot_map.get_resolution(ArenaId::new(0)),
            Some(&Resolution::Local { slot: 0 })
        );

        // 'i' reference inside loop
        assert_eq!(
            slot_map.get_resolution(ArenaId::new(2)),
            Some(&Resolution::Local { slot: 0 })
        );
    }
}
