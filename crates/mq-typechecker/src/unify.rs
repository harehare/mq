//! Unification algorithm for type inference.

use crate::TypeError;
use crate::constraint::Constraint;
use crate::infer::InferenceContext;
use crate::types::{Type, TypeVarId};
use std::collections::HashSet;

/// Converts a Range to a simplified miette::SourceSpan for error reporting.
/// Note: This creates an approximate span based on line/column information.
/// For accurate byte-level spans, the original source text would be needed.
pub fn range_to_span(range: &mq_lang::Range) -> miette::SourceSpan {
    // Create an approximate offset based on line and column
    // This is a simple heuristic: assume average line length of 80 chars
    let line_offset = (range.start.line.saturating_sub(1) as usize) * 80;
    let offset = line_offset + range.start.column.saturating_sub(1);

    // Calculate approximate length based on end position
    let end_line_offset = (range.end.line.saturating_sub(1) as usize) * 80;
    let end_offset = end_line_offset + range.end.column.saturating_sub(1);
    let length = end_offset.saturating_sub(offset).max(1);

    miette::SourceSpan::new(offset.into(), length)
}

/// Solves type constraints through unification
pub fn solve_constraints(ctx: &mut InferenceContext) {
    let constraints = ctx.take_constraints();

    for constraint in constraints {
        match constraint {
            Constraint::Equal(t1, t2, range) => {
                unify(ctx, &t1, &t2, range);
            }
        }
    }
}

/// Unifies two types
pub fn unify(ctx: &mut InferenceContext, t1: &Type, t2: &Type, range: Option<mq_lang::Range>) {
    match (t1, t2) {
        // Same concrete types unify trivially
        (Type::Int, Type::Int)
        | (Type::Float, Type::Float)
        | (Type::Number, Type::Number)
        | (Type::String, Type::String)
        | (Type::Bool, Type::Bool)
        | (Type::Symbol, Type::Symbol)
        | (Type::None, Type::None)
        | (Type::Markdown, Type::Markdown) => {}

        // Type variables
        (Type::Var(v1), Type::Var(v2)) if v1 == v2 => {}

        (Type::Var(_), _) | (_, Type::Var(_)) => {
            // Resolve type variable chains iteratively before unifying
            let t1 = resolve_var_chain(ctx, t1);
            let t2 = resolve_var_chain(ctx, t2);

            // After resolution, if neither is a Var, unify the resolved types
            let (var, ty) = match (&t1, &t2) {
                (Type::Var(v1), Type::Var(v2)) if v1 == v2 => return,
                (Type::Var(var), ty) => (*var, ty),
                (ty, Type::Var(var)) => (*var, ty),
                _ => {
                    unify(ctx, &t1, &t2, range);
                    return;
                }
            };

            // Occurs check: ensure var doesn't occur in ty
            if occurs_check(var, ty) {
                // Resolve types for better error messages
                let var_ty = ctx.resolve_type(&Type::Var(var));
                let resolved_ty = ctx.resolve_type(ty);
                ctx.add_error(TypeError::OccursCheck {
                    var: var_ty.display_renumbered(),
                    ty: resolved_ty.display_renumbered(),
                    span: range.as_ref().map(range_to_span),
                    location: range.as_ref().map(|r| (r.start.line, r.start.column)),
                });
                return;
            }

            // Bind the type variable
            ctx.bind_type_var(var, ty.clone());
        }

        // Arrays
        (Type::Array(elem1), Type::Array(elem2)) => unify(ctx, elem1, elem2, range),

        // Dictionaries
        (Type::Dict(k1, v1), Type::Dict(k2, v2)) => {
            unify(ctx, k1, k2, range);
            unify(ctx, v1, v2, range);
        }

        // Functions
        (Type::Function(params1, ret1), Type::Function(params2, ret2)) => {
            if params1.len() != params2.len() {
                ctx.add_error(TypeError::WrongArity {
                    expected: params1.len(),
                    found: params2.len(),
                    span: range.as_ref().map(range_to_span),
                    location: range.as_ref().map(|r| (r.start.line, r.start.column)),
                });
                return;
            }

            // Unify parameter types
            for (p1, p2) in params1.iter().zip(params2.iter()) {
                unify(ctx, p1, p2, range);
            }

            // Unify return types
            unify(ctx, ret1, ret2, range);
        }

        // Union types: a union can unify with a type if any of its members can unify with it
        (Type::Union(types), other) | (other, Type::Union(types)) => {
            // Check if the other type matches any member of the union
            let matches_any = types.iter().any(|t| {
                // Try to check if types can match without adding errors
                match (t, other) {
                    (t1, t2) if std::mem::discriminant(t1) == std::mem::discriminant(t2) => true,
                    (Type::Var(_), _) | (_, Type::Var(_)) => true,
                    _ => false,
                }
            });

            if !matches_any {
                // No member of the union can unify with the other type - report error
                let resolved_t1 = ctx.resolve_type(t1);
                let resolved_t2 = ctx.resolve_type(t2);
                ctx.add_error(TypeError::Mismatch {
                    expected: resolved_t1.display_renumbered(),
                    found: resolved_t2.display_renumbered(),
                    span: range.as_ref().map(range_to_span),
                    location: range.as_ref().map(|r| (r.start.line, r.start.column)),
                });
            }
            // If at least one member matches, allow it (union type semantics)
        }

        // Mismatch
        _ => {
            // Resolve types for better error messages (use renumbered display for clean names)
            let resolved_t1 = ctx.resolve_type(t1);
            let resolved_t2 = ctx.resolve_type(t2);
            ctx.add_error(TypeError::Mismatch {
                expected: resolved_t1.display_renumbered(),
                found: resolved_t2.display_renumbered(),
                span: range.as_ref().map(range_to_span),
                location: range.as_ref().map(|r| (r.start.line, r.start.column)),
            });
        }
    }
}

