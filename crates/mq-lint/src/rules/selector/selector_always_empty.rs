use crate::{Diagnostic, LintContext, LintRule, Severity};

pub struct SelectorAlwaysEmpty;

impl LintRule for SelectorAlwaysEmpty {
    fn id(&self) -> &'static str {
        "selector_always_empty"
    }

    fn severity(&self) -> Severity {
        Severity::Warn
    }

    fn check(&self, _ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        Vec::new()
    }
}
