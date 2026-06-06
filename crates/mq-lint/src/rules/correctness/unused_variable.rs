use rustc_hash::FxHashSet;

use crate::{Diagnostic, LintContext, LintRule, Severity};
use mq_hir::SymbolKind;

pub struct UnusedVariable;

impl LintRule for UnusedVariable {
    fn id(&self) -> &'static str {
        "unused_variable"
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
                let mut d = Diagnostic::new(self.id(), self.severity(), format!("unused variable `{name}`"));
                if let Some(range) = sym.source.text_range {
                    d = d.with_range(range);
                }
                Some(d.with_help(format!("if this is intentional, prefix with `_`: `_{name}`")))
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
        UnusedVariable.check(&ctx)
    }

    #[test]
    fn detects_unused_variable() {
        let diags = check("let x = .h1");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("unused variable `x`"));
    }

    #[test]
    fn no_diagnostic_when_variable_used() {
        let diags = check("let x = .h1 | x");
        assert_eq!(diags.len(), 0);
    }

    #[test]
    fn underscore_prefix_suppresses_warning() {
        let diags = check("let _x = .h1");
        assert_eq!(diags.len(), 0);
    }
}
