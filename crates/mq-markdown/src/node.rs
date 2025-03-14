use std::fmt::{self, Display};

use compact_str::CompactString;
use itertools::Itertools;
use markdown::mdast;

type Level = u8;

pub const EMPTY_NODE: Node = Node::Text(Text {
    value: String::new(),
    position: None,
});

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
    pub cells: Vec<Node>,
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
    pub values: Vec<Node>,
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
    TableRow(TableRow),
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

impl PartialOrd for Node {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        let (self_node, other_node) = (self, other);
        let self_pos = self_node.position();
        let other_pos = other_node.position();

        match (self_pos, other_pos) {
            (Some(self_pos), Some(other_pos)) => {
                match self_pos.start.line.cmp(&other_pos.start.line) {
                    std::cmp::Ordering::Equal => {
                        // If lines are equal, compare by column
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
        write!(f, "{}", self.to_string_with(&ListStyle::default()))
    }
}

impl Node {
    pub fn text_type(text: &str) -> Self {
        Self::Text(Text {
            value: text.to_string(),
            position: None,
        })
    }

    pub fn to_string_with(&self, list_style: &ListStyle) -> String {
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
                    list_style,
                    checked
                        .map(|it| if it { "[x] " } else { "[] " })
                        .unwrap_or_else(|| ""),
                    values
                        .iter()
                        .map(|value| value.to_string_with(list_style))
                        .join("")
                )
            }
            Self::TableRow(TableRow { cells, .. }) => cells
                .iter()
                .map(|cell| cell.to_string_with(list_style))
                .join(""),
            Self::TableCell(TableCell {
                last_cell_in_row,
                last_cell_of_in_table,
                values,
                ..
            }) => {
                if last_cell_in_row || last_cell_of_in_table {
                    format!(
                        "|{}|",
                        values
                            .iter()
                            .map(|value| value.to_string_with(list_style))
                            .join("")
                    )
                } else {
                    format!(
                        "|{}",
                        values
                            .iter()
                            .map(|value| value.to_string_with(list_style))
                            .join("")
                    )
                }
            }
            Self::TableHeader(TableHeader { align, .. }) => {
                format!("|{}|", align.iter().map(|a| a.to_string()).join("|"))
            }
            Self::Blockquote(Value { values, .. }) => {
                format!(
                    "> {}",
                    values
                        .iter()
                        .map(|value| value.to_string_with(list_style))
                        .join("")
                )
            }
            Self::Code(Code { value, lang, .. }) => {
                let lang_str = lang.as_deref().unwrap_or("");
                format!("\n```{}\n{}\n```\n", lang_str, value)
            }
            Self::Definition(Definition { label, url, .. }) => {
                format!("[{}]: {}", label.unwrap_or_default(), url)
            }
            Self::Delete(Value { values, .. }) => {
                format!(
                    "~~{}~~",
                    values
                        .iter()
                        .map(|value| value.to_string_with(list_style))
                        .join("")
                )
            }
            Self::Emphasis(Value { values, .. }) => {
                format!(
                    "*{}*",
                    values
                        .iter()
                        .map(|value| value.to_string_with(list_style))
                        .join("")
                )
            }
            Self::Footnote(Footnote { label, ident, .. }) => {
                format!("[^{}]: {}", label.unwrap_or_default(), ident)
            }
            Self::FootnoteRef(FootnoteRef { label, .. }) => {
                format!("[^{}]", label.unwrap_or_default())
            }
            Self::Heading(Heading { depth, values, .. }) => {
                format!(
                    "{} {}",
                    "#".repeat(depth as usize),
                    values
                        .iter()
                        .map(|value| value.to_string_with(list_style))
                        .join("")
                )
            }
            Self::Html(Html { value, .. }) => format!("\n{}\n", value),
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
            Self::Strong(Value { values, .. }) => {
                format!(
                    "**{}**",
                    values
                        .iter()
                        .map(|value| value.to_string_with(list_style))
                        .join("")
                )
            }
            Self::Yaml(Yaml { value, .. }) => format!("\n{}\n", value),
            Self::Toml(Toml { value, .. }) => format!("\n{}\n", value),
            Self::Break { .. } => "\\".to_string(),
            Self::HorizontalRule { .. } => "---".to_string(),
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
            Self::TableRow(v) => v.cells.get(index).cloned(),
            _ => None,
        }
    }

