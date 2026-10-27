use crate::node::attr_value::{
    AttrValue,
    attr_keys::{self, CHILDREN},
};
use itertools::Itertools;
use smol_str::SmolStr;
use std::{
    borrow::Cow,
    fmt::{self, Display},
};

type ColorPair<'a> = (Cow<'a, str>, Cow<'a, str>);

mod attr;
pub mod attr_value;
mod render;
pub(crate) use render::{
    for_html, indent_lines, list_own_prefix_width, reindent_all_lines, render_before, render_cell_values,
    render_values_block, separated_from, values_to_string,
};
use render::{render_link_destination, render_link_title};
#[cfg(test)]
mod tests;

/// Color theme for rendering markdown nodes with optional ANSI escape codes.
///
/// Each field is a tuple of `(prefix, suffix)` ANSI escape code strings that
/// wrap the corresponding markdown element during colored rendering.
#[derive(Debug, Clone, Default)]
pub struct ColorTheme<'a> {
    /// Style for headings.
    pub heading: ColorPair<'a>,
    /// Style for code blocks.
    pub code: ColorPair<'a>,
    /// Style for inline code.
    pub code_inline: ColorPair<'a>,
    /// Style for emphasis.
    pub emphasis: ColorPair<'a>,
    /// Style for strong text.
    pub strong: ColorPair<'a>,
    /// Style for link text.
    pub link: ColorPair<'a>,
    /// Style for link destinations.
    pub link_url: ColorPair<'a>,
    /// Style for images.
    pub image: ColorPair<'a>,
    /// Style for block quote markers.
    pub blockquote_marker: ColorPair<'a>,
    /// Style for strikethrough.
    pub delete: ColorPair<'a>,
    /// Style for thematic breaks.
    pub horizontal_rule: ColorPair<'a>,
    /// Style for HTML.
    pub html: ColorPair<'a>,
    /// Style for front matter.
    pub frontmatter: ColorPair<'a>,
    /// Style for list markers.
    pub list_marker: ColorPair<'a>,
    /// Style for table separators.
    pub table_separator: ColorPair<'a>,
    /// Style for math.
    pub math: ColorPair<'a>,
}

const EMPTY: Cow<'_, str> = Cow::Borrowed("");
#[cfg(feature = "color")]
const RESET: Cow<'_, str> = Cow::Borrowed("\x1b[0m");

impl ColorTheme<'_> {
    /// A theme that adds no styling.
    pub const PLAIN: ColorTheme<'static> = ColorTheme {
        heading: (EMPTY, EMPTY),
        code: (EMPTY, EMPTY),
        code_inline: (EMPTY, EMPTY),
        emphasis: (EMPTY, EMPTY),
        strong: (EMPTY, EMPTY),
        link: (EMPTY, EMPTY),
        link_url: (EMPTY, EMPTY),
        image: (EMPTY, EMPTY),
        blockquote_marker: (EMPTY, EMPTY),
        delete: (EMPTY, EMPTY),
        horizontal_rule: (EMPTY, EMPTY),
        html: (EMPTY, EMPTY),
        frontmatter: (EMPTY, EMPTY),
        list_marker: (EMPTY, EMPTY),
        table_separator: (EMPTY, EMPTY),
        math: (EMPTY, EMPTY),
    };

    /// The default ANSI colored theme.
    #[cfg(feature = "color")]
    pub const COLORED: ColorTheme<'static> = ColorTheme {
        heading: (Cow::Borrowed("\x1b[1m\x1b[36m"), RESET),
        code: (Cow::Borrowed("\x1b[32m"), RESET),
        code_inline: (Cow::Borrowed("\x1b[32m"), RESET),
        emphasis: (Cow::Borrowed("\x1b[3m\x1b[33m"), RESET),
        strong: (Cow::Borrowed("\x1b[1m"), RESET),
        link: (Cow::Borrowed("\x1b[4m\x1b[34m"), RESET),
        link_url: (Cow::Borrowed("\x1b[34m"), RESET),
        image: (Cow::Borrowed("\x1b[35m"), RESET),
        blockquote_marker: (Cow::Borrowed("\x1b[2m"), RESET),
        delete: (Cow::Borrowed("\x1b[31m\x1b[2m"), RESET),
        horizontal_rule: (Cow::Borrowed("\x1b[2m"), RESET),
        html: (Cow::Borrowed("\x1b[2m"), RESET),
        frontmatter: (Cow::Borrowed("\x1b[2m"), RESET),
        list_marker: (Cow::Borrowed("\x1b[33m"), RESET),
        table_separator: (Cow::Borrowed("\x1b[2m"), RESET),
        math: (Cow::Borrowed("\x1b[32m"), RESET),
    };

    /// Creates a color theme from the `MQ_COLORS` environment variable.
    #[cfg(feature = "color")]
    pub fn from_env() -> ColorTheme<'static> {
        match std::env::var("MQ_COLORS") {
            Ok(v) if !v.is_empty() => ColorTheme::parse_colors(&v),
            _ => Self::COLORED,
        }
    }

    /// Parses a color configuration string into a `ColorTheme`.
    ///
    /// The format is `key=SGR:key=SGR:...` where each key corresponds to a
    /// markdown element and the value is a semicolon-separated list of SGR
    /// parameters. Unspecified keys use the default colored theme values.
    /// Invalid entries are silently ignored.
    #[cfg(feature = "color")]
    pub fn parse_colors(spec: &str) -> ColorTheme<'static> {
        let mut theme = Self::COLORED;

        for entry in spec.split(':') {
            let Some((key, sgr)) = entry.split_once('=') else {
                continue;
            };

            if !Self::is_valid_sgr(sgr) {
                continue;
            }

            let prefix = Cow::Owned(format!("\x1b[{}m", sgr));
            let pair = (prefix, RESET);

            match key {
                "heading" => theme.heading = pair,
                "code" => theme.code = pair,
                "code_inline" => theme.code_inline = pair,
                "emphasis" => theme.emphasis = pair,
                "strong" => theme.strong = pair,
                "link" => theme.link = pair,
                "link_url" => theme.link_url = pair,
                "image" => theme.image = pair,
                "blockquote" => theme.blockquote_marker = pair,
                "delete" => theme.delete = pair,
                "hr" => theme.horizontal_rule = pair,
                "html" => theme.html = pair,
                "frontmatter" => theme.frontmatter = pair,
                "list" => theme.list_marker = pair,
                "table" => theme.table_separator = pair,
                "math" => theme.math = pair,
                _ => {}
            }
        }

        theme
    }

    /// Validates that a string contains only valid SGR parameters
    /// (semicolon-separated numbers).
    #[cfg(feature = "color")]
    fn is_valid_sgr(sgr: &str) -> bool {
        !sgr.is_empty() && sgr.split(';').all(|part| part.parse::<u8>().is_ok())
    }
}

