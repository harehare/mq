use crate::{Diagnostic, LintContext, LintRule, Severity};

pub struct UnusedImport;

impl LintRule for UnusedImport {
    fn id(&self) -> &'static str {
        "unused_import"
    }

    fn severity(&self) -> Severity {
        Severity::Warn
    }

    fn check(&self, _ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        Vec::new()
    }
}
