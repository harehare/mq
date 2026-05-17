use std::fmt::{self, Display, Formatter};

#[cfg(feature = "ast-json")]
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{Ident, Token, TokenKind};

/// Error type returned when an unknown selector is encountered during parsing.
#[derive(Error, Clone, Debug, PartialOrd, Eq, Ord, PartialEq)]
#[error("Unknown selector `{0}`")]
pub struct UnknownSelector(pub Token);

/// Parses a bracket-based selector string like `.[n]` (List) or `.[n][m]` (Table).
///
/// Returns `Some(Selector)` if the string matches, `None` otherwise.
fn parse_bracket_selector(s: &str) -> Option<Selector> {
    let inner = s.strip_prefix(".[")?;
    let (first, rest) = inner.split_once(']')?;

    if !first.is_empty() && !first.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let first_idx: Option<usize> = if first.is_empty() {
        None
    } else {
        Some(first.parse().ok()?)
    };

    if rest.is_empty() {
        // ".[n]" → List
        return Some(Selector::List(first_idx, None));
    }

    let inner2 = rest.strip_prefix('[')?;
    let (second, tail) = inner2.split_once(']')?;
    if !tail.is_empty() {
        return None;
    }
    if !second.is_empty() && !second.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let second_idx: Option<usize> = if second.is_empty() {
        None
    } else {
        Some(second.parse().ok()?)
    };
    // ".[n][m]" → Table
    Some(Selector::Table(first_idx, second_idx))
}

impl UnknownSelector {
    /// Creates a new `UnknownSelector` error with the given token.
    pub fn new(token: Token) -> Self {
        Self(token)
    }
}

/// Unescapes `\"` and `\\` sequences in a quoted property key.
fn unescape_property_key(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(next) = chars.next() {
                result.push(next);
            }
        } else {
            result.push(c);
        }
    }
    result
}

/// Escapes `"` and `\` characters in a property key for display as `."key"`.
fn escape_property_key(key: &str) -> String {
    let mut result = String::with_capacity(key.len() + 2);
    for c in key.chars() {
        match c {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            _ => result.push(c),
        }
    }
    result
}

/// A selector for matching specific types of markdown nodes.
///
/// Selectors are used to query and filter markdown documents, similar to CSS selectors
/// for HTML. Each variant matches a specific type of markdown element.
#[cfg_attr(feature = "ast-json", derive(Serialize, Deserialize))]
#[derive(PartialEq, PartialOrd, Debug, Eq, Clone)]
pub enum Selector {
    /// Matches blockquote elements (e.g., `> quoted text`).
    Blockquote,
    /// Matches footnote definitions.
    Footnote,
    /// Matches list elements.
    ///
    /// The first `Option<usize>` specifies an item index, the second `Option<bool>` indicates ordered/unordered.
    List(Option<usize>, Option<bool>),
    /// Matches TOML frontmatter blocks.
    Toml,
    /// Matches YAML frontmatter blocks.
    Yaml,
    /// Matches line break elements.
    Break,
    /// Matches inline code elements (e.g., `` `code` ``).
    InlineCode,
    /// Matches inline math elements (e.g., `$math$`).
    InlineMath,
    /// Matches strikethrough/delete elements (e.g., `~~text~~`).
    Delete,
    /// Matches emphasis elements (e.g., `*text*` or `_text_`).
    Emphasis,
    /// Matches footnote references (e.g., `[^1]`).
    FootnoteRef,
    /// Matches raw HTML elements.
    Html,
    /// Matches image elements (e.g., `![alt](url)`).
    Image,
    /// Matches image reference elements (e.g., `![alt][ref]`).
    ImageRef,
    /// Matches MDX JSX text elements.
    MdxJsxTextElement,
    /// Matches link elements (e.g., `[text](url)`).
    Link,
    /// Matches link reference elements (e.g., `[text][ref]`).
    LinkRef,
    /// Matches strong/bold elements (e.g., `**text**`).
    Strong,
    /// Matches code block elements.
    Code,
    /// Matches math block elements (e.g., `$$math$$`).
    Math,
    /// Matches heading elements.
    ///
    /// The `Option<u8>` specifies the heading level (1-6). If `None`, matches any heading level.
    Heading(Option<u8>),
    /// Matches table elements.
    ///
    /// The first `Option<usize>` specifies row index, the second specifies column index.
    Table(Option<usize>, Option<usize>),
    /// Matches table alignment elements.
    TableAlign,
    /// Matches text nodes.
    Text,
    /// Matches horizontal rule elements (e.g., `---`, `***`, `___`).
    HorizontalRule,
    /// Matches link/image definition elements.
    Definition,
    /// Matches MDX flow expression elements.
    MdxFlowExpression,
    /// Matches MDX text expression elements.
    MdxTextExpression,
    /// Matches MDX ES module import/export elements.
    MdxJsEsm,
    /// Matches MDX JSX flow elements.
    MdxJsxFlowElement,
    /// Matches recursively all child nodes.
    Recursive,
    /// Matches a task list markdown node.
    Task,
    /// Matches a task list markdown node with an unchecked status.
    Todo,
    /// Matches a task list markdown node with a checked status.
    Done,
    /// Matches a specific attribute of a markdown node.
    Attr(AttrKind),
    /// Matches a specific property of a dict or array.
    Property(Ident),
}