/// Occurs check: ensures a type variable doesn't occur in a type
///
/// This prevents infinite types like T = [T]
fn occurs_check(var: TypeVarId, ty: &Type) -> bool {
    match ty {
        Type::Var(v) => var == *v,
        Type::Array(elem) => occurs_check(var, elem),
        Type::Dict(key, value) => occurs_check(var, key) || occurs_check(var, value),
        Type::Function(params, ret) => params.iter().any(|p| occurs_check(var, p)) || occurs_check(var, ret),
        Type::Union(types) => types.iter().any(|t| occurs_check(var, t)),
        _ => false,
    }
}

/// Applies substitutions to resolve all type variables.
///
/// Type variable chains are followed iteratively to avoid stack overflow.
pub fn apply_substitution(ctx: &InferenceContext, ty: &Type) -> Type {
    // Follow type variable chains iteratively
    let ty = resolve_var_chain(ctx, ty);
    match &ty {
        Type::Var(_) => ty,
        Type::Array(elem) => Type::Array(Box::new(apply_substitution(ctx, elem))),
        Type::Dict(key, value) => Type::Dict(
            Box::new(apply_substitution(ctx, key)),
            Box::new(apply_substitution(ctx, value)),
        ),
        Type::Function(params, ret) => {
            let new_params = params.iter().map(|p| apply_substitution(ctx, p)).collect();
            Type::Function(new_params, Box::new(apply_substitution(ctx, ret)))
        }
        Type::Union(types) => {
            let new_types = types.iter().map(|t| apply_substitution(ctx, t)).collect();
            Type::union(new_types)
        }
        _ => ty,
    }
}

/// Follows a type variable substitution chain iteratively until reaching
/// a non-variable type or an unbound variable.
fn resolve_var_chain(ctx: &InferenceContext, ty: &Type) -> Type {
    let mut current = ty.clone();
    loop {
        match &current {
            Type::Var(var) => {
                if let Some(bound) = ctx.get_type_var(*var) {
                    current = bound;
                } else {
                    return current;
                }
            }
            _ => return current,
        }
    }
}

/// Gets all free type variables in a type after applying current substitutions
pub fn free_vars(ctx: &InferenceContext, ty: &Type) -> HashSet<TypeVarId> {
    let resolved = apply_substitution(ctx, ty);
    let mut vars = HashSet::new();
    collect_free_vars(&resolved, &mut vars);
    vars
}

fn collect_free_vars(ty: &Type, vars: &mut HashSet<TypeVarId>) {
    match ty {
        Type::Var(var) => {
            vars.insert(*var);
        }
        Type::Array(elem) => collect_free_vars(elem, vars),
        Type::Dict(key, value) => {
            collect_free_vars(key, vars);
            collect_free_vars(value, vars);
        }
        Type::Function(params, ret) => {
            for param in params {
                collect_free_vars(param, vars);
            }
            collect_free_vars(ret, vars);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::TypeVarContext;

    #[test]
    fn test_unify_concrete_types() {
        let mut ctx = InferenceContext::new();
        unify(&mut ctx, &Type::Number, &Type::Number, None);
        assert!(ctx.take_errors().is_empty());

        unify(&mut ctx, &Type::String, &Type::Number, None);
        assert!(!ctx.take_errors().is_empty());
    }

    #[test]
    fn test_unify_type_vars() {
        let mut var_ctx = TypeVarContext::new();
        let mut ctx = InferenceContext::new();

        let var1 = var_ctx.fresh();
        let var2 = var_ctx.fresh();

        // Unify var1 with Number
        unify(&mut ctx, &Type::Var(var1), &Type::Number, None);
        assert!(ctx.take_errors().is_empty());

        // Unify var2 with var1 (should transitively become Number)
        unify(&mut ctx, &Type::Var(var2), &Type::Var(var1), None);
        assert!(ctx.take_errors().is_empty());
    }

    #[test]
    fn test_unify_arrays() {
        let mut ctx = InferenceContext::new();
        let arr1 = Type::array(Type::Number);
        let arr2 = Type::array(Type::Number);
        unify(&mut ctx, &arr1, &arr2, None);
        assert!(ctx.take_errors().is_empty());

        let arr3 = Type::array(Type::String);
        unify(&mut ctx, &arr1, &arr3, None);
        assert!(!ctx.take_errors().is_empty());
    }

    #[test]
    fn test_occurs_check() {
        let mut var_ctx = TypeVarContext::new();
        let var = var_ctx.fresh();

        // T = [T] should fail occurs check
        let recursive = Type::array(Type::Var(var));
        assert!(occurs_check(var, &recursive));

        // T = number should pass occurs check
        assert!(!occurs_check(var, &Type::Number));
    }
}
