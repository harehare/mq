use crate::{Diagnostic, LintContext, LintRule, Severity};

pub struct BooleanComparison;

impl LintRule for BooleanComparison {
    fn id(&self) -> &'static str {
        "boolean_comparison"
    }

    fn severity(&self) -> Severity {
        Severity::Style
    }

    fn check(&self, _ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        Vec::new()
    }
}
