//! Char position lookups on strings. Long strings get a one-entry cached sparse index so that
//! scanning with `s[i]` or `s[a:b]` stays linear instead of re-walking the string on every call.

use crate::Shared;
use std::cell::RefCell;
#[cfg(not(feature = "sync"))]
use std::rc::Weak;
#[cfg(feature = "sync")]
use std::sync::Weak;

/// Strings shorter than this many bytes are scanned directly.
const MIN_INDEXED_LEN: usize = 1024;
/// One byte offset is kept per this many chars.
const STRIDE: usize = 64;

struct CharIndex {
    /// A live `Weak` keeps the allocation reserved and makes `Shared::make_mut` move instead of
    /// mutate, so a pointer match means the content is unchanged.
    owner: Weak<String>,
    char_len: usize,
    /// `marks[k]` is the byte offset of char `k * STRIDE`.
    marks: Vec<usize>,
}

thread_local! {
    static CACHE: RefCell<Option<CharIndex>> = const { RefCell::new(None) };
}

impl CharIndex {
    fn build(s: &Shared<String>) -> Self {
        let mut marks = Vec::with_capacity(s.len() / STRIDE + 1);
        let mut char_len = 0;
        for (byte, _) in s.char_indices() {
            if char_len % STRIDE == 0 {
                marks.push(byte);
            }
            char_len += 1;
        }
        Self {
            owner: Shared::downgrade(s),
            char_len,
            marks,
        }
    }

    fn char_to_byte(&self, s: &str, n: usize) -> usize {
        if self.char_len == s.len() {
            return n.min(s.len());
        }
        if n >= self.char_len {
            return s.len();
        }
        let base = self.marks[n / STRIDE];
        base + scan_char_to_byte(&s[base..], n % STRIDE)
    }
}

fn scan_char_to_byte(s: &str, n: usize) -> usize {
    s.char_indices().nth(n).map_or(s.len(), |(byte, _)| byte)
}

fn with_index<R>(s: &Shared<String>, f: impl FnOnce(&CharIndex) -> R) -> R {
    CACHE.with(|cache| {
        let index = match cache.borrow_mut().take() {
            Some(index) if std::ptr::eq(index.owner.as_ptr(), Shared::as_ptr(s)) => index,
            _ => CharIndex::build(s),
        };
        let result = f(&index);
        *cache.borrow_mut() = Some(index);
        result
    })
}

/// Number of chars in `s`. An ASCII string has one char per byte, which `is_ascii` confirms several
/// times faster than counting, and a string with other chars fails the check at the first of them.
pub(super) fn count_chars(s: &str) -> usize {
    if s.is_ascii() { s.len() } else { s.chars().count() }
}

/// Number of chars in `s`.
pub(super) fn char_len(s: &Shared<String>) -> usize {
    if s.len() < MIN_INDEXED_LEN {
        count_chars(s)
    } else {
        with_index(s, |index| index.char_len)
    }
}

/// Byte offset of the `n`-th char of `s`, or `s.len()` when `s` has fewer chars.
pub(super) fn char_to_byte(s: &Shared<String>, n: usize) -> usize {
    if s.len() < MIN_INDEXED_LEN {
        scan_char_to_byte(s, n)
    } else {
        with_index(s, |index| index.char_to_byte(s, n))
    }
}