/// Represents an attribute that can be accessed from markdown nodes.
///
/// Attributes allow extracting specific properties from markdown elements,
/// such as the URL from a link or the language from a code block.
#[cfg_attr(feature = "ast-json", derive(Serialize, Deserialize))]
#[derive(PartialEq, PartialOrd, Debug, Eq, Clone)]
pub enum AttrKind {
    /// The text value or content of a node.
    Value,
    /// Collection of values (used for certain node types).
    Values,
    /// The children nodes of an element.
    Children,

    /// The programming language identifier for code blocks.
    Lang,
    /// Additional metadata for code blocks.
    Meta,
    /// The fence character used for code blocks (e.g., `` ` `` or `~`).
    Fence,

    /// The URL for links and images.
    Url,
    /// The alt text for images.
    Alt,
    /// The title attribute for links and images.
    Title,

    /// The identifier for references (LinkRef, ImageRef, FootnoteRef, Definition, Footnote).
    Ident,
    /// The label for references.
    Label,

    /// The depth level of a heading (1-6).
    Depth,
    /// Alias for `Depth` - the level of a heading.
    Level,

    /// The index of a list item within its parent list.
    Index,
    /// Whether a list is ordered (numbered) or unordered.
    Ordered,
    /// The checked status of a task list item.
    Checked,

    /// The column index of a table cell.
    Column,
    /// The row index of a table cell.
    Row,

    /// The alignment of a table header (left, right, center, none).
    Align,

    /// The name attribute for MDX JSX elements.
    Name,
}

impl Display for AttrKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            AttrKind::Value => write!(f, ".value"),
            AttrKind::Values => write!(f, ".values"),
            AttrKind::Children => write!(f, ".children"),
            AttrKind::Lang => write!(f, ".lang"),
            AttrKind::Meta => write!(f, ".meta"),
            AttrKind::Fence => write!(f, ".fence"),
            AttrKind::Url => write!(f, ".url"),
            AttrKind::Alt => write!(f, ".alt"),
            AttrKind::Title => write!(f, ".title"),
            AttrKind::Ident => write!(f, ".ident"),
            AttrKind::Label => write!(f, ".label"),
            AttrKind::Depth => write!(f, ".depth"),
            AttrKind::Level => write!(f, ".level"),
            AttrKind::Index => write!(f, ".index"),
            AttrKind::Ordered => write!(f, ".ordered"),
            AttrKind::Checked => write!(f, ".checked"),
            AttrKind::Column => write!(f, ".column"),
            AttrKind::Row => write!(f, ".row"),
            AttrKind::Align => write!(f, ".align"),
            AttrKind::Name => write!(f, ".name"),
        }
    }
}

