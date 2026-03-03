use std::sync::{Arc, RwLock};

use tower_lsp_server::ls_types::{InlayHint, InlayHintKind, InlayHintLabel, Position, Range};
use url::Url;

/// Returns inlay hints for the visible range of a document.
///
/// When type checking is enabled and a `TypeEnv` is provided, this function
/// produces inlay hints showing the inferred type for variable bindings and
/// function definitions within the given range.
pub fn response(
    hir: Arc<RwLock<mq_hir::Hir>>,
    url: Url,
    type_env: Option<mq_check::TypeEnv>,
    range: Range,
) -> Option<Vec<InlayHint>> {
    let type_env = type_env?;
    let source = hir.write().unwrap().source_by_url(&url)?;

    let hints: Vec<InlayHint> = hir
        .read()
        .unwrap()
        .symbols()
        .filter_map(|(symbol_id, symbol)| {
            // Only process symbols from this source file
            if symbol.source.source_id != Some(source) {
                return None;
            }

            let text_range = symbol.source.text_range.as_ref()?;

            // Convert mq 1-based positions to LSP 0-based positions
            let symbol_line = text_range.start.line.saturating_sub(1);
            let symbol_col = (text_range.start.column as u32).saturating_sub(1);

            // Only show hints for symbols within the requested range
            if symbol_line < range.start.line
                || symbol_line > range.end.line
                || (symbol_line == range.start.line && symbol_col < range.start.character)
                || (symbol_line == range.end.line && symbol_col > range.end.character)
            {
                return None;
            }

            let type_scheme = type_env.get(&symbol_id)?;
            let type_label = format!(": {}", type_scheme.ty);

            match &symbol.kind {
                mq_hir::SymbolKind::Variable => {
                    // Place hint after the variable name (end of the symbol)
                    let hint_col = (text_range.end.column as u32).saturating_sub(1);
                    let hint_line = text_range.end.line.saturating_sub(1);
                    Some(InlayHint {
                        position: Position::new(hint_line, hint_col),
                        label: InlayHintLabel::String(type_label),
                        kind: Some(InlayHintKind::TYPE),
                        text_edits: None,
                        tooltip: None,
                        padding_left: Some(false),
                        padding_right: Some(true),
                        data: None,
                    })
                }
                mq_hir::SymbolKind::Function(_) | mq_hir::SymbolKind::Macro(_) => {
                    // Place hint after the function name showing the return type
                    let hint_col = (text_range.end.column as u32).saturating_sub(1);
                    let hint_line = text_range.end.line.saturating_sub(1);
                    Some(InlayHint {
                        position: Position::new(hint_line, hint_col),
                        label: InlayHintLabel::String(type_label),
                        kind: Some(InlayHintKind::TYPE),
                        text_edits: None,
                        tooltip: None,
                        padding_left: Some(false),
                        padding_right: Some(true),
                        data: None,
                    })
                }
                _ => None,
            }
        })
        .collect();

    Some(hints)
}

#[cfg(test)]
mod tests {
    use mq_check::TypeChecker;
    use mq_hir::Hir;

    use super::*;

    fn make_full_range() -> Range {
        Range::new(Position::new(0, 0), Position::new(u32::MAX, u32::MAX))
    }

    #[test]
    fn test_no_hints_without_type_env() {
        let mut hir = Hir::default();
        let url = Url::parse("file:///test.mq").unwrap();
        hir.add_code(Some(url.clone()), "let x = 1 | x");

        let result = response(Arc::new(RwLock::new(hir)), url, None, make_full_range());
        assert!(result.is_none());
    }

    #[test]
    fn test_variable_inlay_hint() {
        let mut hir = Hir::default();
        let url = Url::parse("file:///test.mq").unwrap();
        hir.add_code(Some(url.clone()), "let x = 1 | x");

        let hir = Arc::new(RwLock::new(hir));
        let mut checker = TypeChecker::new();
        checker.check(&hir.read().unwrap());
        let type_env = Some(checker.symbol_types().clone());

        let hints = response(Arc::clone(&hir), url, type_env, make_full_range());
        assert!(hints.is_some());
        let hints = hints.unwrap();
        assert!(!hints.is_empty(), "Should produce at least one inlay hint");
        // The hint label should contain a type annotation starting with ':'
        assert!(
            hints
                .iter()
                .any(|h| matches!(&h.label, InlayHintLabel::String(s) if s.starts_with(':'))),
            "Should have a type annotation hint"
        );
    }

    #[test]
    fn test_hints_outside_range_excluded() {
        let mut hir = Hir::default();
        let url = Url::parse("file:///test.mq").unwrap();
        hir.add_code(Some(url.clone()), "let x = 1 | x");

        let hir = Arc::new(RwLock::new(hir));
        let mut checker = TypeChecker::new();
        checker.check(&hir.read().unwrap());
        let type_env = Some(checker.symbol_types().clone());

        // Range that doesn't cover line 0
        let restricted_range = Range::new(Position::new(5, 0), Position::new(10, 100));
        let hints = response(Arc::clone(&hir), url, type_env, restricted_range);
        let hints = hints.unwrap_or_default();
        assert!(hints.is_empty(), "No hints should appear outside the range");
    }
}
