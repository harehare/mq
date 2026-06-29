use crate::{Diagnostic, LintContext, LintMessage, LintRule, RuleId, Severity};
use mq_hir::SymbolKind;

pub struct InefficientSelector;

impl LintRule for InefficientSelector {
    fn id(&self) -> RuleId {
        RuleId::InefficientSelector
    }

    fn severity(&self) -> Severity {
        Severity::Perf
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // Selector symbols use insert_symbol → use all_symbols
        let recursive_selectors: Vec<_> = ctx
            .all_symbols()
            .filter(|(_, s)| matches!(s.kind, SymbolKind::Selector(mq_lang::Selector::Recursive)))
            .collect();

        for (rec_id, rec_sym) in recursive_selectors {
            let rec_range = match rec_sym.source.text_range {
                Some(r) => r,
                None => continue,
            };

            // Check if any sibling selector (same parent, starts after `..`) is a
            // concrete non-recursive non-attr selector.
            let has_redundant_follow = ctx.all_symbols().any(|(sid, s)| {
                if sid == rec_id {
                    return false;
                }
                if s.parent != rec_sym.parent {
                    return false;
                }
                let s_start = match s.source.text_range {
                    Some(r) => r.start,
                    None => return false,
                };
                if s_start <= rec_range.start {
                    return false;
                }
                matches!(
                    &s.kind,
                    SymbolKind::Selector(sel) if !matches!(sel, mq_lang::Selector::Recursive | mq_lang::Selector::Attr(_))
                )
            });

            if has_redundant_follow {
                let mut d = Diagnostic::new(LintMessage::InefficientSelector, self.severity());
                d = d.with_range(rec_range);
                diagnostics.push(d);
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
        InefficientSelector.check(&ctx)
    }

    #[rstest]
    #[case(".. | .h1")]
    #[case(".. | .h2")]
    fn detects_recursive_followed_by_specific_selector(#[case] code: &str) {
        let diags = check(code);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message().contains("inefficient selector"));
    }

    #[rstest]
    #[case(".h1")]
    #[case("..")]
    #[case(".h1 | .h2")]
    fn no_diagnostic(#[case] code: &str) {
        let diags = check(code);
        assert_eq!(diags.len(), 0);
    }
}
