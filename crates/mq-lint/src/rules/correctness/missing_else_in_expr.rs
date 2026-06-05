use crate::{Diagnostic, LintContext, LintRule, Severity};

pub struct MissingElseInExpr;

impl LintRule for MissingElseInExpr {
    fn id(&self) -> &'static str {
        "missing_else_in_expr"
    }

    fn severity(&self) -> Severity {
        Severity::Warn
    }

    fn check(&self, _ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        Vec::new()
    }
}
