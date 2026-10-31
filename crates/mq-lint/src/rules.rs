//! All lint rules, organized by category.

/// The `complexity` rules.
pub mod complexity;
/// The `correctness` rules.
pub mod correctness;
/// The `module` rules.
pub mod module;
/// The `security` rules.
pub mod security;
/// The `selector` rules.
pub mod selector;
/// The `style` rules.
pub mod style;

use crate::LintRule;

/// Returns all built-in lint rules.
pub fn all_rules() -> Vec<Box<dyn LintRule>> {
    let mut rules: Vec<Box<dyn LintRule>> = Vec::new();
    rules.extend(correctness::all());
    rules.extend(style::all());
    rules.extend(complexity::all());
    rules.extend(selector::all());
    rules.extend(module::all());
    rules.extend(security::all());
    rules
}

/// True if `sym` is the raw text of a `${...}` interpolation embed rather than a declaration
/// (both use `SymbolKind::Variable`; the embedded expression is lowered as child symbols).
pub(crate) fn is_interpolation_embed(ctx: &crate::LintContext<'_>, sym: &mq_hir::Symbol) -> bool {
    sym.parent
        .and_then(|id| ctx.hir.symbol(id))
        .is_some_and(|p| matches!(p.kind, mq_hir::SymbolKind::InterpolatedString))
}
