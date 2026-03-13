use std::path::PathBuf;
use std::str::FromStr;
use std::sync::{Arc, RwLock};

use bimap::BiMap;
use dashmap::DashMap;
use rustc_hash::FxHashSet;
use tower_lsp_server::ls_types::DocumentRangeFormattingParams;
use url::Url;

use crate::error::LspError;
use crate::{
    capabilities, completions, document_symbol, execute_command, goto_definition, hover, inlay_hints, references,
    semantic_tokens,
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
    type_env_map: DashMap<String, mq_check::TypeEnv>,
    type_error_map: DashMap<String, Vec<LspError>>,
    incremental_parser_map: DashMap<String, mq_lang::IncrementalParser>,
    config: LspConfig,
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
        self.type_error_map.remove(&uri_string);
        self.incremental_parser_map.remove(&uri_string);
        self.type_env_map.remove(&uri_string);

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

        let type_env = if self.config.enable_type_checking {
            self.type_env_map.get(&url.to_string()).map(|e| e.clone())
        } else {
            None
        };

        Ok(hover::response(Arc::clone(&self.hir), to_url(&url), type_env, position))
    }

    async fn inlay_hint(&self, params: ls_types::InlayHintParams) -> jsonrpc::Result<Option<Vec<ls_types::InlayHint>>> {
        if !self.config.enable_type_checking {
            return Ok(None);
        }
        let url = to_url(&params.text_document.uri);
        let type_env = self.type_env_map.get(&url.to_string()).map(|e| e.clone());
        Ok(inlay_hints::response(
            Arc::clone(&self.hir),
            url,
            type_env,
            params.range,
        ))
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
        let uri_str = params.text_document.uri.to_string();
        let text = match self.incremental_parser_map.get(&uri_str) {
            Some(parser) if !parser.result().1.has_errors() => parser.source(),
            _ => return Ok(None),
        };

        // Handle work done progress
        let progress = if let Some(token) = params.work_done_progress_params.work_done_token.clone() {
            self.client.create_work_done_progress(token.clone()).await.ok();
            Some(
                self.client
                    .progress(token, "Formatting")
                    .with_message("Formatting document...")
                    .begin()
                    .await,
            )
        } else {
            None
        };
        let formatted_text = tokio::task::spawn_blocking(move || {
            mq_formatter::Formatter::new(Some(mq_formatter::FormatterConfig {
                indent_width: 2,
                ..Default::default()
            }))
            .format(&text)
        })
        .await
        .map_err(|_| jsonrpc::Error::new(jsonrpc::ErrorCode::InternalError))?
        .map_err(|_| jsonrpc::Error::new(jsonrpc::ErrorCode::ParseError))?;

        // End work done progress
        if let Some(progress) = progress {
            progress.finish_with_message("Formatting complete").await;
        }

        Ok(Some(vec![ls_types::TextEdit {
            range: ls_types::Range::new(
                ls_types::Position::new(0, 0),
                ls_types::Position::new(formatted_text.lines().count() as u32, u32::MAX),
            ),
            new_text: formatted_text,
        }]))
    }

    async fn range_formatting(
        &self,
        params: DocumentRangeFormattingParams,
    ) -> jsonrpc::Result<Option<Vec<ls_types::TextEdit>>> {
        let uri_str = params.text_document.uri.to_string();
        let text = match self.incremental_parser_map.get(&uri_str) {
            Some(parser) if !parser.result().1.has_errors() => parser.source(),
            _ => return Ok(None),
        };

        // Handle work done progress
        let progress = if let Some(token) = params.work_done_progress_params.work_done_token.clone() {
            self.client.create_work_done_progress(token.clone()).await.ok();
            Some(
                self.client
                    .progress(token, "Range Formatting")
                    .with_message("Formatting range...")
                    .begin()
                    .await,
            )
        } else {
            None
        };

        let lines: Vec<&str> = text.lines().collect();
        let start_line = params.range.start.line as usize;
        let start_char = params.range.start.character as usize;
        let end_line = params.range.end.line as usize;
        let end_char = params.range.end.character as usize;

        if start_line >= lines.len() || end_line > lines.len() || start_line > end_line {
            return Ok(None);
        }

        // Extract the text in the specified range (line and character based).
        let mut selected = String::new();
        if start_line == end_line {
            // Single line selection
            let line = lines.get(start_line).unwrap_or(&"");
            let start = start_char.min(line.len());
            let end = end_char.min(line.len());
            selected.push_str(&line[start..end]);
        } else {
            // First line: from start_char to end of line
            let first_line = lines.get(start_line).unwrap_or(&"");
            let start = start_char.min(first_line.len());
            selected.push_str(&first_line[start..]);
            selected.push('\n');
            // Middle lines: whole lines
            for l in (start_line + 1)..end_line {
                if let Some(line) = lines.get(l) {
                    selected.push_str(line);
                    selected.push('\n');
                }
            }
            // Last line: from 0 to end_char
            if let Some(last_line) = lines.get(end_line) {
                let end = end_char.min(last_line.len());
                selected.push_str(&last_line[..end]);
            }
        }

        let formatted_range = tokio::task::spawn_blocking(move || {
            mq_formatter::Formatter::new(Some(mq_formatter::FormatterConfig {
                indent_width: 2,
                ..Default::default()
            }))
            .format(&selected)
        })
        .await
        .map_err(|_| jsonrpc::Error::new(jsonrpc::ErrorCode::InternalError))?
        .map_err(|_| jsonrpc::Error::new(jsonrpc::ErrorCode::ParseError))?;

        // End work done progress
        if let Some(progress) = progress {
            progress.finish_with_message("Range formatting complete").await;
        }

        Ok(Some(vec![ls_types::TextEdit {
            range: params.range,
            new_text: formatted_range,
        }]))
    }
}

