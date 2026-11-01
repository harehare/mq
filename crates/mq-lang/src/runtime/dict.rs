//! The map held by [`RuntimeValue::Dict`](crate::RuntimeValue::Dict).

use crate::{Ident, RuntimeValue};
use hashbrown::HashTable;
use rustc_hash::FxBuildHasher;
use std::hash::BuildHasher;

type Slot = Option<(Ident, RuntimeValue)>;

/// Slot count below which removal never compacts.
const COMPACT_MIN_SLOTS: usize = 32;

/// An insertion-ordered map from keys to runtime values.
///
/// Keys are interned [`Ident`]s. Lookups take any [`DictKey`]: a `&str` lookup never interns
/// the key, so querying a dict with arbitrary strings does not grow the interner.
///
/// Entries live in insertion order in `slots`. Removal leaves a hole that is reclaimed once holes
/// outnumber live entries, so it is amortized O(1) and keeps the order of the remaining entries.
#[derive(Default)]
pub struct DictMap {
    slots: Vec<Slot>,
    /// Slot positions, hashed by the key stored in the slot.
    index: HashTable<usize>,
}

/// Keeps the spare capacity of `slots`, so inserting into a copy does not reallocate.
impl Clone for DictMap {
    fn clone(&self) -> Self {
        let mut slots = Vec::with_capacity(self.slots.capacity());
        slots.extend_from_slice(&self.slots);
        Self {
            slots,
            index: self.index.clone(),
        }
    }
}

impl std::fmt::Debug for DictMap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_map().entries(self.iter()).finish()
    }
}

/// Maps are equal when they hold the same entries, regardless of order.
impl PartialEq for DictMap {
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len() && self.iter().all(|(k, v)| other.get(k) == Some(v))
    }
}

mod sealed {
    pub trait Sealed {
        fn to_ident(&self) -> Option<crate::Ident>;
    }
}

/// A type that can look up a [`DictMap`] entry: [`str`], [`String`] or [`Ident`].
///
/// This trait is sealed and cannot be implemented outside this crate.
pub trait DictKey: sealed::Sealed {}

impl sealed::Sealed for str {
    #[inline]
    fn to_ident(&self) -> Option<Ident> {
        Ident::lookup(self)
    }
}
impl DictKey for str {}

impl sealed::Sealed for String {
    #[inline]
    fn to_ident(&self) -> Option<Ident> {
        Ident::lookup(self)
    }
}
impl DictKey for String {}

impl sealed::Sealed for Ident {
    #[inline]
    fn to_ident(&self) -> Option<Ident> {
        Some(*self)
    }
}
impl DictKey for Ident {}

impl<T: sealed::Sealed + ?Sized> sealed::Sealed for &T {
    #[inline]
    fn to_ident(&self) -> Option<Ident> {
        (**self).to_ident()
    }
}
impl<T: DictKey + ?Sized> DictKey for &T {}

#[inline]
fn hash_key(key: &Ident) -> u64 {
    FxBuildHasher.hash_one(key)
}

#[inline]
fn slot_hash(slots: &[Slot], slot: usize) -> u64 {
    slots[slot].as_ref().map_or(0, |(k, _)| hash_key(k))
}

