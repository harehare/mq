use std::collections::HashSet;

use crate::{Diagnostic, LintContext, LintMessage, LintRule, RuleId, Severity};
use mq_hir::{SymbolId, SymbolKind};

pub struct NegatedCondition;

impl LintRule for NegatedCondition {
    fn id(&self) -> RuleId {
        RuleId::NegatedCondition
    }

    fn severity(&self) -> Severity {
        Severity::Style
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        let if_ids_with_else: HashSet<SymbolId> = ctx
            .hir
            .symbols_for_source(ctx.source_id)
            .filter(|(_, s)| matches!(s.kind, SymbolKind::Else))
            .filter_map(|(_, s)| s.parent)
            .collect();

        for (if_id, if_sym) in ctx
            .hir
            .symbols_for_source(ctx.source_id)
            .filter(|(id, s)| matches!(s.kind, SymbolKind::If) && if_ids_with_else.contains(id))
        {
            let mut children: Vec<_> = ctx
                .all_symbols()
                .filter(|(_, s)| s.parent == Some(if_id))
                .filter_map(|(_, s)| s.source.text_range.map(|r| (r, s)))
                .collect();
            children.sort_by_key(|(r, _)| (r.start.line, r.start.column));

            let Some((_, cond_sym)) = children.first() else {
                continue;
            };

            if !matches!(cond_sym.kind, SymbolKind::UnaryOp) || cond_sym.value.as_deref() != Some("!") {
                continue;
            }

            let mut d = Diagnostic::new(LintMessage::NegatedCondition, self.severity());
            if let Some(range) = if_sym.source.text_range {
                d = d.with_range(range);
            }
            diagnostics.push(d);
        }

        diagnostics
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
        NegatedCondition.check(&ctx)
    }

    #[rstest]
    #[case("if (!.checked): 1 else: 2")]
    #[case("if (!x): \"yes\" else: \"no\"")]
    fn detects_negated_condition(#[case] code: &str) {
        let diags = check(code);
        assert_eq!(diags.len(), 1);
    }

    #[rstest]
    #[case("if (.checked): 1 else: 2")]
    #[case("if (.value == none): 1 else: 2")]
    fn no_diagnostic_for_non_negated_condition(#[case] code: &str) {
        let diags = check(code);
        assert_eq!(diags.len(), 0);
    }

    #[test]
    fn no_diagnostic_without_else_branch() {
        let diags = check("if (!.checked): 1");
        assert_eq!(diags.len(), 0);
    }
}
