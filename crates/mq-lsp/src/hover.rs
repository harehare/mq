use std::sync::{Arc, RwLock};

use itertools::Itertools;
use tower_lsp_server::ls_types::{Hover, HoverContents, MarkupContent, MarkupKind, Position, Range};
use url::Url;

/// Returns `true` if a doc line is a deprecation marker.
///
/// A line is considered a deprecation marker if its trimmed content starts
/// with `deprecated` (case-insensitive) followed by `:`, a space, or end of string.
/// This avoids false-positives for lines that merely *mention* "deprecated".
fn is_deprecated_marker(text: &str) -> bool {
    let trimmed = text.trim();
    let lower = trimmed.to_lowercase();
    lower == "deprecated" || lower.starts_with("deprecated:") || lower.starts_with("deprecated ")
}

/// Extracts the human-readable message from a deprecation marker line, if any.
///
/// For example `"deprecated: use foo instead"` returns `Some("use foo instead")`.
fn extract_deprecated_message(text: &str) -> Option<String> {
    let after_colon = text.trim().split_once(':')?.1.trim();
    if after_colon.is_empty() {
        None
    } else {
        Some(after_colon.to_string())
    }
}

/// Builds a Markdown hover string from a kind label, name, signature, doc comments,
/// deprecation status, and optional parameter list.
///
/// The layout is:
/// - A heading with the symbol name and kind (e.g. `## \`len\` — function`)
/// - A fenced `mq` code block containing the full signature
/// - An optional blockquote deprecation notice (when `deprecated` is `true`)
/// - An optional `---` separator followed by non-deprecated doc lines
/// - An optional `### Parameters` section listing each parameter
fn format_hover_content(
    kind_label: &str,
    name: &str,
    signature: &str,
    docs: &[mq_hir::Doc],
    deprecated: bool,
    params: &[mq_hir::ParamInfo],
) -> String {
    let mut sections: Vec<String> = Vec::new();

    sections.push(format!("### `{}` — {}", name, kind_label));
    sections.push(format!("```mq\n{}\n```", signature));

    if deprecated {
        let dep_msg = docs
            .iter()
            .find(|(_, text)| is_deprecated_marker(text))
            .and_then(|(_, text)| extract_deprecated_message(text));

        match dep_msg {
            Some(msg) => sections.push(format!("> ⚠️ **Deprecated**: {}", msg)),
            None => sections.push("> ⚠️ **Deprecated**".to_string()),
        }
    }

    let doc_text = docs
        .iter()
        .filter(|(_, text)| !is_deprecated_marker(text))
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


    sections.join("\n\n")
}

pub fn response(
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
                | mq_hir::SymbolKind::Macro(_)
                | mq_hir::SymbolKind::Variable
                | mq_hir::SymbolKind::DestructuringBinding
                | mq_hir::SymbolKind::PatternVariable { .. } => {
                    let deprecated = symbol.is_deprecated();
                    let type_scheme = type_env.as_ref().and_then(|env| env.get(&symbol_id));
                    let name = symbol.value.as_deref().unwrap_or_default();

                    let (kind_label, signature, params) = match &symbol.kind {
                        mq_hir::SymbolKind::Function(args) | mq_hir::SymbolKind::Macro(args) => {
                            let kind_label = if matches!(symbol.kind, mq_hir::SymbolKind::Function(_)) {
                                "function"
                            } else {
                                "macro"
                            };
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
                            value: format_hover_content(kind_label, name, &signature, &symbol.doc, deprecated, &params),
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

    // --- unit tests for helpers ---

    #[test]
    fn test_is_deprecated_marker_variants() {
        assert!(is_deprecated_marker("deprecated"));
        assert!(is_deprecated_marker("deprecated:"));
        assert!(is_deprecated_marker("deprecated: use foo instead"));
        assert!(is_deprecated_marker("Deprecated: use foo instead"));
        assert!(is_deprecated_marker("DEPRECATED: old"));
        assert!(is_deprecated_marker("  deprecated:  "));
        // NOT markers – these merely mention "deprecated"
        assert!(!is_deprecated_marker("use this instead of the deprecated foo API"));
        assert!(!is_deprecated_marker("see the deprecated section below"));
    }

    #[test]
    fn test_extract_deprecated_message() {
        assert_eq!(
            extract_deprecated_message("deprecated: use foo instead"),
            Some("use foo instead".to_string())
        );
        assert_eq!(extract_deprecated_message("deprecated:"), None);
        assert_eq!(extract_deprecated_message("deprecated"), None);
        assert_eq!(
            extract_deprecated_message("  Deprecated:  trimmed message  "),
            Some("trimmed message".to_string())
        );
    }

    #[test]
    fn test_format_hover_content_no_docs() {
        let docs: Vec<mq_hir::Doc> = vec![];
        let result = format_hover_content("function", "func", "func()", &docs, false, &[]);
        assert!(result.contains("## `func` — function"));
        assert!(result.contains("```mq\nfunc()\n```"));
    }

    #[test]
    fn test_format_hover_content_with_docs() {
        let docs: Vec<mq_hir::Doc> = vec![(Default::default(), "Returns a value.".to_string())];
        let result = format_hover_content("function", "func", "func()", &docs, false, &[]);
        assert!(result.contains("## `func` — function"));
        assert!(result.contains("```mq\nfunc()\n```"));
        assert!(result.contains("---"));
        assert!(result.contains("Returns a value."));
    }

    #[test]
    fn test_format_hover_content_deprecated_only() {
        let docs: Vec<mq_hir::Doc> = vec![(Default::default(), "deprecated: use bar instead".to_string())];
        let result = format_hover_content("function", "func", "func()", &docs, true, &[]);
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
        let result = format_hover_content("function", "func", "func()", &docs, true, &[]);
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
        let result = format_hover_content("function", "func", "func()", &docs, false, &[]);
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
        let result = format_hover_content("function", "func", "func(a, b, *rest)", &docs, false, &params);
        assert!(result.contains("### Parameters"));
        assert!(result.contains("- `a`"));
        assert!(result.contains("- `b` *(optional)*"));
        assert!(result.contains("- `*rest` *(variadic)*"));
    }


    // --- integration tests via response() ---

    #[test]
    fn test_function_hover() {
        let mut hir = Hir::default();
        let url = Url::parse("file:///test.mq").unwrap();
        let position = Position::new(0, 5);
        hir.add_code(Some(url.clone()), "def func1(): 1;");

        let hover = response(Arc::new(RwLock::new(hir)), url, None, position);

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

        let hover = response(Arc::new(RwLock::new(hir)), url, None, position);

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

        let hover = response(Arc::new(RwLock::new(hir)), url, None, position);
        assert!(hover.is_none());
    }

    #[test]
    fn test_invalid_url() {
        let hir = Hir::default();
        let url = Url::parse("file:///nonexistent.mq").unwrap();
        let position = Position::new(0, 0);

        let hover = response(Arc::new(RwLock::new(hir)), url, None, position);
        assert!(hover.is_none());
    }

    #[test]
    fn test_builtin_function_hover() {
        let mut hir = Hir::default();
        let url = Url::parse("file:///test.mq").unwrap();

        hir.add_code(Some(url.clone()), "\"hello\" | len");

        let position = Position::new(0, 11);
        let hover = response(Arc::new(RwLock::new(hir)), url, None, position);

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
        let hover = response(Arc::new(RwLock::new(hir)), url, None, position);

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
        let hover = response(Arc::clone(&hir), url, type_env, position);

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
}
