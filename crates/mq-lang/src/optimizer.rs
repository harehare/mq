use std::rc::Rc;

use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet};
use smallvec::SmallVec;

use crate::{Program, ast::IdentName, ast::node::Args}; // Corrected import for IdentName

use super::ast::node as ast;

#[derive(Debug, Default)]
pub struct Optimizer {
    constant_table: FxHashMap<ast::Ident, Rc<ast::Expr>>,
    // No need to store used_identifiers here if it's collected and used per optimize call.
}

impl Optimizer {
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
        match &*node.expr {
            ast::Expr::Ident(ident) => {
                used_idents.insert(ident.name.clone());
            }
            ast::Expr::Call(func_ident, args, _) => {
                // The function name itself is a use if it's not a built-in.
                // However, the current task is about 'let' bound variables.
                // If 'func_ident' refers to a let-bound function, it's a use.
                used_idents.insert(func_ident.name.clone());
                for arg in args {
                    Self::collect_used_identifiers_in_node(arg, used_idents);
                }
            }
            ast::Expr::Let(_ident, value_node) => {
                // The _ident is a definition. Collect uses from its value.
                Self::collect_used_identifiers_in_node(value_node, used_idents);
            }
            ast::Expr::Def(_ident, _params, program_nodes) => {
                // _ident and _params are definitions for the sub-scope.
                // Collect uses from the body of the function/definition.
                for stmt in program_nodes {
                    Self::collect_used_identifiers_in_node(stmt, used_idents);
                }
            }
            ast::Expr::Fn(_params, program_nodes) => {
                // _params are definitions for the sub-scope.
                // Collect uses from the body of the function.
                // For this pass, we are collecting all idents that appear in readable positions.
                for stmt in program_nodes {
                    Self::collect_used_identifiers_in_node(stmt, used_idents);
                }
            }
            ast::Expr::If(conditions) => {
                for (cond_node_opt, body_node) in conditions {
                    if let Some(cond_node) = cond_node_opt {
                        Self::collect_used_identifiers_in_node(cond_node, used_idents);
                    }
                    Self::collect_used_identifiers_in_node(body_node, used_idents);
                }
            }
            ast::Expr::While(cond_node, program_nodes)
            | ast::Expr::Until(cond_node, program_nodes) => {
                Self::collect_used_identifiers_in_node(cond_node, used_idents);
                for stmt in program_nodes {
                    Self::collect_used_identifiers_in_node(stmt, used_idents);
                }
            }
            ast::Expr::Foreach(_item_ident, collection_node, program_nodes) => {
                // _item_ident is a definition for the sub-scope.
                Self::collect_used_identifiers_in_node(collection_node, used_idents);
                for stmt in program_nodes {
                    Self::collect_used_identifiers_in_node(stmt, used_idents);
                }
            }
            ast::Expr::InterpolatedString(segments) => {
                for segment in segments {
                    if let ast::StringSegment::Ident(ident) = segment {
                        used_idents.insert(ident.name.clone());
                    }
                }
            }
            ast::Expr::Include(ast::Literal::String(_path_str)) => {
                // If paths could be dynamic via idents, that would be a use.
                // For literal string paths, no idents used here.
            }
            // Note: ast::Literal does not have an Ident variant. Include only takes Literal::String.
            // The following case is effectively dead code if Literal::Ident is not possible.
            // ast::Expr::Include(ast::Literal::Ident(ident)) => {
            //    used_idents.insert(ident.name.clone());
            // }
            // Literal, Selector, Nodes, Self_ generally don't contain user-defined idents that are "uses"
            // of let-bound variables in the same way.
            ast::Expr::Literal(_)
            | ast::Expr::Selector(_)
            | ast::Expr::Nodes
            | ast::Expr::Self_
            | ast::Expr::Include(_) => {
                // No idents to collect from these directly in terms of variable usage.
            }
        }
    }

    fn collect_used_identifiers(&self, program: &Program) -> FxHashSet<IdentName> {
        let mut used_idents = FxHashSet::default();
        for node in program {
            Self::collect_used_identifiers_in_node(node, &mut used_idents);
        }
        used_idents
    }

    pub fn optimize(&mut self, program: &Program) -> Program {
        // Pass 1: Collect all used identifiers
        let used_identifiers = self.collect_used_identifiers(program);

        // Pass 2: Optimize and eliminate unused Let bindings
        let mut optimized_program = Vec::new();
        for node in program.iter() {
            match &*node.expr {
                ast::Expr::Let(ident, _value) => {
                    if used_identifiers.contains(&ident.name) {
                        // If used, optimize the node (which also handles constant prop for this let)
                        // and add it to the program.
                        optimized_program.push(self.optimize_node(Rc::clone(node)));
                    } else {
                        // If not used, remove it from constant_table if it was a const
                        // and do not add this Let node to the optimized_program.
                        if self.constant_table.contains_key(ident) {
                            // Check using Ident struct
                            self.constant_table.remove(ident);
                        }
                        // Optionally, log that a variable was removed, for debugging.
                        // e.g., println!("Optimizer: Removed unused variable '{}'", ident.name);
                    }
                }
                _ => {
                    // For all other node types, optimize them as usual.
                    optimized_program.push(self.optimize_node(Rc::clone(node)));
                }
            }
        }
        optimized_program
    }

    // optimize_node remains largely the same, but its handling of Let within the main optimize loop is now conditional.
    // The constant_table logic for Let nodes in optimize_node will only be hit if the Let node is deemed used.
    fn optimize_node(&mut self, node: Rc<ast::Node>) -> Rc<ast::Node> {
        match &*node.expr {
            ast::Expr::Call(ident, args, optional) => {
                let args: Args = args
                    .iter()
                    .map(|arg| self.optimize_node(Rc::clone(arg)))
                    .collect::<SmallVec<_>>();

                match (ident.name.as_str(), args.as_slice()) {
                    ("add", [arg1, arg2]) => match (&*arg1.expr, &*arg2.expr) {
                        (
                            ast::Expr::Literal(ast::Literal::Number(a)),
                            ast::Expr::Literal(ast::Literal::Number(b)),
                        ) => Rc::new(ast::Node {
                            token_id: node.token_id,
                            expr: Rc::new(ast::Expr::Literal(ast::Literal::Number(*a + *b))),
                        }),
                        (
                            ast::Expr::Literal(ast::Literal::String(a)),
                            ast::Expr::Literal(ast::Literal::String(b)),
                        ) => Rc::new(ast::Node {
                            token_id: node.token_id,
                            expr: Rc::new(ast::Expr::Literal(ast::Literal::String(format!(
                                "{}{}",
                                a, b
                            )))),
                        }),
                        _ => Rc::clone(&node),
                    },
                    ("sub", [arg1, arg2]) => match (&*arg1.expr, &*arg2.expr) {
                        (
                            ast::Expr::Literal(ast::Literal::Number(a)),
                            ast::Expr::Literal(ast::Literal::Number(b)),
                        ) => Rc::new(ast::Node {
                            token_id: node.token_id,
                            expr: Rc::new(ast::Expr::Literal(ast::Literal::Number(*a - *b))),
                        }),
                        _ => Rc::clone(&node),
                    },
                    ("div", [arg1, arg2]) => match (&*arg1.expr, &*arg2.expr) {
                        (
                            ast::Expr::Literal(ast::Literal::Number(a)),
                            ast::Expr::Literal(ast::Literal::Number(b)),
                        ) => Rc::new(ast::Node {
                            token_id: node.token_id,
                            expr: Rc::new(ast::Expr::Literal(ast::Literal::Number(*a / *b))),
                        }),
                        _ => Rc::clone(&node),
                    },
                    ("mul", [arg1, arg2]) => match (&*arg1.expr, &*arg2.expr) {
                        (
                            ast::Expr::Literal(ast::Literal::Number(a)),
                            ast::Expr::Literal(ast::Literal::Number(b)),
                        ) => Rc::new(ast::Node {
                            token_id: node.token_id,
                            expr: Rc::new(ast::Expr::Literal(ast::Literal::Number(*a * *b))),
                        }),
                        _ => Rc::clone(&node),
                    },
                    ("mod", [arg1, arg2]) => match (&*arg1.expr, &*arg2.expr) {
                        (
                            ast::Expr::Literal(ast::Literal::Number(a)),
                            ast::Expr::Literal(ast::Literal::Number(b)),
                        ) => Rc::new(ast::Node {
                            token_id: node.token_id,
                            expr: Rc::new(ast::Expr::Literal(ast::Literal::Number(*a % *b))),
                        }),
                        _ => Rc::clone(&node),
                    },
                    ("repeat", [arg1, arg2]) => match (&*arg1.expr, &*arg2.expr) {
                        (
                            ast::Expr::Literal(ast::Literal::String(s)),
                            ast::Expr::Literal(ast::Literal::Number(n)),
                        ) => Rc::new(ast::Node {
                            token_id: node.token_id,
                            expr: Rc::new(ast::Expr::Literal(ast::Literal::String(
                                s.repeat(n.value() as usize),
                            ))),
                        }),
                        _ => Rc::clone(&node),
                    },
                    ("reverse", [arg1]) => match &*arg1.expr {
                        ast::Expr::Literal(ast::Literal::String(s)) => Rc::new(ast::Node {
                            token_id: node.token_id,
                            expr: Rc::new(ast::Expr::Literal(ast::Literal::String(
                                s.chars().rev().collect::<String>(),
                            ))),
                        }),
                        _ => Rc::clone(&node),
                    },
                    _ => Rc::new(ast::Node {
                        token_id: node.token_id,
                        expr: Rc::new(ast::Expr::Call(ident.clone(), args, *optional)),
                    }),
                }
            }
            ast::Expr::Ident(ident) => {
                if let Some(expr) = self.constant_table.get(ident) {
                    Rc::new(ast::Node {
                        token_id: node.token_id,
                        expr: Rc::clone(expr),
                    })
                } else {
                    Rc::clone(&node)
                }
            }
            ast::Expr::Foreach(ident, each_values, program) => {
                let each_values = self.optimize_node(Rc::clone(each_values));
                let program = program
                    .iter()
                    .map(|node| self.optimize_node(Rc::clone(node)))
                    .collect::<Vec<_>>();

                Rc::new(ast::Node {
                    token_id: node.token_id,
                    expr: Rc::new(ast::Expr::Foreach(
                        ident.clone(),
                        Rc::clone(&each_values),
                        program,
                    )),
                })
            }
            ast::Expr::If(conditions) => {
                let conditions = conditions
                    .iter()
                    .map(|(cond, expr)| {
                        let cond = cond
                            .as_ref()
                            .map(|cond| self.optimize_node(Rc::clone(cond)));
                        let expr = self.optimize_node(Rc::clone(expr));
                        (cond, expr)
                    })
                    .collect::<SmallVec<_>>();

                Rc::new(ast::Node {
                    token_id: node.token_id,
                    expr: Rc::new(ast::Expr::If(conditions)),
                })
            }
            ast::Expr::Let(ident, value) => {
                // First, optimize the value part of the Let node.
                let optimized_value = self.optimize_node(Rc::clone(value));
                // If the optimized value is a literal, this variable is a candidate for constant propagation.
                if let ast::Expr::Literal(_) = &*optimized_value.expr {
                    // Add to constant_table. If this Let node is later removed by DCE,
                    // the main optimize loop should ideally remove it from constant_table.
                    // However, with the new two-pass approach, this optimize_node for Let
                    // is only called if the variable is USED. So, adding to constant_table here is correct.
                    self.constant_table
                        .insert(ident.clone(), Rc::clone(&optimized_value.expr));
                }
                // Reconstruct the Let node with the optimized value.
                // This node itself might be removed later if the variable `ident` is unused,
                // but that decision is made in the main `optimize` loop.
                Rc::new(ast::Node {
                    token_id: node.token_id,
                    expr: Rc::new(ast::Expr::Let(ident.clone(), optimized_value)),
                })
            }
            ast::Expr::Def(ident, params, program) => {
                let params = params.clone();
                let program = program
                    .iter()
                    .map(|node| self.optimize_node(Rc::clone(node)))
                    .collect::<Vec<_>>();

                Rc::new(ast::Node {
                    token_id: node.token_id,
                    expr: Rc::new(ast::Expr::Def(ident.clone(), params, program)),
                })
            }
            ast::Expr::Fn(params, program) => {
                let params = params.clone();
                let program = program
                    .iter()
                    .map(|node| self.optimize_node(Rc::clone(node)))
                    .collect::<Vec<_>>();

                Rc::new(ast::Node {
                    token_id: node.token_id,
                    expr: Rc::new(ast::Expr::Fn(params, program)),
                })
            }
            ast::Expr::While(cond, program) => {
                let program = program
                    .iter()
                    .map(|node| self.optimize_node(Rc::clone(node)))
                    .collect::<Vec<_>>();

                Rc::new(ast::Node {
                    token_id: node.token_id,
                    expr: Rc::new(ast::Expr::While(Rc::clone(cond), program)),
                })
            }
            ast::Expr::Until(cond, program) => {
                let program = program
                    .iter()
                    .map(|node| self.optimize_node(Rc::clone(node)))
                    .collect::<Vec<_>>();

                Rc::new(ast::Node {
                    token_id: node.token_id,
                    expr: Rc::new(ast::Expr::Until(Rc::clone(cond), program)),
                })
            }
            // TODO: implements interpolated string
            ast::Expr::InterpolatedString(_)
            | ast::Expr::Selector(_)
            | ast::Expr::Include(_)
            | ast::Expr::Literal(_)
            | ast::Expr::Nodes
            | ast::Expr::Self_ => Rc::clone(&node),
            ast::Expr::DictionaryLiteral(pairs) => {
                let dict_ident = ast::Ident::new_with_token(
                    "dict",
                    node.token_id.get_token(&self.constant_table), // Attempt to get token
                );
                let mut args: Args = SmallVec::new();
                for (key_node, value_node) in pairs {
                    args.push(self.optimize_node(Rc::clone(key_node)));
                    args.push(self.optimize_node(Rc::clone(value_node)));
                }
                Rc::new(ast::Node {
                    token_id: node.token_id,
                    expr: Rc::new(ast::Expr::Call(dict_ident, args, false)),
                })
            }
        }
    }

    #[rstest]
    #[case::optimize_empty_dictionary_literal(
        vec![
            Rc::new(Node {
                token_id: 0.into(),
                expr: Rc::new(AstExpr::DictionaryLiteral(vec![])),
            }),
        ],
        vec![
            Rc::new(Node {
                token_id: 0.into(),
                expr: Rc::new(AstExpr::Call(
                    Ident::new_with_token("dict", None), // Token might be None due to TokenResolver
                    smallvec![],
                    false,
                )),
            }),
        ]
    )]
    #[case::optimize_dictionary_literal_simple(
        vec![
            Rc::new(Node {
                token_id: 0.into(), // Token ID for the original DictionaryLiteral node
                expr: Rc::new(AstExpr::DictionaryLiteral(vec![
                    (
                        Rc::new(Node { token_id: 1.into(), expr: Rc::new(AstExpr::Ident(Ident::new("a"))) }),
                        Rc::new(Node { token_id: 2.into(), expr: Rc::new(AstExpr::Literal(Literal::Number(1.0.into()))) })
                    ),
                    (
                        Rc::new(Node { token_id: 3.into(), expr: Rc::new(AstExpr::Literal(Literal::String("b".to_string()))) }),
                        Rc::new(Node { token_id: 4.into(), expr: Rc::new(AstExpr::Literal(Literal::Number(2.0.into()))) })
                    )
                ])),
            }),
        ],
        vec![
            Rc::new(Node {
                token_id: 0.into(), // Original DictionaryLiteral's token_id preserved
                expr: Rc::new(AstExpr::Call(
                    Ident::new_with_token("dict", None),
                    smallvec![
                        Rc::new(Node { token_id: 1.into(), expr: Rc::new(AstExpr::Ident(Ident::new("a"))) }), // Optimized key
                        Rc::new(Node { token_id: 2.into(), expr: Rc::new(AstExpr::Literal(Literal::Number(1.0.into()))) }), // Optimized value
                        Rc::new(Node { token_id: 3.into(), expr: Rc::new(AstExpr::Literal(Literal::String("b".to_string()))) }), // Optimized key
                        Rc::new(Node { token_id: 4.into(), expr: Rc::new(AstExpr::Literal(Literal::Number(2.0.into()))) })  // Optimized value
                    ],
                    false,
                )),
            }),
        ]
    )]
    #[case::optimize_dictionary_with_expressions_to_fold(
        vec![
            Rc::new(Node {
                token_id: 0.into(),
                expr: Rc::new(AstExpr::DictionaryLiteral(vec![
                    (
                        Rc::new(Node { token_id: 1.into(), expr: Rc::new(AstExpr::Ident(Ident::new("key1"))) }),
                        Rc::new(Node { // Value is an expression that can be folded: add(1,2)
                            token_id: 2.into(),
                            expr: Rc::new(AstExpr::Call(
                                Ident::new("add"),
                                smallvec![
                                    Rc::new(Node { token_id: 3.into(), expr: Rc::new(AstExpr::Literal(Literal::Number(1.0.into()))) }),
                                    Rc::new(Node { token_id: 4.into(), expr: Rc::new(AstExpr::Literal(Literal::Number(2.0.into()))) })
                                ],
                                false
                            ))
                        })
                    )
                ])),
            }),
        ],
        vec![
            Rc::new(Node {
                token_id: 0.into(),
                expr: Rc::new(AstExpr::Call(
                    Ident::new_with_token("dict", None),
                    smallvec![
                        Rc::new(Node { token_id: 1.into(), expr: Rc::new(AstExpr::Ident(Ident::new("key1"))) }),
                        Rc::new(Node { token_id: 2.into(), expr: Rc::new(AstExpr::Literal(Literal::Number(3.0.into()))) }) // Value folded
                    ],
                    false,
                )),
            }),
        ]
    )]
    fn test_optimizer_dictionaries(#[case] input: Program, #[case] expected: Program) {
        let mut optimizer = Optimizer::new();
        let optimized_program = optimizer.optimize(&input);
        assert_eq!(optimized_program, expected);
    }
}

