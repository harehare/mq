use crate::{Diagnostic, LintContext, LintRule, Severity};

pub struct PreferSpecificHeading;

impl LintRule for PreferSpecificHeading {
    fn id(&self) -> &'static str {
        "prefer_specific_heading"
    }

    fn severity(&self) -> Severity {
        Severity::Style
    }

    fn check(&self, _ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        Vec::new()
    }
}
