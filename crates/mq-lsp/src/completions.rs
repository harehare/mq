use std::{
    path::PathBuf,
    sync::{Arc, RwLock},
};

use bimap::BiMap;
use itertools::Itertools;
use tower_lsp_server::ls_types::{
    CompletionItem, CompletionItemKind, CompletionResponse, CompletionTextEdit, Documentation, InsertTextFormat,
    MarkupContent, MarkupKind, Position, Range, TextEdit,
};
use url::Url;

/// What the cursor is completing, derived from the text before it on the line.
#[derive(Debug, PartialEq, Eq)]
enum Context<'a> {
    /// `.` followed by a partial selector name.
    Selector { prefix: &'a str },
    /// Inside the string of `import "..."` or `include "..."`.
    ModulePath { prefix: &'a str },
    /// `module::` followed by a partial member name.
    Qualified { module: &'a str, prefix: &'a str },
    /// A bare identifier prefix.
    Identifier { prefix: &'a str },
}

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The typed part of an unclosed `import "..."` / `include "..."` path ending the prefix.
fn module_path(line_prefix: &str) -> Option<&str> {
    let quote = line_prefix.rfind('"')?;
    let before = &line_prefix[..quote];
    // An odd number of quotes before means this one closes a string.
    if !before.matches('"').count().is_multiple_of(2) {
        return None;
    }
    let directive = before.trim_end();
    let keyword_start = ["import", "include"]
        .iter()
        .find_map(|keyword| directive.strip_suffix(keyword).map(str::len))?;
    let is_separated = !directive[..keyword_start].ends_with(is_ident_char);
    (is_separated && directive.len() < before.len()).then_some(&line_prefix[quote + 1..])
}

fn context(line_prefix: &str) -> Context<'_> {
    if let Some(prefix) = module_path(line_prefix) {
        return Context::ModulePath { prefix };
    }

    let word_start = line_prefix.trim_end_matches(is_ident_char).len();
    let (head, prefix) = line_prefix.split_at(word_start);

    if let Some(before) = head.strip_suffix("::") {
        let module = &before[before.trim_end_matches(is_ident_char).len()..];
        if !module.is_empty() {
            return Context::Qualified { module, prefix };
        }
    }

    if let Some(before) = head.strip_suffix('.')
        && !before.ends_with(|c: char| is_ident_char(c) || matches!(c, ')' | ']' | '"' | '.'))
    {
        return Context::Selector { prefix };
    }

    Context::Identifier { prefix }
}

/// Returns the text of `position`'s line up to the cursor.
pub(crate) fn line_prefix(text: &str, position: Position) -> String {
    let line = text.split('\n').nth(position.line as usize).unwrap_or_default();
    let mut units = 0;
    line.chars()
        .take_while(|c| {
            let keep = units < position.character;
            units += c.len_utf16() as u32;
            keep
        })
        .collect()
}

/// Internal helpers start with `_` and are offered only when the user types `_` first.
fn is_visible(label: &str, prefix: &str) -> bool {
    label.starts_with(prefix) && (prefix.starts_with('_') || !label.starts_with('_'))
}

pub(crate) fn response(
    hir: Arc<RwLock<mq_hir::Hir>>,
    url: Url,
    position: Position,
    source_map: &BiMap<String, mq_hir::SourceId>,
    line_prefix: &str,
    module_paths: &[PathBuf],
) -> Option<CompletionResponse> {
    let source_id = source_map.get_by_left(&url.to_string())?;
    let ctx = context(line_prefix);

    if let Context::ModulePath { prefix } = ctx {
        return Some(CompletionResponse::Array(
            mq_lang::DefaultModuleResolver::new(module_paths.to_vec())
                .module_names()
                .into_iter()
                .filter(|name| is_visible(name, prefix))
                .map(|name| CompletionItem {
                    label: name,
                    kind: Some(CompletionItemKind::MODULE),
                    ..Default::default()
                })
                .collect(),
        ));
    }

    let hir_guard = hir.read().unwrap();
    let cursor = mq_lang::Position::new(position.line + 1, (position.character + 1) as usize);

    let scope_id = hir_guard
        .find_scope_in_position(*source_id, cursor)
        .map(|(scope_id, _)| scope_id)
        .unwrap_or_else(|| hir_guard.find_scope_by_source(source_id));

    let module_symbols = match ctx {
        Context::Qualified { module, .. } => hir_guard
            .find_module_source_by_name(scope_id, module)
            .map(|id| hir_guard.find_symbols_in_module(id)),
        _ => None,
    };

    let mut symbols = if let Some(module_symbols) = module_symbols {
        module_symbols
    } else {
        let scope_symbols = hir_guard.find_visible_symbols_in_scope(scope_id, *source_id, cursor);
        let builtin_scope_id = hir_guard.builtin.scope_id;
        // Top-level only; builtin.mq calls/locals would shadow builtins.
        let builtin_symbols = hir_guard
            .find_symbols_in_source(hir_guard.builtin.source_id)
            .into_iter()
            .filter(move |s| s.scope == builtin_scope_id);
        scope_symbols
            .into_iter()
            .chain(builtin_symbols)
            .filter(|s| is_completable(&s.kind))
            .unique_by(|s| s.value.clone())
            .collect::<Vec<_>>()
    };

    let prefix = match ctx {
        Context::Selector { prefix } | Context::Qualified { prefix, .. } | Context::Identifier { prefix } => prefix,
        Context::ModulePath { .. } => unreachable!("handled above"),
    };
    let selector_only = matches!(ctx, Context::Selector { .. });
    symbols.retain(|symbol| {
        let is_selector = matches!(symbol.kind, mq_hir::SymbolKind::Selector(_));
        let name = symbol.value.as_deref().unwrap_or_default();
        if selector_only {
            is_selector && name.trim_start_matches('.').starts_with(prefix)
        } else {
            !is_selector && is_visible(name, prefix)
        }
    });

    drop(hir_guard);

    let mut items = symbols
        .iter()
        .filter_map(|symbol| match &symbol.kind {
            mq_hir::SymbolKind::Function(params) => {
                let deprecated = symbol.is_deprecated();
                Some(CompletionItem {
                    label: symbol.value.clone().unwrap_or_default().to_string(),
                    kind: Some(CompletionItemKind::FUNCTION),
                    detail: Some(symbol.doc.iter().map(|(_, doc)| doc).join("\n")),
                    insert_text: Some(format!(
                        "{}({})",
                        symbol.value.clone().unwrap_or_default(),
                        params
                            .iter()
                            .enumerate()
                            .map(|(i, p)| format!("${{{}:{}}}", i + 1, p))
                            .join(", ")
                    )),
                    insert_text_format: Some(InsertTextFormat::SNIPPET),
                    documentation: Some(Documentation::MarkupContent(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: format!("```md\n{}\n```", symbol.doc.iter().map(|(_, doc)| doc).join("\n")),
                    })),
                    deprecated: Some(deprecated),
                    ..Default::default()
                })
            }
            mq_hir::SymbolKind::Parameter
            | mq_hir::SymbolKind::Variable
            | mq_hir::SymbolKind::DestructuringBinding
            | mq_hir::SymbolKind::PatternVariable { .. } => {
                let deprecated = symbol.is_deprecated();
                Some(CompletionItem {
                    label: symbol.value.clone().unwrap_or_default().to_string(),
                    kind: Some(CompletionItemKind::VARIABLE),
                    detail: Some(symbol.doc.iter().map(|(_, doc)| doc).join("\n")),
                    documentation: Some(Documentation::MarkupContent(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: format!("```md\n{}\n```", symbol.doc.iter().map(|(_, doc)| doc).join("\n")),
                    })),
                    deprecated: Some(deprecated),
                    ..Default::default()
                })
            }
            mq_hir::SymbolKind::Selector(_) => {
                let deprecated = symbol.is_deprecated();
                Some(CompletionItem {
                    label: symbol.value.clone().unwrap_or_default().to_string(),
                    kind: Some(CompletionItemKind::METHOD),
                    detail: Some(symbol.doc.iter().map(|(_, doc)| doc).join("\n")),
                    insert_text: Some(symbol.value.clone().unwrap_or_default().into()),
                    insert_text_format: Some(InsertTextFormat::SNIPPET),
                    documentation: Some(Documentation::MarkupContent(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: format!("```md\n{}\n```", symbol.doc.iter().map(|(_, doc)| doc).join("\n")),
                    })),
                    deprecated: Some(deprecated),
                    ..Default::default()
                })
            }
            _ => None,
        })
        .collect::<Vec<_>>();

    if selector_only {
        // The client treats `.` as a separator, so replace it along with the typed prefix.
        let start = Position::new(
            position.line,
            position
                .character
                .saturating_sub(prefix.encode_utf16().count() as u32 + 1),
        );
        for item in &mut items {
            item.text_edit = Some(CompletionTextEdit::Edit(TextEdit::new(
                Range::new(start, position),
                item.label.clone(),
            )));
            item.insert_text = None;
            item.insert_text_format = None;
        }
    }

    Some(CompletionResponse::Array(items))
}

