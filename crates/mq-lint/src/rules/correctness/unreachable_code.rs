use crate::{Diagnostic, LintContext, LintRule, Severity};

pub struct UnreachableCode;

impl LintRule for UnreachableCode {
    fn id(&self) -> &'static str {
        "unreachable_code"
    }

    fn severity(&self) -> Severity {
        Severity::Error
    }

    fn check(&self, _ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        Vec::new()
    }
}
