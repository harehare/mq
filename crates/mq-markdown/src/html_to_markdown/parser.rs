use super::node::{HtmlElement, HtmlNode};
use rustc_hash::FxHashMap;
use scraper::Node;

// Helper type alias to refer to the NodeRef type that scraper uses
// This comes from scraper's ego-tree dependency (0.10.0), not from the workspace
type NodeRef<'a> = ego_tree::NodeRef<'a, Node>;

fn map_node_to_html_node(node_ref: NodeRef) -> miette::Result<Option<HtmlNode>> {
    match node_ref.value() {
        Node::Text(text) => {
            let text_content = text.text.to_string();
            Ok(Some(HtmlNode::Text(text_content)))
        }
        Node::Element(element) => {
            let tag_name = element.name().to_lowercase();
            let mut attributes = FxHashMap::default();

            for (attr_name, attr_value) in element.attrs() {
                attributes.insert(attr_name.to_string(), Some(attr_value.to_string()));
            }

            // Convert children recursively
            let mut children = Vec::new();
            for child in node_ref.children() {
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
        Node::Comment(comment) => Ok(Some(HtmlNode::Comment(comment.comment.to_string()))),
        _ => Ok(None),
    }
}

pub fn map_nodes_to_html_nodes(nodes: Vec<NodeRef>) -> miette::Result<Vec<HtmlNode>> {
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
