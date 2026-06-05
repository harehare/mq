use crate::{Diagnostic, LintContext, LintRule, Severity};

pub struct UnusedCatchBinding;

impl LintRule for UnusedCatchBinding {
    fn id(&self) -> &'static str {
        "unused_catch_binding"
    }

    fn severity(&self) -> Severity {
        Severity::Warn
    }

    fn check(&self, _ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        Vec::new()
    }
}
