use crate::{Shared, SharedCell};
#[cfg(feature = "ast-json")]
use serde::{Deserialize, Serialize};
use std::{marker::PhantomData, ops::Index};

/// A type-safe identifier for elements stored in an [`Arena`].
///
/// Uses phantom data to ensure type safety - an `ArenaId<A>` cannot be used
/// to access elements from an `Arena<B>`.
#[cfg_attr(feature = "ast-json", derive(Serialize, Deserialize))]
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ArenaId<T> {
    id: u32,
    _phantom_data: PhantomData<T>,
}

impl<T> Copy for ArenaId<T> {}

impl<T> Clone for ArenaId<T> {
    #[inline(always)]
    fn clone(&self) -> ArenaId<T> {
        *self
    }
}

impl<T> std::hash::Hash for ArenaId<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl<T> From<u32> for ArenaId<T> {
    fn from(id: u32) -> Self {
        Self::new(id)
    }
}

impl<T> From<usize> for ArenaId<T> {
    fn from(id: usize) -> Self {
        Self::new(id as u32)
    }
}

impl<T> From<i32> for ArenaId<T> {
    fn from(id: i32) -> Self {
        Self::new(id as u32)
    }
}

impl<T> ArenaId<T> {
    /// Creates a new arena identifier from a raw `u32` index.
    pub const fn new(id: u32) -> ArenaId<T> {
        Self {
            id,
            _phantom_data: PhantomData,
        }
    }

    /// The raw `u32` index.
    pub(crate) const fn raw(self) -> u32 {
        self.id
    }
}

/// An arena allocator for efficiently storing and accessing elements.
///
/// The arena allocates elements sequentially and returns type-safe [`ArenaId`]s
/// that can be used to retrieve elements later. This pattern provides fast allocation
/// and cache-friendly access.
///
/// A layered arena (see [`Arena::layered`]) sits on a parent arena: it allocates its own
/// elements and still resolves the parent's ids, so it can be dropped once its ids are unused.
#[derive(Debug, Clone, Default)]
pub struct Arena<T> {
    /// Leading elements shared with other arenas. Ids index it before `items`.
    prefix: Option<Shared<Vec<T>>>,
    items: Vec<T>,
    parent: Option<Shared<SharedCell<Arena<T>>>>,
}

/// Marks ids allocated in a layered arena, keeping them apart from its parent's.
const LAYER_BIT: u32 = 1 << 31;

impl<T: Clone + PartialEq> Arena<T> {
    /// Creates a new arena with the specified initial capacity.
    pub fn new(size: usize) -> Self {
        Arena {
            prefix: None,
            items: Vec::with_capacity(size),
            parent: None,
        }
    }

    /// Creates an arena layered on `parent`.
    pub(crate) fn layered(parent: Shared<SharedCell<Arena<T>>>) -> Self {
        Arena {
            prefix: None,
            items: Vec::new(),
            parent: Some(parent),
        }
    }

    /// Allocates a value in the arena and returns its identifier.
    pub fn alloc(&mut self, value: T) -> ArenaId<T> {
        let index = self.len() as u32;
        self.items.push(value);
        match self.parent {
            Some(_) => ArenaId::new(index | LAYER_BIT),
            None => ArenaId::new(index),
        }
    }

    /// Applies `f` to the element at `id`, looking through to the parent arena, or to `None` if
    /// no arena in the chain holds it.
    pub(crate) fn with<R>(&self, id: ArenaId<T>, f: impl FnOnce(Option<&T>) -> R) -> R {
        if let Some(item) = self.get(id) {
            return f(Some(item));
        }
        let Some(parent) = self.parent.as_ref() else {
            return f(None);
        };
        #[cfg(not(feature = "sync"))]
        let parent = parent.borrow();
        #[cfg(feature = "sync")]
        let parent = parent.read().unwrap();
        parent.with(id, f)
    }

    /// The parent of a layered arena.
    pub(crate) fn parent(&self) -> Option<&Shared<SharedCell<Arena<T>>>> {
        self.parent.as_ref()
    }

    /// Returns the number of elements in the arena.
    pub fn len(&self) -> usize {
        self.prefix_len() + self.items.len()
    }

    /// Returns `true` if the arena contains no elements.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns `true` if the arena contains the specified value.
    pub fn contains(&self, value: T) -> bool {
        self.prefix.as_ref().is_some_and(|prefix| prefix.contains(&value)) || self.items.contains(&value)
    }

    /// Extends the arena by cloning elements from a slice.
    pub fn extend_from_slice(&mut self, items: &[T]) {
        self.items.extend_from_slice(items);
    }

    /// Makes `prefix` the leading elements of this arena without copying them. Applies only to an
    /// arena that holds nothing but the first element of `prefix`, so every id it handed out
    /// still resolves to the same element. Returns whether it applied.
    pub(crate) fn share_prefix(&mut self, prefix: Shared<Vec<T>>) -> bool {
        if self.prefix.is_some()
            || self.parent.is_some()
            || self.items.len() != 1
            || prefix.first() != self.items.first()
        {
            return false;
        }
        self.items.clear();
        self.prefix = Some(prefix);
        true
    }

    /// A copy of every element, in id order.
    pub(crate) fn to_vec(&self) -> Vec<T> {
        self.iter().cloned().collect()
    }
}

impl<T> Index<ArenaId<T>> for Arena<T> {
    type Output = T;

