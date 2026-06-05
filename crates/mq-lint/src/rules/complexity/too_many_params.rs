use crate::{Diagnostic, LintContext, LintRule, Severity};

pub struct TooManyParams;

impl LintRule for TooManyParams {
    fn id(&self) -> &'static str {
        "too_many_params"
    }

    fn severity(&self) -> Severity {
        Severity::Warn
    }

    fn check(&self, _ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        Vec::new()
    }
}
