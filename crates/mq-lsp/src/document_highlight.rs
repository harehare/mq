use std::sync::{Arc, RwLock};

use tower_lsp_server::ls_types::{DocumentHighlight, Position, Range};
use url::Url;

fn to_range(range: mq_lang::Range) -> Range {
    Range::new(
        Position::new(range.start.line - 1, (range.start.column - 1) as u32),
        Position::new(range.end.line - 1, (range.end.column - 1) as u32),
    )
}

/// Highlights the declaration and every reference of the symbol under the cursor within the
/// same document.
pub(crate) fn response(hir: Arc<RwLock<mq_hir::Hir>>, url: Url, position: Position) -> Option<Vec<DocumentHighlight>> {
    let hir = hir.read().unwrap();
    let source = hir.source_by_url(&url)?;
    let (symbol_id, symbol) = hir.find_symbol_in_position(
        source,
        mq_lang::Position::new(position.line + 1, (position.character + 1) as usize),
    )?;

    let highlights = std::iter::once(&symbol)
        .chain(hir.references(symbol_id).iter().map(|(_, reference)| reference))
        .filter(|symbol| symbol.source.source_id == Some(source))
        .filter_map(|symbol| symbol.source.text_range)
        .map(|range| DocumentHighlight {
            range: to_range(range),
            kind: None,
        })
        .collect::<Vec<_>>();

    (!highlights.is_empty()).then_some(highlights)
}

#[cfg(test)]
mod tests {
    use mq_hir::Hir;

    use super::*;

    fn highlight(code: &str, position: Position) -> Option<Vec<Range>> {
        let mut hir = Hir::default();
        let url = Url::parse("file:///test.mq").unwrap();
        hir.add_code(Some(url.clone()), code);
        let highlights = response(Arc::new(RwLock::new(hir)), url, position)?;
        Some(highlights.into_iter().map(|h| h.range).collect())
    }

    #[test]
    fn test_highlights_definition_and_calls_of_a_function() {
        let ranges = highlight("def f(): 1; | f() | f()", Position::new(0, 4)).expect("highlights");
        assert!(ranges.len() >= 3, "{ranges:?}");
        assert!(ranges.iter().all(|r| r.start.line == 0));
    }

    #[test]
    fn test_no_highlight_on_whitespace() {
        assert!(highlight("def f(): 1;", Position::new(5, 0)).is_none());
    }
}
