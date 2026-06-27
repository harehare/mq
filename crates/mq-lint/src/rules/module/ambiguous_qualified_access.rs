use rustc_hash::FxHashMap;

use crate::{Diagnostic, LintContext, LintMessage, LintRule, RuleId, Severity};
use mq_hir::SymbolKind;

pub struct AmbiguousQualifiedAccess;

impl LintRule for AmbiguousQualifiedAccess {
    fn id(&self) -> RuleId {
        RuleId::AmbiguousQualifiedAccess
    }

    fn severity(&self) -> Severity {
        Severity::Warn
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        // Use the whole HIR, not just this source, since modules may live elsewhere.
        let mut defining_modules: FxHashMap<&str, Vec<&str>> = FxHashMap::default();

        for (_, sym) in ctx.hir.symbols() {
            if !sym.is_function() {
                continue;
            }
            let Some(fn_name) = sym.value.as_deref() else { continue };
            let Some(module_sym) = sym.parent.and_then(|p| ctx.hir.symbol(p)) else {
                continue;
            };
            let SymbolKind::Module(_) = module_sym.kind else {
                continue;
            };
            let Some(module_name) = module_sym.value.as_deref() else {
                continue;
            };

            let modules = defining_modules.entry(fn_name).or_default();
            if !modules.contains(&module_name) {
                modules.push(module_name);
            }
        }

        ctx.all_symbols()
            .filter(|(_, sym)| sym.is_function())
            .filter_map(|(_, sym)| {
                let fn_name = sym.value.as_deref()?;
                let module_sym = sym.parent.and_then(|p| ctx.hir.symbol(p))?;
                let SymbolKind::Module(_) = module_sym.kind else {
                    return None;
                };
                let this_module = module_sym.value.as_deref()?;

                let other_module = defining_modules.get(fn_name)?.iter().find(|&&m| m != this_module)?;

                let mut d = Diagnostic::new(
                    LintMessage::AmbiguousQualifiedAccess {
                        fn_name: fn_name.to_string(),
                        this_module: this_module.to_string(),
                        other_module: other_module.to_string(),
                    },
                    self.severity(),
                );
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

    use super::*;
    use crate::{LintConfig, LintContext};

    fn check(code: &str) -> Vec<Diagnostic> {
        let mut hir = Hir::default();
        let (source_id, _) = hir.add_code(None, code);
        let config = LintConfig::default();
        let ctx = LintContext::new(&hir, source_id, &config);
        AmbiguousQualifiedAccess.check(&ctx)
    }

    #[test]
    fn detects_same_function_name_in_two_modules() {
        let diags = check("module a: def foo(): 1; end | module b: def foo(): 2; end");
        assert_eq!(diags.len(), 2);
        assert!(diags.iter().all(|d| d.message().contains("foo")));
    }

    #[test]
    fn no_diagnostic_for_distinct_function_names() {
        let diags = check("module a: def foo(): 1; end | module b: def bar(): 2; end");
        assert_eq!(diags.len(), 0);
    }

    #[test]
    fn no_diagnostic_for_function_outside_module() {
        let diags = check("def foo(): 1; | foo()");
        assert_eq!(diags.len(), 0);
    }
}
