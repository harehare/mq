use mq_hir::SymbolKind;

use crate::{Diagnostic, LintContext, LintMessage, LintRule, RuleId, Severity};

pub struct PreferPipeStyle;

impl LintRule for PreferPipeStyle {
    fn id(&self) -> RuleId {
        RuleId::PreferPipeStyle
    }

    fn severity(&self) -> Severity {
        Severity::Style
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        // `f(g(x))` can always be rewritten as the pipe `x | g() | f()`.
        ctx.all_symbols()
            .filter(|(_, sym)| matches!(sym.kind, SymbolKind::Call | SymbolKind::CallDynamic))
            .filter_map(|(outer_id, outer_sym)| {
                let mut args = ctx.all_symbols().filter(|(_, s)| s.parent == Some(outer_id));
                let (_, arg) = args.next()?;
                if args.next().is_some() {
                    return None;
                }
                if !matches!(arg.kind, SymbolKind::Call | SymbolKind::CallDynamic) {
                    return None;
                }

                let outer_name = outer_sym.value.as_deref().unwrap_or("<call>").to_string();
                let inner_name = arg.value.as_deref().unwrap_or("<call>").to_string();

                let mut d = Diagnostic::new(LintMessage::PreferPipeStyle { outer_name, inner_name }, self.severity());
                if let Some(range) = outer_sym.source.text_range {
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
        PreferPipeStyle.check(&ctx)
    }

    #[rstest]
    #[case("to_text(to_upper(x))")]
    #[case("trim(to_text(x))")]
    fn detects_nested_unary_calls(#[case] code: &str) {
        let diags = check(code);
        assert_eq!(diags.len(), 1);
    }

    #[rstest]
    #[case("add(foo(x), y)")]
    #[case("to_text(x)")]
    #[case("x | to_upper() | to_text()")]
    fn no_diagnostic(#[case] code: &str) {
        let diags = check(code);
        assert_eq!(diags.len(), 0);
    }
}
