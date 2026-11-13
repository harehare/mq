use rustc_hash::FxHashSet;

use crate::{Diagnostic, LintContext, LintMessage, LintRule, RuleId, Severity};
use mq_hir::{ScopeId, SymbolKind};

/// Reports assignments (`=`, `+=`, ...) to a binding declared with `let`.
pub struct AssignToImmutable;

impl LintRule for AssignToImmutable {
    fn id(&self) -> RuleId {
        RuleId::AssignToImmutable
    }

    fn severity(&self) -> Severity {
        Severity::Error
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // A reference resolves to the first declaration of a rebound name, so skip names
        // that some `var` in the same scope also declares.
        let rebound_mutable: FxHashSet<(ScopeId, &str)> = ctx
            .all_symbols()
            .filter(|(id, s)| s.kind == SymbolKind::Variable && ctx.hir.is_mutable_variable(*id))
            .filter_map(|(_, s)| Some((s.scope, s.value.as_deref()?)))
            .collect();

        for (assign_id, assign) in ctx.all_symbols() {
            if assign.kind != SymbolKind::Assign {
                continue;
            }
            // The first child is the target: `x`, or `x[k]` lowered as a call to `x`.
            let Some((target_id, target)) = ctx.children(assign_id).next() else {
                continue;
            };
            if !matches!(target.kind, SymbolKind::Ref | SymbolKind::Call) {
                continue;
            }
            let Some(name) = target.value.as_deref() else {
                continue;
            };
            let Some(def_id) = ctx.hir.resolve_reference_symbol(target_id) else {
                continue;
            };
            let Some(def) = ctx.hir.symbol(def_id) else {
                continue;
            };
            if def.kind != SymbolKind::Variable
                || ctx.hir.is_mutable_variable(def_id)
                || rebound_mutable.contains(&(def.scope, name))
            {
                continue;
            }

            let mut d = Diagnostic::new(
                LintMessage::AssignToImmutable { name: name.to_string() },
                self.severity(),
            );
            if let Some(range) = target.source.text_range {
                d = d.with_range(range);
            }
            diagnostics.push(d);
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
        AssignToImmutable.check(&ctx)
    }

    #[rstest]
    #[case("let x = 1 | x = 2")]
    #[case("let x = 1 | x += 1")]
    #[case("let a = [1, 2] | a[0] = 3")]
    #[case("let d = {\"k\": 1} | d[\"k\"] = 2")]
    #[case("1 as x | x = 2")]
    #[case("let x = 1 | def f(): x = 2; end")]
    fn detects_assign_to_let(#[case] code: &str) {
        assert_eq!(check(code).len(), 1, "{code}");
    }

    #[rstest]
    #[case("var x = 1 | x = 2")]
    #[case("var x = 1 | x += 1")]
    #[case("var a = [1, 2] | a[0] = 3")]
    #[case("let x = 1 | var x = 2 | x = 3")]
    #[case("let x = 1 | x")]
    fn no_diagnostic(#[case] code: &str) {
        assert_eq!(check(code).len(), 0, "{code}");
    }
}
