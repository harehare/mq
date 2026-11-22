//! JSON output rendering for the `--output-format json` CLI option.
//!
//! Serializes [`mq_lang::RuntimeValue`]s straight into the output buffer with `serde_json`.
//! Supports optional ANSI color output using the [`mq_markdown::ColorTheme`].

use std::io::{self, Write};

use miette::miette;
use mq_lang::RuntimeValue;
use mq_markdown::ColorTheme;
#[cfg(test)]
use mq_syntax::Shared;
use serde::ser::{Serialize, SerializeMap, SerializeSeq, Serializer};

/// Whether `value` is left out of JSON output.
fn is_skipped(value: &RuntimeValue) -> bool {
    matches!(value, RuntimeValue::Markdown(node, _) if node.is_empty() || node.is_empty_fragment())
}

/// Serializes a value like [`RuntimeValue::to_json_value`], without building the tree first.
struct Json<'a>(&'a RuntimeValue);

impl Serialize for Json<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self.0 {
            RuntimeValue::None => serializer.serialize_unit(),
            RuntimeValue::Boolean(b) => serializer.serialize_bool(*b),
            RuntimeValue::Number(n) => {
                let v = n.value();
                // `i64::MAX as f64` and `u64::MAX as f64` round up to 2^63 and 2^64, so `<` keeps the bounds exact.
                if n.is_int() && v >= i64::MIN as f64 && v < i64::MAX as f64 {
                    serializer.serialize_i64(n.to_int())
                } else if n.is_int() && v >= 0.0 && v < u64::MAX as f64 {
                    serializer.serialize_u64(v as u64)
                } else if v.is_infinite() {
                    serializer.serialize_f64(f64::MAX.copysign(v))
                } else {
                    serializer.serialize_f64(v)
                }
            }
            RuntimeValue::String(s) => serializer.serialize_str(s),
            RuntimeValue::Symbol(ident) => ident.resolve_with(|name| serializer.serialize_str(name)),
            RuntimeValue::Array(items) => {
                let mut seq = serializer.serialize_seq(Some(items.len()))?;
                for item in items.iter() {
                    seq.serialize_element(&Json(item))?;
                }
                seq.end()
            }
            RuntimeValue::Dict(entries) => {
                let mut map = serializer.serialize_map(Some(entries.len()))?;
                for (key, value) in entries.iter() {
                    key.resolve_with(|name| map.serialize_entry(name, &Json(value)))?;
                }
                map.end()
            }
            RuntimeValue::Markdown(node, _) => node.as_ref().serialize(serializer),
            other => other.clone().to_json_value().serialize(serializer),
        }
    }
}

/// The values printed for a run: a lone non-Markdown value as it is, anything else as an array
/// without the skipped nodes.
struct JsonOutput<'a>(&'a [RuntimeValue]);

impl Serialize for JsonOutput<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut kept = self.0.iter().filter(|value| !is_skipped(value));
        if let (Some(only), None) = (kept.next(), kept.next())
            && !matches!(only, RuntimeValue::Markdown(_, _))
        {
            return Json(only).serialize(serializer);
        }
        serializer.collect_seq(self.0.iter().filter(|value| !is_skipped(value)).map(Json))
    }
}

/// Lays out and colors JSON: one line when `indent_unit` is `None`, pretty-printed otherwise.
struct ThemedFormatter<'a> {
    theme: &'a ColorTheme<'a>,
    indent_unit: Option<&'a str>,
    depth: usize,
    has_value: bool,
    in_key: bool,
}

impl<'a> ThemedFormatter<'a> {
    fn new(theme: &'a ColorTheme<'a>, indent_unit: Option<&'a str>) -> Self {
        Self {
            theme,
            indent_unit,
            depth: 0,
            has_value: false,
            in_key: false,
        }
    }

