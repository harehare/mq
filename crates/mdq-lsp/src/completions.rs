use std::sync::{Arc, RwLock};

use bimap::BiMap;
use itertools::Itertools;
use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, CompletionResponse, Documentation, InsertTextFormat,
    MarkupContent, MarkupKind, Position, Url,
};

pub fn response(
    hir: Arc<RwLock<mdq_hir::Hir>>,
    url: Url,
    position: Position,
    source_map: BiMap<String, mdq_hir::SourceId>,
) -> Option<CompletionResponse> {
    match source_map.get_by_left(&url.to_string()) {
        Some(source_id) => {
            let scope_id = hir
                .read()
                .unwrap()
                .find_scope_in_position(
                    *source_id,
                    mdq_lang::Position::new(position.line + 1, (position.character + 1) as usize),
                )
                .map(|(scope_id, _)| scope_id)
                .unwrap_or_else(|| hir.read().unwrap().find_scope_by_source(source_id));
            let symbols = itertools::concat(vec![
                hir.read().unwrap().find_symbols_in_scope(scope_id),
                hir.read()
                    .unwrap()
                    .find_symbols_in_source(hir.read().unwrap().builtin.source_id),
            ]);
            Some(CompletionResponse::Array(
                symbols
                    .iter()
                    .filter_map(|symbol| match &symbol.kind {
                        mdq_hir::SymbolKind::Function(params) => Some(CompletionItem {
                            label: symbol.name.clone().unwrap_or_default().to_string(),
                            kind: Some(CompletionItemKind::FUNCTION),
                            detail: Some(symbol.doc.iter().map(|(_, doc)| doc).join("\n")),
                            insert_text: Some(format!(
                                "{}({})",
                                symbol.name.clone().unwrap_or_default(),
                                params
                                    .iter()
                                    .enumerate()
                                    .map(|(i, name)| format!("${{{}:{}{}}}", i + 1, name, i + 1))
                                    .join(", ")
                            )),
                            insert_text_format: Some(InsertTextFormat::SNIPPET),
                            documentation: Some(Documentation::MarkupContent(MarkupContent {
                                kind: MarkupKind::Markdown,
                                value: format!(
                                    "```md\n{}\n```",
                                    symbol.doc.iter().map(|(_, doc)| doc).join("\n")
                                ),
                            })),
                            ..Default::default()
                        }),
                        mdq_hir::SymbolKind::Parameter | mdq_hir::SymbolKind::Variable => {
                            Some(CompletionItem {
                                label: symbol.name.clone().unwrap_or_default().to_string(),
                                kind: Some(CompletionItemKind::VARIABLE),
                                detail: Some(symbol.doc.iter().map(|(_, doc)| doc).join("\n")),
                                documentation: Some(Documentation::MarkupContent(MarkupContent {
                                    kind: MarkupKind::Markdown,
                                    value: format!(
                                        "```md\n{}\n```",
                                        symbol.doc.iter().map(|(_, doc)| doc).join("\n")
                                    ),
                                })),
                                ..Default::default()
                            })
                        }
                        mdq_hir::SymbolKind::Selector => Some(CompletionItem {
                            label: symbol.name.clone().unwrap_or_default().to_string(),
                            kind: Some(CompletionItemKind::METHOD),
                            detail: Some(symbol.doc.iter().map(|(_, doc)| doc).join("\n")),
                            insert_text: Some(symbol.name.clone().unwrap_or_default().into()),
                            insert_text_format: Some(InsertTextFormat::SNIPPET),
                            documentation: Some(Documentation::MarkupContent(MarkupContent {
                                kind: MarkupKind::Markdown,
                                value: format!(
                                    "```md\n{}\n```",
                                    symbol.doc.iter().map(|(_, doc)| doc).join("\n")
                                ),
                            })),
                            ..Default::default()
                        }),
                        _ => None,
                    })
                    .collect_vec(),
            ))
        }
        None => None,
    }
}
