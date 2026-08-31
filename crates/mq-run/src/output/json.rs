//! JSON output rendering for the `--output-format json` CLI option.
//!
//! Converts [`mq_lang::RuntimeValue`]s into JSON using `serde_json`.
//! Supports optional ANSI color output using the [`mq_markdown::ColorTheme`].

use miette::miette;
#[cfg(test)]
use mq_lang::Shared;
use mq_markdown::ColorTheme;

fn json_quote(s: &str) -> String {
    serde_json::to_string(s).unwrap()
}

fn colorize_json_value(value: &serde_json::Value, indent: usize, theme: &ColorTheme<'_>) -> String {
    let reset = &theme.code.1;

    match value {
        serde_json::Value::Null => format!("{}null{}", theme.blockquote_marker.0, reset),
        serde_json::Value::Bool(b) => format!("{}{}{}", theme.heading.0, b, reset),
        serde_json::Value::Number(n) => format!("{}{}{}", theme.emphasis.0, n, reset),
        serde_json::Value::String(s) => format!("{}{}{}", theme.code.0, json_quote(s), reset),
        serde_json::Value::Array(arr) => {
            if arr.is_empty() {
                return "[]".to_string();
            }
            let indent_str = "  ".repeat(indent);
            let inner_indent = "  ".repeat(indent + 1);
            let items: Vec<String> = arr
                .iter()
                .map(|v| format!("{}{}", inner_indent, colorize_json_value(v, indent + 1, theme)))
                .collect();
            format!("[\n{}\n{}]", items.join(",\n"), indent_str)
        }
        serde_json::Value::Object(map) => {
            if map.is_empty() {
                return "{}".to_string();
            }
            let indent_str = "  ".repeat(indent);
            let inner_indent = "  ".repeat(indent + 1);
            let items: Vec<String> = map
                .iter()
                .map(|(k, v)| {
                    format!(
                        "{}{}{}{}: {}",
                        inner_indent,
                        theme.link_url.0,
                        json_quote(k),
                        reset,
                        colorize_json_value(v, indent + 1, theme)
                    )
                })
                .collect();
            format!("{{\n{}\n{}}}", items.join(",\n"), indent_str)
        }
    }
}

/// Merges a list of [`mq_lang::RuntimeValue`]s into a single [`serde_json::Value`].
/// A lone non-Markdown value is returned as-is; otherwise all values (with empty
/// Markdown nodes filtered out) are collected into a JSON array.
pub(crate) fn runtime_values_to_json_value(runtime_values: &[mq_lang::RuntimeValue]) -> serde_json::Value {
    let filtered: Vec<&mq_lang::RuntimeValue> = runtime_values
        .iter()
        .filter(|v| match v {
            mq_lang::RuntimeValue::Markdown(node, _) => !node.is_empty() && !node.is_empty_fragment(),
            _ => true,
        })
        .collect();

    let all_markdown = filtered
        .iter()
        .all(|v| matches!(v, mq_lang::RuntimeValue::Markdown(_, _)));

    if !all_markdown && filtered.len() == 1 {
        filtered[0].clone().to_json_value()
    } else {
        let json_values: Vec<serde_json::Value> = filtered
            .iter()
            .map(|v| match v {
                mq_lang::RuntimeValue::Markdown(node, _) => {
                    serde_json::to_value(node.as_ref()).unwrap_or(serde_json::Value::Null)
                }
                _ => (*v).clone().to_json_value(),
            })
            .collect();
        serde_json::Value::Array(json_values)
    }
}

fn colorize_json_value_compact(value: &serde_json::Value, theme: &ColorTheme<'_>) -> String {
    let reset = &theme.code.1;

    match value {
        serde_json::Value::Null => format!("{}null{}", theme.blockquote_marker.0, reset),
        serde_json::Value::Bool(b) => format!("{}{}{}", theme.heading.0, b, reset),
        serde_json::Value::Number(n) => format!("{}{}{}", theme.emphasis.0, n, reset),
        serde_json::Value::String(s) => format!("{}{}{}", theme.code.0, json_quote(s), reset),
        serde_json::Value::Array(arr) => {
            let items: Vec<String> = arr.iter().map(|v| colorize_json_value_compact(v, theme)).collect();
            format!("[{}]", items.join(","))
        }
        serde_json::Value::Object(map) => {
            let items: Vec<String> = map
                .iter()
                .map(|(k, v)| {
                    format!(
                        "{}{}{}:{}",
                        theme.link_url.0,
                        json_quote(k),
                        reset,
                        colorize_json_value_compact(v, theme)
                    )
                })
                .collect();
            format!("{{{}}}", items.join(","))
        }
    }
}

