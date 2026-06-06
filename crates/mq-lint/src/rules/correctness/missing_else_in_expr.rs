use crate::{Diagnostic, LintContext, LintRule, Severity};
use mq_hir::SymbolKind;

pub struct MissingElseInExpr;

impl LintRule for MissingElseInExpr {
    fn id(&self) -> &'static str {
        "missing_else_in_expr"
    }

    fn severity(&self) -> Severity {
        Severity::Warn
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        // Collect all If symbol IDs that have an Else or Elif child.
        // Both If and Else use add_symbol, so symbols_for_source covers them.
        let if_ids_with_else: std::collections::HashSet<mq_hir::SymbolId> = ctx
            .hir
            .symbols_for_source(ctx.source_id)
            .filter(|(_, s)| matches!(s.kind, SymbolKind::Else))
            .filter_map(|(_, s)| s.parent)
            .collect();

        ctx.hir
            .symbols_for_source(ctx.source_id)
            .filter(|(id, s)| matches!(s.kind, SymbolKind::If) && !if_ids_with_else.contains(id))
            .map(|(_, sym)| {
                let mut d = Diagnostic::new(
                    self.id(),
                    self.severity(),
                    "`if` expression is missing an `else` branch (evaluates to `none` on false)",
                );
                if let Some(range) = sym.source.text_range {
                    d = d.with_range(range);
                }
                d.with_help("add `else: <expr>` to provide a value for the false branch")
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
        MissingElseInExpr.check(&ctx)
    }

    #[test]
    fn detects_if_without_else() {
        let diags = check("if (true): 1;");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("missing an `else` branch"));
    }

    #[test]
    fn no_diagnostic_with_else() {
        let diags = check("if (true): 1 else: 2;");
        assert_eq!(diags.len(), 0);
    }

    #[test]
    fn no_diagnostic_with_elif_else() {
        let diags = check("if (true): 1 elif (false): 2 else: 3;");
        assert_eq!(diags.len(), 0);
    }

    #[test]
    fn detects_if_with_only_elif() {
        // elif but no else is still missing an else
        let diags = check("if (true): 1 elif (false): 2;");
        assert_eq!(diags.len(), 1);
    }
}
