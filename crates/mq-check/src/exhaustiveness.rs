//! Exhaustiveness checking for pattern match expressions.
//!
//! After type inference resolves the type of the matched expression, this module
//! verifies that all possible values are covered by at least one unconditional arm.
//!
//! # Coverage rules
//!
//! | Matched type | Exhaustive when |
//! |---|---|
//! | `Bool` | wildcard/var arm (no guard) **or** both `true` and `false` arms (no guard) |
//! | `None` | wildcard/var arm (no guard) **or** `none` arm (no guard) |
//! | other concrete types | wildcard/var arm (no guard) |
//! | `Union(ts)` | every member type is individually covered |
//! | `Var` / unknown | skip — cannot statically determine coverage |

use mq_hir::{Hir, SymbolId, SymbolKind};

use crate::TypeError;
use crate::constraint::{ChildrenIndex, get_children};
use crate::infer::InferenceContext;
use crate::types::Type;
use crate::unify::range_to_span;

/// Classifies what values a single match arm pattern covers.
#[derive(Debug, Clone, PartialEq)]
enum PatternKind {
    /// Wildcard `_` — no children at all.
    Wildcard,
    /// Variable binding `x` — has a `PatternVariable` child, no literal child.
    VarBinding,
    /// Literal `true`.
    BoolTrue,
    /// Literal `false`.
    BoolFalse,
    /// Literal `none`.
    NoneVal,
    /// Any other specific literal (number, string, symbol, array, dict).
    Specific,
}

/// Information about a single match arm relevant for exhaustiveness.
#[derive(Debug)]
struct ArmInfo {
    pattern: PatternKind,
    has_guard: bool,
}

/// Inspects the `Pattern` child of a `MatchArm` and returns a `PatternKind`.
fn classify_pattern(hir: &Hir, pattern_id: SymbolId, children_index: &ChildrenIndex) -> PatternKind {
    let children = get_children(children_index, pattern_id);

    if children.is_empty() {
        return PatternKind::Wildcard;
    }

    let mut has_pattern_var = false;
    for &child_id in children {
        if let Some(sym) = hir.symbol(child_id) {
            match &sym.kind {
                SymbolKind::Boolean => {
                    let val = sym.value.as_deref().unwrap_or("");
                    if val == "true" {
                        return PatternKind::BoolTrue;
                    } else {
                        return PatternKind::BoolFalse;
                    }
                }
                SymbolKind::None => return PatternKind::NoneVal,
                SymbolKind::Number | SymbolKind::String | SymbolKind::Symbol | SymbolKind::Array => {
                    return PatternKind::Specific;
                }
                SymbolKind::Pattern { .. } => {
                    // Nested array/dict pattern — specific structure.
                    return PatternKind::Specific;
                }
                SymbolKind::PatternVariable { .. } => {
                    has_pattern_var = true;
                }
                _ => {}
            }
        }
    }

    if has_pattern_var {
        PatternKind::VarBinding
    } else {
        // Fallback: treat as wildcard (e.g. dict/array shorthand bindings).
        PatternKind::Wildcard
    }
}

/// Returns `true` if the arm unconditionally catches all values (no guard required).
fn is_catch_all(arm: &ArmInfo) -> bool {
    !arm.has_guard && matches!(arm.pattern, PatternKind::Wildcard | PatternKind::VarBinding)
}

/// Returns `Some(missing)` if the match on `ty` is non-exhaustive, `None` if exhaustive.
///
/// `missing` is a human-readable description of the uncovered case(s).
fn missing_cases(ty: &Type, arms: &[ArmInfo]) -> Option<String> {
    // A catch-all arm always makes the match exhaustive.
    if arms.iter().any(is_catch_all) {
        return None;
    }

    match ty {
        Type::Bool => {
            let has_true = arms.iter().any(|a| !a.has_guard && a.pattern == PatternKind::BoolTrue);
            let has_false = arms.iter().any(|a| !a.has_guard && a.pattern == PatternKind::BoolFalse);
            match (has_true, has_false) {
                (true, true) => None,
                (true, false) => Some("false".to_string()),
                (false, true) => Some("true".to_string()),
                (false, false) => Some("true, false".to_string()),
            }
        }
        Type::None => {
            let has_none = arms.iter().any(|a| !a.has_guard && a.pattern == PatternKind::NoneVal);
            if has_none { None } else { Some("none".to_string()) }
        }
        Type::Union(types) => {
            let mut missing: Vec<String> = Vec::new();
            for member in types {
                if let Some(m) = missing_cases(member, arms) {
                    missing.push(m);
                }
            }
            if missing.is_empty() {
                None
            } else {
                Some(missing.join(", "))
            }
        }
        // For type variables, we cannot statically determine exhaustiveness.
        Type::Var(_) => None,
        // For all other concrete types, only a catch-all covers them.
        // (Already handled above by the catch-all check.)
        other => Some(format!("{}", other)),
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
                    .find_map(|&child_id| {
                        hir.symbol(child_id).and_then(|s| {
                            if matches!(s.kind, SymbolKind::Pattern { .. }) {
                                Some(classify_pattern(hir, child_id, children_index))
                            } else {
                                None
                            }
                        })
                    })
                    .unwrap_or(PatternKind::Wildcard);

                arms.push(ArmInfo {
                    pattern: pattern_kind,
                    has_guard,
                });
            }
        }

        if let Some(missing) = missing_cases(&match_ty, &arms) {
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
