//! Exhaustiveness checking for pattern match expressions.
//!
//! After type inference resolves the type of the matched expression, this module
//! verifies that all possible values are covered by at least one unconditional arm.
//! Only a definite gap is reported: when the coverage cannot be decided (a type that is not
//! known yet, `dynamic`, or nested patterns that are not all irrefutable) the match is accepted.
//!
//! # Coverage rules
//!
//! An arm with a guard covers nothing. `_`, a variable and an alternative containing one cover
//! every value. Otherwise the arms must cover each part of the matched type:
//!
//! | Matched type | Covered by |
//! |---|---|
//! | `bool` | `true` and `false`, or `:bool` |
//! | `none` | `None`, or `:none` |
//! | `number`, `string`, `symbol`, `bytes`, `function` | the type pattern of that name |
//! | node kinds | the type patterns (`:h1`, `:code`, `:markdown`) whose kinds make up the set |
//! | `[T]`, tuple | `:array`, or array patterns of every length with irrefutable elements |
//! | `{k: T}`, `dict` | `:dict`, or for a record a dict pattern of irrefutable values over its keys |
//! | `a \| b` | every member |
//! | variable, `dynamic` | not checked |

use mq_hir::{Hir, SymbolId, SymbolKind};

use crate::TypeError;
use crate::constraint::{ChildrenIndex, get_children};
use crate::infer::InferenceContext;
use crate::kind_set::KindSet;
use crate::narrowing::type_name_to_type;
use crate::types::Type;
use crate::unify::range_to_span;

/// What a single pattern accepts.
#[derive(Debug, Clone, PartialEq)]
enum Pat {
    /// `_` or a variable binding
    Irrefutable,
    /// `true` or `false`
    Bool(bool),
    /// `None`
    None,
    /// A number or string literal
    Literal,
    /// `:name`
    Type(String),
    /// `[a, b]` or `[a, ..rest]`
    Array { elems: Vec<Pat>, rest: bool },
    /// `{key: pattern}`, with the keys it requires
    Dict { entries: Vec<(String, Pat)> },
    /// `p1 || p2`
    Or(Vec<Pat>),
}

impl Pat {
    fn is_irrefutable(&self) -> bool {
        match self {
            Pat::Irrefutable => true,
            Pat::Or(alternatives) => alternatives.iter().any(Pat::is_irrefutable),
            _ => false,
        }
    }
}

/// A match arm as far as exhaustiveness is concerned.
#[derive(Debug)]
struct ArmInfo {
    pattern: Pat,
    has_guard: bool,
}

/// Reads the pattern below `pattern_id`.
fn read_pattern(hir: &Hir, pattern_id: SymbolId, children_index: &ChildrenIndex) -> Pat {
    let Some(symbol) = hir.symbol(pattern_id) else {
        return Pat::Irrefutable;
    };
    let children = get_children(children_index, pattern_id);
    let sub_patterns = || {
        children
            .iter()
            .filter(|&&child| {
                hir.symbol(child)
                    .is_some_and(|s| matches!(s.kind, SymbolKind::Pattern { .. }))
            })
            .map(|&child| (child, read_pattern(hir, child, children_index)))
    };

    match symbol.kind {
        SymbolKind::Pattern { is_or: true, .. } => Pat::Or(sub_patterns().map(|(_, pat)| pat).collect()),
        SymbolKind::Pattern { is_dict: true, .. } => Pat::Dict {
            entries: sub_patterns()
                .map(|(child, pat)| {
                    let key = hir.symbol(child).and_then(|s| s.value.as_deref()).unwrap_or_default();
                    (key.to_string(), pat)
                })
                .collect(),
        },
        _ => {
            for &child in children {
                let Some(child_symbol) = hir.symbol(child) else {
                    continue;
                };
                match child_symbol.kind {
                    SymbolKind::Boolean => return Pat::Bool(child_symbol.value.as_deref() == Some("true")),
                    SymbolKind::None => return Pat::None,
                    SymbolKind::Number | SymbolKind::String => return Pat::Literal,
                    SymbolKind::Symbol => {
                        return Pat::Type(child_symbol.value.as_deref().unwrap_or_default().to_string());
                    }
                    SymbolKind::PatternVariable { .. } => return Pat::Irrefutable,
                    _ => {}
                }
            }
            // `_` and variable bindings have no children of their own, as do `[]` and `{}`
            // (which have no value either).
            if symbol.value.is_some() {
                return Pat::Irrefutable;
            }
            let elems: Vec<(SymbolId, Pat)> = sub_patterns().collect();
            let rest = elems.last().is_some_and(|(id, _)| {
                get_children(children_index, *id).iter().any(|&c| {
                    hir.symbol(c)
                        .is_some_and(|s| matches!(s.kind, SymbolKind::PatternVariable { is_rest: true }))
                })
            });
            let mut elems: Vec<Pat> = elems.into_iter().map(|(_, pat)| pat).collect();
            if rest {
                elems.pop();
            }
            Pat::Array { elems, rest }
        }
    }
}

