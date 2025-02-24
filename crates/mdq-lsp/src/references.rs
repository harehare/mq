use std::sync::{Arc, RwLock};

use bimap::BiMap;
use itertools::Itertools;
use tower_lsp::lsp_types::{Location, Position, Range, Url};

pub fn response(
    hir: Arc<RwLock<mdq_hir::Hir>>,
    url: Url,
    position: Position,
    source_map: BiMap<String, mdq_hir::SourceId>,
) -> Option<Vec<Location>> {
    let source = hir.write().unwrap().source_by_url(&url);

    if let Some(source) = source {
        if let Some((symbol_id, _)) = hir.read().unwrap().find_symbol_in_position(
            source,
            mdq_lang::Position::new(position.line + 1, (position.character + 1) as usize),
        ) {
            let locations = hir
                .read()
                .unwrap()
                .references(symbol_id)
                .iter()
                .filter_map(|(_, symbol)| {
                    symbol.source.text_range.clone().and_then(|text_range| {
                        symbol.source.source_id.and_then(|id| {
                            source_map.get_by_right(&id).map(|url| Location {
                                uri: Url::parse(url).unwrap(),
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
                .collect_vec();

            if locations.is_empty() {
                None
            } else {
                Some(locations)
            }
        } else {
            None
        }
    } else {
        None
    }
}
