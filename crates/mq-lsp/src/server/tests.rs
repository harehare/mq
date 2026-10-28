use tower_lsp_server::ls_types::{self, TextDocumentIdentifier};

use super::*;

#[test]
fn test_document_end_position() {
    for (text, expected) in [
        ("", (0, 0)),
        ("abc", (0, 3)),
        ("abc\n", (1, 0)),
        ("a\nbc", (1, 2)),
        ("a\n\n", (2, 0)),
        ("a\n🦀x", (1, 3)),
    ] {
        assert_eq!(
            document_end_position(text),
            ls_types::Position::new(expected.0, expected.1),
            "text: {text:?}"
        );
    }
}

#[tokio::test]
async fn test_formatting_edit_covers_removed_trailing_lines() {
    let (service, _) = LspService::new(|client| Backend {
        client,
        hir: Arc::new(RwLock::new(mq_hir::Hir::default())),
        source_map: RwLock::new(BiMap::new()),
        type_env_map: DashMap::new(),
        error_map: DashMap::new(),
        documents: DashMap::new(),
        config: LspConfig::default(),
    });

    let backend = service.inner();
    let uri = Url::parse("file:///shrink.mq").unwrap();
    // Formatting removes lines here.
    let text = "def f(x):\n  if (x): 1\n  else: 2\n  ;\nend\n\n\n";
    backend.on_change(uri.clone(), text.to_string()).await;

    let edits = backend
        .formatting(ls_types::DocumentFormattingParams {
            text_document: ls_types::TextDocumentIdentifier { uri: to_uri(&uri) },
            options: Default::default(),
            work_done_progress_params: Default::default(),
        })
        .await
        .unwrap()
        .unwrap();

    assert_eq!(edits.len(), 1);
    assert!(edits[0].new_text.lines().count() < text.lines().count());
    assert_eq!(edits[0].range.start, ls_types::Position::new(0, 0));
    assert_eq!(edits[0].range.end, document_end_position(text));
}