/// The patterns of the unguarded arms, with alternatives flattened.
fn unguarded_patterns(arms: &[ArmInfo]) -> Vec<&Pat> {
    fn flatten<'a>(pat: &'a Pat, out: &mut Vec<&'a Pat>) {
        match pat {
            Pat::Or(alternatives) => alternatives.iter().for_each(|alt| flatten(alt, out)),
            other => out.push(other),
        }
    }
    let mut out = Vec::new();
    for arm in arms.iter().filter(|arm| !arm.has_guard) {
        flatten(&arm.pattern, &mut out);
    }
    out
}

fn has_type_pattern(pats: &[&Pat], name: &str) -> bool {
    pats.iter().any(|pat| matches!(pat, Pat::Type(n) if n == name))
}

/// Whether array patterns of every length are among `pats`.
///
/// `None` when that cannot be told because an element is a refutable pattern.
fn arrays_cover_every_length(pats: &[&Pat]) -> Option<bool> {
    let arrays: Vec<(&Vec<Pat>, bool)> = pats
        .iter()
        .filter_map(|pat| match pat {
            Pat::Array { elems, rest } => Some((elems, *rest)),
            _ => None,
        })
        .collect();
    if arrays
        .iter()
        .any(|(elems, _)| elems.iter().any(|e| !e.is_irrefutable()))
    {
        return None;
    }
    let Some(shortest_rest) = arrays
        .iter()
        .filter(|(_, rest)| *rest)
        .map(|(elems, _)| elems.len())
        .min()
    else {
        return Some(false);
    };
    Some((0..shortest_rest).all(|len| arrays.iter().any(|(elems, rest)| !rest && elems.len() == len)))
}

