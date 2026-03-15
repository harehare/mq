//! Helper functions for constraint generation.

use mq_hir::{Hir, SymbolId, SymbolKind};
use rustc_hash::{FxHashMap, FxHashSet};
use smol_str::SmolStr;

use crate::infer::{DeferredOverload, InferenceContext};
use crate::types::Type;
use crate::walk_ancestors;

use super::{Constraint, ConstraintOrigin};

/// Walks the HIR parent chain from `symbol_id` and returns the nearest ancestor
/// whose kind is `SymbolKind::Function(_)`, if any.
pub(super) fn find_enclosing_function(hir: &Hir, symbol_id: SymbolId) -> Option<SymbolId> {
    walk_ancestors(hir, symbol_id).find_map(|(id, sym)| if sym.is_function() { Some(id) } else { None })
}

/// Pre-built index mapping each parent symbol to its children.
///
/// Built once at the start of constraint generation to avoid O(n) full HIR scans
/// on every `get_children` call.
pub(crate) type ChildrenIndex = FxHashMap<SymbolId, Vec<SymbolId>>;

/// Builds the children index from all HIR symbols in a single pass.
///
/// Children are sorted by their insertion order so that the order within each
/// child list reflects the source-level order (left-to-right in a pipe chain,
/// etc.).  Using slot-based iteration order instead would break after any
/// `add_nodes` call that reloads a source, because SlotMap reuses freed slots
/// in LIFO order, reversing the apparent position of siblings.
pub(crate) fn build_children_index(hir: &Hir) -> ChildrenIndex {
    let mut index: ChildrenIndex = FxHashMap::default();
    for (id, symbol) in hir.symbols() {
        if let Some(parent) = symbol.parent {
            index.entry(parent).or_default().push(id);
        }
    }
    // Sort each child list by insertion order to restore source-level ordering.
    for children in index.values_mut() {
        children.sort_by_key(|&id| hir.symbol_insertion_order(id));
    }
    index
}

/// Helper function to get children of a symbol from the pre-built index.
pub(crate) fn get_children(children_index: &ChildrenIndex, parent_id: SymbolId) -> &[SymbolId] {
    children_index.get(&parent_id).map(|v| v.as_slice()).unwrap_or(&[])
}

/// Returns the non-keyword children of a symbol.
///
/// Keyword symbols (e.g. `fn` in lambda expressions) are syntax elements that
/// should not be treated as arguments or operands. This helper filters them out
/// and is used wherever argument lists are extracted from Call symbols.
pub(crate) fn get_non_keyword_children(
    hir: &Hir,
    symbol_id: SymbolId,
    children_index: &ChildrenIndex,
) -> Vec<SymbolId> {
    get_children(children_index, symbol_id)
        .iter()
        .copied()
        .filter(|&child_id| {
            hir.symbol(child_id)
                .map(|s| !matches!(s.kind, SymbolKind::Keyword))
                .unwrap_or(true)
        })
        .collect()
}

/// Helper function to get the range of a symbol
pub(super) fn get_symbol_range(hir: &Hir, symbol_id: SymbolId) -> Option<mq_lang::Range> {
    hir.symbol(symbol_id).and_then(|symbol| symbol.source.text_range)
}

/// Checks if a symbol belongs to a module source (include/import/module).
///
/// Symbols from included/imported modules are trusted library code and should
/// not be type-checked, similar to builtin symbols.
pub(super) fn is_module_symbol(
    _hir: &Hir,
    symbol: &mq_hir::Symbol,
    module_source_ids: &FxHashSet<mq_hir::SourceId>,
) -> bool {
    symbol
        .source
        .source_id
        .is_some_and(|sid| module_source_ids.contains(&sid))
}

/// Checks if a symbol is a foreach iterable reference.
///
/// In the HIR, `foreach (var, iterable): body;` is represented as:
/// - Foreach has children: [Variable(item), Ref(iterable), body_expr...]
/// - The iterable is a `Ref` direct child of `Foreach`
///
/// These Refs should not receive piped input since they are iterable function
/// references whose arguments are not represented in the HIR.
pub(super) fn is_foreach_iterable_ref(hir: &Hir, symbol_id: SymbolId) -> bool {
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
    // Check if the direct parent is a Foreach symbol
    hir.symbol(parent_id)
        .map(|s| matches!(s.kind, SymbolKind::Foreach))
        .unwrap_or(false)
}

/// Maps a Markdown node attribute kind to its concrete return type.
///
/// - String attributes: value, lang, meta, fence, url, alt, title, ident, label, align, name
/// - Number attributes: depth, level, index, column, row
/// - Bool attributes: ordered, checked
/// - Markdown array attributes: values, children
pub(crate) fn attr_kind_to_type(attr_kind: &mq_lang::AttrKind) -> Type {
    use mq_lang::AttrKind;
    match attr_kind {
        AttrKind::Value
        | AttrKind::Lang
        | AttrKind::Meta
        | AttrKind::Fence
        | AttrKind::Url
        | AttrKind::Alt
        | AttrKind::Title
        | AttrKind::Ident
        | AttrKind::Label
        | AttrKind::Align
        | AttrKind::Name => Type::String,
        AttrKind::Depth | AttrKind::Level | AttrKind::Index | AttrKind::Column | AttrKind::Row => Type::Number,
        AttrKind::Ordered | AttrKind::Checked => Type::Bool,
        AttrKind::Values | AttrKind::Children => Type::array(Type::Markdown),
    }
}

