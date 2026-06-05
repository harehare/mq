use crate::{Diagnostic, LintContext, LintRule, Severity};

pub struct MissingDepthGuard;

impl LintRule for MissingDepthGuard {
    fn id(&self) -> &'static str {
        "missing_depth_guard"
    }

    fn severity(&self) -> Severity {
        Severity::Warn
    }

    fn check(&self, _ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        Vec::new()
    }
}
