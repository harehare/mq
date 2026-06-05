use crate::{Diagnostic, LintContext, LintRule, Severity};

pub struct ComplexInterpolation;

impl LintRule for ComplexInterpolation {
    fn id(&self) -> &'static str {
        "complex_interpolation"
    }

    fn severity(&self) -> Severity {
        Severity::Warn
    }

    fn check(&self, _ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        Vec::new()
    }
}