pub(crate) type Level = u8;

/// Options for writing nodes as Markdown.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RenderOptions {
    /// Bullet marker for lists. `None` keeps each list's original marker (`-` if unknown).
    pub list_style: Option<ListStyle>,
    /// How link destinations are wrapped.
    pub link_url_style: UrlSurroundStyle,
    /// How link titles are quoted.
    pub link_title_style: TitleSurroundStyle,
    /// Write for MDX: text that MDX would read as an expression or as ESM is escaped.
    pub mdx: bool,
}

/// Bullet marker used when writing lists.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum ListStyle {
    /// `-`
    #[default]
    Dash,
    /// `+`
    Plus,
    /// `*`
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

/// The marker of a list item as it is written in the source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
pub enum ListMarker {
    /// `-`, a bullet.
    #[cfg_attr(feature = "json", serde(rename = "-"))]
    Dash,
    /// `+`, a bullet.
    #[cfg_attr(feature = "json", serde(rename = "+"))]
    Plus,
    /// `*`, a bullet.
    #[cfg_attr(feature = "json", serde(rename = "*"))]
    Star,
    /// `.` after the number of an ordered item.
    #[cfg_attr(feature = "json", serde(rename = "."))]
    Period,
    /// `)` after the number of an ordered item.
    #[cfg_attr(feature = "json", serde(rename = ")"))]
    Paren,
}

impl ListMarker {
    /// The character the marker is written with.
    pub fn as_char(self) -> char {
        match self {
            Self::Dash => '-',
            Self::Plus => '+',
            Self::Star => '*',
            Self::Period => '.',
            Self::Paren => ')',
        }
    }

    /// The marker written with `char`: `-`, `+`, `*`, `.` or `)`.
    pub fn from_char(char: char) -> Option<Self> {
        match char {
            '-' => Some(Self::Dash),
            '+' => Some(Self::Plus),
            '*' => Some(Self::Star),
            '.' => Some(Self::Period),
            ')' => Some(Self::Paren),
            _ => None,
        }
    }

    /// The bullet style of the marker, if it is a bullet.
    pub fn style(self) -> Option<ListStyle> {
        match self {
            Self::Dash => Some(ListStyle::Dash),
            Self::Plus => Some(ListStyle::Plus),
            Self::Star => Some(ListStyle::Star),
            Self::Period | Self::Paren => None,
        }
    }
}

impl From<ListStyle> for ListMarker {
    fn from(style: ListStyle) -> Self {
        match style {
            ListStyle::Dash => Self::Dash,
            ListStyle::Plus => Self::Plus,
            ListStyle::Star => Self::Star,
        }
    }
}

/// The character a horizontal rule is written with in the source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
pub enum HorizontalRuleMarker {
    /// `*`
    #[cfg_attr(feature = "json", serde(rename = "*"))]
    Star,
    /// `-`
    #[cfg_attr(feature = "json", serde(rename = "-"))]
    Dash,
    /// `_`
    #[cfg_attr(feature = "json", serde(rename = "_"))]
    Underscore,
}

impl HorizontalRuleMarker {
    /// The character the rule is written with.
    pub fn as_char(self) -> char {
        match self {
            Self::Star => '*',
            Self::Dash => '-',
            Self::Underscore => '_',
        }
    }

    /// The marker written with `char`: `*`, `-` or `_`.
    pub fn from_char(char: char) -> Option<Self> {
        match char {
            '*' => Some(Self::Star),
            '-' => Some(Self::Dash),
            '_' => Some(Self::Underscore),
            _ => None,
        }
    }
}

/// A link destination.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase")
)]
pub struct Url(pub(crate) String);

/// How a link destination is wrapped when written.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum UrlSurroundStyle {
    /// Written as is.
    #[default]
    None,
    /// Wrapped in `<>`.
    Angle,
}

impl Url {
    /// Creates a value from `value`.
    pub fn new(value: String) -> Self {
        Self(value)
    }

    /// The destination as written.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Writes the node as Markdown with the given options.
    pub fn to_string_with(&self, options: &RenderOptions) -> Cow<'_, str> {
        Cow::Owned(render_link_destination(&self.0, &options.link_url_style))
    }
}

/// How a link title is quoted when written.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum TitleSurroundStyle {
    /// Wrapped in `"`.
    #[default]
    Double,
    /// Wrapped in `'`.
    Single,
    /// Wrapped in `()`.
    Paren,
}

/// A link title.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase")
)]
pub struct Title(pub(crate) String);

impl Display for Title {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Title {
    /// Creates a value from `value`.
    pub fn new(value: String) -> Self {
        Self(value)
    }

    /// The title text.
    pub fn to_value(&self) -> String {
        self.0.clone()
    }

    /// Writes the node as Markdown with the given options.
    pub fn to_string_with(&self, options: &RenderOptions) -> Cow<'_, str> {
        Cow::Owned(render_link_title(&self.0, &options.link_title_style))
    }
}

/// The alignment of a table column.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase")
)]
pub enum TableAlignKind {
    /// Left aligned.
    Left,
    /// Right aligned.
    Right,
    /// Centered.
    Center,
    /// No alignment given.
    None,
}

impl From<&str> for TableAlignKind {
    fn from(value: &str) -> Self {
        match value {
            "left" | ":---" => Self::Left,
            "right" | "---:" => Self::Right,
            "center" | ":---:" => Self::Center,
            "---" => Self::None,
            _ => Self::None,
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

/// A list item.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct List {
    /// The content of the item.
    pub values: Vec<Node>,
    /// Index of the item in its list.
    pub index: usize,
    /// Nesting level of the list.
    pub level: Level,
    /// Whether the list is ordered.
    pub ordered: bool,
    /// Task state: `None` if the item is not a task.
    pub checked: Option<bool>,
    /// Whether the whole list is loose (blank line between/within items).
    pub spread: bool,
    /// Starting number for an ordered list (e.g. `5` in `5. foo`); `None` means 1.
    #[cfg_attr(feature = "json", serde(skip_serializing_if = "Option::is_none"))]
    pub start: Option<u32>,
    /// Source marker. `None` keeps the default of the list kind: `-` for bullets, `.` for ordered lists.
    #[cfg_attr(feature = "json", serde(default, skip_serializing_if = "Option::is_none"))]
    pub marker: Option<ListMarker>,
    /// Where the node is in the source, if known.
    #[cfg_attr(feature = "json", serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
}

/// A table cell.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct TableCell {
    /// Child nodes.
    pub values: Vec<Node>,
    /// Index of the column.
    pub column: usize,
    /// Index of the row.
    pub row: usize,
    /// Where the node is in the source, if known.
    #[cfg_attr(feature = "json", serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
}

/// A table row.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct TableRow {
    /// Child nodes.
    pub values: Vec<Node>,
    /// Where the node is in the source, if known.
    #[cfg_attr(feature = "json", serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
}

