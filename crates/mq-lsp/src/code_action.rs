use std::{
    collections::HashMap,
    sync::{Arc, LazyLock, RwLock},
};

use rustc_hash::FxHashMap;
use smol_str::SmolStr;
use tower_lsp_server::ls_types::{
    self, CodeAction, CodeActionKind, CodeActionOrCommand, CodeActionParams, Position, Range, TextEdit, WorkspaceEdit,
};
use url::Url;

static STD_MODULE_EXPORTS: LazyLock<FxHashMap<SmolStr, Vec<SmolStr>>> = LazyLock::new(|| {
    let mut exports: FxHashMap<SmolStr, Vec<SmolStr>> = FxHashMap::default();

    for (module_name, source) in mq_lang::STANDARD_MODULES.iter() {
        let mut hir = mq_hir::Hir::default();
        hir.add_code(None, source());

        for (_, symbol) in hir.symbols() {
            if symbol.is_function()
                && let Some(name) = &symbol.value
            {
                exports.entry(name.clone()).or_default().push(module_name.clone());
            }
        }
    }

    exports
});

fn to_range(text_range: mq_lang::Range) -> Range {
    Range::new(
        Position::new(text_range.start.line - 1, (text_range.start.column - 1) as u32),
        Position::new(text_range.end.line - 1, (text_range.end.column - 1) as u32),
    )
}

/// Builds a quick fix that inserts `keyword "module_name";` at the top of the file.
fn add_module_statement_action(
    uri: &ls_types::Uri,
    diagnostic: &ls_types::Diagnostic,
    keyword: &str,
    module_name: &str,
) -> CodeActionOrCommand {
    let mut changes = HashMap::new();
    changes.insert(
        uri.clone(),
        vec![TextEdit {
            range: Range::new(Position::new(0, 0), Position::new(0, 0)),
            new_text: format!("{keyword} \"{module_name}\";\n"),
        }],
    );

    CodeActionOrCommand::CodeAction(CodeAction {
        title: format!("Add `{keyword} \"{module_name}\"`"),
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: Some(vec![diagnostic.clone()]),
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            ..Default::default()
        }),
        ..Default::default()
    })
}

