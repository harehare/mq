//! Converts HTML content to Markdown.
//!
//! This module provides the `convert_html_to_markdown` function, which takes an HTML string
//! as input and attempts to convert it into a Markdown string. The conversion process
//! involves parsing the HTML into an internal representation and then rendering that
//! representation as Markdown.
//!
//! This functionality is available when the `html-to-markdown` feature of the
//! `mq-markdown` crate is enabled.
//!
//! ## Current Status
//!
//! The HTML parser and Markdown converter are currently under development.
//! Support for various HTML tags and attributes will be added incrementally.
//! At present, only very basic HTML structures might be handled correctly.
//!
//! ## Error Handling
//!
//! The conversion can fail due to parsing errors (e.g., malformed HTML) or if
//! unsupported HTML constructs are encountered. Errors are reported using the
//! `HtmlToMarkdownError` type, which provides details about the failure.
//!
//! ## Example
//!
//! ```rust
//! # #[cfg(feature = "html-to-markdown")] // For doctest
//! # fn main() -> Result<(), mq_markdown::HtmlToMarkdownError> {
//! use mq_markdown::convert_html_to_markdown;
//!
//! let html = "<p>Hello, <strong>world</strong>!</p>";
//! // The actual output will depend on the implemented parser and converter logic.
//! // This is an illustrative example.
//! let expected_markdown = "Hello, **world**!"; // Simplified expected output
//!
//! // Placeholder: current parser is very basic, so this will likely error or give unexpected output.
//! // let markdown = convert_html_to_markdown(html)?;
//! // assert_eq!(markdown, expected_markdown);
//! # Ok(())
//! # }
//! # #[cfg(not(feature = "html-to-markdown"))]
//! # fn main() {}
//! ```

#[cfg(feature = "html-to-markdown")]
pub mod converter;
#[cfg(feature = "html-to-markdown")]
pub mod error;
#[cfg(feature = "html-to-markdown")]
pub mod node;
#[cfg(feature = "html-to-markdown")]
pub mod parser;
#[cfg(feature = "html-to-markdown")]
pub mod options; // Added

#[cfg(feature = "html-to-markdown")]
pub use error::HtmlToMarkdownError;
#[cfg(feature = "html-to-markdown")]
pub use options::ConversionOptions; // Added


#[cfg(feature = "html-to-markdown")]
fn find_element_by_name<'a>(nodes: &'a [node::HtmlNode], name: &str) -> Option<&'a node::HtmlElement> {
    nodes.iter().find_map(|node| {
        if let node::HtmlNode::Element(el) = node {
            if el.tag_name == name {
                return Some(el);
            }
        }
        None
    })
}

#[cfg(feature = "html-to-markdown")]
fn extract_text_from_title_element(title_el: &node::HtmlElement, _html_input_for_error: &str) -> Result<String, error::HtmlToMarkdownError> {
    let mut title_text = String::new();
    for child in &title_el.children {
        if let node::HtmlNode::Text(text) = child {
            title_text.push_str(text);
        }
    }
    Ok(title_text.trim().to_string())
}


#[cfg(feature = "html-to-markdown")]
fn extract_front_matter_data(
    head_element: Option<&node::HtmlElement>,
    html_input_for_error: &str,
) -> Option<BTreeMap<String, serde_yaml::Value>> {
    let head_el = head_element?;
    let mut fm_map = BTreeMap::new();

    if let Some(title_el) = find_element_by_name(&head_el.children, "title") {
        if let Ok(title_str) = extract_text_from_title_element(title_el, html_input_for_error) {
            if !title_str.is_empty() {
                fm_map.insert("title".to_string(), serde_yaml::Value::String(title_str));
            }
        }
    }

    let mut keywords: Vec<serde_yaml::Value> = Vec::new();
    for node in &head_el.children {
        if let node::HtmlNode::Element(meta_el) = node {
            if meta_el.tag_name == "meta" {
                if let Some(Some(name_attr)) = meta_el.attributes.get("name") {
                    if let Some(Some(content_attr)) = meta_el.attributes.get("content") {
                        if !content_attr.is_empty() {
                            match name_attr.to_lowercase().as_str() {
                                "description" => {
                                    fm_map.insert("description".to_string(), serde_yaml::Value::String(content_attr.clone()));
                                }
                                "keywords" => {
                                    content_attr.split(',')
                                        .map(|s| s.trim())
                                        .filter(|s| !s.is_empty())
                                        .for_each(|k| keywords.push(serde_yaml::Value::String(k.to_string())));
                                }
                                "author" => {
                                    fm_map.insert("author".to_string(), serde_yaml::Value::String(content_attr.clone()));
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
        }
    }
    if !keywords.is_empty() {
        fm_map.insert("keywords".to_string(), serde_yaml::Value::Sequence(keywords));
    }

    if fm_map.is_empty() { None } else { Some(fm_map) }
}


/// Converts an HTML string into a Markdown string.
///
/// This function parses the input HTML and then converts the parsed structure
/// into Markdown format.
///
/// # Arguments
///
/// * `html_input`: A string slice representing the HTML content to convert.
/// * `options`: Configuration options for the conversion process.
///
/// # Returns
///
/// * `Ok(String)`: A `String` containing the converted Markdown if successful.
/// * `Err(HtmlToMarkdownError)`: An error if parsing or conversion fails.
///
/// # Features
///
/// This function is only available if the `html-to-markdown` feature is enabled.
#[cfg(feature = "html-to-markdown")]
pub fn convert_html_to_markdown(
    html_input: &str,
    options: ConversionOptions,
) -> Result<String, HtmlToMarkdownError> {
    let all_nodes = parser::parse(html_input)?;

    let mut front_matter_str = String::new();
    let body_nodes: &[node::HtmlNode];

    let html_element = find_element_by_name(&all_nodes, "html");

    let (head_el_opt, body_el_opt) = if let Some(html_el) = html_element {
        (find_element_by_name(&html_el.children, "head"), find_element_by_name(&html_el.children, "body"))
    } else {
        (find_element_by_name(&all_nodes, "head"), find_element_by_name(&all_nodes, "body"))
    };

    if options.generate_front_matter {
        if let Some(fm_data) = extract_front_matter_data(head_el_opt, html_input) {
            if !fm_data.is_empty() {
                let yaml_value_map: serde_yaml::Mapping = fm_data.into_iter().map(|(k,v)| (serde_yaml::Value::String(k), v)).collect();
                let yaml_value = serde_yaml::Value::Mapping(yaml_value_map);
                match serde_yaml::to_string(&yaml_value) {
                    Ok(yaml) => {
                        front_matter_str = format!("---\n{}---\n\n", yaml.trim_start_matches("---\n").trim_end());
                    }
                    Err(_e) => { /* Log error optionally */ }
                }
            }
        }
    }

    if let Some(body_el) = body_el_opt {
        body_nodes = &body_el.children;
    } else if html_element.is_some() && head_el_opt.is_some() {
        body_nodes = &[];
    } else {
        body_nodes = &all_nodes;
    }

    let body_markdown = converter::convert_nodes_to_markdown(body_nodes, html_input, options.extract_scripts_as_code_blocks)?;

    Ok(format!("{}{}", front_matter_str, body_markdown))
}
