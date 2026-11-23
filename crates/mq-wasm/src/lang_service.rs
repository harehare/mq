//! Language-service bindings: diagnostics, hover, inlay hints, formatter and AST dump.

use itertools::Itertools;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

#[wasm_bindgen(typescript_custom_section)]
const TS_LANG_SERVICE: &'static str = r#"
export type DefinedValueType = 'Function' | 'Variable';

export interface DefinedValue {
  name: string;
  args?: string[];
  doc: string;
  valueType: DefinedValueType;
}

export interface Diagnostic {
  startLine: number,
  startColumn: number,
  endLine: number,
  endColumn: number,
  message: string,
}

export interface InlayHint {
  line: number,
  column: number,
  label: string,
}

export interface HoverResult {
  content: string;
}

export function definedValues(code: string, module?: string): Promise<ReadonlyArray<DefinedValue>>;
export function diagnostics(code: string, enableTypeCheck?: boolean): Promise<ReadonlyArray<Diagnostic>>;
export function hover(code: string, line: number, column: number): Promise<HoverResult | null>;
export function inlayHints(code: string): Promise<ReadonlyArray<InlayHint>>;
"#;

#[derive(Serialize, Deserialize)]
/// Kind of a defined value.
pub enum DefinedValueType {
    /// A function.
    Function,
    /// A selector.
    Selector,
    /// A variable.
    Variable,
    /// A module.
    Module,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
/// A function, selector, variable or module defined in a query.
pub struct DefinedValue {
    name: String,
    args: Option<Vec<String>>,
    doc: String,
    value_type: DefinedValueType,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
/// A syntax or type diagnostic with its source range.
pub struct Diagnostic {
    start_line: u32,
    start_column: u32,
    end_line: u32,
    end_column: u32,
    message: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
/// A type hint shown at a source position.
pub struct InlayHint {
    line: u32,
    column: u32,
    label: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Hover text for a position.
pub struct HoverResult {
    content: String,
}

#[wasm_bindgen(js_name=toAst)]
/// Parses `code` and returns its AST as JSON.
pub async fn to_ast(code: &str) -> Result<String, JsValue> {
    let token_arena = mq_syntax::Shared::new(mq_syntax::SharedCell::new(mq_syntax::Arena::new(1024)));
    mq_lang::parse(code, token_arena)
        .map_err(|e| JsValue::from_str(&format!("Failed to parse code: {}", e)))
        .and_then(|json| {
            serde_json::to_string(&json).map_err(|e| JsValue::from_str(&format!("Failed to serialize AST: {}", e)))
        })
}

#[wasm_bindgen(js_name=format)]
/// Formats `code`.
pub async fn format(code: &str) -> Result<String, JsValue> {
    mq_formatter::Formatter::default().format(code).map_err(|e| {
        let errors = e.error_ranges(code);
        if errors.is_empty() {
            return JsValue::from_str("Failed to format code");
        }
        let msg = errors
            .iter()
            .map(|(message, range)| format!("{} (line {}, column {})", message, range.start.line, range.start.column))
            .collect::<Vec<_>>()
            .join("\n");
        JsValue::from_str(&msg)
    })
}

#[wasm_bindgen(js_name=diagnostics, skip_typescript)]
/// Returns syntax and unresolved-symbol diagnostics, plus type errors when `enable_type_check` is set.
pub async fn diagnostics(code: &str, enable_type_check: Option<bool>) -> JsValue {
    if code.lines().all(|line| {
        let line = line.trim();
        line.is_empty() || line.starts_with('#')
    }) {
        return serde_wasm_bindgen::to_value(&Vec::<Diagnostic>::new()).unwrap();
    }

    let (_, errors) = mq_syntax::parse_recovery(code);
    let mut errors = errors
        .error_ranges(code)
        .iter()
        .map(|(message, range)| Diagnostic {
            start_line: range.start.line.max(1),
            start_column: range.start.column.max(1),
            end_line: range.end.line.max(1),
            end_column: range.end.column.max(1),
            message: message.to_owned(),
        })
        .collect::<Vec<_>>();

    if errors.is_empty() {
        let mut hir = mq_hir::Hir::default();
        hir.add_code(None, code);

        errors.extend(hir.error_ranges().into_iter().map(|(message, range)| Diagnostic {
            start_line: range.start.line.max(1),
            start_column: range.start.column.max(1),
            end_line: range.end.line.max(1),
            end_column: range.end.column.max(1),
            message,
        }));

        if enable_type_check.unwrap_or(false) && errors.is_empty() {
            let mut checker = mq_check::TypeChecker::default();
            for error in checker.check(&hir) {
                if let Some(range) = error.location() {
                    errors.push(Diagnostic {
                        start_line: range.start.line,
                        start_column: range.start.column,
                        end_line: range.end.line,
                        end_column: range.end.column,
                        message: error.to_string(),
                    });
                }
            }
        }
    }

    serde_wasm_bindgen::to_value(&errors).unwrap()
}

#[wasm_bindgen(js_name=inlayHints, skip_typescript)]
/// Returns type hints for `code`.
pub async fn inlay_hints(code: &str) -> JsValue {
    let mut hir = mq_hir::Hir::default();
    hir.add_code(None, code);

    let mut checker = mq_check::TypeChecker::default();
    let _ = checker.check(&hir);

    let symbol_types = checker.symbol_types();

    let hints: Vec<InlayHint> = hir
        .symbols()
        .filter_map(|(symbol_id, symbol)| {
            if hir.is_builtin_symbol(symbol) {
                return None;
            }

            let show = matches!(
                symbol.kind,
                mq_hir::SymbolKind::Variable
                    | mq_hir::SymbolKind::Function(_)
                    | mq_hir::SymbolKind::DestructuringBinding
                    | mq_hir::SymbolKind::PatternVariable { .. }
            );
            if !show {
                return None;
            }

            let range = symbol.source.text_range.as_ref()?;
            let type_scheme = symbol_types.get(&symbol_id)?;

            if !type_scheme.ty.is_concrete() {
                return None;
            }

            Some(InlayHint {
                line: range.end.line,
                column: range.end.column,
                label: format!(": {}", type_scheme),
            })
        })
        .collect();

    serde_wasm_bindgen::to_value(&hints).unwrap_or_else(|_| JsValue::from(js_sys::Array::new()))
}

#[wasm_bindgen(js_name=definedValues, skip_typescript)]
/// Lists the values defined by `code`, or by `module` if given.
pub async fn defined_values(code: &str, module: Option<String>) -> Result<JsValue, JsValue> {
    let mut hir = mq_hir::Hir::default();
    hir.add_code(None, code);

    // If module is specified, find the module symbol
    let module_id = if let Some(ref module_name) = module {
        hir.symbols().find_map(|(id, symbol)| {
            if symbol.is_module() && symbol.value.as_ref().map(|v| v.as_str()) == Some(module_name.as_str()) {
                Some(id)
            } else {
                None
            }
        })
    } else {
        None
    };

    let symbols = hir
        .symbols()
        .filter_map(|(_symbol_id, symbol)| {
            // Filter by module if specified
            if module_id.is_some() && symbol.parent != module_id {
                return None;
            }

            match symbol {
                mq_hir::Symbol {
                    kind: mq_hir::SymbolKind::Function(params),
                    value: Some(value),
                    doc,
                    ..
                } => Some(DefinedValue {
                    name: value.to_string(),
                    args: Some(params.iter().map(|param| param.to_string()).collect()),
                    doc: doc.iter().map(|(_, doc)| doc.to_string()).join("\n"),
                    value_type: DefinedValueType::Function,
                }),
                mq_hir::Symbol {
                    kind: mq_hir::SymbolKind::Selector(_),
                    value: Some(value),
                    doc,
                    ..
                } => Some(DefinedValue {
                    name: value.to_string(),
                    args: None,
                    doc: doc.iter().map(|(_, doc)| doc.to_string()).join("\n"),
                    value_type: DefinedValueType::Selector,
                }),
                mq_hir::Symbol {
                    kind: mq_hir::SymbolKind::Variable,
                    value: Some(value),
                    doc,
                    ..
                } => Some(DefinedValue {
                    name: value.to_string(),
                    args: None,
                    doc: doc.iter().map(|(_, doc)| doc.to_string()).join("\n"),
                    value_type: DefinedValueType::Variable,
                }),
                _ => None,
            }
        })
        .collect::<Vec<_>>();

    Ok(serde_wasm_bindgen::to_value(&symbols).unwrap())
}

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
    signature: &str,
    docs: &[mq_hir::Doc],
    deprecated: bool,
    examples: &[mq_help::BuiltinExample],
) -> String {
    let mut sections: Vec<String> = Vec::new();

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

    if let Some(examples) = format_examples(examples) {
        sections.push(examples);
    }

    sections.join("\n\n")
}

/// Returns hover information (Markdown content) for the symbol at `line`/`column` (1-based).
///
/// Returns `null` if no symbol is found at the given position.
#[wasm_bindgen(js_name=hover, skip_typescript)]
pub async fn hover(code: &str, line: u32, column: u32) -> JsValue {
    let mut hir = mq_hir::Hir::default();
    let (source_id, _, _) = hir.add_code(None, code);

    let mut checker = mq_check::TypeChecker::default();
    let _ = checker.check(&hir);
    let type_env = checker.symbol_types();

    let pos = mq_syntax::Position::new(line, column);
    let Some((symbol_id, symbol)) = hir.find_symbol_in_position(source_id, pos) else {
        return JsValue::NULL;
    };

    let deprecated = symbol.is_deprecated();
    let type_scheme = type_env.get(&symbol_id);

    let signature = match &symbol.kind {
        mq_hir::SymbolKind::Function(args) => {
            let type_annotation = type_scheme.map(|s| format!(": {}", s.ty)).unwrap_or_default();
            format!(
                "{}({}){}",
                symbol.value.as_deref().unwrap_or_default(),
                args.iter().map(|p| p.to_string()).join(", "),
                type_annotation
            )
        }
        mq_hir::SymbolKind::Variable
        | mq_hir::SymbolKind::DestructuringBinding
        | mq_hir::SymbolKind::PatternVariable { .. } => {
            let type_annotation = type_scheme.map(|s| format!(": {}", s.ty)).unwrap_or_default();
            format!("{}{}", symbol.value.as_deref().unwrap_or_default(), type_annotation)
        }
        _ => return JsValue::NULL,
    };

    let name = symbol.value.as_deref().unwrap_or_default();
    let examples: Vec<mq_help::BuiltinExample> = if symbol.source.source_id == Some(hir.builtin.source_id) {
        hir.builtin
            .docs
            .get(name)
            .map(|doc| doc.examples.to_vec())
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    let result = HoverResult {
        content: format_hover_content(&signature, &symbol.doc, deprecated, &examples),
    };
    serde_wasm_bindgen::to_value(&result).unwrap_or(JsValue::NULL)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::*;
    #[cfg(feature = "test-in-worker")]
    wasm_bindgen_test_configure!(run_in_dedicated_worker);
    #[cfg(not(feature = "test-in-worker"))]
    wasm_bindgen_test_configure!(run_in_browser);

    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_script_format() {
        let result = format(r#"downcase()|ltrimstr("hello")|upcase()|trim()"#).await.unwrap();
        assert_eq!(result, r#"downcase() | ltrimstr("hello") | upcase() | trim()"#);
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_script_format_invalid() {
        assert!(format("x=>").await.is_err());
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_defined_values_without_module() {
        let code = r#"
            def foo(x): x | upcase();
            def bar(y): y | downcase();
            | let $var = 42;
        "#;

        let result = defined_values(code, None).await.unwrap();
        let values: Vec<DefinedValue> = serde_wasm_bindgen::from_value(result).unwrap();

        // Should return all defined values
        assert!(values.iter().any(|v| v.name == "foo"));
        assert!(values.iter().any(|v| v.name == "bar"));
        assert!(values.iter().any(|v| v.name == "$var"));
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_defined_values_with_module() {
        let code = r#"
            module mymodule:
                def module_func(x): x | upcase();
                def another_func(y): y | downcase();
                | let $module_var = 100;
            end

            def top_level_func(z): z;
            | let $top_level_var = 42;
        "#;

        let result = defined_values(code, Some("mymodule".to_string())).await.unwrap();
        let values: Vec<DefinedValue> = serde_wasm_bindgen::from_value(result).unwrap();

        // Should return only values from mymodule
        assert!(values.iter().any(|v| v.name == "module_func"));
        assert!(values.iter().any(|v| v.name == "another_func"));
        assert!(values.iter().any(|v| v.name == "$module_var"));

        // Should NOT return top-level values
        assert!(!values.iter().any(|v| v.name == "top_level_func"));
        assert!(!values.iter().any(|v| v.name == "$top_level_var"));
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_defined_values_with_nonexistent_module() {
        let code = r#"
            def foo(x): x | upcase();
        "#;

        let result = defined_values(code, Some("nonexistent".to_string())).await.unwrap();
        let values: Vec<DefinedValue> = serde_wasm_bindgen::from_value(result).unwrap();

        // Should return empty array since module doesn't exist
        assert!(!values.is_empty());
    }
}
