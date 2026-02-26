//! Constraint generation for type inference.

use crate::infer::{DeferredOverload, DeferredUserCall, InferenceContext};
use crate::types::Type;
use crate::unify::range_to_span;
use mq_hir::{Hir, SymbolId, SymbolKind};
use smol_str::SmolStr;
use std::fmt;

/// Type constraint for unification
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Constraint {
    /// Two types must be equal
    Equal(Type, Type, Option<mq_lang::Range>),
}

impl fmt::Display for Constraint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Constraint::Equal(t1, t2, _) => write!(f, "{} ~ {}", t1, t2),
        }
    }
}

/// Helper function to get children of a symbol
fn get_children(hir: &Hir, parent_id: SymbolId) -> Vec<SymbolId> {
    hir.symbols()
        .filter_map(|(id, symbol)| {
            if symbol.parent == Some(parent_id) {
                Some(id)
            } else {
                None
            }
        })
        .collect()
}

/// Helper function to get the range of a symbol
fn get_symbol_range(hir: &Hir, symbol_id: SymbolId) -> Option<mq_lang::Range> {
    hir.symbol(symbol_id).and_then(|symbol| symbol.source.text_range)
}

/// Checks if a symbol belongs to a module source (include/import/module).
///
/// Symbols from included/imported modules are trusted library code and should
/// not be type-checked, similar to builtin symbols.
fn is_module_symbol(
    _hir: &Hir,
    symbol: &mq_hir::Symbol,
    module_source_ids: &rustc_hash::FxHashSet<mq_hir::SourceId>,
) -> bool {
    symbol
        .source
        .source_id
        .is_some_and(|sid| module_source_ids.contains(&sid))
}

/// Collects all source IDs from include/import/module symbols.
fn collect_module_source_ids(hir: &Hir) -> rustc_hash::FxHashSet<mq_hir::SourceId> {
    let mut ids = rustc_hash::FxHashSet::default();
    for (_, symbol) in hir.symbols() {
        match symbol.kind {
            SymbolKind::Include(source_id) | SymbolKind::Import(source_id) | SymbolKind::Module(source_id) => {
                ids.insert(source_id);
            }
            _ => {}
        }
    }
    ids
}

/// Generates type constraints from HIR
pub fn generate_constraints(hir: &Hir, ctx: &mut InferenceContext) {
    // Collect module source IDs to skip their symbols during type checking.
    // Symbols from included/imported modules are trusted library code.
    let module_source_ids = collect_module_source_ids(hir);

    // Use a two-pass approach to ensure literals have concrete types before operators use them

    // Pass 1: Assign types to literals, variables, and simple constructs
    // This ensures base types are established first
    for (symbol_id, symbol) in hir.symbols() {
        // Skip builtin and module symbols to avoid type checking their implementations
        if hir.is_builtin_symbol(symbol) || is_module_symbol(hir, symbol, &module_source_ids) {
            continue;
        }

        match symbol.kind {
            SymbolKind::Number
            | SymbolKind::String
            | SymbolKind::Boolean
            | SymbolKind::Symbol
            | SymbolKind::None
            | SymbolKind::Variable
            | SymbolKind::Parameter
            | SymbolKind::PatternVariable
            | SymbolKind::Function(_)
            | SymbolKind::Macro(_) => {
                generate_symbol_constraints(hir, symbol_id, symbol.kind.clone(), ctx);
            }
            _ => {}
        }
    }

    // Pass 2: Set up piped inputs before processing operators/calls
    // Root-level symbols (parent=None, not builtin/module) form an implicit pipe chain
    let root_symbols: Vec<SymbolId> = hir
        .symbols()
        .filter(|(_, symbol)| {
            symbol.parent.is_none()
                && !hir.is_builtin_symbol(symbol)
                && !is_module_symbol(hir, symbol, &module_source_ids)
        })
        .map(|(id, _)| id)
        .collect();
    for i in 1..root_symbols.len() {
        let prev_ty = ctx.get_or_create_symbol_type(root_symbols[i - 1]);
        ctx.set_piped_input(root_symbols[i], prev_ty);
    }

    // Pass 3: Process all other symbols (operators, calls, etc.) except Block
    // These can now reference the concrete types from pass 1 and piped inputs from pass 2
    // Process in reverse order so children (higher IDs) are typed before parents (lower IDs)
    let pass3_symbols: Vec<(SymbolId, SymbolKind)> = hir
        .symbols()
        .filter(|(_, symbol)| {
            if hir.is_builtin_symbol(symbol) || is_module_symbol(hir, symbol, &module_source_ids) {
                return false;
            }
            // Skip kinds already processed in pass 1
            if matches!(
                symbol.kind,
                SymbolKind::Number
                    | SymbolKind::String
                    | SymbolKind::Boolean
                    | SymbolKind::Symbol
                    | SymbolKind::None
                    | SymbolKind::Variable
                    | SymbolKind::Parameter
                    | SymbolKind::PatternVariable
                    | SymbolKind::Function(_)
                    | SymbolKind::Macro(_)
                    | SymbolKind::Block
            ) {
                return false;
            }
            true
        })
        .map(|(id, symbol)| (id, symbol.kind.clone()))
        .collect();
    for (symbol_id, kind) in pass3_symbols.into_iter().rev() {
        generate_symbol_constraints(hir, symbol_id, kind, ctx);
    }

    // Pass 4: Process Block symbols and Function body pipe chains
    // Children are now typed, so we can thread output types through the chain
    for (symbol_id, symbol) in hir.symbols() {
        if hir.is_builtin_symbol(symbol) || is_module_symbol(hir, symbol, &module_source_ids) {
            continue;
        }
        if matches!(symbol.kind, SymbolKind::Block) {
            generate_block_constraints(hir, symbol_id, ctx);
        }
        // Handle implicit pipe chains in Function/Macro bodies
        if matches!(symbol.kind, SymbolKind::Function(_) | SymbolKind::Macro(_)) {
            generate_function_body_pipe_constraints(hir, symbol_id, ctx);
        }
    }
}

/// Generates constraints for Block symbols (pipe chains).
///
/// In mq, `x | f(y)` means the output of `x` flows into `f` as an implicit first argument.
/// Block symbols represent pipe chains; their children are the expressions in sequence.
/// This function threads the output type of each child to the next child's piped input,
/// and sets the Block's type to its last child's type.
fn generate_block_constraints(hir: &Hir, symbol_id: SymbolId, ctx: &mut InferenceContext) {
    let children = get_children(hir, symbol_id);

    if children.is_empty() {
        let ty_var = ctx.fresh_var();
        ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
        return;
    }

    // Thread types through the pipe chain sequentially:
    // Set piped input and re-process each child before moving to the next,
    // so that resolved types propagate correctly through the chain.
    for i in 1..children.len() {
        let prev_ty = ctx.get_or_create_symbol_type(children[i - 1]);
        ctx.set_piped_input(children[i], prev_ty);

        // Re-process Call/Ref children that received piped input,
        // since they were already processed in Pass 3 before piped inputs were set
        if let Some(child_symbol) = hir.symbol(children[i])
            && matches!(child_symbol.kind, SymbolKind::Call | SymbolKind::Ref)
        {
            generate_symbol_constraints(hir, children[i], child_symbol.kind.clone(), ctx);
        }
    }

    // Propagate piped input through UnaryOp to inner Call/Ref children.
    // e.g., `x | !is_empty()` — piped input flows through `!` to `is_empty()`
    propagate_piped_input_through_unary_ops(hir, &children, ctx);

    // The Block's type is the last child's type
    let last_ty = ctx.get_or_create_symbol_type(*children.last().unwrap());
    ctx.set_symbol_type(symbol_id, last_ty);
}

