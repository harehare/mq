use crate::{Diagnostic, LintContext, LintRule, Severity};

pub struct TooManyMatchArms;

impl LintRule for TooManyMatchArms {
    fn id(&self) -> &'static str {
        "too_many_match_arms"
    }

    fn severity(&self) -> Severity {
        Severity::Warn
    }

    fn check(&self, _ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        Vec::new()
    }
}
