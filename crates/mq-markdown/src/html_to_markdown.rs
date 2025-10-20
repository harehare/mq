#![cfg(feature = "html-to-markdown")]
pub mod converter;
pub mod node;
pub mod options;
pub mod parser;

use miette::miette;
pub use options::ConversionOptions;
use scraper::Html;
use scraper::Selector;
use std::collections::BTreeMap;

fn find_element_by_tag_name<'a>(html: &'a Html, tag_name: &str) -> Option<scraper::ElementRef<'a>> {
    use scraper::Selector;
    if let Ok(selector) = Selector::parse(tag_name) {
        html.select(&selector).next()
    } else {
        None
    }
}

fn extract_front_matter_from_head_ref(html: &Html) -> Option<BTreeMap<String, serde_yaml::Value>> {
    // First, find the <head> element
    let head_element = find_element_by_tag_name(html, "head")?;
    let mut fm_map = BTreeMap::new();

    // Extract <title> only from within <head>
    if let Ok(title_selector) = Selector::parse("title")
        && let Some(title_node) = head_element.select(&title_selector).next()
    {
        let title_str = title_node.text().collect::<String>().trim().to_string();
        if !title_str.is_empty() {
            fm_map.insert("title".to_string(), serde_yaml::Value::String(title_str));
        }
    }

    // Extract <meta> tags only from within <head>
    if let Ok(meta_selector) = Selector::parse("meta") {
        let mut keywords: Vec<serde_yaml::Value> = Vec::new();

        for meta_node in head_element.select(&meta_selector) {
            if let (Some(name_attr), Some(content_attr)) = (
                meta_node.value().attr("name"),
                meta_node.value().attr("content"),
            ) && !content_attr.is_empty()
            {
                match name_attr.to_lowercase().as_str() {
                    "description" => {
                        fm_map.insert(
                            "description".to_string(),
                            serde_yaml::Value::String(content_attr.to_string()),
                        );
                    }
                    "keywords" => {
                        content_attr
                            .split(',')
                            .map(|s| s.trim())
                            .filter(|s| !s.is_empty())
                            .for_each(|k| keywords.push(serde_yaml::Value::String(k.to_string())));
                    }
                    "author" => {
                        fm_map.insert(
                            "author".to_string(),
                            serde_yaml::Value::String(content_attr.to_string()),
                        );
                    }
                    _ => {}
                }
            }
        }

        if !keywords.is_empty() {
            fm_map.insert(
                "keywords".to_string(),
                serde_yaml::Value::Sequence(keywords),
            );
        }
    }

    if fm_map.is_empty() {
        None
    } else {
        Some(fm_map)
    }
}

pub fn convert_html_to_markdown(
    html_input: &str,
    options: ConversionOptions,
) -> miette::Result<String> {
    if html_input.trim().is_empty() {
        return Ok("".to_string());
    }

    let html = Html::parse_document(html_input);

    let mut front_matter_str = String::new();

    if options.generate_front_matter
        && let Some(fm_data) = extract_front_matter_from_head_ref(&html)
        && !fm_data.is_empty()
    {
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
                let content = yaml
                    .trim_start_matches("---\n")
                    .trim_end_matches('\n')
                    .trim_end_matches("...");
                front_matter_str = format!("---\n{}\n---\n\n", content.trim());
            }
            Err(_) => {
                return Err(miette!("YAML serialization failed"));
            }
        }
    }

    let doc_children: Vec<_> = html.root_element().children().collect();
    let nodes_for_markdown_conversion = parser::map_nodes_to_html_nodes(doc_children)?;
    let body_markdown =
        converter::convert_nodes_to_markdown(&nodes_for_markdown_conversion, options)?;

    Ok(format!("{}{}", front_matter_str, body_markdown))
}
