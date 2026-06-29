use rustc_hash::FxHashSet;

use crate::{Diagnostic, LintContext, LintMessage, LintRule, RuleId, Severity};
use mq_hir::{ScopeId, ScopeKind, SymbolId, SymbolKind};

pub struct UnusedParameter;

fn is_in_scope_subtree(ctx: &LintContext<'_>, scope_id: ScopeId, target_scope_id: ScopeId) -> bool {
    let mut current = Some(scope_id);
    while let Some(sid) = current {
        if sid == target_scope_id {
            return true;
        }
        current = ctx.hir.scope(sid).and_then(|s| s.parent_id);
    }
    false
}

impl LintRule for UnusedParameter {
    fn id(&self) -> RuleId {
        RuleId::UnusedParameter
    }

    fn severity(&self) -> Severity {
        Severity::Warn
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        let functions: Vec<(SymbolId, _)> = ctx.all_symbols().filter(|(_, s)| s.is_function()).collect();

        for (fn_id, _) in functions {
            let Some((fn_scope_id, _)) = ctx
                .hir
                .scopes()
                .find(|(_, scope)| matches!(&scope.kind, ScopeKind::Function(id) if *id == fn_id))
            else {
                continue;
            };

            let used_names: FxHashSet<&str> = ctx
                .all_symbols()
                .filter(|(_, s)| matches!(s.kind, SymbolKind::Ref | SymbolKind::Ident | SymbolKind::Call))
                .filter(|(_, s)| is_in_scope_subtree(ctx, s.scope, fn_scope_id))
                .filter_map(|(_, s)| s.value.as_deref())
                .collect();

            for (_, param_sym) in ctx
                .all_symbols()
                .filter(|(_, s)| s.parent == Some(fn_id) && matches!(s.kind, SymbolKind::Parameter))
            {
                let name = match param_sym.value.as_deref() {
                    Some(n) => n,
                    None => continue,
                };

                if name.starts_with('_') || used_names.contains(name) {
                    continue;
                }

                let mut d = Diagnostic::new(LintMessage::UnusedParameter { name: name.to_string() }, self.severity());
                if let Some(range) = param_sym.source.text_range {
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
        UnusedParameter.check(&ctx)
    }

    #[rstest]
    #[case("def f(a, b): a", 1, "b")]
    #[case("def f(a, b, c): a + b", 1, "c")]
    fn detects_unused_parameter(#[case] code: &str, #[case] expected_count: usize, #[case] expected_name: &str) {
        let diags = check(code);
        assert_eq!(diags.len(), expected_count);
        assert!(diags[0].message().contains(expected_name));
    }

    #[rstest]
    #[case("def f(a, b): a + b")]
    #[case("def f(a): a")]
    #[case("def f(): .h1")]
    fn no_diagnostic_when_all_params_used(#[case] code: &str) {
        let diags = check(code);
        assert_eq!(diags.len(), 0);
    }

    #[test]
    fn underscore_prefix_suppresses_warning() {
        let diags = check("def f(_unused, b): b");
        assert_eq!(diags.len(), 0);
    }

    #[test]
    fn detects_multiple_unused_parameters() {
        let diags = check("def f(a, b, c): .h1");
        assert_eq!(diags.len(), 3);
    }
}
