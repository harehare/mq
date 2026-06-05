use crate::{Diagnostic, LintContext, LintRule, Severity};

pub struct EnvVarInSelector;

impl LintRule for EnvVarInSelector {
    fn id(&self) -> &'static str {
        "env_var_in_selector"
    }

    fn severity(&self) -> Severity {
        Severity::Warn
    }

    fn check(&self, _ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        Vec::new()
    }
}
