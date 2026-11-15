use crate::{Diagnostic, LintContext, LintMessage, LintRule, RuleId, Severity};
use mq_hir::SymbolKind;

/// Reports `if`/`unless`/`elif`/`else` branches that start with `let`/`var` and no `do ... end`.
///
/// A bare `|` after such a branch continues after the whole `if`, not inside the branch.
pub struct BarePipeInBranch;

impl LintRule for BarePipeInBranch {
    fn id(&self) -> RuleId {
        RuleId::BarePipeInBranch
    }

    fn severity(&self) -> Severity {
        Severity::Warn
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        ctx.hir
            .symbols_for_source(ctx.source_id())
            .filter(|(_, s)| {
                matches!(
                    s.kind,
                    SymbolKind::If | SymbolKind::Unless | SymbolKind::Elif | SymbolKind::Else
                )
            })
            .filter_map(|(id, sym)| {
                let binding = ctx
                    .children(id)
                    .find(|(_, c)| {
                        matches!(c.kind, SymbolKind::Keyword) && matches!(c.value.as_deref(), Some("let" | "var"))
                    })?
                    .1;
                let range = binding.source.text_range.or(sym.source.text_range)?;
                Some(Diagnostic::new(LintMessage::BarePipeInBranch, self.severity()).with_range(range))
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
        let (source_id, _, _) = hir.add_code(None, code);
        let config = LintConfig::default();
        let ctx = LintContext::new(&hir, source_id, &config);
        BarePipeInBranch.check(&ctx)
    }

    #[rstest]
    #[case("if (c): let y = 1 | y + 1 else: 3;", 1)]
    #[case("if (c): 2 else: let y = 1 | y + 1;", 1)]
    #[case("if (c): var z = 1 | z;", 1)]
    #[case("if (c): 1 elif (d): let y = 2 | y else: 4;", 1)]
    #[case("unless (c): let y = 1 | y;", 1)]
    fn detects_let_in_branch(#[case] code: &str, #[case] count: usize) {
        assert_eq!(check(code).len(), count);
    }

    #[rstest]
    #[case("if (c): do let y = 1 | y + 1 end else: 3;")]
    #[case("if (c): 1 else: 2 | add(1);")]
    #[case("let y = 1 | y + 1")]
    fn no_diagnostic_without_bare_binding(#[case] code: &str) {
        assert!(check(code).is_empty());
    }
}
