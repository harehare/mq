use crate::{Diagnostic, LintContext, LintMessage, LintRule, RuleId, Severity};
use mq_hir::SymbolKind;

pub struct BooleanComparison;

impl LintRule for BooleanComparison {
    fn id(&self) -> RuleId {
        RuleId::BooleanComparison
    }

    fn severity(&self) -> Severity {
        Severity::Style
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // BinaryOp uses add_symbol; Boolean child may use insert_symbol.
        // Use all_symbols for both.
        for (bop_id, bop_sym) in ctx.all_symbols() {
            if !matches!(bop_sym.kind, SymbolKind::BinaryOp) {
                continue;
            }
            let op = bop_sym.value.as_deref().unwrap_or("");
            if op != "==" && op != "!=" {
                continue;
            }

            let bool_child = ctx
                .all_symbols()
                .find(|(_, s)| s.parent == Some(bop_id) && matches!(s.kind, SymbolKind::Boolean));

            let Some((_, bool_sym)) = bool_child else {
                continue;
            };

            let bool_val = bool_sym.value.as_deref().unwrap_or("true");

            let mut d = Diagnostic::new(
                LintMessage::BooleanComparison {
                    op: op.to_string(),
                    bool_val: bool_val.to_string(),
                },
                self.severity(),
            );
            if let Some(range) = bop_sym.source.text_range {
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
        let (source_id, _) = hir.add_code(None, code);
        let config = LintConfig::default();
        let ctx = LintContext::new(&hir, source_id, &config);
        BooleanComparison.check(&ctx)
    }

    #[rstest]
    #[case(".checked == true")]
    #[case(".checked == false")]
    #[case(".checked != true")]
    #[case(".checked != false")]
    fn detects_boolean_comparison(#[case] code: &str) {
        let diags = check(code);
        assert_eq!(diags.len(), 1);
    }

    #[rstest]
    #[case(r#".type == "heading""#)]
    #[case(".h1 | .checked")]
    fn no_diagnostic(#[case] code: &str) {
        let diags = check(code);
        assert_eq!(diags.len(), 0);
    }
}
