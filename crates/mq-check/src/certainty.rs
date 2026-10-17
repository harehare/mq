//! How sure the checker is that a diagnostic is a real mistake.
//!
//! The checker infers types flow-insensitively, so a diagnostic that rests on one of those
//! approximations may be wrong for a program that runs fine. Those are reported as
//! [`Certainty::Possible`]; the rest are [`Certainty::Definite`].

use mq_hir::{Hir, SymbolId, SymbolKind};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::infer::InferenceContext;
use crate::{TypeError, symbol_range};

/// How sure the checker is that a [`TypeError`] is a real mistake.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Certainty {
    /// The operands have fully known types and the program fails at runtime if it gets here.
    Definite,
    /// The diagnostic relies on an approximation, or the program may handle the failure.
    Possible,
}

/// A [`TypeError`] together with how sure the checker is about it.
#[derive(Debug, Clone)]
pub struct Classified {
    /// The diagnostic.
    pub error: TypeError,
    /// How sure the checker is about it.
    pub certainty: Certainty,
}

/// Classifies every error against the program it was found in.
pub(crate) fn classify_all(hir: &Hir, ctx: &InferenceContext, errors: Vec<TypeError>) -> Vec<Classified> {
    let mut index: Option<Index> = None;
    errors
        .into_iter()
        .map(|error| {
            let certainty = match &error {
                TypeError::NullablePropagation { .. }
                | TypeError::UnreachableCode { .. }
                | TypeError::NonExhaustiveMatch { .. }
                | TypeError::HeterogeneousArray { .. } => Certainty::Possible,
                _ => match error.location() {
                    Some(range) => {
                        let index = index.get_or_insert_with(|| Index::new(hir));
                        classify_at(hir, ctx, index, range)
                    }
                    None => Certainty::Definite,
                },
            };
            Classified { error, certainty }
        })
        .collect()
}

/// Lookup tables over the HIR, built once per check.
struct Index {
    children: FxHashMap<SymbolId, Vec<SymbolId>>,
    /// Variables that are the target of an assignment
    assigned: FxHashSet<SymbolId>,
}

impl Index {
    fn new(hir: &Hir) -> Self {
        let mut children: FxHashMap<SymbolId, Vec<SymbolId>> = FxHashMap::default();
        for (id, symbol) in hir.symbols() {
            if let Some(parent) = symbol.parent {
                children.entry(parent).or_default().push(id);
            }
        }
        let mut assigned = FxHashSet::default();
        for (id, symbol) in hir.symbols() {
            if symbol.kind != SymbolKind::Assign {
                continue;
            }
            let target = children.get(&id).and_then(|c| c.first()).copied();
            if let Some(def) = target.and_then(|t| hir.resolve_reference_symbol(t)) {
                assigned.insert(def);
            }
        }
        Self { children, assigned }
    }
}

fn classify_at(hir: &Hir, ctx: &InferenceContext, index: &Index, range: mq_lang::Range) -> Certainty {
    let at_range: Vec<SymbolId> = hir
        .symbols()
        .filter(|(_, symbol)| symbol_range(hir, symbol) == Some(range))
        .map(|(id, _)| id)
        .collect();

    for &root in &at_range {
        if inside_try_body(hir, root)
            || reads_reassigned_variable(hir, index, root)
            || has_unsettled_operand(ctx, index, root)
        {
            return Certainty::Possible;
        }
    }
    Certainty::Definite
}

/// The failure of a `try:` body is handled by its `catch`.
fn inside_try_body(hir: &Hir, id: SymbolId) -> bool {
    let mut child = id;
    let mut current = hir.symbol(id).and_then(|s| s.parent);
    while let Some(parent_id) = current {
        let Some(parent) = hir.symbol(parent_id) else {
            return false;
        };
        if parent.kind == SymbolKind::Try {
            let is_handler = hir.symbol(child).is_some_and(|s| s.kind == SymbolKind::Catch);
            return !is_handler;
        }
        child = parent_id;
        current = parent.parent;
    }
    false
}

/// The type of a variable that is assigned more than once depends on the order of execution, and
/// the type of a parameter is only what the body has used it for so far. The checker tracks
/// neither precisely.
fn reads_reassigned_variable(hir: &Hir, index: &Index, root: SymbolId) -> bool {
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        if hir.symbol(id).is_some_and(|s| s.kind == SymbolKind::Ref)
            && hir.resolve_reference_symbol(id).is_some_and(|def| {
                index.assigned.contains(&def) || hir.symbol(def).is_some_and(|s| s.kind == SymbolKind::Parameter)
            })
        {
            return true;
        }
        if let Some(children) = index.children.get(&id) {
            stack.extend(children.iter().copied());
        }
    }
    false
}

/// An operand whose type the checker could not settle.
fn has_unsettled_operand(ctx: &InferenceContext, index: &Index, root: SymbolId) -> bool {
    let unsettled = |id: SymbolId| {
        ctx.get_symbol_type(id)
            .map(|ty| ctx.resolve_type(ty))
            .is_some_and(|ty| ty.has_pending_var() || ty.is_dynamic())
    };
    index
        .children
        .get(&root)
        .is_some_and(|c| c.iter().any(|&id| unsettled(id)))
}
