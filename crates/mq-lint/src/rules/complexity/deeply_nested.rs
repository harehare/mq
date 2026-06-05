use crate::{Diagnostic, LintContext, LintRule, Severity};

pub struct DeeplyNested;

impl LintRule for DeeplyNested {
    fn id(&self) -> &'static str {
        "deeply_nested"
    }

    fn severity(&self) -> Severity {
        Severity::Warn
    }

    fn check(&self, _ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        Vec::new()
    }
}
