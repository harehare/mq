//! Constraint generation for type inference.

use crate::Result;
use crate::infer::InferenceContext;
use crate::types::{Type, TypeVarId};
use mq_hir::{Hir, SymbolId, SymbolKind};
use std::fmt;

/// Type constraint for unification
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Constraint {
    /// Two types must be equal
    Equal(Type, Type, Option<mq_lang::Range>),
    /// A type must be an instance of a type scheme
    Instance(Type, TypeVarId, Option<mq_lang::Range>),
}

impl fmt::Display for Constraint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Constraint::Equal(t1, t2, _) => write!(f, "{} ~ {}", t1, t2),
            Constraint::Instance(t, var, _) => write!(f, "{} :: '{:?}", t, var),
        }
    }
}

/// Helper function to get children of a symbol
fn get_children(hir: &Hir, parent_id: SymbolId) -> Vec<SymbolId> {
    hir.symbols()
        .filter_map(|(id, symbol)| {
            if symbol.parent == Some(parent_id) {
                Some(id)
            } else {
                None
            }
        })
        .collect()
}

/// Helper function to get the range of a symbol
fn get_symbol_range(hir: &Hir, symbol_id: SymbolId) -> Option<mq_lang::Range> {
    hir.symbol(symbol_id)
        .and_then(|symbol| symbol.source.text_range.clone())
}

/// Generates type constraints from HIR
pub fn generate_constraints(hir: &Hir, ctx: &mut InferenceContext) -> Result<()> {
    // Use a two-pass approach to ensure literals have concrete types before operators use them

    // Pass 1: Assign types to literals, variables, and simple constructs
    // This ensures base types are established first
    for (symbol_id, symbol) in hir.symbols() {
        match symbol.kind {
            SymbolKind::Number
            | SymbolKind::String
            | SymbolKind::Boolean
            | SymbolKind::Symbol
            | SymbolKind::None
            | SymbolKind::Variable
            | SymbolKind::Parameter
            | SymbolKind::PatternVariable => {
                generate_symbol_constraints(hir, symbol_id, symbol.kind.clone(), ctx)?;
            }
            _ => {}
        }
    }

    // Pass 2: Process all other symbols (operators, calls, etc.)
    // These can now reference the concrete types from pass 1
    for (symbol_id, symbol) in hir.symbols() {
        // Skip if already processed in pass 1
        if ctx.get_symbol_type(symbol_id).is_some() {
            continue;
        }
        generate_symbol_constraints(hir, symbol_id, symbol.kind.clone(), ctx)?;
    }

    Ok(())
}

