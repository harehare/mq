use compact_str::CompactString;
use thiserror::Error;

use crate::{Hir, Symbol, SymbolKind};

#[derive(Debug, Error)]
pub enum HirError {
    #[error(
        "Unresolved symbol: {}",
        similar_name
            .as_ref()
            .map(|names| format!("{}, these names seem close though: `{}`", symbol, names.join(", ")))
            .unwrap_or_else(|| symbol.to_string())
    )]
    UnresolvedSymbol {
        symbol: Symbol,
        similar_name: Option<Vec<CompactString>>,
    },
}

impl Hir {
    pub fn errors(&self) -> Vec<HirError> {
        self.symbols
            .iter()
            .filter_map(|(symbol_id, symbol)| match symbol.kind {
                SymbolKind::Call | SymbolKind::Ref => {
                    if !self.references.contains_key(&symbol_id) {
                        Some(HirError::UnresolvedSymbol {
                            symbol: symbol.clone(),
                            similar_name: self
                                .find_similar_names(&symbol.clone().value.unwrap_or_default()),
                        })
                    } else {
                        None
                    }
                }
                _ => None,
            })
            .collect::<Vec<_>>()
    }

    pub fn error_ranges(&self) -> Vec<(String, mq_lang::Range)> {
        self.errors()
            .iter()
            .map(|e| {
                (
                    e.to_string(),
                    match e {
                        HirError::UnresolvedSymbol { symbol, .. } => {
                            symbol.source.text_range.clone().unwrap_or_default()
                        }
                    },
                )
            })
            .collect::<Vec<_>>()
    }

    fn find_similar_names(&self, target: &str) -> Option<Vec<CompactString>> {
        let similar_names: Vec<CompactString> = self
            .symbols
            .iter()
            .filter_map(|(_, symbol)| {
                if (matches!(&symbol.kind, SymbolKind::Function(_))
                    || matches!(&symbol.kind, SymbolKind::Variable))
                    && symbol.value.as_ref().is_some_and(|name| name != target)
                {
                    let name = symbol.value.as_deref().unwrap_or("");
                    let similarity = strsim::jaro_winkler(target, name);
                    if similarity > 0.85 {
                        symbol.value.clone()
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        if similar_names.is_empty() {
            None
        } else {
            Some(similar_names)
        }
    }
}
#[cfg(test)]
mod tests {
    use url::Url;

    use super::*;

    #[test]
    fn test_find_similar_names() {
        let mut hir = Hir::default();
        let url = Url::parse("file:///test").unwrap();
        let _ = hir.add_code(url.clone(), "let test = 1 | let test2 = 1 | let test3 = 1");

        let similar = hir.find_similar_names("test");
        assert!(similar.is_some());
        let similar_vec = similar.unwrap();
        assert_eq!(similar_vec.len(), 2);
        assert!(similar_vec.contains(&"test2".into()));
        assert!(similar_vec.contains(&"test3".into()));

        let no_similar = hir.find_similar_names("xyz123");
        assert!(no_similar.is_none());
    }
    #[test]
    fn test_errors() {
        let mut hir = Hir::default();
        let url = Url::parse("file:///test").unwrap();
        let _ = hir.add_code(url.clone(), "let abc = 1 | unknown_var | let xyz = 2");

        let errors = hir.errors();
        assert_eq!(errors.len(), 1);
        match &errors[0] {
            HirError::UnresolvedSymbol {
                symbol,
                similar_name,
            } => {
                assert_eq!(symbol.value.as_deref(), Some("unknown_var"));
                assert!(similar_name.is_none());
            }
        }
    }

    #[test]
    fn test_error_ranges() {
        let mut hir = Hir::default();
        let url = Url::parse("file:///test").unwrap();
        let _ = hir.add_code(url.clone(), "let abc = 1 | unknown_var | let xyz = 2");

        let error_ranges = hir.error_ranges();
        assert_eq!(error_ranges.len(), 1);
    }
}
