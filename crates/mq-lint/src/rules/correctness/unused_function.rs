use crate::{Diagnostic, LintContext, LintRule, Severity};

pub struct UnusedFunction;

impl LintRule for UnusedFunction {
    fn id(&self) -> &'static str {
        "unused_function"
    }

    fn severity(&self) -> Severity {
        Severity::Warn
    }

    fn check(&self, _ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        Vec::new()
    }
}
