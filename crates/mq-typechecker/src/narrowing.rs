//! Type narrowing analysis and resolution for mq's type checker.
//!
//! This module handles flow-sensitive type narrowing: narrowing variable types
//! within branches based on type predicates in conditions.
//!
//! # Overview
//!
//! Type narrowing works in two phases:
//!
//! 1. **Analysis** (during constraint generation): `analyze_condition` inspects
//!    condition expressions and extracts `ConditionNarrowings` — lists of which
//!    variables should be narrowed to which types in the then/else branches.
//!
//! 2. **Resolution** (post-unification): `resolve_type_narrowings` applies the
//!    collected narrowings to `Ref` symbols within the appropriate branches,
//!    overriding their inferred types.
//!
//! # Supported Patterns
//!
//! - Type predicates: `is_string(x)`, `is_number(x)`, `is_bool(x)`, …
//! - Literal equality: `x == "foo"` (then-branch: String), `x != none` (removes None)
//! - Type name check: `type(x) == "string"` → String
//! - Negation: `!is_string(x)` → swaps then/else
//! - AND: `is_string(x) && is_bool(y)` → both in then-branch
//! - OR (same variable): `is_string(x) || is_number(x)` → then: String|Number

use crate::constraint::{ChildrenIndex, get_children};
use crate::infer::{InferenceContext, NarrowingEntry};
use crate::types::Type;
use crate::walk_ancestors;
use mq_hir::{Hir, SymbolId, SymbolKind};
use rustc_hash::{FxHashMap, FxHashSet};

/// Narrowings extracted from a condition expression.
///
/// Contains separate narrowing lists for the then-branch (condition is true)
/// and else-branch (condition is false).
pub(crate) struct ConditionNarrowings {
    pub(crate) then_narrowings: Vec<NarrowingEntry>,
    pub(crate) else_narrowings: Vec<NarrowingEntry>,
}

impl ConditionNarrowings {
    /// Returns `true` if there are no narrowings in either branch.
    pub(crate) fn is_empty(&self) -> bool {
        self.then_narrowings.is_empty() && self.else_narrowings.is_empty()
    }

    /// Returns an empty `ConditionNarrowings` with no narrowings in either branch.
    pub(crate) fn empty() -> Self {
        Self {
            then_narrowings: Vec::new(),
            else_narrowings: Vec::new(),
        }
    }
}

/// Analyzes a single type predicate call (e.g., `is_string(x)`) and returns
/// the variable definition ID and the narrowed type if the pattern matches.
///
/// Recognized predicates and their narrowed types:
/// - `is_string(x)` → `String`
/// - `is_number(x)` → `Number`
/// - `is_bool(x)` → `Bool`
/// - `is_none(x)` → `None`
/// - `is_symbol(x)` → `Symbol`
/// - `is_array(x)` → `Array('a)`
/// - `is_dict(x)` → `Dict('k, 'v)`
/// - `is_markdown(x)`, `is_h(x)`, `is_p(x)`, … → `Markdown`
pub(crate) fn analyze_type_predicate_call(
    hir: &Hir,
    call_id: SymbolId,
    children_index: &ChildrenIndex,
    ctx: &mut InferenceContext,
) -> Option<(SymbolId, Type)> {
    let symbol = hir.symbol(call_id)?;
    if !matches!(symbol.kind, SymbolKind::Call) {
        return None;
    }
    let name = symbol.value.as_ref()?;

    let narrowed_type = match name.as_str() {
        "is_string" => Type::String,
        "is_number" => Type::Number,
        "is_bool" => Type::Bool,
        "is_none" => Type::None,
        "is_symbol" => Type::Symbol,
        "is_array" => {
            let elem = ctx.fresh_var();
            Type::array(Type::Var(elem))
        }
        "is_dict" => {
            let k = ctx.fresh_var();
            let v = ctx.fresh_var();
            Type::dict(Type::Var(k), Type::Var(v))
        }
        // All Markdown structural predicates narrow to Markdown
        "is_markdown" | "is_h" | "is_h1" | "is_h2" | "is_h3" | "is_h4" | "is_h5" | "is_h6" | "is_p" | "is_code"
        | "is_code_inline" | "is_code_block" | "is_em" | "is_strong" | "is_link" | "is_image" | "is_list"
        | "is_list_item" | "is_table" | "is_table_row" | "is_table_cell" | "is_blockquote" | "is_hr" | "is_html"
        | "is_text" | "is_softbreak" | "is_hardbreak" | "is_task_list_item" | "is_footnote" | "is_footnote_ref"
        | "is_strikethrough" | "is_math" | "is_math_inline" | "is_toml" | "is_yaml" => Type::Markdown,
        _ => return None,
    };

    // The argument must be a single variable reference
    let children: Vec<SymbolId> = get_children(children_index, call_id)
        .iter()
        .copied()
        .filter(|&child_id| {
            hir.symbol(child_id)
                .map(|s| !matches!(s.kind, SymbolKind::Keyword))
                .unwrap_or(true)
        })
        .collect();

    if children.len() != 1 {
        return None;
    }

    let arg_id = children[0];
    let arg_symbol = hir.symbol(arg_id)?;
    if !matches!(arg_symbol.kind, SymbolKind::Ref) {
        return None;
    }

    let def_id = hir.resolve_reference_symbol(arg_id)?;
    Some((def_id, narrowed_type))
}

