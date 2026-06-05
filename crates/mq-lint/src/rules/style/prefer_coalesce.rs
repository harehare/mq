use crate::{Diagnostic, LintContext, LintRule, Severity};

pub struct PreferCoalesce;

impl LintRule for PreferCoalesce {
    fn id(&self) -> &'static str {
        "prefer_coalesce"
    }

    fn severity(&self) -> Severity {
        Severity::Style
    }

    fn check(&self, _ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        Vec::new()
    }
}
