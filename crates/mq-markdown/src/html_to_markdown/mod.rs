//! Converts HTML content to Markdown.
// ... (module docs from before) ...

#[cfg(feature = "html-to-markdown")]
pub mod converter;
#[cfg(feature = "html-to-markdown")]
pub mod error;
#[cfg(feature = "html-to-markdown")]
pub mod node;
#[cfg(feature = "html-to-markdown")]
pub mod parser;
#[cfg(feature = "html-to-markdown")]
pub mod options;

#[cfg(feature = "html-to-markdown")]
use scraper::{Html, Selector, ElementRef};
#[cfg(feature = "html-to-markdown")]
use serde_yaml;
#[cfg(feature = "html-to-markdown")]
use std::collections::BTreeMap;

#[cfg(feature = "html-to-markdown")]
pub use error::HtmlToMarkdownError;
#[cfg(feature = "html-to-markdown")]
pub use options::ConversionOptions;


#[cfg(feature = "html-to-markdown")]
fn extract_front_matter_from_head_ref(
    head_el_ref: Option<ElementRef>,
    // html_input_for_error: &str, // Not strictly needed if errors are handled via Result/Option
) -> Option<BTreeMap<String, serde_yaml::Value>> {
    let head_el = head_el_ref?;
    let mut fm_map = BTreeMap::new();

    // Extract <title>
    if let Ok(title_selector) = Selector::parse("title") { // Selector parsing should not fail for "title"
        if let Some(title_el_r) = head_el.select(&title_selector).next() {
            let title_str = title_el_r.text().collect::<String>().trim().to_string();
            if !title_str.is_empty() {
                fm_map.insert("title".to_string(), serde_yaml::Value::String(title_str));
            }
        }
    }

    // Extract <meta> tags
    if let Ok(meta_selector) = Selector::parse("meta") {
        let mut keywords: Vec<serde_yaml::Value> = Vec::new();
        for meta_el_r in head_el.select(&meta_selector) {
            if let Some(name_attr) = meta_el_r.value().attr("name") {
                if let Some(content_attr) = meta_el_r.value().attr("content") {
                    if !content_attr.is_empty() {
                        match name_attr.to_lowercase().as_str() {
                            "description" => {
                                fm_map.insert("description".to_string(), serde_yaml::Value::String(content_attr.to_string()));
                            }
                            "keywords" => {
                                content_attr.split(',')
                                    .map(|s| s.trim())
                                    .filter(|s| !s.is_empty())
                                    .for_each(|k| keywords.push(serde_yaml::Value::String(k.to_string())));
                            }
                            "author" => {
                                fm_map.insert("author".to_string(), serde_yaml::Value::String(content_attr.to_string()));
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        if !keywords.is_empty() {
            fm_map.insert("keywords".to_string(), serde_yaml::Value::Sequence(keywords));
        }
    }

    if fm_map.is_empty() { None } else { Some(fm_map) }
}


#[cfg(feature = "html-to-markdown")]
pub fn convert_html_to_markdown(
    html_input: &str,
    options: ConversionOptions,
) -> Result<String, HtmlToMarkdownError> {
    if html_input.trim().is_empty() {
        return Ok("".to_string());
    }

    let document = Html::parse_document(html_input);
    let mut front_matter_str = String::new();

    if options.generate_front_matter {
        let head_selector = Selector::parse("head").map_err(|e| HtmlToMarkdownError::ParseError {
            html_snippet: "Internal selector error for <head>".to_string(), message: format!("{:?}", e)
        })?;
        let head_el_ref = document.select(&head_selector).next(); // Option<ElementRef>

        if let Some(fm_data) = extract_front_matter_from_head_ref(head_el_ref) {
            if !fm_data.is_empty() {
                // Convert BTreeMap<String, Value> to serde_yaml::Mapping (which is BTreeMap<Value, Value>)
                let mut yaml_map = serde_yaml::Mapping::new();
                for (k, v) in fm_data {
                    yaml_map.insert(serde_yaml::Value::String(k), v);
                }
                let yaml_value = serde_yaml::Value::Mapping(yaml_map);

                match serde_yaml::to_string(&yaml_value) {
                    Ok(yaml) => {
                        // serde_yaml::to_string might add its own "---" if it's a single doc,
                        // or not if it's just a mapping. We want to ensure our format.
                        // It typically does not add --- for a Value::Mapping.
                        let content = yaml.trim_start_matches("---\n").trim_end_matches('\n').trim_end_matches("...");
                        front_matter_str = format!("---\n{}---\n\n", content.trim());
                    }
                    Err(e) => return Err(HtmlToMarkdownError::ParseError{
                        html_snippet: "YAML serialization failed".to_string(), message: e.to_string()
                    }),
                }
            }
        }
    }

    let body_selector = Selector::parse("body").map_err(|e| HtmlToMarkdownError::ParseError{
        html_snippet: "Internal selector error for <body>".to_string(), message: format!("{:?}", e)
    })?;

    let nodes_for_markdown_conversion: Vec<node::HtmlNode>;

    if let Some(body_element_ref) = document.select(&body_selector).next() {
        // Full HTML document with a <body> tag
        nodes_for_markdown_conversion = parser::map_scraper_nodes_to_html_nodes(body_element_ref.children(), html_input)?;
    } else {
        // No <body> tag, treat the entire input as an HTML fragment
        let fragment = Html::parse_fragment(html_input);
        nodes_for_markdown_conversion = parser::map_scraper_nodes_to_html_nodes(fragment.root_element().children(), html_input)?;
    }

    let body_markdown = converter::convert_nodes_to_markdown(&nodes_for_markdown_conversion, html_input, options.extract_scripts_as_code_blocks)?;

    Ok(format!("{}{}", front_matter_str, body_markdown))
}
