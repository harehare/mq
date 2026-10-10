//! Helper functions for constraint generation.

use mq_hir::{Hir, SymbolId, SymbolKind};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::builtin::PARTIAL;
use crate::infer::InferenceContext;
use crate::types::Type;
use crate::walk_ancestors;

use super::call::{BuiltinCall, CallKind, resolve_builtin};
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

/// Returns the element type an array child contributes to its enclosing `[...]`.
///
/// For a plain element this is just its own type. For a `...expr` spread element
/// (`SymbolKind::Spread`), the spread target must itself be an array, so its
/// *element* type — not the array type itself — is what should be compared
/// against sibling elements (e.g. `[0, ...a, 99]` with `a: [number]` should be
/// seen as homogeneous `number` elements, not a 3-slot tuple).
pub(super) fn spread_element_type(
    hir: &Hir,
    child_id: SymbolId,
    children_index: &ChildrenIndex,
    ctx: &mut InferenceContext,
) -> Type {
    if !hir.symbol(child_id).is_some_and(|s| s.kind == SymbolKind::Spread) {
        return ctx.get_or_create_symbol_type(child_id);
    }

    let target_ty = get_children(children_index, child_id)
        .first()
        .map(|&id| ctx.get_or_create_symbol_type(id))
        .unwrap_or_else(|| Type::Var(ctx.fresh_var()));
    let elem_var = ctx.fresh_var();
    let range = get_symbol_range(hir, child_id);
    ctx.add_constraint(Constraint::Equal(
        target_ty,
        Type::array(Type::Var(elem_var)),
        range,
        ConstraintOrigin::General,
    ));

    Type::Var(elem_var)
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
/// - Number attributes: depth, level, index, column, row, line, end_line
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
        | AttrKind::Name
        | AttrKind::Kind => Type::String,
        AttrKind::Depth
        | AttrKind::Level
        | AttrKind::Index
        | AttrKind::Column
        | AttrKind::Row
        | AttrKind::Line
        | AttrKind::EndLine => Type::Number,
        AttrKind::Ordered | AttrKind::Checked => Type::Bool,
        AttrKind::Values | AttrKind::Children => Type::array(Type::markdown()),
    }
}

/// Checks if a symbol might receive piped input later (i.e., is inside a Block
/// or a Function body with multiple expressions).
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
        SymbolKind::Block | SymbolKind::Function(_) | SymbolKind::Call
    ) {
        return true;
    }
    // Check grandparent: if parent is UnaryOp inside a pipe-capable construct
    if matches!(parent.kind, SymbolKind::UnaryOp)
        && let Some(grandparent_id) = parent.parent
        && let Some(grandparent) = hir.symbol(grandparent_id)
    {
        return matches!(grandparent.kind, SymbolKind::Block | SymbolKind::Function(_));
    }
    // Check grandparent: if parent is Variable (let binding) inside a pipe-capable construct,
    // e.g. `items | let x = first()` — the Call inside the Variable will receive piped
    // input in Pass 4 via propagate_piped_input_to_variable_initializer, so defer errors.
    if matches!(parent.kind, SymbolKind::Variable | SymbolKind::DestructuringBinding)
        && let Some(grandparent_id) = parent.parent
        && let Some(grandparent) = hir.symbol(grandparent_id)
    {
        return matches!(grandparent.kind, SymbolKind::Block | SymbolKind::Function(_));
    }
    false
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

/// The arguments of `partial`: its function, then the values bound to it. A single argument is
/// always a bound value and the piped input is the function. With more, the first argument is the
/// function unless it is not one, in which case the piped input is.
///
/// While the first argument is not settled, the piped input is returned as well, for the
/// deferred resolution to decide.
fn build_partial_args(
    ctx: &mut InferenceContext,
    symbol_id: SymbolId,
    explicit_arg_tys: &[Type],
) -> (Vec<Type>, Option<Type>) {
    let piped = ctx.get_piped_input(symbol_id).cloned();
    let piped = match explicit_arg_tys {
        [] => return (Vec::new(), None),
        [_] => piped.unwrap_or_else(|| Type::Var(ctx.fresh_var())),
        [first, ..] => {
            let first = ctx.resolve_type(first);
            match piped {
                Some(piped) if first.is_pending_operand() => return (explicit_arg_tys.to_vec(), Some(piped)),
                Some(piped) if !matches!(first, Type::Function(..)) => piped,
                _ => return (explicit_arg_tys.to_vec(), None),
            }
        }
    };
    (
        std::iter::once(piped).chain(explicit_arg_tys.iter().cloned()).collect(),
        None,
    )
}

