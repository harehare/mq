use crate::{Diagnostic, LintContext, LintMessage, LintRule, RuleId, Severity};
use mq_hir::SymbolKind;
use rustc_hash::FxHashMap;

pub struct UnnecessaryInterpolation;

impl LintRule for UnnecessaryInterpolation {
    fn id(&self) -> RuleId {
        RuleId::UnnecessaryInterpolation
    }

    fn severity(&self) -> Severity {
        Severity::Style
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let mut child_counts: FxHashMap<mq_hir::SymbolId, usize> = FxHashMap::default();

        for (_, sym) in ctx.all_symbols() {
            if let Some(parent_id) = sym.parent {
                *child_counts.entry(parent_id).or_insert(0) += 1;
            }
        }

        ctx.all_symbols()
            .filter(|(_, sym)| matches!(sym.kind, SymbolKind::InterpolatedString))
            .filter_map(|(sym_id, sym)| {
                let count = child_counts.get(&sym_id).copied().unwrap_or(0);
                if count == 1 {
                    sym.source.text_range.map(|range| {
                        Diagnostic::new(LintMessage::UnnecessaryInterpolation, self.severity()).with_range(range)
                    })
                } else {
                    None
                }
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
        UnnecessaryInterpolation.check(&ctx)
    }

    #[rstest]
    #[case(r#"s"${x}""#)]
    #[case(r#"s"${.h1}""#)]
    fn detects_unnecessary_interpolation(#[case] code: &str) {
        let diags = check(code);
        assert_eq!(diags.len(), 1);
    }

    #[rstest]
    #[case(r#"s"${x}${y}""#)]
    #[case(r#"s"prefix ${x}""#)]
    #[case(r#"s"${x} suffix""#)]
    fn no_diagnostic(#[case] code: &str) {
        let diags = check(code);
        assert_eq!(diags.len(), 0);
    }
}
