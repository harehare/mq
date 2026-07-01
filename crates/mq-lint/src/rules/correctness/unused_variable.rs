use rustc_hash::FxHashSet;

use crate::{Diagnostic, Fix, LintContext, LintMessage, LintRule, RuleId, Severity};
use mq_hir::SymbolKind;

pub struct UnusedVariable;

/// True if `sym` is a `${name}` interpolation embed rather than an actual `let`/`var` declaration
/// (both reuse `SymbolKind::Variable`; see `Hir::add_interpolated_string`).
fn is_interpolation_embed(ctx: &LintContext<'_>, sym: &mq_hir::Symbol) -> bool {
    sym.parent
        .and_then(|id| ctx.hir.symbol(id))
        .is_some_and(|p| matches!(p.kind, SymbolKind::InterpolatedString))
}

impl LintRule for UnusedVariable {
    fn id(&self) -> RuleId {
        RuleId::UnusedVariable
    }

    fn severity(&self) -> Severity {
        Severity::Warn
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        // Build a set of all names referenced by Ref/Ident/Call symbols, plus bare `${name}`
        // interpolation embeds (which reuse `SymbolKind::Variable` for the embedded text).
        let used_names: FxHashSet<&str> = ctx
            .all_symbols()
            .filter(|(_, s)| {
                matches!(s.kind, SymbolKind::Ref | SymbolKind::Ident | SymbolKind::Call)
                    || is_interpolation_embed(ctx, s)
            })
            .filter_map(|(_, s)| s.value.as_deref())
            .collect();

        ctx.all_symbols()
            .filter(|(_, s)| matches!(s.kind, SymbolKind::Variable))
            .filter_map(|(_, sym)| {
                let name = sym.value.as_deref()?;
                // Variables prefixed with `_` are intentionally unused
                if name.starts_with('_') {
                    return None;
                }
                // An interpolation embed is itself a reference, not a declaration, so it can't be
                // "unused" or renamed.
                if is_interpolation_embed(ctx, sym) {
                    return None;
                }
                if used_names.contains(name) {
                    return None;
                }
                let mut d = Diagnostic::new(LintMessage::UnusedVariable { name: name.to_string() }, self.severity());
                if let Some(range) = sym.source.text_range {
                    d = d.with_range(range).with_fix(Fix::literal(range, format!("_{name}")));
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
        UnusedVariable.check(&ctx)
    }

    #[rstest]
    #[case("let x = .h1", "unused variable `x`")]
    #[case("let my_var = .h2", "unused variable `my_var`")]
    fn detects_unused_variable(#[case] code: &str, #[case] msg: &str) {
        let diags = check(code);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message().contains(msg));
    }

    #[rstest]
    #[case("let x = .h1 | x")]
    #[case("let _x = .h1")]
    #[case("let _ignored = .h1")]
    #[case(r#"s"${x}""#)]
    #[case(r#"let x = 1 | s"${x}""#)]
    #[case("let a = [1, 2, 3] | [...a]")]
    #[case("let base = {x: 1} | {...base, y: 2}")]
    fn no_diagnostic(#[case] code: &str) {
        let diags = check(code);
        assert_eq!(diags.len(), 0);
    }

    #[test]
    fn fix_prefixes_name_with_underscore() {
        let code = "let x = .h1";
        let diags = check(code);
        let fix = diags[0].fix.as_ref().unwrap();
        let (range, replacement) = fix.resolve(code).unwrap();
        assert_eq!(crate::fix::apply_edits(code, &[(range, replacement)]), "let _x = .h1");
    }
}