    fn reset(&self) -> &'a str {
        &self.theme.code.1
    }

    fn scalar<W: ?Sized + Write>(&self, writer: &mut W, style: &str, text: &str) -> io::Result<()> {
        writer.write_all(style.as_bytes())?;
        writer.write_all(text.as_bytes())?;
        writer.write_all(self.reset().as_bytes())
    }

    fn line_break<W: ?Sized + Write>(&self, writer: &mut W) -> io::Result<()> {
        let Some(indent_unit) = self.indent_unit else {
            return Ok(());
        };
        writer.write_all(b"\n")?;
        for _ in 0..self.depth {
            writer.write_all(indent_unit.as_bytes())?;
        }
        Ok(())
    }

    fn open<W: ?Sized + Write>(&mut self, writer: &mut W, bracket: &[u8]) -> io::Result<()> {
        self.depth += 1;
        self.has_value = false;
        writer.write_all(bracket)
    }

    fn close<W: ?Sized + Write>(&mut self, writer: &mut W, bracket: &[u8]) -> io::Result<()> {
        self.depth -= 1;
        if self.has_value {
            self.line_break(writer)?;
        }
        writer.write_all(bracket)
    }

    fn begin_item<W: ?Sized + Write>(&mut self, writer: &mut W, first: bool) -> io::Result<()> {
        if !first {
            writer.write_all(b",")?;
        }
        self.line_break(writer)
    }
}

/// Number methods that wrap the default formatting in the number style.
macro_rules! themed_numbers {
    ($($method:ident: $ty:ty),* $(,)?) => {
        $(
            fn $method<W: ?Sized + Write>(&mut self, writer: &mut W, value: $ty) -> io::Result<()> {
                writer.write_all(self.theme.emphasis.0.as_bytes())?;
                serde_json::ser::CompactFormatter.$method(writer, value)?;
                writer.write_all(self.reset().as_bytes())
            }
        )*
    };
}

impl serde_json::ser::Formatter for ThemedFormatter<'_> {
    themed_numbers! {
        write_i8: i8, write_i16: i16, write_i32: i32, write_i64: i64, write_i128: i128,
        write_u8: u8, write_u16: u16, write_u32: u32, write_u64: u64, write_u128: u128,
        write_f32: f32, write_f64: f64, write_number_str: &str,
    }

    fn write_null<W: ?Sized + Write>(&mut self, writer: &mut W) -> io::Result<()> {
        self.scalar(writer, &self.theme.blockquote_marker.0, "null")
    }

    fn write_bool<W: ?Sized + Write>(&mut self, writer: &mut W, value: bool) -> io::Result<()> {
        self.scalar(writer, &self.theme.heading.0, if value { "true" } else { "false" })
    }

    fn begin_string<W: ?Sized + Write>(&mut self, writer: &mut W) -> io::Result<()> {
        let style = if self.in_key {
            &self.theme.link_url.0
        } else {
            &self.theme.code.0
        };
        writer.write_all(style.as_bytes())?;
        writer.write_all(b"\"")
    }

    fn end_string<W: ?Sized + Write>(&mut self, writer: &mut W) -> io::Result<()> {
        writer.write_all(b"\"")?;
        writer.write_all(self.reset().as_bytes())
    }

    fn begin_array<W: ?Sized + Write>(&mut self, writer: &mut W) -> io::Result<()> {
        self.open(writer, b"[")
    }

    fn end_array<W: ?Sized + Write>(&mut self, writer: &mut W) -> io::Result<()> {
        self.close(writer, b"]")
    }

    fn begin_array_value<W: ?Sized + Write>(&mut self, writer: &mut W, first: bool) -> io::Result<()> {
        self.begin_item(writer, first)
    }

    fn end_array_value<W: ?Sized + Write>(&mut self, _writer: &mut W) -> io::Result<()> {
        self.has_value = true;
        Ok(())
    }

    fn begin_object<W: ?Sized + Write>(&mut self, writer: &mut W) -> io::Result<()> {
        self.open(writer, b"{")
    }

    fn end_object<W: ?Sized + Write>(&mut self, writer: &mut W) -> io::Result<()> {
        self.close(writer, b"}")
    }

    fn begin_object_key<W: ?Sized + Write>(&mut self, writer: &mut W, first: bool) -> io::Result<()> {
        self.in_key = true;
        self.begin_item(writer, first)
    }

    fn end_object_key<W: ?Sized + Write>(&mut self, _writer: &mut W) -> io::Result<()> {
        self.in_key = false;
        Ok(())
    }

    fn begin_object_value<W: ?Sized + Write>(&mut self, writer: &mut W) -> io::Result<()> {
        writer.write_all(if self.indent_unit.is_some() { b": " } else { b":" })
    }

    fn end_object_value<W: ?Sized + Write>(&mut self, _writer: &mut W) -> io::Result<()> {
        self.has_value = true;
        Ok(())
    }
}

