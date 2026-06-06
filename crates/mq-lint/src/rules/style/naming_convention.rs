use crate::{Diagnostic, LintContext, LintRule, Severity};
use mq_hir::SymbolKind;

pub struct NamingConvention;

impl LintRule for NamingConvention {
    fn id(&self) -> &'static str {
        "naming_convention"
    }

    fn severity(&self) -> Severity {
        Severity::Style
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        // Function symbols use add_symbol; Variable symbols use insert_symbol.
        // Use all_symbols to cover both.
        ctx.all_symbols()
            .filter(|(_, s)| matches!(s.kind, SymbolKind::Function(_) | SymbolKind::Variable))
            .filter_map(|(_, sym)| {
                let name = sym.value.as_deref()?;
                if name.starts_with('_') {
                    return None;
                }
                if is_snake_case(name) {
                    return None;
                }
                let suggested = to_snake_case(name);
                let mut d = Diagnostic::new(
                    self.id(),
                    self.severity(),
                    format!("`{name}` should be written in snake_case"),
                );
                if let Some(range) = sym.source.text_range {
                    d = d.with_range(range);
                }
                Some(d.with_help(format!("rename to `{suggested}`")))
            })
            .collect()
    }
}

/// Returns true if the name is already valid snake_case (lowercase + underscores + digits).
fn is_snake_case(name: &str) -> bool {
    name.chars()
        .all(|c| c.is_ascii_lowercase() || c == '_' || c.is_ascii_digit())
}

/// Naive camelCase / PascalCase → snake_case conversion for the help message.
fn to_snake_case(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for (i, c) in name.char_indices() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
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
        NamingConvention.check(&ctx)
    }

    #[test]
    fn detects_camel_case_function() {
        let diags = check("def myFunc(): .h1;");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("myFunc"));
    }

    #[test]
    fn no_diagnostic_for_snake_case() {
        let diags = check("def my_func(): .h1;");
        assert_eq!(diags.len(), 0);
    }

    #[test]
    fn detects_camel_case_variable() {
        let diags = check("let myVar = .h1");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn underscore_prefix_is_ok() {
        let diags = check("def _myHelper(): .h1;");
        assert_eq!(diags.len(), 0);
    }
}
