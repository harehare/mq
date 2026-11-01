use std::sync::{Arc, RwLock};

use tower_lsp_server::ls_types::{DocumentHighlight, Position};
use url::Url;

use crate::position::LineIndex;

/// Highlights the declaration and every reference of the symbol under the cursor within the
/// same document.
pub(crate) fn response(
    hir: Arc<RwLock<mq_hir::Hir>>,
    url: Url,
    lines: &LineIndex,
    position: Position,
) -> Option<Vec<DocumentHighlight>> {
    let hir = hir.read().unwrap();
    let source = hir.source_by_url(&url)?;
    let (symbol_id, symbol) = hir.find_symbol_in_position(source, lines.to_mq(position))?;

    let highlights = std::iter::once(&symbol)
        .chain(hir.references(symbol_id).iter().map(|(_, reference)| reference))
        .filter(|symbol| symbol.source.source_id == Some(source))
        .filter_map(|symbol| symbol.source.text_range)
        .map(|range| DocumentHighlight {
            range: lines.to_lsp_range(range),
            kind: None,
        })
        .collect::<Vec<_>>();

    (!highlights.is_empty()).then_some(highlights)
}

#[cfg(test)]
mod tests {
    use mq_hir::Hir;
    use tower_lsp_server::ls_types::Range;

    use super::*;

    fn highlight(code: &str, position: Position) -> Option<Vec<Range>> {
        let mut hir = Hir::default();
        let url = Url::parse("file:///test.mq").unwrap();
        hir.add_code(Some(url.clone()), code);
        let lines = LineIndex::new(Arc::new(code.to_string()));
        let highlights = response(Arc::new(RwLock::new(hir)), url, &lines, position)?;
        Some(highlights.into_iter().map(|h| h.range).collect())
    }

    #[test]
    fn test_highlights_definition_and_calls_of_a_function() {
        let ranges = highlight("def f(): 1; | f() | f()", Position::new(0, 4)).expect("highlights");
        assert!(ranges.len() >= 3, "{ranges:?}");
        assert!(ranges.iter().all(|r| r.start.line == 0));
    }

    #[test]
    fn test_highlights_use_utf16_columns_after_non_bmp_characters() {
        // `😀` is two UTF-16 units: the call `f` is at 0-based UTF-16 column 21, not 20.
        let ranges = highlight("def f(): 1; | \"😀\" | f()", Position::new(0, 21)).expect("highlights");
        assert!(ranges.iter().any(|r| r.start == Position::new(0, 21)), "{ranges:?}");
    }

    #[test]
    fn test_no_highlight_on_whitespace() {
        assert!(highlight("def f(): 1;", Position::new(5, 0)).is_none());
    }
}
