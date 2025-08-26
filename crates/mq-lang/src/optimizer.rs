use super::ast::node as ast;
use crate::{Program, ast::IdentName};
use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet};
use smallvec::smallvec;
use std::rc::Rc;

#[derive(Debug, Clone)]
struct InlineContext {
    in_conditional: bool,
}

#[derive(Debug, Default)]
pub struct Optimizer {
    constant_table: FxHashMap<ast::Ident, Rc<ast::Expr>>,
    function_table: FxHashMap<ast::Ident, (ast::Params, Program)>,
    recursive_functions: FxHashSet<ast::Ident>,
    max_inline_lines: usize,
    // No need to store used_identifiers here if it's collected and used per optimize call.
}

impl Optimizer {
    pub fn new() -> Self {
        Self::new_with_inline_limit(10) // Default max inline lines
    }

    pub fn new_with_inline_limit(max_inline_lines: usize) -> Self {
        Self {
            constant_table: FxHashMap::with_capacity_and_hasher(100, FxBuildHasher),
            function_table: FxHashMap::with_capacity_and_hasher(50, FxBuildHasher),
            recursive_functions: FxHashSet::default(),
            max_inline_lines,
        }
    }

    pub fn optimize(&mut self, program: &mut Program) {
        // First pass: collect function definitions and detect recursion
        self.collect_function_definitions(program);
        self.detect_recursive_functions(program);

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

        let context = InlineContext {
            in_conditional: false,
        };
        for node in program {
            self.optimize_node_with_context(node, &context);
        }
    }

    fn collect_function_definitions(&mut self, program: &Program) {
        for node in program {
            self.collect_function_definitions_in_node(node);
        }
    }

    fn collect_function_definitions_in_node(&mut self, node: &Rc<ast::Node>) {
        match &*node.expr {
            ast::Expr::Def(ident, params, body) => {
                self.function_table
                    .insert(ident.clone(), (params.clone(), body.clone()));
                // Also recursively collect from the function body
                for stmt in body {
                    self.collect_function_definitions_in_node(stmt);
                }
            }
            ast::Expr::Fn(_, program_nodes)
            | ast::Expr::While(_, program_nodes)
            | ast::Expr::Until(_, program_nodes) => {
                for stmt in program_nodes {
                    self.collect_function_definitions_in_node(stmt);
                }
            }
            ast::Expr::Foreach(_, _, program_nodes) => {
                for stmt in program_nodes {
                    self.collect_function_definitions_in_node(stmt);
                }
            }
            ast::Expr::If(conditions) => {
                for (_, body_node) in conditions {
                    self.collect_function_definitions_in_node(body_node);
                }
            }
            _ => {}
        }
    }

    fn detect_recursive_functions(&mut self, _program: &Program) {
        for (func_name, (_, body)) in &self.function_table.clone() {
            if self.is_function_recursive(func_name, body) {
                self.recursive_functions.insert(func_name.clone());
            }
        }
    }

    fn is_function_recursive(&self, func_name: &ast::Ident, body: &Program) -> bool {
        for node in body {
            if Self::contains_recursive_call(func_name, node) {
                return true;
            }
        }
        false
    }

    fn contains_recursive_call(func_name: &ast::Ident, node: &Rc<ast::Node>) -> bool {
        match &*node.expr {
            ast::Expr::Call(ident, args, _) => {
                if ident == func_name {
                    return true;
                }
                for arg in args {
                    if Self::contains_recursive_call(func_name, arg) {
                        return true;
                    }
                }
                false
            }
            ast::Expr::Def(_, _, program_nodes)
            | ast::Expr::Fn(_, program_nodes)
            | ast::Expr::While(_, program_nodes)
            | ast::Expr::Until(_, program_nodes) => {
                for stmt in program_nodes {
                    if Self::contains_recursive_call(func_name, stmt) {
                        return true;
                    }
                }
                false
            }
            ast::Expr::Foreach(_, collection, program_nodes) => {
                if Self::contains_recursive_call(func_name, collection) {
                    return true;
                }
                for stmt in program_nodes {
                    if Self::contains_recursive_call(func_name, stmt) {
                        return true;
                    }
                }
                false
            }
            ast::Expr::If(conditions) => {
                for (cond_opt, body_node) in conditions {
                    if let Some(cond) = cond_opt {
                        if Self::contains_recursive_call(func_name, cond) {
                            return true;
                        }
                    }
                    if Self::contains_recursive_call(func_name, body_node) {
                        return true;
                    }
                }
                false
            }
            ast::Expr::Let(_, value) | ast::Expr::Paren(value) => {
                Self::contains_recursive_call(func_name, value)
            }
            _ => false,
        }
    }

