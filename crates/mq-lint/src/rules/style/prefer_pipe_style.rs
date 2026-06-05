use crate::{Diagnostic, LintContext, LintRule, Severity};

pub struct PreferPipeStyle;

impl LintRule for PreferPipeStyle {
    fn id(&self) -> &'static str {
        "prefer_pipe_style"
    }

    fn severity(&self) -> Severity {
        Severity::Style
    }

    fn check(&self, _ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        Vec::new()
    }
}
