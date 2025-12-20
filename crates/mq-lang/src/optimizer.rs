use super::ast::node as ast;
use crate::{Ident, Program, Shared};
use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet};

/// Optimization levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OptimizationLevel {
    /// No optimization
    None,
    /// Full optimization (constant folding + dead code elimination)
    #[default]
    Full,
}

#[derive(Debug)]
pub struct Optimizer {
    constant_table: FxHashMap<Ident, Shared<ast::Expr>>,
    optimization_level: OptimizationLevel,
}

impl Default for Optimizer {
    fn default() -> Self {
        Self {
            constant_table: FxHashMap::with_capacity_and_hasher(200, FxBuildHasher),
            optimization_level: OptimizationLevel::default(),
        }
    }
}

impl Optimizer {
    /// Creates a new optimizer with a custom optimization level
    #[allow(dead_code)]
    pub fn with_level(level: OptimizationLevel) -> Self {
        Self {
            optimization_level: level,
            ..Default::default()
        }
    }

    pub fn optimize(&mut self, program: &mut Program) {
        match self.optimization_level {
            OptimizationLevel::None => {
                // No optimization
            }
            OptimizationLevel::Full => {
                self.dead_code_elimination(program);
                self.constant_folding(program);
            }
        }
    }

    #[inline(always)]
    fn constant_folding(&mut self, program: &mut Program) {
        for node in program {
            self.optimize_node(node);
        }
    }

    #[inline(always)]
    fn dead_code_elimination(&mut self, program: &mut Program) {
        let used_identifiers = self.collect_used_identifiers(program);

        program.retain_mut(|node| {
            if let ast::Expr::Let(ident, _) = &*node.expr
                && !used_identifiers.contains(&ident.name)
            {
                self.constant_table.remove(&ident.name);
                return false;
            }
            true
        });
    }

    #[inline(always)]
    fn collect_used_identifiers(&mut self, program: &Program) -> FxHashSet<Ident> {
        let mut used_idents = FxHashSet::default();
        for node in program {
            Self::collect_used_identifiers_in_node(node, &mut used_idents);
        }
        used_idents
    }

    fn collect_used_identifiers_in_node(node: &Shared<ast::Node>, used_idents: &mut FxHashSet<Ident>) {
        match &*node.expr {
            ast::Expr::Ident(ident) => {
                used_idents.insert(ident.name);
            }
            ast::Expr::Call(func_ident, args) => {
                used_idents.insert(func_ident.name);
                for arg in args {
                    Self::collect_used_identifiers_in_node(arg, used_idents);
                }
            }
            ast::Expr::CallDynamic(callable, args) => {
                Self::collect_used_identifiers_in_node(callable, used_idents);
                for arg in args {
                    Self::collect_used_identifiers_in_node(arg, used_idents);
                }
            }
            ast::Expr::Let(_, value_node) | ast::Expr::Var(_, value_node) | ast::Expr::Assign(_, value_node) => {
                Self::collect_used_identifiers_in_node(value_node, used_idents);
            }
            ast::Expr::Block(program_nodes) | ast::Expr::Def(_, _, program_nodes) | ast::Expr::Fn(_, program_nodes) => {
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
            ast::Expr::While(cond_node, program_nodes) => {
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
                    if let ast::StringSegment::Expr(node) = segment {
                        Self::collect_used_identifiers_in_node(node, used_idents);
                    }
                }
            }
            ast::Expr::Paren(node) => {
                Self::collect_used_identifiers_in_node(node, used_idents);
            }
            ast::Expr::Try(try_node, catch_node) => {
                Self::collect_used_identifiers_in_node(try_node, used_idents);
                Self::collect_used_identifiers_in_node(catch_node, used_idents);
            }
            ast::Expr::And(expr1, expr2) | ast::Expr::Or(expr1, expr2) => {
                Self::collect_used_identifiers_in_node(expr1, used_idents);
                Self::collect_used_identifiers_in_node(expr2, used_idents);
            }
            ast::Expr::Match(value, arms) => {
                Self::collect_used_identifiers_in_node(value, used_idents);
                for arm in arms {
                    // Collect identifiers from guard
                    if let Some(guard) = &arm.guard {
                        Self::collect_used_identifiers_in_node(guard, used_idents);
                    }
                    // Collect identifiers from body
                    Self::collect_used_identifiers_in_node(&arm.body, used_idents);
                }
            }
            ast::Expr::QualifiedAccess(module_path, access_target) => {
                // Collect all module names in the path
                for module_ident in module_path {
                    used_idents.insert(module_ident.name);
                }
                // Collect from access target
                match access_target {
                    ast::AccessTarget::Call(_, args) => {
                        for arg in args {
                            Self::collect_used_identifiers_in_node(arg, used_idents);
                        }
                    }
                    ast::AccessTarget::Ident(_) => {}
                }
            }
            ast::Expr::Literal(_)
            | ast::Expr::Selector(_)
            | ast::Expr::Nodes
            | ast::Expr::Self_
            | ast::Expr::Include(_)
            | ast::Expr::Import(_)
            | ast::Expr::Module(_, _)
            | ast::Expr::Break
            | ast::Expr::Continue => {}
        }
    }