/// The delimiter row of a table, giving the column alignments.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct TableAlign {
    /// Alignment of each column.
    pub align: Vec<TableAlignKind>,
    /// Where the node is in the source, if known.
    #[cfg_attr(feature = "json", serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
}

/// A sequence of nodes with no wrapper.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct Fragment {
    /// Child nodes.
    pub values: Vec<Node>,
}

/// A code block.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct Code {
    /// The code.
    pub value: String,
    /// The language of the info string.
    pub lang: Option<String>,
    /// Where the node is in the source, if known.
    #[cfg_attr(feature = "json", serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
    /// The rest of the info string after the language.
    pub meta: Option<String>,
    /// Whether the block is fenced rather than indented.
    pub fence: bool,
}

/// An inline image.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct Image {
    /// The alternative text.
    pub alt: String,
    /// The destination.
    pub url: String,
    /// The title, if any.
    pub title: Option<String>,
    /// Where the node is in the source, if known.
    #[cfg_attr(feature = "json", serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
}

/// A reference-style image.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct ImageRef {
    /// The alternative text.
    pub alt: String,
    /// The normalized identifier of the reference.
    pub ident: String,
    /// The identifier as written in the source.
    pub label: Option<String>,
    /// Where the node is in the source, if known.
    #[cfg_attr(feature = "json", serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
}
/// An inline link.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct Link {
    /// The destination.
    pub url: Url,
    /// The title, if any.
    pub title: Option<Title>,
    /// Child nodes.
    pub values: Vec<Node>,
    /// Where the node is in the source, if known.
    #[cfg_attr(feature = "json", serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
}

/// An Obsidian-style callout (admonition): `> [!TYPE]` or `> [!TYPE] title`.
#[cfg(feature = "callout")]
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct Callout {
    /// The callout type as written, e.g. `"NOTE"` or `"warning"`.
    pub kind: String,
    /// The fold marker right after `[!TYPE]`: `+` for open and `-` for folded.
    #[cfg_attr(feature = "json", serde(default, skip_serializing_if = "Option::is_none"))]
    pub fold: Option<char>,
    /// Optional custom title after the `[!TYPE]` marker.
    pub title: Option<String>,
    /// Body content nodes (the lines after the header).
    pub values: Vec<Node>,
    /// Where the node is in the source, if known.
    #[cfg_attr(feature = "json", serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
}

/// An Obsidian-style file embed: `![[target]]` or `![[target|display]]`.
#[cfg(feature = "embed")]
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct Embed {
    /// The target file or note name (e.g. `"image.png"`, `"note.md"`).
    pub target: String,
    /// Optional display hint after `|` (size for images, heading for notes).
    pub display: Option<String>,
    /// Where the node is in the source, if known.
    #[cfg_attr(feature = "json", serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
}

/// An Obsidian-style wikilink: `[[target]]` or `[[target|text]]`.
#[cfg(feature = "wikilink")]
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct WikiLink {
    /// The target page or file name (e.g. `"Three laws of motion"`).
    pub target: String,
    /// Optional display text (the part after `|`).
    pub text: Option<String>,
    /// Where the node is in the source, if known.
    #[cfg_attr(feature = "json", serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
}

/// A footnote reference.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct FootnoteRef {
    /// The normalized identifier of the reference.
    pub ident: String,
    /// The identifier as written in the source.
    pub label: Option<String>,
    /// Where the node is in the source, if known.
    #[cfg_attr(feature = "json", serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
}

/// A footnote definition.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct Footnote {
    /// The normalized identifier of the reference.
    pub ident: String,
    /// Child nodes.
    pub values: Vec<Node>,
    /// Where the node is in the source, if known.
    #[cfg_attr(feature = "json", serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
}

/// A reference-style link.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct LinkRef {
    /// The normalized identifier of the reference.
    pub ident: String,
    /// The identifier as written in the source.
    pub label: Option<String>,
    /// Child nodes.
    pub values: Vec<Node>,
    /// Where the node is in the source, if known.
    #[cfg_attr(feature = "json", serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
}

/// The depth of a heading, from 1 to 6.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(try_from = "u8", into = "u8")
)]
pub struct HeadingDepth(u8);

impl fmt::Debug for HeadingDepth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

impl HeadingDepth {
    /// Depth 1.
    pub const H1: Self = Self(1);
    /// Depth 2.
    pub const H2: Self = Self(2);
    /// Depth 3.
    pub const H3: Self = Self(3);
    /// Depth 4.
    pub const H4: Self = Self(4);
    /// Depth 5.
    pub const H5: Self = Self(5);
    /// Depth 6.
    pub const H6: Self = Self(6);

    /// The depth `depth`, if it is from 1 to 6.
    pub const fn new(depth: u8) -> Option<Self> {
        if depth >= 1 && depth <= 6 {
            Some(Self(depth))
        } else {
            None
        }
    }

    /// The depth nearest to `depth` from 1 to 6.
    pub fn saturating(depth: i64) -> Self {
        Self(depth.clamp(1, 6) as u8)
    }

    /// The depth as a number.
    pub const fn get(self) -> u8 {
        self.0
    }

    /// The depth `levels` deeper, if that is still at most 6.
    pub fn checked_add(self, levels: u8) -> Option<Self> {
        self.0.checked_add(levels).and_then(Self::new)
    }

    /// The depth `levels` shallower, or 1 when that is less.
    pub fn saturating_sub(self, levels: u8) -> Self {
        Self(self.0.saturating_sub(levels).max(1))
    }
}

impl Default for HeadingDepth {
    fn default() -> Self {
        Self::H1
    }
}

impl TryFrom<u8> for HeadingDepth {
    type Error = InvalidHeadingDepth;

    fn try_from(depth: u8) -> Result<Self, Self::Error> {
        Self::new(depth).ok_or(InvalidHeadingDepth(depth))
    }
}

impl From<HeadingDepth> for u8 {
    fn from(depth: HeadingDepth) -> Self {
        depth.0
    }
}

impl PartialEq<u8> for HeadingDepth {
    fn eq(&self, other: &u8) -> bool {
        self.0 == *other
    }
}

impl PartialEq<HeadingDepth> for u8 {
    fn eq(&self, other: &HeadingDepth) -> bool {
        *self == other.0
    }
}

impl PartialOrd<u8> for HeadingDepth {
    fn partial_cmp(&self, other: &u8) -> Option<std::cmp::Ordering> {
        self.0.partial_cmp(other)
    }
}

impl PartialOrd<HeadingDepth> for u8 {
    fn partial_cmp(&self, other: &HeadingDepth) -> Option<std::cmp::Ordering> {
        self.partial_cmp(&other.0)
    }
}

