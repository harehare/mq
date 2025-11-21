use std::sync::{Arc, RwLock};

use bimap::BiMap;
use tower_lsp_server::lsp_types::{DocumentSymbol, DocumentSymbolResponse, Position, Range, SymbolKind};
use url::Url;

#[allow(deprecated)]
pub fn response(
    hir: Arc<RwLock<mq_hir::Hir>>,
    url: Url,
    source_map: &BiMap<String, mq_hir::SourceId>,
) -> Option<DocumentSymbolResponse> {
    source_map.get_by_left(&url.to_string()).map(|source_id| {
        let symbols = hir
            .read()
            .unwrap()
            .find_symbols_in_source(*source_id)
            .iter()
            .filter_map(|symbol| {
                symbol.source.text_range.and_then(|text_range| {
                    let kind = match &symbol.kind {
                        mq_hir::SymbolKind::Function(_) => SymbolKind::FUNCTION,
                        mq_hir::SymbolKind::Variable => SymbolKind::FIELD,
                        mq_hir::SymbolKind::String => SymbolKind::STRING,
                        mq_hir::SymbolKind::Boolean => SymbolKind::BOOLEAN,
                        mq_hir::SymbolKind::None => SymbolKind::NULL,
                        _ => return None,
                    };

                    symbol.value.as_ref().and_then(|name| {
                        if name.is_empty() {
                            None
                        } else {
                            Some(DocumentSymbol {
                                name: name.to_string(),
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
                        }
                    })
                })
            })
            .collect::<Vec<_>>();

        DocumentSymbolResponse::Nested(symbols)
    })
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_response_with_empty_symbols() {
        let hir = Arc::new(RwLock::new(mq_hir::Hir::default()));
        let url = Url::parse("file:///test.mq").unwrap();
        let source_map = BiMap::new();
        let res = response(hir.clone(), url.clone(), &source_map);

        assert!(res.is_none());
    }

    #[test]
    fn test_response_with_various_symbols() {
        let mut hir = mq_hir::Hir::default();
        let url = Url::parse("file:///test.mq").unwrap();

        let (source_id, _) = hir.add_code(Some(url.clone()), "def func1(): 1; let var1 = 2");
        let mut source_map = BiMap::new();
        source_map.insert(url.to_string(), source_id);

        let res = response(Arc::new(RwLock::new(hir)), url.clone(), &source_map);
        assert!(res.is_some());

        if let DocumentSymbolResponse::Nested(symbols) = res.unwrap() {
            assert_eq!(symbols.len(), 2);

            let func = symbols.iter().find(|s| s.name == "func1");
            assert!(func.is_some());
            assert_eq!(func.unwrap().kind, SymbolKind::FUNCTION);

            let var = symbols.iter().find(|s| s.name == "var1");
            assert!(var.is_some());
            assert_eq!(var.unwrap().kind, SymbolKind::FIELD);
        } else {
            panic!("Expected Nested response");
        }
    }
}
