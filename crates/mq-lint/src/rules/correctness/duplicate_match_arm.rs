use rustc_hash::FxHashSet;

use crate::{Diagnostic, LintContext, LintMessage, LintRule, RuleId, Severity};
use mq_hir::SymbolKind;

pub struct DuplicateMatchArm;

impl LintRule for DuplicateMatchArm {
    fn id(&self) -> RuleId {
        RuleId::DuplicateMatchArm
    }

    fn severity(&self) -> Severity {
        Severity::Error
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // Match symbols are tracked via add_symbol
        let match_ids: Vec<_> = ctx
            .hir
            .symbols_for_source(ctx.source_id)
            .filter(|(_, s)| matches!(s.kind, SymbolKind::Match))
            .map(|(id, _)| id)
            .collect();

        for match_id in match_ids {
            // MatchArm children are also tracked via add_symbol
            let arms: Vec<_> = ctx
                .hir
                .symbols_for_source(ctx.source_id)
                .filter(|(_, s)| s.parent == Some(match_id) && matches!(s.kind, SymbolKind::MatchArm { .. }))
                .collect();

            let mut seen_patterns: FxHashSet<String> = FxHashSet::default();

            for (arm_id, arm_sym) in arms {
                // Pattern children of MatchArm use insert_symbol → use all_symbols
                let pattern = ctx
                    .all_symbols()
                    .find(|(_, s)| s.parent == Some(arm_id) && matches!(s.kind, SymbolKind::Pattern { .. }));

                let pattern_repr = pattern.and_then(|(_, p)| p.value.clone()).map(|v| v.to_string());

                let Some(repr) = pattern_repr else {
                    continue;
                };

                if repr == "_" {
                    continue;
                }

                if !seen_patterns.insert(repr.clone()) {
                    let mut d = Diagnostic::new(LintMessage::DuplicateMatchArm { pattern: repr }, self.severity());
                    if let Some(range) = arm_sym.source.text_range {
                        d = d.with_range(range);
                    }
                    diagnostics.push(d);
                }
            }
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
        DuplicateMatchArm.check(&ctx)
    }

    #[rstest]
    #[case(r#"match (1): | "a": "h" | "a": "hh" | _: "other" end"#)]
    #[case(r#"match (1): | "x": 1 | "x": 2 | _: 0 end"#)]
    fn detects_duplicate_arm(#[case] code: &str) {
        let diags = check(code);
        assert_eq!(diags.len(), 1);
    }

    #[rstest]
    #[case(r#"match (1): | "a": "h" | "b": "t" | _: "other" end"#)]
    #[case(r#"match (1): | "a": 1 | _: 0 end"#)]
    #[case(r#"match (1): | _: 0 | _: 1 end"#)]
    fn no_diagnostic(#[case] code: &str) {
        let diags = check(code);
        assert_eq!(diags.len(), 0);
    }
}
