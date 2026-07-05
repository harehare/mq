use mq_hir::SymbolKind;

use crate::{Diagnostic, Fix, LintContext, LintMessage, LintRule, RuleId, Severity};

pub struct RedundantTry;

impl LintRule for RedundantTry {
    fn id(&self) -> RuleId {
        RuleId::RedundantTry
    }

    fn severity(&self) -> Severity {
        Severity::Style
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        // `try: <expr> catch: none` is exactly what `?` does.
        ctx.all_symbols()
            .filter(|(_, sym)| matches!(sym.kind, SymbolKind::Try))
            .filter_map(|(try_id, try_sym)| {
                let (catch_id, catch_sym) = ctx
                    .all_symbols()
                    .find(|(_, s)| s.parent == Some(try_id) && matches!(s.kind, SymbolKind::Catch))?;
                let (catch_expr_id, catch_expr) = ctx.all_symbols().find(|(_, s)| s.parent == Some(catch_id))?;

                let is_none_literal =
                    matches!(catch_expr.kind, SymbolKind::None) || catch_expr.value.as_deref() == Some("none");
                let has_no_children = !ctx.all_symbols().any(|(_, s)| s.parent == Some(catch_expr_id));

                if !is_none_literal || !has_no_children {
                    return None;
                }

                let mut d = Diagnostic::new(LintMessage::RedundantTry, self.severity());

                // The try body is the one child of `try_id` that isn't `catch`; bound its end by
                // where `catch` starts, since a compound body's trailing `)` isn't tracked by any
                // HIR symbol.
                let body_start = ctx
                    .all_symbols()
                    .find(|(id, s)| s.parent == Some(try_id) && *id != catch_id)
                    .and_then(|(id, _)| ctx.full_range(id))
                    .map(|r| r.start);

                if let (Some(try_start), Some(catch_expr_range), Some(body_start), Some(catch_start)) = (
                    try_sym.source.text_range,
                    catch_expr.source.text_range,
                    body_start,
                    catch_sym.source.text_range,
                ) {
                    let range = mq_lang::Range {
                        start: try_start.start,
                        end: catch_expr_range.end,
                    };
                    let body_range = mq_lang::Range {
                        start: body_start,
                        end: catch_start.start,
                    };
                    d = d
                        .with_range(range)
                        .with_fix(Fix::verbatim(range, body_range).with_suffix("?"));
                } else if let Some(range) = try_sym.source.text_range {
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
        RedundantTry.check(&ctx)
    }

    #[rstest]
    #[case(r#"try: get("x") catch: none"#)]
    fn detects_catch_none(#[case] code: &str) {
        let diags = check(code);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn fix_replaces_try_catch_with_error_suppression_operator() {
        let code = r#"try: get("x") catch: none"#;
        let diags = check(code);
        let fix = diags[0].fix.as_ref().unwrap();
        let (range, replacement) = fix.resolve(code).unwrap();
        assert_eq!(range, diags[0].range.unwrap());
        assert_eq!(crate::fix::apply_edits(code, &[(range, replacement)]), r#"get("x")?"#);
    }

    #[rstest]
    #[case(r#"try: get("x") catch: "default""#)]
    #[case(r#"try: get("x") catch: 0"#)]
    fn no_diagnostic(#[case] code: &str) {
        let diags = check(code);
        assert_eq!(diags.len(), 0);
    }
}
