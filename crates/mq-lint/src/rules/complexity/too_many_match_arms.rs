use crate::{Diagnostic, LintContext, LintMessage, LintRule, RuleId, Severity};
use mq_hir::SymbolKind;

pub struct TooManyMatchArms;

impl LintRule for TooManyMatchArms {
    fn id(&self) -> RuleId {
        RuleId::TooManyMatchArms
    }

    fn severity(&self) -> Severity {
        Severity::Warn
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let max_arms = ctx.config.complexity.max_match_arms;

        // Match and MatchArm both use add_symbol
        let match_ids: Vec<_> = ctx
            .hir
            .symbols_for_source(ctx.source_id)
            .filter(|(_, s)| matches!(s.kind, SymbolKind::Match))
            .collect();

        match_ids
            .into_iter()
            .filter_map(|(match_id, match_sym)| {
                let arm_count = ctx
                    .hir
                    .symbols_for_source(ctx.source_id)
                    .filter(|(_, s)| s.parent == Some(match_id) && matches!(s.kind, SymbolKind::MatchArm { .. }))
                    .count();

                if arm_count <= max_arms {
                    return None;
                }

                let mut d = Diagnostic::new(LintMessage::TooManyMatchArms { arm_count, max_arms }, self.severity());
                if let Some(range) = match_sym.source.text_range {
                    d = d.with_range(range);
                }
                Some(d)
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

    fn check_with_max(code: &str, max: usize) -> Vec<Diagnostic> {
        let mut hir = Hir::default();
        let (source_id, _) = hir.add_code(None, code);
        let mut config = LintConfig::default();
        config.complexity.max_match_arms = max;
        let ctx = LintContext::new(&hir, source_id, &config);
        TooManyMatchArms.check(&ctx)
    }

    #[rstest]
    #[case(r#"match (1): | "a": 1 | "b": 2 | "c": 3 | _: 0 end"#, 3)]
    #[case(r#"match (1): | "a": 1 | "b": 2 | _: 0 end"#, 2)]
    fn detects_too_many_arms(#[case] code: &str, #[case] max: usize) {
        let diags = check_with_max(code, max);
        assert_eq!(diags.len(), 1);
    }

    #[rstest]
    #[case(r#"match (1): | "a": 1 | "b": 2 | _: 0 end"#, 3)]
    #[case(r#"match (1): | "a": 1 | _: 0 end"#, 2)]
    fn no_diagnostic_within_limit(#[case] code: &str, #[case] max: usize) {
        let diags = check_with_max(code, max);
        assert_eq!(diags.len(), 0);
    }
}