fn is_completable(kind: &mq_hir::SymbolKind) -> bool {
    matches!(
        kind,
        mq_hir::SymbolKind::Function(_)
            | mq_hir::SymbolKind::Parameter
            | mq_hir::SymbolKind::Variable
            | mq_hir::SymbolKind::DestructuringBinding
            | mq_hir::SymbolKind::PatternVariable { .. }
            | mq_hir::SymbolKind::Selector(_)
    )
}

#[cfg(test)]
mod tests {
    use mq_hir::Hir;

    use super::*;

    fn complete(code: &str, position: Position) -> Vec<CompletionItem> {
        let mut hir = Hir::default();
        let mut source_map = BiMap::new();
        let url = Url::parse("file:///test.mq").unwrap();
        let (source_id, _, _) = hir.add_code(Some(url.clone()), code);
        source_map.insert(url.to_string(), source_id);
        let prefix = line_prefix(code, position);
        match response(Arc::new(RwLock::new(hir)), url, position, &source_map, &prefix, &[]) {
            Some(CompletionResponse::Array(items)) => items,
            _ => panic!("expected completion items"),
        }
    }

    fn labels(items: &[CompletionItem]) -> Vec<&str> {
        items.iter().map(|item| item.label.as_str()).collect()
    }

    #[rstest::rstest]
    #[case("| .", Context::Selector { prefix: "" })]
    #[case(".h", Context::Selector { prefix: "h" })]
    #[case("x | .li", Context::Selector { prefix: "li" })]
    #[case("x.", Context::Identifier { prefix: "" })]
    #[case("f(1).", Context::Identifier { prefix: "" })]
    #[case("import \"se", Context::ModulePath { prefix: "se" })]
    #[case("  include \"", Context::ModulePath { prefix: "" })]
    #[case("1 | import \"se", Context::ModulePath { prefix: "se" })]
    #[case("import \"csv\" as c | include \"", Context::ModulePath { prefix: "" })]
    #[case("\"import \"", Context::Identifier { prefix: "" })]
    #[case("reimport \"se", Context::Identifier { prefix: "se" })]
    #[case("import \"section\" | sec", Context::Identifier { prefix: "sec" })]
    #[case("section::ti", Context::Qualified { module: "section", prefix: "ti" })]
    #[case("| section::", Context::Qualified { module: "section", prefix: "" })]
    #[case("::", Context::Identifier { prefix: "" })]
    #[case("up", Context::Identifier { prefix: "up" })]
    fn test_context(#[case] line: &str, #[case] expected: Context<'_>) {
        assert_eq!(context(line), expected);
    }

