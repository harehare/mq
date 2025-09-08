#[cfg(feature = "ast-json")]
use serde::{Deserialize, Serialize};
use std::{marker::PhantomData, ops::Index};

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
    pub const fn new(id: u32) -> ArenaId<T> {
        Self {
            id,
            _phantom_data: PhantomData,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Arena<T> {
    items: Vec<T>,
}

impl<T: Clone + PartialEq> Arena<T> {
    pub fn new(size: usize) -> Self {
        Arena {
            items: Vec::with_capacity(size),
        }
    }

    pub fn alloc(&mut self, value: T) -> ArenaId<T> {
        let arena_id = self.items.len() as u32;
        self.items.push(value);
        ArenaId::new(arena_id)
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn contains(&self, value: T) -> bool {
        self.items.contains(&value)
    }
}

impl<T> Index<ArenaId<T>> for Arena<T> {
    type Output = T;

    fn index(&self, index: ArenaId<T>) -> &Self::Output {
        &self.items[index.id as usize]
    }
}

impl<T> Arena<T> {
    /// Returns a reference to the element at the given `ArenaId`, or `None` if out of bounds.
    pub fn get(&self, id: ArenaId<T>) -> Option<&T> {
        self.items.get(id.id as usize)
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
    fn test_from() {
        let id_u32: ArenaId<i32> = 5u32.into();
        assert_eq!(id_u32.id, 5);

        let id_usize: ArenaId<i32> = 10usize.into();
        assert_eq!(id_usize.id, 10);

        let id_i32: ArenaId<i32> = 15i32.into();
        assert_eq!(id_i32.id, 15);
    }
}
