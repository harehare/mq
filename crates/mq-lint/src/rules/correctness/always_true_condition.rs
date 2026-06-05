use crate::{Diagnostic, LintContext, LintRule, Severity};

pub struct AlwaysTrueCondition;

impl LintRule for AlwaysTrueCondition {
    fn id(&self) -> &'static str {
        "always_true_condition"
    }

    fn severity(&self) -> Severity {
        Severity::Warn
    }

    fn check(&self, _ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        Vec::new()
    }
}
