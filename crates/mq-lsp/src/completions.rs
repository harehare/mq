use std::sync::{Arc, RwLock};

use bimap::BiMap;
use itertools::Itertools;
use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, CompletionResponse, Documentation, InsertTextFormat,
    MarkupContent, MarkupKind, Position, Url,
};

pub fn response(
    hir: Arc<RwLock<mq_hir::Hir>>,
    url: Url,
    position: Position,
    source_map: BiMap<String, mq_hir::SourceId>,
) -> Option<CompletionResponse> {
    match source_map.get_by_left(&url.to_string()) {
        Some(source_id) => {
            let scope_id = hir
                .read()
                .unwrap()
                .find_scope_in_position(
                    *source_id,
                    mq_lang::Position::new(position.line + 1, (position.character + 1) as usize),
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
                        mq_hir::SymbolKind::Function(params) => Some(CompletionItem {
                            label: symbol.value.clone().unwrap_or_default().to_string(),
                            kind: Some(CompletionItemKind::FUNCTION),
                            detail: Some(symbol.doc.iter().map(|(_, doc)| doc).join("\n")),
                            insert_text: Some(format!(
                                "{}({})",
                                symbol.value.clone().unwrap_or_default(),
                                params
                                    .iter()
                                    .enumerate()
                                    .map(|(i, name)| format!("${{{}:{}}}", i + 1, name))
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
                        mq_hir::SymbolKind::Parameter | mq_hir::SymbolKind::Variable => {
                            Some(CompletionItem {
                                label: symbol.value.clone().unwrap_or_default().to_string(),
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
                        mq_hir::SymbolKind::Selector => Some(CompletionItem {
                            label: symbol.value.clone().unwrap_or_default().to_string(),
                            kind: Some(CompletionItemKind::METHOD),
                            detail: Some(symbol.doc.iter().map(|(_, doc)| doc).join("\n")),
                            insert_text: Some(symbol.value.clone().unwrap_or_default().into()),
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
                    .collect::<Vec<_>>(),
            ))
        }
        None => None,
    }
}

#[cfg(test)]
mod tests {
    use mq_hir::Hir;

    use super::*;

    #[test]
    fn test_completion_response_none_for_unknown_url() {
        let hir = Arc::new(RwLock::new(Hir::default()));
        let source_map = BiMap::new();
        let url = Url::parse("file:///unknown.mql").unwrap();
        let position = Position::new(0, 0);

        let result = response(hir, url, position, source_map);
        assert!(result.is_none());
    }

    #[test]
    fn test_completion_response_returns_symbols() {
        let mut hir = Hir::default();
        let mut source_map = BiMap::new();
        let url = Url::parse("file:///unknown.mql").unwrap();
        let (source_id, _) = hir.add_code(Some(url.clone()), "def func1(): 1;");

        source_map.insert(url.to_string(), source_id);

        let result = response(
            Arc::new(RwLock::new(hir)),
            url,
            Position::new(0, 0),
            source_map,
        );
        assert!(result.is_some());

        if let Some(CompletionResponse::Array(items)) = result {
            assert!(items.iter().any(|item| item.label == "add"));
        }
    }
}
