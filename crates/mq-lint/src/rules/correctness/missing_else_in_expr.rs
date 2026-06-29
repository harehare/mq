use crate::{Diagnostic, LintContext, LintMessage, LintRule, RuleId, Severity};
use mq_hir::SymbolKind;

pub struct MissingElseInExpr;

impl LintRule for MissingElseInExpr {
    fn id(&self) -> RuleId {
        RuleId::MissingElseInExpr
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
                let mut d = Diagnostic::new(LintMessage::MissingElseInExpr, self.severity());
                if let Some(range) = sym.source.text_range {
                    d = d.with_range(range);
                }
                d
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use mq_hir::Hir;
    use rstest::rstest;

    use super::*;
    use crate::{LintConfig, LintContext};

    fn check(code: &str) -> Vec<Diagnostic> {
        let mut hir = Hir::default();
        let (source_id, _) = hir.add_code(None, code);
        let config = LintConfig::default();
        let ctx = LintContext::new(&hir, source_id, &config);
        MissingElseInExpr.check(&ctx)
    }

    #[rstest]
    #[case("if (true): 1;")]
    #[case("if (.h1): 2;")]
    #[case("if (true): 1 elif (false): 2;")]
    fn detects_missing_else(#[case] code: &str) {
        let diags = check(code);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message().contains("missing an `else` branch"));
    }

    #[rstest]
    #[case("if (true): 1 else: 2;")]
    #[case("if (true): 1 elif (false): 2 else: 3;")]
    #[case(".h1")]
    fn no_diagnostic(#[case] code: &str) {
        let diags = check(code);
        assert_eq!(diags.len(), 0);
    }
}
