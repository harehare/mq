use crate::{Diagnostic, LintContext, LintRule, Severity};

pub struct DeprecatedSelector;

impl LintRule for DeprecatedSelector {
    fn id(&self) -> &'static str {
        "deprecated_selector"
    }

    fn severity(&self) -> Severity {
        Severity::Error
    }

    fn check(&self, _ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        Vec::new()
    }
}
