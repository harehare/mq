use std::sync::{Arc, RwLock};

use bimap::BiMap;
use dashmap::DashMap;
use itertools::Itertools;
use mq_lsp::capabilities;
use mq_lsp::completions;
use mq_lsp::document_symbol;
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
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
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
            .to_vec()
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
            .collect_vec();

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
                .collect_vec(),
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
    });

    Server::new(stdin, stdout, socket).serve(service).await;
}
