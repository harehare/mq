use tower_lsp_server::ls_types::{
    CodeActionKind, CodeActionOptions, CodeActionProviderCapability, CompletionOptions, DiagnosticOptions,
    DiagnosticServerCapabilities, DocumentFormattingOptions, DocumentRangeFormattingOptions, ExecuteCommandOptions,
    HoverProviderCapability, InlayHintOptions, InlayHintServerCapabilities, OneOf, RenameOptions,
    SemanticTokensFullOptions, SemanticTokensLegend, SemanticTokensOptions, SemanticTokensServerCapabilities,
    ServerCapabilities, SignatureHelpOptions, TextDocumentSyncCapability, TextDocumentSyncKind,
};

use crate::semantic_tokens;

pub(crate) fn server_capabilities() -> ServerCapabilities {
    ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        signature_help_provider: Some(SignatureHelpOptions {
            trigger_characters: Some(vec!["(".to_string(), ",".to_string()]),
            retrigger_characters: None,
            work_done_progress_options: Default::default(),
        }),
        completion_provider: Some(CompletionOptions {
            trigger_characters: Some(vec!["|".to_string(), ":".to_string(), ".".to_string()]),
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
        document_formatting_provider: Some(OneOf::Right(DocumentFormattingOptions {
            work_done_progress_options: tower_lsp_server::ls_types::WorkDoneProgressOptions {
                work_done_progress: Some(true),
            },
        })),
        document_range_formatting_provider: Some(OneOf::Right(DocumentRangeFormattingOptions {
            work_done_progress_options: tower_lsp_server::ls_types::WorkDoneProgressOptions {
                work_done_progress: Some(true),
            },
        })),
        document_symbol_provider: Some(OneOf::Left(true)),
        workspace_symbol_provider: Some(OneOf::Left(true)),
        diagnostic_provider: Some(DiagnosticServerCapabilities::Options(DiagnosticOptions {
            identifier: None,
            // Editing a module/import can change diagnostics in files that depend on it.
            inter_file_dependencies: true,
            workspace_diagnostics: false,
            work_done_progress_options: Default::default(),
        })),
        definition_provider: Some(OneOf::Left(true)),
        references_provider: Some(OneOf::Left(true)),
        code_action_provider: Some(CodeActionProviderCapability::Options(CodeActionOptions {
            code_action_kinds: Some(vec![CodeActionKind::QUICKFIX]),
            resolve_provider: Some(false),
            ..Default::default()
        })),
        rename_provider: Some(OneOf::Right(RenameOptions {
            prepare_provider: Some(false),
            work_done_progress_options: Default::default(),
        })),
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
        inlay_hint_provider: Some(OneOf::Right(InlayHintServerCapabilities::Options(InlayHintOptions {
            resolve_provider: Some(false),
            ..Default::default()
        }))),
        ..Default::default()
    }
}
