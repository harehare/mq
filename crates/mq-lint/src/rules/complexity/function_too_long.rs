use crate::{Diagnostic, LintContext, LintRule, Severity};

pub struct FunctionTooLong;

impl LintRule for FunctionTooLong {
    fn id(&self) -> &'static str {
        "function_too_long"
    }

    fn severity(&self) -> Severity {
        Severity::Warn
    }

    fn check(&self, _ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        Vec::new()
    }
}
