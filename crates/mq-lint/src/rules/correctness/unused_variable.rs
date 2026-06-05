use crate::{Diagnostic, LintContext, LintRule, Severity};

pub struct UnusedVariable;

impl LintRule for UnusedVariable {
    fn id(&self) -> &'static str {
        "unused_variable"
    }

    fn severity(&self) -> Severity {
        Severity::Warn
    }

    fn check(&self, _ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        Vec::new()
    }
}