#[tokio::test]
async fn test_did_open() {
    let (service, _) = LspService::new(|client| Backend {
        client,
        hir: Arc::new(RwLock::new(mq_hir::Hir::default())),
        source_map: RwLock::new(BiMap::new()),
        type_env_map: DashMap::new(),
        error_map: DashMap::new(),
        documents: DashMap::new(),
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
    assert!(backend.error_map.get(&uri.to_string()).unwrap().is_empty());
}

#[tokio::test]
async fn test_formatting_and_folding_use_cached_cst() {
    let (service, _) = LspService::new(|client| Backend {
        client,
        hir: Arc::new(RwLock::new(mq_hir::Hir::default())),
        source_map: RwLock::new(BiMap::new()),
        type_env_map: DashMap::new(),
        error_map: DashMap::new(),
        documents: DashMap::new(),
        config: LspConfig::default(),
    });

    let backend = service.inner();
    let uri = Url::parse("file:///test.mq").unwrap();
    let text = "def main():\n  if(true):1 else:2;\n| main()";
    backend.on_change(uri.clone(), text.to_string()).await;
    assert!(backend.documents.contains_key(&uri.to_string()));

    let edits = backend
        .formatting(ls_types::DocumentFormattingParams {
            text_document: ls_types::TextDocumentIdentifier { uri: to_uri(&uri) },
            options: Default::default(),
            work_done_progress_params: Default::default(),
        })
        .await
        .unwrap()
        .unwrap();
    let expected = mq_formatter::Formatter::new(Some(mq_formatter::FormatterConfig {
        indent_width: 2,
        ..Default::default()
    }))
    .format(text)
    .unwrap();
    assert_eq!(edits[0].new_text, expected);
    assert_eq!(edits[0].range.end, document_end_position(text));

    let folding = backend
        .folding_range(ls_types::FoldingRangeParams {
            text_document: ls_types::TextDocumentIdentifier { uri: to_uri(&uri) },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .await
        .unwrap();
    assert_eq!(folding, folding_range::response(Some(text)));

    // A later change replaces the cached CST.
    let changed = "def main():\n  1;\n| def other():\n  2;";
    backend.on_change(uri.clone(), changed.to_string()).await;
    let folding = backend
        .folding_range(ls_types::FoldingRangeParams {
            text_document: ls_types::TextDocumentIdentifier { uri: to_uri(&uri) },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .await
        .unwrap();
    assert_eq!(folding, folding_range::response(Some(changed)));
    assert_eq!(folding.map(|ranges| ranges.len()), Some(2));

    backend
        .did_close(ls_types::DidCloseTextDocumentParams {
            text_document: ls_types::TextDocumentIdentifier { uri: to_uri(&uri) },
        })
        .await;
    assert!(!backend.documents.contains_key(&uri.to_string()));
}

#[tokio::test]
async fn test_formatting() {
    let (service, _) = LspService::new(|client| Backend {
        client,
        hir: Arc::new(RwLock::new(mq_hir::Hir::default())),
        source_map: RwLock::new(BiMap::new()),
        type_env_map: DashMap::new(),
        error_map: DashMap::new(),
        documents: DashMap::new(),
        config: LspConfig::default(),
    });

    let backend = service.inner();
    let uri = Url::parse("file:///test.mq").unwrap();
    let text = "def main():1;";

    let (_, errors) = mq_lang::parse_recovery(text);
    backend.documents.insert(
        uri.to_string(),
        Arc::new(DocumentSnapshot {
            text: Arc::new(text.to_string()),
            nodes: None,
        }),
    );
    backend.error_map.insert(
        uri.to_string(),
        errors
            .diagnostics(text)
            .into_iter()
            .map(LspError::SyntaxError)
            .collect(),
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
        error_map: DashMap::new(),
        documents: DashMap::new(),
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
        error_map: DashMap::new(),
        documents: DashMap::new(),
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
        error_map: DashMap::new(),
        documents: DashMap::new(),
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
async fn test_signature_help() {
    let (service, _) = LspService::new(|client| Backend {
        client,
        hir: Arc::new(RwLock::new(mq_hir::Hir::default())),
        source_map: RwLock::new(BiMap::new()),
        type_env_map: DashMap::new(),
        error_map: DashMap::new(),
        documents: DashMap::new(),
        config: LspConfig::default(),
    });

    let backend = service.inner();
    let uri = Url::parse("file:///test.mq").unwrap();
    let code = "def foo(a, b): a + b; | foo(1, 2)";

    backend
        .did_open(ls_types::DidOpenTextDocumentParams {
            text_document: ls_types::TextDocumentItem {
                uri: to_uri(&uri),
                language_id: "mq".to_string(),
                version: 1,
                text: code.to_string(),
            },
        })
        .await;

    // Cursor right before the `2` argument.
    let result = backend
        .signature_help(ls_types::SignatureHelpParams {
            text_document_position_params: ls_types::TextDocumentPositionParams {
                text_document: ls_types::TextDocumentIdentifier { uri: to_uri(&uri) },
                position: ls_types::Position::new(0, 31),
            },
            work_done_progress_params: Default::default(),
            context: None,
        })
        .await;

    assert!(result.is_ok());
    let help = result.unwrap().unwrap();

    assert_eq!(help.signatures.len(), 1);
    assert_eq!(help.signatures[0].label, "foo(a, b)");
    assert_eq!(help.active_parameter, Some(1));
}

#[tokio::test]
async fn test_folding_range() {
    let (service, _) = LspService::new(|client| Backend {
        client,
        hir: Arc::new(RwLock::new(mq_hir::Hir::default())),
        source_map: RwLock::new(BiMap::new()),
        type_env_map: DashMap::new(),
        error_map: DashMap::new(),
        documents: DashMap::new(),
        config: LspConfig::default(),
    });

    let backend = service.inner();
    let uri = Url::parse("file:///test.mq").unwrap();
    let code = "def foo(a):\n  let b = a + 1\n  | b;\n| foo(1)";

    backend
        .did_open(ls_types::DidOpenTextDocumentParams {
            text_document: ls_types::TextDocumentItem {
                uri: to_uri(&uri),
                language_id: "mq".to_string(),
                version: 1,
                text: code.to_string(),
            },
        })
        .await;

    let result = backend
        .folding_range(ls_types::FoldingRangeParams {
            text_document: ls_types::TextDocumentIdentifier { uri: to_uri(&uri) },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .await;

    assert!(result.is_ok());
    let ranges = result.unwrap().unwrap();

    assert!(
        ranges
            .iter()
            .any(|r| r.kind == Some(ls_types::FoldingRangeKind::Region) && r.start_line == 0 && r.end_line == 2)
    );
}

#[tokio::test]
async fn test_semantic_tokens() {
    let (service, _) = LspService::new(|client| Backend {
        client,
        hir: Arc::new(RwLock::new(mq_hir::Hir::default())),
        source_map: RwLock::new(BiMap::new()),
        type_env_map: DashMap::new(),
        error_map: DashMap::new(),
        documents: DashMap::new(),
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
        error_map: DashMap::new(),
        documents: DashMap::new(),
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
async fn test_code_action_suggests_include() {
    let (service, _) = LspService::new(|client| Backend {
        client,
        hir: Arc::new(RwLock::new(mq_hir::Hir::default())),
        source_map: RwLock::new(BiMap::new()),
        type_env_map: DashMap::new(),
        error_map: DashMap::new(),
        documents: DashMap::new(),
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
                text: "csv_parse(\"a,b\", false)".to_string(),
            },
        })
        .await;

    let range = ls_types::Range::new(ls_types::Position::new(0, 0), ls_types::Position::new(0, 9));
    let result = backend
        .code_action(ls_types::CodeActionParams {
            text_document: TextDocumentIdentifier { uri: to_uri(&uri) },
            range,
            context: ls_types::CodeActionContext {
                diagnostics: vec![ls_types::Diagnostic::new_simple(
                    range,
                    "Unresolved symbol: csv_parse".to_string(),
                )],
                only: None,
                trigger_kind: None,
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .await;

    assert!(result.is_ok());
    let actions = result.unwrap().unwrap();
    assert!(actions.iter().any(|action| match action {
        ls_types::CodeActionOrCommand::CodeAction(action) => action.title.contains("csv"),
        _ => false,
    }));
}

#[tokio::test]
async fn test_code_action_using_real_diagnostics_from_pipeline() {
    let (service, _) = LspService::new(|client| Backend {
        client,
        hir: Arc::new(RwLock::new(mq_hir::Hir::default())),
        source_map: RwLock::new(BiMap::new()),
        type_env_map: DashMap::new(),
        error_map: DashMap::new(),
        documents: DashMap::new(),
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
                text: "json_parse(\"{}\")".to_string(),
            },
        })
        .await;

    // Build the diagnostic the exact same way `diagnostics()` does, instead of
    // hand-computing a range, so this test exercises the real HIR error pipeline.
    let diagnostics: Vec<ls_types::Diagnostic> = {
        let hir_guard = backend.hir.read().unwrap();
        hir_guard
            .error_ranges()
            .into_iter()
            .map(|(message, item)| {
                ls_types::Diagnostic::new_simple(
                    ls_types::Range::new(
                        ls_types::Position::new(item.start.line - 1, (item.start.column - 1) as u32),
                        ls_types::Position::new(item.end.line - 1, (item.end.column - 1) as u32),
                    ),
                    message,
                )
            })
            .collect()
    };
    assert_eq!(
        diagnostics.len(),
        1,
        "expected exactly one unresolved-symbol diagnostic"
    );

    let result = backend
        .code_action(ls_types::CodeActionParams {
            text_document: TextDocumentIdentifier { uri: to_uri(&uri) },
            range: diagnostics[0].range,
            context: ls_types::CodeActionContext {
                diagnostics,
                only: None,
                trigger_kind: None,
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .await;

    assert!(result.is_ok());
    let actions = result.unwrap().unwrap();
    assert!(actions.iter().any(|action| match action {
        ls_types::CodeActionOrCommand::CodeAction(action) => action.title.contains("json"),
        _ => false,
    }));
}

#[tokio::test]
async fn test_code_action_no_actions_for_clean_file() {
    let (service, _) = LspService::new(|client| Backend {
        client,
        hir: Arc::new(RwLock::new(mq_hir::Hir::default())),
        source_map: RwLock::new(BiMap::new()),
        type_env_map: DashMap::new(),
        error_map: DashMap::new(),
        documents: DashMap::new(),
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

    let result = backend
        .code_action(ls_types::CodeActionParams {
            text_document: TextDocumentIdentifier { uri: to_uri(&uri) },
            range: ls_types::Range::new(ls_types::Position::new(0, 0), ls_types::Position::new(0, 0)),
            context: ls_types::CodeActionContext {
                diagnostics: vec![],
                only: None,
                trigger_kind: None,
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .await;

    assert!(result.is_ok());
    assert!(result.unwrap().is_none());
}

#[test]
fn test_capabilities_advertise_code_action_and_rename_shape() {
    let capabilities = capabilities::server_capabilities();

    match capabilities.code_action_provider {
        Some(ls_types::CodeActionProviderCapability::Options(options)) => {
            assert_eq!(
                options.code_action_kinds,
                Some(vec![
                    ls_types::CodeActionKind::QUICKFIX,
                    ls_types::CodeActionKind::REFACTOR_EXTRACT,
                    ls_types::CodeActionKind::REFACTOR_INLINE,
                ])
            );
        }
        other => panic!("expected CodeActionProviderCapability::Options, got {other:?}"),
    }

    match capabilities.rename_provider {
        Some(ls_types::OneOf::Right(options)) => {
            assert_eq!(options.prepare_provider, Some(true));
        }
        other => panic!("expected rename_provider OneOf::Right(RenameOptions), got {other:?}"),
    }
}

#[tokio::test]
async fn test_rename() {
    let (service, _) = LspService::new(|client| Backend {
        client,
        hir: Arc::new(RwLock::new(mq_hir::Hir::default())),
        source_map: RwLock::new(BiMap::new()),
        type_env_map: DashMap::new(),
        error_map: DashMap::new(),
        documents: DashMap::new(),
        config: LspConfig::default(),
    });

    let backend = service.inner();
    let uri = Url::parse("file:///test.mq").unwrap();

    let code = "def test_func(): 1;\ndef main(): test_func();";
    let (nodes, _) = mq_lang::parse_recovery(code);
    let (source_id, _) = backend.hir.write().unwrap().add_nodes(uri.clone(), &nodes);

    backend.source_map.write().unwrap().insert(uri.to_string(), source_id);

    let result = backend
        .rename(ls_types::RenameParams {
            text_document_position: ls_types::TextDocumentPositionParams {
                text_document: ls_types::TextDocumentIdentifier { uri: to_uri(&uri) },
                position: ls_types::Position::new(0, 6),
            },
            new_name: "renamed_func".to_string(),
            work_done_progress_params: Default::default(),
        })
        .await;

    assert!(result.is_ok());
    let edit = result.unwrap().unwrap();
    let edits = edit.changes.unwrap().into_values().next().unwrap();
    assert_eq!(edits.len(), 2);
    assert!(edits.iter().all(|e| e.new_text == "renamed_func"));
}

#[tokio::test]
async fn test_prepare_rename_valid_symbol() {
    let (service, _) = LspService::new(|client| Backend {
        client,
        hir: Arc::new(RwLock::new(mq_hir::Hir::default())),
        source_map: RwLock::new(BiMap::new()),
        type_env_map: DashMap::new(),
        error_map: DashMap::new(),
        documents: DashMap::new(),
        config: LspConfig::default(),
    });

    let backend = service.inner();
    let uri = Url::parse("file:///test.mq").unwrap();

    let code = "def test_func(): 1;\ndef main(): test_func();";
    let (nodes, _) = mq_lang::parse_recovery(code);
    let (source_id, _) = backend.hir.write().unwrap().add_nodes(uri.clone(), &nodes);

    backend.source_map.write().unwrap().insert(uri.to_string(), source_id);

    let result = backend
        .prepare_rename(ls_types::TextDocumentPositionParams {
            text_document: ls_types::TextDocumentIdentifier { uri: to_uri(&uri) },
            position: ls_types::Position::new(0, 6),
        })
        .await;

    assert!(result.is_ok());
    match result.unwrap() {
        Some(ls_types::PrepareRenameResponse::RangeWithPlaceholder { placeholder, .. }) => {
            assert_eq!(placeholder, "test_func");
        }
        other => panic!("expected PrepareRenameResponse::RangeWithPlaceholder, got {other:?}"),
    }
}

#[tokio::test]
async fn test_prepare_rename_rejects_builtin() {
    let (service, _) = LspService::new(|client| Backend {
        client,
        hir: Arc::new(RwLock::new(mq_hir::Hir::default())),
        source_map: RwLock::new(BiMap::new()),
        type_env_map: DashMap::new(),
        error_map: DashMap::new(),
        documents: DashMap::new(),
        config: LspConfig::default(),
    });

    let backend = service.inner();
    let uri = Url::parse("file:///test.mq").unwrap();

    let code = "\"hello\" | len";
    let (nodes, _) = mq_lang::parse_recovery(code);
    let (source_id, _) = backend.hir.write().unwrap().add_nodes(uri.clone(), &nodes);

    backend.source_map.write().unwrap().insert(uri.to_string(), source_id);

    let result = backend
        .prepare_rename(ls_types::TextDocumentPositionParams {
            text_document: ls_types::TextDocumentIdentifier { uri: to_uri(&uri) },
            position: ls_types::Position::new(0, 11),
        })
        .await;

    assert!(result.is_ok());
    assert!(result.unwrap().is_none());
}

#[tokio::test]
async fn test_rename_across_two_tracked_sources() {
    let (service, _) = LspService::new(|client| Backend {
        client,
        hir: Arc::new(RwLock::new(mq_hir::Hir::default())),
        source_map: RwLock::new(BiMap::new()),
        type_env_map: DashMap::new(),
        error_map: DashMap::new(),
        documents: DashMap::new(),
        config: LspConfig::default(),
    });

    let backend = service.inner();
    let uri = Url::parse("file:///main.mq").unwrap();

    backend
        .did_open(ls_types::DidOpenTextDocumentParams {
            text_document: ls_types::TextDocumentItem {
                uri: to_uri(&uri),
                language_id: "mq".to_string(),
                version: 1,
                text: "include \"csv\" | csv_parse(\"a,b\", false)".to_string(),
            },
        })
        .await;

    // `did_open` only registers the opened file's own URI in `source_map`. The
    // included module gets lowered into its own HIR source/url too (see
    // hir/lower.rs::add_include_expr); track it here the same way a client would
    // if it had that module file open as well.
    let module_source_id = {
        let hir_guard = backend.hir.read().unwrap();
        hir_guard
            .symbols()
            .find_map(|(_, symbol)| match symbol.kind {
                mq_hir::SymbolKind::Include(module_source_id) => Some(module_source_id),
                _ => None,
            })
            .expect("expected an Include symbol for the csv module")
    };
    let module_url = backend
        .hir
        .read()
        .unwrap()
        .url_by_source(&module_source_id)
        .unwrap()
        .clone();
    backend
        .source_map
        .write()
        .unwrap()
        .insert(module_url.to_string(), module_source_id);

    let result = backend
        .rename(ls_types::RenameParams {
            text_document_position: ls_types::TextDocumentPositionParams {
                text_document: ls_types::TextDocumentIdentifier { uri: to_uri(&uri) },
                position: ls_types::Position::new(0, 18),
            },
            new_name: "csv_load".to_string(),
            work_done_progress_params: Default::default(),
        })
        .await;

    assert!(result.is_ok());
    let changes = result.unwrap().unwrap().changes.unwrap();
    assert_eq!(
        changes.len(),
        2,
        "expected edits in both the main file and the csv module source"
    );
    for edits in changes.values() {
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].new_text, "csv_load");
    }
}

#[tokio::test]
async fn test_document_symbol() {
    let (service, _) = LspService::new(|client| Backend {
        client,
        hir: Arc::new(RwLock::new(mq_hir::Hir::default())),
        source_map: RwLock::new(BiMap::new()),
        type_env_map: DashMap::new(),
        error_map: DashMap::new(),
        documents: DashMap::new(),
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
async fn test_workspace_symbol() {
    let (service, _) = LspService::new(|client| Backend {
        client,
        hir: Arc::new(RwLock::new(mq_hir::Hir::default())),
        source_map: RwLock::new(BiMap::new()),
        type_env_map: DashMap::new(),
        error_map: DashMap::new(),
        documents: DashMap::new(),
        config: LspConfig::default(),
    });

    let backend = service.inner();

    let uri_a = Url::parse("file:///a.mq").unwrap();
    let code_a = "def test_func(): 1;";
    let (nodes_a, _) = mq_lang::parse_recovery(code_a);
    let (source_id_a, _) = backend.hir.write().unwrap().add_nodes(uri_a.clone(), &nodes_a);
    backend
        .source_map
        .write()
        .unwrap()
        .insert(uri_a.to_string(), source_id_a);

    let uri_b = Url::parse("file:///b.mq").unwrap();
    let code_b = "def other_func(): 2;";
    let (nodes_b, _) = mq_lang::parse_recovery(code_b);
    let (source_id_b, _) = backend.hir.write().unwrap().add_nodes(uri_b.clone(), &nodes_b);
    backend
        .source_map
        .write()
        .unwrap()
        .insert(uri_b.to_string(), source_id_b);

    let result = backend
        .symbol(ls_types::WorkspaceSymbolParams {
            query: "test_func".to_string(),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .await;

    assert!(result.is_ok());
    let symbols = result.unwrap().unwrap();

    if let ls_types::WorkspaceSymbolResponse::Nested(symbols) = symbols {
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "test_func");
    } else {
        panic!("Expected nested workspace symbol response");
    }

    let result = backend
        .symbol(ls_types::WorkspaceSymbolParams {
            query: "func".to_string(),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .await;

    assert!(result.is_ok());
    let symbols = result.unwrap().unwrap();

    if let ls_types::WorkspaceSymbolResponse::Nested(symbols) = symbols {
        let symbol_names: Vec<String> = symbols.iter().map(|s| s.name.clone()).collect();
        assert!(symbol_names.contains(&"test_func".to_string()));
        assert!(symbol_names.contains(&"other_func".to_string()));
        assert_eq!(symbols.len(), 2);
    } else {
        panic!("Expected nested workspace symbol response");
    }
}

#[tokio::test]
async fn test_did_change() {
    let (service, _) = LspService::new(|client| Backend {
        client,
        hir: Arc::new(RwLock::new(mq_hir::Hir::default())),
        source_map: RwLock::new(BiMap::new()),
        type_env_map: DashMap::new(),
        error_map: DashMap::new(),
        documents: DashMap::new(),
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
    let text = backend.documents.get(&uri.to_string());
    assert!(text.is_some());
}

#[tokio::test]
async fn test_did_close() {
    let (service, _) = LspService::new(|client| Backend {
        client,
        hir: Arc::new(RwLock::new(mq_hir::Hir::default())),
        source_map: RwLock::new(BiMap::new()),
        type_env_map: DashMap::new(),
        error_map: DashMap::new(),
        documents: DashMap::new(),
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
    assert!(backend.error_map.contains_key(&uri.to_string()));
    assert!(backend.documents.contains_key(&uri.to_string()));
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
    assert!(!backend.documents.contains_key(&uri.to_string()));
    assert!(!backend.source_map.read().unwrap().contains_left(&uri.to_string()));
}

#[tokio::test]
async fn test_did_save() {
    let (service, _) = LspService::new(|client| Backend {
        client,
        hir: Arc::new(RwLock::new(mq_hir::Hir::default())),
        source_map: RwLock::new(BiMap::new()),
        type_env_map: DashMap::new(),
        error_map: DashMap::new(),
        documents: DashMap::new(),
        config: LspConfig::default(),
    });

    let backend = service.inner();
    let uri = Url::parse("file:///test.mq").unwrap();

    // Setup some content with errors
    let text = "def main(): invalid_syntax";
    let (_, errors) = mq_lang::parse_recovery(text);
    backend.documents.insert(
        uri.to_string(),
        Arc::new(DocumentSnapshot {
            text: Arc::new(text.to_string()),
            nodes: None,
        }),
    );
    backend.error_map.insert(
        uri.to_string(),
        errors
            .diagnostics(text)
            .into_iter()
            .map(LspError::SyntaxError)
            .collect(),
    );

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
        error_map: DashMap::new(),
        documents: DashMap::new(),
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
    assert!(capabilities.code_action_provider.is_some());
    assert!(capabilities.rename_provider.is_some());
    assert!(capabilities.document_symbol_provider.is_some());
    assert!(capabilities.workspace_symbol_provider.is_some());
    assert!(capabilities.signature_help_provider.is_some());
    assert!(capabilities.diagnostic_provider.is_some());
    assert!(capabilities.folding_range_provider.is_some());

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
        error_map: DashMap::new(),
        documents: DashMap::new(),
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
        error_map: DashMap::new(),
        documents: DashMap::new(),
        config: LspConfig::default(),
    });

    let backend = service.inner();
    let uri = Url::parse("file:///test.mq").unwrap();

    // Add content with parsing errors
    let text = "def main() 1;"; // Missing colon
    let (_, errors) = mq_lang::parse_recovery(text);
    backend.documents.insert(
        uri.to_string(),
        Arc::new(DocumentSnapshot {
            text: Arc::new(text.to_string()),
            nodes: None,
        }),
    );
    backend.error_map.insert(
        uri.to_string(),
        errors
            .diagnostics(text)
            .into_iter()
            .map(LspError::SyntaxError)
            .collect(),
    );

    // We can't directly test client.publish_diagnostics was called with correct diagnostics,
    // but we can verify the code doesn't panic
    backend.diagnostics(uri, None).await;
}

#[tokio::test]
async fn test_pull_diagnostics_returns_syntax_errors() {
    let (service, _) = LspService::new(|client| Backend {
        client,
        hir: Arc::new(RwLock::new(mq_hir::Hir::default())),
        source_map: RwLock::new(BiMap::new()),
        type_env_map: DashMap::new(),
        error_map: DashMap::new(),
        documents: DashMap::new(),
        config: LspConfig::default(),
    });

    let backend = service.inner();
    let uri = Url::parse("file:///test.mq").unwrap();

    // Unlike `publish_diagnostics`, the pull model's response can be asserted on directly.
    let text = "let x = ;"; // Missing expression
    let (_, errors) = mq_lang::parse_recovery(text);
    backend.documents.insert(
        uri.to_string(),
        Arc::new(DocumentSnapshot {
            text: Arc::new(text.to_string()),
            nodes: None,
        }),
    );
    backend.error_map.insert(
        uri.to_string(),
        errors
            .diagnostics(text)
            .into_iter()
            .map(LspError::SyntaxError)
            .collect(),
    );

    let result = backend
        .diagnostic(ls_types::DocumentDiagnosticParams {
            text_document: ls_types::TextDocumentIdentifier { uri: to_uri(&uri) },
            identifier: None,
            previous_result_id: None,
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .await;

    assert!(result.is_ok());
    let ls_types::DocumentDiagnosticReportResult::Report(ls_types::DocumentDiagnosticReport::Full(report)) =
        result.unwrap()
    else {
        panic!("Expected a full document diagnostic report");
    };

    assert!(!report.full_document_diagnostic_report.items.is_empty());
}

#[tokio::test]
async fn test_pull_diagnostics_syntax_error_includes_did_you_mean_hint() {
    // Regression test: syntax errors reported over LSP now carry the same
    // "did you mean" hint text as the CLI's miette-rendered errors, instead of
    // being flattened to a bare message before hints could be attached.
    let (service, _) = LspService::new(|client| Backend {
        client,
        hir: Arc::new(RwLock::new(mq_hir::Hir::default())),
        source_map: RwLock::new(BiMap::new()),
        type_env_map: DashMap::new(),
        error_map: DashMap::new(),
        documents: DashMap::new(),
        config: LspConfig::default(),
    });

    let backend = service.inner();
    let uri = Url::parse("file:///test.mq").unwrap();

    let text = ".hedaing"; // Typo of the `.heading` selector
    let (_, errors) = mq_lang::parse_recovery(text);
    backend.documents.insert(
        uri.to_string(),
        Arc::new(DocumentSnapshot {
            text: Arc::new(text.to_string()),
            nodes: None,
        }),
    );
    backend.error_map.insert(
        uri.to_string(),
        errors
            .diagnostics(text)
            .into_iter()
            .map(LspError::SyntaxError)
            .collect(),
    );

    let result = backend
        .diagnostic(ls_types::DocumentDiagnosticParams {
            text_document: ls_types::TextDocumentIdentifier { uri: to_uri(&uri) },
            identifier: None,
            previous_result_id: None,
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .await;

    let ls_types::DocumentDiagnosticReportResult::Report(ls_types::DocumentDiagnosticReport::Full(report)) =
        result.unwrap()
    else {
        panic!("Expected a full document diagnostic report");
    };

    assert!(
        report
            .full_document_diagnostic_report
            .items
            .iter()
            .any(|item| item.message.contains("help:") && item.message.contains(".heading")),
        "Expected a diagnostic with a `.heading` suggestion, got: {:?}",
        report.full_document_diagnostic_report.items
    );
}

#[tokio::test]
async fn test_pull_diagnostics_empty_for_unknown_document() {
    let (service, _) = LspService::new(|client| Backend {
        client,
        hir: Arc::new(RwLock::new(mq_hir::Hir::default())),
        source_map: RwLock::new(BiMap::new()),
        type_env_map: DashMap::new(),
        error_map: DashMap::new(),
        documents: DashMap::new(),
        config: LspConfig::default(),
    });

    let backend = service.inner();
    let uri = Url::parse("file:///unknown.mq").unwrap();

    let result = backend
        .diagnostic(ls_types::DocumentDiagnosticParams {
            text_document: ls_types::TextDocumentIdentifier { uri: to_uri(&uri) },
            identifier: None,
            previous_result_id: None,
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .await;

    assert!(result.is_ok());
    let ls_types::DocumentDiagnosticReportResult::Report(ls_types::DocumentDiagnosticReport::Full(report)) =
        result.unwrap()
    else {
        panic!("Expected a full document diagnostic report");
    };

    assert!(report.full_document_diagnostic_report.items.is_empty());
}

#[tokio::test]
async fn test_unused_function_warnings() {
    let (service, _) = LspService::new(|client| Backend {
        client,
        hir: Arc::new(RwLock::new(mq_hir::Hir::default())),
        source_map: RwLock::new(BiMap::new()),
        type_env_map: DashMap::new(),
        error_map: DashMap::new(),
        documents: DashMap::new(),
        config: LspConfig::default(),
    });

    let backend = service.inner();
    let uri = Url::parse("file:///test.mq").unwrap();

    // Code with unused function
    let code = "def used_function(): 1; def unused_function(): 2; | used_function()";
    let (nodes, errors) = mq_lang::parse_recovery(code);
    let (source_id, _) = backend.hir.write().unwrap().add_nodes(uri.clone(), &nodes);

    backend.source_map.write().unwrap().insert(uri.to_string(), source_id);
    backend.documents.insert(
        uri.to_string(),
        Arc::new(DocumentSnapshot {
            text: Arc::new(code.to_string()),
            nodes: None,
        }),
    );
    backend.error_map.insert(
        uri.to_string(),
        errors
            .diagnostics(code)
            .into_iter()
            .map(LspError::SyntaxError)
            .collect(),
    );

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
async fn test_lint_diagnostics_enabled() {
    let (service, _) = LspService::new(|client| Backend {
        client,
        hir: Arc::new(RwLock::new(mq_hir::Hir::default())),
        source_map: RwLock::new(BiMap::new()),
        type_env_map: DashMap::new(),
        error_map: DashMap::new(),
        documents: DashMap::new(),
        config: LspConfig::new(
            vec![],
            false,
            mq_check::TypeCheckerOptions::default(),
            true,
            mq_lint::LintConfig::default(),
        ),
    });

    let backend = service.inner();
    let uri = Url::parse("file:///test.mq").unwrap();

    // `x` is declared but never used.
    let code = "let x = .h1 | .text";
    let (nodes, errors) = mq_lang::parse_recovery(code);
    let (source_id, _) = backend.hir.write().unwrap().add_nodes(uri.clone(), &nodes);

    backend.source_map.write().unwrap().insert(uri.to_string(), source_id);
    backend.documents.insert(
        uri.to_string(),
        Arc::new(DocumentSnapshot {
            text: Arc::new(code.to_string()),
            nodes: None,
        }),
    );
    backend.error_map.insert(
        uri.to_string(),
        errors
            .diagnostics(code)
            .into_iter()
            .map(LspError::SyntaxError)
            .collect(),
    );

    {
        let hir_guard = backend.hir.read().unwrap();
        let lint_ctx = mq_lint::LintContext::new(&hir_guard, source_id, &backend.config.lint_config);
        let diagnostics = mq_lint::Linter::with_default_rules().run(&lint_ctx);
        assert!(
            diagnostics
                .iter()
                .any(|d| d.rule_id() == mq_lint::RuleId::UnusedVariable),
            "expected an unused_variable lint diagnostic"
        );
    }

    // Should complete without panic.
    backend.diagnostics(uri, None).await;
}

#[tokio::test]
async fn test_lint_diagnostics_disabled_by_default() {
    let (service, _) = LspService::new(|client| Backend {
        client,
        hir: Arc::new(RwLock::new(mq_hir::Hir::default())),
        source_map: RwLock::new(BiMap::new()),
        type_env_map: DashMap::new(),
        error_map: DashMap::new(),
        documents: DashMap::new(),
        config: LspConfig::default(),
    });

    let backend = service.inner();
    assert!(!backend.config.enable_lint);

    let uri = Url::parse("file:///test.mq").unwrap();
    let code = "let x = .h1 | .text";
    let (nodes, errors) = mq_lang::parse_recovery(code);
    let (source_id, _) = backend.hir.write().unwrap().add_nodes(uri.clone(), &nodes);

    backend.source_map.write().unwrap().insert(uri.to_string(), source_id);
    backend.documents.insert(
        uri.to_string(),
        Arc::new(DocumentSnapshot {
            text: Arc::new(code.to_string()),
            nodes: None,
        }),
    );
    backend.error_map.insert(
        uri.to_string(),
        errors
            .diagnostics(code)
            .into_iter()
            .map(LspError::SyntaxError)
            .collect(),
    );

    // Should complete without panic.
    backend.diagnostics(uri, None).await;
}

#[tokio::test]
async fn test_formatting_with_errors() {
    let (service, _) = LspService::new(|client| Backend {
        client,
        hir: Arc::new(RwLock::new(mq_hir::Hir::default())),
        source_map: RwLock::new(BiMap::new()),
        type_env_map: DashMap::new(),
        error_map: DashMap::new(),
        documents: DashMap::new(),
        config: LspConfig::default(),
    });

    let backend = service.inner();
    let uri = Url::parse("file:///test.mq").unwrap();

    let text = "def main() 1;";
    let _ = mq_lang::parse_recovery(text);
    backend.documents.insert(
        uri.to_string(),
        Arc::new(DocumentSnapshot {
            text: Arc::new(text.to_string()),
            nodes: None,
        }),
    );
    backend.error_map.insert(
        uri.to_string(),
        vec![LspError::SyntaxError(mq_lang::Diagnostic {
            message: "Syntax error".to_string(),
            range: Some(mq_lang::Range {
                start: mq_lang::Position { line: 1, column: 10 },
                end: mq_lang::Position { line: 1, column: 11 },
            }),
            hints: Vec::new(),
        })],
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
        type_env_map: DashMap::new(),
        error_map: DashMap::new(),
        documents: DashMap::new(),
        config: LspConfig::default(),
    });

    let backend = service.inner();
    let uri = Url::parse("file:///test.mq").unwrap();

    // Code with unreachable code after halt()
    let code = "def test(): halt(1) | let x = 42";
    let (nodes, errors) = mq_lang::parse_recovery(code);
    let (source_id, _) = backend.hir.write().unwrap().add_nodes(uri.clone(), &nodes);

    backend.source_map.write().unwrap().insert(uri.to_string(), source_id);
    backend.documents.insert(
        uri.to_string(),
        Arc::new(DocumentSnapshot {
            text: Arc::new(code.to_string()),
            nodes: None,
        }),
    );
    backend.error_map.insert(
        uri.to_string(),
        errors
            .diagnostics(code)
            .into_iter()
            .map(LspError::SyntaxError)
            .collect(),
    );

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
        error_map: DashMap::new(),
        documents: DashMap::new(),
        config: LspConfig::default(),
    });
    let backend = service.inner();
    backend.documents.insert(
        uri.to_string(),
        Arc::new(DocumentSnapshot {
            text: Arc::new(text.to_string()),
            nodes: None,
        }),
    );

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
        error_map: DashMap::new(),
        documents: DashMap::new(),
        config: LspConfig::default(),
    });
    let backend = service.inner();
    backend.documents.insert(
        uri.to_string(),
        Arc::new(DocumentSnapshot {
            text: Arc::new(text.to_string()),
            nodes: None,
        }),
    );

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
    let text = "invalid mq";
    let (service, _) = LspService::new(|client| Backend {
        client,
        hir: Arc::new(RwLock::new(mq_hir::Hir::default())),
        source_map: RwLock::new(BiMap::new()),
        type_env_map: DashMap::new(),
        error_map: DashMap::new(),
        documents: DashMap::new(),
        config: LspConfig::default(),
    });
    let backend = service.inner();
    backend.documents.insert(
        uri.to_string(),
        Arc::new(DocumentSnapshot {
            text: Arc::new(text.to_string()),
            nodes: None,
        }),
    );
    backend.error_map.insert(
        uri.to_string(),
        vec![LspError::SyntaxError(mq_lang::Diagnostic {
            message: "Syntax error".to_string(),
            range: Some(mq_lang::Range {
                start: mq_lang::Position { line: 0, column: 0 },
                end: mq_lang::Position { line: 0, column: 10 },
            }),
            hints: Vec::new(),
        })],
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
    let mut lint_config = mq_lint::LintConfig::default();
    lint_config.disable_rule(mq_lint::RuleId::NamingConvention);
    let config = LspConfig::new(
        vec![],
        true,
        mq_check::TypeCheckerOptions {
            strict_array: true,
            ..Default::default()
        },
        true,
        lint_config,
    );
    assert!(config.enable_type_checking);
    assert!(config.type_checker_options.strict_array);
    assert!(config.enable_lint);
    assert!(!config.lint_config.is_rule_enabled(mq_lint::RuleId::NamingConvention));
}

#[tokio::test]
async fn test_lsp_config_keeps_the_declared_input_type() {
    let input_type = mq_check::type_expr::parse_type("h1 | h2").unwrap();
    let config = LspConfig::new(
        vec![],
        true,
        mq_check::TypeCheckerOptions {
            input_type: Some(input_type.clone()),
            ..Default::default()
        },
        false,
        mq_lint::LintConfig::default(),
    );
    assert_eq!(config.type_checker_options.input_type, Some(input_type));
    assert_eq!(LspConfig::default().type_checker_options.input_type, None);
}

#[tokio::test]
async fn test_lsp_config_default() {
    let config = LspConfig::default();
    assert!(!config.enable_type_checking);
    assert!(!config.type_checker_options.strict_array);
    assert!(!config.enable_lint);
}

#[tokio::test]
async fn test_diagnostics_with_type_checking_disabled() {
    let (service, _) = LspService::new(|client| Backend {
        client,
        hir: Arc::new(RwLock::new(mq_hir::Hir::default())),
        source_map: RwLock::new(BiMap::new()),
        type_env_map: DashMap::new(),
        error_map: DashMap::new(),
        documents: DashMap::new(),
        config: LspConfig::new(
            vec![],
            false,
            mq_check::TypeCheckerOptions::default(),
            false,
            mq_lint::LintConfig::default(),
        ),
    });

    let backend = service.inner();
    let uri = Url::parse("file:///test.mq").unwrap();

    // Code that would cause a type error (number + string)
    let code = r#"1 + "string""#;
    let (nodes, errors) = mq_lang::parse_recovery(code);
    let (source_id, _) = backend.hir.write().unwrap().add_nodes(uri.clone(), &nodes);

    backend.source_map.write().unwrap().insert(uri.to_string(), source_id);
    backend.documents.insert(
        uri.to_string(),
        Arc::new(DocumentSnapshot {
            text: Arc::new(code.to_string()),
            nodes: None,
        }),
    );
    backend.error_map.insert(
        uri.to_string(),
        errors
            .diagnostics(code)
            .into_iter()
            .map(LspError::SyntaxError)
            .collect(),
    );

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
        error_map: DashMap::new(),
        documents: DashMap::new(),
        config: LspConfig::new(
            vec![],
            true,
            mq_check::TypeCheckerOptions::default(),
            false,
            mq_lint::LintConfig::default(),
        ),
    });

    let backend = service.inner();
    let uri = Url::parse("file:///test.mq").unwrap();

    // Valid code with no type errors
    let code = "def add(x, y): x + y;\n| add(1, 2)";
    let (nodes, errors) = mq_lang::parse_recovery(code);
    let (source_id, _) = backend.hir.write().unwrap().add_nodes(uri.clone(), &nodes);

    backend.source_map.write().unwrap().insert(uri.to_string(), source_id);
    backend.documents.insert(
        uri.to_string(),
        Arc::new(DocumentSnapshot {
            text: Arc::new(code.to_string()),
            nodes: None,
        }),
    );
    backend.error_map.insert(
        uri.to_string(),
        errors
            .diagnostics(code)
            .into_iter()
            .map(LspError::SyntaxError)
            .collect(),
    );

    assert!(backend.error_map.get(&uri.to_string()).unwrap().is_empty());

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
        error_map: DashMap::new(),
        documents: DashMap::new(),
        config: LspConfig::new(
            vec![],
            true,
            mq_check::TypeCheckerOptions::default(),
            false,
            mq_lint::LintConfig::default(),
        ),
    });

    let backend = service.inner();
    let uri = Url::parse("file:///test.mq").unwrap();

    // Code with type error: function arity mismatch
    let code = "def add(x, y): x + y;\n| add(1, 2, 3)";

    // Exercise the full diagnostics pipeline: on_change should parse, type check,
    // and populate error_map with type errors when there are no parse errors.
    backend.on_change(uri.clone(), code.to_string()).await;

    let errors = backend
        .error_map
        .get(&uri.to_string())
        .expect("expected diagnostics entry for URI");

    // Ensure that a type error is surfaced through the diagnostics pipeline.
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, LspError::TypeError(..) | LspError::UncertainTypeError(..))),
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
        error_map: DashMap::new(),
        documents: DashMap::new(),
        config: LspConfig::new(
            vec![],
            true,
            mq_check::TypeCheckerOptions::default(),
            false,
            mq_lint::LintConfig::default(),
        ),
    });

    let backend = service.inner();
    let uri = Url::parse("file:///test.mq").unwrap();

    // Manually insert a parse error so type checking is skipped
    let text = "def add(x, y): x + y;\n| add(1)";
    backend.documents.insert(
        uri.to_string(),
        Arc::new(DocumentSnapshot {
            text: Arc::new(text.to_string()),
            nodes: None,
        }),
    );
    backend.error_map.insert(
        uri.to_string(),
        vec![LspError::SyntaxError(mq_lang::Diagnostic {
            message: "Syntax error".to_string(),
            range: Some(mq_lang::Range {
                start: mq_lang::Position { line: 1, column: 1 },
                end: mq_lang::Position { line: 1, column: 5 },
            }),
            hints: Vec::new(),
        })],
    );

    // Parse errors are present, so type checking should be skipped
    assert!(!backend.error_map.get(&uri.to_string()).unwrap().is_empty());

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
        error_map: DashMap::new(),
        documents: DashMap::new(),
        config: LspConfig::new(
            vec![],
            true,
            mq_check::TypeCheckerOptions {
                strict_array: true,
                ..Default::default()
            },
            false,
            mq_lint::LintConfig::default(),
        ),
    });

    let backend = service.inner();
    let uri = Url::parse("file:///test.mq").unwrap();

    // Homogeneous array, valid even with strict_array
    let code = "[1, 2, 3]";
    let (nodes, errors) = mq_lang::parse_recovery(code);
    let (source_id, _) = backend.hir.write().unwrap().add_nodes(uri.clone(), &nodes);

    backend.source_map.write().unwrap().insert(uri.to_string(), source_id);
    backend.documents.insert(
        uri.to_string(),
        Arc::new(DocumentSnapshot {
            text: Arc::new(code.to_string()),
            nodes: None,
        }),
    );
    backend.error_map.insert(
        uri.to_string(),
        errors
            .diagnostics(code)
            .into_iter()
            .map(LspError::SyntaxError)
            .collect(),
    );

    assert!(backend.error_map.get(&uri.to_string()).unwrap().is_empty());

    backend.diagnostics(uri, None).await;
}

#[tokio::test]
async fn test_diagnostics_with_tuple_option() {
    let (service, _) = LspService::new(|client| Backend {
        client,
        hir: Arc::new(RwLock::new(mq_hir::Hir::default())),
        source_map: RwLock::new(BiMap::new()),
        type_env_map: DashMap::new(),
        error_map: DashMap::new(),
        documents: DashMap::new(),
        config: LspConfig::new(
            vec![],
            true,
            mq_check::TypeCheckerOptions {
                strict_array: false,
                ..Default::default()
            },
            false,
            mq_lint::LintConfig::default(),
        ),
    });

    let backend = service.inner();
    let uri = Url::parse("file:///test.mq").unwrap();

    // Heterogeneous array typed as tuple, valid with tuple mode
    let code = r#"[1, "hello", true]"#;
    let (nodes, errors) = mq_lang::parse_recovery(code);
    let (source_id, _) = backend.hir.write().unwrap().add_nodes(uri.clone(), &nodes);

    backend.source_map.write().unwrap().insert(uri.to_string(), source_id);
    backend.documents.insert(
        uri.to_string(),
        Arc::new(DocumentSnapshot {
            text: Arc::new(code.to_string()),
            nodes: None,
        }),
    );
    backend.error_map.insert(
        uri.to_string(),
        errors
            .diagnostics(code)
            .into_iter()
            .map(LspError::SyntaxError)
            .collect(),
    );

    assert!(backend.error_map.get(&uri.to_string()).unwrap().is_empty());

    backend.diagnostics(uri, None).await;
}
