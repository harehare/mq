//! XML output rendering for the `--output-format xml` CLI option.
//!
//! When the (sole) value is shaped like the `xml_parse()` module's output
//! (a dict with a string `tag` key, plus `attributes`/`children`/`text`), it is
//! rendered back into a faithful XML element tree, giving a true round trip with
//! `-I xml`. Otherwise the values are merged into a [`serde_json::Value`] (see
//! [`crate::json`]) and rendered generically under a `<root>` element, with JSON
//! arrays becoming repeated `<item>` elements.

use miette::miette;
#[cfg(test)]
use mq_lang::Shared;
use mq_lang::{Ident, RuntimeValue};
use quick_xml::Writer;
use quick_xml::escape::escape;
use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};
use std::collections::BTreeMap;

fn xml_err(e: std::io::Error) -> miette::Report {
    miette!("Failed to write XML: {}", e)
}

fn is_element_shape(map: &BTreeMap<Ident, RuntimeValue>) -> bool {
    matches!(map.get(&Ident::new("tag")), Some(RuntimeValue::String(_)))
}

/// Writes a `{tag, attributes, children, text}`-shaped dict (the shape produced by
/// `xml_parse()`) as a real XML element, recursing into `children` of the same shape.
fn write_element(writer: &mut Writer<&mut Vec<u8>>, map: &BTreeMap<Ident, RuntimeValue>) -> std::io::Result<()> {
    let tag = match map.get(&Ident::new("tag")) {
        Some(RuntimeValue::String(s)) => s.clone(),
        _ => return Ok(()),
    };

    let mut start = BytesStart::new(tag.as_str());
    if let Some(RuntimeValue::Dict(attrs)) = map.get(&Ident::new("attributes")) {
        for (k, v) in attrs.iter() {
            start.push_attribute((k.to_string().as_str(), escape(v.to_string()).as_ref()));
        }
    }

    let children: &[RuntimeValue] = match map.get(&Ident::new("children")) {
        Some(RuntimeValue::Array(items)) => items,
        _ => &[],
    };
    let text = match map.get(&Ident::new("text")) {
        Some(RuntimeValue::String(s)) => Some(s.clone()),
        _ => None,
    };

    if children.is_empty() && text.is_none() {
        writer.write_event(Event::Empty(start))?;
    } else {
        writer.write_event(Event::Start(start))?;
        if let Some(t) = text {
            writer.write_event(Event::Text(BytesText::new(&t)))?;
        }
        for child in children {
            if let RuntimeValue::Dict(cmap) = child {
                write_element(writer, cmap)?;
            }
        }
        writer.write_event(Event::End(BytesEnd::new(tag.as_str())))?;
    }

    Ok(())
}

/// Writes an arbitrary JSON value under `tag`, mapping objects to child elements,
/// arrays to repeated `<item>` elements, and scalars to text content.
fn write_json_as_xml(writer: &mut Writer<&mut Vec<u8>>, tag: &str, value: &serde_json::Value) -> std::io::Result<()> {
    match value {
        serde_json::Value::Null => {
            writer.write_event(Event::Empty(BytesStart::new(tag)))?;
        }
        serde_json::Value::Object(map) => {
            if map.is_empty() {
                writer.write_event(Event::Empty(BytesStart::new(tag)))?;
            } else {
                writer.write_event(Event::Start(BytesStart::new(tag)))?;
                for (k, v) in map {
                    write_json_as_xml(writer, k, v)?;
                }
                writer.write_event(Event::End(BytesEnd::new(tag)))?;
            }
        }
        serde_json::Value::Array(items) => {
            if items.is_empty() {
                writer.write_event(Event::Empty(BytesStart::new(tag)))?;
            } else {
                writer.write_event(Event::Start(BytesStart::new(tag)))?;
                for item in items {
                    write_json_as_xml(writer, "item", item)?;
                }
                writer.write_event(Event::End(BytesEnd::new(tag)))?;
            }
        }
        serde_json::Value::String(s) => {
            writer.write_event(Event::Start(BytesStart::new(tag)))?;
            writer.write_event(Event::Text(BytesText::new(s)))?;
            writer.write_event(Event::End(BytesEnd::new(tag)))?;
        }
        serde_json::Value::Bool(_) | serde_json::Value::Number(_) => {
            writer.write_event(Event::Start(BytesStart::new(tag)))?;
            writer.write_event(Event::Text(BytesText::new(&value.to_string())))?;
            writer.write_event(Event::End(BytesEnd::new(tag)))?;
        }
    }
    Ok(())
}

/// Converts a list of [`RuntimeValue`]s into an XML string.
pub(crate) fn runtime_values_to_xml(runtime_values: &[RuntimeValue]) -> miette::Result<String> {
    let non_none: Vec<&RuntimeValue> = runtime_values.iter().filter(|v| !v.is_none()).collect();

    let mut buf = Vec::new();
    let mut writer = Writer::new_with_indent(&mut buf, b' ', 2);
    writer
        .write_event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)))
        .map_err(xml_err)?;

    if let [RuntimeValue::Dict(map)] = non_none.as_slice()
        && is_element_shape(map)
    {
        write_element(&mut writer, map).map_err(xml_err)?;
    } else {
        let json_value = super::json::runtime_values_to_json_value(runtime_values);
        write_json_as_xml(&mut writer, "root", &json_value).map_err(xml_err)?;
    }

    let mut result = String::from_utf8(buf).map_err(|e| miette!("Failed to convert XML output to UTF-8: {}", e))?;
    result.push('\n');
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_element_shape_round_trip() {
        let mut attrs = BTreeMap::new();
        attrs.insert(Ident::new("id"), RuntimeValue::String("1".to_string()));
        let mut map = BTreeMap::new();
        map.insert(Ident::new("tag"), RuntimeValue::String("root".to_string()));
        map.insert(Ident::new("attributes"), RuntimeValue::Dict(Shared::new(attrs)));
        map.insert(Ident::new("children"), RuntimeValue::Array(Shared::new(vec![])));
        map.insert(Ident::new("text"), RuntimeValue::String("hello".to_string()));

        let values = vec![RuntimeValue::Dict(Shared::new(map))];
        let result = runtime_values_to_xml(&values).unwrap();
        assert!(result.contains("<root id=\"1\">hello</root>"));
    }

    #[test]
    fn test_generic_dict() {
        let mut map = BTreeMap::new();
        map.insert(Ident::new("name"), RuntimeValue::String("Alice".to_string()));
        let values = vec![RuntimeValue::Dict(Shared::new(map))];
        let result = runtime_values_to_xml(&values).unwrap();
        assert!(result.contains("<root>"));
        assert!(result.contains("<name>Alice</name>"));
    }

    #[test]
    fn test_generic_array_becomes_items() {
        let values = vec![RuntimeValue::Array(Shared::new(vec![
            RuntimeValue::String("a".to_string()),
            RuntimeValue::String("b".to_string()),
        ]))];
        let result = runtime_values_to_xml(&values).unwrap();
        assert!(result.contains("<item>a</item>"));
        assert!(result.contains("<item>b</item>"));
    }

    #[test]
    fn test_text_escaping() {
        let values = vec![RuntimeValue::String("<tag> & \"quote\"".to_string())];
        let result = runtime_values_to_xml(&values).unwrap();
        assert!(result.contains("&lt;tag&gt; &amp;"));
    }

    #[test]
    fn test_empty_dict() {
        let values = vec![RuntimeValue::Dict(Shared::new(BTreeMap::new()))];
        let result = runtime_values_to_xml(&values).unwrap();
        assert!(result.contains("<root/>"));
    }
}
