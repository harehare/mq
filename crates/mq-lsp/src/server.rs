use std::path::PathBuf;
use std::str::FromStr;
use std::sync::{Arc, RwLock};

use bimap::BiMap;
use dashmap::DashMap;
use url::Url;

use crate::{
    capabilities, completions, document_symbol, execute_command, goto_definition, hover, references, semantic_tokens,
};
use tower_lsp_server::{Client, LanguageServer, LspService, Server, jsonrpc, ls_types};

fn to_url(uri: &ls_types::Uri) -> Url {
    Url::parse(&uri.to_string()).unwrap()
}

fn to_uri(uri: &Url) -> ls_types::Uri {
    ls_types::Uri::from_str(uri.as_ref()).unwrap()
}

#[derive(Debug)]
struct Backend {
    client: Client,
    hir: Arc<RwLock<mq_hir::Hir>>,
    source_map: RwLock<BiMap<String, mq_hir::SourceId>>,
    error_map: DashMap<String, Vec<(std::string::String, mq_lang::Range)>>,
    text_map: DashMap<String, Arc<String>>,
}

impl LanguageServer for Backend {
    async fn initialize(&self, _: ls_types::InitializeParams) -> jsonrpc::Result<ls_types::InitializeResult> {
        self.client
            .log_message(ls_types::MessageType::INFO, "Server initialized")
            .await;
        Ok(ls_types::InitializeResult {
            capabilities: capabilities::server_capabilities(),
            ..Default::default()
        })
    }

