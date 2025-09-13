use smol_str::SmolStr;
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
        similar_name: Option<Vec<SmolStr>>,
    },
    #[error("Included module not found: {module_name}")]
    ModuleNotFound {
        symbol: Symbol,
        module_name: SmolStr,
    },
}

#[derive(Debug, Error)]
pub enum HirWarning {
    #[error("Unreachable code after halt() function call")]
    UnreachableCode { symbol: Symbol },
}

impl Hir {
    pub fn errors(&self) -> Vec<HirError> {
        self.symbols
            .iter()
            .filter_map(|(symbol_id, symbol)| match symbol.kind {
                SymbolKind::Call | SymbolKind::Ref => {
                    if self.references.contains_key(&symbol_id) {
                        None
                    } else {
                        Some(HirError::UnresolvedSymbol {
                            symbol: symbol.clone(),
                            similar_name: self
                                .find_similar_names(&symbol.clone().value.unwrap_or_default()),
                        })
                    }
                }
                SymbolKind::Include(_) => {
                    let module_name = symbol
                        .clone()
                        .value
                        .unwrap_or(SmolStr::new("unknown"))
                        .clone();
                    match self.module_loader.read_file(&module_name) {
                        Ok(_) => None,
                        Err(_) => Some(HirError::ModuleNotFound {
                            symbol: symbol.clone(),
                            module_name,
                        }),
                    }
                }
                _ => None,
            })
            .collect::<Vec<_>>()
    }

    pub fn warnings(&self) -> Vec<HirWarning> {
        let mut warnings = Vec::new();

        // Find all halt() function calls
        let halt_calls: Vec<_> = self
            .symbols
            .iter()
            .filter(|(_, symbol)| {
                matches!(symbol.kind, SymbolKind::Call) && symbol.value.as_deref() == Some("halt")
            })
            .collect();

        for (halt_symbol_id, halt_symbol) in halt_calls {
            // Find parent scope that contains this halt call
            if let Some(parent_id) = halt_symbol.parent {
                // Find all symbols that come after the halt call in the same parent
                let unreachable_symbols: Vec<_> = self.symbols
                    .iter()
                    .filter(|(_, other_symbol)| {
                        other_symbol.parent == Some(parent_id) &&
                        other_symbol.source.text_range.as_ref()
                            .zip(halt_symbol.source.text_range.as_ref())
                            .map(|(other_range, halt_range)| {
                                // Check if other symbol comes after halt call
                                other_range.start > halt_range.end
                            })
                            .unwrap_or(false) &&
                        // Don't warn about tokens, trivial symbols, or arguments/literals that are part of the halt call
                        !matches!(other_symbol.kind, SymbolKind::Keyword | SymbolKind::Argument | SymbolKind::Number | SymbolKind::String | SymbolKind::Boolean) &&
                        other_symbol.value.is_some() &&
                        other_symbol.parent != Some(halt_symbol_id)
                    })
                    .collect();

                // Add warnings for unreachable symbols
                for (_, unreachable_symbol) in unreachable_symbols {
                    warnings.push(HirWarning::UnreachableCode {
                        symbol: unreachable_symbol.clone(),
                    });
                }
            }
        }

        warnings
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
                        HirError::ModuleNotFound { symbol, .. } => {
                            symbol.source.text_range.clone().unwrap_or_default()
                        }
                    },
                )
            })
            .collect::<Vec<_>>()
    }

    pub fn warning_ranges(&self) -> Vec<(String, mq_lang::Range)> {
        self.warnings()
            .iter()
            .map(|w| {
                (
                    w.to_string(),
                    match w {
                        HirWarning::UnreachableCode { symbol } => {
                            symbol.source.text_range.clone().unwrap_or_default()
                        }
                    },
                )
            })
            .collect::<Vec<_>>()
    }

    fn find_similar_names(&self, target: &str) -> Option<Vec<SmolStr>> {
        let similar_names: Vec<SmolStr> = self
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
    use super::*;

    #[test]
    fn test_find_similar_names() {
        let mut hir = Hir::default();
        let _ = hir.add_code(None, "let test = 1 | let test2 = 1 | let test3 = 1");

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
        let _ = hir.add_code(None, "let abc = 1 | unknown_var | let xyz = 2");

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
            _ => {
                panic!("Expected UnresolvedSymbol error");
            }
        }
    }

    #[test]
    fn test_error_ranges() {
        let mut hir = Hir::default();
        let _ = hir.add_code(None, "let abc = 1 | unknown_var | let xyz = 2");

        let error_ranges = hir.error_ranges();
        assert_eq!(error_ranges.len(), 1);
    }

    #[test]
    fn test_warnings_unreachable_after_halt() {
        let mut hir = Hir::default();
        hir.builtin.disabled = true;

        // Test case: halt() followed by unreachable code
        let code = "def test(): halt(1) | let x = 42";
        let _ = hir.add_code(None, code);

        let warnings = hir.warnings();
        assert_eq!(warnings.len(), 1);

        match &warnings[0] {
            HirWarning::UnreachableCode { symbol } => {
                assert_eq!(symbol.value.as_deref(), Some("x"));
                assert_eq!(symbol.kind, SymbolKind::Variable);
            }
        }
    }

    #[test]
    fn test_warnings_no_unreachable_without_halt() {
        let mut hir = Hir::default();
        hir.builtin.disabled = true;

        // Test case: no halt() call
        let code = "def test(): let x = 42 | let y = 24";
        let _ = hir.add_code(None, code);

        let warnings = hir.warnings();
        assert_eq!(warnings.len(), 0);
    }

    #[test]
    fn test_warnings_halt_at_end_no_warning() {
        let mut hir = Hir::default();
        hir.builtin.disabled = true;

        // Test case: halt() at the end, no unreachable code
        let code = "def test(): let x = 42 | halt(1)";
        let _ = hir.add_code(None, code);

        let warnings = hir.warnings();
        assert_eq!(warnings.len(), 0);
    }

    #[test]
    fn test_warning_ranges() {
        let mut hir = Hir::default();
        hir.builtin.disabled = true;

        // Test case: halt() followed by unreachable code
        let code = "def test(): halt(1) | let x = 42";
        let _ = hir.add_code(None, code);

        let warning_ranges = hir.warning_ranges();
        assert_eq!(warning_ranges.len(), 1);

        let (message, _) = &warning_ranges[0];
        assert_eq!(message, "Unreachable code after halt() function call");
    }
}
