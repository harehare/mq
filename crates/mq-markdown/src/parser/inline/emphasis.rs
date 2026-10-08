//! Emphasis, strong emphasis and strikethrough, resolved with a stack of delimiter runs.
//!
//! This follows the reference implementation of `CommonMark`. `markdown-rs` differs in how runs next
//! to other runs open and close, and in using the remaining length of a run for the rule of three.

use super::punctuation::is_punctuation;
use super::{Context, Delim, Item, to_nodes};
use crate::node::{Delete, Emphasis, Node, Strong};

#[derive(PartialEq)]
enum Kind {
    Whitespace,
    Punctuation,
    Other,
}

fn kind(char: Option<char>) -> Kind {
    match char {
        None => Kind::Whitespace,
        Some(char) if char.is_whitespace() => Kind::Whitespace,
        Some(char) if is_punctuation(char) => Kind::Punctuation,
        Some(_) => Kind::Other,
    }
}

/// Whether a run of `ch` between `before` and `after` can open and can close, by the rules of
/// `CommonMark` for left- and right-flanking runs, which `~` follows too.
pub(super) fn flanking(ch: u8, before: Option<char>, after: Option<char>) -> (bool, bool) {
    let (before_kind, after_kind) = (kind(before), kind(after));

    let open = after_kind == Kind::Other || (after_kind == Kind::Punctuation && before_kind != Kind::Other);
    let close = before_kind == Kind::Other || (before_kind == Kind::Punctuation && after_kind != Kind::Other);

    if ch == b'_' {
        (
            open && (before_kind != Kind::Other || !close),
            close && (after_kind != Kind::Other || !open),
        )
    } else {
        (open, close)
    }
}

/// Whether `opener` and `closer` can match, by character and by the rule of three, which goes by the
/// length of the whole runs.
fn matches(opener: &Delim, closer: &Delim) -> bool {
    if !opener.can_open || opener.ch != closer.ch {
        return false;
    }
    if (opener.can_close || closer.can_open)
        && !closer.original.is_multiple_of(3)
        && (opener.original + closer.original).is_multiple_of(3)
    {
        return false;
    }
    // Strikethrough matches runs of the same length, one or two tildes.
    !(closer.ch == b'~' && (closer.count != opener.count || closer.count > 2))
}

/// Matches delimiters in `items[bottom..]`, replacing each pair and what is between them by a node.
pub(super) fn process(items: &mut Vec<Item>, bottom: usize, context: &Context<'_>) {
    let mut closer = bottom;

    while closer < items.len() {
        let Item::Delim(current) = &items[closer] else {
            closer += 1;
            continue;
        };
        let opener = if current.can_close {
            (bottom..closer).rev().find(|&index| match &items[index] {
                Item::Delim(candidate) => matches(candidate, current),
                _ => false,
            })
        } else {
            None
        };
        let Some(opener) = opener else {
            closer += 1;
            continue;
        };

        let (open, close) = match (&items[opener], &items[closer]) {
            (Item::Delim(open), Item::Delim(close)) => (open, close),
            _ => unreachable!("both indexes are delimiters"),
        };
        let used = if open.count > 1 && close.count > 1 { 2 } else { 1 };
        let ch = close.ch;
        let start = open.start + open.count - used;
        let end = close.start + used;

        let children = items.drain(opener + 1..closer).collect::<Vec<_>>();
        let values = to_nodes(children, context);
        let position = Some(context.position(start, end));
        let node = match (ch, used) {
            (b'~', _) => Node::Delete(Delete { values, position }),
            (_, 2) => Node::Strong(Strong { values, position }),
            _ => Node::Emphasis(Emphasis { values, position }),
        };

        // The drained items are gone, so the closer is now right after the new node.
        closer = opener + 1;
        items.insert(closer, Item::Node(node));
        closer += 1;

        if let Item::Delim(open) = &mut items[opener] {
            open.count -= used;
        }
        if let Item::Delim(close) = &mut items[closer] {
            close.start += used;
            close.count -= used;
        }

        // Delimiters inside the new node cannot open anything else, and those used up are removed.
        if matches!(&items[closer], Item::Delim(close) if close.count == 0) {
            items.remove(closer);
        }
        if matches!(&items[opener], Item::Delim(open) if open.count == 0) {
            items.remove(opener);
            closer -= 1;
        }
    }
}
