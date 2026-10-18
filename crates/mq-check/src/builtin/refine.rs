//! Signatures of builtins that depend on the arguments, beyond what an overload says.

use crate::types::Type;

/// The signature `(args) -> ret`.
fn answer(args: &[Type], ret: Type) -> Option<Type> {
    Some(Type::function(args.to_vec(), ret))
}

/// Whether no type is still being inferred.
fn settled(types: &[&Type]) -> bool {
    types.iter().all(|ty| !ty.has_pending_var())
}

/// Whether the runtime can look for a value in it.
fn is_searchable(container: &Type) -> bool {
    matches!(
        container,
        Type::None | Type::String | Type::Bytes | Type::Array(_) | Type::Dict(..) | Type::Record(..)
    )
}

/// The signature of `name` specialised to the already resolved `args`, for builtins whose result
/// type depends on the structure of an argument rather than on a type variable.
pub(crate) fn refine_signature(name: &str, args: &[Type]) -> Option<Type> {
    match (name, args) {
        ("flatten", [arg @ (Type::Array(_) | Type::Tuple(_))]) => {
            Some(Type::function(vec![arg.clone()], Type::array(flatten_leaf(arg))))
        }
        // Setters change a node's contents but keep its kind.
        (
            "set_check" | "set_list_ordered" | "set_code_block_lang" | "set_ref" | "set_attr" | "set_children",
            [node @ Type::Node(_), rest @ ..],
        ) => Some(Type::function(
            std::iter::once(node.clone()).chain(rest.iter().cloned()).collect(),
            node.clone(),
        )),
        // Looking for a value of another type than the elements finds nothing.
        ("has" | "contains" | "in", [container, needle])
            if is_searchable(container) && settled(&[container, needle]) =>
        {
            answer(args, Type::Bool)
        }
        ("index" | "rindex", [Type::Array(element), needle]) if needle != &**element && settled(&[element, needle]) => {
            answer(args, Type::Number)
        }
        ("index" | "rindex", [Type::None, _]) => answer(args, Type::Number),
        ("bsearch", [Type::Array(_), target]) if settled(&[target]) => answer(args, Type::Number),
        ("set", [container, key, value]) => widened_dict(container, key, value).and_then(|dict| answer(args, dict)),
        // The first value that is not none, so the result is either of the two.
        ("coalesce" | "??", [first, second])
            if first != second && !matches!(first, Type::None) && settled(&[first, second]) =>
        {
            answer(args, Type::union(vec![first.clone(), second.clone()]))
        }
        // Concatenating or appending values of other element types widens the element type.
        ("+" | "add", [Type::Array(left), Type::Array(right)]) if left != right && settled(&[left, right]) => answer(
            args,
            Type::array(Type::union(vec![(**left).clone(), (**right).clone()])),
        ),
        ("+" | "add", [Type::Array(left), right])
            if !matches!(right, Type::Array(_) | Type::Var(_)) && **left != *right && settled(&[left, right]) =>
        {
            answer(args, Type::array(Type::union(vec![(**left).clone(), right.clone()])))
        }
        _ => None,
    }
}

/// The signature of `name` for arguments that no overload takes but the runtime handles anyway.
pub(crate) fn fallback_signature(name: &str, args: &[Type]) -> Option<Type> {
    if !settled(&args.iter().collect::<Vec<_>>()) {
        return None;
    }
    match (name, args) {
        ("contains" | "in", [container, _]) if is_searchable(container) => answer(args, Type::Bool),
        ("index" | "rindex", [Type::None, _]) => answer(args, Type::Number),
        ("indices", [Type::None | Type::Array(_), _]) => answer(args, Type::array(Type::Number)),
        ("bsearch", [Type::Array(_), _]) => answer(args, Type::Number),
        ("insert", [container, key, value]) => widened_dict(container, key, value).and_then(|dict| answer(args, dict)),
        _ => None,
    }
}

/// The dict that `set` or `insert` makes from a dict or a record, when the key or the value is of
/// another type than the ones it holds.
fn widened_dict(container: &Type, key: &Type, value: &Type) -> Option<Type> {
    if !settled(&[container, key, value]) {
        return None;
    }
    let (keys, values) = match container {
        Type::Dict(k, v) => ((**k).clone(), (**v).clone()),
        Type::Record(fields, _) => (Type::String, Type::union(fields.values().cloned().collect())),
        _ => return None,
    };
    Some(Type::dict(
        Type::union(vec![keys, key.clone()]),
        Type::union(vec![values, value.clone()]),
    ))
}

/// The element type left after `flatten` removes every level of array nesting.
fn flatten_leaf(ty: &Type) -> Type {
    match ty {
        Type::Array(elem) => flatten_leaf(elem),
        Type::Tuple(elems) => Type::union(elems.iter().map(flatten_leaf).collect()),
        Type::Union(members) => Type::union(members.iter().map(flatten_leaf).collect()),
        other => other.clone(),
    }
}