/// Generates pipe constraints for implicit pipe chains in Function/Macro bodies.
///
/// When a function body has multiple non-parameter children (e.g., `def f(x): a | b`),
/// the output of each expression flows into the next as piped input.
fn generate_function_body_pipe_constraints(hir: &Hir, symbol_id: SymbolId, ctx: &mut InferenceContext) {
    let children = get_children(hir, symbol_id);

    // Get non-parameter body children
    let body_children: Vec<SymbolId> = children
        .into_iter()
        .filter(|&child_id| {
            hir.symbol(child_id)
                .map(|s| !matches!(s.kind, SymbolKind::Parameter | SymbolKind::Keyword))
                .unwrap_or(false)
        })
        .collect();

    if body_children.len() <= 1 {
        return;
    }

    // Thread types through the pipe chain sequentially:
    // Set piped input and re-process each child before moving to the next,
    // so that resolved types propagate correctly through the chain.
    for i in 1..body_children.len() {
        let prev_ty = ctx.get_or_create_symbol_type(body_children[i - 1]);
        ctx.set_piped_input(body_children[i], prev_ty);

        // Re-process Call/Ref children that received piped input
        if let Some(child_symbol) = hir.symbol(body_children[i])
            && matches!(child_symbol.kind, SymbolKind::Call | SymbolKind::Ref)
        {
            generate_symbol_constraints(hir, body_children[i], child_symbol.kind.clone(), ctx);
        }
    }

    // Propagate piped input through UnaryOp to inner Call/Ref children
    propagate_piped_input_through_unary_ops(hir, &body_children, ctx);
}

/// Propagates piped input through UnaryOp nodes to their inner Call/Ref children.
///
/// When a pipe chain contains `x | !f()`, the piped value from `x` flows through
/// the `!` (UnaryOp) to `f()` (Call). This function handles that propagation and
/// re-processes the inner Call/Ref to pick up the piped input.
fn propagate_piped_input_through_unary_ops(hir: &Hir, children: &[SymbolId], ctx: &mut InferenceContext) {
    for &child_id in children.iter().skip(1) {
        if let Some(child_symbol) = hir.symbol(child_id)
            && matches!(child_symbol.kind, SymbolKind::UnaryOp)
            && let Some(piped_ty) = ctx.get_piped_input(child_id).cloned()
        {
            let inner_children = get_children(hir, child_id);
            for &inner_id in &inner_children {
                if let Some(inner_sym) = hir.symbol(inner_id)
                    && matches!(inner_sym.kind, SymbolKind::Call | SymbolKind::Ref)
                {
                    ctx.set_piped_input(inner_id, piped_ty.clone());
                    generate_symbol_constraints(hir, inner_id, inner_sym.kind.clone(), ctx);
                }
            }
        }
    }
}

/// Resolves the body type of an elif/else branch.
///
/// For Elif: children are [condition, body] — constrains condition to Bool, returns body type.
/// For Else: children are [body] — returns body type.
/// For other kinds: returns the symbol's type directly.
fn resolve_branch_body_type(hir: &Hir, branch_id: SymbolId, ctx: &mut InferenceContext) -> Type {
    if let Some(symbol) = hir.symbol(branch_id) {
        let children = get_children(hir, branch_id);
        match symbol.kind {
            SymbolKind::Elif => {
                if children.len() >= 2 {
                    let range = get_symbol_range(hir, branch_id);
                    // First child is condition
                    let cond_ty = ctx.get_or_create_symbol_type(children[0]);
                    ctx.add_constraint(Constraint::Equal(cond_ty, Type::Bool, range));
                    // Last child is the body
                    let body_ty = ctx.get_or_create_symbol_type(*children.last().unwrap());
                    ctx.set_symbol_type(branch_id, body_ty.clone());
                    body_ty
                } else {
                    ctx.get_or_create_symbol_type(branch_id)
                }
            }
            SymbolKind::Else => {
                if let Some(&last_child) = children.last() {
                    let body_ty = ctx.get_or_create_symbol_type(last_child);
                    ctx.set_symbol_type(branch_id, body_ty.clone());
                    body_ty
                } else {
                    ctx.get_or_create_symbol_type(branch_id)
                }
            }
            _ => ctx.get_or_create_symbol_type(branch_id),
        }
    } else {
        ctx.get_or_create_symbol_type(branch_id)
    }
}

/// Checks if a symbol is a foreach iterable reference.
///
/// In the HIR, `foreach (var, iterable): body;` is represented as:
/// - Parent has children: [Foreach, Variable, Ref(iterable)]
/// - The iterable is a `Ref` sibling of `Foreach` under the same parent
///
/// These Refs should not receive piped input since they are iterable function
/// references whose arguments are not represented in the HIR.
fn is_foreach_iterable_ref(hir: &Hir, symbol_id: SymbolId) -> bool {
    let symbol = match hir.symbol(symbol_id) {
        Some(s) => s,
        None => return false,
    };
    if !matches!(symbol.kind, SymbolKind::Ref) {
        return false;
    }
    let parent_id = match symbol.parent {
        Some(id) => id,
        None => return false,
    };
    // Check if any sibling under the same parent is a Foreach
    hir.symbols()
        .any(|(_, s)| s.parent == Some(parent_id) && matches!(s.kind, SymbolKind::Foreach))
}

/// Checks if a symbol might receive piped input later (i.e., is inside a Block
/// or a Function/Macro body with multiple expressions).
///
/// Also handles the case where a Call/Ref is inside a UnaryOp that is itself
/// inside a Block, e.g., `x | !f()` where `f()` eventually receives piped input.
fn might_receive_piped_input(hir: &Hir, symbol_id: SymbolId) -> bool {
    let parent_id = match hir.symbol(symbol_id).and_then(|s| s.parent) {
        Some(id) => id,
        None => return false,
    };
    let parent = match hir.symbol(parent_id) {
        Some(p) => p,
        None => return false,
    };
    if matches!(
        parent.kind,
        SymbolKind::Block | SymbolKind::Function(_) | SymbolKind::Macro(_)
    ) {
        return true;
    }
    // Check grandparent: if parent is UnaryOp inside a pipe-capable construct
    if matches!(parent.kind, SymbolKind::UnaryOp)
        && let Some(grandparent_id) = parent.parent
        && let Some(grandparent) = hir.symbol(grandparent_id)
    {
        return matches!(
            grandparent.kind,
            SymbolKind::Block | SymbolKind::Function(_) | SymbolKind::Macro(_)
        );
    }
    false
}