impl Backend {
    async fn on_change(&self, uri: Url, text: String) {
        let uri_string = uri.to_string();
        let (nodes, cst_errors) = if text.is_empty() {
            self.incremental_parser_map.remove(&uri_string);
            (Vec::new(), mq_lang::CstErrorReporter::default())
        } else {
            let entry = self.incremental_parser_map.entry(uri_string.clone());
            let parser = entry
                .and_modify(|p| {
                    p.update(&text);
                })
                .or_insert_with(|| mq_lang::IncrementalParser::new(&text));
            let (nodes, errors) = parser.result();
            (nodes.to_vec(), errors.clone())
        };
        let (source_id, _) = self.hir.write().unwrap().add_nodes(uri.clone(), &nodes);

        if !cst_errors.has_errors() && self.config.enable_type_checking {
            let hir_guard = self.hir.read().unwrap();
            let mut checker = mq_check::TypeChecker::with_options(self.config.type_checker_options);
            let type_errors = checker.check(&hir_guard);

            // Build a set of text ranges from the current source's symbols
            // so that type errors originating from other sources (e.g., pre-loaded modules)
            // are not incorrectly attributed to this file.
            let source_locations: FxHashSet<mq_lang::Range> = hir_guard
                .symbols_for_source(source_id)
                .filter_map(|(_, symbol)| symbol.source.text_range)
                .collect();

            self.type_env_map
                .insert(uri_string.clone(), checker.symbol_types().clone());

            let lsp_errors: Vec<LspError> = type_errors
                .into_iter()
                .filter(|e| {
                    e.location()
                        .map(|range| source_locations.contains(&range))
                        .unwrap_or(false)
                })
                .map(LspError::TypeError)
                .collect();
            self.type_error_map.insert(uri_string.clone(), lsp_errors);
        } else {
            self.type_error_map.remove(&uri_string);
        }

        self.source_map.write().unwrap().insert(uri_string, source_id);
    }

