use crate::{Diagnostic, LintContext, LintMessage, LintRule, RuleId, Severity};
use mq_hir::SymbolKind;

/// Maps a capability-gated builtin function name to the CLI flag that unlocks it.
///
/// Kept in sync with the functions gated in
/// `mq-lang/src/eval/builtin/capability.rs` (`http`, `read_file`,
/// `read_file_bytes`, `write_file`).
fn capability_flag(name: &str) -> Option<&'static str> {
    match name {
        "http" => Some("--allow-net"),
        "read_file" | "read_file_bytes" => Some("--allow-read"),
        "write_file" => Some("--allow-write"),
        _ => None,
    }
}

pub struct DangerousCapabilityCall;

impl LintRule for DangerousCapabilityCall {
    fn id(&self) -> RuleId {
        RuleId::DangerousCapabilityCall
    }

    fn severity(&self) -> Severity {
        Severity::Warn
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        ctx.all_symbols()
            .filter_map(|(_, s)| {
                if !matches!(s.kind, SymbolKind::Call) {
                    return None;
                }

                let name = s.value.as_deref()?;
                let flag = capability_flag(name)?;

                let mut d = Diagnostic::new(
                    LintMessage::DangerousCapabilityCall {
                        name: name.to_string(),
                        flag: flag.to_string(),
                    },
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
        DangerousCapabilityCall.check(&ctx)
    }

    #[rstest]
    #[case(r#"http("https://example.com", "GET")"#, 1, "http", "--allow-net")]
    #[case(r#"read_file("secrets.txt")"#, 1, "read_file", "--allow-read")]
    #[case(r#"read_file_bytes("image.png")"#, 1, "read_file_bytes", "--allow-read")]
    #[case(r#"write_file("out.txt", "data")"#, 1, "write_file", "--allow-write")]
    #[case(r#"def wrapper(): read_file("x.txt"); | wrapper()"#, 1, "read_file", "--allow-read")]
    #[case(
        r#"module m: def leak(): http("https://evil.example", "GET"); end | m::leak()"#,
        1,
        "http",
        "--allow-net"
    )]
    fn detects_dangerous_capability_call(
        #[case] code: &str,
        #[case] expected: usize,
        #[case] fn_name: &str,
        #[case] flag: &str,
    ) {
        let diags = check(code);
        assert_eq!(diags.len(), expected);
        assert!(diags[0].message().contains(fn_name));
        assert!(diags[0].message().contains(flag));
        assert!(diags[0].help().unwrap().contains(flag));
    }

    #[test]
    fn detects_multiple_dangerous_calls() {
        let diags = check(r#"read_file("a.txt") | write_file("b.txt", "data")"#);
        assert_eq!(diags.len(), 2);
        let names: Vec<_> = diags.iter().map(|d| d.message()).collect();
        assert!(names.iter().any(|m| m.contains("read_file")));
        assert!(names.iter().any(|m| m.contains("write_file")));
    }

    #[rstest]
    #[case(".h1")]
    #[case(r#"let x = "http""#)]
    #[case(r#"def http_like(): 1; | http_like()"#)]
    #[case(r#"to_text("read_file")"#)]
    fn no_diagnostic(#[case] code: &str) {
        let diags = check(code);
        assert_eq!(diags.len(), 0);
    }

    #[test]
    fn severity_is_warn() {
        assert_eq!(DangerousCapabilityCall.severity(), Severity::Warn);
    }
}
