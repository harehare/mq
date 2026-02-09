//! Constraint generation for type inference.

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
    hir.symbol(symbol_id).and_then(|symbol| symbol.source.text_range)
}

/// Generates type constraints from HIR
pub fn generate_constraints(hir: &Hir, ctx: &mut InferenceContext) {
    // Use a two-pass approach to ensure literals have concrete types before operators use them

    // Pass 1: Assign types to literals, variables, and simple constructs
    // This ensures base types are established first
    for (symbol_id, symbol) in hir.symbols() {
        // Skip builtin symbols to avoid type checking builtin function implementations
        if hir.is_builtin_symbol(symbol) {
            continue;
        }

        match symbol.kind {
            SymbolKind::Number
            | SymbolKind::String
            | SymbolKind::Boolean
            | SymbolKind::Symbol
            | SymbolKind::None
            | SymbolKind::Variable
            | SymbolKind::Parameter
<<<<<<< HEAD
            | SymbolKind::PatternVariable => {
=======
            | SymbolKind::PatternVariable
            | SymbolKind::Function(_) => {
>>>>>>> 02334e86 (✨ Refactor type checker to return a list of type errors instead of a Result)
                generate_symbol_constraints(hir, symbol_id, symbol.kind.clone(), ctx);
            }
            _ => {}
        }
    }

    // Pass 2: Set up piped inputs before processing operators/calls
    // Root-level symbols (parent=None, not builtin) form an implicit pipe chain
    let root_symbols: Vec<SymbolId> = hir
        .symbols()
        .filter(|(_, symbol)| symbol.parent.is_none() && !hir.is_builtin_symbol(symbol))
        .map(|(id, _)| id)
        .collect();
    for i in 1..root_symbols.len() {
        let prev_ty = ctx.get_or_create_symbol_type(root_symbols[i - 1]);
        ctx.set_piped_input(root_symbols[i], prev_ty);
    }

    // Pass 3: Process all other symbols (operators, calls, etc.) except Block
    // These can now reference the concrete types from pass 1 and piped inputs from pass 2
    for (symbol_id, symbol) in hir.symbols() {
        // Skip builtin symbols
        if hir.is_builtin_symbol(symbol) {
            continue;
        }

        // Skip if already processed in pass 1
        if ctx.get_symbol_type(symbol_id).is_some() {
            continue;
        }

        // Skip Block — handled in pass 4 after children are typed
        if matches!(symbol.kind, SymbolKind::Block) {
            continue;
        }

        generate_symbol_constraints(hir, symbol_id, symbol.kind.clone(), ctx);
    }
}

    // Pass 4: Process Block symbols (pipe chains)
    // Children are now typed, so we can thread output types through the chain
    for (symbol_id, symbol) in hir.symbols() {
        if hir.is_builtin_symbol(symbol) {
            continue;
        }
        if matches!(symbol.kind, SymbolKind::Block) {
            generate_block_constraints(hir, symbol_id, ctx);
        }
    }
}

/// Generates constraints for Block symbols (pipe chains).
///
/// In mq, `x | f(y)` means the output of `x` flows into `f` as an implicit first argument.
/// Block symbols represent pipe chains; their children are the expressions in sequence.
/// This function threads the output type of each child to the next child's piped input,
/// and sets the Block's type to its last child's type.
fn generate_block_constraints(hir: &Hir, symbol_id: SymbolId, ctx: &mut InferenceContext) {
    let children = get_children(hir, symbol_id);

    if children.is_empty() {
        let ty_var = ctx.fresh_var();
        ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
        return;
    }

    // Thread types through the pipe chain:
    // For each child after the first, set its piped_input to the previous child's type
    for i in 1..children.len() {
        let prev_ty = ctx.get_or_create_symbol_type(children[i - 1]);
        ctx.set_piped_input(children[i], prev_ty);
    }

    // The Block's type is the last child's type
    let last_ty = ctx.get_or_create_symbol_type(*children.last().unwrap());
    ctx.set_symbol_type(symbol_id, last_ty);
}