/// Returns `Some(missing)` if the match on `ty` is non-exhaustive, `None` if exhaustive.
///
/// `missing` is a human-readable description of the uncovered case(s).
fn missing_cases(ty: &Type, arms: &[ArmInfo], ctx: &mut InferenceContext) -> Option<String> {
    let pats = unguarded_patterns(arms);
    if pats.iter().any(|pat| pat.is_irrefutable()) {
        return None;
    }
    let gap = || Some(ty.to_string());

    match ty {
        Type::Bool => {
            let has_true = pats.iter().any(|pat| matches!(pat, Pat::Bool(true)));
            let has_false = pats.iter().any(|pat| matches!(pat, Pat::Bool(false)));
            if has_type_pattern(&pats, "bool") {
                return None;
            }
            match (has_true, has_false) {
                (true, true) => None,
                (true, false) => Some("false".to_string()),
                (false, true) => Some("true".to_string()),
                (false, false) => Some("true, false".to_string()),
            }
        }
        Type::None => (!pats.iter().any(|pat| matches!(pat, Pat::None)) && !has_type_pattern(&pats, "none"))
            .then(|| "none".to_string()),
        Type::Number | Type::Int | Type::Float => (!has_type_pattern(&pats, "number")).then(|| ty.to_string()),
        Type::String => (!has_type_pattern(&pats, "string")).then(|| ty.to_string()),
        Type::Symbol => (!has_type_pattern(&pats, "symbol")).then(|| ty.to_string()),
        Type::Bytes => (!has_type_pattern(&pats, "bytes")).then(|| ty.to_string()),
        Type::Function(..) => (!has_type_pattern(&pats, "function")).then(|| ty.to_string()),
        Type::Node(set) => {
            let covered = pats
                .iter()
                .filter_map(|pat| match pat {
                    Pat::Type(name) => match type_name_to_type(name, ctx) {
                        Some(Type::Node(kinds)) => Some(kinds),
                        _ => None,
                    },
                    _ => None,
                })
                .fold(KindSet::EMPTY, KindSet::union);
            let rest = set.difference(covered);
            (!rest.is_empty()).then(|| Type::Node(rest).to_string())
        }
        Type::Array(_) | Type::Tuple(_) => {
            if has_type_pattern(&pats, "array") {
                return None;
            }
            match arrays_cover_every_length(&pats) {
                Some(false) => gap(),
                _ => None,
            }
        }
        Type::Dict(..) => (!has_type_pattern(&pats, "dict")).then(|| ty.to_string()),
        Type::Record(fields, _) => {
            if has_type_pattern(&pats, "dict") {
                return None;
            }
            let dicts: Vec<&Vec<(String, Pat)>> = pats
                .iter()
                .filter_map(|pat| match pat {
                    Pat::Dict { entries } => Some(entries),
                    _ => None,
                })
                .collect();
            let all_irrefutable = dicts
                .iter()
                .all(|entries| entries.iter().all(|(_, pat)| pat.is_irrefutable()));
            let covers = dicts.iter().any(|entries| {
                entries
                    .iter()
                    .all(|(key, pat)| fields.contains_key(key) && pat.is_irrefutable())
            });
            (all_irrefutable && !covers).then(|| ty.to_string())
        }
        Type::Union(members) => {
            let missing: Vec<String> = members
                .iter()
                .filter_map(|member| missing_cases(member, arms, ctx))
                .collect();
            (!missing.is_empty()).then(|| missing.join(", "))
        }
        // Not known well enough to decide.
        Type::Var(_) | Type::Dynamic | Type::Never | Type::Generator(_) | Type::RowEmpty => None,
    }
}

/// Checks all match expressions in the HIR for exhaustiveness and returns a list of errors.
///
/// `children_index` is passed in to avoid rebuilding it — it is already constructed
/// by `generate_constraints` and reused here to save an O(N) full-HIR scan.
pub(crate) fn check_match_exhaustiveness(
    hir: &Hir,
    ctx: &mut InferenceContext,
    children_index: &ChildrenIndex,
) -> Vec<TypeError> {
    let mut errors = Vec::new();

    for (match_id, symbol) in hir.symbols() {
        if !matches!(symbol.kind, SymbolKind::Match) {
            continue;
        }

        let children = get_children(children_index, match_id);
        if children.len() < 2 {
            // No arms — nothing to check.
            continue;
        }

        // First child is the match expression; rest are MatchArm symbols.
        let match_expr_id = children[0];
        let match_ty_raw = ctx.get_or_create_symbol_type(match_expr_id);
        let match_ty = ctx.resolve_type(&match_ty_raw);

        // Collect arm info.
        let mut arms: Vec<ArmInfo> = Vec::new();
        for &arm_id in &children[1..] {
            if let Some(arm_sym) = hir.symbol(arm_id) {
                let SymbolKind::MatchArm { has_guard } = arm_sym.kind else {
                    continue;
                };

                // Find the Pattern child of this arm.
                let arm_children = get_children(children_index, arm_id);
                let pattern_kind = arm_children
                    .iter()
                    .find(|&&child_id| {
                        hir.symbol(child_id)
                            .is_some_and(|s| matches!(s.kind, SymbolKind::Pattern { .. }))
                    })
                    .map_or(Pat::Irrefutable, |&child_id| {
                        read_pattern(hir, child_id, children_index)
                    });

                arms.push(ArmInfo {
                    pattern: pattern_kind,
                    has_guard,
                });
            }
        }

        if let Some(missing) = missing_cases(&match_ty, &arms, ctx) {
            let range = hir.symbol(match_id).and_then(|s| s.source.text_range);
            errors.push(TypeError::NonExhaustiveMatch {
                missing: missing.clone(),
                span: range.as_ref().map(range_to_span),
                location: range,
                context: Some(format!(
                    "add a wildcard arm `| _: ...` or cover the missing case(s): {missing}"
                )),
            });
        }
    }

    errors
}