    async fn diagnostics(&self, uri: Url, version: Option<i32>) {
        let uri_string = uri.to_string();
        let mut diagnostics = Vec::new();

        // Add syntax errors from incremental parser
        if let Some(parser) = self.incremental_parser_map.get(&uri_string) {
            let (_, cst_errors) = parser.result();
            let text = parser.source();
            let syntax_diagnostics: Vec<ls_types::Diagnostic> = cst_errors
                .error_ranges(&text)
                .into_iter()
                .map(|(message, range)| {
                    let lsp_error = LspError::SyntaxError((message, range));
                    ls_types::Diagnostic::from(&lsp_error)
                })
                .collect();
            diagnostics.extend(syntax_diagnostics);
        }

        // Add type errors
        if let Some(type_errors) = self.type_error_map.get(&uri_string) {
            let errs: Vec<ls_types::Diagnostic> = (*type_errors).iter().map(Into::into).collect();
            diagnostics.extend(errs);
        }

        {
            let source_map_guard = self.source_map.read().unwrap();
            if let Some(source_id) = source_map_guard.get_by_left(&uri_string) {
                let hir_guard = self.hir.read().unwrap();

                // Build a set of text_ranges for this file's symbols for O(1) lookup
                let range_set: FxHashSet<mq_lang::Range> = hir_guard
                    .symbols()
                    .filter_map(|(_, symbol)| {
                        if symbol.source.source_id == Some(*source_id) {
                            symbol.source.text_range
                        } else {
                            None
                        }
                    })
                    .collect();

                // Filter HIR errors to only include ones from this specific source
                diagnostics.extend(hir_guard.error_ranges().into_iter().filter_map(|(message, item)| {
                    if range_set.contains(&item) {
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
                    if range_set.contains(&item) {
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
    enable_type_checking: bool,
    type_checker_options: mq_check::TypeCheckerOptions,
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
    pub fn new(
        module_paths: Vec<PathBuf>,
        enable_type_checking: bool,
        type_checker_options: mq_check::TypeCheckerOptions,
    ) -> Self {
        Self {
            module_paths,
            enable_type_checking,
            type_checker_options,
        }
    }
}

pub async fn start(config: LspConfig) {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(|client| Backend {
        client,
        hir: Arc::new(RwLock::new(mq_hir::Hir::new(config.module_paths.clone()))),
        source_map: RwLock::new(BiMap::new()),
        type_env_map: DashMap::new(),
        type_error_map: DashMap::new(),
        incremental_parser_map: DashMap::new(),
        config,
    });

    Server::new(stdin, stdout, socket).serve(service).await;
}

#[cfg(test)]
mod tests {
    use tower_lsp_server::ls_types::{self, TextDocumentIdentifier};

    use super::*;

    #[tokio::test]
    async fn test_did_open() {
        let (service, _) = LspService::new(|client| Backend {
            client,
            hir: Arc::new(RwLock::new(mq_hir::Hir::default())),
            source_map: RwLock::new(BiMap::new()),
            type_env_map: DashMap::new(),
            type_error_map: DashMap::new(),
            incremental_parser_map: DashMap::new(),
            config: LspConfig::default(),
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
        assert!(
            !backend
                .incremental_parser_map
                .get(&uri.to_string())
                .unwrap()
                .result()
                .1
                .has_errors()
        );
    }

    #[tokio::test]
    async fn test_formatting() {
        let (service, _) = LspService::new(|client| Backend {
            client,
            hir: Arc::new(RwLock::new(mq_hir::Hir::default())),
            source_map: RwLock::new(BiMap::new()),
            type_env_map: DashMap::new(),
            type_error_map: DashMap::new(),
            incremental_parser_map: DashMap::new(),
            config: LspConfig::default(),
        });

        let backend = service.inner();
        let uri = Url::parse("file:///test.mq").unwrap();
        let text = "def main():1;";

        backend
            .incremental_parser_map
            .insert(uri.to_string(), mq_lang::IncrementalParser::new(text));

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
            type_env_map: DashMap::new(),
            type_error_map: DashMap::new(),
            incremental_parser_map: DashMap::new(),
            config: LspConfig::default(),
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
            type_env_map: DashMap::new(),
            type_error_map: DashMap::new(),
            incremental_parser_map: DashMap::new(),
            config: LspConfig::default(),
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
            type_env_map: DashMap::new(),
            type_error_map: DashMap::new(),
            incremental_parser_map: DashMap::new(),
            config: LspConfig::default(),
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
            type_env_map: DashMap::new(),
            type_error_map: DashMap::new(),
            incremental_parser_map: DashMap::new(),
            config: LspConfig::default(),
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
            type_env_map: DashMap::new(),
            type_error_map: DashMap::new(),
            incremental_parser_map: DashMap::new(),
            config: LspConfig::default(),
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
            type_env_map: DashMap::new(),
            type_error_map: DashMap::new(),
            incremental_parser_map: DashMap::new(),
            config: LspConfig::default(),
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
            type_env_map: DashMap::new(),
            type_error_map: DashMap::new(),
            incremental_parser_map: DashMap::new(),
            config: LspConfig::default(),
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
        assert!(backend.incremental_parser_map.contains_key(&uri.to_string()));
    }

    #[tokio::test]
    async fn test_did_close() {
        let (service, _) = LspService::new(|client| Backend {
            client,
            hir: Arc::new(RwLock::new(mq_hir::Hir::default())),
            source_map: RwLock::new(BiMap::new()),
            type_env_map: DashMap::new(),
            type_error_map: DashMap::new(),
            incremental_parser_map: DashMap::new(),
            config: LspConfig::default(),
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
        assert!(backend.incremental_parser_map.contains_key(&uri.to_string()));
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
        assert!(!backend.incremental_parser_map.contains_key(&uri.to_string()));
        assert!(!backend.source_map.read().unwrap().contains_left(&uri.to_string()));
    }

    #[tokio::test]
    async fn test_did_save() {
        let (service, _) = LspService::new(|client| Backend {
            client,
            hir: Arc::new(RwLock::new(mq_hir::Hir::default())),
            source_map: RwLock::new(BiMap::new()),
            type_env_map: DashMap::new(),
            type_error_map: DashMap::new(),
            incremental_parser_map: DashMap::new(),
            config: LspConfig::default(),
        });

        let backend = service.inner();
        let uri = Url::parse("file:///test.mq").unwrap();

        // Setup some content with errors
        let text = "def main(): invalid_syntax";
        backend
            .incremental_parser_map
            .insert(uri.to_string(), mq_lang::IncrementalParser::new(text));

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
            type_env_map: DashMap::new(),
            type_error_map: DashMap::new(),
            incremental_parser_map: DashMap::new(),
            config: LspConfig::default(),
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
            type_env_map: DashMap::new(),
            type_error_map: DashMap::new(),
            incremental_parser_map: DashMap::new(),
            config: LspConfig::default(),
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
            type_env_map: DashMap::new(),
            type_error_map: DashMap::new(),
            incremental_parser_map: DashMap::new(),
            config: LspConfig::default(),
        });

        let backend = service.inner();
        let uri = Url::parse("file:///test.mq").unwrap();

        // Add content with parsing errors
        let text = "def main() 1;"; // Missing colon
        backend
            .incremental_parser_map
            .insert(uri.to_string(), mq_lang::IncrementalParser::new(text));

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
            type_env_map: DashMap::new(),
            type_error_map: DashMap::new(),
            incremental_parser_map: DashMap::new(),
            config: LspConfig::default(),
        });

        let backend = service.inner();
        let uri = Url::parse("file:///test.mq").unwrap();

        // Code with unused function
        let code = "def used_function(): 1; def unused_function(): 2; | used_function()";
        let (nodes, _) = mq_lang::parse_recovery(code);
        let (source_id, _) = backend.hir.write().unwrap().add_nodes(uri.clone(), &nodes);

        backend.source_map.write().unwrap().insert(uri.to_string(), source_id);
        backend
            .incremental_parser_map
            .insert(uri.to_string(), mq_lang::IncrementalParser::new(code));

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
            type_env_map: DashMap::new(),
            type_error_map: DashMap::new(),
            incremental_parser_map: DashMap::new(),
            config: LspConfig::default(),
        });

        let backend = service.inner();
        let uri = Url::parse("file:///test.mq").unwrap();

        let text = "def add(x"; // incomplete def = parse error
        backend
            .incremental_parser_map
            .insert(uri.to_string(), mq_lang::IncrementalParser::new(text));

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
            type_env_map: DashMap::new(),
            type_error_map: DashMap::new(),
            incremental_parser_map: DashMap::new(),
            config: LspConfig::default(),
        });

        let backend = service.inner();
        let uri = Url::parse("file:///test.mq").unwrap();

        // Code with unreachable code after halt()
        let code = "def test(): halt(1) | let x = 42";
        let (nodes, _) = mq_lang::parse_recovery(code);
        let (source_id, _) = backend.hir.write().unwrap().add_nodes(uri.clone(), &nodes);

        backend.source_map.write().unwrap().insert(uri.to_string(), source_id);
        backend
            .incremental_parser_map
            .insert(uri.to_string(), mq_lang::IncrementalParser::new(code));

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

    #[tokio::test]
    async fn test_range_formatting_single_line() {
        let uri = "file:///test1.md";
        let text = "def v():1;";
        let (service, _) = LspService::new(|client| Backend {
            client,
            hir: Arc::new(RwLock::new(mq_hir::Hir::default())),
            source_map: RwLock::new(BiMap::new()),
            type_env_map: DashMap::new(),
            type_error_map: DashMap::new(),
            incremental_parser_map: DashMap::new(),
            config: LspConfig::default(),
        });
        let backend = service.inner();
        backend
            .incremental_parser_map
            .insert(uri.to_string(), mq_lang::IncrementalParser::new(text));

        let params = DocumentRangeFormattingParams {
            text_document: TextDocumentIdentifier {
                uri: ls_types::Uri::from_str(uri).unwrap(),
            },
            range: ls_types::Range {
                start: ls_types::Position { line: 0, character: 0 },
                end: ls_types::Position { line: 0, character: 10 },
            },
            options: ls_types::FormattingOptions {
                tab_size: 2,
                insert_spaces: true,
                ..Default::default()
            },
            work_done_progress_params: Default::default(),
        };

        let result = backend.range_formatting(params).await.unwrap();
        assert!(result.is_some());
        let edits = result.unwrap();
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].range.start.line, 0);
        assert_eq!(edits[0].range.end.line, 0);
        assert!(edits[0].new_text.trim().starts_with("def"));
    }

    #[tokio::test]
    async fn test_range_formatting_multi_line() {
        let uri = "file:///test2.md";
        let text = "def v():1;\ndef w():2;\ndef x():3;";
        let (service, _) = LspService::new(|client| Backend {
            client,
            hir: Arc::new(RwLock::new(mq_hir::Hir::default())),
            source_map: RwLock::new(BiMap::new()),
            type_env_map: DashMap::new(),
            type_error_map: DashMap::new(),
            incremental_parser_map: DashMap::new(),
            config: LspConfig::default(),
        });
        let backend = service.inner();
        backend
            .incremental_parser_map
            .insert(uri.to_string(), mq_lang::IncrementalParser::new(text));

        let params = DocumentRangeFormattingParams {
            text_document: TextDocumentIdentifier {
                uri: ls_types::Uri::from_str(uri).unwrap(),
            },
            range: ls_types::Range {
                start: ls_types::Position { line: 1, character: 0 },
                end: ls_types::Position { line: 2, character: 10 },
            },
            options: ls_types::FormattingOptions {
                tab_size: 2,
                insert_spaces: true,
                ..Default::default()
            },
            work_done_progress_params: Default::default(),
        };

        let result = backend.range_formatting(params).await.unwrap();
        assert!(result.is_some());
        let edits = result.unwrap();
        assert_eq!(edits.len(), 1);
        // The formatted text should be the formatted substring of the range.
        assert!(edits[0].new_text.contains("def"));
    }

    #[tokio::test]
    async fn test_range_formatting_with_errors() {
        let uri = "file:///test3.md";
        let (service, _) = LspService::new(|client| Backend {
            client,
            hir: Arc::new(RwLock::new(mq_hir::Hir::default())),
            source_map: RwLock::new(BiMap::new()),
            type_env_map: DashMap::new(),
            type_error_map: DashMap::new(),
            incremental_parser_map: DashMap::new(),
            config: LspConfig::default(),
        });
        let backend = service.inner();
        backend.incremental_parser_map.insert(
            uri.to_string(),
            mq_lang::IncrementalParser::new("def add(x"), // incomplete def = parse error
        );

        let params = DocumentRangeFormattingParams {
            text_document: TextDocumentIdentifier {
                uri: ls_types::Uri::from_str(uri).unwrap(),
            },
            range: ls_types::Range {
                start: ls_types::Position { line: 0, character: 0 },
                end: ls_types::Position { line: 0, character: 20 },
            },
            options: ls_types::FormattingOptions {
                tab_size: 2,
                insert_spaces: true,
                ..Default::default()
            },
            work_done_progress_params: Default::default(),
        };

        let result = backend.range_formatting(params).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_lsp_config_new() {
        let config = LspConfig::new(
            vec![],
            true,
            mq_check::TypeCheckerOptions {
                strict_array: true,
                ..Default::default()
            },
        );
        assert!(config.enable_type_checking);
        assert!(config.type_checker_options.strict_array);
    }

    #[tokio::test]
    async fn test_lsp_config_default() {
        let config = LspConfig::default();
        assert!(!config.enable_type_checking);
        assert!(!config.type_checker_options.strict_array);
    }

    #[tokio::test]
    async fn test_diagnostics_with_type_checking_disabled() {
        let (service, _) = LspService::new(|client| Backend {
            client,
            hir: Arc::new(RwLock::new(mq_hir::Hir::default())),
            source_map: RwLock::new(BiMap::new()),
            type_env_map: DashMap::new(),
            type_error_map: DashMap::new(),
            incremental_parser_map: DashMap::new(),
            config: LspConfig::new(vec![], false, mq_check::TypeCheckerOptions::default()),
        });

        let backend = service.inner();
        let uri = Url::parse("file:///test.mq").unwrap();

        // Code that would cause a type error (number + string)
        let code = r#"1 + "string""#;
        let (nodes, _) = mq_lang::parse_recovery(code);
        let (source_id, _) = backend.hir.write().unwrap().add_nodes(uri.clone(), &nodes);

        backend.source_map.write().unwrap().insert(uri.to_string(), source_id);
        backend
            .incremental_parser_map
            .insert(uri.to_string(), mq_lang::IncrementalParser::new(code));

        // No type errors should be emitted since type checking is disabled
        // (diagnostics() calls publish_diagnostics internally, we just verify it doesn't panic)
        backend.diagnostics(uri, None).await;
    }

    #[tokio::test]
    async fn test_diagnostics_with_type_checking_enabled_no_errors() {
        let (service, _) = LspService::new(|client| Backend {
            client,
            hir: Arc::new(RwLock::new(mq_hir::Hir::default())),
            source_map: RwLock::new(BiMap::new()),
            type_env_map: DashMap::new(),
            type_error_map: DashMap::new(),
            incremental_parser_map: DashMap::new(),
            config: LspConfig::new(vec![], true, mq_check::TypeCheckerOptions::default()),
        });

        let backend = service.inner();
        let uri = Url::parse("file:///test.mq").unwrap();

        // Valid code with no type errors
        let code = "def add(x, y): x + y;\n| add(1, 2)";
        let (nodes, _) = mq_lang::parse_recovery(code);
        let (source_id, _) = backend.hir.write().unwrap().add_nodes(uri.clone(), &nodes);

        backend.source_map.write().unwrap().insert(uri.to_string(), source_id);
        backend
            .incremental_parser_map
            .insert(uri.to_string(), mq_lang::IncrementalParser::new(code));

        assert!(
            !backend
                .incremental_parser_map
                .get(&uri.to_string())
                .unwrap()
                .result()
                .1
                .has_errors()
        );

        // Should complete without panic
        backend.diagnostics(uri, None).await;
    }

    #[tokio::test]
    async fn test_diagnostics_with_type_checking_enabled_type_error() {
        let (service, _) = LspService::new(|client| Backend {
            client,
            hir: Arc::new(RwLock::new(mq_hir::Hir::default())),
            source_map: RwLock::new(BiMap::new()),
            type_env_map: DashMap::new(),
            type_error_map: DashMap::new(),
            incremental_parser_map: DashMap::new(),
            config: LspConfig::new(vec![], true, mq_check::TypeCheckerOptions::default()),
        });

        let backend = service.inner();
        let uri = Url::parse("file:///test.mq").unwrap();

        // Code with type error: function arity mismatch
        let code = "def add(x, y): x + y;\n| add(1)";

        // Exercise the full diagnostics pipeline: on_change should parse, type check,
        // and populate type_error_map with type errors when there are no parse errors.
        backend.on_change(uri.clone(), code.to_string()).await;

        let errors = backend
            .type_error_map
            .get(&uri.to_string())
            .expect("expected type diagnostics entry for URI");

        // Ensure that a type error is surfaced through the diagnostics pipeline.
        assert!(
            errors.iter().any(|e| matches!(e, LspError::TypeError(_))),
            "expected at least one type error diagnostic for arity mismatch"
        );

        // Also run diagnostics publishing to ensure the diagnostics path executes.
        backend.diagnostics(uri, None).await;
    }

    #[tokio::test]
    async fn test_diagnostics_type_checking_skipped_when_parse_errors_exist() {
        let (service, _) = LspService::new(|client| Backend {
            client,
            hir: Arc::new(RwLock::new(mq_hir::Hir::default())),
            source_map: RwLock::new(BiMap::new()),
            type_env_map: DashMap::new(),
            type_error_map: DashMap::new(),
            incremental_parser_map: DashMap::new(),
            config: LspConfig::new(vec![], true, mq_check::TypeCheckerOptions::default()),
        });

        let backend = service.inner();
        let uri = Url::parse("file:///test.mq").unwrap();

        // Insert a parser with parse errors so type checking is skipped
        let text = "def add(x"; // incomplete def = parse error
        backend
            .incremental_parser_map
            .insert(uri.to_string(), mq_lang::IncrementalParser::new(text));

        // Parse errors are present, so type checking should be skipped
        assert!(
            backend
                .incremental_parser_map
                .get(&uri.to_string())
                .unwrap()
                .result()
                .1
                .has_errors()
        );

        // Should complete without panic; type checking branch is not entered
        backend.diagnostics(uri, None).await;
    }

    #[tokio::test]
    async fn test_diagnostics_with_strict_array_option() {
        let (service, _) = LspService::new(|client| Backend {
            client,
            hir: Arc::new(RwLock::new(mq_hir::Hir::default())),
            source_map: RwLock::new(BiMap::new()),
            type_env_map: DashMap::new(),
            type_error_map: DashMap::new(),
            incremental_parser_map: DashMap::new(),
            config: LspConfig::new(
                vec![],
                true,
                mq_check::TypeCheckerOptions {
                    strict_array: true,
                    ..Default::default()
                },
            ),
        });

        let backend = service.inner();
        let uri = Url::parse("file:///test.mq").unwrap();

        // Homogeneous array — valid even with strict_array
        let code = "[1, 2, 3]";
        let (nodes, _) = mq_lang::parse_recovery(code);
        let (source_id, _) = backend.hir.write().unwrap().add_nodes(uri.clone(), &nodes);

        backend.source_map.write().unwrap().insert(uri.to_string(), source_id);
        backend
            .incremental_parser_map
            .insert(uri.to_string(), mq_lang::IncrementalParser::new(code));

        assert!(
            !backend
                .incremental_parser_map
                .get(&uri.to_string())
                .unwrap()
                .result()
                .1
                .has_errors()
        );

        backend.diagnostics(uri, None).await;
    }

    #[tokio::test]
    async fn test_diagnostics_with_tuple_option() {
        let (service, _) = LspService::new(|client| Backend {
            client,
            hir: Arc::new(RwLock::new(mq_hir::Hir::default())),
            source_map: RwLock::new(BiMap::new()),
            type_env_map: DashMap::new(),
            type_error_map: DashMap::new(),
            incremental_parser_map: DashMap::new(),
            config: LspConfig::new(
                vec![],
                true,
                mq_check::TypeCheckerOptions {
                    strict_array: false,
                    ..Default::default()
                },
            ),
        });

        let backend = service.inner();
        let uri = Url::parse("file:///test.mq").unwrap();

        // Heterogeneous array typed as tuple — valid with tuple mode
        let code = r#"[1, "hello", true]"#;
        let (nodes, _) = mq_lang::parse_recovery(code);
        let (source_id, _) = backend.hir.write().unwrap().add_nodes(uri.clone(), &nodes);

        backend.source_map.write().unwrap().insert(uri.to_string(), source_id);
        backend
            .incremental_parser_map
            .insert(uri.to_string(), mq_lang::IncrementalParser::new(code));

        assert!(
            !backend
                .incremental_parser_map
                .get(&uri.to_string())
                .unwrap()
                .result()
                .1
                .has_errors()
        );

        backend.diagnostics(uri, None).await;
    }
}
