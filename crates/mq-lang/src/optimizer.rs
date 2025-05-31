use std::rc::Rc;

use compact_str::CompactString;
use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet};
use smallvec::SmallVec;

use crate::{Program, ast::IdentName, ast::node::Args}; // Corrected import for IdentName

use super::ast::node as ast;

/// The `Optimizer` is responsible for applying various optimization techniques
/// to an mq program's AST. These techniques include constant folding,
/// constant propagation, and dead code elimination for `let` bindings.
#[derive(Debug, Default)]
pub struct Optimizer {
    /// A table to store identified constants (`let` bindings with literal values)
    /// for constant propagation. The key is the identifier of the constant,
    /// and the value is the expression (literal) it's bound to.
    constant_table: FxHashMap<ast::Ident, Rc<ast::Expr>>,
    // Note: `used_identifiers` is collected per `optimize` call and not stored
    // globally in the struct, as it's specific to each optimization pass.
}

impl Optimizer {
    /// Creates a new `Optimizer` instance with an empty constant table.
    pub fn new() -> Self {
        Self {
            constant_table: FxHashMap::with_capacity_and_hasher(100, FxBuildHasher),
        }
    }

    fn collect_used_identifiers_in_node(
        node: &Rc<ast::Node>,
        used_idents: &mut FxHashSet<IdentName>,
        // TODO: Add handling for scopes if necessary for more complex DCE.
        // For now, this collects all idents used in any readable position.
        // Def/Fn parameter names and Let binding names are definitions, not uses here.
    ) {
        // Recursively traverses the AST node to find all identifiers used in expressions.
        // This information is crucial for Dead Code Elimination (DCE) to determine
        // if a `let`-bound variable can be safely removed.
        match &*node.expr {
            ast::Expr::Ident(ident) => {
                // An identifier is a use of a variable.
                used_idents.insert(ident.name.clone());
            }
            ast::Expr::Call(func_ident, args, _) => {
                // The function name itself is treated as a used identifier.
                // If it's a user-defined function, this ensures it's not eliminated if bound by a `let`.
                used_idents.insert(func_ident.name.clone());
                // Recursively collect used identifiers from all arguments.
                for arg in args {
                    Self::collect_used_identifiers_in_node(arg, used_idents);
                }
            }
            ast::Expr::Let(_ident, value_node) => {
                // The `_ident` in `let _ident = ...` is a definition, not a use in this context.
                // Collect used identifiers from the expression assigned to the variable.
                Self::collect_used_identifiers_in_node(value_node, used_idents);
            }
            ast::Expr::Def(_ident, _params, program_nodes) => {
                // `_ident` (function name) and `_params` are definitions within the function's scope.
                // Collect used identifiers from the function's body.
                for stmt in program_nodes {
                    Self::collect_used_identifiers_in_node(stmt, used_idents);
                }
            }
            ast::Expr::Fn(_params, program_nodes) => {
                // `_params` are definitions for the anonymous function's scope.
                // Collect used identifiers from the function's body.
                for stmt in program_nodes {
                    Self::collect_used_identifiers_in_node(stmt, used_idents);
                }
            }
            ast::Expr::If(conditions) => {
                // Collect used identifiers from all condition expressions and all body expressions.
                for (cond_node_opt, body_node) in conditions {
                    if let Some(cond_node) = cond_node_opt {
                        Self::collect_used_identifiers_in_node(cond_node, used_idents);
                    }
                    Self::collect_used_identifiers_in_node(body_node, used_idents);
                }
            }
            ast::Expr::While(cond_node, program_nodes)
            | ast::Expr::Until(cond_node, program_nodes) => {
                // Collect used identifiers from the condition and all statements in the loop body.
                Self::collect_used_identifiers_in_node(cond_node, used_idents);
                for stmt in program_nodes {
                    Self::collect_used_identifiers_in_node(stmt, used_idents);
                }
            }
            ast::Expr::Foreach(_item_ident, collection_node, program_nodes) => {
                // `_item_ident` is a definition for the loop's scope.
                // Collect used identifiers from the collection being iterated over
                // and from all statements in the loop body.
                Self::collect_used_identifiers_in_node(collection_node, used_idents);
                for stmt in program_nodes {
                    Self::collect_used_identifiers_in_node(stmt, used_idents);
                }
            }
            ast::Expr::InterpolatedString(segments) => {
                // Collect used identifiers from any identifier segments within an interpolated string.
                for segment in segments {
                    if let ast::StringSegment::Ident(ident) = segment {
                        used_idents.insert(ident.name.clone());
                    }
                }
            }
            ast::Expr::Include(ast::Literal::String(_path_str)) => {
                // `include` with a string literal path does not directly use program identifiers.
                // If paths could be dynamic (e.g., `include some_var`), `some_var` would be a use.
            }
            // Literal, Selector, Nodes, Self_ expressions generally do not contain user-defined identifiers
            // that represent uses of `let`-bound variables in the way this collection pass is concerned.
            ast::Expr::Literal(_)
            | ast::Expr::Selector(_)
            | ast::Expr::Nodes
            | ast::Expr::Self_
            | ast::Expr::Include(_) => { // Handles other literal types for Include if they were possible.
                // No identifiers to collect from these expression types directly.
            }
        }
    }

