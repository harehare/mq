use crate::{Diagnostic, LintContext, LintMessage, LintRule, RuleId, Severity};
use mq_hir::SymbolKind;

pub struct PreferSpecificHeading;

impl LintRule for PreferSpecificHeading {
    fn id(&self) -> RuleId {
        RuleId::PreferSpecificHeading
    }

    fn severity(&self) -> Severity {
        Severity::Style
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        // Selector symbols use insert_symbol → use all_symbols
        ctx.all_symbols()
            .filter(|(_, s)| matches!(s.kind, SymbolKind::Selector(mq_lang::Selector::Heading(None))))
            .map(|(_, sym)| {
                let mut d = Diagnostic::new(LintMessage::PreferSpecificHeading, self.severity());
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
        PreferSpecificHeading.check(&ctx)
    }

    #[rstest]
    #[case(".h")]
    fn detects_generic_heading_selector(#[case] code: &str) {
        let diags = check(code);
        assert_eq!(diags.len(), 1);
    }

    #[rstest]
    #[case(".h1")]
    #[case(".h2")]
    #[case(".h6")]
    #[case(".h1 | .value")]
    fn no_diagnostic(#[case] code: &str) {
        let diags = check(code);
        assert_eq!(diags.len(), 0);
    }
}
