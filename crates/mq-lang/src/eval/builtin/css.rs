//! `css`/`css_text`/`css_attr` builtins: CSS-selector search directly over a raw HTML string.
//!
//! `-I html` (see [`crate::parse_html_input`]) always routes through
//! `mq_markdown::Markdown::from_html_str`, which unconditionally converts the document into a
//! Markdown AST and discards container tags (`div`/`span`/`section`, ...) along with their
//! `class`/`id`/`data-*` attributes. That conversion has no toggle, so information that only
//! lives on those attributes is unrecoverable once a value has gone through it. These builtins
//! query the pre-conversion HTML string instead — typically the raw `-I html` input or an
//! `http()` response body — so element tags and attributes are still available.
//!
//! Gated at compile time by the `css-selector` feature.

use std::collections::BTreeMap;

use ego_tree::NodeRef;
use scraper::{Html, Node, Selector};

use crate::eval::runtime_value::RuntimeValue;
use crate::{Ident, Shared};

use super::Error;

fn err(msg: impl std::fmt::Display) -> Error {
    Error::Runtime(format!("css: {msg}"))
}

// Same `{tag, attributes, children, text}` shape as `_xml_parse`; whitespace-only text is dropped.
fn build_element(node: NodeRef<'_, Node>) -> RuntimeValue {
    let element = node.value().as_element().expect("node is an element");
    let mut attributes = BTreeMap::new();

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

    let mut dict = BTreeMap::new();
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

fn parse_selector(selector: &str) -> Result<Selector, Error> {
    Selector::parse(selector).map_err(|e| err(format!("invalid CSS selector {selector:?}: {e}")))
}

/// Returns the outer HTML of every element in `html` matching `selector`.
pub(super) fn select_html(html: &str, selector: &str) -> Result<Vec<String>, Error> {
    let selector = parse_selector(selector)?;
    let document = Html::parse_document(html);
    Ok(document.select(&selector).map(|el| el.html()).collect())
}

/// Returns the concatenated text content of every element in `html` matching `selector`.
pub(super) fn select_text(html: &str, selector: &str) -> Result<Vec<String>, Error> {
    let selector = parse_selector(selector)?;
    let document = Html::parse_document(html);
    Ok(document
        .select(&selector)
        .map(|el| el.text().collect::<String>())
        .collect())
}

/// Returns the value of attribute `name` for every element in `html` matching `selector`,
/// `None` where the element doesn't have that attribute.
pub(super) fn select_attr(html: &str, selector: &str, name: &str) -> Result<Vec<Option<String>>, Error> {
    let selector = parse_selector(selector)?;
    let document = Html::parse_document(html);
    Ok(document
        .select(&selector)
        .map(|el| el.value().attr(name).map(str::to_string))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    const HTML: &str = r#"
        <html><body>
            <div class="price" data-id="1">$10</div>
            <div class="price" data-id="2">$20</div>
            <a href="https://example.com">link</a>
            <span class="empty"></span>
        </body></html>
    "#;

    #[test]
    fn test_select_html_returns_outer_html_of_matches() {
        let result = select_html(HTML, ".price").unwrap();
        assert_eq!(result.len(), 2);
        assert!(result[0].contains("data-id=\"1\""));
        assert!(result[1].contains("data-id=\"2\""));
    }

    #[test]
    fn test_select_html_no_matches_returns_empty_array() {
        assert_eq!(select_html(HTML, ".missing").unwrap(), Vec::<String>::new());
    }

    #[test]
    fn test_select_text_returns_text_content() {
        assert_eq!(select_text(HTML, ".price").unwrap(), vec!["$10", "$20"]);
    }

    #[test]
    fn test_select_text_empty_element_returns_empty_string() {
        assert_eq!(select_text(HTML, ".empty").unwrap(), vec![""]);
    }

    #[test]
    fn test_select_attr_returns_named_attribute_values() {
        assert_eq!(
            select_attr(HTML, ".price", "data-id").unwrap(),
            vec![Some("1".to_string()), Some("2".to_string())]
        );
        assert_eq!(
            select_attr(HTML, "a", "href").unwrap(),
            vec![Some("https://example.com".to_string())]
        );
    }

    #[test]
    fn test_select_attr_missing_attribute_is_none() {
        assert_eq!(select_attr(HTML, ".price", "data-missing").unwrap(), vec![None, None]);
    }

    #[test]
    fn test_invalid_selector_is_error() {
        assert!(select_html(HTML, "[[[").is_err());
        assert!(select_text(HTML, "[[[").is_err());
        assert!(select_attr(HTML, "[[[", "href").is_err());
    }
}
