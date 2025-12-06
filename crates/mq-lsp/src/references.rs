use std::{
    str::FromStr,
    sync::{Arc, RwLock},
};

use bimap::BiMap;
use tower_lsp_server::lsp_types::{self, Location, Position, Range};
use url::Url;

pub fn response(
    hir: Arc<RwLock<mq_hir::Hir>>,
    url: Url,
    position: Position,
    source_map: &BiMap<String, mq_hir::SourceId>,
) -> Option<Vec<Location>> {
    let hir_guard = hir.read().unwrap();
    let source = hir_guard.source_by_url(&url);

    if let Some(source) = source {
        if let Some((symbol_id, _)) = hir_guard.find_symbol_in_position(
            source,
            mq_lang::Position::new(position.line + 1, (position.character + 1) as usize),
        ) {
            let locations = hir_guard
                .references(symbol_id)
                .iter()
                .filter_map(|(_, symbol)| {
                    symbol.source.text_range.and_then(|text_range| {
                        symbol.source.source_id.and_then(|id| {
                            source_map.get_by_right(&id).map(|url| Location {
                                uri: lsp_types::Uri::from_str(url).unwrap(),
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
                            })
                        })
                    })
                })
                .collect::<Vec<_>>();

            if locations.is_empty() { None } else { Some(locations) }
        } else {
            None
        }
    } else {
        None
    }
}
#[cfg(test)]
mod tests {
    use mq_hir::{Hir, SourceId};

    use super::*;

    fn setup() -> (Arc<RwLock<Hir>>, BiMap<String, SourceId>) {
        let hir = Arc::new(RwLock::new(Hir::default()));
        let source_map = BiMap::new();
        (hir, source_map)
    }

    #[test]
    fn test_response_no_source() {
        let (hir, source_map) = setup();
        let url = Url::parse("file:///test.mq").unwrap();
        let position = Position::new(0, 0);

        let result = response(hir, url, position, &source_map);
        assert!(result.is_none());
    }

    #[test]
    fn test_response_with_references() {
        let (hir, mut source_map) = setup();
        let url = Url::parse("file:///test.mq").unwrap();
        let (source_id, _) = hir
            .write()
            .unwrap()
            .add_code(Some(url.clone()), "def func1(): 1; | let x = func1()");
        source_map.insert(url.to_string(), source_id);

        let position = Position::new(0, 5);
        let result = response(hir, url, position, &source_map);

        assert!(result.is_some());
        let locations = result.unwrap();
        assert_eq!(locations.len(), 1);
    }
}
