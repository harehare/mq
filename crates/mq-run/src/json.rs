//! JSON output rendering for the `--output-format json` CLI option.
//!
//! Converts [`mq_lang::RuntimeValue`]s into JSON using `serde_json`.
//! Supports optional ANSI color output using the [`mq_markdown::ColorTheme`].

use miette::miette;
use mq_markdown::ColorTheme;

fn colorize_json_value(value: &serde_json::Value, indent: usize, theme: &ColorTheme<'_>) -> String {
    let indent_str = "  ".repeat(indent);
    let inner_indent = "  ".repeat(indent + 1);
    let reset = &theme.code.1;

    match value {
        serde_json::Value::Null => format!("{}null{}", theme.blockquote_marker.0, reset),
        serde_json::Value::Bool(b) => format!("{}{}{}", theme.heading.0, b, reset),
        serde_json::Value::Number(n) => format!("{}{}{}", theme.emphasis.0, n, reset),
        serde_json::Value::String(s) => {
            let json_str = serde_json::to_string(s).unwrap_or_else(|_| format!("\"{}\"", s));
            format!("{}{}{}", theme.code.0, json_str, reset)
        }
        serde_json::Value::Array(arr) => {
            if arr.is_empty() {
                return "[]".to_string();
            }
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
            let items: Vec<String> = map
                .iter()
                .map(|(k, v)| {
                    let key = serde_json::to_string(k).unwrap_or_else(|_| format!("\"{}\"", k));
                    format!(
                        "{}{}{}{}: {}",
                        inner_indent,
                        theme.link_url.0,
                        key,
                        reset,
                        colorize_json_value(v, indent + 1, theme)
                    )
                })
                .collect();
            format!("{{\n{}\n{}}}", items.join(",\n"), indent_str)
        }
    }
}

/// Converts a list of [`mq_lang::RuntimeValue`]s into a JSON string.
///
/// When `color` is `true`, ANSI color codes are applied using the current
/// [`ColorTheme`] (read from the `MQ_COLORS` environment variable).
pub(crate) fn runtime_values_to_json(runtime_values: &[mq_lang::RuntimeValue], color: bool) -> miette::Result<String> {
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

    let result = if !all_markdown && filtered.len() == 1 {
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
    };

    if color {
        let theme = ColorTheme::from_env();
        Ok(colorize_json_value(&result, 0, &theme))
    } else {
        serde_json::to_string_pretty(&result).map_err(|e| miette!("Failed to serialize to JSON: {}", e))
    }
}
