use crate::{Diagnostic, Fix, LintContext, LintMessage, LintRule, RuleId, Severity};
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

            let Some((bool_id, bool_sym)) = bool_child else {
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
            // The other operand: the child of `bop_id` that isn't the boolean literal. Bound its
            // end by the operator's own start when it precedes the operator, since a compound
            // operand's trailing `)` isn't tracked by any HIR symbol; `Fix::resolve` trims the
            // resulting whitespace.
            let operand_id = ctx
                .all_symbols()
                .find(|(id, s)| s.parent == Some(bop_id) && *id != bool_id)
                .map(|(id, _)| id);
            let operand_range = operand_id
                .and_then(|id| ctx.full_range(id))
                .map(|r| match bop_sym.source.text_range {
                    Some(op_range) if r.start <= op_range.start => mq_lang::Range {
                        start: r.start,
                        end: op_range.start,
                    },
                    _ => r,
                });

            if let (Some(bool_range), Some(operand_range)) = (ctx.full_range(bool_id), operand_range) {
                let range = crate::fix::union(bool_range, operand_range);
                d = d.with_range(range);

                // `== true` / `!= false` keep the operand as-is; `== false` / `!= true` negate it.
                let negate = (op == "==" && bool_val == "false") || (op == "!=" && bool_val == "true");
                let mut fix = Fix::verbatim(range, operand_range);
                if negate {
                    fix = fix.with_prefix("!");
                }
                d = d.with_fix(fix);
            } else if let Some(range) = bop_sym.source.text_range {
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

    #[rstest]
    #[case(".checked == true", ".checked")]
    #[case(".checked == false", "!.checked")]
    #[case(".checked != true", "!.checked")]
    #[case(".checked != false", ".checked")]
    fn fix_simplifies_boolean_comparison(#[case] code: &str, #[case] expected: &str) {
        let diags = check(code);
        let fix = diags[0].fix.as_ref().unwrap();
        let (range, replacement) = fix.resolve(code).unwrap();
        assert_eq!(crate::fix::apply_edits(code, &[(range, replacement)]), expected);
    }

    #[test]
    fn fix_keeps_trailing_call_parens_when_operand_ends_in_punctuation() {
        // The operand's own HIR range doesn't track its closing `)`, so the fix must recover it
        // rather than truncating to `to_bool(x`.
        let code = r#"to_bool(x) == false"#;
        let diags = check(code);
        let fix = diags[0].fix.as_ref().unwrap();
        let (range, replacement) = fix.resolve(code).unwrap();
        assert_eq!(crate::fix::apply_edits(code, &[(range, replacement)]), "!to_bool(x)");
    }
}
