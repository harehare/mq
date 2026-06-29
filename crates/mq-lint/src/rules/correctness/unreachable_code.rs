use crate::{Diagnostic, LintContext, LintMessage, LintRule, RuleId, Severity};
use mq_hir::SymbolKind;

pub struct UnreachableCode;

impl LintRule for UnreachableCode {
    fn id(&self) -> RuleId {
        RuleId::UnreachableCode
    }

    fn severity(&self) -> Severity {
        Severity::Error
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // Keyword symbols use insert_symbol → use all_symbols
        for (_, sym) in ctx.all_symbols() {
            if !matches!(sym.kind, SymbolKind::Keyword) {
                continue;
            }
            let kw = sym.value.as_deref().unwrap_or("");
            if kw != "break" && kw != "continue" {
                continue;
            }

            let break_end = match sym.source.text_range {
                Some(r) => r.end,
                None => continue,
            };

            let parent_id = sym.parent;

            // Any sibling (same parent) that starts after this break's end position
            // and is not a structural keyword (like "end") is unreachable.
            let unreachable: Vec<_> = ctx
                .all_symbols()
                .filter(|(_, s)| {
                    s.parent == parent_id
                        && s.source.text_range.is_some_and(|r| r.start > break_end)
                        && !matches!(s.kind, SymbolKind::Keyword)
                })
                .collect();

            for (_, dead_sym) in unreachable {
                let mut d = Diagnostic::new(
                    LintMessage::UnreachableCode {
                        keyword: kw.to_string(),
                    },
                    self.severity(),
                );
                if let Some(range) = dead_sym.source.text_range {
                    d = d.with_range(range);
                }
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
        UnreachableCode.check(&ctx)
    }

    #[rstest]
    #[case("loop break | .h1 end")]
    fn detects_unreachable_after_break(#[case] code: &str) {
        let diags = check(code);
        assert!(!diags.is_empty());
    }

    #[rstest]
    #[case(".h1 | .value")]
    #[case("loop break end")]
    fn no_diagnostic(#[case] code: &str) {
        let diags = check(code);
        assert_eq!(diags.len(), 0);
    }
}