/// Resolves a call `f(args)` of a builtin and assigns the result type to `symbol_id`.
///
/// If `may_get_piped_input` is true (e.g., the call is an argument that is applied to each
/// element), a mismatch is reported only when no piped input could make the call match.
pub(super) fn resolve_builtin_call(
    ctx: &mut InferenceContext,
    symbol_id: SymbolId,
    func_name: &str,
    arg_tys: &[Type],
    range: Option<mq_lang::Range>,
    may_get_piped_input: bool,
) -> Type {
    let mut call = BuiltinCall::new(symbol_id, func_name, arg_tys, range, CallKind::Function);
    call.may_get_piped_input = may_get_piped_input;
    let result = resolve_builtin(ctx, &call).into_type();
    ctx.set_symbol_type(symbol_id, result.clone());
    result
}

/// Resolves a builtin call, splitting off and chaining any trailing bracket-access
/// keys (`first(xs)[:ident]`) via `DeferredCallReturnAccess`.
pub(super) fn resolve_builtin_call_with_brackets(
    hir: &Hir,
    ctx: &mut InferenceContext,
    symbol_id: SymbolId,
    func_name: &str,
    explicit_arg_tys: &[Type],
    children: &[SymbolId],
    range: Option<mq_lang::Range>,
) {
    let trailing_bracket_count = hir.bracket_key_count(symbol_id).min(explicit_arg_tys.len());
    let real_arg_tys = &explicit_arg_tys[..explicit_arg_tys.len() - trailing_bracket_count];
    let (arg_tys, unsettled_piped) = if func_name == PARTIAL {
        build_partial_args(ctx, symbol_id, real_arg_tys)
    } else {
        (build_piped_call_args(ctx, symbol_id, real_arg_tys, func_name), None)
    };

    let mut call = BuiltinCall::new(symbol_id, func_name, &arg_tys, range, CallKind::Function);
    call.may_get_piped_input = might_receive_piped_input(hir, symbol_id);
    call.unsettled_piped = unsettled_piped;
    let result_ty = resolve_builtin(ctx, &call).into_type();
    ctx.set_symbol_type(symbol_id, result_ty.clone());
    if trailing_bracket_count == 0 {
        return;
    }

    let current_ty = chain_bracket_accesses(hir, ctx, children, trailing_bracket_count, result_ty, range);
    ctx.set_symbol_type(symbol_id, current_ty);
}

/// The type of `f(x)[k1][k2]...`: each trailing bracket key of the call, which are the last
/// `key_count` of its `children`, accesses a field of the previous result. The accesses are
/// resolved once the type of the call result is known.
pub(super) fn chain_bracket_accesses(
    hir: &Hir,
    ctx: &mut InferenceContext,
    children: &[SymbolId],
    key_count: usize,
    result_ty: Type,
    range: Option<mq_lang::Range>,
) -> Type {
    let mut current_ty = result_ty;
    for &key_id in &children[children.len() - key_count..] {
        let field_name = hir
            .symbol(key_id)
            .and_then(|s| s.value.as_ref().map(|v| v.to_string()))
            .unwrap_or_default();
        let next_ty = Type::Var(ctx.fresh_var());
        ctx.add_deferred_call_return_access(crate::infer::DeferredCallReturnAccess {
            return_type: current_ty,
            result_ty: next_ty.clone(),
            field_name,
            range,
        });
        current_ty = next_ty;
    }
    current_ty
}

/// Returns the type if the pattern matches an entire type class (safe to subtract cross-arm).
/// None literals and type-label symbols qualify; literal patterns like "foo" or 42 return None.
pub(super) fn resolve_whole_type_pattern(
    hir: &Hir,
    pattern_id: SymbolId,
    ctx: &mut InferenceContext,
    children_index: &ChildrenIndex,
) -> Option<Type> {
    for &child_id in get_children(children_index, pattern_id) {
        if let Some(child_sym) = hir.symbol(child_id) {
            match child_sym.kind {
                SymbolKind::None => return Some(Type::None),
                SymbolKind::Symbol => {
                    return child_sym
                        .value
                        .as_deref()
                        .and_then(|n| crate::narrowing::type_name_to_type(n, ctx));
                }
                _ => {}
            }
        }
    }

    None
}

/// Determines the type of a Pattern symbol from its literal children.
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
                SymbolKind::Symbol => {
                    // A Symbol child in a Pattern may be a type-label pattern (e.g. `:array:`,
                    // `:string:`). If the value matches a known type name, return that type.
                    // Otherwise fall back to Type::Symbol for a plain symbol pattern.
                    let ty = child_symbol
                        .value
                        .as_deref()
                        .and_then(|name| crate::narrowing::type_name_to_type(name, ctx));
                    return ty.unwrap_or(Type::Symbol);
                }
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
/// Collects all `PatternVariable` symbol IDs that are descendants of `root_id`.
///
/// Used by `DestructuringBinding` constraint generation to wire each bound
/// pattern variable to the array element type of the initializer.
pub(super) fn collect_pattern_variable_descendants(
    hir: &Hir,
    root_id: SymbolId,
    children_index: &ChildrenIndex,
) -> Vec<SymbolId> {
    let mut result = Vec::new();
    collect_pattern_variables_inner(hir, root_id, children_index, &mut result);
    result
}

