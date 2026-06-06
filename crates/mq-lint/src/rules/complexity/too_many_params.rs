use crate::{Diagnostic, LintContext, LintRule, Severity};
use mq_hir::SymbolKind;

pub struct TooManyParams;

impl LintRule for TooManyParams {
    fn id(&self) -> &'static str {
        "too_many_params"
    }

    fn severity(&self) -> Severity {
        Severity::Warn
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let max = ctx.config.complexity.max_params;

        // Function symbols use add_symbol; also cover all_symbols for completeness
        ctx.all_symbols()
            .filter_map(|(_, sym)| {
                let params = match &sym.kind {
                    SymbolKind::Function(p) => p,
                    _ => return None,
                };
                if params.len() <= max {
                    return None;
                }
                let name = sym.value.as_deref().unwrap_or("<anonymous>");
                let count = params.len();
                let mut d = Diagnostic::new(
                    self.id(),
                    self.severity(),
                    format!("function `{name}` has {count} parameters (limit: {max})"),
                );
                if let Some(range) = sym.source.text_range {
                    d = d.with_range(range);
                }
                Some(d.with_help("consider grouping related parameters or using default arguments"))
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
        config.complexity.max_params = max;
        let ctx = LintContext::new(&hir, source_id, &config);
        TooManyParams.check(&ctx)
    }

    #[test]
    fn detects_too_many_params() {
        let diags = check_with_max("def f(a, b, c): a", 2);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("3 parameters"));
    }

    #[test]
    fn no_diagnostic_within_limit() {
        let diags = check_with_max("def f(a, b): a", 2);
        assert_eq!(diags.len(), 0);
    }
}
