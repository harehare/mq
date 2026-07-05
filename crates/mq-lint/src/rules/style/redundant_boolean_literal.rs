use crate::{Diagnostic, Fix, LintContext, LintMessage, LintRule, RuleId, Severity};
use mq_hir::{SymbolId, SymbolKind};

pub struct RedundantBooleanLiteral;

impl LintRule for RedundantBooleanLiteral {
    fn id(&self) -> RuleId {
        RuleId::RedundantBooleanLiteral
    }

    fn severity(&self) -> Severity {
        Severity::Style
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // Find If symbols that have an Else child.
        let if_ids: Vec<_> = ctx
            .hir
            .symbols_for_source(ctx.source_id)
            .filter(|(_, s)| matches!(s.kind, SymbolKind::If))
            .collect();

        for (if_id, if_sym) in if_ids {
            let Some(else_id) = find_else_child(ctx, if_id) else {
                continue;
            };

            // Condition and then-body are the first two children of If by position.
            let if_children = sorted_children(ctx, if_id, true);
            let Some(&(cond_id, _)) = if_children.first() else {
                continue;
            };
            let Some(&(_, then_sym)) = if_children.get(1) else {
                continue;
            };
            let Some(&(_, else_sym)) = sorted_children(ctx, else_id, false).first() else {
                continue;
            };

            let (Some(then_val), Some(else_val)) = (boolean_value(then_sym), boolean_value(else_sym)) else {
                continue;
            };

            // Pattern: if (cond): true else: false  →  cond
            //          if (cond): false else: true  →  !cond
            if (then_val == "true" && else_val == "false") || (then_val == "false" && else_val == "true") {
                let mut d = Diagnostic::new(
                    LintMessage::RedundantBooleanLiteral {
                        then_val: then_val.to_string(),
                    },
                    self.severity(),
                );
                if let (Some(if_start), Some(else_range), Some(cond_range)) = (
                    if_sym.source.text_range,
                    else_sym.source.text_range,
                    ctx.full_range(cond_id),
                ) {
                    let range = mq_lang::Range {
                        start: if_start.start,
                        end: else_range.end,
                    };
                    let mut fix = Fix::verbatim(range, cond_range);
                    if then_val == "false" {
                        fix = fix.with_prefix("!");
                    }
                    d = d.with_range(range).with_fix(fix);
                } else if let Some(range) = if_sym.source.text_range {
                    d = d.with_range(range);
                }
                diagnostics.push(d);
            }
        }

        diagnostics
    }
}

fn find_else_child(ctx: &LintContext<'_>, if_id: SymbolId) -> Option<SymbolId> {
    ctx.hir
        .symbols_for_source(ctx.source_id)
        .find(|(_, s)| s.parent == Some(if_id) && matches!(s.kind, SymbolKind::Else))
        .map(|(id, _)| id)
}

fn boolean_value(sym: &mq_hir::Symbol) -> Option<&str> {
    matches!(sym.kind, SymbolKind::Boolean)
        .then(|| sym.value.as_deref())
        .flatten()
}

/// Children of `parent_id`, sorted by source position. When `exclude_branches` is set, `Else`
/// and `Elif` children are skipped (used for `If`'s children, where they aren't part of the
/// condition/then-body pair).
fn sorted_children<'a>(
    ctx: &'a LintContext<'_>,
    parent_id: SymbolId,
    exclude_branches: bool,
) -> Vec<(SymbolId, &'a mq_hir::Symbol)> {
    let mut children: Vec<_> = ctx
        .all_symbols()
        .filter(|(_, s)| {
            s.parent == Some(parent_id)
                && s.source.text_range.is_some()
                && !(exclude_branches && matches!(s.kind, SymbolKind::Else | SymbolKind::Elif))
        })
        .collect();
    children.sort_by_key(|(_, s)| s.source.text_range.map(|r| (r.start.line, r.start.column)));
    children
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
        RedundantBooleanLiteral.check(&ctx)
    }

    #[rstest]
    #[case("if (.h1): true else: false;")]
    #[case("if (.h1): false else: true;")]
    fn detects_redundant_boolean_branches(#[case] code: &str) {
        let diags = check(code);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message().contains("redundant boolean literal"));
    }

    #[rstest]
    #[case("if (.h1): 1 else: 2;")]
    #[case("if (.h1): true else: 2;")]
    #[case("if (.h1): 1 else: false;")]
    fn no_diagnostic(#[case] code: &str) {
        let diags = check(code);
        assert_eq!(diags.len(), 0);
    }

    #[rstest]
    #[case("if (.h1): true else: false;", ".h1;")]
    #[case("if (.h1): false else: true;", "!.h1;")]
    fn fix_simplifies_to_condition(#[case] code: &str, #[case] expected: &str) {
        let diags = check(code);
        let fix = diags[0].fix.as_ref().unwrap();
        let (range, replacement) = fix.resolve(code).unwrap();
        assert_eq!(crate::fix::apply_edits(code, &[(range, replacement)]), expected);
    }
}
