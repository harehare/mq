use crate::{Diagnostic, LintContext, LintMessage, LintRule, RuleId, Severity};
use rustc_hash::FxHashMap;

pub struct DuplicateImport;

impl LintRule for DuplicateImport {
    fn id(&self) -> RuleId {
        RuleId::DuplicateImport
    }

    fn severity(&self) -> Severity {
        Severity::Error
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let import_symbols = ctx
            .all_symbols()
            .filter_map(|(_, s)| if s.is_import() { Some(s) } else { None })
            .collect::<Vec<_>>();
        let mut counts = FxHashMap::default();

        for s in &import_symbols {
            match &s.value {
                Some(name) => {
                    counts.entry(name).or_insert((s, 0)).1 += 1;
                }
                None => {
                    // If the import has no name, we can skip it
                    continue;
                }
            }
        }

        counts
            .into_iter()
            .filter(|(_, (_, count))| *count > 1)
            .map(|(name, (s, _))| {
                let mut d = Diagnostic::new(LintMessage::DuplicateImport { name: name.to_string() }, self.severity());

                if let Some(range) = s.source.text_range {
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

    use super::*;
    use crate::{LintConfig, LintContext};

    fn check(code: &str) -> Vec<Diagnostic> {
        let mut hir = Hir::default();
        let (source_id, _) = hir.add_code(None, code);
        let config = LintConfig::default();
        let ctx = LintContext::new(&hir, source_id, &config);
        DuplicateImport.check(&ctx)
    }

    #[test]
    fn detects_duplicate_import() {
        let diags = check("import \"a\" | import \"a\" | a");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn no_duplicate_import_with_other_module() {
        let diags = check("import \"a\" | import \"b\" | a");
        assert_eq!(diags.len(), 0);
    }

    #[test]
    fn no_duplicate_import() {
        let diags = check("import \"a\" | a");
        assert_eq!(diags.len(), 0);
    }
}