    #[test]
    fn test_line_prefix_counts_utf16_units() {
        assert_eq!(line_prefix("a\nあ😀b", Position::new(1, 3)), "あ😀");
        assert_eq!(line_prefix("abc", Position::new(5, 0)), "");
    }

    #[test]
    fn test_selector_context_offers_only_selectors() {
        let items = complete("| .h", Position::new(0, 4));
        assert!(!items.is_empty());
        assert!(items.iter().all(|i| i.kind == Some(CompletionItemKind::METHOD)));
        assert!(labels(&items).iter().all(|l| l.starts_with(".h")));
        assert!(!complete("| .", Position::new(0, 3)).is_empty());
        let edit = items[0].text_edit.clone().expect("text edit");
        let CompletionTextEdit::Edit(edit) = edit else { panic!() };
        assert_eq!(edit.range, Range::new(Position::new(0, 2), Position::new(0, 4)));
    }

    #[test]
    fn test_import_context_offers_module_names() {
        let items = complete("import \"se", Position::new(0, 10));
        assert!(items.iter().all(|i| i.kind == Some(CompletionItemKind::MODULE)));
        assert!(labels(&items).contains(&"section"));
        assert!(labels(&items).iter().all(|l| l.starts_with("se")));
    }

    #[test]
    fn test_identifier_context_filters_by_prefix_and_hides_internals() {
        let items = complete("upc", Position::new(0, 3));
        let names = labels(&items);
        assert!(names.contains(&"upcase"));
        assert!(names.iter().all(|l| l.starts_with("upc")));

        let all = complete("", Position::new(0, 0));
        assert!(labels(&all).iter().all(|l| !l.starts_with('_') && !l.starts_with('.')));

        let internal = complete("_", Position::new(0, 1));
        assert!(!internal.is_empty());
        assert!(labels(&internal).iter().all(|l| l.starts_with('_')));
    }

