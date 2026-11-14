use std::{
    str::FromStr,
    sync::{Arc, RwLock},
};

use bimap::BiMap;
use tower_lsp_server::ls_types::{self, Location, Position, Range};
use url::Url;

use crate::virtual_document;

fn to_location(
    hir: &mq_hir::Hir,
    source_map: &BiMap<String, mq_hir::SourceId>,
    symbol: &mq_hir::Symbol,
) -> Option<Location> {
    let text_range = symbol.source.text_range?;
    let source_id = symbol.source.source_id?;
    // Module files are not open documents, so they are absent from `source_map`.
    let uri = match source_map.get_by_right(&source_id) {
        Some(url) => ls_types::Uri::from_str(url).ok()?,
        None => virtual_document::uri_for_source(hir, source_id)?,
    };
    Some(Location {
        uri,
        range: Range {
            start: Position {
                line: text_range.start.line - 1,
                character: (text_range.start.column - 1),
            },
            end: Position {
                line: text_range.end.line - 1,
                character: (text_range.end.column - 1),
            },
        },
    })
}

pub(crate) fn response(
    hir: Arc<RwLock<mq_hir::Hir>>,
    url: Url,
    position: Position,
    include_declaration: bool,
    source_map: &BiMap<String, mq_hir::SourceId>,
) -> Option<Vec<Location>> {
    let hir_guard = hir.read().unwrap();
    let source = hir_guard.source_by_url(&url)?;
    let (symbol_id, symbol) = hir_guard.find_symbol_in_position(
        source,
        mq_lang::Position::new(position.line + 1, position.character + 1),
    )?;

    let declaration = (include_declaration && mq_hir::Hir::is_resolvable_target(&symbol)).then_some(&symbol);
    let references = hir_guard.references(symbol_id);
    let locations = declaration
        .into_iter()
        .chain(references.iter().map(|(_, reference)| reference))
        .filter_map(|symbol| to_location(&hir_guard, source_map, symbol))
        .collect::<Vec<_>>();

    if locations.is_empty() { None } else { Some(locations) }
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

        let result = response(hir, url, position, false, &source_map);
        assert!(result.is_none());
    }

    #[test]
    fn test_response_with_references() {
        let (hir, mut source_map) = setup();
        let url = Url::parse("file:///test.mq").unwrap();
        let (source_id, _, _) = hir
            .write()
            .unwrap()
            .add_code(Some(url.clone()), "def func1(): 1; | let x = func1()");
        source_map.insert(url.to_string(), source_id);

        let position = Position::new(0, 5);
        let result = response(hir, url, position, false, &source_map);

        assert!(result.is_some());
        let locations = result.unwrap();
        assert_eq!(locations.len(), 1);
    }

    #[rstest::rstest]
    #[case::without_declaration(false, 1)]
    #[case::with_declaration(true, 2)]
    fn test_references_of_qualified_member_shadowing_builtin(#[case] include_declaration: bool, #[case] count: usize) {
        let (hir, url) = crate::test_support::hir_with_lib("refs", "import \"lib\"\n| lib::add(1, 2)\n| lib::exported");
        let source = hir.read().unwrap().source_by_url(&url).unwrap();
        let mut source_map = BiMap::new();
        source_map.insert(url.to_string(), source);

        let locations = response(hir, url.clone(), Position::new(1, 8), include_declaration, &source_map).unwrap();
        assert_eq!(locations.len(), count);
        assert!(locations.iter().any(|l| l.uri.as_str() == url.as_str()));
        assert_eq!(
            locations.iter().any(|l| l.uri.as_str().ends_with("/lib.mq")),
            include_declaration
        );
    }
}
