use std::sync::{Arc, RwLock};

use itertools::Itertools;
use tower_lsp_server::ls_types::{Hover, HoverContents, MarkupContent, MarkupKind, Position, Range};
use url::Url;

fn format_examples(examples: &[mq_help::BuiltinExample]) -> Option<String> {
    if examples.is_empty() {
        return None;
    }

    let items = examples
        .iter()
        .map(|example| format!("```mq\n{}\n```\n\n`#=>` `{}`", example.code, example.expected))
        .collect::<Vec<_>>()
        .join("\n\n");

    Some(format!("### Examples\n\n{}", items))
}

fn format_hover_content(
    kind_label: &str,
    name: &str,
    signature: &str,
    docs: &[mq_hir::Doc],
    deprecated: bool,
    params: &[mq_hir::ParamInfo],
    examples: &[mq_help::BuiltinExample],
) -> String {
    let mut sections: Vec<String> = Vec::new();

    sections.push(format!("### `{}` — {}", name, kind_label));
    sections.push(format!("```mq\n{}\n```", signature));

    if deprecated {
        let dep_msg = docs
            .iter()
            .find(|(_, text)| mq_hir::is_deprecation_marker(text))
            .and_then(|(_, text)| mq_hir::deprecation_message(text));

        match dep_msg {
            Some(msg) => sections.push(format!("> ⚠️ **Deprecated**: {}", msg)),
            None => sections.push("> ⚠️ **Deprecated**".to_string()),
        }
    }

    let doc_text = docs
        .iter()
        .filter(|(_, text)| !mq_hir::is_deprecation_marker(text))
        .map(|(_, text)| text.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    if !doc_text.trim().is_empty() {
        sections.push("---".to_string());
        sections.push(doc_text);
    }

    if !params.is_empty() {
        let param_items = params
            .iter()
            .map(|p| {
                if p.is_variadic {
                    format!("- `*{}` *(variadic)*", p.name)
                } else if p.has_default {
                    format!("- `{}` *(optional)*", p.name)
                } else {
                    format!("- `{}`", p.name)
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        sections.push(format!("### Parameters\n{}", param_items));
    }

    if let Some(examples) = format_examples(examples) {
        sections.push(examples);
    }

    sections.join("\n\n")
}

/// Returns the word under the cursor, keeping a selector's leading `.`.
fn word_at(line: &str, character: u32) -> Option<String> {
    let chars = line.chars().collect::<Vec<_>>();
    let mut units = 0;
    let cursor = chars
        .iter()
        .position(|c| {
            let hit = units >= character;
            units += c.len_utf16() as u32;
            hit
        })
        .unwrap_or(chars.len());
    let is_word = |c: &char| c.is_alphanumeric() || *c == '_';
    // The cursor on a selector's `.` belongs to the name after it.
    let cursor = if chars.get(cursor) == Some(&'.') && chars.get(cursor + 1).is_some_and(is_word) {
        cursor + 1
    } else {
        cursor
    };

    let mut start = cursor;
    while start > 0 && is_word(&chars[start - 1]) {
        start -= 1;
    }
    let mut end = cursor;
    while end < chars.len() && is_word(&chars[end]) {
        end += 1;
    }
    if start > 0
        && chars[start - 1] == '.'
        && !(start > 1 && (is_word(&chars[start - 2]) || matches!(chars[start - 2], ')' | ']' | '"')))
    {
        start -= 1;
    }
    (start < end).then(|| chars[start..end].iter().collect())
}

fn help_hover(name: &str) -> Option<Hover> {
    let entries = mq_help::lookup(name);
    let value = if entries.is_empty() {
        mq_help::render_module_markdown(&mq_help::lookup_module(name)?)
    } else {
        entries.iter().map(mq_help::render_markdown).join("\n\n---\n\n")
    };
    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value,
        }),
        range: None,
    })
}

/// Hover for the symbol under the cursor, falling back to the `mq-help` catalog for
/// selectors and keywords, which have no declaration in the HIR.
pub(crate) fn response(
    hir: Arc<RwLock<mq_hir::Hir>>,
    url: Url,
    type_env: Option<mq_check::TypeEnv>,
    position: Position,
    line: &str,
) -> Option<Hover> {
    hir_hover(hir, url, type_env, position).or_else(|| help_hover(&word_at(line, position.character)?))
}