impl From<HeadingDepth> for usize {
    fn from(depth: HeadingDepth) -> Self {
        usize::from(depth.0)
    }
}

impl From<HeadingDepth> for i64 {
    fn from(depth: HeadingDepth) -> Self {
        i64::from(depth.0)
    }
}

impl Display for HeadingDepth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// A heading depth outside of 1 to 6.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidHeadingDepth(pub u8);

impl Display for InvalidHeadingDepth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "heading depth {} is not from 1 to 6", self.0)
    }
}

impl std::error::Error for InvalidHeadingDepth {}

/// A heading.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct Heading {
    /// The depth of the heading.
    pub depth: HeadingDepth,
    /// Child nodes.
    pub values: Vec<Node>,
    /// Where the node is in the source, if known.
    #[cfg_attr(feature = "json", serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
}

/// A link reference definition.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct Definition {
    /// Where the node is in the source, if known.
    #[cfg_attr(feature = "json", serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
    /// The destination of the definition.
    pub url: Url,
    /// The title, if any.
    pub title: Option<Title>,
    /// The normalized identifier of the reference.
    pub ident: String,
    /// The identifier as written in the source.
    pub label: Option<String>,
}
/// Plain text.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct Text {
    /// The text.
    pub value: String,
    /// Where the node is in the source, if known.
    #[cfg_attr(feature = "json", serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
}

/// Raw HTML.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct Html {
    /// The HTML source.
    pub value: String,
    /// Where the node is in the source, if known.
    #[cfg_attr(feature = "json", serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
}

/// TOML front matter.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct Toml {
    /// The TOML source.
    pub value: String,
    /// Where the node is in the source, if known.
    #[cfg_attr(feature = "json", serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
}

/// YAML front matter.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct Yaml {
    /// The YAML source.
    pub value: String,
    /// Where the node is in the source, if known.
    #[cfg_attr(feature = "json", serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
}

/// Inline code.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct CodeInline {
    /// The content.
    pub value: SmolStr,
    /// Where the node is in the source, if known.
    #[cfg_attr(feature = "json", serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
}

/// Inline math.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct MathInline {
    /// The content.
    pub value: SmolStr,
    /// Where the node is in the source, if known.
    #[cfg_attr(feature = "json", serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
}

/// A math block.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct Math {
    /// The LaTeX source.
    pub value: String,
    /// Where the node is in the source, if known.
    #[cfg_attr(feature = "json", serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
}

/// A block-level MDX expression.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct MdxFlowExpression {
    /// The source of the expression.
    pub value: SmolStr,
    /// Where the node is in the source, if known.
    #[cfg_attr(feature = "json", serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
}

/// A block-level JSX element.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct MdxJsxFlowElement {
    /// The child nodes.
    pub children: Vec<Node>,
    /// Where the node is in the source, if known.
    #[cfg_attr(feature = "json", serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
    /// The element name, or `None` for a fragment.
    pub name: Option<String>,
    /// The attributes of the element.
    pub attributes: Vec<MdxAttributeContent>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type", content = "value")
)]
/// An attribute of a JSX element: a property such as `a="b"`, or an expression such as `{...props}`.
pub enum MdxAttributeContent {
    /// An expression such as `{...props}`.
    Expression(SmolStr),
    /// A property such as `a="b"`.
    Property(MdxJsxAttribute),
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
/// A property of a JSX element, with the value it has after its quotes or braces.
pub struct MdxJsxAttribute {
    /// The name of the property.
    pub name: SmolStr,
    /// The value, or `None` for a property with none.
    pub value: Option<MdxAttributeValue>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type", content = "value")
)]
/// The value of a JSX property: a string, or an expression in braces.
pub enum MdxAttributeValue {
    /// An expression in braces.
    Expression(SmolStr),
    /// A string.
    Literal(SmolStr),
}

/// An inline JSX element.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct MdxJsxTextElement {
    /// The child nodes.
    pub children: Vec<Node>,
    /// Where the node is in the source, if known.
    #[cfg_attr(feature = "json", serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
    /// The element name, or `None` for a fragment.
    pub name: Option<SmolStr>,
    /// The attributes of the element.
    pub attributes: Vec<MdxAttributeContent>,
}

/// An inline MDX expression.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct MdxTextExpression {
    /// The source of the expression.
    pub value: SmolStr,
    /// Where the node is in the source, if known.
    #[cfg_attr(feature = "json", serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
}

/// MDX ESM `import` or `export`.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct MdxJsEsm {
    /// The source of the statements.
    pub value: SmolStr,
    /// Where the node is in the source, if known.
    #[cfg_attr(feature = "json", serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
}

/// A block quote.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct Blockquote {
    /// Child nodes.
    pub values: Vec<Node>,
    /// Where the node is in the source, if known.
    #[cfg_attr(feature = "json", serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
}

/// Strikethrough text.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct Delete {
    /// Child nodes.
    pub values: Vec<Node>,
    /// Where the node is in the source, if known.
    #[cfg_attr(feature = "json", serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
}

/// Emphasized text.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct Emphasis {
    /// Child nodes.
    pub values: Vec<Node>,
    /// Where the node is in the source, if known.
    #[cfg_attr(feature = "json", serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
}

/// Strongly emphasized text.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct Strong {
    /// Child nodes.
    pub values: Vec<Node>,
    /// Where the node is in the source, if known.
    #[cfg_attr(feature = "json", serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
}

/// A hard line break.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct Break {
    /// Where the node is in the source, if known.
    #[cfg_attr(feature = "json", serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
}

/// A thematic break.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", tag = "type")
)]
pub struct HorizontalRule {
    /// Source marker. `None` renders as `***`.
    #[cfg_attr(feature = "json", serde(default, skip_serializing_if = "Option::is_none"))]
    pub marker: Option<HorizontalRuleMarker>,
    /// Where the node is in the source, if known.
    #[cfg_attr(feature = "json", serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
}

