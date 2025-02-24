use std::sync::{Arc, RwLock};

use tower_lsp::lsp_types::{GotoDefinitionResponse, Location, Position, Range, Url};

pub fn response(
    hir: Arc<RwLock<mq_hir::Hir>>,
    url: Url,
    position: Position,
) -> Option<GotoDefinitionResponse> {
    let source = hir.write().unwrap().source_by_url(&url);

    if let Some(source) = source {
        if let Some((_, symbol)) = hir.read().unwrap().find_symbol_in_position(
            source,
            mq_lang::Position::new(position.line + 1, (position.character + 1) as usize),
        ) {
            symbol.source.text_range.map(|text_range| {
                GotoDefinitionResponse::Scalar(Location {
                    uri: url,
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
        } else {
            None
        }
    } else {
        None
    }
}
