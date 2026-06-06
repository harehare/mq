use mq_hir::{Symbol, SymbolId, SymbolKind};

use crate::{Diagnostic, LintContext, LintRule, Severity};

pub struct PreferCoalesce;

/// True if `id` has no children (e.g. a bare `Ref`/`Selector`, not a call).
fn is_leaf(ctx: &LintContext<'_>, id: SymbolId) -> bool {
    !ctx.all_symbols().any(|(_, s)| s.parent == Some(id))
}

fn is_none_literal(sym: &Symbol) -> bool {
    matches!(sym.kind, SymbolKind::None) || sym.value.as_deref() == Some("none")
}

fn children_sorted_by_position<'a>(ctx: &'a LintContext<'_>, parent: SymbolId) -> Vec<(SymbolId, &'a Symbol)> {
    let mut children: Vec<_> = ctx.all_symbols().filter(|(_, s)| s.parent == Some(parent)).collect();
    children.sort_by_key(|(_, s)| s.source.text_range.map(|r| (r.start.line, r.start.column)));
    children
}

/// Detects `if (x == none): fallback else: x` and its `!=` form, both equivalent to `x ?? fallback`.
fn matches_null_check_pattern(ctx: &LintContext<'_>, if_id: SymbolId) -> bool {
    let children = children_sorted_by_position(ctx, if_id);
    let Some(&(cond_id, cond)) = children.first() else {
        return false;
    };
    let Some(&(then_id, _)) = children.get(1) else {
        return false;
    };
    let Some(&(else_kw_id, _)) = children.iter().find(|(_, s)| matches!(s.kind, SymbolKind::Else)) else {
        return false;
    };
    let Some(&(else_expr_id, _)) = children_sorted_by_position(ctx, else_kw_id).first() else {
        return false;
    };

    if !matches!(cond.kind, SymbolKind::BinaryOp) {
        return false;
    }
    let Some(op) = cond.value.as_deref() else { return false };
    if op != "==" && op != "!=" {
        return false;
    }

    let cond_children = children_sorted_by_position(ctx, cond_id);
    let (Some(&(lhs_id, lhs)), Some(&(rhs_id, rhs))) = (cond_children.first(), cond_children.get(1)) else {
        return false;
    };

    // Whichever side isn't the `none` literal is the value being null-checked.
    let value_id = if is_none_literal(rhs) && is_leaf(ctx, lhs_id) {
        lhs_id
    } else if is_none_literal(lhs) && is_leaf(ctx, rhs_id) {
        rhs_id
    } else {
        return false;
    };
    let Some(value_sym) = ctx.hir.symbol(value_id) else {
        return false;
    };

    // `==`: the value must be repeated in the else-branch; `!=`: in the then-branch.
    let value_branch_id = if op == "==" { else_expr_id } else { then_id };

    if !is_leaf(ctx, value_branch_id) {
        return false;
    }
    let Some(value_branch) = ctx.hir.symbol(value_branch_id) else {
        return false;
    };
    value_branch.kind == value_sym.kind && value_branch.value == value_sym.value
}

impl LintRule for PreferCoalesce {
    fn id(&self) -> &'static str {
        "prefer_coalesce"
    }

    fn severity(&self) -> Severity {
        Severity::Style
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        ctx.all_symbols()
            .filter(|(_, sym)| matches!(sym.kind, SymbolKind::If))
            .filter(|(if_id, _)| matches_null_check_pattern(ctx, *if_id))
            .map(|(_, if_sym)| {
                let mut d = Diagnostic::new(
                    self.id(),
                    self.severity(),
                    "`if`/`else` null-check can be simplified using the `??` coalesce operator",
                );
                if let Some(range) = if_sym.source.text_range {
                    d = d.with_range(range);
                }
                d.with_help("rewrite as `<value> ?? <fallback>`")
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use mq_hir::Hir;

    use super::*;
    use crate::{LintConfig, LintContext};

    fn check(code: &str) -> Vec<Diagnostic> {
        let mut hir = Hir::default();
        let (source_id, _) = hir.add_code(None, code);
        let config = LintConfig::default();
        let ctx = LintContext::new(&hir, source_id, &config);
        PreferCoalesce.check(&ctx)
    }

    #[test]
    fn detects_eq_none_pattern() {
        let diags = check(r#"if (.value == none): "default" else: .value"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn detects_ne_none_pattern() {
        let diags = check(r#"if (x != none): x else: "default""#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn no_diagnostic_when_branches_differ() {
        let diags = check(r#"if (.value == none): "default" else: .other"#);
        assert_eq!(diags.len(), 0);
    }

    #[test]
    fn no_diagnostic_for_unrelated_condition() {
        let diags = check(r#"if (.checked == true): "yes" else: "no""#);
        assert_eq!(diags.len(), 0);
    }
}
