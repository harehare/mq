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

/// The type of a node attribute as returned by [`Node::attr`](crate::Node::attr).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttrType {
    String,
    Integer,
    Boolean,
    /// An array of nodes.
    Nodes,
}

/// An attribute a node kind has, with its type. `optional` attributes are `none` when absent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttrSpec {
    pub name: &'static str,
    pub ty: AttrType,
    pub optional: bool,
}

const fn attr(name: &'static str, ty: AttrType) -> AttrSpec {
    AttrSpec {
        name,
        ty,
        optional: false,
    }
}

const fn optional(name: &'static str, ty: AttrType) -> AttrSpec {
    AttrSpec {
        name,
        ty,
        optional: true,
    }
}

const VALUE: AttrSpec = attr("value", AttrType::String);
const VALUES: AttrSpec = attr("values", AttrType::Nodes);
const CHILDREN: AttrSpec = attr("children", AttrType::Nodes);
const IDENT: AttrSpec = attr("ident", AttrType::String);
const URL: AttrSpec = attr("url", AttrType::String);
const LABEL: AttrSpec = optional("label", AttrType::String);
const TITLE: AttrSpec = optional("title", AttrType::String);
const ALT: AttrSpec = attr("alt", AttrType::String);
const NAME: AttrSpec = optional("name", AttrType::String);
const ALIGN: AttrSpec = attr("align", AttrType::String);
const CHECKED: AttrSpec = optional("checked", AttrType::Boolean);
const COLUMN: AttrSpec = attr("column", AttrType::Integer);
const DEPTH: AttrSpec = attr("depth", AttrType::Integer);
const FENCE: AttrSpec = attr("fence", AttrType::Boolean);
const INDEX: AttrSpec = attr("index", AttrType::Integer);
const KIND: AttrSpec = attr("kind", AttrType::String);
const LANG: AttrSpec = optional("lang", AttrType::String);
const LEVEL: AttrSpec = attr("level", AttrType::Integer);
const META: AttrSpec = optional("meta", AttrType::String);
const ORDERED: AttrSpec = attr("ordered", AttrType::Boolean);
const ROW: AttrSpec = attr("row", AttrType::Integer);
const LINE: AttrSpec = optional("line", AttrType::Integer);
const END_LINE: AttrSpec = optional("end_line", AttrType::Integer);

impl NodeKind {
    /// The attributes of this kind, including `line` and `end_line` (absent for nodes
    /// without a position). Mirrors [`Node::attr`](crate::Node::attr).
    pub const fn attrs(self) -> &'static [AttrSpec] {
        match self {
            Self::Footnote => &[LINE, END_LINE, IDENT, VALUE, VALUES, CHILDREN],
            Self::Html
            | Self::Text
            | Self::CodeInline
            | Self::MathInline
            | Self::Math
            | Self::Yaml
            | Self::Toml
            | Self::MdxFlowExpression
            | Self::MdxTextExpression
            | Self::MdxJsEsm => &[LINE, END_LINE, VALUE],
            Self::Code => &[LINE, END_LINE, VALUE, LANG, META, FENCE],
            Self::Image => &[LINE, END_LINE, ALT, URL, TITLE],
            Self::ImageRef => &[LINE, END_LINE, ALT, IDENT, LABEL],
            Self::Link => &[LINE, END_LINE, URL, TITLE, VALUE, VALUES, CHILDREN],
            Self::WikiLink | Self::Embed => &[LINE, END_LINE, URL, VALUE],
            Self::Callout => &[LINE, END_LINE, KIND, TITLE, VALUE, VALUES, CHILDREN],
            Self::LinkRef | Self::FootnoteRef => &[LINE, END_LINE, IDENT, LABEL],
            Self::Definition => &[LINE, END_LINE, IDENT, URL, TITLE, LABEL],
            Self::H1 | Self::H2 | Self::H3 | Self::H4 | Self::H5 | Self::H6 => {
                &[LINE, END_LINE, DEPTH, LEVEL, VALUE, VALUES, CHILDREN]
            }
            Self::List => &[LINE, END_LINE, INDEX, LEVEL, ORDERED, CHECKED, VALUE, VALUES, CHILDREN],
            Self::TableCell => &[LINE, END_LINE, COLUMN, ROW, VALUE, VALUES, CHILDREN],
            Self::TableAlign => &[LINE, END_LINE, ALIGN],
            Self::MdxJsxFlowElement | Self::MdxJsxTextElement => &[LINE, END_LINE, NAME, VALUES, CHILDREN],
            Self::Strong | Self::Blockquote | Self::Delete | Self::Emphasis | Self::TableRow | Self::Fragment => {
                &[LINE, END_LINE, VALUE, VALUES, CHILDREN]
            }
            Self::Break | Self::HorizontalRule | Self::Empty => &[LINE, END_LINE],
        }
    }

    /// The attribute `name` of this kind, if it has one.
    pub fn attr_spec(self, name: &str) -> Option<AttrSpec> {
        self.attrs().iter().copied().find(|spec| spec.name == name)
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

    /// A document with a node of most kinds, and every node reachable through `children`.
    fn sample_nodes() -> Vec<crate::Node> {
        let markdown = "---\ntitle: a\n---\n\n# Heading *em* **strong** ~~del~~ `code` $x$ [link](http://a \"t\") ![img](http://b) [ref][r] ![iref][r] [^1]\n\n[^1]: note\n\n[r]: http://c\n\n> quote\n\n- a\n- [x] done\n- [ ] todo\n\n1. one\n\n```rust\nfn f() {}\n```\n\n```\nplain\n```\n\n| a | b |\n|:--|--:|\n| 1 | 2 |\n\n---\n\n<div>x</div>\n\nline  \nbreak\n\n$$\nm\n$$\n";
        let mut nodes: Vec<crate::Node> = markdown.parse::<crate::Markdown>().unwrap().nodes;
        let mut all = Vec::new();
        while let Some(node) = nodes.pop() {
            if let Some(crate::node::attr_value::AttrValue::Array(children)) = node.attr("children") {
                nodes.extend(children);
            }
            all.push(node);
        }
        all
    }

    /// Every attribute name any kind has.
    fn attr_names() -> std::collections::BTreeSet<&'static str> {
        NodeKind::ALL
            .iter()
            .flat_map(|kind| kind.attrs())
            .map(|spec| spec.name)
            .collect()
    }

    #[test]
    fn test_attr_table_agrees_with_node_attr() {
        use crate::node::attr_value::AttrValue;

        let nodes = sample_nodes();
        let kinds: std::collections::BTreeSet<_> = nodes.iter().map(|node| node.kind()).collect();
        assert!(kinds.len() > 20, "sample covers too few kinds: {kinds:?}");

        for node in &nodes {
            let kind = node.kind();
            for name in attr_names() {
                let value = node.attr(name);
                match kind.attr_spec(name) {
                    None => assert!(
                        value.is_none(),
                        "{kind:?} returns `{name}` but the table does not list it"
                    ),
                    Some(spec) => {
                        let Some(value) = value else {
                            assert!(spec.optional, "{kind:?} lacks `{name}`, which is not optional");
                            continue;
                        };
                        let ty = match value {
                            AttrValue::String(_) => AttrType::String,
                            AttrValue::Integer(_) => AttrType::Integer,
                            AttrValue::Boolean(_) => AttrType::Boolean,
                            AttrValue::Array(_) => AttrType::Nodes,
                            other => panic!("unexpected {other:?} for `{name}`"),
                        };
                        assert_eq!(ty, spec.ty, "{kind:?}.{name}");
                    }
                }
            }
        }
    }

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
