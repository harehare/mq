//! Links, images, references and footnote references, resolved when a `]` is found.

use super::entity::{remove_line_indent, unescape};
use super::tail::{Tail, inline_tail, reference};
use super::{Context, Item, MAX_NESTING, Scanner, Value, emphasis, item_depth, to_nodes};
#[cfg(feature = "wikilink")]
use crate::node::RenderOptions;
use crate::node::{FootnoteRef, Image, ImageRef, Link, LinkRef, Node, Position, Text, Title, Url};

/// Normalizes a reference label: whitespace runs become one space, the ends are trimmed, and the case
/// is folded.
pub(crate) fn normalize(label: &str) -> String {
    let bytes = label.as_bytes();
    let mut result = String::with_capacity(label.len());
    let mut in_whitespace = true;
    let mut start = 0;

    for (index, byte) in bytes.iter().enumerate() {
        if matches!(byte, b'\t' | b'\n' | b'\r' | b' ') {
            if !in_whitespace {
                result.push_str(&label[start..index]);
                in_whitespace = true;
            }
        } else if in_whitespace {
            if !result.is_empty() {
                result.push(' ');
            }
            start = index;
            in_whitespace = false;
        }
    }
    if !in_whitespace {
        result.push_str(&label[start..]);
    }

    result.to_lowercase().to_uppercase().to_lowercase()
}

/// What a closing bracket resolves to.
enum Kind {
    Inline(Tail),
    /// `derived` tells that the label is the text of the reference itself.
    Reference {
        label: String,
        end: usize,
        derived: bool,
    },
    Footnote {
        label: String,
    },
}

/// Handles a `]` at the scanner position. Returns whether it closed a link or an image.
pub(super) fn close(scanner: &mut Scanner<'_>) -> bool {
    let Some(&opener_index) = scanner.openers.last() else {
        return false;
    };
    let Item::Open(opener) = &scanner.items[opener_index] else {
        return false;
    };
    let (image, opener_start) = (opener.image, opener.start);
    let active = image || scanner.openers.len() > scanner.inactive_below;
    scanner.openers.pop();
    scanner.inactive_below = scanner.inactive_below.min(scanner.openers.len());
    let inner_depth = scanner.depths.pop().unwrap_or(0);
    // The brackets stay text when they do not close, and what is inside them is still inside the outer ones.
    scanner.note_depth(inner_depth);
    if !active || inner_depth >= MAX_NESTING {
        return false;
    }

    let context = scanner.context;
    let src = context.src();
    let pos = scanner.pos;
    let label_start = opener_start + if image { 2 } else { 1 };
    let label = &src[label_start..pos];
    let after = pos + 1;

    // After a `!`, a footnote reference is the reference and the `!` is text, unless a destination follows.
    let is_footnote = footnote_label(label).is_some_and(|name| context.references.footnotes.contains(&normalize(name)))
        && (!image || inline_tail(src, after).is_none());
    let bang = image && is_footnote;
    let (image, opener_start) = if bang {
        (false, opener_start + 1)
    } else {
        (image, opener_start)
    };
    let kind = if is_footnote {
        Some(Kind::Footnote {
            label: label[1..].to_string(),
        })
    } else if let Some(tail) = inline_tail(src, after) {
        Some(Kind::Inline(tail))
    } else {
        reference(context, label, after).map(|(label, end, derived)| Kind::Reference { label, end, derived })
    };
    let Some(kind) = kind else {
        return false;
    };

    // Everything after the opener becomes the content.
    let mut content = scanner.items.split_off(opener_index + 1);
    scanner.items.pop();
    if bang {
        scanner.items.push(Item::Text {
            start: opener_start - 1,
            end: opener_start,
            value: Value::Slice(opener_start - 1, opener_start),
        });
    }
    // Email addresses are not linked inside link text.
    let inner = Context {
        emails: false,
        ..*context
    };
    emphasis::process(&mut content, &inner);
    let depth = content.iter().map(item_depth).max().unwrap_or(0) + 1;
    let values = to_nodes(content, &inner);

    let (node, end) = build(kind, image, values, after, |end| context.position(opener_start, end));

    if !image {
        // Links cannot contain links: earlier `[` openers can no longer become links.
        scanner.inactive_below = scanner.openers.len();
    }

    scanner.note_depth(depth);
    scanner.items.push(Item::Node(node, depth));
    scanner.pos = end;
    scanner.run = end;
    true
}

