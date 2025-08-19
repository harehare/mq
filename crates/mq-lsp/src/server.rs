use std::sync::{Arc, RwLock};

use bimap::BiMap;
use dashmap::DashMap;

use tower_lsp::jsonrpc::{ErrorCode, Result};
use tower_lsp::lsp_types::CompletionParams;
use tower_lsp::lsp_types::CompletionResponse;
use tower_lsp::lsp_types::Diagnostic;
use tower_lsp::lsp_types::DiagnosticSeverity;
use tower_lsp::lsp_types::DidChangeTextDocumentParams;
use tower_lsp::lsp_types::DidCloseTextDocumentParams;
use tower_lsp::lsp_types::DidOpenTextDocumentParams;
use tower_lsp::lsp_types::DidSaveTextDocumentParams;
use tower_lsp::lsp_types::DocumentFormattingParams;
use tower_lsp::lsp_types::DocumentSymbolParams;
use tower_lsp::lsp_types::DocumentSymbolResponse;
use tower_lsp::lsp_types::GotoDefinitionParams;
use tower_lsp::lsp_types::GotoDefinitionResponse;
use tower_lsp::lsp_types::Hover;
use tower_lsp::lsp_types::HoverParams;
use tower_lsp::lsp_types::InitializeParams;
use tower_lsp::lsp_types::InitializeResult;
use tower_lsp::lsp_types::Location;
use tower_lsp::lsp_types::MessageType;
use tower_lsp::lsp_types::Position;
use tower_lsp::lsp_types::Range;
use tower_lsp::lsp_types::ReferenceParams;
use tower_lsp::lsp_types::SemanticTokens;
use tower_lsp::lsp_types::SemanticTokensParams;
use tower_lsp::lsp_types::SemanticTokensRangeParams;
use tower_lsp::lsp_types::SemanticTokensRangeResult;
use tower_lsp::lsp_types::SemanticTokensResult;
use tower_lsp::lsp_types::TextEdit;
use tower_lsp::lsp_types::Url;
use tower_lsp::{Client, LanguageServer, LspService, Server};

use crate::{
    capabilities, completions, document_symbol, execute_command, goto_definition, hover,
    references, semantic_tokens,
};