impl Selector {
    /// Converts a dot-prefixed selector string (e.g. `".text"`, `".h"`) to a `Selector`.
    ///
    /// Returns `None` for unknown or non-simple selectors (bracket forms, quoted keys).
    pub fn from_selector_str(s: &str) -> Option<Self> {
        match s {
            ".h" | ".heading" => Some(Selector::Heading(None)),
            ".h1" => Some(Selector::Heading(Some(1))),
            ".h2" => Some(Selector::Heading(Some(2))),
            ".h3" => Some(Selector::Heading(Some(3))),
            ".h4" => Some(Selector::Heading(Some(4))),
            ".h5" => Some(Selector::Heading(Some(5))),
            ".h6" => Some(Selector::Heading(Some(6))),
            ".>" | ".blockquote" => Some(Selector::Blockquote),
            ".^" | ".footnote" => Some(Selector::Footnote),
            ".<" | ".mdx_jsx_flow_element" => Some(Selector::MdxJsxFlowElement),
            ".**" | ".emphasis" => Some(Selector::Emphasis),
            ".$$" | ".math" => Some(Selector::Math),
            ".horizontal_rule" | ".---" | ".***" | ".___" => Some(Selector::HorizontalRule),
            ".{}" | ".mdx_text_expression" => Some(Selector::MdxTextExpression),
            ".[^]" | ".footnote_ref" => Some(Selector::FootnoteRef),
            ".definition" => Some(Selector::Definition),
            ".break" => Some(Selector::Break),
            ".delete" => Some(Selector::Delete),
            ".<>" | ".html" => Some(Selector::Html),
            ".image" => Some(Selector::Image),
            ".image_ref" => Some(Selector::ImageRef),
            ".code_inline" => Some(Selector::InlineCode),
            ".math_inline" => Some(Selector::InlineMath),
            ".link" => Some(Selector::Link),
            ".link_ref" => Some(Selector::LinkRef),
            ".[]" | ".list" => Some(Selector::List(None, None)),
            ".task" => Some(Selector::Task),
            ".todo" => Some(Selector::Todo),
            ".done" => Some(Selector::Done),
            ".toml" => Some(Selector::Toml),
            ".strong" => Some(Selector::Strong),
            ".yaml" => Some(Selector::Yaml),
            ".code" => Some(Selector::Code),
            ".mdx_js_esm" => Some(Selector::MdxJsEsm),
            ".mdx_jsx_text_element" => Some(Selector::MdxJsxTextElement),
            ".mdx_flow_expression" => Some(Selector::MdxFlowExpression),
            ".text" => Some(Selector::Text),
            ".[][]" | ".table" => Some(Selector::Table(None, None)),
            ".table_align" => Some(Selector::TableAlign),
            ".." => Some(Selector::Recursive),
            ".value" => Some(Selector::Attr(AttrKind::Value)),
            ".values" => Some(Selector::Attr(AttrKind::Values)),
            ".children" | ".cn" => Some(Selector::Attr(AttrKind::Children)),
            ".lang" => Some(Selector::Attr(AttrKind::Lang)),
            ".meta" => Some(Selector::Attr(AttrKind::Meta)),
            ".fence" => Some(Selector::Attr(AttrKind::Fence)),
            ".url" => Some(Selector::Attr(AttrKind::Url)),
            ".alt" => Some(Selector::Attr(AttrKind::Alt)),
            ".title" => Some(Selector::Attr(AttrKind::Title)),
            ".ident" => Some(Selector::Attr(AttrKind::Ident)),
            ".label" => Some(Selector::Attr(AttrKind::Label)),
            ".depth" => Some(Selector::Attr(AttrKind::Depth)),
            ".level" => Some(Selector::Attr(AttrKind::Level)),
            ".index" => Some(Selector::Attr(AttrKind::Index)),
            ".ordered" => Some(Selector::Attr(AttrKind::Ordered)),
            ".checked" => Some(Selector::Attr(AttrKind::Checked)),
            ".column" => Some(Selector::Attr(AttrKind::Column)),
            ".row" => Some(Selector::Attr(AttrKind::Row)),
            ".align" => Some(Selector::Attr(AttrKind::Align)),
            ".name" => Some(Selector::Attr(AttrKind::Name)),
            _ => None,
        }
    }
}

impl TryFrom<&Token> for Selector {
    type Error = UnknownSelector;

    fn try_from(token: &Token) -> Result<Self, Self::Error> {
        if let TokenKind::Selector(s) = &token.kind {
            if let Some(sel) = Self::from_selector_str(s.as_str()) {
                return Ok(sel);
            }
            if let Some(sel) = parse_bracket_selector(s.as_str()) {
                return Ok(sel);
            }
            // Quoted property selector: ."key" is the only way to access dict keys
            if let Some(quoted) = s.strip_prefix(".\"").and_then(|r| r.strip_suffix('"')) {
                return Ok(Selector::Property(Ident::new(&unescape_property_key(quoted))));
            }
            Err(UnknownSelector(token.clone()))
        } else {
            Err(UnknownSelector(token.clone()))
        }
    }
}