/// Maps the runtime type-name string (returned by the `type()` builtin) to its `Type`.
///
/// Used for narrowing `type(x) == "string"` conditions.
pub(crate) fn type_name_to_type(name: &str, ctx: &mut InferenceContext) -> Option<Type> {
    match name {
        "string" => Some(Type::String),
        "number" => Some(Type::Number),
        "bool" => Some(Type::Bool),
        "none" => Some(Type::None),
        "symbol" => Some(Type::Symbol),
        "markdown" => Some(Type::Markdown),
        "array" => {
            let elem = ctx.fresh_var();
            Some(Type::array(Type::Var(elem)))
        }
        "dict" => {
            let k = ctx.fresh_var();
            let v = ctx.fresh_var();
            Some(Type::dict(Type::Var(k), Type::Var(v)))
        }
        _ => None,
    }
}

/// Returns the narrowed `Type` if `lit_id` is a literal symbol whose kind
/// maps unambiguously to a single `Type` (String, Number, Bool, Symbol, None).
pub(crate) fn literal_symbol_type(hir: &Hir, lit_id: SymbolId) -> Option<Type> {
    match hir.symbol(lit_id)?.kind {
        SymbolKind::String => Some(Type::String),
        SymbolKind::Number => Some(Type::Number),
        SymbolKind::Boolean => Some(Type::Bool),
        SymbolKind::Symbol => Some(Type::Symbol),
        SymbolKind::None => Some(Type::None),
        _ => None,
    }
}

/// Tries to extract `(def_id, narrowed_type)` from `type(x) == "typename"`.
///
/// `call_id` must be the `Call("type", x)` symbol and `lit_id` must be the
/// String literal `"typename"`.  Returns `None` if the pattern doesn't match.
pub(crate) fn analyze_type_call_equality(
    hir: &Hir,
    call_id: SymbolId,
    lit_id: SymbolId,
    children_index: &ChildrenIndex,
    ctx: &mut InferenceContext,
) -> Option<(SymbolId, Type)> {
    let call_sym = hir.symbol(call_id)?;
    if !matches!(call_sym.kind, SymbolKind::Call) {
        return None;
    }
    if call_sym.value.as_deref() != Some("type") {
        return None;
    }

    // The argument to type() must be a single Ref (variable reference)
    let call_args: Vec<SymbolId> = get_children(children_index, call_id)
        .iter()
        .copied()
        .filter(|&c| {
            hir.symbol(c)
                .map(|s| !matches!(s.kind, SymbolKind::Keyword))
                .unwrap_or(true)
        })
        .collect();
    if call_args.len() != 1 {
        return None;
    }
    let arg_id = call_args[0];
    if !matches!(hir.symbol(arg_id)?.kind, SymbolKind::Ref) {
        return None;
    }

    // The other side must be a String literal naming the type
    let lit_sym = hir.symbol(lit_id)?;
    if !matches!(lit_sym.kind, SymbolKind::String) {
        return None;
    }
    let type_name = lit_sym.value.as_deref()?;
    let narrowed_type = type_name_to_type(type_name, ctx)?;
    let def_id = hir.resolve_reference_symbol(arg_id)?;
    Some((def_id, narrowed_type))
}

