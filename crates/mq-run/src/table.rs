//! Table rendering for the `--output-format table` CLI option.
//!
//! Converts [`mq_lang::RuntimeValue`]s into an ASCII table using the `tabled` crate.
//! Nested dicts are rendered as embedded tables inside cells. Arrays of dicts produce
//! a multi-column sub-table; other arrays produce a single-column sub-table. A top-level
//! single `Array` is automatically expanded so each element becomes its own row.
//! Markdown nodes with children are displayed with a nested children table.

use mq_lang::RuntimeValue;
use mq_markdown::ColorTheme;
use std::collections::{BTreeMap, BTreeSet};
use tabled::Table;
use tabled::builder::Builder;
use tabled::settings::location::Locator;
use tabled::settings::object::{Rows, Segment};
use tabled::settings::style::BorderColor;
use tabled::settings::{Color, Modify, Style};

/// Converts a list of [`RuntimeValue`]s into a [`Table`].
pub(crate) fn runtime_values_to_table<'a>(
    runtime_values: &[RuntimeValue],
    theme: Option<&'a ColorTheme<'a>>,
) -> Table {
    let non_none: Vec<&RuntimeValue> = runtime_values.iter().filter(|v| !v.is_none()).collect();

    // unwrap a single top-level Array
    let expanded: Option<Vec<&RuntimeValue>> = if let [RuntimeValue::Array(items)] = non_none.as_slice() {
        Some(items.iter().collect())
    } else {
        None
    };
    let candidates: &[&RuntimeValue] = expanded.as_deref().unwrap_or(&non_none);

    // direct keys as headers, nested values as embedded tables
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
                                .map(|v| format_cell_value(v, theme))
                                .unwrap_or_default()
                        })
                        .collect();
                    builder.push_record(row);
                }
            }
            return apply_color(builder.build().with(Style::rounded()).to_owned(), theme, true);
        }
    }

    let all_md = !candidates.is_empty() && candidates.iter().all(|v| matches!(*v, RuntimeValue::Markdown(..)));

    if all_md {
        let mut builder = Builder::default();
        for val in candidates.iter() {
            if let RuntimeValue::Markdown(node, _) = *val {
                if node.value().is_empty() {
                    continue;
                }

                let mut rows: Vec<Vec<String>> = vec![
                    vec!["type".to_string(), node.name().to_string()],
                    vec!["value".to_string(), node.value().to_string()],
                ];
                let children_str = format_markdown_children(node, theme);
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
                    let pos_str = format_cell_value(&RuntimeValue::Dict(pos_map), theme);
                    rows.push(vec!["position".to_string(), pos_str]);
                }
                builder.push_record([build_nested_table(&rows, theme)]);
            }
        }
        return apply_color(builder.build().with(Style::rounded()).to_owned(), theme, false);
    }

    let mut builder = Builder::default();
    builder.push_record(["value"]);
    for val in candidates.iter() {
        builder.push_record([val.to_string()]);
    }
    apply_color(builder.build().with(Style::rounded()).to_owned(), theme, true)
}

/// Converts a theme color pair `(prefix, suffix)` to a tabled [`Color`].
fn pair_to_color(prefix: &str, suffix: &str) -> Color {
    Color::new(prefix.to_string(), suffix.to_string())
}

/// Applies theme colors and border color to a table.
/// `has_header` controls whether the first row is styled as a header.
fn apply_color<'a>(mut table: Table, theme: Option<&'a ColorTheme<'a>>, has_header: bool) -> Table {
    let Some(theme) = theme else {
        return table;
    };

    let heading = pair_to_color(&theme.heading.0, &theme.heading.1);
    let bool_color = pair_to_color(&theme.link_url.0, &theme.link_url.1);
    let none_color = pair_to_color(&theme.horizontal_rule.0, &theme.horizontal_rule.1);
    let border_color = pair_to_color(&theme.table_separator.0, &theme.table_separator.1);

    table.modify(Segment::all(), BorderColor::filled(border_color));

    if has_header {
        table.with(Modify::new(Rows::first()).with(heading.clone()));
    }

    table
        .modify(Locator::content("true"), bool_color.clone())
        .modify(Locator::content("false"), bool_color)
        .modify(Locator::content("None"), none_color)
        .modify(Locator::content("type"), heading.clone())
        .modify(Locator::content("value"), heading.clone())
        .modify(Locator::content("children"), heading.clone())
        .modify(Locator::content("position"), heading.clone())
        .modify(Locator::content("start"), heading.clone())
        .modify(Locator::content("end"), heading.clone())
        .modify(Locator::content("line"), heading.clone())
        .modify(Locator::content("column"), heading);

    table
}

/// Renders rows as a nested rounded table string using `tabled`.
fn build_nested_table<'a>(rows: &[Vec<String>], theme: Option<&'a ColorTheme<'a>>) -> String {
    if rows.is_empty() {
        return String::new();
    }
    let mut builder = Builder::default();
    for row in rows {
        builder.push_record(row.iter().map(|s| s.as_str()));
    }
    apply_color(
        builder.build().with(Style::rounded().remove_horizontals()).to_owned(),
        theme,
        false,
    )
    .to_string()
}

/// Renders a Markdown node's children as a nested table string.
fn format_markdown_children<'a>(node: &mq_markdown::Node, theme: Option<&'a ColorTheme<'a>>) -> String {
    let children = node.children();
    if children.is_empty() {
        return String::new();
    }
    let rows: Vec<Vec<String>> = children
        .iter()
        .map(|child| vec![child.name().to_string(), format_markdown_node(child, theme)])
        .collect();
    build_nested_table(&rows, theme)
}

/// Formats a single Markdown node for display in a table cell.
fn format_markdown_node<'a>(node: &mq_markdown::Node, theme: Option<&'a ColorTheme<'a>>) -> String {
    let children = node.children();
    if children.is_empty() {
        node.value().to_string()
    } else {
        let rows: Vec<Vec<String>> = children
            .iter()
            .map(|child| vec![child.name().to_string(), format_markdown_node(child, theme)])
            .collect();
        build_nested_table(&rows, theme)
    }
}

/// Formats a [`RuntimeValue`] as a string suitable for a table cell.
fn format_cell_value<'a>(value: &RuntimeValue, theme: Option<&'a ColorTheme<'a>>) -> String {
    match value {
        RuntimeValue::Dict(map) => {
            if map.is_empty() {
                return String::new();
            }
            let rows: Vec<Vec<String>> = map
                .iter()
                .map(|(k, v)| vec![k.to_string(), format_cell_value(v, theme)])
                .collect();
            build_nested_table(&rows, theme)
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
                                    .map(|v| format_cell_value(v, theme))
                                    .unwrap_or_default()
                            })
                            .collect();
                        table_rows.push(row);
                    }
                }
                build_nested_table(&table_rows, theme)
            } else {
                let rows: Vec<Vec<String>> = items.iter().map(|v| vec![format_cell_value(v, theme)]).collect();
                build_nested_table(&rows, theme)
            }
        }
        RuntimeValue::Markdown(node, _) => format_markdown_node(node, theme),
        _ => value.to_string(),
    }
}
