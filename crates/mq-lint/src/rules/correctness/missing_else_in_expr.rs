use crate::{Diagnostic, LintContext, LintMessage, LintRule, RuleId, Severity};
use mq_hir::SymbolKind;

/// Reports `if` expressions that have no `else` branch.
///
/// An `if` at the end of a pipeline is exempt: without `else` it yields `None`,
/// which is the idiom for dropping a node.
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

        let is_pipeline_tail = |sym: &mq_hir::Symbol| {
            let Some(range) = sym.source.text_range else {
                return false;
            };
            let in_pipeline = match sym.parent {
                None => true,
                Some(parent) => ctx
                    .hir
                    .symbol(parent)
                    .is_some_and(|p| matches!(p.kind, SymbolKind::Function(_) | SymbolKind::Block)),
            };
            in_pipeline
                && !ctx.siblings(sym.parent).any(|(_, other)| {
                    other
                        .source
                        .text_range
                        .is_some_and(|r| (r.start.line, r.start.column) > (range.start.line, range.start.column))
                })
        };

        ctx.hir
            .symbols_for_source(ctx.source_id)
            .filter(|(id, s)| {
                matches!(s.kind, SymbolKind::If) && !if_ids_with_else.contains(id) && !is_pipeline_tail(s)
            })
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
        let (source_id, _, _) = hir.add_code(None, code);
        let config = LintConfig::default();
        let ctx = LintContext::new(&hir, source_id, &config);
        MissingElseInExpr.check(&ctx)
    }

    #[rstest]
    #[case("if (true): 1 | add(1)")]
    #[case("if (.h1): 2 | add(1)")]
    #[case("if (true): 1 elif (false): 2 | add(1)")]
    #[case("if (true): 1 | .h1")]
    #[case("if (true): 1 | let x = 2")]
    fn detects_missing_else(#[case] code: &str) {
        let diags = check(code);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message().contains("missing an `else` branch"));
    }

    #[rstest]
    #[case("if (true): 1 else: 2;")]
    #[case("if (true): 1 elif (false): 2 else: 3;")]
    #[case(".h1")]
    #[case("if (true): 1;")]
    #[case(".h | if (!is_none(1)): to_md_list(1, 2)")]
    #[case("def f(x): add(x, 1) | if (x): 1;")]
    fn no_diagnostic(#[case] code: &str) {
        let diags = check(code);
        assert_eq!(diags.len(), 0);
    }
}