// Helper trait to attempt to get a token from TokenId via constant_table
// This is a conceptual placeholder; actual implementation depends on how TokenId relates to tokens
// stored or accessible via optimizer's state or Arena.
// For now, this will likely be None as constant_table doesn't store tokens directly.
trait TokenResolver {
    fn get_token(&self, _constant_table: &FxHashMap<ast::Ident, Rc<ast::Expr>>) -> Option<Rc<crate::Token>>;
}

impl TokenResolver for crate::arena::ArenaId<Rc<crate::Token>> {
    fn get_token(&self, _constant_table: &FxHashMap<ast::Ident, Rc<ast::Expr>>) -> Option<Rc<crate::Token>> {
        // In a real scenario, you might look up the token in an Arena if self is an ID.
        // Or, if the Ident itself stored an Rc<Token>, you'd use that.
        // Given the current structure, directly getting the original token for "dict" is non-trivial
        // without passing the token arena or ensuring Idents store their tokens.
        // For now, returning None, meaning Ident::new will be used (no specific token).
        None
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
    #[case::constant_folding_repeat(
        vec![
            Rc::new(Node {
                token_id: 0.into(),
                expr: Rc::new(AstExpr::Call(
                    Ident::new("repeat"),
                    smallvec![
                        Rc::new(Node {
                            token_id: 1.into(),
                            expr: Rc::new(AstExpr::Literal(Literal::String("ab".to_string()))),
                        }),
                        Rc::new(Node {
                            token_id: 2.into(),
                            expr: Rc::new(AstExpr::Literal(Literal::Number(3.0.into()))),
                        }),
                    ],
                    false,
                )),
            }),
        ],
        vec![
            Rc::new(Node {
                token_id: 0.into(),
                expr: Rc::new(AstExpr::Literal(Literal::String("ababab".to_string()))),
            }),
        ]
    )]
    #[case::constant_folding_reverse(
        vec![
            Rc::new(Node {
                token_id: 0.into(),
                expr: Rc::new(AstExpr::Call(
                    Ident::new("reverse"),
                    smallvec![
                        Rc::new(Node {
                            token_id: 1.into(),
                            expr: Rc::new(AstExpr::Literal(Literal::String("abc".to_string()))),
                        }),
                    ],
                    false,
                )),
            }),
        ],
        vec![
            Rc::new(Node {
                token_id: 0.into(),
                expr: Rc::new(AstExpr::Literal(Literal::String("cba".to_string()))),
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
