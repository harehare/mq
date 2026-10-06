//! Algebraic laws of the type representation: normalization of unions, joins, subtraction and
//! the node kind sets.

use std::collections::BTreeMap;

use mq_check::{kind_set::KindSet, types::Type};
use mq_markdown::NodeKind;
use proptest::prelude::*;

fn kind_set_of(min: usize) -> impl Strategy<Value = KindSet> {
    proptest::collection::vec(0..NodeKind::ALL.len(), min..8)
        .prop_map(|indices| KindSet::from_kinds(indices.into_iter().map(|i| NodeKind::ALL[i])))
}

fn kind_set() -> impl Strategy<Value = KindSet> {
    kind_set_of(0)
}

/// Types without variables, `Dynamic` or `Never`, so that equality is structural.
fn ground_type() -> impl Strategy<Value = Type> {
    let leaf = prop_oneof![
        Just(Type::Number),
        Just(Type::String),
        Just(Type::Bool),
        Just(Type::Symbol),
        Just(Type::None),
        Just(Type::Bytes),
        kind_set_of(1).prop_map(Type::Node),
    ];
    leaf.prop_recursive(3, 24, 4, |inner| {
        prop_oneof![
            inner.clone().prop_map(Type::array),
            inner.clone().prop_map(|t| Type::Generator(Box::new(t))),
            (inner.clone(), inner.clone()).prop_map(|(k, v)| Type::dict(k, v)),
            proptest::collection::vec(inner.clone(), 0..3).prop_map(Type::tuple),
            proptest::collection::btree_map("[a-c]", inner.clone(), 0..3)
                .prop_map(|fields: BTreeMap<String, Type>| Type::record(fields, Type::RowEmpty)),
            proptest::collection::vec(inner, 2..4).prop_map(Type::union),
        ]
    })
}

fn members(ty: &Type) -> Vec<Type> {
    match ty {
        Type::Union(members) => members.clone(),
        other => vec![other.clone()],
    }
}

/// The members of a type as a sorted list, so that the order of a union does not matter.
fn member_set(ty: &Type) -> Vec<String> {
    let mut set: Vec<String> = members(ty).iter().map(|m| format!("{m:?}")).collect();
    set.sort();
    set
}

proptest! {
    #[test]
    fn kind_set_union_is_commutative_and_associative(a in kind_set(), b in kind_set(), c in kind_set()) {
        prop_assert_eq!(a.union(b), b.union(a));
        prop_assert_eq!(a.union(b).union(c), a.union(b.union(c)));
    }

    #[test]
    fn kind_set_intersect_and_difference_laws(a in kind_set(), b in kind_set()) {
        prop_assert_eq!(a.intersect(b), b.intersect(a));
        prop_assert!(a.intersect(b).is_subset_of(a));
        prop_assert!(a.is_subset_of(a.union(b)));
        prop_assert_eq!(a.difference(b).union(a.intersect(b)), a);
        prop_assert!(!a.difference(b).intersects(b));
        prop_assert_eq!(a.intersects(b), !a.intersect(b).is_empty());
        prop_assert!(a.is_subset_of(KindSet::ALL));
        prop_assert_eq!(a.is_subset_of(b) && b.is_subset_of(a), a == b);
    }

    #[test]
    fn kind_set_len_matches_kinds(a in kind_set()) {
        prop_assert_eq!(a.len(), a.kinds().count());
        prop_assert_eq!(KindSet::from_kinds(a.kinds()), a);
    }

    #[test]
    fn union_is_idempotent(types in proptest::collection::vec(ground_type(), 0..6)) {
        let once = Type::union(types);
        prop_assert_eq!(Type::union(members(&once)), once);
    }

    #[test]
    fn union_does_not_depend_on_order(types in proptest::collection::vec(ground_type(), 1..6)) {
        let mut reversed = types.clone();
        reversed.reverse();
        prop_assert_eq!(member_set(&Type::union(types)), member_set(&Type::union(reversed)));
    }

    #[test]
    fn union_flattens(a in proptest::collection::vec(ground_type(), 1..4), b in proptest::collection::vec(ground_type(), 1..4)) {
        let nested = Type::union(vec![Type::union(a.clone()), Type::union(b.clone())]);
        let flat = Type::union(a.into_iter().chain(b).collect());
        prop_assert_eq!(member_set(&nested), member_set(&flat));
    }

    #[test]
    fn union_has_no_nested_union_or_duplicate(types in proptest::collection::vec(ground_type(), 0..6)) {
        let ty = Type::union(types);
        let list = members(&ty);
        prop_assert!(list.iter().all(|m| !m.is_union()));
        let set = member_set(&ty);
        let mut deduped = set.clone();
        deduped.dedup();
        prop_assert_eq!(set, deduped);
    }

    #[test]
    fn join_is_idempotent(types in proptest::collection::vec(ground_type(), 1..5)) {
        let joined = Type::join(types);
        prop_assert_eq!(member_set(&Type::join(members(&joined))), member_set(&joined));
    }

    #[test]
    fn join_covers_every_input_kind(types in proptest::collection::vec(ground_type(), 1..5)) {
        let joined = Type::join(types.clone());
        for ty in types.iter().flat_map(members) {
            prop_assert!(
                members(&joined)
                    .iter()
                    .any(|m| std::mem::discriminant(m) == std::mem::discriminant(&ty)),
                "{joined:?} does not cover {ty:?}"
            );
        }
    }

    #[test]
    fn subtract_removes_the_excluded_kind(types in proptest::collection::vec(ground_type(), 2..6), exclude in ground_type()) {
        let ty = Type::union(types);
        let rest = ty.subtract(&exclude);
        if ty.is_union() && !matches!(exclude, Type::Node(_)) {
            prop_assert!(members(&rest).iter().all(|m| m.is_never()
                || std::mem::discriminant(m) != std::mem::discriminant(&exclude)));
        }
        for m in members(&rest) {
            let from_original = members(&ty).iter().any(|orig| match (orig, &m) {
                (Type::Node(a), Type::Node(b)) => b.is_subset_of(*a),
                _ => orig == &m,
            });
            prop_assert!(m.is_never() || from_original);
        }
    }

    #[test]
    fn subtracting_the_whole_node_set_leaves_the_rest(a in kind_set_of(1)) {
        let ty = Type::union(vec![Type::Node(a), Type::Number]);
        prop_assert_eq!(ty.subtract(&Type::Node(a)), Type::Number);
    }

    #[test]
    fn a_type_matches_itself(ty in ground_type()) {
        prop_assert!(ty.can_match(&ty));
        prop_assert!(ty.match_score(&ty).is_some());
    }
}
