use crate::{Diagnostic, LintContext, LintRule, Severity};

pub struct RedundantTry;

impl LintRule for RedundantTry {
    fn id(&self) -> &'static str {
        "redundant_try"
    }

    fn severity(&self) -> Severity {
        Severity::Style
    }

    fn check(&self, _ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        Vec::new()
    }
}