/// Generates constraints for a single symbol
fn generate_symbol_constraints(hir: &Hir, symbol_id: SymbolId, kind: SymbolKind, ctx: &mut InferenceContext) {
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
            let func_ty = Type::function(param_tys.clone(), ret_ty.clone());
            ctx.set_symbol_type(symbol_id, func_ty);

            // Bind parameter types to their parameter symbols
            let children = get_children(hir, symbol_id);
            let param_children: Vec<SymbolId> = children
                .iter()
                .filter(|&&child_id| {
                    hir.symbol(child_id)
                        .map(|s| matches!(s.kind, SymbolKind::Parameter))
                        .unwrap_or(false)
                })
                .copied()
                .collect();
            for (param_sym, param_ty) in param_children.iter().zip(param_tys.iter()) {
                let sym_ty = ctx.get_or_create_symbol_type(*param_sym);
                let range = get_symbol_range(hir, *param_sym);
                ctx.add_constraint(Constraint::Equal(sym_ty, param_ty.clone(), range));
            }

            // Connect function body's type to the return type
            // The body is the last non-parameter child
            let body_children: Vec<SymbolId> = children
                .iter()
                .filter(|&&child_id| {
                    hir.symbol(child_id)
                        .map(|s| !matches!(s.kind, SymbolKind::Parameter))
                        .unwrap_or(false)
                })
                .copied()
                .collect();
            if let Some(&last_body) = body_children.last() {
                let body_ty = ctx.get_or_create_symbol_type(last_body);
                let range = get_symbol_range(hir, symbol_id);
                ctx.add_constraint(Constraint::Equal(ret_ty, body_ty, range));
            }
        }

        // References should unify with their definition
        SymbolKind::Ref => {
            if let Some(def_id) = hir.resolve_reference_symbol(symbol_id) {
                // Check if the reference is to a builtin function
                if let Some(symbol) = hir.symbol(def_id)
                    && let Some(name) = &symbol.value
                {
                    // Check if this is a builtin function
                    let has_builtin = ctx.get_builtin_overloads(name.as_str()).is_some();
                    if has_builtin {
                        // If there's a piped input, treat this as a call with the piped value
                        if let Some(piped_ty) = ctx.get_piped_input(symbol_id).cloned() {
                            let arg_tys = vec![piped_ty];
                            if let Some(resolved_ty) = ctx.resolve_overload(name.as_str(), &arg_tys) {
                                if let Type::Function(param_tys, ret_ty) = resolved_ty {
                                    let range = get_symbol_range(hir, symbol_id);
                                    for (arg_ty, param_ty) in arg_tys.iter().zip(param_tys.iter()) {
                                        ctx.add_constraint(Constraint::Equal(arg_ty.clone(), param_ty.clone(), range));
                                    }
                                    ctx.set_symbol_type(symbol_id, ret_ty.as_ref().clone());
                                    return;
                                }
                            } else {
                                // Piped input doesn't match any overload - type error
                                let range = get_symbol_range(hir, symbol_id);
                                ctx.add_error(crate::TypeError::Mismatch {
                                    expected: format!("argument matching {} overloads", name),
                                    found: arg_tys[0].to_string(),
                                    span: range.as_ref().map(|r| {
                                        let offset = (r.start.line.saturating_sub(1) as usize) * 80
                                            + r.start.column.saturating_sub(1);
                                        miette::SourceSpan::new(offset.into(), 1)
                                    }),
                                });
                                let ty_var = ctx.fresh_var();
                                ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
                                return;
                            }
                        }

                        let overload_count = ctx.get_builtin_overloads(name.as_str()).map(|o| o.len()).unwrap_or(0);

                        // For builtin functions with overloads, we need to handle them specially
                        // For now, we'll create a fresh type variable that will be resolved
                        // during call resolution
                        let ref_ty = ctx.get_or_create_symbol_type(symbol_id);

                        // If there's only one overload, use it directly
                        if overload_count == 1 {
                            let builtin_ty = ctx.get_builtin_overloads(name.as_str()).unwrap()[0].clone();
                            let range = get_symbol_range(hir, symbol_id);
                            ctx.add_constraint(Constraint::Equal(ref_ty, builtin_ty, range));
                        }
                        // For multiple overloads, the type will be resolved at the call site
                        return;
                    }
                }

                // Normal reference resolution
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

                        // Resolve types to get their concrete values if already determined
                        let resolved_left = ctx.resolve_type(&left_ty);
                        let resolved_right = ctx.resolve_type(&right_ty);

                        // Try to resolve the best matching overload
                        let arg_types = vec![resolved_left.clone(), resolved_right.clone()];
                        if let Some(resolved_ty) = ctx.resolve_overload(op_name.as_str(), &arg_types) {
                            // resolved_ty is the matched function type: (T1, T2) -> T3
                            if let Type::Function(param_tys, ret_ty) = resolved_ty {
                                if param_tys.len() == 2 {
                                    ctx.add_constraint(Constraint::Equal(left_ty, param_tys[0].clone(), range));
                                    ctx.add_constraint(Constraint::Equal(right_ty, param_tys[1].clone(), range));
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
                            // No matching overload found - collect error and assign fresh type var
                            ctx.add_error(crate::TypeError::UnificationError {
                                left: format!("{} with arguments ({}, {})", op_name, left_ty, right_ty),
                                right: "no matching overload".to_string(),
                                span: None,
                            });
                            let ty_var = ctx.fresh_var();
                            ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
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

                        // Resolve type to get its concrete value if already determined
                        let resolved_operand = ctx.resolve_type(&operand_ty);

                        // Try to resolve the best matching overload
                        let arg_types = vec![resolved_operand.clone()];
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
                            // No matching overload found - collect error and assign fresh type var
                            ctx.add_error(crate::TypeError::UnificationError {
                                left: format!("{} with argument ({})", op_name, operand_ty),
                                right: "no matching overload".to_string(),
                                span: None,
                            });
                            let ty_var = ctx.fresh_var();
                            ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
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
            // Get the function name from the Call symbol itself
            if let Some(call_symbol) = hir.symbol(symbol_id) {
                if let Some(func_name) = &call_symbol.value {
                    // All children are arguments
                    let children = get_children(hir, symbol_id);
                    let arg_tys: Vec<Type> = children
                        .iter()
                        .map(|&arg_id| ctx.get_or_create_symbol_type(arg_id))
                        .collect();

                    // If there's a piped input, try with it prepended first
                    let piped_ty = ctx.get_piped_input(symbol_id).cloned();
                    let (effective_arg_tys, _used_piped) = if let Some(ref pt) = piped_ty {
                        let mut piped_args = vec![pt.clone()];
                        piped_args.extend(arg_tys.clone());
                        // Try piped version first
                        if ctx.resolve_overload(func_name.as_str(), &piped_args).is_some() {
                            (piped_args, true)
                        } else {
                            // Fallback to non-piped
                            (arg_tys.clone(), false)
                        }
                    } else {
                        (arg_tys.clone(), false)
                    };

                    // Try to resolve as a builtin function
                    if let Some(resolved_ty) = ctx.resolve_overload(func_name.as_str(), &effective_arg_tys) {
                        // resolved_ty is the matched function type
                        if let Type::Function(param_tys, ret_ty) = resolved_ty {
                            let range = get_symbol_range(hir, symbol_id);

                            // Add constraints for each effective argument
                            for (arg_ty, param_ty) in effective_arg_tys.iter().zip(param_tys.iter()) {
                                ctx.add_constraint(Constraint::Equal(arg_ty.clone(), param_ty.clone(), range));
                            }
                        } else {
                            // Resolved to a builtin - handle via overload resolution
                            resolve_builtin_call(ctx, symbol_id, func_name, &arg_tys, range);
                        }
                    } else {
                        // Check if it's a known builtin with wrong arguments
                        let has_builtin = ctx.get_builtin_overloads(func_name.as_str()).is_some();
                        if has_builtin {
                            // Known builtin but no matching overload - type error
                            let range = get_symbol_range(hir, symbol_id);
                            let arg_strs: Vec<String> = effective_arg_tys.iter().map(|t| t.to_string()).collect();

                            // Check if it's an arity mismatch
                            let overloads = ctx.get_builtin_overloads(func_name.as_str()).unwrap();
                            let expected_arities: Vec<usize> = overloads
                                .iter()
                                .filter_map(|o| {
                                    if let Type::Function(params, _) = o {
                                        Some(params.len())
                                    } else {
                                        None
                                    }
                                })
                                .collect();
                            let error = if !expected_arities.contains(&effective_arg_tys.len()) {
                                crate::TypeError::WrongArity {
                                    expected: expected_arities[0],
                                    found: effective_arg_tys.len(),
                                    span: range.as_ref().map(|r| {
                                        let offset = (r.start.line.saturating_sub(1) as usize) * 80
                                            + r.start.column.saturating_sub(1);
                                        miette::SourceSpan::new(offset.into(), 1)
                                    }),
                                }
                            } else {
                                crate::TypeError::Mismatch {
                                    expected: format!(
                                        "{}({})",
                                        func_name,
                                        overloads.iter().map(|o| o.to_string()).collect::<Vec<_>>().join(" | ")
                                    ),
                                    found: format!("{}({})", func_name, arg_strs.join(", ")),
                                    span: range.as_ref().map(|r| {
                                        let offset = (r.start.line.saturating_sub(1) as usize) * 80
                                            + r.start.column.saturating_sub(1);
                                        miette::SourceSpan::new(offset.into(), 1)
                                    }),
                                }
                            };
                            ctx.add_error(error);
                            // Assign fresh type var to continue processing
                            let ty_var = ctx.fresh_var();
                            ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
                        }

                        // User-defined function - try to resolve via HIR references
                        if let Some(def_id) = hir.resolve_reference_symbol(symbol_id) {
                            let func_ty = ctx.get_or_create_symbol_type(def_id);
                            let range = get_symbol_range(hir, symbol_id);

                            // Build expected function type from arguments
                            let ret_var = ctx.fresh_var();
                            let ret_ty = Type::Var(ret_var);
                            let expected_func_ty = Type::function(arg_tys.clone(), ret_ty.clone());
                            ctx.add_constraint(Constraint::Equal(func_ty, expected_func_ty, range));
                            ctx.set_symbol_type(symbol_id, ret_ty);
                        } else {
                            let ret_ty = Type::Var(ctx.fresh_var());
                            ctx.set_symbol_type(symbol_id, ret_ty);
                        }
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
                    ctx.add_constraint(Constraint::Equal(elem_ty.clone(), ty.clone(), range));
                }

                let array_ty = Type::array(elem_ty);
                ctx.set_symbol_type(symbol_id, array_ty);
            }
        }

        SymbolKind::Dict => {
            // Dict structure in HIR: Dict -> key_symbol -> value_expr
            // Direct children of Dict are the key symbols
            // Note: mq dicts are like JSON objects - values can have different types
            // So we only unify key types, not value types
            let key_symbols = get_children(hir, symbol_id);
            if key_symbols.is_empty() {
                let key_ty_var = ctx.fresh_var();
                let val_ty_var = ctx.fresh_var();
                let dict_ty = Type::dict(Type::Var(key_ty_var), Type::Var(val_ty_var));
                ctx.set_symbol_type(symbol_id, dict_ty);
            } else {
                let key_ty_var = ctx.fresh_var();
                let val_ty_var = ctx.fresh_var();
                let key_ty = Type::Var(key_ty_var);
                let range = get_symbol_range(hir, symbol_id);

                for &key_id in &key_symbols {
                    // Key symbol type - all keys should be the same type
                    let k_ty = ctx.get_or_create_symbol_type(key_id);
                    ctx.add_constraint(Constraint::Equal(key_ty.clone(), k_ty, range));

                    // Process value expressions (to assign types to them)
                    let value_children = get_children(hir, key_id);
                    for &val_id in &value_children {
                        ctx.get_or_create_symbol_type(val_id);
                    }
                }

                let dict_ty = Type::dict(key_ty, Type::Var(val_ty_var));
                ctx.set_symbol_type(symbol_id, dict_ty);
            }
        }

        // Control flow constructs
        SymbolKind::If => {
            let children = get_children(hir, symbol_id);
            if !children.is_empty() {
                let range = get_symbol_range(hir, symbol_id);

                // First child is the condition
                let cond_ty = ctx.get_or_create_symbol_type(children[0]);
                ctx.add_constraint(Constraint::Equal(cond_ty, Type::Bool, range));

                // Subsequent children are then-branch and else-branches
                // All branches should have the same type
                if children.len() > 1 {
                    let branch_ty = ctx.get_or_create_symbol_type(children[1]);
                    ctx.set_symbol_type(symbol_id, branch_ty.clone());

                    // Unify all branch types
                    for &child_id in &children[2..] {
                        let child_ty = ctx.get_or_create_symbol_type(child_id);
                        ctx.add_constraint(Constraint::Equal(branch_ty.clone(), child_ty, range));
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

        SymbolKind::While => {
            let ty_var = ctx.fresh_var();
            ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
        }

        SymbolKind::Foreach => {
            let children = get_children(hir, symbol_id);
            let range = get_symbol_range(hir, symbol_id);

            // Foreach children: loop variable, iterable expression, body expressions
            // Find the Variable child (loop variable) and the rest
            let mut loop_var_id = None;
            let mut other_children = Vec::new();
            for &child_id in &children {
                if let Some(s) = hir.symbol(child_id) {
                    if matches!(s.kind, SymbolKind::Variable) && loop_var_id.is_none() {
                        loop_var_id = Some(child_id);
                    } else {
                        other_children.push(child_id);
                    }
                }
            }

            // The iterable (second child) should be an array type
            // The loop variable gets the element type
            if let Some(var_id) = loop_var_id
                && let Some(&iterable_id) = other_children.first()
            {
                let elem_ty = ctx.get_or_create_symbol_type(var_id);
                let iterable_ty = ctx.get_or_create_symbol_type(iterable_id);
                ctx.add_constraint(Constraint::Equal(iterable_ty, Type::array(elem_ty), range));
            }

            // The result type is the body's last expression type
            if let Some(&last_body) = other_children.last() {
                let body_ty = ctx.get_or_create_symbol_type(last_body);
                ctx.set_symbol_type(symbol_id, body_ty);
            } else {
                let ty_var = ctx.fresh_var();
                ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
            }
        }

        SymbolKind::Match => {
            let children = get_children(hir, symbol_id);
            let range = get_symbol_range(hir, symbol_id);

            // First child may be the match value, then MatchArm children
            let arm_children: Vec<SymbolId> = children
                .iter()
                .filter(|&&child_id| {
                    hir.symbol(child_id)
                        .map(|s| matches!(s.kind, SymbolKind::MatchArm))
                        .unwrap_or(false)
                })
                .copied()
                .collect();

            if arm_children.is_empty() {
                let ty_var = ctx.fresh_var();
                ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
            } else {
                // All arms should have the same result type
                let result_ty = ctx.get_or_create_symbol_type(arm_children[0]);
                ctx.set_symbol_type(symbol_id, result_ty.clone());

                for &arm_id in &arm_children[1..] {
                    let arm_ty = ctx.get_or_create_symbol_type(arm_id);
                    ctx.add_constraint(Constraint::Equal(result_ty.clone(), arm_ty, range));
                }
            }
        }

        SymbolKind::Try => {
            let children = get_children(hir, symbol_id);
            let range = get_symbol_range(hir, symbol_id);

            // Try children include the try body expressions and a Catch child
            let mut try_body_children = Vec::new();
            let mut catch_id = None;
            for &child_id in &children {
                if let Some(s) = hir.symbol(child_id) {
                    if matches!(s.kind, SymbolKind::Catch) {
                        catch_id = Some(child_id);
                    } else {
                        try_body_children.push(child_id);
                    }
                }
            }

            // The result type is the try body's last expression
            if let Some(&last_try) = try_body_children.last() {
                let try_ty = ctx.get_or_create_symbol_type(last_try);
                ctx.set_symbol_type(symbol_id, try_ty.clone());

                // Unify with the catch body's type
                if let Some(catch_sym_id) = catch_id {
                    let catch_children = get_children(hir, catch_sym_id);
                    if let Some(&last_catch) = catch_children.last() {
                        let catch_ty = ctx.get_or_create_symbol_type(last_catch);
                        ctx.add_constraint(Constraint::Equal(try_ty, catch_ty, range));
                    }
                }
            } else {
                let ty_var = ctx.fresh_var();
                ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
            }
        }

        SymbolKind::Catch => {
            // Catch body type is the last child's type
            let children = get_children(hir, symbol_id);
            if let Some(&last_child) = children.last() {
                let body_ty = ctx.get_or_create_symbol_type(last_child);
                ctx.set_symbol_type(symbol_id, body_ty);
            } else {
                let ty_var = ctx.fresh_var();
                ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
            }
        }

        SymbolKind::MatchArm => {
            // MatchArm body type is the last child's type (pattern is first, body is last)
            let children = get_children(hir, symbol_id);
            if let Some(&last_child) = children.last() {
                let body_ty = ctx.get_or_create_symbol_type(last_child);
                ctx.set_symbol_type(symbol_id, body_ty);
            } else {
                let ty_var = ctx.fresh_var();
                ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
            }
        }

        // Selectors produce Markdown type
        SymbolKind::Selector => {
            ctx.set_symbol_type(symbol_id, Type::Markdown);
            // If there's a piped input, it must be Markdown
            if let Some(piped_ty) = ctx.get_piped_input(symbol_id).cloned() {
                let range = get_symbol_range(hir, symbol_id);
                ctx.add_constraint(Constraint::Equal(piped_ty, Type::Markdown, range));
            }
        }

        // Other kinds
        _ => {
            // Default: assign a fresh type variable
            let ty_var = ctx.fresh_var();
            ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
        }
    }
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
