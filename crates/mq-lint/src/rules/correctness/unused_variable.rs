use rustc_hash::FxHashSet;

use crate::{Diagnostic, LintContext, LintMessage, LintRule, RuleId, Severity};
use mq_hir::SymbolKind;

pub struct UnusedVariable;

impl LintRule for UnusedVariable {
    fn id(&self) -> RuleId {
        RuleId::UnusedVariable
    }

    fn severity(&self) -> Severity {
        Severity::Warn
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        // Build a set of all names referenced by Ref/Ident/Call symbols
        let used_names: FxHashSet<&str> = ctx
            .all_symbols()
            .filter(|(_, s)| matches!(s.kind, SymbolKind::Ref | SymbolKind::Ident | SymbolKind::Call))
            .filter_map(|(_, s)| s.value.as_deref())
            .collect();

        ctx.all_symbols()
            .filter(|(_, s)| matches!(s.kind, SymbolKind::Variable))
            .filter_map(|(_, sym)| {
                let name = sym.value.as_deref()?;
                // Variables prefixed with `_` are intentionally unused
                if name.starts_with('_') {
                    return None;
                }
                if used_names.contains(name) {
                    return None;
                }
                let mut d = Diagnostic::new(LintMessage::UnusedVariable { name: name.to_string() }, self.severity());
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

    fn check(code: &str) -> Vec<Diagnostic> {
        let mut hir = Hir::default();
        let (source_id, _) = hir.add_code(None, code);
        let config = LintConfig::default();
        let ctx = LintContext::new(&hir, source_id, &config);
        UnusedVariable.check(&ctx)
    }

    #[rstest]
    #[case("let x = .h1", "unused variable `x`")]
    #[case("let my_var = .h2", "unused variable `my_var`")]
    fn detects_unused_variable(#[case] code: &str, #[case] msg: &str) {
        let diags = check(code);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message().contains(msg));
    }

    #[rstest]
    #[case("let x = .h1 | x")]
    #[case("let _x = .h1")]
    #[case("let _ignored = .h1")]
    fn no_diagnostic(#[case] code: &str) {
        let diags = check(code);
        assert_eq!(diags.len(), 0);
    }
}
