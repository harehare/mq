use crate::{Diagnostic, LintContext, LintRule, Severity};

pub struct UnnecessaryParens;

impl LintRule for UnnecessaryParens {
    fn id(&self) -> &'static str {
        "unnecessary_parens"
    }

    fn severity(&self) -> Severity {
        Severity::Style
    }

    fn check(&self, _ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        Vec::new()
    }
}