/// Builds the argument type list for a piped builtin function call.
///
/// When a function is called via pipe (e.g., `arr | join(",")`) the piped value
/// becomes the implicit first argument. This function checks if there's a piped input
/// and whether prepending it produces a valid overload match. If so, the piped input
/// is prepended; otherwise, only the explicit arguments are returned.
fn build_piped_call_args(
    ctx: &mut InferenceContext,
    symbol_id: SymbolId,
    explicit_arg_tys: &[Type],
    func_name: &str,
) -> Vec<Type> {
    if let Some(piped_ty) = ctx.get_piped_input(symbol_id).cloned() {
        // Try explicit args first — if they already match an overload,
        // the piped input should not be prepended (it flows through unchanged)
        let resolved_explicit: Vec<Type> = explicit_arg_tys.iter().map(|ty| ctx.resolve_type(ty)).collect();
        if ctx.resolve_overload(func_name, &resolved_explicit).is_some() {
            return explicit_arg_tys.to_vec();
        }

        // Explicit args don't match; try with piped input prepended as implicit first argument
        let mut piped_args = vec![piped_ty];
        piped_args.extend_from_slice(explicit_arg_tys);

        piped_args
    } else {
        explicit_arg_tys.to_vec()
    }
}

/// Resolves a builtin function call using overload resolution.
///
/// If `defer_error` is true (e.g., the call might receive piped input later),
/// no error is generated on mismatch — only a fresh type variable is assigned.
fn resolve_builtin_call(
    ctx: &mut InferenceContext,
    symbol_id: SymbolId,
    func_name: &str,
    arg_tys: &[Type],
    range: Option<mq_lang::Range>,
    defer_error: bool,
) {
    let resolved_arg_tys: Vec<Type> = arg_tys.iter().map(|ty| ctx.resolve_type(ty)).collect();
    let is_builtin = ctx.get_builtin_overloads(func_name).is_some();
    let has_unresolved_args = resolved_arg_tys.iter().any(|ty| ty.is_var());

    // If any argument is still a type variable and there are multiple overloads,
    // defer resolution to avoid committing to the wrong overload
    if has_unresolved_args && is_builtin {
        let overload_count = ctx.get_builtin_overloads(func_name).map(|o| o.len()).unwrap_or(0);
        if overload_count > 1 {
            let ty_var = ctx.fresh_var();
            ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
            ctx.add_deferred_overload(DeferredOverload {
                symbol_id,
                op_name: SmolStr::new(func_name),
                operand_tys: arg_tys.to_vec(),
                range,
            });
            return;
        }
    }

    if let Some(resolved_ty) = ctx.resolve_overload(func_name, &resolved_arg_tys) {
        if let Type::Function(param_tys, ret_ty) = resolved_ty {
            for (arg_ty, param_ty) in arg_tys.iter().zip(param_tys.iter()) {
                ctx.add_constraint(Constraint::Equal(arg_ty.clone(), param_ty.clone(), range));
            }
            ctx.set_symbol_type(symbol_id, ret_ty.as_ref().clone());
        } else {
            let ty_var = ctx.fresh_var();
            ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
        }
    } else if is_builtin && !defer_error {
        ctx.add_error(crate::TypeError::UnificationError {
            left: format!(
                "{} with arguments ({})",
                func_name,
                resolved_arg_tys
                    .iter()
                    .map(|t| t.display_renumbered())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            right: "no matching overload".to_string(),
            span: range.as_ref().map(range_to_span),
            location: range.as_ref().map(|r| (r.start.line, r.start.column)),
        });
        let ty_var = ctx.fresh_var();
        ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
    } else {
        let ret_ty = Type::Var(ctx.fresh_var());
        ctx.set_symbol_type(symbol_id, ret_ty);
    }
}

/// Determines the type of a Pattern symbol from its literal children.
///
/// Pattern types:
/// - Has a Number child → Number
/// - Has a String child → String
/// - Has a Boolean child → Bool
/// - Has a Symbol child → Symbol
/// - Has a None child → None
/// - Otherwise → fresh type variable (wildcard or variable pattern)
fn resolve_pattern_type(hir: &Hir, pattern_id: SymbolId, ctx: &mut InferenceContext) -> Type {
    let children = get_children(hir, pattern_id);
    for &child_id in &children {
        if let Some(child_symbol) = hir.symbol(child_id) {
            match child_symbol.kind {
                SymbolKind::Number => return Type::Number,
                SymbolKind::String => return Type::String,
                SymbolKind::Boolean => return Type::Bool,
                SymbolKind::Symbol => return Type::Symbol,
                SymbolKind::None => return Type::None,
                SymbolKind::Array => return ctx.get_or_create_symbol_type(child_id),
                _ => {}
            }
        }
    }
    // Wildcard or variable pattern - compatible with any type
    Type::Var(ctx.fresh_var())
}

/// Generates constraints for a single symbol
fn generate_symbol_constraints(hir: &Hir, symbol_id: SymbolId, kind: SymbolKind, ctx: &mut InferenceContext) {
    match kind {
        // Literals have concrete types
        SymbolKind::Number => {
            let ty = Type::Number;
            ctx.set_symbol_type(symbol_id, ty);
        }
        SymbolKind::String => {
            let ty = Type::String;
            ctx.set_symbol_type(symbol_id, ty);
        }
        SymbolKind::Boolean => {
            let ty = Type::Bool;
            ctx.set_symbol_type(symbol_id, ty);
        }
        SymbolKind::Symbol => {
            let ty = Type::Symbol;
            ctx.set_symbol_type(symbol_id, ty);
        }
        SymbolKind::None => {
            let ty = Type::None;
            ctx.set_symbol_type(symbol_id, ty);
        }

        // Parameters and pattern variables get fresh type variables
        SymbolKind::Parameter | SymbolKind::PatternVariable => {
            let ty_var = ctx.fresh_var();
            ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
        }

        // Variables get fresh type variables, constrained to their initializer
        SymbolKind::Variable => {
            let ty_var = ctx.fresh_var();
            ctx.set_symbol_type(symbol_id, Type::Var(ty_var));

            // Connect variable type to its initializer expression (last child)
            let children = get_children(hir, symbol_id);
            if let Some(&last_child) = children.last() {
                let child_ty = ctx.get_or_create_symbol_type(last_child);
                let range = get_symbol_range(hir, symbol_id);
                ctx.add_constraint(Constraint::Equal(Type::Var(ty_var), child_ty, range));
            }
        }

        // Function definitions
        SymbolKind::Function(params) => {
            // Create type variables for each parameter
            let param_tys: Vec<Type> = params.iter().map(|_| Type::Var(ctx.fresh_var())).collect();

            // Create type variable for return type
            let ret_ty = Type::Var(ctx.fresh_var());

            // Function type is (param_tys) -> ret_ty
            let func_ty = Type::function(param_tys.clone(), ret_ty.clone());
            ctx.set_symbol_type(symbol_id, func_ty);

            // Bind parameter types to their parameter symbols
            let children = get_children(hir, symbol_id);
            let param_children: Vec<SymbolId> = children
                .iter()
                .filter(|&&child_id| {
                    hir.symbol(child_id)
                        .map(|s| matches!(s.kind, SymbolKind::Parameter))
                        .unwrap_or(false)
                })
                .copied()
                .collect();
            for (i, (param_sym, param_ty)) in param_children.iter().zip(param_tys.iter()).enumerate() {
                let sym_ty = ctx.get_or_create_symbol_type(*param_sym);
                let range = get_symbol_range(hir, *param_sym);
                ctx.add_constraint(Constraint::Equal(sym_ty, param_ty.clone(), range));

                // Connect default parameter values to their parameter types.
                // In HIR, parameters with defaults have `has_default: true` and the
                // default value expression appears as the next non-parameter, non-keyword
                // child after the parameter in the children list.
                if i < params.len() && params[i].has_default {
                    // Find the default value expression: it follows the parameter in the children
                    // list and is not a Parameter or Keyword
                    let param_pos = children.iter().position(|&c| c == *param_sym);
                    if let Some(pos) = param_pos {
                        // Look at the next child after this parameter
                        if let Some(&default_id) = children.get(pos + 1)
                            && let Some(default_sym) = hir.symbol(default_id)
                            && !matches!(default_sym.kind, SymbolKind::Parameter | SymbolKind::Keyword)
                        {
                            let default_ty = ctx.get_or_create_symbol_type(default_id);
                            ctx.add_constraint(Constraint::Equal(param_ty.clone(), default_ty, range));
                        }
                    }
                }
            }

            // Connect function body's type to the return type
            // The body is the last non-parameter child
            let body_children: Vec<SymbolId> = children
                .iter()
                .filter(|&&child_id| {
                    hir.symbol(child_id)
                        .map(|s| !matches!(s.kind, SymbolKind::Parameter))
                        .unwrap_or(false)
                })
                .copied()
                .collect();
            if let Some(&last_body) = body_children.last() {
                let body_ty = ctx.get_or_create_symbol_type(last_body);
                let range = get_symbol_range(hir, symbol_id);
                ctx.add_constraint(Constraint::Equal(ret_ty, body_ty, range));
            }
        }

        // References should unify with their definition
        SymbolKind::Ref => {
            if let Some(def_id) = hir.resolve_reference_symbol(symbol_id) {
                // Check if the reference is to a builtin function
                if let Some(symbol) = hir.symbol(def_id)
                    && let Some(name) = &symbol.value
                {
                    // Only treat as builtin if the resolved definition is actually a builtin symbol,
                    // not a user-defined parameter/variable that happens to share a builtin name
                    let has_builtin =
                        hir.is_builtin_symbol(symbol) && ctx.get_builtin_overloads(name.as_str()).is_some();
                    if has_builtin {
                        // Skip piped input handling for foreach iterable Refs.
                        // In HIR, `foreach (var, iterable)` stores the iterable as a Ref
                        // sibling of the Foreach symbol. These Refs should not be called
                        // with piped input since they reference the iterable function.
                        if is_foreach_iterable_ref(hir, symbol_id) {
                            let ty_var = ctx.fresh_var();
                            ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
                            return;
                        }

                        // If there's a piped input, treat this as a call with the piped value
                        if let Some(piped_ty) = ctx.get_piped_input(symbol_id).cloned() {
                            // Resolve the piped type through substitutions before overload resolution,
                            // so that bound type variables are replaced with their concrete types.
                            let resolved_piped = ctx.resolve_type(&piped_ty);

                            // If the piped input is still a type variable, defer overload resolution
                            // to avoid committing to a wrong overload when multiple are available.
                            if resolved_piped.is_var() {
                                let overload_count =
                                    ctx.get_builtin_overloads(name.as_str()).map(|o| o.len()).unwrap_or(0);
                                if overload_count > 1 {
                                    let ty_var = ctx.fresh_var();
                                    ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
                                    ctx.add_deferred_overload(DeferredOverload {
                                        symbol_id,
                                        op_name: SmolStr::new(name.as_str()),
                                        operand_tys: vec![piped_ty],
                                        range: get_symbol_range(hir, symbol_id),
                                    });
                                    return;
                                }
                            }

                            let arg_tys = vec![resolved_piped];
                            if let Some(resolved_ty) = ctx.resolve_overload(name.as_str(), &arg_tys) {
                                if let Type::Function(param_tys, ret_ty) = resolved_ty {
                                    let range = get_symbol_range(hir, symbol_id);
                                    for (arg_ty, param_ty) in [piped_ty].iter().zip(param_tys.iter()) {
                                        ctx.add_constraint(Constraint::Equal(arg_ty.clone(), param_ty.clone(), range));
                                    }
                                    ctx.set_symbol_type(symbol_id, ret_ty.as_ref().clone());
                                    return;
                                }
                            } else {
                                let range = get_symbol_range(hir, symbol_id);
                                ctx.add_error(crate::TypeError::Mismatch {
                                    expected: format!("argument matching {} overloads", name),
                                    found: arg_tys[0].display_renumbered(),
                                    span: range.as_ref().map(range_to_span),
                                    location: range.as_ref().map(|r| (r.start.line, r.start.column)),
                                });
                                let ty_var = ctx.fresh_var();
                                ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
                                return;
                            }
                        }

                        let overload_count = ctx.get_builtin_overloads(name.as_str()).map(|o| o.len()).unwrap_or(0);

                        // For builtin functions with overloads, we need to handle them specially
                        // For now, we'll create a fresh type variable that will be resolved
                        // during call resolution
                        let ref_ty = ctx.get_or_create_symbol_type(symbol_id);

                        // If there's only one overload, use it directly
                        if overload_count == 1 {
                            let builtin_ty = ctx.get_builtin_overloads(name.as_str()).unwrap()[0].clone();
                            let range = get_symbol_range(hir, symbol_id);
                            ctx.add_constraint(Constraint::Equal(ref_ty, builtin_ty, range));
                        }
                        // For multiple overloads, the type will be resolved at the call site
                        return;
                    }
                }

                // Normal reference resolution
                let ref_ty = ctx.get_or_create_symbol_type(symbol_id);
                let def_ty = ctx.get_or_create_symbol_type(def_id);
                // Instantiate fresh type variables for function references to enable
                // polymorphic use at different call/reference sites
                let def_ty = if matches!(def_ty, Type::Function(_, _)) {
                    ctx.instantiate_fresh(&def_ty)
                } else {
                    def_ty
                };
                let range = get_symbol_range(hir, symbol_id);
                ctx.add_constraint(Constraint::Equal(ref_ty, def_ty, range));
            } else {
                // No HIR resolution — try builtin registry by name as fallback
                if let Some(symbol) = hir.symbol(symbol_id)
                    && let Some(name) = &symbol.value
                    && ctx.get_builtin_overloads(name.as_str()).is_some()
                {
                    if let Some(piped_ty) = ctx.get_piped_input(symbol_id).cloned() {
                        let arg_tys = vec![piped_ty];
                        if let Some(Type::Function(param_tys, ret_ty)) = ctx.resolve_overload(name.as_str(), &arg_tys) {
                            let range = get_symbol_range(hir, symbol_id);
                            for (arg_ty, param_ty) in arg_tys.iter().zip(param_tys.iter()) {
                                ctx.add_constraint(Constraint::Equal(arg_ty.clone(), param_ty.clone(), range));
                            }
                            ctx.set_symbol_type(symbol_id, ret_ty.as_ref().clone());
                            return;
                        }
                    }

                    let overload_count = ctx.get_builtin_overloads(name.as_str()).map(|o| o.len()).unwrap_or(0);
                    let ref_ty = ctx.get_or_create_symbol_type(symbol_id);
                    if overload_count == 1 {
                        let builtin_ty = ctx.get_builtin_overloads(name.as_str()).unwrap()[0].clone();
                        let range = get_symbol_range(hir, symbol_id);
                        ctx.add_constraint(Constraint::Equal(ref_ty, builtin_ty, range));
                    }
                }
            }
        }

        // Binary operators
        SymbolKind::BinaryOp => {
            if let Some(symbol) = hir.symbol(symbol_id) {
                if let Some(op_name) = &symbol.value {
                    // Get left and right operands
                    let children = get_children(hir, symbol_id);
                    if children.len() >= 2 {
                        let left_ty = ctx.get_or_create_symbol_type(children[0]);
                        let right_ty = ctx.get_or_create_symbol_type(children[1]);
                        let range = get_symbol_range(hir, symbol_id);

                        // Resolve types to get their concrete values if already determined
                        let resolved_left = ctx.resolve_type(&left_ty);
                        let resolved_right = ctx.resolve_type(&right_ty);

                        // Check if any operand is a union type
                        let has_union = resolved_left.is_union() || resolved_right.is_union();

                        // If any operand is still a type variable, defer overload resolution
                        // until after the first round of unification when types may be known
                        if resolved_left.is_var() || resolved_right.is_var() {
                            let ty_var = ctx.fresh_var();
                            ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
                            ctx.add_deferred_overload(DeferredOverload {
                                symbol_id,
                                op_name: SmolStr::new(op_name.as_str()),
                                operand_tys: vec![left_ty, right_ty],
                                range,
                            });
                        } else if has_union {
                            // Union types cannot be used with binary operators
                            // Report an error with a clear message
                            ctx.add_error(crate::TypeError::UnificationError {
                                left: format!(
                                    "{} with arguments ({}, {})",
                                    op_name,
                                    resolved_left.display_renumbered(),
                                    resolved_right.display_renumbered()
                                ),
                                right: "union types cannot be used with binary operators".to_string(),
                                span: range.as_ref().map(range_to_span),
                                location: range.as_ref().map(|r| (r.start.line, r.start.column)),
                            });
                            let ty_var = ctx.fresh_var();
                            ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
                        } else {
                            // Try to resolve the best matching overload
                            let arg_types = vec![resolved_left.clone(), resolved_right.clone()];
                            if let Some(resolved_ty) = ctx.resolve_overload(op_name.as_str(), &arg_types) {
                                // resolved_ty is the matched function type: (T1, T2) -> T3
                                if let Type::Function(param_tys, ret_ty) = resolved_ty {
                                    if param_tys.len() == 2 {
                                        ctx.add_constraint(Constraint::Equal(left_ty, param_tys[0].clone(), range));
                                        ctx.add_constraint(Constraint::Equal(right_ty, param_tys[1].clone(), range));
                                        ctx.set_symbol_type(symbol_id, *ret_ty);
                                    } else {
                                        let ty_var = ctx.fresh_var();
                                        ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
                                    }
                                } else {
                                    let ty_var = ctx.fresh_var();
                                    ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
                                }
                            } else {
                                // No matching overload found - collect error
                                ctx.add_error(crate::TypeError::UnificationError {
                                    left: format!(
                                        "{} with arguments ({}, {})",
                                        op_name,
                                        resolved_left.display_renumbered(),
                                        resolved_right.display_renumbered()
                                    ),
                                    right: "no matching overload".to_string(),
                                    span: range.as_ref().map(range_to_span),
                                    location: range.as_ref().map(|r| (r.start.line, r.start.column)),
                                });
                                let ty_var = ctx.fresh_var();
                                ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
                            }
                        }
                    } else {
                        // Not enough operands
                        let ty_var = ctx.fresh_var();
                        ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
                    }
                } else {
                    // No operator name
                    let ty_var = ctx.fresh_var();
                    ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
                }
            }
        }

        // Unary operators
        SymbolKind::UnaryOp => {
            if let Some(symbol) = hir.symbol(symbol_id) {
                if let Some(op_name) = &symbol.value {
                    let children = get_children(hir, symbol_id);
                    if !children.is_empty() {
                        let operand_ty = ctx.get_or_create_symbol_type(children[0]);
                        let range = get_symbol_range(hir, symbol_id);

                        // Resolve type to get its concrete value if already determined
                        let resolved_operand = ctx.resolve_type(&operand_ty);

                        // If the operand is still a type variable, defer overload resolution
                        if resolved_operand.is_var() {
                            let ty_var = ctx.fresh_var();
                            ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
                            ctx.add_deferred_overload(DeferredOverload {
                                symbol_id,
                                op_name: SmolStr::new(op_name.as_str()),
                                operand_tys: vec![operand_ty],
                                range,
                            });
                        } else {
                            // Try to resolve the best matching overload
                            let arg_types = vec![resolved_operand.clone()];
                            if let Some(resolved_ty) = ctx.resolve_overload(op_name.as_str(), &arg_types) {
                                if let Type::Function(param_tys, ret_ty) = resolved_ty {
                                    if param_tys.len() == 1 {
                                        ctx.add_constraint(Constraint::Equal(operand_ty, param_tys[0].clone(), range));
                                        ctx.set_symbol_type(symbol_id, *ret_ty);
                                    } else {
                                        let ty_var = ctx.fresh_var();
                                        ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
                                    }
                                } else {
                                    let ty_var = ctx.fresh_var();
                                    ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
                                }
                            } else {
                                // No matching overload found - collect error
                                ctx.add_error(crate::TypeError::UnificationError {
                                    left: format!(
                                        "{} with argument ({})",
                                        op_name,
                                        resolved_operand.display_renumbered()
                                    ),
                                    right: "no matching overload".to_string(),
                                    span: range.as_ref().map(range_to_span),
                                    location: range.as_ref().map(|r| (r.start.line, r.start.column)),
                                });
                                let ty_var = ctx.fresh_var();
                                ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
                            }
                        }
                    } else {
                        let ty_var = ctx.fresh_var();
                        ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
                    }
                } else {
                    let ty_var = ctx.fresh_var();
                    ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
                }
            }
        }

        // Function calls
        SymbolKind::Call => {
            // Get the function name from the Call symbol itself
            if let Some(call_symbol) = hir.symbol(symbol_id) {
                if let Some(func_name) = &call_symbol.value {
                    // All children are explicit arguments (filter out Keyword symbols
                    // which are syntax elements like `fn` in lambda expressions)
                    let children: Vec<SymbolId> = get_children(hir, symbol_id)
                        .into_iter()
                        .filter(|&child_id| {
                            hir.symbol(child_id)
                                .map(|s| !matches!(s.kind, SymbolKind::Keyword))
                                .unwrap_or(true)
                        })
                        .collect();
                    let explicit_arg_tys: Vec<Type> = children
                        .iter()
                        .map(|&arg_id| ctx.get_or_create_symbol_type(arg_id))
                        .collect();

                    let range = get_symbol_range(hir, symbol_id);

                    // Try user-defined function first (via HIR reference resolution)
                    if let Some(def_id) = hir.resolve_reference_symbol(symbol_id) {
                        let def_symbol = hir.symbol(def_id);
                        let is_user_defined = def_symbol.map(|s| !hir.is_builtin_symbol(s)).unwrap_or(false);

                        if is_user_defined {
                            let original_func_ty = ctx.get_or_create_symbol_type(def_id);
                            // Instantiate fresh type variables so each call site is independent
                            let func_ty = ctx.instantiate_fresh(&original_func_ty);

                            if let Type::Function(param_tys, ret_ty) = &func_ty {
                                // Try piped input if explicit args don't match arity
                                let arg_tys = if param_tys.len() != explicit_arg_tys.len() {
                                    if let Some(piped_ty) = ctx.get_piped_input(symbol_id).cloned() {
                                        let mut piped_args = vec![piped_ty];
                                        piped_args.extend(explicit_arg_tys.iter().cloned());
                                        piped_args
                                    } else {
                                        explicit_arg_tys.clone()
                                    }
                                } else {
                                    explicit_arg_tys.clone()
                                };

                                // Check arity
                                if param_tys.len() != arg_tys.len() {
                                    // If arity doesn't match and the call might receive piped input later
                                    // (inside a Block), defer the error until Pass 4 re-processing
                                    if !might_receive_piped_input(hir, symbol_id) {
                                        ctx.add_error(crate::TypeError::WrongArity {
                                            expected: param_tys.len(),
                                            found: arg_tys.len(),
                                            span: range.as_ref().map(range_to_span),
                                            location: range.as_ref().map(|r| (r.start.line, r.start.column)),
                                        });
                                    }
                                } else {
                                    // Unify argument types with parameter types
                                    for (arg_ty, param_ty) in arg_tys.iter().zip(param_tys.iter()) {
                                        ctx.add_constraint(Constraint::Equal(arg_ty.clone(), param_ty.clone(), range));
                                    }
                                }
                                ctx.set_symbol_type(symbol_id, ret_ty.as_ref().clone());

                                // Track this call for post-unification resolution.
                                // After unification, the original function's return type
                                // will be concrete, allowing propagation to this call site.
                                ctx.add_deferred_user_call(DeferredUserCall {
                                    call_symbol_id: symbol_id,
                                    def_id,
                                    fresh_param_tys: param_tys.clone(),
                                    fresh_ret_ty: ret_ty.as_ref().clone(),
                                    arg_tys,
                                    range,
                                });
                            } else {
                                // The definition exists but isn't a function type yet
                                let ret_ty = Type::Var(ctx.fresh_var());
                                let expected_func_ty = Type::function(explicit_arg_tys.clone(), ret_ty.clone());
                                ctx.add_constraint(Constraint::Equal(func_ty, expected_func_ty, range));
                                ctx.set_symbol_type(symbol_id, ret_ty);
                            }
                        } else {
                            // Resolved to a builtin - handle via overload resolution
                            // If there's piped input, prepend it as the implicit first argument
                            let arg_tys = build_piped_call_args(ctx, symbol_id, &explicit_arg_tys, func_name);
                            // Defer error if call might receive piped input later (inside a Block)
                            let defer = might_receive_piped_input(hir, symbol_id);
                            resolve_builtin_call(ctx, symbol_id, func_name, &arg_tys, range, defer);
                        }
                    } else {
                        // No HIR resolution - try builtin overload resolution
                        // If there's piped input, prepend it as the implicit first argument
                        let arg_tys = build_piped_call_args(ctx, symbol_id, &explicit_arg_tys, func_name);
                        let defer = might_receive_piped_input(hir, symbol_id);
                        resolve_builtin_call(ctx, symbol_id, func_name, &arg_tys, range, defer);
                    }
                } else {
                    let ty_var = ctx.fresh_var();
                    ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
                }
            } else {
                let ty_var = ctx.fresh_var();
                ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
            }
        }

        // Collections
        SymbolKind::Array => {
            // Array elements should have consistent types when possible.
            // mq is dynamically typed and allows heterogeneous arrays (e.g., [string, number]
            // used as tuples). When elements have different concrete types, we skip
            // unification to avoid cascading false-positive type errors.
            let children = get_children(hir, symbol_id);
            if children.is_empty() {
                // Empty array - element type is a fresh type variable
                let elem_ty_var = ctx.fresh_var();
                let array_ty = Type::array(Type::Var(elem_ty_var));
                ctx.set_symbol_type(symbol_id, array_ty);
            } else {
                // Get types of all elements
                let elem_tys: Vec<Type> = children
                    .iter()
                    .map(|&child_id| ctx.get_or_create_symbol_type(child_id))
                    .collect();

                // Resolve element types to check for heterogeneous concrete types
                let resolved_tys: Vec<Type> = elem_tys.iter().map(|ty| ctx.resolve_type(ty)).collect();
                let concrete_tys: Vec<&Type> = resolved_tys.iter().filter(|ty| !ty.is_var()).collect();

                // Check if concrete types are all the same (homogeneous)
                let is_heterogeneous = concrete_tys.len() >= 2
                    && concrete_tys
                        .windows(2)
                        .any(|w| std::mem::discriminant(w[0]) != std::mem::discriminant(w[1]));

                if is_heterogeneous || concrete_tys.len() != elem_tys.len() {
                    // Heterogeneous or partially-unresolved array (used as tuple) —
                    // use fresh type variable to avoid corrupting type inference
                    // for downstream code. When some elements are still type variables,
                    // we cannot know if they will resolve to compatible types.
                    let elem_ty_var = ctx.fresh_var();
                    let array_ty = Type::array(Type::Var(elem_ty_var));
                    ctx.set_symbol_type(symbol_id, array_ty);
                } else {
                    // Homogeneous or unresolved — unify all element types
                    let elem_ty = elem_tys[0].clone();
                    let range = get_symbol_range(hir, symbol_id);
                    for ty in &elem_tys[1..] {
                        ctx.add_constraint(Constraint::Equal(elem_ty.clone(), ty.clone(), range));
                    }

                    let array_ty = Type::array(elem_ty);
                    ctx.set_symbol_type(symbol_id, array_ty);
                }
            }
        }

        SymbolKind::Dict => {
            // Dict structure in HIR: Dict -> key_symbol -> value_expr
            // Direct children of Dict are the key symbols
            // Note: mq dicts are like JSON objects - values can have different types
            // So we only unify key types, not value types
            let key_symbols = get_children(hir, symbol_id);
            if key_symbols.is_empty() {
                let key_ty_var = ctx.fresh_var();
                let val_ty_var = ctx.fresh_var();
                let dict_ty = Type::dict(Type::Var(key_ty_var), Type::Var(val_ty_var));
                ctx.set_symbol_type(symbol_id, dict_ty);
            } else {
                let key_ty_var = ctx.fresh_var();
                let val_ty_var = ctx.fresh_var();
                let key_ty = Type::Var(key_ty_var);
                let range = get_symbol_range(hir, symbol_id);

                for &key_id in &key_symbols {
                    // Key symbol type - all keys should be the same type
                    let k_ty = ctx.get_or_create_symbol_type(key_id);
                    ctx.add_constraint(Constraint::Equal(key_ty.clone(), k_ty, range));

                    // Process value expressions (to assign types to them)
                    let value_children = get_children(hir, key_id);
                    for &val_id in &value_children {
                        ctx.get_or_create_symbol_type(val_id);
                    }
                }

                let dict_ty = Type::dict(key_ty, Type::Var(val_ty_var));
                ctx.set_symbol_type(symbol_id, dict_ty);
            }
        }

        // Control flow constructs
        SymbolKind::If => {
            let children = get_children(hir, symbol_id);
            if !children.is_empty() {
                let range = get_symbol_range(hir, symbol_id);

                // First child is the condition
                let cond_ty = ctx.get_or_create_symbol_type(children[0]);
                ctx.add_constraint(Constraint::Equal(cond_ty, Type::Bool, range));

                // Subsequent children are then-branch and elif/else branches.
                // mq is dynamically typed: branches may return different types
                // (e.g., `if (...): true elif (...): false else: None`).
                // Use Union types when branches have different concrete types.
                if children.len() > 1 {
                    let then_ty = ctx.get_or_create_symbol_type(children[1]);
                    let mut branch_tys = vec![then_ty.clone()];
                    for &child_id in &children[2..] {
                        let child_ty = resolve_branch_body_type(hir, child_id, ctx);
                        branch_tys.push(child_ty);
                    }

                    // Check if branches have different concrete types
                    let resolved: Vec<Type> = branch_tys.iter().map(|ty| ctx.resolve_type(ty)).collect();
                    let concrete: Vec<&Type> = resolved.iter().filter(|ty| !ty.is_var()).collect();

                    // Check if all concrete types are the same
                    let all_same = if concrete.len() >= 2 {
                        concrete
                            .windows(2)
                            .all(|w| std::mem::discriminant(w[0]) == std::mem::discriminant(w[1]))
                    } else {
                        true
                    };

                    if !all_same && concrete.len() >= 2 {
                        // Different concrete types in branches — use Union type
                        let unique_types: Vec<Type> = concrete.into_iter().cloned().collect();
                        let union_ty = Type::union(unique_types);
                        ctx.set_symbol_type(symbol_id, union_ty);
                    } else {
                        // Homogeneous or unresolved — unify all branch types
                        ctx.set_symbol_type(symbol_id, then_ty.clone());
                        for ty in &branch_tys[1..] {
                            ctx.add_constraint(Constraint::Equal(then_ty.clone(), ty.clone(), range));
                        }
                    }
                } else {
                    let ty_var = ctx.fresh_var();
                    ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
                }
            } else {
                let ty_var = ctx.fresh_var();
                ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
            }
        }

        SymbolKind::Elif | SymbolKind::Else => {
            // Handled inline by the parent If handler via resolve_branch_body_type
            if ctx.get_symbol_type(symbol_id).is_none() {
                let ty_var = ctx.fresh_var();
                ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
            }
        }

        SymbolKind::While => {
            // While loop: condition must be Bool, result type from body
            let children = get_children(hir, symbol_id);
            let range = get_symbol_range(hir, symbol_id);

            if !children.is_empty() {
                // First child is the condition
                let cond_ty = ctx.get_or_create_symbol_type(children[0]);
                ctx.add_constraint(Constraint::Equal(cond_ty, Type::Bool, range));

                // Result type comes from the body (last child after condition)
                if children.len() > 1 {
                    let body_ty = ctx.get_or_create_symbol_type(*children.last().unwrap());
                    ctx.set_symbol_type(symbol_id, body_ty);
                } else {
                    let ty_var = ctx.fresh_var();
                    ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
                }
            } else {
                let ty_var = ctx.fresh_var();
                ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
            }
        }

        SymbolKind::Foreach => {
            // Foreach: iterable should be an array, loop variable gets element type
            let children = get_children(hir, symbol_id);
            let range = get_symbol_range(hir, symbol_id);

            // Children layout: [variable, iterable, body...]
            if children.len() >= 2 {
                let elem_ty_var = ctx.fresh_var();
                let elem_ty = Type::Var(elem_ty_var);

                // The iterable should be an array
                let iterable_ty = ctx.get_or_create_symbol_type(children[1]);
                ctx.add_constraint(Constraint::Equal(iterable_ty, Type::array(elem_ty.clone()), range));

                // The loop variable gets the element type
                let var_ty = ctx.get_or_create_symbol_type(children[0]);
                ctx.add_constraint(Constraint::Equal(var_ty, elem_ty, range));

                // The result type is an array of body values (foreach collects results)
                if children.len() > 2 {
                    let body_ty = ctx.get_or_create_symbol_type(*children.last().unwrap());
                    ctx.set_symbol_type(symbol_id, Type::array(body_ty));
                } else {
                    let ty_var = ctx.fresh_var();
                    ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
                }
            } else {
                let ty_var = ctx.fresh_var();
                ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
            }
        }

        SymbolKind::MatchArm | SymbolKind::Pattern => {
            // These are handled by the Match handler below.
            // Assign a fresh type variable as default.
            let ty_var = ctx.fresh_var();
            ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
        }

        SymbolKind::Match => {
            // All match arms should have the same type
            // Pattern types should be compatible with the match expression type
            let children = get_children(hir, symbol_id);
            let range = get_symbol_range(hir, symbol_id);

            if children.len() > 1 {
                // First child is the match expression, rest are arms
                let match_expr_ty = ctx.get_or_create_symbol_type(children[0]);
                let arm_children: Vec<_> = children[1..].to_vec();

                if !arm_children.is_empty() {
                    let mut arm_body_tys = Vec::new();

                    for &arm_id in &arm_children {
                        let arm_children_inner = get_children(hir, arm_id);

                        // Separate pattern and body children
                        let mut body_ty = None;
                        for &arm_child_id in &arm_children_inner {
                            if let Some(arm_child_symbol) = hir.symbol(arm_child_id) {
                                if matches!(arm_child_symbol.kind, SymbolKind::Pattern) {
                                    // Determine pattern type from its literal children
                                    let pattern_ty = resolve_pattern_type(hir, arm_child_id, ctx);
                                    ctx.add_constraint(Constraint::Equal(match_expr_ty.clone(), pattern_ty, range));
                                } else {
                                    // Track the last non-Pattern child as the body
                                    body_ty = Some(ctx.get_or_create_symbol_type(arm_child_id));
                                }
                            }
                        }

                        if let Some(body_ty) = body_ty {
                            ctx.set_symbol_type(arm_id, body_ty.clone());
                            arm_body_tys.push(body_ty);
                        } else {
                            let arm_ty = ctx.get_or_create_symbol_type(arm_id);
                            arm_body_tys.push(arm_ty);
                        }
                    }

                    // Check if arms have different concrete types (heterogeneous match)
                    let resolved: Vec<Type> = arm_body_tys.iter().map(|ty| ctx.resolve_type(ty)).collect();
                    let concrete: Vec<&Type> = resolved.iter().filter(|ty| !ty.is_var()).collect();

                    // Check if all concrete types are the same
                    let all_same = if concrete.len() >= 2 {
                        concrete
                            .windows(2)
                            .all(|w| std::mem::discriminant(w[0]) == std::mem::discriminant(w[1]))
                    } else {
                        true
                    };

                    if !all_same && concrete.len() >= 2 {
                        // Different concrete types in arms — use Union type
                        let unique_types: Vec<Type> = concrete.into_iter().cloned().collect();
                        let union_ty = Type::union(unique_types);
                        ctx.set_symbol_type(symbol_id, union_ty);
                    } else {
                        let result_ty_var = ctx.fresh_var();
                        let result_ty = Type::Var(result_ty_var);
                        for ty in &arm_body_tys {
                            ctx.add_constraint(Constraint::Equal(result_ty.clone(), ty.clone(), range));
                        }
                        ctx.set_symbol_type(symbol_id, result_ty);
                    }
                } else {
                    let ty_var = ctx.fresh_var();
                    ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
                }
            } else {
                let ty_var = ctx.fresh_var();
                ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
            }
        }

        SymbolKind::Try => {
            // Try/Catch: try body and catch body may have different types in dynamically typed mq
            let children = get_children(hir, symbol_id);
            let range = get_symbol_range(hir, symbol_id);

            if children.len() >= 2 {
                // First child is try body
                let try_ty = ctx.get_or_create_symbol_type(children[0]);

                // Second child may be a Catch symbol wrapping the catch body
                let catch_ty = if let Some(catch_symbol) = hir.symbol(children[1])
                    && matches!(catch_symbol.kind, SymbolKind::Catch)
                {
                    // Look into Catch's children to get the actual catch body type
                    let catch_children = get_children(hir, children[1]);
                    if let Some(&last_child) = catch_children.last() {
                        let body_ty = ctx.get_or_create_symbol_type(last_child);
                        ctx.set_symbol_type(children[1], body_ty.clone());
                        body_ty
                    } else {
                        ctx.get_or_create_symbol_type(children[1])
                    }
                } else {
                    ctx.get_or_create_symbol_type(children[1])
                };

                // Resolve the types to check if they're concrete
                let resolved_try = ctx.resolve_type(&try_ty);
                let resolved_catch = ctx.resolve_type(&catch_ty);
                let both_concrete = !resolved_try.is_var() && !resolved_catch.is_var();
                let same_discriminant =
                    both_concrete && std::mem::discriminant(&resolved_try) == std::mem::discriminant(&resolved_catch);

                if both_concrete && !same_discriminant {
                    // Different concrete types: use Union type to represent both possibilities
                    let union_ty = Type::union(vec![resolved_try, resolved_catch]);
                    ctx.set_symbol_type(symbol_id, union_ty);
                } else {
                    // Same type or at least one is a type variable: unify them
                    ctx.add_constraint(Constraint::Equal(try_ty.clone(), catch_ty, range));
                    ctx.set_symbol_type(symbol_id, try_ty);
                }
            } else if !children.is_empty() {
                let try_ty = ctx.get_or_create_symbol_type(children[0]);
                ctx.set_symbol_type(symbol_id, try_ty);
            } else {
                let ty_var = ctx.fresh_var();
                ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
            }
        }

        // Macro definitions (same type logic as Function)
        SymbolKind::Macro(params) => {
            let param_tys: Vec<Type> = params.iter().map(|_| Type::Var(ctx.fresh_var())).collect();
            let ret_ty = Type::Var(ctx.fresh_var());
            let func_ty = Type::function(param_tys.clone(), ret_ty.clone());
            ctx.set_symbol_type(symbol_id, func_ty);

            // Bind parameter types to their parameter symbols
            let children = get_children(hir, symbol_id);
            let param_children: Vec<SymbolId> = children
                .iter()
                .filter(|&&child_id| {
                    hir.symbol(child_id)
                        .map(|s| matches!(s.kind, SymbolKind::Parameter))
                        .unwrap_or(false)
                })
                .copied()
                .collect();
            for (param_sym, param_ty) in param_children.iter().zip(param_tys.iter()) {
                let sym_ty = ctx.get_or_create_symbol_type(*param_sym);
                let range = get_symbol_range(hir, *param_sym);
                ctx.add_constraint(Constraint::Equal(sym_ty, param_ty.clone(), range));
            }

            // Connect macro body's type to the return type
            let body_children: Vec<SymbolId> = children
                .iter()
                .filter(|&&child_id| {
                    hir.symbol(child_id)
                        .map(|s| !matches!(s.kind, SymbolKind::Parameter))
                        .unwrap_or(false)
                })
                .copied()
                .collect();
            if let Some(&last_body) = body_children.last() {
                let body_ty = ctx.get_or_create_symbol_type(last_body);
                let range = get_symbol_range(hir, symbol_id);
                ctx.add_constraint(Constraint::Equal(ret_ty, body_ty, range));
            }
        }

        // Dynamic function calls
        SymbolKind::CallDynamic => {
            let children = get_children(hir, symbol_id);
            if !children.is_empty() {
                let callable_ty = ctx.get_or_create_symbol_type(children[0]);
                let arg_tys: Vec<Type> = children[1..]
                    .iter()
                    .map(|&arg_id| ctx.get_or_create_symbol_type(arg_id))
                    .collect();
                let range = get_symbol_range(hir, symbol_id);

                // Build the expected function type and unify with the callable
                let ret_ty = Type::Var(ctx.fresh_var());
                let expected_func_ty = Type::function(arg_tys, ret_ty.clone());
                ctx.add_constraint(Constraint::Equal(callable_ty, expected_func_ty, range));
                ctx.set_symbol_type(symbol_id, ret_ty);
            } else {
                let ty_var = ctx.fresh_var();
                ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
            }
        }

        // Selector, QualifiedAccess, and other kinds that don't need deep type checking
        SymbolKind::Selector | SymbolKind::QualifiedAccess => {
            let ty_var = ctx.fresh_var();
            ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
        }

        // Inert kinds: keywords, identifiers, arguments, imports, modules, etc.
        _ => {
            let ty_var = ctx.fresh_var();
            ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constraint_display() {
        let c = Constraint::Equal(Type::Number, Type::String, None);
        assert_eq!(c.to_string(), "number ~ string");
    }
}
