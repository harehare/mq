//! `_html_parse` builtin: parses an HTML string into the `{tag, attributes, children, text}` tree.
//!
//! Gated at compile time by the `html-parse` feature.

use crate::DictMap;

use ego_tree::NodeRef;
use scraper::{Html, Node};

use crate::runtime::runtime_value::RuntimeValue;
use crate::{Ident, Shared};

// Same `{tag, attributes, children, text}` shape as `_xml_parse`; whitespace-only text is dropped.
fn build_element(node: NodeRef<'_, Node>) -> RuntimeValue {
    let element = node.value().as_element().expect("node is an element");
    let mut attributes = DictMap::default();

    for (name, value) in element.attrs() {
        attributes.insert(Ident::new(name), RuntimeValue::String(Shared::new(value.to_string())));
    }

    let mut children = Vec::new();
    let mut text: Option<String> = None;

    for child in node.children() {
        match child.value() {
            Node::Element(_) => children.push(build_element(child)),
            Node::Text(t) => {
                let trimmed = t.trim();
                if !trimmed.is_empty() {
                    match &mut text {
                        Some(existing) => {
                            existing.push(' ');
                            existing.push_str(trimmed);
                        }
                        None => text = Some(trimmed.to_string()),
                    }
                }
            }
            _ => {}
        }
    }

    let mut dict = DictMap::default();
    dict.insert(
        Ident::new("tag"),
        RuntimeValue::String(Shared::new(element.name().to_string())),
    );
    dict.insert(Ident::new("attributes"), RuntimeValue::Dict(Shared::new(attributes)));
    dict.insert(Ident::new("children"), RuntimeValue::Array(Shared::new(children)));
    dict.insert(
        Ident::new("text"),
        text.map(|t| RuntimeValue::String(t.into()))
            .unwrap_or(RuntimeValue::NONE),
    );
    RuntimeValue::Dict(Shared::new(dict))
}

/// Parses `html` into the `{tag, attributes, children, text}` tree rooted at `<html>`;
/// comments, doctypes, and processing instructions are dropped.
pub(super) fn parse_html(html: &str) -> RuntimeValue {
    let document = Html::parse_document(html);
    build_element(*document.root_element())
}
