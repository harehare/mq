//! Myers text diff over `&str` tokens (lines or chars).
//!
//! Internal to the mq crates, not a stable API.
//!
//! Calls `similar`'s Myers implementation directly so the unused Patience and LCS
//! algorithms are not linked into the binary.
use similar::algorithms::{Capture, Compact, Replace, myers};
use similar::udiff::UnifiedHunkHeader;
use similar::{DiffOp, DiffableStr, group_diff_ops};
use std::fmt::Write;

pub use similar::ChangeTag;

/// One token of a diff: kept, deleted or inserted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Change<'a> {
    tag: ChangeTag,
    old_index: Option<usize>,
    new_index: Option<usize>,
    value: &'a str,
}

impl<'a> Change<'a> {
    /// Whether the token is kept, deleted or inserted.
    pub fn tag(&self) -> ChangeTag {
        self.tag
    }

    /// Index in the old tokens, `None` for an insertion.
    pub fn old_index(&self) -> Option<usize> {
        self.old_index
    }

    /// Index in the new tokens, `None` for a deletion.
    pub fn new_index(&self) -> Option<usize> {
        self.new_index
    }

    /// The token text.
    pub fn value(&self) -> &'a str {
        self.value
    }
}

/// Splits `s` into lines, each keeping its trailing newline.
pub fn lines(s: &str) -> Vec<&str> {
    s.tokenize_lines()
}

/// Splits `s` into single-character tokens.
pub fn chars(s: &str) -> Vec<&str> {
    s.tokenize_chars()
}

fn diff_ops(old: &[&str], new: &[&str]) -> Vec<DiffOp> {
    let mut hook = Compact::new(Replace::new(Capture::new()), old, new);
    myers::diff(&mut hook, old, 0..old.len(), new, 0..new.len()).unwrap_or_else(|never| match never {});
    hook.into_inner().into_inner().into_ops()
}

/// Diffs two token lists, returning every token in order.
pub fn changes<'a>(old: &[&'a str], new: &[&'a str]) -> Vec<Change<'a>> {
    diff_ops(old, new)
        .iter()
        .flat_map(|op| op.iter_changes(old, new))
        .map(|change| Change {
            tag: change.tag(),
            old_index: change.old_index(),
            new_index: change.new_index(),
            value: change.value(),
        })
        .collect()
}

/// A unified diff of `old` against `new` with 3 lines of context, headed by `label`.
pub fn unified(old: &str, new: &str, label: &str) -> String {
    let (old_lines, new_lines) = (lines(old), lines(new));
    let mut out = String::new();
    let hunks = group_diff_ops(diff_ops(&old_lines, &new_lines), 3);
    for (i, hunk) in hunks.iter().filter(|hunk| !hunk.is_empty()).enumerate() {
        if i == 0 {
            let _ = writeln!(out, "--- {label}\n+++ {label}");
        }
        let _ = writeln!(out, "{}", UnifiedHunkHeader::new(hunk));
        for change in hunk.iter().flat_map(|op| op.iter_changes(&old_lines, &new_lines)) {
            let value = change.value();
            let _ = write!(out, "{}{value}", change.tag());
            if !value.ends_with('\n') {
                out.push_str("\n\\ No newline at end of file\n");
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use similar::TextDiff;

    fn assert_same(expected: Vec<(ChangeTag, Option<usize>, Option<usize>, String)>, actual: Vec<Change>) {
        let actual: Vec<_> = actual
            .iter()
            .map(|c| (c.tag(), c.old_index(), c.new_index(), c.value().to_string()))
            .collect();
        assert_eq!(expected, actual);
    }

    fn expected_changes(diff: &TextDiff<'_, '_, str>) -> Vec<(ChangeTag, Option<usize>, Option<usize>, String)> {
        diff.iter_all_changes()
            .map(|c| (c.tag(), c.old_index(), c.new_index(), c.value().to_string()))
            .collect()
    }

    proptest! {
        #[test]
        fn test_line_changes_match_similar(old in "[ab\n]{0,40}", new in "[ab\n]{0,40}") {
            assert_same(
                expected_changes(&TextDiff::from_lines(old.as_str(), new.as_str())),
                changes(&lines(&old), &lines(&new)),
            );
        }

        #[test]
        fn test_char_changes_match_similar(old in "[abc あ]{0,30}", new in "[abc あ]{0,30}") {
            assert_same(
                expected_changes(&TextDiff::from_chars(old.as_str(), new.as_str())),
                changes(&chars(&old), &chars(&new)),
            );
        }

        #[test]
        fn test_unified_matches_similar(old in "[ab\n]{0,60}", new in "[ab\n]{0,60}") {
            let expected = TextDiff::from_lines(old.as_str(), new.as_str())
                .unified_diff()
                .header("x", "x")
                .to_string();
            assert_eq!(expected, unified(&old, &new, "x"));
        }
    }
}
