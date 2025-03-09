use std::sync::{Arc, RwLock};

use bimap::BiMap;
use tower_lsp::lsp_types::{
    DocumentSymbol, DocumentSymbolResponse, Position, Range, SymbolKind, Url,
};

#[allow(deprecated)]
pub fn response(
    hir: Arc<RwLock<mq_hir::Hir>>,
    url: Url,
    source_map: BiMap<String, mq_hir::SourceId>,
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
                        mq_hir::SymbolKind::Function(_) => SymbolKind::FUNCTION,
                        mq_hir::SymbolKind::Variable => SymbolKind::FIELD,
                        mq_hir::SymbolKind::String => SymbolKind::STRING,
                        mq_hir::SymbolKind::Boolean => SymbolKind::BOOLEAN,
                        mq_hir::SymbolKind::None => SymbolKind::NULL,
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
            .collect::<Vec<_>>();

        DocumentSymbolResponse::Nested(symbols)
    })
}
