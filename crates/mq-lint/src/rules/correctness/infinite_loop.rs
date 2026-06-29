use crate::{Diagnostic, LintContext, LintMessage, LintRule, RuleId, Severity};
use mq_hir::SymbolId;
use mq_hir::SymbolKind;

pub struct InfiniteLoop;

impl LintRule for InfiniteLoop {
    fn id(&self) -> RuleId {
        RuleId::InfiniteLoop
    }

    fn severity(&self) -> Severity {
        Severity::Warn
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // Loop symbols are tracked via add_symbol
        let loops: Vec<_> = ctx
            .hir
            .symbols_for_source(ctx.source_id)
            .filter(|(_, s)| matches!(s.kind, SymbolKind::Loop))
            .collect();

        for (loop_id, loop_sym) in loops {
            if !has_break_descendant(ctx, loop_id) {
                let mut d = Diagnostic::new(LintMessage::InfiniteLoop, self.severity());
                if let Some(range) = loop_sym.source.text_range {
                    d = d.with_range(range);
                }
                diagnostics.push(d);
            }
        }

        diagnostics
    }
}

/// Returns true if any descendant of `ancestor_id` in the source is `Keyword("break")`.
/// Keyword symbols use `insert_symbol`, so we use `all_symbols`.
fn has_break_descendant(ctx: &LintContext<'_>, ancestor_id: SymbolId) -> bool {
    ctx.all_symbols().any(|(_, s)| {
        matches!(s.kind, SymbolKind::Keyword)
            && s.value.as_deref() == Some("break")
            && is_descendant_of(ctx, s.parent, ancestor_id)
    })
}

fn is_descendant_of(ctx: &LintContext<'_>, maybe_parent: Option<SymbolId>, target: SymbolId) -> bool {
    let mut current = maybe_parent;
    while let Some(id) = current {
        if id == target {
            return true;
        }
        current = ctx.hir.symbol(id).and_then(|s| s.parent);
    }
    false
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
        InfiniteLoop.check(&ctx)
    }

    #[rstest]
    #[case("loop .h1 end")]
    #[case("loop .h1 | .h2 end")]
    fn detects_loop_without_break(#[case] code: &str) {
        let diags = check(code);
        assert_eq!(diags.len(), 1);
    }

    #[rstest]
    #[case("loop break end")]
    #[case("loop if (true): break else: .h1 end")]
    #[case("while (.h1): .h1 end")]
    fn no_diagnostic(#[case] code: &str) {
        let diags = check(code);
        assert_eq!(diags.len(), 0);
    }
}
