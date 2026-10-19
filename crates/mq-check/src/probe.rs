//! The types a value of unknown type is tried as.

use std::collections::BTreeMap;

use crate::types::Type;

/// One type of each kind that an overload, a fallback or a refinement can tell apart.
pub(crate) fn representatives() -> Vec<Type> {
    let mut types = vec![
        Type::Number,
        Type::String,
        Type::Bool,
        Type::Symbol,
        Type::None,
        Type::Bytes,
        Type::markdown(),
        Type::array(Type::Dynamic),
        Type::dict(Type::Dynamic, Type::Dynamic),
        Type::Record(BTreeMap::new(), Box::new(Type::RowEmpty)),
        Type::Generator(Box::new(Type::Dynamic)),
        Type::Dynamic,
    ];
    types.extend((0..=3).map(|arity| Type::function(vec![Type::Dynamic; arity], Type::Dynamic)));
    types
}

/// Every way of choosing one type from each of `choices`.
pub(crate) fn combinations<'a>(choices: &'a [&'a [Type]]) -> impl Iterator<Item = Vec<Type>> + 'a {
    let total: usize = choices.iter().map(|c| c.len()).product();
    (0..total).map(move |mut n| {
        choices
            .iter()
            .map(|options| {
                let ty = options[n % options.len()].clone();
                n /= options.len();
                ty
            })
            .collect()
    })
}

/// Builtins that call a function on each element of their first argument, sorted by name.
pub(crate) const ELEMENT_BUILTINS: &[&str] = &[
    "all",
    "any",
    "count_by",
    "each",
    "filter",
    "find",
    "flat_map",
    "group_by",
    "index_by",
    "map",
    "max_by",
    "min_by",
    "partition",
    "reject",
    "sort_by",
    "sum_by",
];
