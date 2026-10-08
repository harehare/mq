//! Links, images, references and footnote references, resolved when a `]` is found.

use super::entity::{remove_line_indent, unescape};
use super::tail::{Tail, inline_tail, reference};
use super::{Context, Item, Scanner, emphasis, to_nodes};
use crate::node::{FootnoteRef, Image, ImageRef, Link, LinkRef, Node, Position, Title, Url};

/// Normalizes a reference label the way `markdown-rs` does, including its handling of whitespace:
/// the first gap between words is dropped.
pub(in crate::parser) fn normalize(label: &str) -> String {
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
            if start != 0 {
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
    let (image, active, opener_start) = (opener.image, opener.active, opener.start);
    scanner.openers.pop();
    if !active {
        return false;
    }

    let context = scanner.context;
    let src = context.src();
    let pos = scanner.pos;
    let label_start = opener_start + if image { 2 } else { 1 };
    let label = &src[label_start..pos];
    let after = pos + 1;

    let is_footnote =
        !image && footnote_label(label).is_some_and(|name| context.references.footnotes.contains(&normalize(name)));
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
    // Email addresses are not linked inside link text.
    let inner = Context {
        emails: false,
        ..*context
    };
    emphasis::process(&mut content, 0, &inner);
    let values = to_nodes(content, &inner);

    let (node, end) = build(kind, image, values, after, |end| context.position(opener_start, end));

    if !image {
        // Links cannot contain links: earlier `[` openers can no longer become links.
        for &index in &scanner.openers {
            if let Item::Open(earlier) = &mut scanner.items[index]
                && !earlier.image
            {
                earlier.active = false;
            }
        }
    }

    scanner.items.push(Item::Node(node));
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
                    values,
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

/// Builds a link, unwrapping nested links to the same destination (from autolink literals).
pub(super) fn make_link(url: String, title: Option<String>, values: Vec<Node>, position: Option<Position>) -> Node {
    let values = values
        .into_iter()
        .flat_map(|child| match child {
            Node::Link(Link {
                url: Url(ref inner),
                values,
                ..
            }) if *inner == url => values,
            other => vec![other],
        })
        .collect();

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
            _ => {}
        }
    }
    text
}