/// Appends `value` to `out`: colored by `theme`, pretty-printed when `indent_unit` is set.
fn write_json<T: Serialize>(
    out: &mut Vec<u8>,
    value: &T,
    theme: Option<&ColorTheme<'_>>,
    indent_unit: Option<&str>,
) -> miette::Result<()> {
    let result = match (theme, indent_unit) {
        (None, None) => serde_json::to_writer(out, value),
        (theme, indent_unit) => {
            let formatter = ThemedFormatter::new(theme.unwrap_or(&ColorTheme::PLAIN), indent_unit);
            value.serialize(&mut serde_json::Serializer::with_formatter(out, formatter))
        }
    };
    result.map_err(|e| miette!("Failed to serialize to JSON: {}", e))
}

fn into_json_string(out: Vec<u8>) -> miette::Result<String> {
    String::from_utf8(out).map_err(|e| miette!("Failed to serialize to JSON: {}", e))
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

/// Serializes each value as one compact JSON line; empty Markdown nodes are skipped.
pub(crate) fn runtime_values_to_jsonl(
    runtime_values: &[RuntimeValue],
    theme: Option<&ColorTheme<'_>>,
) -> miette::Result<String> {
    let mut out = Vec::new();
    for value in runtime_values.iter().filter(|value| !is_skipped(value)) {
        write_json(&mut out, &Json(value), theme, None)?;
        out.push(b'\n');
    }
    into_json_string(out)
}

/// Converts a list of [`mq_lang::RuntimeValue`]s into a JSON string.
/// `theme` enables ANSI color; `compact` renders a single line; `indent_unit`
/// (e.g. `"  "` or `"\t"`) sets the per-level indent for pretty output.
pub(crate) fn runtime_values_to_json(
    runtime_values: &[RuntimeValue],
    theme: Option<&ColorTheme<'_>>,
    compact: bool,
    indent_unit: &str,
) -> miette::Result<String> {
    let mut out = Vec::new();
    write_json(
        &mut out,
        &JsonOutput(runtime_values),
        theme,
        (!compact).then_some(indent_unit),
    )?;
    into_json_string(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mq_lang::RuntimeValue;
    use rstest::rstest;

    #[test]
    fn test_jsonl_one_line_per_value() {
        let values = vec![
            RuntimeValue::String(Shared::new("a".to_string())),
            RuntimeValue::Boolean(true),
            RuntimeValue::None,
        ];
        assert_eq!(runtime_values_to_jsonl(&values, None).unwrap(), "\"a\"\ntrue\nnull\n");
    }

    #[test]
    fn test_jsonl_empty() {
        assert_eq!(runtime_values_to_jsonl(&[], None).unwrap(), "");
    }

    fn nested() -> Vec<RuntimeValue> {
        let mut inner = mq_lang::DictMap::default();
        inner.insert(mq_syntax::Ident::new("z"), RuntimeValue::Array(Shared::new(Vec::new())));
        inner.insert(mq_syntax::Ident::new("a"), RuntimeValue::None);
        let mut outer = mq_lang::DictMap::default();
        outer.insert(
            mq_syntax::Ident::new("k"),
            RuntimeValue::Array(Shared::new(vec![
                RuntimeValue::from(1usize),
                RuntimeValue::Dict(Shared::new(inner)),
            ])),
        );
        outer.insert(
            mq_syntax::Ident::new("e"),
            RuntimeValue::Dict(Shared::new(mq_lang::DictMap::default())),
        );
        vec![RuntimeValue::Dict(Shared::new(outer))]
    }

    #[rstest]
    #[case::compact(true, "  ", r#"{"k":[1,{"z":[],"a":null}],"e":{}}"#)]
    #[case::pretty(
        false,
        "\t",
        "{\n\t\"k\": [\n\t\t1,\n\t\t{\n\t\t\t\"z\": [],\n\t\t\t\"a\": null\n\t\t}\n\t],\n\t\"e\": {}\n}"
    )]
    fn test_layout_keeps_insertion_order(#[case] compact: bool, #[case] indent: &str, #[case] expected: &str) {
        assert_eq!(
            runtime_values_to_json(&nested(), None, compact, indent).unwrap(),
            expected
        );
    }

    #[test]
    fn test_colored_compact_styles_keys_and_scalars() {
        let theme = ColorTheme::COLORED;
        let reset = &theme.code.1;
        let expected = format!(
            "{{{key}\"k\"{reset}:[{num}1{reset},{{{key}\"z\"{reset}:[],{key}\"a\"{reset}:{null}null{reset}}}],{key}\"e\"{reset}:{{}}}}",
            key = theme.link_url.0,
            num = theme.emphasis.0,
            null = theme.blockquote_marker.0,
        );
        assert_eq!(
            runtime_values_to_json(&nested(), Some(&theme), true, "  ").unwrap(),
            expected
        );
    }

    fn plain_theme() -> ColorTheme<'static> {
        ColorTheme::PLAIN
    }

    #[rstest]
    #[case(vec![RuntimeValue::String(Shared::new("hello".to_string()))], "\"hello\"")]
    #[case(vec![RuntimeValue::Boolean(true)], "true")]
    #[case(vec![RuntimeValue::Boolean(false)], "false")]
    #[case(vec![RuntimeValue::None], "null")]
    fn test_single_non_markdown_no_theme(#[case] values: Vec<RuntimeValue>, #[case] expected: &str) {
        let result = runtime_values_to_json(&values, None, false, "  ").unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn test_multiple_values_becomes_array() {
        let values = vec![
            RuntimeValue::String(Shared::new("a".to_string())),
            RuntimeValue::String(Shared::new("b".to_string())),
        ];
        let result = runtime_values_to_json(&values, None, false, "  ").unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(parsed.is_array());
        assert_eq!(parsed.as_array().unwrap().len(), 2);
    }

    #[test]
    fn test_empty_markdown_filtered_out() {
        let empty_node = mq_markdown::Node::Empty;
        let values = vec![RuntimeValue::Markdown(Shared::new(empty_node), None)];
        let result = runtime_values_to_json(&values, None, false, "  ").unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(parsed.is_array());
        assert!(parsed.as_array().unwrap().is_empty());
    }

    #[test]
    fn test_with_plain_theme_null() {
        let theme = plain_theme();
        let values = vec![RuntimeValue::None];
        let result = runtime_values_to_json(&values, Some(&theme), false, "  ").unwrap();
        assert!(result.contains("null"));
    }

    #[test]
    fn test_with_plain_theme_bool() {
        let theme = plain_theme();
        let values = vec![RuntimeValue::Boolean(true)];
        let result = runtime_values_to_json(&values, Some(&theme), false, "  ").unwrap();
        assert!(result.contains("true"));
    }

    #[test]
    fn test_with_plain_theme_number() {
        let theme = plain_theme();
        // Build a Number RuntimeValue via From<usize>
        let values = vec![RuntimeValue::from(42usize)];
        let result = runtime_values_to_json(&values, Some(&theme), false, "  ").unwrap();
        assert!(result.contains("42"));
    }

    #[test]
    fn test_with_plain_theme_string() {
        let theme = plain_theme();
        let values = vec![RuntimeValue::String(Shared::new("hi".to_string()))];
        let result = runtime_values_to_json(&values, Some(&theme), false, "  ").unwrap();
        assert!(result.contains("hi"));
    }

    #[test]
    fn test_colorize_array_empty() {
        let theme = plain_theme();
        let values = vec![RuntimeValue::Array(Shared::new(vec![]))];
        let result = runtime_values_to_json(&values, Some(&theme), false, "  ").unwrap();
        assert_eq!(result, "[]");
    }

    #[test]
    fn test_colorize_array_non_empty() {
        let theme = plain_theme();
        let values = vec![RuntimeValue::Array(Shared::new(vec![
            RuntimeValue::String(Shared::new("x".to_string())),
            RuntimeValue::String(Shared::new("y".to_string())),
        ]))];
        let result = runtime_values_to_json(&values, Some(&theme), false, "  ").unwrap();
        assert!(result.contains('[') && result.contains(']'));
        assert!(result.contains("\"x\"") && result.contains("\"y\""));
    }

    #[test]
    fn test_colorize_object_empty() {
        let theme = plain_theme();
        let values = vec![RuntimeValue::Dict(Shared::new(mq_lang::DictMap::default()))];
        let result = runtime_values_to_json(&values, Some(&theme), false, "  ").unwrap();
        assert_eq!(result, "{}");
    }

    #[test]
    fn test_colorize_object_non_empty() {
        let theme = plain_theme();
        let mut map = mq_lang::DictMap::default();
        map.insert(
            mq_syntax::Ident::new("key"),
            RuntimeValue::String(Shared::new("val".to_string())),
        );
        let values = vec![RuntimeValue::Dict(Shared::new(map))];
        let result = runtime_values_to_json(&values, Some(&theme), false, "  ").unwrap();
        assert!(result.contains("key") && result.contains("val"));
    }

    #[test]
    fn test_compact_no_theme() {
        let mut map = mq_lang::DictMap::default();
        map.insert(mq_syntax::Ident::new("a"), RuntimeValue::from(1usize));
        map.insert(mq_syntax::Ident::new("b"), RuntimeValue::from(2usize));
        let values = vec![RuntimeValue::Dict(Shared::new(map))];
        let result = runtime_values_to_json(&values, None, true, "  ").unwrap();
        assert_eq!(result, r#"{"a":1,"b":2}"#);
    }

    #[test]
    fn test_compact_with_theme() {
        let theme = plain_theme();
        let mut map = mq_lang::DictMap::default();
        map.insert(mq_syntax::Ident::new("a"), RuntimeValue::from(1usize));
        map.insert(mq_syntax::Ident::new("b"), RuntimeValue::from(2usize));
        let values = vec![RuntimeValue::Dict(Shared::new(map))];
        let result = runtime_values_to_json(&values, Some(&theme), true, "  ").unwrap();
        assert_eq!(result, r#"{"a":1,"b":2}"#);
    }

    #[test]
    fn test_custom_indent_width_no_theme() {
        let values = vec![RuntimeValue::Array(Shared::new(vec![RuntimeValue::from(1usize)]))];
        let result = runtime_values_to_json(&values, None, false, "    ").unwrap();
        assert_eq!(result, "[\n    1\n]");
    }

    #[test]
    fn test_tab_indent_with_theme() {
        let theme = plain_theme();
        let values = vec![RuntimeValue::Array(Shared::new(vec![RuntimeValue::from(1usize)]))];
        let result = runtime_values_to_json(&values, Some(&theme), false, "\t").unwrap();
        assert_eq!(result, "[\n\t1\n]");
    }
}
