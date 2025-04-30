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

export interface RunOptions {
    isMdx: boolean,
    isUpdate: boolean,
    inputFormat: 'html' | 'markdown' | 'text' | null,
    listStyle: 'dash' | 'plus' | 'star' | null,
    linkTitleStyle: 'double' | 'single' | 'paren' | null,
    linkUrlStyle: 'angle' | 'none' | null,
}

export function definedValues(code: string): ReadonlyArray<DefinedValue>;
export function diagnostics(code: string): ReadonlyArray<Diagnostic>;
export function runScript(code: string, content: string, options: RunOptions): string;
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

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen(js_name=RunOptions, skip_typescript)]
struct RunOptions {
    is_mdx: bool,
    is_update: bool,
    input_format: Option<InputFormat>,
    list_style: Option<ListStyle>,
    link_title_style: Option<TitleSurroundStyle>,
    link_url_style: Option<UrlSurroundStyle>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InputFormat {
    #[serde(rename = "html")]
    Html,
    #[serde(rename = "markdown")]
    Markdown,
    #[serde(rename = "text")]
    Text,
}

impl FromStr for InputFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "html" => Ok(Self::Html),
            "markdown" => Ok(Self::Markdown),
            _ => Err(format!("Unknown input format: {}", s)),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ListStyle {
    #[serde(rename = "dash")]
    Dash,
    #[serde(rename = "plus")]
    Plus,
    #[serde(rename = "star")]
    Star,
}

impl FromStr for ListStyle {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "dash" => Ok(Self::Dash),
            "plus" => Ok(Self::Plus),
            "star" => Ok(Self::Plus),
            _ => Err(format!("Unknown list style: {}", s)),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TitleSurroundStyle {
    #[serde(rename = "double")]
    Double,
    #[serde(rename = "single")]
    Single,
    #[serde(rename = "paren")]
    Paren,
}

impl FromStr for TitleSurroundStyle {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "double" => Ok(Self::Double),
            "single" => Ok(Self::Single),
            "paren" => Ok(Self::Paren),
            _ => Err(format!("Unknown title surround style: {}", s)),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UrlSurroundStyle {
    #[serde(rename = "angle")]
    Angle,
    #[serde(rename = "none")]
    None,
}

impl FromStr for UrlSurroundStyle {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "angle" => Ok(Self::Angle),
            "none" => Ok(Self::None),
            _ => Err(format!("Unknown URL surround style: {}", s)),
        }
    }
}

#[wasm_bindgen(js_name=runScript, skip_typescript)]
pub fn run_script(code: &str, content: &str, options: JsValue) -> Result<String, JsValue> {
    let options: RunOptions = serde_wasm_bindgen::from_value(options)
        .map_err(|e| JsValue::from_str(&format!("Failed to parse options: {}", e)))?;

    let is_mdx = options.is_mdx;
    let is_update = options.is_update;
    let mut engine = mq_lang::Engine::default();

    engine.load_builtin_module();

    let input = match options.input_format {
        Some(InputFormat::Text) => content
            .lines()
            .map(mq_lang::Value::from)
            .collect::<Vec<_>>(),
        _ => {
            let md = match options.input_format {
                Some(InputFormat::Html) => mq_markdown::Markdown::from_html(content),
                Some(InputFormat::Markdown) if is_mdx => {
                    mq_markdown::Markdown::from_mdx_str(content)
                }
                _ => mq_markdown::Markdown::from_str(content),
            }
            .map_err(|e| JsValue::from_str(&e.to_string()))?;

            md.nodes
                .into_iter()
                .map(mq_lang::Value::from)
                .collect::<Vec<_>>()
        }
    };

    engine
        .eval(code, input.clone().into_iter())
        .map_err(|e| JsValue::from_str(&format!("{}", &e.cause)))
        .map(|result_values| {
            let values = if matches!(options.input_format, Some(InputFormat::Markdown)) && is_update
            {
                let values: mq_lang::Values = input.into();
                values.update_with(result_values)
            } else {
                result_values
            };

            let mut markdown = mq_markdown::Markdown::new(
                values
                    .into_iter()
                    .map(|runtime_value| match runtime_value {
                        mq_lang::Value::Markdown(node) => node.clone(),
                        _ => runtime_value.to_string().into(),
                    })
                    .collect(),
            );
            markdown.set_options(mq_markdown::RenderOptions {
                list_style: options
                    .list_style
                    .map(|style| match style {
                        ListStyle::Dash => mq_markdown::ListStyle::Dash,
                        ListStyle::Plus => mq_markdown::ListStyle::Plus,
                        ListStyle::Star => mq_markdown::ListStyle::Star,
                    })
                    .unwrap_or_default(),
                link_title_style: options
                    .link_title_style
                    .map(|style| match style {
                        TitleSurroundStyle::Double => mq_markdown::TitleSurroundStyle::Double,
                        TitleSurroundStyle::Single => mq_markdown::TitleSurroundStyle::Single,
                        TitleSurroundStyle::Paren => mq_markdown::TitleSurroundStyle::Paren,
                    })
                    .unwrap_or_default(),
                link_url_style: options
                    .link_url_style
                    .map(|style| match style {
                        UrlSurroundStyle::Angle => mq_markdown::UrlSurroundStyle::Angle,
                        UrlSurroundStyle::None => mq_markdown::UrlSurroundStyle::None,
                    })
                    .unwrap_or_default(),
            });
            markdown.to_string()
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
                kind: mq_hir::SymbolKind::Selector,
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
            serde_wasm_bindgen::to_value(&RunOptions {
                is_mdx: false,
                is_update: true,
                input_format: None,
                list_style: None,
                link_title_style: None,
                link_url_style: None,
            })
            .unwrap(),
        );
        assert_eq!(result.unwrap(), "WORLD\n");
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    fn test_script_run_list() {
        let result = run_script(
            ".[]",
            "- test",
            serde_wasm_bindgen::to_value(&RunOptions {
                is_mdx: false,
                is_update: true,
                input_format: None,
                list_style: Some(ListStyle::Star),
                link_title_style: None,
                link_url_style: None,
            })
            .unwrap(),
        );
        assert_eq!(result.unwrap(), "* test\n");
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    fn test_script_run_link() {
        let result = run_script(
            ".link",
            "[test](https://example.com)",
            serde_wasm_bindgen::to_value(&RunOptions {
                is_mdx: false,
                is_update: true,
                input_format: None,
                list_style: None,
                link_title_style: None,
                link_url_style: Some(UrlSurroundStyle::Angle),
            })
            .unwrap(),
        );
        assert_eq!(result.unwrap(), "[test](<https://example.com>)\n");
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    fn test_script_run_invalid_syntax() {
        assert!(
            run_script(
                "invalid syntax",
                "test",
                serde_wasm_bindgen::to_value(&RunOptions {
                    is_mdx: false,
                    is_update: true,
                    input_format: None,
                    list_style: None,
                    link_title_style: None,
                    link_url_style: None,
                })
                .unwrap()
            )
            .is_err()
        );
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    fn test_script_format() {
        let result = format_script(r#"downcase()|ltrimstr("hello")|upcase()|trim()"#).unwrap();
        assert_eq!(
            result,
            r#"downcase() | ltrimstr("hello") | upcase() | trim()"#
        );
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    fn test_script_format_invalid() {
        assert!(format_script("x=>").is_err());
    }
}