    fn index(&self, index: ArenaId<T>) -> &Self::Output {
        self.get(index).expect("id belongs to this arena")
    }
}

impl<T> Arena<T> {
    /// Returns a reference to the element this arena itself holds at `id`, or `None` if out of
    /// bounds or held by a parent arena.
    pub fn get(&self, id: ArenaId<T>) -> Option<&T> {
        let layered = id.id & LAYER_BIT != 0;
        if layered != self.parent.is_some() {
            return None;
        }
        let index = (id.id & !LAYER_BIT) as usize;
        match &self.prefix {
            Some(prefix) if index < prefix.len() => prefix.get(index),
            Some(prefix) => self.items.get(index - prefix.len()),
            None => self.items.get(index),
        }
    }

    /// Iterates over the elements this arena itself holds, in id order.
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        let prefix = self.prefix.as_deref().map_or(&[][..], Vec::as_slice);
        prefix.iter().chain(&self.items)
    }

    fn prefix_len(&self) -> usize {
        self.prefix.as_ref().map_or(0, |prefix| prefix.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case(vec![1, 2, 3], 1, true)]
    #[case(vec![1, 2, 3], 4, false)]
    #[case(Vec::new(), 1, false)]
    fn test_contains(#[case] values: Vec<i32>, #[case] value: i32, #[case] expected: bool) {
        let mut arena = Arena::new(values.len());
        for v in values {
            arena.alloc(v);
        }
        assert_eq!(arena.contains(value), expected);
    }

    #[rstest]
    #[case(vec![1, 2, 3], 1, 2)]
    #[case(vec![1, 2, 3], 0, 1)]
    #[case(vec![1, 2, 3], 2, 3)]
    fn test_get(#[case] values: Vec<i32>, #[case] index: u32, #[case] expected: i32) {
        let mut arena = Arena::new(values.len());
        for v in values {
            arena.alloc(v);
        }
        let id = ArenaId::new(index);
        assert_eq!(arena[id], expected);
    }

    #[rstest]
    #[case(vec![1, 2, 3], 3)]
    #[case(Vec::new(), 0)]
    fn test_len(#[case] values: Vec<i32>, #[case] expected: usize) {
        let mut arena = Arena::new(values.len());
        for v in values {
            arena.alloc(v);
        }
        assert_eq!(arena.len(), expected);
    }

    #[rstest]
    #[case(vec![1, 2, 3], false)]
    #[case(Vec::new(), true)]
    fn test_is_empty(#[case] values: Vec<i32>, #[case] expected: bool) {
        let mut arena = Arena::new(values.len());
        for v in values {
            arena.alloc(v);
        }
        assert_eq!(arena.is_empty(), expected);
    }

    #[test]
    fn test_layered_arena_resolves_its_own_and_parent_ids() {
        let parent = Shared::new(SharedCell::new(Arena::new(1)));
        let parent_id = {
            #[cfg(not(feature = "sync"))]
            let mut parent = parent.borrow_mut();
            #[cfg(feature = "sync")]
            let mut parent = parent.write().unwrap();
            parent.alloc(1)
        };
        let mut layered = Arena::layered(Shared::clone(&parent));
        let own_id = layered.alloc(2);

        assert_eq!(layered.with(own_id, |v| v.copied()), Some(2));
        assert_eq!(layered.with(parent_id, |v| v.copied()), Some(1));
        assert_eq!(layered.get(parent_id), None);
        assert_eq!(layered[own_id], 2);
        #[cfg(not(feature = "sync"))]
        let parent = parent.borrow();
        #[cfg(feature = "sync")]
        let parent = parent.read().unwrap();
        assert_eq!(parent.with(own_id, |v| v.copied()), None);
        assert_eq!(parent.get(own_id), None);
    }

    #[test]
    fn test_shared_prefix_resolves_ids_and_continues_after_it() {
        let prefix = Shared::new(vec![10, 11, 12]);
        let mut arena = Arena::new(4);
        let first = arena.alloc(10);
        assert!(arena.share_prefix(Shared::clone(&prefix)));

        let next = arena.alloc(13);
        assert_eq!(arena[first], 10);
        assert_eq!(arena[ArenaId::new(2)], 12);
        assert_eq!(next.raw(), 3);
        assert_eq!(arena[next], 13);
        assert_eq!(arena.len(), 4);
        assert!(arena.contains(11) && arena.contains(13));
        assert_eq!(arena.to_vec(), vec![10, 11, 12, 13]);
    }

    #[test]
    fn test_share_prefix_requires_a_matching_first_element() {
        let prefix = Shared::new(vec![10, 11]);

        let mut other_first = Arena::new(2);
        other_first.alloc(99);
        assert!(!other_first.share_prefix(Shared::clone(&prefix)));
        assert_eq!(other_first.to_vec(), vec![99]);

        let mut already_grown = Arena::new(2);
        already_grown.alloc(10);
        already_grown.alloc(11);
        assert!(!already_grown.share_prefix(prefix));
    }

    #[test]
    fn test_from() {
        let id_u32: ArenaId<i32> = 5u32.into();
        assert_eq!(id_u32.id, 5);

        let id_usize: ArenaId<i32> = 10usize.into();
        assert_eq!(id_usize.id, 10);

        let id_i32: ArenaId<i32> = 15i32.into();
        assert_eq!(id_i32.id, 15);
    }
}