    fn optimize_node(&mut self, node: &mut Shared<ast::Node>) {
        let mut_node = Shared::make_mut(node);
        let mut_expr = Shared::make_mut(&mut mut_node.expr);

        match mut_expr {
            ast::Expr::Call(ident, args) => {
                for arg in args.iter_mut() {
                    self.optimize_node(arg);
                }

                let new_expr = ident.name.resolve_with(|name_str| match (name_str, args.as_slice()) {
                    ("add", [arg1, arg2]) => match (&*arg1.expr, &*arg2.expr) {
                        (ast::Expr::Literal(ast::Literal::Number(a)), ast::Expr::Literal(ast::Literal::Number(b))) => {
                            Some(ast::Expr::Literal(ast::Literal::Number(*a + *b)))
                        }
                        (ast::Expr::Literal(ast::Literal::String(a)), ast::Expr::Literal(ast::Literal::String(b))) => {
                            Some(ast::Expr::Literal(ast::Literal::String(format!("{}{}", a, b))))
                        }
                        _ => None,
                    },
                    ("sub", [arg1, arg2]) => match (&*arg1.expr, &*arg2.expr) {
                        (ast::Expr::Literal(ast::Literal::Number(a)), ast::Expr::Literal(ast::Literal::Number(b))) => {
                            Some(ast::Expr::Literal(ast::Literal::Number(*a - *b)))
                        }
                        _ => None,
                    },
                    ("div", [arg1, arg2]) => match (&*arg1.expr, &*arg2.expr) {
                        (ast::Expr::Literal(ast::Literal::Number(a)), ast::Expr::Literal(ast::Literal::Number(b))) => {
                            Some(ast::Expr::Literal(ast::Literal::Number(*a / *b)))
                        }
                        _ => None,
                    },
                    ("mul", [arg1, arg2]) => match (&*arg1.expr, &*arg2.expr) {
                        (ast::Expr::Literal(ast::Literal::Number(a)), ast::Expr::Literal(ast::Literal::Number(b))) => {
                            Some(ast::Expr::Literal(ast::Literal::Number(*a * *b)))
                        }
                        _ => None,
                    },
                    ("mod", [arg1, arg2]) => match (&*arg1.expr, &*arg2.expr) {
                        (ast::Expr::Literal(ast::Literal::Number(a)), ast::Expr::Literal(ast::Literal::Number(b))) => {
                            Some(ast::Expr::Literal(ast::Literal::Number(*a % *b)))
                        }
                        _ => None,
                    },
                    ("repeat", [arg1, arg2]) => match (&*arg1.expr, &*arg2.expr) {
                        (ast::Expr::Literal(ast::Literal::String(s)), ast::Expr::Literal(ast::Literal::Number(n))) => {
                            Some(ast::Expr::Literal(ast::Literal::String(s.repeat(n.value() as usize))))
                        }
                        _ => None,
                    },
                    ("reverse", [arg1]) => match &*arg1.expr {
                        ast::Expr::Literal(ast::Literal::String(s)) => Some(ast::Expr::Literal(ast::Literal::String(
                            s.chars().rev().collect::<String>(),
                        ))),
                        _ => None,
                    },
                    _ => None,
                });
                if let Some(expr) = new_expr {
                    mut_node.expr = Shared::new(expr);
                }
            }
            ast::Expr::Ident(ident) => {
                if let Some(expr) = self.constant_table.get(&ident.name) {
                    mut_node.expr = Shared::clone(expr);
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
                        .insert(ident.name.to_owned(), Shared::clone(&value.expr));
                }
            }
            ast::Expr::Def(_, _, program) | ast::Expr::Fn(_, program) => {
                // Save current constant table to prevent leaking function-local constants
                let saved_constant_table = std::mem::take(&mut self.constant_table);

                for node in program {
                    self.optimize_node(node);
                }

                // Restore the outer scope's constant table
                self.constant_table = saved_constant_table;
            }
            ast::Expr::Paren(expr) => {
                self.optimize_node(expr);
            }
            ast::Expr::InterpolatedString(segments) => {
                for segment in segments.iter_mut() {
                    if let ast::StringSegment::Expr(node) = segment {
                        self.optimize_node(node);
                    }
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::node::{Expr as AstExpr, IdentWithToken, Literal, Node}; // Added Ident
    use rstest::rstest;
    use smallvec::smallvec;

    #[rstest]
    #[case::constant_folding_add(
            vec![
                Shared::new(ast::Node {
                    token_id: 0.into(),
                    expr: Shared::new(ast::Expr::Call(
                        IdentWithToken::new("add"),
                        smallvec![
                            Shared::new(ast::Node {
                                token_id: 0.into(),
                                expr: Shared::new(ast::Expr::Literal(ast::Literal::Number(2.0.into()))),
                            }),
                            Shared::new(ast::Node {
                                token_id: 0.into(),
                                expr: Shared::new(ast::Expr::Literal(ast::Literal::Number(3.0.into()))),
                            }),
                        ],
                    )),
                })
            ],
            vec![
                Shared::new(ast::Node {
                    token_id: 0.into(),
                    expr: Shared::new(ast::Expr::Literal(ast::Literal::Number(5.0.into()))),
                })
            ])]
    #[case::constant_folding_add(
            vec![
                Shared::new(ast::Node {
                    token_id: 0.into(),
                    expr: Shared::new(ast::Expr::Call(
                        IdentWithToken::new("add"),
                        smallvec![
                            Shared::new(ast::Node {
                                token_id: 0.into(),
                                expr: Shared::new(ast::Expr::Literal(ast::Literal::String("hello".to_string()))),
                            }),
                            Shared::new(ast::Node {
                                token_id: 0.into(),
                                expr: Shared::new(ast::Expr::Literal(ast::Literal::String("world".to_string()))),
                            }),
                        ],
                    )),
                })
            ],
            vec![
                Shared::new(ast::Node {
                    token_id: 0.into(),
                    expr: Shared::new(ast::Expr::Literal(ast::Literal::String("helloworld".to_string()))),
                })
            ])]
    #[case::constant_folding_sub(
            vec![
                Shared::new(ast::Node {
                    token_id: 0.into(),
                    expr: Shared::new(ast::Expr::Call(
                        IdentWithToken::new("sub"),
                        smallvec![
                            Shared::new(ast::Node {
                                token_id: 0.into(),
                                expr: Shared::new(ast::Expr::Literal(ast::Literal::Number(5.0.into()))),
                            }),
                            Shared::new(ast::Node {
                                token_id: 0.into(),
                                expr: Shared::new(ast::Expr::Literal(ast::Literal::Number(3.0.into()))),
                            }),
                        ],
                    )),
                })
            ],
            vec![
                Shared::new(ast::Node {
                    token_id: 0.into(),
                    expr: Shared::new(ast::Expr::Literal(ast::Literal::Number(2.0.into()))),
                })
            ])]
    #[case::constant_folding_mul(
            vec![
                Shared::new(ast::Node {
                    token_id: 0.into(),
                    expr: Shared::new(ast::Expr::Call(
                        IdentWithToken::new("mul"),
                        smallvec![
                            Shared::new(ast::Node {
                                token_id: 0.into(),
                                expr: Shared::new(ast::Expr::Literal(ast::Literal::Number(2.0.into()))),
                            }),
                            Shared::new(ast::Node {
                                token_id: 0.into(),
                                expr: Shared::new(ast::Expr::Literal(ast::Literal::Number(3.0.into()))),
                            }),
                        ],
                    )),
                })
            ],
            vec![
                Shared::new(ast::Node {
                    token_id: 0.into(),
                    expr: Shared::new(ast::Expr::Literal(ast::Literal::Number(6.0.into()))),
                })
            ])]
    #[case::constant_folding_div(
            vec![
                Shared::new(ast::Node {
                    token_id: 0.into(),
                    expr: Shared::new(ast::Expr::Call(
                        IdentWithToken::new("div"),
                        smallvec![
                            Shared::new(ast::Node {
                                token_id: 0.into(),
                                expr: Shared::new(ast::Expr::Literal(ast::Literal::Number(6.0.into()))),
                            }),
                            Shared::new(ast::Node {
                                token_id: 0.into(),
                                expr: Shared::new(ast::Expr::Literal(ast::Literal::Number(3.0.into()))),
                            }),
                        ],
                    )),
                })
            ],
            vec![
                Shared::new(ast::Node {
                    token_id: 0.into(),
                    expr: Shared::new(ast::Expr::Literal(ast::Literal::Number(2.0.into()))),
                })
            ])]
    #[case::constant_folding_mod(
            vec![
                Shared::new(ast::Node {
                    token_id: 0.into(),
                    expr: Shared::new(ast::Expr::Call(
                        IdentWithToken::new("mod"),
                        smallvec![
                            Shared::new(ast::Node {
                                token_id: 0.into(),
                                expr: Shared::new(ast::Expr::Literal(ast::Literal::Number(5.0.into()))),
                            }),
                            Shared::new(ast::Node {
                                token_id: 0.into(),
                                expr: Shared::new(ast::Expr::Literal(ast::Literal::Number(3.0.into()))),
                            }),
                        ],
                    )),
                })
            ],
            vec![
                Shared::new(ast::Node {
                    token_id: 0.into(),
                    expr: Shared::new(ast::Expr::Literal(ast::Literal::Number(2.0.into()))),
                })
            ])]
    #[case::constant_propagation(
            vec![
                Shared::new(ast::Node {
                    token_id: 0.into(),
                    expr: Shared::new(ast::Expr::Let(
                        IdentWithToken::new("x"),
                        Shared::new(ast::Node {
                            token_id: 0.into(),
                            expr: Shared::new(ast::Expr::Literal(ast::Literal::Number(5.0.into()))),
                        }),
                    )),
                }),
                Shared::new(ast::Node {
                    token_id: 0.into(),
                    expr: Shared::new(ast::Expr::Ident(IdentWithToken::new("x"))),
                })
            ],
            vec![
                Shared::new(ast::Node {
                    token_id: 0.into(),
                    expr: Shared::new(ast::Expr::Let(
                        IdentWithToken::new("x"),
                        Shared::new(ast::Node {
                            token_id: 0.into(),
                            expr: Shared::new(ast::Expr::Literal(ast::Literal::Number(5.0.into()))),
                        }),
                    )),
                }),
                Shared::new(ast::Node {
                    token_id: 0.into(),
                    expr: Shared::new(ast::Expr::Literal(ast::Literal::Number(5.0.into()))),
                })
            ])]
    #[case::dead_code_elimination_simple_unused(
        vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Let(
                    IdentWithToken::new("unused_var"),
                    Shared::new(Node {
                        token_id: 1.into(),
                        expr: Shared::new(AstExpr::Literal(Literal::Number(10.0.into()))),
                    }),
                )),
            }),
            Shared::new(Node {
                token_id: 2.into(),
                expr: Shared::new(AstExpr::Let(
                    IdentWithToken::new("used_var"),
                    Shared::new(Node {
                        token_id: 3.into(),
                        expr: Shared::new(AstExpr::Literal(Literal::Number(20.0.into()))),
                    }),
                )),
            }),
            Shared::new(Node {
                token_id: 4.into(),
                expr: Shared::new(AstExpr::Ident(IdentWithToken::new("used_var"))),
            }),
        ],
        // Expected: unused_var is removed
        vec![
            Shared::new(Node {
                token_id: 2.into(),
                expr: Shared::new(AstExpr::Let(
                    IdentWithToken::new("used_var"),
                    Shared::new(Node {
                        token_id: 3.into(),
                        expr: Shared::new(AstExpr::Literal(Literal::Number(20.0.into()))),
                    }),
                )),
            }),
            Shared::new(Node {
                token_id: 4.into(),
                expr: Shared::new(AstExpr::Literal(Literal::Number(20.0.into()))),
            }),
        ]
    )]
    #[case::dead_code_elimination_used_variable_kept(
        vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Let(
                    IdentWithToken::new("x"),
                    Shared::new(Node {
                        token_id: 1.into(),
                        expr: Shared::new(AstExpr::Literal(Literal::Number(5.0.into()))),
                    }),
                )),
            }),
            Shared::new(Node {
                token_id: 2.into(),
                expr: Shared::new(AstExpr::Ident(IdentWithToken::new("x"))),
            }),
        ],
        vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Let(
                    IdentWithToken::new("x"),
                    Shared::new(Node {
                        token_id: 1.into(),
                        expr: Shared::new(AstExpr::Literal(Literal::Number(5.0.into()))),
                    }),
                )),
            }),
            Shared::new(Node {
                token_id: 2.into(),
                expr: Shared::new(AstExpr::Literal(Literal::Number(5.0.into()))),
            }),
        ]
    )]
    #[case::dead_code_elimination_multiple_unused(
        vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Let(IdentWithToken::new("a"), Shared::new(Node { token_id: 1.into(), expr: Shared::new(AstExpr::Literal(Literal::Number(1.0.into()))) }))),
            }),
            Shared::new(Node {
                token_id: 2.into(),
                expr: Shared::new(AstExpr::Let(IdentWithToken::new("b"), Shared::new(Node { token_id: 3.into(), expr: Shared::new(AstExpr::Literal(Literal::Number(2.0.into()))) }))),
            }),
            Shared::new(Node {
                token_id: 4.into(),
                expr: Shared::new(AstExpr::Let(IdentWithToken::new("c"), Shared::new(Node { token_id: 5.into(), expr: Shared::new(AstExpr::Literal(Literal::Number(30.0.into()))) }))),
            }),
             Shared::new(Node {
                token_id: 6.into(),
                expr: Shared::new(AstExpr::Ident(IdentWithToken::new("c"))),
            }),
        ],
        vec![
            Shared::new(Node {
                token_id: 4.into(),
                expr: Shared::new(AstExpr::Let(IdentWithToken::new("c"), Shared::new(Node { token_id: 5.into(), expr: Shared::new(AstExpr::Literal(Literal::Number(30.0.into()))) }))),
            }),
             Shared::new(Node {
                token_id: 6.into(),
                expr: Shared::new(AstExpr::Literal(Literal::Number(30.0.into()))),
            }),
        ]
    )]
    #[case::dead_code_elimination_mixed_used_unused(
        vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Let(IdentWithToken::new("unused1"), Shared::new(Node { token_id: 1.into(), expr: Shared::new(AstExpr::Literal(Literal::Number(1.0.into()))) }))),
            }),
            Shared::new(Node {
                token_id: 2.into(),
                expr: Shared::new(AstExpr::Let(IdentWithToken::new("used1"), Shared::new(Node { token_id: 3.into(), expr: Shared::new(AstExpr::Literal(Literal::Number(10.0.into()))) }))),
            }),
            Shared::new(Node {
                token_id: 4.into(),
                expr: Shared::new(AstExpr::Let(IdentWithToken::new("unused2"), Shared::new(Node { token_id: 5.into(), expr: Shared::new(AstExpr::Literal(Literal::Number(2.0.into()))) }))),
            }),
            Shared::new(Node {
                token_id: 6.into(),
                expr: Shared::new(AstExpr::Ident(IdentWithToken::new("used1"))),
            }),
        ],
        vec![
            Shared::new(Node {
                token_id: 2.into(),
                expr: Shared::new(AstExpr::Let(IdentWithToken::new("used1"), Shared::new(Node { token_id: 3.into(), expr: Shared::new(AstExpr::Literal(Literal::Number(10.0.into()))) }))),
            }),
            Shared::new(Node {
                token_id: 6.into(),
                expr: Shared::new(AstExpr::Literal(Literal::Number(10.0.into()))),
            }),
        ]
    )]
    #[case::dead_code_elimination_unused_constant_candidate(
        vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Let(
                    IdentWithToken::new("const_unused"),
                    Shared::new(Node {
                        token_id: 1.into(),
                        expr: Shared::new(AstExpr::Literal(Literal::Number(100.0.into()))),
                    }),
                )),
            }),
            Shared::new(Node {
                token_id: 2.into(),
                expr: Shared::new(AstExpr::Let(
                    IdentWithToken::new("another_var"),
                    Shared::new(Node {
                        token_id: 3.into(),
                        expr: Shared::new(AstExpr::Literal(Literal::Number(200.0.into()))),
                    }),
                )),
            }),
            Shared::new(Node {
                token_id: 4.into(),
                expr: Shared::new(AstExpr::Ident(IdentWithToken::new("another_var"))),
            }),
        ],
        vec![
            Shared::new(Node {
                token_id: 2.into(),
                expr: Shared::new(AstExpr::Let(
                    IdentWithToken::new("another_var"),
                    Shared::new(Node {
                        token_id: 3.into(),
                        expr: Shared::new(AstExpr::Literal(Literal::Number(200.0.into()))),
                    }),
                )),
            }),
            Shared::new(Node {
                token_id: 4.into(),
                expr: Shared::new(AstExpr::Literal(Literal::Number(200.0.into()))),
            }),
        ]
    )]
    #[case::constant_folding_repeat(
        vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Call(
                    IdentWithToken::new("repeat"),
                    smallvec![
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Shared::new(AstExpr::Literal(Literal::String("ab".to_string()))),
                        }),
                        Shared::new(Node {
                            token_id: 2.into(),
                            expr: Shared::new(AstExpr::Literal(Literal::Number(3.0.into()))),
                        }),
                    ],
                )),
            }),
        ],
        vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Literal(Literal::String("ababab".to_string()))),
            }),
        ]
    )]
    #[case::constant_folding_reverse(
        vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Call(
                    IdentWithToken::new("reverse"),
                    smallvec![
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Shared::new(AstExpr::Literal(Literal::String("abc".to_string()))),
                        }),
                    ],
                )),
            }),
        ],
        vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Literal(Literal::String("cba".to_string()))),
            }),
        ]
    )]
    #[case::constant_folding_interpolated_string_expr(
        vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Let(
                    IdentWithToken::new("name"),
                    Shared::new(Node {
                        token_id: 1.into(),
                        expr: Shared::new(AstExpr::Literal(Literal::String("Alice".to_string()))),
                    }),
                )),
            }),
            Shared::new(Node {
                token_id: 2.into(),
                expr: Shared::new(AstExpr::InterpolatedString(vec![
                    ast::StringSegment::Text("Hello, ".to_string()),
                    ast::StringSegment::Expr(Shared::new(Node {
                        token_id: 3.into(),
                        expr: Shared::new(AstExpr::Ident(IdentWithToken::new("name"))),
                    })),
                    ast::StringSegment::Text("!".to_string()),
                ])),
            }),
        ],
        vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Let(
                    IdentWithToken::new("name"),
                    Shared::new(Node {
                        token_id: 1.into(),
                        expr: Shared::new(AstExpr::Literal(Literal::String("Alice".to_string()))),
                    }),
                )),
            }),
            Shared::new(Node {
                token_id: 2.into(),
                expr: Shared::new(AstExpr::InterpolatedString(vec![
                    ast::StringSegment::Text("Hello, ".to_string()),
                    ast::StringSegment::Expr(Shared::new(Node {
                        token_id: 3.into(),
                        expr: Shared::new(AstExpr::Literal(Literal::String("Alice".to_string()))),
                    })),
                    ast::StringSegment::Text("!".to_string()),
                ])),
            }),
        ]
    )]
    #[case::constant_folding_interpolated_string_multiple_exprs(
        vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Let(
                    IdentWithToken::new("first"),
                    Shared::new(Node {
                        token_id: 1.into(),
                        expr: Shared::new(AstExpr::Literal(Literal::String("Bob".to_string()))),
                    }),
                )),
            }),
            Shared::new(Node {
                token_id: 2.into(),
                expr: Shared::new(AstExpr::Let(
                    IdentWithToken::new("last"),
                    Shared::new(Node {
                        token_id: 3.into(),
                        expr: Shared::new(AstExpr::Literal(Literal::String("Smith".to_string()))),
                    }),
                )),
            }),
            Shared::new(Node {
                token_id: 4.into(),
                expr: Shared::new(AstExpr::InterpolatedString(vec![
                    ast::StringSegment::Text("Name: ".to_string()),
                    ast::StringSegment::Expr(Shared::new(Node {
                        token_id: 5.into(),
                        expr: Shared::new(AstExpr::Ident(IdentWithToken::new("first"))),
                    })),
                    ast::StringSegment::Text(" ".to_string()),
                    ast::StringSegment::Expr(Shared::new(Node {
                        token_id: 6.into(),
                        expr: Shared::new(AstExpr::Ident(IdentWithToken::new("last"))),
                    })),
                ])),
            }),
        ],
        vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Let(
                    IdentWithToken::new("first"),
                    Shared::new(Node {
                        token_id: 1.into(),
                        expr: Shared::new(AstExpr::Literal(Literal::String("Bob".to_string()))),
                    }),
                )),
            }),
            Shared::new(Node {
                token_id: 2.into(),
                expr: Shared::new(AstExpr::Let(
                    IdentWithToken::new("last"),
                    Shared::new(Node {
                        token_id: 3.into(),
                        expr: Shared::new(AstExpr::Literal(Literal::String("Smith".to_string()))),
                    }),
                )),
            }),
            Shared::new(Node {
                token_id: 4.into(),
                expr: Shared::new(AstExpr::InterpolatedString(vec![
                    ast::StringSegment::Text("Name: ".to_string()),
                    ast::StringSegment::Expr(Shared::new(Node {
                        token_id: 5.into(),
                        expr: Shared::new(AstExpr::Literal(Literal::String("Bob".to_string()))),
                    })),
                    ast::StringSegment::Text(" ".to_string()),
                    ast::StringSegment::Expr(Shared::new(Node {
                        token_id: 6.into(),
                        expr: Shared::new(AstExpr::Literal(Literal::String("Smith".to_string()))),
                    })),
                ])),
            }),
        ]
    )]
    #[case::constant_folding_interpolated_string_expr_non_const(
        vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Let(
                    IdentWithToken::new("dynamic"),
                    Shared::new(Node {
                        token_id: 1.into(),
                        expr: Shared::new(AstExpr::Call(
                            IdentWithToken::new("some_func"),
                            smallvec![],
                        )),
                    }),
                )),
            }),
            Shared::new(Node {
                token_id: 2.into(),
                expr: Shared::new(AstExpr::InterpolatedString(vec![
                    ast::StringSegment::Text("Value: ".to_string()),
                    ast::StringSegment::Expr(Shared::new(Node {
                        token_id: 3.into(),
                        expr: Shared::new(AstExpr::Ident(IdentWithToken::new("dynamic"))),
                    })),
                ])),
            }),
        ],
        vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Let(
                    IdentWithToken::new("dynamic"),
                    Shared::new(Node {
                        token_id: 1.into(),
                        expr: Shared::new(AstExpr::Call(
                            IdentWithToken::new("some_func"),
                            smallvec![],
                        )),
                    }),
                )),
            }),
            Shared::new(Node {
                token_id: 2.into(),
                expr: Shared::new(AstExpr::InterpolatedString(vec![
                    ast::StringSegment::Text("Value: ".to_string()),
                    ast::StringSegment::Expr(Shared::new(Node {
                        token_id: 3.into(),
                        expr: Shared::new(AstExpr::Ident(IdentWithToken::new("dynamic"))),
                    })),
                ])),
            }),
        ]
    )]
    fn test(#[case] input: Program, #[case] expected: Program) {
        let mut optimizer = Optimizer::default();
        let mut optimized_program = input.clone();
        optimizer.optimize(&mut optimized_program);
        assert_eq!(optimized_program, expected);

        // Additionally, for the unused constant candidate test, check constant_table
        if input.len() == 3 && expected.len() == 2 {
            // Heuristic for this specific test case
            if let AstExpr::Let(ident, _) = &*input[0].expr
                && ident.name.as_str() == "const_unused"
            {
                assert!(
                    !optimizer.constant_table.contains_key(&ident.name),
                    "const_unused should be removed from constant_table"
                );
            }
        }
    }

    #[test]
    fn test_optimization_level_none() {
        let mut optimizer = Optimizer::with_level(OptimizationLevel::None);

        let input = vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Let(
                    IdentWithToken::new("x"),
                    Shared::new(Node {
                        token_id: 0.into(),
                        expr: Shared::new(AstExpr::Literal(Literal::Number(5.0.into()))),
                    }),
                )),
            }),
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Call(
                    IdentWithToken::new("add"),
                    smallvec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Shared::new(AstExpr::Literal(Literal::Number(2.0.into()))),
                        }),
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Shared::new(AstExpr::Literal(Literal::Number(3.0.into()))),
                        }),
                    ],
                )),
            }),
        ];

        let mut optimized_program = input.clone();
        optimizer.optimize(&mut optimized_program);

        // No optimization should be applied
        assert_eq!(optimized_program, input);
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use crate::{Arena, DefaultEngine, SharedCell, strategies::*};
    use proptest::prelude::*;
    proptest! {
        #[test]
        fn test_optimization_idempotent(
            (expr_str, _expected) in arb_arithmetic_expr()
        ) {
            let token_arena = Shared::new(SharedCell::new(Arena::new(100)));

            let program = crate::parse(&expr_str, Shared::clone(&token_arena));
            prop_assume!(program.is_ok());

            let mut program = program.unwrap();

            let mut optimizer1 = Optimizer::default();
            optimizer1.optimize(&mut program);
            let first_optimized = program.clone();

            let mut optimizer2 = Optimizer::default();
            optimizer2.optimize(&mut program);
            let second_optimized = program;

            prop_assert_eq!(first_optimized, second_optimized, "Optimization should be idempotent");
        }

        /// Property: Optimization preserves semantics for constant folding
        /// The optimized and non-optimized versions should evaluate to the same value
        #[test]
        fn test_optimization_preserves_semantics_constant_folding(
            (expr_str, expected) in arb_arithmetic_expr()
        ) {
            let token_arena = Shared::new(SharedCell::new(Arena::new(100)));

            let program = crate::parse(&expr_str, Shared::clone(&token_arena));
            prop_assume!(program.is_ok());

            let mut engine = DefaultEngine::default();
            let result_no_opt = engine.eval_with_level(&expr_str, crate::null_input().into_iter(), OptimizationLevel::None);

            let result_opt = engine.eval_with_level(&expr_str, crate::null_input().into_iter(), OptimizationLevel::Full);

            prop_assert!(result_no_opt.is_ok(), "Non-optimized evaluation should succeed");
            prop_assert!(result_opt.is_ok(), "Optimized evaluation should succeed");

            let val_no_opt = &result_no_opt.unwrap()[0];
            let val_opt = &result_opt.unwrap()[0];

            prop_assert_eq!(
                val_no_opt,
                val_opt,
                "Expected value: {}", expected
            );
        }

        /// Property: Nested expressions are also optimized correctly
        #[test]
        fn test_optimization_preserves_semantics_nested(
            expr_str in arb_nested_arithmetic_expr()
        ) {
            let mut engine = DefaultEngine::default();
            let result_no_opt = engine.eval_with_level(&expr_str, crate::null_input().into_iter(), OptimizationLevel::None);

            let mut engine_opt = DefaultEngine::default();
            let result_opt = engine_opt.eval_with_level(&expr_str, crate::null_input().into_iter(), OptimizationLevel::Full);

            prop_assert!(result_no_opt.is_ok(), "Non-optimized evaluation should succeed");
            prop_assert!(result_opt.is_ok(), "Optimized evaluation should succeed");

            let val_no_opt = &result_no_opt.unwrap()[0];
            let val_opt = &result_opt.unwrap()[0];

            prop_assert_eq!(val_no_opt, val_opt);
        }

        /// Property: String concatenation optimization preserves semantics
        #[test]
        fn test_optimization_string_concat(
            expr_str in arb_string_concat_expr()
        ) {
            let mut engine = DefaultEngine::default();
            let result_no_opt = engine.eval_with_level(&expr_str, crate::null_input().into_iter(), OptimizationLevel::None);
            let result_opt = engine.eval_with_level(&expr_str, crate::null_input().into_iter(), OptimizationLevel::Full);

            prop_assert!(result_no_opt.is_ok());
            prop_assert!(result_opt.is_ok());

            let val_no_opt = &result_no_opt.unwrap()[0];
            let val_opt = &result_opt.unwrap()[0];

            prop_assert_eq!(val_no_opt, val_opt);
        }

        /// Property: Comparison expressions optimization preserves semantics
        #[test]
        fn test_optimization_comparison(
            expr_str in arb_comparison_expr()
        ) {
            let mut engine_no_opt = DefaultEngine::default();
            engine_no_opt.load_builtin_module();
            engine_no_opt.set_optimization_level(OptimizationLevel::None);
            let result_no_opt = engine_no_opt.eval(&expr_str, crate::null_input().into_iter());

            let mut engine_opt = DefaultEngine::default();
            engine_opt.load_builtin_module();
            engine_opt.set_optimization_level(OptimizationLevel::Full);
            let result_opt = engine_opt.eval(&expr_str, crate::null_input().into_iter());

            prop_assert!(result_no_opt.is_ok());
            prop_assert!(result_opt.is_ok());

            let val_no_opt = &result_no_opt.unwrap()[0];
            let val_opt = &result_opt.unwrap()[0];

            prop_assert_eq!(val_no_opt, val_opt);
        }

        /// Property: Logical expressions optimization preserves semantics
        #[test]
        fn test_optimization_logical(
            expr_str in arb_logical_expr()
        ) {
            let mut engine = DefaultEngine::default();
            let result_no_opt = engine.eval_with_level(&expr_str, crate::null_input().into_iter(), OptimizationLevel::None);
            let result_opt = engine.eval_with_level(&expr_str, crate::null_input().into_iter(), OptimizationLevel::Full);

            prop_assert!(result_no_opt.is_ok());
            prop_assert!(result_opt.is_ok());

            let val_no_opt = &result_no_opt.unwrap()[0];
            let val_opt = &result_opt.unwrap()[0];

            prop_assert_eq!(val_no_opt, val_opt);
        }

        /// Property: Division and modulo optimization preserves semantics
        #[test]
        fn test_optimization_div_mod(
            expr_str in arb_div_mod_expr()
        ) {
            let mut engine = DefaultEngine::default();
            let result_no_opt = engine.eval_with_level(&expr_str, crate::null_input().into_iter(), OptimizationLevel::None);
            let result_opt = engine.eval_with_level(&expr_str, crate::null_input().into_iter(), OptimizationLevel::Full);

            prop_assert!(result_no_opt.is_ok());
            prop_assert!(result_opt.is_ok());

            let val_no_opt = &result_no_opt.unwrap()[0];
            let val_opt = &result_opt.unwrap()[0];

            prop_assert_eq!(val_no_opt, val_opt);
        }

        /// Property: Let expressions optimization preserves semantics
        #[test]
        fn test_optimization_let_expr(
            expr_str in arb_let_expr()
        ) {
            let mut engine = DefaultEngine::default();
            let result_no_opt = engine.eval_with_level(&expr_str, crate::null_input().into_iter(), OptimizationLevel::None);
            let result_opt = engine.eval_with_level(&expr_str, crate::null_input().into_iter(), OptimizationLevel::Full);

            prop_assert!(result_no_opt.is_ok());
            prop_assert!(result_opt.is_ok());

            let val_no_opt = &result_no_opt.unwrap()[0];
            let val_opt = &result_opt.unwrap()[0];

            prop_assert_eq!(val_no_opt, val_opt);
        }

        /// Property: Deeply nested expressions optimization preserves semantics
        #[test]
        fn test_optimization_deeply_nested(
            expr_str in arb_deeply_nested_expr()
        ) {
            let mut engine = DefaultEngine::default();
            let result_no_opt = engine.eval_with_level(&expr_str, crate::null_input().into_iter(), OptimizationLevel::None);
            let result_opt = engine.eval_with_level(&expr_str, crate::null_input().into_iter(), OptimizationLevel::Full);

            prop_assert!(result_no_opt.is_ok());
            prop_assert!(result_opt.is_ok());

            let val_no_opt = &result_no_opt.unwrap()[0];
            let val_opt = &result_opt.unwrap()[0];

            prop_assert_eq!(val_no_opt, val_opt);
        }

        /// Property: Mixed type expressions optimization preserves semantics
        #[test]
        fn test_optimization_mixed_type(
            expr_str in arb_mixed_type_expr()
        ) {
            let mut engine = DefaultEngine::default();
            let result_no_opt = engine.eval_with_level(&expr_str, crate::null_input().into_iter(), OptimizationLevel::None);
            let result_opt = engine.eval_with_level(&expr_str, crate::null_input().into_iter(), OptimizationLevel::Full);

            prop_assert!(result_no_opt.is_ok());
            prop_assert!(result_opt.is_ok());

            let val_no_opt = &result_no_opt.unwrap()[0];
            let val_opt = &result_opt.unwrap()[0];

            prop_assert_eq!(val_no_opt, val_opt);
        }

        /// Property: Function definition and inlining preserves semantics
        #[test]
        fn test_optimization_function_def(
            expr_str in arb_function_def_expr()
        ) {
            let mut engine = DefaultEngine::default();
            let result_no_opt = engine.eval_with_level(&expr_str, crate::null_input().into_iter(), OptimizationLevel::None);
            let result_opt = engine.eval_with_level(&expr_str, crate::null_input().into_iter(), OptimizationLevel::Full);

            prop_assert!(result_no_opt.is_ok());
            prop_assert!(result_opt.is_ok());

            let val_no_opt = &result_no_opt.unwrap()[0];
            let val_opt = &result_opt.unwrap()[0];

            prop_assert_eq!(
                val_no_opt,
                val_opt
            );
        }

        /// Property: Complex expressions optimization preserves semantics
        #[test]
        fn test_optimization_complex(
            expr_str in arb_any_expr()
        ) {
            let mut engine = DefaultEngine::default();
            let result_no_opt = engine.eval_with_level(&expr_str, crate::null_input().into_iter(), OptimizationLevel::None);
            let result_opt = engine.eval_with_level(&expr_str, crate::null_input().into_iter(), OptimizationLevel::Full);

            prop_assert!(result_no_opt.is_ok());
            prop_assert!(result_opt.is_ok());

            let val_no_opt = &result_no_opt.unwrap()[0];
            let val_opt = &result_opt.unwrap()[0];

            prop_assert_eq!(
                val_no_opt,
                val_opt
            );
        }
    }

    /// Test for implicit first argument handling in function inlining
    #[test]
    fn test_inline_with_implicit_first_argument() {
        let query = r#"
def my_func(x):
  x
end
| 42 | my_func()
"#;

        let mut engine_no_opt = DefaultEngine::default();
        let result_no_opt =
            engine_no_opt.eval_with_level(query, crate::null_input().into_iter(), OptimizationLevel::None);

        let mut engine_opt = DefaultEngine::default();
        let result_opt = engine_opt.eval_with_level(query, crate::null_input().into_iter(), OptimizationLevel::Full);

        assert!(result_no_opt.is_ok(), "No optimization failed: {:?}", result_no_opt);
        assert!(result_opt.is_ok(), "Optimization failed: {:?}", result_opt);

        let val_no_opt = &result_no_opt.unwrap()[0];
        let val_opt = &result_opt.unwrap()[0];

        assert_eq!(
            val_no_opt, val_opt,
            "Results differ between optimized and non-optimized versions"
        );
    }

    /// Test for implicit first argument with builtin function call
    #[test]
    fn test_inline_with_implicit_arg_and_builtin() {
        let query = r#"
def my_split(x):
  split(x, "_")
end
| "hello_world" | my_split()
"#;

        let mut engine_no_opt = DefaultEngine::default();
        let result_no_opt =
            engine_no_opt.eval_with_level(query, crate::null_input().into_iter(), OptimizationLevel::None);

        let mut engine_opt = DefaultEngine::default();
        let result_opt = engine_opt.eval_with_level(query, crate::null_input().into_iter(), OptimizationLevel::Full);

        assert!(result_no_opt.is_ok(), "No optimization failed: {:?}", result_no_opt);
        assert!(result_opt.is_ok(), "Optimization failed: {:?}", result_opt);

        let val_no_opt = &result_no_opt.unwrap()[0];
        let val_opt = &result_opt.unwrap()[0];

        assert_eq!(
            val_no_opt, val_opt,
            "Results differ between optimized and non-optimized versions"
        );
    }
}
