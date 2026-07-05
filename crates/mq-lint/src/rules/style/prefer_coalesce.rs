use mq_hir::{Symbol, SymbolId, SymbolKind};

use crate::fix::Core;
use crate::{Diagnostic, Fix, LintContext, LintMessage, LintRule, RuleId, Severity};

pub struct PreferCoalesce;

/// True if `id` has no children (e.g. a bare `Ref`/`Selector`, not a call).
fn is_leaf(ctx: &LintContext<'_>, id: SymbolId) -> bool {
    !ctx.all_symbols().any(|(_, s)| s.parent == Some(id))
}

fn is_none_literal(sym: &Symbol) -> bool {
    matches!(sym.kind, SymbolKind::None) || sym.value.as_deref() == Some("none")
}

fn children_sorted_by_position<'a>(ctx: &'a LintContext<'_>, parent: SymbolId) -> Vec<(SymbolId, &'a Symbol)> {
    let mut children: Vec<_> = ctx.all_symbols().filter(|(_, s)| s.parent == Some(parent)).collect();
    children.sort_by_key(|(_, s)| s.source.text_range.map(|r| (r.start.line, r.start.column)));
    children
}

/// A confirmed `if (x <op> none): ... else: ...` null-check, equivalent to `x ?? fallback`.
struct CoalesceMatch {
    /// `"=="` or `"!="`.
    op: &'static str,
    then_id: SymbolId,
    else_kw_id: SymbolId,
    else_expr_id: SymbolId,
    /// Whichever branch (`then_id` or `else_expr_id`) repeats the null-checked value; it's
    /// confirmed to be a leaf, so its range is safe to use verbatim.
    value_branch_id: SymbolId,
}

/// Detects `if (x == none): fallback else: x` and its `!=` form, both equivalent to `x ?? fallback`.
fn find_coalesce_match(ctx: &LintContext<'_>, if_id: SymbolId) -> Option<CoalesceMatch> {
    let children = children_sorted_by_position(ctx, if_id);
    let &(cond_id, cond) = children.first()?;
    let &(then_id, _) = children.get(1)?;
    let &(else_kw_id, _) = children.iter().find(|(_, s)| matches!(s.kind, SymbolKind::Else))?;
    let &(else_expr_id, _) = children_sorted_by_position(ctx, else_kw_id).first()?;

    if !matches!(cond.kind, SymbolKind::BinaryOp) {
        return None;
    }
    let op = cond.value.as_deref()?;
    let op: &'static str = match op {
        "==" => "==",
        "!=" => "!=",
        _ => return None,
    };

    let cond_children = children_sorted_by_position(ctx, cond_id);
    let (&(lhs_id, lhs), &(rhs_id, rhs)) = (cond_children.first()?, cond_children.get(1)?);

    // Whichever side isn't the `none` literal is the value being null-checked.
    let value_id = if is_none_literal(rhs) && is_leaf(ctx, lhs_id) {
        lhs_id
    } else if is_none_literal(lhs) && is_leaf(ctx, rhs_id) {
        rhs_id
    } else {
        return None;
    };
    let value_sym = ctx.hir.symbol(value_id)?;

    // `==`: the value must be repeated in the else-branch; `!=`: in the then-branch.
    let value_branch_id = if op == "==" { else_expr_id } else { then_id };

    if !is_leaf(ctx, value_branch_id) {
        return None;
    }
    let value_branch = ctx.hir.symbol(value_branch_id)?;
    if value_branch.kind != value_sym.kind || value_branch.value != value_sym.value {
        return None;
    }

    Some(CoalesceMatch {
        op,
        then_id,
        else_kw_id,
        else_expr_id,
        value_branch_id,
    })
}

