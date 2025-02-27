use std::fmt::{self, Display, Formatter};

use compact_str::CompactString;
use itertools::Itertools;
use markdown::mdast;

type Indent = u8;

pub const EMPTY_NODE: Node = Node::Text(Text {
    value: String::new(),
    position: None,
});

#[derive(Debug, Clone, PartialEq)]
pub struct List {
    pub value: Box<Node>,
    pub index: usize,
    pub indent: Indent,
    pub checked: Option<bool>,
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TableCell {
    pub value: Box<Node>,
    pub column: usize,
    pub row: usize,
    pub last_cell_in_row: bool,
    pub last_cell_of_in_table: bool,
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TableHeader {
    pub align: Vec<mdast::AlignKind>,
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Value {
    pub value: Box<Node>,
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Code {
    pub value: String,
    pub lang: Option<String>,
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Image {
    pub alt: String,
    pub url: String,
    pub title: Option<String>,
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImageRef {
    pub alt: String,
    pub ident: String,
    pub label: Option<String>,
    pub position: Option<Position>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Link {
    pub url: String,
    pub title: Option<String>,
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FootnoteRef {
    pub ident: String,
    pub label: Option<String>,
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Footnote {
    pub ident: String,
    pub label: Option<String>,
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LinkRef {
    pub ident: String,
    pub label: Option<String>,
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Heading {
    pub depth: u8,
    pub value: Box<Node>,
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Definition {
    pub position: Option<Position>,
    pub url: String,
    pub title: Option<String>,
    pub ident: String,
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Text {
    pub value: String,
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Html {
    pub value: String,
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Toml {
    pub value: String,
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Yaml {
    pub value: String,
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CodeInline {
    pub value: String,
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MathInline {
    pub value: String,
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Math {
    pub value: String,
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Blockquote(Value),
    Break { position: Option<Position> },
    Definition(Definition),
    Delete(Value),
    Heading(Heading),
    Emphasis(Value),
    Footnote(Footnote),
    FootnoteRef(FootnoteRef),
    Html(Html),
    Yaml(Yaml),
    Toml(Toml),
    Image(Image),
    ImageRef(ImageRef),
    CodeInline(CodeInline),
    MathInline(MathInline),
    Link(Link),
    LinkRef(LinkRef),
    Math(Math),
    List(List),
    TableHeader(TableHeader),
    TableCell(TableCell),
    Code(Code),
    Strong(Value),
    HorizontalRule { position: Option<Position> },
    MdxFlowExpression(mdast::MdxFlowExpression),
    MdxJsxFlowElement(mdast::MdxJsxFlowElement),
    MdxJsxTextElement(mdast::MdxJsxTextElement),
    MdxTextExpression(mdast::MdxTextExpression),
    MdxjsEsm(mdast::MdxjsEsm),
    Text(Text),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Position {
    pub start: Point,
    pub end: Point,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Point {
    pub line: usize,
    pub column: usize,
}

impl From<markdown::unist::Position> for Position {
    fn from(value: markdown::unist::Position) -> Self {
        Self {
            start: Point {
                line: value.start.line,
                column: value.start.column,
            },
            end: Point {
                line: value.end.line,
                column: value.end.column,
            },
        }
    }
}

impl From<String> for Node {
    fn from(value: String) -> Self {
        Self::Text(Text {
            value,
            position: None,
        })
    }
}

impl Display for Node {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let s = match self.clone() {
            Self::List(List {
                indent,
                checked,
                value,
                ..
            }) => {
                // TODO: set indent
                format!(
                    "{}- {}{}",
                    "  ".repeat(indent as usize),
                    checked
                        .map(|it| if it { "[x] " } else { "[] " })
                        .unwrap_or_else(|| ""),
                    value
                )
            }
            Self::TableCell(TableCell {
                last_cell_in_row,
                last_cell_of_in_table,
                value,
                ..
            }) => {
                if last_cell_in_row || last_cell_of_in_table {
                    format!("|{}|", value)
                } else {
                    format!("|{}", value)
                }
            }
            Self::TableHeader(TableHeader { align, .. }) => {
                format!(
                    "|{}|",
                    align
                        .iter()
                        .map(|a| match a {
                            mdast::AlignKind::Left => ":---",
                            mdast::AlignKind::Right => "---:",
                            mdast::AlignKind::Center => ":---:",
                            mdast::AlignKind::None => "---",
                        })
                        .join("|")
                )
            }
            Self::Blockquote(Value { value, .. }) => format!("> {}", value),
            Self::Code(Code { value, lang, .. }) => format!(
                "```{}\n{}\n```",
                lang.unwrap_or_else(|| "".to_string()),
                value
            ),
            Self::Definition(Definition { label, url, .. }) => {
                format!("[{}]: {}", label.unwrap_or_default(), url)
            }
            Self::Delete(Value { value, .. }) => format!("~~{}~~", value),
            Self::Emphasis(Value { value, .. }) => format!("*{}*", value),
            Self::Footnote(Footnote { label, ident, .. }) => {
                format!("[^{}]: {}", label.unwrap_or_default(), ident)
            }
            Self::FootnoteRef(FootnoteRef { label, .. }) => {
                format!("[^{}]", label.unwrap_or_default())
            }
            Self::Heading(Heading { depth, value, .. }) => {
                format!("{} {}", "#".repeat(depth as usize), value)
            }
            Self::Html(Html { value, .. }) => value,
            Self::Image(Image {
                alt, url, title, ..
            }) => format!(
                "![{}]({}{})",
                alt,
                url.replace(' ', "%20"),
                title.map(|it| format!(" \"{}\"", it)).unwrap_or_default()
            ),
            Self::ImageRef(ImageRef { alt, ident, .. }) => {
                format!("![{}][{}]", alt, ident)
            }
            Self::CodeInline(CodeInline { value, .. }) => {
                format!("`{}`", value)
            }
            Self::MathInline(MathInline { value, .. }) => {
                format!("${}$", value)
            }
            Self::Link(Link { url, title, .. }) => {
                format!(
                    "[{}]({})",
                    title.unwrap_or_default().replace(' ', "-"),
                    url.replace(' ', "-")
                )
            }
            Self::LinkRef(LinkRef { ident, label, .. }) => {
                format!("[{}][{}]", ident, label.unwrap_or_default())
            }
            Self::Math(Math { value, .. }) => format!("$$\n{}\n$$", value),
            Self::Text(Text { value, .. }) => value,
            Self::MdxFlowExpression(mdx_flow_expression) => {
                format!("{{{}}}", mdx_flow_expression.value)
            }
            Self::MdxJsxFlowElement(mdx_jsx_flow_element) => {
                let name = mdx_jsx_flow_element.name.unwrap_or_default();
                format!(
                    "<{} {}>{}</{}>",
                    name,
                    mdx_jsx_flow_element
                        .attributes
                        .into_iter()
                        .map(Self::attribute_content_to_string)
                        .join(""),
                    mdx_jsx_flow_element
                        .children
                        .into_iter()
                        .map(Self::mdast_value)
                        .join(""),
                    name
                )
            }
            Self::MdxJsxTextElement(mdx_jsx_text_element) => {
                let name = mdx_jsx_text_element.name.unwrap_or_default();
                format!(
                    "<{} {}>{}</{}>",
                    name,
                    mdx_jsx_text_element
                        .attributes
                        .into_iter()
                        .map(Self::attribute_content_to_string)
                        .join(""),
                    mdx_jsx_text_element
                        .children
                        .into_iter()
                        .map(Self::mdast_value)
                        .join(""),
                    name
                )
            }
            Self::MdxTextExpression(mdx_text_expression) => {
                format!("{{{}}}", mdx_text_expression.value)
            }
            Self::MdxjsEsm(mdxjs_esm) => mdxjs_esm.value,
            Self::Strong(Value { value, .. }) => {
                format!("**{}**", value)
            }
            Self::Yaml(Yaml { value, .. }) => value,
            Self::Toml(Toml { value, .. }) => value,
            Self::Break { .. } => "\\".to_string(),
            Self::HorizontalRule { .. } => "---".to_string(),
        };

        write!(f, "{}", s)
    }
}

impl Node {
    pub fn text_type(text: &str) -> Self {
        Self::Text(Text {
            value: text.to_string(),
            position: None,
        })
    }

    pub fn node_value(&self) -> Box<Node> {
        match self.clone() {
            Self::Blockquote(v) => v.value,
            Self::Delete(v) => v.value,
            Self::Heading(h) => h.value,
            Self::Emphasis(v) => v.value,
            Self::List(l) => l.value,
            Self::Strong(v) => v.value,
            _ => Box::new(self.clone()),
        }
    }

    pub fn value(&self) -> String {
        match self.clone() {
            Self::Blockquote(v) => v.value.value(),
            Self::Definition(d) => d.ident,
            Self::Delete(v) => v.value.value(),
            Self::Heading(h) => h.value.value(),
            Self::Emphasis(v) => v.value.value(),
            Self::Footnote(f) => f.ident,
            Self::FootnoteRef(f) => f.ident,
            Self::Html(v) => v.value,
            Self::Yaml(v) => v.value,
            Self::Toml(v) => v.value,
            Self::Image(i) => i.url,
            Self::ImageRef(i) => i.ident,
            Self::CodeInline(v) => v.value,
            Self::MathInline(v) => v.value,
            Self::Link(l) => l.url,
            Self::LinkRef(l) => l.ident,
            Self::Math(v) => v.value,
            Self::List(l) => l.value.value(),
            Self::TableCell(c) => c.value.value(),
            Self::Code(c) => c.value,
            Self::Strong(v) => v.value.value(),
            Self::Text(t) => t.value,
            Self::Break { .. } => String::new(),
            Self::TableHeader(_) => String::new(),
            Self::MdxFlowExpression(mdx) => mdx.value,
            Self::MdxJsxFlowElement(mdx) => {
                mdx.children.into_iter().map(Self::mdast_value).join("")
            }
            Self::MdxTextExpression(mdx) => mdx.value,
            Self::MdxJsxTextElement(mdx) => {
                mdx.children.into_iter().map(Self::mdast_value).join("")
            }
            Self::MdxjsEsm(mdx) => mdx.value,
            Self::HorizontalRule { .. } => String::new(),
        }
    }

    pub fn name(&self) -> CompactString {
        match self {
            Self::Blockquote(_) => "blockquote".into(),
            Self::Break { .. } => "break".into(),
            Self::Definition(_) => "definition".into(),
            Self::Delete(_) => "delete".into(),
            Self::Heading(Heading { depth, .. }) => match depth {
                1 => "h1".into(),
                2 => "h2".into(),
                3 => "h3".into(),
                4 => "h4".into(),
                5 => "h5".into(),
                _ => "h".into(),
            },
            Self::Emphasis(_) => "emphasis".into(),
            Self::Footnote(_) => "footnote".into(),
            Self::FootnoteRef(_) => "footnoteref".into(),
            Self::Html(_) => "html".into(),
            Self::Yaml(_) => "yaml".into(),
            Self::Toml(_) => "toml".into(),
            Self::Image(_) => "image".into(),
            Self::ImageRef(_) => "image_ref".into(),
            Self::CodeInline(_) => "code_inline".into(),
            Self::MathInline(_) => "math_inline".into(),
            Self::Link(_) => "link".into(),
            Self::LinkRef(_) => "link_ref".into(),
            Self::Math(_) => "math".into(),
            Self::List(_) => "list".into(),
            Self::TableHeader(_) => "table_header".into(),
            Self::TableCell(_) => "table_cell".into(),
            Self::Code(_) => "code".into(),
            Self::Strong(_) => "strong".into(),
            Self::HorizontalRule { .. } => "Horizontal_rule".into(),
            Self::MdxFlowExpression(_) => "mdx_flow_expression".into(),
            Self::MdxJsxFlowElement(_) => "mdx_jsx_flow_element".into(),
            Self::MdxJsxTextElement(_) => "mdx_jsx_text_element".into(),
            Self::MdxTextExpression(_) => "mdx_text_expression".into(),
            Self::MdxjsEsm(_) => "mdx_js_esm".into(),
            Self::Text(_) => "text".into(),
        }
    }

    pub fn position(&self) -> Option<Position> {
        match self {
            Self::Blockquote(v) => v.position.clone(),
            Self::Definition(d) => d.position.clone(),
            Self::Delete(v) => v.position.clone(),
            Self::Heading(h) => h.position.clone(),
            Self::Emphasis(v) => v.position.clone(),
            Self::Footnote(f) => f.position.clone(),
            Self::FootnoteRef(f) => f.position.clone(),
            Self::Html(v) => v.position.clone(),
            Self::Yaml(v) => v.position.clone(),
            Self::Toml(v) => v.position.clone(),
            Self::Image(i) => i.position.clone(),
            Self::ImageRef(i) => i.position.clone(),
            Self::CodeInline(v) => v.position.clone(),
            Self::MathInline(v) => v.position.clone(),
            Self::Link(l) => l.position.clone(),
            Self::LinkRef(l) => l.position.clone(),
            Self::Math(v) => v.position.clone(),
            Self::Code(c) => c.position.clone(),
            Self::TableCell(c) => c.position.clone(),
            Self::TableHeader(c) => c.position.clone(),
            Self::List(l) => l.position.clone(),
            Self::Strong(s) => s.position.clone(),
            Self::MdxFlowExpression(m) => m.clone().position.map(|p| p.into()),
            Self::MdxTextExpression(m) => m.clone().position.map(|p| p.into()),
            Self::MdxjsEsm(m) => m.clone().position.map(|p| p.into()),
            Self::MdxJsxFlowElement(m) => m.clone().position.map(|p| p.into()),
            Self::MdxJsxTextElement(m) => m.clone().position.map(|p| p.into()),
            Self::Break { position } => position.clone(),
            Self::Text(t) => t.position.clone(),
            Self::HorizontalRule { position } => position.clone(),
        }
    }

    pub fn is_inline_code(&self) -> bool {
        matches!(self, Self::CodeInline(_))
    }

    pub fn is_inline_math(&self) -> bool {
        matches!(self, Self::MathInline(_))
    }

    pub fn is_strong(&self) -> bool {
        matches!(self, Self::Strong(_))
    }

    pub fn is_emphasis(&self) -> bool {
        matches!(self, Self::Emphasis(_))
    }

    pub fn is_delete(&self) -> bool {
        matches!(self, Self::Delete(_))
    }

    pub fn is_link(&self) -> bool {
        matches!(self, Self::Link(_))
    }

    pub fn is_link_ref(&self) -> bool {
        matches!(self, Self::LinkRef(_))
    }

    pub fn is_image(&self) -> bool {
        matches!(self, Self::Image(_))
    }

    pub fn is_horizontal_rule(&self) -> bool {
        matches!(self, Self::HorizontalRule { .. })
    }

    pub fn is_blockquote(&self) -> bool {
        matches!(self, Self::Blockquote(_))
    }

    pub fn is_html(&self) -> bool {
        matches!(self, Self::Html { .. })
    }

    pub fn is_footnote(&self) -> bool {
        matches!(self, Self::Footnote(_))
    }

    pub fn is_mdx_jsx_flow_element(&self) -> bool {
        matches!(
            self,
            Self::MdxJsxFlowElement(mdast::MdxJsxFlowElement { .. })
        )
    }

    pub fn is_msx_js_esm(&self) -> bool {
        matches!(self, Self::MdxjsEsm(mdast::MdxjsEsm { .. }))
    }

    pub fn is_toml(&self) -> bool {
        matches!(self, Self::Toml { .. })
    }

    pub fn is_yaml(&self) -> bool {
        matches!(self, Self::Yaml { .. })
    }

    pub fn is_break(&self) -> bool {
        matches!(self, Self::Break { .. })
    }

    pub fn is_mdx_text_expression(&self) -> bool {
        matches!(
            self,
            Self::MdxTextExpression(mdast::MdxTextExpression { .. })
        )
    }

    pub fn is_footnote_ref(&self) -> bool {
        matches!(self, Self::FootnoteRef { .. })
    }

    pub fn is_image_ref(&self) -> bool {
        matches!(self, Self::ImageRef(_))
    }

    pub fn is_mdx_jsx_text_element(&self) -> bool {
        matches!(
            self,
            Self::MdxJsxTextElement(mdast::MdxJsxTextElement { .. })
        )
    }

    pub fn is_math(&self) -> bool {
        matches!(self, Self::Math(_))
    }

    pub fn is_mdx_flow_expression(&self) -> bool {
        matches!(
            self,
            Self::MdxFlowExpression(mdast::MdxFlowExpression { .. })
        )
    }

    pub fn is_definition(&self) -> bool {
        matches!(self, Self::Definition(_))
    }

    pub fn is_code(&self, lang: Option<CompactString>) -> bool {
        if lang.is_none() {
            true
        } else if let Self::Code(Code {
            lang: node_lang, ..
        }) = &self
        {
            node_lang.clone().unwrap_or_default() == lang.unwrap_or_default()
        } else {
            false
        }
    }

    pub fn is_heading(&self, depth: Option<u8>) -> bool {
        if let Self::Heading(Heading {
            depth: heading_depth,
            ..
        }) = &self
        {
            depth.is_none() || *heading_depth == depth.unwrap()
        } else {
            false
        }
    }

    pub fn with_value(&self, value: &str) -> Self {
        match self.clone() {
            Self::Blockquote(mut v) => {
                v.value = Box::new(v.value.with_value(value));
                Self::Blockquote(v)
            }
            Self::Delete(mut v) => {
                v.value = Box::new(v.value.with_value(value));
                Self::Delete(v)
            }
            Self::Emphasis(mut v) => {
                v.value = Box::new(v.value.with_value(value));
                Self::Emphasis(v)
            }
            Self::Html(mut html) => {
                html.value = value.to_string();
                Self::Html(html)
            }
            Self::Yaml(mut yaml) => {
                yaml.value = value.to_string();
                Self::Yaml(yaml)
            }
            Self::Toml(mut toml) => {
                toml.value = value.to_string();
                Self::Toml(toml)
            }
            Self::CodeInline(mut code) => {
                code.value = value.to_string();
                Self::CodeInline(code)
            }
            Self::MathInline(mut math) => {
                math.value = value.to_string();
                Self::MathInline(math)
            }
            Self::Math(mut math) => {
                math.value = value.to_string();
                Self::Math(math)
            }
            Self::List(mut list) => {
                list.value = Box::new(list.value.with_value(value));
                Self::List(list)
            }
            Self::TableCell(mut cell) => {
                cell.value = Box::new(cell.value.with_value(value));
                Self::TableCell(cell)
            }
            Self::Strong(mut strong) => {
                strong.value = Box::new(strong.value.with_value(value));
                Self::Strong(strong)
            }
            Self::Code(mut code) => {
                code.value = value.to_string();
                Self::Code(code)
            }
            Self::Image(mut image) => {
                image.url = value.to_string();
                Self::Image(image)
            }
            Self::ImageRef(mut image) => {
                image.ident = value.to_string();
                Self::ImageRef(image)
            }
            Self::Link(mut link) => {
                link.url = value.to_string();
                Self::Link(link)
            }
            Self::LinkRef(mut link) => {
                link.ident = value.to_string();
                Self::LinkRef(link)
            }
            Self::Footnote(mut footnote) => {
                footnote.ident = value.to_string();
                Self::Footnote(footnote)
            }
            Self::FootnoteRef(mut footnote) => {
                footnote.ident = value.to_string();
                Self::FootnoteRef(footnote)
            }
            Self::Heading(mut heading) => {
                heading.value = Box::new(heading.value.with_value(value));
                Self::Heading(heading)
            }
            Self::Definition(mut def) => {
                def.ident = value.to_string();
                Self::Definition(def)
            }
            node @ Self::Break { .. } => node,
            node @ Self::TableHeader(_) => node,
            node @ Self::HorizontalRule { .. } => node,
            Self::Text(mut text) => {
                text.value = value.to_string();
                Self::Text(text)
            }
            Self::MdxFlowExpression(mut mdx) => {
                mdx.value = value.to_string();
                Self::MdxFlowExpression(mdx)
            }
            Self::MdxTextExpression(mut mdx) => {
                mdx.value = value.to_string();
                Self::MdxTextExpression(mdx)
            }
            Self::MdxjsEsm(mut mdx) => {
                mdx.value = value.to_string();
                Self::MdxjsEsm(mdx)
            }
            Self::MdxJsxFlowElement(mdx) => Self::MdxJsxFlowElement(mdast::MdxJsxFlowElement {
                name: mdx.name,
                attributes: mdx.attributes,
                children: mdx
                    .children
                    .into_iter()
                    .map(|n| Self::set_mdast_value(n, value))
                    .collect_vec(),
                ..mdx
            }),
            Self::MdxJsxTextElement(mdx) => Self::MdxJsxTextElement(mdast::MdxJsxTextElement {
                name: mdx.name,
                attributes: mdx.attributes,
                children: mdx
                    .children
                    .into_iter()
                    .map(|n| Self::set_mdast_value(n, value))
                    .collect_vec(),
                ..mdx
            }),
        }
    }

    pub(crate) fn from_mdast_node(node: mdast::Node) -> Vec<Node> {
        match node.clone() {
            mdast::Node::Root(root) => root
                .children
                .into_iter()
                .flat_map(Self::from_mdast_node)
                .collect_vec(),
            mdast::Node::ListItem(list_item) => list_item
                .children
                .into_iter()
                .flat_map(Self::from_mdast_node)
                .collect_vec(),
            mdast::Node::List(list) => Self::mdast_list_items(&list, 0),
            mdast::Node::Table(table) => table
                .children
                .iter()
                .enumerate()
                .flat_map(|(row, n)| {
                    if let mdast::Node::TableRow(table_row) = n {
                        itertools::concat(vec![
                            table_row
                                .children
                                .iter()
                                .enumerate()
                                .flat_map(|(column, node)| {
                                    if let mdast::Node::TableCell(_) = node {
                                        vec![Self::TableCell(TableCell {
                                            row,
                                            column,
                                            last_cell_in_row: table_row.children.len() - 1
                                                == column,
                                            last_cell_of_in_table: table_row.children.len() - 1
                                                == column
                                                && table.children.len() - 1 == row,
                                            value: Self::mdast_children_to_node(node.clone()),
                                            position: node.position().map(|p| p.clone().into()),
                                        })]
                                    } else {
                                        Vec::new()
                                    }
                                })
                                .collect(),
                            if row == 0 {
                                vec![Self::TableHeader(TableHeader {
                                    align: table.align.clone(),
                                    position: n.position().map(|p| p.clone().into()),
                                })]
                            } else {
                                Vec::new()
                            },
                        ])
                    } else {
                        Vec::new()
                    }
                })
                .collect(),
            mdast::Node::Code(mdast::Code {
                value,
                position,
                lang,
                ..
            }) => {
                vec![Self::Code(Code {
                    value,
                    lang,
                    position: position.map(|p| p.clone().into()),
                })]
            }
            mdast::Node::Blockquote(mdast::Blockquote { position, .. }) => {
                vec![Self::Blockquote(Value {
                    value: Self::mdast_children_to_node(node),
                    position: position.map(|p| p.clone().into()),
                })]
            }
            mdast::Node::Definition(mdast::Definition {
                url,
                title,
                identifier,
                label,
                position,
                ..
            }) => {
                vec![Self::Definition(Definition {
                    ident: identifier,
                    url,
                    label,
                    title,
                    position: position.map(|p| p.clone().into()),
                })]
            }
            mdast::Node::Heading(mdast::Heading {
                depth, position, ..
            }) => {
                vec![Self::Heading(Heading {
                    value: Self::mdast_children_to_node(node),
                    depth,
                    position: position.map(|p| p.clone().into()),
                })]
            }
            mdast::Node::Break(mdast::Break { position }) => {
                vec![Self::Break {
                    position: position.map(|p| p.clone().into()),
                }]
            }
            mdast::Node::Delete(mdast::Delete { position, .. }) => {
                vec![Self::Delete(Value {
                    value: Self::mdast_children_to_node(node),
                    position: position.map(|p| p.clone().into()),
                })]
            }
            mdast::Node::Emphasis(mdast::Emphasis { position, .. }) => {
                vec![Self::Emphasis(Value {
                    value: Self::mdast_children_to_node(node),
                    position: position.map(|p| p.clone().into()),
                })]
            }
            mdast::Node::Strong(mdast::Strong { position, .. }) => {
                vec![Self::Strong(Value {
                    value: Self::mdast_children_to_node(node),
                    position: position.map(|p| p.clone().into()),
                })]
            }
            mdast::Node::ThematicBreak(mdast::ThematicBreak { position, .. }) => {
                vec![Self::HorizontalRule {
                    position: position.map(|p| p.clone().into()),
                }]
            }
            mdast::Node::Html(mdast::Html { value, position }) => {
                vec![Self::Html(Html {
                    value,
                    position: position.map(|p| p.clone().into()),
                })]
            }
            mdast::Node::Yaml(mdast::Yaml { value, position }) => {
                vec![Self::Yaml(Yaml {
                    value,
                    position: position.map(|p| p.clone().into()),
                })]
            }
            mdast::Node::Toml(mdast::Toml { value, position }) => {
                vec![Self::Toml(Toml {
                    value,
                    position: position.map(|p| p.clone().into()),
                })]
            }
            mdast::Node::Image(mdast::Image {
                alt,
                url,
                title,
                position,
            }) => {
                vec![Self::Image(Image {
                    alt,
                    url,
                    title,
                    position: position.map(|p| p.clone().into()),
                })]
            }
            mdast::Node::ImageReference(mdast::ImageReference {
                alt,
                identifier,
                label,
                position,
                ..
            }) => {
                vec![Self::ImageRef(ImageRef {
                    alt,
                    ident: identifier,
                    label,
                    position: position.map(|p| p.clone().into()),
                })]
            }
            mdast::Node::InlineCode(mdast::InlineCode {
                value, position, ..
            }) => {
                vec![Self::CodeInline(CodeInline {
                    value,
                    position: position.map(|p| p.clone().into()),
                })]
            }
            mdast::Node::InlineMath(mdast::InlineMath { value, position }) => {
                vec![Self::MathInline(MathInline {
                    value,
                    position: position.map(|p| p.clone().into()),
                })]
            }
            mdast::Node::Link(mdast::Link { url, position, .. }) => {
                let title = Self::mdast_children_to_node(node).to_string();
                vec![Self::Link(Link {
                    url,
                    title: if title.is_empty() {
                        None
                    } else {
                        Some(title.to_owned())
                    },
                    position: position.map(|p| p.clone().into()),
                })]
            }
            mdast::Node::LinkReference(mdast::LinkReference {
                identifier,
                label,
                position,
                ..
            }) => {
                vec![Self::LinkRef(LinkRef {
                    ident: identifier,
                    label,
                    position: position.map(|p| p.clone().into()),
                })]
            }
            mdast::Node::Math(mdast::Math {
                value, position, ..
            }) => {
                vec![Self::Math(Math {
                    value,
                    position: position.map(|p| p.clone().into()),
                })]
            }
            mdast::Node::FootnoteDefinition(mdast::FootnoteDefinition {
                identifier,
                label,
                position,
                ..
            }) => {
                vec![Self::Footnote(Footnote {
                    ident: identifier,
                    label,
                    position: position.map(|p| p.clone().into()),
                })]
            }
            mdast::Node::FootnoteReference(mdast::FootnoteReference {
                identifier,
                label,
                position,
                ..
            }) => {
                vec![Self::FootnoteRef(FootnoteRef {
                    ident: identifier,
                    label,
                    position: position.map(|p| p.clone().into()),
                })]
            }
            mdast::Node::MdxFlowExpression(mdx) => {
                vec![Self::MdxFlowExpression(mdx)]
            }
            mdast::Node::MdxJsxFlowElement(mdx) => {
                vec![Self::MdxJsxFlowElement(mdx)]
            }
            mdast::Node::MdxJsxTextElement(mdx) => {
                vec![Self::MdxJsxTextElement(mdx)]
            }
            mdast::Node::MdxTextExpression(mdx) => {
                vec![Self::MdxTextExpression(mdx)]
            }
            mdast::Node::MdxjsEsm(mdx) => {
                vec![Self::MdxjsEsm(mdx)]
            }
            mdast::Node::Text(mdast::Text { position, .. }) => {
                vec![Self::Text(Text {
                    value: Self::mdast_value(node),
                    position: position.map(|p| p.clone().into()),
                })]
            }
            mdast::Node::Paragraph(mdast::Paragraph { position, .. }) => {
                vec![Self::Text(Text {
                    value: Self::mdast_value(node),
                    position: position.map(|p| p.clone().into()),
                })]
            }
            _ => Vec::new(),
        }
    }

    fn mdast_children_to_node(node: mdast::Node) -> Box<Node> {
        node.children()
            .and_then(|children| {
                children.first().map(|v| {
                    Self::from_mdast_node(v.clone())
                        .first()
                        .map(|v| Box::new(v.clone()))
                        .unwrap_or_else(|| Box::new(EMPTY_NODE))
                })
            })
            .unwrap_or_else(|| Box::new(EMPTY_NODE))
    }

    fn mdast_list_items(list: &mdast::List, indent: Indent) -> Vec<Node> {
        list.children
            .iter()
            .flat_map(|n| {
                if let mdast::Node::ListItem(list) = n {
                    itertools::concat(vec![
                        vec![Self::List(List {
                            indent,
                            index: 0,
                            checked: list.checked,
                            value: Self::from_mdast_node(n.clone())
                                .first()
                                .map(|v| Box::new(v.clone()))
                                .unwrap_or_else(|| {
                                    Box::new(Node::Text(Text {
                                        value: String::new(),
                                        position: None,
                                    }))
                                }),
                            position: n.position().map(|p| p.clone().into()),
                        })],
                        list.children
                            .iter()
                            .flat_map(|node| {
                                if let mdast::Node::List(sub_list) = node {
                                    Self::mdast_list_items(sub_list, indent + 1)
                                } else if let mdast::Node::ListItem(list) = node {
                                    vec![Self::List(List {
                                        indent: indent + 1,
                                        index: 0,
                                        checked: list.checked,
                                        value: Self::from_mdast_node(n.clone())
                                            .first()
                                            .map(|v| Box::new(v.clone()))
                                            .unwrap_or_else(|| {
                                                Box::new(Node::Text(Text {
                                                    value: String::new(),
                                                    position: None,
                                                }))
                                            }),
                                        position: node.position().map(|p| p.clone().into()),
                                    })]
                                } else {
                                    Vec::new()
                                }
                            })
                            .collect(),
                    ])
                } else if let mdast::Node::List(sub_list) = n {
                    Self::mdast_list_items(sub_list, indent + 1)
                } else {
                    Vec::new()
                }
            })
            .enumerate()
            .filter_map(|(i, node)| match node {
                Self::List(List {
                    indent,
                    index: _,
                    checked,
                    value,
                    position,
                }) => Some(Self::List(List {
                    indent,
                    index: i,
                    checked,
                    value,
                    position,
                })),
                _ => None,
            })
            .collect()
    }

    fn set_mdast_value(node: mdast::Node, value: &str) -> mdast::Node {
        match node {
            mdast::Node::Root(root) => mdast::Node::Root(mdast::Root {
                children: root
                    .children
                    .into_iter()
                    .map(|children| Self::set_mdast_value(children, value))
                    .collect_vec(),
                ..root
            }),
            mdast::Node::ListItem(list_item) => mdast::Node::ListItem(mdast::ListItem {
                children: list_item
                    .children
                    .into_iter()
                    .map(|children| Self::set_mdast_value(children, value))
                    .collect_vec(),
                ..list_item
            }),
            mdast::Node::TableCell(table_cell) => mdast::Node::TableCell(mdast::TableCell {
                children: table_cell
                    .children
                    .into_iter()
                    .map(|children| Self::set_mdast_value(children, value))
                    .collect_vec(),
                ..table_cell
            }),
            mdast::Node::Blockquote(blockquote) => mdast::Node::Blockquote(mdast::Blockquote {
                children: blockquote
                    .children
                    .into_iter()
                    .map(|children| Self::set_mdast_value(children, value))
                    .collect_vec(),
                ..blockquote
            }),
            mdast::Node::Code(code) => mdast::Node::Code(mdast::Code {
                value: value.to_string(),
                ..code.clone()
            }),
            mdast::Node::Definition(definition) => mdast::Node::Definition(mdast::Definition {
                url: value.to_string(),
                ..definition.clone()
            }),
            mdast::Node::Delete(delete) => mdast::Node::Delete(mdast::Delete {
                children: delete
                    .children
                    .into_iter()
                    .map(|children| Self::set_mdast_value(children, value))
                    .collect_vec(),
                ..delete
            }),
            mdast::Node::Emphasis(emphasis) => mdast::Node::Emphasis(mdast::Emphasis {
                children: emphasis
                    .children
                    .into_iter()
                    .map(|children| Self::set_mdast_value(children, value))
                    .collect_vec(),
                ..emphasis
            }),
            mdast::Node::FootnoteDefinition(footnote_definition) => {
                mdast::Node::FootnoteDefinition(mdast::FootnoteDefinition {
                    children: footnote_definition
                        .children
                        .into_iter()
                        .map(|children| Self::set_mdast_value(children, value))
                        .collect_vec(),
                    ..footnote_definition
                })
            }
            mdast::Node::FootnoteReference(footnote_reference) => {
                mdast::Node::FootnoteReference(mdast::FootnoteReference {
                    identifier: footnote_reference.identifier,
                    ..footnote_reference
                })
            }
            mdast::Node::Heading(heading) => mdast::Node::Heading(mdast::Heading {
                children: heading
                    .children
                    .into_iter()
                    .map(|children| Self::set_mdast_value(children, value))
                    .collect_vec(),
                ..heading
            }),
            mdast::Node::Html(html) => mdast::Node::Html(mdast::Html {
                value: value.to_string(),
                ..html
            }),
            mdast::Node::Image(image) => mdast::Node::Image(mdast::Image {
                title: Some(value.to_string()),
                ..image
            }),
            mdast::Node::ImageReference(image_reference) => {
                mdast::Node::ImageReference(mdast::ImageReference {
                    label: Some(value.to_string()),
                    ..image_reference
                })
            }
            mdast::Node::InlineCode(inline_code) => mdast::Node::InlineCode(mdast::InlineCode {
                value: value.to_string(),
                ..inline_code
            }),
            mdast::Node::InlineMath(inline_math) => mdast::Node::InlineMath(mdast::InlineMath {
                value: value.to_string(),
                ..inline_math
            }),
            mdast::Node::Link(link) => mdast::Node::Link(mdast::Link {
                url: value.to_string(),
                ..link
            }),
            mdast::Node::LinkReference(link_reference) => {
                mdast::Node::LinkReference(mdast::LinkReference {
                    label: Some(value.to_string()),
                    ..link_reference
                })
            }
            mdast::Node::Math(math) => mdast::Node::Math(mdast::Math {
                value: value.to_string(),
                ..math
            }),
            mdast::Node::Paragraph(paragraph) => mdast::Node::Paragraph(mdast::Paragraph {
                children: paragraph
                    .children
                    .into_iter()
                    .map(|children| Self::set_mdast_value(children, value))
                    .collect_vec(),
                ..paragraph
            }),
            mdast::Node::Text(text) => mdast::Node::Text(mdast::Text {
                value: value.to_string(),
                ..text
            }),
            mdast::Node::MdxFlowExpression(mdx_flow_expression) => {
                mdast::Node::MdxFlowExpression(mdast::MdxFlowExpression {
                    value: value.to_string(),
                    ..mdx_flow_expression
                })
            }
            mdast::Node::MdxJsxFlowElement(mdx_jsx_flow_element) => {
                mdast::Node::MdxJsxFlowElement(mdast::MdxJsxFlowElement {
                    children: mdx_jsx_flow_element
                        .children
                        .into_iter()
                        .map(|children| Self::set_mdast_value(children, value))
                        .collect_vec(),
                    ..mdx_jsx_flow_element
                })
            }
            mdast::Node::MdxJsxTextElement(mdx_jsx_text_element) => {
                mdast::Node::MdxJsxTextElement(mdast::MdxJsxTextElement {
                    children: mdx_jsx_text_element
                        .children
                        .into_iter()
                        .map(|children| Self::set_mdast_value(children, value))
                        .collect_vec(),
                    ..mdx_jsx_text_element
                })
            }
            mdast::Node::MdxTextExpression(mdx_text_expression) => {
                mdast::Node::MdxTextExpression(mdast::MdxTextExpression {
                    value: value.to_string(),
                    ..mdx_text_expression
                })
            }
            mdast::Node::MdxjsEsm(mdxjs_esm) => mdast::Node::MdxjsEsm(mdast::MdxjsEsm {
                value: value.to_string(),
                ..mdxjs_esm
            }),
            mdast::Node::Strong(strong) => mdast::Node::Strong(mdast::Strong {
                children: strong
                    .children
                    .into_iter()
                    .map(|children| Self::set_mdast_value(children, value))
                    .collect_vec(),
                ..strong
            }),
            mdast::Node::Yaml(yaml) => mdast::Node::Yaml(mdast::Yaml {
                value: value.to_string(),
                ..yaml
            }),
            mdast::Node::Toml(toml) => mdast::Node::Toml(mdast::Toml {
                value: value.to_string(),
                ..toml
            }),
            mdast::Node::List(_)
            | mdast::Node::Break(_)
            | mdast::Node::TableRow(_)
            | mdast::Node::ThematicBreak(_)
            | mdast::Node::Table(_) => node.clone(),
        }
    }

    fn attribute_content_to_string(attr: mdast::AttributeContent) -> String {
        match attr {
            mdast::AttributeContent::Expression(expression) => {
                format!("{{...{}}}", expression.value)
            }
            mdast::AttributeContent::Property(property) => match property.value {
                Some(value) => match value {
                    mdast::AttributeValue::Expression(expression) => {
                        format!("{}={{{}}}", property.name, expression.value)
                    }
                    mdast::AttributeValue::Literal(literal) => {
                        format!("{}=\"{}\"", property.name, literal)
                    }
                },
                None => property.name,
            },
        }
    }

    fn mdast_value(mdast_node: mdast::Node) -> String {
        match mdast_node.clone() {
            mdast::Node::Root(root) => root.children.into_iter().map(Self::mdast_value).join(""),
            mdast::Node::List(_) => "".to_string(),
            mdast::Node::ListItem(list_item) => list_item
                .children
                .into_iter()
                .map(Self::mdast_value)
                .join(""),
            mdast::Node::TableCell(table_cell) => table_cell
                .children
                .into_iter()
                .map(Self::mdast_value)
                .join(""),
            mdast::Node::Blockquote(blockquote) => blockquote
                .children
                .into_iter()
                .map(Self::mdast_value)
                .join(""),
            mdast::Node::Code(code) => code.value,
            mdast::Node::Definition(definition) => definition.url,
            mdast::Node::Delete(delete) => {
                delete.children.into_iter().map(Self::mdast_value).join("")
            }
            mdast::Node::Emphasis(emphasis) => emphasis
                .children
                .into_iter()
                .map(Self::mdast_value)
                .join(""),
            mdast::Node::FootnoteDefinition(footnote_definition) => footnote_definition
                .children
                .into_iter()
                .map(Self::mdast_value)
                .join(""),
            mdast::Node::FootnoteReference(footnote_ref) => footnote_ref.label.unwrap_or_default(),
            mdast::Node::Heading(heading) => {
                heading.children.into_iter().map(Self::mdast_value).join("")
            }
            mdast::Node::Html(html) => html.value,
            mdast::Node::Image(image) => image.url,
            mdast::Node::ImageReference(image_ref) => image_ref.identifier,
            mdast::Node::InlineCode(inline_code) => inline_code.value,
            mdast::Node::InlineMath(inline_math) => inline_math.value,
            mdast::Node::Link(link) => link.url,
            mdast::Node::LinkReference(link_ref) => link_ref
                .children
                .into_iter()
                .map(Self::mdast_value)
                .join(""),
            mdast::Node::Math(math) => math.value,
            mdast::Node::Paragraph(paragraph) => paragraph
                .children
                .into_iter()
                .map(Self::mdast_value)
                .join(""),
            mdast::Node::Text(text) => text.value,
            mdast::Node::MdxFlowExpression(mdx_flow_expression) => mdx_flow_expression.value,
            mdast::Node::MdxJsxFlowElement(mdx_jsx_flow_element) => mdx_jsx_flow_element
                .children
                .into_iter()
                .map(Self::mdast_value)
                .join(""),
            mdast::Node::MdxJsxTextElement(mdx_jsx_text_element) => mdx_jsx_text_element
                .children
                .into_iter()
                .map(Self::mdast_value)
                .join(""),
            mdast::Node::MdxTextExpression(mdx_text_expression) => mdx_text_expression.value,
            mdast::Node::MdxjsEsm(mdxjs_esm) => mdxjs_esm.value,
            mdast::Node::Strong(strong) => {
                strong.children.into_iter().map(Self::mdast_value).join("")
            }
            mdast::Node::Yaml(yaml) => yaml.value,
            mdast::Node::Toml(toml) => toml.value,
            mdast::Node::Break(_) => "\\".to_string(),
            mdast::Node::ThematicBreak(_) => "---".to_string(),
            mdast::Node::TableRow(_) => "".to_string(),
            mdast::Node::Table(_) => "".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case::text(Node::Text(Text{value: "".to_string(), position: None}),
           "test".to_string(),
           Node::Text(Text{value: "test".to_string(), position: None }))]
    #[case::blockquote(Node::Blockquote(Value {value: Box::new("test".to_string().into()), position: None }),
           "test".to_string(),
           Node::Blockquote(Value{value: Box::new("test".to_string().into()), position: None }))]
    #[case::delete(Node::Delete(Value {value: Box::new("test".to_string().into()), position: None }),
           "test".to_string(),
           Node::Delete(Value{value: Box::new("test".to_string().into()), position: None }))]
    #[case::emphasis(Node::Emphasis(Value {value: Box::new("test".to_string().into()), position: None }),
           "test".to_string(),
           Node::Emphasis(Value{value: Box::new("test".to_string().into()), position: None }))]
    #[case::strong(Node::Strong(Value {value: Box::new("test".to_string().into()), position: None }),
           "test".to_string(),
           Node::Strong(Value{value: Box::new("test".to_string().into()), position: None }))]
    #[case::heading(Node::Heading(Heading {depth: 1, value: Box::new("test".to_string().into()), position: None }),
           "test".to_string(),
           Node::Heading(Heading{depth: 1, value: Box::new("test".to_string().into()), position: None }))]
    #[case::link(Node::Link(Link {url: "test".to_string(), title: None, position: None }),
           "test".to_string(),
           Node::Link(Link{url: "test".to_string(), title: None, position: None }))]
    #[case::image(Node::Image(Image {alt: "test".to_string(), url: "test".to_string(), title: None, position: None }),
           "test".to_string(),
           Node::Image(Image{alt: "test".to_string(), url: "test".to_string(), title: None, position: None }))]
    #[case::code(Node::Code(Code {value: "test".to_string(), lang: None, position: None }),
           "test".to_string(),
           Node::Code(Code{value: "test".to_string(), lang: None, position: None }))]
    #[case::footnoteref(Node::FootnoteRef(FootnoteRef {ident: "test".to_string(), label: None, position: None }),
           "test".to_string(),
           Node::FootnoteRef(FootnoteRef{ident: "test".to_string(), label: None, position: None }))]
    #[case::footnote(Node::Footnote(Footnote {ident: "test".to_string(), label: None, position: None }),
           "test".to_string(),
           Node::Footnote(Footnote{ident: "test".to_string(), label: None, position: None }))]
    #[case::list(Node::List(List{index: 0, indent: 0, checked: None, value: Box::new("test".to_string().into()), position: None }),
           "test".to_string(),
           Node::List(List{index: 0, indent: 0, checked: None, value: Box::new("test".to_string().into()), position: None }))]
    #[case::list(Node::List(List{index: 1, indent: 1, checked: Some(true), value: Box::new("test".to_string().into()), position: None }),
           "test".to_string(),
           Node::List(List{index: 1, indent: 1, checked: Some(true), value: Box::new("test".to_string().into()), position: None }))]
    #[case::list(Node::List(List{index: 2, indent: 2, checked: Some(false), value: Box::new("test".to_string().into()), position: None }),
           "test".to_string(),
           Node::List(List{index: 2, indent: 2, checked: Some(false), value: Box::new("test".to_string().into()), position: None }))]
    fn test_with_value(#[case] node: Node, #[case] input: String, #[case] expected: Node) {
        assert_eq!(node.with_value(input.as_str()), expected);
    }

    #[rstest]
    #[case(Node::Text(Text{value: "test".to_string(), position: None }),
           "test".to_string())]
    #[case(Node::List(List{index: 0, indent: 2, checked: None, value: Box::new("test".to_string().into()), position: None}),
           "    - test".to_string())]
    fn test_display(#[case] node: Node, #[case] expected: String) {
        assert_eq!(node.to_string(), expected);
    }
}
