use rustc_hash::FxHashSet;

use crate::{Diagnostic, LintContext, LintMessage, LintRule, RuleId, Severity};
use mq_hir::SymbolKind;

/// Reports functions that are never called.
///
/// Public functions of a module file (one with only definitions at top level) or of a `module`
/// block are exported, so they are not reported. `_`-prefixed functions are always checked.
pub struct UnusedFunction;

/// Whether every top-level symbol of the source is a definition.
fn is_definitions_only(ctx: &LintContext<'_>) -> bool {
    ctx.all_symbols().filter(|(_, s)| s.parent.is_none()).all(|(_, s)| {
        matches!(
            s.kind,
            SymbolKind::Function(_)
                | SymbolKind::Variable
                | SymbolKind::Import(_)
                | SymbolKind::Include(_)
                | SymbolKind::Module(_)
                | SymbolKind::Keyword
                | SymbolKind::Ident
        )
    })
}

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

        let is_module_file = is_definitions_only(ctx);

        ctx.all_symbols()
            .filter(|(_, s)| s.is_function() && !ctx.hir.is_builtin_symbol(s))
            .filter_map(|(_, sym)| {
                let name = sym.value.as_deref()?;
                let in_module_block = sym
                    .parent
                    .and_then(|id| ctx.hir.symbol(id))
                    .is_some_and(|p| matches!(p.kind, SymbolKind::Module(_)));
                let exported = (is_module_file || in_module_block) && !name.starts_with('_');
                if exported || used_names.contains(name) {
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
    #[case("def foo(): .h1; | .h1", "unused function `foo`")]
    #[case("def bar(): .h2; | .h1", "unused function `bar`")]
    #[case("def _helper(): 1;", "unused function `_helper`")]
    #[case("def _helper(): 1;\ndef pub(): 2;", "unused function `_helper`")]
    #[case("module m: def _a(): 1; end | .h1", "unused function `_a`")]
    fn detects_unused_function(#[case] code: &str, #[case] msg: &str) {
        let diags = check(code);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message().contains(msg));
    }

    #[rstest]
    #[case("def foo(): .h1; | foo")]
    #[case("def foo(): .h1; | foo()")]
    #[case("def foo(): .h1;")]
    #[case("def _h(): 1;\ndef foo(): _h();")]
    #[case("let x = 1\ndef foo(): x;")]
    #[case("import \"json\"\ndef foo(): 1;")]
    #[case("module m: def a(): 1; end | .h1")]
    fn no_diagnostic(#[case] code: &str) {
        let diags = check(code);
        assert_eq!(diags.len(), 0);
    }
}