/// Tries to extract `(def_id, narrowed_type)` from `x == literal` or `literal == x`.
pub(crate) fn analyze_literal_equality(hir: &Hir, lhs: SymbolId, rhs: SymbolId) -> Option<(SymbolId, Type)> {
    // Try (Ref, Literal) then (Literal, Ref)
    let (var_id, lit_id) =
        if matches!(hir.symbol(lhs)?.kind, SymbolKind::Ref) && literal_symbol_type(hir, rhs).is_some() {
            (lhs, rhs)
        } else if matches!(hir.symbol(rhs)?.kind, SymbolKind::Ref) && literal_symbol_type(hir, lhs).is_some() {
            (rhs, lhs)
        } else {
            return None;
        };
    let def_id = hir.resolve_reference_symbol(var_id)?;
    let narrowed_type = literal_symbol_type(hir, lit_id)?;
    Some((def_id, narrowed_type))
}

/// Recursively analyzes a condition expression to extract type narrowing information.
///
/// Supports:
/// - Type predicate calls: `is_string(x)` → narrows x to String in then-branch
/// - Equality with literals: `x == "foo"` → narrows x to String; `x != none` → removes None
/// - `type(x) == "typename"`: `type(x) == "string"` → narrows x to String
/// - Negation: `!is_string(x)` → swaps then/else narrowings
/// - Logical AND: `is_string(x) && is_number(y)` → both in then-branch, complement in else
/// - Logical OR (same variable): `is_string(x) || is_number(x)` → then: String|Number
pub(crate) fn analyze_condition(
    hir: &Hir,
    cond_id: SymbolId,
    children_index: &ChildrenIndex,
    ctx: &mut InferenceContext,
) -> ConditionNarrowings {
    let symbol = match hir.symbol(cond_id) {
        Some(s) => s,
        None => return ConditionNarrowings::empty(),
    };

    match &symbol.kind {
        // Simple type predicate call: is_string(x)
        SymbolKind::Call => {
            if let Some((def_id, narrowed_type)) = analyze_type_predicate_call(hir, cond_id, children_index, ctx) {
                // Store the predicate type for both branches.
                // then-branch narrows TO the type (is_complement=false),
                // else-branch narrows AWAY from it (is_complement=true).
                // The complement is computed in resolve_type_narrowings (post-unification)
                // when the variable's union type is fully resolved.
                ConditionNarrowings {
                    then_narrowings: vec![NarrowingEntry {
                        def_id,
                        narrowed_type: narrowed_type.clone(),
                        is_complement: false,
                    }],
                    else_narrowings: vec![NarrowingEntry {
                        def_id,
                        narrowed_type,
                        is_complement: true,
                    }],
                }
            } else {
                ConditionNarrowings::empty()
            }
        }

        // Negation: !expr → swap then/else
        SymbolKind::UnaryOp if symbol.value.as_deref() == Some("!") => {
            let children = get_children(children_index, cond_id);
            if children.is_empty() {
                return ConditionNarrowings::empty();
            }
            let mut inner = analyze_condition(hir, children[0], children_index, ctx);
            // Swap then and else
            std::mem::swap(&mut inner.then_narrowings, &mut inner.else_narrowings);
            inner
        }

        // Equality / inequality: x == literal, literal == x, type(x) == "typename"
        SymbolKind::BinaryOp if matches!(symbol.value.as_deref(), Some("==") | Some("!=")) => {
            let children = get_children(children_index, cond_id);
            if children.len() < 2 {
                return ConditionNarrowings::empty();
            }
            let is_neq = symbol.value.as_deref() == Some("!=");

            // Try (in order):
            //   1. type(x) == "typename"  (either argument order)
            //   2. x == literal           (either argument order)
            let narrowing = analyze_type_call_equality(hir, children[0], children[1], children_index, ctx)
                .or_else(|| analyze_type_call_equality(hir, children[1], children[0], children_index, ctx))
                .or_else(|| analyze_literal_equality(hir, children[0], children[1]))
                .or_else(|| analyze_literal_equality(hir, children[1], children[0]));

            if let Some((def_id, narrowed_type)) = narrowing {
                // == : then-branch narrows TO the type, else-branch narrows AWAY
                // != : then-branch narrows AWAY, else-branch narrows TO
                let (then_complement, else_complement) = if is_neq { (true, false) } else { (false, true) };
                ConditionNarrowings {
                    then_narrowings: vec![NarrowingEntry {
                        def_id,
                        narrowed_type: narrowed_type.clone(),
                        is_complement: then_complement,
                    }],
                    else_narrowings: vec![NarrowingEntry {
                        def_id,
                        narrowed_type,
                        is_complement: else_complement,
                    }],
                }
            } else {
                ConditionNarrowings::empty()
            }
        }

        // Logical AND / OR
        SymbolKind::BinaryOp
            if matches!(
                symbol.value.as_deref(),
                Some("&&") | Some("and") | Some("||") | Some("or")
            ) =>
        {
            let children = get_children(children_index, cond_id);
            if children.len() < 2 {
                return ConditionNarrowings::empty();
            }

            let left = analyze_condition(hir, children[0], children_index, ctx);
            let right = analyze_condition(hir, children[1], children_index, ctx);

            let is_and = matches!(symbol.value.as_deref(), Some("&&") | Some("and"));

            if is_and {
                // AND: both narrowings apply in then-branch; complement of each in else-branch
                let mut then_narrowings = left.then_narrowings;
                then_narrowings.extend(right.then_narrowings);
                let mut else_narrowings = left.else_narrowings;
                else_narrowings.extend(right.else_narrowings);
                ConditionNarrowings {
                    then_narrowings,
                    else_narrowings,
                }
            } else {
                // OR: in the then-branch, if both sides narrow the SAME variable to concrete
                // (non-complement) types, we can safely narrow it to their union.
                // Example: `is_string(x) || is_number(x)` → then: x: String|Number
                // For different variables or mixed complement flags, no then-branch narrowing.
                let mut by_def: FxHashMap<SymbolId, Vec<&NarrowingEntry>> = FxHashMap::default();
                for entry in left.then_narrowings.iter().chain(right.then_narrowings.iter()) {
                    by_def.entry(entry.def_id).or_default().push(entry);
                }

                let mut then_narrowings = Vec::new();
                // Only narrow variables that appear on BOTH sides (otherwise we can't be sure).
                let left_defs: FxHashSet<_> = left.then_narrowings.iter().map(|e| e.def_id).collect();
                let right_defs: FxHashSet<_> = right.then_narrowings.iter().map(|e| e.def_id).collect();
                for def_id in left_defs.intersection(&right_defs) {
                    let entries = &by_def[def_id];
                    // Only merge if every entry is a direct narrowing (not complement).
                    if entries.iter().all(|e| !e.is_complement) {
                        let types: Vec<Type> = entries.iter().map(|e| e.narrowed_type.clone()).collect();
                        then_narrowings.push(NarrowingEntry {
                            def_id: *def_id,
                            narrowed_type: Type::union(types),
                            is_complement: false,
                        });
                    }
                }

                let mut else_narrowings = left.else_narrowings;
                else_narrowings.extend(right.else_narrowings);
                ConditionNarrowings {
                    then_narrowings,
                    else_narrowings,
                }
            }
        }

        _ => ConditionNarrowings::empty(),
    }
}

