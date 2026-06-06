use crate::{Diagnostic, LintContext, LintRule, Severity};
use mq_hir::SymbolKind;

pub struct MissingDepthGuard;

impl LintRule for MissingDepthGuard {
    fn id(&self) -> &'static str {
        "missing_depth_guard"
    }

    fn severity(&self) -> Severity {
        Severity::Warn
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        // Check for depth-guard attributes (`.depth` / `.level`) anywhere in this source.
        // If any depth attribute is present, we assume the author is aware of depth guarding.
        let has_depth_attr = ctx.all_symbols().any(|(_, s)| {
            matches!(
                s.kind,
                SymbolKind::Selector(mq_lang::Selector::Attr(mq_lang::AttrKind::Depth))
                    | SymbolKind::Selector(mq_lang::Selector::Attr(mq_lang::AttrKind::Level))
            )
        });

        if has_depth_attr {
            return Vec::new();
        }

        // Flag every `..` selector that appears without any depth guard in the source.
        ctx.all_symbols()
            .filter(|(_, s)| matches!(s.kind, SymbolKind::Selector(mq_lang::Selector::Recursive)))
            .map(|(_, sym)| {
                let mut d = Diagnostic::new(
                    self.id(),
                    self.severity(),
                    "`..` (recursive selector) used without a depth guard",
                );
                if let Some(range) = sym.source.text_range {
                    d = d.with_range(range);
                }
                d.with_help(
                    "consider adding a depth limit, e.g. `.. | select(.depth <= 3)`, \
                     to avoid traversing the entire document",
                )
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
        MissingDepthGuard.check(&ctx)
    }

    #[test]
    fn detects_bare_recursive_selector() {
        let diags = check("..");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("depth guard"));
    }

    #[test]
    fn no_diagnostic_when_depth_attr_present() {
        // When .depth is used somewhere, we consider depth guarding in place.
        let diags = check(".. | .depth");
        assert_eq!(diags.len(), 0);
    }

    #[test]
    fn no_diagnostic_for_non_recursive_selector() {
        let diags = check(".h1");
        assert_eq!(diags.len(), 0);
    }
}
