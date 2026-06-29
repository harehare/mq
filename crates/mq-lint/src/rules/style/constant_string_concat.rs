use crate::{Diagnostic, LintContext, LintMessage, LintRule, RuleId, Severity};
use mq_hir::SymbolKind;

pub struct ConstantStringConcat;

impl LintRule for ConstantStringConcat {
    fn id(&self) -> RuleId {
        RuleId::ConstantStringConcat
    }

    fn severity(&self) -> Severity {
        Severity::Style
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        ctx.all_symbols()
            .filter(|(_, sym)| matches!(sym.kind, SymbolKind::BinaryOp) && sym.value.as_deref() == Some("+"))
            .filter_map(|(op_id, op_sym)| {
                let children: Vec<_> = ctx.all_symbols().filter(|(_, s)| s.parent == Some(op_id)).collect();

                if children.len() < 2 || !children.iter().all(|(_, s)| matches!(s.kind, SymbolKind::String)) {
                    return None;
                }

                let mut d = Diagnostic::new(LintMessage::ConstantStringConcat, self.severity());
                if let Some(range) = op_sym.source.text_range {
                    d = d.with_range(range);
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
        ConstantStringConcat.check(&ctx)
    }

    #[rstest]
    #[case(r#""hello" + " world""#)]
    #[case(r#""foo" + "bar""#)]
    fn detects_string_literal_concat(#[case] code: &str) {
        let diags = check(code);
        assert_eq!(diags.len(), 1);
    }

    #[rstest]
    #[case(r#""hello" + to_text()"#)]
    #[case(r#"x + "world""#)]
    #[case(r#"1 + 2"#)]
    fn no_diagnostic_for_non_literal_concat(#[case] code: &str) {
        let diags = check(code);
        assert_eq!(diags.len(), 0);
    }
}
