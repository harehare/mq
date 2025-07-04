use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum HtmlNode {
    Text(String),
    Element(HtmlElement),
    Comment(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct HtmlElement {
    pub tag_name: String,
    pub attributes: HashMap<String, Option<String>>,
    pub children: Vec<HtmlNode>,
}
