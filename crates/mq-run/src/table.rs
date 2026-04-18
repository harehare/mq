//! Table rendering for the `--output-format table` CLI option.
//!
//! Converts [`mq_lang::RuntimeValue`]s into an ASCII table using the `tabled` crate.
//! Nested dicts are rendered as embedded tables inside cells. Arrays of dicts produce
//! a multi-column sub-table; other arrays produce a single-column sub-table. A top-level
//! single `Array` is automatically expanded so each element becomes its own row.
//! Markdown nodes with children are displayed with a nested children table.

use mq_lang::RuntimeValue;
use std::collections::{BTreeMap, BTreeSet};
use tabled::Table;
use tabled::builder::Builder;
use tabled::settings::object::Rows;
use tabled::settings::themes::Colorization;
use tabled::settings::{Color, Style};

/// Converts a list of [`RuntimeValue`]s into a [`Table`].
pub(crate) fn runtime_values_to_table(runtime_values: &[RuntimeValue], color_output: bool) -> Table {
    let non_none: Vec<&RuntimeValue> = runtime_values.iter().filter(|v| !v.is_none()).collect();

    // Step 1 – unwrap a single top-level Array
    let expanded: Option<Vec<&RuntimeValue>> = if let [RuntimeValue::Array(items)] = non_none.as_slice() {
        Some(items.iter().collect())
    } else {
        None
    };
    let candidates: &[&RuntimeValue] = expanded.as_deref().unwrap_or(&non_none);

    // Step 2 – Dict mode: direct keys as headers, nested values as embedded tables
    let all_dicts = !candidates.is_empty() && candidates.iter().all(|v| matches!(*v, RuntimeValue::Dict(_)));

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

        if !headers.is_empty() {
            let mut builder = Builder::default();
            builder.push_record(headers.clone());
            for val in candidates.iter() {
                if let RuntimeValue::Dict(map) = *val {
                    let row: Vec<String> = headers
                        .iter()
                        .map(|h| {
                            map.get(&mq_lang::Ident::new(h.as_str()))
                                .map(format_cell_value)
                                .unwrap_or_default()
                        })
                        .collect();
                    builder.push_record(row);
                }
            }
            return apply_color(builder.build().with(Style::rounded()).to_owned(), color_output);
        }
    }

    // Step 3 – Markdown mode: type / value / children / position
    let all_md = !candidates.is_empty() && candidates.iter().all(|v| matches!(*v, RuntimeValue::Markdown(..)));

    if all_md {
        let mut builder = Builder::default();
        for val in candidates.iter() {
            if let RuntimeValue::Markdown(node, _) = *val {
                let mut rows: Vec<Vec<String>> = vec![
                    vec!["type".to_string(), node.name().to_string()],
                    vec!["value".to_string(), node.value().to_string()],
                ];
                let children_str = format_markdown_children(node);
                if !children_str.is_empty() {
                    rows.push(vec!["children".to_string(), children_str]);
                }
                if let Some(pos) = node.position() {
                    let mut start_map = BTreeMap::new();
                    start_map.insert(mq_lang::Ident::new("line"), pos.start.line.to_string().into());
                    start_map.insert(mq_lang::Ident::new("column"), pos.start.column.to_string().into());

                    let mut end_map = BTreeMap::new();
                    end_map.insert(mq_lang::Ident::new("line"), pos.end.line.to_string().into());
                    end_map.insert(mq_lang::Ident::new("column"), pos.end.column.to_string().into());

                    let mut pos_map = BTreeMap::new();
                    pos_map.insert(mq_lang::Ident::new("start"), RuntimeValue::Dict(start_map));
                    pos_map.insert(mq_lang::Ident::new("end"), RuntimeValue::Dict(end_map));
                    let pos_str = format_cell_value(&RuntimeValue::Dict(pos_map));
                    rows.push(vec!["position".to_string(), pos_str]);
                }
                builder.push_record([build_nested_table(&rows)]);
            }
        }
        return apply_color(builder.build().with(Style::rounded()).to_owned(), color_output);
    }

    // Step 4 – Fallback: single "value" column
    let mut builder = Builder::default();
    builder.push_record(["value"]);
    for val in candidates.iter() {
        builder.push_record([val.to_string()]);
    }
    apply_color(builder.build().with(Style::rounded()).to_owned(), color_output)
}

/// Applies color settings to a table when `color_output` is enabled.
fn apply_color(mut table: Table, color_output: bool) -> Table {
    if color_output {
        table.with(Colorization::exact([Color::BOLD | Color::FG_CYAN], Rows::first()));
    }
    table
}

/// Renders rows as a nested rounded table string using `tabled`.
fn build_nested_table(rows: &[Vec<String>]) -> String {
    if rows.is_empty() {
        return String::new();
    }
    let mut builder = Builder::default();
    for row in rows {
        builder.push_record(row.iter().map(|s| s.as_str()));
    }
    builder.build().with(Style::rounded().remove_horizontals()).to_string()
}

/// Renders a Markdown node's children as a nested table string.
fn format_markdown_children(node: &mq_markdown::Node) -> String {
    let children = node.children();
    if children.is_empty() {
        return String::new();
    }
    let rows: Vec<Vec<String>> = children
        .iter()
        .map(|child| vec![child.name().to_string(), format_markdown_node(child)])
        .collect();
    build_nested_table(&rows)
}

/// Formats a single Markdown node for display in a table cell.
fn format_markdown_node(node: &mq_markdown::Node) -> String {
    let children = node.children();
    if children.is_empty() {
        node.value().to_string()
    } else {
        let rows: Vec<Vec<String>> = children
            .iter()
            .map(|child| vec![child.name().to_string(), format_markdown_node(child)])
            .collect();
        build_nested_table(&rows)
    }
}

/// Formats a [`RuntimeValue`] as a string suitable for a table cell.
fn format_cell_value(value: &RuntimeValue) -> String {
    match value {
        RuntimeValue::Dict(map) => {
            if map.is_empty() {
                return String::new();
            }
            let rows: Vec<Vec<String>> = map
                .iter()
                .map(|(k, v)| vec![k.to_string(), format_cell_value(v)])
                .collect();
            build_nested_table(&rows)
        }
        RuntimeValue::Array(items) => {
            if items.is_empty() {
                return String::new();
            }
            let all_dicts = items.iter().all(|v| matches!(v, RuntimeValue::Dict(_)));
            if all_dicts {
                let mut header_set: BTreeSet<String> = BTreeSet::new();
                for item in items {
                    if let RuntimeValue::Dict(map) = item {
                        for key in map.keys() {
                            header_set.insert(key.to_string());
                        }
                    }
                }
                let headers: Vec<String> = header_set.into_iter().collect();
                let mut table_rows = vec![headers.clone()];
                for item in items {
                    if let RuntimeValue::Dict(map) = item {
                        let row: Vec<String> = headers
                            .iter()
                            .map(|h| {
                                map.get(&mq_lang::Ident::new(h.as_str()))
                                    .map(format_cell_value)
                                    .unwrap_or_default()
                            })
                            .collect();
                        table_rows.push(row);
                    }
                }
                build_nested_table(&table_rows)
            } else {
                let rows: Vec<Vec<String>> = items.iter().map(|v| vec![format_cell_value(v)]).collect();
                build_nested_table(&rows)
            }
        }
        RuntimeValue::Markdown(node, _) => format_markdown_node(node),
        RuntimeValue::String(s) => s.clone(),
        _ => value.to_string(),
    }
}
