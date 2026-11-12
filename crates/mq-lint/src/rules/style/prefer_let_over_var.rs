use rustc_hash::FxHashSet;

use crate::{Diagnostic, Fix, LintContext, LintMessage, LintRule, RuleId, Severity};
use mq_hir::{SymbolId, SymbolKind};
use mq_lang::Range;

/// Reports `var` bindings that are never reassigned.
pub struct PreferLetOverVar;

impl LintRule for PreferLetOverVar {
    fn id(&self) -> RuleId {
        RuleId::PreferLetOverVar
    }

    fn severity(&self) -> Severity {
        Severity::Warn
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        // Names that are the root of an assignment target: `x = v`, `x[k] = v`, `x[a][b] += v`
        // and `get(x, k) = v`. The target is the first Ident/Ref/Call child by source position.
        let mut reassigned_names: FxHashSet<&str> = FxHashSet::default();
        for (assign_id, _) in ctx.all_symbols().filter(|(_, s)| matches!(s.kind, SymbolKind::Assign)) {
            let Some((target_id, target)) = ctx
                .children(assign_id)
                .filter(|(_, s)| matches!(s.kind, SymbolKind::Ident | SymbolKind::Ref | SymbolKind::Call))
                .filter_map(|(id, s)| s.source.text_range.map(|r| (r.start.line, r.start.column, id, s)))
                .min_by_key(|(line, column, _, _)| (*line, *column))
                .map(|(_, _, id, s)| (id, s))
            else {
                continue;
            };

            match target.kind {
                SymbolKind::Ident | SymbolKind::Ref => reassigned_names.extend(target.value.as_deref()),
                SymbolKind::Call => {
                    reassigned_names.extend(target.value.as_deref());
                    if target.value.as_deref() == Some("get")
                        && let Some((_, first_arg)) = ctx
                            .children(target_id)
                            .filter(|(_, s)| matches!(s.kind, SymbolKind::Ref))
                            .min_by_key(|(_, s)| s.source.text_range.map(|r| (r.start.line, r.start.column)))
                    {
                        reassigned_names.extend(first_arg.value.as_deref());
                    }
                }
                _ => {}
            }
        }

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
            d = d.with_range(kw_range).with_fix(Fix::literal(kw_range, "let"));
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

    #[rstest]
    #[case("var s = {} | s[1] = true | s")]
    #[case("var s = {} | s[\"k\"] += 1 | s")]
    #[case("var s = [[1]] | s[0][0] = 2 | s")]
    #[case("var s = {} | s[a][b] = 1 | s")]
    #[case("var s = {} | s[a][b] -= 1 | s")]
    #[case("var s = [1] | get(s, 0) = 2 | s")]
    #[case("var s = {} | foreach (x, [1]): s[x] = true end | s")]
    #[case("var s = {} | if (true): s[1] = [2] else: s[1] += [2];")]
    #[case("var s = {} | var i = 0 | while (i < 2): s[i] = true | i += 1 end | s")]
    fn no_diagnostic_for_index_assign(#[case] code: &str) {
        let diags = check(code);
        assert!(
            diags.iter().all(|d| !d.message().contains("`s`")),
            "{code}: {:?}",
            diags.iter().map(|d| d.message()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn fix_replaces_var_with_let() {
        let code = "var x = .h1 | x";
        let diags = check(code);
        let fix = diags[0].fix.as_ref().unwrap();
        let (range, replacement) = fix.resolve(code).unwrap();
        assert_eq!(
            crate::fix::apply_edits(code, &[(range, replacement)]),
            "let x = .h1 | x"
        );
    }
}
