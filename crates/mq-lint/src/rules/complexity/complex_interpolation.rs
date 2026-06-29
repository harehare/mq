use crate::{Diagnostic, LintContext, LintMessage, LintRule, RuleId, Severity};
use mq_hir::SymbolKind;

pub struct ComplexInterpolation;

impl LintRule for ComplexInterpolation {
    fn id(&self) -> RuleId {
        RuleId::ComplexInterpolation
    }

    fn severity(&self) -> Severity {
        Severity::Warn
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let max_exprs = ctx.config.complexity.max_interpolation_exprs;

        ctx.hir
            .symbols_for_source(ctx.source_id)
            .filter(|(_, s)| matches!(s.kind, SymbolKind::InterpolatedString))
            .filter_map(|(interp_id, sym)| {
                // Variable children represent `${...}` interpolated expressions.
                let expr_count = ctx
                    .all_symbols()
                    .filter(|(_, s)| s.parent == Some(interp_id) && matches!(s.kind, SymbolKind::Variable))
                    .count();

                if expr_count <= max_exprs {
                    return None;
                }

                let mut d = Diagnostic::new(
                    LintMessage::ComplexInterpolation { expr_count, max_exprs },
                    self.severity(),
                );
                if let Some(range) = sym.source.text_range {
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
        config.complexity.max_interpolation_exprs = max;
        let ctx = LintContext::new(&hir, source_id, &config);
        ComplexInterpolation.check(&ctx)
    }

    #[rstest]
    #[case(r#"s"${.h1} ${.h2} ${.h3} ${.h4}""#, 3)]
    #[case(r#"s"${.h1} ${.h2} ${.h3} ${.h4} ${.h5}""#, 3)]
    #[case(r#"s"${.h1} ${.h2}""#, 1)]
    fn detects_too_many_interpolations(#[case] code: &str, #[case] max: usize) {
        let diags = check_with_max(code, max);
        assert_eq!(diags.len(), 1);
    }

    #[rstest]
    #[case(r#"s"${.h1} ${.h2}""#, 3)]
    #[case(r#"s"${.h1} ${.h2} ${.h3}""#, 3)]
    #[case(r#"s"hello world""#, 1)]
    fn no_diagnostic_within_limit(#[case] code: &str, #[case] max: usize) {
        let diags = check_with_max(code, max);
        assert_eq!(diags.len(), 0);
    }
}
