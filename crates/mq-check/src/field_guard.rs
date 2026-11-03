//! What a condition says about the field of a record that a branch reads.
//!
//! In `if (v["a"] != None): v["a"] + 1`, the read of `v["a"]` in the branch cannot be `none`,
//! and in `if (contains(keys(v), "a")): ...` the key is present. The access resolves the field
//! type with this knowledge, so a record whose field is `number | none` is not reported there.

use mq_hir::{Hir, SymbolId, SymbolKind};

use crate::constraint::{ChildrenIndex, get_children};
use crate::types::Lit;
use crate::walk_ancestors;

/// What is known about one field where it is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FieldGuard {
    /// The key exists
    pub present: bool,
    /// The key exists and its value is not `none`
    pub non_none: bool,
}

impl FieldGuard {
    fn merge(self, other: Self) -> Self {
        Self {
            present: self.present || other.present,
            non_none: self.non_none || other.non_none,
        }
    }

    const PRESENT: Self = Self {
        present: true,
        non_none: false,
    };
    const NON_NONE: Self = Self {
        present: true,
        non_none: true,
    };
}

/// The variable definition and key that a bracket access `v["key"]` reads.
fn accessed_field(hir: &Hir, id: SymbolId, children_index: &ChildrenIndex) -> Option<(SymbolId, String)> {
    if hir.symbol(id)?.kind != SymbolKind::Call {
        return None;
    }
    let def_id = hir.resolve_reference_symbol(id)?;
    let key = get_children(children_index, id).first().and_then(|&key_id| {
        let key = hir.symbol(key_id)?;
        matches!(key.kind, SymbolKind::String | SymbolKind::Symbol)
            .then(|| key.value.as_ref().map(ToString::to_string))
            .flatten()
    })?;
    Some((def_id, key))
}

fn is_none_literal(hir: &Hir, id: SymbolId) -> bool {
    hir.symbol(id).is_some_and(|s| s.kind == SymbolKind::None)
}

fn call_name(hir: &Hir, id: SymbolId) -> Option<&str> {
    let symbol = hir.symbol(id)?;
    if symbol.kind == SymbolKind::Call {
        symbol.value.as_deref()
    } else {
        None
    }
}

/// What `cond` being `truth` says about the field `key` of `def_id`.
fn guard_of_condition(
    hir: &Hir,
    cond: SymbolId,
    truth: bool,
    target: (SymbolId, &str),
    children_index: &ChildrenIndex,
) -> FieldGuard {
    let Some(symbol) = hir.symbol(cond) else {
        return FieldGuard::default();
    };
    let children = get_children(children_index, cond);
    let reads_target = |id: SymbolId| {
        accessed_field(hir, id, children_index).is_some_and(|(def, key)| def == target.0 && key == target.1)
    };

    match (&symbol.kind, symbol.value.as_deref()) {
        (SymbolKind::UnaryOp, Some("!")) => children.first().map_or(FieldGuard::default(), |&inner| {
            guard_of_condition(hir, inner, !truth, target, children_index)
        }),
        (SymbolKind::BinaryOp, Some("&&")) if truth => children.iter().fold(FieldGuard::default(), |acc, &c| {
            acc.merge(guard_of_condition(hir, c, true, target, children_index))
        }),
        (SymbolKind::BinaryOp, Some("||")) if !truth => children.iter().fold(FieldGuard::default(), |acc, &c| {
            acc.merge(guard_of_condition(hir, c, false, target, children_index))
        }),
        (SymbolKind::BinaryOp, Some(op @ ("==" | "!="))) => {
            let [lhs, rhs] = children else {
                return FieldGuard::default();
            };
            let compares_with_none = (reads_target(*lhs) && is_none_literal(hir, *rhs))
                || (reads_target(*rhs) && is_none_literal(hir, *lhs));
            // `v["k"] != None` is true, or `v["k"] == None` is false
            if compares_with_none && truth == (op == "!=") {
                FieldGuard::NON_NONE
            } else {
                FieldGuard::default()
            }
        }
        (SymbolKind::Call, _) => {
            let args = children;
            match (call_name(hir, cond), args) {
                (Some("is_none"), [arg]) if !truth && reads_target(*arg) => FieldGuard::NON_NONE,
                (Some("not"), [arg]) => guard_of_condition(hir, *arg, !truth, target, children_index),
                (Some("contains"), [keys, key]) if truth => {
                    let keys_of_target = call_name(hir, *keys) == Some("keys")
                        && get_children(children_index, *keys)
                            .first()
                            .is_some_and(|&arg| hir.resolve_reference_symbol(arg) == Some(target.0));
                    let key_matches = hir.symbol(*key).is_some_and(|k| k.value.as_deref() == Some(target.1));
                    if keys_of_target && key_matches {
                        FieldGuard::PRESENT
                    } else {
                        FieldGuard::default()
                    }
                }
                _ => FieldGuard::default(),
            }
        }
        _ => FieldGuard::default(),
    }
}