/// A Markdown node.
#[derive(Debug, Clone, Default)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase", untagged)
)]
pub enum Node {
    /// A block quote.
    Blockquote(Blockquote),
    /// A hard line break.
    Break(Break),
    /// An Obsidian-style callout.
    #[cfg(feature = "callout")]
    Callout(Callout),
    /// An Obsidian-style file embed.
    #[cfg(feature = "embed")]
    Embed(Embed),
    /// A link reference definition.
    Definition(Definition),
    /// Strikethrough text.
    Delete(Delete),
    /// A heading.
    Heading(Heading),
    /// Emphasized text.
    Emphasis(Emphasis),
    /// A footnote definition.
    Footnote(Footnote),
    /// A footnote reference.
    FootnoteRef(FootnoteRef),
    /// Raw HTML.
    Html(Html),
    /// YAML front matter.
    Yaml(Yaml),
    /// TOML front matter.
    Toml(Toml),
    /// An inline image.
    Image(Image),
    /// A reference-style image.
    ImageRef(ImageRef),
    /// Inline code.
    CodeInline(CodeInline),
    /// Inline math.
    MathInline(MathInline),
    /// An inline link.
    Link(Link),
    /// A reference-style link.
    LinkRef(LinkRef),
    /// An Obsidian-style wikilink.
    #[cfg(feature = "wikilink")]
    WikiLink(WikiLink),
    /// A math block.
    Math(Math),
    /// A list item.
    List(List),
    /// The delimiter row of a table, giving the column alignments.
    TableAlign(TableAlign),
    /// A table row.
    TableRow(TableRow),
    /// A table cell.
    TableCell(TableCell),
    /// A code block.
    Code(Code),
    /// Strongly emphasized text.
    Strong(Strong),
    /// A thematic break.
    HorizontalRule(HorizontalRule),
    /// A block-level MDX expression.
    MdxFlowExpression(MdxFlowExpression),
    /// A block-level JSX element.
    MdxJsxFlowElement(MdxJsxFlowElement),
    /// An inline JSX element.
    MdxJsxTextElement(MdxJsxTextElement),
    /// An inline MDX expression.
    MdxTextExpression(MdxTextExpression),
    /// MDX ESM `import` or `export`.
    MdxJsEsm(MdxJsEsm),
    /// Plain text.
    Text(Text),
    /// A sequence of nodes with no wrapper.
    Fragment(Fragment),
    /// An empty node.
    #[default]
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
            (Some(self_pos), Some(other_pos)) => match self_pos.start.line.cmp(&other_pos.start.line) {
                std::cmp::Ordering::Equal => self_pos.start.column.partial_cmp(&other_pos.start.column),
                ordering => Some(ordering),
            },
            (Some(_), None) => Some(std::cmp::Ordering::Less),
            (None, Some(_)) => Some(std::cmp::Ordering::Greater),
            (None, None) => Some(self.name().cmp(&other.name())),
        }
    }
}

/// A span of the source.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase")
)]
pub struct Position {
    /// Where the node starts.
    pub start: Point,
    /// Where the node ends.
    pub end: Point,
}

/// A location in the source.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    feature = "json",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase")
)]
pub struct Point {
    /// The one-based line.
    pub line: u32,
    /// The one-based column, counted in UTF-16 code units. A tab is one column.
    pub column: u32,
}

