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
    capabilities, code_action, completions, document_highlight, document_link, document_symbol, execute_command,
    folding_range, goto_definition, hover, inlay_hints, references, rename, selection_range, semantic_tokens,
    signature_help, virtual_document, workspace_symbol,
};
use tower_lsp_server::{Client, LanguageServer, LspService, Server, jsonrpc, ls_types};

fn to_url(uri: &ls_types::Uri) -> Url {
    Url::parse(&uri.to_string()).unwrap()
}

fn to_uri(uri: &Url) -> ls_types::Uri {
    ls_types::Uri::from_str(uri.as_ref()).unwrap()
}

/// End position of `text` in UTF-16 columns.
fn document_end_position(text: &str) -> ls_types::Position {
    let last_line_start = text.rfind('\n').map_or(0, |i| i + 1);
    ls_types::Position::new(
        text.matches('\n').count() as u32,
        text[last_line_start..].encode_utf16().count() as u32,
    )
}

/// A document's text and parsed CST as of a single `on_change`, published together so
/// readers never observe a text/CST pair from two different edits.
#[derive(Debug)]
struct DocumentSnapshot {
    text: Arc<String>,
    /// `None` when the document has never been parsed (only set directly in tests).
    nodes: Option<Arc<Vec<mq_lang::Shared<mq_lang::CstNode>>>>,
}

/// Text and CST of each open document, keyed by URI string, published atomically per change.
type DocumentMap = DashMap<String, Arc<DocumentSnapshot>>;

#[derive(Debug)]
struct Backend {
    client: Client,
    hir: Arc<RwLock<mq_hir::Hir>>,
    source_map: RwLock<BiMap<String, mq_hir::SourceId>>,
    type_env_map: DashMap<String, mq_check::TypeEnv>,
    error_map: DashMap<String, Vec<LspError>>,
    documents: DocumentMap,
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
        self.error_map.remove(&uri_string);
        self.documents.remove(&uri_string);
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

    async fn document_highlight(
        &self,
        params: ls_types::DocumentHighlightParams,
    ) -> jsonrpc::Result<Option<Vec<ls_types::DocumentHighlight>>> {
        let url = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        Ok(document_highlight::response(
            Arc::clone(&self.hir),
            to_url(&url),
            position,
        ))
    }

    async fn document_link(
        &self,
        params: ls_types::DocumentLinkParams,
    ) -> jsonrpc::Result<Option<Vec<ls_types::DocumentLink>>> {
        Ok(document_link::response(
            Arc::clone(&self.hir),
            to_url(&params.text_document.uri),
        ))
    }