    pub fn value(&self) -> String {
        match self.clone() {
            Self::Blockquote(v) => v
                .values
                .first()
                .map(|value| value.value())
                .unwrap_or_default(),
            Self::Definition(d) => d.ident,
            Self::Delete(v) => v.values.iter().map(|value| value.value()).join(""),
            Self::Heading(h) => h.values.iter().map(|value| value.value()).join(""),
            Self::Emphasis(v) => v.values.iter().map(|value| value.value()).join(""),
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
            Self::List(l) => l.values.iter().map(|value| value.value()).join(""),
            Self::TableCell(c) => c.values.iter().map(|value| value.value()).join(""),
            Self::TableRow(c) => c.cells.iter().map(|cell| cell.value()).join(","),
            Self::Code(c) => c.value,
            Self::Strong(v) => v.values.iter().map(|value| value.value()).join(""),
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
            Self::TableRow(_) => "table_row".into(),
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
            Self::TableRow(r) => r.position.clone(),
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
                row.cells = row
                    .cells
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
            Self::Heading(mut v) => {
                if let Some(node) = v.values.first() {
                    v.values[0] = node.with_value(value);
                }

                Self::Heading(v)
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
                    .collect::<Vec<_>>(),
                ..mdx
            }),
            Self::MdxJsxTextElement(mdx) => Self::MdxJsxTextElement(mdast::MdxJsxTextElement {
                name: mdx.name,
                attributes: mdx.attributes,
                children: mdx
                    .children
                    .into_iter()
                    .map(|n| Self::set_mdast_value(n, value))
                    .collect::<Vec<_>>(),
                ..mdx
            }),
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
            Self::Heading(mut v) => {
                if v.values.get(index).is_some() {
                    v.values[index] = v.values[index].with_value(value);
                }

                Self::Heading(v)
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
                let title = Self::mdast_children_to_node(node)
                    .iter()
                    .map(|value| value.to_string())
                    .join("");
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
                    itertools::concat(vec![
                        vec![Self::List(List {
                            level,
                            index: 0,
                            checked: list.checked,
                            values: Self::from_mdast_node(n.clone()),
                            position: n.position().map(|p| p.clone().into()),
                        })],
                        list.children
                            .iter()
                            .flat_map(|node| {
                                if let mdast::Node::List(sub_list) = node {
                                    Self::mdast_list_items(sub_list, level + 1)
                                } else if let mdast::Node::ListItem(list) = node {
                                    vec![Self::List(List {
                                        level: level + 1,
                                        index: 0,
                                        checked: list.checked,
                                        values: Self::from_mdast_node(n.clone()),
                                        position: node.position().map(|p| p.clone().into()),
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

    fn set_mdast_value(node: mdast::Node, value: &str) -> mdast::Node {
        match node {
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
                        .collect::<Vec<_>>(),
                    ..mdx_jsx_flow_element
                })
            }
            mdast::Node::MdxJsxTextElement(mdx_jsx_text_element) => {
                mdast::Node::MdxJsxTextElement(mdast::MdxJsxTextElement {
                    children: mdx_jsx_text_element
                        .children
                        .into_iter()
                        .map(|children| Self::set_mdast_value(children, value))
                        .collect::<Vec<_>>(),
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
            _ => unreachable!(),
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
    #[case::list(Node::List(List{index: 0, level: 0, checked: None, values: vec!["test".to_string().into()], position: None }),
           "test".to_string(),
           Node::List(List{index: 0, level: 0, checked: None, values: vec!["test".to_string().into()], position: None }))]
    #[case::list(Node::List(List{index: 1, level: 1, checked: Some(true), values: vec!["test".to_string().into()], position: None }),
           "test".to_string(),
           Node::List(List{index: 1, level: 1, checked: Some(true), values: vec!["test".to_string().into()], position: None }))]
    #[case::list(Node::List(List{index: 2, level: 2, checked: Some(false), values: vec!["test".to_string().into()], position: None }),
           "test".to_string(),
           Node::List(List{index: 2, level: 2, checked: Some(false), values: vec!["test".to_string().into()], position: None }))]
    #[case::code_inline(Node::CodeInline(CodeInline{ value: "t".to_string(), position: None }),
           "test".to_string(),
           Node::CodeInline(CodeInline{ value: "test".to_string(), position: None }))]
    #[case::math_inline(Node::MathInline(MathInline{ value: "t".to_string(), position: None }),
           "test".to_string(),
           Node::MathInline(MathInline{ value: "test".to_string(), position: None }))]
    #[case::toml(Node::Toml(Toml{ value: "t".to_string(), position: None }),
           "test".to_string(),
           Node::Toml(Toml{ value: "test".to_string(), position: None }))]
    #[case::yaml(Node::Yaml(Yaml{ value: "t".to_string(), position: None }),
           "test".to_string(),
           Node::Yaml(Yaml{ value: "test".to_string(), position: None }))]
    #[case::html(Node::Html(Html{ value: "t".to_string(), position: None }),
           "test".to_string(),
           Node::Html(Html{ value: "test".to_string(), position: None }))]
    #[case::table_row(Node::TableRow(TableRow{ cells: vec![
                        Node::TableCell(TableCell{values: vec!["test1".to_string().into()], row:0, column:1, last_cell_in_row: false, last_cell_of_in_table: false, position: None}),
                        Node::TableCell(TableCell{values: vec!["test2".to_string().into()], row:0, column:2, last_cell_in_row: true, last_cell_of_in_table: false, position: None})
                    ]
                    , position: None }),
           "test3,test4".to_string(),
           Node::TableRow(TableRow{ cells: vec![
                        Node::TableCell(TableCell{values: vec!["test3".to_string().into()], row:0, column:1, last_cell_in_row: false, last_cell_of_in_table: false, position: None}),
                        Node::TableCell(TableCell{values: vec!["test4".to_string().into()], row:0, column:2, last_cell_in_row: true, last_cell_of_in_table: false, position: None})
                    ]
                    , position: None }))]
    #[case::table_cell(Node::TableCell(TableCell{values: vec!["test1".to_string().into()], row:0, column:1, last_cell_in_row: false, last_cell_of_in_table: false, position: None}),
            "test2".to_string(),
            Node::TableCell(TableCell{values: vec!["test2".to_string().into()], row:0, column:1, last_cell_in_row: false, last_cell_of_in_table: false, position: None}),)]
    #[case::link_ref(Node::LinkRef(LinkRef{ident: "test1".to_string(), label: None, position: None}),
            "test2".to_string(),
            Node::LinkRef(LinkRef{ident: "test2".to_string(), label: None, position: None}),)]
    #[case::image_ref(Node::ImageRef(ImageRef{alt: "alt".to_string(), ident: "test1".to_string(), label: None, position: None}),
            "test2".to_string(),
            Node::ImageRef(ImageRef{alt: "alt".to_string(), ident: "test2".to_string(), label: None, position: None}),)]
    #[case::definition(Node::Definition(Definition{ url: "url".to_string(), title: None, ident: "test1".to_string(), label: None, position: None}),
            "test2".to_string(),
            Node::Definition(Definition{url: "url".to_string(), title: None, ident: "test2".to_string(), label: None, position: None}),)]
    #[case::break_(Node::Break{ position: None},
            "test".to_string(),
            Node::Break{position: None})]
    #[case::horizontal_rule(Node::HorizontalRule { position: None},
            "test".to_string(),
            Node::HorizontalRule{position: None})]
    fn test_with_value(#[case] node: Node, #[case] input: String, #[case] expected: Node) {
        assert_eq!(node.with_value(input.as_str()), expected);
    }

    #[rstest]
    #[case(Node::Text(Text{value: "test".to_string(), position: None }),
           "test".to_string())]
    #[case(Node::List(List{index: 0, level: 2, checked: None, values: vec!["test".to_string().into()], position: None}),
           "    - test".to_string())]
    fn test_display(#[case] node: Node, #[case] expected: String) {
        assert_eq!(node.to_string_with(&ListStyle::default()), expected);
    }

    #[rstest]
    #[case(Node::Text(Text{value: "test".to_string(), position: None}), true)]
    #[case(Node::CodeInline(CodeInline{value: "test".to_string(), position: None}), false)]
    #[case(Node::MathInline(MathInline{value: "test".to_string(), position: None}), false)]
    fn test_is_text(#[case] node: Node, #[case] expected: bool) {
        assert_eq!(node.is_text(), expected);
    }

    #[rstest]
    #[case(Node::CodeInline(CodeInline{value: "test".to_string(), position: None}), true)]
    #[case(Node::Text(Text{value: "test".to_string(), position: None}), false)]
    fn test_is_inline_code(#[case] node: Node, #[case] expected: bool) {
        assert_eq!(node.is_inline_code(), expected);
    }

    #[rstest]
    #[case(Node::MathInline(MathInline{value: "test".to_string(), position: None}), true)]
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
    #[case(Node::Link(Link{url: "test".to_string(), title: None, position: None}), true)]
    #[case(Node::Text(Text{value: "test".to_string(), position: None}), false)]
    fn test_is_link(#[case] node: Node, #[case] expected: bool) {
        assert_eq!(node.is_link(), expected);
    }

    #[rstest]
    #[case(Node::LinkRef(LinkRef{ident: "test".to_string(), label: None, position: None}), true)]
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
    #[case(Node::Code(Code{value: "code".to_string(), lang: Some("rust".to_string()), position: None}), true, Some("rust".into()))]
    #[case(Node::Code(Code{value: "code".to_string(), lang: Some("rust".to_string()), position: None}), false, Some("python".into()))]
    #[case(Node::Code(Code{value: "code".to_string(), lang: None, position: None}), true, None)]
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
    fn test_node_value(#[case] actual: Vec<Node>, #[case] expected: Vec<Node>) {
        assert_eq!(actual, expected);
    }

    #[rstest]
    #[case(Node::Footnote(Footnote{ident: "test".to_string(), label: None, position: None}), true)]
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
    #[case(Node::Definition(Definition{ident: "ident".to_string(), url: "url".to_string(), title: None, label: None, position: None}), true)]
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
    #[case(Node::MdxFlowExpression(mdast::MdxFlowExpression{value: "test".to_string(), position: None, stops: vec![]}), true)]
    #[case(Node::Text(Text{value: "test".to_string(), position: None}), false)]
    fn test_is_mdx_flow_expression(#[case] node: Node, #[case] expected: bool) {
        assert_eq!(node.is_mdx_flow_expression(), expected);
    }

    #[rstest]
    #[case(Node::MdxTextExpression(mdast::MdxTextExpression{value: "test".to_string(), position: None, stops: vec![]}), true)]
    #[case(Node::Text(Text{value: "test".to_string(), position: None}), false)]
    fn test_is_mdx_text_expression(#[case] node: Node, #[case] expected: bool) {
        assert_eq!(node.is_mdx_text_expression(), expected);
    }

    #[rstest]
    #[case(Node::Text(Text{value: "test".to_string(), position: None }), ListStyle::Dash, "test")]
    #[case(Node::List(List{index: 0, level: 2, checked: None, values: vec!["test".to_string().into()], position: None}), ListStyle::Dash, "    - test")]
    #[case(Node::List(List{index: 0, level: 1, checked: None, values: vec!["test".to_string().into()], position: None}), ListStyle::Plus, "  + test")]
    #[case(Node::List(List{index: 0, level: 1, checked: Some(true), values: vec!["test".to_string().into()], position: None}), ListStyle::Star, "  * [x] test")]
    #[case(Node::List(List{index: 0, level: 1, checked: Some(false), values: vec!["test".to_string().into()], position: None}), ListStyle::Dash, "  - [] test")]
    #[case(Node::TableRow(TableRow{cells: vec![Node::TableCell(TableCell{column: 0, row: 0, last_cell_in_row: false, last_cell_of_in_table: false, values: vec!["test".to_string().into()], position: None})], position: None}), ListStyle::Dash, "|test")]
    #[case(Node::TableRow(TableRow{cells: vec![Node::TableCell(TableCell{column: 0, row: 0, last_cell_in_row: true, last_cell_of_in_table: false, values: vec!["test".to_string().into()], position: None})], position: None}), ListStyle::Dash, "|test|")]
    #[case(Node::TableCell(TableCell{column: 0, row: 0, last_cell_in_row: false, last_cell_of_in_table: false, values: vec!["test".to_string().into()], position: None}), ListStyle::Dash, "|test")]
    #[case(Node::TableCell(TableCell{column: 0, row: 0, last_cell_in_row: true, last_cell_of_in_table: false, values: vec!["test".to_string().into()], position: None}), ListStyle::Dash, "|test|")]
    #[case(Node::TableHeader(TableHeader{align: vec![TableAlignKind::Left, TableAlignKind::Right, TableAlignKind::Center, TableAlignKind::None], position: None}), ListStyle::Dash, "|:---|---:|:---:|---|")]
    #[case(Node::Blockquote(Value{values: vec!["test".to_string().into()], position: None}), ListStyle::Dash, "> test")]
    #[case(Node::Code(Code{value: "code".to_string(), lang: Some("rust".to_string()), position: None}), ListStyle::Dash, "\n```rust\ncode\n```\n")]
    #[case(Node::Code(Code{value: "code".to_string(), lang: None, position: None}), ListStyle::Dash, "\n```\ncode\n```\n")]
    #[case(Node::Definition(Definition{ident: "id".to_string(), url: "url".to_string(), title: None, label: Some("label".to_string()), position: None}), ListStyle::Dash, "[label]: url")]
    #[case(Node::Delete(Value{values: vec!["test".to_string().into()], position: None}), ListStyle::Dash, "~~test~~")]
    #[case(Node::Emphasis(Value{values: vec!["test".to_string().into()], position: None}), ListStyle::Dash, "*test*")]
    #[case(Node::Footnote(Footnote{ident: "id".to_string(), label: Some("label".to_string()), position: None}), ListStyle::Dash, "[^label]: id")]
    #[case(Node::FootnoteRef(FootnoteRef{ident: "id".to_string(), label: Some("label".to_string()), position: None}), ListStyle::Dash, "[^label]")]
    #[case(Node::Heading(Heading{depth: 1, values: vec!["test".to_string().into()], position: None}), ListStyle::Dash, "# test")]
    #[case(Node::Heading(Heading{depth: 3, values: vec!["test".to_string().into()], position: None}), ListStyle::Dash, "### test")]
    #[case(Node::Html(Html{value: "<div>test</div>".to_string(), position: None}), ListStyle::Dash, "\n<div>test</div>\n")]
    #[case(Node::Image(Image{alt: "alt".to_string(), url: "url".to_string(), title: None, position: None}), ListStyle::Dash, "![alt](url)")]
    #[case(Node::Image(Image{alt: "alt".to_string(), url: "url with space".to_string(), title: Some("title".to_string()), position: None}), ListStyle::Dash, "![alt](url%20with%20space \"title\")")]
    #[case(Node::ImageRef(ImageRef{alt: "alt".to_string(), ident: "id".to_string(), label: None, position: None}), ListStyle::Dash, "![alt][id]")]
    #[case(Node::CodeInline(CodeInline{value: "code".to_string(), position: None}), ListStyle::Dash, "`code`")]
    #[case(Node::MathInline(MathInline{value: "x^2".to_string(), position: None}), ListStyle::Dash, "$x^2$")]
    #[case(Node::Link(Link{url: "url".to_string(), title: Some("title".to_string()), position: None}), ListStyle::Dash, "[title](url)")]
    #[case(Node::Link(Link{url: "url with space".to_string(), title: Some("title with space".to_string()), position: None}), ListStyle::Dash, "[title-with-space](url-with-space)")]
    #[case(Node::LinkRef(LinkRef{ident: "id".to_string(), label: Some("label".to_string()), position: None}), ListStyle::Dash, "[id][label]")]
    #[case(Node::Math(Math{value: "x^2".to_string(), position: None}), ListStyle::Dash, "$$\nx^2\n$$")]
    #[case(Node::Strong(Value{values: vec!["test".to_string().into()], position: None}), ListStyle::Dash, "**test**")]
    #[case(Node::Yaml(Yaml{value: "key: value".to_string(), position: None}), ListStyle::Dash, "\nkey: value\n")]
    #[case(Node::Toml(Toml{value: "key = \"value\"".to_string(), position: None}), ListStyle::Dash, "\nkey = \"value\"\n")]
    #[case(Node::Break{position: None}, ListStyle::Dash, "\\")]
    #[case(Node::HorizontalRule{position: None}, ListStyle::Dash, "---")]
    fn test_to_string_with(
        #[case] node: Node,
        #[case] list_style: ListStyle,
        #[case] expected: &str,
    ) {
        assert_eq!(node.to_string_with(&list_style), expected);
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
            position: None,
        });

        assert_eq!(node6.partial_cmp(&node4), Some(std::cmp::Ordering::Less));
        assert_eq!(node4.partial_cmp(&node6), Some(std::cmp::Ordering::Greater));
    }

    #[rstest]
    #[case(Node::Blockquote(Value{values: vec![], position: None}), "blockquote")]
    #[case(Node::Break{position: None}, "break")]
    #[case(Node::Definition(Definition{ident: "".to_string(), url: "".to_string(), title: None, label: None, position: None}), "definition")]
    #[case(Node::Delete(Value{values: vec![], position: None}), "delete")]
    #[case(Node::Heading(Heading{depth: 1, values: vec![], position: None}), "h1")]
    #[case(Node::Heading(Heading{depth: 2, values: vec![], position: None}), "h2")]
    #[case(Node::Heading(Heading{depth: 3, values: vec![], position: None}), "h3")]
    #[case(Node::Heading(Heading{depth: 4, values: vec![], position: None}), "h4")]
    #[case(Node::Heading(Heading{depth: 5, values: vec![], position: None}), "h5")]
    #[case(Node::Heading(Heading{depth: 6, values: vec![], position: None}), "h")]
    #[case(Node::Emphasis(Value{values: vec![], position: None}), "emphasis")]
    #[case(Node::Footnote(Footnote{ident: "".to_string(), label: None, position: None}), "footnote")]
    #[case(Node::FootnoteRef(FootnoteRef{ident: "".to_string(), label: None, position: None}), "footnoteref")]
    #[case(Node::Html(Html{value: "".to_string(), position: None}), "html")]
    #[case(Node::Yaml(Yaml{value: "".to_string(), position: None}), "yaml")]
    #[case(Node::Toml(Toml{value: "".to_string(), position: None}), "toml")]
    #[case(Node::Image(Image{alt: "".to_string(), url: "".to_string(), title: None, position: None}), "image")]
    #[case(Node::ImageRef(ImageRef{alt: "".to_string(), ident: "".to_string(), label: None, position: None}), "image_ref")]
    #[case(Node::CodeInline(CodeInline{value: "".to_string(), position: None}), "code_inline")]
    #[case(Node::MathInline(MathInline{value: "".to_string(), position: None}), "math_inline")]
    #[case(Node::Link(Link{url: "".to_string(), title: None, position: None}), "link")]
    #[case(Node::LinkRef(LinkRef{ident: "".to_string(), label: None, position: None}), "link_ref")]
    #[case(Node::Math(Math{value: "".to_string(), position: None}), "math")]
    #[case(Node::List(List{index: 0, level: 0, checked: None, values: vec![], position: None}), "list")]
    #[case(Node::TableHeader(TableHeader{align: vec![], position: None}), "table_header")]
    #[case(Node::TableRow(TableRow{cells: vec![], position: None}), "table_row")]
    #[case(Node::TableCell(TableCell{column: 0, row: 0, last_cell_in_row: false, last_cell_of_in_table: false, values: vec![], position: None}), "table_cell")]
    #[case(Node::Code(Code{value: "".to_string(), lang: None, position: None}), "code")]
    #[case(Node::Strong(Value{values: vec![], position: None}), "strong")]
    #[case(Node::HorizontalRule{position: None}, "Horizontal_rule")]
    #[case(Node::MdxFlowExpression(mdast::MdxFlowExpression{value: "".to_string(), position: None, stops: vec![]}), "mdx_flow_expression")]
    #[case(Node::MdxJsxFlowElement(mdast::MdxJsxFlowElement{name: None, attributes: vec![], children: vec![], position: None}), "mdx_jsx_flow_element")]
    #[case(Node::MdxJsxTextElement(mdast::MdxJsxTextElement{name: None, attributes: vec![], children: vec![], position: None}), "mdx_jsx_text_element")]
    #[case(Node::MdxTextExpression(mdast::MdxTextExpression{value: "".to_string(), position: None, stops: vec![]}), "mdx_text_expression")]
    #[case(Node::MdxjsEsm(mdast::MdxjsEsm{value: "".to_string(), position: None, stops: vec![]}), "mdx_js_esm")]
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
    #[case(Node::Footnote(Footnote{ident: "test".to_string(), label: None, position: None}), "test")]
    #[case(Node::FootnoteRef(FootnoteRef{ident: "test".to_string(), label: None, position: None}), "test")]
    #[case(Node::Html(Html{value: "test".to_string(), position: None}), "test")]
    #[case(Node::Yaml(Yaml{value: "test".to_string(), position: None}), "test")]
    #[case(Node::Toml(Toml{value: "test".to_string(), position: None}), "test")]
    #[case(Node::Image(Image{alt: "alt".to_string(), url: "test".to_string(), title: None, position: None}), "test")]
    #[case(Node::ImageRef(ImageRef{alt: "alt".to_string(), ident: "test".to_string(), label: None, position: None}), "test")]
    #[case(Node::CodeInline(CodeInline{value: "test".to_string(), position: None}), "test")]
    #[case(Node::MathInline(MathInline{value: "test".to_string(), position: None}), "test")]
    #[case(Node::Link(Link{url: "test".to_string(), title: None, position: None}), "test")]
    #[case(Node::LinkRef(LinkRef{ident: "test".to_string(), label: None, position: None}), "test")]
    #[case(Node::Math(Math{value: "test".to_string(), position: None}), "test")]
    #[case(Node::Code(Code{value: "test".to_string(), lang: None, position: None}), "test")]
    #[case(Node::Strong(Value{values: vec![Node::Text(Text{value: "test".to_string(), position: None})], position: None}), "test")]
    #[case(Node::TableCell(TableCell{column: 0, row: 0, last_cell_in_row: false, last_cell_of_in_table: false, values: vec![Node::Text(Text{value: "test".to_string(), position: None})], position: None}), "test")]
    #[case(Node::TableRow(TableRow{cells: vec![Node::TableCell(TableCell{column: 0, row: 0, last_cell_in_row: false, last_cell_of_in_table: false, values: vec![Node::Text(Text{value: "test".to_string(), position: None})], position: None})], position: None}), "test")]
    #[case(Node::Break{position: None}, "")]
    #[case(Node::HorizontalRule{position: None}, "")]
    #[case(Node::TableHeader(TableHeader{align: vec![], position: None}), "")]
    #[case(Node::MdxFlowExpression(mdast::MdxFlowExpression{value: "test".to_string(), position: None, stops: vec![]}), "test")]
    #[case(Node::MdxTextExpression(mdast::MdxTextExpression{value: "test".to_string(), position: None, stops: vec![]}), "test")]
    #[case(Node::MdxjsEsm(mdast::MdxjsEsm{value: "test".to_string(), position: None, stops: vec![]}), "test")]
    fn test_value(#[case] node: Node, #[case] expected: &str) {
        assert_eq!(node.value(), expected);
    }

    #[rstest]
    #[case(Node::Text(Text{value: "test".to_string(), position: None}), None)]
    #[case(Node::Text(Text{value: "test".to_string(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    #[case(Node::List(List{index: 0, level: 0, checked: None, values: vec![], position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    #[case(Node::Blockquote(Value{values: vec![], position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    #[case(Node::Delete(Value{values: vec![], position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    #[case(Node::Heading(Heading{depth: 1, values: vec![], position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    #[case(Node::Emphasis(Value{values: vec![], position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    #[case(Node::Footnote(Footnote{ident: "".to_string(), label: None, position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    #[case(Node::FootnoteRef(FootnoteRef{ident: "".to_string(), label: None, position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    #[case(Node::Html(Html{value: "".to_string(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    #[case(Node::Yaml(Yaml{value: "".to_string(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    #[case(Node::Toml(Toml{value: "".to_string(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    #[case(Node::Image(Image{alt: "".to_string(), url: "".to_string(), title: None, position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    #[case(Node::ImageRef(ImageRef{alt: "".to_string(), ident: "".to_string(), label: None, position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    #[case(Node::CodeInline(CodeInline{value: "".to_string(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    #[case(Node::MathInline(MathInline{value: "".to_string(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    #[case(Node::Link(Link{url: "".to_string(), title: None, position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    #[case(Node::LinkRef(LinkRef{ident: "".to_string(), label: None, position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    #[case(Node::Math(Math{value: "".to_string(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    #[case(Node::Code(Code{value: "".to_string(), lang: None, position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    #[case(Node::Strong(Value{values: vec![], position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    #[case(Node::TableCell(TableCell{column: 0, row: 0, last_cell_in_row: false, last_cell_of_in_table: false, values: vec![], position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    #[case(Node::TableRow(TableRow{cells: vec![], position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    #[case(Node::TableHeader(TableHeader{align: vec![], position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    #[case(Node::Break{position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}, Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    #[case(Node::HorizontalRule{position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}, Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
    fn test_position(#[case] node: Node, #[case] expected: Option<Position>) {
        assert_eq!(node.position(), expected);
    }
}