/// Generates constraints for a single symbol
fn generate_symbol_constraints(
    hir: &Hir,
    symbol_id: SymbolId,
    kind: SymbolKind,
    ctx: &mut InferenceContext,
) -> Result<()> {
    match kind {
        // Literals have concrete types
        SymbolKind::Number => {
            let ty = Type::Number;
            ctx.set_symbol_type(symbol_id, ty);
        }
        SymbolKind::String => {
            let ty = Type::String;
            ctx.set_symbol_type(symbol_id, ty);
        }
        SymbolKind::Boolean => {
            let ty = Type::Bool;
            ctx.set_symbol_type(symbol_id, ty);
        }
        SymbolKind::Symbol => {
            let ty = Type::Symbol;
            ctx.set_symbol_type(symbol_id, ty);
        }
        SymbolKind::None => {
            let ty = Type::None;
            ctx.set_symbol_type(symbol_id, ty);
        }

        // Variables and parameters get fresh type variables
        SymbolKind::Variable | SymbolKind::Parameter | SymbolKind::PatternVariable => {
            let ty_var = ctx.fresh_var();
            ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
        }

        // Function definitions
        SymbolKind::Function(params) => {
            // Create type variables for each parameter
            let param_tys: Vec<Type> = params.iter().map(|_| Type::Var(ctx.fresh_var())).collect();

            // Create type variable for return type
            let ret_ty = Type::Var(ctx.fresh_var());

            // Function type is (param_tys) -> ret_ty
            let func_ty = Type::function(param_tys, ret_ty);
            ctx.set_symbol_type(symbol_id, func_ty);
        }

        // References should unify with their definition
        SymbolKind::Ref => {
            if let Some(def_id) = hir.resolve_reference_symbol(symbol_id) {
                let ref_ty = ctx.get_or_create_symbol_type(symbol_id);
                let def_ty = ctx.get_or_create_symbol_type(def_id);
                let range = get_symbol_range(hir, symbol_id);
                ctx.add_constraint(Constraint::Equal(ref_ty, def_ty, range));
            }
        }

        // Binary operators
        SymbolKind::BinaryOp => {
            if let Some(symbol) = hir.symbol(symbol_id) {
                if let Some(op_name) = &symbol.value {
                    // Get left and right operands
                    let children = get_children(hir, symbol_id);
                    if children.len() >= 2 {
                        let left_ty = ctx.get_or_create_symbol_type(children[0]);
                        let right_ty = ctx.get_or_create_symbol_type(children[1]);
                        let range = get_symbol_range(hir, symbol_id);

                        // Try to resolve the best matching overload
                        let arg_types = vec![left_ty.clone(), right_ty.clone()];
                        if let Some(resolved_ty) = ctx.resolve_overload(op_name.as_str(), &arg_types) {
                            // resolved_ty is the matched function type: (T1, T2) -> T3
                            if let Type::Function(param_tys, ret_ty) = resolved_ty {
                                if param_tys.len() == 2 {
                                    ctx.add_constraint(Constraint::Equal(left_ty, param_tys[0].clone(), range.clone()));
                                    ctx.add_constraint(Constraint::Equal(
                                        right_ty,
                                        param_tys[1].clone(),
                                        range.clone(),
                                    ));
                                    ctx.set_symbol_type(symbol_id, *ret_ty);
                                } else {
                                    // Fallback: just create a fresh type variable
                                    let ty_var = ctx.fresh_var();
                                    ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
                                }
                            } else {
                                // Fallback: just create a fresh type variable
                                let ty_var = ctx.fresh_var();
                                ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
                            }
                        } else {
                            // No matching overload found - return error
                            return Err(crate::TypeError::UnificationError {
                                left: format!("{} with arguments ({}, {})", op_name, left_ty, right_ty),
                                right: "no matching overload".to_string(),
                                span: None,
                            });
                        }
                    } else {
                        // Not enough operands
                        let ty_var = ctx.fresh_var();
                        ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
                    }
                } else {
                    // No operator name
                    let ty_var = ctx.fresh_var();
                    ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
                }
            }
        }

        // Unary operators
        SymbolKind::UnaryOp => {
            if let Some(symbol) = hir.symbol(symbol_id) {
                if let Some(op_name) = &symbol.value {
                    let children = get_children(hir, symbol_id);
                    if !children.is_empty() {
                        let operand_ty = ctx.get_or_create_symbol_type(children[0]);
                        let range = get_symbol_range(hir, symbol_id);

                        // Try to resolve the best matching overload
                        let arg_types = vec![operand_ty.clone()];
                        if let Some(resolved_ty) = ctx.resolve_overload(op_name.as_str(), &arg_types) {
                            if let Type::Function(param_tys, ret_ty) = resolved_ty {
                                if param_tys.len() == 1 {
                                    ctx.add_constraint(Constraint::Equal(operand_ty, param_tys[0].clone(), range));
                                    ctx.set_symbol_type(symbol_id, *ret_ty);
                                } else {
                                    let ty_var = ctx.fresh_var();
                                    ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
                                }
                            } else {
                                let ty_var = ctx.fresh_var();
                                ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
                            }
                        } else {
                            // No matching overload found - return error
                            return Err(crate::TypeError::UnificationError {
                                left: format!("{} with argument ({})", op_name, operand_ty),
                                right: "no matching overload".to_string(),
                                span: None,
                            });
                        }
                    } else {
                        let ty_var = ctx.fresh_var();
                        ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
                    }
                } else {
                    let ty_var = ctx.fresh_var();
                    ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
                }
            }
        }

        // Function calls
        SymbolKind::Call => {
            let children = get_children(hir, symbol_id);
            if !children.is_empty() {
                // First child is the function being called
                let func_ty = ctx.get_or_create_symbol_type(children[0]);

                // Rest are arguments
                let arg_tys: Vec<Type> = children[1..]
                    .iter()
                    .map(|&arg_id| ctx.get_or_create_symbol_type(arg_id))
                    .collect();

                // Create result type
                let ret_ty = Type::Var(ctx.fresh_var());

                // Function should have type (arg_tys) -> ret_ty
                let expected_func_ty = Type::function(arg_tys, ret_ty.clone());

                // Unify function type
                let range = get_symbol_range(hir, symbol_id);
                ctx.add_constraint(Constraint::Equal(func_ty, expected_func_ty, range));

                // Call result has type ret_ty
                ctx.set_symbol_type(symbol_id, ret_ty);
            } else {
                let ty_var = ctx.fresh_var();
                ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
            }
        }

        // Collections
        SymbolKind::Array => {
            // Array elements should have consistent types
            let children = get_children(hir, symbol_id);
            if children.is_empty() {
                // Empty array - element type is a fresh type variable
                let elem_ty_var = ctx.fresh_var();
                let array_ty = Type::array(Type::Var(elem_ty_var));
                ctx.set_symbol_type(symbol_id, array_ty);
            } else {
                // Get types of all elements
                let elem_tys: Vec<Type> = children
                    .iter()
                    .map(|&child_id| ctx.get_or_create_symbol_type(child_id))
                    .collect();

                // All elements should have the same type
                let elem_ty = elem_tys[0].clone();
                let range = get_symbol_range(hir, symbol_id);
                for ty in &elem_tys[1..] {
                    ctx.add_constraint(Constraint::Equal(elem_ty.clone(), ty.clone(), range.clone()));
                }

                let array_ty = Type::array(elem_ty);
                ctx.set_symbol_type(symbol_id, array_ty);
            }
        }

        SymbolKind::Dict => {
            // Dict keys and values should have consistent types
            // For now, use fresh type variables
            // TODO: Process dict entries properly
            let key_ty_var = ctx.fresh_var();
            let val_ty_var = ctx.fresh_var();
            let dict_ty = Type::dict(Type::Var(key_ty_var), Type::Var(val_ty_var));
            ctx.set_symbol_type(symbol_id, dict_ty);
        }

        // Control flow constructs
        SymbolKind::If => {
            let children = get_children(hir, symbol_id);
            if !children.is_empty() {
                let range = get_symbol_range(hir, symbol_id);

                // First child is the condition
                let cond_ty = ctx.get_or_create_symbol_type(children[0]);
                ctx.add_constraint(Constraint::Equal(cond_ty, Type::Bool, range.clone()));

                // Subsequent children are then-branch and else-branches
                // All branches should have the same type
                if children.len() > 1 {
                    let branch_ty = ctx.get_or_create_symbol_type(children[1]);
                    ctx.set_symbol_type(symbol_id, branch_ty.clone());

                    // Unify all branch types
                    for &child_id in &children[2..] {
                        let child_ty = ctx.get_or_create_symbol_type(child_id);
                        ctx.add_constraint(Constraint::Equal(branch_ty.clone(), child_ty, range.clone()));
                    }
                } else {
                    let ty_var = ctx.fresh_var();
                    ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
                }
            } else {
                let ty_var = ctx.fresh_var();
                ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
            }
        }

        SymbolKind::Elif | SymbolKind::Else => {
            // These are handled by their parent If node
            let ty_var = ctx.fresh_var();
            ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
        }

        SymbolKind::While | SymbolKind::Until => {
            // Loop condition must be bool
            // Loop body type doesn't matter much
            let ty_var = ctx.fresh_var();
            ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
        }

        SymbolKind::Foreach => {
            // TODO: Iterator type checking
            let ty_var = ctx.fresh_var();
            ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
        }

        SymbolKind::Match => {
            // All match arms should have the same type
            let ty_var = ctx.fresh_var();
            ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
        }

        SymbolKind::Try => {
            // Try expression type
            let ty_var = ctx.fresh_var();
            ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
        }

        // Other kinds
        _ => {
            // Default: assign a fresh type variable
            let ty_var = ctx.fresh_var();
            ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constraint_display() {
        let c = Constraint::Equal(Type::Number, Type::String, None);
        assert_eq!(c.to_string(), "number ~ string");
    }
}
