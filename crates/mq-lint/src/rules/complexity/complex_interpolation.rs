use crate::{Diagnostic, LintContext, LintRule, Severity};
use mq_hir::SymbolKind;

pub struct ComplexInterpolation;

impl LintRule for ComplexInterpolation {
    fn id(&self) -> &'static str {
        "complex_interpolation"
    }

    fn severity(&self) -> Severity {
        Severity::Warn
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let max_exprs = ctx.config.complexity.max_interpolation_exprs;

        ctx.hir
            .symbols_for_source(ctx.source_id)
            .filter(|(_, s)| matches!(s.kind, SymbolKind::InterpolatedString))
            .filter_map(|(interp_id, sym)| {
                // Variable children represent `${...}` interpolated expressions.
                let expr_count = ctx
                    .all_symbols()
                    .filter(|(_, s)| s.parent == Some(interp_id) && matches!(s.kind, SymbolKind::Variable))
                    .count();

                if expr_count <= max_exprs {
                    return None;
                }

                let mut d = Diagnostic::new(
                    self.id(),
                    self.severity(),
                    format!("interpolated string has {expr_count} interpolated expressions (limit: {max_exprs})"),
                );
                if let Some(range) = sym.source.text_range {
                    d = d.with_range(range);
                }
                Some(d.with_help("consider extracting parts into intermediate `let` bindings for readability"))
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
        config.complexity.max_interpolation_exprs = max;
        let ctx = LintContext::new(&hir, source_id, &config);
        ComplexInterpolation.check(&ctx)
    }

    #[test]
    fn detects_too_many_interpolations() {
        // 4 interpolated expressions, limit 3
        let diags = check_with_max(r#"s"${.h1} ${.h2} ${.h3} ${.h4}""#, 3);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn no_diagnostic_within_limit() {
        let diags = check_with_max(r#"s"${.h1} ${.h2}""#, 3);
        assert_eq!(diags.len(), 0);
    }
}