/// Resolves type narrowings from type predicate conditions in if/elif/while expressions.
///
/// After unification, this pass overrides the types of variable references (Ref symbols)
/// within narrowed branches. For example, if `is_string(x)` is the condition of an if,
/// all references to `x` in the then-branch are narrowed to `String`, and references
/// in the else-branch are narrowed to the complement type.
///
/// Builds a def→refs index and a branch→descendants index in single HIR passes to
/// allow O(1) containment checks instead of O(depth) ancestor walks per (ref, branch) pair.
pub(crate) fn resolve_type_narrowings(hir: &Hir, ctx: &mut InferenceContext) {
    let narrowings = ctx.take_type_narrowings();
    if narrowings.is_empty() {
        return;
    }

    // Build def→refs index: for each variable definition, collect all Ref symbols
    // that point to it. This avoids an O(n) HIR scan per narrowing entry.
    let mut def_to_refs: FxHashMap<SymbolId, Vec<SymbolId>> = FxHashMap::default();
    for (ref_id, ref_sym) in hir.symbols() {
        if !matches!(ref_sym.kind, mq_hir::SymbolKind::Ref) {
            continue;
        }
        if let Some(def_id) = hir.resolve_reference_symbol(ref_id) {
            def_to_refs.entry(def_id).or_default().push(ref_id);
        }
    }

    // Collect all unique branch IDs referenced by any narrowing.
    let mut tracked_branches: FxHashSet<SymbolId> = FxHashSet::default();
    for narrowing in &narrowings {
        tracked_branches.insert(narrowing.then_branch_id);
        tracked_branches.extend(narrowing.else_branch_ids.iter().copied());
    }

    // Build branch→descendants index in a single HIR pass.
    // For each symbol, walk its ancestor chain; whenever an ancestor is a tracked
    // branch, record the symbol as a descendant of that branch.  This allows O(1)
    // containment checks later instead of re-walking the chain per (ref, branch) pair.
    let mut branch_descendants: FxHashMap<SymbolId, FxHashSet<SymbolId>> = FxHashMap::default();
    for (sym_id, _) in hir.symbols() {
        for (ancestor_id, _) in walk_ancestors(hir, sym_id) {
            if tracked_branches.contains(&ancestor_id) {
                branch_descendants.entry(ancestor_id).or_default().insert(sym_id);
            }
        }
    }

    for narrowing in &narrowings {
        // Apply then-branch narrowings
        for entry in &narrowing.then_narrowings {
            if let Some(effective_ty) = compute_narrowed_type(ctx, entry) {
                apply_narrowing_to_branch(
                    ctx,
                    entry.def_id,
                    &effective_ty,
                    narrowing.then_branch_id,
                    &def_to_refs,
                    &branch_descendants,
                );
            }
        }

        // Apply else-branch narrowings
        for entry in &narrowing.else_narrowings {
            if let Some(effective_ty) = compute_narrowed_type(ctx, entry) {
                for &else_branch_id in &narrowing.else_branch_ids {
                    apply_narrowing_to_branch(
                        ctx,
                        entry.def_id,
                        &effective_ty,
                        else_branch_id,
                        &def_to_refs,
                        &branch_descendants,
                    );
                }
            }
        }
    }
}