    async fn selection_range(
        &self,
        params: ls_types::SelectionRangeParams,
    ) -> jsonrpc::Result<Option<Vec<ls_types::SelectionRange>>> {
        let uri = params.text_document.uri.to_string();
        let nodes = self.documents.get(&uri).and_then(|snapshot| snapshot.nodes.clone());

        Ok(nodes.map(|nodes| selection_range::response(&nodes, &params.positions)))
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

    async fn folding_range(
        &self,
        params: ls_types::FoldingRangeParams,
    ) -> jsonrpc::Result<Option<Vec<ls_types::FoldingRange>>> {
        let uri = params.text_document.uri.to_string();
        let snapshot = self.documents.get(&uri).map(|snapshot| Arc::clone(snapshot.value()));

        if let Some(nodes) = snapshot.as_ref().and_then(|snapshot| snapshot.nodes.clone()) {
            return Ok(folding_range::response_from_nodes(&nodes));
        }

        Ok(folding_range::response(
            snapshot.as_ref().map(|snapshot| snapshot.text.as_str()),
        ))
    }

    async fn symbol(
        &self,
        params: ls_types::WorkspaceSymbolParams,
    ) -> jsonrpc::Result<Option<ls_types::WorkspaceSymbolResponse>> {
        let source_map_guard = self.source_map.read().unwrap();
        Ok(workspace_symbol::response(
            Arc::clone(&self.hir),
            &params.query,
            &source_map_guard,
        ))
    }

    async fn diagnostic(
        &self,
        params: ls_types::DocumentDiagnosticParams,
    ) -> jsonrpc::Result<ls_types::DocumentDiagnosticReportResult> {
        let uri = to_url(&params.text_document.uri);
        let items = self.compute_diagnostics(&uri);

        Ok(ls_types::DocumentDiagnosticReportResult::Report(
            ls_types::DocumentDiagnosticReport::Full(ls_types::RelatedFullDocumentDiagnosticReport {
                related_documents: None,
                full_document_diagnostic_report: ls_types::FullDocumentDiagnosticReport { result_id: None, items },
            }),
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

        let line = self
            .documents
            .get(&url.to_string())
            .and_then(|snapshot| {
                snapshot
                    .text
                    .split('\n')
                    .nth(position.line as usize)
                    .map(str::to_string)
            })
            .unwrap_or_default();

        Ok(hover::response(
            Arc::clone(&self.hir),
            to_url(&url),
            type_env,
            position,
            &line,
        ))
    }

    async fn signature_help(
        &self,
        params: ls_types::SignatureHelpParams,
    ) -> jsonrpc::Result<Option<ls_types::SignatureHelp>> {
        let url = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let source_text = self
            .documents
            .get(&url.to_string())
            .map(|snapshot| Arc::clone(&snapshot.text));

        Ok(signature_help::response(
            Arc::clone(&self.hir),
            to_url(&url),
            position,
            source_text.as_deref().map(String::as_str),
        ))
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

        let line_prefix = self
            .documents
            .get(&uri.to_string())
            .map(|snapshot| completions::line_prefix(&snapshot.text, position))
            .unwrap_or_default();

        let source_map_guard = self.source_map.read().unwrap();
        Ok(completions::response(
            Arc::clone(&self.hir),
            to_url(&uri),
            position,
            &source_map_guard,
            &line_prefix,
            &self.config.module_paths,
        ))
    }

    async fn execute_command(
        &self,
        params: ls_types::ExecuteCommandParams,
    ) -> jsonrpc::Result<Option<serde_json::Value>> {
        execute_command::response(params)
    }

    async fn code_action(
        &self,
        params: ls_types::CodeActionParams,
    ) -> jsonrpc::Result<Option<ls_types::CodeActionResponse>> {
        let url = to_url(&params.text_document.uri.clone());
        let uri_string = params.text_document.uri.to_string();

        let source_id = self.source_map.read().unwrap().get_by_left(&uri_string).copied();
        let source_text = self
            .documents
            .get(&uri_string)
            .map(|snapshot| Arc::clone(&snapshot.text));
        let lint_config = self.config.enable_lint.then(|| self.config.lint_config.clone());

        Ok(code_action::response(
            Arc::clone(&self.hir),
            url,
            params,
            source_id,
            lint_config.as_ref(),
            source_text.as_deref().map(String::as_str),
        ))
    }

    async fn rename(&self, params: ls_types::RenameParams) -> jsonrpc::Result<Option<ls_types::WorkspaceEdit>> {
        let url = to_url(&params.text_document_position.text_document.uri);
        let position = params.text_document_position.position;

        let source_map_guard = self.source_map.read().unwrap();
        rename::response(
            Arc::clone(&self.hir),
            url,
            position,
            &params.new_name,
            &source_map_guard,
        )
        .map_err(jsonrpc::Error::invalid_params)
    }

    async fn prepare_rename(
        &self,
        params: ls_types::TextDocumentPositionParams,
    ) -> jsonrpc::Result<Option<ls_types::PrepareRenameResponse>> {
        let url = to_url(&params.text_document.uri);
        Ok(rename::prepare(Arc::clone(&self.hir), url, params.position))
    }

    async fn formatting(
        &self,
        params: ls_types::DocumentFormattingParams,
    ) -> jsonrpc::Result<Option<Vec<ls_types::TextEdit>>> {
        if self
            .error_map
            .get(&params.text_document.uri.to_string())
            .unwrap()
            .iter()
            .any(|e| matches!(e, LspError::SyntaxError(_)))
        {
            return Ok(None);
        }

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

        let uri_string = params.text_document.uri.to_string();
        let snapshot = Arc::clone(self.documents.get(&uri_string).unwrap().value());
        let text = Arc::clone(&snapshot.text);
        let nodes = snapshot.nodes.clone();
        // Replace the whole original text, not the formatted length.
        let end = document_end_position(&text);
        let formatted_text = tokio::task::spawn_blocking(move || {
            let mut formatter = mq_formatter::Formatter::new(Some(mq_formatter::FormatterConfig {
                indent_width: 2,
                ..Default::default()
            }));
            match nodes {
                Some(nodes) => formatter.format_with_cst(&mut Vec::clone(&nodes)),
                None => formatter.format(&text),
            }
        })
        .await
        .map_err(|_| jsonrpc::Error::new(jsonrpc::ErrorCode::InternalError))?
        .map_err(|_| jsonrpc::Error::new(jsonrpc::ErrorCode::ParseError))?;

        // End work done progress
        if let Some(progress) = progress {
            progress.finish_with_message("Formatting complete").await;
        }

        Ok(Some(vec![ls_types::TextEdit {
            range: ls_types::Range::new(ls_types::Position::new(0, 0), end),
            new_text: formatted_text,
        }]))
    }

    async fn range_formatting(
        &self,
        params: DocumentRangeFormattingParams,
    ) -> jsonrpc::Result<Option<Vec<ls_types::TextEdit>>> {
        if let Some(errors) = self.error_map.get(&params.text_document.uri.to_string())
            && !(*errors).is_empty()
        {
            return Ok(None);
        }

        let text = if let Some(snapshot) = self.documents.get(&params.text_document.uri.to_string()) {
            Arc::clone(&snapshot.text)
        } else {
            return Ok(None);
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
        let (nodes, errors) = if text.is_empty() {
            (Vec::new(), mq_lang::CstErrorReporter::default())
        } else {
            mq_lang::parse_recovery(&text)
        };
        let (source_id, _) = self.hir.write().unwrap().add_nodes(uri.clone(), &nodes);
        let nodes = Arc::new(nodes);

        let uri_string = uri.to_string();
        let mut errors = errors
            .diagnostics(&text)
            .into_iter()
            .map(LspError::SyntaxError)
            .collect::<Vec<_>>();

        if errors.is_empty() && self.config.enable_type_checking {
            let hir_guard = self.hir.read().unwrap();
            let mut checker = mq_check::TypeChecker::with_options(self.config.type_checker_options.clone());
            let type_errors = checker.check_classified(&hir_guard);

            // Build a set of text ranges from the current source's symbols
            // so that type errors originating from other sources (e.g., pre-loaded modules)
            // are not incorrectly attributed to this file.
            let source_locations: FxHashSet<mq_lang::Range> = hir_guard
                .symbols_for_source(source_id)
                .filter_map(|(_, symbol)| symbol.source.text_range)
                .collect();

            self.type_env_map
                .insert(uri_string.clone(), checker.symbol_types().clone());
            errors.extend(
                type_errors
                    .into_iter()
                    .filter(|c| {
                        c.error
                            .location()
                            .map(|range| source_locations.contains(&range))
                            .unwrap_or(false)
                    })
                    .filter_map(|c| {
                        let options = &self.config.type_checker_options;
                        match c.severity(options)? {
                            mq_check::Severity::Error => Some(LspError::TypeError(c.error, c.related)),
                            mq_check::Severity::Warning => Some(LspError::UncertainTypeError(c.error, c.related)),
                        }
                    }),
            );
        }

        self.source_map.write().unwrap().insert(uri_string.clone(), source_id);
        self.documents.insert(
            uri_string.clone(),
            Arc::new(DocumentSnapshot {
                text: Arc::new(text),
                nodes: Some(nodes),
            }),
        );
        self.error_map.insert(uri_string, errors);
    }

    /// Computes the full diagnostic set for a document: cached parse/type errors from
    /// `error_map`, HIR errors/warnings (e.g. unreachable code), and, when enabled, lint
    /// diagnostics. Shared by the push (`publish_diagnostics`) and pull
    /// (`textDocument/diagnostic`) flows so they can't drift apart.
    fn compute_diagnostics(&self, uri: &Url) -> Vec<ls_types::Diagnostic> {
        let uri_string = uri.to_string();

        // Get errors for this specific file
        let file_errors = self.error_map.get(&uri_string);
        let mut diagnostics = Vec::new();

        // Add parsing errors if they exist
        if let Some(errors) = file_errors {
            let document = to_uri(uri);
            diagnostics.extend(errors.iter().map(|error| error.to_diagnostic(&document)));
        }

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

            if self.config.enable_lint {
                let lint_ctx = mq_lint::LintContext::new(&hir_guard, *source_id, &self.config.lint_config);
                let linter = mq_lint::Linter::with_default_rules();
                diagnostics.extend(
                    linter
                        .run(&lint_ctx)
                        .into_iter()
                        .map(|d| (&LspError::LintWarning(d)).into()),
                );
            }
        }

        diagnostics
    }

    async fn diagnostics(&self, uri: Url, version: Option<i32>) {
        let diagnostics = self.compute_diagnostics(&uri);

        self.client
            .publish_diagnostics(to_uri(&uri), diagnostics, version)
            .await;
    }
}

#[derive(Debug, Clone, Default)]
/// Language server settings.
pub struct LspConfig {
    module_paths: Vec<PathBuf>,
    enable_type_checking: bool,
    type_checker_options: mq_check::TypeCheckerOptions,
    enable_lint: bool,
    lint_config: mq_lint::LintConfig,
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
    /// * `enable_lint` - Whether to also publish `mq-lint` diagnostics.
    /// * `lint_config` - Per-rule lint configuration, used when `enable_lint` is `true`.
    pub fn new(
        module_paths: Vec<PathBuf>,
        enable_type_checking: bool,
        type_checker_options: mq_check::TypeCheckerOptions,
        enable_lint: bool,
        lint_config: mq_lint::LintConfig,
    ) -> Self {
        Self {
            module_paths,
            enable_type_checking,
            type_checker_options,
            enable_lint,
            lint_config,
        }
    }
}

/// Parameters of the `mq/virtualDocument` request.
#[derive(Debug, serde::Deserialize)]
pub struct VirtualDocumentParams {
    /// URI of a document returned by go-to-definition, e.g. `mq-std:/builtin.mq`.
    pub uri: String,
}

impl Backend {
    /// Handles `mq/virtualDocument`: the text of `builtin.mq` or a standard module.
    async fn virtual_document(&self, params: VirtualDocumentParams) -> jsonrpc::Result<Option<String>> {
        Ok(virtual_document::content(&params.uri))
    }
}

/// Runs the language server on stdio.
pub async fn start(config: LspConfig) {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let resolver = mq_lang::DefaultModuleResolver::new(config.module_paths.clone());
    let module_loader = mq_lang::ModuleLoader::new(resolver);

    let (service, socket) = LspService::build(|client| Backend {
        client,
        hir: Arc::new(RwLock::new(mq_hir::Hir::new(module_loader))),
        source_map: RwLock::new(BiMap::new()),
        type_env_map: DashMap::new(),
        error_map: DashMap::new(),
        documents: DashMap::new(),
        config,
    })
    .custom_method("mq/virtualDocument", Backend::virtual_document)
    .finish();

    Server::new(stdin, stdout, socket).serve(service).await;
}

#[cfg(test)]
mod tests;