/// The `if`/`elif` conditions enclosing `access`, with whether each held on the way to it.
fn enclosing_conditions(hir: &Hir, access: SymbolId, children_index: &ChildrenIndex) -> Vec<(SymbolId, bool)> {
    let mut conditions = Vec::new();
    let mut child = access;
    for (ancestor, symbol) in walk_ancestors(hir, access) {
        if matches!(symbol.kind, SymbolKind::If | SymbolKind::Elif) {
            let children = get_children(children_index, ancestor);
            if let (Some(&cond), Some(position)) = (children.first(), children.iter().position(|&c| c == child)) {
                match position {
                    0 => {}
                    1 => conditions.push((cond, true)),
                    // another branch of an `if`: its condition did not hold
                    _ => conditions.push((cond, false)),
                }
            }
        }
        child = ancestor;
    }
    conditions
}

/// What the `if`/`elif` conditions enclosing `access` say about the field it reads.
pub(crate) fn field_guard(
    hir: &Hir,
    access: SymbolId,
    def_id: SymbolId,
    key: &str,
    children_index: &ChildrenIndex,
) -> FieldGuard {
    enclosing_conditions(hir, access, children_index)
        .into_iter()
        .fold(FieldGuard::default(), |guard, (cond, truth)| {
            guard.merge(guard_of_condition(hir, cond, truth, (def_id, key), children_index))
        })
}

/// A test of a tag field against a literal that an enclosing condition made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagTest {
    /// The field that was tested
    pub key: String,
    /// The literal it was compared with
    pub lit: Lit,
    /// Whether the field equals the literal where the test is in force
    pub equal: bool,
}

fn literal_of(hir: &Hir, id: SymbolId) -> Option<Lit> {
    let symbol = hir.symbol(id)?;
    let value = symbol.value.as_deref()?;
    match symbol.kind {
        SymbolKind::Boolean => Some(Lit::Bool(value == "true")),
        SymbolKind::String => Some(Lit::Str(value.to_string())),
        _ => None,
    }
}

fn collect_tag_tests(
    hir: &Hir,
    cond: SymbolId,
    truth: bool,
    def_id: SymbolId,
    children_index: &ChildrenIndex,
    out: &mut Vec<TagTest>,
) {
    let Some(symbol) = hir.symbol(cond) else {
        return;
    };
    let children = get_children(children_index, cond);
    let read = |id: SymbolId| accessed_field(hir, id, children_index).filter(|(def, _)| *def == def_id);

    match (&symbol.kind, symbol.value.as_deref()) {
        (SymbolKind::UnaryOp, Some("!")) => {
            if let Some(&inner) = children.first() {
                collect_tag_tests(hir, inner, !truth, def_id, children_index, out);
            }
        }
        (SymbolKind::BinaryOp, Some("&&")) if truth => {
            for &c in children {
                collect_tag_tests(hir, c, true, def_id, children_index, out);
            }
        }
        (SymbolKind::BinaryOp, Some("||")) if !truth => {
            for &c in children {
                collect_tag_tests(hir, c, false, def_id, children_index, out);
            }
        }
        (SymbolKind::BinaryOp, Some(op @ ("==" | "!="))) => {
            let [lhs, rhs] = children else {
                return;
            };
            let tested = read(*lhs)
                .zip(literal_of(hir, *rhs))
                .or_else(|| read(*rhs).zip(literal_of(hir, *lhs)));
            if let Some(((_, key), lit)) = tested {
                out.push(TagTest {
                    key,
                    lit,
                    equal: truth == (op == "=="),
                });
            }
        }
        (SymbolKind::Call, _) => {
            if let Some((_, key)) = read(cond) {
                // `if (r["ok"])` holds when the tag is `true`
                out.push(TagTest {
                    key,
                    lit: Lit::Bool(true),
                    equal: truth,
                });
            } else if let (Some("not"), [arg]) = (call_name(hir, cond), children) {
                collect_tag_tests(hir, *arg, !truth, def_id, children_index, out);
            }
        }
        _ => {}
    }
}

/// The tests of tag fields of `def_id` that the conditions enclosing `access` make.
pub(crate) fn tag_tests(hir: &Hir, access: SymbolId, def_id: SymbolId, children_index: &ChildrenIndex) -> Vec<TagTest> {
    let mut tests = Vec::new();
    for (cond, truth) in enclosing_conditions(hir, access, children_index) {
        collect_tag_tests(hir, cond, truth, def_id, children_index, &mut tests);
    }
    tests
}
