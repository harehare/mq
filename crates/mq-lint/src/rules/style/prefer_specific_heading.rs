use crate::{Diagnostic, LintContext, LintRule, Severity};
use mq_hir::SymbolKind;

pub struct PreferSpecificHeading;

impl LintRule for PreferSpecificHeading {
    fn id(&self) -> &'static str {
        "prefer_specific_heading"
    }

    fn severity(&self) -> Severity {
        Severity::Style
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        // Selector symbols use insert_symbol → use all_symbols
        ctx.all_symbols()
            .filter(|(_, s)| matches!(s.kind, SymbolKind::Selector(mq_lang::Selector::Heading(None))))
            .map(|(_, sym)| {
                let mut d = Diagnostic::new(
                    self.id(),
                    self.severity(),
                    "prefer a specific heading level selector (`.h1`–`.h6`) over `.h`",
                );
                if let Some(range) = sym.source.text_range {
                    d = d.with_range(range);
                }
                d.with_help("using `.h1`–`.h6` makes the intended heading level explicit")
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
        PreferSpecificHeading.check(&ctx)
    }

    #[test]
    fn detects_generic_heading_selector() {
        let diags = check(".h");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn no_diagnostic_for_specific_heading() {
        let diags = check(".h1");
        assert_eq!(diags.len(), 0);
        let diags2 = check(".h6");
        assert_eq!(diags2.len(), 0);
    }
}
