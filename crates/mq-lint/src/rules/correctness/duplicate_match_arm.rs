use crate::{Diagnostic, LintContext, LintRule, Severity};

pub struct DuplicateMatchArm;

impl LintRule for DuplicateMatchArm {
    fn id(&self) -> &'static str {
        "duplicate_match_arm"
    }

    fn severity(&self) -> Severity {
        Severity::Error
    }

    fn check(&self, _ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        Vec::new()
    }
}
