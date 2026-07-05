use crate::{Diagnostic, Fix, LintContext, LintMessage, LintRule, RuleId, Severity};
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
            .filter_map(|(op_id, _)| {
                let mut children: Vec<_> = ctx.all_symbols().filter(|(_, s)| s.parent == Some(op_id)).collect();

                if children.len() < 2 || !children.iter().all(|(_, s)| matches!(s.kind, SymbolKind::String)) {
                    return None;
                }
                children.sort_by_key(|(_, s)| s.source.text_range.map(|r| (r.start.line, r.start.column)));

                let mut d = Diagnostic::new(LintMessage::ConstantStringConcat, self.severity());
                if let (Some(first), Some(last)) = (
                    children.first().and_then(|(_, s)| s.source.text_range),
                    children.last().and_then(|(_, s)| s.source.text_range),
                ) {
                    let range = crate::fix::union(first, last);
                    let combined: String = children.iter().filter_map(|(_, s)| s.value.as_deref()).collect();
                    d = d
                        .with_range(range)
                        .with_fix(Fix::literal(range, format!("\"{}\"", escape_string_literal(&combined))));
                }
                Some(d)
            })
            .collect()
    }
}

/// Escapes `\` and `"` so `s` round-trips as a valid mq string literal body.
fn escape_string_literal(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            _ => out.push(c),
        }
    }
    out
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

    #[rstest]
    #[case(r#""hello" + " world""#, r#""hello world""#)]
    #[case(r#""foo" + "bar""#, r#""foobar""#)]
    fn fix_combines_string_literals(#[case] code: &str, #[case] expected: &str) {
        let diags = check(code);
        let fix = diags[0].fix.as_ref().unwrap();
        let (range, replacement) = fix.resolve(code).unwrap();
        assert_eq!(crate::fix::apply_edits(code, &[(range, replacement)]), expected);
    }

    #[test]
    fn fix_escapes_quotes_and_backslashes_in_combined_literal() {
        let code = r#""a\"b" + "c\\d""#;
        let diags = check(code);
        let fix = diags[0].fix.as_ref().unwrap();
        let (range, replacement) = fix.resolve(code).unwrap();
        let fixed = crate::fix::apply_edits(code, &[(range, replacement)]);
        assert_eq!(fixed, r#""a\"bc\\d""#);

        // The escaped literal must itself lex back to the original concatenated value.
        let mut hir = mq_hir::Hir::default();
        let (source_id, _) = hir.add_code(None, &fixed);
        let sym = hir
            .symbols()
            .find(|(_, s)| s.source.source_id == Some(source_id) && matches!(s.kind, SymbolKind::String))
            .map(|(_, s)| s.value.clone().unwrap());
        assert_eq!(sym.as_deref(), Some("a\"bc\\d"));
    }
}