    #[test]
    fn test_module_members_hide_internal_helpers() {
        let code = "module m: def pub_fn(): 1; def _helper(): 2; end | m::";
        let items = complete(code, Position::new(0, code.len() as u32));
        assert!(labels(&items).contains(&"pub_fn"));
        assert!(!labels(&items).contains(&"_helper"));
    }

    fn complete_in(sources: &[(&str, &str)], target: usize, position: Position) -> Vec<CompletionItem> {
        let mut hir = Hir::default();
        let mut source_map = BiMap::new();
        let mut urls = Vec::new();
        for (name, code) in sources {
            let url = Url::parse(&format!("file:///{name}")).unwrap();
            let (source_id, _, _) = hir.add_code(Some(url.clone()), code);
            source_map.insert(url.to_string(), source_id);
            urls.push(url);
        }
        let prefix = line_prefix(sources[target].1, position);
        match response(
            Arc::new(RwLock::new(hir)),
            urls.remove(target),
            position,
            &source_map,
            &prefix,
            &[],
        ) {
            Some(CompletionResponse::Array(items)) => items,
            _ => panic!("expected completion items"),
        }
    }

    #[rstest::rstest]
    #[case::unaliased("import \"csv\" | csv::")]
    #[case::aliased("import \"csv\" as c | c::")]
    fn test_qualified_import_offers_module_members(#[case] code: &str) {
        let items = complete(code, Position::new(0, code.len() as u32));
        assert!(labels(&items).contains(&"csv_parse"), "{:?}", labels(&items));
        assert!(!labels(&items).contains(&"upcase"));
    }

    #[test]
    fn test_qualified_completion_ignores_modules_of_other_files() {
        let sources = [
            ("a.mq", "module m: def alpha(): 1; end"),
            ("b.mq", "module m: def beta(): 1; end | m::"),
        ];
        let items = complete_in(&sources, 1, Position::new(0, sources[1].1.len() as u32));
        assert!(labels(&items).contains(&"beta"));
        assert!(!labels(&items).contains(&"alpha"));
    }

    #[test]
    fn test_qualified_completion_prefers_the_nearest_module() {
        let code = "module m: def outer(): 1; end\n| module n:\n  module m: def inner(): 1; end\n  | m::\nend";
        let items = complete(code, Position::new(3, 6));
        assert!(labels(&items).contains(&"inner"));
        assert!(!labels(&items).contains(&"outer"));
    }

