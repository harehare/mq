//! CSV output rendering for the `--output-format csv` CLI option.
//!
//! Converts [`mq_lang::RuntimeValue`]s into CSV using the `csv` crate. A top-level
//! `Array` is expanded so each element becomes its own row (mirroring `table.rs`).
//! Arrays of dicts produce a header row from the union of all keys; arrays of arrays
//! treat the first row as the header; anything else becomes a single "value" column.
//! With a [`ColorTheme`], header cells use the heading color and numbers/booleans are
//! colored by type, so stripping the ANSI codes yields the plain output.

use miette::miette;
use mq_lang::RuntimeValue;
#[cfg(test)]
use mq_lang::Shared;
use mq_markdown::ColorTheme;
use std::borrow::Cow;
use std::collections::BTreeSet;

fn cell_value(value: &RuntimeValue) -> String {
    match value {
        RuntimeValue::Dict(_) | RuntimeValue::Array(_) => {
            serde_json::to_string(&value.clone().to_json_value()).unwrap_or_default()
        }
        RuntimeValue::Markdown(node, _) => node.to_string(),
        _ => value.to_string(),
    }
}

fn paint(text: String, pair: &(Cow<'_, str>, Cow<'_, str>)) -> String {
    if text.is_empty() {
        text
    } else {
        format!("{}{}{}", pair.0, text, pair.1)
    }
}

fn data_cell(value: &RuntimeValue, theme: Option<&ColorTheme<'_>>) -> String {
    let text = cell_value(value);
    match (theme, value) {
        (Some(theme), RuntimeValue::Number(_)) => paint(text, &theme.emphasis),
        (Some(theme), RuntimeValue::Boolean(_)) => paint(text, &theme.link_url),
        _ => text,
    }
}

fn header_cell(name: &str, theme: Option<&ColorTheme<'_>>) -> String {
    match theme {
        Some(theme) => paint(name.to_string(), &theme.heading),
        None => name.to_string(),
    }
}

