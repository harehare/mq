use crate::{Diagnostic, LintContext, LintRule, Severity};
use mq_hir::SymbolKind;

pub struct BooleanComparison;

impl LintRule for BooleanComparison {
    fn id(&self) -> &'static str {
        "boolean_comparison"
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
            let help = match (op, bool_val) {
                ("==", "true") => "use the value directly instead of `== true`",
                ("==", "false") => "use `!` prefix instead of `== false`",
                ("!=", "true") => "use `!` prefix instead of `!= true`",
                ("!=", "false") => "use the value directly instead of `!= false`",
                _ => "simplify this boolean comparison",
            };

            let mut d = Diagnostic::new(
                self.id(),
                self.severity(),
                format!("unnecessary comparison with boolean literal `{bool_val}`"),
            );
            if let Some(range) = bop_sym.source.text_range {
                d = d.with_range(range);
            }
            diagnostics.push(d.with_help(help));
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use mq_hir::Hir;

    use super::*;
    use crate::{LintConfig, LintContext};

    fn check(code: &str) -> Vec<Diagnostic> {
        let mut hir = Hir::default();
        let (source_id, _) = hir.add_code(None, code);
        let config = LintConfig::default();
        let ctx = LintContext::new(&hir, source_id, &config);
        BooleanComparison.check(&ctx)
    }

    #[test]
    fn no_diagnostic_for_non_boolean_comparison() {
        let diags = check(r#".type == "heading""#);
        assert_eq!(diags.len(), 0);
    }
}
