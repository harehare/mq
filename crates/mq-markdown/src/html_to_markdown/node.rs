#[cfg(feature = "html-to-markdown")]
use std::collections::HashMap;

#[cfg(feature = "html-to-markdown")]
#[derive(Debug, Clone, PartialEq)]
pub enum HtmlNode {
    Text(String),
    Element(HtmlElement),
    Comment(String),
}

#[cfg(feature = "html-to-markdown")]
#[derive(Debug, Clone, PartialEq)]
pub struct HtmlElement {
    pub tag_name: String,
    pub attributes: HashMap<String, Option<String>>,
    pub children: Vec<HtmlNode>,
}

// Basic constructor for convenience
#[cfg(feature = "html-to-markdown")]
impl HtmlElement {
    pub fn new(tag_name: &str, attributes: HashMap<String, Option<String>>, children: Vec<HtmlNode>) -> Self {
        HtmlElement {
            tag_name: tag_name.to_lowercase(), // Ensure tag names are consistent
            attributes,
            children,
        }
    }
}
