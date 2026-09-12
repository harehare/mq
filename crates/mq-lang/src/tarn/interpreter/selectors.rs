//! Applies a compiled `Selector` to a runtime value: dispatches to `mq_markdown`'s selector
//! evaluator for `Markdown` values and recurses through `Array`/`Dict` for the rest.
use crate::DictMap;
use crate::runtime::builtin;
use crate::runtime::runtime_value::RuntimeValue;
use crate::selector::Selector;
use crate::{Ident, Shared};
use std::sync::LazyLock;

fn type_ident() -> &'static Ident {
    static TYPE: LazyLock<Ident> = LazyLock::new(|| Ident::new("type"));
    &TYPE
}

/// Rare `:type` matching, kept off the hot dispatch path.
#[cold]
#[inline(never)]
pub(super) fn type_check(v: &RuntimeValue, type_str: &str) -> bool {
    match type_str {
        "string" => matches!(v, RuntimeValue::String(_)),
        "number" => matches!(v, RuntimeValue::Number(_)),
        "bool" => matches!(v, RuntimeValue::Boolean(_)),
        "array" => matches!(v, RuntimeValue::Array(_)),
        "dict" => matches!(v, RuntimeValue::Dict(_)),
        "bytes" => matches!(v, RuntimeValue::Bytes(_)),
        "markdown" => matches!(v, RuntimeValue::Markdown(_, _)),
        "function" => v.is_function(),
        "symbol" => matches!(v, RuntimeValue::Symbol(_)),
        "none" => matches!(v, RuntimeValue::None),
        _ => match v {
            RuntimeValue::Markdown(node, _) => Selector::from_selector_str(&format!(".{type_str}"))
                .filter(|selector| !selector.is_attribute_selector())
                .is_some_and(|selector| builtin::eval_selector(node, &selector) != RuntimeValue::NONE),
            _ => false,
        },
    }
}

fn eval_markdown_selector(
    node: &Shared<mq_markdown::Node>,
    selector: &Selector,
    args: Option<&[RuntimeValue]>,
) -> RuntimeValue {
    match args {
        Some(args) => builtin::eval_selector_with_args(node, selector, args),
        None => builtin::eval_selector(node, selector),
    }
}

/// Evaluates `selector` against `value`, threading `args` through to `mq_markdown` for
/// argument-taking selectors (e.g. `[1:2]`). `eval_selector_expr`/`eval_selector_expr_with_args`
/// are thin callers of this with `args` fixed to `None`/`Some`.
fn eval_selector_expr_impl(value: &RuntimeValue, selector: &Selector, args: Option<&[RuntimeValue]>) -> RuntimeValue {
    if let (Selector::Property(property_name), None) = (selector, args) {
        return eval_property_selector_expr(value, property_name);
    }
    match value {
        RuntimeValue::Markdown(node, _) => eval_markdown_selector(node, selector, args),
        RuntimeValue::Array(values) => {
            if let Selector::List(Some(idx), None) = selector {
                return values.get(*idx).cloned().unwrap_or(RuntimeValue::None);
            }
            // The previous `flat_map` implementation created a one-element `Vec` for every
            // non-array result. Large input arrays therefore performed one allocation per
            // element. Append each mapped result directly to the final buffer instead.
            let mut mapped = Vec::with_capacity(values.len());
            for value in values.iter() {
                match value {
                    RuntimeValue::Markdown(node, _) => match eval_markdown_selector(node, selector, args) {
                        RuntimeValue::Array(array) => mapped.extend(Shared::unwrap_or_clone(array)),
                        other => mapped.push(other),
                    },
                    _ if matches!(selector, Selector::List(None, None)) && args.is_none_or(<[_]>::is_empty) => {
                        mapped.push(value.clone());
                    }
                    RuntimeValue::Dict(_) => match eval_selector_expr_impl(value, selector, args) {
                        RuntimeValue::Array(array) if args.is_none() && matches!(selector, Selector::Recursive) => {
                            mapped.extend(Shared::unwrap_or_clone(array));
                        }
                        other => mapped.push(other),
                    },
                    _ => mapped.push(RuntimeValue::None),
                }
            }
            RuntimeValue::Array(Shared::new(mapped))
        }
        RuntimeValue::Dict(map) => {
            if args.is_none() && matches!(selector, Selector::List(None, None)) {
                return RuntimeValue::Array(Shared::new(map.values().cloned().collect()));
            }
            if args.is_none() && matches!(selector, Selector::Recursive) {
                return RuntimeValue::Array(Shared::new(collect_recursive(value)));
            }
            let new_map: DictMap = map
                .iter()
                .map(|(k, v)| {
                    let new_v = if k == type_ident() {
                        v.clone()
                    } else {
                        eval_selector_expr_impl(v, selector, args)
                    };
                    (*k, new_v)
                })
                .collect();
            if new_map.is_empty() {
                RuntimeValue::None
            } else {
                RuntimeValue::Dict(Shared::new(new_map))
            }
        }
        _ => RuntimeValue::None,
    }
}

