use std::sync::{Arc, RwLock};

use itertools::Itertools;
use tower_lsp::lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind, Position, Range, Url};

pub fn response(hir: Arc<RwLock<mq_hir::Hir>>, url: Url, position: Position) -> Option<Hover> {
    let source = hir.write().unwrap().source_by_url(&url);

    if let Some(source) = source {
        if let Some((_, symbol)) = hir.read().unwrap().find_symbol_in_position(
            source,
            mq_lang::Position::new(position.line + 1, (position.character + 1) as usize),
        ) {
            match &symbol.kind {
                mq_hir::SymbolKind::Function(_) | mq_hir::SymbolKind::Variable => Some(Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: match symbol.kind {
                            mq_hir::SymbolKind::Function(args) => {
                                format!(
                                    "```mq\n{}({})\n```\n\n{doc}",
                                    &symbol.value.unwrap_or_default(),
                                    args.join(", "),
                                    doc = symbol.doc.iter().map(|(_, doc)| doc).join("\n")
                                )
                            }
                            mq_hir::SymbolKind::Variable => {
                                format!(
                                    "```mq\n{}```\n\n{doc}",
                                    &symbol.value.unwrap_or_default(),
                                    doc = symbol.doc.iter().map(|(_, doc)| doc).join("\n")
                                )
                            }
                            _ => String::new(),
                        },
                    }),
                    range: symbol.source.text_range.map(|text_range| {
                        Range::new(
                            Position::new(
                                text_range.start.line - 1,
                                (text_range.start.column - 1) as u32,
                            ),
                            Position::new(
                                text_range.end.line - 1,
                                (text_range.end.column - 1) as u32,
                            ),
                        )
                    }),
                }),
                _ => None,
            }
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
    fn test_function_hover() {
        let mut hir = Hir::default();
        let url = Url::parse("file:///test.mq").unwrap();
        let position = Position::new(0, 5);
        hir.add_code(Some(url.clone()), "def func1(): 1;");

        let hover = response(Arc::new(RwLock::new(hir)), url, position);

        assert!(hover.is_some());
        let hover = hover.unwrap();

        if let HoverContents::Markup(content) = hover.contents {
            assert_eq!(content.kind, MarkupKind::Markdown);
            assert!(content.value.contains("func1"));
        } else {
            panic!("Expected markup content");
        }
    }

    #[test]
    fn test_val_hover() {
        let mut hir = Hir::default();
        let url = Url::parse("file:///test.mq").unwrap();
        let position = Position::new(0, 5);
        hir.add_code(Some(url.clone()), "let val = 1 | val");

        let hover = response(Arc::new(RwLock::new(hir)), url, position);

        assert!(hover.is_some());
        let hover = hover.unwrap();

        if let HoverContents::Markup(content) = hover.contents {
            assert_eq!(content.kind, MarkupKind::Markdown);
            assert!(content.value.contains("val"));
        } else {
            panic!("Expected markup content");
        }
    }

    #[test]
    fn test_no_symbol_at_position() {
        let hir = Hir::default();
        let url = Url::parse("file:///test.mq").unwrap();
        let position = Position::new(10, 10); // Position where no symbol exists

        let hover = response(Arc::new(RwLock::new(hir)), url, position);
        assert!(hover.is_none());
    }

    #[test]
    fn test_invalid_url() {
        let hir = Hir::default();
        let url = Url::parse("file:///nonexistent.mq").unwrap();
        let position = Position::new(0, 0);

        let hover = response(Arc::new(RwLock::new(hir)), url, position);
        assert!(hover.is_none());
    }

    #[test]
    fn test_builtin_function_hover() {
        let mut hir = Hir::default();
        let url = Url::parse("file:///test.mq").unwrap();

        // Add code that calls a builtin function
        hir.add_code(Some(url.clone()), "\"hello\" | len");

        // Try to find symbol at position of "len" (around column 11)
        let position = Position::new(0, 11);

        let hover = response(Arc::new(RwLock::new(hir)), url, position);

        assert!(hover.is_some());
        let hover = hover.unwrap();

        if let tower_lsp::lsp_types::HoverContents::Markup(content) = hover.contents {
            assert_eq!(content.kind, tower_lsp::lsp_types::MarkupKind::Markdown);
            // Check that the hover contains the function signature and description
            assert!(content.value.contains("len(value)"));
            assert!(
                content
                    .value
                    .contains("Returns the length of the given string or array")
            );
        } else {
            panic!("Expected markup content");
        }
    }
}
