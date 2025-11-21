use std::fmt::{self, Display, Formatter};

#[cfg(feature = "ast-json")]
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{Token, TokenKind};

#[derive(Error, Clone, Debug, PartialOrd, Eq, Ord, PartialEq)]
#[error("Unknown selector `{0}`")]
pub struct UnknownSelector(pub Token);

impl UnknownSelector {
    pub fn new(token: Token) -> Self {
        Self(token)
    }
}

#[cfg_attr(feature = "ast-json", derive(Serialize, Deserialize))]
#[derive(PartialEq, PartialOrd, Debug, Eq, Clone)]
pub enum Selector {
    Blockquote,
    Footnote,
    List(Option<usize>, Option<bool>),
    Toml,
    Yaml,
    Break,
    InlineCode,
    InlineMath,
    Delete,
    Emphasis,
    FootnoteRef,
    Html,
    Image,
    ImageRef,
    MdxJsxTextElement,
    Link,
    LinkRef,
    Strong,
    Code,
    Math,
    Heading(Option<u8>),
    Table(Option<usize>, Option<usize>),
    Text,
    HorizontalRule,
    Definition,
    MdxFlowExpression,
    MdxTextExpression,
    MdxJsEsm,
    MdxJsxFlowElement,
    Attr(AttrKind),
}

/// Represents an attribute that can be accessed from Markdown nodes
#[cfg_attr(feature = "ast-json", derive(Serialize, Deserialize))]
#[derive(PartialEq, PartialOrd, Debug, Eq, Clone)]
pub enum AttrKind {
    // Common text attributes
    Value,    // "value"
    Values,   // "values"
    Children, // "children"

    // Code attributes
    Lang,  // "lang"
    Meta,  // "meta"
    Fence, // "fence"

    // Link/Image attributes
    Url,   // "url"
    Alt,   // "alt"
    Title, // "title"

    // Reference attributes (LinkRef, ImageRef, FootnoteRef, Definition, Footnote)
    Ident, // "ident"
    Label, // "label"

    // Heading attributes
    Depth, // "depth"
    Level, // "level" (alias for depth)

    // List attributes
    Index,   // "index"
    Ordered, // "ordered"
    Checked, // "checked"

    // TableCell attributes
    Column,            // "column"
    Row,               // "row"
    LastCellInRow,     // "last_cell_in_row"
    LastCellOfInTable, // "last_cell_of_in_table"

    // TableHeader attributes
    Align, // "align"

    // MDX attributes
    Name, // "name" (for MdxJsxFlowElement, MdxJsxTextElement)
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
            AttrKind::LastCellInRow => write!(f, ".last_cell_in_row"),
            AttrKind::LastCellOfInTable => write!(f, ".last_cell_of_in_table"),
            AttrKind::Align => write!(f, ".align"),
            AttrKind::Name => write!(f, ".name"),
        }
    }
}

impl TryFrom<&Token> for Selector {
    type Error = UnknownSelector;

