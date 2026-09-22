//! Myers-only text differences shared by builtins and the CLI.

use similar::algorithms::{Capture, Compact, IdentifyDistinct, Replace, myers};
use similar::{Change, DiffOp, DiffableStr, group_diff_ops};
use std::hash::Hash;
use std::ops::Index;

/// A text difference using the same tokenization and compaction as `similar::TextDiff`.
///
/// The algorithm is fixed to Myers so unused algorithm implementations need not
/// be linked into consumers. Large inputs retain integer remapping and the
/// identical-input fast path used by `similar`.
///
/// ```
/// use mq_lang::diff::TextDiff;
/// use similar::ChangeTag;
///
/// let diff = TextDiff::from_lines("before\n", "after\n");
/// let tags: Vec<_> = diff.iter_all_changes().map(|change| change.tag()).collect();
/// assert_eq!(tags, [ChangeTag::Delete, ChangeTag::Insert]);
/// ```
pub struct TextDiff<'a> {
    old: Vec<&'a str>,
    new: Vec<&'a str>,
    ops: Vec<DiffOp>,
}

impl<'a> TextDiff<'a> {
    /// Compares lines, retaining LF, CRLF, and CR terminators.
    pub fn from_lines(old: &'a str, new: &'a str) -> Self {
        Self::new(old.tokenize_lines(), new.tokenize_lines())
    }

    /// Compares Unicode scalar values without allocating individual strings.
    pub fn from_chars(old: &'a str, new: &'a str) -> Self {
        Self::new(old.tokenize_chars(), new.tokenize_chars())
    }

    /// Compares arbitrary string tokens.
    pub fn from_slices(old: &[&'a str], new: &[&'a str]) -> Self {
        Self::new(old.to_vec(), new.to_vec())
    }

    fn new(old: Vec<&'a str>, new: Vec<&'a str>) -> Self {
        let (ol, nl) = (old.len(), new.len());
        let ops = if ol > 100 || nl > 100 {
            if old == new {
                vec![DiffOp::Equal {
                    old_index: 0,
                    new_index: 0,
                    len: ol,
                }]
            } else {
                let ids = IdentifyDistinct::<u32>::new(&old, 0..ol, &new, 0..nl);
                capture(ids.old_lookup(), ol, ids.new_lookup(), nl)
            }
        } else {
            capture(&old, ol, &new, nl)
        };
        Self { old, new, ops }
    }

    /// Iterates equal, deleted, and inserted tokens in display order.
    pub fn iter_all_changes(&self) -> impl Iterator<Item = Change<&'a str>> {
        self.ops.iter().flat_map(|op| op.iter_changes(&self.old, &self.new))
    }

    /// Returns operations grouped with the requested number of context tokens.
    pub fn grouped_ops(&self, context: usize) -> Vec<Vec<DiffOp>> {
        group_diff_ops(self.ops.clone(), context)
    }

    /// Iterates the changes represented by a single operation.
    pub fn iter_changes(&self, op: &DiffOp) -> impl Iterator<Item = Change<&'a str>> {
        op.iter_changes(&self.old, &self.new)
    }
}

fn capture<O, N>(old: &O, ol: usize, new: &N, nl: usize) -> Vec<DiffOp>
where
    O: Index<usize> + ?Sized,
    N: Index<usize> + ?Sized,
    O::Output: Hash + Eq,
    N::Output: PartialEq<O::Output> + Hash + Eq,
{
    let mut hook = Compact::new(Replace::new(Capture::new()), old, new);
    // Capture's hook error is Infallible.
    match myers::diff(&mut hook, old, 0..ol, new, 0..nl) {
        Ok(()) => hook.into_inner().into_inner().into_ops(),
        Err(error) => match error {},
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use similar::TextDiff as ReferenceTextDiff;

    fn compare(old: &str, new: &str) {
        for (actual, expected) in [
            (TextDiff::from_lines(old, new), ReferenceTextDiff::from_lines(old, new)),
            (TextDiff::from_chars(old, new), ReferenceTextDiff::from_chars(old, new)),
        ] {
            assert_eq!(actual.ops, expected.ops(), "{old:?} -> {new:?}");
            let changes: Vec<_> = actual
                .iter_all_changes()
                .map(|c| (c.tag(), c.old_index(), c.new_index(), c.value()))
                .collect();
            let reference: Vec<_> = expected
                .iter_all_changes()
                .map(|c| (c.tag(), c.old_index(), c.new_index(), c.value()))
                .collect();
            assert_eq!(changes, reference);
        }
    }

    #[test]
    fn matches_text_diff_at_token_and_remapping_boundaries() {
        for (old, new) in [
            ("", ""),
            ("", "a"),
            ("a", ""),
            ("a\r\nb\rc\n", "a\nb\r\nc"),
            ("a\na\nb\n", "a\nb\na\n"),
            ("日本語🙂e\u{301}", "日本語🙃é"),
        ] {
            compare(old, new);
        }
        for len in [99, 100, 101, 1000] {
            let old = "a\nb\n".repeat(len);
            compare(&old, &old);
            compare(&old, &format!("x\n{old}y\n"));
            compare(&old, &"c\nd\n".repeat(len));
            let tokens = vec!["a"; len];
            let mut new = tokens.clone();
            new[len / 2] = "b";
            assert_eq!(
                TextDiff::from_slices(&tokens, &new).ops,
                ReferenceTextDiff::from_slices(&tokens, &new).ops()
            );
        }
    }

    #[test]
    fn matches_text_diff_for_generated_unicode_and_repeated_tokens() {
        let mut seed = 42u64;
        let mut make = || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            let len = (seed >> 32) as usize % 180;
            let mut s = String::new();
            for _ in 0..len {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                s.push_str(["a", "b", "\n", "\r", "日本語", "🙂", " "][(seed >> 32) as usize % 7]);
            }
            s
        };
        for _ in 0..500 {
            compare(&make(), &make());
        }
    }
}