    /// Collects all unique identifiers used throughout a program.
    fn collect_used_identifiers(&self, program: &Program) -> FxHashSet<IdentName> {
        let mut used_idents = FxHashSet::default();
        for node in program {
            Self::collect_used_identifiers_in_node(node, &mut used_idents);
        }
        used_idents
    }

    /// Optimizes a given program.
    ///
    /// The optimization process involves two main passes:
    /// 1. **Dead Code Elimination (DCE) Pre-pass (Identifier Collection):**
    ///    - Traverses the entire AST to collect all identifiers that are actually used
    ///      (read from). This does not yet modify the AST.
    /// 2. **Optimization and DCE Pass:**
    ///    - Iterates through the program nodes again.
    ///    - For `let` bindings:
    ///        - If the bound variable was not found in the `used_identifiers` set from Pass 1,
    ///          the `let` binding is considered dead code and is removed from the program.
    ///          If this variable was also a candidate for constant propagation (i.e., its value
    ///          was a literal and it was in `constant_table`), it's removed from `constant_table`
    ///          to prevent incorrect propagation if the same variable name is reused later.
    ///        - If the bound variable *is* used, the `let` binding is processed by `optimize_node`.
    ///          `optimize_node` will perform constant folding on its value and, if the value
    ///          becomes a literal, add it to `constant_table` for potential propagation.
    ///    - For other expression types:
    ///        - They are processed by `optimize_node` which performs constant folding on expressions
    ///          (e.g., `add(1,2)` becomes `3`) and constant propagation (replacing used identifiers
    ///          with their literal values if they are in `constant_table`).
    ///
    /// This two-pass approach ensures that DCE for `let` bindings is based on actual usage
    /// across the entire program scope before attempting to inline or fold constants related to them.
    pub fn optimize(&mut self, program: &Program) -> Program {
        // Pass 1: Collect all used identifiers to inform Dead Code Elimination for `let` bindings.
        let used_identifiers = self.collect_used_identifiers(program);

        // Pass 2: Optimize nodes, perform constant folding/propagation, and eliminate unused `Let` bindings.
        let mut optimized_program = Vec::new();
        for node in program.iter() {
            match &*node.expr {
                ast::Expr::Let(ident, _value) => {
                    // Dead Code Elimination for `let` bindings.
                    if used_identifiers.contains(&ident.name) {
                        // If the variable is used, optimize the `Let` node.
                        // `optimize_node` will handle potential constant folding of `_value`
                        // and add `ident` to `constant_table` if `_value` folds to a literal.
                        optimized_program.push(self.optimize_node(Rc::clone(node)));
                    } else {
                        // If the variable is not used, it's dead code.
                        // Remove it from `constant_table` if it was previously added as a constant.
                        // This is important if a `let` binding that was initially a constant
                        // becomes unused due to other optimizations or simply isn't referenced.
                        if self.constant_table.contains_key(ident) {
                            self.constant_table.remove(ident);
                        }
                        // The `Let` node itself is not added to `optimized_program`, effectively removing it.
                        // Optionally, log this removal for debugging:
                        // println!("Optimizer: Removed unused variable '{}'", ident.name);
                    }
                }
                _ => {
                    // For all other expression types, optimize them directly.
                    // This will handle constant folding and propagation for these nodes.
                    optimized_program.push(self.optimize_node(Rc::clone(node)));
                }
            }
        }
        optimized_program
    }

