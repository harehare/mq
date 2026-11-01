use rustc_hash::{FxHashMap, FxHashSet};

use crate::{Diagnostic, Fix, LintContext, LintMessage, LintRule, RuleId, Severity};
use mq_hir::{ScopeId, ScopeKind, SymbolId, SymbolKind};

/// Reports function parameters that are never used.
pub struct UnusedParameter;

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

        // The scope of each function, keeping the first when a function has several.
        let mut fn_scopes: FxHashMap<SymbolId, ScopeId> = FxHashMap::default();
        for (scope_id, scope) in ctx.hir.scopes() {
            if let ScopeKind::Function(id) = &scope.kind {
                fn_scopes.entry(*id).or_insert(scope_id);
            }
        }
        let fn_scope_ids: FxHashSet<ScopeId> = fn_scopes.values().copied().collect();

        // Names used in each function scope, including its nested scopes.
        let mut used_names: FxHashMap<ScopeId, FxHashSet<&str>> = FxHashMap::default();
        for (_, s) in ctx
            .all_symbols()
            .filter(|(_, s)| matches!(s.kind, SymbolKind::Ref | SymbolKind::Ident | SymbolKind::Call))
        {
            let Some(name) = s.value.as_deref() else {
                continue;
            };
            let mut current = Some(s.scope);
            while let Some(scope_id) = current {
                if fn_scope_ids.contains(&scope_id) {
                    used_names.entry(scope_id).or_default().insert(name);
                }
                current = ctx.hir.scope(scope_id).and_then(|scope| scope.parent_id);
            }
        }

        for (fn_id, _) in functions {
            let Some(fn_scope_id) = fn_scopes.get(&fn_id) else {
                continue;
            };
            let used_names = used_names.get(fn_scope_id);

            for (_, param_sym) in ctx
                .children(fn_id)
                .filter(|(_, s)| matches!(s.kind, SymbolKind::Parameter))
            {
                let name = match param_sym.value.as_deref() {
                    Some(n) => n,
                    None => continue,
                };

                if name.starts_with('_') || used_names.is_some_and(|names| names.contains(name)) {
                    continue;
                }

                let mut d = Diagnostic::new(LintMessage::UnusedParameter { name: name.to_string() }, self.severity());
                if let Some(range) = param_sym.source.text_range {
                    d = d.with_range(range).with_fix(Fix::literal(range, format!("_{name}")));
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
        let (source_id, _, _) = hir.add_code(None, code);
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
    #[case(r#"def f(name): s"Hello ${name}""#)]
    #[case(r#"def f(*args): s"DEBUG: ${args[0]}";"#)]
    #[case(r#"def f(a, b): s"${a + b}";"#)]
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

    #[test]
    fn fix_prefixes_name_with_underscore() {
        let code = "def f(a, b): a";
        let diags = check(code);
        let fix = diags[0].fix.as_ref().unwrap();
        let (range, replacement) = fix.resolve(code).unwrap();
        assert_eq!(
            crate::fix::apply_edits(code, &[(range, replacement)]),
            "def f(a, _b): a"
        );
    }
}
