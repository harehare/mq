//! Turns the block tree into nodes: parses the inline content, pairs JSX tags and flattens lists and
//! tables.

use super::inline::{self, MAX_NESTING};
use super::mdx::TagKind;
use super::tree::{Block, InlineBlock, InlineKind, JsxTag, ListBlock, QuoteBlock, TableItem};
use crate::node::{
    Blockquote, Footnote, Heading, Level, List, MdxJsxFlowElement, Node, Position, TableAlign, TableCell,
};
use rustc_hash::FxHashSet;

/// The identifiers that references can resolve to.
#[derive(Default)]
pub(super) struct References {
    /// Normalized labels of link reference definitions.
    pub(super) definitions: FxHashSet<String>,
    /// Normalized labels of footnote definitions.
    pub(super) footnotes: FxHashSet<String>,
    /// Whether the document is MDX.
    pub(super) mdx: bool,
}

pub(super) fn resolve(blocks: Vec<Block>, mdx: bool) -> Result<Vec<Node>, String> {
    let mut references = References {
        mdx,
        ..References::default()
    };
    collect(&blocks, &mut references);

    let mut nodes = Vec::new();
    flatten(blocks, &references, &mut nodes)?;
    Ok(nodes)
}

pub(super) fn collect(blocks: &[Block], references: &mut References) {
    for block in blocks {
        match block {
            Block::Node(Node::Definition(definition)) => {
                references.definitions.insert(definition.ident.clone());
            }
            Block::Quote(quote) => collect(&quote.children, references),
            Block::Footnote(footnote) => {
                references.footnotes.insert(footnote.ident.clone());
                collect(&footnote.children, references);
            }
            Block::List(list) => list.items.iter().for_each(|item| collect(&item.children, references)),
            _ => {}
        }
    }
}

/// The JSX elements that are open while blocks are turned into nodes, and the nodes inside the
/// innermost one.
struct Frames {
    open: Vec<(JsxTag, Vec<Node>)>,
    root: Vec<Node>,
}

impl Frames {
    fn push(&mut self, node: Node) {
        match self.open.last_mut() {
            Some((_, children)) => children.push(node),
            None => self.root.push(node),
        }
    }

    fn tag(&mut self, tag: JsxTag) -> Result<(), String> {
        match tag.kind {
            TagKind::Open if self.open.len() >= MAX_NESTING => {
                return Err(format!("Elements are nested deeper than {MAX_NESTING} levels"));
            }
            TagKind::Open => self.open.push((tag, Vec::new())),
            TagKind::SelfClosing => self.push(element(tag, Vec::new(), None)),
            TagKind::Close => {
                let Some((open, children)) = self.open.pop() else {
                    return Err("Unexpected closing slash `/` in tag, expected an open tag first".into());
                };
                if open.name != tag.name {
                    return Err(format!(
                        "Unexpected closing tag `</{}>`, expected corresponding closing tag for `<{}>` ({}:{})",
                        tag.name.as_deref().unwrap_or_default(),
                        open.name.as_deref().unwrap_or_default(),
                        open.position.start.line,
                        open.position.start.column
                    ));
                }
                let end = tag.position.end;
                self.push(element(open, children, Some(end)));
            }
        }
        Ok(())
    }
}

fn element(tag: JsxTag, children: Vec<Node>, end: Option<crate::node::Point>) -> Node {
    Node::MdxJsxFlowElement(MdxJsxFlowElement {
        children,
        position: Some(Position {
            start: tag.position.start,
            end: end.unwrap_or(tag.position.end),
        }),
        name: tag.name,
        attributes: tag.attributes,
    })
}