impl DictMap {
    /// Creates an empty map.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates an empty map with room for `capacity` entries.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            slots: Vec::with_capacity(capacity),
            index: HashTable::with_capacity(capacity),
        }
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.index.len()
    }

    /// Whether the map has no entries.
    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    /// The value for `key`, if present.
    #[inline]
    pub fn get<K: DictKey + ?Sized>(&self, key: &K) -> Option<&RuntimeValue> {
        let slot = self.find(&key.to_ident()?)?;
        self.slots[slot].as_ref().map(|(_, v)| v)
    }

    /// A mutable reference to the value for `key`, if present.
    #[inline]
    pub fn get_mut<K: DictKey + ?Sized>(&mut self, key: &K) -> Option<&mut RuntimeValue> {
        let slot = self.find(&key.to_ident()?)?;
        self.slots[slot].as_mut().map(|(_, v)| v)
    }

    /// Whether the map has an entry for `key`.
    #[inline]
    pub fn contains_key<K: DictKey + ?Sized>(&self, key: &K) -> bool {
        key.to_ident().is_some_and(|key| self.find(&key).is_some())
    }

    /// Inserts `value` under `key`, keeping the position of an existing key, and returns the
    /// previous value.
    #[inline]
    pub fn insert(&mut self, key: impl Into<Ident>, value: RuntimeValue) -> Option<RuntimeValue> {
        let key = key.into();
        if let Some(slot) = self.find(&key) {
            return self.slots[slot].as_mut().map(|(_, v)| std::mem::replace(v, value));
        }
        let slots = &self.slots;
        self.index
            .insert_unique(hash_key(&key), slots.len(), |&slot| slot_hash(slots, slot));
        self.slots.push(Some((key, value)));
        None
    }

    /// Removes the entry for `key`, keeping the order of the remaining entries, and returns its
    /// value.
    pub fn remove<K: DictKey + ?Sized>(&mut self, key: &K) -> Option<RuntimeValue> {
        let key = key.to_ident()?;
        let slots = &self.slots;
        let slot = self
            .index
            .find_entry(
                hash_key(&key),
                |&slot| matches!(&slots[slot], Some((k, _)) if *k == key),
            )
            .ok()?
            .remove()
            .0;
        let (_, value) = self.slots[slot].take()?;

        if slot + 1 == self.slots.len() {
            while matches!(self.slots.last(), Some(None)) {
                self.slots.pop();
            }
        } else if self.slots.len() >= COMPACT_MIN_SLOTS && self.slots.len() > self.index.len() * 2 {
            self.compact();
        }
        Some(value)
    }

    /// Drops the holes left by removals and renumbers the index.
    fn compact(&mut self) {
        self.slots.retain(Option::is_some);
        self.index.clear();
        let slots = &self.slots;

        for (position, slot) in slots.iter().enumerate() {
            if let Some((key, _)) = slot {
                self.index
                    .insert_unique(hash_key(key), position, |&slot| slot_hash(slots, slot));
            }
        }
    }

    #[inline]
    fn find(&self, key: &Ident) -> Option<usize> {
        self.index
            .find(
                hash_key(key),
                |&slot| matches!(&self.slots[slot], Some((k, _)) if k == key),
            )
            .copied()
    }

    /// Iterates over the entries in insertion order.
    pub fn iter(&self) -> DictIter<'_> {
        DictIter {
            remaining: self.len(),
            inner: self.slots.iter(),
        }
    }

    /// Iterates over the entries in insertion order, with mutable values.
    pub fn iter_mut(&mut self) -> DictIterMut<'_> {
        DictIterMut {
            remaining: self.index.len(),
            inner: self.slots.iter_mut(),
        }
    }

    /// Iterates over the keys in insertion order.
    pub fn keys(&self) -> impl DoubleEndedIterator<Item = &Ident> + ExactSizeIterator {
        self.iter().map(|(k, _)| k)
    }

    /// Iterates over the values in insertion order.
    pub fn values(&self) -> impl DoubleEndedIterator<Item = &RuntimeValue> + ExactSizeIterator {
        self.iter().map(|(_, v)| v)
    }

    /// Iterates over the values in insertion order, mutably.
    pub fn values_mut(&mut self) -> impl DoubleEndedIterator<Item = &mut RuntimeValue> + ExactSizeIterator {
        self.iter_mut().map(|(_, v)| v)
    }
}

impl<K: DictKey + ?Sized> std::ops::Index<&K> for DictMap {
    type Output = RuntimeValue;

    /// Panics if `key` is not present.
    fn index(&self, key: &K) -> &RuntimeValue {
        self.get(key).expect("DictMap: key not found")
    }
}

impl<K: Into<Ident>> FromIterator<(K, RuntimeValue)> for DictMap {
    fn from_iter<I: IntoIterator<Item = (K, RuntimeValue)>>(iter: I) -> Self {
        let mut map = Self::new();
        map.extend(iter);
        map
    }
}

impl<K: Into<Ident>> Extend<(K, RuntimeValue)> for DictMap {
    fn extend<I: IntoIterator<Item = (K, RuntimeValue)>>(&mut self, iter: I) {
        for (key, value) in iter {
            self.insert(key, value);
        }
    }
}

