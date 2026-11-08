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

/// Elements whose content is raw text, so tag-shaped substrings inside are not markup.
const RAW_TEXT_ELEMENTS: &[&str] = &[
    "script", "style", "textarea", "title", "xmp", "iframe", "noembed", "noframes",
];

/// Returns the index just past the `>` that ends the tag starting at `pos`, skipping quoted
/// attribute values. `None` when the tag is unterminated.
fn tag_end(bytes: &[u8], mut pos: usize) -> Option<usize> {
    while pos < bytes.len() {
        match bytes[pos] {
            b'>' => return Some(pos + 1),
            b'=' => {
                pos += 1;
                while bytes.get(pos).is_some_and(u8::is_ascii_whitespace) {
                    pos += 1;
                }
                if let Some(&q @ (b'"' | b'\'')) = bytes.get(pos) {
                    let close = bytes[pos + 1..].iter().position(|&c| c == q)?;
                    pos += close + 2;
                }
            }
            _ => pos += 1,
        }
    }
    None
}

/// Index of the next `</name` (ASCII case-insensitive) at or after `from`, or `bytes.len()`.
fn find_close_tag(bytes: &[u8], from: usize, name: &[u8]) -> usize {
    let n = name.len() + 2;
    (from..bytes.len().saturating_sub(n - 1))
        .find(|&i| bytes[i] == b'<' && bytes[i + 1] == b'/' && bytes[i + 2..i + n].eq_ignore_ascii_case(name))
        .unwrap_or(bytes.len())
}

/// Rejects markup whose tags nest deeper than `MAX_RAW_DEPTH`, without building a tree.
///
/// Comments, quoted attribute values and raw text elements (`<script>`, `<style>`, ...) are
/// skipped, so only real tags are counted.
pub fn check_raw_depth(html: &str) -> miette::Result<()> {
    let bytes = html.as_bytes();
    let mut depth = 0usize;
    let mut pos = 0;

    while let Some(offset) = bytes[pos..].iter().position(|&b| b == b'<') {
        pos += offset + 1;
        let Some(&first) = bytes.get(pos) else { break };

        if bytes[pos..].starts_with(b"!--") {
            match html[pos + 3..].find("-->") {
                Some(end) => pos += 3 + end + 3,
                None => break,
            }
        } else if first == b'!' || first == b'?' {
            match bytes[pos..].iter().position(|&b| b == b'>') {
                Some(end) => pos += end + 1,
                None => break,
            }
        } else if first == b'/' {
            if bytes.get(pos + 1).is_some_and(u8::is_ascii_alphabetic) {
                depth = depth.saturating_sub(1);
                match tag_end(bytes, pos + 1) {
                    Some(end) => pos = end,
                    None => break,
                }
            }
        } else if first.is_ascii_alphabetic() {
            let name_len = bytes[pos..]
                .iter()
                .position(|c| !c.is_ascii_alphanumeric())
                .unwrap_or(bytes.len() - pos);
            let name = &bytes[pos..pos + name_len];
            let Some(end) = tag_end(bytes, pos + name_len) else {
                break;
            };
            let self_closing = bytes[end - 2] == b'/';
            pos = end;

            if self_closing || VOID_ELEMENTS.iter().any(|v| v.as_bytes().eq_ignore_ascii_case(name)) {
                continue;
            }

            if RAW_TEXT_ELEMENTS
                .iter()
                .any(|r| r.as_bytes().eq_ignore_ascii_case(name))
            {
                pos = find_close_tag(bytes, pos, name);
                continue;
            }

            depth += 1;
            if depth > MAX_RAW_DEPTH {
                return Err(miette::miette!("HTML is nested deeper than {MAX_RAW_DEPTH} levels"));
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

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case::quoted_attr("<div title=\"</div>\">")]
    #[case::single_quoted_attr("<div title='</div>'>")]
    fn test_check_raw_depth_rejects_quoted_close_tags(#[case] unit: &str) {
        let n = MAX_RAW_DEPTH + 1;
        let input = format!("{}{}", unit.repeat(n), "</div>".repeat(n));
        assert!(check_raw_depth(&input).is_err());
    }

    #[test]
    fn test_check_raw_depth_ignores_script_and_comment_text() {
        let n = MAX_RAW_DEPTH + 1;
        let script = format!("<script>{}</script><a href=\"/next\">x</a>", "<div>".repeat(n));
        let comment = format!("<!--{}--><p>x</p>", "<div>".repeat(n));
        let attr = format!("<a title=\"{}\">x</a>", "<div>".repeat(n));
        assert!(check_raw_depth(&script).is_ok());
        assert!(check_raw_depth(&comment).is_ok());
        assert!(check_raw_depth(&attr).is_ok());
    }

    #[test]
    fn test_check_raw_depth_rejects_plain_nesting() {
        let input = "<div>".repeat(MAX_RAW_DEPTH + 1);
        assert!(check_raw_depth(&input).is_err());
    }
}
