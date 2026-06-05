use crate::{Diagnostic, LintContext, LintRule, Severity};

pub struct InfiniteLoop;

impl LintRule for InfiniteLoop {
    fn id(&self) -> &'static str {
        "infinite_loop"
    }

    fn severity(&self) -> Severity {
        Severity::Warn
    }

    fn check(&self, _ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        Vec::new()
    }
}
