use mq_hir::SymbolKind;

use crate::{Diagnostic, LintContext, LintMessage, LintRule, RuleId, Severity};

/// Reports `x = set(x, key, value)`, which `x[key] = value` writes in place.
///
/// Reading `x` for the `set` call clones it, so when `x` is shared with a closure the whole
/// collection is copied on every update. The index assignment updates it where it is.
pub struct PreferIndexAssignment;

impl LintRule for PreferIndexAssignment {
    fn id(&self) -> RuleId {
        RuleId::PreferIndexAssignment
    }

    fn severity(&self) -> Severity {
        Severity::Style
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        ctx.all_symbols()
            .filter(|(_, s)| matches!(s.kind, SymbolKind::Assign) && s.value.as_deref() == Some("="))
            .filter_map(|(assign_id, assign)| {
                let mut parts: Vec<_> = ctx.children(assign_id).collect();
                parts.sort_by_key(|(_, s)| s.source.text_range.map(|r| (r.start.line, r.start.column)));
                let [(_, target), (call_id, call)] = parts.as_slice() else {
                    return None;
                };
                if !matches!(target.kind, SymbolKind::Ref | SymbolKind::Ident)
                    || !matches!(call.kind, SymbolKind::Call)
                    || call.value.as_deref() != Some("set")
                {
                    return None;
                }
                let mut args: Vec<_> = ctx.children(*call_id).collect();
                args.sort_by_key(|(_, s)| s.source.text_range.map(|r| (r.start.line, r.start.column)));
                let [(_, first), _, _] = args.as_slice() else {
                    return None;
                };
                let name = target.value.as_deref()?;
                if !matches!(first.kind, SymbolKind::Ref | SymbolKind::Ident) || first.value.as_deref() != Some(name) {
                    return None;
                }
                let range = assign.source.text_range?;
                Some(
                    Diagnostic::new(
                        LintMessage::PreferIndexAssignment { name: name.to_string() },
                        self.severity(),
                    )
                    .with_range(range),
                )
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
        let (source_id, _, _) = hir.add_code(None, code);
        let config = LintConfig::default();
        let ctx = LintContext::new(&hir, source_id, &config);
        PreferIndexAssignment.check(&ctx)
    }

    #[rstest]
    #[case("var x = [1, 2] | x = set(x, 0, 9) | x")]
    #[case("var d = {} | d = set(d, \"k\", 1) | d")]
    #[case("var x = [1] | def f(i): x = set(x, i, i + 1); | f(0)")]
    fn reports_a_set_assigned_back_to_the_same_variable(#[case] code: &str) {
        assert_eq!(check(code).len(), 1, "{code}");
    }

    #[rstest]
    #[case("var x = [1, 2] | var y = [3] | x = set(y, 0, 9) | x")]
    #[case("var x = [1, 2] | let y = set(x, 0, 9) | y")]
    #[case("var x = [1, 2] | x = insert(x, 0, 9) | x")]
    #[case("var x = [1, 2] | x[0] = 9 | x")]
    #[case("var x = [1, 2] | x = set(x, 0) | x")]
    fn ignores_everything_else(#[case] code: &str) {
        assert!(check(code).is_empty(), "{code}");
    }
}