#[derive(Debug)]
struct Backend {
    client: Client,
    hir: Arc<RwLock<mq_hir::Hir>>,
    source_map: RwLock<BiMap<String, mq_hir::SourceId>>,
    error_map: DashMap<String, Vec<(std::string::String, mq_lang::Range)>>,
    cst_nodes_map: DashMap<String, Vec<Arc<mq_lang::CstNode>>>,
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        self.client
            .log_message(MessageType::INFO, "Server initialized")
            .await;
        Ok(InitializeResult {
            capabilities: capabilities::server_capabilities(),
            ..Default::default()
        })
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.on_change(params.text_document.uri.clone(), params.text_document.text)
            .await;
        self.diagnostics(params.text_document.uri, Some(params.text_document.version))
            .await;
    }

    async fn did_change(&self, mut params: DidChangeTextDocumentParams) {
        self.on_change(
            params.text_document.uri,
            std::mem::take(&mut params.content_changes[0].text),
        )
        .await
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        self.diagnostics(params.text_document.uri, None).await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri_string = params.text_document.uri.to_string();

        // Remove error information for the closed file
        self.error_map.remove(&uri_string);
        self.cst_nodes_map.remove(&uri_string);

        // Remove from source map
        self.source_map.write().unwrap().remove_by_left(&uri_string);

        // Clear diagnostics for the closed file
        self.client
            .publish_diagnostics(params.text_document.uri, Vec::new(), None)
            .await;
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let url = params.text_document.uri;

        Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data: semantic_tokens::response(Arc::clone(&self.hir), url),
        })))
    }

    async fn semantic_tokens_range(
        &self,
        params: SemanticTokensRangeParams,
    ) -> Result<Option<SemanticTokensRangeResult>> {
        let url = params.text_document.uri;

        Ok(Some(SemanticTokensRangeResult::Tokens(SemanticTokens {
            result_id: None,
            data: semantic_tokens::response(Arc::clone(&self.hir), url),
        })))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let url = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        Ok(goto_definition::response(
            Arc::clone(&self.hir),
            url,
            position,
        ))
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let url = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        Ok(references::response(
            Arc::clone(&self.hir),
            url,
            position,
            self.source_map.read().unwrap().clone(),
        ))
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let url = params.text_document.uri;
        Ok(document_symbol::response(
            Arc::clone(&self.hir),
            url,
            self.source_map.read().unwrap().clone(),
        ))
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let url = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        Ok(hover::response(Arc::clone(&self.hir), url, position))
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        Ok(completions::response(
            Arc::clone(&self.hir),
            uri,
            position,
            self.source_map.read().unwrap().clone(),
        ))
    }

    async fn execute_command(
        &self,
        params: tower_lsp::lsp_types::ExecuteCommandParams,
    ) -> Result<Option<serde_json::Value>> {
        execute_command::response(params)
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        if !self
            .error_map
            .get(&params.text_document.uri.to_string())
            .unwrap()
            .is_empty()
        {
            return Ok(None);
        }

        let nodes = self
            .cst_nodes_map
            .get(&params.text_document.uri.to_string())
            .unwrap();
        let formatted_text =
            mq_formatter::Formatter::new(Some(mq_formatter::FormatterConfig { indent_width: 2 }))
                .format_with_cst(nodes.to_vec())
                .map_err(|_| tower_lsp::jsonrpc::Error::new(ErrorCode::ParseError))?;

        Ok(Some(vec![TextEdit {
            range: Range::new(
                Position::new(0, 0),
                Position::new(formatted_text.lines().count() as u32, u32::MAX),
            ),
            new_text: formatted_text,
        }]))
    }
}

impl Backend {
    async fn on_change(&self, uri: Url, text: String) {
        let (nodes, errors) = mq_lang::parse_recovery(&text);
        let (source_id, _) = self.hir.write().unwrap().add_nodes(uri.clone(), &nodes);

        self.source_map
            .write()
            .unwrap()
            .insert(uri.to_string(), source_id);
        self.cst_nodes_map.insert(uri.to_string(), nodes);
        self.error_map
            .insert(uri.to_string(), errors.error_ranges(&text));
    }

    async fn diagnostics(&self, uri: Url, version: Option<i32>) {
        let uri_string = uri.to_string();

        // Get errors for this specific file
        let file_errors = self.error_map.get(&uri_string);

        let mut diagnostics = Vec::new();

        // Add parsing errors if they exist
        if let Some(errors) = file_errors {
            diagnostics.extend(errors.iter().cloned().map(|(message, item)| {
                Diagnostic::new_simple(
                    Range::new(
                        Position {
                            line: item.start.line - 1,
                            character: (item.start.column - 1) as u32,
                        },
                        Position {
                            line: item.end.line - 1,
                            character: (item.end.column - 1) as u32,
                        },
                    ),
                    message,
                )
            }));
        }

        // Add HIR errors for this specific file
        if let Some(source_id) = self.source_map.read().unwrap().get_by_left(&uri_string) {
            // Filter HIR errors to only include ones from this specific source
            diagnostics.extend(
                self.hir
                    .read()
                    .unwrap()
                    .error_ranges()
                    .into_iter()
                    .filter_map(|(message, item)| {
                        // Only include errors if they are related to this file's source
                        // We'll check this by examining the source_id of symbols in HIR
                        let hir = self.hir.read().unwrap();
                        let has_error_in_this_file = hir.symbols().any(|(_, symbol)| {
                            symbol.source.source_id == Some(*source_id)
                                && symbol.source.text_range.as_ref() == Some(&item)
                        });

                        if has_error_in_this_file {
                            Some(Diagnostic::new_simple(
                                Range::new(
                                    Position {
                                        line: item.start.line - 1,
                                        character: (item.start.column - 1) as u32,
                                    },
                                    Position {
                                        line: item.end.line - 1,
                                        character: (item.end.column - 1) as u32,
                                    },
                                ),
                                message,
                            ))
                        } else {
                            None
                        }
                    }),
            );

            // Add unused function warnings
            let hir = self.hir.read().unwrap();
            let unused_functions = hir.unused_functions(*source_id);
            for (_, symbol) in unused_functions {
                if let Some(text_range) = &symbol.source.text_range {
                    let mut diagnostic = Diagnostic::new_simple(
                        Range::new(
                            Position {
                                line: text_range.start.line - 1,
                                character: (text_range.start.column - 1) as u32,
                            },
                            Position {
                                line: text_range.end.line - 1,
                                character: (text_range.end.column - 1) as u32,
                            },
                        ),
                        format!(
                            "Function '{}' is defined but never used",
                            symbol.value.as_ref().unwrap_or(&"<anonymous>".into())
                        ),
                    );
                    diagnostic.severity = Some(DiagnosticSeverity::WARNING);
                    diagnostics.push(diagnostic);
                }
            }
        }

        self.client
            .publish_diagnostics(uri, diagnostics, version)
            .await;
    }
}