/// Owning iterator over the entries of a [`DictMap`].
pub struct DictIntoIter {
    inner: std::vec::IntoIter<Slot>,
    remaining: usize,
}

/// Borrowing iterator over the entries of a [`DictMap`].
pub struct DictIter<'a> {
    inner: std::slice::Iter<'a, Slot>,
    remaining: usize,
}

/// Borrowing iterator over the entries of a [`DictMap`], with mutable values.
pub struct DictIterMut<'a> {
    inner: std::slice::IterMut<'a, Slot>,
    remaining: usize,
}

macro_rules! slot_iterator {
    ($ty:ty, $item:ty, $slot:ident => $conv:expr) => {
        impl<'a> Iterator for $ty {
            type Item = $item;

            #[inline]
            fn next(&mut self) -> Option<Self::Item> {
                for $slot in self.inner.by_ref() {
                    if let Some(item) = $conv {
                        debug_assert!(self.remaining > 0, "iterator count is out of sync");
                        self.remaining = self.remaining.saturating_sub(1);
                        return Some(item);
                    }
                }
                None
            }

            #[inline]
            fn size_hint(&self) -> (usize, Option<usize>) {
                (self.remaining, Some(self.remaining))
            }
        }

        impl<'a> DoubleEndedIterator for $ty {
            #[inline]
            fn next_back(&mut self) -> Option<Self::Item> {
                while let Some($slot) = self.inner.next_back() {
                    if let Some(item) = $conv {
                        debug_assert!(self.remaining > 0, "iterator count is out of sync");
                        self.remaining = self.remaining.saturating_sub(1);
                        return Some(item);
                    }
                }
                None
            }
        }

        impl<'a> ExactSizeIterator for $ty {}
    };
}

