use std::sync::{Arc, RwLock};

use bimap::BiMap;
use itertools::Itertools;
use tower_lsp_server::ls_types::{
    CompletionItem, CompletionItemKind, CompletionResponse, Documentation, InsertTextFormat, MarkupContent, MarkupKind,
    Position,
};
use url::Url;

pub fn response(
    hir: Arc<RwLock<mq_hir::Hir>>,
    url: Url,
    position: Position,
    source_map: &BiMap<String, mq_hir::SourceId>,
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

            let module_completion = if position.character >= 3 {
                let before_pos =
                    mq_lang::Position::new(position.line + 1, (position.character.saturating_sub(3)) as usize);

                hir.read()
                    .unwrap()
                    .find_symbol_in_position(*source_id, before_pos)
                    .and_then(|(_, symbol)| {
                        if matches!(symbol.kind, mq_hir::SymbolKind::QualifiedAccess) {
                            let module_name = symbol.value.as_ref()?;

                            let hir_guard = hir.read().unwrap();
                            for (_, mod_symbol) in hir_guard.symbols() {
                                if mod_symbol.is_module()
                                    && mod_symbol.value.as_ref() == Some(module_name)
                                    && let mq_hir::SymbolKind::Module(module_source_id) = mod_symbol.kind
                                {
                                    return Some(hir_guard.find_symbols_in_module(module_source_id));
                                } else if mod_symbol.is_import()
                                    && mod_symbol.value.as_ref() == Some(module_name)
                                    && let mq_hir::SymbolKind::Import(module_source_id) = mod_symbol.kind
                                {
                                    return Some(hir_guard.find_symbols_in_module(module_source_id));
                                }
                            }
                            None
                        } else if symbol.is_module() {
                            // Direct module reference
                            if let mq_hir::SymbolKind::Module(module_source_id) = symbol.kind {
                                Some(hir.read().unwrap().find_symbols_in_module(module_source_id))
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    })
            } else {
                None
            };

            let symbols = if let Some(module_symbols) = module_completion {
                module_symbols
            } else {
                let hir_guard = hir.read().unwrap();

                itertools::concat(vec![
                    hir_guard.find_symbols_in_scope(scope_id),
                    hir_guard.find_symbols_in_source(hir_guard.builtin.source_id),
                ])
                .into_iter()
                .unique_by(|s| s.value.clone())
                .collect::<Vec<_>>()
            };

            Some(CompletionResponse::Array(
                symbols
                    .iter()
                    .filter_map(|symbol| match &symbol.kind {
                        mq_hir::SymbolKind::Function(params) | mq_hir::SymbolKind::Macro(params) => {
                            let deprecated = symbol.is_deprecated();
                            Some(CompletionItem {
                                label: symbol.value.clone().unwrap_or_default().to_string(),
                                kind: Some(CompletionItemKind::FUNCTION),
                                detail: Some(symbol.doc.iter().map(|(_, doc)| doc).join("\n")),
                                insert_text: Some(format!(
                                    "{}({})",
                                    symbol.value.clone().unwrap_or_default(),
                                    params
                                        .iter()
                                        .enumerate()
                                        .map(|(i, p)| format!("${{{}:{}}}", i + 1, p))
                                        .join(", ")
                                )),
                                insert_text_format: Some(InsertTextFormat::SNIPPET),
                                documentation: Some(Documentation::MarkupContent(MarkupContent {
                                    kind: MarkupKind::Markdown,
                                    value: format!("```md\n{}\n```", symbol.doc.iter().map(|(_, doc)| doc).join("\n")),
                                })),
                                deprecated: Some(deprecated),
                                ..Default::default()
                            })
                        }
                        mq_hir::SymbolKind::Parameter | mq_hir::SymbolKind::Variable => {
                            let deprecated = symbol.is_deprecated();
                            Some(CompletionItem {
                                label: symbol.value.clone().unwrap_or_default().to_string(),
                                kind: Some(CompletionItemKind::VARIABLE),
                                detail: Some(symbol.doc.iter().map(|(_, doc)| doc).join("\n")),
                                documentation: Some(Documentation::MarkupContent(MarkupContent {
                                    kind: MarkupKind::Markdown,
                                    value: format!("```md\n{}\n```", symbol.doc.iter().map(|(_, doc)| doc).join("\n")),
                                })),
                                deprecated: Some(deprecated),
                                ..Default::default()
                            })
                        }
                        mq_hir::SymbolKind::Selector => {
                            let deprecated = symbol.is_deprecated();
                            Some(CompletionItem {
                                label: symbol.value.clone().unwrap_or_default().to_string(),
                                kind: Some(CompletionItemKind::METHOD),
                                detail: Some(symbol.doc.iter().map(|(_, doc)| doc).join("\n")),
                                insert_text: Some(symbol.value.clone().unwrap_or_default().into()),
                                insert_text_format: Some(InsertTextFormat::SNIPPET),
                                documentation: Some(Documentation::MarkupContent(MarkupContent {
                                    kind: MarkupKind::Markdown,
                                    value: format!("```md\n{}\n```", symbol.doc.iter().map(|(_, doc)| doc).join("\n")),
                                })),
                                deprecated: Some(deprecated),
                                ..Default::default()
                            })
                        }
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

        let result = response(hir, url, position, &source_map);
        assert!(result.is_none());
    }

    #[test]
    fn test_completion_response_returns_symbols() {
        let mut hir = Hir::default();
        let mut source_map = BiMap::new();
        let url = Url::parse("file:///unknown.mql").unwrap();
        let (source_id, _) = hir.add_code(Some(url.clone()), "def func1(): 1;");

        source_map.insert(url.to_string(), source_id);

        let result = response(Arc::new(RwLock::new(hir)), url, Position::new(0, 0), &source_map);
        assert!(result.is_some());

        if let Some(CompletionResponse::Array(items)) = result {
            assert!(items.iter().any(|item| item.label == "add"));
        }
    }

    #[test]
    fn test_completion_response_returns_symbols_in_module() {
        let mut hir = Hir::default();
        let mut source_map = BiMap::new();
        let url = Url::parse("file:///unknown.mql").unwrap();
        let (source_id, _) = hir.add_code(Some(url.clone()), "module mod1: def func1(): 1; end");

        source_map.insert(url.to_string(), source_id);

        let result = response(Arc::new(RwLock::new(hir)), url, Position::new(0, 0), &source_map);
        assert!(result.is_some());

        if let Some(CompletionResponse::Array(items)) = result {
            assert!(items.iter().any(|item| item.label == "func1"));
        }
    }

    #[test]
    fn test_completion_qualified_access() {
        let mut hir = Hir::default();
        let mut source_map = BiMap::new();
        let url = Url::parse("file:///test.mql").unwrap();

        // Create a module with functions
        let code = "module math: def add(a, b): a + b; def sub(a, b): a - b; end | math::";
        let (source_id, _) = hir.add_code(Some(url.clone()), code);

        source_map.insert(url.to_string(), source_id);

        let result = response(
            Arc::new(RwLock::new(hir)),
            url,
            Position::new(0, 70), // Position right after "math::", before "a"
            &source_map,
        );

        assert!(result.is_some());

        if let Some(CompletionResponse::Array(items)) = result {
            // Should include functions from the math module
            assert!(
                items.iter().any(|item| item.label == "add"),
                "Should include 'add' function"
            );
            assert!(
                items.iter().any(|item| item.label == "sub"),
                "Should include 'sub' function"
            );

            // Should NOT include builtin functions when in qualified access mode
            assert!(
                !items.iter().any(|item| item.label == "map"),
                "Should not include builtin 'map' function"
            );
        }
    }

    #[test]
    fn test_completion_qualified_access_with_nested_module() {
        let mut hir = Hir::default();
        let mut source_map = BiMap::new();
        let url = Url::parse("file:///test.mql").unwrap();

        // Create a module with functions
        let code = "module utils: def helper(): 1; end | utils::";
        let (source_id, _) = hir.add_code(Some(url.clone()), code);

        source_map.insert(url.to_string(), source_id);

        let result = response(
            Arc::new(RwLock::new(hir)),
            url,
            Position::new(0, 46), // Position right after "utils::"
            &source_map,
        );

        assert!(result.is_some());

        if let Some(CompletionResponse::Array(items)) = result {
            assert!(
                items.iter().any(|item| item.label == "helper"),
                "Should include 'helper' function from utils module"
            );
        }
    }

    #[test]
    fn test_completion_with_deprecated_function() {
        let mut hir = Hir::default();
        let mut source_map = BiMap::new();
        let url = Url::parse("file:///test.mql").unwrap();

        // Create a function with deprecated marker in doc
        let code = r#"# deprecated: This function is no longer supported
def old_func(x): x + 1;"#;
        let (source_id, _) = hir.add_code(Some(url.clone()), code);

        source_map.insert(url.to_string(), source_id);

        let result = response(Arc::new(RwLock::new(hir)), url, Position::new(1, 0), &source_map);

        assert!(result.is_some());

        if let Some(CompletionResponse::Array(items)) = result {
            let old_func_item = items.iter().find(|item| item.label == "old_func");
            assert!(old_func_item.is_some(), "Should include 'old_func'");

            if let Some(item) = old_func_item {
                assert_eq!(item.deprecated, Some(true), "old_func should be marked as deprecated");
            }
        }
    }
}
