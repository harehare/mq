use crate::{Diagnostic, LintContext, LintRule, Severity};

pub struct AmbiguousQualifiedAccess;

impl LintRule for AmbiguousQualifiedAccess {
    fn id(&self) -> &'static str {
        "ambiguous_qualified_access"
    }

    fn severity(&self) -> Severity {
        Severity::Warn
    }

    fn check(&self, _ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        Vec::new()
    }
}
