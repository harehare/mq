use std::fmt::{self, Display};

use compact_str::CompactString;
use itertools::Itertools;
use markdown::mdast::{self};

type Level = u8;

pub const EMPTY_NODE: Node = Node::Text(Text {
    value: String::new(),
    position: None,
});

#[derive(Debug, Clone, Default, PartialEq)]
pub struct RenderOptions {
    pub list_style: ListStyle,
    pub link_url_style: UrlSurroundStyle,
    pub link_title_style: TitleSurroundStyle,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub enum ListStyle {
    #[default]
    Dash,
    Plus,
    Star,
}

impl Display for ListStyle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ListStyle::Dash => write!(f, "-"),
            ListStyle::Plus => write!(f, "+"),
            ListStyle::Star => write!(f, "*"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Url(String);

#[derive(Debug, Clone, PartialEq, Default)]
pub enum UrlSurroundStyle {
    #[default]
    None,
    Angle,
}

impl Url {
    pub fn new(value: String) -> Self {
        Self(value)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn to_string_with(&self, options: &RenderOptions) -> String {
        match options.link_url_style {
            UrlSurroundStyle::None if self.0.is_empty() => "<>".to_string(),
            UrlSurroundStyle::None => self.0.clone(),
            UrlSurroundStyle::Angle => format!("<{}>", self.0),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum TitleSurroundStyle {
    #[default]
    Double,
    Single,
    Paren,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Title(String);

impl Display for Title {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Title {
    pub fn new(value: String) -> Self {
        Self(value)
    }

    pub fn to_value(&self) -> String {
        self.0.clone()
    }

    pub fn to_string_with(&self, options: &RenderOptions) -> String {
        match options.link_title_style {
            TitleSurroundStyle::Double => format!("\"{}\"", self),
            TitleSurroundStyle::Single => format!("'{}'", self),
            TitleSurroundStyle::Paren => format!("({})", self),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TableAlignKind {
    Left,
    Right,
    Center,
    None,
}

impl From<mdast::AlignKind> for TableAlignKind {
    fn from(value: mdast::AlignKind) -> Self {
        match value {
            mdast::AlignKind::Left => Self::Left,
            mdast::AlignKind::Right => Self::Right,
            mdast::AlignKind::Center => Self::Center,
            mdast::AlignKind::None => Self::None,
        }
    }
}

impl Display for TableAlignKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TableAlignKind::Left => write!(f, ":---"),
            TableAlignKind::Right => write!(f, "---:"),
            TableAlignKind::Center => write!(f, ":---:"),
            TableAlignKind::None => write!(f, "---"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct List {
    pub values: Vec<Node>,
    pub index: usize,
    pub level: Level,
    pub checked: Option<bool>,
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TableCell {
    pub values: Vec<Node>,
    pub column: usize,
    pub row: usize,
    pub last_cell_in_row: bool,
    pub last_cell_of_in_table: bool,
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TableRow {
    pub values: Vec<Node>,
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TableHeader {
    pub align: Vec<TableAlignKind>,
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Value {
    pub values: Vec<Node>,
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Fragment {
    pub values: Vec<Node>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Code {
    pub value: String,
    pub lang: Option<String>,
    pub position: Option<Position>,
    pub meta: Option<String>,
    pub fence: bool,
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
    pub url: Url,
    pub title: Option<Title>,
    pub values: Vec<Node>,
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
    pub values: Vec<Node>,
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LinkRef {
    pub ident: String,
    pub label: Option<String>,
    pub values: Vec<Node>,
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Heading {
    pub depth: u8,
    pub values: Vec<Node>,
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Definition {
    pub position: Option<Position>,
    pub url: Url,
    pub title: Option<Title>,
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
    pub value: CompactString,
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MathInline {
    pub value: CompactString,
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Math {
    pub value: String,
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MdxFlowExpression {
    pub value: CompactString,
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MdxJsxFlowElement {
    pub children: Vec<Node>,
    pub position: Option<Position>,
    pub name: Option<String>,
    pub attributes: Vec<MdxAttributeContent>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MdxAttributeContent {
    Expression(CompactString),
    Property(MdxJsxAttribute),
}

#[derive(Debug, Clone, PartialEq)]
pub struct MdxJsxAttribute {
    pub name: CompactString,
    pub value: Option<MdxAttributeValue>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MdxAttributeValue {
    Expression(CompactString),
    Literal(CompactString),
}

#[derive(Debug, Clone, PartialEq)]
pub struct MdxJsxTextElement {
    pub children: Vec<Node>,
    pub position: Option<Position>,
    pub name: Option<CompactString>,
    pub attributes: Vec<MdxAttributeContent>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MdxTextExpression {
    pub value: CompactString,
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MdxJsEsm {
    pub value: CompactString,
    pub position: Option<Position>,
}

#[derive(Debug, Clone)]
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
    TableRow(TableRow),
    TableCell(TableCell),
    Code(Code),
    Strong(Value),
    HorizontalRule { position: Option<Position> },
    MdxFlowExpression(MdxFlowExpression),
    MdxJsxFlowElement(MdxJsxFlowElement),
    MdxJsxTextElement(MdxJsxTextElement),
    MdxTextExpression(MdxTextExpression),
    MdxJsEsm(MdxJsEsm),
    Text(Text),
    Fragment(Fragment),
    Empty,
}

impl PartialEq for Node {
    fn eq(&self, other: &Self) -> bool {
        self.to_string() == other.to_string()
    }
}

impl PartialOrd for Node {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        let (self_node, other_node) = (self, other);
        let self_pos = self_node.position();
        let other_pos = other_node.position();

        match (self_pos, other_pos) {
            (Some(self_pos), Some(other_pos)) => {
                match self_pos.start.line.cmp(&other_pos.start.line) {
                    std::cmp::Ordering::Equal => {
                        self_pos.start.column.partial_cmp(&other_pos.start.column)
                    }
                    ordering => Some(ordering),
                }
            }
            (Some(_), None) => Some(std::cmp::Ordering::Less),
            (None, Some(_)) => Some(std::cmp::Ordering::Greater),
            (None, None) => Some(self.name().cmp(&other.name())),
        }
    }
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
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_string_with(&RenderOptions::default()))
    }
}

impl Node {
    pub fn map_values<E, F>(&self, f: &mut F) -> Result<Node, E>
    where
        E: std::error::Error,
        F: FnMut(&Node) -> Result<Node, E>,
    {
        Self::_map_values(self.clone(), f)
    }

    fn _map_values<E, F>(node: Node, f: &mut F) -> Result<Node, E>
    where
        E: std::error::Error,
        F: FnMut(&Node) -> Result<Node, E>,
    {
        match f(&node)? {
            Node::Fragment(mut v) => {
                let values = v
                    .values
                    .into_iter()
                    .map(|node| Self::_map_values(node, f))
                    .collect::<Result<Vec<_>, _>>();
                match values {
                    Ok(values) => {
                        v.values = values;
                        Ok(Node::Fragment(v))
                    }
                    Err(e) => Err(e),
                }
            }
            node => Ok(node),
        }
    }

    pub fn to_fragment(&self) -> Node {
        match self.clone() {
            Node::List(List { values, .. })
            | Node::TableCell(TableCell { values, .. })
            | Node::TableRow(TableRow { values, .. })
            | Node::Link(Link { values, .. })
            | Node::Footnote(Footnote { values, .. })
            | Node::LinkRef(LinkRef { values, .. })
            | Node::Heading(Heading { values, .. })
            | Node::Blockquote(Value { values, .. })
            | Node::Delete(Value { values, .. })
            | Node::Emphasis(Value { values, .. })
            | Node::Strong(Value { values, .. }) => Node::Fragment(Fragment { values }),
            node @ Node::Fragment(_) => node,
            _ => Self::Empty,
        }
    }

    pub fn apply_fragment(&mut self, fragment: Node) {
        Self::_apply_fragment(self, fragment)
    }

    fn _apply_fragment(node: &mut Node, fragment: Node) {
        match node {
            Node::List(List { values, .. })
            | Node::TableCell(TableCell { values, .. })
            | Node::TableRow(TableRow { values, .. })
            | Node::Link(Link { values, .. })
            | Node::Footnote(Footnote { values, .. })
            | Node::LinkRef(LinkRef { values, .. })
            | Node::Heading(Heading { values, .. })
            | Node::Blockquote(Value { values, .. })
            | Node::Delete(Value { values, .. })
            | Node::Emphasis(Value { values, .. })
            | Node::Strong(Value { values, .. }) => {
                if let Node::Fragment(Fragment { values: new_values }) = fragment {
                    let new_values = values
                        .iter()
                        .zip(new_values)
                        .map(|(current_value, new_value)| {
                            if new_value.is_empty() {
                                current_value.clone()
                            } else if new_value.is_fragment() {
                                let mut current_value = current_value.clone();
                                Self::_apply_fragment(&mut current_value, new_value);
                                current_value
                            } else {
                                new_value
                            }
                        })
                        .collect::<Vec<_>>();
                    *values = new_values;
                }
            }
            _ => {}
        }
    }

    pub fn to_string_with(&self, options: &RenderOptions) -> String {
        match self.clone() {
            Self::List(List {
                level,
                checked,
                values,
                ..
            }) => {
                format!(
                    "{}{} {}{}",
                    "  ".repeat(level as usize),
                    options.list_style,
                    checked
                        .map(|it| if it { "[x] " } else { "[] " })
                        .unwrap_or_else(|| ""),
                    Self::values_to_string(values, options)
                )
            }
            Self::TableRow(TableRow { values, .. }) => values
                .iter()
                .map(|cell| cell.to_string_with(options))
                .join(""),
            Self::TableCell(TableCell {
                last_cell_in_row,
                last_cell_of_in_table,
                values,
                ..
            }) => {
                if last_cell_in_row || last_cell_of_in_table {
                    format!("|{}|", Self::values_to_string(values, options))
                } else {
                    format!("|{}", Self::values_to_string(values, options))
                }
            }
            Self::TableHeader(TableHeader { align, .. }) => {
                format!("|{}|", align.iter().map(|a| a.to_string()).join("|"))
            }
            Self::Blockquote(Value { values, .. }) => Self::values_to_string(values, options)
                .split('\n')
                .map(|line| format!("> {}", line))
                .join("\n"),
            Self::Code(Code {
                value,
                lang,
                fence,
                meta,
                ..
            }) => {
                let meta = meta
                    .as_deref()
                    .map(|meta| format!(" {}", meta))
                    .unwrap_or_default();

                match lang {
                    Some(lang) => format!("```{}{}\n{}\n```", lang, meta, value),
                    None if fence => {
                        format!("```{}\n{}\n```", lang.as_deref().unwrap_or(""), value)
                    }
                    None => value.lines().map(|line| format!("    {}", line)).join("\n"),
                }
            }
            Self::Definition(Definition {
                label, url, title, ..
            }) => {
                format!(
                    "[{}]: {}{}",
                    label.unwrap_or_default(),
                    url.to_string_with(options),
                    title
                        .map(|title| format!(" {}", title.to_string_with(options)))
                        .unwrap_or_default()
                )
            }
            Self::Delete(Value { values, .. }) => {
                format!("~~{}~~", Self::values_to_string(values, options))
            }
            Self::Emphasis(Value { values, .. }) => {
                format!("*{}*", Self::values_to_string(values, options))
            }
            Self::Footnote(Footnote { values, ident, .. }) => {
                format!("[^{}]: {}", ident, Self::values_to_string(values, options))
            }
            Self::FootnoteRef(FootnoteRef { label, .. }) => {
                format!("[^{}]", label.unwrap_or_default())
            }
            Self::Heading(Heading { depth, values, .. }) => {
                format!(
                    "{} {}",
                    "#".repeat(depth as usize),
                    Self::values_to_string(values, options)
                )
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
                if alt == ident {
                    format!("![{}]", ident)
                } else {
                    format!("![{}][{}]", alt, ident)
                }
            }
            Self::CodeInline(CodeInline { value, .. }) => {
                format!("`{}`", value)
            }
            Self::MathInline(MathInline { value, .. }) => {
                format!("${}$", value)
            }
            Self::Link(Link {
                url, title, values, ..
            }) => {
                format!(
                    "[{}]({}{})",
                    Self::values_to_string(values, options),
                    url.to_string_with(options),
                    title
                        .map(|title| format!(" {}", title.to_string_with(options)))
                        .unwrap_or_default(),
                )
            }
            Self::LinkRef(LinkRef { values, label, .. }) => {
                let ident = Self::values_to_string(values, options);
                let label = label.unwrap_or_default();

                if ident == label {
                    format!("[{}]", ident,)
                } else {
                    format!("[{}][{}]", ident, label)
                }
            }
            Self::Math(Math { value, .. }) => format!("$$\n{}\n$$", value),
            Self::Text(Text { value, .. }) => value,
            Self::MdxFlowExpression(mdx_flow_expression) => {
                format!("{{{}}}", mdx_flow_expression.value)
            }
            Self::MdxJsxFlowElement(mdx_jsx_flow_element) => {
                let name = mdx_jsx_flow_element.name.unwrap_or_default();
                let attributes = if mdx_jsx_flow_element.attributes.is_empty() {
                    "".to_string()
                } else {
                    format!(
                        " {}",
                        mdx_jsx_flow_element
                            .attributes
                            .into_iter()
                            .map(Self::mdx_attribute_content_to_string)
                            .join(" ")
                    )
                };

                if mdx_jsx_flow_element.children.is_empty() {
                    format!("<{}{} />", name, attributes,)
                } else {
                    format!(
                        "<{}{}>{}</{}>",
                        name,
                        attributes,
                        Self::values_to_string(mdx_jsx_flow_element.children, options),
                        name
                    )
                }
            }
            Self::MdxJsxTextElement(mdx_jsx_text_element) => {
                let name = mdx_jsx_text_element.name.unwrap_or_default();
                let attributes = if mdx_jsx_text_element.attributes.is_empty() {
                    "".to_string()
                } else {
                    format!(
                        " {}",
                        mdx_jsx_text_element
                            .attributes
                            .into_iter()
                            .map(Self::mdx_attribute_content_to_string)
                            .join(" ")
                    )
                };

                if mdx_jsx_text_element.children.is_empty() {
                    format!("<{}{} />", name, attributes,)
                } else {
                    format!(
                        "<{}{}>{}</{}>",
                        name,
                        attributes,
                        Self::values_to_string(mdx_jsx_text_element.children, options),
                        name
                    )
                }
            }
            Self::MdxTextExpression(mdx_text_expression) => {
                format!("{{{}}}", mdx_text_expression.value)
            }
            Self::MdxJsEsm(mdxjs_esm) => mdxjs_esm.value.to_string(),
            Self::Strong(Value { values, .. }) => {
                format!(
                    "**{}**",
                    values
                        .iter()
                        .map(|value| value.to_string_with(options))
                        .join("")
                )
            }
            Self::Yaml(Yaml { value, .. }) => format!("---\n{}\n---", value),
            Self::Toml(Toml { value, .. }) => format!("+++\n{}\n+++", value),
            Self::Break { .. } => "\\".to_string(),
            Self::HorizontalRule { .. } => "---".to_string(),
            Self::Fragment(Fragment { values }) => values
                .iter()
                .map(|value| value.to_string_with(options))
                .join(""),
            Self::Empty => String::new(),
        }
    }

    pub fn node_values(&self) -> Vec<Node> {
        match self.clone() {
            Self::Blockquote(v) => v.values,
            Self::Delete(v) => v.values,
            Self::Heading(h) => h.values,
            Self::Emphasis(v) => v.values,
            Self::List(l) => l.values,
            Self::Strong(v) => v.values,
            _ => vec![self.clone()],
        }
    }

    pub fn find_at_index(&self, index: usize) -> Option<Node> {
        match self {
            Self::Blockquote(v) | Self::Delete(v) | Self::Emphasis(v) | Self::Strong(v) => {
                v.values.get(index).cloned()
            }
            Self::Heading(v) => v.values.get(index).cloned(),
            Self::List(v) => v.values.get(index).cloned(),
            Self::TableCell(v) => v.values.get(index).cloned(),
            Self::TableRow(v) => v.values.get(index).cloned(),
            _ => None,
        }
    }

    pub fn value(&self) -> String {
        match self.clone() {
            Self::Blockquote(v) => Self::values_to_value(v.values),
            Self::Definition(d) => d.url.as_str().to_string(),
            Self::Delete(v) => Self::values_to_value(v.values),
            Self::Heading(h) => Self::values_to_value(h.values),
            Self::Emphasis(v) => Self::values_to_value(v.values),
            Self::Footnote(f) => Self::values_to_value(f.values),
            Self::FootnoteRef(f) => f.ident,
            Self::Html(v) => v.value,
            Self::Yaml(v) => v.value,
            Self::Toml(v) => v.value,
            Self::Image(i) => i.url,
            Self::ImageRef(i) => i.ident,
            Self::CodeInline(v) => v.value.to_string(),
            Self::MathInline(v) => v.value.to_string(),
            Self::Link(l) => l.url.as_str().to_string(),
            Self::LinkRef(l) => l.ident,
            Self::Math(v) => v.value,
            Self::List(l) => Self::values_to_value(l.values),
            Self::TableCell(c) => Self::values_to_value(c.values),
            Self::TableRow(c) => Self::values_to_value(c.values),
            Self::Code(c) => c.value,
            Self::Strong(v) => Self::values_to_value(v.values),
            Self::Text(t) => t.value,
            Self::Break { .. } => String::new(),
            Self::TableHeader(_) => String::new(),
            Self::MdxFlowExpression(mdx) => mdx.value.to_string(),
            Self::MdxJsxFlowElement(mdx) => Self::values_to_value(mdx.children),
            Self::MdxTextExpression(mdx) => mdx.value.to_string(),
            Self::MdxJsxTextElement(mdx) => Self::values_to_value(mdx.children),
            Self::MdxJsEsm(mdx) => mdx.value.to_string(),
            Self::HorizontalRule { .. } => String::new(),
            Self::Fragment(v) => Self::values_to_value(v.values),
            Self::Empty => String::new(),
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
            Self::TableRow(_) => "table_row".into(),
            Self::TableCell(_) => "table_cell".into(),
            Self::Code(_) => "code".into(),
            Self::Strong(_) => "strong".into(),
            Self::HorizontalRule { .. } => "Horizontal_rule".into(),
            Self::MdxFlowExpression(_) => "mdx_flow_expression".into(),
            Self::MdxJsxFlowElement(_) => "mdx_jsx_flow_element".into(),
            Self::MdxJsxTextElement(_) => "mdx_jsx_text_element".into(),
            Self::MdxTextExpression(_) => "mdx_text_expression".into(),
            Self::MdxJsEsm(_) => "mdx_js_esm".into(),
            Self::Text(_) => "text".into(),
            Self::Fragment(_) | Self::Empty => "".into(),
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
            Self::TableRow(r) => r.position.clone(),
            Self::TableHeader(c) => c.position.clone(),
            Self::List(l) => l.position.clone(),
            Self::Strong(s) => s.position.clone(),
            Self::MdxFlowExpression(m) => m.position.clone(),
            Self::MdxTextExpression(m) => m.position.clone(),
            Self::MdxJsEsm(m) => m.position.clone(),
            Self::MdxJsxFlowElement(m) => m.position.clone(),
            Self::MdxJsxTextElement(m) => m.position.clone(),
            Self::Break { position } => position.clone(),
            Self::Text(t) => t.position.clone(),
            Self::HorizontalRule { position } => position.clone(),
            Self::Fragment(v) => {
                let positions: Vec<Position> =
                    v.values.iter().filter_map(|node| node.position()).collect();

                match (positions.first(), positions.last()) {
                    (Some(start), Some(end)) => Some(Position {
                        start: start.start.clone(),
                        end: end.end.clone(),
                    }),
                    (Some(start), None) => Some(start.clone()),
                    (None, Some(end)) => Some(end.clone()),
                    _ => None,
                }
            }
            Self::Empty => None,
        }
    }

    pub fn is_empty(&self) -> bool {
        matches!(self, Self::Empty)
    }

    pub fn is_fragment(&self) -> bool {
        matches!(self, Self::Fragment(_))
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

    pub fn is_list(&self) -> bool {
        matches!(self, Self::List(_))
    }

    pub fn is_table_cell(&self) -> bool {
        matches!(self, Self::TableCell(_))
    }

    pub fn is_table_row(&self) -> bool {
        matches!(self, Self::TableRow(_))
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

    pub fn is_text(&self) -> bool {
        matches!(self, Self::Text(_))
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
        matches!(self, Self::MdxJsxFlowElement(MdxJsxFlowElement { .. }))
    }

    pub fn is_msx_js_esm(&self) -> bool {
        matches!(self, Self::MdxJsEsm(MdxJsEsm { .. }))
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
        matches!(self, Self::MdxTextExpression(MdxTextExpression { .. }))
    }

    pub fn is_footnote_ref(&self) -> bool {
        matches!(self, Self::FootnoteRef { .. })
    }

    pub fn is_image_ref(&self) -> bool {
        matches!(self, Self::ImageRef(_))
    }

    pub fn is_mdx_jsx_text_element(&self) -> bool {
        matches!(self, Self::MdxJsxTextElement(MdxJsxTextElement { .. }))
    }

    pub fn is_math(&self) -> bool {
        matches!(self, Self::Math(_))
    }

    pub fn is_mdx_flow_expression(&self) -> bool {
        matches!(self, Self::MdxFlowExpression(MdxFlowExpression { .. }))
    }

    pub fn is_definition(&self) -> bool {
        matches!(self, Self::Definition(_))
    }

    pub fn is_code(&self, lang: Option<CompactString>) -> bool {
        if let Self::Code(Code {
            lang: node_lang, ..
        }) = &self
        {
            if lang.is_none() {
                true
            } else {
                node_lang.clone().unwrap_or_default() == lang.unwrap_or_default()
            }
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
                if let Some(node) = v.values.first() {
                    v.values[0] = node.with_value(value);
                }

                Self::Blockquote(v)
            }
            Self::Delete(mut v) => {
                if let Some(node) = v.values.first() {
                    v.values[0] = node.with_value(value);
                }

                Self::Delete(v)
            }
            Self::Emphasis(mut v) => {
                if let Some(node) = v.values.first() {
                    v.values[0] = node.with_value(value);
                }

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
                code.value = value.into();
                Self::CodeInline(code)
            }
            Self::MathInline(mut math) => {
                math.value = value.into();
                Self::MathInline(math)
            }
            Self::Math(mut math) => {
                math.value = value.to_string();
                Self::Math(math)
            }
            Self::List(mut v) => {
                if let Some(node) = v.values.first() {
                    v.values[0] = node.with_value(value);
                }

                Self::List(v)
            }
            Self::TableCell(mut v) => {
                if let Some(node) = v.values.first() {
                    v.values[0] = node.with_value(value);
                }

                Self::TableCell(v)
            }
            Self::TableRow(mut row) => {
                row.values = row
                    .values
                    .iter()
                    .zip(value.split(","))
                    .map(|(cell, value)| cell.with_value(value))
                    .collect::<Vec<_>>();

                Self::TableRow(row)
            }
            Self::Strong(mut v) => {
                if let Some(node) = v.values.first() {
                    v.values[0] = node.with_value(value);
                }

                Self::Strong(v)
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
                link.url = Url(value.to_string());
                Self::Link(link)
            }
            Self::LinkRef(mut v) => {
                if let Some(node) = v.values.first() {
                    v.values[0] = node.with_value(value);
                }

                Self::LinkRef(v)
            }
            Self::Footnote(mut footnote) => {
                footnote.ident = value.to_string();
                Self::Footnote(footnote)
            }
            Self::FootnoteRef(mut footnote) => {
                footnote.ident = value.to_string();
                Self::FootnoteRef(footnote)
            }
            Self::Heading(mut v) => {
                if let Some(node) = v.values.first() {
                    v.values[0] = node.with_value(value);
                }

                Self::Heading(v)
            }
            Self::Definition(mut def) => {
                def.url = Url(value.to_string());
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
                mdx.value = value.into();
                Self::MdxFlowExpression(mdx)
            }
            Self::MdxTextExpression(mut mdx) => {
                mdx.value = value.into();
                Self::MdxTextExpression(mdx)
            }
            Self::MdxJsEsm(mut mdx) => {
                mdx.value = value.into();
                Self::MdxJsEsm(mdx)
            }
            Self::MdxJsxFlowElement(mut mdx) => {
                if let Some(node) = mdx.children.first() {
                    mdx.children[0] = node.with_value(value);
                }

                Self::MdxJsxFlowElement(MdxJsxFlowElement {
                    name: mdx.name,
                    attributes: mdx.attributes,
                    children: mdx.children,
                    ..mdx
                })
            }
            Self::MdxJsxTextElement(mut mdx) => {
                if let Some(node) = mdx.children.first() {
                    mdx.children[0] = node.with_value(value);
                }

                Self::MdxJsxTextElement(MdxJsxTextElement {
                    name: mdx.name,
                    attributes: mdx.attributes,
                    children: mdx.children,
                    ..mdx
                })
            }
            node @ Self::Fragment(_) | node @ Self::Empty => node,
        }
    }

    pub fn with_children_value(&self, value: &str, index: usize) -> Self {
        match self.clone() {
            Self::Blockquote(mut v) => {
                if v.values.get(index).is_some() {
                    v.values[index] = v.values[index].with_value(value);
                }

                Self::Blockquote(v)
            }
            Self::Delete(mut v) => {
                if v.values.get(index).is_some() {
                    v.values[index] = v.values[index].with_value(value);
                }

                Self::Delete(v)
            }
            Self::Emphasis(mut v) => {
                if v.values.get(index).is_some() {
                    v.values[index] = v.values[index].with_value(value);
                }

                Self::Emphasis(v)
            }
            Self::List(mut v) => {
                if v.values.get(index).is_some() {
                    v.values[index] = v.values[index].with_value(value);
                }

                Self::List(v)
            }
            Self::TableCell(mut v) => {
                if v.values.get(index).is_some() {
                    v.values[index] = v.values[index].with_value(value);
                }

                Self::TableCell(v)
            }
            Self::Strong(mut v) => {
                if v.values.get(index).is_some() {
                    v.values[index] = v.values[index].with_value(value);
                }

                Self::Strong(v)
            }
            Self::LinkRef(mut v) => {
                if v.values.get(index).is_some() {
                    v.values[index] = v.values[index].with_value(value);
                }

                Self::LinkRef(v)
            }
            Self::Heading(mut v) => {
                if v.values.get(index).is_some() {
                    v.values[index] = v.values[index].with_value(value);
                }

                Self::Heading(v)
            }
            Self::MdxJsxFlowElement(mut mdx) => {
                if let Some(node) = mdx.children.first() {
                    mdx.children[index] = node.with_value(value);
                }

                Self::MdxJsxFlowElement(MdxJsxFlowElement {
                    name: mdx.name,
                    attributes: mdx.attributes,
                    children: mdx.children,
                    ..mdx
                })
            }
            Self::MdxJsxTextElement(mut mdx) => {
                if let Some(node) = mdx.children.first() {
                    mdx.children[index] = node.with_value(value);
                }

                Self::MdxJsxTextElement(MdxJsxTextElement {
                    name: mdx.name,
                    attributes: mdx.attributes,
                    children: mdx.children,
                    ..mdx
                })
            }
            a => a,
        }
    }

    pub(crate) fn from_mdast_node(node: mdast::Node) -> Vec<Node> {
        match node.clone() {
            mdast::Node::Root(root) => root
                .children
                .into_iter()
                .flat_map(Self::from_mdast_node)
                .collect::<Vec<_>>(),
            mdast::Node::ListItem(list_item) => list_item
                .children
                .into_iter()
                .flat_map(Self::from_mdast_node)
                .collect::<Vec<_>>(),
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
                                            values: Self::mdast_children_to_node(node.clone()),
                                            position: node.position().map(|p| p.clone().into()),
                                        })]
                                    } else {
                                        Vec::new()
                                    }
                                })
                                .collect(),
                            if row == 0 {
                                vec![Self::TableHeader(TableHeader {
                                    align: table
                                        .align
                                        .iter()
                                        .map(|a| (*a).into())
                                        .collect::<Vec<_>>(),
                                    position: n.position().map(|p| Position {
                                        start: Point {
                                            line: p.start.line + 1,
                                            column: 1,
                                        },
                                        end: Point {
                                            line: p.start.line + 1,
                                            column: 1,
                                        },
                                    }),
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
                meta,
                ..
            }) => match lang {
                Some(lang) => {
                    vec![Self::Code(Code {
                        value,
                        lang: Some(lang),
                        position: position.map(|p| p.clone().into()),
                        meta,
                        fence: true,
                    })]
                }
                None => {
                    let line_count = position
                        .as_ref()
                        .map(|p| p.end.line - p.start.line + 1)
                        .unwrap_or_default();
                    let fence = value.lines().count() != line_count;

                    vec![Self::Code(Code {
                        value,
                        lang,
                        position: position.map(|p| p.clone().into()),
                        meta,
                        fence,
                    })]
                }
            },
            mdast::Node::Blockquote(mdast::Blockquote { position, .. }) => {
                vec![Self::Blockquote(Value {
                    values: Self::mdast_children_to_node(node),
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
                    url: Url(url),
                    label,
                    title: title.map(Title),
                    position: position.map(|p| p.clone().into()),
                })]
            }
            mdast::Node::Heading(mdast::Heading {
                depth, position, ..
            }) => {
                vec![Self::Heading(Heading {
                    values: Self::mdast_children_to_node(node),
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
                    values: Self::mdast_children_to_node(node),
                    position: position.map(|p| p.clone().into()),
                })]
            }
            mdast::Node::Emphasis(mdast::Emphasis { position, .. }) => {
                vec![Self::Emphasis(Value {
                    values: Self::mdast_children_to_node(node),
                    position: position.map(|p| p.clone().into()),
                })]
            }
            mdast::Node::Strong(mdast::Strong { position, .. }) => {
                vec![Self::Strong(Value {
                    values: Self::mdast_children_to_node(node),
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
                    value: value.into(),
                    position: position.map(|p| p.clone().into()),
                })]
            }
            mdast::Node::InlineMath(mdast::InlineMath { value, position }) => {
                vec![Self::MathInline(MathInline {
                    value: value.into(),
                    position: position.map(|p| p.clone().into()),
                })]
            }
            mdast::Node::Link(mdast::Link {
                title,
                url,
                position,
                children,
                ..
            }) => {
                vec![Self::Link(Link {
                    url: Url(url),
                    title: title.map(Title),
                    values: children
                        .into_iter()
                        .flat_map(Self::from_mdast_node)
                        .collect::<Vec<_>>(),
                    position: position.map(|p| p.clone().into()),
                })]
            }
            mdast::Node::LinkReference(mdast::LinkReference {
                identifier,
                label,
                position,
                children,
                ..
            }) => {
                vec![Self::LinkRef(LinkRef {
                    ident: identifier,
                    values: children
                        .into_iter()
                        .flat_map(Self::from_mdast_node)
                        .collect::<Vec<_>>(),
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
                position,
                children,
                ..
            }) => {
                vec![Self::Footnote(Footnote {
                    ident: identifier,
                    values: children
                        .into_iter()
                        .flat_map(Self::from_mdast_node)
                        .collect::<Vec<_>>(),
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
                vec![Self::MdxFlowExpression(MdxFlowExpression {
                    value: mdx.value.into(),
                    position: mdx.position.map(|position| position.into()),
                })]
            }
            mdast::Node::MdxJsxFlowElement(mdx) => {
                vec![Self::MdxJsxFlowElement(MdxJsxFlowElement {
                    children: mdx
                        .children
                        .into_iter()
                        .flat_map(Self::from_mdast_node)
                        .collect::<Vec<_>>(),
                    position: mdx.position.map(|p| p.clone().into()),
                    name: mdx.name,
                    attributes: mdx
                        .attributes
                        .iter()
                        .map(|attr| match attr {
                            mdast::AttributeContent::Expression(
                                mdast::MdxJsxExpressionAttribute { value, .. },
                            ) => MdxAttributeContent::Expression(value.into()),
                            mdast::AttributeContent::Property(mdast::MdxJsxAttribute {
                                value,
                                name,
                                ..
                            }) => MdxAttributeContent::Property(MdxJsxAttribute {
                                name: name.into(),
                                value: value.as_ref().map(|value| match value {
                                    mdast::AttributeValue::Literal(value) => {
                                        MdxAttributeValue::Literal(value.into())
                                    }
                                    mdast::AttributeValue::Expression(
                                        mdast::AttributeValueExpression { value, .. },
                                    ) => MdxAttributeValue::Expression(value.into()),
                                }),
                            }),
                        })
                        .collect(),
                })]
            }
            mdast::Node::MdxJsxTextElement(mdx) => {
                vec![Self::MdxJsxTextElement(MdxJsxTextElement {
                    children: mdx
                        .children
                        .into_iter()
                        .flat_map(Self::from_mdast_node)
                        .collect::<Vec<_>>(),
                    position: mdx.position.map(|p| p.clone().into()),
                    name: mdx.name.map(|name| name.into()),
                    attributes: mdx
                        .attributes
                        .iter()
                        .map(|attr| match attr {
                            mdast::AttributeContent::Expression(
                                mdast::MdxJsxExpressionAttribute { value, .. },
                            ) => MdxAttributeContent::Expression(value.into()),
                            mdast::AttributeContent::Property(mdast::MdxJsxAttribute {
                                value,
                                name,
                                ..
                            }) => MdxAttributeContent::Property(MdxJsxAttribute {
                                name: name.into(),
                                value: value.as_ref().map(|value| match value {
                                    mdast::AttributeValue::Literal(value) => {
                                        MdxAttributeValue::Literal(value.into())
                                    }
                                    mdast::AttributeValue::Expression(
                                        mdast::AttributeValueExpression { value, .. },
                                    ) => MdxAttributeValue::Expression(value.into()),
                                }),
                            }),
                        })
                        .collect(),
                })]
            }
            mdast::Node::MdxTextExpression(mdx) => {
                vec![Self::MdxTextExpression(MdxTextExpression {
                    value: mdx.value.into(),
                    position: mdx.position.map(|position| position.into()),
                })]
            }
            mdast::Node::MdxjsEsm(mdx) => {
                vec![Self::MdxJsEsm(MdxJsEsm {
                    value: mdx.value.into(),
                    position: mdx.position.map(|position| position.into()),
                })]
            }
            mdast::Node::Text(mdast::Text {
                position, value, ..
            }) => {
                vec![Self::Text(Text {
                    value,
                    position: position.map(|p| p.clone().into()),
                })]
            }
            mdast::Node::Paragraph(mdast::Paragraph { children, .. }) => children
                .into_iter()
                .flat_map(Self::from_mdast_node)
                .collect::<Vec<_>>(),
            _ => Vec::new(),
        }
    }

    fn mdast_children_to_node(node: mdast::Node) -> Vec<Node> {
        node.children()
            .map(|children| {
                children
                    .iter()
                    .flat_map(|v| Self::from_mdast_node(v.clone()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_else(|| vec![EMPTY_NODE])
    }

    fn mdast_list_items(list: &mdast::List, level: Level) -> Vec<Node> {
        list.children
            .iter()
            .flat_map(|n| {
                if let mdast::Node::ListItem(list) = n {
                    let values = Self::from_mdast_node(n.clone())
                        .into_iter()
                        .filter(|value| !matches!(value, Self::List(_)))
                        .collect::<Vec<_>>();
                    let position = if values.is_empty() {
                        n.position().map(|p| p.clone().into())
                    } else {
                        let first_pos = values.first().and_then(|v| v.position());
                        let last_pos = values.last().and_then(|v| v.position());
                        match (first_pos, last_pos) {
                            (Some(start), Some(end)) => Some(Position {
                                start: start.start.clone(),
                                end: end.end.clone(),
                            }),
                            _ => n.position().map(|p| p.clone().into()),
                        }
                    };

                    itertools::concat(vec![
                        vec![Self::List(List {
                            level,
                            index: 0,
                            checked: list.checked,
                            values,
                            position,
                        })],
                        list.children
                            .iter()
                            .flat_map(|node| {
                                if let mdast::Node::List(sub_list) = node {
                                    Self::mdast_list_items(sub_list, level + 1)
                                } else if let mdast::Node::ListItem(list) = node {
                                    let values = Self::from_mdast_node(n.clone())
                                        .into_iter()
                                        .filter(|value| !matches!(value, Self::List(_)))
                                        .collect::<Vec<_>>();
                                    let position = if values.is_empty() {
                                        n.position().map(|p| p.clone().into())
                                    } else {
                                        let first_pos = values.first().and_then(|v| v.position());
                                        let last_pos = values.last().and_then(|v| v.position());
                                        match (first_pos, last_pos) {
                                            (Some(start), Some(end)) => Some(Position {
                                                start: start.start.clone(),
                                                end: end.end.clone(),
                                            }),
                                            _ => n.position().map(|p| p.clone().into()),
                                        }
                                    };
                                    vec![Self::List(List {
                                        level: level + 1,
                                        index: 0,
                                        checked: list.checked,
                                        values,
                                        position,
                                    })]
                                } else {
                                    Vec::new()
                                }
                            })
                            .collect(),
                    ])
                } else if let mdast::Node::List(sub_list) = n {
                    Self::mdast_list_items(sub_list, level + 1)
                } else {
                    Vec::new()
                }
            })
            .enumerate()
            .filter_map(|(i, node)| match node {
                Self::List(List {
                    level,
                    index: _,
                    checked,
                    values,
                    position,
                }) => Some(Self::List(List {
                    level,
                    index: i,
                    checked,
                    values,
                    position,
                })),
                _ => None,
            })
            .collect()
    }

    fn mdx_attribute_content_to_string(attr: MdxAttributeContent) -> CompactString {
        match attr {
            MdxAttributeContent::Expression(value) => format!("{{{}}}", value).into(),
            MdxAttributeContent::Property(property) => match property.value {
                Some(value) => match value {
                    MdxAttributeValue::Expression(value) => {
                        format!("{}={{{}}}", property.name, value).into()
                    }
                    MdxAttributeValue::Literal(literal) => {
                        format!("{}=\"{}\"", property.name, literal).into()
                    }
                },
                None => property.name,
            },
        }
    }

    #[inline(always)]
    fn values_to_string(values: Vec<Node>, options: &RenderOptions) -> String {
        let mut pre_position: Option<Position> = None;
        values
            .iter()
            .map(|value| {
                if let Some(pos) = value.position() {
                    let new_line_count = pre_position
                        .as_ref()
                        .map(|p: &Position| pos.start.line - p.end.line)
                        .unwrap_or_default();

                    let space = if new_line_count > 0
                        && pre_position
                            .as_ref()
                            .map(|p| pos.start.line > p.end.line)
                            .unwrap_or_default()
                    {
                        " ".repeat(pos.start.column.saturating_sub(1))
                    } else {
                        "".to_string()
                    };

                    pre_position = Some(pos);

                    if space.is_empty() {
                        format!(
                            "{}{}",
                            "\n".repeat(new_line_count),
                            value.to_string_with(options)
                        )
                    } else {
                        format!(
                            "{}{}",
                            "\n".repeat(new_line_count),
                            value
                                .to_string_with(options)
                                .lines()
                                .map(|line| format!("{}{}", space, line))
                                .join("\n")
                        )
                    }
                } else {
                    pre_position = None;
                    value.to_string_with(options)
                }
            })
            .join("")
    }

    #[inline(always)]
    fn values_to_value(values: Vec<Node>) -> String {
        values.iter().map(|value| value.value()).join("")
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
    #[case::blockquote(Node::Blockquote(Value {values: vec!["test".to_string().into()], position: None }),
           "test".to_string(),
           Node::Blockquote(Value{values: vec!["test".to_string().into()], position: None }))]
    #[case::delete(Node::Delete(Value {values: vec!["test".to_string().into()], position: None }),
           "test".to_string(),
           Node::Delete(Value{values: vec!["test".to_string().into()], position: None }))]
    #[case::emphasis(Node::Emphasis(Value {values: vec!["test".to_string().into()], position: None }),
           "test".to_string(),
           Node::Emphasis(Value{values: vec!["test".to_string().into()], position: None }))]
    #[case::strong(Node::Strong(Value {values: vec!["test".to_string().into()], position: None }),
           "test".to_string(),
           Node::Strong(Value{values: vec!["test".to_string().into()], position: None }))]
    #[case::heading(Node::Heading(Heading {depth: 1, values: vec!["test".to_string().into()], position: None }),
           "test".to_string(),
           Node::Heading(Heading{depth: 1, values: vec!["test".to_string().into()], position: None }))]
    #[case::link(Node::Link(Link {url: Url::new("test".to_string()), values: Vec::new(), title: None, position: None }),
           "test".to_string(),
           Node::Link(Link{url: Url::new("test".to_string()), values: Vec::new(), title: None, position: None }))]
    #[case::image(Node::Image(Image {alt: "test".to_string(), url: "test".to_string(), title: None, position: None }),
           "test".to_string(),
           Node::Image(Image{alt: "test".to_string(), url: "test".to_string(), title: None, position: None }))]
    #[case::code(Node::Code(Code {value: "test".to_string(), lang: None, fence: true, meta: None, position: None }),
           "test".to_string(),
           Node::Code(Code{value: "test".to_string(), lang: None, fence: true, meta: None, position: None }))]
    #[case::footnoteref(Node::FootnoteRef(FootnoteRef {ident: "test".to_string(), label: None, position: None }),
           "test".to_string(),
           Node::FootnoteRef(FootnoteRef{ident: "test".to_string(), label: None, position: None }))]
    #[case::footnote(Node::Footnote(Footnote {ident: "test".to_string(), values: Vec::new(), position: None }),
           "test".to_string(),
           Node::Footnote(Footnote{ident: "test".to_string(), values: Vec::new(), position: None }))]
    #[case::list(Node::List(List{index: 0, level: 0, checked: None, values: vec!["test".to_string().into()], position: None }),
           "test".to_string(),
           Node::List(List{index: 0, level: 0, checked: None, values: vec!["test".to_string().into()], position: None }))]
    #[case::list(Node::List(List{index: 1, level: 1, checked: Some(true), values: vec!["test".to_string().into()], position: None }),
           "test".to_string(),
           Node::List(List{index: 1, level: 1, checked: Some(true), values: vec!["test".to_string().into()], position: None }))]
    #[case::list(Node::List(List{index: 2, level: 2, checked: Some(false), values: vec!["test".to_string().into()], position: None }),
           "test".to_string(),
           Node::List(List{index: 2, level: 2, checked: Some(false), values: vec!["test".to_string().into()], position: None }))]
    #[case::code_inline(Node::CodeInline(CodeInline{ value: "t".into(), position: None }),
           "test".to_string(),
           Node::CodeInline(CodeInline{ value: "test".into(), position: None }))]
    #[case::math_inline(Node::MathInline(MathInline{ value: "t".into(), position: None }),
           "test".to_string(),
           Node::MathInline(MathInline{ value: "test".into(), position: None }))]
    #[case::toml(Node::Toml(Toml{ value: "t".to_string(), position: None }),
           "test".to_string(),
           Node::Toml(Toml{ value: "test".to_string(), position: None }))]
    #[case::yaml(Node::Yaml(Yaml{ value: "t".to_string(), position: None }),
           "test".to_string(),
           Node::Yaml(Yaml{ value: "test".to_string(), position: None }))]
    #[case::html(Node::Html(Html{ value: "t".to_string(), position: None }),
           "test".to_string(),
           Node::Html(Html{ value: "test".to_string(), position: None }))]
    #[case::table_row(Node::TableRow(TableRow{ values: vec![
                        Node::TableCell(TableCell{values: vec!["test1".to_string().into()], row:0, column:1, last_cell_in_row: false, last_cell_of_in_table: false, position: None}),
                        Node::TableCell(TableCell{values: vec!["test2".to_string().into()], row:0, column:2, last_cell_in_row: true, last_cell_of_in_table: false, position: None})
                    ]
                    , position: None }),
           "test3,test4".to_string(),
           Node::TableRow(TableRow{ values: vec![
                        Node::TableCell(TableCell{values: vec!["test3".to_string().into()], row:0, column:1, last_cell_in_row: false, last_cell_of_in_table: false, position: None}),
                        Node::TableCell(TableCell{values: vec!["test4".to_string().into()], row:0, column:2, last_cell_in_row: true, last_cell_of_in_table: false, position: None})
                    ]
                    , position: None }))]
    #[case::table_cell(Node::TableCell(TableCell{values: vec!["test1".to_string().into()], row:0, column:1, last_cell_in_row: false, last_cell_of_in_table: false, position: None}),
            "test2".to_string(),
            Node::TableCell(TableCell{values: vec!["test2".to_string().into()], row:0, column:1, last_cell_in_row: false, last_cell_of_in_table: false, position: None}),)]
    #[case::link_ref(Node::LinkRef(LinkRef{ident: "test1".to_string(), values: vec!["value".to_string().into()], label: None, position: None}),
            "test2".to_string(),
            Node::LinkRef(LinkRef{ident: "test1".to_string(), values: vec!["test2".to_string().into()], label: None, position: None}),)]
    #[case::image_ref(Node::ImageRef(ImageRef{alt: "alt".to_string(), ident: "test1".to_string(), label: None, position: None}),
            "test2".to_string(),
            Node::ImageRef(ImageRef{alt: "alt".to_string(), ident: "test2".to_string(), label: None, position: None}),)]
    #[case::definition(Node::Definition(Definition{ url: Url::new("url".to_string()), title: None, ident: "test1".to_string(), label: None, position: None}),
            "test2".to_string(),
            Node::Definition(Definition{url: Url::new("test2".to_string()), title: None, ident: "test1".to_string(), label: None, position: None}),)]
    #[case::break_(Node::Break{ position: None},
            "test".to_string(),
            Node::Break{position: None})]
    #[case::horizontal_rule(Node::HorizontalRule { position: None},
            "test".to_string(),
            Node::HorizontalRule{position: None})]
    #[case::mdx_flow_expression(Node::MdxFlowExpression(MdxFlowExpression{value: "test".into(), position: None}),
           "updated".to_string(),
           Node::MdxFlowExpression(MdxFlowExpression{value: "updated".into(), position: None}))]
    #[case::mdx_text_expression(Node::MdxTextExpression(MdxTextExpression{value: "test".into(), position: None}),
           "updated".to_string(),
           Node::MdxTextExpression(MdxTextExpression{value: "updated".into(), position: None}))]
    #[case::mdx_js_esm(Node::MdxJsEsm(MdxJsEsm{value: "test".into(), position: None}),
           "updated".to_string(),
           Node::MdxJsEsm(MdxJsEsm{value: "updated".into(), position: None}))]
    #[case(Node::MdxJsxFlowElement(MdxJsxFlowElement{
            name: Some("div".to_string()),
            attributes: Vec::new(),
            children: vec!["test".to_string().into()],
            position: None
        }),
        "updated".to_string(),
        Node::MdxJsxFlowElement(MdxJsxFlowElement{
            name: Some("div".to_string()),
            attributes: Vec::new(),
            children: vec!["updated".to_string().into()],
            position: None
        }))]
    #[case::mdx_jsx_text_element(Node::MdxJsxTextElement(MdxJsxTextElement{
            name: Some("span".into()),
            attributes: Vec::new(),
            children: vec!["test".to_string().into()],
            position: None
        }),
        "updated".to_string(),
        Node::MdxJsxTextElement(MdxJsxTextElement{
            name: Some("span".into()),
            attributes: Vec::new(),
            children: vec!["updated".to_string().into()],
            position: None
        }))]
    #[case(Node::Math(Math{ value: "x^2".to_string(), position: None }),
           "test".to_string(),
           Node::Math(Math{ value: "test".to_string(), position: None }))]
    fn test_with_value(#[case] node: Node, #[case] input: String, #[case] expected: Node) {
        assert_eq!(node.with_value(input.as_str()), expected);
    }

    #[rstest]
    #[case(Node::Blockquote(Value{values: vec![
        Node::Text(Text{value: "first".to_string(), position: None}),
        Node::Text(Text{value: "second".to_string(), position: None})
    ], position: None}),
        "new",
        0,
        Node::Blockquote(Value{values: vec![
            Node::Text(Text{value: "new".to_string(), position: None}),
            Node::Text(Text{value: "second".to_string(), position: None})
        ], position: None}))]
    #[case(Node::Blockquote(Value{values: vec![
        Node::Text(Text{value: "first".to_string(), position: None}),
        Node::Text(Text{value: "second".to_string(), position: None})
    ], position: None}),
        "new",
        1,
        Node::Blockquote(Value{values: vec![
            Node::Text(Text{value: "first".to_string(), position: None}),
            Node::Text(Text{value: "new".to_string(), position: None})
        ], position: None}))]
    #[case(Node::Delete(Value{values: vec![
        Node::Text(Text{value: "first".to_string(), position: None}),
        Node::Text(Text{value: "second".to_string(), position: None})
    ], position: None}),
        "new",
        0,
        Node::Delete(Value{values: vec![
            Node::Text(Text{value: "new".to_string(), position: None}),
            Node::Text(Text{value: "second".to_string(), position: None})
        ], position: None}))]
    #[case(Node::Emphasis(Value{values: vec![
        Node::Text(Text{value: "first".to_string(), position: None}),
        Node::Text(Text{value: "second".to_string(), position: None})
    ], position: None}),
        "new",
        1,
        Node::Emphasis(Value{values: vec![
            Node::Text(Text{value: "first".to_string(), position: None}),
            Node::Text(Text{value: "new".to_string(), position: None})
        ], position: None}))]
    #[case(Node::Strong(Value{values: vec![
        Node::Text(Text{value: "first".to_string(), position: None}),
        Node::Text(Text{value: "second".to_string(), position: None})
    ], position: None}),
        "new",
        0,
        Node::Strong(Value{values: vec![
            Node::Text(Text{value: "new".to_string(), position: None}),
            Node::Text(Text{value: "second".to_string(), position: None})
        ], position: None}))]
    #[case(Node::Heading(Heading{depth: 1, values: vec![
        Node::Text(Text{value: "first".to_string(), position: None}),
        Node::Text(Text{value: "second".to_string(), position: None})
    ], position: None}),
        "new",
        1,
        Node::Heading(Heading{depth: 1, values: vec![
            Node::Text(Text{value: "first".to_string(), position: None}),
            Node::Text(Text{value: "new".to_string(), position: None})
        ], position: None}))]
    #[case(Node::List(List{index: 0, level: 0, checked: None, values: vec![
        Node::Text(Text{value: "first".to_string(), position: None}),
        Node::Text(Text{value: "second".to_string(), position: None})
    ], position: None}),
        "new",
        0,
        Node::List(List{index: 0, level: 0, checked: None, values: vec![
            Node::Text(Text{value: "new".to_string(), position: None}),
            Node::Text(Text{value: "second".to_string(), position: None})
        ], position: None}))]
    #[case(Node::TableCell(TableCell{column: 0, row: 0, last_cell_in_row: false, last_cell_of_in_table: false, values: vec![
        Node::Text(Text{value: "first".to_string(), position: None}),
        Node::Text(Text{value: "second".to_string(), position: None})
    ], position: None}),
        "new",
        1,
        Node::TableCell(TableCell{column: 0, row: 0, last_cell_in_row: false, last_cell_of_in_table: false, values: vec![
            Node::Text(Text{value: "first".to_string(), position: None}),
            Node::Text(Text{value: "new".to_string(), position: None})
        ], position: None}))]
    #[case(Node::Text(Text{value: "plain text".to_string(), position: None}),
        "new",
        0,
        Node::Text(Text{value: "plain text".to_string(), position: None}))]
    #[case(Node::Code(Code{value: "code".to_string(), lang: None, fence: true, meta: None, position: None}),
        "new",
        0,
        Node::Code(Code{value: "code".to_string(), lang: None, fence: true, meta: None, position: None}))]
    #[case(Node::List(List{index: 0, level: 1, checked: Some(true), values: vec![
        Node::Text(Text{value: "first".to_string(), position: None})
    ], position: None}),
        "new",
        0,
        Node::List(List{index: 0, level: 1, checked: Some(true), values: vec![
            Node::Text(Text{value: "new".to_string(), position: None})
        ], position: None}))]
    #[case(Node::List(List{index: 0, level: 1, checked: None, values: vec![
        Node::Text(Text{value: "first".to_string(), position: None})
    ], position: None}),
        "new",
        2,
        Node::List(List{index: 0, level: 1, checked: None, values: vec![
            Node::Text(Text{value: "first".to_string(), position: None})
        ], position: None}))]
    #[case::link_ref(Node::LinkRef(LinkRef{ident: "id".to_string(), values: vec![
            Node::Text(Text{value: "first".to_string(), position: None}),
            Node::Text(Text{value: "second".to_string(), position: None})
        ], label: None, position: None}), "new", 0, Node::LinkRef(LinkRef{ident: "id".to_string(), values: vec![
            Node::Text(Text{value: "new".to_string(), position: None}),
            Node::Text(Text{value: "second".to_string(), position: None})
        ], label: None, position: None}))]
    #[case(Node::MdxJsxFlowElement(MdxJsxFlowElement{
            name: Some("div".to_string()),
            attributes: Vec::new(),
            children: vec![
                Node::Text(Text{value: "first".to_string(), position: None}),
                Node::Text(Text{value: "second".to_string(), position: None})
            ],
            position: None
        }),
        "new",
        0,
        Node::MdxJsxFlowElement(MdxJsxFlowElement{
            name: Some("div".to_string()),
            attributes: Vec::new(),
            children: vec![
                Node::Text(Text{value: "new".to_string(), position: None}),
                Node::Text(Text{value: "second".to_string(), position: None})
            ],
            position: None
        }))]
    #[case(Node::MdxJsxFlowElement(MdxJsxFlowElement{
            name: Some("div".to_string()),
            attributes: Vec::new(),
            children: vec![
                Node::Text(Text{value: "first".to_string(), position: None}),
                Node::Text(Text{value: "second".to_string(), position: None})
            ],
            position: None
        }),
        "new",
        1,
        Node::MdxJsxFlowElement(MdxJsxFlowElement{
            name: Some("div".to_string()),
            attributes: Vec::new(),
            children: vec![
                Node::Text(Text{value: "first".to_string(), position: None}),
                Node::Text(Text{value: "new".to_string(), position: None})
            ],
            position: None
        }))]
    #[case(Node::MdxJsxTextElement(MdxJsxTextElement{
            name: Some("span".into()),
            attributes: Vec::new(),
            children: vec![
                Node::Text(Text{value: "first".to_string(), position: None}),
                Node::Text(Text{value: "second".to_string(), position: None})
            ],
            position: None
        }),
        "new",
        0,
        Node::MdxJsxTextElement(MdxJsxTextElement{
            name: Some("span".into()),
            attributes: Vec::new(),
            children: vec![
                Node::Text(Text{value: "new".to_string(), position: None}),
                Node::Text(Text{value: "second".to_string(), position: None})
            ],
            position: None
        }))]
    #[case(Node::MdxJsxTextElement(MdxJsxTextElement{
            name: Some("span".into()),
            attributes: Vec::new(),
            children: vec![
                Node::Text(Text{value: "first".to_string(), position: None}),
                Node::Text(Text{value: "second".to_string(), position: None})
            ],
            position: None
        }),
        "new",
        1,
        Node::MdxJsxTextElement(MdxJsxTextElement{
            name: Some("span".into()),
            attributes: Vec::new(),
            children: vec![
                Node::Text(Text{value: "first".to_string(), position: None}),
                Node::Text(Text{value: "new".to_string(), position: None})
            ],
            position: None
        }))]
    fn test_with_children_value(
        #[case] node: Node,
        #[case] value: &str,
        #[case] index: usize,
        #[case] expected: Node,
    ) {
        assert_eq!(node.with_children_value(value, index), expected);
    }

    #[rstest]
    #[case(Node::Text(Text{value: "test".to_string(), position: None }),
           "test".to_string())]
    #[case(Node::List(List{index: 0, level: 2, checked: None, values: vec!["test".to_string().into()], position: None}),
           "    - test".to_string())]
    fn test_display(#[case] node: Node, #[case] expected: String) {
        assert_eq!(node.to_string_with(&RenderOptions::default()), expected);
    }

    #[rstest]
    #[case(Node::Text(Text{value: "test".to_string(), position: None}), true)]
    #[case(Node::CodeInline(CodeInline{value: "test".into(), position: None}), false)]
    #[case(Node::MathInline(MathInline{value: "test".into(), position: None}), false)]
    fn test_is_text(#[case] node: Node, #[case] expected: bool) {
        assert_eq!(node.is_text(), expected);
    }

    #[rstest]
    #[case(Node::CodeInline(CodeInline{value: "test".into(), position: None}), true)]
    #[case(Node::Text(Text{value: "test".to_string(), position: None}), false)]
    fn test_is_inline_code(#[case] node: Node, #[case] expected: bool) {
        assert_eq!(node.is_inline_code(), expected);
    }

    #[rstest]
    #[case(Node::MathInline(MathInline{value: "test".into(), position: None}), true)]
    #[case(Node::Text(Text{value: "test".to_string(), position: None}), false)]
    fn test_is_inline_math(#[case] node: Node, #[case] expected: bool) {
        assert_eq!(node.is_inline_math(), expected);
    }

    #[rstest]
    #[case(Node::Strong(Value{values: vec!["test".to_string().into()], position: None}), true)]
    #[case(Node::Text(Text{value: "test".to_string(), position: None}), false)]
    fn test_is_strong(#[case] node: Node, #[case] expected: bool) {
        assert_eq!(node.is_strong(), expected);
    }

    #[rstest]
    #[case(Node::Delete(Value{values: vec!["test".to_string().into()], position: None}), true)]
    #[case(Node::Text(Text{value: "test".to_string(), position: None}), false)]
    fn test_is_delete(#[case] node: Node, #[case] expected: bool) {
        assert_eq!(node.is_delete(), expected);
    }

    #[rstest]
    #[case(Node::Link(Link{url: Url::new("test".to_string()), values: Vec::new(), title: None, position: None}), true)]
    #[case(Node::Text(Text{value: "test".to_string(), position: None}), false)]
    fn test_is_link(#[case] node: Node, #[case] expected: bool) {
        assert_eq!(node.is_link(), expected);
    }

    #[rstest]
    #[case(Node::LinkRef(LinkRef{ident: "test".to_string(), values: Vec::new(), label: None, position: None}), true)]
    #[case(Node::Text(Text{value: "test".to_string(), position: None}), false)]
    fn test_is_link_ref(#[case] node: Node, #[case] expected: bool) {
        assert_eq!(node.is_link_ref(), expected);
    }

    #[rstest]
    #[case(Node::Image(Image{alt: "alt".to_string(), url: "test".to_string(), title: None, position: None}), true)]
    #[case(Node::Text(Text{value: "test".to_string(), position: None}), false)]
    fn test_is_image(#[case] node: Node, #[case] expected: bool) {
        assert_eq!(node.is_image(), expected);
    }

    #[rstest]
    #[case(Node::ImageRef(ImageRef{alt: "alt".to_string(), ident: "test".to_string(), label: None, position: None}), true)]
    #[case(Node::Text(Text{value: "test".to_string(), position: None}), false)]
    fn test_is_image_ref(#[case] node: Node, #[case] expected: bool) {
        assert_eq!(node.is_image_ref(), expected);
    }

    #[rstest]
    #[case(Node::Code(Code{value: "code".to_string(), lang: Some("rust".to_string()), fence: true, meta: None, position: None}), true, Some("rust".into()))]
    #[case(Node::Code(Code{value: "code".to_string(), lang: Some("rust".to_string()), fence: true, meta: None, position: None}), false, Some("python".into()))]
    #[case(Node::Code(Code{value: "code".to_string(), lang: None, fence: true, meta: None, position: None}), true, None)]
    #[case(Node::Code(Code{value: "code".to_string(), lang: None, fence: false, meta: None, position: None}), true, None)]
    #[case(Node::Text(Text{value: "test".to_string(), position: None}), false, None)]
    fn test_is_code(
        #[case] node: Node,
        #[case] expected: bool,
        #[case] lang: Option<CompactString>,
    ) {
        assert_eq!(node.is_code(lang), expected);
    }

    #[rstest]
    #[case(Node::Heading(Heading{depth: 1, values: vec!["test".to_string().into()], position: None}), true, Some(1))]
    #[case(Node::Heading(Heading{depth: 2, values: vec!["test".to_string().into()], position: None}), false, Some(1))]
    #[case(Node::Heading(Heading{depth: 1, values: vec!["test".to_string().into()], position: None}), true, None)]
    #[case(Node::Text(Text{value: "test".to_string(), position: None}), false, None)]
    fn test_is_heading(#[case] node: Node, #[case] expected: bool, #[case] depth: Option<u8>) {
        assert_eq!(node.is_heading(depth), expected);
    }

    #[rstest]
    #[case(Node::HorizontalRule{position: None}, true)]
    #[case(Node::Text(Text{value: "test".to_string(), position: None}), false)]
    fn test_is_horizontal_rule(#[case] node: Node, #[case] expected: bool) {
        assert_eq!(node.is_horizontal_rule(), expected);
    }

    #[rstest]
    #[case(Node::Blockquote(Value{values: vec!["test".to_string().into()], position: None}), true)]
    #[case(Node::Text(Text{value: "test".to_string(), position: None}), false)]
    fn test_is_blockquote(#[case] node: Node, #[case] expected: bool) {
        assert_eq!(node.is_blockquote(), expected);
    }

    #[rstest]
    #[case(Node::Html(Html{value: "<div>test</div>".to_string(), position: None}), true)]
    #[case(Node::Text(Text{value: "test".to_string(), position: None}), false)]
    fn test_is_html(#[case] node: Node, #[case] expected: bool) {
        assert_eq!(node.is_html(), expected);
    }

    #[rstest]
    #[case(Node::node_values(
           &Node::Strong(Value{values: vec!["test".to_string().into()], position: None})),
           vec!["test".to_string().into()])]
    #[case(Node::node_values(
           &Node::Text(Text{value: "test".to_string(), position: None})),
           vec!["test".to_string().into()])]
    #[case(Node::node_values(
           &Node::Blockquote(Value{values: vec!["test".to_string().into()], position: None})),
           vec!["test".to_string().into()])]
    #[case(Node::node_values(
           &Node::Delete(Value{values: vec!["test".to_string().into()], position: None})),
           vec!["test".to_string().into()])]
    #[case(Node::node_values(
           &Node::Emphasis(Value{values: vec!["test".to_string().into()], position: None})),
           vec!["test".to_string().into()])]
    #[case(Node::node_values(
           &Node::Heading(Heading{depth: 1, values: vec!["test".to_string().into()], position: None})),
           vec!["test".to_string().into()])]
    #[case(Node::node_values(
           &Node::List(List{values: vec!["test".to_string().into()], level: 1, checked: Some(false), index: 0, position: None})),
           vec!["test".to_string().into()])]
    fn test_node_value(#[case] actual: Vec<Node>, #[case] expected: Vec<Node>) {
        assert_eq!(actual, expected);
    }

    #[rstest]
    #[case(Node::Footnote(Footnote{ident: "test".to_string(), values: Vec::new(), position: None}), true)]
    #[case(Node::Text(Text{value: "test".to_string(), position: None}), false)]
    fn test_is_footnote(#[case] node: Node, #[case] expected: bool) {
        assert_eq!(node.is_footnote(), expected);
    }

    #[rstest]
    #[case(Node::FootnoteRef(FootnoteRef{ident: "test".to_string(), label: None, position: None}), true)]
    #[case(Node::Text(Text{value: "test".to_string(), position: None}), false)]
    fn test_is_footnote_ref(#[case] node: Node, #[case] expected: bool) {
        assert_eq!(node.is_footnote_ref(), expected);
    }

    #[rstest]
    #[case(Node::Math(Math{value: "x^2".to_string(), position: None}), true)]
    #[case(Node::Text(Text{value: "test".to_string(), position: None}), false)]
    fn test_is_math(#[case] node: Node, #[case] expected: bool) {
        assert_eq!(node.is_math(), expected);
    }

    #[rstest]
    #[case(Node::Break{position: None}, true)]
    #[case(Node::Text(Text{value: "test".to_string(), position: None}), false)]
    fn test_is_break(#[case] node: Node, #[case] expected: bool) {
        assert_eq!(node.is_break(), expected);
    }

    #[rstest]
    #[case(Node::Yaml(Yaml{value: "key: value".to_string(), position: None}), true)]
    #[case(Node::Text(Text{value: "test".to_string(), position: None}), false)]
    fn test_is_yaml(#[case] node: Node, #[case] expected: bool) {
        assert_eq!(node.is_yaml(), expected);
    }

    #[rstest]
    #[case(Node::Toml(Toml{value: "key = \"value\"".to_string(), position: None}), true)]
    #[case(Node::Text(Text{value: "test".to_string(), position: None}), false)]
    fn test_is_toml(#[case] node: Node, #[case] expected: bool) {
        assert_eq!(node.is_toml(), expected);
    }

    #[rstest]
    #[case(Node::Definition(Definition{ident: "ident".to_string(), url: Url::new("url".to_string()), title: None, label: None, position: None}), true)]
    #[case(Node::Text(Text{value: "test".to_string(), position: None}), false)]
    fn test_is_definition(#[case] node: Node, #[case] expected: bool) {
        assert_eq!(node.is_definition(), expected);
    }

    #[rstest]
    #[case(Node::Emphasis(Value{values: vec!["test".to_string().into()], position: None}), true)]
    #[case(Node::Text(Text{value: "test".to_string(), position: None}), false)]
    fn test_is_emphasis(#[case] node: Node, #[case] expected: bool) {
        assert_eq!(node.is_emphasis(), expected);
    }

    #[rstest]
    #[case(Node::MdxFlowExpression(MdxFlowExpression{value: "test".into(), position: None}), true)]
    #[case(Node::Text(Text{value: "test".to_string(), position: None}), false)]
    fn test_is_mdx_flow_expression(#[case] node: Node, #[case] expected: bool) {
        assert_eq!(node.is_mdx_flow_expression(), expected);
    }

    #[rstest]
    #[case(Node::MdxTextExpression(MdxTextExpression{value: "test".into(), position: None}), true)]
    #[case(Node::Text(Text{value: "test".to_string(), position: None}), false)]
    fn test_is_mdx_text_expression(#[case] node: Node, #[case] expected: bool) {
        assert_eq!(node.is_mdx_text_expression(), expected);
    }

    #[rstest]
    #[case(Node::MdxJsxFlowElement(MdxJsxFlowElement{name: None, attributes: Vec::new(), children: Vec::new(), position: None}), true)]
    #[case(Node::Text(Text{value: "test".to_string(), position: None}), false)]
    fn test_is_mdx_jsx_flow_element(#[case] node: Node, #[case] expected: bool) {
        assert_eq!(node.is_mdx_jsx_flow_element(), expected);
    }

    #[rstest]
    #[case(Node::MdxJsxTextElement(MdxJsxTextElement{name: None, attributes: Vec::new(), children: Vec::new(), position: None}), true)]
    #[case(Node::Text(Text{value: "test".to_string(), position: None}), false)]
    fn test_is_mdx_jsx_text_element(#[case] node: Node, #[case] expected: bool) {
        assert_eq!(node.is_mdx_jsx_text_element(), expected);
    }

    #[rstest]
    #[case(Node::MdxJsEsm(MdxJsEsm{value: "test".into(), position: None}), true)]
    #[case(Node::Text(Text{value: "test".to_string(), position: None}), false)]
    fn test_is_msx_js_esm(#[case] node: Node, #[case] expected: bool) {
        assert_eq!(node.is_msx_js_esm(), expected);
    }

    #[rstest]
    #[case::text(Node::Text(Text{value: "test".to_string(), position: None }), RenderOptions::default(), "test")]
    #[case::list(Node::List(List{index: 0, level: 2, checked: None, values: vec!["test".to_string().into()], position: None}), RenderOptions::default(), "    - test")]
    #[case::list(Node::List(List{index: 0, level: 1, checked: None, values: vec!["test".to_string().into()], position: None}), RenderOptions { list_style: ListStyle::Plus, ..Default::default() }, "  + test")]
    #[case::list(Node::List(List{index: 0, level: 1, checked: Some(true), values: vec!["test".to_string().into()], position: None}), RenderOptions { list_style: ListStyle::Star, ..Default::default() }, "  * [x] test")]
    #[case::list(Node::List(List{index: 0, level: 1, checked: Some(false), values: vec!["test".to_string().into()], position: None}), RenderOptions::default(), "  - [] test")]
    #[case::table_row(Node::TableRow(TableRow{values: vec![Node::TableCell(TableCell{column: 0, row: 0, last_cell_in_row: false, last_cell_of_in_table: false, values: vec!["test".to_string().into()], position: None})], position: None}), RenderOptions::default(), "|test")]
    #[case::table_row(Node::TableRow(TableRow{values: vec![Node::TableCell(TableCell{column: 0, row: 0, last_cell_in_row: true, last_cell_of_in_table: false, values: vec!["test".to_string().into()], position: None})], position: None}), RenderOptions::default(), "|test|")]
    #[case::table_cell(Node::TableCell(TableCell{column: 0, row: 0, last_cell_in_row: false, last_cell_of_in_table: false, values: vec!["test".to_string().into()], position: None}), RenderOptions::default(), "|test")]
    #[case::table_cell(Node::TableCell(TableCell{column: 0, row: 0, last_cell_in_row: true, last_cell_of_in_table: false, values: vec!["test".to_string().into()], position: None}), RenderOptions::default(), "|test|")]
    #[case::table_header(Node::TableHeader(TableHeader{align: vec![TableAlignKind::Left, TableAlignKind::Right, TableAlignKind::Center, TableAlignKind::None], position: None}), RenderOptions::default(), "|:---|---:|:---:|---|")]
    #[case::block_quote(Node::Blockquote(Value{values: vec!["test".to_string().into()], position: None}), RenderOptions::default(), "> test")]
    #[case::block_quote(Node::Blockquote(Value{values: vec!["test\ntest2".to_string().into()], position: None}), RenderOptions::default(), "> test\n> test2")]
    #[case::code(Node::Code(Code{value: "code".to_string(), lang: Some("rust".to_string()), fence: true, meta: None, position: None}), RenderOptions::default(), "```rust\ncode\n```")]
    #[case::code(Node::Code(Code{value: "code".to_string(), lang: None, fence: true, meta: None, position: None}), RenderOptions::default(), "```\ncode\n```")]
    #[case::code(Node::Code(Code{value: "code".to_string(), lang: None, fence: false, meta: None, position: None}), RenderOptions::default(), "    code")]
    #[case::code(Node::Code(Code{value: "code".to_string(), lang: Some("rust".to_string()), fence: true, meta: Some("meta".to_string()), position: None}), RenderOptions::default(), "```rust meta\ncode\n```")]
    #[case::definition(Node::Definition(Definition{ident: "id".to_string(), url: Url::new("url".to_string()), title: None, label: Some("label".to_string()), position: None}), RenderOptions::default(), "[label]: url")]
    #[case::definition(Node::Definition(Definition{ident: "id".to_string(), url: Url::new("url".to_string()), title: Some(Title::new("title".to_string())), label: Some("label".to_string()), position: None}), RenderOptions::default(), "[label]: url \"title\"")]
    #[case::definition(Node::Definition(Definition{ident: "id".to_string(), url: Url::new("".to_string()), title: None, label: Some("label".to_string()), position: None}), RenderOptions::default(), "[label]: <>")]
    #[case::delete(Node::Delete(Value{values: vec!["test".to_string().into()], position: None}), RenderOptions::default(), "~~test~~")]
    #[case::emphasis(Node::Emphasis(Value{values: vec!["test".to_string().into()], position: None}), RenderOptions::default(), "*test*")]
    #[case::footnote(Node::Footnote(Footnote{ident: "id".to_string(), values: vec!["label".to_string().into()], position: None}), RenderOptions::default(), "[^id]: label")]
    #[case::footnote_ref(Node::FootnoteRef(FootnoteRef{ident: "id".to_string(), label: Some("label".to_string()), position: None}), RenderOptions::default(), "[^label]")]
    #[case::heading(Node::Heading(Heading{depth: 1, values: vec!["test".to_string().into()], position: None}), RenderOptions::default(), "# test")]
    #[case::heading(Node::Heading(Heading{depth: 3, values: vec!["test".to_string().into()], position: None}), RenderOptions::default(), "### test")]
    #[case::html(Node::Html(Html{value: "<div>test</div>".to_string(), position: None}), RenderOptions::default(), "<div>test</div>")]
    #[case::image(Node::Image(Image{alt: "alt".to_string(), url: "url".to_string(), title: None, position: None}), RenderOptions::default(), "![alt](url)")]
    #[case::image(Node::Image(Image{alt: "alt".to_string(), url: "url with space".to_string(), title: Some("title".to_string()), position: None}), RenderOptions::default(), "![alt](url%20with%20space \"title\")")]
    #[case::image_ref(Node::ImageRef(ImageRef{alt: "alt".to_string(), ident: "id".to_string(), label: None, position: None}), RenderOptions::default(), "![alt][id]")]
    #[case::code_inline(Node::CodeInline(CodeInline{value: "code".into(), position: None}), RenderOptions::default(), "`code`")]
    #[case::math_inline(Node::MathInline(MathInline{value: "x^2".into(), position: None}), RenderOptions::default(), "$x^2$")]
    #[case::link(Node::Link(Link{url: Url::new("url".to_string()), title: Some(Title::new("title".to_string())), values: vec!["value".to_string().into()], position: None}), RenderOptions::default(), "[value](url \"title\")")]
    #[case::link(Node::Link(Link{url: Url::new("".to_string()), title: None, values: vec!["value".to_string().into()], position: None}), RenderOptions::default(), "[value](<>)")]
    #[case::link(Node::Link(Link{url: Url::new("url".to_string()), title: None, values: vec!["value".to_string().into()], position: None}), RenderOptions::default(), "[value](url)")]
    #[case::link_ref(Node::LinkRef(LinkRef{ident: "id".to_string(), values: vec!["Open".to_string().into()], label: Some("open".to_string()), position: None}), RenderOptions::default(), "[Open][open]")]
    #[case::link_ref(Node::LinkRef(LinkRef{ident: "id".to_string(), values: vec!["open".to_string().into()], label: Some("open".to_string()), position: None}), RenderOptions::default(), "[open]")]
    #[case::math(Node::Math(Math{value: "x^2".to_string(), position: None}), RenderOptions::default(), "$$\nx^2\n$$")]
    #[case::strong(Node::Strong(Value{values: vec!["test".to_string().into()], position: None}), RenderOptions::default(), "**test**")]
    #[case::yaml(Node::Yaml(Yaml{value: "key: value".to_string(), position: None}), RenderOptions::default(), "---\nkey: value\n---")]
    #[case::toml(Node::Toml(Toml{value: "key = \"value\"".to_string(), position: None}), RenderOptions::default(), "+++\nkey = \"value\"\n+++")]
    #[case::break_(Node::Break{position: None}, RenderOptions::default(), "\\")]
    #[case::horizontal_rule(Node::HorizontalRule{position: None}, RenderOptions::default(), "---")]
    #[case::mdx_jsx_flow_element(Node::MdxJsxFlowElement(MdxJsxFlowElement{
        name: Some("div".to_string()),
        attributes: vec![
            MdxAttributeContent::Property(MdxJsxAttribute {
                name: "className".into(),
                value: Some(MdxAttributeValue::Literal("container".into()))
            })
        ],
        children: vec![
            "content".to_string().into()
        ],
        position: None
    }), RenderOptions::default(), "<div className=\"container\">content</div>")]
    #[case::mdx_jsx_flow_element(Node::MdxJsxFlowElement(MdxJsxFlowElement{
        name: Some("div".to_string()),
        attributes: vec![
            MdxAttributeContent::Property(MdxJsxAttribute {
                name: "className".into(),
                value: Some(MdxAttributeValue::Literal("container".into()))
            })
        ],
        children: Vec::new(),
        position: None
    }), RenderOptions::default(), "<div className=\"container\" />")]
    #[case::mdx_jsx_flow_element(Node::MdxJsxFlowElement(MdxJsxFlowElement{
        name: Some("div".to_string()),
        attributes: Vec::new(),
        children: Vec::new(),
        position: None
    }), RenderOptions::default(), "<div />")]
    #[case::mdx_jsx_text_element(Node::MdxJsxTextElement(MdxJsxTextElement{
        name: Some("span".into()),
        attributes: vec![
            MdxAttributeContent::Expression("...props".into())
        ],
        children: vec![
            "inline".to_string().into()
        ],
        position: None
    }), RenderOptions::default(), "<span {...props}>inline</span>")]
    #[case::mdx_jsx_text_element(Node::MdxJsxTextElement(MdxJsxTextElement{
        name: Some("span".into()),
        attributes: vec![
            MdxAttributeContent::Expression("...props".into())
        ],
        children: vec![
        ],
        position: None
    }), RenderOptions::default(), "<span {...props} />")]
    #[case::mdx_jsx_text_element(Node::MdxJsxTextElement(MdxJsxTextElement{
        name: Some("span".into()),
        attributes: vec![
        ],
        children: vec![
        ],
        position: None
    }), RenderOptions::default(), "<span />")]
    #[case(Node::MdxTextExpression(MdxTextExpression{
        value: "count + 1".into(),
        position: None,
    }), RenderOptions::default(), "{count + 1}")]
    #[case(Node::MdxJsEsm(MdxJsEsm{
        value: "import React from 'react'".into(),
        position: None,
    }), RenderOptions::default(), "import React from 'react'")]
    fn test_to_string_with(
        #[case] node: Node,
        #[case] options: RenderOptions,
        #[case] expected: &str,
    ) {
        assert_eq!(node.to_string_with(&options), expected);
    }

    #[test]
    fn test_node_partial_ord() {
        let node1 = Node::Text(Text {
            value: "test1".to_string(),
            position: Some(Position {
                start: Point { line: 1, column: 1 },
                end: Point { line: 1, column: 5 },
            }),
        });

        let node2 = Node::Text(Text {
            value: "test2".to_string(),
            position: Some(Position {
                start: Point { line: 1, column: 6 },
                end: Point {
                    line: 1,
                    column: 10,
                },
            }),
        });

        let node3 = Node::Text(Text {
            value: "test3".to_string(),
            position: Some(Position {
                start: Point { line: 2, column: 1 },
                end: Point { line: 2, column: 5 },
            }),
        });

        assert_eq!(node1.partial_cmp(&node2), Some(std::cmp::Ordering::Less));
        assert_eq!(node2.partial_cmp(&node1), Some(std::cmp::Ordering::Greater));

        assert_eq!(node1.partial_cmp(&node3), Some(std::cmp::Ordering::Less));
        assert_eq!(node3.partial_cmp(&node1), Some(std::cmp::Ordering::Greater));

        let node4 = Node::Text(Text {
            value: "test4".to_string(),
            position: None,
        });

        assert_eq!(node1.partial_cmp(&node4), Some(std::cmp::Ordering::Less));
        assert_eq!(node4.partial_cmp(&node1), Some(std::cmp::Ordering::Greater));

        let node5 = Node::Text(Text {
            value: "test5".to_string(),
            position: None,
        });

        assert_eq!(node4.partial_cmp(&node5), Some(std::cmp::Ordering::Equal));

        let node6 = Node::Code(Code {
            value: "code".to_string(),
            lang: None,
            fence: true,
            meta: None,
            position: None,
        });

        assert_eq!(node6.partial_cmp(&node4), Some(std::cmp::Ordering::Less));
        assert_eq!(node4.partial_cmp(&node6), Some(std::cmp::Ordering::Greater));
    }

    #[rstest]
    #[case(Node::Blockquote(Value{values: Vec::new(), position: None}), "blockquote")]
    #[case(Node::Break{position: None}, "break")]
    #[case(Node::Definition(Definition{ident: "".to_string(), url: Url::new("".to_string()), title: None, label: None, position: None}), "definition")]
    #[case(Node::Delete(Value{values: Vec::new(), position: None}), "delete")]
    #[case(Node::Heading(Heading{depth: 1, values: Vec::new(), position: None}), "h1")]
    #[case(Node::Heading(Heading{depth: 2, values: Vec::new(), position: None}), "h2")]
    #[case(Node::Heading(Heading{depth: 3, values: Vec::new(), position: None}), "h3")]
    #[case(Node::Heading(Heading{depth: 4, values: Vec::new(), position: None}), "h4")]
    #[case(Node::Heading(Heading{depth: 5, values: Vec::new(), position: None}), "h5")]
    #[case(Node::Heading(Heading{depth: 6, values: Vec::new(), position: None}), "h")]
    #[case(Node::Emphasis(Value{values: Vec::new(), position: None}), "emphasis")]
    #[case(Node::Footnote(Footnote{ident: "".to_string(), values: Vec::new(), position: None}), "footnote")]
    #[case(Node::FootnoteRef(FootnoteRef{ident: "".to_string(), label: None, position: None}), "footnoteref")]
    #[case(Node::Html(Html{value: "".to_string(), position: None}), "html")]
    #[case(Node::Yaml(Yaml{value: "".to_string(), position: None}), "yaml")]
    #[case(Node::Toml(Toml{value: "".to_string(), position: None}), "toml")]
    #[case(Node::Image(Image{alt: "".to_string(), url: "".to_string(), title: None, position: None}), "image")]
    #[case(Node::ImageRef(ImageRef{alt: "".to_string(), ident: "".to_string(), label: None, position: None}), "image_ref")]
    #[case(Node::CodeInline(CodeInline{value: "".into(), position: None}), "code_inline")]
    #[case(Node::MathInline(MathInline{value: "".into(), position: None}), "math_inline")]
    #[case(Node::Link(Link{url: Url::new("".to_string()), title: None, values: Vec::new(), position: None}), "link")]
    #[case(Node::LinkRef(LinkRef{ident: "".to_string(), values: Vec::new(), label: None, position: None}), "link_ref")]
    #[case(Node::Math(Math{value: "".to_string(), position: None}), "math")]
    #[case(Node::List(List{index: 0, level: 0, checked: None, values: Vec::new(), position: None}), "list")]
    #[case(Node::TableHeader(TableHeader{align: Vec::new(), position: None}), "table_header")]
    #[case(Node::TableRow(TableRow{values: Vec::new(), position: None}), "table_row")]
    #[case(Node::TableCell(TableCell{column: 0, row: 0, last_cell_in_row: false, last_cell_of_in_table: false, values: Vec::new(), position: None}), "table_cell")]
    #[case(Node::Code(Code{value: "".to_string(), lang: None, fence: true, meta: None, position: None}), "code")]
    #[case(Node::Strong(Value{values: Vec::new(), position: None}), "strong")]
    #[case(Node::HorizontalRule{position: None}, "Horizontal_rule")]
    #[case(Node::MdxFlowExpression(MdxFlowExpression{value: "".into(), position: None}), "mdx_flow_expression")]
    #[case(Node::MdxJsxFlowElement(MdxJsxFlowElement{name: None, attributes: Vec::new(), children: Vec::new(), position: None}), "mdx_jsx_flow_element")]
    #[case(Node::MdxJsxTextElement(MdxJsxTextElement{name: None, attributes: Vec::new(), children: Vec::new(), position: None}), "mdx_jsx_text_element")]
    #[case(Node::MdxTextExpression(MdxTextExpression{value: "".into(), position: None}), "mdx_text_expression")]
    #[case(Node::MdxJsEsm(MdxJsEsm{value: "".into(), position: None}), "mdx_js_esm")]
    #[case(Node::Text(Text{value: "".to_string(), position: None}), "text")]
    fn test_name(#[case] node: Node, #[case] expected: &str) {
        assert_eq!(node.name(), expected);
    }

    #[rstest]
    #[case(Node::Text(Text{value: "test".to_string(), position: None}), "test")]
    #[case(Node::List(List{index: 0, level: 0, checked: None, values: vec![Node::Text(Text{value: "test".to_string(), position: None})], position: None}), "test")]
    #[case(Node::Blockquote(Value{values: vec![Node::Text(Text{value: "test".to_string(), position: None})], position: None}), "test")]
    #[case(Node::Delete(Value{values: vec![Node::Text(Text{value: "test".to_string(), position: None})], position: None}), "test")]
    #[case(Node::Heading(Heading{depth: 1, values: vec![Node::Text(Text{value: "test".to_string(), position: None})], position: None}), "test")]
    #[case(Node::Emphasis(Value{values: vec![Node::Text(Text{value: "test".to_string(), position: None})], position: None}), "test")]
    #[case(Node::Footnote(Footnote{ident: "test".to_string(), values: vec![Node::Text(Text{value: "test".to_string(), position: None})], position: None}), "test")]
    #[case(Node::FootnoteRef(FootnoteRef{ident: "test".to_string(), label: None, position: None}), "test")]
    #[case(Node::Html(Html{value: "test".to_string(), position: None}), "test")]
    #[case(Node::Yaml(Yaml{value: "test".to_string(), position: None}), "test")]
    #[case(Node::Toml(Toml{value: "test".to_string(), position: None}), "test")]
    #[case(Node::Image(Image{alt: "alt".to_string(), url: "test".to_string(), title: None, position: None}), "test")]
    #[case(Node::ImageRef(ImageRef{alt: "alt".to_string(), ident: "test".to_string(), label: None, position: None}), "test")]
    #[case(Node::CodeInline(CodeInline{value: "test".into(), position: None}), "test")]
    #[case(Node::MathInline(MathInline{value: "test".into(), position: None}), "test")]
    #[case(Node::Link(Link{url: Url::new("test".to_string()), title: None, values: Vec::new(), position: None}), "test")]
    #[case(Node::LinkRef(LinkRef{ident: "test".to_string(), values: Vec::new(), label: None, position: None}), "test")]
    #[case(Node::Math(Math{value: "test".to_string(), position: None}), "test")]
    #[case(Node::Code(Code{value: "test".to_string(), lang: None, fence: true, meta: None, position: None}), "test")]
    #[case(Node::Strong(Value{values: vec![Node::Text(Text{value: "test".to_string(), position: None})], position: None}), "test")]
    #[case(Node::TableCell(TableCell{column: 0, row: 0, last_cell_in_row: false, last_cell_of_in_table: false, values: vec![Node::Text(Text{value: "test".to_string(), position: None})], position: None}), "test")]
    #[case(Node::TableRow(TableRow{values: vec![Node::TableCell(TableCell{column: 0, row: 0, last_cell_in_row: false, last_cell_of_in_table: false, values: vec![Node::Text(Text{value: "test".to_string(), position: None})], position: None})], position: None}), "test")]
    #[case(Node::Break{position: None}, "")]
    #[case(Node::HorizontalRule{position: None}, "")]
    #[case(Node::TableHeader(TableHeader{align: Vec::new(), position: None}), "")]
    #[case(Node::MdxFlowExpression(MdxFlowExpression{value: "test".into(), position: None}), "test")]
    #[case(Node::MdxTextExpression(MdxTextExpression{value: "test".into(), position: None}), "test")]
    #[case(Node::MdxJsEsm(MdxJsEsm{value: "test".into(), position: None}), "test")]
    #[case(Node::MdxJsxFlowElement(MdxJsxFlowElement{name: Some("name".to_string()), attributes: Vec::new(), children: vec![Node::Text(Text{value: "test".to_string(), position: None})],  position: None}), "test")]
    #[case(Node::Definition(Definition{ident: "test".to_string(), url: Url::new("url".to_string()), title: None, label: None, position: None}), "url")]
    #[case(Node::Fragment(Fragment {values: vec![Node::Text(Text{value: "test".to_string(), position: None})]}), "test")]
    fn test_value(#[case] node: Node, #[case] expected: &str) {
        assert_eq!(node.value(), expected);
    }

    #[rstest]
    #[case(Node::Text(Text{value: "test".to_string(), position: None}), None)]
    #[case(Node::Text(Text{value: "test".to_string(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    #[case(Node::List(List{index: 0, level: 0, checked: None, values: Vec::new(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    #[case(Node::Blockquote(Value{values: Vec::new(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    #[case(Node::Delete(Value{values: Vec::new(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    #[case(Node::Heading(Heading{depth: 1, values: Vec::new(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    #[case(Node::Emphasis(Value{values: Vec::new(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    #[case(Node::Footnote(Footnote{ident: "".to_string(), values: Vec::new(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    #[case(Node::FootnoteRef(FootnoteRef{ident: "".to_string(), label: None, position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    #[case(Node::Html(Html{value: "".to_string(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    #[case(Node::Yaml(Yaml{value: "".to_string(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    #[case(Node::Toml(Toml{value: "".to_string(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    #[case(Node::Image(Image{alt: "".to_string(), url: "".to_string(), title: None, position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    #[case(Node::ImageRef(ImageRef{alt: "".to_string(), ident: "".to_string(), label: None, position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    #[case(Node::CodeInline(CodeInline{value: "".into(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    #[case(Node::MathInline(MathInline{value: "".into(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    #[case(Node::Link(Link{url: Url("".to_string()), title: None, values: Vec::new(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    #[case(Node::LinkRef(LinkRef{ident: "".to_string(), values: Vec::new(), label: None, position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    #[case(Node::Math(Math{value: "".to_string(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    #[case(Node::Code(Code{value: "".to_string(), lang: None, fence: true, meta: None, position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    #[case(Node::Strong(Value{values: Vec::new(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    #[case(Node::TableCell(TableCell{column: 0, row: 0, last_cell_in_row: false, last_cell_of_in_table: false, values: Vec::new(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    #[case(Node::TableRow(TableRow{values: Vec::new(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    #[case(Node::TableHeader(TableHeader{align: Vec::new(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    #[case(Node::Break{position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}, Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    #[case(Node::HorizontalRule{position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}, Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    #[case(Node::MdxFlowExpression(MdxFlowExpression{value: "test".into(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    #[case(Node::MdxTextExpression(MdxTextExpression{value: "test".into(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    #[case(Node::MdxJsEsm(MdxJsEsm{value: "test".into(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    #[case(Node::MdxJsxFlowElement(MdxJsxFlowElement{name: Some("div".to_string()), attributes: Vec::new(), children: Vec::new(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    #[case(Node::MdxJsxTextElement(MdxJsxTextElement{name: Some("span".into()), attributes: Vec::new(), children: Vec::new(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    #[case(Node::Definition(Definition{ident: "".to_string(), url: Url("".to_string()), title: None, label: None, position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    #[case(Node::Fragment(Fragment{values: Vec::new()}), None)]
    #[case(Node::Fragment(Fragment{values: vec![
        Node::Text(Text{value: "test1".to_string(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}),
        Node::Text(Text{value: "test2".to_string(), position: Some(Position{start: Point{line: 1, column: 6}, end: Point{line: 1, column: 10}})})
    ]}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 10}}))]
    #[case(Node::Fragment(Fragment{values: vec![
        Node::Text(Text{value: "test".to_string(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}),
        Node::Text(Text{value: "test2".to_string(), position: None})
    ]}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    #[case(Node::Fragment(Fragment{values: vec![
        Node::Text(Text{value: "test".to_string(), position: None}),
        Node::Text(Text{value: "test2".to_string(), position: Some(Position{start: Point{line: 1, column: 6}, end: Point{line: 1, column: 10}})})
    ]}), Some(Position{start: Point{line: 1, column: 6}, end: Point{line: 1, column: 10}}))]
    #[case(Node::Fragment(Fragment{values: vec![
        Node::Text(Text{value: "test".to_string(), position: None}),
        Node::Text(Text{value: "test2".to_string(), position: None})
    ]}), None)]
    #[case(Node::Empty, None)]
    fn test_position(#[case] node: Node, #[case] expected: Option<Position>) {
        assert_eq!(node.position(), expected);
    }

    #[rstest]
    #[case(Node::Blockquote(Value{values: vec![
        Node::Text(Text{value: "first".to_string(), position: None}),
        Node::Text(Text{value: "second".to_string(), position: None})
    ], position: None}), 0, Some(Node::Text(Text{value: "first".to_string(), position: None})))]
    #[case(Node::Blockquote(Value{values: vec![
        Node::Text(Text{value: "first".to_string(), position: None}),
        Node::Text(Text{value: "second".to_string(), position: None})
    ], position: None}), 1, Some(Node::Text(Text{value: "second".to_string(), position: None})))]
    #[case(Node::Blockquote(Value{values: vec![
        Node::Text(Text{value: "first".to_string(), position: None})
    ], position: None}), 1, None)]
    #[case(Node::Delete(Value{values: vec![
        Node::Text(Text{value: "first".to_string(), position: None}),
        Node::Text(Text{value: "second".to_string(), position: None})
    ], position: None}), 0, Some(Node::Text(Text{value: "first".to_string(), position: None})))]
    #[case(Node::Emphasis(Value{values: vec![
        Node::Text(Text{value: "first".to_string(), position: None}),
        Node::Text(Text{value: "second".to_string(), position: None})
    ], position: None}), 1, Some(Node::Text(Text{value: "second".to_string(), position: None})))]
    #[case(Node::Strong(Value{values: vec![
        Node::Text(Text{value: "first".to_string(), position: None})
    ], position: None}), 0, Some(Node::Text(Text{value: "first".to_string(), position: None})))]
    #[case(Node::Heading(Heading{depth: 1, values: vec![
        Node::Text(Text{value: "first".to_string(), position: None}),
        Node::Text(Text{value: "second".to_string(), position: None})
    ], position: None}), 0, Some(Node::Text(Text{value: "first".to_string(), position: None})))]
    #[case(Node::List(List{index: 0, level: 0, checked: None, values: vec![
        Node::Text(Text{value: "first".to_string(), position: None}),
        Node::Text(Text{value: "second".to_string(), position: None})
    ], position: None}), 1, Some(Node::Text(Text{value: "second".to_string(), position: None})))]
    #[case(Node::TableCell(TableCell{column: 0, row: 0, last_cell_in_row: false, last_cell_of_in_table: false, values: vec![
        Node::Text(Text{value: "cell content".to_string(), position: None})
    ], position: None}), 0, Some(Node::Text(Text{value: "cell content".to_string(), position: None})))]
    #[case(Node::TableRow(TableRow{values: vec![
        Node::TableCell(TableCell{column: 0, row: 0, last_cell_in_row: false, last_cell_of_in_table: false, values: Vec::new(), position: None}),
        Node::TableCell(TableCell{column: 1, row: 0, last_cell_in_row: true, last_cell_of_in_table: false, values: Vec::new(), position: None})
    ], position: None}), 1, Some(Node::TableCell(TableCell{column: 1, row: 0, last_cell_in_row: true, last_cell_of_in_table: false, values: Vec::new(), position: None})))]
    #[case(Node::Text(Text{value: "plain text".to_string(), position: None}), 0, None)]
    #[case(Node::Code(Code{value: "code".to_string(), lang: None, fence: true, meta: None, position: None}), 0, None)]
    #[case(Node::Html(Html{value: "<div>".to_string(), position: None}), 0, None)]
    fn test_find_at_index(
        #[case] node: Node,
        #[case] index: usize,
        #[case] expected: Option<Node>,
    ) {
        assert_eq!(node.find_at_index(index), expected);
    }

    #[rstest]
    #[case(Node::Blockquote(Value{values: vec!["test".to_string().into()], position: None}),
           Node::Fragment(Fragment{values: vec!["test".to_string().into()]}))]
    #[case(Node::Delete(Value{values: vec!["test".to_string().into()], position: None}),
           Node::Fragment(Fragment{values: vec!["test".to_string().into()]}))]
    #[case(Node::Heading(Heading{depth: 1, values: vec!["test".to_string().into()], position: None}),
           Node::Fragment(Fragment{values: vec!["test".to_string().into()]}))]
    #[case(Node::Emphasis(Value{values: vec!["test".to_string().into()], position: None}),
           Node::Fragment(Fragment{values: vec!["test".to_string().into()]}))]
    #[case(Node::List(List{index: 0, level: 0, checked: None, values: vec!["test".to_string().into()], position: None}),
           Node::Fragment(Fragment{values: vec!["test".to_string().into()]}))]
    #[case(Node::Strong(Value{values: vec!["test".to_string().into()], position: None}),
           Node::Fragment(Fragment{values: vec!["test".to_string().into()]}))]
    #[case(Node::Link(Link{url: Url("url".to_string()), title: None, values: vec!["test".to_string().into()], position: None}),
           Node::Fragment(Fragment{values: vec!["test".to_string().into()]}))]
    #[case(Node::LinkRef(LinkRef{ident: "id".to_string(), values: vec!["test".to_string().into()], label: None, position: None}),
           Node::Fragment(Fragment{values: vec!["test".to_string().into()]}))]
    #[case(Node::Footnote(Footnote{ident: "id".to_string(), values: vec!["test".to_string().into()], position: None}),
           Node::Fragment(Fragment{values: vec!["test".to_string().into()]}))]
    #[case(Node::TableCell(TableCell{column: 0, row: 0, last_cell_in_row: false, last_cell_of_in_table: false, values: vec!["test".to_string().into()], position: None}),
           Node::Fragment(Fragment{values: vec!["test".to_string().into()]}))]
    #[case(Node::TableRow(TableRow{values: vec!["test".to_string().into()], position: None}),
           Node::Fragment(Fragment{values: vec!["test".to_string().into()]}))]
    #[case(Node::Fragment(Fragment{values: vec!["test".to_string().into()]}),
           Node::Fragment(Fragment{values: vec!["test".to_string().into()]}))]
    #[case(Node::Text(Text{value: "test".to_string(), position: None}),
           Node::Empty)]
    #[case(Node::Code(Code{value: "test".to_string(), lang: None, fence: true, meta: None, position: None}),
           Node::Empty)]
    #[case(Node::Image(Image{alt: "alt".to_string(), url: "url".to_string(), title: None, position: None}),
           Node::Empty)]
    #[case(Node::Empty, Node::Empty)]
    fn test_to_fragment(#[case] node: Node, #[case] expected: Node) {
        assert_eq!(node.to_fragment(), expected);
    }

    #[rstest]
    #[case(
        &mut Node::Blockquote(Value{values: vec![
            Node::Text(Text{value: "old".to_string(), position: None})
        ], position: None}),
        Node::Fragment(Fragment{values: vec![
            Node::Text(Text{value: "new".to_string(), position: None})
        ]}),
        Node::Blockquote(Value{values: vec![
            Node::Text(Text{value: "new".to_string(), position: None})
        ], position: None})
    )]
    #[case(
        &mut Node::Delete(Value{values: vec![
            Node::Text(Text{value: "old".to_string(), position: None})
        ], position: None}),
        Node::Fragment(Fragment{values: vec![
            Node::Text(Text{value: "new".to_string(), position: None})
        ]}),
        Node::Delete(Value{values: vec![
            Node::Text(Text{value: "new".to_string(), position: None})
        ], position: None})
    )]
    #[case(
        &mut Node::Emphasis(Value{values: vec![
            Node::Text(Text{value: "old".to_string(), position: None})
        ], position: None}),
        Node::Fragment(Fragment{values: vec![
            Node::Text(Text{value: "new".to_string(), position: None})
        ]}),
        Node::Emphasis(Value{values: vec![
            Node::Text(Text{value: "new".to_string(), position: None})
        ], position: None})
    )]
    #[case(
        &mut Node::Strong(Value{values: vec![
            Node::Text(Text{value: "old".to_string(), position: None})
        ], position: None}),
        Node::Fragment(Fragment{values: vec![
            Node::Text(Text{value: "new".to_string(), position: None})
        ]}),
        Node::Strong(Value{values: vec![
            Node::Text(Text{value: "new".to_string(), position: None})
        ], position: None})
    )]
    #[case(
        &mut Node::List(List{index: 0, level: 0, checked: None, values: vec![
            Node::Text(Text{value: "old".to_string(), position: None})
        ], position: None}),
        Node::Fragment(Fragment{values: vec![
            Node::Text(Text{value: "new".to_string(), position: None})
        ]}),
        Node::List(List{index: 0, level: 0, checked: None, values: vec![
            Node::Text(Text{value: "new".to_string(), position: None})
        ], position: None})
    )]
    #[case(
        &mut Node::Heading(Heading{depth: 1, values: vec![
            Node::Text(Text{value: "old".to_string(), position: None})
        ], position: None}),
        Node::Fragment(Fragment{values: vec![
            Node::Text(Text{value: "new".to_string(), position: None})
        ]}),
        Node::Heading(Heading{depth: 1, values: vec![
            Node::Text(Text{value: "new".to_string(), position: None})
        ], position: None})
    )]
    #[case(
        &mut Node::Link(Link{url: Url("url".to_string()), title: None, values: vec![
            Node::Text(Text{value: "old".to_string(), position: None})
        ], position: None}),
        Node::Fragment(Fragment{values: vec![
            Node::Text(Text{value: "new".to_string(), position: None})
        ]}),
        Node::Link(Link{url: Url("url".to_string()), title: None, values: vec![
            Node::Text(Text{value: "new".to_string(), position: None})
        ], position: None})
    )]
    #[case(
        &mut Node::LinkRef(LinkRef{ident: "id".to_string(), values: vec![
            Node::Text(Text{value: "old".to_string(), position: None})
        ], label: None, position: None}),
        Node::Fragment(Fragment{values: vec![
            Node::Text(Text{value: "new".to_string(), position: None})
        ]}),
        Node::LinkRef(LinkRef{ident: "id".to_string(), values: vec![
            Node::Text(Text{value: "new".to_string(), position: None})
        ], label: None, position: None})
    )]
    #[case(
        &mut Node::Footnote(Footnote{ident: "id".to_string(), values: vec![
            Node::Text(Text{value: "old".to_string(), position: None})
        ], position: None}),
        Node::Fragment(Fragment{values: vec![
            Node::Text(Text{value: "new".to_string(), position: None})
        ]}),
        Node::Footnote(Footnote{ident: "id".to_string(), values: vec![
            Node::Text(Text{value: "new".to_string(), position: None})
        ], position: None})
    )]
    #[case(
        &mut Node::TableCell(TableCell{column: 0, row: 0, last_cell_in_row: false, last_cell_of_in_table: false, values: vec![
            Node::Text(Text{value: "old".to_string(), position: None})
        ], position: None}),
        Node::Fragment(Fragment{values: vec![
            Node::Text(Text{value: "new".to_string(), position: None})
        ]}),
        Node::TableCell(TableCell{column: 0, row: 0, last_cell_in_row: false, last_cell_of_in_table: false, values: vec![
            Node::Text(Text{value: "new".to_string(), position: None})
        ], position: None})
    )]
    #[case(
        &mut Node::TableRow(TableRow{values: vec![
            Node::TableCell(TableCell{column: 0, row: 0, last_cell_in_row: false, last_cell_of_in_table: false, values: vec![
                Node::Text(Text{value: "old".to_string(), position: None})
            ], position: None})
        ], position: None}),
        Node::Fragment(Fragment{values: vec![
            Node::TableCell(TableCell{column: 0, row: 0, last_cell_in_row: false, last_cell_of_in_table: false, values: vec![
                Node::Text(Text{value: "new".to_string(), position: None})
            ], position: None})
        ]}),
        Node::TableRow(TableRow{values: vec![
            Node::TableCell(TableCell{column: 0, row: 0, last_cell_in_row: false, last_cell_of_in_table: false, values: vec![
                Node::Text(Text{value: "new".to_string(), position: None})
            ], position: None})
        ], position: None})
    )]
    #[case(
        &mut Node::Text(Text{value: "old".to_string(), position: None}),
        Node::Fragment(Fragment{values: vec![
            Node::Text(Text{value: "new".to_string(), position: None})
        ]}),
        Node::Text(Text{value: "old".to_string(), position: None})
    )]
    #[case(
        &mut Node::Blockquote(Value{values: vec![
            Node::Text(Text{value: "text1".to_string(), position: None}),
            Node::Text(Text{value: "text2".to_string(), position: None})
        ], position: None}),
        Node::Fragment(Fragment{values: vec![
            Node::Text(Text{value: "new1".to_string(), position: None}),
            Node::Text(Text{value: "new2".to_string(), position: None})
        ]}),
        Node::Blockquote(Value{values: vec![
            Node::Text(Text{value: "new1".to_string(), position: None}),
            Node::Text(Text{value: "new2".to_string(), position: None})
        ], position: None})
    )]
    #[case(
        &mut Node::Strong(Value{values: vec![
            Node::Text(Text{value: "text1".to_string(), position: None}),
            Node::Text(Text{value: "text2".to_string(), position: None})
        ], position: None}),
        Node::Fragment(Fragment{values: vec![
            Node::Empty,
            Node::Text(Text{value: "new2".to_string(), position: None})
        ]}),
        Node::Strong(Value{values: vec![
            Node::Text(Text{value: "text1".to_string(), position: None}),
            Node::Text(Text{value: "new2".to_string(), position: None})
        ], position: None})
    )]
    #[case(
        &mut Node::List(List{index: 0, level: 0, checked: None, values: vec![
            Node::Text(Text{value: "text1".to_string(), position: None}),
            Node::Text(Text{value: "text2".to_string(), position: None})
        ], position: None}),
        Node::Fragment(Fragment{values: vec![
            Node::Text(Text{value: "new1".to_string(), position: None}),
            Node::Fragment(Fragment{values: Vec::new()})
        ]}),
        Node::List(List{index: 0, level: 0, checked: None, values: vec![
            Node::Text(Text{value: "new1".to_string(), position: None}),
            Node::Text(Text{value: "text2".to_string(), position: None})
        ], position: None})
    )]
    fn test_apply_fragment(
        #[case] node: &mut Node,
        #[case] fragment: Node,
        #[case] expected: Node,
    ) {
        node.apply_fragment(fragment);
        assert_eq!(*node, expected);
    }
}
