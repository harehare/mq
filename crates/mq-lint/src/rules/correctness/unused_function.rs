use rustc_hash::FxHashSet;

use crate::{Diagnostic, LintContext, LintMessage, LintRule, RuleId, Severity};
use mq_hir::SymbolKind;

pub struct UnusedFunction;

impl LintRule for UnusedFunction {
    fn id(&self) -> RuleId {
        RuleId::UnusedFunction
    }

    fn severity(&self) -> Severity {
        Severity::Warn
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        // Collect all names referenced via Call/Ref/Ident in the entire HIR
        // (cross-file usage counts too, so we search all symbols not just this source)
        let used_names: FxHashSet<&str> = ctx
            .hir
            .symbols()
            .filter(|(_, s)| matches!(s.kind, SymbolKind::Call | SymbolKind::Ref | SymbolKind::Ident))
            .filter_map(|(_, s)| s.value.as_deref())
            .collect();

        ctx.all_symbols()
            .filter(|(_, s)| s.is_function() && !ctx.hir.is_builtin_symbol(s) && !s.is_internal_function())
            .filter_map(|(_, sym)| {
                let name = sym.value.as_deref()?;
                if name.starts_with('_') || used_names.contains(name) {
                    return None;
                }
                let mut d = Diagnostic::new(LintMessage::UnusedFunction { name: name.to_string() }, self.severity());
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
        UnusedFunction.check(&ctx)
    }

    #[rstest]
    #[case("def foo(): .h1;", "unused function `foo`")]
    #[case("def bar(): .h2;", "unused function `bar`")]
    fn detects_unused_function(#[case] code: &str, #[case] msg: &str) {
        let diags = check(code);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message().contains(msg));
    }

    #[rstest]
    #[case("def foo(): .h1; | foo")]
    #[case("def foo(): .h1; | foo()")]
    fn no_diagnostic(#[case] code: &str) {
        let diags = check(code);
        assert_eq!(diags.len(), 0);
    }
}