impl Display for Selector {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            Selector::Heading(None) => write!(f, ".h"),
            Selector::Heading(Some(1)) => write!(f, ".h1"),
            Selector::Heading(Some(2)) => write!(f, ".h2"),
            Selector::Heading(Some(3)) => write!(f, ".h3"),
            Selector::Heading(Some(4)) => write!(f, ".h4"),
            Selector::Heading(Some(5)) => write!(f, ".h5"),
            Selector::Heading(Some(6)) => write!(f, ".h6"),
            Selector::Heading(Some(n)) => write!(f, ".h{}", n),
            Selector::Blockquote => write!(f, ".blockquote"),
            Selector::Footnote => write!(f, ".footnote"),
            Selector::List(None, None) => write!(f, ".list"),
            Selector::List(Some(idx), None) => write!(f, ".[{}]", idx),
            Selector::List(Some(idx), _) => write!(f, ".[{}]", idx),
            Selector::List(None, _) => write!(f, ".[]"),
            Selector::Toml => write!(f, ".toml"),
            Selector::Yaml => write!(f, ".yaml"),
            Selector::Break => write!(f, ".break"),
            Selector::InlineCode => write!(f, ".code_inline"),
            Selector::InlineMath => write!(f, ".math_inline"),
            Selector::Delete => write!(f, ".delete"),
            Selector::Emphasis => write!(f, ".emphasis"),
            Selector::FootnoteRef => write!(f, ".footnote_ref"),
            Selector::Html => write!(f, ".html"),
            Selector::Image => write!(f, ".image"),
            Selector::ImageRef => write!(f, ".image_ref"),
            Selector::MdxJsxTextElement => write!(f, ".mdx_jsx_text_element"),
            Selector::Link => write!(f, ".link"),
            Selector::LinkRef => write!(f, ".link_ref"),
            Selector::Strong => write!(f, ".strong"),
            Selector::Code => write!(f, ".code"),
            Selector::Math => write!(f, ".math"),
            Selector::Table(None, None) => write!(f, ".table"),
            Selector::Table(Some(row), None) => write!(f, ".[{}][]", row),
            Selector::Table(Some(row), Some(col)) => write!(f, ".[{}][{}]", row, col),
            Selector::Table(None, Some(col)) => write!(f, ".[][{}]", col),
            Selector::TableAlign => write!(f, ".table_align"),
            Selector::Text => write!(f, ".text"),
            Selector::HorizontalRule => write!(f, ".horizontal_rule"),
            Selector::Definition => write!(f, ".definition"),
            Selector::MdxFlowExpression => write!(f, ".mdx_flow_expression"),
            Selector::MdxTextExpression => write!(f, ".mdx_text_expression"),
            Selector::MdxJsEsm => write!(f, ".mdx_js_esm"),
            Selector::MdxJsxFlowElement => write!(f, ".mdx_jsx_flow_element"),
            Selector::Recursive => write!(f, ".."),
            Selector::Task => write!(f, ".task"),
            Selector::Todo => write!(f, ".todo"),
            Selector::Done => write!(f, ".done"),
            Selector::Attr(attr) => write!(f, "{}", attr),
            Selector::Property(property) => write!(f, ".\"{}\"", escape_property_key(&property.as_str())),
        }
    }
}

impl Selector {
    /// Returns `true` if this is an attribute selector.
    pub fn is_attribute_selector(&self) -> bool {
        matches!(self, Selector::Attr(_))
    }

    /// Returns the selector as a string without a leading dot.
    pub fn name(&self) -> String {
        self.to_string().trim_start_matches('.').to_string()
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        ArenaId, Position, Range, Token, TokenKind,
        selector::{AttrKind, Selector, UnknownSelector},
    };
    use rstest::rstest;
    use smol_str::SmolStr;

