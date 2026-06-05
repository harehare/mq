use crate::{Diagnostic, LintContext, LintRule, Severity};

pub struct CircularImport;

impl LintRule for CircularImport {
    fn id(&self) -> &'static str {
        "circular_import"
    }

    fn severity(&self) -> Severity {
        Severity::Error
    }

    fn check(&self, _ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        Vec::new()
    }
}
