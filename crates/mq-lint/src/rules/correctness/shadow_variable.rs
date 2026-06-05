use crate::{Diagnostic, LintContext, LintRule, Severity};

pub struct ShadowVariable;

impl LintRule for ShadowVariable {
    fn id(&self) -> &'static str {
        "shadow_variable"
    }

    fn severity(&self) -> Severity {
        Severity::Warn
    }

    fn check(&self, _ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        Vec::new()
    }
}
