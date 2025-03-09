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

use compact_str::CompactString;
use env::Env;
use error::EvalError;
use itertools::Itertools;
use log::debug;
use runtime_value::RuntimeValue;

#[derive(Debug, Clone)]
pub struct Evaluator {
    env: Rc<RefCell<Env>>,
    token_arena: Rc<RefCell<Arena<Rc<Token>>>>,
    pub module_loader: module::ModuleLoader,
}

impl Evaluator {
    pub(crate) fn new(
        module_loader: module::ModuleLoader,
        token_arena: Rc<RefCell<Arena<Rc<Token>>>>,
    ) -> Self {
        Self {
            env: Rc::new(RefCell::new(Env::new(None))),
            module_loader,
            token_arena,
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
                                params.iter().map(Rc::clone).collect_vec(),
                                program.iter().map(Rc::clone).collect_vec(),
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
                self.eval_program(&program, &runtime_value, Rc::clone(&self.env))
                    .map_err(InnerError::Eval)
            })
            .collect()
    }

    pub(crate) fn defined_runtime_values(&self) -> Vec<(AstIdentName, Box<RuntimeValue>)> {
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
                            params.iter().map(Rc::clone).collect_vec(),
                            program.iter().map(Rc::clone).collect_vec(),
                            Rc::clone(&self.env),
                        ),
                    );
                }
            });

            module.vars.iter().try_for_each(|node| {
                if let ast::Expr::Let(ident, _) = &*node.expr {
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

    #[inline(always)]
    fn eval_program(
        &mut self,
        program: &Program,
        runtime_value: &RuntimeValue,
        env: Rc<RefCell<Env>>,
    ) -> Result<RuntimeValue, EvalError> {
        program
            .iter()
            .try_fold(runtime_value.clone(), |runtime_value, ast| {
                match &*ast.expr {
                    ast::Expr::Selector(ident) => {
                        Ok(Self::eval_selector_expr(&runtime_value, ident))
                    }
                    ast::Expr::Include(module_id) => {
                        self.eval_include(module_id.to_owned())?;
                        Ok(runtime_value)
                    }
                    ast::Expr::Def(ident, params, program) => {
                        let function = RuntimeValue::Function(
                            params.iter().map(Rc::clone).collect_vec(),
                            program.iter().map(Rc::clone).collect_vec(),
                            Rc::clone(&env),
                        );
                        env.borrow_mut().define(ident, function.clone());
                        Ok(function)
                    }
                    ast::Expr::Let(ident, node) => {
                        let let_ =
                            self.eval_expr(&runtime_value, Rc::clone(node), Rc::clone(&env))?;
                        env.borrow_mut().define(ident, let_);
                        Ok(runtime_value)
                    }
                    ast::Expr::Call(_, _, _) => {
                        self.eval_expr(&runtime_value, Rc::clone(ast), Rc::clone(&env))
                    }
                    ast::Expr::Literal(ast::Literal::Bool(b)) => {
                        if *b {
                            Ok(runtime_value)
                        } else {
                            Ok(RuntimeValue::NONE)
                        }
                    }
                    ast::Expr::Literal(ast::Literal::String(s)) => {
                        Ok(RuntimeValue::String(s.to_string()))
                    }
                    ast::Expr::Literal(ast::Literal::Number(n)) => Ok(RuntimeValue::Number(*n)),
                    ast::Expr::Literal(ast::Literal::None) => Ok(RuntimeValue::NONE),
                    ast::Expr::Self_ => Ok(runtime_value),
                    ast::Expr::While(_, _) => {
                        self.eval_while(&runtime_value, Rc::clone(ast), Rc::clone(&env))
                    }
                    ast::Expr::Until(_, _) => {
                        self.eval_until(&runtime_value, Rc::clone(ast), Rc::clone(&env))
                    }
                    ast::Expr::Foreach(_, _, _) => {
                        self.eval_foreach(&runtime_value, Rc::clone(ast), Rc::clone(&env))
                    }
                    ast::Expr::If(_) => {
                        self.eval_if(&runtime_value, Rc::clone(ast), Rc::clone(&env))
                    }
                    ast::Expr::Ident(_) => {
                        self.eval_expr(&runtime_value, Rc::clone(ast), Rc::clone(&env))
                    }
                }
            })
    }

    #[inline(always)]
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

    #[inline(always)]
    fn eval_selector_expr(runtime_value: &RuntimeValue, ident: &ast::Selector) -> RuntimeValue {
        match runtime_value {
            RuntimeValue::Markdown(node_value) => {
                if builtin::eval_selector(node_value.clone(), ident).is_empty() {
                    RuntimeValue::NONE
                } else {
                    RuntimeValue::Markdown(node_value.clone())
                }
            }
            _ => RuntimeValue::NONE,
        }
    }

    #[inline(always)]
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
            ast::Expr::Ident(ident) => env
                .borrow()
                .resolve(ident)
                .map(|o| *o)
                .map_err(|e| e.to_eval_error((*node).clone(), Rc::clone(&self.token_arena))),
            ast::Expr::Selector(ident) => match runtime_value {
                RuntimeValue::Markdown(node_value) => Ok(RuntimeValue::Bool(
                    !builtin::eval_selector(node_value.clone(), ident).is_empty(),
                )),
                _ => Err(EvalError::InvalidTypes {
                    token: (*self.token_arena.borrow()[node.token_id]).clone(),
                    name: TokenKind::Selector(CompactString::new("")).to_string(),
                    args: vec![runtime_value.to_string().into()],
                }),
            },
            ast::Expr::Def(ident, params, program) => {
                let function = RuntimeValue::Function(
                    params.iter().map(Rc::clone).collect_vec(),
                    program.iter().map(Rc::clone).collect_vec(),
                    Rc::clone(&env),
                );
                env.borrow_mut().define(ident, function.clone());
                Ok(function)
            }
            ast::Expr::Let(ident, _) => {
                let let_ = self.eval_expr(runtime_value, Rc::clone(&node), Rc::clone(&env))?;
                env.borrow_mut().define(ident, let_);
                Ok(runtime_value.clone())
            }
            ast::Expr::While(_, _) => self.eval_while(runtime_value, node, env),
            ast::Expr::Until(_, _) => self.eval_until(runtime_value, node, env),
            ast::Expr::Foreach(_, _, _) => self.eval_foreach(runtime_value, node, env),
            ast::Expr::If(_) => self.eval_if(runtime_value, node, env),
        }
    }

    #[inline(always)]
    fn eval_foreach(
        &mut self,
        runtime_value: &RuntimeValue,
        node: Rc<ast::Node>,
        env: Rc<RefCell<Env>>,
    ) -> Result<RuntimeValue, EvalError> {
        if let ast::Expr::Foreach(ident, values, body) = &*node.expr {
            let values = self.eval_expr(runtime_value, Rc::clone(values), Rc::clone(&env))?;

            if !values.is_array() {
                return Err(EvalError::InvalidTypes {
                    token: (*self.token_arena.borrow()[node.token_id]).clone(),
                    name: TokenKind::Foreach.to_string(),
                    args: vec![values.to_string().into()],
                });
            }

            let mut runtime_values: Vec<RuntimeValue> = Vec::with_capacity(values.len());

            if let RuntimeValue::Array(values) = values {
                let env = Rc::new(RefCell::new(Env::new(Some(Rc::downgrade(&env)))));

                for value in values.into_iter() {
                    env.borrow_mut().define(ident, value);
                    runtime_values.push(self.eval_program(body, runtime_value, Rc::clone(&env))?);
                }
            }

            Ok(RuntimeValue::Array(runtime_values))
        } else {
            Err(EvalError::InvalidTypes {
                token: (*self.token_arena.borrow()[node.token_id]).clone(),
                name: TokenKind::Foreach.to_string(),
                args: vec![runtime_value.to_string().into()],
            })
        }
    }

    #[inline(always)]
    fn eval_until(
        &mut self,
        runtime_value: &RuntimeValue,
        node: Rc<ast::Node>,
        env: Rc<RefCell<Env>>,
    ) -> Result<RuntimeValue, EvalError> {
        if let ast::Expr::Until(cond, body) = &*node.expr {
            let mut runtime_value = runtime_value.clone();
            let env = Rc::new(RefCell::new(Env::new(Some(Rc::downgrade(&env)))));
            let mut cond_value =
                self.eval_expr(&runtime_value, Rc::clone(cond), Rc::clone(&env))?;

            while cond_value.is_true() {
                runtime_value = self.eval_program(body, &runtime_value, Rc::clone(&env))?;
                cond_value = self.eval_expr(&runtime_value, Rc::clone(cond), Rc::clone(&env))?;
            }

            Ok(runtime_value)
        } else {
            Err(EvalError::InvalidTypes {
                token: (*self.token_arena.borrow()[node.token_id]).clone(),
                name: TokenKind::While.to_string(),
                args: vec![runtime_value.to_string().into()],
            })
        }
    }

    #[inline(always)]
    fn eval_while(
        &mut self,
        runtime_value: &RuntimeValue,
        node: Rc<ast::Node>,
        env: Rc<RefCell<Env>>,
    ) -> Result<RuntimeValue, EvalError> {
        if let ast::Expr::While(cond, body) = &*node.expr {
            let mut runtime_value = runtime_value.clone();
            let env = Rc::new(RefCell::new(Env::new(Some(Rc::downgrade(&env)))));
            let mut cond_value =
                self.eval_expr(&runtime_value, Rc::clone(cond), Rc::clone(&env))?;
            let mut values = Vec::with_capacity(100_000);

            while cond_value.is_true() {
                runtime_value = self.eval_program(body, &runtime_value, Rc::clone(&env))?;
                cond_value = self.eval_expr(&runtime_value, Rc::clone(cond), Rc::clone(&env))?;
                values.push(runtime_value.clone());
            }

            Ok(RuntimeValue::Array(values))
        } else {
            Err(EvalError::InvalidTypes {
                token: (*self.token_arena.borrow()[node.token_id]).clone(),
                name: TokenKind::While.to_string(),
                args: vec![runtime_value.to_string().into()],
            })
        }
    }

    #[inline(always)]
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
            Err(EvalError::InvalidTypes {
                token: (*self.token_arena.borrow()[node.token_id]).clone(),
                name: TokenKind::While.to_string(),
                args: vec![runtime_value.to_string().into()],
            })
        }
    }

    #[inline(always)]
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
            if let RuntimeValue::Function(params, program, fn_env) = &*fn_value {
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

                let new_env = Rc::new(RefCell::new(Env::new(Some(Rc::downgrade(fn_env)))));

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
                debug!("current env: {}", new_env.borrow());
                self.eval_program(program, runtime_value, new_env)
            } else if let RuntimeValue::NativeFunction(ident) = *fn_value {
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

    #[inline(always)]
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
}

