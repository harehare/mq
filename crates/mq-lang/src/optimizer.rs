use super::ast::node as ast;
use crate::{Program, ast::IdentName};
use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet};
use std::rc::Rc;

type LineCount = usize;

#[derive(Debug, Default)]
pub struct Optimizer {
    constant_table: FxHashMap<ast::Ident, Rc<ast::Expr>>,
    function_table: FxHashMap<ast::Ident, (ast::Params, Program, LineCount)>,
    inline_threshold: LineCount,
}

impl Optimizer {
    pub fn new() -> Self {
        Self {
            constant_table: FxHashMap::with_capacity_and_hasher(100, FxBuildHasher),
            function_table: FxHashMap::with_capacity_and_hasher(50, FxBuildHasher),
            inline_threshold: 5,
        }
    }

    /// Creates a new optimizer with a custom inline threshold
    #[allow(dead_code)]
    pub fn with_inline_threshold(threshold: usize) -> Self {
        let mut optimizer = Self::new();
        optimizer.inline_threshold = threshold;
        optimizer
    }

    pub fn optimize(&mut self, program: &mut Program) {
        // First pass: collect function definitions for inlining
        self.collect_functions_for_inlining(program);

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

        self.inline_functions(program);

        for node in program {
            self.optimize_node(node);
        }
    }

