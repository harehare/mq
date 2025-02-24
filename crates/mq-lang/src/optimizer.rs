use std::rc::Rc;

use compact_str::CompactString;
use itertools::Itertools;
use rustc_hash::FxHashMap;

use crate::Program;

use super::ast::node as ast;

#[derive(Debug, Default)]
pub struct Optimizer {
    constant_table: FxHashMap<ast::Ident, Rc<ast::Expr>>,
}

impl Optimizer {
    pub fn new() -> Self {
        Self {
            constant_table: FxHashMap::default(),
        }
    }

    pub fn optimize(&mut self, program: &Program) -> Program {
        program
            .iter()
            .map(|node| self.optimize_node(Rc::clone(node)))
            .collect_vec()
    }

    #[inline(always)]
    fn optimize_node(&mut self, node: Rc<ast::Node>) -> Rc<ast::Node> {
        match &*node.expr {
            ast::Expr::Call(ident, args, optional) => {
                let args = args
                    .iter()
                    .map(|arg| self.optimize_node(Rc::clone(arg)))
                    .collect::<Vec<_>>();

                if ident.name == CompactString::new("add") {
                    match args.as_slice() {
                        [arg1, arg2] => {
                            if let (
                                ast::Expr::Literal(ast::Literal::Number(a)),
                                ast::Expr::Literal(ast::Literal::Number(b)),
                            ) = (&*arg1.expr, &*arg2.expr)
                            {
                                Rc::new(ast::Node {
                                    token_id: node.token_id,
                                    expr: Rc::new(ast::Expr::Literal(ast::Literal::Number(
                                        *a + *b,
                                    ))),
                                })
                            } else if let (
                                ast::Expr::Literal(ast::Literal::String(a)),
                                ast::Expr::Literal(ast::Literal::String(b)),
                            ) = (&*arg1.expr, &*arg2.expr)
                            {
                                Rc::new(ast::Node {
                                    token_id: node.token_id,
                                    expr: Rc::new(ast::Expr::Literal(ast::Literal::String(
                                        format!("{}{}", a, b),
                                    ))),
                                })
                            } else {
                                Rc::clone(&node)
                            }
                        }
                        _ => Rc::clone(&node),
                    }
                } else if ident.name == CompactString::new("sub") {
                    match args.as_slice() {
                        [arg1, arg2] => {
                            if let (
                                ast::Expr::Literal(ast::Literal::Number(a)),
                                ast::Expr::Literal(ast::Literal::Number(b)),
                            ) = (&*arg1.expr, &*arg2.expr)
                            {
                                Rc::new(ast::Node {
                                    token_id: node.token_id,
                                    expr: Rc::new(ast::Expr::Literal(ast::Literal::Number(
                                        *a - *b,
                                    ))),
                                })
                            } else {
                                Rc::clone(&node)
                            }
                        }
                        _ => Rc::clone(&node),
                    }
                } else if ident.name == CompactString::new("div") {
                    match args.as_slice() {
                        [arg1, arg2] => {
                            if let (
                                ast::Expr::Literal(ast::Literal::Number(a)),
                                ast::Expr::Literal(ast::Literal::Number(b)),
                            ) = (&*arg1.expr, &*arg2.expr)
                            {
                                Rc::new(ast::Node {
                                    token_id: node.token_id,
                                    expr: Rc::new(ast::Expr::Literal(ast::Literal::Number(
                                        *a / *b,
                                    ))),
                                })
                            } else {
                                Rc::clone(&node)
                            }
                        }
                        _ => Rc::clone(&node),
                    }
                } else if ident.name == CompactString::new("mul") {
                    match args.as_slice() {
                        [arg1, arg2] => {
                            if let (
                                ast::Expr::Literal(ast::Literal::Number(a)),
                                ast::Expr::Literal(ast::Literal::Number(b)),
                            ) = (&*arg1.expr, &*arg2.expr)
                            {
                                Rc::new(ast::Node {
                                    token_id: node.token_id,
                                    expr: Rc::new(ast::Expr::Literal(ast::Literal::Number(
                                        *a * *b,
                                    ))),
                                })
                            } else {
                                Rc::clone(&node)
                            }
                        }
                        _ => Rc::clone(&node),
                    }
                } else if ident.name == CompactString::new("mod") {
                    match args.as_slice() {
                        [arg1, arg2] => {
                            if let (
                                ast::Expr::Literal(ast::Literal::Number(a)),
                                ast::Expr::Literal(ast::Literal::Number(b)),
                            ) = (&*arg1.expr, &*arg2.expr)
                            {
                                Rc::new(ast::Node {
                                    token_id: node.token_id,
                                    expr: Rc::new(ast::Expr::Literal(ast::Literal::Number(
                                        *a % *b,
                                    ))),
                                })
                            } else {
                                Rc::clone(&node)
                            }
                        }
                        _ => Rc::clone(&node),
                    }
                } else {
                    Rc::new(ast::Node {
                        token_id: node.token_id,
                        expr: Rc::new(ast::Expr::Call(ident.clone(), args, *optional)),
                    })
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
                    .collect_vec();

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
                    .collect_vec();

                Rc::new(ast::Node {
                    token_id: node.token_id,
                    expr: Rc::new(ast::Expr::If(conditions)),
                })
            }
            ast::Expr::Let(ident, value) => match &*value.expr {
                ast::Expr::Literal(_) => {
                    self.constant_table
                        .insert(ident.clone(), Rc::clone(&value.expr));
                    Rc::clone(&node)
                }
                _ => Rc::clone(&node),
            },
            ast::Expr::Def(ident, params, program) => {
                let params = params.iter().map(Rc::clone).collect_vec();
                let program = program
                    .iter()
                    .map(|node| self.optimize_node(Rc::clone(node)))
                    .collect_vec();

                Rc::new(ast::Node {
                    token_id: node.token_id,
                    expr: Rc::new(ast::Expr::Def(ident.clone(), params, program)),
                })
            }
            ast::Expr::While(cond, program) => {
                let program = program
                    .iter()
                    .map(|node| self.optimize_node(Rc::clone(node)))
                    .collect_vec();

                Rc::new(ast::Node {
                    token_id: node.token_id,
                    expr: Rc::new(ast::Expr::While(Rc::clone(cond), program)),
                })
            }
            ast::Expr::Until(cond, program) => {
                let program = program
                    .iter()
                    .map(|node| self.optimize_node(Rc::clone(node)))
                    .collect_vec();

                Rc::new(ast::Node {
                    token_id: node.token_id,
                    expr: Rc::new(ast::Expr::Until(Rc::clone(cond), program)),
                })
            }
            ast::Expr::Selector(_)
            | ast::Expr::Include(_)
            | ast::Expr::Literal(_)
            | ast::Expr::Self_ => Rc::clone(&node),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Program;
    use rstest::rstest;

    #[rstest]
    #[case::constant_folding_add(
            vec![
                Rc::new(ast::Node {
                    token_id: 0.into(),
                    expr: Rc::new(ast::Expr::Call(
                        ast::Ident::new("add"),
                        vec![
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
    #[case::constant_folding_sub(
            vec![
                Rc::new(ast::Node {
                    token_id: 0.into(),
                    expr: Rc::new(ast::Expr::Call(
                        ast::Ident::new("sub"),
                        vec![
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
                        vec![
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
                        vec![
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
                        vec![
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
    fn test(#[case] input: Program, #[case] expected: Program) {
        assert_eq!(Optimizer::new().optimize(&input), expected);
    }
}
