use crate::{Diagnostic, Fix, LintContext, LintMessage, LintRule, RuleId, Severity};
use mq_hir::SymbolKind;
use rustc_hash::FxHashMap;

pub struct UnnecessaryInterpolation;

impl LintRule for UnnecessaryInterpolation {
    fn id(&self) -> RuleId {
        RuleId::UnnecessaryInterpolation
    }

    fn severity(&self) -> Severity {
        Severity::Style
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let mut child_counts: FxHashMap<mq_hir::SymbolId, usize> = FxHashMap::default();

        for (_, sym) in ctx.all_symbols() {
            if let Some(parent_id) = sym.parent {
                *child_counts.entry(parent_id).or_insert(0) += 1;
            }
        }

        ctx.all_symbols()
            .filter(|(_, sym)| matches!(sym.kind, SymbolKind::InterpolatedString))
            .filter_map(|(sym_id, sym)| {
                let count = child_counts.get(&sym_id).copied().unwrap_or(0);
                if count != 1 {
                    return None;
                }
                let range = sym.source.text_range?;
                let mut d = Diagnostic::new(LintMessage::UnnecessaryInterpolation, self.severity()).with_range(range);

                // The lone segment's text is already captured verbatim as a `Variable` symbol's
                // value, so use it directly rather than slicing `${...}`'s span (which includes
                // the delimiters).
                let expr_text = ctx
                    .all_symbols()
                    .find(|(_, s)| s.parent == Some(sym_id) && matches!(s.kind, SymbolKind::Variable))
                    .and_then(|(_, s)| s.value.clone());
                if let Some(expr_text) = expr_text {
                    d = d.with_fix(Fix::literal(range, expr_text));
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
        UnnecessaryInterpolation.check(&ctx)
    }

    #[rstest]
    #[case(r#"s"${x}""#)]
    #[case(r#"s"${.h1}""#)]
    fn detects_unnecessary_interpolation(#[case] code: &str) {
        let diags = check(code);
        assert_eq!(diags.len(), 1);
    }

    #[rstest]
    #[case(r#"s"${x}""#, "x")]
    #[case(r#"s"${.h1}""#, ".h1")]
    fn fix_replaces_interpolation_with_inner_expr(#[case] code: &str, #[case] expected: &str) {
        let diags = check(code);
        let fix = diags[0].fix.as_ref().unwrap();
        let (range, replacement) = fix.resolve(code).unwrap();
        assert_eq!(crate::fix::apply_edits(code, &[(range, replacement)]), expected);
    }

    #[rstest]
    #[case(r#"s"${x}${y}""#)]
    #[case(r#"s"prefix ${x}""#)]
    #[case(r#"s"${x} suffix""#)]
    fn no_diagnostic(#[case] code: &str) {
        let diags = check(code);
        assert_eq!(diags.len(), 0);
    }
}