/// Converts a list of [`mq_lang::RuntimeValue`]s into a JSON string.
/// `theme` enables ANSI color; `compact` renders a single line.
pub(crate) fn runtime_values_to_json(
    runtime_values: &[mq_lang::RuntimeValue],
    theme: Option<&ColorTheme<'_>>,
    compact: bool,
) -> miette::Result<String> {
    let result = runtime_values_to_json_value(runtime_values);

    match (theme, compact) {
        (Some(theme), true) => Ok(colorize_json_value_compact(&result, theme)),
        (Some(theme), false) => Ok(colorize_json_value(&result, 0, theme)),
        (None, true) => serde_json::to_string(&result).map_err(|e| miette!("Failed to serialize to JSON: {}", e)),
        (None, false) => {
            serde_json::to_string_pretty(&result).map_err(|e| miette!("Failed to serialize to JSON: {}", e))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mq_lang::RuntimeValue;
    use rstest::rstest;

    fn plain_theme() -> ColorTheme<'static> {
        ColorTheme::PLAIN
    }

    #[rstest]
    #[case(vec![RuntimeValue::String("hello".to_string())], "\"hello\"")]
    #[case(vec![RuntimeValue::Boolean(true)], "true")]
    #[case(vec![RuntimeValue::Boolean(false)], "false")]
    #[case(vec![RuntimeValue::None], "null")]
    fn test_single_non_markdown_no_theme(#[case] values: Vec<RuntimeValue>, #[case] expected: &str) {
        let result = runtime_values_to_json(&values, None, false).unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn test_multiple_values_becomes_array() {
        let values = vec![
            RuntimeValue::String("a".to_string()),
            RuntimeValue::String("b".to_string()),
        ];
        let result = runtime_values_to_json(&values, None, false).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(parsed.is_array());
        assert_eq!(parsed.as_array().unwrap().len(), 2);
    }

    #[test]
    fn test_empty_markdown_filtered_out() {
        let empty_node = mq_markdown::Node::Empty;
        let values = vec![RuntimeValue::Markdown(Box::new(empty_node), None)];
        let result = runtime_values_to_json(&values, None, false).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(parsed.is_array());
        assert!(parsed.as_array().unwrap().is_empty());
    }

    #[test]
    fn test_with_plain_theme_null() {
        let theme = plain_theme();
        let values = vec![RuntimeValue::None];
        let result = runtime_values_to_json(&values, Some(&theme), false).unwrap();
        assert!(result.contains("null"));
    }

    #[test]
    fn test_with_plain_theme_bool() {
        let theme = plain_theme();
        let values = vec![RuntimeValue::Boolean(true)];
        let result = runtime_values_to_json(&values, Some(&theme), false).unwrap();
        assert!(result.contains("true"));
    }

    #[test]
    fn test_with_plain_theme_number() {
        let theme = plain_theme();
        // Build a Number RuntimeValue via From<usize>
        let values = vec![RuntimeValue::from(42usize)];
        let result = runtime_values_to_json(&values, Some(&theme), false).unwrap();
        assert!(result.contains("42"));
    }

    #[test]
    fn test_with_plain_theme_string() {
        let theme = plain_theme();
        let values = vec![RuntimeValue::String("hi".to_string())];
        let result = runtime_values_to_json(&values, Some(&theme), false).unwrap();
        assert!(result.contains("hi"));
    }

    #[test]
    fn test_colorize_array_empty() {
        let theme = plain_theme();
        let values = vec![RuntimeValue::Array(Shared::new(vec![]))];
        let result = runtime_values_to_json(&values, Some(&theme), false).unwrap();
        assert_eq!(result, "[]");
    }

    #[test]
    fn test_colorize_array_non_empty() {
        let theme = plain_theme();
        let values = vec![RuntimeValue::Array(Shared::new(vec![
            RuntimeValue::String("x".to_string()),
            RuntimeValue::String("y".to_string()),
        ]))];
        let result = runtime_values_to_json(&values, Some(&theme), false).unwrap();
        assert!(result.contains('[') && result.contains(']'));
        assert!(result.contains("\"x\"") && result.contains("\"y\""));
    }

    #[test]
    fn test_colorize_object_empty() {
        let theme = plain_theme();
        let values = vec![RuntimeValue::Dict(Shared::new(std::collections::BTreeMap::new()))];
        let result = runtime_values_to_json(&values, Some(&theme), false).unwrap();
        assert_eq!(result, "{}");
    }

    #[test]
    fn test_colorize_object_non_empty() {
        let theme = plain_theme();
        let mut map = std::collections::BTreeMap::new();
        map.insert(mq_lang::Ident::new("key"), RuntimeValue::String("val".to_string()));
        let values = vec![RuntimeValue::Dict(Shared::new(map))];
        let result = runtime_values_to_json(&values, Some(&theme), false).unwrap();
        assert!(result.contains("key") && result.contains("val"));
    }

    #[test]
    fn test_compact_no_theme() {
        let mut map = std::collections::BTreeMap::new();
        map.insert(mq_lang::Ident::new("a"), RuntimeValue::from(1usize));
        map.insert(mq_lang::Ident::new("b"), RuntimeValue::from(2usize));
        let values = vec![RuntimeValue::Dict(Shared::new(map))];
        let result = runtime_values_to_json(&values, None, true).unwrap();
        assert_eq!(result, r#"{"a":1,"b":2}"#);
    }

    #[test]
    fn test_compact_with_theme() {
        let theme = plain_theme();
        let mut map = std::collections::BTreeMap::new();
        map.insert(mq_lang::Ident::new("a"), RuntimeValue::from(1usize));
        map.insert(mq_lang::Ident::new("b"), RuntimeValue::from(2usize));
        let values = vec![RuntimeValue::Dict(Shared::new(map))];
        let result = runtime_values_to_json(&values, Some(&theme), true).unwrap();
        assert_eq!(result, r#"{"a":1,"b":2}"#);
    }
}