    fn try_from(token: &Token) -> Result<Self, Self::Error> {
        if let TokenKind::Selector(s) = &token.kind {
            match s.as_str() {
                // Heading selectors
                ".h" => Ok(Selector::Heading(None)),
                ".h1" => Ok(Selector::Heading(Some(1))),
                ".h2" => Ok(Selector::Heading(Some(2))),
                ".h3" => Ok(Selector::Heading(Some(3))),
                ".h4" => Ok(Selector::Heading(Some(4))),
                ".h5" => Ok(Selector::Heading(Some(5))),
                ".h6" => Ok(Selector::Heading(Some(6))),

                // Blockquote
                ".>" | ".blockquote" => Ok(Selector::Blockquote),

                // Footnote
                ".^" | ".footnote" => Ok(Selector::Footnote),

                // MDX JSX Flow Element
                ".<" | ".mdx_jsx_flow_element" => Ok(Selector::MdxJsxFlowElement),

                // Emphasis
                ".**" | ".emphasis" => Ok(Selector::Emphasis),

                // Math
                ".$$" | ".math" => Ok(Selector::Math),

                // Horizontal Rule
                ".horizontal_rule" | ".---" | ".***" | ".___" => Ok(Selector::HorizontalRule),

                // MDX Text Expression
                ".{}" | ".mdx_text_expression" => Ok(Selector::MdxTextExpression),

                // Footnote Reference
                ".[^]" | ".footnote_ref" => Ok(Selector::FootnoteRef),

                // Definition
                ".definition" => Ok(Selector::Definition),

                // Break
                ".break" => Ok(Selector::Break),

                // Delete
                ".delete" => Ok(Selector::Delete),

                // HTML
                ".<>" | ".html" => Ok(Selector::Html),

                // Image
                ".image" => Ok(Selector::Image),

                // Image Reference
                ".image_ref" => Ok(Selector::ImageRef),

                // Inline Code
                ".code_inline" => Ok(Selector::InlineCode),

                // Inline Math
                ".math_inline" => Ok(Selector::InlineMath),

                // Link
                ".link" => Ok(Selector::Link),

                // Link Reference
                ".link_ref" => Ok(Selector::LinkRef),

                // List
                ".list" => Ok(Selector::List(None, None)),

                // TOML
                ".toml" => Ok(Selector::Toml),

                // Strong
                ".strong" => Ok(Selector::Strong),

                // YAML
                ".yaml" => Ok(Selector::Yaml),

                // Code
                ".code" => Ok(Selector::Code),

                // MDX JS ESM
                ".mdx_js_esm" => Ok(Selector::MdxJsEsm),

                // MDX JSX Text Element
                ".mdx_jsx_text_element" => Ok(Selector::MdxJsxTextElement),

                // MDX Flow Expression
                ".mdx_flow_expression" => Ok(Selector::MdxFlowExpression),

                // Text
                ".text" => Ok(Selector::Text),

                // Table
                ".table" => Ok(Selector::Table(None, None)),

                // Attribute selectors - Common
                ".value" => Ok(Selector::Attr(AttrKind::Value)),
                ".values" => Ok(Selector::Attr(AttrKind::Values)),
                ".children" | ".cn" => Ok(Selector::Attr(AttrKind::Children)),

                // Attribute selectors - Code
                ".lang" => Ok(Selector::Attr(AttrKind::Lang)),
                ".meta" => Ok(Selector::Attr(AttrKind::Meta)),
                ".fence" => Ok(Selector::Attr(AttrKind::Fence)),

                // Attribute selectors - Link/Image
                ".url" => Ok(Selector::Attr(AttrKind::Url)),
                ".alt" => Ok(Selector::Attr(AttrKind::Alt)),
                ".title" => Ok(Selector::Attr(AttrKind::Title)),

                // Attribute selectors - Reference
                ".ident" => Ok(Selector::Attr(AttrKind::Ident)),
                ".label" => Ok(Selector::Attr(AttrKind::Label)),

                // Attribute selectors - Heading
                ".depth" => Ok(Selector::Attr(AttrKind::Depth)),
                ".level" => Ok(Selector::Attr(AttrKind::Level)),

                // Attribute selectors - List
                ".index" => Ok(Selector::Attr(AttrKind::Index)),
                ".ordered" => Ok(Selector::Attr(AttrKind::Ordered)),
                ".checked" => Ok(Selector::Attr(AttrKind::Checked)),

                // Attribute selectors - TableCell
                ".column" => Ok(Selector::Attr(AttrKind::Column)),
                ".row" => Ok(Selector::Attr(AttrKind::Row)),
                ".last_cell_in_row" => Ok(Selector::Attr(AttrKind::LastCellInRow)),
                ".last_cell_of_in_table" => Ok(Selector::Attr(AttrKind::LastCellOfInTable)),

                // Attribute selectors - TableHeader
                ".align" => Ok(Selector::Attr(AttrKind::Align)),

                // Attribute selectors - MDX
                ".name" => Ok(Selector::Attr(AttrKind::Name)),

                _ => Err(UnknownSelector(token.clone())),
            }
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
            Selector::List(Some(idx), None) => write!(f, ".list({})", idx),
            Selector::List(idx, ordered) => write!(f, ".list({:?}, {:?})", idx, ordered),
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
            Selector::Table(Some(row), None) => write!(f, ".[{}]", row),
            Selector::Table(Some(row), Some(col)) => write!(f, ".[{}][{}]", row, col),
            Selector::Table(None, Some(col)) => write!(f, ".[][{}]", col),
            Selector::Text => write!(f, ".text"),
            Selector::HorizontalRule => write!(f, ".horizontal_rule"),
            Selector::Definition => write!(f, ".definition"),
            Selector::MdxFlowExpression => write!(f, ".mdx_flow_expression"),
            Selector::MdxTextExpression => write!(f, ".mdx_text_expression"),
            Selector::MdxJsEsm => write!(f, ".mdx_js_esm"),
            Selector::MdxJsxFlowElement => write!(f, ".mdx_jsx_flow_element"),
            Selector::Attr(attr) => write!(f, "{}", attr),
        }
    }
}

impl Selector {
    pub fn is_attribute_selector(&self) -> bool {
        matches!(self, Selector::Attr(_))
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
    #[case::attr_last_cell_in_row(".last_cell_in_row", Selector::Attr(AttrKind::LastCellInRow), ".last_cell_in_row")]
    #[case::attr_last_cell_of_in_table(
        ".last_cell_of_in_table",
        Selector::Attr(AttrKind::LastCellOfInTable),
        ".last_cell_of_in_table"
    )]
    // Attribute selectors - TableHeader
    #[case::attr_align(".align", Selector::Attr(AttrKind::Align), ".align")]
    // Attribute selectors - MDX
    #[case::attr_name(".name", Selector::Attr(AttrKind::Name), ".name")]
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

    #[test]
    fn test_selector_try_from_unknown() {
        let token = Token {
            kind: TokenKind::Selector(SmolStr::new(".unknown")),
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
