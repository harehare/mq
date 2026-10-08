//! Emphasis, strong emphasis and strikethrough, resolved with a stack of delimiter runs.
//!
//! This follows the reference implementation of `CommonMark`: the rule of three goes by the length of
//! the whole runs.

use super::punctuation::is_punctuation;
use super::{Context, Delim, Item, MAX_NESTING, item_depth, to_nodes};
use crate::node::{Delete, Emphasis, Node, Strong};
use std::num::NonZeroUsize;

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
///
/// With `strikethrough`, a `~` next to a run of `*` or `_` counts as what lets the run open or close, so
/// that `a*~b~*c` is emphasis around strikethrough, and `$**~~b~~` still closes.
pub(super) fn flanking(ch: u8, before: Option<char>, after: Option<char>, strikethrough: bool) -> (bool, bool) {
    let tilde = |char: Option<char>| strikethrough && ch != b'~' && char == Some('~');
    // For opening a `~` after the run is a letter and a `~` before it is punctuation, and for closing the
    // other way round.
    let (before_for_open, after_for_open) = (
        if tilde(before) { Kind::Punctuation } else { kind(before) },
        if tilde(after) { Kind::Other } else { kind(after) },
    );
    let (before_for_close, after_for_close) = (
        if tilde(before) { Kind::Other } else { kind(before) },
        if tilde(after) { Kind::Punctuation } else { kind(after) },
    );

    let open = after_for_open == Kind::Other || (after_for_open == Kind::Punctuation && before_for_open != Kind::Other);
    let close =
        before_for_close == Kind::Other || (before_for_close == Kind::Punctuation && after_for_close != Kind::Other);

    if ch == b'_' {
        (
            open && (before_for_open != Kind::Other || !close),
            close && (after_for_close != Kind::Other || !open),
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

/// The place of an item in `Slots`.
///
/// It is not zero, so that an optional one takes no more room than a number.
#[derive(Clone, Copy, PartialEq, Eq)]
struct SlotId(NonZeroUsize);

impl SlotId {
    /// The slot at `index` of the vector of the slots.
    fn at(index: usize) -> Self {
        Self(NonZeroUsize::MIN.saturating_add(index))
    }

    fn index(self) -> usize {
        self.0.get() - 1
    }
}

/// A node of the list the delimiters are matched in.
struct Slot {
    item: Option<Item>,
    prev: SlotId,
    next: Option<SlotId>,
}

/// The list the delimiters are matched in, which lets matched items be taken out between two delimiters
/// without moving the items after them.
///
/// It starts with a slot that holds no item, so that every other slot has one before it.
struct Slots(Vec<Slot>);

impl Slots {
    const HEAD: SlotId = SlotId(NonZeroUsize::MIN);

    /// Moves `items` into a list.
    fn new(items: &mut Vec<Item>) -> Self {
        let total = items.len();
        let mut slots = Vec::with_capacity(total + 1);
        slots.push(Slot {
            item: None,
            prev: Self::HEAD,
            next: (total > 0).then(|| SlotId::at(1)),
        });
        for (index, item) in items.drain(..).enumerate() {
            slots.push(Slot {
                item: Some(item),
                prev: SlotId::at(index),
                next: (index + 1 < total).then(|| SlotId::at(index + 2)),
            });
        }
        Self(slots)
    }

    fn slot(&self, id: SlotId) -> &Slot {
        &self.0[id.index()]
    }

    fn slot_mut(&mut self, id: SlotId) -> &mut Slot {
        &mut self.0[id.index()]
    }

    fn first(&self) -> Option<SlotId> {
        self.next(Self::HEAD)
    }

    fn next(&self, id: SlotId) -> Option<SlotId> {
        self.slot(id).next
    }

    fn is_delim(&self, id: SlotId) -> bool {
        matches!(self.slot(id).item, Some(Item::Delim(_)))
    }

    fn delim(&self, id: SlotId) -> &Delim {
        match &self.slot(id).item {
            Some(Item::Delim(delim)) => delim,
            _ => unreachable!("the slot holds a delimiter"),
        }
    }

    fn delim_mut(&mut self, id: SlotId) -> &mut Delim {
        match &mut self.slot_mut(id).item {
            Some(Item::Delim(delim)) => delim,
            _ => unreachable!("the slot holds a delimiter"),
        }
    }

    /// The slots after `from` and before `to`, which comes later in the list.
    fn between(&self, from: SlotId, to: SlotId) -> impl Iterator<Item = SlotId> + '_ {
        std::iter::successors(self.next(from), |&id| self.next(id)).take_while(move |&id| id != to)
    }

    /// Whether the node for the items between `opener` and `closer` stays within the nesting limit.
    fn fits(&self, opener: SlotId, closer: SlotId) -> bool {
        let depth = self
            .between(opener, closer)
            .filter_map(|id| self.slot(id).item.as_ref())
            .map(item_depth)
            .max()
            .unwrap_or(0);
        depth < MAX_NESTING
    }

    /// Takes the items between `opener` and `closer` out, and returns them with the depth of the deepest.
    fn take_between(&mut self, opener: SlotId, closer: SlotId) -> (Vec<Item>, usize) {
        let mut children = Vec::new();
        let mut depth = 0;
        let mut cursor = self.next(opener);
        while let Some(id) = cursor.filter(|&id| id != closer) {
            if let Some(item) = self.slot_mut(id).item.take() {
                depth = depth.max(item_depth(&item));
                children.push(item);
            }
            cursor = self.next(id);
        }
        (children, depth)
    }

    /// Puts `item` between `opener` and `closer`, which are next to each other once the items between
    /// them are taken out.
    fn insert_between(&mut self, opener: SlotId, closer: SlotId, item: Item) {
        let id = SlotId::at(self.0.len());
        self.0.push(Slot {
            item: Some(item),
            prev: opener,
            next: Some(closer),
        });
        self.slot_mut(opener).next = Some(id);
        self.slot_mut(closer).prev = id;
    }

    /// Takes the slot out of the list.
    fn unlink(&mut self, id: SlotId) {
        let (prev, next) = (self.slot(id).prev, self.slot(id).next);
        self.slot_mut(prev).next = next;
        if let Some(next) = next {
            self.slot_mut(next).prev = prev;
        }
        self.slot_mut(id).item = None;
    }

    /// Moves the items that are left, in order, to the end of `items`.
    fn move_into(mut self, items: &mut Vec<Item>) {
        let mut cursor = self.first();
        while let Some(id) = cursor {
            items.extend(self.slot_mut(id).item.take());
            cursor = self.next(id);
        }
    }
}

/// What decides whether a closer can match an opener: the openers a closer with the same key
/// rejected once are rejected for every later closer with it.
#[derive(Clone, Copy)]
struct Key(usize);

impl Key {
    /// The number of different keys: the character, whether it can open, the rest of the length of the
    /// run by three, and for strikethrough the length of the run.
    const COUNT: usize = 3 * 2 * 3 * 3;

    fn of(closer: &Delim) -> Self {
        let ch = match closer.ch {
            b'*' => 0,
            b'_' => 1,
            _ => 2,
        };
        // Strikethrough also goes by the length of the run.
        let count = if ch == 2 { closer.count.min(2) } else { 0 };
        Self(((ch * 2 + usize::from(closer.can_open)) * 3 + closer.original % 3) * 3 + count)
    }
}

/// For each key, the offset that the openers a closer with it rejected start at or after.
struct Bottoms([usize; Key::COUNT]);

impl Bottoms {
    fn get(&self, key: Key) -> usize {
        self.0[key.0]
    }

    fn set(&mut self, key: Key, start: usize) {
        self.0[key.0] = start;
    }
}

/// The place in `stack` of the nearest opener that `closer` can match, not looking at openers that start
/// before `bottom`.
fn find_opener(slots: &Slots, stack: &[SlotId], closer: &Delim, bottom: usize) -> Option<usize> {
    for (at, &id) in stack.iter().enumerate().rev() {
        let candidate = slots.delim(id);
        if candidate.start < bottom {
            break;
        }
        if matches(candidate, closer) {
            return Some(at);
        }
    }
    None
}

/// Matches the delimiters in `items`, replacing each pair and what is between them by a node.
///
/// The openers that can still match are kept on a stack, and the openers a closer cannot match are
/// remembered by the offset they start at, so that the time is linear in the number of items.
pub(super) fn process(items: &mut Vec<Item>, context: &Context<'_>) {
    if !items.iter().any(|item| matches!(item, Item::Delim(_))) {
        return;
    }

    let mut slots = Slots::new(items);
    let mut stack: Vec<SlotId> = Vec::new();
    let mut bottoms = Bottoms([0; Key::COUNT]);
    let mut cursor = slots.first();

    while let Some(closer) = cursor {
        if !slots.is_delim(closer) {
            cursor = slots.next(closer);
            continue;
        }
        let current = slots.delim(closer);
        let (can_open, can_close, key) = (current.can_open, current.can_close, Key::of(current));

        let found = can_close
            .then(|| find_opener(&slots, &stack, current, bottoms.get(key)))
            .flatten()
            .filter(|&at| slots.fits(stack[at], closer));

        let Some(at) = found else {
            if can_close {
                bottoms.set(key, slots.delim(closer).start);
            }
            if can_open {
                stack.push(closer);
            }
            cursor = slots.next(closer);
            continue;
        };

        let opener = stack[at];
        let (open, close) = (slots.delim(opener), slots.delim(closer));
        let used = if open.count > 1 && close.count > 1 { 2 } else { 1 };
        let ch = close.ch;
        let start = open.start + open.count - used;
        let end = close.start + used;

        let (children, depth) = slots.take_between(opener, closer);
        let values = to_nodes(children, context);
        let position = Some(context.position(start, end));
        let node = match (ch, used) {
            (b'~', _) => Node::Delete(Delete { values, position }),
            (_, 2) => Node::Strong(Strong { values, position }),
            _ => Node::Emphasis(Emphasis { values, position }),
        };
        slots.insert_between(opener, closer, Item::Node(node, depth + 1));

        slots.delim_mut(opener).count -= used;
        let close = slots.delim_mut(closer);
        close.start += used;
        close.count -= used;

        // Delimiters inside the new node cannot open anything else.
        stack.truncate(at + 1);
        if slots.delim(opener).count == 0 {
            slots.unlink(opener);
            stack.pop();
        }
        cursor = if slots.delim(closer).count == 0 {
            let next = slots.next(closer);
            slots.unlink(closer);
            next
        } else {
            Some(closer)
        };
    }

    slots.move_into(items);
}
