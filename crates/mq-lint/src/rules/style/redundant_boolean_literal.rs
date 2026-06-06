use crate::{Diagnostic, LintContext, LintRule, Severity};
use mq_hir::{SymbolId, SymbolKind};

pub struct RedundantBooleanLiteral;

impl LintRule for RedundantBooleanLiteral {
    fn id(&self) -> &'static str {
        "redundant_boolean_literal"
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

            // The then-body is the second child of If by position (first is the condition).
            let then_bool = second_child_boolean(ctx, if_id);
            // The else-body is the first (only) child of Else.
            let else_bool = first_child_boolean(ctx, else_id);

            let (then_val, else_val) = match (then_bool, else_bool) {
                (Some(t), Some(e)) => (t, e),
                _ => continue,
            };

            // Pattern: if (cond): true else: false  →  cond
            //          if (cond): false else: true  →  !cond
            if (then_val == "true" && else_val == "false") || (then_val == "false" && else_val == "true") {
                let suggestion = if then_val == "true" {
                    "replace `if (cond): true else: false` with just `cond`"
                } else {
                    "replace `if (cond): false else: true` with `not(cond)` or `!(cond)`"
                };

                let mut d = Diagnostic::new(
                    self.id(),
                    self.severity(),
                    "redundant boolean literal in `if`/`else` — condition already is the result",
                );
                if let Some(range) = if_sym.source.text_range {
                    d = d.with_range(range);
                }
                diagnostics.push(d.with_help(suggestion));
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

/// Returns the value of the second child (by source position) of `parent_id` if it is Boolean.
fn second_child_boolean<'a>(ctx: &'a LintContext<'_>, parent_id: SymbolId) -> Option<&'a str> {
    let mut children: Vec<_> = ctx
        .all_symbols()
        .filter(|(_, s)| {
            s.parent == Some(parent_id)
                && !matches!(s.kind, SymbolKind::Else | SymbolKind::Elif)
                && s.source.text_range.is_some()
        })
        .filter_map(|(id, s)| s.source.text_range.map(|r| (id, r, s)))
        .collect();
    children.sort_by_key(|(_, r, _)| (r.start.line, r.start.column));

    let (_, _, sym) = children.get(1)?;
    if matches!(sym.kind, SymbolKind::Boolean) {
        sym.value.as_deref()
    } else {
        None
    }
}

/// Returns the value of the first child (by source position) of `parent_id` if it is Boolean.
fn first_child_boolean<'a>(ctx: &'a LintContext<'_>, parent_id: SymbolId) -> Option<&'a str> {
    let mut children: Vec<_> = ctx
        .all_symbols()
        .filter(|(_, s)| s.parent == Some(parent_id) && s.source.text_range.is_some())
        .filter_map(|(id, s)| s.source.text_range.map(|r| (id, r, s)))
        .collect();
    children.sort_by_key(|(_, r, _)| (r.start.line, r.start.column));

    let (_, _, sym) = children.first()?;
    if matches!(sym.kind, SymbolKind::Boolean) {
        sym.value.as_deref()
    } else {
        None
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
        RedundantBooleanLiteral.check(&ctx)
    }

    #[test]
    fn detects_true_false_pattern() {
        let diags = check("if (.h1): true else: false;");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("redundant boolean literal"));
    }

    #[test]
    fn detects_false_true_pattern() {
        let diags = check("if (.h1): false else: true;");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn no_diagnostic_for_non_boolean_branches() {
        let diags = check("if (.h1): 1 else: 2;");
        assert_eq!(diags.len(), 0);
    }

    #[test]
    fn no_diagnostic_for_mixed_branches() {
        let diags = check("if (.h1): true else: 2;");
        assert_eq!(diags.len(), 0);
    }
}
