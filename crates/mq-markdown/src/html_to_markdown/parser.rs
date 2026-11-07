use super::node::{HtmlElement, HtmlNode};
use ego_tree;
use rustc_hash::FxHashMap;
use scraper::Node;

/// Deepest element nesting accepted, so later recursive passes cannot overflow the stack.
const MAX_DEPTH: usize = if cfg!(debug_assertions) { 128 } else { 512 };

/// Looser cap applied to the raw markup before parsing, because the HTML parser slows down
/// quadratically with nesting. Unclosed optional-end tags (`<p>`, `<li>`) count as nesting here.
const MAX_RAW_DEPTH: usize = MAX_DEPTH * 16;

const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source", "track", "wbr",
];

/// Rejects markup whose tags nest deeper than `MAX_RAW_DEPTH`, without building a tree.
pub fn check_raw_depth(html: &str) -> miette::Result<()> {
    let mut depth = 0usize;
    let mut pos = 0;
    // Position of the next `>`; only searched again once passed, to keep the scan linear.
    let mut gt = 0;

    while let Some(offset) = html[pos..].find('<') {
        pos += offset + 1;
        let Some(&first) = html.as_bytes().get(pos) else { break };

        if first == b'/' {
            depth = depth.saturating_sub(1);
        } else if first.is_ascii_alphabetic() {
            if gt < pos {
                gt = html[pos..].find('>').map_or(html.len(), |g| pos + g);
            }
            let tag = &html[pos..gt];
            let name = tag
                .split(|c: char| !c.is_ascii_alphanumeric())
                .next()
                .unwrap_or_default();

            if !tag.ends_with('/') && !VOID_ELEMENTS.iter().any(|v| v.eq_ignore_ascii_case(name)) {
                depth += 1;
                if depth > MAX_RAW_DEPTH {
                    return Err(miette::miette!("HTML is nested deeper than {MAX_RAW_DEPTH} levels"));
                }
            }
        }
    }

    Ok(())
}

fn map_node_to_html_node(node_ref: ego_tree::NodeRef<Node>, depth: usize) -> miette::Result<Option<HtmlNode>> {
    if depth > MAX_DEPTH {
        return Err(miette::miette!("HTML is nested deeper than {MAX_DEPTH} levels"));
    }

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
                if let Some(html_node) = map_node_to_html_node(child, depth + 1)? {
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

pub fn map_nodes_to_html_nodes(nodes: Vec<ego_tree::NodeRef<Node>>) -> miette::Result<Vec<HtmlNode>> {
    let mut html_nodes = Vec::new();
    for node in nodes {
        match map_node_to_html_node(node, 0) {
            Ok(Some(html_node)) => html_nodes.push(html_node),
            Ok(None) => {}
            Err(e) => return Err(e),
        }
    }
    Ok(html_nodes)
}