pub async fn start() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(|client| Backend {
        client,
        hir: Arc::new(RwLock::new(mq_hir::Hir::new())),
        source_map: RwLock::new(BiMap::new()),
        error_map: DashMap::new(),
        cst_nodes_map: DashMap::new(),
    });

    Server::new(stdin, stdout, socket).serve(service).await;
}

#[cfg(test)]
mod tests {
    use tower_lsp::lsp_types::{SemanticToken, TextDocumentItem};

    use super::*;

    #[tokio::test]
    async fn test_did_open() {
        let (service, _) = LspService::new(|client| Backend {
            client,
            hir: Arc::new(RwLock::new(mq_hir::Hir::new())),
            source_map: RwLock::new(BiMap::new()),
            error_map: DashMap::new(),
            cst_nodes_map: DashMap::new(),
        });

        let backend = service.inner();
        let uri = Url::parse("file:///test.mq").unwrap();

        backend
            .did_open(DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri: uri.clone(),
                    language_id: "mq".to_string(),
                    version: 1,
                    text: "def main(): 1;".to_string(),
                },
            })
            .await;

        assert!(
            backend
                .source_map
                .read()
                .unwrap()
                .contains_left(&uri.to_string())
        );
        assert!(
            backend
                .hir
                .read()
                .unwrap()
                .symbols()
                .map(|(_, s)| {
                    s.value
                        .as_ref()
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "".into())
                })
                .collect::<Vec<_>>()
                .contains(&"main".into()),
        );
        assert!(backend.error_map.get(&uri.to_string()).unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_formatting() {
        let (service, _) = LspService::new(|client| Backend {
            client,
            hir: Arc::new(RwLock::new(mq_hir::Hir::new())),
            source_map: RwLock::new(BiMap::new()),
            error_map: DashMap::new(),
            cst_nodes_map: DashMap::new(),
        });

        let backend = service.inner();
        let uri = Url::parse("file:///test.mq").unwrap();
        let text = "def main():1;";

        let (nodes, errors) = mq_lang::parse_recovery(text);
        backend.cst_nodes_map.insert(uri.to_string(), nodes);
        backend
            .error_map
            .insert(uri.to_string(), errors.error_ranges(text));

        let result = backend
            .formatting(DocumentFormattingParams {
                text_document: tower_lsp::lsp_types::TextDocumentIdentifier { uri: uri.clone() },
                options: tower_lsp::lsp_types::FormattingOptions {
                    tab_size: 2,
                    insert_spaces: true,
                    ..Default::default()
                },
                work_done_progress_params: Default::default(),
            })
            .await;

        assert!(result.is_ok());
        if let Ok(Some(edits)) = result {
            assert_eq!(edits.len(), 1);
            assert_eq!(edits[0].new_text, "def main(): 1;");
        }
    }
    #[tokio::test]
    async fn test_completion() {
        let (service, _) = LspService::new(|client| Backend {
            client,
            hir: Arc::new(RwLock::new(mq_hir::Hir::new())),
            source_map: RwLock::new(BiMap::new()),
            error_map: DashMap::new(),
            cst_nodes_map: DashMap::new(),
        });

        let backend = service.inner();
        let uri = Url::parse("file:///test.mq").unwrap();

        let result = backend
            .completion(CompletionParams {
                text_document_position: tower_lsp::lsp_types::TextDocumentPositionParams {
                    text_document: tower_lsp::lsp_types::TextDocumentIdentifier { uri },
                    position: Position::new(0, 0),
                },
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
                context: None,
            })
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_goto_definition() {
        let (service, _) = LspService::new(|client| Backend {
            client,
            hir: Arc::new(RwLock::new(mq_hir::Hir::new())),
            source_map: RwLock::new(BiMap::new()),
            error_map: DashMap::new(),
            cst_nodes_map: DashMap::new(),
        });

        let backend = service.inner();
        let uri = Url::parse("file:///test.mq").unwrap();

        let result = backend
            .goto_definition(GotoDefinitionParams {
                text_document_position_params: tower_lsp::lsp_types::TextDocumentPositionParams {
                    text_document: tower_lsp::lsp_types::TextDocumentIdentifier { uri },
                    position: Position::new(0, 0),
                },
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            })
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_hover() {
        let (service, _) = LspService::new(|client| Backend {
            client,
            hir: Arc::new(RwLock::new(mq_hir::Hir::new())),
            source_map: RwLock::new(BiMap::new()),
            error_map: DashMap::new(),
            cst_nodes_map: DashMap::new(),
        });

        let backend = service.inner();
        let uri = Url::parse("file:///test.mq").unwrap();

        let result = backend
            .hover(HoverParams {
                text_document_position_params: tower_lsp::lsp_types::TextDocumentPositionParams {
                    text_document: tower_lsp::lsp_types::TextDocumentIdentifier { uri },
                    position: Position::new(0, 0),
                },
                work_done_progress_params: Default::default(),
            })
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_semantic_tokens() {
        let (service, _) = LspService::new(|client| Backend {
            client,
            hir: Arc::new(RwLock::new(mq_hir::Hir::new())),
            source_map: RwLock::new(BiMap::new()),
            error_map: DashMap::new(),
            cst_nodes_map: DashMap::new(),
        });

        let backend = service.inner();
        let uri = Url::parse("file:///test.mq").unwrap();

        backend
            .hir
            .write()
            .unwrap()
            .add_code(Some(uri.clone()), "def main(): 1;");

        let result = backend
            .semantic_tokens_full(SemanticTokensParams {
                text_document: tower_lsp::lsp_types::TextDocumentIdentifier { uri },
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            })
            .await;

        assert_eq!(
            result.unwrap().unwrap(),
            SemanticTokensResult::Tokens(SemanticTokens {
                result_id: None,
                data: vec![
                    SemanticToken {
                        delta_line: 0,
                        delta_start: 0,
                        length: 3,
                        token_type: 2,
                        token_modifiers_bitset: 0
                    },
                    SemanticToken {
                        delta_line: 0,
                        delta_start: 4,
                        length: 4,
                        token_type: 1,
                        token_modifiers_bitset: 0
                    },
                    SemanticToken {
                        delta_line: 0,
                        delta_start: 8,
                        length: 1,
                        token_type: 5,
                        token_modifiers_bitset: 0
                    }
                ]
            })
        );
    }

    #[tokio::test]
    async fn test_references() {
        let (service, _) = LspService::new(|client| Backend {
            client,
            hir: Arc::new(RwLock::new(mq_hir::Hir::new())),
            source_map: RwLock::new(BiMap::new()),
            error_map: DashMap::new(),
            cst_nodes_map: DashMap::new(),
        });

        let backend = service.inner();
        let uri = Url::parse("file:///test.mq").unwrap();

        let code = "def test_func(): 1;\ndef main(): test_func();";
        let (nodes, _) = mq_lang::parse_recovery(code);
        let (source_id, _) = backend.hir.write().unwrap().add_nodes(uri.clone(), &nodes);

        backend
            .source_map
            .write()
            .unwrap()
            .insert(uri.to_string(), source_id);

        let result = backend
            .references(ReferenceParams {
                text_document_position: tower_lsp::lsp_types::TextDocumentPositionParams {
                    text_document: tower_lsp::lsp_types::TextDocumentIdentifier {
                        uri: uri.clone(),
                    },
                    position: Position::new(0, 6),
                },
                work_done_progress_params: Default::default(),
                context: tower_lsp::lsp_types::ReferenceContext {
                    include_declaration: true,
                },
                partial_result_params: Default::default(),
            })
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_document_symbol() {
        let (service, _) = LspService::new(|client| Backend {
            client,
            hir: Arc::new(RwLock::new(mq_hir::Hir::new())),
            source_map: RwLock::new(BiMap::new()),
            error_map: DashMap::new(),
            cst_nodes_map: DashMap::new(),
        });

        let backend = service.inner();
        let uri = Url::parse("file:///test.mq").unwrap();

        let code = "def test_func(): 1;\ndef main(): test_func();";
        let (nodes, _) = mq_lang::parse_recovery(code);
        let (source_id, _) = backend.hir.write().unwrap().add_nodes(uri.clone(), &nodes);

        backend
            .source_map
            .write()
            .unwrap()
            .insert(uri.to_string(), source_id);

        let result = backend
            .document_symbol(DocumentSymbolParams {
                text_document: tower_lsp::lsp_types::TextDocumentIdentifier { uri },
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            })
            .await;

        assert!(result.is_ok());

        let symbols = result.unwrap().unwrap();

        if let DocumentSymbolResponse::Nested(symbols) = symbols {
            let symbol_names: Vec<String> = symbols.iter().map(|s| s.name.clone()).collect();

            assert!(symbol_names.contains(&"test_func".to_string()));
            assert!(symbol_names.contains(&"main".to_string()));
            assert_eq!(symbols.len(), 2);
        } else {
            panic!("Expected flat symbol response");
        }
    }

    #[tokio::test]
    async fn test_did_change() {
        let (service, _) = LspService::new(|client| Backend {
            client,
            hir: Arc::new(RwLock::new(mq_hir::Hir::new())),
            source_map: RwLock::new(BiMap::new()),
            error_map: DashMap::new(),
            cst_nodes_map: DashMap::new(),
        });

        let backend = service.inner();
        let uri = Url::parse("file:///test.mq").unwrap();

        backend
            .did_open(DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri: uri.clone(),
                    language_id: "mq".to_string(),
                    version: 1,
                    text: "def main(): 1;".to_string(),
                },
            })
            .await;

        backend
            .did_change(DidChangeTextDocumentParams {
                text_document: tower_lsp::lsp_types::VersionedTextDocumentIdentifier {
                    uri: uri.clone(),
                    version: 2,
                },
                content_changes: vec![tower_lsp::lsp_types::TextDocumentContentChangeEvent {
                    range: None,
                    range_length: None,
                    text: "def main(): 2;".to_string(),
                }],
            })
            .await;

        // Check if content was updated
        let nodes = backend.cst_nodes_map.get(&uri.to_string()).unwrap();
        assert!(!nodes.is_empty());
    }

    #[tokio::test]
    async fn test_did_close() {
        let (service, _) = LspService::new(|client| Backend {
            client,
            hir: Arc::new(RwLock::new(mq_hir::Hir::new())),
            source_map: RwLock::new(BiMap::new()),
            error_map: DashMap::new(),
            cst_nodes_map: DashMap::new(),
        });

        let backend = service.inner();
        let uri = Url::parse("file:///test.mq").unwrap();

        // Open and setup a file with content
        backend
            .did_open(DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri: uri.clone(),
                    language_id: "mq".to_string(),
                    version: 1,
                    text: "def main(): invalid_syntax".to_string(),
                },
            })
            .await;

        // Verify data exists before close
        assert!(backend.error_map.contains_key(&uri.to_string()));
        assert!(backend.cst_nodes_map.contains_key(&uri.to_string()));
        assert!(
            backend
                .source_map
                .read()
                .unwrap()
                .contains_left(&uri.to_string())
        );

        // Close the file
        use tower_lsp::lsp_types::DidCloseTextDocumentParams;
        use tower_lsp::lsp_types::TextDocumentIdentifier;
        backend
            .did_close(DidCloseTextDocumentParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
            })
            .await;

        // Verify data is cleaned up after close
        assert!(!backend.error_map.contains_key(&uri.to_string()));
        assert!(!backend.cst_nodes_map.contains_key(&uri.to_string()));
        assert!(
            !backend
                .source_map
                .read()
                .unwrap()
                .contains_left(&uri.to_string())
        );
    }

    #[tokio::test]
    async fn test_did_save() {
        let (service, _) = LspService::new(|client| Backend {
            client,
            hir: Arc::new(RwLock::new(mq_hir::Hir::new())),
            source_map: RwLock::new(BiMap::new()),
            error_map: DashMap::new(),
            cst_nodes_map: DashMap::new(),
        });

        let backend = service.inner();
        let uri = Url::parse("file:///test.mq").unwrap();

        // Setup some content with errors
        let text = "def main(): invalid_syntax";
        let (nodes, errors) = mq_lang::parse_recovery(text);
        backend.cst_nodes_map.insert(uri.to_string(), nodes);
        backend
            .error_map
            .insert(uri.to_string(), errors.error_ranges(text));

        backend
            .source_map
            .write()
            .unwrap()
            .insert(uri.to_string(), mq_hir::SourceId::default());

        // Trigger save
        backend
            .did_save(DidSaveTextDocumentParams {
                text_document: tower_lsp::lsp_types::TextDocumentIdentifier { uri },
                text: None,
            })
            .await;

        // There's no direct way to verify diagnostics were published
        // since that involves the client, but we can verify no errors occurred
    }

    #[tokio::test]
    async fn test_initialize_shutdown() {
        let (service, _) = LspService::new(|client| Backend {
            client,
            hir: Arc::new(RwLock::new(mq_hir::Hir::new())),
            source_map: RwLock::new(BiMap::new()),
            error_map: DashMap::new(),
            cst_nodes_map: DashMap::new(),
        });

        let backend = service.inner();

        // Test initialize
        let init_result = backend.initialize(InitializeParams::default()).await;

        assert!(init_result.is_ok());
        let capabilities = init_result.unwrap().capabilities;
        assert!(capabilities.text_document_sync.is_some());
        assert!(capabilities.hover_provider.is_some());
        assert!(capabilities.completion_provider.is_some());

        // Test shutdown
        let shutdown_result = backend.shutdown().await;
        assert!(shutdown_result.is_ok());
    }

    #[tokio::test]
    async fn test_semantic_tokens_range() {
        let (service, _) = LspService::new(|client| Backend {
            client,
            hir: Arc::new(RwLock::new(mq_hir::Hir::new())),
            source_map: RwLock::new(BiMap::new()),
            error_map: DashMap::new(),
            cst_nodes_map: DashMap::new(),
        });

        let backend = service.inner();
        let uri = Url::parse("file:///test.mq").unwrap();

        backend
            .hir
            .write()
            .unwrap()
            .add_code(Some(uri.clone()), "def main(): 1;");

        let result = backend
            .semantic_tokens_range(SemanticTokensRangeParams {
                text_document: tower_lsp::lsp_types::TextDocumentIdentifier { uri },
                range: Range {
                    start: Position {
                        line: 0,
                        character: 0,
                    },
                    end: Position {
                        line: 0,
                        character: 12,
                    },
                },
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            })
            .await;

        assert!(result.is_ok());
        // Same expectations as full tokens since our implementation doesn't filter by range
    }

    #[tokio::test]
    async fn test_diagnostics_with_errors() {
        let (service, _) = LspService::new(|client| Backend {
            client,
            hir: Arc::new(RwLock::new(mq_hir::Hir::new())),
            source_map: RwLock::new(BiMap::new()),
            error_map: DashMap::new(),
            cst_nodes_map: DashMap::new(),
        });

        let backend = service.inner();
        let uri = Url::parse("file:///test.mq").unwrap();

        // Add content with parsing errors
        let text = "def main() 1;"; // Missing colon
        let (nodes, errors) = mq_lang::parse_recovery(text);
        backend.cst_nodes_map.insert(uri.to_string(), nodes);
        backend
            .error_map
            .insert(uri.to_string(), errors.error_ranges(text));

        // We can't directly test client.publish_diagnostics was called with correct diagnostics,
        // but we can verify the code doesn't panic
        backend.diagnostics(uri, None).await;
    }

    #[tokio::test]
    async fn test_unused_function_warnings() {
        let (service, _) = LspService::new(|client| Backend {
            client,
            hir: Arc::new(RwLock::new(mq_hir::Hir::new())),
            source_map: RwLock::new(BiMap::new()),
            error_map: DashMap::new(),
            cst_nodes_map: DashMap::new(),
        });

        let backend = service.inner();
        let uri = Url::parse("file:///test.mq").unwrap();

        // Code with unused function
        let code = "def used_function(): 1; def unused_function(): 2; | used_function()";
        let (nodes, errors) = mq_lang::parse_recovery(code);
        let (source_id, _) = backend.hir.write().unwrap().add_nodes(uri.clone(), &nodes);

        backend
            .source_map
            .write()
            .unwrap()
            .insert(uri.to_string(), source_id);
        backend.cst_nodes_map.insert(uri.to_string(), nodes);
        backend
            .error_map
            .insert(uri.to_string(), errors.error_ranges(code));

        // Check unused functions are detected
        {
            let hir_lock = backend.hir.read().unwrap();
            let unused = hir_lock.unused_functions(source_id);
            assert_eq!(unused.len(), 1);
            assert_eq!(unused[0].1.value.as_ref().unwrap(), "unused_function");
            drop(hir_lock);
        }

        // Diagnostics should be published (we can't directly test this without mocking the client)
        backend.diagnostics(uri, None).await;
    }

    #[tokio::test]
    async fn test_formatting_with_errors() {
        let (service, _) = LspService::new(|client| Backend {
            client,
            hir: Arc::new(RwLock::new(mq_hir::Hir::new())),
            source_map: RwLock::new(BiMap::new()),
            error_map: DashMap::new(),
            cst_nodes_map: DashMap::new(),
        });

        let backend = service.inner();
        let uri = Url::parse("file:///test.mq").unwrap();

        let text = "def main() 1;";
        let (nodes, _) = mq_lang::parse_recovery(text);
        backend.cst_nodes_map.insert(uri.to_string(), nodes);
        backend.error_map.insert(
            uri.to_string(),
            vec![(
                "Syntax error".to_string(),
                mq_lang::Range {
                    start: mq_lang::Position {
                        line: 1,
                        column: 10,
                    },
                    end: mq_lang::Position {
                        line: 1,
                        column: 11,
                    },
                },
            )],
        );

        let result = backend
            .formatting(DocumentFormattingParams {
                text_document: tower_lsp::lsp_types::TextDocumentIdentifier { uri: uri.clone() },
                options: tower_lsp::lsp_types::FormattingOptions {
                    tab_size: 2,
                    insert_spaces: true,
                    ..Default::default()
                },
                work_done_progress_params: Default::default(),
            })
            .await;

        assert_eq!(result.unwrap(), None);
    }
}
