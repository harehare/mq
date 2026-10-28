//! Places that explain a diagnostic: where the variables a call was given got their type.

use mq_hir::{Hir, SymbolId, SymbolKind};

use crate::infer::InferenceContext;
use crate::symbol_range;

/// A second place a diagnostic points at, with what to say about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Related {
    /// Where it is.
    pub range: mq_lang::Range,
    /// What to say about it, e.g. `` `x` is `string` ``.
    pub message: String,
}

/// The variables passed to the calls at `at_range`, with their types and where they are defined.
///
/// Only a variable or parameter of the program with a settled type is listed, since a type that
/// is still unknown explains nothing.
pub(crate) fn variable_arguments(hir: &Hir, ctx: &InferenceContext, at_range: &[SymbolId]) -> Vec<Related> {
    let mut related: Vec<Related> = Vec::new();
    for &call in at_range
        .iter()
        .filter(|&&id| hir.symbol(id).is_some_and(|s| s.kind == SymbolKind::Call))
    {
        let children: Vec<SymbolId> = hir
            .symbols()
            .filter(|(_, symbol)| symbol.parent == Some(call))
            .map(|(id, _)| id)
            .collect();
        for argument in children {
            let Some(symbol) = hir.symbol(argument).filter(|s| s.kind == SymbolKind::Ref) else {
                continue;
            };
            // `x.depth` is typed by the selector, not by `x`
            if hir
                .symbols()
                .any(|(_, s)| s.parent == Some(argument) && matches!(s.kind, SymbolKind::Selector(_)))
            {
                continue;
            }
            let Some(definition) = hir
                .resolve_reference_symbol(argument)
                .and_then(|id| hir.symbol(id).map(|s| (id, s)))
                .filter(|(_, s)| {
                    matches!(s.kind, SymbolKind::Variable | SymbolKind::Parameter) && !hir.is_builtin_symbol(s)
                })
            else {
                continue;
            };
            let (id, definition) = definition;
            let (Some(name), Some(range)) = (symbol.value.as_deref(), symbol_range(hir, definition)) else {
                continue;
            };
            let Some(ty) = ctx.get_symbol_type(id).map(|ty| ctx.resolve_type(ty)) else {
                continue;
            };
            if ty.has_pending_var() || ty.is_dynamic() || related.iter().any(|r| r.range == range) {
                continue;
            }
            related.push(Related {
                range,
                message: format!("`{name}` is `{}`", ty.display_renumbered()),
            });
        }
    }
    related
}
