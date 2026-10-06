//! Sets of Markdown node kinds, the payload of [`Type::Node`](crate::types::Type::Node).

use mq_markdown::NodeKind;

/// A set of [`NodeKind`]s as a bitset. `markdown` is the set of every kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct KindSet(u64);

impl KindSet {
    /// No kind. A node type with this set is unreachable.
    pub const EMPTY: Self = Self(0);
    /// Every kind, written `markdown`.
    pub const ALL: Self = Self((1 << NodeKind::ALL.len()) - 1);
    /// Headings of any depth, written `h`.
    pub const HEADING: Self = Self(
        Self::bit(NodeKind::H1)
            | Self::bit(NodeKind::H2)
            | Self::bit(NodeKind::H3)
            | Self::bit(NodeKind::H4)
            | Self::bit(NodeKind::H5)
            | Self::bit(NodeKind::H6),
    );

    const fn bit(kind: NodeKind) -> u64 {
        1 << kind as u8
    }

    /// The set holding only `kind`.
    pub const fn of(kind: NodeKind) -> Self {
        Self(Self::bit(kind))
    }

    /// The set holding every kind in `kinds`.
    pub fn from_kinds(kinds: impl IntoIterator<Item = NodeKind>) -> Self {
        kinds
            .into_iter()
            .fold(Self::EMPTY, |set, kind| set.union(Self::of(kind)))
    }

    pub const fn contains(self, kind: NodeKind) -> bool {
        self.0 & Self::bit(kind) != 0
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub const fn is_all(self) -> bool {
        self.0 == Self::ALL.0
    }

    pub const fn len(self) -> usize {
        self.0.count_ones() as usize
    }

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub const fn intersect(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }

    /// The kinds of `self` that are not in `other`.
    pub const fn difference(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }

    pub const fn is_subset_of(self, other: Self) -> bool {
        self.0 & !other.0 == 0
    }

    pub const fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    /// The kinds in the set, in declaration order.
    pub fn kinds(self) -> impl Iterator<Item = NodeKind> {
        NodeKind::ALL.into_iter().filter(move |kind| self.contains(*kind))
    }

    /// Renders as `markdown`, `h1 | h2`, `code`, or `markdown - code` when most kinds are in.
    pub fn display(self) -> String {
        if self.is_all() {
            return "markdown".to_string();
        }
        if self.is_empty() {
            return "never".to_string();
        }
        if self.len() > NodeKind::ALL.len() / 2 {
            let excluded = Self::ALL.difference(self).names().join(" - ");
            return format!("markdown - {excluded}");
        }
        self.names().join(" | ")
    }

    /// Kind names, with the six heading depths collapsed into `h` when all are present.
    fn names(self) -> Vec<&'static str> {
        let headings_whole = Self::HEADING.is_subset_of(self);
        let mut names = Vec::new();
        for kind in self.kinds() {
            if headings_whole && Self::HEADING.contains(kind) {
                if kind == NodeKind::H1 {
                    names.push("h");
                }
                continue;
            }
            names.push(kind.name());
        }
        names
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[test]
    fn test_all_holds_every_kind() {
        assert_eq!(KindSet::ALL.len(), NodeKind::ALL.len());
        assert!(NodeKind::ALL.iter().all(|kind| KindSet::ALL.contains(*kind)));
    }

    #[test]
    fn test_set_operations() {
        let code = KindSet::of(NodeKind::Code);
        let h1_h2 = KindSet::from_kinds([NodeKind::H1, NodeKind::H2]);
        assert!(code.intersect(h1_h2).is_empty());
        assert_eq!(code.union(h1_h2).len(), 3);
        assert_eq!(KindSet::ALL.difference(code).len(), NodeKind::ALL.len() - 1);
        assert!(h1_h2.is_subset_of(KindSet::HEADING));
        assert!(!KindSet::HEADING.is_subset_of(h1_h2));
        assert!(h1_h2.intersects(KindSet::HEADING));
    }

    #[rstest]
    #[case::all(KindSet::ALL, "markdown")]
    #[case::empty(KindSet::EMPTY, "never")]
    #[case::one(KindSet::of(NodeKind::Code), "code")]
    #[case::some(KindSet::from_kinds([NodeKind::H1, NodeKind::H2]), "h1 | h2")]
    #[case::headings(KindSet::HEADING, "h")]
    #[case::headings_and_code(KindSet::HEADING.union(KindSet::of(NodeKind::Code)), "h | code")]
    #[case::most(KindSet::ALL.difference(KindSet::of(NodeKind::Code)), "markdown - code")]
    #[case::most_without_headings(KindSet::ALL.difference(KindSet::HEADING), "markdown - h")]
    fn test_display(#[case] set: KindSet, #[case] expected: &str) {
        assert_eq!(set.display(), expected);
    }
}
