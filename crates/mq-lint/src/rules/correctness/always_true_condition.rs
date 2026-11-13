use crate::{Diagnostic, LintContext, LintMessage, LintRule, RuleId, Severity};
use mq_hir::{SymbolId, SymbolKind};

/// Reports `if` conditions that are a `true` or `false` literal.
pub struct AlwaysTrueCondition;

impl LintRule for AlwaysTrueCondition {
    fn id(&self) -> RuleId {
        RuleId::AlwaysTrueCondition
    }

    fn severity(&self) -> Severity {
        Severity::Warn
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        let if_ids: Vec<_> = ctx
            .hir
            .symbols_for_source(ctx.source_id())
            .filter(|(_, s)| matches!(s.kind, SymbolKind::If))
            .collect();

        for (if_id, if_sym) in if_ids {
            let Some(value) = condition_literal(ctx, if_id) else {
                continue;
            };

            let mut d = Diagnostic::new(LintMessage::AlwaysTrueCondition { value }, self.severity());
            if let Some(range) = if_sym.source.text_range {
                d = d.with_range(range);
            }
            diagnostics.push(d);
        }

        diagnostics
    }
}

/// Returns the `true`/`false` literal used as the condition (first child by position) of `id`.
pub(crate) fn condition_literal(ctx: &LintContext<'_>, id: SymbolId) -> Option<String> {
    let (_, cond_sym) = ctx
        .children(id)
        .filter_map(|(_, s)| s.source.text_range.map(|r| (r, s)))
        .min_by_key(|(r, _)| (r.start.line, r.start.column))?;

    if !matches!(cond_sym.kind, SymbolKind::Boolean) {
        return None;
    }
    cond_sym.value.as_deref().map(str::to_string)
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
        AlwaysTrueCondition.check(&ctx)
    }

    #[rstest]
    #[case("if (true): 1 else: 2;", "always `true`")]
    #[case("if (false): 1 else: 2;", "always `false`")]
    #[case("if (true): 1;", "always `true`")]
    fn detects_constant_boolean_condition(#[case] code: &str, #[case] expected_msg: &str) {
        let diags = check(code);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message().contains(expected_msg));
    }

    #[rstest]
    #[case("if (.h1): 1 else: 2;")]
    #[case("if (x): 1 else: 2;")]
    #[case("if (.checked == true): 1 else: 2;")]
    fn no_diagnostic_for_dynamic_condition(#[case] code: &str) {
        let diags = check(code);
        assert_eq!(diags.len(), 0);
    }
}
