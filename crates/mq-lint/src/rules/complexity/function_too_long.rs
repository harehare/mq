use crate::{Diagnostic, LintContext, LintRule, Severity};

pub struct FunctionTooLong;

impl LintRule for FunctionTooLong {
    fn id(&self) -> &'static str {
        "function_too_long"
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
                let name = sym.value.as_deref().unwrap_or("<anonymous>");
                let mut d = Diagnostic::new(
                    self.id(),
                    self.severity(),
                    format!("function `{name}` is {line_count} lines long (limit: {max_lines})"),
                );
                d = d.with_range(range);
                Some(d.with_help("consider splitting into smaller, focused helper functions"))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use mq_hir::Hir;

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

    #[test]
    fn no_diagnostic_for_short_function() {
        let diags = check_with_max("def f(): .h1;", 50);
        assert_eq!(diags.len(), 0);
    }
}
