use crate::{Diagnostic, LintContext, LintRule, Severity};

pub struct PreferLetOverVar;

impl LintRule for PreferLetOverVar {
    fn id(&self) -> &'static str {
        "prefer_let_over_var"
    }

    fn severity(&self) -> Severity {
        Severity::Warn
    }

    fn check(&self, _ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        Vec::new()
    }
}