    #[inline(always)]
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
            ast::Expr::Paren(node) => {
                Self::collect_used_identifiers_in_node(node, used_idents);
            }
            ast::Expr::Literal(_)
            | ast::Expr::Selector(_)
            | ast::Expr::Nodes
            | ast::Expr::Self_
            | ast::Expr::Include(_)
            | ast::Expr::Break
            | ast::Expr::Continue => {}
        }
    }

    /// Collects function definitions that are candidates for inlining
    fn collect_functions_for_inlining(&mut self, program: &Program) {
        for node in program {
            if let ast::Expr::Def(func_ident, params, body) = &*node.expr {
                let line_count = self.estimate_line_count(body);

                // Check if function is eligible for inlining:
                // 1. Not used in if/elif/else conditions
                // 2. Below line count threshold
                // 3. Not recursive
                if line_count < self.inline_threshold
                    && !Self::is_used_in_conditionals(func_ident, program)
                    && !Self::is_recursive_function(func_ident, body)
                {
                    self.function_table.insert(
                        func_ident.clone(),
                        (params.clone(), body.clone(), line_count),
                    );
                }
            }
        }
    }

    /// Estimates the number of lines a function body would take
    #[inline(always)]
    fn estimate_line_count(&self, program: &Program) -> usize {
        // Simple heuristic: each node represents approximately one line
        // This could be improved with actual range information
        program.len()
    }

    /// Checks if a function is used within if/elif/else conditions
    #[inline(always)]
    fn is_used_in_conditionals(func_name: &ast::Ident, program: &Program) -> bool {
        for node in program {
            if Self::check_conditional_usage_in_node(func_name, node) {
                return true;
            }
        }
        false
    }

    /// Recursively checks if a function is used in conditional contexts within a node
    fn check_conditional_usage_in_node(func_name: &ast::Ident, node: &Rc<ast::Node>) -> bool {
        match &*node.expr {
            ast::Expr::If(conditions) => {
                for (cond_node_opt, _) in conditions {
                    if let Some(cond_node) = cond_node_opt {
                        if Self::contains_function_call(func_name, cond_node) {
                            return true;
                        }
                    }
                }
            }
            ast::Expr::While(cond_node, body) | ast::Expr::Until(cond_node, body) => {
                if Self::contains_function_call(func_name, cond_node) {
                    return true;
                }
                for stmt in body {
                    if Self::check_conditional_usage_in_node(func_name, stmt) {
                        return true;
                    }
                }
            }
            ast::Expr::Def(_, _, body) | ast::Expr::Fn(_, body) => {
                for stmt in body {
                    if Self::check_conditional_usage_in_node(func_name, stmt) {
                        return true;
                    }
                }
            }
            ast::Expr::Foreach(_, collection_node, body) => {
                if Self::contains_function_call(func_name, collection_node) {
                    return true;
                }
                for stmt in body {
                    if Self::check_conditional_usage_in_node(func_name, stmt) {
                        return true;
                    }
                }
            }
            _ => {}
        }
        false
    }

    /// Checks if a function call exists within a node tree
    fn contains_function_call(func_name: &ast::Ident, node: &Rc<ast::Node>) -> bool {
        match &*node.expr {
            ast::Expr::Call(call_ident, args, _) => {
                if call_ident == func_name {
                    return true;
                }
                for arg in args {
                    if Self::contains_function_call(func_name, arg) {
                        return true;
                    }
                }
            }
            ast::Expr::Paren(inner_node) => {
                return Self::contains_function_call(func_name, inner_node);
            }
            ast::Expr::Let(_, value_node) => {
                return Self::contains_function_call(func_name, value_node);
            }
            // Add other recursive cases as needed
            _ => {}
        }
        false
    }

    #[inline(always)]
    fn is_recursive_function(func_name: &ast::Ident, body: &Program) -> bool {
        for node in body {
            if Self::contains_function_call(func_name, node) {
                return true;
            }
        }
        false
    }

    /// Applies function inlining to the program
    /// Efficiently applies function inlining to the program.
    fn inline_functions(&mut self, program: &mut Program) {
        let mut new_program = Vec::with_capacity(program.len());
        for node in program.drain(..) {
            let processed_node = self.inline_functions_in_node(node);
            if let ast::Expr::Call(func_ident, args, _) = &*processed_node.expr {
                if let Some((params, body, _)) = self.function_table.get(func_ident) {
                    // Create parameter bindings
                    let mut param_bindings = FxHashMap::default();
                    for (param, arg) in params.iter().zip(args.iter()) {
                        if let ast::Expr::Ident(param_ident) = &*param.expr {
                            param_bindings.insert(param_ident.clone(), arg.clone());
                        }
                    }
                    // Inline the function body with parameter substitution
                    for body_node in body {
                        let inlined_node = Self::substitute_parameters(body_node, &param_bindings);
                        new_program.push(inlined_node);
                    }
                    continue;
                }
            }
            new_program.push(processed_node);
        }
        *program = new_program;
    }

    /// Recursively applies function inlining within a node
    fn inline_functions_in_node(&mut self, node: Rc<ast::Node>) -> Rc<ast::Node> {
        let new_expr = match &*node.expr {
            ast::Expr::Def(ident, params, body) => {
                let mut new_body = body.clone();
                self.inline_functions(&mut new_body);
                Rc::new(ast::Expr::Def(ident.clone(), params.clone(), new_body))
            }
            ast::Expr::Fn(params, body) => {
                let mut new_body = body.clone();
                self.inline_functions(&mut new_body);
                Rc::new(ast::Expr::Fn(params.clone(), new_body))
            }
            ast::Expr::While(cond, body) => {
                let new_cond = self.inline_functions_in_node(cond.clone());
                let mut new_body = body.clone();
                self.inline_functions(&mut new_body);
                Rc::new(ast::Expr::While(new_cond, new_body))
            }
            ast::Expr::Until(cond, body) => {
                let new_cond = self.inline_functions_in_node(cond.clone());
                let mut new_body = body.clone();
                self.inline_functions(&mut new_body);
                Rc::new(ast::Expr::Until(new_cond, new_body))
            }
            ast::Expr::Foreach(ident, collection, body) => {
                let new_collection = self.inline_functions_in_node(Rc::clone(collection));
                let mut new_body = body.clone();
                self.inline_functions(&mut new_body);
                Rc::new(ast::Expr::Foreach(ident.clone(), new_collection, new_body))
            }
            ast::Expr::If(conditions) => {
                let new_conditions = conditions
                    .iter()
                    .map(|(cond_opt, body)| {
                        let new_cond = cond_opt
                            .as_ref()
                            .map(|cond| self.inline_functions_in_node(Rc::clone(cond)));
                        let new_body = self.inline_functions_in_node(Rc::clone(body));
                        (new_cond, new_body)
                    })
                    .collect();
                Rc::new(ast::Expr::If(new_conditions))
            }
            ast::Expr::Call(func_ident, args, optional) => {
                let new_args = args
                    .iter()
                    .map(|arg| self.inline_functions_in_node(Rc::clone(arg)))
                    .collect();
                Rc::new(ast::Expr::Call(func_ident.clone(), new_args, *optional))
            }
            ast::Expr::Let(ident, value) => {
                let new_value = self.inline_functions_in_node(Rc::clone(value));
                Rc::new(ast::Expr::Let(ident.clone(), new_value))
            }
            ast::Expr::Paren(inner) => {
                let new_inner = self.inline_functions_in_node(Rc::clone(inner));
                Rc::new(ast::Expr::Paren(new_inner))
            }
            _ => Rc::clone(&node.expr),
        };

        Rc::new(ast::Node {
            token_id: node.token_id,
            expr: new_expr,
        })
    }

    fn substitute_parameters(
        node: &Rc<ast::Node>,
        param_bindings: &FxHashMap<ast::Ident, Rc<ast::Node>>,
    ) -> Rc<ast::Node> {
        let new_expr = match &*node.expr {
            ast::Expr::Ident(ident) => {
                if let Some(arg_node) = param_bindings.get(ident) {
                    return arg_node.clone();
                }
                node.expr.clone()
            }
            ast::Expr::Call(func_ident, args, optional) => {
                let substituted_args = args
                    .iter()
                    .map(|arg| Self::substitute_parameters(arg, param_bindings))
                    .collect();
                Rc::new(ast::Expr::Call(
                    func_ident.clone(),
                    substituted_args,
                    *optional,
                ))
            }
            ast::Expr::Let(ident, value) => {
                let substituted_value = Self::substitute_parameters(value, param_bindings);
                Rc::new(ast::Expr::Let(ident.clone(), substituted_value))
            }
            ast::Expr::Paren(inner) => {
                let substituted_inner = Self::substitute_parameters(inner, param_bindings);
                Rc::new(ast::Expr::Paren(substituted_inner))
            }
            _ => node.expr.clone(),
        };

        Rc::new(ast::Node {
            token_id: node.token_id,
            expr: new_expr,
        })
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
            ast::Expr::Paren(expr) => {
                self.optimize_node(expr);
            }
            ast::Expr::InterpolatedString(segments) => {
                for segment in segments.iter_mut() {
                    if let ast::StringSegment::Ident(ident) = segment {
                        if let Some(expr) = self.constant_table.get(ident) {
                            if let ast::Expr::Literal(lit) = &**expr {
                                *segment = ast::StringSegment::Text(lit.to_string());
                            }
                        }
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
    #[case::constant_folding_interpolated_string_ident(
        vec![
            Rc::new(Node {
                token_id: 0.into(),
                expr: Rc::new(AstExpr::Let(
                    Ident::new("name"),
                    Rc::new(Node {
                        token_id: 1.into(),
                        expr: Rc::new(AstExpr::Literal(Literal::String("Alice".to_string()))),
                    }),
                )),
            }),
            Rc::new(Node {
                token_id: 2.into(),
                expr: Rc::new(AstExpr::InterpolatedString(vec![
                    ast::StringSegment::Text("Hello, ".to_string()),
                    ast::StringSegment::Ident(Ident::new("name")),
                    ast::StringSegment::Text("!".to_string()),
                ])),
            }),
        ],
        vec![
            Rc::new(Node {
                token_id: 0.into(),
                expr: Rc::new(AstExpr::Let(
                    Ident::new("name"),
                    Rc::new(Node {
                        token_id: 1.into(),
                        expr: Rc::new(AstExpr::Literal(Literal::String("Alice".to_string()))),
                    }),
                )),
            }),
            Rc::new(Node {
                token_id: 2.into(),
                expr: Rc::new(AstExpr::InterpolatedString(vec![
                    ast::StringSegment::Text("Hello, ".to_string()),
                    ast::StringSegment::Text("Alice".to_string()),
                    ast::StringSegment::Text("!".to_string()),
                ])),
            }),
        ]
    )]
    #[case::constant_folding_interpolated_string_multiple_idents(
        vec![
            Rc::new(Node {
                token_id: 0.into(),
                expr: Rc::new(AstExpr::Let(
                    Ident::new("first"),
                    Rc::new(Node {
                        token_id: 1.into(),
                        expr: Rc::new(AstExpr::Literal(Literal::String("Bob".to_string()))),
                    }),
                )),
            }),
            Rc::new(Node {
                token_id: 2.into(),
                expr: Rc::new(AstExpr::Let(
                    Ident::new("last"),
                    Rc::new(Node {
                        token_id: 3.into(),
                        expr: Rc::new(AstExpr::Literal(Literal::String("Smith".to_string()))),
                    }),
                )),
            }),
            Rc::new(Node {
                token_id: 4.into(),
                expr: Rc::new(AstExpr::InterpolatedString(vec![
                    ast::StringSegment::Text("Name: ".to_string()),
                    ast::StringSegment::Ident(Ident::new("first")),
                    ast::StringSegment::Text(" ".to_string()),
                    ast::StringSegment::Ident(Ident::new("last")),
                ])),
            }),
        ],
        vec![
            Rc::new(Node {
                token_id: 0.into(),
                expr: Rc::new(AstExpr::Let(
                    Ident::new("first"),
                    Rc::new(Node {
                        token_id: 1.into(),
                        expr: Rc::new(AstExpr::Literal(Literal::String("Bob".to_string()))),
                    }),
                )),
            }),
            Rc::new(Node {
                token_id: 2.into(),
                expr: Rc::new(AstExpr::Let(
                    Ident::new("last"),
                    Rc::new(Node {
                        token_id: 3.into(),
                        expr: Rc::new(AstExpr::Literal(Literal::String("Smith".to_string()))),
                    }),
                )),
            }),
            Rc::new(Node {
                token_id: 4.into(),
                expr: Rc::new(AstExpr::InterpolatedString(vec![
                    ast::StringSegment::Text("Name: ".to_string()),
                    ast::StringSegment::Text("Bob".to_string()),
                    ast::StringSegment::Text(" ".to_string()),
                    ast::StringSegment::Text("Smith".to_string()),
                ])),
            }),
        ]
    )]
    #[case::constant_folding_interpolated_string_ident_non_const(
        vec![
            Rc::new(Node {
                token_id: 0.into(),
                expr: Rc::new(AstExpr::Let(
                    Ident::new("dynamic"),
                    Rc::new(Node {
                        token_id: 1.into(),
                        expr: Rc::new(AstExpr::Call(
                            Ident::new("some_func"),
                            smallvec![],
                            false,
                        )),
                    }),
                )),
            }),
            Rc::new(Node {
                token_id: 2.into(),
                expr: Rc::new(AstExpr::InterpolatedString(vec![
                    ast::StringSegment::Text("Value: ".to_string()),
                    ast::StringSegment::Ident(Ident::new("dynamic")),
                ])),
            }),
        ],
        vec![
            Rc::new(Node {
                token_id: 0.into(),
                expr: Rc::new(AstExpr::Let(
                    Ident::new("dynamic"),
                    Rc::new(Node {
                        token_id: 1.into(),
                        expr: Rc::new(AstExpr::Call(
                            Ident::new("some_func"),
                            smallvec![],
                            false,
                        )),
                    }),
                )),
            }),
            Rc::new(Node {
                token_id: 2.into(),
                expr: Rc::new(AstExpr::InterpolatedString(vec![
                    ast::StringSegment::Text("Value: ".to_string()),
                    ast::StringSegment::Ident(Ident::new("dynamic")),
                ])),
            }),
        ]
    )]
    #[case::function_inlining_simple(
        vec![
            Rc::new(Node {
                token_id: 0.into(),
                expr: Rc::new(AstExpr::Def(
                    Ident::new("add_one"),
                    smallvec![
                        Rc::new(Node {
                            token_id: 0.into(),
                            expr: Rc::new(AstExpr::Ident(Ident::new("x"))),
                        })
                    ],
                    vec![
                        Rc::new(Node {
                            token_id: 0.into(),
                            expr: Rc::new(AstExpr::Call(
                                Ident::new("add"),
                                smallvec![
                                    Rc::new(Node {
                                        token_id: 0.into(),
                                        expr: Rc::new(AstExpr::Ident(Ident::new("x"))),
                                    }),
                                    Rc::new(Node {
                                        token_id: 0.into(),
                                        expr: Rc::new(AstExpr::Literal(Literal::Number(1.0.into()))),
                                    }),
                                ],
                                false,
                            )),
                        }),
                    ],
                )),
            }),
            Rc::new(Node {
                token_id: 0.into(),
                expr: Rc::new(AstExpr::Call(
                    Ident::new("add_one"),
                    smallvec![
                        Rc::new(Node {
                            token_id: 0.into(),
                            expr: Rc::new(AstExpr::Literal(Literal::Number(5.0.into()))),
                        })
                    ],
                    false,
                )),
            }),
        ],
        vec![
            Rc::new(Node {
                token_id: 0.into(),
                expr: Rc::new(AstExpr::Def(
                    Ident::new("add_one"),
                    smallvec![
                        Rc::new(Node {
                            token_id: 0.into(),
                            expr: Rc::new(AstExpr::Ident(Ident::new("x"))),
                        })
                    ],
                    vec![
                        Rc::new(Node {
                            token_id: 0.into(),
                            expr: Rc::new(AstExpr::Call(
                                Ident::new("add"),
                                smallvec![
                                    Rc::new(Node {
                                        token_id: 0.into(),
                                        expr: Rc::new(AstExpr::Ident(Ident::new("x"))),
                                    }),
                                    Rc::new(Node {
                                        token_id: 0.into(),
                                        expr: Rc::new(AstExpr::Literal(Literal::Number(1.0.into()))),
                                    }),
                                ],
                                false,
                            )),
                        }),
                    ],
                )),
            }),
            // Inlined function call
            Rc::new(Node {
                token_id: 0.into(),
                expr: Rc::new(AstExpr::Literal(Literal::Number(6.0.into()))),
            }),
        ]
    )]
    #[case::function_inlining_not_recursive(
        vec![
            Rc::new(Node {
                token_id: 0.into(),
                expr: Rc::new(AstExpr::Def(
                    Ident::new("square"),
                    smallvec![
                        Rc::new(Node {
                            token_id: 0.into(),
                            expr: Rc::new(AstExpr::Ident(Ident::new("n"))),
                        })
                    ],
                    vec![
                        Rc::new(Node {
                            token_id: 0.into(),
                            expr: Rc::new(AstExpr::Call(
                                Ident::new("mul"),
                                smallvec![
                                    Rc::new(Node {
                                        token_id: 0.into(),
                                        expr: Rc::new(AstExpr::Ident(Ident::new("n"))),
                                    }),
                                    Rc::new(Node {
                                        token_id: 0.into(),
                                        expr: Rc::new(AstExpr::Ident(Ident::new("n"))),
                                    }),
                                ],
                                false,
                            )),
                        }),
                    ],
                )),
            }),
            Rc::new(Node {
                token_id: 0.into(),
                expr: Rc::new(AstExpr::Call(
                    Ident::new("square"),
                    smallvec![
                        Rc::new(Node {
                            token_id: 0.into(),
                            expr: Rc::new(AstExpr::Literal(Literal::Number(3.0.into()))),
                        })
                    ],
                    false,
                )),
            }),
        ],
        vec![
            Rc::new(Node {
                token_id: 0.into(),
                expr: Rc::new(AstExpr::Def(
                    Ident::new("square"),
                    smallvec![
                        Rc::new(Node {
                            token_id: 0.into(),
                            expr: Rc::new(AstExpr::Ident(Ident::new("n"))),
                        })
                    ],
                    vec![
                        Rc::new(Node {
                            token_id: 0.into(),
                            expr: Rc::new(AstExpr::Call(
                                Ident::new("mul"),
                                smallvec![
                                    Rc::new(Node {
                                        token_id: 0.into(),
                                        expr: Rc::new(AstExpr::Ident(Ident::new("n"))),
                                    }),
                                    Rc::new(Node {
                                        token_id: 0.into(),
                                        expr: Rc::new(AstExpr::Ident(Ident::new("n"))),
                                    }),
                                ],
                                false,
                            )),
                        }),
                    ],
                )),
            }),
            // Inlined and optimized function call: 3 * 3 = 9
            Rc::new(Node {
                token_id: 0.into(),
                expr: Rc::new(AstExpr::Literal(Literal::Number(9.0.into()))),
            }),
        ]
    )]
    #[case::function_not_inlined_recursive(
        vec![
            Rc::new(Node {
                token_id: 0.into(),
                expr: Rc::new(AstExpr::Def(
                    Ident::new("factorial"),
                    smallvec![
                        Rc::new(Node {
                            token_id: 0.into(),
                            expr: Rc::new(AstExpr::Ident(Ident::new("n"))),
                        })
                    ],
                    vec![
                        Rc::new(Node {
                            token_id: 0.into(),
                            expr: Rc::new(AstExpr::Call(
                                Ident::new("factorial"),
                                smallvec![
                                    Rc::new(Node {
                                        token_id: 0.into(),
                                        expr: Rc::new(AstExpr::Call(
                                            Ident::new("sub"),
                                            smallvec![
                                                Rc::new(Node {
                                                    token_id: 0.into(),
                                                    expr: Rc::new(AstExpr::Ident(Ident::new("n"))),
                                                }),
                                                Rc::new(Node {
                                                    token_id: 0.into(),
                                                    expr: Rc::new(AstExpr::Literal(Literal::Number(1.0.into()))),
                                                }),
                                            ],
                                            false,
                                        )),
                                    })
                                ],
                                false,
                            )),
                        }),
                    ],
                )),
            }),
            Rc::new(Node {
                token_id: 0.into(),
                expr: Rc::new(AstExpr::Call(
                    Ident::new("factorial"),
                    smallvec![
                        Rc::new(Node {
                            token_id: 0.into(),
                            expr: Rc::new(AstExpr::Literal(Literal::Number(5.0.into()))),
                        })
                    ],
                    false,
                )),
            }),
        ],
        // Should not be inlined because it's recursive
        vec![
            Rc::new(Node {
                token_id: 0.into(),
                expr: Rc::new(AstExpr::Def(
                    Ident::new("factorial"),
                    smallvec![
                        Rc::new(Node {
                            token_id: 0.into(),
                            expr: Rc::new(AstExpr::Ident(Ident::new("n"))),
                        })
                    ],
                    vec![
                        Rc::new(Node {
                            token_id: 0.into(),
                            expr: Rc::new(AstExpr::Call(
                                Ident::new("factorial"),
                                smallvec![
                                    Rc::new(Node {
                                        token_id: 0.into(),
                                        expr: Rc::new(AstExpr::Call(
                                            Ident::new("sub"),
                                            smallvec![
                                                Rc::new(Node {
                                                    token_id: 0.into(),
                                                    expr: Rc::new(AstExpr::Ident(Ident::new("n"))),
                                                }),
                                                Rc::new(Node {
                                                    token_id: 0.into(),
                                                    expr: Rc::new(AstExpr::Literal(Literal::Number(1.0.into()))),
                                                }),
                                            ],
                                            false,
                                        )),
                                    })
                                ],
                                false,
                            )),
                        }),
                    ],
                )),
            }),
            Rc::new(Node {
                token_id: 0.into(),
                expr: Rc::new(AstExpr::Call(
                    Ident::new("factorial"),
                    smallvec![
                        Rc::new(Node {
                            token_id: 0.into(),
                            expr: Rc::new(AstExpr::Literal(Literal::Number(5.0.into()))),
                        })
                    ],
                    false,
                )),
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

    #[test]
    fn test_inlining_with_custom_threshold() {
        let mut optimizer = Optimizer::with_inline_threshold(1);

        let input = vec![
            Rc::new(Node {
                token_id: 0.into(),
                expr: Rc::new(AstExpr::Def(
                    Ident::new("long_func"),
                    smallvec![],
                    vec![
                        Rc::new(Node {
                            token_id: 0.into(),
                            expr: Rc::new(AstExpr::Literal(Literal::Number(1.0.into()))),
                        }),
                        Rc::new(Node {
                            token_id: 0.into(),
                            expr: Rc::new(AstExpr::Literal(Literal::Number(2.0.into()))),
                        }),
                    ],
                )),
            }),
            Rc::new(Node {
                token_id: 0.into(),
                expr: Rc::new(AstExpr::Call(Ident::new("long_func"), smallvec![], false)),
            }),
        ];

        let mut optimized_program = input.clone();
        optimizer.optimize(&mut optimized_program);

        // Function should not be inlined because it exceeds the threshold
        assert_eq!(optimized_program, input);
    }
}
