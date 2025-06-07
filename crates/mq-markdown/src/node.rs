use std::{
    borrow::Cow,
    fmt::{self, Display},
};

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
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase")
)]
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
            UrlSurroundStyle::None if self.0.is_empty() => "".to_string(),
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
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase")
)]
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
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase")
)]
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
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct List {
    pub values: Vec<Node>,
    pub index: usize,
    pub level: Level,
    pub checked: Option<bool>,
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]

pub struct TableCell {
    pub values: Vec<Node>,
    pub column: usize,
    pub row: usize,
    pub last_cell_in_row: bool,
    pub last_cell_of_in_table: bool,
    #[cfg_attr(feature = "json", serde(skip))]
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct TableRow {
    pub values: Vec<Node>,
    #[cfg_attr(feature = "json", serde(skip))]
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct TableHeader {
    pub align: Vec<TableAlignKind>,
    #[cfg_attr(feature = "json", serde(skip))]
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct Fragment {
    pub values: Vec<Node>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct Code {
    pub value: String,
    pub lang: Option<String>,
    #[cfg_attr(feature = "json", serde(skip))]
    pub position: Option<Position>,
    pub meta: Option<String>,
    pub fence: bool,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct Image {
    pub alt: String,
    pub url: String,
    pub title: Option<String>,
    #[cfg_attr(feature = "json", serde(skip))]
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct ImageRef {
    pub alt: String,
    pub ident: String,
    pub label: Option<String>,
    #[cfg_attr(feature = "json", serde(skip))]
    pub position: Option<Position>,
}
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct Link {
    pub url: Url,
    pub title: Option<Title>,
    pub values: Vec<Node>,
    #[cfg_attr(feature = "json", serde(skip))]
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct FootnoteRef {
    pub ident: String,
    pub label: Option<String>,
    #[cfg_attr(feature = "json", serde(skip))]
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct Footnote {
    pub ident: String,
    pub values: Vec<Node>,
    #[cfg_attr(feature = "json", serde(skip))]
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct LinkRef {
    pub ident: String,
    pub label: Option<String>,
    pub values: Vec<Node>,
    #[cfg_attr(feature = "json", serde(skip))]
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct Heading {
    pub depth: u8,
    pub values: Vec<Node>,
    #[cfg_attr(feature = "json", serde(skip))]
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]

pub struct Definition {
    #[cfg_attr(feature = "json", serde(skip))]
    pub position: Option<Position>,
    pub url: Url,
    pub title: Option<Title>,
    pub ident: String,
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct Text {
    pub value: String,
    #[cfg_attr(feature = "json", serde(skip))]
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]

pub struct Html {
    pub value: String,
    #[cfg_attr(feature = "json", serde(skip))]
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct Toml {
    pub value: String,
    #[cfg_attr(feature = "json", serde(skip))]
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct Yaml {
    pub value: String,
    #[cfg_attr(feature = "json", serde(skip))]
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct CodeInline {
    pub value: CompactString,
    #[cfg_attr(feature = "json", serde(skip))]
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct MathInline {
    pub value: CompactString,
    #[cfg_attr(feature = "json", serde(skip))]
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct Math {
    pub value: String,
    #[cfg_attr(feature = "json", serde(skip))]
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct MdxFlowExpression {
    pub value: CompactString,
    #[cfg_attr(feature = "json", serde(skip))]
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct MdxJsxFlowElement {
    pub children: Vec<Node>,
    #[cfg_attr(feature = "json", serde(skip))]
    pub position: Option<Position>,
    pub name: Option<String>,
    pub attributes: Vec<MdxAttributeContent>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub enum MdxAttributeContent {
    Expression(CompactString),
    Property(MdxJsxAttribute),
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct MdxJsxAttribute {
    pub name: CompactString,
    pub value: Option<MdxAttributeValue>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub enum MdxAttributeValue {
    Expression(CompactString),
    Literal(CompactString),
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct MdxJsxTextElement {
    pub children: Vec<Node>,
    #[cfg_attr(feature = "json", serde(skip))]
    pub position: Option<Position>,
    pub name: Option<CompactString>,
    pub attributes: Vec<MdxAttributeContent>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct MdxTextExpression {
    pub value: CompactString,
    #[cfg_attr(feature = "json", serde(skip))]
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct MdxJsEsm {
    pub value: CompactString,
    #[cfg_attr(feature = "json", serde(skip))]
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct Blockquote {
    pub values: Vec<Node>,
    #[cfg_attr(feature = "json", serde(skip))]
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct Delete {
    pub values: Vec<Node>,
    #[cfg_attr(feature = "json", serde(skip))]
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct Emphasis {
    pub values: Vec<Node>,
    #[cfg_attr(feature = "json", serde(skip))]
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct Strong {
    pub values: Vec<Node>,
    #[cfg_attr(feature = "json", serde(skip))]
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct Break {
    #[cfg_attr(feature = "json", serde(skip))]
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct HorizontalRule {
    #[cfg_attr(feature = "json", serde(skip))]
    pub position: Option<Position>,
}

#[derive(Debug, Clone)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", untagged)
)]
pub enum Node {
    Blockquote(Blockquote),
    Break(Break),
    Definition(Definition),
    Delete(Delete),
    Heading(Heading),
    Emphasis(Emphasis),
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
    Strong(Strong),
    HorizontalRule(HorizontalRule),
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
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase")
)]
pub struct Position {
    pub start: Point,
    pub end: Point,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase")
)]
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

impl From<&str> for Node {
    fn from(value: &str) -> Self {
        Self::Text(Text {
            value: value.to_string(),
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
            | Node::Blockquote(Blockquote { values, .. })
            | Node::Delete(Delete { values, .. })
            | Node::Emphasis(Emphasis { values, .. })
            | Node::Strong(Strong { values, .. }) => Self::Fragment(Fragment { values }),
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
            | Node::Blockquote(Blockquote { values, .. })
            | Node::Delete(Delete { values, .. })
            | Node::Emphasis(Emphasis { values, .. })
            | Node::Strong(Strong { values, .. }) => {
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
        match self { // Changed self.clone() to self
            Self::List(List {
                level,
                checked,
                values,
                ..
            }) => {
                format!(
                    "{}{} {}{}",
                    "  ".repeat(*level as usize), // Dereference level
                    options.list_style,
                    checked // This is Option<bool>, which is Copy
                        .map(|it| if it { "[x] " } else { "[ ] " })
                        .unwrap_or_else(|| ""),
                    Self::values_to_string(values.iter(), options) // Pass iterator
                )
            }
            Self::TableRow(TableRow { values, .. }) => values // values is &Vec<Node>
                .iter() // Iterates to &Node
                .map(|cell| cell.to_string_with(options)) // cell is &Node
                .collect::<String>(),
            Self::TableCell(TableCell {
                last_cell_in_row, // &bool
                last_cell_of_in_table, // &bool
                values, // &Vec<Node>
                ..
            }) => {
                if *last_cell_in_row || *last_cell_of_in_table { // Dereference bools
                    format!("|{}|", Self::values_to_string(values.iter(), options)) // Pass iterator
                } else {
                    format!("|{}", Self::values_to_string(values.iter(), options)) // Pass iterator
                }
            }
            Self::TableHeader(TableHeader { align, .. }) => { // align is &Vec<TableAlignKind>
                format!("|{}|", align.iter().map(|a| a.to_string()).join("|"))
            }
            Self::Blockquote(Blockquote { values, .. }) => Self::values_to_string(values.iter(), options) // Pass iterator
                .split('\n')
                .map(|line| format!("> {}", line))
                .join("\n"),
            Self::Code(Code {
                value, // &String
                lang,  // &Option<String>
                fence, // &bool
                meta,  // &Option<String>
                ..
            }) => {
                let meta_str = meta // meta is &Option<String>
                    .as_deref()
                    .map(|m| format!(" {}", m))
                    .unwrap_or_default();

                match lang { // lang is &Option<String>
                    Some(l) => format!("```{}{}\n{}\n```", l, meta_str, value),
                    None if *fence => { // Dereference fence
                        format!("```{}\n{}\n```", lang.as_deref().unwrap_or(""), value)
                    }
                    None => value.lines().map(|line| format!("    {}", line)).join("\n"),
                }
            }
            Self::Definition(Definition { // All fields are references
                ident,
                label,
                url,
                title,
                ..
            }) => {
                format!(
                    "[{}]: {}{}",
                    label.as_ref().unwrap_or(ident), // label is &Option<String>, ident is &String
                    url.to_string_with(options), // url is &Url, to_string_with takes &self
                    title
                        .as_ref() // title is &Option<Title>
                        .map(|t| format!(" {}", t.to_string_with(options))) // t is &Title
                        .unwrap_or_default()
                )
            }
            Self::Delete(Delete { values, .. }) => { // values is &Vec<Node>
                format!("~~{}~~", Self::values_to_string(values.iter(), options))
            }
            Self::Emphasis(Emphasis { values, .. }) => { // values is &Vec<Node>
                format!("*{}*", Self::values_to_string(values.iter(), options))
            }
            Self::Footnote(Footnote { values, ident, .. }) => { // values is &Vec<Node>, ident is &String
                format!("[^{}]: {}", ident, Self::values_to_string(values.iter(), options))
            }
            Self::FootnoteRef(FootnoteRef { label, .. }) => { // label is &Option<String>
                format!("[^{}]", label.as_ref().unwrap_or(&String::new())) // Provide default if None
            }
            Self::Heading(Heading { depth, values, .. }) => { // depth is &u8, values is &Vec<Node>
                format!(
                    "{} {}",
                    "#".repeat(*depth as usize), // Dereference depth
                    Self::values_to_string(values.iter(), options)
                )
            }
            Self::Html(Html { value, .. }) => value.clone(), // value is &String, clone it
            Self::Image(Image { // All fields are references
                alt, url, title, ..
            }) => format!(
                "![{}]({}{})",
                alt, // &String
                url.replace(' ', "%20"), // &String
                title.as_ref().map(|it| format!(" \"{}\"", it)).unwrap_or_default() // title is &Option<String>
            ),
            Self::ImageRef(ImageRef { // All fields are references
                alt, ident, label, ..
            }) => {
                if alt == ident { // Both &String
                    format!("![{}]", ident)
                } else {
                    format!("![{}][{}]", alt, label.as_ref().unwrap_or(ident)) // label is &Option<String>
                }
            }
            Self::CodeInline(CodeInline { value, .. }) => { // value is &CompactString
                format!("`{}`", value)
            }
            Self::MathInline(MathInline { value, .. }) => { // value is &CompactString
                format!("${}$", value)
            }
            Self::Link(Link { // All fields are references
                url, title, values, ..
            }) => {
                format!(
                    "[{}]({}{})",
                    Self::values_to_string(values.iter(), options), // values is &Vec<Node>
                    url.to_string_with(options), // url is &Url
                    title // title is &Option<Title>
                        .as_ref()
                        .map(|t| format!(" {}", t.to_string_with(options))) // t is &Title
                        .unwrap_or_default(),
                )
            }
            Self::LinkRef(LinkRef { values, label, .. }) => { // values is &Vec<Node>, label is &Option<String>
                let ident_str = Self::values_to_string(values.iter(), options); // ident_str is String

                label // label is &Option<String>
                    .as_ref()
                    .map(|lbl_str| { // lbl_str is &String
                        if lbl_str == &ident_str {
                            format!("[{}]", ident_str)
                        } else {
                            format!("[{}][{}]", ident_str, lbl_str)
                        }
                    })
                    .unwrap_or(format!("[{}]", ident_str))
            }
            Self::Math(Math { value, .. }) => format!("$$\n{}\n$$", value), // value is &String
            Self::Text(Text { value, .. }) => value.clone(), // value is &String
            Self::MdxFlowExpression(mdx_flow_expression) => { // mdx_flow_expression is &MdxFlowExpression
                format!("{{{}}}", mdx_flow_expression.value) // value is CompactString (Copy or Clone)
            }
            Self::MdxJsxFlowElement(mdx_jsx_flow_element) => { // mdx_jsx_flow_element is &MdxJsxFlowElement
                let name = mdx_jsx_flow_element.name.as_ref().unwrap_or(&String::new()); // name is &Option<String>
                let attributes = if mdx_jsx_flow_element.attributes.is_empty() { // attributes is &Vec<MdxAttributeContent>
                    "".to_string()
                } else {
                    format!(
                        " {}",
                        mdx_jsx_flow_element
                            .attributes // &Vec<MdxAttributeContent>
                            .iter() // Iterator of &MdxAttributeContent
                            .map(Self::mdx_attribute_content_to_string) // Expects &MdxAttributeContent
                            .join(" ")
                    )
                };

                if mdx_jsx_flow_element.children.is_empty() { // children is &Vec<Node>
                    format!("<{}{} />", name, attributes,)
                } else {
                    format!(
                        "<{}{}>{}</{}>",
                        name,
                        attributes,
                        Self::values_to_string(mdx_jsx_flow_element.children.iter(), options), // children.iter() is Iterator of &Node
                        name
                    )
                }
            }
            Self::MdxJsxTextElement(mdx_jsx_text_element) => { // mdx_jsx_text_element is &MdxJsxTextElement
                let name = mdx_jsx_text_element.name.as_ref().unwrap_or(&CompactString::new("")); // name is &Option<CompactString>
                let attributes = if mdx_jsx_text_element.attributes.is_empty() { // attributes is &Vec<MdxAttributeContent>
                    "".to_string()
                } else {
                    format!(
                        " {}",
                        mdx_jsx_text_element
                            .attributes // &Vec<MdxAttributeContent>
                            .iter() // Iterator of &MdxAttributeContent
                            .map(Self::mdx_attribute_content_to_string) // Expects &MdxAttributeContent
                            .join(" ")
                    )
                };

                if mdx_jsx_text_element.children.is_empty() { // children is &Vec<Node>
                    format!("<{}{} />", name, attributes,)
                } else {
                    format!(
                        "<{}{}>{}</{}>",
                        name,
                        attributes,
                        Self::values_to_string(mdx_jsx_text_element.children.iter(), options), // children.iter() is Iterator of &Node
                        name
                    )
                }
            }
            Self::MdxTextExpression(mdx_text_expression) => { // mdx_text_expression is &MdxTextExpression
                format!("{{{}}}", mdx_text_expression.value) // value is CompactString
            }
            Self::MdxJsEsm(mdxjs_esm) => mdxjs_esm.value.to_string(), // value is CompactString
            Self::Strong(Strong { values, .. }) => { // values is &Vec<Node>
                format!(
                    "**{}**",
                    values // &Vec<Node>
                        .iter() // Iterates to &Node
                        .map(|value_ref| value_ref.to_string_with(options)) // value_ref is &Node
                        .collect::<String>()
                )
            }
            Self::Yaml(Yaml { value, .. }) => format!("---\n{}\n---", value), // value is &String
            Self::Toml(Toml { value, .. }) => format!("+++\n{}\n+++", value), // value is &String
            Self::Break(_) => "\\".to_string(),
            Self::HorizontalRule(_) => "---".to_string(),
            Self::Fragment(Fragment { values }) => values // values is &Vec<Node>
                .iter() // Iterates to &Node
                .map(|value_ref| value_ref.to_string_with(options)) // value_ref is &Node
                .collect::<String>(),
            Self::Empty => String::new(),
        }
    }

    pub fn node_values_ref(&self) -> Vec<&Node> { // Renamed from node_values
        match self { // Changed self.clone() to self
            Self::Blockquote(v) => v.values.iter().collect(),
            Self::Delete(v) => v.values.iter().collect(),
            Self::Heading(h) => h.values.iter().collect(),
            Self::Emphasis(v) => v.values.iter().collect(),
            Self::List(l) => l.values.iter().collect(),
            Self::Strong(v) => v.values.iter().collect(),
            Self::Link(l) => l.values.iter().collect(),
            Self::Footnote(f) => f.values.iter().collect(),
            Self::LinkRef(lr) => lr.values.iter().collect(),
            Self::TableCell(tc) => tc.values.iter().collect(),
            Self::TableRow(tr) => tr.values.iter().collect(),
            Self::MdxJsxFlowElement(m) => m.children.iter().collect(),
            Self::MdxJsxTextElement(m) => m.children.iter().collect(),
            Self::Fragment(f) => f.values.iter().collect(),
            _ => vec![self], // self is already &Node, vec! expects Node, so this is correct for &Node
        }
    }

    pub fn find_at_index(&self, index: usize) -> Option<&Node> {
        match self {
            Self::Blockquote(v) => v.values.get(index),
            Self::Delete(v) => v.values.get(index),
            Self::Emphasis(v) => v.values.get(index),
            Self::Strong(v) => v.values.get(index),
            Self::Heading(v) => v.values.get(index),
            Self::List(v) => v.values.get(index),
            Self::TableCell(v) => v.values.get(index),
            Self::TableRow(v) => v.values.get(index),
            Self::Link(v) => v.values.get(index),
            Self::Footnote(v) => v.values.get(index),
            Self::LinkRef(v) => v.values.get(index),
            Self::MdxJsxFlowElement(v) => v.children.get(index),
            Self::MdxJsxTextElement(v) => v.children.get(index),
            Self::Fragment(v) => v.values.get(index),
            _ => None,
        }
    }

    pub fn value(&self) -> Cow<str> {
        match self {
            Self::Blockquote(v) => Cow::Owned(Self::values_to_value(v.values.iter())),
            Self::Definition(d) => Cow::Borrowed(d.url.as_str()),
            Self::Delete(v) => Cow::Owned(Self::values_to_value(v.values.iter())),
            Self::Heading(h) => Cow::Owned(Self::values_to_value(h.values.iter())),
            Self::Emphasis(v) => Cow::Owned(Self::values_to_value(v.values.iter())),
            Self::Footnote(f) => Cow::Owned(Self::values_to_value(f.values.iter())),
            Self::FootnoteRef(f) => Cow::Borrowed(&f.ident),
            Self::Html(v) => Cow::Borrowed(&v.value),
            Self::Yaml(v) => Cow::Borrowed(&v.value),
            Self::Toml(v) => Cow::Borrowed(&v.value),
            Self::Image(i) => Cow::Borrowed(&i.url),
            Self::ImageRef(i) => Cow::Borrowed(&i.ident),
            Self::CodeInline(v) => Cow::Borrowed(v.value.as_str()),
            Self::MathInline(v) => Cow::Borrowed(v.value.as_str()),
            Self::Link(l) => Cow::Borrowed(l.url.as_str()),
            Self::LinkRef(l) => Cow::Borrowed(&l.ident),
            Self::Math(v) => Cow::Borrowed(&v.value),
            Self::List(l) => Cow::Owned(Self::values_to_value(l.values.iter())),
            Self::TableCell(c) => Cow::Owned(Self::values_to_value(c.values.iter())),
            Self::TableRow(c) => Cow::Owned(Self::values_to_value(c.values.iter())),
            Self::Code(c) => Cow::Borrowed(&c.value),
            Self::Strong(v) => Cow::Owned(Self::values_to_value(v.values.iter())),
            Self::Text(t) => Cow::Borrowed(&t.value),
            Self::MdxFlowExpression(mdx) => Cow::Borrowed(mdx.value.as_str()),
            Self::MdxJsxFlowElement(mdx) => Cow::Owned(Self::values_to_value(mdx.children.iter())),
            Self::MdxTextExpression(mdx) => Cow::Borrowed(mdx.value.as_str()),
            Self::MdxJsxTextElement(mdx) => Cow::Owned(Self::values_to_value(mdx.children.iter())),
            Self::MdxJsEsm(mdx) => Cow::Borrowed(mdx.value.as_str()),
            Self::Break { .. }
            | Self::TableHeader(_)
            | Self::HorizontalRule { .. }
            | Self::Empty => Cow::Borrowed(""),
            Self::Fragment(v) => Cow::Owned(Self::values_to_value(v.values.iter())),
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
                6 => "h6".into(),
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

    pub fn set_position(&mut self, pos: Option<Position>) {
        match self {
            Self::Blockquote(v) => v.position = pos,
            Self::Definition(d) => d.position = pos,
            Self::Delete(v) => v.position = pos,
            Self::Heading(h) => h.position = pos,
            Self::Emphasis(v) => v.position = pos,
            Self::Footnote(f) => f.position = pos,
            Self::FootnoteRef(f) => f.position = pos,
            Self::Html(v) => v.position = pos,
            Self::Yaml(v) => v.position = pos,
            Self::Toml(v) => v.position = pos,
            Self::Image(i) => i.position = pos,
            Self::ImageRef(i) => i.position = pos,
            Self::CodeInline(v) => v.position = pos,
            Self::MathInline(v) => v.position = pos,
            Self::Link(l) => l.position = pos,
            Self::LinkRef(l) => l.position = pos,
            Self::Math(v) => v.position = pos,
            Self::Code(c) => c.position = pos,
            Self::TableCell(c) => c.position = pos,
            Self::TableRow(r) => r.position = pos,
            Self::TableHeader(c) => c.position = pos,
            Self::List(l) => l.position = pos,
            Self::Strong(s) => s.position = pos,
            Self::MdxFlowExpression(m) => m.position = pos,
            Self::MdxTextExpression(m) => m.position = pos,
            Self::MdxJsEsm(m) => m.position = pos,
            Self::MdxJsxFlowElement(m) => m.position = pos,
            Self::MdxJsxTextElement(m) => m.position = pos,
            Self::Break(b) => b.position = pos,
            Self::HorizontalRule(h) => h.position = pos,
            Self::Text(t) => t.position = pos,
            Self::Fragment(_) | Self::Empty => {}
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
            Self::Break(b) => b.position.clone(),
            Self::Text(t) => t.position.clone(),
            Self::HorizontalRule(h) => h.position.clone(),
            Self::Fragment(v) => {
                let positions: Vec<Position> =
                    v.values.iter().filter_map(|node| node.position()).collect();

                match (positions.first(), positions.last()) {
                    (Some(start), Some(end)) => Some(Position {
                        start: start.start.clone(),
                        end: end.end.clone(),
                    }),
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

    pub fn is_empty_fragment(&self) -> bool {
        if let Self::Fragment(_) = self {
            Self::_fragment_inner_nodes(self).is_empty()
        } else {
            false
        }
    }

    fn _fragment_inner_nodes(node: &Node) -> Vec<Node> {
        if let Self::Fragment(fragment) = node {
            fragment
                .values
                .iter()
                .flat_map(Self::_fragment_inner_nodes)
                .collect()
        } else {
            vec![node.clone()]
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
                image.label = Some(value.to_string());
                Self::ImageRef(image)
            }
            Self::Link(mut link) => {
                link.url = Url(value.to_string());
                Self::Link(link)
            }
            Self::LinkRef(mut v) => {
                v.label = Some(value.to_string());
                v.ident = value.to_string();
                Self::LinkRef(v)
            }
            Self::Footnote(mut footnote) => {
                footnote.ident = value.to_string();
                Self::Footnote(footnote)
            }
            Self::FootnoteRef(mut footnote) => {
                footnote.ident = value.to_string();
                footnote.label = Some(value.to_string());
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
                vec![Self::Blockquote(Blockquote {
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
                vec![Self::Break(Break {
                    position: position.map(|p| p.clone().into()),
                })]
            }
            mdast::Node::Delete(mdast::Delete { position, .. }) => {
                vec![Self::Delete(Delete {
                    values: Self::mdast_children_to_node(node),
                    position: position.map(|p| p.clone().into()),
                })]
            }
            mdast::Node::Emphasis(mdast::Emphasis { position, .. }) => {
                vec![Self::Emphasis(Emphasis {
                    values: Self::mdast_children_to_node(node),
                    position: position.map(|p| p.clone().into()),
                })]
            }
            mdast::Node::Strong(mdast::Strong { position, .. }) => {
                vec![Self::Strong(Strong {
                    values: Self::mdast_children_to_node(node),
                    position: position.map(|p| p.clone().into()),
                })]
            }
            mdast::Node::ThematicBreak(mdast::ThematicBreak { position, .. }) => {
                vec![Self::HorizontalRule(HorizontalRule {
                    position: position.map(|p| p.clone().into()),
                })]
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

    fn mdx_attribute_content_to_string(attr: &MdxAttributeContent) -> CompactString { // Changed to take reference
        match attr {
            MdxAttributeContent::Expression(value) => format!("{{{}}}", value).into(), // value is &CompactString
            MdxAttributeContent::Property(property) => match &property.value { // property is &MdxJsxAttribute, property.value is &Option<MdxAttributeValue>
                Some(val) => match val { // val is &MdxAttributeValue
                    MdxAttributeValue::Expression(expr_val) => { // expr_val is &CompactString
                        format!("{}={{{}}}", property.name, expr_val).into()
                    }
                    MdxAttributeValue::Literal(literal) => { // literal is &CompactString
                        format!("{}=\"{}\"", property.name, literal).into()
                    }
                },
                None => property.name.clone(), // property.name is CompactString, clone it
            },
        }
    }

    #[inline(always)]
    fn values_to_string<'a>(
        values: impl IntoIterator<Item = &'a Node>,
        options: &RenderOptions,
    ) -> String {
        let mut pre_position: Option<Position> = None;
        values
            .into_iter() // values is now an iterator of &'a Node
            .map(|value_ref| { // value_ref is &'a Node
                if let Some(pos) = value_ref.position() {
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
                            value_ref.to_string_with(options) // Use value_ref
                        )
                    } else {
                        format!(
                            "{}{}",
                            "\n".repeat(new_line_count),
                            value_ref // Use value_ref
                                .to_string_with(options)
                                .lines()
                                .map(|line| format!("{}{}", space, line))
                                .join("\n")
                        )
                    }
                } else {
                    pre_position = None;
                    value_ref.to_string_with(options) // Use value_ref
                }
            })
            .collect::<String>()
    }

    #[inline(always)]
    fn values_to_value<'a>(values: impl IntoIterator<Item = &'a Node>) -> String {
        values
            .into_iter()
            .map(|value_ref| value_ref.value()) // value_ref is &Node, value() returns Cow<str>
            .collect::<String>() // Collects Vec<Cow<str>> into String
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
    #[case::blockquote(Node::Blockquote(Blockquote{values: vec!["test".to_string().into()], position: None }),
           "test".to_string(),
           Node::Blockquote(Blockquote{values: vec!["test".to_string().into()], position: None }))]
    #[case::delete(Node::Delete(Delete{values: vec!["test".to_string().into()], position: None }),
           "test".to_string(),
           Node::Delete(Delete{values: vec!["test".to_string().into()], position: None }))]
    #[case::emphasis(Node::Emphasis(Emphasis{values: vec!["test".to_string().into()], position: None }),
           "test".to_string(),
           Node::Emphasis(Emphasis{values: vec!["test".to_string().into()], position: None }))]
    #[case::strong(Node::Strong(Strong{values: vec!["test".to_string().into()], position: None }),
           "test".to_string(),
           Node::Strong(Strong{values: vec!["test".to_string().into()], position: None }))]
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
    #[case::footnote_ref(Node::FootnoteRef(FootnoteRef {ident: "test".to_string(), label: None, position: None }),
           "test".to_string(),
           Node::FootnoteRef(FootnoteRef{ident: "test".to_string(), label: Some("test".to_string()), position: None }))]
    #[case::footnote(Node::Footnote(Footnote {ident: "test".to_string(), values: Vec::new(), position: None }),
           "test".to_string(),
           Node::Footnote(Footnote{ident: "test".to_string(), values: Vec::new(), position: None }))]
    #[case::list(Node::List(List{index: 0, level: 0, checked: None, values: vec!["test".to_string().into()], position: None}),
           "test".to_string(),
           Node::List(List{index: 0, level: 0, checked: None, values: vec!["test".to_string().into()], position: None }))]
    #[case::list(Node::List(List{index: 1, level: 1, checked: Some(true), values: vec!["test".to_string().into()], position: None}),
           "test".to_string(),
           Node::List(List{index: 1, level: 1, checked: Some(true), values: vec!["test".to_string().into()], position: None }))]
    #[case::list(Node::List(List{index: 2, level: 2, checked: Some(false), values: vec!["test".to_string().into()], position: None}),
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
    #[case::link_ref(Node::LinkRef(LinkRef{ident: "test2".to_string(), values: vec!["value".to_string().into()], label: Some("test2".to_string()), position: None}),
            "test2".to_string(),
            Node::LinkRef(LinkRef{ident: "test2".to_string(), values: vec!["value".to_string().into()], label: Some("test2".to_string()), position: None}),)]
    #[case::image_ref(Node::ImageRef(ImageRef{alt: "alt".to_string(), ident: "test1".to_string(), label: None, position: None}),
            "test2".to_string(),
            Node::ImageRef(ImageRef{alt: "alt".to_string(), ident: "test2".to_string(), label: Some("test2".to_string()), position: None}),)]
    #[case::definition(Node::Definition(Definition{ url: Url::new("url".to_string()), title: None, ident: "test1".to_string(), label: None, position: None}),
            "test2".to_string(),
            Node::Definition(Definition{url: Url::new("test2".to_string()), title: None, ident: "test1".to_string(), label: None, position: None}),)]
    #[case::break_(Node::Break(Break{ position: None}),
            "test".to_string(),
            Node::Break(Break{position: None}))]
    #[case::horizontal_rule(Node::HorizontalRule(HorizontalRule{ position: None}),
            "test".to_string(),
            Node::HorizontalRule(HorizontalRule{position: None}))]
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
    #[case(Node::Blockquote(Blockquote{values: vec![
        Node::Text(Text{value: "first".to_string(), position: None}),
        Node::Text(Text{value: "second".to_string(), position: None})
    ], position: None}),
        "new",
        0,
        Node::Blockquote(Blockquote{values: vec![
            Node::Text(Text{value: "new".to_string(), position: None}),
            Node::Text(Text{value: "second".to_string(), position: None})
        ], position: None}))]
    #[case(Node::Blockquote(Blockquote{values: vec![
        Node::Text(Text{value: "first".to_string(), position: None}),
        Node::Text(Text{value: "second".to_string(), position: None})
    ], position: None}),
        "new",
        1,
        Node::Blockquote(Blockquote{values: vec![
            Node::Text(Text{value: "first".to_string(), position: None}),
            Node::Text(Text{value: "new".to_string(), position: None})
        ], position: None}))]
    #[case(Node::Delete(Delete{values: vec![
        Node::Text(Text{value: "first".to_string(), position: None}),
        Node::Text(Text{value: "second".to_string(), position: None})
    ], position: None}),
        "new",
        0,
        Node::Delete(Delete{values: vec![
            Node::Text(Text{value: "new".to_string(), position: None}),
            Node::Text(Text{value: "second".to_string(), position: None})
        ], position: None}))]
    #[case(Node::Emphasis(Emphasis{values: vec![
        Node::Text(Text{value: "first".to_string(), position: None}),
        Node::Text(Text{value: "second".to_string(), position: None})
    ], position: None}),
        "new",
        1,
        Node::Emphasis(Emphasis{values: vec![
            Node::Text(Text{value: "first".to_string(), position: None}),
            Node::Text(Text{value: "new".to_string(), position: None})
        ], position: None}))]
    #[case(Node::Strong(Strong{values: vec![
        Node::Text(Text{value: "first".to_string(), position: None}),
        Node::Text(Text{value: "second".to_string(), position: None})
    ], position: None}),
        "new",
        0,
        Node::Strong(Strong{values: vec![
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
    #[case(Node::Text(Text{value: "test".to_string(), position: None }), RenderOptions::default(), "test")]
    #[case(Node::List(List{index: 0, level: 2, checked: None, values: vec!["test".to_string().into()], position: None}), RenderOptions::default(), "    - test")]
    #[case(Node::List(List{index: 0, level: 1, checked: None, values: vec!["test".to_string().into()], position: None}), RenderOptions { list_style: ListStyle::Plus, ..Default::default() }, "  + test")]
    #[case(Node::List(List{index: 0, level: 1, checked: Some(true), values: vec!["test".to_string().into()], position: None}), RenderOptions { list_style: ListStyle::Star, ..Default::default() }, "  * [x] test")]
    #[case(Node::List(List{index: 0, level: 1, checked: Some(false), values: vec!["test".to_string().into()], position: None}), RenderOptions::default(), "  - [ ] test")]
    #[case(Node::TableRow(TableRow{values: vec![Node::TableCell(TableCell{column: 0, row: 0, last_cell_in_row: false, last_cell_of_in_table: false, values: vec!["test".to_string().into()], position: None})], position: None}), RenderOptions::default(), "|test")]
    #[case(Node::TableRow(TableRow{values: vec![Node::TableCell(TableCell{column: 0, row: 0, last_cell_in_row: true, last_cell_of_in_table: false, values: vec!["test".to_string().into()], position: None})], position: None}), RenderOptions::default(), "|test|")]
    #[case(Node::TableCell(TableCell{column: 0, row: 0, last_cell_in_row: false, last_cell_of_in_table: false, values: vec!["test".to_string().into()], position: None}), RenderOptions::default(), "|test")]
    #[case(Node::TableCell(TableCell{column: 0, row: 0, last_cell_in_row: true, last_cell_of_in_table: false, values: vec!["test".to_string().into()], position: None}), RenderOptions::default(), "|test|")]
    #[case(Node::TableHeader(TableHeader{align: vec![TableAlignKind::Left, TableAlignKind::Right, TableAlignKind::Center, TableAlignKind::None], position: None}), RenderOptions::default(), "|:---|---:|:---:|---|")]
    #[case(Node::Blockquote(Blockquote{values: vec!["test".to_string().into()], position: None}), RenderOptions::default(), "> test")]
    #[case(Node::Blockquote(Blockquote{values: vec!["test\ntest2".to_string().into()], position: None}), RenderOptions::default(), "> test\n> test2")]
    #[case(Node::Code(Code{value: "code".to_string(), lang: Some("rust".to_string()), fence: true, meta: None, position: None}), RenderOptions::default(), "```rust\ncode\n```")]
    #[case(Node::Code(Code{value: "code".to_string(), lang: None, fence: true, meta: None, position: None}), RenderOptions::default(), "```\ncode\n```")]
    #[case(Node::Code(Code{value: "code".to_string(), lang: None, fence: false, meta: None, position: None}), RenderOptions::default(), "    code")]
    #[case(Node::Code(Code{value: "code".to_string(), lang: Some("rust".to_string()), fence: true, meta: Some("meta".to_string()), position: None}), RenderOptions::default(), "```rust meta\ncode\n```")]
    #[case(Node::Definition(Definition{ident: "id".to_string(), url: Url::new("url".to_string()), title: None, label: Some("label".to_string()), position: None}), RenderOptions::default(), "[label]: url")]
    #[case(Node::Definition(Definition{ident: "id".to_string(), url: Url::new("url".to_string()), title: Some(Title::new("title".to_string())), label: Some("label".to_string()), position: None}), RenderOptions::default(), "[label]: url \"title\"")]
    #[case(Node::Definition(Definition{ident: "id".to_string(), url: Url::new("".to_string()), title: None, label: Some("label".to_string()), position: None}), RenderOptions::default(), "[label]: ")]
    #[case(Node::Delete(Delete{values: vec!["test".to_string().into()], position: None}), RenderOptions::default(), "~~test~~")]
    #[case(Node::Emphasis(Emphasis{values: vec!["test".to_string().into()], position: None}), RenderOptions::default(), "*test*")]
    #[case(Node::Footnote(Footnote{ident: "id".to_string(), values: vec!["label".to_string().into()], position: None}), RenderOptions::default(), "[^id]: label")]
    #[case(Node::FootnoteRef(FootnoteRef{ident: "label".to_string(), label: Some("label".to_string()), position: None}), RenderOptions::default(), "[^label]")]
    #[case(Node::Heading(Heading{depth: 1, values: vec!["test".to_string().into()], position: None}), RenderOptions::default(), "# test")]
    #[case(Node::Heading(Heading{depth: 3, values: vec!["test".to_string().into()], position: None}), RenderOptions::default(), "### test")]
    #[case(Node::Html(Html{value: "<div>test</div>".to_string(), position: None}), RenderOptions::default(), "<div>test</div>")]
    #[case(Node::Image(Image{alt: "alt".to_string(), url: "url".to_string(), title: None, position: None}), RenderOptions::default(), "![alt](url)")]
    #[case(Node::Image(Image{alt: "alt".to_string(), url: "url with space".to_string(), title: Some("title".to_string()), position: None}), RenderOptions::default(), "![alt](url%20with%20space \"title\")")]
    #[case(Node::ImageRef(ImageRef{alt: "alt".to_string(), ident: "id".to_string(), label: Some("id".to_string()), position: None}), RenderOptions::default(), "![alt][id]")]
    #[case(Node::ImageRef(ImageRef{alt: "id".to_string(), ident: "id".to_string(), label: Some("id".to_string()), position: None}), RenderOptions::default(), "![id]")]
    #[case(Node::CodeInline(CodeInline{value: "code".into(), position: None}), RenderOptions::default(), "`code`")]
    #[case(Node::MathInline(MathInline{value: "x^2".into(), position: None}), RenderOptions::default(), "$x^2$")]
    #[case(Node::Link(Link{url: Url::new("url".to_string()), title: Some(Title::new("title".to_string())), values: vec!["value".to_string().into()], position: None}), RenderOptions::default(), "[value](url \"title\")")]
    #[case(Node::Link(Link{url: Url::new("".to_string()), title: None, values: vec!["value".to_string().into()], position: None}), RenderOptions::default(), "[value]()")]
    #[case(Node::Link(Link{url: Url::new("url".to_string()), title: None, values: vec!["value".to_string().into()], position: None}), RenderOptions::default(), "[value](url)")]
    #[case(Node::LinkRef(LinkRef{ident: "id".to_string(), values: vec!["id".to_string().into()], label: Some("id".to_string()), position: None}), RenderOptions::default(), "[id]")]
    #[case(Node::LinkRef(LinkRef{ident: "id".to_string(), values: vec!["open".to_string().into()], label: Some("id".to_string()), position: None}), RenderOptions::default(), "[open][id]")]
    #[case(Node::Math(Math{value: "x^2".to_string(), position: None}), RenderOptions::default(), "$$\nx^2\n$$")]
    #[case(Node::Strong(Strong{values: vec!["test".to_string().into()], position: None}), RenderOptions::default(), "**test**")]
    #[case(Node::Yaml(Yaml{value: "key: value".to_string(), position: None}), RenderOptions::default(), "---\nkey: value\n---")]
    #[case(Node::Toml(Toml{value: "key = \"value\"".to_string(), position: None}), RenderOptions::default(), "+++\nkey = \"value\"\n+++")]
    #[case(Node::Break(Break{position: None}), RenderOptions::default(), "\\")]
    #[case(Node::HorizontalRule(HorizontalRule{position: None}), RenderOptions::default(), "---")]
    #[case(Node::MdxJsxFlowElement(MdxJsxFlowElement{
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
    #[case(Node::MdxJsxFlowElement(MdxJsxFlowElement{
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
    #[case(Node::MdxJsxFlowElement(MdxJsxFlowElement{
        name: Some("div".to_string()),
        attributes: Vec::new(),
        children: Vec::new(),
        position: None
    }), RenderOptions::default(), "<div />")]
    #[case(Node::MdxJsxTextElement(MdxJsxTextElement{
        name: Some("span".into()),
        attributes: vec![
            MdxAttributeContent::Expression("...props".into())
        ],
        children: vec![
            "inline".to_string().into()
        ],
        position: None
    }), RenderOptions::default(), "<span {...props}>inline</span>")]
    #[case(Node::MdxJsxTextElement(MdxJsxTextElement{
        name: Some("span".into()),
        attributes: vec![
            MdxAttributeContent::Expression("...props".into())
        ],
        children: vec![
        ],
        position: None
    }), RenderOptions::default(), "<span {...props} />")]
    #[case(Node::MdxJsxTextElement(MdxJsxTextElement{
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
    #[case(Node::Blockquote(Blockquote{values: Vec::new(), position: None}), "blockquote")]
    #[case(Node::Break(Break{position: None}), "break")]
    #[case(Node::Definition(Definition{ident: "".to_string(), url: Url::new("".to_string()), title: None, label: None, position: None}), "definition")]
    #[case(Node::Delete(Delete{values: Vec::new(), position: None}), "delete")]
    #[case(Node::Heading(Heading{depth: 1, values: Vec::new(), position: None}), "h1")]
    #[case(Node::Heading(Heading{depth: 2, values: Vec::new(), position: None}), "h2")]
    #[case(Node::Heading(Heading{depth: 3, values: Vec::new(), position: None}), "h3")]
    #[case(Node::Heading(Heading{depth: 4, values: Vec::new(), position: None}), "h4")]
    #[case(Node::Heading(Heading{depth: 5, values: Vec::new(), position: None}), "h5")]
    #[case(Node::Heading(Heading{depth: 6, values: Vec::new(), position: None}), "h6")]
    #[case(Node::Heading(Heading{depth: 7, values: Vec::new(), position: None}), "h")]
    #[case(Node::Emphasis(Emphasis{values: Vec::new(), position: None}), "emphasis")]
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
    #[case(Node::Strong(Strong{values: Vec::new(), position: None}), "strong")]
    #[case(Node::HorizontalRule(HorizontalRule{position: None}), "Horizontal_rule")]
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
    #[case(Node::Text(Text{value: "test".to_string(), position: None }), "test")]
    #[case(Node::List(List{index: 0, level: 0, checked: None, values: vec![Node::Text(Text{value: "test".to_string(), position: None})], position: None}), "test")]
    #[case(Node::Blockquote(Blockquote{values: vec![Node::Text(Text{value: "test".to_string(), position: None})], position: None}), "test")]
    #[case(Node::Delete(Delete{values: vec![Node::Text(Text{value: "test".to_string(), position: None})], position: None}), "test")]
    #[case(Node::Heading(Heading{depth: 1, values: vec![Node::Text(Text{value: "test".to_string(), position: None})], position: None}), "test")]
    #[case(Node::Emphasis(Emphasis{values: vec![Node::Text(Text{value: "test".to_string(), position: None})], position: None}), "test")]
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
    #[case(Node::Strong(Strong{values: vec![Node::Text(Text{value: "test".to_string(), position: None})], position: None}), "test")]
    #[case(Node::TableCell(TableCell{column: 0, row: 0, last_cell_in_row: false, last_cell_of_in_table: false, values: vec![Node::Text(Text{value: "test".to_string(), position: None})], position: None}), "test")]
    #[case(Node::TableRow(TableRow{values: vec![Node::TableCell(TableCell{column: 0, row: 0, last_cell_in_row: false, last_cell_of_in_table: false, values: vec![Node::Text(Text{value: "test".to_string(), position: None})], position: None})], position: None}), "test")]
    #[case(Node::Break(Break{position: None}), "")]
    #[case(Node::HorizontalRule(HorizontalRule{position: None}), "")]
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
    #[case(Node::Blockquote(Blockquote{values: Vec::new(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    #[case(Node::Delete(Delete{values: Vec::new(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    #[case(Node::Heading(Heading{depth: 1, values: Vec::new(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    #[case(Node::Emphasis(Emphasis{values: Vec::new(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
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
    #[case(Node::Strong(Strong{values: Vec::new(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    #[case(Node::TableCell(TableCell{column: 0, row: 0, last_cell_in_row: false, last_cell_of_in_table: false, values: Vec::new(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    #[case(Node::TableRow(TableRow{values: Vec::new(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    #[case(Node::TableHeader(TableHeader{align: Vec::new(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    #[case(Node::Break(Break{position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    #[case(Node::HorizontalRule(HorizontalRule{position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
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
        Node::Text(Text{value: "test2".to_string(), position: Some(Position{start: Point{line: 1, column: 6}, end: Point{line: 1, column: 10}})}),
        Node::Text(Text{value: "test".to_string(), position: None})
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
    #[case(Node::Blockquote(Blockquote{values: vec![
        Node::Text(Text{value: "first".to_string(), position: None}),
        Node::Text(Text{value: "second".to_string(), position: None})
    ], position: None}), 0, Some(Node::Text(Text{value: "first".to_string(), position: None})))]
    #[case(Node::Blockquote(Blockquote{values: vec![
        Node::Text(Text{value: "first".to_string(), position: None}),
        Node::Text(Text{value: "second".to_string(), position: None})
    ], position: None}), 1, Some(Node::Text(Text{value: "second".to_string(), position: None})))]
    #[case(Node::Blockquote(Blockquote{values: vec![
        Node::Text(Text{value: "first".to_string(), position: None})
    ], position: None}), 1, None)]
    #[case(Node::Delete(Delete{values: vec![
        Node::Text(Text{value: "first".to_string(), position: None}),
        Node::Text(Text{value: "second".to_string(), position: None})
    ], position: None}), 0, Some(Node::Text(Text{value: "first".to_string(), position: None})))]
    #[case(Node::Emphasis(Emphasis{values: vec![
        Node::Text(Text{value: "first".to_string(), position: None}),
        Node::Text(Text{value: "second".to_string(), position: None})
    ], position: None}), 1, Some(Node::Text(Text{value: "second".to_string(), position: None})))]
    #[case(Node::Strong(Strong{values: vec![
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
    #[case(Node::Blockquote(Blockquote{values: vec!["test".to_string().into()], position: None}),
           Node::Fragment(Fragment{values: vec!["test".to_string().into()]}))]
    #[case(Node::Delete(Delete{values: vec!["test".to_string().into()], position: None}),
           Node::Fragment(Fragment{values: vec!["test".to_string().into()]}))]
    #[case(Node::Heading(Heading{depth: 1, values: vec!["test".to_string().into()], position: None}),
           Node::Fragment(Fragment{values: vec!["test".to_string().into()]}))]
    #[case(Node::Emphasis(Emphasis{values: vec!["test".to_string().into()], position: None}),
           Node::Fragment(Fragment{values: vec!["test".to_string().into()]}))]
    #[case(Node::List(List{index: 0, level: 0, checked: None, values: vec!["test".to_string().into()], position: None}),
           Node::Fragment(Fragment{values: vec!["test".to_string().into()]}))]
    #[case(Node::Strong(Strong{values: vec!["test".to_string().into()], position: None}),
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
        &mut Node::Blockquote(Blockquote{values: vec![
            Node::Text(Text{value: "old".to_string(), position: None})
        ], position: None}),
        Node::Fragment(Fragment{values: vec![
            Node::Text(Text{value: "new".to_string(), position: None})
        ]}),
        Node::Blockquote(Blockquote{values: vec![
            Node::Text(Text{value: "new".to_string(), position: None})
        ], position: None})
    )]
    #[case(
        &mut Node::Delete(Delete{values: vec![
            Node::Text(Text{value: "old".to_string(), position: None})
        ], position: None}),
        Node::Fragment(Fragment{values: vec![
            Node::Text(Text{value: "new".to_string(), position: None})
        ]}),
        Node::Delete(Delete{values: vec![
            Node::Text(Text{value: "new".to_string(), position: None})
        ], position: None})
    )]
    #[case(
        &mut Node::Emphasis(Emphasis{values: vec![
            Node::Text(Text{value: "old".to_string(), position: None})
        ], position: None}),
        Node::Fragment(Fragment{values: vec![
            Node::Text(Text{value: "new".to_string(), position: None})
        ]}),
        Node::Emphasis(Emphasis{values: vec![
            Node::Text(Text{value: "new".to_string(), position: None})
        ], position: None})
    )]
    #[case(
        &mut Node::Strong(Strong{values: vec![
            Node::Text(Text{value: "old".to_string(), position: None})
        ], position: None}),
        Node::Fragment(Fragment{values: vec![
            Node::Text(Text{value: "new".to_string(), position: None})
        ]}),
        Node::Strong(Strong{values: vec![
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
        &mut Node::Blockquote(Blockquote{values: vec![
            Node::Text(Text{value: "text1".to_string(), position: None}),
            Node::Text(Text{value: "text2".to_string(), position: None})
        ], position: None}),
        Node::Fragment(Fragment{values: vec![
            Node::Text(Text{value: "new1".to_string(), position: None}),
            Node::Text(Text{value: "new2".to_string(), position: None})
        ]}),
        Node::Blockquote(Blockquote{values: vec![
            Node::Text(Text{value: "new1".to_string(), position: None}),
            Node::Text(Text{value: "new2".to_string(), position: None})
        ], position: None})
    )]
    #[case(
        &mut Node::Strong(Strong{values: vec![
            Node::Text(Text{value: "text1".to_string(), position: None}),
            Node::Text(Text{value: "text2".to_string(), position: None})
        ], position: None}),
        Node::Fragment(Fragment{values: vec![
            Node::Empty,
            Node::Text(Text{value: "new2".to_string(), position: None})
        ]}),
        Node::Strong(Strong{values: vec![
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

    #[rstest]
    #[case(Node::Text(Text{value: "test".to_string(), position: None}),
       Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}},
       Node::Text(Text{value: "test".to_string(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}))]
    #[case(Node::Code(Code{value: "code".to_string(), lang: None, fence: true, meta: None, position: None}),
       Position{start: Point{line: 1, column: 1}, end: Point{line: 3, column: 3}},
       Node::Code(Code{value: "code".to_string(), lang: None, fence: true, meta: None, position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 3, column: 3}})}))]
    #[case(Node::List(List{index: 0, level: 1, checked: None, values: vec![], position: None}),
       Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}},
       Node::List(List{index: 0, level: 1, checked: None, values: vec![], position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}))]
    #[case(Node::Definition(Definition{ident: "id".to_string(), url: Url::new("url".to_string()), title: None, label: None, position: None}),
       Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 10}},
       Node::Definition(Definition{ident: "id".to_string(), url: Url::new("url".to_string()), title: None, label: None, position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 10}})}))]
    #[case(Node::Delete(Delete{values: vec![Node::Text(Text{value: "test".to_string(), position: None})], position: None}),
        Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}},
        Node::Delete(Delete{values: vec![Node::Text(Text{value: "test".to_string(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})})], position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}))]
    #[case(Node::Emphasis(Emphasis{values: vec![Node::Text(Text{value: "test".to_string(), position: None})], position: None}),
        Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}},
        Node::Emphasis(Emphasis{values: vec![Node::Text(Text{value: "test".to_string(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})})], position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}))]
    #[case(Node::Footnote(Footnote{ident: "id".to_string(), values: vec![Node::Text(Text{value: "test".to_string(), position: None})], position: None}),
        Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}},
        Node::Footnote(Footnote{ident: "id".to_string(), values: vec![Node::Text(Text{value: "test".to_string(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})})], position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}))]
    #[case(Node::FootnoteRef(FootnoteRef{ident: "id".to_string(), label: Some("label".to_string()), position: None}),
        Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}},
        Node::FootnoteRef(FootnoteRef{ident: "id".to_string(), label: Some("label".to_string()), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}))]
    #[case(Node::Html(Html{value: "<div>test</div>".to_string(), position: None}),
        Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 15}},
        Node::Html(Html{value: "<div>test</div>".to_string(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 15}})}))]
    #[case(Node::Yaml(Yaml{value: "key: value".to_string(), position: None}),
        Position{start: Point{line: 1, column: 1}, end: Point{line: 3, column: 4}},
        Node::Yaml(Yaml{value: "key: value".to_string(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 3, column: 4}})}))]
    #[case(Node::Toml(Toml{value: "key = \"value\"".to_string(), position: None}),
        Position{start: Point{line: 1, column: 1}, end: Point{line: 3, column: 4}},
        Node::Toml(Toml{value: "key = \"value\"".to_string(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 3, column: 4}})}))]
    #[case(Node::Image(Image{alt: "alt".to_string(), url: "url".to_string(), title: None, position: None}),
        Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 12}},
        Node::Image(Image{alt: "alt".to_string(), url: "url".to_string(), title: None, position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 12}})}))]
    #[case(Node::ImageRef(ImageRef{alt: "alt".to_string(), ident: "id".to_string(), label: None, position: None}),
        Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 10}},
        Node::ImageRef(ImageRef{alt: "alt".to_string(), ident: "id".to_string(), label: None, position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 10}})}))]
    #[case(Node::CodeInline(CodeInline{value: "code".into(), position: None}),
        Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 7}},
        Node::CodeInline(CodeInline{value: "code".into(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 7}})}))]
    #[case(Node::MathInline(MathInline{value: "x^2".into(), position: None}),
        Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}},
        Node::MathInline(MathInline{value: "x^2".into(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}))]
    #[case(Node::Link(Link{url: Url::new("url".to_string()), title: None, values: vec![Node::Text(Text{value: "text".to_string(), position: None})], position: None}),
        Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 10}},
        Node::Link(Link{url: Url::new("url".to_string()), title: None, values: vec![Node::Text(Text{value: "text".to_string(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 10}})})], position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 10}})}))]
    #[case(Node::LinkRef(LinkRef{ident: "id".to_string(), values: vec![Node::Text(Text{value: "text".to_string(), position: None})], label: None, position: None}),
        Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 10}},
        Node::LinkRef(LinkRef{ident: "id".to_string(), values: vec![Node::Text(Text{value: "text".to_string(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 10}})})], label: None, position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 10}})}))]
    #[case(Node::Math(Math{value: "x^2".to_string(), position: None}),
        Position{start: Point{line: 1, column: 1}, end: Point{line: 3, column: 3}},
        Node::Math(Math{value: "x^2".to_string(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 3, column: 3}})}))]
    #[case(Node::TableCell(TableCell{column: 0, row: 0, last_cell_in_row: false, last_cell_of_in_table: false, values: vec![Node::Text(Text{value: "cell".to_string(), position: None})], position: None}),
        Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 6}},
        Node::TableCell(TableCell{column: 0, row: 0, last_cell_in_row: false, last_cell_of_in_table: false, values: vec![Node::Text(Text{value: "cell".to_string(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 6}})})], position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 6}})}))]
    #[case(Node::TableHeader(TableHeader{align: vec![TableAlignKind::Left, TableAlignKind::Right], position: None}),
        Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 15}},
        Node::TableHeader(TableHeader{align: vec![TableAlignKind::Left, TableAlignKind::Right], position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 15}})}))]
    #[case(Node::MdxFlowExpression(MdxFlowExpression{value: "test".into(), position: None}),
        Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 7}},
        Node::MdxFlowExpression(MdxFlowExpression{value: "test".into(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 7}})}))]
    #[case(Node::MdxTextExpression(MdxTextExpression{value: "test".into(), position: None}),
        Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 7}},
        Node::MdxTextExpression(MdxTextExpression{value: "test".into(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 7}})}))]
    #[case(Node::MdxJsEsm(MdxJsEsm{value: "import React from 'react'".into(), position: None}),
        Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 25}},
        Node::MdxJsEsm(MdxJsEsm{value: "import React from 'react'".into(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 25}})}))]
    #[case(Node::MdxJsxTextElement(MdxJsxTextElement{name: Some("span".into()), attributes: Vec::new(), children: vec![Node::Text(Text{value: "text".to_string(), position: None})], position: None}),
        Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 20}},
        Node::MdxJsxTextElement(MdxJsxTextElement{name: Some("span".into()), attributes: Vec::new(), children: vec![Node::Text(Text{value: "text".to_string(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 20}})})], position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 20}})}))]
    #[case(Node::Break(Break{position: None}),
        Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 2}},
        Node::Break(Break{position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 2}})}))]
    #[case(Node::Empty,
       Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}},
       Node::Empty)]
    #[case(Node::Fragment(Fragment{values: vec![
           Node::Text(Text{value: "test1".to_string(), position: None}),
           Node::Text(Text{value: "test2".to_string(), position: None})
       ]}),
       Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 10}},
       Node::Fragment(Fragment{values: vec![
           Node::Text(Text{value: "test1".to_string(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 10}})}),
           Node::Text(Text{value: "test2".to_string(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 10}})})
       ]}))]
    #[case(Node::Blockquote(Blockquote{values: vec![
        Node::Text(Text{value: "test".to_string(), position: None})], position: None}),
        Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}},
        Node::Blockquote(Blockquote{values: vec![
            Node::Text(Text{value: "test".to_string(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})})
        ], position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}))]
    #[case(Node::Heading(Heading{depth: 1, values: vec![
            Node::Text(Text{value: "test".to_string(), position: None})], position: None}),
            Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}},
            Node::Heading(Heading{depth: 1, values: vec![
                Node::Text(Text{value: "test".to_string(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})})
            ], position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}))]
    #[case(Node::Strong(Strong{values: vec![
            Node::Text(Text{value: "test".to_string(), position: None})], position: None}),
            Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}},
            Node::Strong(Strong{values: vec![
                Node::Text(Text{value: "test".to_string(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})})
            ], position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}))]
    #[case(Node::TableRow(TableRow{values: vec![
            Node::TableCell(TableCell{column: 0, row: 0, last_cell_in_row: false, last_cell_of_in_table: false, values: vec![
                Node::Text(Text{value: "cell".to_string(), position: None})
            ], position: None})
        ], position: None}),
            Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 10}},
            Node::TableRow(TableRow{values: vec![
                Node::TableCell(TableCell{column: 0, row: 0, last_cell_in_row: false, last_cell_of_in_table: false, values: vec![
                    Node::Text(Text{value: "cell".to_string(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 10}})})
                ], position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 10}})})
            ], position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 10}})}))]
    #[case(Node::MdxJsxFlowElement(MdxJsxFlowElement{
            name: Some("div".to_string()),
            attributes: Vec::new(),
            children: vec![Node::Text(Text{value: "content".to_string(), position: None})],
            position: None
        }),
            Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 20}},
            Node::MdxJsxFlowElement(MdxJsxFlowElement{
                name: Some("div".to_string()),
                attributes: Vec::new(),
                children: vec![Node::Text(Text{value: "content".to_string(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 20}})})],
                position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 20}})
            }))]
    fn test_set_position(
        #[case] mut node: Node,
        #[case] position: Position,
        #[case] expected: Node,
    ) {
        node.set_position(Some(position));
        assert_eq!(node, expected);
    }

    #[rstest]
    #[case(Node::List(List{index: 0, level: 0, checked: None, values: vec!["test".to_string().into()], position: None}), true)]
    #[case(Node::List(List{index: 1, level: 2, checked: Some(true), values: vec!["test".to_string().into()], position: None}), true)]
    #[case(Node::Text(Text{value: "test".to_string(), position: None}), false)]
    fn test_is_list(#[case] node: Node, #[case] expected: bool) {
        assert_eq!(node.is_list(), expected);
    }

    #[rstest]
    #[case(Node::TableCell(TableCell{column: 0, row: 0, last_cell_in_row: false, last_cell_of_in_table: false, values: vec![], position: None}), true)]
    #[case(Node::TableCell(TableCell{column: 1, row: 2, last_cell_in_row: true, last_cell_of_in_table: true, values: vec!["content".to_string().into()], position: None}), true)]
    #[case(Node::Text(Text{value: "test".to_string(), position: None}), false)]
    fn test_is_table_cell(#[case] node: Node, #[case] expected: bool) {
        assert_eq!(node.is_table_cell(), expected);
    }

    #[rstest]
    #[case(Node::TableRow(TableRow{values: vec![], position: None}), true)]
    #[case(Node::TableRow(TableRow{values: vec![Node::TableCell(TableCell{column: 0, row: 0, last_cell_in_row: true, last_cell_of_in_table: false, values: vec![], position: None})], position: None}), true)]
    #[case(Node::Text(Text{value: "test".to_string(), position: None}), false)]
    fn test_is_table_row(#[case] node: Node, #[case] expected: bool) {
        assert_eq!(node.is_table_row(), expected);
    }

    #[rstest]
    #[case(Url::new("https://example.com".to_string()), RenderOptions{link_url_style: UrlSurroundStyle::None, ..Default::default()}, "https://example.com")]
    #[case(Url::new("https://example.com".to_string()), RenderOptions{link_url_style: UrlSurroundStyle::Angle, ..Default::default()}, "<https://example.com>")]
    #[case(Url::new("".to_string()), RenderOptions::default(), "")]
    fn test_url_to_string_with(
        #[case] url: Url,
        #[case] options: RenderOptions,
        #[case] expected: &str,
    ) {
        assert_eq!(url.to_string_with(&options), expected);
    }

    #[rstest]
    #[case(Title::new("title".to_string()), RenderOptions::default(), "\"title\"")]
    #[case(Title::new(r#"title with "quotes""#.to_string()), RenderOptions::default(), r#""title with "quotes"""#)]
    #[case(Title::new("title with spaces".to_string()), RenderOptions::default(), "\"title with spaces\"")]
    #[case(Title::new("".to_string()), RenderOptions::default(), "\"\"")]
    #[case(Title::new("title".to_string()), RenderOptions{link_title_style: TitleSurroundStyle::Single, ..Default::default()}, "'title'")]
    #[case(Title::new("title with 'quotes'".to_string()), RenderOptions{link_title_style: TitleSurroundStyle::Double, ..Default::default()}, "\"title with 'quotes'\"")]
    #[case(Title::new("title".to_string()), RenderOptions{link_title_style: TitleSurroundStyle::Paren, ..Default::default()}, "(title)")]
    fn test_title_to_string_with(
        #[case] title: Title,
        #[case] options: RenderOptions,
        #[case] expected: &str,
    ) {
        assert_eq!(title.to_string_with(&options), expected);
    }

    #[rstest]
    #[case(Node::Fragment(Fragment{values: vec![]}), true)]
    #[case(Node::Fragment(Fragment{values: vec![
        Node::Text(Text{value: "not_empty".to_string(), position: None})
    ]}), false)]
    #[case(Node::Fragment(Fragment{values: vec![
        Node::Fragment(Fragment{values: vec![]}),
        Node::Fragment(Fragment{values: vec![]})
    ]}), true)]
    #[case(Node::Fragment(Fragment{values: vec![
        Node::Fragment(Fragment{values: vec![]}),
        Node::Text(Text{value: "not_empty".to_string(), position: None})
    ]}), false)]
    #[case(Node::Text(Text{value: "not_fragment".to_string(), position: None}), false)]
    fn test_is_empty_fragment(#[case] node: Node, #[case] expected: bool) {
        assert_eq!(node.is_empty_fragment(), expected);
    }
}
