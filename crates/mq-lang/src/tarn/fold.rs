//! Compile-time evaluation of constant expressions.
//!
//! An expression folds only when every operand is a literal and the VM would produce a value
//! without error, so a fold never changes a result or hides a runtime error.

use super::bytecode::BinaryOp;
use super::interpreter::{compare_numbers, number_arithmetic};
use crate::ast::node::{self as ast, Expr, Literal, Node};
use crate::runtime::runtime_value::RuntimeValue;
use rustc_hash::FxHashSet;

/// Addresses of operator nodes already known not to fold, so a chain such as `x + 1 + 1 + ...` is
/// inspected once rather than once per link.
pub(super) type NonConst = FxHashSet<usize>;

/// The value of `node` when it is a literal or a pure operation over literals.
pub(super) fn const_value(node: &Node, non_const: &mut NonConst) -> Option<RuntimeValue> {
    let key = node as *const Node as usize;
    match &node.expr {
        Expr::Literal(literal) => return Some(literal_value(literal)),
        Expr::Paren(inner) => return const_value(inner, non_const),
        Expr::UnaryOp(..) | Expr::BinaryOp(..) | Expr::And(_) | Expr::Or(_) => {
            if non_const.contains(&key) {
                return None;
            }
        }
        _ => return None,
    }
    let value = fold_operator(node, non_const);
    if value.is_none() {
        non_const.insert(key);
    }
    value
}

fn fold_operator(node: &Node, non_const: &mut NonConst) -> Option<RuntimeValue> {
    match &node.expr {
        Expr::UnaryOp(op, operand) => fold_unary(*op, const_value(operand, non_const)?),
        Expr::BinaryOp(op, lhs, rhs) => {
            let lhs = const_value(lhs, non_const)?;
            let rhs = const_value(rhs, non_const)?;
            fold_binary(super::compiler::to_vm_binary_op(*op), &lhs, &rhs)
        }
        Expr::And(operands) => {
            let values = const_values(operands, non_const)?;
            // Mirrors the VM: a falsy operand yields `false`, otherwise the last operand.
            Some(if values.iter().all(RuntimeValue::is_truthy) {
                values.into_iter().last().unwrap_or(RuntimeValue::Boolean(true))
            } else {
                RuntimeValue::Boolean(false)
            })
        }
        Expr::Or(operands) => {
            // Mirrors the VM: the first truthy operand, otherwise `false`.
            Some(
                const_values(operands, non_const)?
                    .into_iter()
                    .find(RuntimeValue::is_truthy)
                    .unwrap_or(RuntimeValue::Boolean(false)),
            )
        }
        _ => None,
    }
}

fn const_values(operands: &[crate::Shared<Node>], non_const: &mut NonConst) -> Option<Vec<RuntimeValue>> {
    operands.iter().map(|operand| const_value(operand, non_const)).collect()
}

fn literal_value(literal: &Literal) -> RuntimeValue {
    super::compiler::literal_to_runtime_value(literal)
}

fn fold_unary(op: ast::UnaryOp, value: RuntimeValue) -> Option<RuntimeValue> {
    match (op, value) {
        (ast::UnaryOp::Not, value) => Some(RuntimeValue::Boolean(!value.is_truthy())),
        (ast::UnaryOp::Neg, RuntimeValue::Number(n)) => {
            Some(RuntimeValue::Number(crate::number::Number::new(-n.value())))
        }
        _ => None,
    }
}

fn fold_binary(op: BinaryOp, left: &RuntimeValue, right: &RuntimeValue) -> Option<RuntimeValue> {
    match (left, right) {
        (RuntimeValue::Number(l), RuntimeValue::Number(r)) => number_arithmetic(op, *l, *r)
            .map(RuntimeValue::Number)
            .or_else(|| compare_numbers(op, *l, *r).map(RuntimeValue::Boolean)),
        (RuntimeValue::String(l), RuntimeValue::String(r)) if op == BinaryOp::Add => Some(format!("{l}{r}").into()),
        _ if op.is_comparison() => Some(RuntimeValue::Boolean(match op {
            BinaryOp::Eq => left == right,
            BinaryOp::Ne => left != right,
            BinaryOp::Lt => left < right,
            BinaryOp::Le => left <= right,
            BinaryOp::Gt => left > right,
            _ => left >= right,
        })),
        _ => None,
    }
}
