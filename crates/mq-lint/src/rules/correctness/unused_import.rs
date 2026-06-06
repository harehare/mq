use crate::{Diagnostic, LintContext, LintRule, Severity};
use mq_hir::SymbolKind;

pub struct UnusedImport;

impl LintRule for UnusedImport {
    fn id(&self) -> &'static str {
        "unused_import"
    }

    fn severity(&self) -> Severity {
        Severity::Warn
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        // Collect all module names referenced via QualifiedAccess (module::function).
        let used_modules: std::collections::HashSet<&str> = ctx
            .hir
            .symbols()
            .filter(|(_, s)| matches!(s.kind, SymbolKind::QualifiedAccess))
            .filter_map(|(_, s)| s.value.as_deref())
            .collect();

        // Find Import symbols in this source whose module name is never accessed.
        ctx.hir
            .symbols_for_source(ctx.source_id)
            .filter(|(_, s)| matches!(s.kind, SymbolKind::Import(_)))
            .filter_map(|(_, sym)| {
                let name = sym.value.as_deref()?;
                if used_modules.contains(name) {
                    return None;
                }
                let mut d = Diagnostic::new(
                    self.id(),
                    self.severity(),
                    format!("imported module `{name}` is never used"),
                );
                if let Some(range) = sym.source.text_range {
                    d = d.with_range(range);
                }
                Some(d.with_help(format!("remove `import \"{name}\"` or use it with `{name}::function`")))
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
        // Note: import resolution requires a real module resolver; we test the symbol
        // detection logic with a manually constructed HIR scenario.
        let (source_id, _) = hir.add_code(None, code);
        let config = LintConfig::default();
        let ctx = LintContext::new(&hir, source_id, &config);
        UnusedImport.check(&ctx)
    }

    #[test]
    fn no_imports_no_diagnostic() {
        let diags = check(".h1");
        assert_eq!(diags.len(), 0);
    }
}
