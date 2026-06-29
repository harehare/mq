use crate::{Diagnostic, LintContext, LintMessage, LintRule, RuleId, Severity};
use mq_hir::{ScopeId, ScopeKind};

pub struct DeeplyNested;

impl LintRule for DeeplyNested {
    fn id(&self) -> RuleId {
        RuleId::DeeplyNested
    }

    fn severity(&self) -> Severity {
        Severity::Warn
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let max_depth = ctx.config.complexity.max_nesting_depth;
        let mut diagnostics = Vec::new();

        for (scope_id, scope) in ctx.hir.scopes() {
            // Only consider scopes belonging to this source
            if scope.source.source_id != Some(ctx.source_id) {
                continue;
            }
            // Module scopes are depth 0; skip them
            if scope.is_module() {
                continue;
            }

            let depth = scope_depth(ctx, scope_id);
            if depth <= max_depth {
                continue;
            }

            // Report at the symbol that owns this scope
            let range = match &scope.kind {
                ScopeKind::Function(sym_id)
                | ScopeKind::Block(sym_id)
                | ScopeKind::Loop(sym_id)
                | ScopeKind::MatchArm(sym_id)
                | ScopeKind::Let(sym_id) => ctx.hir.symbol(*sym_id).and_then(|s| s.source.text_range),
                ScopeKind::Module(_) => None,
            };

            let mut d = Diagnostic::new(LintMessage::DeeplyNested { depth, max_depth }, self.severity());
            if let Some(r) = range {
                d = d.with_range(r);
            }
            diagnostics.push(d);
        }

        diagnostics
    }
}

/// Returns the nesting depth of `scope_id`, counting non-module parent scopes.
fn scope_depth(ctx: &LintContext<'_>, scope_id: ScopeId) -> usize {
    let mut depth = 0;
    let mut current = ctx.hir.scope(scope_id).and_then(|s| s.parent_id);
    while let Some(sid) = current {
        let scope = match ctx.hir.scope(sid) {
            Some(s) => s,
            None => break,
        };
        if !scope.is_module() {
            depth += 1;
        }
        current = scope.parent_id;
    }
    depth
}

#[cfg(test)]
mod tests {
    use mq_hir::Hir;
    use rstest::rstest;

    use super::*;
    use crate::{LintConfig, LintContext};

    fn check_with_max(code: &str, max: usize) -> Vec<Diagnostic> {
        let mut hir = Hir::default();
        let (source_id, _) = hir.add_code(None, code);
        let mut config = LintConfig::default();
        config.complexity.max_nesting_depth = max;
        let ctx = LintContext::new(&hir, source_id, &config);
        DeeplyNested.check(&ctx)
    }

    #[rstest]
    #[case("def f(): if (true): if (true): 1 else: 2 else: 3;", 1)]
    #[case("loop if (true): if (true): 1 else: 2 else: 3 end", 1)]
    fn detects_deeply_nested(#[case] code: &str, #[case] max: usize) {
        let diags = check_with_max(code, max);
        assert!(!diags.is_empty());
    }

    #[rstest]
    #[case("def f(): .h1;", 4)]
    #[case(".h1 | .h2", 4)]
    fn no_diagnostic_for_shallow_code(#[case] code: &str, #[case] max: usize) {
        let diags = check_with_max(code, max);
        assert_eq!(diags.len(), 0);
    }
}
