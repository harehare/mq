use std::str::FromStr;

use itertools::Itertools;
use serde::{Deserialize, Serialize};
use url::Url;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(typescript_custom_section)]
const TS_CUSTOM_SECTION: &'static str = r#"
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

export function definedValues(code: string): ReadonlyArray<DefinedValue>;
export function diagnostics(code: string): ReadonlyArray<Diagnostic>;
"#;

#[derive(Serialize, Deserialize)]
pub enum DefinedValueType {
    Function,
    Selector,
    Variable,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DefinedValue {
    name: String,
    args: Option<Vec<String>>,
    doc: String,
    value_type: DefinedValueType,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    start_line: u32,
    start_column: u32,
    end_line: u32,
    end_column: u32,
    message: String,
}

#[wasm_bindgen(js_name=runScript)]
pub fn run_script(code: &str, content: &str, mdx: bool) -> Result<String, JsValue> {
    let mut engine = mq_lang::Engine::default();
    engine.load_builtin_module().unwrap();
    let markdown = if mdx {
        mq_markdown::Markdown::from_mdx_str(content)
    } else {
        mq_markdown::Markdown::from_str(content)
    };

    markdown
        .map_err(|e| JsValue::from_str(&e.to_string()))
        .and_then(move |markdown| {
            engine
                .eval(code, markdown.nodes.into_iter().map(mq_lang::Value::from))
                .map_err(|e| JsValue::from_str(&format!("{}", &e.cause)))
                .map(|r| {
                    let markdown = mq_markdown::Markdown::new(
                        r.into_iter()
                            .map(|runtime_value| match runtime_value {
                                mq_lang::Value::Markdown(node) => node.clone(),
                                _ => runtime_value.to_string().into(),
                            })
                            .collect(),
                    );
                    markdown.to_string()
                })
        })
}

#[wasm_bindgen(js_name=formatScript)]
pub fn format_script(code: &str) -> Result<String, JsValue> {
    mq_formatter::Formatter::default()
        .format(code)
        .map_err(|e| JsValue::from_str(&format!("{:?}", &e)))
}

#[wasm_bindgen(js_name=diagnostics, skip_typescript)]
pub fn diagnostics(code: &str) -> JsValue {
    let (_, errors) = mq_lang::parse_recovery(code);
    let errors = errors
        .error_ranges(code)
        .iter()
        .map(|(message, range)| Diagnostic {
            start_line: range.start.line,
            start_column: range.start.column as u32,
            end_line: range.end.line,
            end_column: range.end.column as u32,
            message: message.to_owned(),
        })
        .collect::<Vec<_>>();

    serde_wasm_bindgen::to_value(&errors).unwrap()
}

#[wasm_bindgen(js_name=definedValues, skip_typescript)]
pub fn defined_values(code: &str) -> Result<JsValue, JsValue> {
    let mut hir = mq_hir::Hir::default();
    let file = Url::parse("file:///").unwrap();
    hir.add_code(file, code);

    let symbols = hir
        .symbols()
        .filter_map(|(_, symbol)| match symbol {
            mq_hir::Symbol {
                kind: mq_hir::SymbolKind::Function(params),
                name: Some(name),
                doc,
                ..
            } => Some(DefinedValue {
                name: name.to_string(),
                args: Some(params.iter().map(|param| param.to_string()).collect()),
                doc: doc.iter().map(|(_, doc)| doc.to_string()).join("\n"),
                value_type: DefinedValueType::Function,
            }),
            mq_hir::Symbol {
                kind: mq_hir::SymbolKind::Selector,
                name: Some(name),
                doc,
                ..
            } => Some(DefinedValue {
                name: name.to_string(),
                args: None,
                doc: doc.iter().map(|(_, doc)| doc.to_string()).join("\n"),
                value_type: DefinedValueType::Selector,
            }),
            mq_hir::Symbol {
                kind: mq_hir::SymbolKind::Variable,
                name: Some(name),
                doc,
                ..
            } => Some(DefinedValue {
                name: name.to_string(),
                args: None,
                doc: doc.iter().map(|(_, doc)| doc.to_string()).join("\n"),
                value_type: DefinedValueType::Variable,
            }),
            _ => None,
        })
        .collect::<Vec<_>>();

    Ok(serde_wasm_bindgen::to_value(&symbols).unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::*;
    wasm_bindgen_test_configure!(run_in_browser);

    #[allow(unused)]
    #[wasm_bindgen_test]
    fn test_script_run_simple() {
        let result = run_script(
            "downcase() | ltrimstr(\"hello\") | upcase() | trim()",
            "Hello world",
            false,
        );
        assert_eq!(result.unwrap(), "WORLD\n");
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    fn test_script_run_invalid_syntax() {
        assert!(run_script("invalid syntax", "test", false).is_err());
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    fn test_script_format() {
        let result = format_script("downcase()|ltrimstr(\"hello\")|upcase()|trim()").unwrap();
        assert_eq!(
            result,
            "downcase() | ltrimstr(\"hello\") | upcase() | trim()"
        );
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    fn test_script_format_invalid() {
        assert!(format_script("x=>").is_err());
    }
}