/// Builds quick fixes for diagnostics in the request range:
/// - For an unresolved bare call/ref whose name matches an export of one of the
///   standard modules, suggests `include "<module>"`.
/// - For an unresolved qualified access (`module::func()`) where `module` is a
///   recognized standard module that simply hasn't been imported yet, suggests
///   `import "<module>"`. The module name comes directly from the qualifier the
///   user already wrote, rather than from a function-name lookup.
///
/// Unresolved symbols are looked up via `Hir::errors()` rather than
/// `find_symbol_in_position`, because that helper follows `Call`/`Ref` symbols to
/// their resolved definition and therefore returns `None` for symbols that are
/// unresolved by definition.
pub(crate) fn response(
    hir: Arc<RwLock<mq_hir::Hir>>,
    url: Url,
    params: CodeActionParams,
) -> Option<Vec<CodeActionOrCommand>> {
    let hir_guard = hir.read().unwrap();
    hir_guard.source_by_url(&url)?;
    let uri = &params.text_document.uri;

    // For a `module::func()` call, the unresolved error is reported on the `func`
    // `Ref` symbol, whose parent is the `QualifiedAccess` symbol holding the
    // already-written module name (see hir/lower.rs::add_qualified_access_expr).
    let unresolved: Vec<(SmolStr, Range, Option<SmolStr>)> = hir_guard
        .errors()
        .into_iter()
        .filter_map(|error| match error {
            mq_hir::HirError::UnresolvedSymbol { symbol, .. } => {
                let name = symbol.value?;
                let range = to_range(symbol.source.text_range?);
                let qualified_module = symbol.parent.and_then(|parent_id| {
                    hir_guard.symbol(parent_id).and_then(|parent| match &parent.kind {
                        mq_hir::SymbolKind::QualifiedAccess => parent.value.clone(),
                        _ => None,
                    })
                });
                Some((name, range, qualified_module))
            }
            _ => None,
        })
        .collect();

    let mut actions = Vec::new();

    for diagnostic in &params.context.diagnostics {
        let Some((name, _, qualified_module)) = unresolved.iter().find(|(_, range, _)| *range == diagnostic.range)
        else {
            continue;
        };

        if let Some(module_name) = qualified_module {
            if mq_lang::STANDARD_MODULES.contains_key(module_name.as_str()) {
                actions.push(add_module_statement_action(uri, diagnostic, "import", module_name));
            }
        } else if let Some(modules) = STD_MODULE_EXPORTS.get(name.as_str()) {
            actions.extend(
                modules
                    .iter()
                    .map(|module_name| add_module_statement_action(uri, diagnostic, "include", module_name)),
            );
        }
    }

    if actions.is_empty() { None } else { Some(actions) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;
    use tower_lsp_server::ls_types::{CodeActionContext, TextDocumentIdentifier, WorkDoneProgressParams};

    fn params_for(uri: &ls_types::Uri, range: Range, message: &str) -> CodeActionParams {
        CodeActionParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            range,
            context: CodeActionContext {
                diagnostics: vec![ls_types::Diagnostic::new_simple(range, message.to_string())],
                only: None,
                trigger_kind: None,
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: Default::default(),
        }
    }

    #[test]
    fn test_suggests_include_for_unresolved_std_module_function() {
        let mut hir = mq_hir::Hir::default();
        let url = Url::parse("file:///test.mq").unwrap();
        hir.add_code(Some(url.clone()), "csv_parse(\"a,b\", false)");

        let uri = ls_types::Uri::from_str(url.as_str()).unwrap();
        // `csv_parse` spans columns 0..9 on line 0 (0-based, end-exclusive in LSP terms).
        let range = Range::new(Position::new(0, 0), Position::new(0, 9));
        let params = params_for(&uri, range, "Unresolved symbol: csv_parse");

        let actions = response(Arc::new(RwLock::new(hir)), url, params);

        assert!(actions.is_some());
        let actions = actions.unwrap();
        assert!(actions.iter().any(|action| match action {
            CodeActionOrCommand::CodeAction(action) => action.title.contains("csv"),
            _ => false,
        }));
    }

    #[test]
    fn test_no_action_for_resolved_symbol() {
        let mut hir = mq_hir::Hir::default();
        let url = Url::parse("file:///test.mq").unwrap();
        hir.add_code(Some(url.clone()), "def func1(): 1; | func1()");

        let uri = ls_types::Uri::from_str(url.as_str()).unwrap();
        let range = Range::new(Position::new(0, 19), Position::new(0, 24));
        let params = params_for(&uri, range, "Unresolved symbol: func1");

        let actions = response(Arc::new(RwLock::new(hir)), url, params);
        assert!(actions.is_none());
    }

    #[test]
    fn test_no_action_for_unknown_symbol() {
        let mut hir = mq_hir::Hir::default();
        let url = Url::parse("file:///test.mq").unwrap();
        hir.add_code(Some(url.clone()), "totally_unknown_fn()");

        let uri = ls_types::Uri::from_str(url.as_str()).unwrap();
        let range = Range::new(Position::new(0, 0), Position::new(0, 17));
        let params = params_for(&uri, range, "Unresolved symbol: totally_unknown_fn");

        let actions = response(Arc::new(RwLock::new(hir)), url, params);
        assert!(actions.is_none());
    }

    #[test]
    fn test_offers_one_action_per_module_when_exported_by_several() {
        let mut hir = mq_hir::Hir::default();
        let url = Url::parse("file:///test.mq").unwrap();
        hir.add_code(Some(url.clone()), "csv_parse(\"a,b\", false)");

        // `csv_parse` is exported by both the `csv` and `table` standard modules.
        let exporters = STD_MODULE_EXPORTS.get("csv_parse").cloned().unwrap_or_default();
        assert!(exporters.len() >= 2, "expected csv_parse to have multiple exporters");

        let uri = ls_types::Uri::from_str(url.as_str()).unwrap();
        let range = Range::new(Position::new(0, 0), Position::new(0, 9));
        let params = params_for(&uri, range, "Unresolved symbol: csv_parse");

        let actions = response(Arc::new(RwLock::new(hir)), url, params).unwrap();
        assert_eq!(actions.len(), exporters.len());

        let titles: Vec<String> = actions
            .iter()
            .map(|action| match action {
                CodeActionOrCommand::CodeAction(action) => action.title.clone(),
                _ => String::new(),
            })
            .collect();
        for module_name in &exporters {
            assert!(titles.iter().any(|t| t.contains(module_name.as_str())));
        }
    }

    #[test]
    fn test_action_edit_inserts_include_at_top_of_file() {
        let mut hir = mq_hir::Hir::default();
        let url = Url::parse("file:///test.mq").unwrap();
        hir.add_code(Some(url.clone()), "csv_parse(\"a,b\", false)");

        let uri = ls_types::Uri::from_str(url.as_str()).unwrap();
        let range = Range::new(Position::new(0, 0), Position::new(0, 9));
        let params = params_for(&uri, range, "Unresolved symbol: csv_parse");

        let actions = response(Arc::new(RwLock::new(hir)), url, params).unwrap();
        let CodeActionOrCommand::CodeAction(action) = &actions[0] else {
            panic!("expected a CodeAction");
        };

        let edit = action.edit.as_ref().unwrap();
        let edits = edit.changes.as_ref().unwrap().get(&uri).unwrap();
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].range, Range::new(Position::new(0, 0), Position::new(0, 0)));
        assert!(edits[0].new_text.starts_with("include \""));
        assert!(edits[0].new_text.ends_with("\";\n"));

        assert_eq!(action.kind, Some(CodeActionKind::QUICKFIX));
        assert_eq!(
            action.diagnostics,
            Some(vec![params_diagnostic(range, "Unresolved symbol: csv_parse")])
        );
    }

    #[test]
    fn test_suggests_include_for_unresolved_ref_not_just_call() {
        let mut hir = mq_hir::Hir::default();
        let url = Url::parse("file:///test.mq").unwrap();
        // A bare identifier (no call parens) lowers to a `Ref` symbol rather than `Call`.
        hir.add_code(Some(url.clone()), "csv_parse");

        let uri = ls_types::Uri::from_str(url.as_str()).unwrap();
        let range = Range::new(Position::new(0, 0), Position::new(0, 9));
        let params = params_for(&uri, range, "Unresolved symbol: csv_parse");

        let actions = response(Arc::new(RwLock::new(hir)), url, params);
        assert!(actions.is_some());
    }

    #[test]
    fn test_no_action_when_diagnostic_range_does_not_match_any_unresolved_symbol() {
        let mut hir = mq_hir::Hir::default();
        let url = Url::parse("file:///test.mq").unwrap();
        hir.add_code(Some(url.clone()), "csv_parse(\"a,b\", false)");

        let uri = ls_types::Uri::from_str(url.as_str()).unwrap();
        // Range doesn't line up with the actual `csv_parse` symbol range (0,0)-(0,9).
        let range = Range::new(Position::new(5, 0), Position::new(5, 9));
        let params = params_for(&uri, range, "Unresolved symbol: csv_parse");

        let actions = response(Arc::new(RwLock::new(hir)), url, params);
        assert!(actions.is_none());
    }

    #[test]
    fn test_no_action_when_no_diagnostics_supplied() {
        let mut hir = mq_hir::Hir::default();
        let url = Url::parse("file:///test.mq").unwrap();
        hir.add_code(Some(url.clone()), "csv_parse(\"a,b\", false)");

        let uri = ls_types::Uri::from_str(url.as_str()).unwrap();
        let params = CodeActionParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            range: Range::new(Position::new(0, 0), Position::new(0, 9)),
            context: CodeActionContext {
                diagnostics: vec![],
                only: None,
                trigger_kind: None,
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: Default::default(),
        };

        let actions = response(Arc::new(RwLock::new(hir)), url, params);
        assert!(actions.is_none());
    }

    #[test]
    fn test_no_action_for_unopened_document() {
        let hir = mq_hir::Hir::default();
        let url = Url::parse("file:///never-opened.mq").unwrap();
        let uri = ls_types::Uri::from_str(url.as_str()).unwrap();
        let range = Range::new(Position::new(0, 0), Position::new(0, 9));
        let params = params_for(&uri, range, "Unresolved symbol: csv_parse");

        let actions = response(Arc::new(RwLock::new(hir)), url, params);
        assert!(actions.is_none());
    }

    #[test]
    fn test_aggregates_actions_across_multiple_diagnostics() {
        let mut hir = mq_hir::Hir::default();
        let url = Url::parse("file:///test.mq").unwrap();
        hir.add_code(Some(url.clone()), "json_parse(\"{}\") | totally_unknown_fn()");

        // Derive the exact ranges from the HIR itself instead of hand-computing
        // column offsets, so the test stays correct if the fixture text changes.
        let mut ranges_by_name: std::collections::HashMap<String, Range> = hir
            .errors()
            .into_iter()
            .filter_map(|error| match error {
                mq_hir::HirError::UnresolvedSymbol { symbol, .. } => {
                    Some((symbol.value?.to_string(), to_range(symbol.source.text_range?)))
                }
                _ => None,
            })
            .collect();
        let json_range = ranges_by_name.remove("json_parse").unwrap();
        let unknown_range = ranges_by_name.remove("totally_unknown_fn").unwrap();

        let uri = ls_types::Uri::from_str(url.as_str()).unwrap();

        let params = CodeActionParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            range: Range::new(Position::new(0, 0), Position::new(0, 38)),
            context: CodeActionContext {
                diagnostics: vec![
                    params_diagnostic(json_range, "Unresolved symbol: json_parse"),
                    params_diagnostic(unknown_range, "Unresolved symbol: totally_unknown_fn"),
                ],
                only: None,
                trigger_kind: None,
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: Default::default(),
        };

        let actions = response(Arc::new(RwLock::new(hir)), url, params);
        assert!(actions.is_some());
        let actions = actions.unwrap();
        // Only `json_parse` resolves to a std module export; `totally_unknown_fn` contributes nothing.
        assert!(actions.iter().any(|action| match action {
            CodeActionOrCommand::CodeAction(action) => action.title.contains("json"),
            _ => false,
        }));
        assert!(!actions.iter().any(|action| match action {
            CodeActionOrCommand::CodeAction(action) => action.title.contains("totally_unknown_fn"),
            _ => false,
        }));
    }

    fn params_diagnostic(range: Range, message: &str) -> ls_types::Diagnostic {
        ls_types::Diagnostic::new_simple(range, message.to_string())
    }

    /// Derives the LSP range of the unresolved symbol named `name` directly from the
    /// HIR, instead of hand-computing column offsets that drift if the fixture changes.
    fn unresolved_range(hir: &mq_hir::Hir, name: &str) -> Range {
        hir.errors()
            .into_iter()
            .find_map(|error| match error {
                mq_hir::HirError::UnresolvedSymbol { symbol, .. } if symbol.value.as_deref() == Some(name) => {
                    symbol.source.text_range.map(to_range)
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("expected an unresolved symbol named `{name}`"))
    }

    #[test]
    fn test_suggests_import_for_unresolved_qualified_access_to_known_module() {
        let mut hir = mq_hir::Hir::default();
        let url = Url::parse("file:///test.mq").unwrap();
        hir.add_code(Some(url.clone()), "csv::csv_parse(\"a,b\", false)");

        let range = unresolved_range(&hir, "csv_parse");
        let uri = ls_types::Uri::from_str(url.as_str()).unwrap();
        let params = params_for(&uri, range, "Unresolved symbol: csv_parse");

        let actions = response(Arc::new(RwLock::new(hir)), url, params);

        assert!(actions.is_some());
        let actions = actions.unwrap();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            CodeActionOrCommand::CodeAction(action) => {
                assert_eq!(action.title, "Add `import \"csv\"`");
                let edits = action
                    .edit
                    .as_ref()
                    .unwrap()
                    .changes
                    .as_ref()
                    .unwrap()
                    .get(&uri)
                    .unwrap();
                assert_eq!(edits[0].new_text, "import \"csv\";\n");
            }
            _ => panic!("expected a CodeAction"),
        }
    }

    #[test]
    fn test_no_import_action_for_unrecognized_qualified_module() {
        let mut hir = mq_hir::Hir::default();
        let url = Url::parse("file:///test.mq").unwrap();
        hir.add_code(Some(url.clone()), "totally_unknown_module::some_fn()");

        let range = unresolved_range(&hir, "some_fn");
        let uri = ls_types::Uri::from_str(url.as_str()).unwrap();
        let params = params_for(&uri, range, "Unresolved symbol: some_fn");

        let actions = response(Arc::new(RwLock::new(hir)), url, params);
        assert!(actions.is_none());
    }

    #[test]
    fn test_no_action_once_module_is_imported() {
        let mut hir = mq_hir::Hir::default();
        let url = Url::parse("file:///test.mq").unwrap();
        hir.add_code(Some(url.clone()), "import \"csv\" | csv::csv_parse(\"a,b\", false)");

        // Resolved via the explicit `import`, so there should be no unresolved-symbol
        // diagnostic at all, and therefore nothing for a code action to attach to.
        assert!(hir.errors().is_empty());

        let uri = ls_types::Uri::from_str(url.as_str()).unwrap();
        let range = Range::new(Position::new(0, 0), Position::new(0, 1));
        let params = params_for(&uri, range, "Unresolved symbol: csv_parse");

        let actions = response(Arc::new(RwLock::new(hir)), url, params);
        assert!(actions.is_none());
    }
}
