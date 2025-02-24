use std::sync::{Arc, RwLock};

use bimap::BiMap;
use itertools::Itertools;
use tower_lsp::lsp_types::{
    DocumentSymbol, DocumentSymbolResponse, Position, Range, SymbolKind, Url,
};

#[allow(deprecated)]
pub fn response(
    hir: Arc<RwLock<mdq_hir::Hir>>,
    url: Url,
    source_map: BiMap<String, mdq_hir::SourceId>,
) -> Option<DocumentSymbolResponse> {
    source_map.get_by_left(&url.to_string()).map(|source_id| {
        let symbols = hir
            .read()
            .unwrap()
            .find_symbols_in_source(*source_id)
            .iter()
            .filter_map(|symbol| {
                symbol.source.text_range.clone().and_then(|text_range| {
                    let kind = match &symbol.kind {
                        mdq_hir::SymbolKind::Function(_) => SymbolKind::FUNCTION,
                        mdq_hir::SymbolKind::Variable => SymbolKind::FIELD,
                        mdq_hir::SymbolKind::String => SymbolKind::STRING,
                        mdq_hir::SymbolKind::Boolean => SymbolKind::BOOLEAN,
                        mdq_hir::SymbolKind::None => SymbolKind::NULL,
                        _ => return None,
                    };

                    Some(DocumentSymbol {
                        name: symbol.name.clone().unwrap_or_default().to_string(),
                        detail: None,
                        kind,
                        tags: None,
                        range: Range {
                            start: Position {
                                line: text_range.start.line - 1,
                                character: (text_range.start.column - 1) as u32,
                            },
                            end: Position {
                                line: text_range.end.line - 1,
                                character: (text_range.end.column - 1) as u32,
                            },
                        },
                        selection_range: Range {
                            start: Position {
                                line: text_range.start.line - 1,
                                character: (text_range.start.column - 1) as u32,
                            },
                            end: Position {
                                line: text_range.start.line - 1,
                                character: (text_range.start.column - 1) as u32,
                            },
                        },
                        children: None,
                        deprecated: None,
                    })
                })
            })
            .collect_vec();

        DocumentSymbolResponse::Nested(symbols)
    })
}
