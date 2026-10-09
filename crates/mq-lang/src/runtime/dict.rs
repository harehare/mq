//! The map held by [`RuntimeValue::Dict`](crate::RuntimeValue::Dict).

use crate::{Ident, RuntimeValue};
use indexmap::IndexMap;
use rustc_hash::FxBuildHasher;

type Inner = IndexMap<Ident, RuntimeValue, FxBuildHasher>;

/// An insertion-ordered map from keys to runtime values.
///
/// Keys are interned [`Ident`]s. Lookups take any [`DictKey`]: a `&str` lookup never interns
/// the key, so querying a dict with arbitrary strings does not grow the interner.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DictMap(Inner);

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

impl DictMap {
    /// Creates an empty map.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates an empty map with room for `capacity` entries.
    pub fn with_capacity(capacity: usize) -> Self {
        Self(Inner::with_capacity_and_hasher(capacity, FxBuildHasher))
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the map has no entries.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The value for `key`, if present.
    #[inline]
    pub fn get<K: DictKey + ?Sized>(&self, key: &K) -> Option<&RuntimeValue> {
        self.0.get(&key.to_ident()?)
    }

    /// A mutable reference to the value for `key`, if present.
    #[inline]
    pub fn get_mut<K: DictKey + ?Sized>(&mut self, key: &K) -> Option<&mut RuntimeValue> {
        self.0.get_mut(&key.to_ident()?)
    }

    /// Whether the map has an entry for `key`.
    #[inline]
    pub fn contains_key<K: DictKey + ?Sized>(&self, key: &K) -> bool {
        key.to_ident().is_some_and(|key| self.0.contains_key(&key))
    }

    /// Inserts `value` under `key`, keeping the position of an existing key, and returns the
    /// previous value.
    #[inline]
    pub fn insert(&mut self, key: impl Into<Ident>, value: RuntimeValue) -> Option<RuntimeValue> {
        self.0.insert(key.into(), value)
    }

    /// Removes the entry for `key`, keeping the order of the remaining entries, and returns its
    /// value.
    pub fn remove<K: DictKey + ?Sized>(&mut self, key: &K) -> Option<RuntimeValue> {
        self.0.shift_remove(&key.to_ident()?)
    }

    /// Iterates over the entries in insertion order.
    pub fn iter(&self) -> DictIter<'_> {
        DictIter(self.0.iter())
    }

    /// Iterates over the entries in insertion order, with mutable values.
    pub fn iter_mut(&mut self) -> DictIterMut<'_> {
        DictIterMut(self.0.iter_mut())
    }

    /// Iterates over the keys in insertion order.
    pub fn keys(&self) -> impl DoubleEndedIterator<Item = &Ident> + ExactSizeIterator {
        self.0.keys()
    }

    /// Iterates over the values in insertion order.
    pub fn values(&self) -> impl DoubleEndedIterator<Item = &RuntimeValue> + ExactSizeIterator {
        self.0.values()
    }

    /// Iterates over the values in insertion order, mutably.
    pub fn values_mut(&mut self) -> impl DoubleEndedIterator<Item = &mut RuntimeValue> + ExactSizeIterator {
        self.0.values_mut()
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
        Self(iter.into_iter().map(|(k, v)| (k.into(), v)).collect())
    }
}

impl<K: Into<Ident>> Extend<(K, RuntimeValue)> for DictMap {
    fn extend<I: IntoIterator<Item = (K, RuntimeValue)>>(&mut self, iter: I) {
        self.0.extend(iter.into_iter().map(|(k, v)| (k.into(), v)));
    }
}

/// Owning iterator over the entries of a [`DictMap`].
pub struct DictIntoIter(indexmap::map::IntoIter<Ident, RuntimeValue>);

/// Borrowing iterator over the entries of a [`DictMap`].
pub struct DictIter<'a>(indexmap::map::Iter<'a, Ident, RuntimeValue>);

/// Borrowing iterator over the entries of a [`DictMap`], with mutable values.
pub struct DictIterMut<'a>(indexmap::map::IterMut<'a, Ident, RuntimeValue>);

macro_rules! delegate_iterator {
    ($ty:ty, $item:ty) => {
        impl<'a> Iterator for $ty {
            type Item = $item;

            #[inline]
            fn next(&mut self) -> Option<Self::Item> {
                self.0.next()
            }

            #[inline]
            fn size_hint(&self) -> (usize, Option<usize>) {
                self.0.size_hint()
            }
        }

        impl<'a> DoubleEndedIterator for $ty {
            #[inline]
            fn next_back(&mut self) -> Option<Self::Item> {
                self.0.next_back()
            }
        }

        impl<'a> ExactSizeIterator for $ty {}
    };
}

delegate_iterator!(DictIntoIter, (Ident, RuntimeValue));
delegate_iterator!(DictIter<'a>, (&'a Ident, &'a RuntimeValue));
delegate_iterator!(DictIterMut<'a>, (&'a Ident, &'a mut RuntimeValue));

impl IntoIterator for DictMap {
    type Item = (Ident, RuntimeValue);
    type IntoIter = DictIntoIter;

    fn into_iter(self) -> Self::IntoIter {
        DictIntoIter(self.0.into_iter())
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
}