/// Builds the `value ?? fallback` fix for a confirmed match.
///
/// The fallback branch is a leaf (a bare literal/selector) or not, but either way its own end
/// isn't reliably tracked once it has children (e.g. a call's closing `)`). So instead of slicing
/// the fallback's own range, this bounds it by whichever neighboring token *is* reliable: `else`'s
/// own start when the fallback comes first (the `==` shape), or — since nothing follows it when
/// the fallback comes last (the `!=` shape) — by simply not replacing it at all, leaving its
/// original text in place right after the edit.
fn build_fix(ctx: &LintContext<'_>, if_start: mq_lang::Position, m: &CoalesceMatch) -> Option<(mq_lang::Range, Fix)> {
    let value_range = ctx.hir.symbol(m.value_branch_id)?.source.text_range?;

    if m.op == "==" {
        let else_kw_start = ctx.hir.symbol(m.else_kw_id)?.source.text_range?.start;
        let fallback_range = mq_lang::Range {
            start: ctx.full_range(m.then_id)?.start,
            end: else_kw_start,
        };
        let range = mq_lang::Range {
            start: if_start,
            end: value_range.end,
        };
        let fix = Fix::concat(
            range,
            vec![
                Core::Verbatim(value_range),
                Core::Literal(" ?? ".to_string()),
                Core::Verbatim(fallback_range),
            ],
        );
        Some((range, fix))
    } else {
        let fallback_start = ctx.full_range(m.else_expr_id)?.start;
        let range = mq_lang::Range {
            start: if_start,
            end: fallback_start,
        };
        let fix = Fix::concat(
            range,
            vec![Core::Verbatim(value_range), Core::Literal(" ?? ".to_string())],
        );
        Some((range, fix))
    }
}

impl LintRule for PreferCoalesce {
    fn id(&self) -> RuleId {
        RuleId::PreferCoalesce
    }

    fn severity(&self) -> Severity {
        Severity::Style
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        ctx.all_symbols()
            .filter(|(_, sym)| matches!(sym.kind, SymbolKind::If))
            .filter_map(|(if_id, if_sym)| {
                let m = find_coalesce_match(ctx, if_id)?;
                let mut d = Diagnostic::new(LintMessage::PreferCoalesce, self.severity());
                match if_sym.source.text_range.and_then(|r| build_fix(ctx, r.start, &m)) {
                    Some((range, fix)) => d = d.with_range(range).with_fix(fix),
                    None => {
                        if let Some(range) = if_sym.source.text_range {
                            d = d.with_range(range);
                        }
                    }
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
        PreferCoalesce.check(&ctx)
    }

    #[rstest]
    #[case(r#"if (.value == none): "default" else: .value"#)]
    #[case(r#"if (x != none): x else: "default""#)]
    fn detects_null_check_pattern(#[case] code: &str) {
        let diags = check(code);
        assert_eq!(diags.len(), 1);
    }

    #[rstest]
    #[case(r#"if (.value == none): "default" else: .other"#)]
    #[case(r#"if (.checked == true): "yes" else: "no""#)]
    fn no_diagnostic(#[case] code: &str) {
        let diags = check(code);
        assert_eq!(diags.len(), 0);
    }

    #[rstest]
    #[case(r#"if (.value == none): "default" else: .value"#, r#".value ?? "default""#)]
    #[case(r#"if (x != none): x else: "default""#, r#"x ?? "default""#)]
    fn fix_rewrites_as_coalesce(#[case] code: &str, #[case] expected: &str) {
        let diags = check(code);
        let fix = diags[0].fix.as_ref().unwrap();
        let (range, replacement) = fix.resolve(code).unwrap();
        assert_eq!(crate::fix::apply_edits(code, &[(range, replacement)]), expected);
    }

    #[rstest]
    // `==` shape: the fallback is the then-branch, which is *not* the source's last token, so its
    // end must be bounded by `else`'s start rather than its own (potentially truncated) end.
    #[case(r#"if (x == none): get_default(1, 2) else: x"#, "x ?? get_default(1, 2)")]
    // `!=` shape: the fallback is the else-branch and the source's last token; its closing `)` is
    // recovered by leaving it untouched rather than slicing it out.
    #[case(r#"if (x != none): x else: get_default()"#, "x ?? get_default()")]
    fn fix_preserves_compound_fallback_with_trailing_punctuation(#[case] code: &str, #[case] expected: &str) {
        let diags = check(code);
        let fix = diags[0].fix.as_ref().unwrap();
        let (range, replacement) = fix.resolve(code).unwrap();
        assert_eq!(crate::fix::apply_edits(code, &[(range, replacement)]), expected);
    }
}
