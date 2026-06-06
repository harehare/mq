use mq_hir::SymbolKind;
use mq_lang::Selector;

use crate::{Diagnostic, LintContext, LintRule, Severity};

pub struct SelectorAlwaysEmpty;

/// Conservative: only flags same-variant value mismatches (plus `.todo`/`.done`)
/// to avoid false positives across mq's many selector kinds.
fn mutually_exclusive(first: &Selector, second: &Selector) -> bool {
    match (first, second) {
        (Selector::Heading(Some(a)), Selector::Heading(Some(b))) => a != b,
        (Selector::List(Some(a), _), Selector::List(Some(b), _)) => a != b,
        (Selector::Table(Some(r1), Some(c1)), Selector::Table(Some(r2), Some(c2))) => r1 != r2 || c1 != c2,
        (Selector::Todo, Selector::Done) | (Selector::Done, Selector::Todo) => true,
        _ => false,
    }
}

impl LintRule for SelectorAlwaysEmpty {
    fn id(&self) -> &'static str {
        "selector_always_empty"
    }

    fn severity(&self) -> Severity {
        Severity::Warn
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let mut by_parent: std::collections::HashMap<Option<mq_hir::SymbolId>, Vec<_>> =
            std::collections::HashMap::new();
        for (id, sym) in ctx.all_symbols() {
            by_parent.entry(sym.parent).or_default().push((id, sym));
        }

        let mut diagnostics = Vec::new();
        for siblings in by_parent.values_mut() {
            siblings.sort_by_key(|(_, s)| s.source.text_range.map(|r| (r.start.line, r.start.column)));

            for pair in siblings.windows(2) {
                let [(_, first), (_, second)] = pair else { continue };
                let (SymbolKind::Selector(sel1), SymbolKind::Selector(sel2)) = (&first.kind, &second.kind) else {
                    continue;
                };
                if !mutually_exclusive(sel1, sel2) {
                    continue;
                }

                let first_text = first.value.as_deref().unwrap_or("<selector>");
                let second_text = second.value.as_deref().unwrap_or("<selector>");
                let mut d = Diagnostic::new(
                    self.id(),
                    self.severity(),
                    format!("`{first_text} | {second_text}` can never match: a node can't be both"),
                );
                if let Some(range) = second.source.text_range {
                    d = d.with_range(range);
                }
                diagnostics
                    .push(d.with_help("remove one of the selectors, or replace the pipe with a different query"));
            }
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
        SelectorAlwaysEmpty.check(&ctx)
    }

    #[test]
    fn detects_conflicting_heading_levels() {
        let diags = check(".h1 | .h2");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn detects_todo_then_done() {
        let diags = check(".todo | .done");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn no_diagnostic_for_same_selector_repeated() {
        let diags = check(".h1 | .h1");
        assert_eq!(diags.len(), 0);
    }

    #[test]
    fn no_diagnostic_for_generic_then_specific_heading() {
        let diags = check(".heading | .h1");
        assert_eq!(diags.len(), 0);
    }

    #[test]
    fn no_diagnostic_when_unrelated_step_is_between() {
        let diags = check(".h1 | to_text() | .h2");
        assert_eq!(diags.len(), 0);
    }
}