/// Converts a list of [`RuntimeValue`]s into a CSV string.
pub(crate) fn runtime_values_to_csv(
    runtime_values: &[RuntimeValue],
    theme: Option<&ColorTheme<'_>>,
) -> miette::Result<String> {
    let non_none: Vec<&RuntimeValue> = runtime_values.iter().filter(|v| !v.is_none()).collect();

    // unwrap a single top-level Array, matching table.rs's behavior
    let expanded: Option<Vec<&RuntimeValue>> = if let [RuntimeValue::Array(items)] = non_none.as_slice() {
        Some(items.iter().collect())
    } else {
        None
    };
    let candidates: &[&RuntimeValue] = expanded.as_deref().unwrap_or(&non_none);

    let mut writer = csv::WriterBuilder::new().from_writer(vec![]);

    let all_dicts = !candidates.is_empty() && candidates.iter().all(|v| matches!(*v, RuntimeValue::Dict(_)));
    let all_arrays = !candidates.is_empty() && candidates.iter().all(|v| matches!(*v, RuntimeValue::Array(_)));

    let write_err = |e: csv::Error| miette!("Failed to write CSV record: {}", e);

    if all_dicts {
        let mut header_set: BTreeSet<String> = BTreeSet::new();
        for val in candidates.iter() {
            if let RuntimeValue::Dict(map) = *val {
                for key in map.keys() {
                    header_set.insert(key.to_string());
                }
            }
        }
        let headers: Vec<String> = header_set.into_iter().collect();
        let header_row: Vec<String> = headers.iter().map(|h| header_cell(h, theme)).collect();
        writer.write_record(&header_row).map_err(write_err)?;

        for val in candidates.iter() {
            if let RuntimeValue::Dict(map) = *val {
                let row: Vec<String> = headers
                    .iter()
                    .map(|h| {
                        map.get(&mq_lang::Ident::new(h.as_str()))
                            .map(|v| data_cell(v, theme))
                            .unwrap_or_default()
                    })
                    .collect();
                writer.write_record(&row).map_err(write_err)?;
            }
        }
    } else if all_arrays {
        for (i, val) in candidates.iter().enumerate() {
            if let RuntimeValue::Array(items) = *val {
                let row: Vec<String> = items
                    .iter()
                    .map(|v| match (i, theme) {
                        (0, Some(_)) => header_cell(&cell_value(v), theme),
                        _ => data_cell(v, theme),
                    })
                    .collect();
                writer.write_record(&row).map_err(write_err)?;
            }
        }
    } else {
        writer.write_record([header_cell("value", theme)]).map_err(write_err)?;
        for val in candidates.iter() {
            writer.write_record([data_cell(val, theme)]).map_err(write_err)?;
        }
    }

    let bytes = writer.into_inner().map_err(|e| miette!("Failed to write CSV: {}", e))?;
    String::from_utf8(bytes).map_err(|e| miette!("Failed to convert CSV output to UTF-8: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_array_of_dicts() {
        let mut m1 = mq_lang::DictMap::default();
        m1.insert(
            mq_lang::Ident::new("name"),
            RuntimeValue::String(Shared::new("Alice".to_string())),
        );
        m1.insert(
            mq_lang::Ident::new("age"),
            RuntimeValue::String(Shared::new("30".to_string())),
        );
        let mut m2 = mq_lang::DictMap::default();
        m2.insert(
            mq_lang::Ident::new("name"),
            RuntimeValue::String(Shared::new("Bob".to_string())),
        );
        m2.insert(
            mq_lang::Ident::new("age"),
            RuntimeValue::String(Shared::new("25".to_string())),
        );
        let values = vec![RuntimeValue::Array(Shared::new(vec![
            RuntimeValue::Dict(Shared::new(m1)),
            RuntimeValue::Dict(Shared::new(m2)),
        ]))];
        let result = runtime_values_to_csv(&values, None).unwrap();
        assert_eq!(result, "age,name\n30,Alice\n25,Bob\n");
    }

    #[test]
    fn test_single_dict() {
        let mut map = mq_lang::DictMap::default();
        map.insert(
            mq_lang::Ident::new("a"),
            RuntimeValue::String(Shared::new("1".to_string())),
        );
        let values = vec![RuntimeValue::Dict(Shared::new(map))];
        let result = runtime_values_to_csv(&values, None).unwrap();
        assert_eq!(result, "a\n1\n");
    }

    #[test]
    fn test_needs_quoting() {
        let mut map = mq_lang::DictMap::default();
        map.insert(
            mq_lang::Ident::new("a"),
            RuntimeValue::String(Shared::new("has,comma".to_string())),
        );
        let values = vec![RuntimeValue::Dict(Shared::new(map))];
        let result = runtime_values_to_csv(&values, None).unwrap();
        assert_eq!(result, "a\n\"has,comma\"\n");
    }

    #[test]
    fn test_scalars_single_column() {
        let values = vec![
            RuntimeValue::String(Shared::new("x".to_string())),
            RuntimeValue::String(Shared::new("y".to_string())),
        ];
        let result = runtime_values_to_csv(&values, None).unwrap();
        assert_eq!(result, "value\nx\ny\n");
    }

    #[test]
    fn test_missing_key_is_empty() {
        let mut m1 = mq_lang::DictMap::default();
        m1.insert(
            mq_lang::Ident::new("a"),
            RuntimeValue::String(Shared::new("1".to_string())),
        );
        m1.insert(
            mq_lang::Ident::new("b"),
            RuntimeValue::String(Shared::new("2".to_string())),
        );
        let mut m2 = mq_lang::DictMap::default();
        m2.insert(
            mq_lang::Ident::new("a"),
            RuntimeValue::String(Shared::new("3".to_string())),
        );
        let values = vec![RuntimeValue::Array(Shared::new(vec![
            RuntimeValue::Dict(Shared::new(m1)),
            RuntimeValue::Dict(Shared::new(m2)),
        ]))];
        let result = runtime_values_to_csv(&values, None).unwrap();
        assert_eq!(result, "a,b\n1,2\n3,\n");
    }

    #[test]
    fn test_array_of_arrays() {
        let values = vec![RuntimeValue::Array(Shared::new(vec![
            RuntimeValue::Array(Shared::new(vec![
                RuntimeValue::String(Shared::new("h1".to_string())),
                RuntimeValue::String(Shared::new("h2".to_string())),
            ])),
            RuntimeValue::Array(Shared::new(vec![
                RuntimeValue::String(Shared::new("v1".to_string())),
                RuntimeValue::String(Shared::new("v2".to_string())),
            ])),
        ]))];
        let result = runtime_values_to_csv(&values, None).unwrap();
        assert_eq!(result, "h1,h2\nv1,v2\n");
    }

    #[test]
    fn test_none_filtered_out() {
        let values = vec![
            RuntimeValue::None,
            RuntimeValue::String(Shared::new("visible".to_string())),
        ];
        let result = runtime_values_to_csv(&values, None).unwrap();
        assert_eq!(result, "value\nvisible\n");
    }

    fn colored_theme() -> ColorTheme<'static> {
        ColorTheme::COLORED
    }

    fn strip_ansi(s: &str) -> String {
        let mut out = String::new();
        let mut chars = s.chars();
        while let Some(c) = chars.next() {
            if c == '\x1b' {
                for c in chars.by_ref() {
                    if c == 'm' {
                        break;
                    }
                }
            } else {
                out.push(c);
            }
        }
        out
    }

    #[test]
    fn test_colored_header_and_typed_cells() {
        let mut map = mq_lang::DictMap::default();
        map.insert(mq_lang::Ident::new("n"), RuntimeValue::Number(3i64.into()));
        map.insert(mq_lang::Ident::new("ok"), RuntimeValue::Boolean(true));
        map.insert(
            mq_lang::Ident::new("s"),
            RuntimeValue::String(Shared::new("a,b".to_string())),
        );
        let values = vec![RuntimeValue::Dict(Shared::new(map))];
        let theme = colored_theme();
        let colored = runtime_values_to_csv(&values, Some(&theme)).unwrap();
        let plain = runtime_values_to_csv(&values, None).unwrap();

        assert!(colored.starts_with(&format!("{}n{}", theme.heading.0, theme.heading.1)));
        assert!(colored.contains(&format!("{}3{}", theme.emphasis.0, theme.emphasis.1)));
        assert!(colored.contains(&format!("{}true{}", theme.link_url.0, theme.link_url.1)));
        assert_eq!(strip_ansi(&colored), plain);
    }

    #[test]
    fn test_colored_array_of_arrays_header_row() {
        let values = vec![RuntimeValue::Array(Shared::new(vec![
            RuntimeValue::Array(Shared::new(vec![RuntimeValue::String(Shared::new("h".to_string()))])),
            RuntimeValue::Array(Shared::new(vec![RuntimeValue::String(Shared::new("v".to_string()))])),
        ]))];
        let theme = colored_theme();
        let colored = runtime_values_to_csv(&values, Some(&theme)).unwrap();
        assert_eq!(colored, format!("{}h{}\nv\n", theme.heading.0, theme.heading.1));
    }
}
