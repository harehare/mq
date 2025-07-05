use super::node::{HtmlElement, HtmlNode};
use markup5ever_rcdom::{Node, NodeData};
use std::collections::HashMap;
use std::rc::Rc;

fn map_node_to_html_node(node: &Rc<Node>) -> miette::Result<Option<HtmlNode>> {
    match &node.data {
        NodeData::Text { contents } => {
            let text_content = contents.borrow().to_string();
            Ok(Some(HtmlNode::Text(text_content)))
        }
        NodeData::Element { name, attrs, .. } => {
            let tag_name = name.local.to_lowercase();
            let mut attributes = HashMap::new();

            for attr in attrs.borrow().iter() {
                let attr_name = attr.name.local.to_string();
                let attr_value = attr.value.to_string();
                attributes.insert(attr_name, Some(attr_value));
            }

            // Convert children recursively
            let mut children = Vec::new();
            for child in node.children.borrow().iter() {
                if let Some(html_node) = map_node_to_html_node(child)? {
                    children.push(html_node);
                }
            }

            Ok(Some(HtmlNode::Element(HtmlElement {
                tag_name,
                attributes,
                children,
            })))
        }
        NodeData::Comment { contents } => Ok(Some(HtmlNode::Comment(contents.to_string()))),
        NodeData::Document | NodeData::Doctype { .. } | NodeData::ProcessingInstruction { .. } => {
            Ok(None)
        }
    }
}

pub fn map_nodes_to_html_nodes(nodes: &[Rc<Node>]) -> miette::Result<Vec<HtmlNode>> {
    let mut html_nodes = Vec::new();
    for node in nodes {
        match map_node_to_html_node(node) {
            Ok(Some(html_node)) => html_nodes.push(html_node),
            Ok(None) => {}
            Err(e) => return Err(e),
        }
    }
    Ok(html_nodes)
}