#[cfg(test)]
mod tests {
    use crate::range::Range;
    use crate::{AstExpr, AstNode, ModuleLoader};
    use crate::{Token, TokenKind};

    use super::*;
    use Program;
    use rstest::{fixture, rstest};
    use std::fs::File;
    use std::io::Write;

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

    #[rstest]
    #[case::starts_with(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("starts_with"),
                    vec![ast_node(ast::Expr::Literal(ast::Literal::String("te".to_string())))], false))
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::starts_with(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "test".to_string(), position: None}))],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("starts_with"),
                     vec![ast_node(ast::Expr::Literal(ast::Literal::String("te".to_string())))], false))
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::starts_with(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("starts_with"), vec![
                ast_node(ast::Expr::Literal(ast::Literal::String("st".to_string())))
            ], false))
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::ends_with(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("ends_with"), vec![
                ast_node(ast::Expr::Literal(ast::Literal::String("st".to_string())))
            ], false))
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::ends_with(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "test".to_string(), position: None}))],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("ends_with"), vec![
                ast_node(ast::Expr::Literal(ast::Literal::String("st".to_string())))
            ], false))
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::ends_with(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("ends_with"), vec![
                ast_node(ast::Expr::Literal(ast::Literal::String("te".to_string())))
            ], false))
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::downcase(vec![RuntimeValue::String("TEST".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("downcase"), Vec::new(), false))
       ],
       Ok(vec![RuntimeValue::String("test".to_string())]))]
    #[case::downcase(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "TEST".to_string(), position: None}))],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("downcase"), Vec::new(), false))
       ],
       Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "test".to_string(), position: None}))]))]
    #[case::downcase(vec![RuntimeValue::NONE],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("downcase"), Vec::new(), false))
       ],
       Ok(vec![RuntimeValue::NONE]))]
    #[case::upcase(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("upcase"), Vec::new(), false))
       ],
       Ok(vec![RuntimeValue::String("TEST".to_string())]))]
    #[case::upcase(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "test".to_string(), position: None}))],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("upcase"), Vec::new(), false))
       ],
       Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "TEST".to_string(), position: None}))]))]
    #[case::replace(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("replace"), vec![
                ast_node(ast::Expr::Literal(ast::Literal::String("test".to_string()))),
                ast_node(ast::Expr::Literal(ast::Literal::String("exam".to_string())))
            ], false))
       ],
       Ok(vec![RuntimeValue::String("examString".to_string())]))]
    #[case::replace(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "testString".to_string(), position: None}))],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("replace"), vec![
                ast_node(ast::Expr::Literal(ast::Literal::String("test".to_string()))),
                ast_node(ast::Expr::Literal(ast::Literal::String("exam".to_string())))
            ], false))
       ],
       Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "examString".to_string(), position: None}))]))]
    #[case::replace_regex(vec![RuntimeValue::String("test123".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("gsub"), vec![
                ast_node(ast::Expr::Literal(ast::Literal::String(r"\d+".to_string()))),
                ast_node(ast::Expr::Literal(ast::Literal::String("456".to_string())))
            ], false))
       ],
       Ok(vec![RuntimeValue::String("test456".to_string())]))]
    #[case::replace_regex(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "test123".to_string(), position: None}))],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("gsub"), vec![
                ast_node(ast::Expr::Literal(ast::Literal::String(r"\d+".to_string()))),
                ast_node(ast::Expr::Literal(ast::Literal::String("456".to_string())))
            ], false))
       ],
       Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "test456".to_string(), position: None}))]))]
    #[case::len(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("len"), Vec::new(), false))
       ],
       Ok(vec![RuntimeValue::Number(10.into())]))]
    #[case::len(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "testString".to_string(), position: None}))],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("len"), Vec::new(), false))
       ],
       Ok(vec![RuntimeValue::Number(10.into())]))]
    #[case::len(vec![RuntimeValue::String("テスト".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("len"), Vec::new(), false))
       ],
       Ok(vec![RuntimeValue::Number(3.into())]))]
    #[case::len(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "テスト".to_string(), position: None}))],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("len"), Vec::new(), false))
       ],
       Ok(vec![RuntimeValue::Number(3.into())]))]
    #[case::utf8bytelen(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("utf8bytelen"), Vec::new(), false))
       ],
       Ok(vec![RuntimeValue::Number(4.into())]))]
    #[case::utf8bytelen(vec![RuntimeValue::String("テスト".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("utf8bytelen"), Vec::new(), false))
       ],
       Ok(vec![RuntimeValue::Number(9.into())]))]
    #[case::utf8bytelen(vec![RuntimeValue::String("😊".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("utf8bytelen"), Vec::new(), false))
       ],
       Ok(vec![RuntimeValue::Number(4.into())]))]
    #[case::index(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("index"), vec![
                ast_node(ast::Expr::Literal(ast::Literal::String("test".to_string())))
            ], false))
       ],
       Ok(vec![RuntimeValue::Number(0.into())]))]
    #[case::index(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "testString".to_string(), position: None}))],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("index"), vec![
                ast_node(ast::Expr::Literal(ast::Literal::String("test".to_string())))
            ], false))
       ],
       Ok(vec![RuntimeValue::Number(0.into())]))]
    #[case::rindex(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("rindex"), vec![
                ast_node(ast::Expr::Literal(ast::Literal::String("String".to_string())))
            ], false))
       ],
       Ok(vec![RuntimeValue::Number(4.into())]))]
    #[case::rindex(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "testString".to_string(), position: None}))],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("rindex"), vec![
                ast_node(ast::Expr::Literal(ast::Literal::String("String".to_string())))
            ], false))
       ],
       Ok(vec![RuntimeValue::Number(4.into())]))]
    #[case::eq(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("eq"), vec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("eq".to_string()))),
                    ast_node(ast::Expr::Literal(ast::Literal::String("eq".to_string())))
                ], false))
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::eq(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("eq"), vec![
                ast_node(ast::Expr::Literal(ast::Literal::String("eq".to_string()))),
                ast_node(ast::Expr::Literal(ast::Literal::String("eq1".to_string())))
            ], false))
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::ne(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("ne"), vec![
                ast_node(ast::Expr::Literal(ast::Literal::String("eq".to_string()))),
                ast_node(ast::Expr::Literal(ast::Literal::String("eq1".to_string())))
            ], false))
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::ne(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("ne"), vec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("eq".to_string()))),
                    ast_node(ast::Expr::Literal(ast::Literal::String("eq".to_string())))
                ], false))
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::ne(vec![RuntimeValue::Number(1.3.into())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("ne"), vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.3.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.3.into()))),
                ], false)),
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::ne(vec![RuntimeValue::Number(1.3.into())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("ne"), vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.3.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.2.into()))),
                ], false)),
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::gt(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("gt"), vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(2.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                ], false)),
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::gt(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("gt"), vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(2.into()))),
                ], false)),
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::gt(vec![RuntimeValue::Number(1.3.into())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("gt"), vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.4.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.3.into()))),
                ], false)),
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::gt(vec![RuntimeValue::Number(1.3.into())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("gt"), vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.3.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.4.into()))),
                ], false)),
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::gte(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("gte"), vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                ], false)),
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::gte(vec![RuntimeValue::String("test".to_string())],
       vec! [
            ast_node(ast::Expr::Call(ast::Ident::new("gte"), vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(2.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                ], false)),
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::gte(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("gte"), vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(2.into()))),
                ], false)),
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::gte(vec![RuntimeValue::Number(1.3.into())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("gte"), vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.3.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.3.into()))),
                ], false)),
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::gte(vec![RuntimeValue::Number(1.3.into())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("gte"), vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.2.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.3.into()))),
                ], false)),
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::lt(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("lt"), vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(2.into()))),
                ], false)),
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::lt(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("lt"), vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(2.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                ], false)),
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::lt(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("lt"), vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.2.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.3.into()))),
                ], false))
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::lt(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("lt"), vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.3.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.2.into()))),
                ], false))
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::lte(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("lte"), vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                ], false))
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::lte(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("lte"), vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(2.into()))),
                ], false))
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::lte(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("lte"), vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(2.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                ], false))
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::lte(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("lte"), vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.3.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.3.into()))),
                ], false))
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::lte(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("lte"), vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.2.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.3.into()))),
                ], false))
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::lte(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("lte"), vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.4.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.3.into()))),
                ], false)),
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::add(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("add"), vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                ], false)),
       ],
       Ok(vec![RuntimeValue::Number(2.into())]))]
    #[case::add(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("add"), vec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("te".to_string()))),
                    ast_node(ast::Expr::Literal(ast::Literal::String("st".to_string()))),
                ], false)),
       ],
       Ok(vec![RuntimeValue::String("test".to_string())]))]
    #[case::add(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("add"), vec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("te".to_string()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                ], false)),
       ],
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                         name: "add".to_string(),
                                                         args: vec!["te".into(), 1.to_string().into()]})))]
    #[case::add(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("add"), vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.3.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.3.into()))),
                ], false)),
       ],
       Ok(vec![RuntimeValue::Number(2.6.into())]))]
    #[case::sub(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("sub"), vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                ], false)),
       ],
       Ok(vec![RuntimeValue::Number(0.into())]))]
    #[case::sub(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("sub"), vec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("te".to_string()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                ], false)),
       ],
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                         name: "sub".to_string(),
                                                         args: vec!["te".to_string().into(), 1.to_string().into()]})))]
    #[case::sub(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("sub"), vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.3.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.2.into()))),
                ], false))
       ],
       Ok(vec![RuntimeValue::Number(0.10000000000000009.into())]))]
    #[case::div(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("div"), vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                ], false))
       ],
       Ok(vec![RuntimeValue::Number(1.into())]))]
    #[case::div(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("div"), vec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("te".to_string()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                ], false))
       ],
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                         name: "div".to_string(),
                                                         args: vec!["te".to_string().into(), 1.to_string().into()]})))]
    #[case::div(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("div"), vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(0.into()))),
                ], false))
       ],
       Err(InnerError::Eval(EvalError::ZeroDivision(Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()}))))]
    #[case::div(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("div"), vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.3.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.1.into()))),
                ], false))
       ],
       Ok(vec![RuntimeValue::Number(1.1818181818181817.into())]))]
    #[case::mul(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("mul"), vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(2.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                ], false)),
       ],
       Ok(vec![RuntimeValue::Number(2.into())]))]
    #[case::mul(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("mul"), vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(2.0.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.3.into()))),
                ], false)),
       ],
       Ok(vec![RuntimeValue::Number(2.6.into())]))]
    #[case::mul(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("mul"), vec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("te".to_string()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                ], false)),
       ],
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                         name: "mul".to_string(),
                                                         args: vec!["te".to_string().into(), 1.to_string().into()]})))]
    #[case::mod_(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("mod"), vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(2.into()))),
                ], false)),
       ],
       Ok(vec![RuntimeValue::Number(1.into())]))]
    #[case::mod_(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("mod"), vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.1.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(2.0.into()))),
                ], false)),
       ],
       Ok(vec![RuntimeValue::Number(1.1.into())]))]
    #[case::mod_(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("mod"), vec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("te".to_string()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                ], false)),
       ],
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                    name: "mod".to_string(),
                                                    args: vec!["te".to_string().into(), 1.to_string().into()]})))]
    #[case::pow(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("pow"), vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(2.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(3.into()))),
                ], false)),
       ],
       Ok(vec![RuntimeValue::Number(8.into())]))]
    #[case::pow(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("pow"), vec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("te".to_string()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                ], false)),
       ],
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                    name: "pow".to_string(),
                                                    args: vec!["te".to_string().into(), 1.to_string().into()]})))]
    #[case::and(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("and"), vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(true))),
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(true))),
                ], false)),
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::and(vec![RuntimeValue::TRUE],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("and"), vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(true))),
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(false))),
                ], false)),
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::and(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("and"), vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(false))),
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(true))),
                ], false)),
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::and(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("and"), vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(false))),
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(false))),
                ], false)),
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::or(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("or"), vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(true))),
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(true))),
                ], false)),
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::or(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("or"), vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(true))),
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(false))),
                ], false)),
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::or(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("or"), vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(false))),
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(true))),
                ], false)),
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::or(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("or"), vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(false))),
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(false))),
                ], false)),
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::not(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("not"), vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(true))),
                ], false)),
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::not(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("not"), vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(false))),
                ], false)),
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::to_string(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("to_string"), Vec::new(), false))
       ],
       Ok(vec![RuntimeValue::String("test".to_string())]))]
    #[case::to_string(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "test".to_string(), position: None}))],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("to_string"), Vec::new(), false))
       ],
       Ok(vec![RuntimeValue::String("test".to_string())]))]
    #[case::split1(vec![RuntimeValue::String("test1,test2".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("split"), vec![
                        ast_node(ast::Expr::Literal(ast::Literal::String(",".to_string())))].into()
                        , false))
       ],
       Ok(vec![RuntimeValue::Array(vec![RuntimeValue::String("test1".to_string()), RuntimeValue::String("test2".to_string())])]))]
    #[case::split2(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "test1,test2".to_string(), position: None}))],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("split"), vec![
                ast_node(ast::Expr::Literal(ast::Literal::String(",".to_string())))
            ], false))
       ],
       Ok(vec![RuntimeValue::Array(vec![RuntimeValue::String("test1".to_string()), RuntimeValue::String("test2".to_string())])]))]
    #[case::join1(vec![RuntimeValue::String("test1,test2".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("join"), vec![
                ast_node(ast::Expr::Call(ast::Ident::new("split"), vec![
                    ast_node(ast::Expr::Literal(ast::Literal::String(",".to_string())))
                ], false)),
                ast_node(ast::Expr::Literal(ast::Literal::String("#".to_string())))
            ], false))
       ],
       Ok(vec![RuntimeValue::String("test1#test2".to_string())]))]
    #[case::base641(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("base64"), vec![
                ast_node(ast::Expr::Literal(ast::Literal::String("test".to_string())))
            ], false))
       ],
       Ok(vec![RuntimeValue::String("dGVzdA==".to_string())]))]
    #[case::base642(vec![RuntimeValue::String("dGVzdA==".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("base64d"), vec![
                ast_node(ast::Expr::Literal(ast::Literal::String("dGVzdA==".to_string())))
            ], false))
       ],
       Ok(vec![RuntimeValue::String("test".to_string())]))]
    #[case::def(vec![RuntimeValue::String("test1,test2".to_string())],
       vec![
            ast_node(ast::Expr::Def(
                ast::Ident::new("split2"),
                vec![
                    ast_node(ast::Expr::Ident(ast::Ident::new("str"))),
                ],
                vec![ast_node(ast::Expr::Call(
                    ast::Ident::new("split"),
                    vec![
                        ast_node(ast::Expr::Ident(ast::Ident::new("str"))),
                        ast_node(ast::Expr::Literal(ast::Literal::String(",".to_string()))),
                    ], false
                ))]
            )),
            ast_node(ast::Expr::Call(ast::Ident::new("split2"), vec![
                ast_node(ast::Expr::Literal(ast::Literal::String("test1,test2".to_string()))),
            ], false)),
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
                vec![ast_node(ast::Expr::Call(
                    ast::Ident::new("add"),
                    vec![
                        ast_node(ast::Expr::Ident(ast::Ident::new("str1"))),
                        ast_node(ast::Expr::Ident(ast::Ident::new("str2"))),
                    ], false
                ))]
            )),
            ast_node(ast::Expr::Call(ast::Ident::new("concat_self"), vec![
                ast_node(ast::Expr::Literal(ast::Literal::String("Hello".to_string()))),
                ast_node(ast::Expr::Literal(ast::Literal::String("World".to_string()))),
            ], false))
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
                vec![ast_node(ast::Expr::Call(
                    ast::Ident::new("add"),
                    vec![
                        ast_node(ast::Expr::Ident(ast::Ident::new("str1"))),
                        ast_node(ast::Expr::Ident(ast::Ident::new("str2"))),
                    ], false
                ))]
            )),
            ast_node(ast::Expr::Call(ast::Ident::new("prepend_self"), vec![
                ast_node(ast::Expr::Literal(ast::Literal::String("test".to_string())))
            ], false))
       ],
       Ok(vec![RuntimeValue::String("Testtest".to_string())]))]
    #[case::type_string(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("type"), Vec::new(), false))
       ],
       Ok(vec![RuntimeValue::String("string".to_string())]))]
    #[case::type_int(vec![RuntimeValue::Number(42.into())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("type"), Vec::new(), false))
       ],
       Ok(vec![RuntimeValue::String("number".to_string())]))]
    #[case::type_bool(vec![RuntimeValue::TRUE],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("type"), Vec::new(), false))
       ],
       Ok(vec![RuntimeValue::String("bool".to_string())]))]
    #[case::type_array(vec![RuntimeValue::Array(vec![RuntimeValue::String("test".to_string())])],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("type"), Vec::new(), false))
       ],
       Ok(vec![RuntimeValue::String("array".to_string())]))]
    #[case::type_none(vec![RuntimeValue::NONE],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("type"), Vec::new(), false))
       ],
       Ok(vec![RuntimeValue::String("None".to_string())]))]
    #[case::min(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("min"), vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(2.into()))),
                ], false))
        ],
       Ok(vec![RuntimeValue::Number(1.into())]))]
    #[case::min(vec![RuntimeValue::Number(1.into())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("min"), vec![
                ast_node(ast::Expr::Literal(ast::Literal::Number(2.into()))),
            ], false))
        ],
       Ok(vec![RuntimeValue::Number(1.into())]))]
    #[case::min(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("min"), vec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("te".to_string()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                ], false))
            ],
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                    name: "min".to_string(),
                                                    args: vec!["te".to_string().into(), 1.to_string().into()]})))]
    #[case::max(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("max"), vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(2.into()))),
                ], false))
            ],
       Ok(vec![RuntimeValue::Number(2.into())]))]
    #[case::max(vec![RuntimeValue::Number(3.into())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("max"), vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                ], false))
            ],
       Ok(vec![RuntimeValue::Number(3.into())]))]
    #[case::max(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("max"), vec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("te".to_string()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                ], false))
            ],
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                    name: "max".to_string(),
                                                    args: vec!["te".to_string().into(), 1.to_string().into()]})))]
    #[case::trim(vec![RuntimeValue::String("  test  ".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("trim"), Vec::new(), false))
       ],
       Ok(vec![RuntimeValue::String("test".to_string())]))]
    #[case::trim(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "  test  ".to_string(), position: None}))],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("trim"), Vec::new(), false))
       ],
       Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "test".to_string(), position: None}))]))]
    #[case::slice(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "testString".to_string(), position: None}))],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("slice"), vec![
                ast_node(ast::Expr::Literal(ast::Literal::Number(0.into()))),
                ast_node(ast::Expr::Literal(ast::Literal::Number(4.into()))),
            ], false))
       ],
       Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "test".to_string(), position: None}))]))]
    #[case::slice(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("slice"), vec![
                ast_node(ast::Expr::Literal(ast::Literal::Number(0.into()))),
                ast_node(ast::Expr::Literal(ast::Literal::Number(4.into()))),
            ], false))
       ],
       Ok(vec![RuntimeValue::String("test".to_string())]))]
    #[case::slice(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("slice"), vec![
                ast_node(ast::Expr::Literal(ast::Literal::Number(4.into()))),
                ast_node(ast::Expr::Literal(ast::Literal::Number(10.into()))),
            ], false))
       ],
       Ok(vec![RuntimeValue::String("String".to_string())]))]
    #[case::match_regex(vec![RuntimeValue::String("test123".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("match"), vec![
                ast_node(ast::Expr::Literal(ast::Literal::String(r"\d+".to_string()))),
            ], false))
       ],
       Ok(vec![RuntimeValue::Array(vec![RuntimeValue::String("123".to_string())])]))]
    #[case::explode(vec![RuntimeValue::String("ABC".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("explode"), Vec::new(), false))
       ],
       Ok(vec![RuntimeValue::Array(vec![
            RuntimeValue::Number(65.into()),
            RuntimeValue::Number(66.into()),
            RuntimeValue::Number(67.into()),
       ])]))]
    #[case::implode(vec![RuntimeValue::Array(vec![
            RuntimeValue::Number(65.into()),
            RuntimeValue::Number(66.into()),
            RuntimeValue::Number(67.into()),
       ])],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("implode"), Vec::new(), false))
       ],
       Ok(vec![RuntimeValue::String("ABC".to_string())]))]
    #[case::range(vec![RuntimeValue::Number(1.into())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("range"), vec![
                ast_node(ast::Expr::Literal(ast::Literal::Number(0.into()))),
                ast_node(ast::Expr::Literal(ast::Literal::Number(5.into()))),
            ], false))
       ],
       Ok(vec![RuntimeValue::Array(vec![
            RuntimeValue::Number(0.into()),
            RuntimeValue::Number(1.into()),
            RuntimeValue::Number(2.into()),
            RuntimeValue::Number(3.into()),
            RuntimeValue::Number(4.into()),
       ])]))]
    #[case::to_number(vec![RuntimeValue::String("42".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("to_number"), Vec::new(), false))
       ],
       Ok(vec![RuntimeValue::Number(42.into())]))]
    #[case::to_number(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "42".to_string(), position: None}))],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("to_number"), Vec::new(), false))
       ],
       Ok(vec![RuntimeValue::Number(42.into())]))]
    #[case::to_number(vec![RuntimeValue::String("42.5".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("to_number"), Vec::new(), false))
       ],
       Ok(vec![RuntimeValue::Number(42.5.into())]))]
    #[case::to_number(vec![RuntimeValue::String("not a number".to_string())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("to_number"), Vec::new(), false))
       ],
       Err(InnerError::Eval(EvalError::RuntimeError(Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()}, "invalid float literal".to_string()))))]
    #[case::trunc(vec![RuntimeValue::Number(42.5.into())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("trunc"), Vec::new(), false))
       ],
       Ok(vec![RuntimeValue::Number(42.into())]))]
    #[case::trunc(vec![RuntimeValue::Number((-42.5).into())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("trunc"), Vec::new(), false))
       ],
       Ok(vec![RuntimeValue::Number((-42).into())]))]
    #[case::ceil(vec![RuntimeValue::Number(42.1.into())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("ceil"), Vec::new(), false))
       ],
       Ok(vec![RuntimeValue::Number(43.into())]))]
    #[case::ceil(vec![RuntimeValue::Number((-42.1).into())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("ceil"), Vec::new(), false))
       ],
       Ok(vec![RuntimeValue::Number((-42).into())]))]
    #[case::round(vec![RuntimeValue::Number(42.5.into())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("round"), Vec::new(), false))
       ],
       Ok(vec![RuntimeValue::Number(43.into())]))]
    #[case::round(vec![RuntimeValue::Number(42.4.into())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("round"), Vec::new(), false))
       ],
       Ok(vec![RuntimeValue::Number(42.into())]))]
    #[case::floor(vec![RuntimeValue::Number(42.9.into())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("floor"), Vec::new(), false))
       ],
       Ok(vec![RuntimeValue::Number(42.into())]))]
    #[case::floor(vec![RuntimeValue::Number((-42.9).into())],
       vec![
            ast_node(ast::Expr::Call(ast::Ident::new("floor"), Vec::new(), false))
       ],
       Ok(vec![RuntimeValue::Number((-43).into())]))]
    #[case::del(vec![RuntimeValue::Array(vec![RuntimeValue::String("test1".to_string()), RuntimeValue::String("test2".to_string())])],
        vec![
              ast_node(ast::Expr::Call(ast::Ident::new("del"), vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(0.into()))),
              ], false)),
        ],
        Ok(vec![RuntimeValue::Array(vec![RuntimeValue::String("test2".to_string())])]))]
    #[case::del(vec![RuntimeValue::Array(vec![RuntimeValue::String("test1".to_string()), RuntimeValue::String("test2".to_string())])],
        vec![
              ast_node(ast::Expr::Call(ast::Ident::new("del"), vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
              ], false)),
        ],
        Ok(vec![RuntimeValue::Array(vec![RuntimeValue::String("test1".to_string())])]))]
    #[case::to_code(vec![RuntimeValue::String("test1".to_string())],
        vec![
              ast_node(ast::Expr::Call(ast::Ident::new("to_code"), vec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("elm".into()))),
              ], false)),
        ],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Code(mq_markdown::Code{lang: Some("elm".to_string()), value: "test1".to_string(), position: None}))]))]
    #[case::md_h1(vec![RuntimeValue::String("Heading 1".to_string())],
            vec![
                  ast_node(ast::Expr::Call(ast::Ident::new("to_h"), vec![
                        ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                  ], false)),
            ],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Heading(mq_markdown::Heading{depth: 1, value: Box::new("Heading 1".to_string().into()), position: None}))]))]
    #[case::md_h2(vec![RuntimeValue::String("Heading 2".to_string())],
            vec![
                  ast_node(ast::Expr::Call(ast::Ident::new("to_h"), vec![
                        ast_node(ast::Expr::Literal(ast::Literal::Number(2.into()))),
                  ], false)),
            ],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Heading(mq_markdown::Heading{depth: 2, value: Box::new("Heading 2".to_string().into()), position: None}))]))]
    #[case::md_h3(vec![RuntimeValue::String("Heading 3".to_string())],
            vec![
                  ast_node(ast::Expr::Call(ast::Ident::new("to_h"), vec![
                        ast_node(ast::Expr::Literal(ast::Literal::Number(3.into()))),
                  ], false)),
            ],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Heading(mq_markdown::Heading{depth: 3, value: Box::new("Heading 3".to_string().into()), position: None}))]))]
    #[case::md_h(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "Heading".to_string(), position: None}))],
            vec![
                  ast_node(ast::Expr::Call(ast::Ident::new("to_h"), vec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(2.into()))),
                  ], false)),
            ],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Heading(mq_markdown::Heading{depth: 2, value: Box::new("Heading".to_string().into()), position: None}))]))]
    #[case::to_math(vec![RuntimeValue::String("E=mc^2".to_string())],
            vec![
                  ast_node(ast::Expr::Call(ast::Ident::new("to_math"), Vec::new(), false)),
            ],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Math(mq_markdown::Math{value: "E=mc^2".to_string(), position: None}))]))]
    #[case::to_math_inline(vec![RuntimeValue::String("E=mc^2".to_string())],
            vec![
                  ast_node(ast::Expr::Call(ast::Ident::new("to_math_inline"), Vec::new(), false)),
            ],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::MathInline(mq_markdown::MathInline{value: "E=mc^2".to_string(), position: None}))]))]
    #[case::to_md_text(vec![RuntimeValue::String("This is a text".to_string())],
            vec![
                  ast_node(ast::Expr::Call(ast::Ident::new("to_md_text"), Vec::new(), false)),
            ],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "This is a text".to_string(), position: None}))]))]
    #[case::to_strong(vec![RuntimeValue::String("Bold text".to_string())],
            vec![
                  ast_node(ast::Expr::Call(ast::Ident::new("to_strong"), Vec::new(), false)),
            ],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Strong(mq_markdown::Value{value: Box::new("Bold text".to_string().into()), position: None}))]))]
    #[case::to_em(vec![RuntimeValue::String("Italic text".to_string())],
            vec![
                  ast_node(ast::Expr::Call(ast::Ident::new("to_em"), Vec::new(), false)),
            ],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Emphasis(mq_markdown::Value{value: Box::new("Italic text".to_string().into()), position: None}))]))]
    #[case::to_image(vec![RuntimeValue::String("Image Alt".to_string())],
            vec![
                  ast_node(ast::Expr::Call(ast::Ident::new("to_image"), vec![
                        ast_node(ast::Expr::Literal(ast::Literal::String("https://example.com/image.png".to_string()))),
                        ast_node(ast::Expr::Literal(ast::Literal::String("Image Alt".to_string()))),
                        ast_node(ast::Expr::Literal(ast::Literal::String("Image Title".to_string()))),
                  ], false)),
            ],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Image(mq_markdown::Image{
                url: "https://example.com/image.png".to_string(),
                alt: "Image Alt".to_string(),
                title: Some("Image Title".to_string()),
                position: None
            }))]))]
    #[case::to_link(vec![RuntimeValue::String("Link Text".to_string())],
            vec![
                  ast_node(ast::Expr::Call(ast::Ident::new("to_link"), vec![
                        ast_node(ast::Expr::Literal(ast::Literal::String("https://example.com".to_string()))),
                        ast_node(ast::Expr::Literal(ast::Literal::String("Link Title".to_string()))),
                  ], false)),
            ],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Link(mq_markdown::Link{
                url: "https://example.com".to_string(),
                title: Some("Link Title".to_string()),
                position: None
            }))]))]
    #[case::to_hr(vec![RuntimeValue::NONE],
            vec![
                  ast_node(ast::Expr::Call(ast::Ident::new("to_hr"), Vec::new(), false)),
            ],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::HorizontalRule{position: None})]))]
    #[case::to_md_list(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "list".to_string(), position: None}))],
            vec![
                  ast_node(ast::Expr::Call(ast::Ident::new("to_md_list"),
                           vec![
                                 ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                           ].into(), false)),
            ],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::List(
                mq_markdown::List{value: Box::new("list".to_string().into()), index: 0, level: 1 as u8, checked: None, position: None}))]))]
    #[case::to_md_list(vec![RuntimeValue::String("list".to_string())],
            vec![
                  ast_node(ast::Expr::Call(ast::Ident::new("to_md_list"),
                           vec![
                                 ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                           ].into(), false)),
            ],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::List(
                mq_markdown::List{value: Box::new("list".to_string().into()), index: 0, level: 1 as u8, checked: None, position: None}))]))]
    #[case::set_md_check(vec![RuntimeValue::Markdown(mq_markdown::Node::List(mq_markdown::List{value: Box::new("Checked Item".to_string().into()), level: 0, index: 0, checked: None, position: None}))],
            vec![
                  ast_node(ast::Expr::Call(ast::Ident::new("set_md_check"), vec![
                        ast_node(ast::Expr::Literal(ast::Literal::Bool(true))),
                  ], false)),
            ],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::List(mq_markdown::List{value: Box::new("Checked Item".to_string().into()), level: 0, index: 0, checked: Some(true), position: None}))]))]
    #[case::set_md_check(vec![RuntimeValue::Markdown(mq_markdown::Node::List(mq_markdown::List{value: Box::new("Unchecked Item".to_string().into()), level: 0, index: 0, checked: None, position: None}))],
            vec![
                  ast_node(ast::Expr::Call(ast::Ident::new("set_md_check"), vec![
                        ast_node(ast::Expr::Literal(ast::Literal::Bool(false))),
                  ], false)),
            ],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::List(mq_markdown::List{value: Box::new("Unchecked Item".to_string().into()), level: 0, index: 0, checked: Some(false), position: None}))]))]
    #[case::compact(vec![RuntimeValue::Array(vec![
                RuntimeValue::String("test1".to_string()),
                RuntimeValue::NONE,
                RuntimeValue::String("test2".to_string()),
                RuntimeValue::NONE,
                RuntimeValue::String("test3".to_string()),
            ])],
            vec![
                ast_node(ast::Expr::Call(ast::Ident::new("compact"), Vec::new(), false))
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
                ast_node(ast::Expr::Call(ast::Ident::new("compact"), Vec::new(), false))
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
                ast_node(ast::Expr::Call(ast::Ident::new("compact"), Vec::new(), false))
            ],
            Ok(vec![RuntimeValue::Array(vec![])]))]
    #[case::compact_no_none(vec![RuntimeValue::Array(vec![
                RuntimeValue::String("test1".to_string()),
                RuntimeValue::String("test2".to_string()),
            ])],
            vec![
                ast_node(ast::Expr::Call(ast::Ident::new("compact"), Vec::new(), false))
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
                ast_node(ast::Expr::Call(ast::Ident::new("to_csv"), Vec::new(), false))
            ],
            Ok(vec![RuntimeValue::String("test1,test2,test3".to_string())]))]
    #[case::to_csv_empty(vec![RuntimeValue::Array(vec![])],
            vec![
                ast_node(ast::Expr::Call(ast::Ident::new("to_csv"), Vec::new(), false))
            ],
            Ok(vec![RuntimeValue::String("".to_string())]))]
    #[case::to_csv_mixed(vec![RuntimeValue::Array(vec![
                RuntimeValue::String("test1".to_string()),
                RuntimeValue::Number(42.into()),
                RuntimeValue::Bool(true),
            ])],
            vec![
                ast_node(ast::Expr::Call(ast::Ident::new("to_csv"), Vec::new(), false))
            ],
            Ok(vec![RuntimeValue::String("test1,42,true".to_string())]))]
    #[case::to_tsv(vec![RuntimeValue::Array(vec![
                RuntimeValue::String("test1".to_string()),
                RuntimeValue::String("test2".to_string()),
                RuntimeValue::String("test3".to_string()),
            ])],
            vec![
                ast_node(ast::Expr::Call(ast::Ident::new("to_tsv"), Vec::new(), false))
            ],
            Ok(vec![RuntimeValue::String("test1\ttest2\ttest3".to_string())]))]
    #[case::to_tsv_empty(vec![RuntimeValue::Array(vec![])],
            vec![
                ast_node(ast::Expr::Call(ast::Ident::new("to_tsv"), Vec::new(), false))
            ],
            Ok(vec![RuntimeValue::String("".to_string())]))]
    #[case::to_tsv_mixed(vec![RuntimeValue::Array(vec![
                RuntimeValue::String("test1".to_string()),
                RuntimeValue::Number(42.into()),
                RuntimeValue::Bool(true),
            ])],
            vec![
                ast_node(ast::Expr::Call(ast::Ident::new("to_tsv"), Vec::new(), false))
            ],
            Ok(vec![RuntimeValue::String("test1\t42\ttrue".to_string())]))]
    #[case::get_md_list_level(vec![RuntimeValue::Markdown(mq_markdown::Node::List(mq_markdown::List{value: Box::new("List Item".to_string().into()), level: 1, index: 0, checked: None, position: None}))],
            vec![
                  ast_node(ast::Expr::Call(ast::Ident::new("get_md_list_level"), vec![], false)),
            ],
            Ok(vec![RuntimeValue::Number(1.into())]))]
    #[case::text_selector(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "test".to_string(), position: None}))],
           vec![
                ast_node(ast::Expr::Selector(ast::Selector::Text)),
           ],
           Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "test".to_string(), position: None}))]))]
    #[case::text_selector_heading(vec![RuntimeValue::Markdown(mq_markdown::Node::Heading(mq_markdown::Heading{depth: 1, value: Box::new("Heading 1".to_string().into()), position: None}))],
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
                ast_node(ast::Expr::Call(ast::Ident::new("to_md_table_row"), Vec::new(), false))
            ],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::TableRow(mq_markdown::TableRow{
                cells: vec![
                    mq_markdown::Node::TableCell(mq_markdown::TableCell{
                        row: 0,
                        column: 0,
                        value: Box::new("Cell 1".to_string().into()),
                        last_cell_in_row: false,
                        last_cell_of_in_table: false,
                        position: None
                    }),
                    mq_markdown::Node::TableCell(mq_markdown::TableCell{
                        row: 0,
                        column: 1,
                        value: Box::new("Cell 2".to_string().into()),
                        last_cell_in_row: false,
                        last_cell_of_in_table: false,
                        position: None
                    }),
                    mq_markdown::Node::TableCell(mq_markdown::TableCell{
                        row: 0,
                        column: 2,
                        value: Box::new("Cell 3".to_string().into()),
                        last_cell_in_row: true,
                        last_cell_of_in_table: false,
                        position: None
                    }),
                ],
                position: None
            }))]))]
    #[case::to_md_table_row(vec![RuntimeValue::String("Cell 4".to_string())],
            vec![
                ast_node(ast::Expr::Call(ast::Ident::new("to_md_table_row"), vec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("Cell 1".to_string()))),
                    ast_node(ast::Expr::Literal(ast::Literal::String("Cell 2".to_string()))),
                ], false))
            ],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::TableRow(mq_markdown::TableRow{
                cells: vec![
                    mq_markdown::Node::TableCell(mq_markdown::TableCell{
                        row: 0,
                        column: 0,
                        value: Box::new("Cell 1".to_string().into()),
                        last_cell_in_row: false,
                        last_cell_of_in_table: false,
                        position: None
                    }),
                    mq_markdown::Node::TableCell(mq_markdown::TableCell{
                        row: 0,
                        column: 1,
                        value: Box::new("Cell 2".to_string().into()),
                        last_cell_in_row: true,
                        last_cell_of_in_table: false,
                        position: None
                    }),
                ],
                position: None
            }))]))]
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

    #[test]
    fn test_include() {
        let tmp_dir = std::env::temp_dir();
        let tmp_file_path = tmp_dir.join("test_module.mq");

        let mut file = File::create(&tmp_file_path).expect("Failed to create temp file");
        write!(file, r#"def test(): 42;"#).expect("Failed to write to temp file");
        let loader = ModuleLoader::new(Some(vec![tmp_dir.clone()]));
        let program = vec![
            Rc::new(ast::Node {
                token_id: 0.into(),
                expr: Rc::new(ast::Expr::Include(ast::Literal::String(
                    "test_module".to_string(),
                ))),
            }),
            Rc::new(ast::Node {
                token_id: 0.into(),
                expr: Rc::new(ast::Expr::Call(ast::Ident::new("test"), Vec::new(), false)),
            }),
        ];
        assert_eq!(
            Evaluator::new(loader, token_arena()).eval(
                &program,
                vec![RuntimeValue::String("".to_string())].into_iter()
            ),
            Ok(vec![RuntimeValue::Number(42.into())])
        );

        std::fs::remove_file(tmp_file_path).expect("Failed to remove temp dir");
    }
}
