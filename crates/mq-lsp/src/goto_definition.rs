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
#[cfg(test)]
mod tests {
    use mq_hir::Hir;

    use super::*;

    #[test]
    fn test_goto_definition_found() {
        let mut hir = Hir::default();
        let url = Url::parse("file:///test.mq").unwrap();
        hir.add_code(Some(url.clone()), "def func1(): 1; func1()");

        let result = response(Arc::new(RwLock::new(hir)), url.clone(), Position::new(0, 4));

        assert!(result.is_some());
        if let Some(GotoDefinitionResponse::Scalar(location)) = result {
            assert_eq!(location.uri, url);
            assert_eq!(location.range.start, Position::new(0, 4));
            assert_eq!(location.range.end, Position::new(0, 9));
        } else {
            panic!("Expected Scalar response");
        }
    }

    #[test]
    fn test_goto_definition_not_found() {
        let hir = Arc::new(RwLock::new(mq_hir::Hir::default()));
        let url = Url::parse("file:///test.mq").unwrap();

        let result = response(hir, url, Position::new(0, 0));
        assert!(result.is_none());
    }

    #[test]
    fn test_goto_definition_no_text_range() {
        let mut hir = mq_hir::Hir::default();
        let url = Url::parse("file:///test.mq").unwrap();
        hir.add_code(Some(url.clone()), "let x = 42;");

        let result = response(Arc::new(RwLock::new(hir)), url, Position::new(0, 11));
        assert!(result.is_none());
    }
}