fn collect_pattern_variables_inner(
    hir: &Hir,
    symbol_id: SymbolId,
    children_index: &ChildrenIndex,
    result: &mut Vec<SymbolId>,
) {
    for &child_id in get_children(children_index, symbol_id) {
        if let Some(sym) = hir.symbol(child_id) {
            if matches!(sym.kind, SymbolKind::PatternVariable { .. }) {
                result.push(child_id);
            } else {
                collect_pattern_variables_inner(hir, child_id, children_index, result);
            }
        }
    }
}

/// `break: expr` (with a value) contributes its type to the union. A bare `break`
/// contributes `none` when `bare_break_yields_none` is set (`loop`, `while`). A `foreach`
/// returns the results collected so far on a bare `break`, so it passes `false` and the
/// bare `break` falls through to the loop's normal exit type.
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
    bare_break_yields_none: bool,
) -> Vec<Type> {
    let mut types = Vec::new();
    for &child_id in get_children(children_index, loop_symbol_id) {
        collect_break_types_inner(hir, child_id, ctx, &mut types, children_index, bare_break_yields_none);
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
    bare_break_yields_none: bool,
) -> bool {
    let Some(symbol) = hir.symbol(symbol_id) else {
        return false;
    };
    // Do not descend into nested loops; their breaks belong to them, not the outer loop.
    if matches!(symbol.kind, SymbolKind::While | SymbolKind::Loop | SymbolKind::Foreach) {
        return false;
    }
    if matches!(symbol.kind, SymbolKind::Keyword) && symbol.value.as_deref() == Some("break") {
        let children = get_children(children_index, symbol_id);
        if !children.is_empty() {
            // `break: value` — carries the value's type.
            result.push(ctx.get_or_create_symbol_type(symbol_id));
        } else if bare_break_yields_none {
            // bare `break` (no value) — the loop exits returning `none`.
            result.push(Type::None);
        } else {
            return false;
        }
        return true;
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
            if collect_break_types_inner(hir, child_id, ctx, result, children_index, bare_break_yields_none) {
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
        if collect_break_types_inner(hir, child_id, ctx, result, children_index, bare_break_yields_none) {
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

/// Whether two record types have a field of the same name whose types can never be the same,
/// such as `{a: none}` and `{a: {b: 1}}`. Unresolved variables never conflict.
///
/// `is_heterogeneous` compares only the outermost type constructor, which cannot tell these
/// two apart; unifying them would report an error, unlike mixed element types such as
/// `[none, 1]`.
pub(super) fn records_have_conflicting_fields(a: &Type, b: &Type, ctx: &InferenceContext) -> bool {
    fn conflict(a: &Type, b: &Type, ctx: &InferenceContext) -> bool {
        let (a, b) = (ctx.resolve_type(a), ctx.resolve_type(b));
        match (&a, &b) {
            (Type::Record(fields_a, _), Type::Record(fields_b, _)) => fields_a
                .iter()
                .any(|(name, ty_a)| fields_b.get(name).is_some_and(|ty_b| conflict(ty_a, ty_b, ctx))),
            (Type::Array(elem_a), Type::Array(elem_b)) => conflict(elem_a, elem_b, ctx),
            (Type::Array(elem), Type::Tuple(items)) | (Type::Tuple(items), Type::Array(elem)) => {
                items.iter().any(|item| conflict(item, elem, ctx))
            }
            (Type::Tuple(items_a), Type::Tuple(items_b)) => {
                items_a.len() != items_b.len() || items_a.iter().zip(items_b).any(|(x, y)| conflict(x, y, ctx))
            }
            (Type::Var(_), _) | (_, Type::Var(_)) | (Type::Dynamic, _) | (_, Type::Dynamic) => false,
            // Unions and other compound types are left to unification.
            (Type::Union(_), _) | (_, Type::Union(_)) => false,
            (Type::Dict(..) | Type::Function(..), _) | (_, Type::Dict(..) | Type::Function(..)) => false,
            _ => std::mem::discriminant(&a) != std::mem::discriminant(&b),
        }
    }

    matches!((a, b), (Type::Record(..), Type::Record(..))) && conflict(a, b, ctx)
}