/// Computes the effective narrowed type for a NarrowingEntry.
///
/// - For union types: then-branch returns the predicate type directly; else-branch
///   subtracts the predicate type from the union.
/// - For non-union types: then-branch returns the predicate type directly (useful when
///   the variable has an unresolved type variable or a non-union concrete type); else-branch
///   returns `None` (no useful narrowing — keep the original type).
fn compute_narrowed_type(ctx: &InferenceContext, entry: &NarrowingEntry) -> Option<Type> {
    let var_ty = ctx.get_symbol_type(entry.def_id)?;
    let var_ty = ctx.resolve_type(var_ty);

    if var_ty.is_union() {
        if entry.is_complement {
            Some(var_ty.subtract(&entry.narrowed_type))
        } else {
            Some(entry.narrowed_type.clone())
        }
    } else {
        // For non-union types, only apply then-branch narrowing (narrow TO the type).
        // Else-branch complement on a non-union is a no-op (nothing to subtract).
        if entry.is_complement {
            None
        } else {
            Some(entry.narrowed_type.clone())
        }
    }
}

/// Applies a type narrowing to all Ref symbols within a branch that reference
/// the given variable definition, using pre-built def→refs and branch→descendants indices.
fn apply_narrowing_to_branch(
    ctx: &mut InferenceContext,
    def_id: SymbolId,
    narrowed_type: &Type,
    branch_id: SymbolId,
    def_to_refs: &FxHashMap<SymbolId, Vec<SymbolId>>,
    branch_descendants: &FxHashMap<SymbolId, FxHashSet<SymbolId>>,
) {
    let Some(refs) = def_to_refs.get(&def_id) else {
        return;
    };
    let Some(descendants) = branch_descendants.get(&branch_id) else {
        return;
    };
    for &ref_id in refs {
        // O(1) containment check via pre-built descendants set.
        if !descendants.contains(&ref_id) {
            continue;
        }
        // Use set_symbol_type (not _no_bind) to also update the substitution
        // chain for this Ref's type variable. This ensures deferred overload
        // resolution sees the narrowed type instead of the original union.
        ctx.set_symbol_type(ref_id, narrowed_type.clone());
    }
}
