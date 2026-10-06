//! Types of node attributes (`.depth`, `.lang`, `.url`, ...) by node kind.

use mq_markdown::AttrType;

use crate::{kind_set::KindSet, narrowing::selector_kinds, types::Type};

/// The type of attribute `name` on a node of one of `kinds`, or `None` when no kind has it.
///
/// A kind that lacks the attribute, or has it optionally, makes the result `none` too. A value of
/// unknown kind (`markdown`) is left as the plain attribute type, since the kind is not known.
pub(crate) fn node_attr_type(kinds: KindSet, name: &str) -> Option<Type> {
    let mut types = Vec::new();
    let mut maybe_none = false;
    for kind in kinds.kinds() {
        match kind.attr_spec(name) {
            Some(spec) => {
                types.push(match spec.ty {
                    AttrType::String => Type::String,
                    AttrType::Integer => Type::Number,
                    AttrType::Boolean => Type::Bool,
                    AttrType::Nodes => Type::array(Type::markdown()),
                });
                maybe_none |= spec.optional;
            }
            None => maybe_none = true,
        }
    }
    if types.is_empty() {
        return None;
    }
    if maybe_none && !kinds.is_all() {
        types.push(Type::None);
    }
    Some(Type::union(types))
}

/// What a selector yields when applied to a node of one of `kinds`.
pub(crate) enum SelectorOutput {
    Type(Type),
    /// An attribute selector for an attribute that none of the kinds has.
    MissingAttr(String),
}

pub(crate) fn node_selector_output(selector: &mq_lang::Selector, kinds: KindSet) -> SelectorOutput {
    match selector {
        mq_lang::Selector::Attr(attr) => {
            let name = attr.to_string();
            let name = name.trim_start_matches('.');
            match node_attr_type(kinds, name) {
                Some(ty) => SelectorOutput::Type(ty),
                None => SelectorOutput::MissingAttr(name.to_string()),
            }
        }
        mq_lang::Selector::Recursive => SelectorOutput::Type(Type::array(Type::markdown())),
        other => SelectorOutput::Type(match selector_kinds(other) {
            Some(selected) => {
                let both = selected.intersect(kinds);
                Type::Node(if both.is_empty() { selected } else { both })
            }
            None => Type::markdown(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use mq_markdown::NodeKind::{Code, H1, H2, Link};
    use rstest::rstest;

    use super::*;

    fn kinds(kinds: impl IntoIterator<Item = mq_markdown::NodeKind>) -> KindSet {
        KindSet::from_kinds(kinds)
    }

    #[rstest]
    #[case::heading_depth(kinds([H1, H2]), "depth", Some(Type::Number))]
    #[case::optional_attribute(kinds([Code]), "lang", Some(Type::union(vec![Type::String, Type::None])))]
    #[case::required_attribute(kinds([Code]), "fence", Some(Type::Bool))]
    #[case::some_kinds_lack_it(kinds([H1, Code]), "depth", Some(Type::union(vec![Type::Number, Type::None])))]
    #[case::no_kind_has_it(kinds([Code]), "depth", None)]
    #[case::children(kinds([Link]), "children", Some(Type::array(Type::markdown())))]
    #[case::unknown_kind_is_not_made_optional(KindSet::ALL, "lang", Some(Type::String))]
    #[case::unknown_kind_still_needs_a_holder(KindSet::ALL, "nonexistent", None)]
    fn test_node_attr_type(#[case] set: KindSet, #[case] name: &str, #[case] expected: Option<Type>) {
        assert_eq!(node_attr_type(set, name), expected);
    }
}
