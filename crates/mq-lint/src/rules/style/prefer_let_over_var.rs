use rustc_hash::FxHashSet;

use crate::{Diagnostic, LintContext, LintMessage, LintRule, RuleId, Severity};
use mq_hir::{SymbolId, SymbolKind};
use mq_lang::Range;

pub struct PreferLetOverVar;

impl LintRule for PreferLetOverVar {
    fn id(&self) -> RuleId {
        RuleId::PreferLetOverVar
    }

    fn severity(&self) -> Severity {
        Severity::Warn
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        // Collect names that appear as the LHS of an Assign node (re-assigned variables).
        let reassigned_names: FxHashSet<&str> = ctx
            .all_symbols()
            .filter(|(_, s)| matches!(s.kind, SymbolKind::Assign))
            .flat_map(|(assign_id, _)| {
                ctx.all_symbols()
                    .filter(move |(_, s)| {
                        s.parent == Some(assign_id) && matches!(s.kind, SymbolKind::Ident | SymbolKind::Ref)
                    })
                    .filter_map(|(_, s)| s.value.as_deref())
                    .collect::<Vec<_>>()
            })
            .collect();

        let mut diagnostics = Vec::new();

        // Sort all source symbols by position for sequential pairing.
        let mut syms: Vec<(SymbolId, Range, &mq_hir::Symbol)> = ctx
            .all_symbols()
            .filter_map(|(id, s)| s.source.text_range.map(|r| (id, r, s)))
            .collect();
        syms.sort_by_key(|(_, r, _)| (r.start.line, r.start.column));

        // Find Keyword("var") symbols and pair each with the next Variable in the same scope.
        for i in 0..syms.len() {
            let (_, kw_range, kw_sym) = syms[i];
            if !matches!(kw_sym.kind, SymbolKind::Keyword) || kw_sym.value.as_deref() != Some("var") {
                continue;
            }

            // The Variable for this `var` is the next Variable after the keyword
            // in the same scope (same parent) at a later position.
            let var_sym = syms[i + 1..].iter().find(|(_, _, s)| {
                s.parent == kw_sym.parent && matches!(s.kind, SymbolKind::Variable) && s.scope == kw_sym.scope
            });

            let Some((_, _, var_sym)) = var_sym else {
                continue;
            };
            let name = match var_sym.value.as_deref() {
                Some(n) => n,
                None => continue,
            };

            if reassigned_names.contains(name) {
                continue;
            }

            let mut d = Diagnostic::new(
                LintMessage::PreferLetOverVar { name: name.to_string() },
                self.severity(),
            );
            d = d.with_range(kw_range);
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
        let (source_id, _) = hir.add_code(None, code);
        let config = LintConfig::default();
        let ctx = LintContext::new(&hir, source_id, &config);
        PreferLetOverVar.check(&ctx)
    }

    #[rstest]
    #[case("var x = .h1 | x")]
    #[case("var my_val = .h2 | my_val")]
    fn detects_var_never_reassigned(#[case] code: &str) {
        let diags = check(code);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message().contains("prefer `let` over `var`"));
    }

    #[rstest]
    #[case("let x = .h1 | x")]
    #[case(".h1 | .value")]
    fn no_diagnostic(#[case] code: &str) {
        let diags = check(code);
        assert_eq!(diags.len(), 0);
    }
}
