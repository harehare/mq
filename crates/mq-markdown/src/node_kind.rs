use crate::node::{Heading, Node};

/// The kind of a [`Node`]. Headings are split by depth, so every `Node` has exactly one kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum NodeKind {
    Blockquote,
    Break,
    Callout,
    Embed,
    Definition,
    Delete,
    H1,
    H2,
    H3,
    H4,
    H5,
    H6,
    Emphasis,
    Footnote,
    FootnoteRef,
    Html,
    Yaml,
    Toml,
    Image,
    ImageRef,
    CodeInline,
    MathInline,
    Link,
    LinkRef,
    WikiLink,
    Math,
    List,
    TableAlign,
    TableRow,
    TableCell,
    Code,
    Strong,
    HorizontalRule,
    MdxFlowExpression,
    MdxJsxFlowElement,
    MdxJsxTextElement,
    MdxTextExpression,
    MdxJsEsm,
    Text,
    Fragment,
    Empty,
}

impl NodeKind {
    /// Every kind, in declaration order.
    pub const ALL: [NodeKind; 41] = [
        Self::Blockquote,
        Self::Break,
        Self::Callout,
        Self::Embed,
        Self::Definition,
        Self::Delete,
        Self::H1,
        Self::H2,
        Self::H3,
        Self::H4,
        Self::H5,
        Self::H6,
        Self::Emphasis,
        Self::Footnote,
        Self::FootnoteRef,
        Self::Html,
        Self::Yaml,
        Self::Toml,
        Self::Image,
        Self::ImageRef,
        Self::CodeInline,
        Self::MathInline,
        Self::Link,
        Self::LinkRef,
        Self::WikiLink,
        Self::Math,
        Self::List,
        Self::TableAlign,
        Self::TableRow,
        Self::TableCell,
        Self::Code,
        Self::Strong,
        Self::HorizontalRule,
        Self::MdxFlowExpression,
        Self::MdxJsxFlowElement,
        Self::MdxJsxTextElement,
        Self::MdxTextExpression,
        Self::MdxJsEsm,
        Self::Text,
        Self::Fragment,
        Self::Empty,
    ];

    /// The name `to_md_name` returns for a node of this kind.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Blockquote => "blockquote",
            Self::Break => "break",
            Self::Callout => "callout",
            Self::Embed => "embed",
            Self::Definition => "definition",
            Self::Delete => "delete",
            Self::H1 => "h1",
            Self::H2 => "h2",
            Self::H3 => "h3",
            Self::H4 => "h4",
            Self::H5 => "h5",
            Self::H6 => "h6",
            Self::Emphasis => "emphasis",
            Self::Footnote => "footnote",
            Self::FootnoteRef => "footnoteref",
            Self::Html => "html",
            Self::Yaml => "yaml",
            Self::Toml => "toml",
            Self::Image => "image",
            Self::ImageRef => "image_ref",
            Self::CodeInline => "code_inline",
            Self::MathInline => "math_inline",
            Self::Link => "link",
            Self::LinkRef => "link_ref",
            Self::WikiLink => "wikilink",
            Self::Math => "math",
            Self::List => "list",
            Self::TableAlign => "table_align",
            Self::TableRow => "table_row",
            Self::TableCell => "table_cell",
            Self::Code => "code",
            Self::Strong => "strong",
            Self::HorizontalRule => "Horizontal_rule",
            Self::MdxFlowExpression => "mdx_flow_expression",
            Self::MdxJsxFlowElement => "mdx_jsx_flow_element",
            Self::MdxJsxTextElement => "mdx_jsx_text_element",
            Self::MdxTextExpression => "mdx_text_expression",
            Self::MdxJsEsm => "mdx_js_esm",
            Self::Text => "text",
            Self::Fragment | Self::Empty => "",
        }
    }
}

impl Node {
    /// The kind of this node. A heading depth outside `1..=6`, which parsing never produces,
    /// is clamped.
    pub fn kind(&self) -> NodeKind {
        match self {
            Node::Blockquote(_) => NodeKind::Blockquote,
            Node::Break(_) => NodeKind::Break,
            #[cfg(feature = "callout")]
            Node::Callout(_) => NodeKind::Callout,
            #[cfg(feature = "embed")]
            Node::Embed(_) => NodeKind::Embed,
            Node::Definition(_) => NodeKind::Definition,
            Node::Delete(_) => NodeKind::Delete,
            Node::Heading(Heading { depth, .. }) => match depth {
                0 | 1 => NodeKind::H1,
                2 => NodeKind::H2,
                3 => NodeKind::H3,
                4 => NodeKind::H4,
                5 => NodeKind::H5,
                _ => NodeKind::H6,
            },
            Node::Emphasis(_) => NodeKind::Emphasis,
            Node::Footnote(_) => NodeKind::Footnote,
            Node::FootnoteRef(_) => NodeKind::FootnoteRef,
            Node::Html(_) => NodeKind::Html,
            Node::Yaml(_) => NodeKind::Yaml,
            Node::Toml(_) => NodeKind::Toml,
            Node::Image(_) => NodeKind::Image,
            Node::ImageRef(_) => NodeKind::ImageRef,
            Node::CodeInline(_) => NodeKind::CodeInline,
            Node::MathInline(_) => NodeKind::MathInline,
            Node::Link(_) => NodeKind::Link,
            Node::LinkRef(_) => NodeKind::LinkRef,
            #[cfg(feature = "wikilink")]
            Node::WikiLink(_) => NodeKind::WikiLink,
            Node::Math(_) => NodeKind::Math,
            Node::List(_) => NodeKind::List,
            Node::TableAlign(_) => NodeKind::TableAlign,
            Node::TableRow(_) => NodeKind::TableRow,
            Node::TableCell(_) => NodeKind::TableCell,
            Node::Code(_) => NodeKind::Code,
            Node::Strong(_) => NodeKind::Strong,
            Node::HorizontalRule(_) => NodeKind::HorizontalRule,
            Node::MdxFlowExpression(_) => NodeKind::MdxFlowExpression,
            Node::MdxJsxFlowElement(_) => NodeKind::MdxJsxFlowElement,
            Node::MdxJsxTextElement(_) => NodeKind::MdxJsxTextElement,
            Node::MdxTextExpression(_) => NodeKind::MdxTextExpression,
            Node::MdxJsEsm(_) => NodeKind::MdxJsEsm,
            Node::Text(_) => NodeKind::Text,
            Node::Fragment(_) => NodeKind::Fragment,
            Node::Empty => NodeKind::Empty,
        }
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[test]
    fn test_all_lists_every_kind_once() {
        let mut seen = std::collections::HashSet::new();
        assert!(NodeKind::ALL.iter().all(|kind| seen.insert(*kind)));
        assert_eq!(
            NodeKind::ALL.iter().map(|k| *k as usize).max(),
            Some(NodeKind::ALL.len() - 1)
        );
    }

    #[rstest]
    #[case("# a", NodeKind::H1)]
    #[case("### a", NodeKind::H3)]
    #[case("###### a", NodeKind::H6)]
    #[case("```rust\nfn f() {}\n```", NodeKind::Code)]
    #[case("- a", NodeKind::List)]
    #[case("> a", NodeKind::Blockquote)]
    #[case("---", NodeKind::HorizontalRule)]
    fn test_kind_agrees_with_name(#[case] markdown: &str, #[case] kind: NodeKind) {
        let node = markdown
            .parse::<crate::Markdown>()
            .unwrap()
            .nodes
            .into_iter()
            .next()
            .unwrap();
        assert_eq!(node.kind(), kind);
        assert_eq!(node.name().as_str(), kind.name());
    }
}
