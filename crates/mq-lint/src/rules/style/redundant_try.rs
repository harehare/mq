use mq_hir::SymbolKind;

use crate::{Diagnostic, LintContext, LintRule, Severity};

pub struct RedundantTry;

impl LintRule for RedundantTry {
    fn id(&self) -> &'static str {
        "redundant_try"
    }

    fn severity(&self) -> Severity {
        Severity::Style
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        // `try: <expr> catch: none` is exactly what `?` does.
        ctx.all_symbols()
            .filter(|(_, sym)| matches!(sym.kind, SymbolKind::Try))
            .filter_map(|(try_id, try_sym)| {
                let (catch_id, _) = ctx
                    .all_symbols()
                    .find(|(_, s)| s.parent == Some(try_id) && matches!(s.kind, SymbolKind::Catch))?;
                let (catch_expr_id, catch_expr) = ctx.all_symbols().find(|(_, s)| s.parent == Some(catch_id))?;

                let is_none_literal =
                    matches!(catch_expr.kind, SymbolKind::None) || catch_expr.value.as_deref() == Some("none");
                let has_no_children = !ctx.all_symbols().any(|(_, s)| s.parent == Some(catch_expr_id));

                if !is_none_literal || !has_no_children {
                    return None;
                }

                let mut d = Diagnostic::new(
                    self.id(),
                    self.severity(),
                    "`try: ... catch: none` is equivalent to the `?` error-suppression operator",
                );
                if let Some(range) = try_sym.source.text_range {
                    d = d.with_range(range);
                }
                Some(d.with_help("rewrite as `<expr>?`"))
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
        RedundantTry.check(&ctx)
    }

    #[test]
    fn detects_catch_none() {
        let diags = check(r#"try: get("x") catch: none"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn no_diagnostic_for_catch_with_fallback() {
        let diags = check(r#"try: get("x") catch: "default""#);
        assert_eq!(diags.len(), 0);
    }
}
