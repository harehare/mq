#[cfg(feature = "html-to-markdown")]
use scraper::{Html, Selector, ElementRef, Node as ScraperNode};
#[cfg(feature = "html-to-markdown")]
use std::collections::HashMap;
#[cfg(feature = "html-to-markdown")]
use super::node::{HtmlNode, HtmlElement};
#[cfg(feature = "html-to-markdown")]
use super::error::HtmlToMarkdownError;

#[cfg(feature = "html-to-markdown")]
fn map_scraper_node_to_html_node(scraper_node: scraper::Node, doc_source_for_error: &str) -> Result<Option<HtmlNode>, HtmlToMarkdownError> {
    match scraper_node {
        ScraperNode::Text(text_node) => {
            Ok(Some(HtmlNode::Text(text_node.text.to_string())))
        }
        ScraperNode::Element(element_ref) => {
            let tag_name = element_ref.name().to_lowercase();
            let mut attributes = HashMap::new();
            for (name, value) in element_ref.attrs() {
                attributes.insert(name.to_string(), Some(value.to_string()));
            }

            let mut children = Vec::new();
            for child_scraper_node in element_ref.children() {
                if let Some(child_html_node) = map_scraper_node_to_html_node(child_scraper_node, doc_source_for_error)? {
                    children.push(child_html_node);
                }
            }

            Ok(Some(HtmlNode::Element(HtmlElement {
                tag_name,
                attributes,
                children,
            })))
        }
        ScraperNode::Comment(comment_node) => {
            Ok(Some(HtmlNode::Comment(comment_node.comment.to_string())))
        }
        ScraperNode::Document | ScraperNode::Doctype(_) | ScraperNode::Fragment | ScraperNode::ProcessingInstruction(_) => {
            Ok(None)
        }
    }
}

#[cfg(feature = "html-to-markdown")]
pub fn map_scraper_nodes_to_html_nodes<'a>(
    scraper_nodes_iterator: impl Iterator<Item = scraper::Node<'a>>,
    doc_source_for_error: &str,
) -> Result<Vec<HtmlNode>, HtmlToMarkdownError> {
    let mut html_nodes = Vec::new();
    for scraper_node in scraper_nodes_iterator {
        match map_scraper_node_to_html_node(scraper_node, doc_source_for_error) {
            Ok(Some(html_node)) => html_nodes.push(html_node),
            Ok(None) => {}
            Err(e) => return Err(e),
        }
    }
    Ok(html_nodes)
}

// The main `parse` function is effectively moved to `html_to_markdown/mod.rs`
// as it needs to handle `head` and `body` separation before calling node mapping.
// This file now primarily contains the node mapping logic.
// We can keep a simplified `parse` here for fragments if needed by other parts,
// or remove it if `mod.rs` handles all initial parsing.
// For now, let's remove the old `parse` to avoid confusion.
// pub fn parse(html_input: &str) -> Result<Vec<HtmlNode>, HtmlToMarkdownError> { ... }
}