    async fn shutdown(&self) -> jsonrpc::Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: ls_types::DidOpenTextDocumentParams) {
        self.on_change(to_url(&params.text_document.uri), params.text_document.text)
            .await;
        self.diagnostics(to_url(&params.text_document.uri), Some(params.text_document.version))
            .await;
    }

    async fn did_change(&self, mut params: ls_types::DidChangeTextDocumentParams) {
        self.on_change(
            to_url(&params.text_document.uri),
            std::mem::take(&mut params.content_changes[0].text),
        )
        .await
    }

    async fn did_save(&self, params: ls_types::DidSaveTextDocumentParams) {
        self.diagnostics(to_url(&params.text_document.uri), None).await;
    }

    async fn did_close(&self, params: ls_types::DidCloseTextDocumentParams) {
        let uri_string = params.text_document.uri.to_string();

        // Remove error information for the closed file
        self.error_map.remove(&uri_string);
        self.text_map.remove(&uri_string);

        // Remove from source map
        self.source_map.write().unwrap().remove_by_left(&uri_string);

        // Clear diagnostics for the closed file
        self.client
            .publish_diagnostics(params.text_document.uri, Vec::new(), None)
            .await;
    }

    async fn semantic_tokens_full(
        &self,
        params: ls_types::SemanticTokensParams,
    ) -> jsonrpc::Result<Option<ls_types::SemanticTokensResult>> {
        let uri = params.text_document.uri;

        Ok(Some(ls_types::SemanticTokensResult::Tokens(ls_types::SemanticTokens {
            result_id: None,
            data: semantic_tokens::response(Arc::clone(&self.hir), to_url(&uri)),
        })))
    }

    async fn semantic_tokens_range(
        &self,
        params: ls_types::SemanticTokensRangeParams,
    ) -> jsonrpc::Result<Option<ls_types::SemanticTokensRangeResult>> {
        let url = params.text_document.uri;

        Ok(Some(ls_types::SemanticTokensRangeResult::Tokens(
            ls_types::SemanticTokens {
                result_id: None,
                data: semantic_tokens::response(Arc::clone(&self.hir), to_url(&url)),
            },
        )))
    }

    async fn goto_definition(
        &self,
        params: ls_types::GotoDefinitionParams,
    ) -> jsonrpc::Result<Option<ls_types::GotoDefinitionResponse>> {
        let url = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        Ok(goto_definition::response(Arc::clone(&self.hir), to_url(&url), position))
    }

    async fn references(&self, params: ls_types::ReferenceParams) -> jsonrpc::Result<Option<Vec<ls_types::Location>>> {
        let url = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        let source_map_guard = self.source_map.read().unwrap();
        Ok(references::response(
            Arc::clone(&self.hir),
            to_url(&url),
            position,
            &source_map_guard,
        ))
    }

    async fn document_symbol(
        &self,
        params: ls_types::DocumentSymbolParams,
    ) -> jsonrpc::Result<Option<ls_types::DocumentSymbolResponse>> {
        let uri = params.text_document.uri;
        let source_map_guard = self.source_map.read().unwrap();
        Ok(document_symbol::response(
            Arc::clone(&self.hir),
            to_url(&uri),
            &source_map_guard,
        ))
    }

    async fn hover(&self, params: ls_types::HoverParams) -> jsonrpc::Result<Option<ls_types::Hover>> {
        let url = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        Ok(hover::response(Arc::clone(&self.hir), to_url(&url), position))
    }

    async fn completion(
        &self,
        params: ls_types::CompletionParams,
    ) -> jsonrpc::Result<Option<ls_types::CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        let source_map_guard = self.source_map.read().unwrap();
        Ok(completions::response(
            Arc::clone(&self.hir),
            to_url(&uri),
            position,
            &source_map_guard,
        ))
    }

    async fn execute_command(
        &self,
        params: ls_types::ExecuteCommandParams,
    ) -> jsonrpc::Result<Option<serde_json::Value>> {
        execute_command::response(params)
    }

    async fn formatting(
        &self,
        params: ls_types::DocumentFormattingParams,
    ) -> jsonrpc::Result<Option<Vec<ls_types::TextEdit>>> {
        if !self
            .error_map
            .get(&params.text_document.uri.to_string())
            .unwrap()
            .is_empty()
        {
            return Ok(None);
        }

        let text = Arc::clone(&self.text_map.get(&params.text_document.uri.to_string()).unwrap());
        let formatted_text = tokio::task::spawn_blocking(move || {
            mq_formatter::Formatter::new(Some(mq_formatter::FormatterConfig { indent_width: 2 })).format(&text)
        })
        .await
        .map_err(|_| jsonrpc::Error::new(jsonrpc::ErrorCode::InternalError))?
        .map_err(|_| jsonrpc::Error::new(jsonrpc::ErrorCode::ParseError))?;

        Ok(Some(vec![ls_types::TextEdit {
            range: ls_types::Range::new(
                ls_types::Position::new(0, 0),
                ls_types::Position::new(formatted_text.lines().count() as u32, u32::MAX),
            ),
            new_text: formatted_text,
        }]))
    }
}

impl Backend {
    async fn on_change(&self, uri: Url, text: String) {
        let (nodes, errors) = if text.is_empty() {
            (Vec::new(), mq_lang::CstErrorReporter::default())
        } else {
            mq_lang::parse_recovery(&text)
        };
        let (source_id, _) = self.hir.write().unwrap().add_nodes(uri.clone(), &nodes);

        let uri_string = uri.to_string();
        self.source_map.write().unwrap().insert(uri_string.clone(), source_id);
        self.text_map.insert(uri_string.clone(), text.to_string().into());
        self.error_map.insert(uri_string, errors.error_ranges(&text));
    }

