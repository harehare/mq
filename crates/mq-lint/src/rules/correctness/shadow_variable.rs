use crate::{Diagnostic, LintContext, LintMessage, LintRule, RuleId, Severity};
use mq_hir::{ScopeId, SymbolKind};

pub struct ShadowVariable;

impl LintRule for ShadowVariable {
    fn id(&self) -> RuleId {
        RuleId::ShadowVariable
    }

    fn severity(&self) -> Severity {
        Severity::Warn
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // Variable symbols use insert_symbol, so use all_symbols
        for (_, sym) in ctx.all_symbols() {
            if !matches!(sym.kind, SymbolKind::Variable) {
                continue;
            }
            let name = match sym.value.as_deref() {
                Some(n) => n,
                None => continue,
            };

            if shadows_outer_variable(ctx, sym.scope, name) {
                let mut d = Diagnostic::new(LintMessage::ShadowVariable { name: name.to_string() }, self.severity());
                if let Some(range) = sym.source.text_range {
                    d = d.with_range(range);
                }
                diagnostics.push(d);
            }
        }

        diagnostics
    }
}

/// Returns true if any ancestor scope (excluding `current_scope`) contains
/// a Variable or Parameter with the given name defined in this same source.
fn shadows_outer_variable(ctx: &LintContext<'_>, current_scope: ScopeId, name: &str) -> bool {
    let mut scope_id = ctx.hir.scope(current_scope).and_then(|s| s.parent_id);

    while let Some(sid) = scope_id {
        let has_same_name = ctx.all_symbols().any(|(_, s)| {
            s.scope == sid
                && matches!(s.kind, SymbolKind::Variable | SymbolKind::Parameter)
                && s.value.as_deref() == Some(name)
        });

        if has_same_name {
            return true;
        }

        scope_id = ctx.hir.scope(sid).and_then(|s| s.parent_id);
    }

    false
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
        ShadowVariable.check(&ctx)
    }

    #[test]
    fn detects_shadow() {
        // Outer `x`, then inner `x` in function body
        let diags = check("let x = 1 | def f(): let x = 2; x; end");
        assert!(!diags.is_empty());
    }

    #[test]
    fn no_shadow_different_names() {
        let diags = check("let x = 1 | def f(): let y = 2; y; end");
        assert_eq!(diags.len(), 0);
    }
}
