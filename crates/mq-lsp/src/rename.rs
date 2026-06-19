use std::{
    collections::HashMap,
    str::FromStr,
    sync::{Arc, RwLock},
};

use bimap::BiMap;
use tower_lsp_server::ls_types::{self, Position, Range, TextEdit, WorkspaceEdit};
use url::Url;

fn to_range(text_range: mq_lang::Range) -> Range {
    Range::new(
        Position::new(text_range.start.line - 1, (text_range.start.column - 1) as u32),
        Position::new(text_range.end.line - 1, (text_range.end.column - 1) as u32),
    )
}

/// Renames the symbol at `position` and all of its references, across every
/// source file the HIR knows about.
pub(crate) fn response(
    hir: Arc<RwLock<mq_hir::Hir>>,
    url: Url,
    position: Position,
    new_name: &str,
    source_map: &BiMap<String, mq_hir::SourceId>,
) -> Option<WorkspaceEdit> {
    let hir_guard = hir.read().unwrap();
    let source = hir_guard.source_by_url(&url)?;

    let (symbol_id, symbol) = hir_guard.find_symbol_in_position(
        source,
        mq_lang::Position::new(position.line + 1, (position.character + 1) as usize),
    )?;

    // Builtin functions/macros live outside any editable source and must not be renamed.
    if hir_guard.is_builtin_symbol(&symbol) {
        return None;
    }

    let def_id = match symbol.kind {
        mq_hir::SymbolKind::Call
        | mq_hir::SymbolKind::Ref
        | mq_hir::SymbolKind::CallDynamic
        | mq_hir::SymbolKind::Argument
        | mq_hir::SymbolKind::QualifiedAccess => hir_guard.resolve_reference_symbol(symbol_id)?,
        _ => symbol_id,
    };

    let def_symbol = hir_guard.symbol(def_id)?;
    if hir_guard.is_builtin_symbol(def_symbol) {
        return None;
    }

    let mut symbols_to_rename = vec![(def_id, def_symbol.clone())];
    symbols_to_rename.extend(hir_guard.references(def_id));

    let mut changes: HashMap<ls_types::Uri, Vec<TextEdit>> = HashMap::new();

    for (_, symbol) in symbols_to_rename {
        let Some(text_range) = symbol.source.text_range else {
            continue;
        };
        let Some(source_id) = symbol.source.source_id else {
            continue;
        };
        let Some(file_url) = source_map.get_by_right(&source_id) else {
            continue;
        };
        let Ok(uri) = ls_types::Uri::from_str(file_url) else {
            continue;
        };

        changes.entry(uri).or_default().push(TextEdit {
            range: to_range(text_range),
            new_text: new_name.to_string(),
        });
    }

    if changes.is_empty() {
        None
    } else {
        Some(WorkspaceEdit {
            changes: Some(changes),
            ..Default::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mq_hir::Hir;

    fn setup(code: &str) -> (Arc<RwLock<Hir>>, Url, BiMap<String, mq_hir::SourceId>) {
        let mut hir = Hir::default();
        let url = Url::parse("file:///test.mq").unwrap();
        let (source_id, _) = hir.add_code(Some(url.clone()), code);

        let mut source_map = BiMap::new();
        source_map.insert(url.to_string(), source_id);

        (Arc::new(RwLock::new(hir)), url, source_map)
    }

    #[test]
    fn test_rename_function_and_call_sites() {
        let code = "def func1(): 1; | func1()";
        let (hir, url, source_map) = setup(code);

        // Position on the `func1` definition (column 4, 0-based).
        let result = response(hir, url, Position::new(0, 5), "renamed", &source_map);

        assert!(result.is_some());
        let edit = result.unwrap();
        let edits = edit.changes.unwrap().into_values().next().unwrap();
        assert_eq!(edits.len(), 2);
        assert!(edits.iter().all(|e| e.new_text == "renamed"));
    }

    #[test]
    fn test_rename_from_call_site() {
        let code = "def func1(): 1; | func1()";
        let (hir, url, source_map) = setup(code);

        // Position on the call site `func1()`.
        let result = response(hir, url, Position::new(0, 20), "renamed", &source_map);

        assert!(result.is_some());
        let edit = result.unwrap();
        let edits = edit.changes.unwrap().into_values().next().unwrap();
        assert_eq!(edits.len(), 2);
    }

    #[test]
    fn test_no_rename_for_builtin() {
        let code = "\"hello\" | len";
        let (hir, url, source_map) = setup(code);

        // Position on the builtin `len` call.
        let result = response(hir, url, Position::new(0, 11), "renamed", &source_map);
        assert!(result.is_none());
    }

    #[test]
    fn test_no_rename_when_no_symbol_at_position() {
        let (hir, url, source_map) = setup("let x = 1");
        let result = response(hir, url, Position::new(5, 5), "renamed", &source_map);
        assert!(result.is_none());
    }

    #[test]
    fn test_rename_variable_from_definition() {
        let code = "let val1 = 1 | val1";
        let (hir, url, source_map) = setup(code);

        // Position on the `val1` definition.
        let result = response(hir, url, Position::new(0, 5), "renamed_val", &source_map);

        assert!(result.is_some());
        let edits = result.unwrap().changes.unwrap().into_values().next().unwrap();
        assert_eq!(edits.len(), 2);
        assert!(edits.iter().all(|e| e.new_text == "renamed_val"));
    }

    #[test]
    fn test_rename_variable_from_usage() {
        let code = "let val1 = 1 | val1";
        let (hir, url, source_map) = setup(code);

        // Position on the `val1` usage (not the definition).
        let result = response(hir, url, Position::new(0, 16), "renamed_val", &source_map);

        assert!(result.is_some());
        let edits = result.unwrap().changes.unwrap().into_values().next().unwrap();
        assert_eq!(edits.len(), 2);
    }

    #[test]
    fn test_rename_function_parameter_renames_only_within_function_scope() {
        let code = "def func1(x): x + 1;";
        let (hir, url, source_map) = setup(code);

        // Position on the parameter declaration `x`.
        let result = response(hir, url, Position::new(0, 10), "renamed_param", &source_map);

        assert!(result.is_some());
        let edits = result.unwrap().changes.unwrap().into_values().next().unwrap();
        // Parameter declaration + the one usage in the function body.
        assert_eq!(edits.len(), 2);
        assert!(edits.iter().all(|e| e.new_text == "renamed_param"));
    }

    #[test]
    fn test_rename_macro_and_call_site() {
        let code = "macro inc(x): x + 1 | inc(2)";
        let (hir, url, source_map) = setup(code);

        // Position on the macro definition's name.
        let result = response(hir, url, Position::new(0, 7), "increment", &source_map);

        assert!(result.is_some());
        let edits = result.unwrap().changes.unwrap().into_values().next().unwrap();
        assert_eq!(edits.len(), 2);
        assert!(edits.iter().all(|e| e.new_text == "increment"));
    }

    #[test]
    fn test_rename_definition_with_no_call_sites_renames_only_the_definition() {
        let code = "def unused_func(): 1;";
        let (hir, url, source_map) = setup(code);

        let result = response(hir, url, Position::new(0, 5), "renamed_unused", &source_map);

        assert!(result.is_some());
        let edits = result.unwrap().changes.unwrap().into_values().next().unwrap();
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].new_text, "renamed_unused");
    }

    #[test]
    fn test_no_rename_for_unresolved_call() {
        let code = "totally_unknown_fn()";
        let (hir, url, source_map) = setup(code);

        let result = response(hir, url, Position::new(0, 5), "renamed", &source_map);
        assert!(result.is_none());
    }

    #[test]
    fn test_cross_source_rename_via_include_updates_both_files() {
        let mut hir = Hir::default();
        let main_url = Url::parse("file:///main.mq").unwrap();
        let (main_source_id, _) = hir.add_code(Some(main_url.clone()), "include \"csv\" | csv_parse(\"a,b\", false)");

        // Resolving the `include` eagerly lowers the module's source into the same
        // HIR under its own synthetic source/url (see hir/lower.rs::add_include_expr).
        let module_source_id = hir
            .symbols()
            .find_map(|(_, symbol)| match symbol.kind {
                mq_hir::SymbolKind::Include(module_source_id) => Some(module_source_id),
                _ => None,
            })
            .expect("expected an Include symbol for the csv module");
        let module_url = hir.url_by_source(&module_source_id).unwrap().clone();

        let mut source_map = BiMap::new();
        source_map.insert(main_url.to_string(), main_source_id);
        source_map.insert(module_url.to_string(), module_source_id);

        // Position on the `csv_parse` call site (`include "csv" | csv_parse(...)`).
        let result = response(
            Arc::new(RwLock::new(hir)),
            main_url.clone(),
            Position::new(0, 18),
            "csv_load",
            &source_map,
        );

        assert!(result.is_some());
        let changes = result.unwrap().changes.unwrap();
        // The call site (main.mq) and the definition (the csv module source) both get an edit.
        assert_eq!(changes.len(), 2);
        for edits in changes.values() {
            assert_eq!(edits.len(), 1);
            assert_eq!(edits[0].new_text, "csv_load");
        }
    }

    #[test]
    fn test_rename_skips_sources_missing_from_source_map() {
        let mut hir = Hir::default();
        let main_url = Url::parse("file:///main.mq").unwrap();
        let (main_source_id, _) = hir.add_code(Some(main_url.clone()), "include \"csv\" | csv_parse(\"a,b\", false)");

        // Only the main file is tracked; the module's synthetic source is not in source_map,
        // mirroring a real session where that file was never opened in the editor.
        let mut source_map = BiMap::new();
        source_map.insert(main_url.to_string(), main_source_id);

        let result = response(
            Arc::new(RwLock::new(hir)),
            main_url.clone(),
            Position::new(0, 18),
            "csv_load",
            &source_map,
        );

        assert!(result.is_some());
        let changes = result.unwrap().changes.unwrap();
        // Only the call site we can map back to a tracked URL gets an edit.
        assert_eq!(changes.len(), 1);
        let edits = changes.into_values().next().unwrap();
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].new_text, "csv_load");
    }
}