    #[rstest]
    // Heading selectors
    #[case::heading(".h", Selector::Heading(None), ".h")]
    #[case::heading_h1(".h1", Selector::Heading(Some(1)), ".h1")]
    #[case::heading_h2(".h2", Selector::Heading(Some(2)), ".h2")]
    #[case::heading_h3(".h3", Selector::Heading(Some(3)), ".h3")]
    #[case::heading_h4(".h4", Selector::Heading(Some(4)), ".h4")]
    #[case::heading_h5(".h5", Selector::Heading(Some(5)), ".h5")]
    #[case::heading_h6(".h6", Selector::Heading(Some(6)), ".h6")]
    // Blockquote
    #[case::blockquote(".blockquote", Selector::Blockquote, ".blockquote")]
    #[case::blockquote_alias(".>", Selector::Blockquote, ".blockquote")]
    // Footnote
    #[case::footnote(".footnote", Selector::Footnote, ".footnote")]
    #[case::footnote_alias(".^", Selector::Footnote, ".footnote")]
    // MDX JSX Flow Element
    #[case::mdx_jsx_flow_element(".mdx_jsx_flow_element", Selector::MdxJsxFlowElement, ".mdx_jsx_flow_element")]
    #[case::mdx_jsx_flow_element_alias(".<", Selector::MdxJsxFlowElement, ".mdx_jsx_flow_element")]
    // Emphasis
    #[case::emphasis(".emphasis", Selector::Emphasis, ".emphasis")]
    #[case::emphasis_alias(".**", Selector::Emphasis, ".emphasis")]
    // Math
    #[case::math(".math", Selector::Math, ".math")]
    #[case::math_alias(".$$", Selector::Math, ".math")]
    // Horizontal Rule
    #[case::horizontal_rule(".horizontal_rule", Selector::HorizontalRule, ".horizontal_rule")]
    #[case::horizontal_rule_alias_dash(".---", Selector::HorizontalRule, ".horizontal_rule")]
    #[case::horizontal_rule_alias_star(".***", Selector::HorizontalRule, ".horizontal_rule")]
    #[case::horizontal_rule_alias_underscore(".___", Selector::HorizontalRule, ".horizontal_rule")]
    // MDX Text Expression
    #[case::mdx_text_expression(".mdx_text_expression", Selector::MdxTextExpression, ".mdx_text_expression")]
    #[case::mdx_text_expression_alias(".{}", Selector::MdxTextExpression, ".mdx_text_expression")]
    // Footnote Reference
    #[case::footnote_ref(".footnote_ref", Selector::FootnoteRef, ".footnote_ref")]
    #[case::footnote_ref_alias(".[^]", Selector::FootnoteRef, ".footnote_ref")]
    // Definition
    #[case::definition(".definition", Selector::Definition, ".definition")]
    // Break
    #[case::break_selector(".break", Selector::Break, ".break")]
    // Delete
    #[case::delete(".delete", Selector::Delete, ".delete")]
    // HTML
    #[case::html(".html", Selector::Html, ".html")]
    #[case::html_alias(".<>", Selector::Html, ".html")]
    // Image
    #[case::image(".image", Selector::Image, ".image")]
    // Image Reference
    #[case::image_ref(".image_ref", Selector::ImageRef, ".image_ref")]
    // Inline Code
    #[case::code_inline(".code_inline", Selector::InlineCode, ".code_inline")]
    // Inline Math
    #[case::math_inline(".math_inline", Selector::InlineMath, ".math_inline")]
    // Link
    #[case::link(".link", Selector::Link, ".link")]
    // Link Reference
    #[case::link_ref(".link_ref", Selector::LinkRef, ".link_ref")]
    // List
    #[case::list(".list", Selector::List(None, None), ".list")]
    #[case::list_bracket(".[]", Selector::List(None, None), ".list")]
    #[case::list_with_index(".[1]", Selector::List(Some(1), None), ".[1]")]
    // Task List
    #[case::task(".task", Selector::Task, ".task")]
    #[case::task(".todo", Selector::Todo, ".todo")]
    #[case::task(".done", Selector::Done, ".done")]
    // TOML
    #[case::toml(".toml", Selector::Toml, ".toml")]
    // Strong
    #[case::strong(".strong", Selector::Strong, ".strong")]
    // YAML
    #[case::yaml(".yaml", Selector::Yaml, ".yaml")]
    // Code
    #[case::code(".code", Selector::Code, ".code")]
    // MDX JS ESM
    #[case::mdx_js_esm(".mdx_js_esm", Selector::MdxJsEsm, ".mdx_js_esm")]
    // MDX JSX Text Element
    #[case::mdx_jsx_text_element(".mdx_jsx_text_element", Selector::MdxJsxTextElement, ".mdx_jsx_text_element")]
    // MDX Flow Expression
    #[case::mdx_flow_expression(".mdx_flow_expression", Selector::MdxFlowExpression, ".mdx_flow_expression")]
    // Text
    #[case::text(".text", Selector::Text, ".text")]
    // Table
    #[case::table(".table", Selector::Table(None, None), ".table")]
    #[case::table_bracket(".[][]", Selector::Table(None, None), ".table")]
    #[case::table_row_any(".[1][]", Selector::Table(Some(1), None), ".[1][]")]
    #[case::table_row_col(".[1][2]", Selector::Table(Some(1), Some(2)), ".[1][2]")]
    #[case::table_any_col(".[][2]", Selector::Table(None, Some(2)), ".[][2]")]
    // Table Align
    #[case::table_align(".table_align", Selector::TableAlign, ".table_align")]
    // Recursive
    #[case::recursive("..", Selector::Recursive, "..")]
    // Attribute selectors - Common
    #[case::attr_value(".value", Selector::Attr(AttrKind::Value), ".value")]
    #[case::attr_values(".values", Selector::Attr(AttrKind::Values), ".values")]
    #[case::attr_children(".children", Selector::Attr(AttrKind::Children), ".children")]
    // Attribute selectors - Code
    #[case::attr_lang(".lang", Selector::Attr(AttrKind::Lang), ".lang")]
    #[case::attr_meta(".meta", Selector::Attr(AttrKind::Meta), ".meta")]
    #[case::attr_fence(".fence", Selector::Attr(AttrKind::Fence), ".fence")]
    // Attribute selectors - Link/Image
    #[case::attr_url(".url", Selector::Attr(AttrKind::Url), ".url")]
    #[case::attr_alt(".alt", Selector::Attr(AttrKind::Alt), ".alt")]
    #[case::attr_title(".title", Selector::Attr(AttrKind::Title), ".title")]
    // Attribute selectors - Reference
    #[case::attr_ident(".ident", Selector::Attr(AttrKind::Ident), ".ident")]
    #[case::attr_label(".label", Selector::Attr(AttrKind::Label), ".label")]
    // Attribute selectors - Heading
    #[case::attr_depth(".depth", Selector::Attr(AttrKind::Depth), ".depth")]
    #[case::attr_level(".level", Selector::Attr(AttrKind::Level), ".level")]
    // Attribute selectors - List
    #[case::attr_index(".index", Selector::Attr(AttrKind::Index), ".index")]
    #[case::attr_ordered(".ordered", Selector::Attr(AttrKind::Ordered), ".ordered")]
    #[case::attr_checked(".checked", Selector::Attr(AttrKind::Checked), ".checked")]
    // Attribute selectors - TableCell
    #[case::attr_column(".column", Selector::Attr(AttrKind::Column), ".column")]
    #[case::attr_row(".row", Selector::Attr(AttrKind::Row), ".row")]
    // Attribute selectors - TableHeader
    #[case::attr_align(".align", Selector::Attr(AttrKind::Align), ".align")]
    // Attribute selectors - MDX
    #[case::attr_name(".name", Selector::Attr(AttrKind::Name), ".name")]
    // Property selectors: quoted form (."key") – the only way to access dict keys
    #[case::property_quoted_h1(".\"h1\"", Selector::Property("h1".into()), ".\"h1\"")]
    #[case::property_quoted_url(".\"url\"", Selector::Property("url".into()), ".\"url\"")]
    #[case::property_quoted_with_space(".\"my key\"", Selector::Property("my key".into()), ".\"my key\"")]
    #[case::property_quoted_escaped_quote(".\"my\\\"key\"", Selector::Property("my\"key".into()), ".\"my\\\"key\"")]
    #[case::property_quoted_escaped_backslash(".\"my\\\\key\"", Selector::Property("my\\key".into()), ".\"my\\\\key\"")]
    #[case::property_quoted_empty(".\"\"", Selector::Property("".into()), ".\"\"")]
    fn test_selector_try_from_and_display(
        #[case] input: &str,
        #[case] expected_selector: Selector,
        #[case] expected_display: &str,
    ) {
        // Test TryFrom
        let selector = Selector::try_from(&Token {
            kind: TokenKind::Selector(SmolStr::new(input)),
            range: Range {
                start: Position::new(0, 0),
                end: Position::new(0, 0),
            },
            module_id: ArenaId::new(0),
        })
        .expect("Should parse valid selector");
        assert_eq!(selector, expected_selector);

        // Test Display
        assert_eq!(selector.to_string(), expected_display);
    }

    #[rstest]
    #[case(".")]
    #[case(".mykey")]
    #[case(".my_key")]
    #[case(".unknown")]
    #[case(".hedaing")]
    fn test_selector_try_from_invalid(#[case] input: &str) {
        let token = Token {
            kind: TokenKind::Selector(SmolStr::new(input)),
            range: Range {
                start: Position::new(0, 0),
                end: Position::new(0, 0),
            },
            module_id: ArenaId::new(0),
        };
        let result = Selector::try_from(&token);
        assert!(result.is_err());
        if let Err(e) = result {
            assert_eq!(e, UnknownSelector(token));
        }
    }
}