fn flatten(blocks: Vec<Block>, references: &References, nodes: &mut Vec<Node>) -> Result<(), String> {
    let mut frames = Frames {
        open: Vec::new(),
        root: Vec::new(),
    };

    for block in blocks {
        match block {
            Block::Node(node) => frames.push(node),
            Block::Fenced(fenced) => frames.push(fenced.node),
            Block::Inline(block) => {
                let mut out = Vec::new();
                inline_block(block, references, &mut out)?;
                out.into_iter().for_each(|node| frames.push(node));
            }
            Block::Quote(quote) => frames.push(quote_node(quote, references)?),
            Block::List(list) => {
                let mut out = Vec::new();
                list_nodes(list, 0, references, &mut out)?;
                out.into_iter().for_each(|node| frames.push(node));
            }
            Block::Footnote(footnote) => {
                let mut values = Vec::new();
                flatten(footnote.children, references, &mut values)?;
                frames.push(Node::Footnote(Footnote {
                    ident: footnote.ident,
                    values,
                    position: Some(footnote.position),
                }));
            }
            Block::Table(items) => {
                for item in items {
                    let node = match item {
                        TableItem::Cell {
                            row,
                            column,
                            position,
                            source,
                        } => Node::TableCell(TableCell {
                            values: match source {
                                Some(source) => inline::parse(&source, references)?,
                                None => Vec::new(),
                            },
                            column,
                            row,
                            position: Some(position),
                        }),
                        TableItem::Align { align, position } => Node::TableAlign(TableAlign {
                            align,
                            position: Some(position),
                        }),
                    };
                    frames.push(node);
                }
            }
            Block::Jsx(tag) => frames.tag(tag)?,
            Block::Error(message) => return Err(message),
        }
    }

    if let Some((open, _)) = frames.open.last() {
        return Err(format!(
            "Expected a closing tag for `<{}>` ({}:{}) before the end of its container",
            open.name.as_deref().unwrap_or_default(),
            open.position.start.line,
            open.position.start.column
        ));
    }
    nodes.append(&mut frames.root);

    Ok(())
}

/// A block quote, or a callout when the `callout` feature is enabled and its first line is a callout header.
fn quote_node(quote: QuoteBlock, references: &References) -> Result<Node, String> {
    #[cfg(feature = "callout")]
    let (header, children) = {
        let mut children = quote.children;
        (super::callout::take_header(&mut children), children)
    };
    #[cfg(not(feature = "callout"))]
    let children = quote.children;

    let mut values = Vec::new();
    flatten(children, references, &mut values)?;
    let position = Some(quote.position);

    #[cfg(feature = "callout")]
    if let Some(header) = header {
        return Ok(Node::Callout(crate::node::Callout {
            kind: header.kind,
            fold: header.fold,
            title: header.title,
            values,
            position,
        }));
    }
    Ok(Node::Blockquote(Blockquote { values, position }))
}

fn inline_block(block: InlineBlock, references: &References, nodes: &mut Vec<Node>) -> Result<(), String> {
    let values = inline::parse(&block.source, references)?;
    match block.kind {
        InlineKind::Paragraph => nodes.extend(values),
        InlineKind::Heading { depth, position } => nodes.push(Node::Heading(Heading {
            depth,
            values,
            position: Some(position),
        })),
    }
    Ok(())
}

/// Emits one flat `Node::List` per item, followed by the items of its nested lists one level deeper.
fn list_nodes(list: ListBlock, level: Level, references: &References, nodes: &mut Vec<Node>) -> Result<(), String> {
    for (index, item) in list.items.into_iter().enumerate() {
        let (nested, others): (Vec<_>, Vec<_>) = item.children.into_iter().partition(|b| matches!(b, Block::List(_)));
        let mut values = Vec::new();
        flatten(others, references, &mut values)?;
        let position = match (
            values.first().and_then(Node::position),
            values.last().and_then(Node::position),
        ) {
            (Some(first), Some(last)) => Position {
                start: first.start.clone(),
                end: last.end.clone(),
            },
            _ => item.position,
        };

        nodes.push(Node::List(List {
            values,
            index,
            level,
            ordered: list.ordered,
            checked: item.checked,
            spread: list.spread,
            start: list.start,
            marker: Some(list.marker),
            position: Some(position),
        }));

        for block in nested {
            if let Block::List(sub_list) = block {
                list_nodes(sub_list, level + 1, references, nodes)?;
            }
        }
    }
    Ok(())
}
