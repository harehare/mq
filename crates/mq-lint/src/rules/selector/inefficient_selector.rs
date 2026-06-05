use crate::{Diagnostic, LintContext, LintRule, Severity};

pub struct InefficientSelector;

impl LintRule for InefficientSelector {
    fn id(&self) -> &'static str {
        "inefficient_selector"
    }

    fn severity(&self) -> Severity {
        Severity::Perf
    }

    fn check(&self, _ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        Vec::new()
    }
}