    fn can_inline_function(&self, func_name: &ast::Ident, context: &InlineContext) -> bool {
        // Check if function is not used in if/elif/else blocks
        if context.in_conditional {
            return false;
        }

        // Check if function is not recursive
        if self.recursive_functions.contains(func_name) {
            return false;
        }

        // Check line count limit
        if let Some((_, body)) = self.function_table.get(func_name) {
            let line_count = self.count_lines_in_program(body);
            if line_count >= self.max_inline_lines {
                return false;
            }
        }

        true
    }

    fn count_lines_in_program(&self, program: &Program) -> usize {
        program
            .iter()
            .map(Self::count_lines_in_node)
            .sum()
    }

    fn count_lines_in_node(node: &Rc<ast::Node>) -> usize {
        match &*node.expr {
            ast::Expr::Def(_, _, program_nodes)
            | ast::Expr::Fn(_, program_nodes)
            | ast::Expr::While(_, program_nodes)
            | ast::Expr::Until(_, program_nodes) => {
                program_nodes
                    .iter()
                    .map(Self::count_lines_in_node)
                    .sum::<usize>()
                    + 1
            }
            ast::Expr::Foreach(_, _, program_nodes) => {
                program_nodes
                    .iter()
                    .map(Self::count_lines_in_node)
                    .sum::<usize>()
                    + 1
            }
            ast::Expr::If(conditions) => {
                conditions
                    .iter()
                    .map(|(_, body)| Self::count_lines_in_node(body))
                    .sum::<usize>()
                    + 1
            }
            _ => 1,
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

    fn optimize_node_with_context(&mut self, node: &mut Rc<ast::Node>, context: &InlineContext) {
        let mut_node = Rc::make_mut(node);
        let mut_expr = Rc::make_mut(&mut mut_node.expr);

        match mut_expr {
            ast::Expr::Call(ident, args, _optional) => {
                for arg in args.iter_mut() {
                    self.optimize_node_with_context(arg, context);
                }

                // Try to inline function calls
                if self.can_inline_function(ident, context) {
                    if let Some((params, body)) = self.function_table.get(ident).cloned() {
                        if params.len() == args.len() {
                            if let Some(inlined_body) =
                                self.inline_function_call(&params, &body, args)
                            {
                                match inlined_body.len() {
                                    0 => {
                                        // Empty function body
                                        mut_node.expr =
                                            Rc::new(ast::Expr::Literal(ast::Literal::None));
                                        return;
                                    }
                                    1 => {
                                        // Single expression - inline directly
                                        mut_node.expr = Rc::clone(&inlined_body[0].expr);
                                        return;
                                    }
                                    _ => {
                                        // Multiple statements - create a function expression
                                        mut_node.expr =
                                            Rc::new(ast::Expr::Fn(smallvec![], inlined_body));
                                        return;
                                    }
                                }
                            }
                        }
                    }
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
                self.optimize_node_with_context(each_values, context);
                for node in program {
                    self.optimize_node_with_context(node, context);
                }
            }
            ast::Expr::If(conditions) => {
                let conditional_context = InlineContext {
                    in_conditional: true,
                };
                for (cond, expr) in conditions.iter_mut() {
                    if let Some(c) = cond {
                        self.optimize_node_with_context(c, &conditional_context);
                    }
                    self.optimize_node_with_context(expr, &conditional_context);
                }
            }
            ast::Expr::Let(ident, value) => {
                self.optimize_node_with_context(value, context);
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
                    self.optimize_node_with_context(node, context);
                }
            }
            ast::Expr::Paren(expr) => {
                self.optimize_node_with_context(expr, context);
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

    fn inline_function_call(
        &self,
        params: &ast::Params,
        body: &Program,
        args: &ast::Args,
    ) -> Option<Program> {
        if body.is_empty() {
            // Empty function body - return empty program
            return Some(vec![]);
        }

        // Clone the function body and substitute parameters
        let mut inlined_body = body.clone();
        for node in &mut inlined_body {
            Self::substitute_parameters(node, params, args);
        }

        Some(inlined_body)
    }

    fn substitute_parameters(node: &mut Rc<ast::Node>, params: &ast::Params, args: &ast::Args) {
        let mut_node = Rc::make_mut(node);
        let mut_expr = Rc::make_mut(&mut mut_node.expr);

        match mut_expr {
            ast::Expr::Ident(ident) => {
                // Find parameter and replace with argument
                for (i, param) in params.iter().enumerate() {
                    if let ast::Expr::Ident(param_ident) = &*param.expr {
                        if param_ident.name == ident.name {
                            if let Some(arg) = args.get(i) {
                                // Clone the entire argument node to preserve structure
                                *node = Rc::clone(arg);
                            }
                            return;
                        }
                    }
                }
            }
            ast::Expr::Call(_, call_args, _) => {
                for arg in call_args.iter_mut() {
                    Self::substitute_parameters(arg, params, args);
                }
            }
            ast::Expr::Let(_, value) | ast::Expr::Paren(value) => {
                Self::substitute_parameters(value, params, args);
            }
            ast::Expr::Def(_, _, program)
            | ast::Expr::Fn(_, program)
            | ast::Expr::While(_, program)
            | ast::Expr::Until(_, program) => {
                for stmt in program.iter_mut() {
                    Self::substitute_parameters(stmt, params, args);
                }
            }
            ast::Expr::Foreach(_, collection, program) => {
                Self::substitute_parameters(collection, params, args);
                for stmt in program.iter_mut() {
                    Self::substitute_parameters(stmt, params, args);
                }
            }
            ast::Expr::If(conditions) => {
                for (cond_opt, body) in conditions.iter_mut() {
                    if let Some(cond) = cond_opt {
                        Self::substitute_parameters(cond, params, args);
                    }
                    Self::substitute_parameters(body, params, args);
                }
            }
            ast::Expr::InterpolatedString(segments) => {
                for segment in segments.iter_mut() {
                    if let ast::StringSegment::Ident(ident) = segment {
                        for (i, param) in params.iter().enumerate() {
                            if let ast::Expr::Ident(param_ident) = &*param.expr {
                                if param_ident.name == ident.name {
                                    if let Some(arg) = args.get(i) {
                                        if let ast::Expr::Literal(lit) = &*arg.expr {
                                            *segment = ast::StringSegment::Text(lit.to_string());
                                        }
                                    }
                                    break;
                                }
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
                    smallvec![Rc::new(Node {
                        token_id: 1.into(),
                        expr: Rc::new(AstExpr::Ident(Ident::new("x"))),
                    })],
                    vec![Rc::new(Node {
                        token_id: 2.into(),
                        expr: Rc::new(AstExpr::Call(
                            Ident::new("add"),
                            smallvec![
                                Rc::new(Node {
                                    token_id: 3.into(),
                                    expr: Rc::new(AstExpr::Ident(Ident::new("x"))),
                                }),
                                Rc::new(Node {
                                    token_id: 4.into(),
                                    expr: Rc::new(AstExpr::Literal(Literal::Number(1.0.into()))),
                                }),
                            ],
                            false,
                        )),
                    })],
                )),
            }),
            Rc::new(Node {
                token_id: 5.into(),
                expr: Rc::new(AstExpr::Call(
                    Ident::new("add_one"),
                    smallvec![Rc::new(Node {
                        token_id: 6.into(),
                        expr: Rc::new(AstExpr::Literal(Literal::Number(5.0.into()))),
                    })],
                    false,
                )),
            }),
        ],
        vec![
            Rc::new(Node {
                token_id: 0.into(),
                expr: Rc::new(AstExpr::Def(
                    Ident::new("add_one"),
                    smallvec![Rc::new(Node {
                        token_id: 1.into(),
                        expr: Rc::new(AstExpr::Ident(Ident::new("x"))),
                    })],
                    vec![Rc::new(Node {
                        token_id: 2.into(),
                        expr: Rc::new(AstExpr::Call(
                            Ident::new("add"),
                            smallvec![
                                Rc::new(Node {
                                    token_id: 3.into(),
                                    expr: Rc::new(AstExpr::Ident(Ident::new("x"))),
                                }),
                                Rc::new(Node {
                                    token_id: 4.into(),
                                    expr: Rc::new(AstExpr::Literal(Literal::Number(1.0.into()))),
                                }),
                            ],
                            false,
                        )),
                    })],
                )),
            }),
            Rc::new(Node {
                token_id: 5.into(),
                expr: Rc::new(AstExpr::Call(
                    Ident::new("add"),
                    smallvec![
                        Rc::new(Node {
                            token_id: 6.into(),
                            expr: Rc::new(AstExpr::Literal(Literal::Number(5.0.into()))),
                        }),
                        Rc::new(Node {
                            token_id: 4.into(),
                            expr: Rc::new(AstExpr::Literal(Literal::Number(1.0.into()))),
                        }),
                    ],
                    false,
                )),
            }),
        ]
    )]
    #[case::function_inlining_not_in_conditional(
        vec![
            Rc::new(Node {
                token_id: 0.into(),
                expr: Rc::new(AstExpr::Def(
                    Ident::new("get_value"),
                    smallvec![],
                    vec![Rc::new(Node {
                        token_id: 1.into(),
                        expr: Rc::new(AstExpr::Literal(Literal::Number(42.0.into()))),
                    })],
                )),
            }),
            Rc::new(Node {
                token_id: 2.into(),
                expr: Rc::new(AstExpr::If(smallvec![(
                    Some(Rc::new(Node {
                        token_id: 3.into(),
                        expr: Rc::new(AstExpr::Literal(Literal::Bool(true))),
                    })),
                    Rc::new(Node {
                        token_id: 4.into(),
                        expr: Rc::new(AstExpr::Call(
                            Ident::new("get_value"),
                            smallvec![],
                            false,
                        )),
                    })
                )])),
            }),
        ],
        vec![
            Rc::new(Node {
                token_id: 0.into(),
                expr: Rc::new(AstExpr::Def(
                    Ident::new("get_value"),
                    smallvec![],
                    vec![Rc::new(Node {
                        token_id: 1.into(),
                        expr: Rc::new(AstExpr::Literal(Literal::Number(42.0.into()))),
                    })],
                )),
            }),
            Rc::new(Node {
                token_id: 2.into(),
                expr: Rc::new(AstExpr::If(smallvec![(
                    Some(Rc::new(Node {
                        token_id: 3.into(),
                        expr: Rc::new(AstExpr::Literal(Literal::Bool(true))),
                    })),
                    Rc::new(Node {
                        token_id: 4.into(),
                        expr: Rc::new(AstExpr::Call(
                            Ident::new("get_value"),
                            smallvec![],
                            false,
                        )),
                    })
                )])),
            }),
        ]
    )]
    #[case::function_inlining_multiline(
        vec![
            Rc::new(Node {
                token_id: 0.into(),
                expr: Rc::new(AstExpr::Def(
                    Ident::new("multi_line_func"),
                    smallvec![Rc::new(Node {
                        token_id: 1.into(),
                        expr: Rc::new(AstExpr::Ident(Ident::new("x"))),
                    })],
                    vec![
                        Rc::new(Node {
                            token_id: 2.into(),
                            expr: Rc::new(AstExpr::Let(
                                Ident::new("temp"),
                                Rc::new(Node {
                                    token_id: 3.into(),
                                    expr: Rc::new(AstExpr::Call(
                                        Ident::new("add"),
                                        smallvec![
                                            Rc::new(Node {
                                                token_id: 4.into(),
                                                expr: Rc::new(AstExpr::Ident(Ident::new("x"))),
                                            }),
                                            Rc::new(Node {
                                                token_id: 5.into(),
                                                expr: Rc::new(AstExpr::Literal(Literal::Number(1.0.into()))),
                                            }),
                                        ],
                                        false,
                                    )),
                                }),
                            )),
                        }),
                        Rc::new(Node {
                            token_id: 6.into(),
                            expr: Rc::new(AstExpr::Call(
                                Ident::new("mul"),
                                smallvec![
                                    Rc::new(Node {
                                        token_id: 7.into(),
                                        expr: Rc::new(AstExpr::Ident(Ident::new("temp"))),
                                    }),
                                    Rc::new(Node {
                                        token_id: 8.into(),
                                        expr: Rc::new(AstExpr::Literal(Literal::Number(2.0.into()))),
                                    }),
                                ],
                                false,
                            )),
                        }),
                    ],
                )),
            }),
            Rc::new(Node {
                token_id: 9.into(),
                expr: Rc::new(AstExpr::Call(
                    Ident::new("multi_line_func"),
                    smallvec![Rc::new(Node {
                        token_id: 10.into(),
                        expr: Rc::new(AstExpr::Literal(Literal::Number(5.0.into()))),
                    })],
                    false,
                )),
            }),
        ],
        vec![
            Rc::new(Node {
                token_id: 0.into(),
                expr: Rc::new(AstExpr::Def(
                    Ident::new("multi_line_func"),
                    smallvec![Rc::new(Node {
                        token_id: 1.into(),
                        expr: Rc::new(AstExpr::Ident(Ident::new("x"))),
                    })],
                    vec![
                        Rc::new(Node {
                            token_id: 2.into(),
                            expr: Rc::new(AstExpr::Let(
                                Ident::new("temp"),
                                Rc::new(Node {
                                    token_id: 3.into(),
                                    expr: Rc::new(AstExpr::Call(
                                        Ident::new("add"),
                                        smallvec![
                                            Rc::new(Node {
                                                token_id: 4.into(),
                                                expr: Rc::new(AstExpr::Ident(Ident::new("x"))),
                                            }),
                                            Rc::new(Node {
                                                token_id: 5.into(),
                                                expr: Rc::new(AstExpr::Literal(Literal::Number(1.0.into()))),
                                            }),
                                        ],
                                        false,
                                    )),
                                }),
                            )),
                        }),
                        Rc::new(Node {
                            token_id: 6.into(),
                            expr: Rc::new(AstExpr::Call(
                                Ident::new("mul"),
                                smallvec![
                                    Rc::new(Node {
                                        token_id: 7.into(),
                                        expr: Rc::new(AstExpr::Ident(Ident::new("temp"))),
                                    }),
                                    Rc::new(Node {
                                        token_id: 8.into(),
                                        expr: Rc::new(AstExpr::Literal(Literal::Number(2.0.into()))),
                                    }),
                                ],
                                false,
                            )),
                        }),
                    ],
                )),
            }),
            Rc::new(Node {
                token_id: 9.into(),
                expr: Rc::new(AstExpr::Fn(
                    smallvec![],
                    vec![
                        Rc::new(Node {
                            token_id: 2.into(),
                            expr: Rc::new(AstExpr::Let(
                                Ident::new("temp"),
                                Rc::new(Node {
                                    token_id: 3.into(),
                                    expr: Rc::new(AstExpr::Call(
                                        Ident::new("add"),
                                        smallvec![
                                            Rc::new(Node {
                                                token_id: 10.into(),
                                                expr: Rc::new(AstExpr::Literal(Literal::Number(5.0.into()))),
                                            }),
                                            Rc::new(Node {
                                                token_id: 5.into(),
                                                expr: Rc::new(AstExpr::Literal(Literal::Number(1.0.into()))),
                                            }),
                                        ],
                                        false,
                                    )),
                                }),
                            )),
                        }),
                        Rc::new(Node {
                            token_id: 6.into(),
                            expr: Rc::new(AstExpr::Call(
                                Ident::new("mul"),
                                smallvec![
                                    Rc::new(Node {
                                        token_id: 7.into(),
                                        expr: Rc::new(AstExpr::Ident(Ident::new("temp"))),
                                    }),
                                    Rc::new(Node {
                                        token_id: 8.into(),
                                        expr: Rc::new(AstExpr::Literal(Literal::Number(2.0.into()))),
                                    }),
                                ],
                                false,
                            )),
                        }),
                    ],
                )),
            }),
        ]
    )]
    #[case::function_inlining_recursive_not_inlined(
        vec![
            Rc::new(Node {
                token_id: 0.into(),
                expr: Rc::new(AstExpr::Def(
                    Ident::new("factorial"),
                    smallvec![Rc::new(Node {
                        token_id: 1.into(),
                        expr: Rc::new(AstExpr::Ident(Ident::new("n"))),
                    })],
                    vec![Rc::new(Node {
                        token_id: 2.into(),
                        expr: Rc::new(AstExpr::Call(
                            Ident::new("factorial"),
                            smallvec![Rc::new(Node {
                                token_id: 3.into(),
                                expr: Rc::new(AstExpr::Ident(Ident::new("n"))),
                            })],
                            false,
                        )),
                    })],
                )),
            }),
            Rc::new(Node {
                token_id: 4.into(),
                expr: Rc::new(AstExpr::Call(
                    Ident::new("factorial"),
                    smallvec![Rc::new(Node {
                        token_id: 5.into(),
                        expr: Rc::new(AstExpr::Literal(Literal::Number(5.0.into()))),
                    })],
                    false,
                )),
            }),
        ],
        vec![
            Rc::new(Node {
                token_id: 0.into(),
                expr: Rc::new(AstExpr::Def(
                    Ident::new("factorial"),
                    smallvec![Rc::new(Node {
                        token_id: 1.into(),
                        expr: Rc::new(AstExpr::Ident(Ident::new("n"))),
                    })],
                    vec![Rc::new(Node {
                        token_id: 2.into(),
                        expr: Rc::new(AstExpr::Call(
                            Ident::new("factorial"),
                            smallvec![Rc::new(Node {
                                token_id: 3.into(),
                                expr: Rc::new(AstExpr::Ident(Ident::new("n"))),
                            })],
                            false,
                        )),
                    })],
                )),
            }),
            Rc::new(Node {
                token_id: 4.into(),
                expr: Rc::new(AstExpr::Call(
                    Ident::new("factorial"),
                    smallvec![Rc::new(Node {
                        token_id: 5.into(),
                        expr: Rc::new(AstExpr::Literal(Literal::Number(5.0.into()))),
                    })],
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
    fn test_function_inlining_line_limit() {
        let input = vec![
            Rc::new(Node {
                token_id: 0.into(),
                expr: Rc::new(AstExpr::Def(
                    Ident::new("large_function"),
                    smallvec![],
                    // Create a function with many statements (more than default limit)
                    (0..15)
                        .map(|i| {
                            Rc::new(Node {
                                token_id: (i + 1).into(),
                                expr: Rc::new(AstExpr::Literal(Literal::Number((i as f64).into()))),
                            })
                        })
                        .collect(),
                )),
            }),
            Rc::new(Node {
                token_id: 16.into(),
                expr: Rc::new(AstExpr::Call(
                    Ident::new("large_function"),
                    smallvec![],
                    false,
                )),
            }),
        ];

        let expected = input.clone(); // Should not be inlined due to line limit

        let mut optimizer = Optimizer::new_with_inline_limit(10);
        let mut optimized_program = input;
        optimizer.optimize(&mut optimized_program);
        assert_eq!(optimized_program, expected);
    }

    #[test]
    fn test_function_inlining_within_line_limit() {
        let input = vec![
            Rc::new(Node {
                token_id: 0.into(),
                expr: Rc::new(AstExpr::Def(
                    Ident::new("small_function"),
                    smallvec![],
                    vec![Rc::new(Node {
                        token_id: 1.into(),
                        expr: Rc::new(AstExpr::Literal(Literal::Number(42.0.into()))),
                    })],
                )),
            }),
            Rc::new(Node {
                token_id: 2.into(),
                expr: Rc::new(AstExpr::Call(
                    Ident::new("small_function"),
                    smallvec![],
                    false,
                )),
            }),
        ];

        let expected = vec![
            Rc::new(Node {
                token_id: 0.into(),
                expr: Rc::new(AstExpr::Def(
                    Ident::new("small_function"),
                    smallvec![],
                    vec![Rc::new(Node {
                        token_id: 1.into(),
                        expr: Rc::new(AstExpr::Literal(Literal::Number(42.0.into()))),
                    })],
                )),
            }),
            Rc::new(Node {
                token_id: 2.into(),
                expr: Rc::new(AstExpr::Literal(Literal::Number(42.0.into()))),
            }),
        ];

        let mut optimizer = Optimizer::new_with_inline_limit(10);
        let mut optimized_program = input;
        optimizer.optimize(&mut optimized_program);
        assert_eq!(optimized_program, expected);
    }
}
