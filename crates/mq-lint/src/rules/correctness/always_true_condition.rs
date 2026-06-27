use crate::{Diagnostic, LintContext, LintMessage, LintRule, RuleId, Severity};
use mq_hir::SymbolKind;

pub struct AlwaysTrueCondition;

impl LintRule for AlwaysTrueCondition {
    fn id(&self) -> RuleId {
        RuleId::AlwaysTrueCondition
    }

    fn severity(&self) -> Severity {
        Severity::Warn
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        // Find If symbols whose condition child is a literal `true` or `false`.
        // The condition is the first child of the If node by source position.
        let mut diagnostics = Vec::new();

        let if_ids: Vec<_> = ctx
            .hir
            .symbols_for_source(ctx.source_id)
            .filter(|(_, s)| matches!(s.kind, SymbolKind::If))
            .collect();

        for (if_id, if_sym) in if_ids {
            // Sort children by source position and take the first one (the condition).
            let mut children: Vec<_> = ctx
                .all_symbols()
                .filter(|(_, s)| s.parent == Some(if_id))
                .filter_map(|(id, s)| s.source.text_range.map(|r| (id, r, s)))
                .collect();
            children.sort_by_key(|(_, r, _)| (r.start.line, r.start.column));

            let Some((_, _, cond_sym)) = children.first() else {
                continue;
            };

            let Some(value) = cond_sym.value.as_deref() else {
                continue;
            };

            if !matches!(cond_sym.kind, SymbolKind::Boolean) {
                continue;
            }

            let mut d = Diagnostic::new(
                LintMessage::AlwaysTrueCondition {
                    value: value.to_string(),
                },
                self.severity(),
            );
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

    use super::*;
    use crate::{LintConfig, LintContext};

    fn check(code: &str) -> Vec<Diagnostic> {
        let mut hir = Hir::default();
        let (source_id, _) = hir.add_code(None, code);
        let config = LintConfig::default();
        let ctx = LintContext::new(&hir, source_id, &config);
        AlwaysTrueCondition.check(&ctx)
    }

    #[test]
    fn detects_literal_true_condition() {
        let diags = check("if (true): 1 else: 2;");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message().contains("always `true`"));
    }

    #[test]
    fn detects_literal_false_condition() {
        let diags = check("if (false): 1 else: 2;");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message().contains("always `false`"));
    }

    #[test]
    fn no_diagnostic_for_dynamic_condition() {
        let diags = check("if (.h1): 1 else: 2;");
        assert_eq!(diags.len(), 0);
    }
}
