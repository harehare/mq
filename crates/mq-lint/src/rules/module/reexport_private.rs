use crate::{Diagnostic, LintContext, LintRule, Severity};

pub struct ReexportPrivate;

impl LintRule for ReexportPrivate {
    fn id(&self) -> &'static str {
        "reexport_private"
    }

    fn severity(&self) -> Severity {
        Severity::Warn
    }

    fn check(&self, _ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        Vec::new()
    }
}