fn hir_hover(
    hir: Arc<RwLock<mq_hir::Hir>>,
    url: Url,
    type_env: Option<mq_check::TypeEnv>,
    position: Position,
) -> Option<Hover> {
    let source = hir.read().unwrap().source_by_url(&url);

    if let Some(source) = source {
        if let Some((symbol_id, symbol)) = hir.read().unwrap().find_symbol_in_position(
            source,
            mq_lang::Position::new(position.line + 1, (position.character + 1) as usize),
        ) {
            match &symbol.kind {
                mq_hir::SymbolKind::Function(_)
                | mq_hir::SymbolKind::Variable
                | mq_hir::SymbolKind::DestructuringBinding
                | mq_hir::SymbolKind::PatternVariable { .. } => {
                    let deprecated = symbol.is_deprecated();
                    let type_scheme = type_env.as_ref().and_then(|env| env.get(&symbol_id));
                    let name = symbol.value.as_deref().unwrap_or_default();

                    let examples: Vec<mq_help::BuiltinExample> = {
                        let hir = hir.read().unwrap();
                        if symbol.source.source_id == Some(hir.builtin.source_id) {
                            hir.builtin
                                .docs
                                .get(name)
                                .map(|doc| doc.examples.to_vec())
                                .unwrap_or_default()
                        } else {
                            Vec::new()
                        }
                    };

                    let (kind_label, signature, params) = match &symbol.kind {
                        mq_hir::SymbolKind::Function(args) => {
                            let kind_label = "function";
                            let type_annotation = type_scheme.map(|s| format!(": {}", s.ty)).unwrap_or_default();
                            let sig = format!(
                                "{}({}){}",
                                name,
                                args.iter().map(|p| p.to_string()).join(", "),
                                type_annotation
                            );
                            (kind_label, sig, args.clone())
                        }
                        _ => {
                            let type_annotation = type_scheme.map(|s| format!(": {}", s.ty)).unwrap_or_default();
                            ("variable", format!("{}{}", name, type_annotation), Vec::new())
                        }
                    };

                    Some(Hover {
                        contents: HoverContents::Markup(MarkupContent {
                            kind: MarkupKind::Markdown,
                            value: format_hover_content(
                                kind_label,
                                name,
                                &signature,
                                &symbol.doc,
                                deprecated,
                                &params,
                                &examples,
                            ),
                        }),
                        range: symbol.source.text_range.map(|text_range| {
                            Range::new(
                                Position::new(text_range.start.line - 1, (text_range.start.column - 1) as u32),
                                Position::new(text_range.end.line - 1, (text_range.end.column - 1) as u32),
                            )
                        }),
                    })
                }
                _ => None,
            }
        } else {
            None
        }
    } else {
        None
    }
}
#[cfg(test)]
mod tests {
    use mq_hir::Hir;
    use tower_lsp_server::ls_types;

    use super::*;

    #[test]
    fn test_format_hover_content_no_docs() {
        let docs: Vec<mq_hir::Doc> = vec![];
        let result = format_hover_content("function", "func", "func()", &docs, false, &[], &[]);
        assert!(result.contains("## `func` — function"));
        assert!(result.contains("```mq\nfunc()\n```"));
    }

    #[test]
    fn test_format_hover_content_with_docs() {
        let docs: Vec<mq_hir::Doc> = vec![(Default::default(), "Returns a value.".to_string())];
        let result = format_hover_content("function", "func", "func()", &docs, false, &[], &[]);
        assert!(result.contains("## `func` — function"));
        assert!(result.contains("```mq\nfunc()\n```"));
        assert!(result.contains("---"));
        assert!(result.contains("Returns a value."));
    }

    #[test]
    fn test_format_hover_content_deprecated_only() {
        let docs: Vec<mq_hir::Doc> = vec![(Default::default(), "deprecated: use bar instead".to_string())];
        let result = format_hover_content("function", "func", "func()", &docs, true, &[], &[]);
        assert!(result.contains("> ⚠️ **Deprecated**: use bar instead"));
        // The deprecated line itself should NOT appear again in the doc section
        assert!(
            !result.contains("---"),
            "No doc section when only doc is the deprecated line"
        );
    }

    #[test]
    fn test_format_hover_content_deprecated_with_extra_docs() {
        let docs: Vec<mq_hir::Doc> = vec![
            (Default::default(), "deprecated: use bar instead".to_string()),
            (Default::default(), "Some extra context.".to_string()),
        ];
        let result = format_hover_content("function", "func", "func()", &docs, true, &[], &[]);
        assert!(result.contains("> ⚠️ **Deprecated**: use bar instead"));
        assert!(result.contains("---"));
        assert!(result.contains("Some extra context."));
        // The deprecated line should not appear in the doc body
        assert!(!result.contains("deprecated: use bar"));
    }

