use compact_str::CompactString;
use itertools::Itertools;
use thiserror::Error;

use crate::{Hir, Symbol, SymbolKind};

#[derive(Debug, Error)]
pub enum HirError {
    #[error(
        "Unresolved symbol: {}",
        match &similar_name {
            Some(names) => {
                format!("{}, these names seem close though: `{}`", symbol, names.join(", "))
            }
            None => {
                symbol.to_string()
            }
        }
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
                                .find_similar_names(&symbol.clone().name.unwrap_or_default()),
                        })
                    } else {
                        None
                    }
                }
                _ => None,
            })
            .collect_vec()
    }

    pub fn error_ranges(&self) -> Vec<(String, mdq_lang::Range)> {
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
            .collect_vec()
    }

    fn find_similar_names(&self, target: &str) -> Option<Vec<CompactString>> {
        let similar_names: Vec<CompactString> = self
            .symbols
            .iter()
            .filter_map(|(_, symbol)| {
                if matches!(&symbol.kind, SymbolKind::Function(_))
                    | matches!(&symbol.kind, SymbolKind::Variable)
                {
                    let similarity =
                        strsim::jaro_winkler(target, &symbol.name.clone().unwrap_or_default());
                    if similarity > 0.85 {
                        symbol.name.clone()
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect_vec();

        if similar_names.is_empty() {
            None
        } else {
            Some(similar_names)
        }
    }
}
