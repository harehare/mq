use crate::{Diagnostic, LintContext, LintMessage, LintRule, RuleId, Severity};
use mq_hir::SymbolKind;

pub struct MissingModuleDoc;

impl LintRule for MissingModuleDoc {
    fn id(&self) -> RuleId {
        RuleId::MissingModuleDoc
    }

    fn severity(&self) -> Severity {
        Severity::Style
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        ctx.all_symbols()
            .filter(|(_, sym)| matches!(sym.kind, SymbolKind::Module(_)))
            .filter(|(_, sym)| sym.doc.is_empty())
            .map(|(_, sym)| {
                let name = sym.value.as_deref().unwrap_or("<anonymous>").to_string();
                let mut d = Diagnostic::new(LintMessage::MissingModuleDoc { name }, self.severity());
                if let Some(range) = sym.source.text_range {
                    d = d.with_range(range);
                }
                d
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
        MissingModuleDoc.check(&ctx)
    }

    #[rstest]
    #[case("module a: def b(): 1; end", "module `a`")]
    #[case("module my_mod: .h1 end", "module `my_mod`")]
    fn detects_module_without_doc(#[case] code: &str, #[case] msg: &str) {
        let diags = check(code);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message().contains(msg));
    }

    #[rstest]
    #[case("# A module that does something.\nmodule a: def b(): 1; end")]
    #[case("# Docs.\nmodule my_mod: .h1 end")]
    fn no_diagnostic(#[case] code: &str) {
        let diags = check(code);
        assert_eq!(diags.len(), 0);
    }
}
