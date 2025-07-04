#![cfg(feature = "html-to-markdown")]
pub mod converter;
pub mod node;
pub mod options;
pub mod parser;

use html5ever::parse_document;
use html5ever::tendril::TendrilSink;
use markup5ever_rcdom::{Node, NodeData, RcDom};
use miette::miette;
pub use options::ConversionOptions;
use std::collections::BTreeMap;
use std::rc::Rc;

fn find_element_by_tag_name(node: &Rc<Node>, tag_name: &str) -> Option<Rc<Node>> {
    match &node.data {
        NodeData::Element { name, .. } if name.local.to_lowercase() == tag_name => {
            Some(node.clone())
        }
        _ => {
            for child in node.children.borrow().iter() {
                if let Some(found) = find_element_by_tag_name(child, tag_name) {
                    return Some(found);
                }
            }
            None
        }
    }
}

fn find_elements_by_tag_name(node: &Rc<Node>, tag_name: &str) -> Vec<Rc<Node>> {
    let mut results = Vec::new();

    match &node.data {
        NodeData::Element { name, .. } if name.local.to_lowercase() == tag_name => {
            results.push(node.clone());
        }
        _ => {}
    }

    for child in node.children.borrow().iter() {
        results.extend(find_elements_by_tag_name(child, tag_name));
    }

    results
}

fn get_element_text_content(node: &Rc<Node>) -> String {
    let mut text = String::new();

    match &node.data {
        NodeData::Text { contents } => {
            text.push_str(&contents.borrow());
        }
        NodeData::Element { .. } => {
            for child in node.children.borrow().iter() {
                text.push_str(&get_element_text_content(child));
            }
        }
        _ => {}
    }

    text
}

fn get_element_attribute(node: &Rc<Node>, attr_name: &str) -> Option<String> {
    match &node.data {
        NodeData::Element { attrs, .. } => {
            for attr in attrs.borrow().iter() {
                if attr.name.local.to_string() == attr_name {
                    return Some(attr.value.to_string());
                }
            }
            None
        }
        _ => None,
    }
}

fn extract_front_matter_from_head_ref(
    head_node: Option<Rc<Node>>,
) -> Option<BTreeMap<String, serde_yaml::Value>> {
    let head_el = head_node?;
    let mut fm_map = BTreeMap::new();

    // Extract <title>
    if let Some(title_node) = find_element_by_tag_name(&head_el, "title") {
        let title_str = get_element_text_content(&title_node).trim().to_string();
        if !title_str.is_empty() {
            fm_map.insert("title".to_string(), serde_yaml::Value::String(title_str));
        }
    }

    // Extract <meta> tags
    let meta_nodes = find_elements_by_tag_name(&head_el, "meta");
    let mut keywords: Vec<serde_yaml::Value> = Vec::new();

    for meta_node in meta_nodes {
        if let (Some(name_attr), Some(content_attr)) = (
            get_element_attribute(&meta_node, "name"),
            get_element_attribute(&meta_node, "content"),
        ) {
            if !content_attr.is_empty() {
                match name_attr.to_lowercase().as_str() {
                    "description" => {
                        fm_map.insert(
                            "description".to_string(),
                            serde_yaml::Value::String(content_attr),
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
                            serde_yaml::Value::String(content_attr),
                        );
                    }
                    _ => {}
                }
            }
        }
    }

    if !keywords.is_empty() {
        fm_map.insert(
            "keywords".to_string(),
            serde_yaml::Value::Sequence(keywords),
        );
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

    let dom = parse_document(RcDom::default(), Default::default())
        .from_utf8()
        .read_from(&mut html_input.as_bytes())
        .map_err(|_| miette!("Failed to parse HTML document"))?;

    let mut front_matter_str = String::new();

    if options.generate_front_matter {
        let head_node = find_element_by_tag_name(&dom.document, "head");

        if let Some(fm_data) = extract_front_matter_from_head_ref(head_node) {
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
        }
    }

    let doc_children: Vec<Rc<Node>> = dom.document.children.borrow().clone();
    let nodes_for_markdown_conversion = parser::map_nodes_to_html_nodes(&doc_children)?;
    let body_markdown =
        converter::convert_nodes_to_markdown(&nodes_for_markdown_conversion, options)?;

    Ok(format!("{}{}", front_matter_str, body_markdown))
}
