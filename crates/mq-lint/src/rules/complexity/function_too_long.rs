use crate::{Diagnostic, LintContext, LintMessage, LintRule, RuleId, Severity};

pub struct FunctionTooLong;

impl LintRule for FunctionTooLong {
    fn id(&self) -> RuleId {
        RuleId::FunctionTooLong
    }

    fn severity(&self) -> Severity {
        Severity::Warn
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let max_lines = ctx.config.complexity.function_max_lines;

        ctx.hir
            .symbols_for_source(ctx.source_id)
            .filter_map(|(_, sym)| {
                if !sym.is_function() {
                    return None;
                }
                let range = sym.source.text_range?;
                let line_count = (range.end.line - range.start.line + 1) as usize;
                if line_count <= max_lines {
                    return None;
                }
                let name = sym.value.as_deref().unwrap_or("<anonymous>").to_string();
                let mut d = Diagnostic::new(
                    LintMessage::FunctionTooLong {
                        name,
                        line_count,
                        max_lines,
                    },
                    self.severity(),
                );
                d = d.with_range(range);
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

    fn check_with_max(code: &str, max: usize) -> Vec<Diagnostic> {
        let mut hir = Hir::default();
        let (source_id, _) = hir.add_code(None, code);
        let mut config = LintConfig::default();
        config.complexity.function_max_lines = max;
        let ctx = LintContext::new(&hir, source_id, &config);
        FunctionTooLong.check(&ctx)
    }

    #[rstest]
    #[case("def f(): .h1 | .h2 | .h3;", 0)]
    #[case("def g(): .h1;", 0)]
    fn detects_function_too_long(#[case] code: &str, #[case] max: usize) {
        let diags = check_with_max(code, max);
        assert_eq!(diags.len(), 1);
    }

    #[rstest]
    #[case("def f(): .h1;", 50)]
    #[case("def f(): .h1 | .h2;", 10)]
    fn no_diagnostic_for_short_function(#[case] code: &str, #[case] max: usize) {
        let diags = check_with_max(code, max);
        assert_eq!(diags.len(), 0);
    }
}