/// Checks if a symbol might receive piped input later (i.e., is inside a Block
/// or a Function/Macro body with multiple expressions).
///
/// Also handles the case where a Call/Ref is inside a UnaryOp that is itself
/// inside a Block, e.g., `x | !f()` where `f()` eventually receives piped input.
pub(super) fn might_receive_piped_input(hir: &Hir, symbol_id: SymbolId) -> bool {
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
        SymbolKind::Block | SymbolKind::Function(_) | SymbolKind::Macro(_) | SymbolKind::Call
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

/// Checks if a symbol is nested inside a `quote do...end` or `unquote(...)` block.
///
/// Code inside `quote do...end` is template/meta-code that generates AST at runtime
/// rather than being directly executed. Type errors inside such blocks should be
/// suppressed because `unquote(expr)` splices in expressions from macro parameters
/// whose types are only known at the macro call site.
pub(super) fn is_inside_quote_block(hir: &Hir, symbol_id: SymbolId) -> bool {
    let mut current_id = symbol_id;
    loop {
        let parent_id = match hir.symbol(current_id).and_then(|s| s.parent) {
            Some(id) => id,
            None => return false,
        };
        if let Some(parent) = hir.symbol(parent_id) {
            // Quote and unquote keywords have value = None; other keywords (break, self)
            // have named values and are not quote blocks.
            if matches!(parent.kind, SymbolKind::Keyword) && parent.value.is_none() {
                return true;
            }
        }
        current_id = parent_id;
    }
}

/// Builds the argument type list for a piped builtin function call.
///
/// When a function is called via pipe (e.g., `arr | join(",")`) the piped value
/// becomes the implicit first argument. This function checks if there's a piped input
/// and whether prepending it produces a valid overload match. If so, the piped input
/// is prepended; otherwise, only the explicit arguments are returned.
pub(super) fn build_piped_call_args(
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
pub(super) fn resolve_builtin_call(
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
            for (arg_index, (arg_ty, param_ty)) in arg_tys.iter().zip(param_tys.iter()).enumerate() {
                ctx.add_constraint(Constraint::Equal(
                    arg_ty.clone(),
                    param_ty.clone(),
                    range,
                    ConstraintOrigin::Argument {
                        fn_name: SmolStr::new(func_name),
                        arg_index,
                    },
                ));
            }
            ctx.set_symbol_type(symbol_id, ret_ty.as_ref().clone());
        } else {
            let ty_var = ctx.fresh_var();
            ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
        }
    } else if is_builtin && !defer_error {
        ctx.report_no_matching_overload(func_name, &resolved_arg_tys, range);
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
pub(super) fn resolve_pattern_type(
    hir: &Hir,
    pattern_id: SymbolId,
    ctx: &mut InferenceContext,
    children_index: &ChildrenIndex,
) -> Type {
    let children = get_children(children_index, pattern_id);
    for &child_id in children {
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

/// Collects the types of all `break: value` expressions that directly belong to
/// the given loop symbol (i.e., not nested inside an inner while/loop/foreach).
///
/// Only `break: expr` (with a value) contributes to the union type; bare `break`
/// without a value is ignored because it falls through to the loop's normal exit type.
///
/// When a `break: value` is found inside an `if` that has no explicit `else` branch,
/// a fresh type variable is added to represent the implicit else (pass-through) path.
/// This models the fact that when the condition is false the loop body returns the
/// current piped value (of an unknown type), so the loop's exit type must be a union.
pub(super) fn collect_break_value_types(
    hir: &Hir,
    loop_symbol_id: SymbolId,
    ctx: &mut InferenceContext,
    children_index: &ChildrenIndex,
) -> Vec<Type> {
    let mut types = Vec::new();
    for &child_id in get_children(children_index, loop_symbol_id) {
        collect_break_types_inner(hir, child_id, ctx, &mut types, children_index);
    }
    types
}

/// Recursive helper for `collect_break_value_types`.
///
/// Returns `true` if at least one `break: value` was found during the traversal
/// of `symbol_id`'s subtree (used by the `If` arm to decide whether to add an
/// implicit pass-through variable).
fn collect_break_types_inner(
    hir: &Hir,
    symbol_id: SymbolId,
    ctx: &mut InferenceContext,
    result: &mut Vec<Type>,
    children_index: &ChildrenIndex,
) -> bool {
    let Some(symbol) = hir.symbol(symbol_id) else {
        return false;
    };
    // Do not descend into nested loops; their breaks belong to them, not the outer loop.
    if matches!(symbol.kind, SymbolKind::While | SymbolKind::Loop | SymbolKind::Foreach) {
        return false;
    }
    if matches!(symbol.kind, SymbolKind::Keyword) && symbol.value.as_deref() == Some("break") {
        // Only include `break: value` (has a value child), not bare `break`.
        let children = get_children(children_index, symbol_id);
        if !children.is_empty() {
            result.push(ctx.get_or_create_symbol_type(symbol_id));
            return true;
        }
        return false; // bare break — no value, never recurse
    }
    if matches!(symbol.kind, SymbolKind::If) {
        let children = get_children(children_index, symbol_id);
        // An `if` without an explicit `else` child implicitly passes the input through
        // when the condition is false.  Record whether any break was found inside so we
        // can add a fresh type variable for that path.
        let has_explicit_else = children
            .iter()
            .any(|&id| hir.symbol(id).is_some_and(|s| matches!(s.kind, SymbolKind::Else)));

        let mut found_break = false;
        for &child_id in children {
            if collect_break_types_inner(hir, child_id, ctx, result, children_index) {
                found_break = true;
            }
        }
        // When there is no else and a break was found inside this if, the "condition false"
        // path returns None (no else in mq returns None).  Add None to the break type
        // list so the loop type becomes Union(break_value_type, None).
        if !has_explicit_else && found_break {
            result.push(Type::None);
        }
        return found_break;
    }
    let mut found_break = false;
    for &child_id in get_children(children_index, symbol_id) {
        if collect_break_types_inner(hir, child_id, ctx, result, children_index) {
            found_break = true;
        }
    }
    found_break
}

/// Merges a base type with a list of `break` value types into a union when the
/// concrete types differ, or leaves the base type unchanged when they agree.
///
/// When the concrete types are all the same but a type variable is also present
/// (e.g., from an `if`-without-`else` implicit pass-through), the result is a
/// union of the concrete type with the variable so downstream code can detect
/// that the loop might also produce an unknown type.
pub(super) fn merge_loop_types(base_ty: Type, break_tys: Vec<Type>, ctx: &InferenceContext) -> Type {
    if break_tys.is_empty() {
        return base_ty;
    }
    let mut all_tys: Vec<Type> = Vec::with_capacity(1 + break_tys.len());
    all_tys.push(base_ty);
    all_tys.extend(break_tys);

    let resolved: Vec<Type> = all_tys.iter().map(|ty| ctx.resolve_type(ty)).collect();
    let concrete: Vec<&Type> = resolved.iter().filter(|ty| !ty.is_var()).collect();
    let var_ty: Option<&Type> = resolved.iter().find(|ty| ty.is_var());

    if concrete.len() >= 2 {
        let all_same = concrete
            .windows(2)
            .all(|w| std::mem::discriminant(w[0]) == std::mem::discriminant(w[1]));
        if !all_same {
            // Different concrete types → Union (vars are not included to avoid overly
            // broad types when the concrete information is already sufficient).
            let unique: Vec<Type> = concrete.into_iter().cloned().collect();
            return Type::union(unique);
        }
        // All same concrete type: include the Var if present so the loop type
        // reflects the implicit else (pass-through) path.
        if let Some(var) = var_ty {
            return Type::union(vec![concrete[0].clone(), var.clone()]);
        }
        return concrete[0].clone();
    }

    // Exactly one concrete type with an implicit-else Var → Union(concrete, Var)
    if concrete.len() == 1 {
        if let Some(var) = var_ty {
            return Type::union(vec![concrete[0].clone(), var.clone()]);
        }
        return concrete[0].clone();
    }

    // All unresolved type variables: return the base type unchanged and let
    // unification constraints handle the rest.
    all_tys.into_iter().next().unwrap()
}

/// Returns the sibling symbol IDs that come after `while_id` in its parent's child list.
///
/// These are the symbols that execute after the while loop exits (i.e., when the loop
/// condition becomes false), allowing post-loop type narrowing.
pub(super) fn get_post_loop_siblings(hir: &Hir, while_id: SymbolId, children_index: &ChildrenIndex) -> Vec<SymbolId> {
    let parent_id = match hir.symbol(while_id).and_then(|s| s.parent) {
        Some(p) => p,
        None => return Vec::new(),
    };
    let siblings = get_children(children_index, parent_id);
    let pos = match siblings.iter().position(|&id| id == while_id) {
        Some(p) => p,
        None => return Vec::new(),
    };
    siblings[pos + 1..].to_vec()
}

/// Finds the lambda Function symbol that a Variable was initialized with, if any.
///
/// For `let f = fn(x): x - 1;`, the Variable `f` has a Function child (the lambda).
/// Returns the SymbolId of that Function child, enabling call-site type checking
/// for calls like `f("str")` that go through a variable holding a lambda.
pub(super) fn find_lambda_function_child(
    hir: &Hir,
    var_id: SymbolId,
    children_index: &ChildrenIndex,
) -> Option<SymbolId> {
    get_children(children_index, var_id).iter().find_map(|&child_id| {
        hir.symbol(child_id)
            .filter(|s| matches!(s.kind, SymbolKind::Function(_)))
            .map(|_| child_id)
    })
}