    #[test]
    fn test_format_hover_content_doc_mentioning_deprecated() {
        // A doc line that merely mentions "deprecated" is NOT filtered
        let docs: Vec<mq_hir::Doc> = vec![(Default::default(), "Replaces the deprecated foo API.".to_string())];
        let result = format_hover_content("function", "func", "func()", &docs, false, &[], &[]);
        assert!(result.contains("Replaces the deprecated foo API."));
    }

    #[test]
    fn test_format_hover_content_with_params() {
        let docs: Vec<mq_hir::Doc> = vec![];
        let params = vec![
            mq_hir::ParamInfo::from("a"),
            mq_hir::ParamInfo {
                name: "b".into(),
                has_default: true,
                is_variadic: false,
            },
            mq_hir::ParamInfo {
                name: "rest".into(),
                has_default: false,
                is_variadic: true,
            },
        ];
        let result = format_hover_content("function", "func", "func(a, b, *rest)", &docs, false, &params, &[]);
        assert!(result.contains("### Parameters"));
        assert!(result.contains("- `a`"));
        assert!(result.contains("- `b` *(optional)*"));
        assert!(result.contains("- `*rest` *(variadic)*"));
    }

    #[test]
    fn test_format_hover_content_with_examples() {
        let docs: Vec<mq_hir::Doc> = vec![];
        let examples = vec![mq_help::BuiltinExample {
            code: r#"len("hello")"#,
            expected: "5",
        }];
        let result = format_hover_content("function", "len", "len(value)", &docs, false, &[], &examples);
        assert!(result.contains("### Examples"));
        assert!(result.contains("```mq\nlen(\"hello\")\n```"));
        assert!(result.contains("5"));
    }

    #[test]
    fn test_format_hover_content_no_examples_section_when_empty() {
        let docs: Vec<mq_hir::Doc> = vec![];
        let result = format_hover_content("function", "func", "func()", &docs, false, &[], &[]);
        assert!(!result.contains("### Examples"));
    }

    #[test]
    fn test_function_hover() {
        let mut hir = Hir::default();
        let url = Url::parse("file:///test.mq").unwrap();
        let position = Position::new(0, 5);
        hir.add_code(Some(url.clone()), "def func1(): 1;");

        let hover = response(Arc::new(RwLock::new(hir)), url, None, position, "");

        assert!(hover.is_some());
        let hover = hover.unwrap();

        if let HoverContents::Markup(content) = hover.contents {
            assert_eq!(content.kind, MarkupKind::Markdown);
            assert!(content.value.contains("## `func1` — function"));
            assert!(content.value.contains("func1()"));
            assert!(content.value.contains("```mq"));
        } else {
            panic!("Expected markup content");
        }
    }

    #[test]
    fn test_val_hover() {
        let mut hir = Hir::default();
        let url = Url::parse("file:///test.mq").unwrap();
        let position = Position::new(0, 5);
        hir.add_code(Some(url.clone()), "let val = 1 | val");

        let hover = response(Arc::new(RwLock::new(hir)), url, None, position, "");

        assert!(hover.is_some());
        let hover = hover.unwrap();

        if let HoverContents::Markup(content) = hover.contents {
            assert_eq!(content.kind, MarkupKind::Markdown);
            assert!(content.value.contains("## `val` — variable"));
            assert!(content.value.contains("```mq"));
        } else {
            panic!("Expected markup content");
        }
    }

    #[test]
    fn test_no_symbol_at_position() {
        let hir = Hir::default();
        let url = Url::parse("file:///test.mq").unwrap();
        let position = Position::new(10, 10);

        let hover = response(Arc::new(RwLock::new(hir)), url, None, position, "");
        assert!(hover.is_none());
    }

    #[test]
    fn test_invalid_url() {
        let hir = Hir::default();
        let url = Url::parse("file:///nonexistent.mq").unwrap();
        let position = Position::new(0, 0);

        let hover = response(Arc::new(RwLock::new(hir)), url, None, position, "");
        assert!(hover.is_none());
    }