    async fn diagnostics(&self, uri: Url, version: Option<i32>) {
        let uri_string = uri.to_string();

        // Get errors for this specific file
        let file_errors = self.error_map.get(&uri_string);

        let mut diagnostics = Vec::new();

        // Add parsing errors if they exist
        if let Some(errors) = file_errors {
            diagnostics.extend(errors.iter().map(|(message, item)| {
                ls_types::Diagnostic::new_simple(
                    ls_types::Range::new(
                        ls_types::Position {
                            line: item.start.line - 1,
                            character: (item.start.column - 1) as u32,
                        },
                        ls_types::Position {
                            line: item.end.line - 1,
                            character: (item.end.column - 1) as u32,
                        },
                    ),
                    message.to_string(),
                )
            }));
        }

        {
            let source_map_guard = self.source_map.read().unwrap();
            if let Some(source_id) = source_map_guard.get_by_left(&uri_string) {
                let hir_guard = self.hir.read().unwrap();

                // Build a map of text_range -> bool for this file's symbols for O(1) lookup
                let mut range_map = std::collections::HashMap::new();
                for (_, symbol) in hir_guard.symbols() {
                    if symbol.source.source_id == Some(*source_id)
                        && let Some(ref text_range) = symbol.source.text_range
                    {
                        range_map.insert(text_range, true);
                    }
                }

                // Filter HIR errors to only include ones from this specific source
                diagnostics.extend(hir_guard.error_ranges().into_iter().filter_map(|(message, item)| {
                    if range_map.contains_key(&item) {
                        Some(ls_types::Diagnostic::new_simple(
                            ls_types::Range::new(
                                ls_types::Position {
                                    line: item.start.line - 1,
                                    character: (item.start.column - 1) as u32,
                                },
                                ls_types::Position {
                                    line: item.end.line - 1,
                                    character: (item.end.column - 1) as u32,
                                },
                            ),
                            message,
                        ))
                    } else {
                        None
                    }
                }));

                // Add unused function warnings
                let unused_functions = hir_guard.unused_functions(*source_id);
                for (_, symbol) in unused_functions {
                    if let Some(text_range) = &symbol.source.text_range {
                        let mut diagnostic = ls_types::Diagnostic::new_simple(
                            ls_types::Range::new(
                                ls_types::Position {
                                    line: text_range.start.line - 1,
                                    character: (text_range.start.column - 1) as u32,
                                },
                                ls_types::Position {
                                    line: text_range.end.line - 1,
                                    character: (text_range.end.column - 1) as u32,
                                },
                            ),
                            format!(
                                "Function '{}' is defined but never used",
                                symbol.value.as_ref().unwrap_or(&"<anonymous>".into())
                            ),
                        );
                        diagnostic.severity = Some(ls_types::DiagnosticSeverity::WARNING);
                        diagnostics.push(diagnostic);
                    }
                }

                // Add HIR warnings (including unreachable code warnings)
                diagnostics.extend(hir_guard.warning_ranges().into_iter().filter_map(|(message, item)| {
                    if range_map.contains_key(&item) {
                        let mut diagnostic = ls_types::Diagnostic::new_simple(
                            ls_types::Range::new(
                                ls_types::Position {
                                    line: item.start.line - 1,
                                    character: (item.start.column - 1) as u32,
                                },
                                ls_types::Position {
                                    line: item.end.line - 1,
                                    character: (item.end.column - 1) as u32,
                                },
                            ),
                            message,
                        );
                        diagnostic.severity = Some(ls_types::DiagnosticSeverity::WARNING);
                        Some(diagnostic)
                    } else {
                        None
                    }
                }));
            }
        } // Guards are dropped here, before the await

        self.client
            .publish_diagnostics(to_uri(&uri), diagnostics, version)
            .await;
    }
}

#[derive(Debug, Clone, Default)]
pub struct LspConfig {
    module_paths: Vec<PathBuf>,
}

impl LspConfig {
    /// Creates a new LspConfig.
    ///
    /// # Arguments
    ///
    /// * `module_paths` - A vector of paths to modules that should be loaded and made available
    ///   to the LSP server. These paths are used to initialize the language server's environment,
    ///   allowing it to provide features such as code completion, diagnostics, and navigation
    ///   based on the specified modules.
    pub fn new(module_paths: Vec<PathBuf>) -> Self {
        Self { module_paths }
    }
}

pub async fn start(config: LspConfig) {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(|client| Backend {
        client,
        hir: Arc::new(RwLock::new(mq_hir::Hir::new(config.module_paths))),
        source_map: RwLock::new(BiMap::new()),
        error_map: DashMap::new(),
        text_map: DashMap::new(),
    });

    Server::new(stdin, stdout, socket).serve(service).await;
}

#[cfg(test)]
mod tests {
    use tower_lsp_server::ls_types;

