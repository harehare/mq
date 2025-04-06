use std::sync::{Arc, RwLock};

use bimap::BiMap;
use dashmap::DashMap;
use mq_lsp::capabilities;
use mq_lsp::completions;
use mq_lsp::document_symbol;
use mq_lsp::execute_command;
use mq_lsp::goto_definition;
use mq_lsp::hover;
use mq_lsp::references;
use mq_lsp::semantic_tokens;
use tower_lsp::jsonrpc::{ErrorCode, Result};
use tower_lsp::lsp_types::CompletionParams;
use tower_lsp::lsp_types::CompletionResponse;
use tower_lsp::lsp_types::Diagnostic;
use tower_lsp::lsp_types::DidChangeTextDocumentParams;
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

#[derive(Debug)]
struct Backend {
    client: Client,
    hir: Arc<RwLock<mq_hir::Hir>>,
    source_map: RwLock<BiMap<String, mq_hir::SourceId>>,
    error_map: DashMap<String, Vec<(std::string::String, mq_lang::Range)>>,
    cst_nodes_map: DashMap<String, Vec<Arc<mq_lang::CstNode>>>,
    input: RwLock<String>,
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
        match params.command.as_str() {
            "mq/setSelectedTextAsInput" => Ok(params.arguments[0].as_str().map(|text| {
                self.input.write().unwrap().push_str(text);
                format!("Set mq input:\n{}", text).into()
            })),
            "mq/showInputText" => Ok(Some(
                format!("mq input:\n{}", self.input.read().unwrap()).into(),
            )),
            _ => Ok(
                execute_command::response(self.input.read().unwrap().clone(), params)
                    .map(serde_json::Value::String),
            ),
        }
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
        self.client
            .publish_diagnostics(uri.clone(), Vec::new(), version)
            .await;

        let errors = self.error_map.get(&uri.to_string()).unwrap();

        let mut diagnostics = errors
            .iter()
            .cloned()
            .map(|(message, item)| {
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
            })
            .collect::<Vec<_>>();

        diagnostics.extend(
            self.hir
                .read()
                .unwrap()
                .error_ranges()
                .into_iter()
                .map(|(message, item)| {
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
                })
                .collect::<Vec<_>>(),
        );

        self.client
            .publish_diagnostics(uri, diagnostics, version)
            .await;
    }
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(|client| Backend {
        client,
        hir: Arc::new(RwLock::new(mq_hir::Hir::new())),
        source_map: RwLock::new(BiMap::new()),
        error_map: DashMap::new(),
        cst_nodes_map: DashMap::new(),
        input: RwLock::new(String::new()),
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
            input: RwLock::new(String::new()),
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
                .map(|(_, s)| s.value.clone().unwrap().to_owned())
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
            input: RwLock::new(String::new()),
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
            input: RwLock::new(String::new()),
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
            input: RwLock::new(String::new()),
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
            input: RwLock::new(String::new()),
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
            input: RwLock::new(String::new()),
        });

        let backend = service.inner();
        let uri = Url::parse("file:///test.mq").unwrap();

        backend
            .hir
            .write()
            .unwrap()
            .add_code(uri.clone(), "def main(): 1;");

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
                        token_type: 0,
                        token_modifiers_bitset: 0
                    },
                    SemanticToken {
                        delta_line: 0,
                        delta_start: 4,
                        length: 4,
                        token_type: 4,
                        token_modifiers_bitset: 0
                    },
                    SemanticToken {
                        delta_line: 0,
                        delta_start: 8,
                        length: 1,
                        token_type: 2,
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
            input: RwLock::new(String::new()),
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
            input: RwLock::new(String::new()),
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
    async fn test_execute_command() {
        let (service, _) = LspService::new(|client| Backend {
            client,
            hir: Arc::new(RwLock::new(mq_hir::Hir::new())),
            source_map: RwLock::new(BiMap::new()),
            error_map: DashMap::new(),
            cst_nodes_map: DashMap::new(),
            input: RwLock::new(String::new()),
        });

        let backend = service.inner();

        let set_input_result = backend
            .execute_command(tower_lsp::lsp_types::ExecuteCommandParams {
                command: "mq/setSelectedTextAsInput".to_string(),
                arguments: vec![serde_json::Value::String("test input".to_string())],
                work_done_progress_params: Default::default(),
            })
            .await;

        assert!(set_input_result.is_ok());
        if let Ok(Some(result)) = set_input_result {
            assert_eq!(
                result,
                serde_json::Value::String("Set mq input:\ntest input".to_string())
            );
        }
        assert_eq!(*backend.input.read().unwrap(), "test input");

        let show_input_result = backend
            .execute_command(tower_lsp::lsp_types::ExecuteCommandParams {
                command: "mq/showInputText".to_string(),
                arguments: Vec::new(),
                work_done_progress_params: Default::default(),
            })
            .await;

        assert!(show_input_result.is_ok());
        if let Ok(Some(result)) = show_input_result {
            assert_eq!(
                result,
                serde_json::Value::String("mq input:\ntest input".to_string())
            );
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
            input: RwLock::new(String::new()),
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
    async fn test_did_save() {
        let (service, _) = LspService::new(|client| Backend {
            client,
            hir: Arc::new(RwLock::new(mq_hir::Hir::new())),
            source_map: RwLock::new(BiMap::new()),
            error_map: DashMap::new(),
            cst_nodes_map: DashMap::new(),
            input: RwLock::new(String::new()),
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
            input: RwLock::new(String::new()),
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
            input: RwLock::new(String::new()),
        });

        let backend = service.inner();
        let uri = Url::parse("file:///test.mq").unwrap();

        backend
            .hir
            .write()
            .unwrap()
            .add_code(uri.clone(), "def main(): 1;");

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
            input: RwLock::new(String::new()),
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
    async fn test_formatting_with_errors() {
        let (service, _) = LspService::new(|client| Backend {
            client,
            hir: Arc::new(RwLock::new(mq_hir::Hir::new())),
            source_map: RwLock::new(BiMap::new()),
            error_map: DashMap::new(),
            cst_nodes_map: DashMap::new(),
            input: RwLock::new(String::new()),
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
