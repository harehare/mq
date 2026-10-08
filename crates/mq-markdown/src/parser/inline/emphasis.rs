//! Emphasis, strong emphasis and strikethrough, resolved with a stack of delimiter runs.
//!
//! This follows the reference implementation of `CommonMark`. `markdown-rs` differs in how runs next
//! to other runs open and close, and in using the remaining length of a run for the rule of three.

use super::punctuation::is_punctuation;
use super::{Context, Delim, Item, MAX_NESTING, item_depth, to_nodes};
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

/// A node of the list the delimiters are matched in, which lets matched items be taken out between
/// two delimiters without moving the items after them.
struct Slot {
    item: Option<Item>,
    prev: usize,
    next: usize,
}

const NIL: usize = usize::MAX;

/// What decides whether a closer can match an opener: the openers a closer with the same key
/// rejected once are rejected for every later closer with it.
fn key(closer: &Delim) -> usize {
    let ch = match closer.ch {
        b'*' => 0,
        b'_' => 1,
        _ => 2,
    };
    // Strikethrough also goes by the length of the run.
    let count = if ch == 2 { closer.count.min(2) } else { 0 };
    ((ch * 2 + usize::from(closer.can_open)) * 3 + closer.original % 3) * 3 + count
}

fn delim(slots: &[Slot], index: usize) -> &Delim {
    match &slots[index].item {
        Some(Item::Delim(delim)) => delim,
        _ => unreachable!("the index is a delimiter"),
    }
}

fn delim_mut(slots: &mut [Slot], index: usize) -> &mut Delim {
    match &mut slots[index].item {
        Some(Item::Delim(delim)) => delim,
        _ => unreachable!("the index is a delimiter"),
    }
}

fn unlink(slots: &mut [Slot], index: usize) {
    let (prev, next) = (slots[index].prev, slots[index].next);
    slots[prev].next = next;
    if next != NIL {
        slots[next].prev = prev;
    }
    slots[index].item = None;
}

/// Matches the delimiters in `items`, replacing each pair and what is between them by a node.
///
/// The openers that can still match are kept on a stack, and the openers a closer cannot match are
/// remembered by the offset they start at, so that the time is linear in the number of items.
pub(super) fn process(items: &mut Vec<Item>, context: &Context<'_>) {
    if !items.iter().any(|item| matches!(item, Item::Delim(_))) {
        return;
    }

    let mut slots = Vec::with_capacity(items.len() + 1);
    let total = items.len();
    slots.push(Slot {
        item: None,
        prev: NIL,
        next: 1,
    });
    for (index, item) in items.drain(..).enumerate() {
        slots.push(Slot {
            item: Some(item),
            prev: index,
            next: if index + 1 == total { NIL } else { index + 2 },
        });
    }

    let mut stack: Vec<usize> = Vec::new();
    let mut bottoms = [0usize; 54];
    let mut closer = slots[0].next;

    while closer != NIL {
        if !matches!(slots[closer].item, Some(Item::Delim(_))) {
            closer = slots[closer].next;
            continue;
        }
        let current = delim(&slots, closer);
        let (can_open, can_close, key) = (current.can_open, current.can_close, key(current));

        let mut found = None;
        if can_close {
            for at in (0..stack.len()).rev() {
                let candidate = delim(&slots, stack[at]);
                if candidate.start < bottoms[key] {
                    break;
                }
                if matches(candidate, current) {
                    found = Some(at);
                    break;
                }
            }
        }
        let found = found.filter(|&at| fits(&slots, stack[at], closer));

        let Some(at) = found else {
            if can_close {
                bottoms[key] = delim(&slots, closer).start;
            }
            if can_open {
                stack.push(closer);
            }
            closer = slots[closer].next;
            continue;
        };

        let opener = stack[at];
        let (open, close) = (delim(&slots, opener), delim(&slots, closer));
        let used = if open.count > 1 && close.count > 1 { 2 } else { 1 };
        let ch = close.ch;
        let start = open.start + open.count - used;
        let end = close.start + used;

        let mut children = Vec::new();
        let mut depth = 0;
        let mut index = slots[opener].next;
        while index != closer {
            if let Some(item) = slots[index].item.take() {
                depth = depth.max(item_depth(&item));
                children.push(item);
            }
            index = slots[index].next;
        }
        let values = to_nodes(children, context);
        let position = Some(context.position(start, end));
        let node = match (ch, used) {
            (b'~', _) => Node::Delete(Delete { values, position }),
            (_, 2) => Node::Strong(Strong { values, position }),
            _ => Node::Emphasis(Emphasis { values, position }),
        };

        let node_index = slots.len();
        slots.push(Slot {
            item: Some(Item::Node(node, depth + 1)),
            prev: opener,
            next: closer,
        });
        slots[opener].next = node_index;
        slots[closer].prev = node_index;

        delim_mut(&mut slots, opener).count -= used;
        let close = delim_mut(&mut slots, closer);
        close.start += used;
        close.count -= used;

        // Delimiters inside the new node cannot open anything else.
        stack.truncate(at + 1);
        if delim(&slots, opener).count == 0 {
            unlink(&mut slots, opener);
            stack.pop();
        }
        if delim(&slots, closer).count == 0 {
            let next = slots[closer].next;
            unlink(&mut slots, closer);
            closer = next;
        }
    }

    let mut index = slots[0].next;
    while index != NIL {
        items.extend(slots[index].item.take());
        index = slots[index].next;
    }
}

/// Whether the node for the items between `opener` and `closer` stays within the nesting limit.
fn fits(slots: &[Slot], opener: usize, closer: usize) -> bool {
    let mut depth = 0;
    let mut index = slots[opener].next;
    while index != closer {
        if let Some(item) = &slots[index].item {
            depth = depth.max(item_depth(item));
        }
        index = slots[index].next;
    }
    depth < MAX_NESTING
}
