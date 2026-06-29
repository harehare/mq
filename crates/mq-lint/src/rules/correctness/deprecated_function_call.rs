use crate::{Diagnostic, LintContext, LintMessage, LintRule, RuleId, Severity};
use mq_hir::SymbolKind;
use rustc_hash::FxHashSet;

pub struct DeprecatedFunctionCall;

impl LintRule for DeprecatedFunctionCall {
    fn id(&self) -> RuleId {
        RuleId::DeprecatedFunctionCall
    }

    fn severity(&self) -> Severity {
        Severity::Warn
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let deprecated_functions: FxHashSet<&str> = ctx
            .hir
            .symbols()
            .filter(|(_, s)| s.is_function() && s.is_deprecated())
            .filter_map(|(_, s)| s.value.as_deref())
            .collect();

        ctx.all_symbols()
            .filter_map(|(_, s)| {
                if !matches!(s.kind, SymbolKind::Call) {
                    return None;
                }

                let name = s.value.as_deref()?;

                if !deprecated_functions.contains(name) {
                    return None;
                }

                let mut d = Diagnostic::new(
                    LintMessage::DeprecatedFunctionCall { name: name.to_string() },
                    self.severity(),
                );
                if let Some(range) = s.source.text_range {
                    d = d.with_range(range);
                }
                Some(d)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use mq_hir::Hir;
    use rstest::rstest;

    use super::*;
    use crate::{LintConfig, LintContext};

    fn check(code: &str) -> Vec<Diagnostic> {
        let mut hir = Hir::default();
        let (source_id, _) = hir.add_code(None, code);
        let config = LintConfig::default();
        let ctx = LintContext::new(&hir, source_id, &config);
        DeprecatedFunctionCall.check(&ctx)
    }

    #[rstest]
    #[case("# deprecated\ndef old(): 1; | old()", 1, "old")]
    #[case("# Deprecated\ndef old(): 1; | old()", 1, "old")]
    #[case("# DEPRECATED\ndef old(): 1; | old()", 1, "old")]
    #[case("# deprecated: use new_fn instead\ndef old(): 1; | old()", 1, "old")]
    #[case("# deprecated\ndef my_old_func(): 1; | my_old_func()", 1, "my_old_func")]
    #[case("# deprecated\ndef old(): 1;\ndef wrapper(): old();", 1, "old")]
    fn detects_deprecated_call(#[case] code: &str, #[case] expected: usize, #[case] fn_name: &str) {
        let diags = check(code);
        assert_eq!(diags.len(), expected);
        assert!(diags[0].message().contains(fn_name));
    }

    #[test]
    fn detects_multiple_calls_to_same_deprecated_function() {
        let diags = check("# deprecated\ndef old(): 1; | old() | old()");
        assert_eq!(diags.len(), 2);
        assert!(diags.iter().all(|d| d.message().contains("old")));
    }

    #[test]
    fn detects_calls_to_multiple_deprecated_functions() {
        let diags = check("# deprecated\ndef old_a(): 1;\n# deprecated\ndef old_b(): 2; | old_a() | old_b()");
        assert_eq!(diags.len(), 2);
        let messages: Vec<_> = diags.iter().map(|d| d.message()).collect();
        assert!(messages.iter().any(|m| m.contains("old_a")));
        assert!(messages.iter().any(|m| m.contains("old_b")));
    }

    #[rstest]
    #[case("def current(): 1; | current()")]
    #[case("# deprecated\ndef old(): 1;")]
    #[case("# deprecated\ndef old(): 1;\ndef current(): 2; | current()")]
    fn no_diagnostic(#[case] code: &str) {
        let diags = check(code);
        assert_eq!(diags.len(), 0);
    }

    #[test]
    fn severity_is_warn() {
        assert_eq!(DeprecatedFunctionCall.severity(), Severity::Warn);
    }
}
