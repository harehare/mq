//! Applies a compiled `Selector` to a runtime value: dispatches to `mq_markdown`'s selector
//! evaluator for `Markdown` values and recurses through `Array`/`Dict` for the rest.
use crate::runtime::builtin;
use crate::runtime::runtime_value::RuntimeValue;
use crate::selector::Selector;
use crate::{Ident, Shared};
use std::collections::BTreeMap;
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
        "function" => matches!(v, RuntimeValue::Function(_)),
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
            let values = values
                .iter()
                .flat_map(|v| match v {
                    RuntimeValue::Markdown(node, _) => match eval_markdown_selector(node, selector, args) {
                        RuntimeValue::Array(arr) => Shared::unwrap_or_clone(arr),
                        other => vec![other],
                    },
                    _ if matches!(selector, Selector::List(None, None)) && args.is_none_or(<[_]>::is_empty) => {
                        vec![v.clone()]
                    }
                    RuntimeValue::Dict(_) => match eval_selector_expr_impl(v, selector, args) {
                        RuntimeValue::Array(arr) if args.is_none() && matches!(selector, Selector::Recursive) => {
                            Shared::unwrap_or_clone(arr)
                        }
                        other => vec![other],
                    },
                    _ => vec![RuntimeValue::None],
                })
                .collect::<Vec<_>>();
            RuntimeValue::Array(Shared::new(values))
        }
        RuntimeValue::Dict(map) => {
            if args.is_none() && matches!(selector, Selector::List(None, None)) {
                return RuntimeValue::Array(Shared::new(map.values().cloned().collect()));
            }
            if args.is_none() && matches!(selector, Selector::Recursive) {
                return RuntimeValue::Array(Shared::new(collect_recursive(value)));
            }
            let new_map: BTreeMap<_, _> = map
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
    let mut result = vec![value.clone()];
    match value {
        RuntimeValue::Array(items) => {
            for item in items.iter() {
                result.extend(collect_recursive(item));
            }
        }
        RuntimeValue::Dict(map) => {
            for v in map.values() {
                result.extend(collect_recursive(v));
            }
        }
        _ => {}
    }
    result
}