    #[test]
    fn test_builtin_function_hover() {
        let mut hir = Hir::default();
        let url = Url::parse("file:///test.mq").unwrap();

        hir.add_code(Some(url.clone()), "\"hello\" | len");

        let position = Position::new(0, 11);
        let hover = response(Arc::new(RwLock::new(hir)), url, None, position, "");

        assert!(hover.is_some());
        let hover = hover.unwrap();

        if let ls_types::HoverContents::Markup(content) = hover.contents {
            assert_eq!(content.kind, ls_types::MarkupKind::Markdown);
            assert!(
                content.value.contains("## `len` — function"),
                "Should contain heading with kind"
            );
            assert!(
                content.value.contains("len(value)"),
                "Should contain function signature"
            );
            assert!(
                content
                    .value
                    .contains("Returns the length of the given string or array"),
                "Should contain description"
            );
            assert!(
                content.value.contains("### Examples"),
                "Should contain an Examples section for a documented builtin"
            );
            assert!(
                content.value.contains(r#"len("hello")"#),
                "Should contain the example code"
            );
        } else {
            panic!("Expected markup content");
        }
    }

    #[test]
    fn test_deprecated_function_hover() {
        let mut hir = Hir::default();
        let url = Url::parse("file:///test.mq").unwrap();

        let code = r#"# deprecated: This function is no longer supported
def old_func(x): x + 1;"#;
        hir.add_code(Some(url.clone()), code);

        let position = Position::new(1, 5);
        let hover = response(Arc::new(RwLock::new(hir)), url, None, position, "");

        assert!(hover.is_some());
        let hover = hover.unwrap();

        if let ls_types::HoverContents::Markup(content) = hover.contents {
            assert_eq!(content.kind, ls_types::MarkupKind::Markdown);
            assert!(
                content.value.contains("## `old_func` — function"),
                "Should contain heading with kind"
            );
            assert!(
                content.value.contains("⚠️ **Deprecated**"),
                "Should contain deprecated blockquote marker"
            );
            assert!(
                content.value.contains("This function is no longer supported"),
                "Should contain deprecation message"
            );
            // The raw deprecated line should not appear verbatim in the doc body
            assert!(
                !content.value.contains("deprecated: This"),
                "Deprecated line should not repeat in doc body"
            );
        } else {
            panic!("Expected markup content");
        }
    }

    #[test]
    fn test_hover_with_type_info() {
        use mq_check::TypeChecker;

        let mut hir = Hir::default();
        let url = Url::parse("file:///test.mq").unwrap();
        hir.add_code(Some(url.clone()), "let val = 1 | val");

        let hir = Arc::new(RwLock::new(hir));
        let mut checker = TypeChecker::new();
        checker.check(&hir.read().unwrap());
        let type_env = Some(checker.symbol_types().clone());

        let position = Position::new(0, 5);
        let hover = response(Arc::clone(&hir), url, type_env, position, "");

        assert!(hover.is_some());
        let hover = hover.unwrap();

        if let HoverContents::Markup(content) = hover.contents {
            assert_eq!(content.kind, MarkupKind::Markdown);
            assert!(content.value.contains("## `val` — variable"));
            assert!(content.value.contains(":"), "Should contain type annotation");
        } else {
            panic!("Expected markup content");
        }
    }

    #[rstest::rstest]
    #[case("| .h", 3, Some(".h"))]
    #[case(".h", 0, Some(".h"))]
    #[case("let x = 1", 1, Some("let"))]
    #[case("x.foo", 3, Some("foo"))]
    #[case("あ | upcase", 7, Some("upcase"))]
    #[case("a | b", 3, None)]
    fn test_word_at(#[case] line: &str, #[case] character: u32, #[case] expected: Option<&str>) {
        assert_eq!(word_at(line, character).as_deref(), expected);
    }

    fn hover_text(code: &str, position: Position) -> Option<String> {
        let mut hir = Hir::default();
        let url = Url::parse("file:///test.mq").unwrap();
        hir.add_code(Some(url.clone()), code);
        let line = code.lines().nth(position.line as usize).unwrap_or_default();
        match response(Arc::new(RwLock::new(hir)), url, None, position, line)?.contents {
            HoverContents::Markup(markup) => Some(markup.value),
            _ => None,
        }
    }

    #[test]
    fn test_hover_selector_uses_the_help_catalog() {
        let text = hover_text(".h", Position::new(0, 1)).expect("selector hover");
        assert!(text.contains(".h"), "{text}");
    }

    #[test]
    fn test_hover_keyword_and_module_use_the_help_catalog() {
        assert!(hover_text("nodes | len", Position::new(0, 2)).is_some());
        let text = hover_text("import \"section\"", Position::new(0, 10)).expect("module hover");
        assert!(text.contains("section"), "{text}");
    }

    #[test]
    fn test_hover_unknown_word_is_none() {
        assert!(hover_text("zzz_unknown", Position::new(0, 2)).is_none());
    }
}