pub(super) fn eval_selector_expr(value: &RuntimeValue, selector: &Selector) -> RuntimeValue {
    eval_selector_expr_impl(value, selector, None)
}

pub(super) fn eval_selector_expr_with_args(
    value: &RuntimeValue,
    selector: &Selector,
    args: &[RuntimeValue],
) -> RuntimeValue {
    eval_selector_expr_impl(value, selector, Some(args))
}

#[inline]
pub(super) fn eval_compact_selector_expr(value: &RuntimeValue, selector: Selector) -> RuntimeValue {
    match value {
        RuntimeValue::Markdown(node, _) => builtin::eval_selector(node, &selector),
        _ => eval_selector_expr(value, &selector),
    }
}

fn eval_property_selector_expr(value: &RuntimeValue, property_name: &Ident) -> RuntimeValue {
    match value {
        RuntimeValue::Array(values) => RuntimeValue::Array(Shared::new(
            values
                .iter()
                .map(|v| match v {
                    RuntimeValue::Dict(_) => eval_property_selector_expr(v, property_name),
                    _ => RuntimeValue::None,
                })
                .collect(),
        )),
        RuntimeValue::Dict(map) => map.get(property_name).cloned().unwrap_or(RuntimeValue::None),
        _ => RuntimeValue::None,
    }
}

fn collect_recursive(value: &RuntimeValue) -> Vec<RuntimeValue> {
    let mut result = Vec::new();
    collect_recursive_into(value, &mut result);
    result
}

/// Appends a pre-order recursive walk without allocating an intermediate vector per child.
///
/// Recursive selectors are often used on nested data converted from frontmatter. Building a
/// separate `Vec` for every child made the traversal allocation-heavy and repeatedly copied
/// partial results into its parent. A single output buffer preserves the selector's order while
/// making allocation scale with the complete result instead.
fn collect_recursive_into(value: &RuntimeValue, result: &mut Vec<RuntimeValue>) {
    result.push(value.clone());
    match value {
        RuntimeValue::Array(items) => {
            for item in items.iter() {
                collect_recursive_into(item, result);
            }
        }
        RuntimeValue::Dict(map) => {
            for v in map.values() {
                collect_recursive_into(v, result);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DictMap;

    #[test]
    fn list_selector_maps_an_array_without_changing_element_order() {
        let input = RuntimeValue::Array(Shared::new(vec![
            RuntimeValue::Number(1.into()),
            RuntimeValue::String(Shared::new("two".to_string())),
            RuntimeValue::None,
        ]));

        assert_eq!(
            eval_selector_expr(&input, &Selector::List(None, None)),
            input,
            "an array list selector must preserve every element in order"
        );
    }

    #[test]
    fn recursive_selector_flattens_dict_results_inside_an_array() {
        let dict = RuntimeValue::Dict(Shared::new(DictMap::from_iter([(
            Ident::new("key"),
            RuntimeValue::Number(1.into()),
        )])));
        let input = RuntimeValue::Array(Shared::new(vec![dict.clone(), RuntimeValue::Number(2.into())]));

        assert_eq!(
            eval_selector_expr(&input, &Selector::Recursive),
            RuntimeValue::Array(Shared::new(vec![
                dict,
                RuntimeValue::Number(1.into()),
                RuntimeValue::None
            ])),
            "recursive dict results must remain flattened when selected through an array"
        );
    }

    #[test]
    fn collect_recursive_preserves_preorder_for_nested_values() {
        let input = RuntimeValue::Array(Shared::new(vec![
            RuntimeValue::Number(1.into()),
            RuntimeValue::Dict(Shared::new(DictMap::from_iter([(
                Ident::new("nested"),
                RuntimeValue::Array(Shared::new(vec![RuntimeValue::Number(2.into())])),
            )]))),
        ]));

        assert_eq!(
            collect_recursive(&input),
            vec![
                input.clone(),
                RuntimeValue::Number(1.into()),
                RuntimeValue::Dict(Shared::new(DictMap::from_iter([(
                    Ident::new("nested"),
                    RuntimeValue::Array(Shared::new(vec![RuntimeValue::Number(2.into())])),
                )]))),
                RuntimeValue::Array(Shared::new(vec![RuntimeValue::Number(2.into())])),
                RuntimeValue::Number(2.into()),
            ]
        );
    }
}
