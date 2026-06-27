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

    use super::*;
    use crate::{LintConfig, LintContext};

    fn check(code: &str) -> Vec<Diagnostic> {
        let mut hir = Hir::default();
        let (source_id, _) = hir.add_code(None, code);
        let config = LintConfig::default();
        let ctx = LintContext::new(&hir, source_id, &config);
        DeprecatedFunctionCall.check(&ctx)
    }

    #[test]
    fn detects_call_to_deprecated_function() {
        let diags = check("# deprecated\ndef old(): 1; | old()");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message().contains("old"));
        assert_eq!(diags[0].severity, Severity::Warn);
    }

    #[test]
    fn no_diagnostic_for_non_deprecated_function() {
        let diags = check("def current(): 1; | current()");
        assert_eq!(diags.len(), 0);
    }

    #[test]
    fn no_diagnostic_when_deprecated_function_not_called() {
        let diags = check("# deprecated\ndef old(): 1;");
        assert_eq!(diags.len(), 0);
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

    #[test]
    fn detects_deprecated_with_uppercase_marker() {
        let diags = check("# Deprecated\ndef old(): 1; | old()");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn detects_deprecated_with_all_caps_marker() {
        let diags = check("# DEPRECATED\ndef old(): 1; | old()");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn no_diagnostic_when_different_function_called() {
        let diags = check("# deprecated\ndef old(): 1;\ndef current(): 2; | current()");
        assert_eq!(diags.len(), 0);
    }

    #[test]
    fn detects_deprecated_call_inside_function_body() {
        let diags = check("# deprecated\ndef old(): 1;\ndef wrapper(): old();");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message().contains("old"));
    }

    #[test]
    fn detects_deprecated_call_with_message_suffix() {
        let diags = check("# deprecated: use new_fn instead\ndef old(): 1; | old()");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn diagnostic_message_contains_function_name() {
        let diags = check("# deprecated\ndef my_old_func(): 1; | my_old_func()");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message().contains("my_old_func"));
    }

    #[test]
    fn rule_id_is_deprecated_function_call() {
        assert_eq!(DeprecatedFunctionCall.id(), RuleId::DeprecatedFunctionCall);
    }

    #[test]
    fn severity_is_warn() {
        assert_eq!(DeprecatedFunctionCall.severity(), Severity::Warn);
    }
}
