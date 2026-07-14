use std::str::FromStr;
use std::sync::{Arc, RwLock};

use bimap::BiMap;
use tower_lsp_server::ls_types::{
    self, Location, OneOf, Position, Range, SymbolKind, SymbolTag, WorkspaceSymbol, WorkspaceSymbolResponse,
};

pub(crate) fn response(
    hir: Arc<RwLock<mq_hir::Hir>>,
    query: &str,
    source_map: &BiMap<String, mq_hir::SourceId>,
) -> Option<WorkspaceSymbolResponse> {
    let hir_guard = hir.read().unwrap();
    let query = query.to_lowercase();

    let symbols = hir_guard
        .symbols()
        .filter_map(|(_, symbol)| {
            let name = symbol.value.as_ref()?;
            if name.is_empty() || (!query.is_empty() && !name.to_lowercase().contains(&query)) {
                return None;
            }

            let kind = match &symbol.kind {
                mq_hir::SymbolKind::Function(_) | mq_hir::SymbolKind::Macro(_) => SymbolKind::FUNCTION,
                mq_hir::SymbolKind::Variable | mq_hir::SymbolKind::DestructuringBinding => SymbolKind::FIELD,
                mq_hir::SymbolKind::String => SymbolKind::STRING,
                mq_hir::SymbolKind::Boolean => SymbolKind::BOOLEAN,
                mq_hir::SymbolKind::None => SymbolKind::NULL,
                _ => return None,
            };

            let text_range = symbol.source.text_range?;
            let source_id = symbol.source.source_id?;
            let url = source_map.get_by_right(&source_id)?;
            let is_deprecated = symbol.is_deprecated();

            Some(WorkspaceSymbol {
                name: name.to_string(),
                kind,
                tags: if is_deprecated {
                    Some(vec![SymbolTag::DEPRECATED])
                } else {
                    None
                },
                container_name: None,
                location: OneOf::Left(Location {
                    uri: ls_types::Uri::from_str(url).unwrap(),
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
                }),
                data: None,
            })
        })
        .collect::<Vec<_>>();

    Some(WorkspaceSymbolResponse::Nested(symbols))
}

#[cfg(test)]
mod tests {
    use super::*;
    use url::Url;

    #[test]
    fn test_response_with_empty_hir() {
        let hir = Arc::new(RwLock::new(mq_hir::Hir::default()));
        let source_map = BiMap::new();
        let res = response(hir, "", &source_map);

        assert!(matches!(res, Some(WorkspaceSymbolResponse::Nested(symbols)) if symbols.is_empty()));
    }

    #[test]
    fn test_response_matches_across_multiple_sources() {
        let mut hir = mq_hir::Hir::default();
        let url1 = Url::parse("file:///a.mq").unwrap();
        let url2 = Url::parse("file:///b.mq").unwrap();

        let (source_id1, _) = hir.add_code(Some(url1.clone()), "def func_a(): 1;");
        let (source_id2, _) = hir.add_code(Some(url2.clone()), "def func_b(): 2; | let var_b = 3");

        let mut source_map = BiMap::new();
        source_map.insert(url1.to_string(), source_id1);
        source_map.insert(url2.to_string(), source_id2);

        let hir = Arc::new(RwLock::new(hir));

        let res = response(hir.clone(), "func", &source_map);
        if let Some(WorkspaceSymbolResponse::Nested(symbols)) = res {
            assert_eq!(symbols.len(), 2);
            assert!(symbols.iter().any(|s| s.name == "func_a"));
            assert!(symbols.iter().any(|s| s.name == "func_b"));
            assert!(symbols.iter().all(|s| s.kind == SymbolKind::FUNCTION));
        } else {
            panic!("Expected Nested response");
        }

        let res = response(hir.clone(), "var_b", &source_map);
        if let Some(WorkspaceSymbolResponse::Nested(symbols)) = res {
            assert_eq!(symbols.len(), 1);
            assert_eq!(symbols[0].name, "var_b");
        } else {
            panic!("Expected Nested response");
        }

        let res = response(hir, "does_not_exist", &source_map);
        assert!(matches!(res, Some(WorkspaceSymbolResponse::Nested(symbols)) if symbols.is_empty()));
    }
}
