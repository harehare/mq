use std::{cell::RefCell, rc::Rc};

use crate::{
    AstIdentName, Program, Token, TokenKind,
    arena::Arena,
    ast::node::{self as ast, Args},
    error::InnerError,
};

pub mod builtin;
pub mod env;
pub mod error;
pub mod module;
pub mod runtime_value;

use env::Env;
use error::EvalError;
use runtime_value::RuntimeValue;

#[derive(Debug, Clone)]
pub struct Options {
    pub filter_none: bool,
    pub max_call_stack_depth: u32,
}

#[cfg(debug_assertions)]
impl Default for Options {
    fn default() -> Self {
        Self {
            filter_none: true,
            max_call_stack_depth: 32,
        }
    }
}

#[cfg(not(debug_assertions))]
impl Default for Options {
    fn default() -> Self {
        Self {
            filter_none: true,
            max_call_stack_depth: 192,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Evaluator {
    env: Rc<RefCell<Env>>,
    token_arena: Rc<RefCell<Arena<Rc<Token>>>>,
    call_stack_depth: u32,
    pub(crate) options: Options,
    pub(crate) module_loader: module::ModuleLoader,
}

impl Evaluator {
    pub(crate) fn new(
        module_loader: module::ModuleLoader,
        token_arena: Rc<RefCell<Arena<Rc<Token>>>>,
    ) -> Self {
        Self {
            env: Rc::new(RefCell::new(Env::default())),
            module_loader,
            call_stack_depth: 0,
            token_arena,
            options: Options::default(),
        }
    }

    pub(crate) fn eval<I>(
        &mut self,
        program: &Program,
        input: I,
    ) -> Result<Vec<RuntimeValue>, InnerError>
    where
        I: Iterator<Item = RuntimeValue>,
    {
        let program = program.iter().try_fold(
            Vec::with_capacity(program.len()),
            |mut nodes: Vec<Rc<ast::Node>>, node: &Rc<ast::Node>| -> Result<_, InnerError> {
                match &*node.expr {
                    ast::Expr::Def(ident, params, program) => {
                        self.env.borrow_mut().define(
                            ident,
                            RuntimeValue::Function(
                                params.clone(),
                                program.clone(),
                                Rc::clone(&self.env),
                            ),
                        );
                    }
                    ast::Expr::Include(module_id) => {
                        self.eval_include(module_id.to_owned())?;
                    }
                    _ => nodes.push(Rc::clone(node)),
                };

                Ok(nodes)
            },
        )?;

        input
            .map(|runtime_value| {
                self.eval_program(&program, runtime_value, Rc::clone(&self.env))
                    .map_err(InnerError::Eval)
            })
            .collect()
    }

    pub(crate) fn defined_runtime_values(&self) -> Vec<(AstIdentName, RuntimeValue)> {
        self.env.borrow().defined_runtime_values()
    }

    pub fn define_string_value(&self, name: &str, value: &str) {
        self.env.borrow_mut().define(
            &ast::Ident::new(name),
            RuntimeValue::String(value.to_string()),
        );
    }

    pub(crate) fn load_builtin_module(&mut self) -> Result<(), EvalError> {
        let module = self
            .module_loader
            .load_builtin(Rc::clone(&self.token_arena))
            .map_err(EvalError::ModuleLoadError)?;
        self.load_module(module)
    }

    pub(crate) fn load_module(&mut self, module: Option<module::Module>) -> Result<(), EvalError> {
        if let Some(module) = module {
            module.modules.iter().for_each(|node| {
                if let ast::Expr::Def(ident, params, program) = &*node.expr {
                    self.env.borrow_mut().define(
                        ident,
                        RuntimeValue::Function(
                            params.clone(),
                            program.clone(),
                            Rc::clone(&self.env),
                        ),
                    );
                }
            });

            module.vars.iter().try_for_each(|node| {
                if let ast::Expr::Let(ident, node) = &*node.expr {
                    let val =
                        self.eval_expr(&RuntimeValue::NONE, Rc::clone(node), Rc::clone(&self.env))?;
                    self.env.borrow_mut().define(ident, val);
                    Ok(())
                } else {
                    Err(EvalError::InternalError(
                        (*self.token_arena.borrow()[node.token_id]).clone(),
                    ))
                }
            })
        } else {
            Ok(())
        }
    }

    fn eval_program(
        &mut self,
        program: &Program,
        runtime_value: RuntimeValue,
        env: Rc<RefCell<Env>>,
    ) -> Result<RuntimeValue, EvalError> {
        program
            .iter()
            .try_fold(runtime_value, |runtime_value, expr| {
                if self.options.filter_none && runtime_value.is_none() {
                    return Ok(RuntimeValue::NONE);
                }

                self.eval_expr(&runtime_value, Rc::clone(expr), Rc::clone(&env))
            })
    }

    fn eval_ident(
        &self,
        ident: &ast::Ident,
        node: Rc<ast::Node>,
        env: Rc<RefCell<Env>>,
    ) -> Result<RuntimeValue, EvalError> {
        env.borrow()
            .resolve(ident)
            .map_err(|e| e.to_eval_error((*node).clone(), Rc::clone(&self.token_arena)))
    }

    fn eval_include(&mut self, module: ast::Literal) -> Result<(), EvalError> {
        match module {
            ast::Literal::String(module_name) => {
                let module = self
                    .module_loader
                    .load_from_file(&module_name, Rc::clone(&self.token_arena))
                    .map_err(EvalError::ModuleLoadError)?;
                self.load_module(module)
            }
            _ => Err(EvalError::ModuleLoadError(
                module::ModuleError::InvalidModule,
            )),
        }
    }

    fn eval_selector_expr(runtime_value: RuntimeValue, ident: &ast::Selector) -> RuntimeValue {
        match &runtime_value {
            RuntimeValue::Markdown(node_value, _) => {
                if builtin::eval_selector(node_value, ident) {
                    runtime_value
                } else {
                    RuntimeValue::NONE
                }
            }
            _ => RuntimeValue::NONE,
        }
    }

    fn eval_interpolated_string(
        &self,
        node: Rc<ast::Node>,
        env: Rc<RefCell<Env>>,
    ) -> Result<RuntimeValue, EvalError> {
        if let ast::Expr::InterpolatedString(segments) = &*node.expr {
            segments
                .iter()
                .try_fold(String::new(), |mut acc, segment| {
                    match segment {
                        ast::StringSegment::Text(s) => acc.push_str(s),
                        ast::StringSegment::Ident(ident) => {
                            let value =
                                self.eval_ident(ident, Rc::clone(&node), Rc::clone(&env))?;
                            acc.push_str(&value.to_string());
                        }
                    }

                    Ok(acc)
                })
                .map(|acc| acc.into())
        } else {
            unreachable!()
        }
    }

    fn eval_expr(
        &mut self,
        runtime_value: &RuntimeValue,
        node: Rc<ast::Node>,
        env: Rc<RefCell<Env>>,
    ) -> Result<RuntimeValue, EvalError> {
        match &*node.expr {
            ast::Expr::Self_ => Ok(runtime_value.clone()),
            ast::Expr::Include(module_id) => {
                self.eval_include(module_id.to_owned())?;
                Ok(runtime_value.clone())
            }
            ast::Expr::Literal(ast::Literal::None) => Ok(RuntimeValue::None),
            ast::Expr::Literal(ast::Literal::Bool(b)) => Ok(RuntimeValue::Bool(*b)),
            ast::Expr::Literal(ast::Literal::String(s)) => Ok(RuntimeValue::String(s.to_string())),
            ast::Expr::Literal(ast::Literal::Number(n)) => Ok(RuntimeValue::Number(*n)),
            ast::Expr::Call(ident, args, optional) => {
                self.eval_fn(runtime_value, Rc::clone(&node), ident, args, *optional, env)
            }
            ast::Expr::Ident(ident) => self.eval_ident(ident, Rc::clone(&node), Rc::clone(&env)),
            ast::Expr::Selector(ident) => {
                Ok(Self::eval_selector_expr(runtime_value.clone(), ident))
            }
            ast::Expr::Def(ident, params, program) => {
                let function =
                    RuntimeValue::Function(params.clone(), program.clone(), Rc::clone(&env));
                env.borrow_mut().define(ident, function.clone());
                Ok(function)
            }
            ast::Expr::Let(ident, node) => {
                let let_ = self.eval_expr(runtime_value, Rc::clone(node), Rc::clone(&env))?;
                env.borrow_mut().define(ident, let_);
                Ok(runtime_value.clone())
            }
            ast::Expr::While(_, _) => self.eval_while(runtime_value, node, env),
            ast::Expr::Until(_, _) => self.eval_until(runtime_value, node, env),
            ast::Expr::Foreach(_, _, _) => self.eval_foreach(runtime_value, node, env),
            ast::Expr::If(_) => self.eval_if(runtime_value, node, env),
            ast::Expr::InterpolatedString(_) => self.eval_interpolated_string(node, env),
        }
    }

    fn eval_foreach(
        &mut self,
        runtime_value: &RuntimeValue,
        node: Rc<ast::Node>,
        env: Rc<RefCell<Env>>,
    ) -> Result<RuntimeValue, EvalError> {
        if let ast::Expr::Foreach(ident, values, body) = &*node.expr {
            let values = self.eval_expr(runtime_value, Rc::clone(values), Rc::clone(&env))?;
            let values = if let RuntimeValue::Array(values) = values {
                let runtime_values: Vec<RuntimeValue> = Vec::with_capacity(values.len());
                let env = Rc::new(RefCell::new(Env::with_parent(Rc::downgrade(&env))));

                values
                    .into_iter()
                    .try_fold(runtime_values, |mut acc, value| {
                        env.borrow_mut().define(ident, value);
                        let result =
                            self.eval_program(body, runtime_value.clone(), Rc::clone(&env))?;
                        acc.push(result);
                        Ok::<Vec<RuntimeValue>, EvalError>(acc)
                    })?
            } else {
                return Err(EvalError::InvalidTypes {
                    token: (*self.token_arena.borrow()[node.token_id]).clone(),
                    name: TokenKind::Foreach.to_string(),
                    args: vec![values.to_string().into()],
                });
            };

            Ok(RuntimeValue::Array(values))
        } else {
            unreachable!()
        }
    }

    fn eval_until(
        &mut self,
        runtime_value: &RuntimeValue,
        node: Rc<ast::Node>,
        env: Rc<RefCell<Env>>,
    ) -> Result<RuntimeValue, EvalError> {
        if let ast::Expr::Until(cond, body) = &*node.expr {
            let mut runtime_value = runtime_value.clone();
            let env = Rc::new(RefCell::new(Env::with_parent(Rc::downgrade(&env))));
            let mut cond_value =
                self.eval_expr(&runtime_value, Rc::clone(cond), Rc::clone(&env))?;

            while cond_value.is_true() {
                runtime_value = self.eval_program(body, runtime_value, Rc::clone(&env))?;
                cond_value = self.eval_expr(&runtime_value, Rc::clone(cond), Rc::clone(&env))?;
            }

            Ok(runtime_value)
        } else {
            unreachable!()
        }
    }

    fn eval_while(
        &mut self,
        runtime_value: &RuntimeValue,
        node: Rc<ast::Node>,
        env: Rc<RefCell<Env>>,
    ) -> Result<RuntimeValue, EvalError> {
        if let ast::Expr::While(cond, body) = &*node.expr {
            let mut runtime_value = runtime_value.clone();
            let env = Rc::new(RefCell::new(Env::with_parent(Rc::downgrade(&env))));
            let mut cond_value =
                self.eval_expr(&runtime_value, Rc::clone(cond), Rc::clone(&env))?;
            let mut values = Vec::with_capacity(100_000);

            while cond_value.is_true() {
                runtime_value = self.eval_program(body, runtime_value, Rc::clone(&env))?;
                cond_value = self.eval_expr(&runtime_value, Rc::clone(cond), Rc::clone(&env))?;
                values.push(runtime_value.clone());
            }

            Ok(RuntimeValue::Array(values))
        } else {
            unreachable!()
        }
    }

    fn eval_if(
        &mut self,
        runtime_value: &RuntimeValue,
        node: Rc<ast::Node>,
        env: Rc<RefCell<Env>>,
    ) -> Result<RuntimeValue, EvalError> {
        if let ast::Expr::If(conditions) = &*node.expr {
            for (cond_node, body) in conditions {
                match cond_node {
                    Some(cond_node) => {
                        let cond =
                            self.eval_expr(runtime_value, Rc::clone(cond_node), Rc::clone(&env))?;

                        if cond.is_true() {
                            return self.eval_expr(runtime_value, Rc::clone(body), env);
                        }
                    }
                    None => return self.eval_expr(runtime_value, Rc::clone(body), env),
                }
            }

            Ok(RuntimeValue::NONE)
        } else {
            unreachable!()
        }
    }

    fn eval_fn(
        &mut self,
        runtime_value: &RuntimeValue,
        node: Rc<ast::Node>,
        ident: &ast::Ident,
        args: &Args,
        optional: bool,
        env: Rc<RefCell<Env>>,
    ) -> Result<RuntimeValue, EvalError> {
        if runtime_value.is_none() && optional {
            return Ok(RuntimeValue::NONE);
        }

        if let Ok(fn_value) = Rc::clone(&env).borrow().resolve(ident) {
            if let RuntimeValue::Function(params, program, fn_env) = &fn_value {
                self.enter_scope()?;

                let mut args = args.to_owned();

                if params.len() == args.len() + 1 {
                    args.insert(
                        0,
                        Rc::new(ast::Node {
                            token_id: node.token_id,
                            expr: Rc::new(ast::Expr::Self_),
                        }),
                    );
                } else if args.len() != params.len() {
                    return Err(EvalError::InvalidNumberOfArguments(
                        (*self.token_arena.borrow()[node.token_id]).clone(),
                        ident.to_string(),
                        params.len() as u8,
                        args.len() as u8,
                    ));
                }

                let new_env = Rc::new(RefCell::new(Env::with_parent(Rc::downgrade(fn_env))));

                args.iter()
                    .zip(params.iter())
                    .try_for_each(|(arg, param)| {
                        if let ast::Expr::Ident(name) = &*param.expr {
                            let value =
                                self.eval_expr(runtime_value, Rc::clone(arg), Rc::clone(&env))?;

                            new_env.borrow_mut().define(name, value);
                            Ok(())
                        } else {
                            Err(EvalError::InvalidDefinition(
                                (*self.token_arena.borrow()[param.token_id]).clone(),
                                ident.to_string(),
                            ))
                        }
                    })?;

                let result = self.eval_program(program, runtime_value.clone(), new_env);

                self.exit_scope();
                result
            } else if let RuntimeValue::NativeFunction(ident) = fn_value {
                self.eval_builtin(runtime_value, node, &ident, args, env)
            } else {
                Err(EvalError::InvalidDefinition(
                    (*self.token_arena.borrow()[node.token_id]).clone(),
                    ident.to_string(),
                ))
            }
        } else {
            self.eval_builtin(runtime_value, node, ident, args, env)
        }
    }

    fn eval_builtin(
        &mut self,
        runtime_value: &RuntimeValue,
        node: Rc<ast::Node>,
        ident: &ast::Ident,
        args: &Args,
        env: Rc<RefCell<Env>>,
    ) -> Result<RuntimeValue, EvalError> {
        let args: Result<Vec<RuntimeValue>, EvalError> = args
            .iter()
            .map(|arg| self.eval_expr(runtime_value, Rc::clone(arg), Rc::clone(&env)))
            .collect();
        builtin::eval_builtin(runtime_value, ident, &args?)
            .map_err(|e| e.to_eval_error((*node).clone(), Rc::clone(&self.token_arena)))
    }

    fn enter_scope(&mut self) -> Result<(), EvalError> {
        if self.call_stack_depth >= self.options.max_call_stack_depth {
            return Err(EvalError::RecursionError(self.options.max_call_stack_depth));
        }
        self.call_stack_depth += 1;
        Ok(())
    }

    fn exit_scope(&mut self) {
        if self.call_stack_depth > 0 {
            self.call_stack_depth -= 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::range::Range;
    use crate::{AstExpr, AstNode, ModuleLoader};
    use crate::{Token, TokenKind};

    use super::*;
    use mq_test::defer;
    use rstest::{fixture, rstest};

    #[fixture]
    fn token_arena() -> Rc<RefCell<Arena<Rc<Token>>>> {
        let token_arena = Rc::new(RefCell::new(Arena::new(10)));

        token_arena.borrow_mut().alloc(Rc::new(Token {
            kind: TokenKind::Eof,
            range: Range::default(),
            module_id: 1.into(),
        }));

        token_arena
    }

    fn ast_node(expr: AstExpr) -> Rc<AstNode> {
        Rc::new(AstNode {
            token_id: 0.into(),
            expr: Rc::new(expr),
        })
    }

    fn ast_call(name: &str, args: Vec<Rc<AstNode>>) -> Rc<AstNode> {
        Rc::new(AstNode {
            token_id: 0.into(),
            expr: Rc::new(ast::Expr::Call(ast::Ident::new(name), args, false)),
        })
    }

    #[rstest]
    #[case::starts_with(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("starts_with", vec![ast_node(ast::Expr::Literal(ast::Literal::String("te".to_string())))])
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::starts_with(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "test".to_string(), position: None}), None)],
       vec![
            ast_call("starts_with", vec![ast_node(ast::Expr::Literal(ast::Literal::String("te".to_string())))])
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::starts_with(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("starts_with", vec![ast_node(ast::Expr::Literal(ast::Literal::String("st".to_string())))])
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::starts_with(vec![RuntimeValue::Array(vec!["start".to_string().into(), "end".to_string().into()])],
       vec![
            ast_call("starts_with", vec![
                ast_node(ast::Expr::Literal(ast::Literal::String("start".to_string())))
            ])
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::starts_with(vec![RuntimeValue::Array(vec!["start".to_string().into(), "end".to_string().into()])],
       vec![
            ast_call("starts_with", vec![
                ast_node(ast::Expr::Literal(ast::Literal::String("end".to_string())))
            ])
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::starts_with(vec![RuntimeValue::Number(1.into())],
       vec![
            ast_call("starts_with", vec![
                ast_node(ast::Expr::Literal(ast::Literal::String("end".to_string())))
            ])
       ],
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                    name: "starts_with".to_string(),
                                                    args: vec!["1".into(), "end".to_string().into()]})))]
    #[case::ends_with(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("ends_with", vec![
                ast_node(ast::Expr::Literal(ast::Literal::String("st".to_string())))
            ])
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::ends_with(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "test".to_string(), position: None}), None)],
       vec![
            ast_call("ends_with", vec![
                ast_node(ast::Expr::Literal(ast::Literal::String("st".to_string())))
            ])
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::ends_with(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("ends_with", vec![
                ast_node(ast::Expr::Literal(ast::Literal::String("te".to_string())))
            ])
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::ends_with(vec![RuntimeValue::Array(vec!["start".to_string().into(), "end".to_string().into()])],
       vec![
            ast_call("ends_with", vec![
                ast_node(ast::Expr::Literal(ast::Literal::String("end".to_string())))
            ])
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::ends_with(vec![RuntimeValue::Array(vec!["start".to_string().into(), "end".to_string().into()])],
       vec![
            ast_call("ends_with", vec![
                ast_node(ast::Expr::Literal(ast::Literal::String("start".to_string())))
            ])
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::ends_with(vec![RuntimeValue::Number(1.into())],
       vec![
            ast_call("ends_with", vec![
                ast_node(ast::Expr::Literal(ast::Literal::String("te".to_string())))
            ])
       ],
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                    name: "ends_with".to_string(),
                                                    args: vec![1.to_string().into(), "te".into()]})))]
    #[case::downcase(vec![RuntimeValue::String("TEST".to_string())],
       vec![ast_call("downcase", Vec::new())],
       Ok(vec![RuntimeValue::String("test".to_string())]))]
    #[case::downcase(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "TEST".to_string(), position: None}), None)],
       vec![ast_call("downcase", Vec::new())],
       Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "test".to_string(), position: None}), None)]))]
    #[case::upcase(vec![RuntimeValue::String("test".to_string())],
       vec![ast_call("upcase", Vec::new())],
       Ok(vec![RuntimeValue::String("TEST".to_string())]))]
    #[case::upcase(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "test".to_string(), position: None}), None)],
       vec![ast_call("upcase", Vec::new())],
       Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "TEST".to_string(), position: None}), None)]))]
    #[case::upcase(vec![RuntimeValue::NONE],
       vec![ast_call("upcase", Vec::new())],
       Ok(vec![RuntimeValue::NONE]))]
    #[case::upcase(vec![RuntimeValue::Number(123.into())],
       vec![ast_call("upcase", Vec::new())],
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                    name: "upcase".to_string(),
                                                    args: vec![123.to_string().into()]})))]
    #[case::replace(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("replace", vec![
                ast_node(ast::Expr::Literal(ast::Literal::String("test".to_string()))),
                ast_node(ast::Expr::Literal(ast::Literal::String("exam".to_string())))
            ])
       ],
       Ok(vec![RuntimeValue::String("examString".to_string())]))]
    #[case::replace(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "testString".to_string(), position: None}), None)],
       vec![
            ast_call("replace", vec![
                ast_node(ast::Expr::Literal(ast::Literal::String("test".to_string()))),
                ast_node(ast::Expr::Literal(ast::Literal::String("exam".to_string())))
            ])
       ],
       Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "examString".to_string(), position: None}), None)]))]
    #[case::replace(vec![RuntimeValue::NONE],
       vec![
            ast_call("replace", vec![
                ast_node(ast::Expr::Literal(ast::Literal::String("test".to_string()))),
                ast_node(ast::Expr::Literal(ast::Literal::String("exam".to_string())))
            ])
       ],
       Ok(vec![RuntimeValue::NONE]))]
    #[case::replace(vec![RuntimeValue::Number(123.into())],
       vec![
            ast_call("replace", vec![
                ast_node(ast::Expr::Literal(ast::Literal::String("test".to_string()))),
                ast_node(ast::Expr::Literal(ast::Literal::String("exam".to_string())))
            ])
       ],
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                    name: "replace".to_string(),
                                                    args: vec![123.to_string().into(), "test".to_string().into(), "exam".to_string().into()]})))]
    #[case::gsub_regex(vec![RuntimeValue::String("test123".to_string())],
       vec![
            ast_call("gsub", vec![
                ast_node(ast::Expr::Literal(ast::Literal::String(r"\d+".to_string()))),
                ast_node(ast::Expr::Literal(ast::Literal::String("456".to_string())))
            ])
       ],
       Ok(vec![RuntimeValue::String("test456".to_string())]))]
    #[case::gsub_regex(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "test123".to_string(), position: None}), None)],
       vec![
            ast_call("gsub", vec![
                ast_node(ast::Expr::Literal(ast::Literal::String(r"\d+".to_string()))),
                ast_node(ast::Expr::Literal(ast::Literal::String("456".to_string())))
            ])
       ],
       Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "test456".to_string(), position: None}), None)]))]
    #[case::gsub_regex(vec![RuntimeValue::Number(123.into())],
       vec![
            ast_call("gsub", vec![
                ast_node(ast::Expr::Literal(ast::Literal::String("test".to_string()))),
                ast_node(ast::Expr::Literal(ast::Literal::String(r"\d+".to_string()))),
            ])
       ],
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                    name: "gsub".to_string(),
                                                    args: vec![123.to_string().into(), "test".to_string().into(), r"\d+".to_string().into()]})))]
    #[case::len(vec![RuntimeValue::String("testString".to_string())],
       vec![ast_call("len", Vec::new())],
       Ok(vec![RuntimeValue::Number(10.into())]))]
    #[case::len(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "testString".to_string(), position: None}), None)],
       vec![ast_call("len", Vec::new())],
       Ok(vec![RuntimeValue::Number(10.into())]))]
    #[case::len(vec![RuntimeValue::TRUE],
       vec![ast_call("len", Vec::new())],
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                    name: "len".to_string(),
                                                    args: vec![true.to_string().into()]})))]
    #[case::len(vec![RuntimeValue::String("テスト".to_string())],
       vec![ast_call("len", Vec::new())],
       Ok(vec![RuntimeValue::Number(3.into())]))]
    #[case::len(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "テスト".to_string(), position: None}), None)],
       vec![ast_call("len", Vec::new())],
       Ok(vec![RuntimeValue::Number(3.into())]))]
    #[case::utf8bytelen(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("utf8bytelen", Vec::new())
       ],
       Ok(vec![RuntimeValue::Number(4.into())]))]
    #[case::utf8bytelen(vec![RuntimeValue::String("テスト".to_string())],
       vec![
            ast_call("utf8bytelen", Vec::new())
       ],
       Ok(vec![RuntimeValue::Number(9.into())]))]
    #[case::utf8bytelen(vec![RuntimeValue::String("😊".to_string())],
       vec![
            ast_call("utf8bytelen", Vec::new())
       ],
       Ok(vec![RuntimeValue::Number(4.into())]))]
    #[case::utf8bytelen(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "test".to_string(), position: None}), None)],
       vec![
            ast_call("utf8bytelen", Vec::new())
       ],
       Ok(vec![RuntimeValue::Number(4.into())]))]
    #[case::utf8bytelen(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "テスト".to_string(), position: None}), None)],
       vec![
            ast_call("utf8bytelen", Vec::new())
       ],
       Ok(vec![RuntimeValue::Number(9.into())]))]
    #[case::utf8bytelen(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "😊".to_string(), position: None}), None)],
       vec![
            ast_call("utf8bytelen", Vec::new())
       ],
       Ok(vec![RuntimeValue::Number(4.into())]))]
    #[case::utf8bytelen(vec![RuntimeValue::Array(vec![RuntimeValue::String("test".to_string())])],
       vec![
            ast_call("utf8bytelen", Vec::new())
       ],
       Ok(vec![RuntimeValue::Number(1.into())]))]
    #[case::utf8bytelen(vec![RuntimeValue::TRUE],
       vec![
            ast_call("utf8bytelen", Vec::new())
       ],
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                    name: "utf8bytelen".to_string(),
                                                    args: vec![true.to_string().into()]})))]
    #[case::index(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("index", vec![
                ast_node(ast::Expr::Literal(ast::Literal::String("test".to_string())))
            ])
       ],
       Ok(vec![RuntimeValue::Number(0.into())]))]
    #[case::index(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "testString".to_string(), position: None}), None)],
       vec![
            ast_call("index", vec![
                ast_node(ast::Expr::Literal(ast::Literal::String("test".to_string())))
            ])
       ],
       Ok(vec![RuntimeValue::Number(0.into())]))]
    #[case::index(vec![RuntimeValue::Number(1.into())],
       vec![
            ast_call("index", vec![
                ast_node(ast::Expr::Literal(ast::Literal::String("test".to_string())))
            ])
       ],
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                    name: "index".to_string(),
                                                    args: vec!["1".into(), "test".into()]})))]
    #[case::array_index(vec![RuntimeValue::Array(vec![RuntimeValue::String("test1".to_string()), RuntimeValue::String("test2".to_string()), RuntimeValue::String("test3".to_string())])],
        vec![
              ast_call("index", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("test2".to_string())))
              ])
        ],
        Ok(vec![RuntimeValue::Number(1.into())]))]
    #[case::array_index_not_found(vec![RuntimeValue::Array(vec![RuntimeValue::String("test1".to_string()), RuntimeValue::String("test2".to_string()), RuntimeValue::String("test3".to_string())])],
        vec![
              ast_call("index", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("test4".to_string())))
              ])
        ],
        Ok(vec![RuntimeValue::Number((-1).into())]))]
    #[case::rindex(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("rindex", vec![
                ast_node(ast::Expr::Literal(ast::Literal::String("String".to_string())))
            ])
       ],
       Ok(vec![RuntimeValue::Number(4.into())]))]
    #[case::rindex(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "testString".to_string(), position: None}), None)],
       vec![
            ast_call("rindex", vec![
                ast_node(ast::Expr::Literal(ast::Literal::String("String".to_string())))
            ])
       ],
       Ok(vec![RuntimeValue::Number(4.into())]))]
    #[case::rindex(vec![RuntimeValue::Number(123.into())],
       vec![
            ast_call("rindex", vec![
                ast_node(ast::Expr::Literal(ast::Literal::String("String".to_string())))
            ])
       ],
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                    name: "rindex".to_string(),
                                                    args: vec!["123".into(), "String".into()]})))]
    #[case::array_rindex(vec![RuntimeValue::Array(vec![RuntimeValue::String("test1".to_string()), RuntimeValue::String("test2".to_string()), RuntimeValue::String("test1".to_string())])],
        vec![
              ast_call("rindex", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("test1".to_string())))
              ])
        ],
        Ok(vec![RuntimeValue::Number(2.into())]))]
    #[case::array_rindex(vec![RuntimeValue::Array(vec![RuntimeValue::String("test1".to_string()), RuntimeValue::String("test2".to_string()), RuntimeValue::String("test3".to_string())])],
        vec![
              ast_call("rindex", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("test4".to_string())))
              ])
        ],
        Ok(vec![RuntimeValue::Number((-1).into())]))]
    #[case::array_rindex_empty(vec![RuntimeValue::Array(vec![])],
        vec![
              ast_call("rindex", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("test".to_string())))
              ])
        ],
        Ok(vec![RuntimeValue::Number((-1).into())]))]
    #[case::eq(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("eq", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("eq".to_string()))),
                    ast_node(ast::Expr::Literal(ast::Literal::String("eq".to_string())))
                ])
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::eq(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("eq", vec![
                ast_node(ast::Expr::Literal(ast::Literal::String("eq".to_string()))),
                ast_node(ast::Expr::Literal(ast::Literal::String("eq1".to_string())))
            ])
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::ne(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("ne", vec![
                ast_node(ast::Expr::Literal(ast::Literal::String("eq".to_string()))),
                ast_node(ast::Expr::Literal(ast::Literal::String("eq1".to_string())))
            ])
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::ne(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("ne", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("eq".to_string()))),
                    ast_node(ast::Expr::Literal(ast::Literal::String("eq".to_string())))
                ])
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::ne(vec![RuntimeValue::Number(1.3.into())],
       vec![
            ast_call("ne", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.3.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.3.into()))),
                ]),
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::ne(vec![RuntimeValue::Number(1.3.into())],
       vec![
            ast_call("ne", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.3.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.2.into()))),
                ]),
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::gt(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("gt", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(2.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                ]),
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::gt(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("gt", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(2.into()))),
                ]),
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::gt(vec![RuntimeValue::Number(1.3.into())],
       vec![
            ast_call("gt", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.4.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.3.into()))),
                ]),
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::gt(vec![RuntimeValue::Number(1.3.into())],
       vec![
            ast_call("gt", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.3.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.4.into()))),
                ]),
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::gt(vec![RuntimeValue::Number(1.3.into())],
       vec![
            ast_call("gt", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::String(1.to_string()))),
                    ast_node(ast::Expr::Literal(ast::Literal::String(2.to_string()))),
                ]),
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::gt(vec![RuntimeValue::Number(1.3.into())],
       vec![
            ast_call("gt", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::String(2.to_string()))),
                    ast_node(ast::Expr::Literal(ast::Literal::String(1.to_string()))),
                ]),
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::gt(vec![RuntimeValue::FALSE],
       vec![
            ast_call("gt", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(true))),
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(true))),
                ]),
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::gt(vec![RuntimeValue::FALSE],
       vec![
            ast_call("gt", vec![
                ast_call("to_code", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("1".into()))),
                ]),
                ast_call("to_code", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("2".into()))),
                ]),
            ]),
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::gte(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("gte", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                ]),
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::gte(vec![RuntimeValue::String("test".to_string())],
       vec! [
            ast_call("gte", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(2.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                ]),
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::gte(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("gte", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(2.into()))),
                ]),
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::gte(vec![RuntimeValue::Number(1.3.into())],
       vec![
            ast_call("gte", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.3.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.3.into()))),
                ]),
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::gte(vec![RuntimeValue::Number(1.3.into())],
       vec![
            ast_call("gte", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.2.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.3.into()))),
                ]),
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::gte(vec![RuntimeValue::String("test".to_string())],
       vec! [
            ast_call("gte", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::String(2.to_string()))),
                    ast_node(ast::Expr::Literal(ast::Literal::String(1.to_string()))),
                ]),
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::gte(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("gte", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::String(1.to_string()))),
                    ast_node(ast::Expr::Literal(ast::Literal::String(2.to_string()))),
                ]),
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::gte(vec![RuntimeValue::TRUE],
       vec![
            ast_call("gte", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(true))),
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(true))),
                ]),
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::gte(vec![RuntimeValue::TRUE],
       vec![
            ast_call("gte", vec![
                ast_call("to_code", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("1".into()))),
                ]),
                ast_call("to_code", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("1".into()))),
                ]),
            ]),
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::lt(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("lt", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(2.into()))),
                ]),
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::lt(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("lt", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(2.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                ]),
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::lt(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("lt", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.2.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.3.into()))),
                ])
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::lt(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("lt", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.3.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.2.into()))),
                ])
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::lt(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("lt", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::String(1.to_string()))),
                    ast_node(ast::Expr::Literal(ast::Literal::String(2.to_string()))),
                ]),
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::lt(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("lt", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::String(2.to_string()))),
                    ast_node(ast::Expr::Literal(ast::Literal::String(1.to_string()))),
                ]),
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::lt(vec![RuntimeValue::TRUE],
       vec![
            ast_call("lt", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(true))),
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(true))),
                ]),
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::lt(vec![RuntimeValue::TRUE],
       vec![
            ast_call("lt", vec![
                ast_call("to_code", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("2".into()))),
                ]),
                ast_call("to_code", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("1".into()))),
                ]),
            ]),
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::lt(vec![RuntimeValue::TRUE],
       vec![
            ast_call("lt", vec![
                ast_call("to_code", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("1".into()))),
                ]),
                ast_call("to_code", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("2".into()))),
                ]),
            ]),
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::lte(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("lte", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                ])
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::lte(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("lte", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(2.into()))),
                ])
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::lte(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("lte", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(2.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                ])
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::lte(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("lte", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.3.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.3.into()))),
                ])
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::lte(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("lte", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.2.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.3.into()))),
                ])
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::lte(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("lte", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.4.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.3.into()))),
                ]),
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::lte(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("lte", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(2.into()))),
                ])
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::lte(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("lte", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::String(2.to_string()))),
                    ast_node(ast::Expr::Literal(ast::Literal::String(1.to_string()))),
                ])
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::lte(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("lte", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(false))),
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(false))),
                ])
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::add(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("add", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                ]),
       ],
       Ok(vec![RuntimeValue::Number(2.into())]))]
    #[case::add(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("add", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("te".to_string()))),
                    ast_node(ast::Expr::Literal(ast::Literal::String("st".to_string()))),
                ]),
       ],
       Ok(vec![RuntimeValue::String("test".to_string())]))]
    #[case::add(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("add", vec![
                    ast_call("array", vec![
                        ast_node(ast::Expr::Literal(ast::Literal::String("te".to_string())))
                    ]),
                    ast_call("array", vec![
                        ast_node(ast::Expr::Literal(ast::Literal::String("te".to_string())))
                    ])
                ]),
       ],
       Ok(vec![RuntimeValue::Array(vec!["te".to_string().into(), "te".to_string().into()])]))]
    #[case::add(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("add", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("te".to_string()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                ]),
       ],
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                         name: "add".to_string(),
                                                         args: vec!["te".into(), 1.to_string().into()]})))]
    #[case::add(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("add", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.3.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.3.into()))),
                ]),
       ],
       Ok(vec![RuntimeValue::Number(2.6.into())]))]
    #[case::add(vec![RuntimeValue::TRUE],
       vec![
            ast_call("add", vec![
                ast_call("to_code", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("2".into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::None)),
                ]),
                ast_call("to_code", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("1".into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::None)),
                ]),
            ]),
       ],
       Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Code(mq_markdown::Code{value: "21".to_string(), lang: None, position: None}), None)]))]
    #[case::add(vec![RuntimeValue::TRUE],
       vec![
            ast_call("add", vec![
                ast_node(ast::Expr::Literal(ast::Literal::String("2".into()))),
                ast_call("to_code", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("1".into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::None)),
                ]),
            ]),
       ],
       Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Code(mq_markdown::Code{value: "21".to_string(), lang: None, position: None}), None)]))]
    #[case::add(vec![RuntimeValue::TRUE],
       vec![
            ast_call("add", vec![
                ast_call("to_code", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("2".into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::None)),
                ]),
                ast_node(ast::Expr::Literal(ast::Literal::String("1".into()))),
            ]),
       ],
       Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Code(mq_markdown::Code{value: "21".to_string(), lang: None, position: None}), None)]))]
    #[case::sub(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("sub", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                ]),
       ],
       Ok(vec![RuntimeValue::Number(0.into())]))]
    #[case::sub(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("sub", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("te".to_string()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                ]),
       ],
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                         name: "sub".to_string(),
                                                         args: vec!["te".to_string().into(), 1.to_string().into()]})))]
    #[case::sub(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("sub", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.3.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.2.into()))),
                ])
       ],
       Ok(vec![RuntimeValue::Number(0.10000000000000009.into())]))]
    #[case::div(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("div", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                ])
       ],
       Ok(vec![RuntimeValue::Number(1.into())]))]
    #[case::div(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("div", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("te".to_string()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                ])
       ],
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                         name: "div".to_string(),
                                                         args: vec!["te".to_string().into(), 1.to_string().into()]})))]
    #[case::div(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("div", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(0.into()))),
                ])
       ],
       Err(InnerError::Eval(EvalError::ZeroDivision(Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()}))))]
    #[case::div(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("div", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.3.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.1.into()))),
                ])
       ],
       Ok(vec![RuntimeValue::Number(1.1818181818181817.into())]))]
    #[case::mul(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("mul", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(2.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                ]),
       ],
       Ok(vec![RuntimeValue::Number(2.into())]))]
    #[case::mul(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("mul", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(2.0.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.3.into()))),
                ]),
       ],
       Ok(vec![RuntimeValue::Number(2.6.into())]))]
    #[case::mul(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("mul", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("te".to_string()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                ]),
       ],
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                         name: "mul".to_string(),
                                                         args: vec!["te".to_string().into(), 1.to_string().into()]})))]
    #[case::mod_(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("mod", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(2.into()))),
                ]),
       ],
       Ok(vec![RuntimeValue::Number(1.into())]))]
    #[case::mod_(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("mod", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.1.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(2.0.into()))),
                ]),
       ],
       Ok(vec![RuntimeValue::Number(1.1.into())]))]
    #[case::mod_(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("mod", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("te".to_string()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                ]),
       ],
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                    name: "mod".to_string(),
                                                    args: vec!["te".to_string().into(), 1.to_string().into()]})))]
    #[case::pow(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("pow", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(2.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(3.into()))),
                ]),
       ],
       Ok(vec![RuntimeValue::Number(8.into())]))]
    #[case::pow(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("pow", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("te".to_string()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                ]),
       ],
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                    name: "pow".to_string(),
                                                    args: vec!["te".to_string().into(), 1.to_string().into()]})))]
    #[case::and(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("and", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(true))),
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(true))),
                ]),
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::and(vec![RuntimeValue::TRUE],
       vec![
            ast_call("and", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(true))),
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(false))),
                ]),
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::and(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("and", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(false))),
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(true))),
                ]),
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::and(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("and", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(false))),
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(false))),
                ]),
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::or(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("or", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(true))),
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(true))),
                ]),
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::or(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("or", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(true))),
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(false))),
                ]),
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::or(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("or", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(false))),
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(true))),
                ]),
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::or(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("or", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(false))),
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(false))),
                ]),
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::not(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("not", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(true))),
                ]),
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::not(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("not", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(false))),
                ]),
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::to_string(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("to_string", Vec::new())
       ],
       Ok(vec![RuntimeValue::String("test".to_string())]))]
    #[case::to_string(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "test".to_string(), position: None}), None)],
       vec![
            ast_call("to_string", Vec::new())
       ],
       Ok(vec![RuntimeValue::String("test".to_string())]))]
    #[case::split1(vec![RuntimeValue::String("test1,test2".to_string())],
       vec![
            ast_call("split", vec![
                        ast_node(ast::Expr::Literal(ast::Literal::String(",".to_string())))]
                        )
       ],
       Ok(vec![RuntimeValue::Array(vec![RuntimeValue::String("test1".to_string()), RuntimeValue::String("test2".to_string())])]))]
    #[case::split2(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "test1,test2".to_string(), position: None}), None)],
       vec![
            ast_call("split", vec![
                ast_node(ast::Expr::Literal(ast::Literal::String(",".to_string())))
            ])
       ],
       Ok(vec![RuntimeValue::Array(vec![RuntimeValue::String("test1".to_string()), RuntimeValue::String("test2".to_string())])]))]
    #[case::split(vec![RuntimeValue::Number(1.into())],
       vec![
            ast_call("split", vec![ast_node(ast::Expr::Literal(ast::Literal::String(",".to_string())))])
       ],
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                    name: "split".to_string(),
                                                    args: vec![1.to_string().into(), ",".to_string().into()]})))]
    #[case::join1(vec![RuntimeValue::String("test1,test2".to_string())],
       vec![
            ast_call("join", vec![
                ast_call("split", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::String(",".to_string())))
                ]),
                ast_node(ast::Expr::Literal(ast::Literal::String("#".to_string())))
            ])
       ],
       Ok(vec![RuntimeValue::String("test1#test2".to_string())]))]
    #[case::base64(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("base64", vec![
                ast_node(ast::Expr::Literal(ast::Literal::String("test".to_string())))
            ])
       ],
       Ok(vec![RuntimeValue::String("dGVzdA==".to_string())]))]
    #[case::base64(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value:"test".to_string(), position: None}), None)],
       vec![
            ast_call("base64", vec![])
       ],
       Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "dGVzdA==".to_string(), position: None}), None)]))]
    #[case::base64(vec![RuntimeValue::Number(1.into())],
       vec![
            ast_call("base64", vec![])
       ],
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                    name: "base64".to_string(),
                                                    args: vec![1.to_string().into()]})))]
    #[case::base64d(vec![RuntimeValue::String("dGVzdA==".to_string())],
       vec![
            ast_call("base64d", vec![
                ast_node(ast::Expr::Literal(ast::Literal::String("dGVzdA==".to_string())))
            ])
       ],
       Ok(vec![RuntimeValue::String("test".to_string())]))]
    #[case::base64d(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value:"dGVzdA==".to_string(), position: None}), None)],
       vec![
            ast_call("base64d", vec![
            ])
       ],
       Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "test".to_string(), position: None}), None)]))]
    #[case::base64d(vec![RuntimeValue::Number(1.into())],
       vec![
            ast_call("base64d", vec![
            ])
       ],
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                    name: "base64d".to_string(),
                                                    args: vec![1.to_string().into()]})))]
    #[case::def(vec![RuntimeValue::String("test1,test2".to_string())],
       vec![
            ast_node(ast::Expr::Def(
                ast::Ident::new("split2"),
                vec![
                    ast_node(ast::Expr::Ident(ast::Ident::new("str"))),
                ],
                vec![ast_call("split",
                    vec![
                        ast_node(ast::Expr::Ident(ast::Ident::new("str"))),
                        ast_node(ast::Expr::Literal(ast::Literal::String(",".to_string()))),
                    ])
                ]
            )),
            ast_call("split2", vec![
                ast_node(ast::Expr::Literal(ast::Literal::String("test1,test2".to_string()))),
            ]),
       ],
       Ok(vec![RuntimeValue::Array(vec![RuntimeValue::String("test1".to_string()), RuntimeValue::String("test2".to_string())])]))]
    #[case::def2(vec![RuntimeValue::String("Hello".to_string())],
       vec![
            ast_node(ast::Expr::Def(
                ast::Ident::new("concat_self"),
                vec![
                    ast_node(ast::Expr::Ident(ast::Ident::new("str1"))),
                    ast_node(ast::Expr::Ident(ast::Ident::new("str2"))),
                ],
                vec![ast_call("add",
                    vec![
                        ast_node(ast::Expr::Ident(ast::Ident::new("str1"))),
                        ast_node(ast::Expr::Ident(ast::Ident::new("str2"))),
                    ])
                ]
            )),
            ast_call("concat_self", vec![
                ast_node(ast::Expr::Literal(ast::Literal::String("Hello".to_string()))),
                ast_node(ast::Expr::Literal(ast::Literal::String("World".to_string()))),
            ])
       ],
       Ok(vec![RuntimeValue::String("HelloWorld".to_string())]))]
    #[case::def3(vec![RuntimeValue::String("Test".to_string())],
       vec![
            ast_node(ast::Expr::Def(
                ast::Ident::new("prepend_self"),
                vec![
                    ast_node(ast::Expr::Ident(ast::Ident::new("str1"))),
                    ast_node(ast::Expr::Ident(ast::Ident::new("str2"))),
                ],
                vec![ast_call("add",
                    vec![
                        ast_node(ast::Expr::Ident(ast::Ident::new("str1"))),
                        ast_node(ast::Expr::Ident(ast::Ident::new("str2"))),
                    ])
                ]
            )),
            ast_call("prepend_self", vec![
                ast_node(ast::Expr::Literal(ast::Literal::String("test".to_string())))
            ])
       ],
       Ok(vec![RuntimeValue::String("Testtest".to_string())]))]
    #[case::type_string(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("type", Vec::new())
       ],
       Ok(vec![RuntimeValue::String("string".to_string())]))]
    #[case::type_int(vec![RuntimeValue::Number(42.into())],
       vec![
            ast_call("type", Vec::new())
       ],
       Ok(vec![RuntimeValue::String("number".to_string())]))]
    #[case::type_bool(vec![RuntimeValue::TRUE],
       vec![
            ast_call("type", Vec::new())
       ],
       Ok(vec![RuntimeValue::String("bool".to_string())]))]
    #[case::type_array(vec![RuntimeValue::Array(vec![RuntimeValue::String("test".to_string())])],
       vec![
            ast_call("type", Vec::new())
       ],
       Ok(vec![RuntimeValue::String("array".to_string())]))]
    #[case::min(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("min", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(2.into()))),
                ])
        ],
       Ok(vec![RuntimeValue::Number(1.into())]))]
    #[case::min(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("min", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("1".into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::String("2".into()))),
                ])
        ],
       Ok(vec![RuntimeValue::String("1".into())]))]
    #[case::min(vec![RuntimeValue::Number(1.into())],
       vec![
            ast_call("min", vec![
                ast_node(ast::Expr::Literal(ast::Literal::Number(2.into()))),
            ])
        ],
       Ok(vec![RuntimeValue::Number(1.into())]))]
    #[case::min(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("min", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("te".to_string()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                ])
            ],
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                    name: "min".to_string(),
                                                    args: vec!["te".to_string().into(), 1.to_string().into()]})))]
    #[case::max(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("max", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(2.into()))),
                ])
            ],
       Ok(vec![RuntimeValue::Number(2.into())]))]
    #[case::max(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("max", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("1".into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::String("2".into()))),
                ])
            ],
       Ok(vec![RuntimeValue::String("2".into())]))]
    #[case::max(vec![RuntimeValue::Number(3.into())],
       vec![
            ast_call("max", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                ])
            ],
       Ok(vec![RuntimeValue::Number(3.into())]))]
    #[case::max(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("max", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("te".to_string()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                ])
            ],
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                    name: "max".to_string(),
                                                    args: vec!["te".to_string().into(), 1.to_string().into()]})))]
    #[case::trim(vec![RuntimeValue::String("  test  ".to_string())],
       vec![
            ast_call("trim", Vec::new())
       ],
       Ok(vec![RuntimeValue::String("test".to_string())]))]
    #[case::trim(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "  test  ".to_string(), position: None}), None)],
       vec![
            ast_call("trim", Vec::new())
       ],
       Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "test".to_string(), position: None}), None)]))]
    #[case::trim(vec![RuntimeValue::Number(1.into())],
       vec![
            ast_call("trim", Vec::new())
       ],
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                    name: "trim".to_string(),
                                                    args: vec![1.to_string().into()]})))]
    #[case::slice(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "testString".to_string(), position: None}), None)],
       vec![
            ast_call("slice", vec![
                ast_node(ast::Expr::Literal(ast::Literal::Number(0.into()))),
                ast_node(ast::Expr::Literal(ast::Literal::Number(4.into()))),
            ])
       ],
       Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "test".to_string(), position: None}), None)]))]
    #[case::slice(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("slice", vec![
                ast_node(ast::Expr::Literal(ast::Literal::Number(0.into()))),
                ast_node(ast::Expr::Literal(ast::Literal::Number(4.into()))),
            ])
       ],
       Ok(vec![RuntimeValue::String("test".to_string())]))]
    #[case::slice(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("slice", vec![
                ast_node(ast::Expr::Literal(ast::Literal::Number(4.into()))),
                ast_node(ast::Expr::Literal(ast::Literal::Number(10.into()))),
            ])
       ],
       Ok(vec![RuntimeValue::String("String".to_string())]))]
    #[case::slice(vec![RuntimeValue::NONE],
       vec![
            ast_call("slice", vec![
                ast_node(ast::Expr::Literal(ast::Literal::Number(0.into()))),
                ast_node(ast::Expr::Literal(ast::Literal::Number(4.into()))),
            ])
       ],
       Ok(vec![RuntimeValue::NONE]))]
    #[case::slice(vec![RuntimeValue::Number(123.into())],
       vec![
            ast_call("slice", vec![
                ast_node(ast::Expr::Literal(ast::Literal::Number(0.into()))),
                ast_node(ast::Expr::Literal(ast::Literal::Number(4.into()))),
            ])
       ],
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                    name: "slice".to_string(),
                                                    args: vec![123.to_string().into(), 0.to_string().into(), 4.to_string().into()]})))]
    #[case::match_regex(vec![RuntimeValue::String("test123".to_string())],
       vec![
            ast_call("match", vec![
                ast_node(ast::Expr::Literal(ast::Literal::String(r"\d+".to_string()))),
            ])
       ],
       Ok(vec![RuntimeValue::Array(vec![RuntimeValue::String("123".to_string())])]))]
    #[case::match_regex(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "test123".to_string(), position: None}), None)],
       vec![
            ast_call("match", vec![
                ast_node(ast::Expr::Literal(ast::Literal::String(r"\d+".to_string()))),
            ])
       ],
       Ok(vec![RuntimeValue::Array(vec![123.to_string().into()])]))]
    #[case::match_regex(vec![RuntimeValue::Number(123.into())],
       vec![
            ast_call("match", vec![
                ast_node(ast::Expr::Literal(ast::Literal::String(r"\d+".to_string()))),
            ])
       ],
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                    name: "match".to_string(),
                                                    args: vec![123.to_string().into(), r"\d+".to_string().into()]})))]
    #[case::explode(vec![RuntimeValue::String("ABC".to_string())],
       vec![
            ast_call("explode", Vec::new())
       ],
       Ok(vec![RuntimeValue::Array(vec![
            RuntimeValue::Number(65.into()),
            RuntimeValue::Number(66.into()),
            RuntimeValue::Number(67.into()),
       ])]))]
    #[case::explode(vec![RuntimeValue::Number(123.into())],
       vec![
            ast_call("explode", Vec::new())
       ],
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                    name: "explode".to_string(),
                                                    args: vec![123.to_string().into()]})))]
    #[case::implode(vec![RuntimeValue::Array(vec![
            RuntimeValue::Number(65.into()),
            RuntimeValue::Number(66.into()),
            RuntimeValue::Number(67.into()),
       ])],
       vec![
            ast_call("implode", Vec::new())
       ],
       Ok(vec![RuntimeValue::String("ABC".to_string())]))]
    #[case::implode(vec!["test".to_string().into()],
       vec![
            ast_call("implode", Vec::new())
       ],
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                    name: "implode".to_string(),
                                                    args: vec!["test".to_string().into()]})))]
    #[case::explode_markdown(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "ABC".to_string(), position: None}), None)],
        vec![
             ast_call("explode", Vec::new())
        ],
        Ok(vec![RuntimeValue::Array(vec![
             RuntimeValue::Number(65.into()),
             RuntimeValue::Number(66.into()),
             RuntimeValue::Number(67.into()),
        ])]))]
    #[case::range(vec![RuntimeValue::Number(1.into())],
       vec![
            ast_call("range", vec![
                ast_node(ast::Expr::Literal(ast::Literal::Number(0.into()))),
                ast_node(ast::Expr::Literal(ast::Literal::Number(5.into()))),
            ])
       ],
       Ok(vec![RuntimeValue::Array(vec![
            RuntimeValue::Number(0.into()),
            RuntimeValue::Number(1.into()),
            RuntimeValue::Number(2.into()),
            RuntimeValue::Number(3.into()),
            RuntimeValue::Number(4.into()),
       ])]))]
    #[case::range(vec!["1".to_string().into()],
       vec![
            ast_call("range", vec![
                ast_node(ast::Expr::Literal(ast::Literal::Number(0.into()))),
            ])
       ],
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                    name: "range".to_string(),
                                                    args: vec!["1".to_string().into(), "0".to_string().into()]})))]
    #[case::to_number(vec![RuntimeValue::String("42".to_string())],
       vec![
            ast_call("to_number", Vec::new())
       ],
       Ok(vec![RuntimeValue::Number(42.into())]))]
    #[case::to_number(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "42".to_string(), position: None}), None)],
       vec![
            ast_call("to_number", Vec::new())
       ],
       Ok(vec![RuntimeValue::Number(42.into())]))]
    #[case::to_number(vec![RuntimeValue::String("42.5".to_string())],
       vec![
            ast_call("to_number", Vec::new())
       ],
       Ok(vec![RuntimeValue::Number(42.5.into())]))]
    #[case::to_number(vec![RuntimeValue::String("not a number".to_string())],
       vec![
            ast_call("to_number", Vec::new())
       ],
       Err(InnerError::Eval(EvalError::RuntimeError(Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()}, "invalid float literal".to_string()))))]
    #[case::to_number_array(vec![RuntimeValue::Array(vec![RuntimeValue::String("42".to_string()), RuntimeValue::String("43".to_string()), RuntimeValue::String("44".to_string())])],
        vec![
              ast_call("to_number", Vec::new())
        ],
        Ok(vec![RuntimeValue::Array(vec![RuntimeValue::Number(42.into()), RuntimeValue::Number(43.into()), RuntimeValue::Number(44.into())])]))]
    #[case::to_number_array(vec![RuntimeValue::Array(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "42".to_string(), position: None}), None)])],
        vec![
              ast_call("to_number", Vec::new())
        ],
        Ok(vec![RuntimeValue::Array(vec![RuntimeValue::Number(42.into())])]))]
    #[case::to_number_array_with_invalid(vec![RuntimeValue::Array(vec![RuntimeValue::String("42".to_string()), RuntimeValue::String("not a number".to_string()), RuntimeValue::String("44".to_string())])],
        vec![
              ast_call("to_number", Vec::new())
        ],
        Err(InnerError::Eval(EvalError::RuntimeError(Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()}, "invalid float literal".to_string()))))]
    #[case::to_number_array_empty(vec![RuntimeValue::Array(vec![])],
        vec![
              ast_call("to_number", Vec::new())
        ],
        Ok(vec![RuntimeValue::Array(vec![])]))]
    #[case::to_number_array_mixed_types(vec![RuntimeValue::Array(vec![RuntimeValue::String("42".to_string()), RuntimeValue::Number(43.into()), RuntimeValue::String("44".to_string())])],
        vec![
              ast_call("to_number", Vec::new())
        ],
        Ok(vec![RuntimeValue::Array(vec![RuntimeValue::Number(42.into()), RuntimeValue::Number(43.into()), RuntimeValue::Number(44.into())])]))]
    #[case::trunc(vec![RuntimeValue::Number(42.5.into())],
       vec![
            ast_call("trunc", Vec::new())
       ],
       Ok(vec![RuntimeValue::Number(42.into())]))]
    #[case::trunc(vec![RuntimeValue::Number((-42.5).into())],
       vec![
            ast_call("trunc", Vec::new())
       ],
       Ok(vec![RuntimeValue::Number((-42).into())]))]
    #[case::trunc(vec!["42.5".to_string().into()],
       vec![
            ast_call("trunc", Vec::new())
       ],
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                    name: "trunc".to_string(),
                                                    args: vec!["42.5".to_string().into()]})))]
    #[case::abs_positive(vec![RuntimeValue::Number(42.into())],
       vec![
            ast_call("abs", Vec::new())
       ],
       Ok(vec![RuntimeValue::Number(42.into())]))]
    #[case::abs_negative(vec![RuntimeValue::Number((-42).into())],
       vec![
           ast_call("abs", Vec::new())
       ],
       Ok(vec![RuntimeValue::Number(42.into())]))]
    #[case::abs_zero(vec![RuntimeValue::Number(0.into())],
       vec![
            ast_call("abs", Vec::new())
       ],
       Ok(vec![RuntimeValue::Number(0.into())]))]
    #[case::abs_decimal(vec![RuntimeValue::Number((-42.5).into())],
       vec![
            ast_call("abs", Vec::new())
       ],
       Ok(vec![RuntimeValue::Number(42.5.into())]))]
    #[case::abs_invalid_type(vec![RuntimeValue::String("42".to_string())],
       vec![
            ast_call("abs", Vec::new())
       ],
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                    name: "abs".to_string(),
                                                    args: vec!["42".to_string().into()]})))]
    #[case::ceil(vec![RuntimeValue::Number(42.1.into())],
       vec![
            ast_call("ceil", Vec::new())
       ],
       Ok(vec![RuntimeValue::Number(43.into())]))]
    #[case::ceil(vec![RuntimeValue::Number((-42.1).into())],
       vec![
            ast_call("ceil", Vec::new())
       ],
       Ok(vec![RuntimeValue::Number((-42).into())]))]
    #[case::ceil(vec!["42".to_string().into()],
       vec![
            ast_call("ceil", Vec::new())
       ],
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                    name: "ceil".to_string(),
                                                    args: vec!["42".to_string().into()]})))]
    #[case::round(vec![RuntimeValue::Number(42.5.into())],
       vec![
            ast_call("round", Vec::new())
       ],
       Ok(vec![RuntimeValue::Number(43.into())]))]
    #[case::round(vec![RuntimeValue::Number(42.4.into())],
       vec![
            ast_call("round", Vec::new())
       ],
       Ok(vec![RuntimeValue::Number(42.into())]))]
    #[case::round(vec!["42.4".to_string().into()],
       vec![
            ast_call("round", Vec::new())
       ],
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                    name: "round".to_string(),
                                                    args: vec!["42.4".to_string().into()]})))]
    #[case::floor(vec![RuntimeValue::Number(42.9.into())],
       vec![
            ast_call("floor", Vec::new())
       ],
       Ok(vec![RuntimeValue::Number(42.into())]))]
    #[case::floor(vec![RuntimeValue::Number((-42.9).into())],
       vec![
            ast_call("floor", Vec::new())
       ],
       Ok(vec![RuntimeValue::Number((-43).into())]))]
    #[case::floor(vec!["42.9".to_string().into()],
       vec![
            ast_call("floor", Vec::new())
       ],
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                    name: "floor".to_string(),
                                                    args: vec!["42.9".to_string().into()]})))]
    #[case::del(vec![RuntimeValue::Array(vec![RuntimeValue::String("test1".to_string()), RuntimeValue::String("test2".to_string())])],
        vec![
              ast_call("del", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(0.into()))),
              ]),
        ],
        Ok(vec![RuntimeValue::Array(vec![RuntimeValue::String("test2".to_string())])]))]
    #[case::del(vec![RuntimeValue::Array(vec![RuntimeValue::String("test1".to_string()), RuntimeValue::String("test2".to_string())])],
        vec![
              ast_call("del", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
              ]),
        ],
        Ok(vec![RuntimeValue::Array(vec![RuntimeValue::String("test1".to_string())])]))]
    #[case::del(vec![RuntimeValue::String("test1".to_string())],
        vec![
              ast_call("del", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(4.into()))),
              ]),
        ],
        Ok(vec![RuntimeValue::String("test".to_string())]))]
    #[case::del(vec![RuntimeValue::Number(123.into())],
        vec![
              ast_call("del", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(4.into()))),
              ]),
        ],
        Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                     name: "del".to_string(),
                                                     args: vec!["123".to_string().into(), "4".to_string().into()]})))]
    #[case::to_code(vec![RuntimeValue::String("test1".to_string())],
        vec![
              ast_call("to_code", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("elm".into()))),
              ]),
        ],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Code(mq_markdown::Code{lang: Some("elm".to_string()), value: "test1".to_string(), position: None}), None)]))]
    #[case::to_code(vec![RuntimeValue::String("test1".to_string())],
        vec![
              ast_call("to_code", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("elm".into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::None)),
              ]),
        ],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Code(mq_markdown::Code{lang: None, value: "elm".to_string(), position: None}), None)]))]
    #[case::md_h1(vec![RuntimeValue::String("Heading 1".to_string())],
            vec![
                  ast_call("to_h", vec![
                        ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                  ]),
            ],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Heading(mq_markdown::Heading{depth: 1, values: vec!["Heading 1".to_string().into()], position: None}), None)]))]
    #[case::md_h2(vec![RuntimeValue::String("Heading 2".to_string())],
            vec![
                  ast_call("to_h", vec![
                        ast_node(ast::Expr::Literal(ast::Literal::Number(2.into()))),
                  ]),
            ],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Heading(mq_markdown::Heading{depth: 2, values: vec!["Heading 2".to_string().into()], position: None}), None)]))]
    #[case::md_h3(vec![RuntimeValue::String("Heading 3".to_string())],
            vec![
                  ast_call("to_h", vec![
                        ast_node(ast::Expr::Literal(ast::Literal::Number(3.into()))),
                  ]),
            ],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Heading(mq_markdown::Heading{depth: 3, values: vec!["Heading 3".to_string().into()], position: None}), None)]))]
    #[case::md_h3(vec![RuntimeValue::String("Heading 3".to_string())],
            vec![
                  ast_call("to_h", vec![
                        ast_node(ast::Expr::Literal(ast::Literal::String("3".into()))),
                  ]),
            ],
            Ok(vec![RuntimeValue::NONE]))]
    #[case::md_h(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "Heading".to_string(), position: None}), None)],
            vec![
                  ast_call("to_h", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(2.into()))),
                  ]),
            ],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Heading(mq_markdown::Heading{depth: 2, values: vec!["Heading".to_string().into()], position: None}), None)]))]
    #[case::to_math(vec![RuntimeValue::String("E=mc^2".to_string())],
            vec![
                  ast_call("to_math", Vec::new()),
            ],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Math(mq_markdown::Math{value: "E=mc^2".to_string(), position: None}), None)]))]
    #[case::to_math_inline(vec![RuntimeValue::String("E=mc^2".to_string())],
            vec![
                  ast_call("to_math_inline", Vec::new()),
            ],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::MathInline(mq_markdown::MathInline{value: "E=mc^2".into(), position: None}), None)]))]
    #[case::to_md_text(vec![RuntimeValue::String("This is a text".to_string())],
            vec![
                  ast_call("to_md_text", Vec::new()),
            ],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "This is a text".to_string(), position: None}), None)]))]
    #[case::to_strong(vec![RuntimeValue::String("Bold text".to_string())],
            vec![
                  ast_call("to_strong", Vec::new()),
            ],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Strong(mq_markdown::Value{values: vec!["Bold text".to_string().into()], position: None}), None)]))]
    #[case::to_strong(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "Bold text".to_string(), position: None}), None)],
            vec![
                  ast_call("to_strong", Vec::new()),
            ],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Strong(mq_markdown::Value{values: vec![mq_markdown::Node::Text(mq_markdown::Text{value: "Bold text".to_string(), position: None})], position: None}), None)]))]
    #[case::to_em(vec![RuntimeValue::String("Italic text".to_string())],
            vec![
                  ast_call("to_em", Vec::new()),
            ],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Emphasis(mq_markdown::Value{values: vec!["Italic text".to_string().into()], position: None}), None)]))]
    #[case::to_em(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "Italic text".to_string(), position: None}), None)],
            vec![
                  ast_call("to_em", Vec::new()),
            ],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Emphasis(mq_markdown::Value{values: vec![mq_markdown::Node::Text(mq_markdown::Text{value: "Italic text".to_string(), position: None})], position: None}), None)]))]
    #[case::to_image(vec![RuntimeValue::String("Image Alt".to_string())],
            vec![
                  ast_call("to_image", vec![
                        ast_node(ast::Expr::Literal(ast::Literal::String("https://example.com/image.png".to_string()))),
                        ast_node(ast::Expr::Literal(ast::Literal::String("Image Alt".to_string()))),
                        ast_node(ast::Expr::Literal(ast::Literal::String("Image Title".to_string()))),
                  ]),
            ],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Image(mq_markdown::Image{
                url: "https://example.com/image.png".to_string(),
                alt: "Image Alt".to_string(),
                title: Some("Image Title".to_string()),
                position: None
            }), None)]))]
    #[case::to_link(vec![RuntimeValue::String("Link Text".to_string())],
            vec![
                  ast_call("to_link", vec![
                        ast_node(ast::Expr::Literal(ast::Literal::String("https://example.com".to_string()))),
                        ast_node(ast::Expr::Literal(ast::Literal::String("Link Value".to_string()))),
                        ast_node(ast::Expr::Literal(ast::Literal::String("Link Title".to_string()))),
                  ]),
            ],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Link(mq_markdown::Link{
                url: "https://example.com".to_string(),
                title: Some("Link Title".to_string()),
                values: vec!["Link Value".to_string().into()],
                position: None
            }), None)]))]
    #[case::to_link(vec![RuntimeValue::Number(123.into())],
            vec![
                  ast_call("to_link", vec![
                        ast_node(ast::Expr::Literal(ast::Literal::String("Link Title".to_string()))),
                        ast_node(ast::Expr::Literal(ast::Literal::String("Link Value".to_string()))),
                  ]),
            ],
            Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                         name: "to_link".to_string(),
                                                         args: vec![123.to_string().into(), "Link Title".to_string().into(), "Link Value".to_string().into()]})))]
    #[case::to_hr(vec![RuntimeValue::String("".to_owned())],
            vec![
                  ast_call("to_hr", Vec::new()),
            ],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::HorizontalRule{position: None}, None)]))]
    #[case::to_md_list(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "list".to_string(), position: None}), None)],
            vec![
                  ast_call("to_md_list",
                           vec![
                                 ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                           ]),
            ],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::List(
                mq_markdown::List{values: vec!["list".to_string().into()], index: 0, level: 1_u8, checked: None, position: None}), None)]))]
    #[case::to_md_list(vec![RuntimeValue::String("list".to_string())],
            vec![
                  ast_call("to_md_list",
                           vec![
                                 ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                           ]),
            ],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::List(
                mq_markdown::List{values: vec!["list".to_string().into()], index: 0, level: 1_u8, checked: None, position: None}), None)]))]
    #[case::set_md_check(vec![RuntimeValue::Markdown(mq_markdown::Node::List(mq_markdown::List{values: vec!["Checked Item".to_string().into()], level: 0, index: 0, checked: None, position: None}), None)],
            vec![
                  ast_call("set_md_check", vec![
                        ast_node(ast::Expr::Literal(ast::Literal::Bool(true))),
                  ]),
            ],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::List(mq_markdown::List{values: vec!["Checked Item".to_string().into()], level: 0, index: 0, checked: Some(true), position: None}), None)]))]
    #[case::set_md_check(vec![RuntimeValue::Markdown(mq_markdown::Node::List(mq_markdown::List{values: vec!["Unchecked Item".to_string().into()], level: 0, index: 0, checked: None, position: None}), None)],
            vec![
                  ast_call("set_md_check", vec![
                        ast_node(ast::Expr::Literal(ast::Literal::Bool(false))),
                  ]),
            ],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::List(mq_markdown::List{values: vec!["Unchecked Item".to_string().into()], level: 0, index: 0, checked: Some(false), position: None}), None)]))]
    #[case::compact(vec![RuntimeValue::Array(vec![
                RuntimeValue::String("test1".to_string()),
                RuntimeValue::NONE,
                RuntimeValue::String("test2".to_string()),
                RuntimeValue::NONE,
                RuntimeValue::String("test3".to_string()),
            ])],
            vec![
                ast_call("compact", Vec::new())
            ],
            Ok(vec![RuntimeValue::Array(vec![
                RuntimeValue::String("test1".to_string()),
                RuntimeValue::String("test2".to_string()),
                RuntimeValue::String("test3".to_string()),
            ])]))]
    #[case::compact(vec![RuntimeValue::Array(vec![
                RuntimeValue::String("test1".to_string()),
                RuntimeValue::NONE,
                RuntimeValue::String("test2".to_string()),
                RuntimeValue::NONE,
                RuntimeValue::String("test3".to_string()),
            ])],
            vec![
                ast_call("compact", Vec::new())
            ],
            Ok(vec![RuntimeValue::Array(vec![
                RuntimeValue::String("test1".to_string()),
                RuntimeValue::String("test2".to_string()),
                RuntimeValue::String("test3".to_string()),
            ])]))]
    #[case::compact_empty(vec![RuntimeValue::Array(vec![
                RuntimeValue::NONE,
                RuntimeValue::NONE,
            ])],
            vec![
                ast_call("compact", Vec::new())
            ],
            Ok(vec![RuntimeValue::Array(vec![])]))]
    #[case::compact_no_none(vec![RuntimeValue::Array(vec![
                RuntimeValue::String("test1".to_string()),
                RuntimeValue::String("test2".to_string()),
            ])],
            vec![
                ast_call("compact", Vec::new())
            ],
            Ok(vec![RuntimeValue::Array(vec![
                RuntimeValue::String("test1".to_string()),
                RuntimeValue::String("test2".to_string()),
            ])]))]
    #[case::to_csv(vec![RuntimeValue::Array(vec![
                RuntimeValue::String("test1".to_string()),
                RuntimeValue::String("test2".to_string()),
                RuntimeValue::String("test3".to_string()),
            ])],
            vec![
                ast_call("to_csv", Vec::new())
            ],
            Ok(vec![RuntimeValue::String("test1,test2,test3".to_string())]))]
    #[case::to_csv(vec![RuntimeValue::String("test1".to_string())],
            vec![
                ast_call("to_csv", Vec::new())
            ],
            Ok(vec![RuntimeValue::String("test1".to_string())]))]
    #[case::to_csv_empty(vec![RuntimeValue::Array(vec![])],
            vec![
                ast_call("to_csv", Vec::new())
            ],
            Ok(vec![RuntimeValue::String("".to_string())]))]
    #[case::to_csv_mixed(vec![RuntimeValue::Array(vec![
                RuntimeValue::String("test1".to_string()),
                RuntimeValue::Number(42.into()),
                RuntimeValue::Bool(true),
            ])],
            vec![
                ast_call("to_csv", Vec::new())
            ],
            Ok(vec![RuntimeValue::String("test1,42,true".to_string())]))]
    #[case::to_tsv(vec![RuntimeValue::Array(vec![
                RuntimeValue::String("test1".to_string()),
                RuntimeValue::String("test2".to_string()),
                RuntimeValue::String("test3".to_string()),
            ])],
            vec![
                ast_call("to_tsv", Vec::new())
            ],
            Ok(vec![RuntimeValue::String("test1\ttest2\ttest3".to_string())]))]
    #[case::to_tsv(vec![RuntimeValue::String("test1".to_string())],
            vec![
                ast_call("to_tsv", Vec::new())
            ],
            Ok(vec![RuntimeValue::String("test1".to_string())]))]
    #[case::to_tsv_empty(vec![RuntimeValue::Array(vec![])],
            vec![
                ast_call("to_tsv", Vec::new())
            ],
            Ok(vec![RuntimeValue::String("".to_string())]))]
    #[case::to_tsv_mixed(vec![RuntimeValue::Array(vec![
                RuntimeValue::String("test1".to_string()),
                RuntimeValue::Number(42.into()),
                RuntimeValue::Bool(true),
            ])],
            vec![
                ast_call("to_tsv", Vec::new())
            ],
            Ok(vec![RuntimeValue::String("test1\t42\ttrue".to_string())]))]
    #[case::get_md_list_level(vec![RuntimeValue::Markdown(mq_markdown::Node::List(mq_markdown::List{values: vec!["List Item".to_string().into()], level: 1, index: 0, checked: None, position: None}), None)],
            vec![
                  ast_call("get_md_list_level", vec![]),
            ],
            Ok(vec![RuntimeValue::Number(1.into())]))]
    #[case::text_selector(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "test".to_string(), position: None}), None)],
           vec![
                ast_node(ast::Expr::Selector(ast::Selector::Text)),
           ],
           Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "test".to_string(), position: None}), None)]))]
    #[case::text_selector_heading(vec![RuntimeValue::Markdown(mq_markdown::Node::Heading(mq_markdown::Heading{depth: 1, values: vec!["Heading 1".to_string().into()], position: None}), None)],
           vec![
                ast_node(ast::Expr::Selector(ast::Selector::Text)),
           ],
           Ok(vec![RuntimeValue::NONE]))]
    #[case::to_md_table_row(vec![RuntimeValue::Array(vec![
                RuntimeValue::String("Cell 1".to_string()),
                RuntimeValue::String("Cell 2".to_string()),
                RuntimeValue::String("Cell 3".to_string()),
            ])],
            vec![
                ast_call("to_md_table_row", Vec::new())
            ],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::TableRow(mq_markdown::TableRow{
                cells: vec![
                    mq_markdown::Node::TableCell(mq_markdown::TableCell{
                        row: 0,
                        column: 0,
                        values: vec!["Cell 1".to_string().into()],
                        last_cell_in_row: false,
                        last_cell_of_in_table: false,
                        position: None
                    }),
                    mq_markdown::Node::TableCell(mq_markdown::TableCell{
                        row: 0,
                        column: 1,
                        values: vec!["Cell 2".to_string().into()],
                        last_cell_in_row: false,
                        last_cell_of_in_table: false,
                        position: None
                    }),
                    mq_markdown::Node::TableCell(mq_markdown::TableCell{
                        row: 0,
                        column: 2,
                        values: vec!["Cell 3".to_string().into()],
                        last_cell_in_row: true,
                        last_cell_of_in_table: false,
                        position: None
                    }),
                ],
                position: None
            }), None)]))]
    #[case::to_md_table_row(vec![RuntimeValue::String("Cell 4".to_string())],
            vec![
                ast_call("to_md_table_row", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("Cell 1".to_string()))),
                    ast_node(ast::Expr::Literal(ast::Literal::String("Cell 2".to_string()))),
                ])
            ],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::TableRow(mq_markdown::TableRow{
                cells: vec![
                    mq_markdown::Node::TableCell(mq_markdown::TableCell{
                        row: 0,
                        column: 0,
                        values: vec!["Cell 1".to_string().into()],
                        last_cell_in_row: false,
                        last_cell_of_in_table: false,
                        position: None
                    }),
                    mq_markdown::Node::TableCell(mq_markdown::TableCell{
                        row: 0,
                        column: 1,
                        values: vec!["Cell 2".to_string().into()],
                        last_cell_in_row: true,
                        last_cell_of_in_table: false,
                        position: None
                    }),
                ],
                position: None
            }), None)]))]
    #[case::get_title(vec![RuntimeValue::Markdown(mq_markdown::Node::Link(mq_markdown::Link{url: "https://example.com".to_string(), title: Some("title".to_string()), values: vec!["Link".to_string().into()], position: None}), None)],
            vec![
                 ast_call("get_title", Vec::new())
            ],
            Ok(vec![RuntimeValue::String("title".to_string())]))]
    #[case::get_title(vec![RuntimeValue::Markdown(mq_markdown::Node::Link(mq_markdown::Link{url: "https://example.com".to_string(), title: None, values: vec!["Link".to_string().into()], position: None}), None)],
            vec![
                 ast_call("get_title", Vec::new())
            ],
            Ok(vec![RuntimeValue::NONE]))]
    #[case::get_title(vec![RuntimeValue::Markdown(mq_markdown::Node::Image(mq_markdown::Image{url: "https://example.com/image.png".to_string(), alt: "Image Alt".to_string(), title: Some("Image Title".to_string()), position: None}), None)],
            vec![
                 ast_call("get_title", Vec::new())
            ],
            Ok(vec![RuntimeValue::String("Image Title".to_string())]))]
    #[case::get_title(vec![RuntimeValue::Markdown(mq_markdown::Node::Image(mq_markdown::Image{url: "https://example.com/image.png".to_string(), alt: "Image Alt".to_string(), title: None, position: None}), None)],
            vec![
                 ast_call("get_title", Vec::new())
            ],
            Ok(vec![RuntimeValue::NONE]))]
    #[case::nth_markdown(vec![RuntimeValue::Markdown(mq_markdown::Node::List(mq_markdown::List{values: vec![
                mq_markdown::Node::Text(mq_markdown::Text{value: "Item 1".to_string(), position: None}),
                mq_markdown::Node::Text(mq_markdown::Text{value: "Item 2".to_string(), position: None})
            ], level: 1, index: 0, checked: None, position: None}), None)],
                   vec![
                        ast_call("nth", vec![ast_node(ast::Expr::Literal(ast::Literal::Number(1.into())))])
                   ],
                   Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::List(mq_markdown::List{values: vec![
                mq_markdown::Node::Text(mq_markdown::Text{value: "Item 1".to_string(), position: None}),
                mq_markdown::Node::Text(mq_markdown::Text{value: "Item 2".to_string(), position: None})
            ], level: 1, index: 0, checked: None, position: None}), Some(runtime_value::Selector::Index(1)))]))]
    #[case::nth_string(vec![RuntimeValue::String("test1".to_string())],
           vec![
                ast_call("nth", vec![ast_node(ast::Expr::Literal(ast::Literal::Number(0.into())))])
           ],
           Ok(vec![RuntimeValue::String("t".to_string())]))]
    #[case::nth_string(vec![RuntimeValue::String("test1".to_string())],
           vec![
                ast_call("nth", vec![ast_node(ast::Expr::Literal(ast::Literal::Number(5.into())))])
           ],
           Ok(vec![RuntimeValue::NONE]))]
    #[case::nth_array(vec![RuntimeValue::Array(vec!["test1".to_string().into()])],
           vec![
                ast_call("nth", vec![ast_node(ast::Expr::Literal(ast::Literal::Number(2.into())))])
           ],
           Ok(vec![RuntimeValue::NONE]))]
    #[case::nth(vec![RuntimeValue::TRUE],
           vec![
                ast_call("nth", vec![ast_node(ast::Expr::Literal(ast::Literal::Number(0.into())))])
           ],
           Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                        name: "nth".to_string(),
                                                        args: vec![true.to_string().into(), 0.to_string().into()]})))]
    #[case::to_date(vec![RuntimeValue::Number(1609459200000_i64.into())],
            vec![
                ast_call("to_date", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("%Y-%m-%d".to_string())))
                ])
            ],
            Ok(vec![RuntimeValue::String("2021-01-01".to_string())]))]
    #[case::to_date(vec![RuntimeValue::Number(1609459200000_i64.into())],
            vec![
                ast_call("to_date", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("%Y/%m/%d %H:%M:%S".to_string())))
                ])
            ],
            Ok(vec![RuntimeValue::String("2021/01/01 00:00:00".to_string())]))]
    #[case::to_date(vec![RuntimeValue::Number(1609488000000_i64.into())],
            vec![
                ast_call("to_date", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("%d %b %Y %H:%M".to_string())))
                ])
            ],
            Ok(vec![RuntimeValue::String("01 Jan 2021 08:00".to_string())]))]
    #[case::to_date(vec![RuntimeValue::String("test".to_string())],
            vec![
                ast_call("to_date", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("%Y-%m-%d".to_string())))
                ])
            ],
            Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                         name: "to_date".to_string(),
                                                         args: vec!["test".to_string().into(), "%Y-%m-%d".to_string().into()]})))]
    #[case::to_string_array(vec![RuntimeValue::Array(vec![
                RuntimeValue::String("test".to_string()),
                RuntimeValue::Number(1.into()),
                RuntimeValue::Number(2.into()),
                RuntimeValue::Bool(false),
            ])],
            vec![
                ast_call("to_string", Vec::new())
            ],
            Ok(vec![RuntimeValue::Array(vec!["test".to_string().into(), "1".to_string().into(), "2".to_string().into(), "false".to_string().into()])]))]
    #[case::to_string_empty_array(vec![RuntimeValue::Array(vec![])],
            vec![
                ast_call("to_string", Vec::new())
            ],
            Ok(vec![RuntimeValue::Array(vec![])]))]
    #[case::to_text(vec![RuntimeValue::String("test".to_string())],
            vec![
                 ast_call("to_text", Vec::new())
            ],
            Ok(vec!["test".to_string().into()]))]
    #[case::to_text(vec![RuntimeValue::Number(42.into())],
            vec![
                 ast_call("to_text", Vec::new())
            ],
            Ok(vec!["42".to_string().into()]))]
    #[case::to_text(vec![RuntimeValue::Bool(true)],
            vec![
                 ast_call("to_text", Vec::new())
            ],
            Ok(vec!["true".to_string().into()]))]
    #[case::to_text(vec![RuntimeValue::Markdown(mq_markdown::Node::Heading(mq_markdown::Heading{depth: 1, values: vec!["Heading".to_string().into()], position: None}), None)],
            vec![
                 ast_call("to_text", Vec::new())
            ],
            Ok(vec!["Heading".to_string().into()]))]
    #[case::to_text(vec![RuntimeValue::String("Original".to_string())],
            vec![
                 ast_call("to_text",
                  vec![ast_node(ast::Expr::Literal(ast::Literal::String("Override".to_string())))])
            ],
            Ok(vec!["Override".to_string().into()]))]
    #[case::to_text(vec![RuntimeValue::Array(vec!["val1".to_string().into(), "val2".to_string().into()])],
            vec![
                 ast_call("to_text", vec![])
            ],
            Ok(vec!["val1,val2".to_string().into()]))]
    #[case::url_encode(vec![RuntimeValue::String("test string with spaces".to_string())],
            vec![
                 ast_call("url_encode", Vec::new())
            ],
            Ok(vec![RuntimeValue::String("test%20string%20with%20spaces".to_string())]))]
    #[case::url_encode(vec![RuntimeValue::String("test!@#$%^&*()".to_string())],
            vec![
                 ast_call("url_encode", Vec::new())
            ],
            Ok(vec![RuntimeValue::String("test%21%40%23%24%25%5E%26%2A%28%29".to_string())]))]
    #[case::url_encode(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "test string".to_string(), position: None}), None)],
            vec![
                 ast_call("url_encode", Vec::new())
            ],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "test%20string".to_string(), position: None}), None)]))]
    #[case::url_encode(vec![RuntimeValue::Number(1.into())],
            vec![
                 ast_call("url_encode", Vec::new())
            ],
            Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                         name: "url_encode".to_string(),
                                                         args: vec![1.to_string().into()]})))]
    #[case::update(vec!["".to_string().into()],
            vec![
                 ast_call("update", vec![
                  ast_node(ast::Expr::Literal(ast::Literal::Number(0.into()))),
                  ast_node(ast::Expr::Literal(ast::Literal::String("updated".to_string()))),
                 ])
            ],
            Ok(vec![RuntimeValue::String("updated".to_string())]))]
    #[case::update(vec!["".to_string().into()],
            vec![
                 ast_call("update", vec![
                    ast_call("to_strong", vec![
                        ast_node(ast::Expr::Literal(ast::Literal::String("text1".to_string()))),
                    ]),
                    ast_call("to_strong", vec![
                        ast_node(ast::Expr::Literal(ast::Literal::String("text2".to_string()))),
                    ])
                 ])
            ],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Strong(mq_markdown::Value{values: vec![mq_markdown::Node::Text(mq_markdown::Text{value: "text2".to_string(), position: None})], position: None}), None)]))]
    #[case::update(vec!["".to_string().into()],
            vec![
                 ast_call("update", vec![
                    ast_call("to_strong", vec![
                        ast_node(ast::Expr::Literal(ast::Literal::String("text1".to_string()))),
                    ]),
                    ast_node(ast::Expr::Literal(ast::Literal::String("text2".to_string()))),
                 ])
            ],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Strong(mq_markdown::Value{values: vec![mq_markdown::Node::Text(mq_markdown::Text{value: "text2".to_string(), position: None})], position: None}), None)]))]
    #[case::sort_string_array(vec![RuntimeValue::Array(vec![
                RuntimeValue::String("c".to_string()),
                RuntimeValue::String("a".to_string()),
                RuntimeValue::String("b".to_string()),
            ])],
            vec![
                ast_call("sort", Vec::new())
            ],
            Ok(vec![RuntimeValue::Array(vec![
                RuntimeValue::String("a".to_string()),
                RuntimeValue::String("b".to_string()),
                RuntimeValue::String("c".to_string()),
            ])]))]
    #[case::sort_number_array(vec![RuntimeValue::Array(vec![
                RuntimeValue::Number(3.into()),
                RuntimeValue::Number(1.into()),
                RuntimeValue::Number(2.into()),
            ])],
            vec![
                ast_call("sort", Vec::new())
            ],
            Ok(vec![RuntimeValue::Array(vec![
                RuntimeValue::Number(1.into()),
                RuntimeValue::Number(2.into()),
                RuntimeValue::Number(3.into()),
            ])]))]
    #[case::sort_empty_array(vec![RuntimeValue::Array(vec![])],
            vec![
                ast_call("sort", Vec::new())
            ],
            Ok(vec![RuntimeValue::Array(vec![])]))]
    #[case::uniq_string_array(vec![RuntimeValue::Array(vec![
                RuntimeValue::String("a".to_string()),
                RuntimeValue::String("b".to_string()),
                RuntimeValue::String("a".to_string()),
                RuntimeValue::String("c".to_string()),
                RuntimeValue::String("b".to_string()),
            ])],
            vec![
                ast_call("uniq", Vec::new())
            ],
            Ok(vec![RuntimeValue::Array(vec![
                RuntimeValue::String("a".to_string()),
                RuntimeValue::String("b".to_string()),
                RuntimeValue::String("c".to_string()),
            ])]))]
    #[case::uniq_number_array(vec![RuntimeValue::Array(vec![
                RuntimeValue::Number(1.into()),
                RuntimeValue::Number(2.into()),
                RuntimeValue::Number(1.into()),
                RuntimeValue::Number(3.into()),
                RuntimeValue::Number(2.into()),
            ])],
            vec![
                ast_call("uniq", Vec::new())
            ],
            Ok(vec![RuntimeValue::Array(vec![
                RuntimeValue::Number(1.into()),
                RuntimeValue::Number(2.into()),
                RuntimeValue::Number(3.into()),
            ])]))]
    #[case::uniq(vec![RuntimeValue::Number(1.into())],
            vec![
                ast_call("uniq", Vec::new())
            ],
            Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                         name: "uniq".to_string(),
                                                         args: vec![1.to_string().into()]})))]
    #[case::to_html(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "test".to_string(), position: None}), None)],
            vec![
                 ast_call("to_html", Vec::new())
            ],
            Ok(vec![RuntimeValue::String("<p>test</p>\n".to_string())]))]
    #[case::to_html(vec![RuntimeValue::Markdown(mq_markdown::Node::Heading(mq_markdown::Heading{depth: 1, values: vec!["Heading 1".to_string().into()], position: None}), None)],
            vec![
                 ast_call("to_html", Vec::new())
            ],
            Ok(vec![RuntimeValue::String("<h1>Heading 1</h1>\n".to_string())]))]
    #[case::to_html(vec![RuntimeValue::Markdown(mq_markdown::Node::Strong(mq_markdown::Value{values: vec!["Bold".to_string().into()], position: None}), None)],
            vec![
                 ast_call("to_html", Vec::new())
            ],
            Ok(vec![RuntimeValue::String("<p><strong>Bold</strong></p>\n".to_string())]))]
    #[case::to_html(vec![RuntimeValue::Markdown(mq_markdown::Node::Emphasis(mq_markdown::Value{values: vec!["Italic".to_string().into()], position: None}), None)],
            vec![
                 ast_call("to_html", Vec::new())
            ],
            Ok(vec![RuntimeValue::String("<p><em>Italic</em></p>\n".to_string())]))]
    #[case::to_html(vec![RuntimeValue::Markdown(mq_markdown::Node::Link(mq_markdown::Link{url: "https://example.com".to_string(), title: Some("Link Title".to_string()), values: vec!["Link Title".to_string().into()], position: None}), None)],
            vec![
                 ast_call("to_html", Vec::new())
            ],
            Ok(vec![RuntimeValue::String("<p><a href=\"https://example.com\" title=\"Link Title\">Link Title</a></p>\n".to_string())]))]
    #[case::to_html(vec![RuntimeValue::Markdown(mq_markdown::Node::Code(mq_markdown::Code{lang: Some("rust".to_string()), value: "println!(\"Hello\");".to_string(), position: None}), None)],
            vec![
                 ast_call("to_html", Vec::new())
            ],
            Ok(vec![RuntimeValue::String("<pre><code class=\"language-rust\">println!(&quot;Hello&quot;);\n</code></pre>\n".to_string())]))]
    #[case::to_html(vec![RuntimeValue::String("Plain text".to_string())],
            vec![
                 ast_call("to_html", Vec::new())
            ],
            Ok(vec![RuntimeValue::String("<p>Plain text</p>\n".to_string())]))]
    #[case::repeat_string(vec![RuntimeValue::String("abc".to_string())],
            vec![
                ast_call("repeat", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(3.into())))
                ])
            ],
            Ok(vec![RuntimeValue::String("abcabcabc".to_string())]))]
    #[case::repeat_markdown(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "abc".to_string(), position: None}), None)],
            vec![
                ast_call("repeat", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(3.into())))
                ])
            ],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "abcabcabc".to_string(), position: None}), None)]))]
    #[case::repeat_string_zero(vec![RuntimeValue::String("abc".to_string())],
            vec![
                ast_call("repeat", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(0.into())))
                ])
            ],
            Ok(vec![RuntimeValue::String("".to_string())]))]
    #[case::repeat_array(vec![RuntimeValue::Array(vec![
                RuntimeValue::String("a".to_string()),
                RuntimeValue::String("b".to_string()),
            ])],
            vec![
                ast_call("repeat", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(2.into())))
                ])
            ],
            Ok(vec![RuntimeValue::Array(vec![
                RuntimeValue::String("a".to_string()),
                RuntimeValue::String("b".to_string()),
                RuntimeValue::String("a".to_string()),
                RuntimeValue::String("b".to_string()),
            ])]))]
    #[case::repeat_array_zero(vec![RuntimeValue::Array(vec![
                RuntimeValue::String("a".to_string()),
                RuntimeValue::String("b".to_string()),
            ])],
            vec![
                ast_call("repeat", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(0.into())))
                ])
            ],
            Ok(vec![RuntimeValue::Array(vec![])]))]
    #[case::repeat_invalid_count(vec![RuntimeValue::String("abc".to_string())],
            vec![
                ast_call("repeat", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number((-1).into())))
                ])
            ],
            Ok(vec!["".to_string().into()]))]
    #[case::repeat_invalid_type(vec![RuntimeValue::Number(42.into())],
            vec![
                ast_call("repeat", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(3.into())))
                ])
            ],
            Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                    name: "repeat".to_string(),
                                                    args: vec![42.to_string().into(), 3.to_string().into()]})))]
    #[case::debug(vec![RuntimeValue::String("test".to_string())],
            vec![
                ast_call("debug", Vec::new())
            ],
            Ok(vec![RuntimeValue::String("test".to_string())]))]
    #[case::from_date(vec![RuntimeValue::String("2025-03-15T20:00:00+09:00".to_string())],
            vec![
                ast_call("from_date", vec![])
            ],
            Ok(vec![RuntimeValue::Number(1742036400000_i64.into())]))]
    #[case::from_date_invalid_format(vec![RuntimeValue::String("2021-01-01".to_string())],
            vec![
                ast_call("from_date", vec![])
            ],
            Err(InnerError::Eval(EvalError::RuntimeError(Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()}, "premature end of input".to_string()))))]
    #[case::from_date(vec![RuntimeValue::Number(1.into())],
            vec![
                ast_call("from_date", vec![])
            ],
            Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                         name: "from_date".to_string(),
                                                         args: vec![1.to_string().into()]})))]
    #[case::to_code_inline(vec![RuntimeValue::String("test1".to_string())],
            vec![
                  ast_call("to_code_inline", vec![
                        ast_node(ast::Expr::Literal(ast::Literal::String("elm".into()))),
                  ]),
            ],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::CodeInline(mq_markdown::CodeInline{value: "elm".into(), position: None}), None)]))]
    fn test_eval(
        token_arena: Rc<RefCell<Arena<Rc<Token>>>>,
        #[case] runtime_values: Vec<RuntimeValue>,
        #[case] program: Program,
        #[case] expected: Result<Vec<RuntimeValue>, InnerError>,
    ) {
        assert_eq!(
            Evaluator::new(ModuleLoader::new(None), token_arena)
                .eval(&program, runtime_values.into_iter()),
            expected
        );
    }

    #[rstest]
    #[case::type_none(vec![RuntimeValue::NONE],
       vec![
            ast_call("type", Vec::new())
       ],
       Ok(vec![RuntimeValue::String("None".to_string())]))]
    #[case::to_text(vec![RuntimeValue::NONE],
            vec![
                 ast_call("to_text", Vec::new())
            ],
            Ok(vec!["".to_string().into()]))]
    #[case::starts_with(vec![RuntimeValue::NONE],
       vec![
            ast_call("starts_with", vec![
                ast_node(ast::Expr::Literal(ast::Literal::String("te".to_string())))
            ])
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::ends_with(vec![RuntimeValue::NONE],
       vec![
            ast_call("ends_with", vec![
                ast_node(ast::Expr::Literal(ast::Literal::String("te".to_string())))
            ])
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::rindex(vec![RuntimeValue::NONE],
       vec![
            ast_call("rindex", vec![
                ast_node(ast::Expr::Literal(ast::Literal::String("String".to_string())))
            ])
       ],
       Ok(vec![RuntimeValue::Number((-1).into())]))]
    #[case::utf8bytelen(vec![RuntimeValue::NONE],
       vec![
            ast_call("utf8bytelen", Vec::new())
       ],
       Ok(vec![RuntimeValue::Number(0.into())]))]
    #[case::index(vec![RuntimeValue::NONE],
       vec![
            ast_call("index", vec![
                ast_node(ast::Expr::Literal(ast::Literal::String("test".to_string())))
            ])
       ],
       Ok(vec![RuntimeValue::Number((-1).into())]))]
    #[case::del(vec![RuntimeValue::NONE],
        vec![
              ast_call("del", vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(4.into()))),
              ]),
        ],
        Ok(vec![RuntimeValue::NONE]))]
    #[case::downcase(vec![RuntimeValue::NONE],
       vec![
            ast_call("downcase", Vec::new())
       ],
       Ok(vec![RuntimeValue::NONE]))]
    #[case::slice(vec![RuntimeValue::NONE],
       vec![
            ast_call("slice", vec![
                ast_node(ast::Expr::Literal(ast::Literal::Number(0.into()))),
                ast_node(ast::Expr::Literal(ast::Literal::Number(4.into()))),
            ])
       ],
       Ok(vec![RuntimeValue::NONE]))]
    #[case::slice(vec![RuntimeValue::NONE],
       vec![
            ast_call("len", vec![])
       ],
       Ok(vec![RuntimeValue::Number(0.into())]))]
    #[case::slice(vec![RuntimeValue::NONE],
       vec![
            ast_call("upcase", vec![])
       ],
       Ok(vec![RuntimeValue::NONE]))]
    #[case::to_code(vec![RuntimeValue::NONE],
        vec![
              ast_call("to_code", vec![
                ast_node(ast::Expr::Literal(ast::Literal::None)),
              ]),
        ],
        Ok(vec![RuntimeValue::NONE]))]
    #[case::to_code(vec![RuntimeValue::NONE],
        vec![
              ast_call("update", vec![
                ast_node(ast::Expr::Literal(ast::Literal::None)),
              ]),
        ],
        Ok(vec![RuntimeValue::NONE]))]
    #[case::to_code_inline(vec![RuntimeValue::NONE],
        vec![
              ast_call("to_code_inline", vec![
                ast_node(ast::Expr::Literal(ast::Literal::None)),
              ]),
        ],
        Ok(vec![RuntimeValue::NONE]))]
    #[case::to_link(vec![RuntimeValue::NONE],
        vec![
              ast_call("to_link", vec![
                ast_node(ast::Expr::Literal(ast::Literal::None)),
                ast_node(ast::Expr::Literal(ast::Literal::None)),
                ast_node(ast::Expr::Literal(ast::Literal::None)),
              ]),
        ],
        Ok(vec![RuntimeValue::NONE]))]
    #[case::to_strong(vec![RuntimeValue::NONE],
        vec![
              ast_call("to_strong", vec![
                ast_node(ast::Expr::Literal(ast::Literal::None)),
              ]),
        ],
        Ok(vec![RuntimeValue::NONE]))]
    #[case::to_em(vec![RuntimeValue::NONE],
        vec![
              ast_call("to_em", vec![
                ast_node(ast::Expr::Literal(ast::Literal::None)),
              ]),
        ],
        Ok(vec![RuntimeValue::NONE]))]
    #[case::to_md_text(vec![RuntimeValue::NONE],
        vec![
              ast_call("to_md_text", vec![
                ast_node(ast::Expr::Literal(ast::Literal::None)),
              ]),
        ],
        Ok(vec![RuntimeValue::NONE]))]
    #[case::to_md_list(vec![RuntimeValue::NONE],
        vec![
              ast_call("to_md_list", vec![
                ast_node(ast::Expr::Literal(ast::Literal::None)),
                ast_node(ast::Expr::Literal(ast::Literal::None)),
              ]),
        ],
        Ok(vec![RuntimeValue::NONE]))]
    fn test_eval_process_none(
        token_arena: Rc<RefCell<Arena<Rc<Token>>>>,
        #[case] runtime_values: Vec<RuntimeValue>,
        #[case] program: Program,
        #[case] expected: Result<Vec<RuntimeValue>, InnerError>,
    ) {
        let mut eval = Evaluator::new(ModuleLoader::new(None), token_arena);
        eval.options.filter_none = false;

        assert_eq!(eval.eval(&program, runtime_values.into_iter()), expected);
    }

    #[test]
    fn test_include() {
        let (temp_dir, temp_file_path) =
            mq_test::create_file("test_module.mq", "def func1(): 42; | let val1 = 1");

        defer! {
            if temp_file_path.exists() {
                std::fs::remove_file(&temp_file_path).expect("Failed to delete temp file");
            }
        }

        let loader = ModuleLoader::new(Some(vec![temp_dir.clone()]));
        let program = vec![
            Rc::new(ast::Node {
                token_id: 0.into(),
                expr: Rc::new(ast::Expr::Include(ast::Literal::String(
                    "test_module".to_string(),
                ))),
            }),
            Rc::new(ast::Node {
                token_id: 0.into(),
                expr: Rc::new(ast::Expr::Call(ast::Ident::new("func1"), vec![], false)),
            }),
        ];
        assert_eq!(
            Evaluator::new(loader, token_arena()).eval(
                &program,
                vec![RuntimeValue::String("".to_string())].into_iter()
            ),
            Ok(vec![RuntimeValue::Number(42.into())])
        );
    }
}
