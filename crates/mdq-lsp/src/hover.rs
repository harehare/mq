use std::sync::{Arc, RwLock};

use itertools::Itertools;
use tower_lsp::lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind, Position, Range, Url};

pub fn response(hir: Arc<RwLock<mdq_hir::Hir>>, url: Url, position: Position) -> Option<Hover> {
    let source = hir.write().unwrap().source_by_url(&url);

    if let Some(source) = source {
        if let Some((_, symbol)) = hir.read().unwrap().find_symbol_in_position(
            source,
            mdq_lang::Position::new(position.line + 1, (position.character + 1) as usize),
        ) {
            symbol
                .clone()
                .source
                .text_range
                .and_then(|text_range| match &symbol.kind {
                    mdq_hir::SymbolKind::Function(_) | mdq_hir::SymbolKind::Variable => {
                        Some(Hover {
                            contents: HoverContents::Markup(MarkupContent {
                                kind: MarkupKind::Markdown,
                                value: match symbol.kind {
                                    mdq_hir::SymbolKind::Function(args) => {
                                        format!(
                                            "```mdq\n{}({})\n```\n\n{doc}",
                                            &symbol.name.unwrap_or_default(),
                                            args.join(", "),
                                            doc = symbol.doc.iter().map(|(_, doc)| doc).join("\n")
                                        )
                                    }
                                    mdq_hir::SymbolKind::Variable => {
                                        format!(
                                            "```mdq\n{}```\n\n{doc}",
                                            &symbol.name.unwrap_or_default(),
                                            doc = symbol.doc.iter().map(|(_, doc)| doc).join("\n")
                                        )
                                    }
                                    _ => String::new(),
                                },
                            }),
                            range: Some(Range::new(
                                Position::new(
                                    text_range.start.line - 1,
                                    (text_range.start.column - 1) as u32,
                                ),
                                Position::new(
                                    text_range.end.line - 1,
                                    (text_range.end.column - 1) as u32,
                                ),
                            )),
                        })
                    }
                    _ => None,
                })
        } else {
            None
        }
    } else {
        None
    }
}