    #[test]
    fn test_module_path_lists_modules_in_given_directories() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("custom.mq"), "").unwrap();
        let mut hir = Hir::default();
        let mut source_map = BiMap::new();
        let url = Url::parse("file:///test.mq").unwrap();
        let (source_id, _, _) = hir.add_code(Some(url.clone()), "");
        source_map.insert(url.to_string(), source_id);

        let Some(CompletionResponse::Array(items)) = response(
            Arc::new(RwLock::new(hir)),
            url,
            Position::new(0, 10),
            &source_map,
            "import \"cu",
            &[dir.path().to_path_buf()],
        ) else {
            panic!("expected completion items");
        };
        assert_eq!(labels(&items), vec!["custom"]);
    }

    #[test]
    fn test_completion_response_none_for_unknown_url() {
        let hir = Arc::new(RwLock::new(Hir::default()));
        let source_map = BiMap::new();
        let url = Url::parse("file:///unknown.mql").unwrap();
        let position = Position::new(0, 0);

        let result = response(hir, url, position, &source_map, "", &[]);
        assert!(result.is_none());
    }

    #[test]
    fn test_completion_response_returns_symbols() {
        let mut hir = Hir::default();
        let mut source_map = BiMap::new();
        let url = Url::parse("file:///unknown.mql").unwrap();
        let (source_id, _, _) = hir.add_code(Some(url.clone()), "def func1(): 1;");

        source_map.insert(url.to_string(), source_id);

        let result = response(
            Arc::new(RwLock::new(hir)),
            url,
            Position::new(0, 0),
            &source_map,
            "",
            &[],
        );
        assert!(result.is_some());

        if let Some(CompletionResponse::Array(items)) = result {
            assert!(items.iter().any(|item| item.label == "add"));
        }
    }

    #[test]
    fn test_completion_response_returns_symbols_in_module() {
        let mut hir = Hir::default();
        let mut source_map = BiMap::new();
        let url = Url::parse("file:///unknown.mql").unwrap();
        let (source_id, _, _) = hir.add_code(Some(url.clone()), "module mod1: def func1(): 1; end");

        source_map.insert(url.to_string(), source_id);

        let result = response(
            Arc::new(RwLock::new(hir)),
            url,
            Position::new(0, 0),
            &source_map,
            "",
            &[],
        );
        assert!(result.is_some());

        if let Some(CompletionResponse::Array(items)) = result {
            assert!(items.iter().any(|item| item.label == "func1"));
        }
    }

    #[test]
    fn test_completion_inside_module_offers_only_its_own_names() {
        let code = "let outer = 1\n| module m:\n  def f(): 1;\n  | let own = 2\nend\n| outer\n";
        let cases: &[(Position, &[&str], &[&str])] = &[
            (Position::new(2, 11), &["f", "own", "add"], &["outer"]),
            (Position::new(5, 3), &["outer", "add"], &[]),
        ];
        for (position, expected, unexpected) in cases {
            let mut hir = Hir::default();
            let mut source_map = BiMap::new();
            let url = Url::parse("file:///module.mq").unwrap();
            let (source_id, _, _) = hir.add_code(Some(url.clone()), code);
            source_map.insert(url.to_string(), source_id);

            let Some(CompletionResponse::Array(items)) =
                response(Arc::new(RwLock::new(hir)), url, *position, &source_map, "", &[])
            else {
                panic!("expected completion items");
            };
            let labels = items.iter().map(|item| item.label.as_str()).collect::<Vec<_>>();
            for name in *expected {
                assert!(labels.contains(name), "{name} missing at {position:?}");
            }
            for name in *unexpected {
                assert!(!labels.contains(name), "{name} offered at {position:?}");
            }
        }
    }

    #[test]
    fn test_completion_qualified_access() {
        let mut hir = Hir::default();
        let mut source_map = BiMap::new();
        let url = Url::parse("file:///test.mql").unwrap();

        // Create a module with functions
        let code = "module math: def add(a, b): a + b; def sub(a, b): a - b; end | math::";
        let (source_id, _, _) = hir.add_code(Some(url.clone()), code);

        source_map.insert(url.to_string(), source_id);

        let result = response(
            Arc::new(RwLock::new(hir)),
            url,
            Position::new(0, code.len() as u32),
            &source_map,
            &line_prefix(code, Position::new(0, code.len() as u32)),
            &[],
        );

        assert!(result.is_some());

        if let Some(CompletionResponse::Array(items)) = result {
            // Should include functions from the math module
            assert!(
                items.iter().any(|item| item.label == "add"),
                "Should include 'add' function"
            );
            assert!(
                items.iter().any(|item| item.label == "sub"),
                "Should include 'sub' function"
            );

            // Should NOT include builtin functions when in qualified access mode
            assert!(
                !items.iter().any(|item| item.label == "map"),
                "Should not include builtin 'map' function"
            );
        }
    }

    #[test]
    fn test_completion_qualified_access_with_nested_module() {
        let mut hir = Hir::default();
        let mut source_map = BiMap::new();
        let url = Url::parse("file:///test.mql").unwrap();

        // Create a module with functions
        let code = "module utils: def helper(): 1; end | utils::";
        let (source_id, _, _) = hir.add_code(Some(url.clone()), code);

        source_map.insert(url.to_string(), source_id);

        let result = response(
            Arc::new(RwLock::new(hir)),
            url,
            Position::new(0, code.len() as u32),
            &source_map,
            &line_prefix(code, Position::new(0, code.len() as u32)),
            &[],
        );

        assert!(result.is_some());

        if let Some(CompletionResponse::Array(items)) = result {
            assert!(
                items.iter().any(|item| item.label == "helper"),
                "Should include 'helper' function from utils module"
            );
        }
    }

    #[test]
    fn test_completion_with_deprecated_function() {
        let mut hir = Hir::default();
        let mut source_map = BiMap::new();
        let url = Url::parse("file:///test.mql").unwrap();

        // Create a function with deprecated marker in doc
        let code = r#"# deprecated: This function is no longer supported
def old_func(x): x + 1;"#;
        let (source_id, _, _) = hir.add_code(Some(url.clone()), code);

        source_map.insert(url.to_string(), source_id);

        let result = response(
            Arc::new(RwLock::new(hir)),
            url,
            Position::new(1, 0),
            &source_map,
            "",
            &[],
        );

        assert!(result.is_some());

        if let Some(CompletionResponse::Array(items)) = result {
            let old_func_item = items.iter().find(|item| item.label == "old_func");
            assert!(old_func_item.is_some(), "Should include 'old_func'");

            if let Some(item) = old_func_item {
                assert_eq!(item.deprecated, Some(true), "old_func should be marked as deprecated");
            }
        }
    }

    #[test]
    fn test_completion_includes_every_builtin_function() {
        for code in ["", "upcase(\"a\") | len() | def f(): downcase(\"A\");"] {
            let mut hir = Hir::default();
            let mut source_map = BiMap::new();
            let url = Url::parse("file:///test.mq").unwrap();
            let (source_id, _, _) = hir.add_code(Some(url.clone()), code);
            source_map.insert(url.to_string(), source_id);

            let Some(CompletionResponse::Array(items)) = response(
                Arc::new(RwLock::new(hir)),
                url,
                Position::new(0, 0),
                &source_map,
                "",
                &[],
            ) else {
                panic!("expected completion items for {code:?}");
            };
            let missing = mq_help::BUILTIN_DOC
                .functions()
                .filter(|doc| doc.is_available(mq_lang::is_builtin_function))
                .map(|doc| doc.name)
                .filter(|name| {
                    !items
                        .iter()
                        .any(|item| item.label == *name && item.kind == Some(CompletionItemKind::FUNCTION))
                })
                .collect::<Vec<_>>();
            assert!(
                missing.is_empty(),
                "missing builtin completions for {code:?}: {missing:?}"
            );
            assert!(
                items.iter().any(|item| item.label == "is_dict"),
                "builtin.mq def missing"
            );
            assert!(
                !items.iter().any(|item| item.kind == Some(CompletionItemKind::METHOD)),
                "selectors are offered only after `.`"
            );
            assert!(
                !items.iter().any(|item| item.label == "map_dict"),
                "builtin.mq locals must not leak"
            );
        }
    }
}