/// Builds the node for a resolved bracket pair, returning it and the offset where scanning goes on.
/// `position` gives the position from the opener to the given end offset.
fn build(
    kind: Kind,
    image: bool,
    values: Vec<Node>,
    after: usize,
    position: impl Fn(usize) -> Position,
) -> (Node, usize) {
    match kind {
        Kind::Footnote { label } => {
            let node = Node::FootnoteRef(FootnoteRef {
                ident: normalize(&label),
                label: Some(unescape(&remove_line_indent(&label))),
                position: Some(position(after)),
            });
            (node, after)
        }
        Kind::Inline(tail) => {
            let position = Some(position(tail.end));
            let node = if image {
                Node::Image(Image {
                    alt: plain_text(&values),
                    url: tail.url,
                    title: tail.title,
                    position,
                })
            } else {
                make_link(tail.url, tail.title, values, position)
            };
            (node, tail.end)
        }
        Kind::Reference { label, end, derived } => {
            let position = Some(position(end));
            // A label taken from the text has its markup removed.
            let shown = if derived {
                plain_text(&values)
            } else {
                unescape(&remove_line_indent(&label))
            };
            let node = if image {
                Node::ImageRef(ImageRef {
                    alt: plain_text(&values),
                    ident: normalize(&label),
                    label: Some(shown),
                    position,
                })
            } else {
                Node::LinkRef(LinkRef {
                    ident: normalize(&label),
                    label: Some(shown),
                    values: unwrap_links(values),
                    position,
                })
            };
            (node, end)
        }
    }
}

/// The label of a footnote reference: `^` followed by text without whitespace.
fn footnote_label(label: &str) -> Option<&str> {
    let name = label.strip_prefix('^')?;
    (!name.is_empty() && !name.bytes().any(|b| matches!(b, b' ' | b'\t' | b'\n' | b'\r'))).then_some(name)
}

/// Replaces links in link text by their content: a link cannot contain a link, so these come from
/// autolink literals. A wikilink there is text.
fn unwrap_links(values: Vec<Node>) -> Vec<Node> {
    values
        .into_iter()
        .flat_map(|child| match child {
            Node::Link(Link { values, position, .. }) => values
                .into_iter()
                .map(|value| match value {
                    // An autolink literal spans exactly its text, which is not decoded as it was linked. In
                    // link text it is ordinary text, so escapes and references in it apply.
                    Node::Text(Text {
                        value,
                        position: text_position,
                    }) if text_position == position => Node::Text(Text {
                        value: unescape(&value),
                        position: text_position,
                    }),
                    other => other,
                })
                .collect(),
            #[cfg(feature = "wikilink")]
            Node::WikiLink(link) => vec![Node::Text(Text {
                value: Node::WikiLink(link).to_string_with(&RenderOptions::default()),
                position: None,
            })],
            other => vec![other],
        })
        .collect()
}

/// Builds a link, unwrapping the links in its text.
pub(super) fn make_link(url: String, title: Option<String>, values: Vec<Node>, position: Option<Position>) -> Node {
    let values = unwrap_links(values);

    Node::Link(Link {
        url: Url(url),
        title: title.map(Title),
        values,
        position,
    })
}

/// The plain text of inline nodes, as used for the alternative text of images.
fn plain_text(nodes: &[Node]) -> String {
    let mut text = String::new();
    for node in nodes {
        match node {
            Node::Text(node) => text.push_str(&node.value),
            Node::CodeInline(node) => text.push_str(&node.value),
            Node::MathInline(node) => text.push_str(&node.value),
            Node::Html(node) => text.push_str(&node.value),
            Node::Emphasis(node) => text.push_str(&plain_text(&node.values)),
            Node::Strong(node) => text.push_str(&plain_text(&node.values)),
            Node::Delete(node) => text.push_str(&plain_text(&node.values)),
            Node::Link(node) => text.push_str(&plain_text(&node.values)),
            Node::LinkRef(node) => text.push_str(&plain_text(&node.values)),
            Node::Break(_) => text.push('\n'),
            Node::Image(node) => text.push_str(&node.alt),
            Node::ImageRef(node) => text.push_str(&node.alt),
            _ => {}
        }
    }
    text
}
