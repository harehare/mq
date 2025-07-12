use super::ast::node as ast;
use crate::{Program, ast::IdentName};
use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet};
use std::rc::Rc;

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

    pub fn optimize(&mut self, program: &mut Program) {
        let used_identifiers = self.collect_used_identifiers(program);

        program.retain_mut(|node| {
            if let ast::Expr::Let(ident, _) = &*node.expr {
                if !used_identifiers.contains(&ident.name) {
                    self.constant_table.remove(ident);
                    return false;
                }
            }
            true
        });

        for node in program {
            self.optimize_node(node);
        }
    }

    fn collect_used_identifiers(&mut self, program: &Program) -> FxHashSet<IdentName> {
        let mut used_idents = FxHashSet::default();
        for node in program {
            Self::collect_used_identifiers_in_node(node, &mut used_idents);
        }
        used_idents
    }

    fn collect_used_identifiers_in_node(
        node: &Rc<ast::Node>,
        used_idents: &mut FxHashSet<IdentName>,
    ) {
        match &*node.expr {
            ast::Expr::Ident(ident) => {
                used_idents.insert(ident.name.clone());
            }
            ast::Expr::Call(func_ident, args, _) => {
                used_idents.insert(func_ident.name.clone());
                for arg in args {
                    Self::collect_used_identifiers_in_node(arg, used_idents);
                }
            }
            ast::Expr::Let(_, value_node) => {
                Self::collect_used_identifiers_in_node(value_node, used_idents);
            }
            ast::Expr::Def(_, _, program_nodes) | ast::Expr::Fn(_, program_nodes) => {
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
            ast::Expr::Foreach(_, collection_node, program_nodes) => {
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
            ast::Expr::Literal(_)
            | ast::Expr::Selector(_)
            | ast::Expr::Nodes
            | ast::Expr::Self_
            | ast::Expr::Include(_) => {}
        }
    }

    fn optimize_node(&mut self, node: &mut Rc<ast::Node>) {
        let mut_node = Rc::make_mut(node);
        let mut_expr = Rc::make_mut(&mut mut_node.expr);

        match mut_expr {
            ast::Expr::Call(ident, args, _optional) => {
                for arg in args.iter_mut() {
                    self.optimize_node(arg);
                }

                let new_expr = match (ident.name.as_str(), args.as_slice()) {
                    ("add", [arg1, arg2]) => match (&*arg1.expr, &*arg2.expr) {
                        (
                            ast::Expr::Literal(ast::Literal::Number(a)),
                            ast::Expr::Literal(ast::Literal::Number(b)),
                        ) => Some(ast::Expr::Literal(ast::Literal::Number(*a + *b))),
                        (
                            ast::Expr::Literal(ast::Literal::String(a)),
                            ast::Expr::Literal(ast::Literal::String(b)),
                        ) => Some(ast::Expr::Literal(ast::Literal::String(format!(
                            "{}{}",
                            a, b
                        )))),
                        _ => None,
                    },
                    ("sub", [arg1, arg2]) => match (&*arg1.expr, &*arg2.expr) {
                        (
                            ast::Expr::Literal(ast::Literal::Number(a)),
                            ast::Expr::Literal(ast::Literal::Number(b)),
                        ) => Some(ast::Expr::Literal(ast::Literal::Number(*a - *b))),
                        _ => None,
                    },
                    ("div", [arg1, arg2]) => match (&*arg1.expr, &*arg2.expr) {
                        (
                            ast::Expr::Literal(ast::Literal::Number(a)),
                            ast::Expr::Literal(ast::Literal::Number(b)),
                        ) => Some(ast::Expr::Literal(ast::Literal::Number(*a / *b))),
                        _ => None,
                    },
                    ("mul", [arg1, arg2]) => match (&*arg1.expr, &*arg2.expr) {
                        (
                            ast::Expr::Literal(ast::Literal::Number(a)),
                            ast::Expr::Literal(ast::Literal::Number(b)),
                        ) => Some(ast::Expr::Literal(ast::Literal::Number(*a * *b))),
                        _ => None,
                    },
                    ("mod", [arg1, arg2]) => match (&*arg1.expr, &*arg2.expr) {
                        (
                            ast::Expr::Literal(ast::Literal::Number(a)),
                            ast::Expr::Literal(ast::Literal::Number(b)),
                        ) => Some(ast::Expr::Literal(ast::Literal::Number(*a % *b))),
                        _ => None,
                    },
                    ("repeat", [arg1, arg2]) => match (&*arg1.expr, &*arg2.expr) {
                        (
                            ast::Expr::Literal(ast::Literal::String(s)),
                            ast::Expr::Literal(ast::Literal::Number(n)),
                        ) => Some(ast::Expr::Literal(ast::Literal::String(
                            s.repeat(n.value() as usize),
                        ))),
                        _ => None,
                    },
                    ("reverse", [arg1]) => match &*arg1.expr {
                        ast::Expr::Literal(ast::Literal::String(s)) => Some(ast::Expr::Literal(
                            ast::Literal::String(s.chars().rev().collect::<String>()),
                        )),
                        _ => None,
                    },
                    _ => None,
                };
                if let Some(expr) = new_expr {
                    mut_node.expr = Rc::new(expr);
                }
            }
            ast::Expr::Ident(ident) => {
                if let Some(expr) = self.constant_table.get(ident) {
                    mut_node.expr = Rc::clone(expr);
                }
            }
            ast::Expr::Foreach(_, each_values, program) => {
                self.optimize_node(each_values);
                for node in program {
                    self.optimize_node(node);
                }
            }
            ast::Expr::If(conditions) => {
                for (cond, expr) in conditions.iter_mut() {
                    if let Some(c) = cond {
                        self.optimize_node(c);
                    }
                    self.optimize_node(expr);
                }
            }
            ast::Expr::Let(ident, value) => {
                self.optimize_node(value);
                if let ast::Expr::Literal(_) = &*value.expr {
                    self.constant_table
                        .insert(ident.clone(), Rc::clone(&value.expr));
                }
            }
            ast::Expr::Def(_, _, program)
            | ast::Expr::Fn(_, program)
            | ast::Expr::While(_, program)
            | ast::Expr::Until(_, program) => {
                for node in program {
                    self.optimize_node(node);
                }
            }
            _ => {}
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
        let mut optimized_program = input.clone();
        optimizer.optimize(&mut optimized_program);
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