slot_iterator!(DictIntoIter, (Ident, RuntimeValue), slot => slot);
slot_iterator!(DictIter<'a>, (&'a Ident, &'a RuntimeValue), slot => slot.as_ref().map(|(k, v)| (k, v)));
slot_iterator!(DictIterMut<'a>, (&'a Ident, &'a mut RuntimeValue), slot => slot.as_mut().map(|(k, v)| (&*k, v)));

impl IntoIterator for DictMap {
    type Item = (Ident, RuntimeValue);
    type IntoIter = DictIntoIter;

    fn into_iter(self) -> Self::IntoIter {
        DictIntoIter {
            remaining: self.index.len(),
            inner: self.slots.into_iter(),
        }
    }
}

impl<'a> IntoIterator for &'a DictMap {
    type Item = (&'a Ident, &'a RuntimeValue);
    type IntoIter = DictIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a> IntoIterator for &'a mut DictMap {
    type Item = (&'a Ident, &'a mut RuntimeValue);
    type IntoIter = DictIterMut<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> DictMap {
        DictMap::from_iter([
            ("a", RuntimeValue::Number(1.into())),
            ("b", RuntimeValue::Number(2.into())),
        ])
    }

    #[test]
    fn test_lookup_by_str_string_and_ident() {
        let dict = sample();
        let expected = Some(&RuntimeValue::Number(1.into()));

        assert_eq!(dict.get("a"), expected);
        assert_eq!(dict.get(&"a".to_string()), expected);
        assert_eq!(dict.get(&Ident::new("a")), expected);
        assert!(dict.contains_key("b"));
    }

    #[test]
    fn test_str_lookup_does_not_intern_missing_key() {
        let mut dict = sample();

        assert!(dict.get("dict_key_only_used_by_this_test").is_none());
        assert!(!dict.contains_key("dict_key_only_used_by_this_test"));
        assert!(dict.get_mut("dict_key_only_used_by_this_test").is_none());
        assert!(dict.remove("dict_key_only_used_by_this_test").is_none());
        assert!(Ident::lookup("dict_key_only_used_by_this_test").is_none());
    }

    #[test]
    fn test_remove_keeps_insertion_order() {
        let mut dict = sample();
        dict.insert("c", RuntimeValue::Number(3.into()));

        assert_eq!(dict.remove("a"), Some(RuntimeValue::Number(1.into())));
        assert_eq!(dict.keys().map(Ident::to_string).collect::<Vec<_>>(), vec!["b", "c"]);
    }

    #[test]
    fn test_insert_existing_key_keeps_position() {
        let mut dict = sample();

        assert_eq!(
            dict.insert("a", RuntimeValue::Number(10.into())),
            Some(RuntimeValue::Number(1.into()))
        );
        assert_eq!(
            dict.iter().map(|(k, v)| (k.to_string(), v.clone())).collect::<Vec<_>>(),
            vec![
                ("a".to_string(), RuntimeValue::Number(10.into())),
                ("b".to_string(), RuntimeValue::Number(2.into())),
            ]
        );
    }

    #[test]
    fn test_remove_then_reinsert_moves_to_end() {
        let mut dict = sample();
        dict.remove("a");
        dict.insert("a", RuntimeValue::Number(1.into()));

        assert_eq!(dict.keys().map(Ident::to_string).collect::<Vec<_>>(), vec!["b", "a"]);
        assert_eq!(dict.len(), 2);
    }

    #[test]
    fn test_eq_ignores_order() {
        let mut other = DictMap::new();
        other.insert("b", RuntimeValue::Number(2.into()));
        other.insert("a", RuntimeValue::Number(1.into()));

        assert_eq!(sample(), other);
    }

    #[test]
    fn test_fifo_eviction_compacts_and_keeps_order() {
        let mut dict = DictMap::new();
        for i in 0..200 {
            dict.insert(format!("k{i}"), RuntimeValue::Number(i.into()));
            if i >= 10 {
                assert!(dict.remove(&format!("k{}", i - 10)).is_some());
            }
        }

        assert_eq!(dict.len(), 10);
        assert!(dict.slots.len() <= 32.max(dict.len() * 2 + 1));
        let expected: Vec<_> = (190..200).map(|i| format!("k{i}")).collect();
        assert_eq!(dict.keys().map(Ident::to_string).collect::<Vec<_>>(), expected);
        assert_eq!(dict.iter().rev().count(), 10);
        assert_eq!(dict.get("k195"), Some(&RuntimeValue::Number(195.into())));
    }

    #[rstest::rstest]
    #[case::empty(&[], &[])]
    #[case::remove_missing(&["-x"], &[])]
    #[case::remove_only(&["a", "-a"], &[])]
    #[case::remove_first(&["a", "b", "c", "-a"], &["b", "c"])]
    #[case::remove_middle(&["a", "b", "c", "-b"], &["a", "c"])]
    #[case::remove_last_then_push(&["a", "b", "-b", "c"], &["a", "c"])]
    #[case::reinsert_moves_to_end(&["a", "b", "-a", "a"], &["b", "a"])]
    #[case::duplicate_insert_keeps_position(&["a", "b", "a"], &["a", "b"])]
    #[case::remove_all(&["a", "b", "-a", "-b"], &[])]
    fn test_ops_yield_keys(#[case] ops: &[&str], #[case] expected: &[&str]) {
        let mut dict = DictMap::new();
        for (n, op) in ops.iter().enumerate() {
            match op.strip_prefix('-') {
                Some(key) => {
                    dict.remove(key);
                }
                None => {
                    dict.insert(*op, RuntimeValue::Number((n as i64).into()));
                }
            }
        }

        let forward: Vec<_> = dict.keys().map(Ident::to_string).collect();
        let mut backward: Vec<_> = dict.keys().rev().map(Ident::to_string).collect();
        backward.reverse();

        assert_eq!(forward, expected);
        assert_eq!(backward, expected);
        assert_eq!(dict.len(), expected.len());
        assert_eq!(dict.is_empty(), expected.is_empty());
    }

    #[test]
    fn test_mixed_front_and_back_iteration_skips_holes() {
        let mut dict = DictMap::new();
        for i in 0..6 {
            dict.insert(format!("h{i}"), RuntimeValue::Number(i.into()));
        }
        dict.remove("h0");
        dict.remove("h3");
        dict.remove("h5");

        let mut iter = dict.iter();
        assert_eq!(iter.len(), 3);
        assert_eq!(iter.next().map(|(k, _)| k.to_string()), Some("h1".to_string()));
        assert_eq!(iter.next_back().map(|(k, _)| k.to_string()), Some("h4".to_string()));
        assert_eq!(iter.len(), 1);
        assert_eq!(iter.next().map(|(k, _)| k.to_string()), Some("h2".to_string()));
        assert!(iter.next().is_none());
        assert!(iter.next_back().is_none());
    }

    #[test]
    fn test_iter_mut_and_into_iter_skip_holes() {
        let mut dict = sample();
        dict.insert("c", RuntimeValue::Number(3.into()));
        dict.remove("a");
        for (_, v) in dict.iter_mut() {
            *v = RuntimeValue::Number(0.into());
        }
        dict.values_mut().for_each(|v| *v = RuntimeValue::Number(7.into()));

        let owned: Vec<_> = dict.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
        assert_eq!(
            owned,
            vec![
                ("b".to_string(), RuntimeValue::Number(7.into())),
                ("c".to_string(), RuntimeValue::Number(7.into())),
            ]
        );
    }

    #[test]
    fn test_clone_and_get_mut_after_compaction() {
        let mut dict = DictMap::new();
        for i in 0..100 {
            dict.insert(format!("c{i}"), RuntimeValue::Number(i.into()));
        }
        for i in 0..90 {
            dict.remove(&format!("c{i}"));
        }
        let cloned = dict.clone();
        *dict.get_mut("c95").unwrap() = RuntimeValue::Number(0.into());

        assert_eq!(cloned.get("c95"), Some(&RuntimeValue::Number(95.into())));
        assert_eq!(dict.get("c95"), Some(&RuntimeValue::Number(0.into())));
        assert_ne!(cloned, dict);
        assert!(dict.get("c0").is_none());
    }

    mod model {
        use super::*;
        use proptest::prelude::*;

        #[derive(Clone, Debug)]
        enum Op {
            Insert(u8, i64),
            Remove(u8),
            Get(u8),
        }

        fn op() -> impl Strategy<Value = Op> {
            prop_oneof![
                (0u8..24, any::<i64>()).prop_map(|(k, v)| Op::Insert(k, v)),
                (0u8..24).prop_map(Op::Remove),
                (0u8..24).prop_map(Op::Get),
            ]
        }

        proptest! {
            /// `DictMap` behaves like an insertion-ordered association list.
            #[test]
            fn matches_ordered_list_model(ops in proptest::collection::vec(op(), 0..400)) {
                let mut dict = DictMap::new();
                let mut model: Vec<(String, i64)> = Vec::new();

                for op in ops {
                    match op {
                        Op::Insert(k, v) => {
                            let key = format!("pk{k}");
                            let prev = dict.insert(key.as_str(), RuntimeValue::Number(v.into()));
                            match model.iter_mut().find(|(mk, _)| *mk == key) {
                                Some(entry) => {
                                    prop_assert_eq!(prev, Some(RuntimeValue::Number(entry.1.into())));
                                    entry.1 = v;
                                }
                                None => {
                                    prop_assert_eq!(prev, None);
                                    model.push((key, v));
                                }
                            }
                        }
                        Op::Remove(k) => {
                            let key = format!("pk{k}");
                            let removed = dict.remove(key.as_str());
                            let expected = model.iter().position(|(mk, _)| *mk == key).map(|i| model.remove(i).1);
                            prop_assert_eq!(removed, expected.map(|v| RuntimeValue::Number(v.into())));
                        }
                        Op::Get(k) => {
                            let key = format!("pk{k}");
                            let expected = model.iter().find(|(mk, _)| *mk == key).map(|(_, v)| RuntimeValue::Number((*v).into()));
                            prop_assert_eq!(dict.get(key.as_str()).cloned(), expected);
                        }
                    }

                    prop_assert_eq!(dict.len(), model.len());
                }

                let entries: Vec<_> = dict.iter().map(|(k, v)| (k.to_string(), v.clone())).collect();
                let expected: Vec<_> = model.iter().map(|(k, v)| (k.clone(), RuntimeValue::Number((*v).into()))).collect();
                prop_assert_eq!(&entries, &expected);
                prop_assert_eq!(dict.iter().rev().count(), model.len());
                prop_assert_eq!(dict.iter().len(), model.len());
                prop_assert_eq!(dict.keys().len(), model.len());
                prop_assert_eq!(dict.values().len(), model.len());
                prop_assert_eq!(dict.clone().into_iter().len(), model.len());
                prop_assert_eq!(dict.clone().into_iter().count(), model.len());
            }
        }
    }
}