    /// Optimizes a single AST node.
    /// This function recursively optimizes child nodes and applies transformations like
    /// constant folding and constant propagation.
    ///
    /// For `let` bindings that are determined to be *used* (decision made in `optimize` method):
    /// - The value expression of the `let` binding is optimized.
    /// - If the optimized value is a literal, the variable and its literal value are
    ///   added to the `constant_table` for potential propagation into other expressions.
    ///
    /// For `Ident` expressions:
    /// - If the identifier is found in the `constant_table`, it's replaced with its literal value (constant propagation).
    ///
    /// For `Call` expressions:
    /// - Basic constant folding is applied for arithmetic operations if arguments are literals.
    fn optimize_node(&mut self, node: Rc<ast::Node>) -> Rc<ast::Node> {
        match &*node.expr {
            ast::Expr::Call(ident, args, optional) => {
                // Recursively optimize arguments first.
                let optimized_args: Args = args
                    .iter()
                    .map(|arg| self.optimize_node(Rc::clone(arg)))
                    .collect::<SmallVec<_>>();

                // Constant folding for specific known functions (e.g., arithmetic).
                // Example: add(1, 2) -> 3
                if ident.name == CompactString::new("add") {
                    match optimized_args.as_slice() {
                        [arg1, arg2] => {
                            // Check if both arguments are number literals.
                            if let (
                                ast::Expr::Literal(ast::Literal::Number(a)),
                                ast::Expr::Literal(ast::Literal::Number(b)),
                            ) = (&*arg1.expr, &*arg2.expr)
                            {
                                // Fold: replace call with the resulting literal number.
                                return Rc::new(ast::Node {
                                    token_id: node.token_id,
                                    expr: Rc::new(ast::Expr::Literal(ast::Literal::Number(*a + *b))),
                                });
                            } else if let ( // Check if both arguments are string literals.
                                ast::Expr::Literal(ast::Literal::String(a)),
                                ast::Expr::Literal(ast::Literal::String(b)),
                            ) = (&*arg1.expr, &*arg2.expr)
                            {
                                // Fold: replace call with the concatenated string literal.
                                return Rc::new(ast::Node {
                                    token_id: node.token_id,
                                    expr: Rc::new(ast::Expr::Literal(ast::Literal::String(
                                        format!("{}{}", a, b),
                                    ))),
                                });
                            }
                            // If not foldable, return the call with optimized args.
                        }
                        _ => { /* Not a binary add call, or args not literals, do nothing specific here */ }
                    }
                } else if ident.name == CompactString::new("sub") { // Similar folding for 'sub'
                    if let [arg1, arg2] = optimized_args.as_slice() {
                        if let (
                            ast::Expr::Literal(ast::Literal::Number(a)),
                            ast::Expr::Literal(ast::Literal::Number(b)),
                        ) = (&*arg1.expr, &*arg2.expr)
                        {
                            return Rc::new(ast::Node {
                                token_id: node.token_id,
                                expr: Rc::new(ast::Expr::Literal(ast::Literal::Number(*a - *b))),
                            });
                        }
                    }
                } else if ident.name == CompactString::new("div") { // Similar folding for 'div'
                    if let [arg1, arg2] = optimized_args.as_slice() {
                        if let (
                            ast::Expr::Literal(ast::Literal::Number(a)),
                            ast::Expr::Literal(ast::Literal::Number(b)),
                        ) = (&*arg1.expr, &*arg2.expr)
                        {
                            // Note: Division by zero is not handled here; assumed to be runtime error.
                            return Rc::new(ast::Node {
                                token_id: node.token_id,
                                expr: Rc::new(ast::Expr::Literal(ast::Literal::Number(*a / *b))),
                            });
                        }
                    }
                } else if ident.name == CompactString::new("mul") { // Similar folding for 'mul'
                    if let [arg1, arg2] = optimized_args.as_slice() {
                        if let (
                            ast::Expr::Literal(ast::Literal::Number(a)),
                            ast::Expr::Literal(ast::Literal::Number(b)),
                        ) = (&*arg1.expr, &*arg2.expr)
                        {
                            return Rc::new(ast::Node {
                                token_id: node.token_id,
                                expr: Rc::new(ast::Expr::Literal(ast::Literal::Number(*a * *b))),
                            });
                        }
                    }
                } else if ident.name == CompactString::new("mod") { // Similar folding for 'mod'
                    if let [arg1, arg2] = optimized_args.as_slice() {
                        if let (
                            ast::Expr::Literal(ast::Literal::Number(a)),
                            ast::Expr::Literal(ast::Literal::Number(b)),
                        ) = (&*arg1.expr, &*arg2.expr)
                        {
                            // Note: Modulo by zero is not handled here.
                            return Rc::new(ast::Node {
                                token_id: node.token_id,
                                expr: Rc::new(ast::Expr::Literal(ast::Literal::Number(*a % *b))),
                            });
                        }
                    }
                }
                // If no constant folding rule applied, return the call with optimized arguments.
                Rc::new(ast::Node {
                    token_id: node.token_id,
                    expr: Rc::new(ast::Expr::Call(ident.clone(), optimized_args, *optional)),
                })
            }
            ast::Expr::Ident(ident) => {
                // Constant Propagation: If this identifier is in the constant_table,
                // replace its use with the corresponding literal expression.
                if let Some(constant_expr) = self.constant_table.get(ident) {
                    Rc::new(ast::Node {
                        token_id: node.token_id, // Preserve original token info for source mapping if needed.
                        expr: Rc::clone(constant_expr), // Substitute with the constant expression.
                    })
                } else {
                    // Not a known constant, leave the identifier as is.
                    Rc::clone(&node)
                }
            }
            ast::Expr::Foreach(ident, each_values_node, body_program) => {
                // Recursively optimize the collection expression and the body program.
                let optimized_collection = self.optimize_node(Rc::clone(each_values_node));
                let optimized_body = body_program
                    .iter()
                    .map(|stmt_node| self.optimize_node(Rc::clone(stmt_node)))
                    .collect::<Vec<_>>();
                // Reconstruct the Foreach node with optimized parts.
                Rc::new(ast::Node {
                    token_id: node.token_id,
                    expr: Rc::new(ast::Expr::Foreach(
                        ident.clone(), // Loop variable identifier remains the same.
                        optimized_collection,
                        optimized_body,
                    )),
                })
            }
            ast::Expr::If(conditions) => {
                // Recursively optimize all condition and body expressions within the If structure.
                let optimized_conditions = conditions
                    .iter()
                    .map(|(condition_node_opt, body_node)| {
                        let optimized_condition = condition_node_opt
                            .as_ref()
                            .map(|cond| self.optimize_node(Rc::clone(cond)));
                        let optimized_body = self.optimize_node(Rc::clone(body_node));
                        (optimized_condition, optimized_body)
                    })
                    .collect::<SmallVec<_>>();
                // Reconstruct the If node.
                Rc::new(ast::Node {
                    token_id: node.token_id,
                    expr: Rc::new(ast::Expr::If(optimized_conditions)),
                })
            }
            ast::Expr::Let(ident, value_expr_node) => {
                // Optimize the value expression of the `let` binding.
                let optimized_value_node = self.optimize_node(Rc::clone(value_expr_node));
                // If the optimized value is a literal, this variable becomes a candidate for constant propagation.
                // It's added to `constant_table` here. The decision to keep or remove this `Let` node
                // (Dead Code Elimination) is handled in the main `optimize` loop based on `used_identifiers`.
                // If this `Let` node *is* used and its value folds to a constant, it will be available
                // in `constant_table` for `Ident` nodes to use for propagation.
                if let ast::Expr::Literal(_) = &*optimized_value_node.expr {
                    self.constant_table
                        .insert(ident.clone(), Rc::clone(&optimized_value_node.expr));
                }
                // Reconstruct the Let node with the (potentially) optimized value.
                Rc::new(ast::Node {
                    token_id: node.token_id,
                    expr: Rc::new(ast::Expr::Let(ident.clone(), optimized_value_node)),
                })
            }
            ast::Expr::Def(ident, params, program_body) => {
                // Recursively optimize the body of a function definition.
                // Parameters and function name are definitions, not expressions to be optimized here.
                let optimized_body = program_body
                    .iter()
                    .map(|stmt_node| self.optimize_node(Rc::clone(stmt_node)))
                    .collect::<Vec<_>>();
                Rc::new(ast::Node {
                    token_id: node.token_id,
                    expr: Rc::new(ast::Expr::Def(ident.clone(), params.clone(), optimized_body)),
                })
            }
            ast::Expr::Fn(params, program_body) => {
                // Recursively optimize the body of an anonymous function.
                let optimized_body = program_body
                    .iter()
                    .map(|stmt_node| self.optimize_node(Rc::clone(stmt_node)))
                    .collect::<Vec<_>>();
                Rc::new(ast::Node {
                    token_id: node.token_id,
                    expr: Rc::new(ast::Expr::Fn(params.clone(), optimized_body)),
                })
            }
            ast::Expr::While(condition_node, program_body) => {
                // Recursively optimize the condition and the body of a while loop.
                let optimized_condition = self.optimize_node(Rc::clone(condition_node));
                let optimized_body = program_body
                    .iter()
                    .map(|stmt_node| self.optimize_node(Rc::clone(stmt_node)))
                    .collect::<Vec<_>>();
                Rc::new(ast::Node {
                    token_id: node.token_id,
                    expr: Rc::new(ast::Expr::While(optimized_condition, optimized_body)),
                })
            }
            ast::Expr::Until(condition_node, program_body) => {
                // Recursively optimize the condition and the body of an until loop.
                let optimized_condition = self.optimize_node(Rc::clone(condition_node));
                let optimized_body = program_body
                    .iter()
                    .map(|stmt_node| self.optimize_node(Rc::clone(stmt_node)))
                    .collect::<Vec<_>>();
                Rc::new(ast::Node {
                    token_id: node.token_id,
                    expr: Rc::new(ast::Expr::Until(optimized_condition, optimized_body)),
                })
            }
            // For expressions that don't have child expressions to optimize further
            // or for which current optimization rules don't apply (e.g. InterpolatedString, Literal, etc.),
            // clone the node. Literals are already in their most optimized (folded) form.
            // TODO: Implement optimization for InterpolatedString if segments can be constant.
            ast::Expr::InterpolatedString(_) // Could potentially optimize if all segments become literals.
            | ast::Expr::Selector(_)         // Selectors are structural, not value-based for folding here.
            | ast::Expr::Include(_)          // Include paths are literals; content is parsed, not optimized here.
            | ast::Expr::Literal(_)          // Literals are already constants.
            | ast::Expr::Nodes               // Structural.
            | ast::Expr::Self_ => Rc::clone(&node), // Structural.
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::node::{Expr as AstExpr, Ident, Literal, Node}; // Added Ident
    use rstest::rstest;
    use smallvec::smallvec;

    #[rstest]
    #[case::constant_folding_add(
            vec![
                Rc::new(ast::Node {
                    token_id: 0.into(),
                    expr: Rc::new(ast::Expr::Call(
                        ast::Ident::new("add"),
                        smallvec![
                            Rc::new(ast::Node {
                                token_id: 0.into(),
                                expr: Rc::new(ast::Expr::Literal(ast::Literal::Number(2.0.into()))),
                            }),
                            Rc::new(ast::Node {
                                token_id: 0.into(),
                                expr: Rc::new(ast::Expr::Literal(ast::Literal::Number(3.0.into()))),
                            }),
                        ],
                        false
                    )),
                })
            ],
            vec![
                Rc::new(ast::Node {
                    token_id: 0.into(),
                    expr: Rc::new(ast::Expr::Literal(ast::Literal::Number(5.0.into()))),
                })
            ])]
    #[case::constant_folding_add(
            vec![
                Rc::new(ast::Node {
                    token_id: 0.into(),
                    expr: Rc::new(ast::Expr::Call(
                        ast::Ident::new("add"),
                        smallvec![
                            Rc::new(ast::Node {
                                token_id: 0.into(),
                                expr: Rc::new(ast::Expr::Literal(ast::Literal::String("hello".to_string()))),
                            }),
                            Rc::new(ast::Node {
                                token_id: 0.into(),
                                expr: Rc::new(ast::Expr::Literal(ast::Literal::String("world".to_string()))),
                            }),
                        ],
                        false
                    )),
                })
            ],
            vec![
                Rc::new(ast::Node {
                    token_id: 0.into(),
                    expr: Rc::new(ast::Expr::Literal(ast::Literal::String("helloworld".to_string()))),
                })
            ])]
    #[case::constant_folding_sub(
            vec![
                Rc::new(ast::Node {
                    token_id: 0.into(),
                    expr: Rc::new(ast::Expr::Call(
                        ast::Ident::new("sub"),
                        smallvec![
                            Rc::new(ast::Node {
                                token_id: 0.into(),
                                expr: Rc::new(ast::Expr::Literal(ast::Literal::Number(5.0.into()))),
                            }),
                            Rc::new(ast::Node {
                                token_id: 0.into(),
                                expr: Rc::new(ast::Expr::Literal(ast::Literal::Number(3.0.into()))),
                            }),
                        ],
                        false
                    )),
                })
            ],
            vec![
                Rc::new(ast::Node {
                    token_id: 0.into(),
                    expr: Rc::new(ast::Expr::Literal(ast::Literal::Number(2.0.into()))),
                })
            ])]
    #[case::constant_folding_mul(
            vec![
                Rc::new(ast::Node {
                    token_id: 0.into(),
                    expr: Rc::new(ast::Expr::Call(
                        ast::Ident::new("mul"),
                        smallvec![
                            Rc::new(ast::Node {
                                token_id: 0.into(),
                                expr: Rc::new(ast::Expr::Literal(ast::Literal::Number(2.0.into()))),
                            }),
                            Rc::new(ast::Node {
                                token_id: 0.into(),
                                expr: Rc::new(ast::Expr::Literal(ast::Literal::Number(3.0.into()))),
                            }),
                        ],
                        false
                    )),
                })
            ],
            vec![
                Rc::new(ast::Node {
                    token_id: 0.into(),
                    expr: Rc::new(ast::Expr::Literal(ast::Literal::Number(6.0.into()))),
                })
            ])]
    #[case::constant_folding_div(
            vec![
                Rc::new(ast::Node {
                    token_id: 0.into(),
                    expr: Rc::new(ast::Expr::Call(
                        ast::Ident::new("div"),
                        smallvec![
                            Rc::new(ast::Node {
                                token_id: 0.into(),
                                expr: Rc::new(ast::Expr::Literal(ast::Literal::Number(6.0.into()))),
                            }),
                            Rc::new(ast::Node {
                                token_id: 0.into(),
                                expr: Rc::new(ast::Expr::Literal(ast::Literal::Number(3.0.into()))),
                            }),
                        ],
                        false
                    )),
                })
            ],
            vec![
                Rc::new(ast::Node {
                    token_id: 0.into(),
                    expr: Rc::new(ast::Expr::Literal(ast::Literal::Number(2.0.into()))),
                })
            ])]
    #[case::constant_folding_mod(
            vec![
                Rc::new(ast::Node {
                    token_id: 0.into(),
                    expr: Rc::new(ast::Expr::Call(
                        ast::Ident::new("mod"),
                        smallvec![
                            Rc::new(ast::Node {
                                token_id: 0.into(),
                                expr: Rc::new(ast::Expr::Literal(ast::Literal::Number(5.0.into()))),
                            }),
                            Rc::new(ast::Node {
                                token_id: 0.into(),
                                expr: Rc::new(ast::Expr::Literal(ast::Literal::Number(3.0.into()))),
                            }),
                        ],
                        false
                    )),
                })
            ],
            vec![
                Rc::new(ast::Node {
                    token_id: 0.into(),
                    expr: Rc::new(ast::Expr::Literal(ast::Literal::Number(2.0.into()))),
                })
            ])]
    #[case::constant_propagation(
            vec![
                Rc::new(ast::Node {
                    token_id: 0.into(),
                    expr: Rc::new(ast::Expr::Let(
                        ast::Ident::new("x"),
                        Rc::new(ast::Node {
                            token_id: 0.into(),
                            expr: Rc::new(ast::Expr::Literal(ast::Literal::Number(5.0.into()))),
                        }),
                    )),
                }),
                Rc::new(ast::Node {
                    token_id: 0.into(),
                    expr: Rc::new(ast::Expr::Ident(ast::Ident::new("x"))),
                })
            ],
            vec![
                Rc::new(ast::Node {
                    token_id: 0.into(),
                    expr: Rc::new(ast::Expr::Let(
                        ast::Ident::new("x"),
                        Rc::new(ast::Node {
                            token_id: 0.into(),
                            expr: Rc::new(ast::Expr::Literal(ast::Literal::Number(5.0.into()))),
                        }),
                    )),
                }),
                Rc::new(ast::Node {
                    token_id: 0.into(),
                    expr: Rc::new(ast::Expr::Literal(ast::Literal::Number(5.0.into()))),
                })
            ])]
    #[case::dead_code_elimination_simple_unused(
        vec![
            Rc::new(Node {
                token_id: 0.into(),
                expr: Rc::new(AstExpr::Let(
                    Ident::new("unused_var"),
                    Rc::new(Node {
                        token_id: 1.into(),
                        expr: Rc::new(AstExpr::Literal(Literal::Number(10.0.into()))),
                    }),
                )),
            }),
            Rc::new(Node {
                token_id: 2.into(),
                expr: Rc::new(AstExpr::Let(
                    Ident::new("used_var"),
                    Rc::new(Node {
                        token_id: 3.into(),
                        expr: Rc::new(AstExpr::Literal(Literal::Number(20.0.into()))),
                    }),
                )),
            }),
            Rc::new(Node {
                token_id: 4.into(),
                expr: Rc::new(AstExpr::Ident(Ident::new("used_var"))),
            }),
        ],
        // Expected: unused_var is removed
        vec![
            Rc::new(Node {
                token_id: 2.into(),
                expr: Rc::new(AstExpr::Let(
                    Ident::new("used_var"),
                    Rc::new(Node {
                        token_id: 3.into(),
                        expr: Rc::new(AstExpr::Literal(Literal::Number(20.0.into()))),
                    }),
                )),
            }),
            Rc::new(Node {
                token_id: 4.into(),
                expr: Rc::new(AstExpr::Literal(Literal::Number(20.0.into()))),
            }),
        ]
    )]
    #[case::dead_code_elimination_used_variable_kept(
        vec![
            Rc::new(Node {
                token_id: 0.into(),
                expr: Rc::new(AstExpr::Let(
                    Ident::new("x"),
                    Rc::new(Node {
                        token_id: 1.into(),
                        expr: Rc::new(AstExpr::Literal(Literal::Number(5.0.into()))),
                    }),
                )),
            }),
            Rc::new(Node {
                token_id: 2.into(),
                expr: Rc::new(AstExpr::Ident(Ident::new("x"))),
            }),
        ],
        vec![
            Rc::new(Node {
                token_id: 0.into(),
                expr: Rc::new(AstExpr::Let(
                    Ident::new("x"),
                    Rc::new(Node {
                        token_id: 1.into(),
                        expr: Rc::new(AstExpr::Literal(Literal::Number(5.0.into()))),
                    }),
                )),
            }),
            Rc::new(Node {
                token_id: 2.into(),
                expr: Rc::new(AstExpr::Literal(Literal::Number(5.0.into()))),
            }),
        ]
    )]
    #[case::dead_code_elimination_multiple_unused(
        vec![
            Rc::new(Node {
                token_id: 0.into(),
                expr: Rc::new(AstExpr::Let(Ident::new("a"), Rc::new(Node { token_id: 1.into(), expr: Rc::new(AstExpr::Literal(Literal::Number(1.0.into()))) }))),
            }),
            Rc::new(Node {
                token_id: 2.into(),
                expr: Rc::new(AstExpr::Let(Ident::new("b"), Rc::new(Node { token_id: 3.into(), expr: Rc::new(AstExpr::Literal(Literal::Number(2.0.into()))) }))),
            }),
            Rc::new(Node {
                token_id: 4.into(),
                expr: Rc::new(AstExpr::Let(Ident::new("c"), Rc::new(Node { token_id: 5.into(), expr: Rc::new(AstExpr::Literal(Literal::Number(30.0.into()))) }))),
            }),
             Rc::new(Node {
                token_id: 6.into(),
                expr: Rc::new(AstExpr::Ident(Ident::new("c"))),
            }),
        ],
        vec![
            Rc::new(Node {
                token_id: 4.into(),
                expr: Rc::new(AstExpr::Let(Ident::new("c"), Rc::new(Node { token_id: 5.into(), expr: Rc::new(AstExpr::Literal(Literal::Number(30.0.into()))) }))),
            }),
             Rc::new(Node {
                token_id: 6.into(),
                expr: Rc::new(AstExpr::Literal(Literal::Number(30.0.into()))),
            }),
        ]
    )]
    #[case::dead_code_elimination_mixed_used_unused(
        vec![
            Rc::new(Node {
                token_id: 0.into(),
                expr: Rc::new(AstExpr::Let(Ident::new("unused1"), Rc::new(Node { token_id: 1.into(), expr: Rc::new(AstExpr::Literal(Literal::Number(1.0.into()))) }))),
            }),
            Rc::new(Node {
                token_id: 2.into(),
                expr: Rc::new(AstExpr::Let(Ident::new("used1"), Rc::new(Node { token_id: 3.into(), expr: Rc::new(AstExpr::Literal(Literal::Number(10.0.into()))) }))),
            }),
            Rc::new(Node {
                token_id: 4.into(),
                expr: Rc::new(AstExpr::Let(Ident::new("unused2"), Rc::new(Node { token_id: 5.into(), expr: Rc::new(AstExpr::Literal(Literal::Number(2.0.into()))) }))),
            }),
            Rc::new(Node {
                token_id: 6.into(),
                expr: Rc::new(AstExpr::Ident(Ident::new("used1"))),
            }),
        ],
        vec![
            Rc::new(Node {
                token_id: 2.into(),
                expr: Rc::new(AstExpr::Let(Ident::new("used1"), Rc::new(Node { token_id: 3.into(), expr: Rc::new(AstExpr::Literal(Literal::Number(10.0.into()))) }))),
            }),
            Rc::new(Node {
                token_id: 6.into(),
                expr: Rc::new(AstExpr::Literal(Literal::Number(10.0.into()))),
            }),
        ]
    )]
    #[case::dead_code_elimination_unused_constant_candidate(
        vec![
            Rc::new(Node {
                token_id: 0.into(),
                expr: Rc::new(AstExpr::Let(
                    Ident::new("const_unused"),
                    Rc::new(Node {
                        token_id: 1.into(),
                        expr: Rc::new(AstExpr::Literal(Literal::Number(100.0.into()))),
                    }),
                )),
            }),
            Rc::new(Node {
                token_id: 2.into(),
                expr: Rc::new(AstExpr::Let(
                    Ident::new("another_var"),
                    Rc::new(Node {
                        token_id: 3.into(),
                        expr: Rc::new(AstExpr::Literal(Literal::Number(200.0.into()))),
                    }),
                )),
            }),
            Rc::new(Node {
                token_id: 4.into(),
                expr: Rc::new(AstExpr::Ident(Ident::new("another_var"))),
            }),
        ],
        vec![
            Rc::new(Node {
                token_id: 2.into(),
                expr: Rc::new(AstExpr::Let(
                    Ident::new("another_var"),
                    Rc::new(Node {
                        token_id: 3.into(),
                        expr: Rc::new(AstExpr::Literal(Literal::Number(200.0.into()))),
                    }),
                )),
            }),
            Rc::new(Node {
                token_id: 4.into(),
                expr: Rc::new(AstExpr::Literal(Literal::Number(200.0.into()))),
            }),
        ]
    )]
    fn test(#[case] input: Program, #[case] expected: Program) {
        let mut optimizer = Optimizer::new();
        let optimized_program = optimizer.optimize(&input);
        assert_eq!(optimized_program, expected);

        // Additionally, for the unused constant candidate test, check constant_table
        if input.len() == 3 && expected.len() == 2 {
            // Heuristic for this specific test case
            if let AstExpr::Let(ident, _) = &*input[0].expr {
                if ident.name.as_str() == "const_unused" {
                    assert!(
                        !optimizer.constant_table.contains_key(ident),
                        "const_unused should be removed from constant_table"
                    );
                }
            }
        }
    }
}