impl From<String> for Node {
    fn from(value: String) -> Self {
        Self::Text(Text { value, position: None })
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
    /// Maps this node and its fragment descendants, cloning the input tree first.
    pub fn map_values<E, F>(&self, f: &mut F) -> Result<Node, E>
    where
        E: std::error::Error,
        F: FnMut(&Node) -> Result<Node, E>,
    {
        self.clone().map_values_into(f)
    }

    /// Maps this node and its fragment descendants, consuming the input tree.
    ///
    /// Prefer this over [`Self::map_values`] when the caller owns the node and does not need to
    /// retain an unchanged copy.
    pub fn map_values_into<E, F>(self, f: &mut F) -> Result<Node, E>
    where
        E: std::error::Error,
        F: FnMut(&Node) -> Result<Node, E>,
    {
        Self::_map_values(self, f)
    }

    /// Maps this node and its fragment descendants, giving the callback ownership of each node.
    ///
    /// This is useful when a transform must retain an unchanged fallback node without cloning it
    /// again after the callback returns. The traversal continues through a fragment returned by
    /// the callback, matching [`Self::map_values_into`]'s behavior.
    pub fn map_values_into_owned<E, F>(self, f: &mut F) -> Result<Node, E>
    where
        E: std::error::Error,
        F: FnMut(Node) -> Result<Node, E>,
    {
        Self::_map_values_owned(self, f)
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

    fn _map_values_owned<E, F>(node: Node, f: &mut F) -> Result<Node, E>
    where
        E: std::error::Error,
        F: FnMut(Node) -> Result<Node, E>,
    {
        match f(node)? {
            Node::Fragment(mut v) => {
                let values = v
                    .values
                    .into_iter()
                    .map(|node| Self::_map_values_owned(node, f))
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

    /// Converts a clone of this node into a fragment of its children.
    pub fn to_fragment(&self) -> Node {
        self.clone().into_fragment()
    }

    /// Converts this node into a fragment, preserving its children without cloning them.
    ///
    /// Leaf nodes become [`Node::Empty`]. This is the consuming counterpart to
    /// [`Self::to_fragment`].
    pub fn into_fragment(mut self) -> Node {
        if let Some(values) = self.fragment_values_mut() {
            return Self::Fragment(Fragment {
                values: std::mem::take(values),
            });
        }
        match self {
            node @ Node::Fragment(_) => node,
            _ => Self::Empty,
        }
    }

    /// Replaces the children of this node with those of `fragment`.
    pub fn apply_fragment(&mut self, fragment: Node) {
        Self::_apply_fragment(self, fragment)
    }

    fn _apply_fragment(node: &mut Node, fragment: Node) {
        let (Some(values), Node::Fragment(Fragment { values: new_values })) = (node.fragment_values_mut(), fragment)
        else {
            return;
        };
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

    /// The children of a node that has them, which [`Self::set_children`] replaces.
    fn children_mut(&mut self) -> Option<&mut Vec<Node>> {
        match self {
            Node::Footnote(Footnote { values, .. })
            | Node::Link(Link { values, .. })
            | Node::Heading(Heading { values, .. })
            | Node::List(List { values, .. })
            | Node::TableCell(TableCell { values, .. })
            | Node::TableRow(TableRow { values, .. })
            | Node::Strong(Strong { values, .. })
            | Node::Blockquote(Blockquote { values, .. })
            | Node::Delete(Delete { values, .. })
            | Node::Emphasis(Emphasis { values, .. })
            | Node::Fragment(Fragment { values }) => Some(values),
            #[cfg(feature = "callout")]
            Node::Callout(Callout { values, .. }) => Some(values),
            Node::MdxJsxFlowElement(MdxJsxFlowElement { children, .. })
            | Node::MdxJsxTextElement(MdxJsxTextElement { children, .. }) => Some(children),
            _ => None,
        }
    }

    /// The children that a fragment made from this node holds, and that applying a fragment maps onto.
    fn fragment_values_mut(&mut self) -> Option<&mut Vec<Node>> {
        match self {
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
            | Node::Strong(Strong { values, .. }) => Some(values),
            #[cfg(feature = "callout")]
            Node::Callout(Callout { values, .. }) => Some(values),
            _ => None,
        }
    }

    /// The children whose value [`Self::into_with_children_value`] replaces and whose text position
    /// [`Self::clear_text_position_at`] clears.
    fn editable_values_mut(&mut self) -> Option<&mut Vec<Node>> {
        match self {
            Node::Blockquote(Blockquote { values, .. })
            | Node::Delete(Delete { values, .. })
            | Node::Emphasis(Emphasis { values, .. })
            | Node::List(List { values, .. })
            | Node::TableCell(TableCell { values, .. })
            | Node::Strong(Strong { values, .. })
            | Node::LinkRef(LinkRef { values, .. })
            | Node::Heading(Heading { values, .. }) => Some(values),
            #[cfg(feature = "callout")]
            Node::Callout(Callout { values, .. }) => Some(values),
            Node::MdxJsxFlowElement(MdxJsxFlowElement { children, .. })
            | Node::MdxJsxTextElement(MdxJsxTextElement { children, .. }) => Some(children),
            _ => None,
        }
    }

    /// Returns the child nodes, or the node itself if it has none.
    pub fn node_values(&self) -> Vec<Node> {
        match self {
            Self::Blockquote(v) => v.values.clone(),
            Self::Delete(v) => v.values.clone(),
            Self::Heading(h) => h.values.clone(),
            Self::Emphasis(v) => v.values.clone(),
            Self::List(l) => l.values.clone(),
            Self::Strong(v) => v.values.clone(),
            #[cfg(feature = "callout")]
            Self::Callout(v) => v.values.clone(),
            _ => vec![self.clone()],
        }
    }

    /// Returns the child at `index`, if any.
    pub fn find_at_index(&self, index: usize) -> Option<Node> {
        match self {
            Self::Blockquote(v) => v.values.get(index).cloned(),
            Self::Delete(v) => v.values.get(index).cloned(),
            Self::Emphasis(v) => v.values.get(index).cloned(),
            Self::Strong(v) => v.values.get(index).cloned(),
            Self::Heading(v) => v.values.get(index).cloned(),
            Self::List(v) => v.values.get(index).cloned(),
            Self::TableCell(v) => v.values.get(index).cloned(),
            Self::TableRow(v) => v.values.get(index).cloned(),
            #[cfg(feature = "callout")]
            Self::Callout(v) => v.values.get(index).cloned(),
            _ => None,
        }
    }

    /// Returns the text content of the node.
    pub fn value(&self) -> String {
        match self {
            Self::Blockquote(v) => values_to_value(&v.values),
            Self::Definition(d) => d.url.as_str().to_string(),
            Self::Delete(v) => values_to_value(&v.values),
            Self::Heading(h) => values_to_value(&h.values),
            Self::Emphasis(v) => values_to_value(&v.values),
            Self::Footnote(f) => values_to_value(&f.values),
            Self::FootnoteRef(f) => f.ident.clone(),
            Self::Html(v) => v.value.clone(),
            Self::Yaml(v) => v.value.clone(),
            Self::Toml(v) => v.value.clone(),
            Self::Image(i) => i.url.clone(),
            Self::ImageRef(i) => i.ident.clone(),
            Self::CodeInline(v) => v.value.to_string(),
            Self::MathInline(v) => v.value.to_string(),
            Self::Link(l) => l.url.as_str().to_string(),
            Self::LinkRef(l) => l.ident.clone(),
            #[cfg(feature = "wikilink")]
            Self::WikiLink(w) => w.text.clone().unwrap_or_else(|| w.target.clone()),
            #[cfg(feature = "callout")]
            Self::Callout(v) => values_to_value(&v.values),
            #[cfg(feature = "embed")]
            Self::Embed(e) => e.display.clone().unwrap_or_else(|| e.target.clone()),
            Self::Math(v) => v.value.clone(),
            Self::List(l) => values_to_value(&l.values),
            Self::TableCell(c) => values_to_value(&c.values),
            Self::TableRow(c) => values_to_value(&c.values),
            Self::Code(c) => c.value.clone(),
            Self::Strong(v) => values_to_value(&v.values),
            Self::Text(t) => t.value.clone(),
            Self::Break { .. } => String::new(),
            Self::TableAlign(_) => String::new(),
            Self::MdxFlowExpression(mdx) => mdx.value.to_string(),
            Self::MdxJsxFlowElement(mdx) => values_to_value(&mdx.children),
            Self::MdxTextExpression(mdx) => mdx.value.to_string(),
            Self::MdxJsxTextElement(mdx) => values_to_value(&mdx.children),
            Self::MdxJsEsm(mdx) => mdx.value.to_string(),
            Self::HorizontalRule { .. } => String::new(),
            Self::Fragment(v) => values_to_value(&v.values),
            Self::Empty => String::new(),
        }
    }

    /// Returns the node name, such as `"heading"` or `"code"`.
    pub fn name(&self) -> SmolStr {
        match self {
            Self::Blockquote(_) => "blockquote".into(),
            Self::Break { .. } => "break".into(),
            Self::Definition(_) => "definition".into(),
            Self::Delete(_) => "delete".into(),
            Self::Heading(Heading { depth, .. }) => match depth.get() {
                1 => "h1".into(),
                2 => "h2".into(),
                3 => "h3".into(),
                4 => "h4".into(),
                5 => "h5".into(),
                _ => "h6".into(),
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
            #[cfg(feature = "wikilink")]
            Self::WikiLink(_) => "wikilink".into(),
            #[cfg(feature = "callout")]
            Self::Callout(_) => "callout".into(),
            #[cfg(feature = "embed")]
            Self::Embed(_) => "embed".into(),
            Self::Math(_) => "math".into(),
            Self::List(_) => "list".into(),
            Self::TableAlign(_) => "table_align".into(),
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

    /// Get the children nodes of the current node.
    pub fn children(&self) -> Vec<Node> {
        match self.attr(CHILDREN) {
            Some(AttrValue::Array(children)) => children,
            _ => Vec::new(),
        }
    }

    /// Sets the children nodes of the current node, if the node supports children.
    /// Nodes without children (e.g. `Text`, `Code`, `Image`) are left unchanged.
    pub fn set_children(&mut self, children: Vec<Node>) {
        if let Some(values) = self.children_mut() {
            *values = children;
        }
    }

    /// Sets the position of the node.
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
            #[cfg(feature = "wikilink")]
            Self::WikiLink(w) => w.position = pos,
            #[cfg(feature = "callout")]
            Self::Callout(c) => c.position = pos,
            #[cfg(feature = "embed")]
            Self::Embed(e) => e.position = pos,
            Self::Math(v) => v.position = pos,
            Self::Code(c) => c.position = pos,
            Self::TableCell(c) => c.position = pos,
            Self::TableRow(r) => r.position = pos,
            Self::TableAlign(c) => c.position = pos,
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

    /// Returns the position of the node, if known.
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
            #[cfg(feature = "wikilink")]
            Self::WikiLink(w) => w.position.clone(),
            #[cfg(feature = "callout")]
            Self::Callout(c) => c.position.clone(),
            #[cfg(feature = "embed")]
            Self::Embed(e) => e.position.clone(),
            Self::Math(v) => v.position.clone(),
            Self::Code(c) => c.position.clone(),
            Self::TableCell(c) => c.position.clone(),
            Self::TableRow(r) => r.position.clone(),
            Self::TableAlign(c) => c.position.clone(),
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
                let positions: Vec<Position> = v.values.iter().filter_map(|node| node.position()).collect();

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

    /// Recursively clears position information from this node and all of its children.
    /// Useful for shrinking structured output (JSON, YAML, etc.) when source spans
    /// aren't needed, since `Position` accounts for most of a serialized node's size.
    pub fn strip_positions(&mut self) {
        self.set_position(None);
        let mut children = self.children();
        if !children.is_empty() {
            for child in children.iter_mut() {
                child.strip_positions();
            }
            self.set_children(children);
        }
    }

    fn clear_position_at(values: &mut [Node], index: usize) {
        if let Some(slot) = values.get_mut(index) {
            slot.clear_text_position_at(0);
        }
    }

    /// Clears the position of just the `Text` leaf that `into_with_value` /
    /// `into_with_children_value` would write into at `index`, leaving this node and other
    /// descendants untouched. Used to stop the renderer re-escaping an already-escaped leaf.
    pub fn clear_text_position_at(&mut self, index: usize) {
        if let Self::Text(text) = self {
            text.position = None;
        } else if let Some(values) = self.editable_values_mut() {
            Self::clear_position_at(values, index);
        }
    }

    /// Returns `true` if this node is empty.
    pub fn is_empty(&self) -> bool {
        matches!(self, Self::Empty)
    }

    /// Returns `true` if this node is a fragment.
    pub fn is_fragment(&self) -> bool {
        matches!(self, Self::Fragment(_))
    }

    /// Returns `true` if this node is a fragment with no nodes.
    pub fn is_empty_fragment(&self) -> bool {
        if let Self::Fragment(_) = self {
            Self::_fragment_inner_nodes(self).is_empty()
        } else {
            false
        }
    }

    fn _fragment_inner_nodes(node: &Node) -> Vec<Node> {
        if let Self::Fragment(fragment) = node {
            fragment.values.iter().flat_map(Self::_fragment_inner_nodes).collect()
        } else if node.is_empty() {
            Vec::new()
        } else {
            vec![node.clone()]
        }
    }

    /// Returns `true` if this node is inline code.
    pub fn is_inline_code(&self) -> bool {
        matches!(self, Self::CodeInline(_))
    }

    /// Returns `true` if this node is inline math.
    pub fn is_inline_math(&self) -> bool {
        matches!(self, Self::MathInline(_))
    }

    /// Returns `true` if this node is strongly emphasized text.
    pub fn is_strong(&self) -> bool {
        matches!(self, Self::Strong(_))
    }

    /// Returns `true` if this node is a list item.
    pub fn is_list(&self) -> bool {
        matches!(self, Self::List(_))
    }

    /// Returns `true` if this node is emphasized text.
    pub fn is_emphasis(&self) -> bool {
        matches!(self, Self::Emphasis(_))
    }

    /// Returns `true` if this node is a strikethrough.
    pub fn is_delete(&self) -> bool {
        matches!(self, Self::Delete(_))
    }

    /// Returns `true` if this node is a link.
    pub fn is_link(&self) -> bool {
        #[cfg(feature = "wikilink")]
        {
            matches!(self, Self::Link(_) | Self::WikiLink(_))
        }
        #[cfg(not(feature = "wikilink"))]
        {
            matches!(self, Self::Link(_))
        }
    }

    /// Returns `true` if this node is a reference-style link.
    pub fn is_link_ref(&self) -> bool {
        matches!(self, Self::LinkRef(_))
    }

    /// Returns `true` if this node is an Obsidian-style wikilink (`[[target]]`).
    #[cfg(feature = "wikilink")]
    pub fn is_wikilink(&self) -> bool {
        matches!(self, Self::WikiLink(_))
    }

    /// Returns `true` if this node is an Obsidian-style callout (`> [!TYPE]`).
    #[cfg(feature = "callout")]
    pub fn is_callout(&self) -> bool {
        matches!(self, Self::Callout(_))
    }

    /// Returns `true` if this node is an Obsidian-style embed (`![[target]]`).
    #[cfg(feature = "embed")]
    pub fn is_embed(&self) -> bool {
        matches!(self, Self::Embed(_))
    }

    /// Returns `true` if this node is a text node.
    pub fn is_text(&self) -> bool {
        matches!(self, Self::Text(_))
    }

    /// Returns `true` if this node is an image.
    pub fn is_image(&self) -> bool {
        matches!(self, Self::Image(_))
    }

    /// Returns `true` if this node is a thematic break.
    pub fn is_horizontal_rule(&self) -> bool {
        matches!(self, Self::HorizontalRule { .. })
    }

    /// Returns `true` if this node is a block quote.
    pub fn is_blockquote(&self) -> bool {
        matches!(self, Self::Blockquote(_))
    }

    /// True for the nodes that make up the text of a paragraph.
    pub(crate) fn is_paragraph_text(&self) -> bool {
        #[cfg(feature = "wikilink")]
        if matches!(self, Self::WikiLink(_)) {
            return true;
        }
        #[cfg(feature = "embed")]
        if matches!(self, Self::Embed(_)) {
            return true;
        }
        matches!(
            self,
            Self::Text(_)
                | Self::Emphasis(_)
                | Self::Strong(_)
                | Self::Delete(_)
                | Self::CodeInline(_)
                | Self::MathInline(_)
                | Self::Link(_)
                | Self::LinkRef(_)
                | Self::Image(_)
                | Self::ImageRef(_)
                | Self::FootnoteRef(_)
                | Self::Html(_)
                | Self::MdxJsxTextElement(_)
                | Self::MdxTextExpression(_)
        )
    }

    /// True for block quotes and (when enabled) Obsidian-style callouts.
    pub(crate) fn is_blockquote_like(&self) -> bool {
        match self {
            Self::Blockquote(_) => true,
            #[cfg(feature = "callout")]
            Self::Callout(_) => true,
            _ => false,
        }
    }

    /// Returns `true` if this node is a raw HTML node.
    pub fn is_html(&self) -> bool {
        matches!(self, Self::Html { .. })
    }

    /// Returns `true` if this node is a footnote definition.
    pub fn is_footnote(&self) -> bool {
        matches!(self, Self::Footnote(_))
    }

    /// Returns `true` if this node is a block-level JSX element.
    pub fn is_mdx_jsx_flow_element(&self) -> bool {
        matches!(self, Self::MdxJsxFlowElement(MdxJsxFlowElement { .. }))
    }

    /// Returns `true` if this node is an MDX ESM statement.
    pub fn is_mdx_js_esm(&self) -> bool {
        matches!(self, Self::MdxJsEsm(MdxJsEsm { .. }))
    }

    /// Returns `true` if this node is TOML front matter.
    pub fn is_toml(&self) -> bool {
        matches!(self, Self::Toml { .. })
    }

    /// Returns `true` if this node is YAML front matter.
    pub fn is_yaml(&self) -> bool {
        matches!(self, Self::Yaml { .. })
    }

    /// Returns `true` if this node is a hard line break.
    pub fn is_break(&self) -> bool {
        matches!(self, Self::Break { .. })
    }

    /// Returns `true` if this node is an inline MDX expression.
    pub fn is_mdx_text_expression(&self) -> bool {
        matches!(self, Self::MdxTextExpression(MdxTextExpression { .. }))
    }

    /// Returns `true` if this node is a footnote reference.
    pub fn is_footnote_ref(&self) -> bool {
        matches!(self, Self::FootnoteRef { .. })
    }

    /// Returns `true` if this node is a reference-style image.
    pub fn is_image_ref(&self) -> bool {
        matches!(self, Self::ImageRef(_))
    }

    /// Returns `true` if this node is an inline JSX element.
    pub fn is_mdx_jsx_text_element(&self) -> bool {
        matches!(self, Self::MdxJsxTextElement(MdxJsxTextElement { .. }))
    }

    /// Returns `true` if this node is a math block.
    pub fn is_math(&self) -> bool {
        matches!(self, Self::Math(_))
    }

    /// Returns `true` if this node is a block-level MDX expression.
    pub fn is_mdx_flow_expression(&self) -> bool {
        matches!(self, Self::MdxFlowExpression(MdxFlowExpression { .. }))
    }

    /// Returns `true` if this node is a link reference definition.
    pub fn is_definition(&self) -> bool {
        matches!(self, Self::Definition(_))
    }

    /// Returns `true` if this node is a table delimiter row.
    pub fn is_table_align(&self) -> bool {
        matches!(self, Self::TableAlign(_))
    }

    /// Returns `true` if this node is a code block, of language `lang` when given.
    pub fn is_code(&self, lang: Option<SmolStr>) -> bool {
        if let Self::Code(Code { lang: node_lang, .. }) = &self {
            if lang.is_none() {
                true
            } else {
                node_lang.clone().unwrap_or_default() == lang.unwrap_or_default()
            }
        } else {
            false
        }
    }

    /// Returns `true` if this node is a heading, of depth `depth` when given.
    pub fn is_heading(&self, depth: Option<u8>) -> bool {
        if let Self::Heading(Heading {
            depth: heading_depth, ..
        }) = &self
        {
            depth.is_none_or(|depth| heading_depth.get() == depth)
        } else {
            false
        }
    }

    fn replace_value_at(values: &mut [Node], index: usize, value: &str) {
        if let Some(slot) = values.get_mut(index) {
            let node = std::mem::replace(slot, Self::Empty);
            *slot = node.into_with_value(value);
        }
    }

    /// Returns a clone of this node with its value replaced.
    pub fn with_value(&self, value: &str) -> Self {
        self.clone().into_with_value(value)
    }

    /// Replaces this node's value while consuming the original tree.
    ///
    /// Prefer this over [`Self::with_value`] when the caller owns the node and does not need an
    /// unchanged copy.
    pub fn into_with_value(self, value: &str) -> Self {
        match self {
            Self::Blockquote(mut v) => {
                Self::replace_value_at(&mut v.values, 0, value);

                Self::Blockquote(v)
            }
            Self::Delete(mut v) => {
                Self::replace_value_at(&mut v.values, 0, value);

                Self::Delete(v)
            }
            Self::Emphasis(mut v) => {
                Self::replace_value_at(&mut v.values, 0, value);

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
                Self::replace_value_at(&mut v.values, 0, value);

                Self::List(v)
            }
            Self::TableCell(mut v) => {
                Self::replace_value_at(&mut v.values, 0, value);

                Self::TableCell(v)
            }
            Self::TableRow(mut row) => {
                row.values = row
                    .values
                    .into_iter()
                    .zip(value.split(","))
                    .map(|(cell, value)| cell.into_with_value(value))
                    .collect::<Vec<_>>();

                Self::TableRow(row)
            }
            Self::Strong(mut v) => {
                Self::replace_value_at(&mut v.values, 0, value);

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
                Self::replace_value_at(&mut v.values, 0, value);

                Self::Heading(v)
            }
            Self::Definition(mut def) => {
                def.url = Url(value.to_string());
                Self::Definition(def)
            }
            node @ Self::Break { .. } => node,
            node @ Self::TableAlign(_) => node,
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
                Self::replace_value_at(&mut mdx.children, 0, value);

                Self::MdxJsxFlowElement(MdxJsxFlowElement {
                    name: mdx.name,
                    attributes: mdx.attributes,
                    children: mdx.children,
                    ..mdx
                })
            }
            Self::MdxJsxTextElement(mut mdx) => {
                Self::replace_value_at(&mut mdx.children, 0, value);

                Self::MdxJsxTextElement(MdxJsxTextElement {
                    name: mdx.name,
                    attributes: mdx.attributes,
                    children: mdx.children,
                    ..mdx
                })
            }
            node @ Self::Fragment(_) | node @ Self::Empty => node,
            #[cfg(feature = "wikilink")]
            Self::WikiLink(mut w) => {
                if w.text.is_some() {
                    w.text = Some(value.to_string());
                } else {
                    w.target = value.to_string();
                }
                Self::WikiLink(w)
            }
            #[cfg(feature = "callout")]
            Self::Callout(mut c) => {
                Self::replace_value_at(&mut c.values, 0, value);
                Self::Callout(c)
            }
            #[cfg(feature = "embed")]
            Self::Embed(mut e) => {
                if e.display.is_some() {
                    e.display = Some(value.to_string());
                } else {
                    e.target = value.to_string();
                }
                Self::Embed(e)
            }
        }
    }

    /// Returns a clone of this node with the selected child's value replaced.
    pub fn with_children_value(&self, value: &str, index: usize) -> Self {
        self.clone().into_with_children_value(value, index)
    }

    /// Replaces a selected child's value while consuming the original tree.
    ///
    /// Prefer this over [`Self::with_children_value`] when the caller owns the node and does not
    /// need an unchanged copy.
    pub fn into_with_children_value(mut self, value: &str, index: usize) -> Self {
        if let Some(values) = self.editable_values_mut() {
            Self::replace_value_at(values, index, value);
        }
        self
    }
}

fn values_to_value(values: &[Node]) -> String {
    values.iter().map(|value| value.value()).collect::<String>()
}