    use super::*;

    #[tokio::test]
    async fn test_did_open() {
        let (service, _) = LspService::new(|client| Backend {
            client,
            hir: Arc::new(RwLock::new(mq_hir::Hir::default())),
            source_map: RwLock::new(BiMap::new()),
            error_map: DashMap::new(),
            text_map: DashMap::new(),
        });

        let backend = service.inner();
        let uri = Url::parse("file:///test.mq").unwrap();

        backend
            .did_open(ls_types::DidOpenTextDocumentParams {
                text_document: ls_types::TextDocumentItem {
                    uri: to_uri(&uri),
                    language_id: "mq".to_string(),
                    version: 1,
                    text: "def main(): 1;".to_string(),
                },
            })
            .await;

        assert!(backend.source_map.read().unwrap().contains_left(&uri.to_string()));
        assert!(
            backend
                .hir
                .read()
                .unwrap()
                .symbols()
                .map(|(_, s)| { s.value.as_ref().map(|v| v.to_string()).unwrap_or_else(|| "".into()) })
                .collect::<Vec<_>>()
                .contains(&"main".into()),
        );
        assert!(backend.error_map.get(&uri.to_string()).unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_formatting() {
        let (service, _) = LspService::new(|client| Backend {
            client,
            hir: Arc::new(RwLock::new(mq_hir::Hir::default())),
            source_map: RwLock::new(BiMap::new()),
            error_map: DashMap::new(),
            text_map: DashMap::new(),
        });

        let backend = service.inner();
        let uri = Url::parse("file:///test.mq").unwrap();
        let text = "def main():1;";

        let (_, errors) = mq_lang::parse_recovery(text);
        backend.text_map.insert(uri.to_string(), text.to_string().into());
        backend.error_map.insert(uri.to_string(), errors.error_ranges(text));

        let result = backend
            .formatting(ls_types::DocumentFormattingParams {
                text_document: ls_types::TextDocumentIdentifier { uri: to_uri(&uri) },
                options: ls_types::FormattingOptions {
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
            hir: Arc::new(RwLock::new(mq_hir::Hir::default())),
            source_map: RwLock::new(BiMap::new()),
            error_map: DashMap::new(),
            text_map: DashMap::new(),
        });

        let backend = service.inner();
        let uri = Url::parse("file:///test.mq").unwrap();

        let result = backend
            .completion(ls_types::CompletionParams {
                text_document_position: ls_types::TextDocumentPositionParams {
                    text_document: ls_types::TextDocumentIdentifier { uri: to_uri(&uri) },
                    position: ls_types::Position::new(0, 0),
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
            hir: Arc::new(RwLock::new(mq_hir::Hir::default())),
            source_map: RwLock::new(BiMap::new()),
            error_map: DashMap::new(),
            text_map: DashMap::new(),
        });

        let backend = service.inner();
        let uri = Url::parse("file:///test.mq").unwrap();

        let result = backend
            .goto_definition(ls_types::GotoDefinitionParams {
                text_document_position_params: ls_types::TextDocumentPositionParams {
                    text_document: ls_types::TextDocumentIdentifier { uri: to_uri(&uri) },
                    position: ls_types::Position::new(0, 0),
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
            hir: Arc::new(RwLock::new(mq_hir::Hir::default())),
            source_map: RwLock::new(BiMap::new()),
            error_map: DashMap::new(),
            text_map: DashMap::new(),
        });

        let backend = service.inner();
        let uri = Url::parse("file:///test.mq").unwrap();

        let result = backend
            .hover(ls_types::HoverParams {
                text_document_position_params: ls_types::TextDocumentPositionParams {
                    text_document: ls_types::TextDocumentIdentifier { uri: to_uri(&uri) },
                    position: ls_types::Position::new(0, 0),
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
            hir: Arc::new(RwLock::new(mq_hir::Hir::default())),
            source_map: RwLock::new(BiMap::new()),
            error_map: DashMap::new(),
            text_map: DashMap::new(),
        });

        let backend = service.inner();
        let uri = Url::parse("file:///test.mq").unwrap();

        backend
            .hir
            .write()
            .unwrap()
            .add_code(Some(uri.clone()), "def main(): 1;");

        let result = backend
            .semantic_tokens_full(ls_types::SemanticTokensParams {
                text_document: ls_types::TextDocumentIdentifier { uri: to_uri(&uri) },
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            })
            .await;

        assert_eq!(
            result.unwrap().unwrap(),
            ls_types::SemanticTokensResult::Tokens(ls_types::SemanticTokens {
                result_id: None,
                data: vec![
                    ls_types::SemanticToken {
                        delta_line: 0,
                        delta_start: 0,
                        length: 3,
                        token_type: 2,
                        token_modifiers_bitset: 0
                    },
                    ls_types::SemanticToken {
                        delta_line: 0,
                        delta_start: 4,
                        length: 4,
                        token_type: 1,
                        token_modifiers_bitset: 0
                    },
                    ls_types::SemanticToken {
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
            hir: Arc::new(RwLock::new(mq_hir::Hir::default())),
            source_map: RwLock::new(BiMap::new()),
            error_map: DashMap::new(),
            text_map: DashMap::new(),
        });

        let backend = service.inner();
        let uri = Url::parse("file:///test.mq").unwrap();

        let code = "def test_func(): 1;\ndef main(): test_func();";
        let (nodes, _) = mq_lang::parse_recovery(code);
        let (source_id, _) = backend.hir.write().unwrap().add_nodes(uri.clone(), &nodes);

        backend.source_map.write().unwrap().insert(uri.to_string(), source_id);

        let result = backend
            .references(ls_types::ReferenceParams {
                text_document_position: ls_types::TextDocumentPositionParams {
                    text_document: ls_types::TextDocumentIdentifier { uri: to_uri(&uri) },
                    position: ls_types::Position::new(0, 6),
                },
                work_done_progress_params: Default::default(),
                context: ls_types::ReferenceContext {
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
            hir: Arc::new(RwLock::new(mq_hir::Hir::default())),
            source_map: RwLock::new(BiMap::new()),
            error_map: DashMap::new(),
            text_map: DashMap::new(),
        });

        let backend = service.inner();
        let uri = Url::parse("file:///test.mq").unwrap();

        let code = "def test_func(): 1;\ndef main(): test_func();";
        let (nodes, _) = mq_lang::parse_recovery(code);
        let (source_id, _) = backend.hir.write().unwrap().add_nodes(uri.clone(), &nodes);

        backend.source_map.write().unwrap().insert(uri.to_string(), source_id);

        let result = backend
            .document_symbol(ls_types::DocumentSymbolParams {
                text_document: ls_types::TextDocumentIdentifier { uri: to_uri(&uri) },
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            })
            .await;

        assert!(result.is_ok());

        let symbols = result.unwrap().unwrap();

        if let ls_types::DocumentSymbolResponse::Nested(symbols) = symbols {
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
            hir: Arc::new(RwLock::new(mq_hir::Hir::default())),
            source_map: RwLock::new(BiMap::new()),
            error_map: DashMap::new(),
            text_map: DashMap::new(),
        });

        let backend = service.inner();
        let uri = Url::parse("file:///test.mq").unwrap();

        backend
            .did_open(ls_types::DidOpenTextDocumentParams {
                text_document: ls_types::TextDocumentItem {
                    uri: to_uri(&uri),
                    language_id: "mq".to_string(),
                    version: 1,
                    text: "def main(): 1;".to_string(),
                },
            })
            .await;

        backend
            .did_change(ls_types::DidChangeTextDocumentParams {
                text_document: ls_types::VersionedTextDocumentIdentifier {
                    uri: to_uri(&uri),
                    version: 2,
                },
                content_changes: vec![ls_types::TextDocumentContentChangeEvent {
                    range: None,
                    range_length: None,
                    text: "def main(): 2;".to_string(),
                }],
            })
            .await;

        // Check if content was updated
        let text = backend.text_map.get(&uri.to_string());
        assert!(text.is_some());
    }

    #[tokio::test]
    async fn test_did_close() {
        let (service, _) = LspService::new(|client| Backend {
            client,
            hir: Arc::new(RwLock::new(mq_hir::Hir::default())),
            source_map: RwLock::new(BiMap::new()),
            error_map: DashMap::new(),
            text_map: DashMap::new(),
        });

        let backend = service.inner();
        let uri = Url::parse("file:///test.mq").unwrap();

        // Open and setup a file with content
        backend
            .did_open(ls_types::DidOpenTextDocumentParams {
                text_document: ls_types::TextDocumentItem {
                    uri: to_uri(&uri),
                    language_id: "mq".to_string(),
                    version: 1,
                    text: "def main(): invalid_syntax".to_string(),
                },
            })
            .await;

        // Verify data exists before close
        assert!(backend.error_map.contains_key(&uri.to_string()));
        assert!(backend.text_map.contains_key(&uri.to_string()));
        assert!(backend.source_map.read().unwrap().contains_left(&uri.to_string()));

        // Close the file
        use ls_types::DidCloseTextDocumentParams;
        use ls_types::TextDocumentIdentifier;
        backend
            .did_close(DidCloseTextDocumentParams {
                text_document: TextDocumentIdentifier { uri: to_uri(&uri) },
            })
            .await;

        // Verify data is cleaned up after close
        assert!(!backend.error_map.contains_key(&uri.to_string()));
        assert!(!backend.text_map.contains_key(&uri.to_string()));
        assert!(!backend.source_map.read().unwrap().contains_left(&uri.to_string()));
    }

    #[tokio::test]
    async fn test_did_save() {
        let (service, _) = LspService::new(|client| Backend {
            client,
            hir: Arc::new(RwLock::new(mq_hir::Hir::default())),
            source_map: RwLock::new(BiMap::new()),
            error_map: DashMap::new(),
            text_map: DashMap::new(),
        });

        let backend = service.inner();
        let uri = Url::parse("file:///test.mq").unwrap();

        // Setup some content with errors
        let text = "def main(): invalid_syntax";
        let (_, errors) = mq_lang::parse_recovery(text);
        backend.text_map.insert(uri.to_string(), text.to_string().into());
        backend.error_map.insert(uri.to_string(), errors.error_ranges(text));

        backend
            .source_map
            .write()
            .unwrap()
            .insert(uri.to_string(), mq_hir::SourceId::default());

        // Trigger save
        backend
            .did_save(ls_types::DidSaveTextDocumentParams {
                text_document: ls_types::TextDocumentIdentifier { uri: to_uri(&uri) },
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
            hir: Arc::new(RwLock::new(mq_hir::Hir::default())),
            source_map: RwLock::new(BiMap::new()),
            error_map: DashMap::new(),
            text_map: DashMap::new(),
        });

        let backend = service.inner();

        // Test initialize
        let init_result = backend.initialize(ls_types::InitializeParams::default()).await;

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
            hir: Arc::new(RwLock::new(mq_hir::Hir::default())),
            source_map: RwLock::new(BiMap::new()),
            error_map: DashMap::new(),
            text_map: DashMap::new(),
        });

        let backend = service.inner();
        let uri = Url::parse("file:///test.mq").unwrap();

        backend
            .hir
            .write()
            .unwrap()
            .add_code(Some(uri.clone()), "def main(): 1;");

        let result = backend
            .semantic_tokens_range(ls_types::SemanticTokensRangeParams {
                text_document: ls_types::TextDocumentIdentifier { uri: to_uri(&uri) },
                range: ls_types::Range {
                    start: ls_types::Position { line: 0, character: 0 },
                    end: ls_types::Position { line: 0, character: 12 },
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
            hir: Arc::new(RwLock::new(mq_hir::Hir::default())),
            source_map: RwLock::new(BiMap::new()),
            error_map: DashMap::new(),
            text_map: DashMap::new(),
        });

        let backend = service.inner();
        let uri = Url::parse("file:///test.mq").unwrap();

        // Add content with parsing errors
        let text = "def main() 1;"; // Missing colon
        let (_, errors) = mq_lang::parse_recovery(text);
        backend.text_map.insert(uri.to_string(), text.to_string().into());
        backend.error_map.insert(uri.to_string(), errors.error_ranges(text));

        // We can't directly test client.publish_diagnostics was called with correct diagnostics,
        // but we can verify the code doesn't panic
        backend.diagnostics(uri, None).await;
    }

    #[tokio::test]
    async fn test_unused_function_warnings() {
        let (service, _) = LspService::new(|client| Backend {
            client,
            hir: Arc::new(RwLock::new(mq_hir::Hir::default())),
            source_map: RwLock::new(BiMap::new()),
            error_map: DashMap::new(),
            text_map: DashMap::new(),
        });

        let backend = service.inner();
        let uri = Url::parse("file:///test.mq").unwrap();

        // Code with unused function
        let code = "def used_function(): 1; def unused_function(): 2; | used_function()";
        let (nodes, errors) = mq_lang::parse_recovery(code);
        let (source_id, _) = backend.hir.write().unwrap().add_nodes(uri.clone(), &nodes);

        backend.source_map.write().unwrap().insert(uri.to_string(), source_id);
        backend.text_map.insert(uri.to_string(), code.to_string().into());
        backend.error_map.insert(uri.to_string(), errors.error_ranges(code));

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
            hir: Arc::new(RwLock::new(mq_hir::Hir::default())),
            source_map: RwLock::new(BiMap::new()),
            error_map: DashMap::new(),
            text_map: DashMap::new(),
        });

        let backend = service.inner();
        let uri = Url::parse("file:///test.mq").unwrap();

        let text = "def main() 1;";
        let _ = mq_lang::parse_recovery(text);
        backend.text_map.insert(uri.to_string(), text.to_string().into());
        backend.error_map.insert(
            uri.to_string(),
            vec![(
                "Syntax error".to_string(),
                mq_lang::Range {
                    start: mq_lang::Position { line: 1, column: 10 },
                    end: mq_lang::Position { line: 1, column: 11 },
                },
            )],
        );

        let result = backend
            .formatting(ls_types::DocumentFormattingParams {
                text_document: ls_types::TextDocumentIdentifier { uri: to_uri(&uri) },
                options: ls_types::FormattingOptions {
                    tab_size: 2,
                    insert_spaces: true,
                    ..Default::default()
                },
                work_done_progress_params: Default::default(),
            })
            .await;

        assert_eq!(result.unwrap(), None);
    }

    #[tokio::test]
    async fn test_unreachable_code_warnings() {
        let (service, _) = LspService::new(|client| Backend {
            client,
            hir: Arc::new(RwLock::new(mq_hir::Hir::default())),
            source_map: RwLock::new(BiMap::new()),
            error_map: DashMap::new(),
            text_map: DashMap::new(),
        });

        let backend = service.inner();
        let uri = Url::parse("file:///test.mq").unwrap();

        // Code with unreachable code after halt()
        let code = "def test(): halt(1) | let x = 42";
        let (nodes, errors) = mq_lang::parse_recovery(code);
        let (source_id, _) = backend.hir.write().unwrap().add_nodes(uri.clone(), &nodes);

        backend.source_map.write().unwrap().insert(uri.to_string(), source_id);
        backend.text_map.insert(uri.to_string(), code.to_string().into());
        backend.error_map.insert(uri.to_string(), errors.error_ranges(code));

        // Check unreachable code warnings are detected
        {
            let hir_lock = backend.hir.read().unwrap();
            let warnings = hir_lock.warnings();
            assert_eq!(warnings.len(), 1);

            match &warnings[0] {
                mq_hir::HirWarning::UnreachableCode { symbol } => {
                    assert_eq!(symbol.value.as_deref(), Some("x"));
                }
            }
            drop(hir_lock);
        }

        // Diagnostics should be published with warning severity
        backend.diagnostics(uri, None).await;
    }
}
