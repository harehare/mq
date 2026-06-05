use crate::{Diagnostic, LintContext, LintRule, Severity};

pub struct RedundantBooleanLiteral;

impl LintRule for RedundantBooleanLiteral {
    fn id(&self) -> &'static str {
        "redundant_boolean_literal"
    }

    fn severity(&self) -> Severity {
        Severity::Style
    }

    fn check(&self, _ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        Vec::new()
    }
}
