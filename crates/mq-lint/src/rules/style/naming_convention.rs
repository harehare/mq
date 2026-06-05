use crate::{Diagnostic, LintContext, LintRule, Severity};

pub struct NamingConvention;

impl LintRule for NamingConvention {
    fn id(&self) -> &'static str {
        "naming_convention"
    }

    fn severity(&self) -> Severity {
        Severity::Style
    }

    fn check(&self, _ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        Vec::new()
    }
}
