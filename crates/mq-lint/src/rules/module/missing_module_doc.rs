use crate::{Diagnostic, LintContext, LintRule, Severity};

pub struct MissingModuleDoc;

impl LintRule for MissingModuleDoc {
    fn id(&self) -> &'static str {
        "missing_module_doc"
    }

    fn severity(&self) -> Severity {
        Severity::Style
    }

    fn check(&self, _ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        Vec::new()
    }
}
