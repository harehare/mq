use tower_lsp::lsp_types::{
    CompletionOptions, ExecuteCommandOptions, HoverProviderCapability, OneOf,
    SemanticTokensFullOptions, SemanticTokensLegend, SemanticTokensOptions,
    SemanticTokensServerCapabilities, ServerCapabilities, TextDocumentSyncCapability,
    TextDocumentSyncKind,
};

use crate::semantic_tokens;

pub fn server_capabilities() -> ServerCapabilities {
    ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        completion_provider: Some(CompletionOptions {
            trigger_characters: Some(vec![" ".to_string(), "|".to_string()]),
            ..Default::default()
        }),
        execute_command_provider: Some(ExecuteCommandOptions {
            commands: vec![
                "mq/runSelectedText".to_string(),
                "mq/setSelectedTextAsInput".to_string(),
                "mq/showInputText".to_string(),
            ],
            ..Default::default()
        }),
        document_formatting_provider: Some(OneOf::Left(true)),
        document_symbol_provider: Some(OneOf::Left(true)),
        definition_provider: Some(OneOf::Left(true)),
        references_provider: Some(OneOf::Left(true)),
        semantic_tokens_provider: Some(SemanticTokensServerCapabilities::SemanticTokensOptions(
            SemanticTokensOptions {
                legend: SemanticTokensLegend {
                    token_types: semantic_tokens::TOKEN_TYPE.to_vec(),
                    token_modifiers: semantic_tokens::TOKEN_MODIFIER.to_vec(),
                },
                full: Some(SemanticTokensFullOptions::Bool(true)),
                range: Some(true),
                ..Default::default()
            },
        )),
        ..Default::default()
    }
}
