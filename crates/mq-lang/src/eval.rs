use std::{cell::RefCell, rc::Rc};

#[cfg(feature = "debugger")]
use crate::ast::constants;
use crate::{
    Program, Token, TokenKind,
    arena::Arena,
    ast::node::{self as ast},
    error::InnerError,
};

#[cfg(feature = "debugger")]
use debugger::{Breakpoint, DebugContext, Debugger};

pub mod builtin;
#[cfg(feature = "debugger")]
pub mod debugger;
pub mod env;
pub mod error;
pub mod module;
pub mod runtime_value;

use env::Env;
use error::EvalError;
use runtime_value::RuntimeValue;

#[derive(Debug, Clone)]
pub struct Options {
    pub max_call_stack_depth: u32,
}

#[cfg(debug_assertions)]
impl Default for Options {
    fn default() -> Self {
        Self {
            max_call_stack_depth: 40,
        }
    }
}

#[cfg(not(debug_assertions))]
impl Default for Options {
    fn default() -> Self {
        Self {
            max_call_stack_depth: 192,
        }
    }
}

#[derive(Debug)]
pub struct Evaluator {
    env: Rc<RefCell<Env>>,
    token_arena: Rc<RefCell<Arena<Rc<Token>>>>,
    call_stack_depth: u32,
    pub(crate) options: Options,
    pub(crate) module_loader: module::ModuleLoader,
    #[cfg(feature = "debugger")]
    debugger: Rc<RefCell<Debugger>>,
}

impl Default for Evaluator {
    fn default() -> Self {
        Self {
            env: Rc::new(RefCell::new(Env::default())),
            token_arena: Rc::new(RefCell::new(Arena::new(10))),
            call_stack_depth: 0,
            options: Options::default(),
            module_loader: module::ModuleLoader::default(),
            #[cfg(feature = "debugger")]
            debugger: Rc::new(RefCell::new(Debugger::new())),
        }
    }
}

impl Clone for Evaluator {
    fn clone(&self) -> Self {
        Self {
            env: Rc::clone(&self.env),
            token_arena: Rc::clone(&self.token_arena),
            call_stack_depth: self.call_stack_depth,
            options: self.options.clone(),
            module_loader: self.module_loader.clone(),
            #[cfg(feature = "debugger")]
            debugger: Rc::clone(&self.debugger),
        }
    }
}

impl Evaluator {
    pub(crate) fn new(
        module_loader: module::ModuleLoader,
        token_arena: Rc<RefCell<Arena<Rc<Token>>>>,
    ) -> Self {
        Self {
            module_loader,
            token_arena,
            ..Default::default()
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
        let mut program = program.iter().try_fold(
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

        let nodes_index = &program.iter().position(|node| node.is_nodes());

        if let Some(index) = nodes_index {
            let (program, nodes_program) = program.split_at_mut(*index);
            let program = program.to_vec();
            let nodes_program = nodes_program.to_vec();

            let values: Result<Vec<RuntimeValue>, InnerError> = input
                .map(|runtime_value| match &runtime_value {
                    RuntimeValue::Markdown(node, _) => self.eval_markdown_node(&program, node),
                    _ => self
                        .eval_program(&program, runtime_value, Rc::clone(&self.env))
                        .map_err(InnerError::from),
                })
                .collect();

            if nodes_program.is_empty() {
                values
            } else {
                self.eval_program(&nodes_program, values?.into(), Rc::clone(&self.env))
                    .map(|value| {
                        if let RuntimeValue::Array(values) = value {
                            values
                        } else {
                            vec![value]
                        }
                    })
                    .map_err(InnerError::from)
            }
        } else {
            input
                .map(|runtime_value| match &runtime_value {
                    RuntimeValue::Markdown(node, _) => self.eval_markdown_node(&program, node),
                    _ => self
                        .eval_program(&program, runtime_value, Rc::clone(&self.env))
                        .map_err(InnerError::from),
                })
                .collect()
        }
    }

    #[inline(always)]
    fn eval_markdown_node(
        &mut self,
        program: &Program,
        node: &mq_markdown::Node,
    ) -> Result<RuntimeValue, InnerError> {
        node.map_values(&mut |child_node| {
            let value = self.eval_program(
                program,
                RuntimeValue::Markdown(child_node.clone(), None),
                Rc::clone(&self.env),
            )?;

            Ok(match value {
                RuntimeValue::None => child_node.to_fragment(),
                RuntimeValue::Function(_, _, _) | RuntimeValue::NativeFunction(_) => {
                    mq_markdown::Node::Empty
                }
                RuntimeValue::Array(_)
                | RuntimeValue::Dict(_)
                | RuntimeValue::Bool(_)
                | RuntimeValue::Number(_)
                | RuntimeValue::String(_) => value.to_string().into(),
                RuntimeValue::Markdown(node, _) => node,
            })
        })
        .map(|node| RuntimeValue::Markdown(node, None))
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
            .load_builtin(Rc::clone(&self.token_arena))?;
        self.load_module(module)
    }

    pub(crate) fn load_module(&mut self, module: Option<module::Module>) -> Result<(), EvalError> {
        if let Some(module) = module {
            for node in &module.modules {
                if let ast::Expr::Include(_) = &*node.expr {
                    self.eval_expr(&RuntimeValue::NONE, Rc::clone(node), Rc::clone(&self.env))?;
                } else {
                    return Err(EvalError::InternalError(
                        (*self.token_arena.borrow()[node.token_id]).clone(),
                    ));
                }
            }

            let mut mut_env = self.env.borrow_mut();

            for node in &module.functions {
                if let ast::Expr::Def(ident, params, program) = &*node.expr {
                    mut_env.define(
                        ident,
                        RuntimeValue::Function(
                            params.clone(),
                            program.clone(),
                            Rc::clone(&self.env),
                        ),
                    );
                }
            }

            std::mem::drop(mut_env);

            for node in &module.vars {
                if let ast::Expr::Let(ident, node) = &*node.expr {
                    let val =
                        self.eval_expr(&RuntimeValue::NONE, Rc::clone(node), Rc::clone(&self.env))?;
                    self.env.borrow_mut().define(ident, val);
                } else {
                    return Err(EvalError::InternalError(
                        (*self.token_arena.borrow()[node.token_id]).clone(),
                    ));
                }
            }
        }

        Ok(())
    }

    fn eval_program(
        &mut self,
        program: &Program,
        runtime_value: RuntimeValue,
        env: Rc<RefCell<Env>>,
    ) -> Result<RuntimeValue, EvalError> {
        let mut value = runtime_value;
        for expr in program {
            match self.eval_expr(&value, Rc::clone(expr), Rc::clone(&env)) {
                Ok(v) => value = v,
                Err(e) => return Err(e),
            }
        }
        Ok(value)
    }

    #[inline(always)]
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

    #[cfg(feature = "debugger")]
    fn eval_debugger(
        &self,
        runtime_value: &RuntimeValue,
        node: Rc<ast::Node>,
        env: Rc<RefCell<Env>>,
    ) {
        self.debugger.borrow_mut().breakpoint_hit(
            &DebugContext {
                current_value: runtime_value.clone(),
                current_node: Rc::clone(&node),
                call_stack: self.debugger.borrow().current_call_stack(),
                env: Rc::clone(&env),
            },
            &Breakpoint {
                id: 0,
                line: (*self.token_arena.borrow()[node.token_id]).range.start.line as usize,
                column: Some(
                    (*self.token_arena.borrow()[node.token_id])
                        .range
                        .start
                        .column,
                ),
                enabled: true,
            },
        );
    }

    #[inline(always)]
    fn eval_include(&mut self, module: ast::Literal) -> Result<(), EvalError> {
        match module {
            ast::Literal::String(module_name) => {
                let module = self
                    .module_loader
                    .load_from_file(&module_name, Rc::clone(&self.token_arena))?;
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
            RuntimeValue::Markdown(node_value, _) => {
                if builtin::eval_selector(node_value, ident) {
                    runtime_value.clone()
                } else {
                    RuntimeValue::NONE
                }
            }
            RuntimeValue::Array(values) => {
                let values = values
                    .iter()
                    .map(|value| match value {
                        RuntimeValue::Markdown(node_value, _) => {
                            if builtin::eval_selector(node_value, ident) {
                                value.clone()
                            } else {
                                RuntimeValue::NONE
                            }
                        }
                        _ => RuntimeValue::NONE,
                    })
                    .collect::<Vec<_>>();

                RuntimeValue::Array(values)
            }
            _ => RuntimeValue::NONE,
        }
    }

    #[inline(always)]
    fn eval_interpolated_string(
        &self,
        runtime_value: &RuntimeValue,
        node: Rc<ast::Node>,
        env: Rc<RefCell<Env>>,
    ) -> Result<RuntimeValue, EvalError> {
        if let ast::Expr::InterpolatedString(segments) = &*node.expr {
            // Calculate estimated capacity based on segment content
            let estimated_capacity = segments
                .iter()
                .map(|segment| match segment {
                    ast::StringSegment::Text(s) => s.len(),
                    ast::StringSegment::Ident(_) => 32, // Estimated size for variable content
                    ast::StringSegment::Env(_) => 32,   // Estimated size for environment variable
                    ast::StringSegment::Self_ => 64,    // Estimated size for self reference
                })
                .sum();

            segments
                .iter()
                .try_fold(
                    String::with_capacity(estimated_capacity),
                    |mut acc, segment| {
                        match segment {
                            ast::StringSegment::Text(s) => acc.push_str(s),
                            ast::StringSegment::Ident(ident) => {
                                let value =
                                    self.eval_ident(ident, Rc::clone(&node), Rc::clone(&env))?;
                                acc.push_str(&value.to_string());
                            }
                            ast::StringSegment::Env(env) => {
                                acc.push_str(&std::env::var(env).map_err(|_| {
                                    EvalError::EnvNotFound(
                                        (*self.token_arena.borrow()[node.token_id]).clone(),
                                        env.clone(),
                                    )
                                })?);
                            }
                            ast::StringSegment::Self_ => {
                                acc.push_str(&runtime_value.to_string());
                            }
                        }

                        Ok(acc)
                    },
                )
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
        #[cfg(feature = "debugger")]
        {
            let debug_context = DebugContext {
                current_value: runtime_value.clone(),
                current_node: Rc::clone(&node),
                call_stack: self.debugger.borrow().current_call_stack(),
                env: Rc::clone(&env),
            };

            let _ = self
                .debugger
                .borrow_mut()
                .should_break(&debug_context, &self.token_arena.borrow());
        }

        match &*node.expr {
            ast::Expr::Selector(ident) => Ok(Self::eval_selector_expr(runtime_value, ident)),
            ast::Expr::Call(ident, args) => {
                #[cfg(feature = "debugger")]
                if ident.name == constants::DBG {
                    self.eval_debugger(runtime_value, Rc::clone(&node), Rc::clone(&env));
                    return Ok(runtime_value.clone());
                }

                self.eval_fn(runtime_value, Rc::clone(&node), ident, args, env)
            }
            ast::Expr::Self_ | ast::Expr::Nodes => Ok(runtime_value.clone()),
            ast::Expr::Break => Err(self.eval_break(node)),
            ast::Expr::Continue => Err(self.eval_continue(node)),
            ast::Expr::If(_) => self.eval_if(runtime_value, node, env),
            ast::Expr::Ident(ident) => self.eval_ident(ident, Rc::clone(&node), Rc::clone(&env)),
            ast::Expr::Literal(literal) => Ok(self.eval_literal(literal)),
            ast::Expr::Def(ident, params, program) => {
                let function =
                    RuntimeValue::Function(params.clone(), program.clone(), Rc::clone(&env));
                env.borrow_mut().define(ident, function.clone());
                Ok(function)
            }
            ast::Expr::Fn(params, program) => Ok(RuntimeValue::Function(
                params.clone(),
                program.clone(),
                Rc::clone(&env),
            )),
            ast::Expr::Let(ident, node) => {
                let val = self.eval_expr(runtime_value, Rc::clone(node), Rc::clone(&env))?;
                env.borrow_mut().define(ident, val);
                Ok(runtime_value.clone())
            }
            ast::Expr::While(_, _) => self.eval_while(runtime_value, node, env),
            ast::Expr::Until(_, _) => self.eval_until(runtime_value, node, env),
            ast::Expr::Foreach(_, _, _) => self.eval_foreach(runtime_value, node, env),
            ast::Expr::InterpolatedString(_) => {
                self.eval_interpolated_string(runtime_value, node, env)
            }
            ast::Expr::Include(module_id) => {
                self.eval_include(module_id.to_owned())?;
                Ok(runtime_value.clone())
            }
            ast::Expr::Paren(expr) => self.eval_expr(runtime_value, Rc::clone(expr), env),
        }
    }

    #[inline(always)]
    fn eval_break(&self, node: Rc<ast::Node>) -> EvalError {
        EvalError::Break((*self.token_arena.borrow()[node.token_id]).clone())
    }

    #[inline(always)]
    fn eval_continue(&self, node: Rc<ast::Node>) -> EvalError {
        EvalError::Continue((*self.token_arena.borrow()[node.token_id]).clone())
    }

    #[inline(always)]
    fn eval_literal(&self, literal: &ast::Literal) -> RuntimeValue {
        match literal {
            ast::Literal::None => RuntimeValue::None,
            ast::Literal::Bool(b) => RuntimeValue::Bool(*b),
            ast::Literal::String(s) => RuntimeValue::String(s.clone()),
            ast::Literal::Number(n) => RuntimeValue::Number(*n),
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
            let values = match values {
                RuntimeValue::Array(values) => {
                    let env = Rc::new(RefCell::new(Env::with_parent(Rc::downgrade(&env))));
                    let mut results = Vec::with_capacity(values.len());

                    for value in values {
                        env.borrow_mut().define(ident, value.clone());
                        match self.eval_program(body, value, Rc::clone(&env)) {
                            Ok(result) => results.push(result),
                            Err(EvalError::Break(_)) => break,
                            Err(EvalError::Continue(_)) => continue,
                            Err(e) => return Err(e),
                        }
                    }

                    results
                }
                RuntimeValue::String(s) => {
                    let env = Rc::new(RefCell::new(Env::with_parent(Rc::downgrade(&env))));
                    let mut results = Vec::with_capacity(s.len());

                    for c in s.chars() {
                        env.borrow_mut()
                            .define(ident, RuntimeValue::String(c.to_string()));
                        match self.eval_program(
                            body,
                            RuntimeValue::String(c.to_string()),
                            Rc::clone(&env),
                        ) {
                            Ok(result) => results.push(result),
                            Err(EvalError::Break(_)) => break,
                            Err(EvalError::Continue(_)) => continue,
                            Err(e) => return Err(e),
                        }
                    }

                    results
                }
                _ => {
                    return Err(EvalError::InvalidTypes {
                        token: (*self.token_arena.borrow()[node.token_id]).clone(),
                        name: TokenKind::Foreach.to_string(),
                        args: vec![values.to_string().into()],
                    });
                }
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

            if !cond_value.is_truthy() {
                return Ok(RuntimeValue::NONE);
            }
            let mut first = true;

            while cond_value.is_truthy() {
                match self.eval_program(body, runtime_value.clone(), Rc::clone(&env)) {
                    Ok(mut new_runtime_value) => {
                        std::mem::swap(&mut runtime_value, &mut new_runtime_value);
                        cond_value =
                            self.eval_expr(&runtime_value, Rc::clone(cond), Rc::clone(&env))?;
                    }
                    Err(EvalError::Break(_)) if first => {
                        runtime_value = RuntimeValue::NONE;
                        break;
                    }
                    Err(EvalError::Break(_)) => break,
                    Err(EvalError::Continue(_)) if first => {
                        runtime_value = RuntimeValue::NONE;
                        continue;
                    }
                    Err(EvalError::Continue(_)) => continue,
                    Err(e) => return Err(e),
                }

                first = false;
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
            let mut values = Vec::new();

            if !cond_value.is_truthy() {
                return Ok(RuntimeValue::NONE);
            }

            while cond_value.is_truthy() {
                match self.eval_program(body, std::mem::take(&mut runtime_value), Rc::clone(&env)) {
                    Ok(new_runtime_value) => {
                        runtime_value = new_runtime_value;
                        cond_value =
                            self.eval_expr(&runtime_value, Rc::clone(cond), Rc::clone(&env))?;
                        values.push(runtime_value.clone());
                    }
                    Err(EvalError::Break(_)) => break,
                    Err(EvalError::Continue(_)) => continue,
                    Err(e) => return Err(e),
                }
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

                        if cond.is_truthy() {
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
        args: &ast::Args,
        env: Rc<RefCell<Env>>,
    ) -> Result<RuntimeValue, EvalError> {
        if let Ok(fn_value) = Rc::clone(&env).borrow().resolve(ident) {
            if let RuntimeValue::Function(params, program, fn_env) = &fn_value {
                self.enter_scope()?;
                #[cfg(feature = "debugger")]
                self.debugger.borrow_mut().push_call_stack(Rc::clone(&node));

                let new_env = Rc::new(RefCell::new(Env::with_parent(Rc::downgrade(fn_env))));
                let mut new_env_mut = new_env.borrow_mut();

                if params.len() == args.len() + 1 {
                    if let ast::Expr::Ident(name) = &*params.first().unwrap().expr {
                        new_env_mut.define(name, runtime_value.clone());
                    } else {
                        return Err(EvalError::InvalidDefinition(
                            (*self.token_arena.borrow()[params.first().unwrap().token_id]).clone(),
                            ident.to_string(),
                        ));
                    }

                    for (arg, param) in args.into_iter().zip(params.iter().skip(1)) {
                        if let ast::Expr::Ident(name) = &*param.expr {
                            new_env_mut.define(
                                name,
                                self.eval_expr(runtime_value, Rc::clone(arg), Rc::clone(&env))?,
                            );
                        } else {
                            return Err(EvalError::InvalidDefinition(
                                (*self.token_arena.borrow()[param.token_id]).clone(),
                                ident.to_string(),
                            ));
                        }
                    }
                } else if args.len() != params.len() {
                    return Err(EvalError::InvalidNumberOfArguments(
                        (*self.token_arena.borrow()[node.token_id]).clone(),
                        ident.to_string(),
                        params.len() as u8,
                        args.len() as u8,
                    ));
                } else {
                    for (arg, param) in args.into_iter().zip(params.iter()) {
                        if let ast::Expr::Ident(name) = &*param.expr {
                            new_env_mut.define(
                                name,
                                self.eval_expr(runtime_value, Rc::clone(arg), Rc::clone(&env))?,
                            );
                        } else {
                            return Err(EvalError::InvalidDefinition(
                                (*self.token_arena.borrow()[param.token_id]).clone(),
                                ident.to_string(),
                            ));
                        }
                    }
                };
                std::mem::drop(new_env_mut);

                let result = self.eval_program(program, runtime_value.clone(), new_env);
                self.exit_scope();
                #[cfg(feature = "debugger")]
                self.debugger.borrow_mut().pop_call_stack();

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
        args: &ast::Args,
        env: Rc<RefCell<Env>>,
    ) -> Result<RuntimeValue, EvalError> {
        let args: Result<builtin::Args, EvalError> = args
            .iter()
            .map(|arg| self.eval_expr(runtime_value, Rc::clone(arg), Rc::clone(&env)))
            .collect();
        builtin::eval_builtin(runtime_value, ident, args?)
            .map_err(|e| e.to_eval_error((*node).clone(), Rc::clone(&self.token_arena)))
    }

    #[inline(always)]
    fn enter_scope(&mut self) -> Result<(), EvalError> {
        if self.call_stack_depth >= self.options.max_call_stack_depth {
            return Err(EvalError::RecursionError(self.options.max_call_stack_depth));
        }
        self.call_stack_depth += 1;
        Ok(())
    }

    #[inline(always)]
    fn exit_scope(&mut self) {
        if self.call_stack_depth > 0 {
            self.call_stack_depth -= 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::ast::node::Args;
    use crate::range::Range;
    use crate::{AstExpr, AstNode, ModuleLoader};
    use crate::{Token, TokenKind};

    use super::*;
    use mq_test::defer;
    use rstest::{fixture, rstest};
    use smallvec::{SmallVec, smallvec};

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

    fn ast_call(name: &str, args: Args) -> Rc<AstNode> {
        Rc::new(AstNode {
            token_id: 0.into(),
            expr: Rc::new(ast::Expr::Call(ast::Ident::new(name), args)),
        })
    }

    #[rstest]
    #[case::starts_with(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("starts_with", smallvec![ast_node(ast::Expr::Literal(ast::Literal::String("te".to_string())))])
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::starts_with(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "test".to_string(), position: None}), None)],
       vec![
            ast_call("starts_with", smallvec![ast_node(ast::Expr::Literal(ast::Literal::String("te".to_string())))])
       ],
       Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "true".to_string(), position: None}), None)]))]
    #[case::starts_with(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("starts_with", smallvec![ast_node(ast::Expr::Literal(ast::Literal::String("st".to_string())))])
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::starts_with(vec![RuntimeValue::Array(vec!["start".to_string().into(), "end".to_string().into()])],
       vec![
            ast_call("starts_with", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::String("start".to_string())))
            ])
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::starts_with(vec![RuntimeValue::Array(vec!["start".to_string().into(), "end".to_string().into()])],
       vec![
            ast_call("starts_with", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::String("end".to_string())))
            ])
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::starts_with(vec![RuntimeValue::Number(1.into())],
       vec![
            ast_call("starts_with", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::String("end".to_string())))
            ])
       ],
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                    name: "starts_with".to_string(),
                                                    args: vec!["1".into(), "end".to_string().into()]})))]
    #[case::ends_with(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("ends_with", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::String("st".to_string())))
            ])
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::ends_with(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "test".to_string(), position: None}), None)],
       vec![
            ast_call("ends_with", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::String("st".to_string())))
            ])
       ],
       Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "true".to_string(), position: None}), None)]))]
    #[case::ends_with(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("ends_with", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::String("te".to_string())))
            ])
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::ends_with(vec![RuntimeValue::Array(vec!["start".to_string().into(), "end".to_string().into()])],
       vec![
            ast_call("ends_with", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::String("end".to_string())))
            ])
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::ends_with(vec![RuntimeValue::Array(vec!["start".to_string().into(), "end".to_string().into()])],
       vec![
            ast_call("ends_with", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::String("start".to_string())))
            ])
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::ends_with(vec![RuntimeValue::Number(1.into())],
       vec![
            ast_call("ends_with", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::String("te".to_string())))
            ])
       ],
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                    name: "ends_with".to_string(),
                                                    args: vec![1.to_string().into(), "te".into()]})))]
    #[case::downcase(vec![RuntimeValue::String("TEST".to_string())],
       vec![ast_call("downcase", SmallVec::new())],
       Ok(vec![RuntimeValue::String("test".to_string())]))]
    #[case::downcase(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "TEST".to_string(), position: None}), None)],
       vec![ast_call("downcase", SmallVec::new())],
       Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "test".to_string(), position: None}), None)]))]
    #[case::upcase(vec![RuntimeValue::String("test".to_string())],
       vec![ast_call("upcase", SmallVec::new())],
       Ok(vec![RuntimeValue::String("TEST".to_string())]))]
    #[case::upcase(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "test".to_string(), position: None}), None)],
       vec![ast_call("upcase", SmallVec::new())],
       Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "TEST".to_string(), position: None}), None)]))]
    #[case::upcase(vec![RuntimeValue::NONE],
       vec![ast_call("upcase", SmallVec::new())],
       Ok(vec![RuntimeValue::NONE]))]
    #[case::upcase(vec![RuntimeValue::Number(123.into())],
       vec![ast_call("upcase", SmallVec::new())],
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                    name: "upcase".to_string(),
                                                    args: vec![123.to_string().into()]})))]
    #[case::replace(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("replace", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::String("test".to_string()))),
                ast_node(ast::Expr::Literal(ast::Literal::String("exam".to_string())))
            ])
       ],
       Ok(vec![RuntimeValue::String("examString".to_string())]))]
    #[case::replace(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "testString".to_string(), position: None}), None)],
       vec![
            ast_call("replace", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::String("test".to_string()))),
                ast_node(ast::Expr::Literal(ast::Literal::String("exam".to_string())))
            ])
       ],
       Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "examString".to_string(), position: None}), None)]))]
    #[case::replace(vec![RuntimeValue::NONE],
       vec![
            ast_call("replace", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::String("test".to_string()))),
                ast_node(ast::Expr::Literal(ast::Literal::String("exam".to_string())))
            ])
       ],
       Ok(vec![RuntimeValue::NONE]))]
    #[case::replace(vec![RuntimeValue::Number(123.into())],
       vec![
            ast_call("replace", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::String("test".to_string()))),
                ast_node(ast::Expr::Literal(ast::Literal::String("exam".to_string())))
            ])
       ],
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                    name: "replace".to_string(),
                                                    args: vec![123.to_string().into(), "test".to_string().into(), "exam".to_string().into()]})))]
    #[case::gsub_regex(vec![RuntimeValue::String("test123".to_string())],
       vec![
            ast_call("gsub", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::String(r"\d+".to_string()))),
                ast_node(ast::Expr::Literal(ast::Literal::String("456".to_string())))
            ])
       ],
       Ok(vec![RuntimeValue::String("test456".to_string())]))]
    #[case::gsub_regex(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "test123".to_string(), position: None}), None)],
       vec![
            ast_call("gsub", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::String(r"\d+".to_string()))),
                ast_node(ast::Expr::Literal(ast::Literal::String("456".to_string())))
            ])
       ],
       Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "test456".to_string(), position: None}), None)]))]
    #[case::gsub_regex(vec![RuntimeValue::Number(123.into())],
       vec![
            ast_call("gsub", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::String("test".to_string()))),
                ast_node(ast::Expr::Literal(ast::Literal::String(r"\d+".to_string()))),
            ])
       ],
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                    name: "gsub".to_string(),
                                                    args: vec![123.to_string().into(), "test".to_string().into(), r"\d+".to_string().into()]})))]
    #[case::len(vec![RuntimeValue::String("testString".to_string())],
       vec![ast_call("len", SmallVec::new())],
       Ok(vec![RuntimeValue::Number(10.into())]))]
    #[case::len(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "testString".to_string(), position: None}), None)],
       vec![ast_call("len", SmallVec::new())],
       Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "10".to_string(), position: None}), None)]))]
    #[case::len(vec![RuntimeValue::TRUE],
       vec![ast_call("len", SmallVec::new())],
       Ok(vec![RuntimeValue::Number(1.into())]))]
    #[case::len(vec![RuntimeValue::String("テスト".to_string())],
       vec![ast_call("len", SmallVec::new())],
       Ok(vec![RuntimeValue::Number(3.into())]))]
    #[case::len(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "テスト".to_string(), position: None}), None)],
       vec![ast_call("len", SmallVec::new())],
       Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "3".to_string(), position: None}), None)]))]
    #[case::utf8bytelen(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("utf8bytelen", SmallVec::new())
       ],
       Ok(vec![RuntimeValue::Number(4.into())]))]
    #[case::utf8bytelen(vec![RuntimeValue::String("テスト".to_string())],
       vec![
            ast_call("utf8bytelen", SmallVec::new())
       ],
       Ok(vec![RuntimeValue::Number(9.into())]))]
    #[case::utf8bytelen(vec![RuntimeValue::String("😊".to_string())],
       vec![
            ast_call("utf8bytelen", SmallVec::new())
       ],
       Ok(vec![RuntimeValue::Number(4.into())]))]
    #[case::utf8bytelen(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "test".to_string(), position: None}), None)],
       vec![
            ast_call("utf8bytelen", SmallVec::new())
       ],
       Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "4".to_string(), position: None}), None)]))]
    #[case::utf8bytelen(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "テスト".to_string(), position: None}), None)],
       vec![
            ast_call("utf8bytelen", SmallVec::new())
       ],
       Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "9".to_string(), position: None}), None)]))]
    #[case::utf8bytelen(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "😊".to_string(), position: None}), None)],
       vec![
            ast_call("utf8bytelen", SmallVec::new())
       ],
       Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "4".to_string(), position: None}), None)]))]
    #[case::utf8bytelen(vec![RuntimeValue::Array(vec![RuntimeValue::String("test".to_string())])],
       vec![
            ast_call("utf8bytelen", SmallVec::new())
       ],
       Ok(vec![RuntimeValue::Number(1.into())]))]
    #[case::utf8bytelen(vec![RuntimeValue::TRUE],
       vec![
            ast_call("utf8bytelen", SmallVec::new())
       ],
       Ok(vec![RuntimeValue::Number(1.into())]))]
    #[case::index(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("index", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::String("test".to_string())))
            ])
       ],
       Ok(vec![RuntimeValue::Number(0.into())]))]
    #[case::index(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "testString".to_string(), position: None}), None)],
       vec![
            ast_call("index", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::String("test".to_string())))
            ])
       ],
       Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "0".to_string(), position: None}), None)]))]
    #[case::index(vec![RuntimeValue::Number(1.into())],
       vec![
            ast_call("index", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::String("test".to_string())))
            ])
       ],
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                    name: "index".to_string(),
                                                    args: vec!["1".into(), "test".into()]})))]
    #[case::array_index(vec![RuntimeValue::Array(vec![RuntimeValue::String("test1".to_string()), RuntimeValue::String("test2".to_string()), RuntimeValue::String("test3".to_string())])],
        vec![
              ast_call("index", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("test2".to_string())))
              ])
        ],
        Ok(vec![RuntimeValue::Number(1.into())]))]
    #[case::array_index_not_found(vec![RuntimeValue::Array(vec![RuntimeValue::String("test1".to_string()), RuntimeValue::String("test2".to_string()), RuntimeValue::String("test3".to_string())])],
        vec![
              ast_call("index", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("test4".to_string())))
              ])
        ],
        Ok(vec![RuntimeValue::Number((-1).into())]))]
    #[case::rindex(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("rindex", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::String("String".to_string())))
            ])
       ],
       Ok(vec![RuntimeValue::Number(4.into())]))]
    #[case::rindex(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "testString".to_string(), position: None}), None)],
       vec![
            ast_call("rindex", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::String("String".to_string())))
            ])
       ],
       Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "4".to_string(), position: None}), None)]))]
    #[case::rindex(vec![RuntimeValue::Number(123.into())],
       vec![
            ast_call("rindex", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::String("String".to_string())))
            ])
       ],
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                    name: "rindex".to_string(),
                                                    args: vec!["123".into(), "String".into()]})))]
    #[case::array_rindex(vec![RuntimeValue::Array(vec![RuntimeValue::String("test1".to_string()), RuntimeValue::String("test2".to_string()), RuntimeValue::String("test1".to_string())])],
        vec![
              ast_call("rindex", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("test1".to_string())))
              ])
        ],
        Ok(vec![RuntimeValue::Number(2.into())]))]
    #[case::array_rindex(vec![RuntimeValue::Array(vec![RuntimeValue::String("test1".to_string()), RuntimeValue::String("test2".to_string()), RuntimeValue::String("test3".to_string())])],
        vec![
              ast_call("rindex", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("test4".to_string())))
              ])
        ],
        Ok(vec![RuntimeValue::Number((-1).into())]))]
    #[case::array_rindex_empty(vec![RuntimeValue::Array(Vec::new())],
        vec![
              ast_call("rindex", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("test".to_string())))
              ])
        ],
        Ok(vec![RuntimeValue::Number((-1).into())]))]
    #[case::eq(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("eq", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("eq".to_string()))),
                    ast_node(ast::Expr::Literal(ast::Literal::String("eq".to_string())))
                ])
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::eq(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("eq", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::String("eq".to_string()))),
                ast_node(ast::Expr::Literal(ast::Literal::String("eq1".to_string())))
            ])
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::ne(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("ne", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::String("eq".to_string()))),
                ast_node(ast::Expr::Literal(ast::Literal::String("eq1".to_string())))
            ])
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::ne(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("ne", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("eq".to_string()))),
                    ast_node(ast::Expr::Literal(ast::Literal::String("eq".to_string())))
                ])
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::ne(vec![RuntimeValue::Number(1.3.into())],
       vec![
            ast_call("ne", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.3.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.3.into()))),
                ]),
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::ne(vec![RuntimeValue::Number(1.3.into())],
       vec![
            ast_call("ne", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.3.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.2.into()))),
                ]),
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::gt(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("gt", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(2.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                ]),
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::gt(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("gt", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(2.into()))),
                ]),
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::gt(vec![RuntimeValue::Number(1.3.into())],
       vec![
            ast_call("gt", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.4.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.3.into()))),
                ]),
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::gt(vec![RuntimeValue::Number(1.3.into())],
       vec![
            ast_call("gt", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.3.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.4.into()))),
                ]),
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::gt(vec![RuntimeValue::Number(1.3.into())],
       vec![
            ast_call("gt", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::String(1.to_string()))),
                    ast_node(ast::Expr::Literal(ast::Literal::String(2.to_string()))),
                ]),
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::gt(vec![RuntimeValue::Number(1.3.into())],
       vec![
            ast_call("gt", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::String(2.to_string()))),
                    ast_node(ast::Expr::Literal(ast::Literal::String(1.to_string()))),
                ]),
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::gt(vec![RuntimeValue::FALSE],
       vec![
            ast_call("gt", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(true))),
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(true))),
                ]),
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::gt(vec![RuntimeValue::FALSE],
       vec![
            ast_call("gt", smallvec![
                ast_call("to_code", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("1".into()))),
                ]),
                ast_call("to_code", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("2".into()))),
                ]),
            ]),
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::gt(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("gt", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(2.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::String("1".to_string()))),
                ]),
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::gte(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("gte", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                ]),
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::gte(vec![RuntimeValue::String("test".to_string())],
       vec! [
            ast_call("gte", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(2.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                ]),
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::gte(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("gte", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(2.into()))),
                ]),
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::gte(vec![RuntimeValue::Number(1.3.into())],
       vec![
            ast_call("gte", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.3.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.3.into()))),
                ]),
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::gte(vec![RuntimeValue::Number(1.3.into())],
       vec![
            ast_call("gte", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.2.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.3.into()))),
                ]),
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::gte(vec![RuntimeValue::String("test".to_string())],
       vec! [
            ast_call("gte", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::String(2.to_string()))),
                    ast_node(ast::Expr::Literal(ast::Literal::String(1.to_string()))),
                ]),
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::gte(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("gte", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::String(1.to_string()))),
                    ast_node(ast::Expr::Literal(ast::Literal::String(2.to_string()))),
                ]),
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::gte(vec![RuntimeValue::TRUE],
       vec![
            ast_call("gte", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(true))),
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(true))),
                ]),
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::gte(vec![RuntimeValue::TRUE],
       vec![
            ast_call("gte", smallvec![
                ast_call("to_code", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("1".into()))),
                ]),
                ast_call("to_code", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("1".into()))),
                ]),
            ]),
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::lt(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("lt", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(2.into()))),
                ]),
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::lt(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("lt", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(2.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                ]),
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::lt(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("lt", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.2.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.3.into()))),
                ])
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::lt(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("lt", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.3.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.2.into()))),
                ])
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::lt(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("lt", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::String(1.to_string()))),
                    ast_node(ast::Expr::Literal(ast::Literal::String(2.to_string()))),
                ]),
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::lt(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("lt", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::String(2.to_string()))),
                    ast_node(ast::Expr::Literal(ast::Literal::String(1.to_string()))),
                ]),
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::lt(vec![RuntimeValue::TRUE],
       vec![
            ast_call("lt", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(true))),
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(true))),
                ]),
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::lt(vec![RuntimeValue::TRUE],
       vec![
            ast_call("lt", smallvec![
                ast_call("to_code", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("2".into()))),
                ]),
                ast_call("to_code", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("1".into()))),
                ]),
            ]),
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::lt(vec![RuntimeValue::TRUE],
       vec![
            ast_call("lt", smallvec![
                ast_call("to_code", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("1".into()))),
                ]),
                ast_call("to_code", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("2".into()))),
                ]),
            ]),
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::lt(vec![RuntimeValue::TRUE],
       vec![
            ast_call("lt", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::String("2".into()))),
                ]),
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::lte(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("lte", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                ])
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::lte(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("lte", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(2.into()))),
                ])
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::lte(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("lte", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(2.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                ])
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::lte(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("lte", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.3.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.3.into()))),
                ])
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::lte(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("lte", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.2.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.3.into()))),
                ])
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::lte(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("lte", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.4.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.3.into()))),
                ]),
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::lte(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("lte", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(2.into()))),
                ])
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::lte(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("lte", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::String(2.to_string()))),
                    ast_node(ast::Expr::Literal(ast::Literal::String(1.to_string()))),
                ])
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::lte(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("lte", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::String("2".to_string()))),
                ])
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::lte(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("lte", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(false))),
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(false))),
                ])
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::add(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("add", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                ]),
       ],
       Ok(vec![RuntimeValue::Number(2.into())]))]
    #[case::add(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("add", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("te".to_string()))),
                    ast_node(ast::Expr::Literal(ast::Literal::String("st".to_string()))),
                ]),
       ],
       Ok(vec![RuntimeValue::String("test".to_string())]))]
    #[case::add(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("add", smallvec![
                    ast_call("array", smallvec![
                        ast_node(ast::Expr::Literal(ast::Literal::String("te".to_string())))
                    ]),
                    ast_call("array", smallvec![
                        ast_node(ast::Expr::Literal(ast::Literal::String("te".to_string())))
                    ])
                ]),
       ],
       Ok(vec![RuntimeValue::Array(vec!["te".to_string().into(), "te".to_string().into()])]))]
    #[case::add(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("add", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(true))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                ]),
       ],
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                         name: "add".to_string(),
                                                         args: vec![true.to_string().into(), 1.to_string().into()]})))]
    #[case::add(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("add", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.3.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.3.into()))),
                ]),
       ],
       Ok(vec![RuntimeValue::Number(2.6.into())]))]
    #[case::add(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("add", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.3.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.3.into()))),
                ]),
       ],
       Ok(vec![RuntimeValue::Number(2.6.into())]))]
    #[case::add(vec![RuntimeValue::TRUE],
       vec![
            ast_call("add", smallvec![
                ast_call("to_code", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("2".into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::None)),
                ]),
                ast_call("to_code", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("1".into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::None)),
                ]),
            ]),
       ],
       Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Code(mq_markdown::Code{value: "21".to_string(), lang: None, fence: true, meta: None, position: None}), None)]))]
    #[case::add(vec![RuntimeValue::TRUE],
       vec![
            ast_call("add", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::String("2".into()))),
                ast_call("to_code", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("1".into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::None)),
                ]),
            ]),
       ],
       Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Code(mq_markdown::Code{value: "21".to_string(), lang: None, fence: true, meta: None, position: None}), None)]))]
    #[case::add(vec![RuntimeValue::TRUE],
       vec![
            ast_call("add", smallvec![
                ast_call("to_code", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("2".into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::None)),
                ]),
                ast_node(ast::Expr::Literal(ast::Literal::String("1".into()))),
            ]),
       ],
       Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Code(mq_markdown::Code{value: "21".to_string(), lang: None, fence: true, meta: None, position: None}), None)]))]
    #[case::sub(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("sub", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                ]),
       ],
       Ok(vec![RuntimeValue::Number(0.into())]))]
    #[case::sub(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("sub", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("te".to_string()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                ]),
       ],
       Err(InnerError::Eval(EvalError::RuntimeError(Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()}, "invalid float literal".to_string()))))]
    #[case::sub(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("sub", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.3.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.2.into()))),
                ])
       ],
       Ok(vec![RuntimeValue::Number(0.10000000000000009.into())]))]
    #[case::div(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("div", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                ])
       ],
       Ok(vec![RuntimeValue::Number(1.into())]))]
    #[case::div(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("div", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("te".to_string()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                ])
       ],
       Err(InnerError::Eval(EvalError::RuntimeError(Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()}, "invalid float literal".to_string()))))]
    #[case::div(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("div", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(0.into()))),
                ])
       ],
       Err(InnerError::Eval(EvalError::ZeroDivision(Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()}))))]
    #[case::div(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("div", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.3.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.1.into()))),
                ])
       ],
       Ok(vec![RuntimeValue::Number(1.1818181818181817.into())]))]
    #[case::mul(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("mul", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(2.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                ]),
       ],
       Ok(vec![RuntimeValue::Number(2.into())]))]
    #[case::mul(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("mul", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(2.0.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.3.into()))),
                ]),
       ],
       Ok(vec![RuntimeValue::Number(2.6.into())]))]
    #[case::mul(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("mul", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("te".to_string()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(2.into()))),
                ]),
       ],
       Ok(vec![RuntimeValue::String("tete".to_string())]))]
    #[case::mod_(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("mod", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(2.into()))),
                ]),
       ],
       Ok(vec![RuntimeValue::Number(1.into())]))]
    #[case::mod_(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("mod", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.1.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(2.0.into()))),
                ]),
       ],
       Ok(vec![RuntimeValue::Number(1.1.into())]))]
    #[case::mod_(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("mod", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("te".to_string()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                ]),
       ],
       Err(InnerError::Eval(EvalError::RuntimeError(Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()}, "invalid float literal".to_string()))))]
    #[case::pow(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("pow", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(2.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(3.into()))),
                ]),
       ],
       Ok(vec![RuntimeValue::Number(8.into())]))]
    #[case::pow(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("pow", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("te".to_string()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                ]),
       ],
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                    name: "pow".to_string(),
                                                    args: vec!["te".to_string().into(), 1.to_string().into()]})))]
    #[case::and(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("and", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(true))),
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(true))),
                ]),
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::and(vec![RuntimeValue::TRUE],
       vec![
            ast_call("and", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(true))),
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(false))),
                ]),
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::and(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("and", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(false))),
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(true))),
                ]),
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::and(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("and", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(false))),
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(false))),
                ]),
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::or(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("or", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(true))),
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(true))),
                ]),
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::or(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("or", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(true))),
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(false))),
                ]),
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::or(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("or", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(false))),
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(true))),
                ]),
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::or(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("or", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(false))),
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(false))),
                ]),
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::not(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("not", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(true))),
                ]),
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::not(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("not", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(false))),
                ]),
       ],
       Ok(vec![RuntimeValue::TRUE]))]
    #[case::to_string(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("to_string", SmallVec::new())
       ],
       Ok(vec![RuntimeValue::String("test".to_string())]))]
    #[case::to_string(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "test".to_string(), position: None}), None)],
       vec![
            ast_call("to_string", SmallVec::new())
       ],
       Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "test".to_string(), position: None}), None)]))]
    #[case::split1(vec![RuntimeValue::String("test1,test2".to_string())],
       vec![
            ast_call("split", smallvec![
                        ast_node(ast::Expr::Literal(ast::Literal::String(",".to_string())))]
                        )
       ],
       Ok(vec![RuntimeValue::Array(vec![RuntimeValue::String("test1".to_string()), RuntimeValue::String("test2".to_string())])]))]
    #[case::split2(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "test1,test2".to_string(), position: None}), None)],
       vec![
            ast_call("split", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::String(",".to_string())))
            ])
       ],
       Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: r#"["test1", "test2"]"#.to_string(), position: None}), None)]))]
    #[case::split(vec![RuntimeValue::Number(1.into())],
       vec![
            ast_call("split", smallvec![ast_node(ast::Expr::Literal(ast::Literal::String(",".to_string())))])
       ],
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                    name: "split".to_string(),
                                                    args: vec![1.to_string().into(), ",".to_string().into()]})))]
    #[case::split_array(vec![RuntimeValue::Array(vec![
            RuntimeValue::String("value1".to_string()),
            RuntimeValue::String("separator".to_string()),
            RuntimeValue::String("value2".to_string()),
        ])],
        vec![
            ast_call("split", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::String("separator".to_string())))
            ])
        ],
        Ok(vec![RuntimeValue::Array(vec![
            RuntimeValue::Array(vec![RuntimeValue::String("value1".to_string())]),
            RuntimeValue::Array(vec![RuntimeValue::String("value2".to_string())])
        ])]))]
    #[case::split_array_multiple_separators(vec![RuntimeValue::Array(vec![
            RuntimeValue::String("value1".to_string()),
            RuntimeValue::String("separator".to_string()),
            RuntimeValue::String("value2".to_string()),
            RuntimeValue::String("separator".to_string()),
            RuntimeValue::String("value3".to_string()),
        ])],
        vec![
            ast_call("split", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::String("separator".to_string())))
            ])
        ],
        Ok(vec![RuntimeValue::Array(vec![
            RuntimeValue::Array(vec![RuntimeValue::String("value1".to_string())]),
            RuntimeValue::Array(vec![RuntimeValue::String("value2".to_string())]),
            RuntimeValue::Array(vec![RuntimeValue::String("value3".to_string())])
        ])]))]
    #[case::split_array_no_separator(vec![RuntimeValue::Array(vec![
            RuntimeValue::String("value1".to_string()),
            RuntimeValue::String("value2".to_string()),
            RuntimeValue::String("value3".to_string()),
        ])],
        vec![
            ast_call("split", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::String("separator".to_string())))
            ])
        ],
        Ok(vec![RuntimeValue::Array(vec![
        RuntimeValue::Array(vec![
            RuntimeValue::String("value1".to_string()),
            RuntimeValue::String("value2".to_string()),
            RuntimeValue::String("value3".to_string())
        ])
        ])]))]
    #[case::split_array_empty(vec![RuntimeValue::Array(Vec::new())],
        vec![
            ast_call("split", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::String("separator".to_string())))
            ])
        ],
        Ok(vec![RuntimeValue::Array(vec![RuntimeValue::Array(Vec::new())])]))]
    #[case::split_array_mixed_types(vec![RuntimeValue::Array(vec![
            RuntimeValue::Number(1.into()),
            RuntimeValue::String("separator".to_string()),
            RuntimeValue::Bool(true),
        ])],
        vec![
            ast_call("split", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::String("separator".to_string())))
            ])
        ],
        Ok(vec![RuntimeValue::Array(vec![
            RuntimeValue::Array(vec![RuntimeValue::Number(1.into())]),
            RuntimeValue::Array(vec![RuntimeValue::Bool(true)])
        ])]))]
    #[case::join1(vec![RuntimeValue::String("test1,test2".to_string())],
       vec![
            ast_call("join", smallvec![
                ast_call("split", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::String(",".to_string())))
                ]),
                ast_node(ast::Expr::Literal(ast::Literal::String("#".to_string())))
            ])
       ],
       Ok(vec![RuntimeValue::String("test1#test2".to_string())]))]
    #[case::join_error(vec![RuntimeValue::Number(1.into())],
       vec![
            ast_call("join", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::String("#".to_string())))
            ])
       ],
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                    name: "join".to_string(),
                                                    args: vec![1.to_string().into(), "#".to_string().into()]})))]
    #[case::reverse_string(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("reverse", SmallVec::new())
       ],
       Ok(vec![RuntimeValue::String("tset".to_string())]))]
    #[case::reverse_string_empty(vec![RuntimeValue::String("".to_string())],
       vec![
            ast_call("reverse", SmallVec::new())
       ],
       Ok(vec![RuntimeValue::String("".to_string())]))]
    #[case::reverse_array(vec![RuntimeValue::Array(vec![
            RuntimeValue::String("a".to_string()),
            RuntimeValue::String("b".to_string()),
            RuntimeValue::String("c".to_string()),
        ])],
        vec![
            ast_call("reverse", SmallVec::new())
        ],
        Ok(vec![RuntimeValue::Array(vec![
            RuntimeValue::String("c".to_string()),
            RuntimeValue::String("b".to_string()),
            RuntimeValue::String("a".to_string()),
        ])]))]
    #[case::reverse_array_empty(vec![RuntimeValue::Array(Vec::new())],
        vec![
            ast_call("reverse", SmallVec::new())
        ],
        Ok(vec![RuntimeValue::Array(Vec::new())]))]
    #[case::reverse_number(vec![RuntimeValue::Number(123.into())],
       vec![
            ast_call("reverse", SmallVec::new())
       ],
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                    name: "reverse".to_string(),
                                                    args: vec![123.to_string().into()]})))]
    #[case::base64(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("base64", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::String("test".to_string())))
            ])
       ],
       Ok(vec![RuntimeValue::String("dGVzdA==".to_string())]))]
    #[case::base64(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value:"test".to_string(), position: None}), None)],
       vec![
            ast_call("base64", SmallVec::new())
       ],
       Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "dGVzdA==".to_string(), position: None}), None)]))]
    #[case::base64(vec![RuntimeValue::Number(1.into())],
       vec![
            ast_call("base64", SmallVec::new())
       ],
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                    name: "base64".to_string(),
                                                    args: vec![1.to_string().into()]})))]
    #[case::base64d(vec![RuntimeValue::String("dGVzdA==".to_string())],
       vec![
            ast_call("base64d", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::String("dGVzdA==".to_string())))
            ])
       ],
       Ok(vec![RuntimeValue::String("test".to_string())]))]
    #[case::base64d(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value:"dGVzdA==".to_string(), position: None}), None)],
       vec![
            ast_call("base64d", smallvec![
            ])
       ],
       Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "test".to_string(), position: None}), None)]))]
    #[case::base64d(vec![RuntimeValue::Number(1.into())],
       vec![
            ast_call("base64d", smallvec![
            ])
       ],
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                    name: "base64d".to_string(),
                                                    args: vec![1.to_string().into()]})))]
    #[case::def(vec![RuntimeValue::String("test1,test2".to_string())],
       vec![
            ast_node(ast::Expr::Def(
                ast::Ident::new("split2"),
                smallvec![
                    ast_node(ast::Expr::Ident(ast::Ident::new("str"))),
                ],
                vec![ast_call("split",
                    smallvec![
                        ast_node(ast::Expr::Ident(ast::Ident::new("str"))),
                        ast_node(ast::Expr::Literal(ast::Literal::String(",".to_string()))),
                    ])
                ]
            )),
            ast_call("split2", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::String("test1,test2".to_string()))),
            ]),
       ],
       Ok(vec![RuntimeValue::Array(vec![RuntimeValue::String("test1".to_string()), RuntimeValue::String("test2".to_string())])]))]
    #[case::def2(vec![RuntimeValue::String("Hello".to_string())],
       vec![
            ast_node(ast::Expr::Def(
                ast::Ident::new("concat_self"),
                smallvec![
                    ast_node(ast::Expr::Ident(ast::Ident::new("str1"))),
                    ast_node(ast::Expr::Ident(ast::Ident::new("str2"))),
                ],
                vec![ast_call("add",
                    smallvec![
                        ast_node(ast::Expr::Ident(ast::Ident::new("str1"))),
                        ast_node(ast::Expr::Ident(ast::Ident::new("str2"))),
                    ])
                ]
            )),
            ast_call("concat_self", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::String("Hello".to_string()))),
                ast_node(ast::Expr::Literal(ast::Literal::String("World".to_string()))),
            ])
       ],
       Ok(vec![RuntimeValue::String("HelloWorld".to_string())]))]
    #[case::def3(vec![RuntimeValue::String("Test".to_string())],
       vec![
            ast_node(ast::Expr::Def(
                ast::Ident::new("prepend_self"),
                smallvec![
                    ast_node(ast::Expr::Ident(ast::Ident::new("str1"))),
                    ast_node(ast::Expr::Ident(ast::Ident::new("str2"))),
                ],
                vec![ast_call("add",
                    smallvec![
                        ast_node(ast::Expr::Ident(ast::Ident::new("str1"))),
                        ast_node(ast::Expr::Ident(ast::Ident::new("str2"))),
                    ])
                ]
            )),
            ast_call("prepend_self", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::String("test".to_string())))
            ])
       ],
       Ok(vec![RuntimeValue::String("Testtest".to_string())]))]
    #[case::type_string(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("type", SmallVec::new())
       ],
       Ok(vec![RuntimeValue::String("string".to_string())]))]
    #[case::type_int(vec![RuntimeValue::Number(42.into())],
       vec![
            ast_call("type", SmallVec::new())
       ],
       Ok(vec![RuntimeValue::String("number".to_string())]))]
    #[case::type_bool(vec![RuntimeValue::TRUE],
       vec![
            ast_call("type", SmallVec::new())
       ],
       Ok(vec![RuntimeValue::String("bool".to_string())]))]
    #[case::type_array(vec![RuntimeValue::Array(vec![RuntimeValue::String("test".to_string())])],
       vec![
            ast_call("type", SmallVec::new())
       ],
       Ok(vec![RuntimeValue::String("array".to_string())]))]
    #[case::min(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("min", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(2.into()))),
                ])
        ],
       Ok(vec![RuntimeValue::Number(1.into())]))]
    #[case::min(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("min", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("1".into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::String("2".into()))),
                ])
        ],
       Ok(vec![RuntimeValue::String("1".into())]))]
    #[case::min(vec![RuntimeValue::Number(1.into())],
       vec![
            ast_call("min", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::Number(2.into()))),
            ])
        ],
       Ok(vec![RuntimeValue::Number(1.into())]))]
    #[case::min(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("min", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("te".to_string()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                ])
            ],
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                    name: "min".to_string(),
                                                    args: vec!["te".to_string().into(), 1.to_string().into()]})))]
    #[case::max(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("max", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(2.into()))),
                ])
            ],
       Ok(vec![RuntimeValue::Number(2.into())]))]
    #[case::max(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("max", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("1".into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::String("2".into()))),
                ])
            ],
       Ok(vec![RuntimeValue::String("2".into())]))]
    #[case::max(vec![RuntimeValue::Number(3.into())],
       vec![
            ast_call("max", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                ])
            ],
       Ok(vec![RuntimeValue::Number(3.into())]))]
    #[case::max(vec![RuntimeValue::String("test".to_string())],
       vec![
            ast_call("max", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("te".to_string()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                ])
            ],
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                    name: "max".to_string(),
                                                    args: vec!["te".to_string().into(), 1.to_string().into()]})))]
    #[case::trim(vec![RuntimeValue::String("  test  ".to_string())],
       vec![
            ast_call("trim", SmallVec::new())
       ],
       Ok(vec![RuntimeValue::String("test".to_string())]))]
    #[case::trim(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "  test  ".to_string(), position: None}), None)],
       vec![
            ast_call("trim", SmallVec::new())
       ],
       Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "test".to_string(), position: None}), None)]))]
    #[case::trim(vec![RuntimeValue::Number(1.into())],
       vec![
            ast_call("trim", SmallVec::new())
       ],
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                    name: "trim".to_string(),
                                                    args: vec![1.to_string().into()]})))]
    #[case::slice(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "testString".to_string(), position: None}), None)],
       vec![
            ast_call("slice", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::Number(0.into()))),
                ast_node(ast::Expr::Literal(ast::Literal::Number(4.into()))),
            ])
       ],
       Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "test".to_string(), position: None}), None)]))]
    #[case::slice(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("slice", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::Number(0.into()))),
                ast_node(ast::Expr::Literal(ast::Literal::Number(4.into()))),
            ])
       ],
       Ok(vec![RuntimeValue::String("test".to_string())]))]
    #[case::slice(vec![RuntimeValue::String("testString".to_string())],
       vec![
            ast_call("slice", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::Number(4.into()))),
                ast_node(ast::Expr::Literal(ast::Literal::Number(10.into()))),
            ])
       ],
       Ok(vec![RuntimeValue::String("String".to_string())]))]
    #[case::slice(vec![RuntimeValue::NONE],
       vec![
            ast_call("slice", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::Number(0.into()))),
                ast_node(ast::Expr::Literal(ast::Literal::Number(4.into()))),
            ])
       ],
       Ok(vec![RuntimeValue::NONE]))]
    #[case::slice_array(vec![RuntimeValue::Array(vec![
            RuntimeValue::String("item1".to_string()),
            RuntimeValue::String("item2".to_string()),
            RuntimeValue::String("item3".to_string()),
            RuntimeValue::String("item4".to_string()),
            RuntimeValue::String("item5".to_string()),
        ])],
       vec![
            ast_call("slice", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                ast_node(ast::Expr::Literal(ast::Literal::Number(4.into()))),
            ])
       ],
       Ok(vec![RuntimeValue::Array(vec![
            RuntimeValue::String("item2".to_string()),
            RuntimeValue::String("item3".to_string()),
            RuntimeValue::String("item4".to_string()),
        ])]))]
    #[case::slice_array_from_start(vec![RuntimeValue::Array(vec![
            RuntimeValue::String("item1".to_string()),
            RuntimeValue::String("item2".to_string()),
            RuntimeValue::String("item3".to_string()),
        ])],
       vec![
            ast_call("slice", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::Number(0.into()))),
                ast_node(ast::Expr::Literal(ast::Literal::Number(2.into()))),
            ])
       ],
       Ok(vec![RuntimeValue::Array(vec![
            RuntimeValue::String("item1".to_string()),
            RuntimeValue::String("item2".to_string()),
        ])]))]
    #[case::slice_array_to_end(vec![RuntimeValue::Array(vec![
            RuntimeValue::String("item1".to_string()),
            RuntimeValue::String("item2".to_string()),
            RuntimeValue::String("item3".to_string()),
        ])],
       vec![
            ast_call("slice", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                ast_node(ast::Expr::Literal(ast::Literal::Number(3.into()))),
            ])
       ],
       Ok(vec![RuntimeValue::Array(vec![
            RuntimeValue::String("item2".to_string()),
            RuntimeValue::String("item3".to_string()),
       ])]))]
    #[case::slice_array_out_of_bounds(vec![RuntimeValue::Array(vec![
            RuntimeValue::String("item1".to_string()),
            RuntimeValue::String("item2".to_string()),
            RuntimeValue::String("item3".to_string()),
        ])],
       vec![
            ast_call("slice", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::Number(2.into()))),
                ast_node(ast::Expr::Literal(ast::Literal::Number(5.into()))),
            ])
       ],
       Ok(vec![RuntimeValue::Array(vec![
            RuntimeValue::String("item3".to_string()),
        ])]))]
    #[case::slice_array_empty(vec![RuntimeValue::Array(Vec::new())],
       vec![
            ast_call("slice", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::Number(0.into()))),
                ast_node(ast::Expr::Literal(ast::Literal::Number(2.into()))),
            ])
       ],
       Ok(vec![RuntimeValue::Array(Vec::new())]))]
    #[case::slice_array_mixed_types(vec![RuntimeValue::Array(vec![
            RuntimeValue::String("item1".to_string()),
            RuntimeValue::Number(42.into()),
            RuntimeValue::Bool(true),
            RuntimeValue::String("item4".to_string()),
        ])],
       vec![
            ast_call("slice", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                ast_node(ast::Expr::Literal(ast::Literal::Number(3.into()))),
            ])
       ],
       Ok(vec![RuntimeValue::Array(vec![
            RuntimeValue::Number(42.into()),
            RuntimeValue::Bool(true),
        ])]))]
    #[case::slice(vec![RuntimeValue::Number(123.into())],
       vec![
            ast_call("slice", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::Number(0.into()))),
                ast_node(ast::Expr::Literal(ast::Literal::Number(4.into()))),
            ])
       ],
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                    name: "slice".to_string(),
                                                    args: vec![123.to_string().into(), 0.to_string().into(), 4.to_string().into()]})))]
    #[case::match_regex1(vec![RuntimeValue::String("test123".to_string())],
       vec![
            ast_call("match", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::String(r"\d+".to_string()))),
            ])
       ],
       Ok(vec![RuntimeValue::Array(vec![RuntimeValue::String("123".to_string())])]))]
    #[case::match_regex2(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "test123".to_string(), position: None}), None)],
       vec![
            ast_call("match", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::String(r"\d+".to_string()))),
            ])
       ],
       Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: r#"["123"]"#.to_string(), position: None}), None)]))]
    #[case::match_regex3(vec![RuntimeValue::Number(123.into())],
       vec![
            ast_call("match", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::String(r"\d+".to_string()))),
            ])
       ],
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                    name: "match".to_string(),
                                                    args: vec![123.to_string().into(), r"\d+".to_string().into()]})))]
    #[case::explode(vec![RuntimeValue::String("ABC".to_string())],
       vec![
            ast_call("explode", SmallVec::new())
       ],
       Ok(vec![RuntimeValue::Array(vec![
            RuntimeValue::Number(65.into()),
            RuntimeValue::Number(66.into()),
            RuntimeValue::Number(67.into()),
       ])]))]
    #[case::explode(vec![RuntimeValue::Number(123.into())],
       vec![
            ast_call("explode", SmallVec::new())
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
            ast_call("implode", SmallVec::new())
       ],
       Ok(vec![RuntimeValue::String("ABC".to_string())]))]
    #[case::implode(vec!["test".to_string().into()],
       vec![
            ast_call("implode", SmallVec::new())
       ],
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                    name: "implode".to_string(),
                                                    args: vec!["test".to_string().into()]})))]
    #[case::explode_markdown(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "ABC".to_string(), position: None}), None)],
        vec![
             ast_call("explode", SmallVec::new())
        ],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "[65, 66, 67]".to_string(), position: None}), None)]))]
    #[case::to_number(vec![RuntimeValue::String("42".to_string())],
       vec![
            ast_call("to_number", SmallVec::new())
       ],
       Ok(vec![RuntimeValue::Number(42.into())]))]
    #[case::to_number(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "42".to_string(), position: None}), None)],
       vec![
            ast_call("to_number", SmallVec::new())
       ],
       Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "42".to_string(), position: None}), None)]))]
    #[case::to_number(vec![RuntimeValue::String("42.5".to_string())],
       vec![
            ast_call("to_number", SmallVec::new())
       ],
       Ok(vec![RuntimeValue::Number(42.5.into())]))]
    #[case::to_number(vec![RuntimeValue::String("not a number".to_string())],
       vec![
            ast_call("to_number", SmallVec::new())
       ],
       Err(InnerError::Eval(EvalError::RuntimeError(Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()}, "invalid float literal".to_string()))))]
    #[case::to_number_array(vec![RuntimeValue::Array(vec![RuntimeValue::String("42".to_string()), RuntimeValue::String("43".to_string()), RuntimeValue::String("44".to_string())])],
        vec![
              ast_call("to_number", SmallVec::new())
        ],
        Ok(vec![RuntimeValue::Array(vec![RuntimeValue::Number(42.into()), RuntimeValue::Number(43.into()), RuntimeValue::Number(44.into())])]))]
    #[case::to_number_array(vec![RuntimeValue::Array(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "42".to_string(), position: None}), None)])],
        vec![
              ast_call("to_number", SmallVec::new())
        ],
        Ok(vec![RuntimeValue::Array(vec![RuntimeValue::Number(42.into())])]))]
    #[case::to_number_array_with_invalid(vec![RuntimeValue::Array(vec![RuntimeValue::String("42".to_string()), RuntimeValue::String("not a number".to_string()), RuntimeValue::String("44".to_string())])],
        vec![
              ast_call("to_number", SmallVec::new())
        ],
        Err(InnerError::Eval(EvalError::RuntimeError(Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()}, "invalid float literal".to_string()))))]
    #[case::to_number_array_empty(vec![RuntimeValue::Array(Vec::new())],
        vec![
              ast_call("to_number", SmallVec::new())
        ],
        Ok(vec![RuntimeValue::Array(Vec::new())]))]
    #[case::to_number_array_mixed_types(vec![RuntimeValue::Array(vec![RuntimeValue::String("42".to_string()), RuntimeValue::Number(43.into()), RuntimeValue::String("44".to_string())])],
        vec![
              ast_call("to_number", SmallVec::new())
        ],
        Ok(vec![RuntimeValue::Array(vec![RuntimeValue::Number(42.into()), RuntimeValue::Number(43.into()), RuntimeValue::Number(44.into())])]))]
    #[case::trunc(vec![RuntimeValue::Number(42.5.into())],
       vec![
            ast_call("trunc", SmallVec::new())
       ],
       Ok(vec![RuntimeValue::Number(42.into())]))]
    #[case::trunc(vec![RuntimeValue::Number((-42.5).into())],
       vec![
            ast_call("trunc", SmallVec::new())
       ],
       Ok(vec![RuntimeValue::Number((-42).into())]))]
    #[case::trunc(vec!["42.5".to_string().into()],
       vec![
            ast_call("trunc", SmallVec::new())
       ],
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                    name: "trunc".to_string(),
                                                    args: vec!["42.5".to_string().into()]})))]
    #[case::abs_positive(vec![RuntimeValue::Number(42.into())],
       vec![
            ast_call("abs", SmallVec::new())
       ],
       Ok(vec![RuntimeValue::Number(42.into())]))]
    #[case::abs_negative(vec![RuntimeValue::Number((-42).into())],
        vec![
            ast_call("abs", SmallVec::new())
        ],
        Ok(vec![RuntimeValue::Number(42.into())]))]
    #[case::abs_zero(vec![RuntimeValue::Number(0.into())],
        vec![
            ast_call("abs", SmallVec::new())
        ],
        Ok(vec![RuntimeValue::Number(0.into())]))]
    #[case::abs_decimal(vec![RuntimeValue::Number((-42.5).into())],
        vec![
            ast_call("abs", SmallVec::new())
        ],
        Ok(vec![RuntimeValue::Number(42.5.into())]))]
    #[case::abs_invalid_type(vec![RuntimeValue::String("42".to_string())],
        vec![
            ast_call("abs", SmallVec::new())
        ],
        Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                     name: "abs".to_string(),
                                                     args: vec!["42".to_string().into()]})))]
    #[case::ceil(vec![RuntimeValue::Number(42.1.into())],
        vec![
            ast_call("ceil", SmallVec::new())
        ],
        Ok(vec![RuntimeValue::Number(43.into())]))]
    #[case::ceil(vec![RuntimeValue::Number((-42.1).into())],
        vec![
            ast_call("ceil", SmallVec::new())
        ],
        Ok(vec![RuntimeValue::Number((-42).into())]))]
    #[case::ceil(vec!["42".to_string().into()],
        vec![
            ast_call("ceil", SmallVec::new())
        ],
        Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                     name: "ceil".to_string(),
                                                     args: vec!["42".to_string().into()]})))]
    #[case::round(vec![RuntimeValue::Number(42.5.into())],
        vec![
            ast_call("round", SmallVec::new())
        ],
        Ok(vec![RuntimeValue::Number(43.into())]))]
    #[case::round(vec![RuntimeValue::Number(42.4.into())],
        vec![
            ast_call("round", SmallVec::new())
        ],
        Ok(vec![RuntimeValue::Number(42.into())]))]
    #[case::round(vec!["42.4".to_string().into()],
        vec![
            ast_call("round", SmallVec::new())
        ],
        Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                     name: "round".to_string(),
                                                     args: vec!["42.4".to_string().into()]})))]
    #[case::floor(vec![RuntimeValue::Number(42.9.into())],
        vec![
            ast_call("floor", SmallVec::new())
        ],
        Ok(vec![RuntimeValue::Number(42.into())]))]
    #[case::floor(vec![RuntimeValue::Number((-42.9).into())],
        vec![
            ast_call("floor", SmallVec::new())
        ],
        Ok(vec![RuntimeValue::Number((-43).into())]))]
    #[case::floor_erro(vec!["42.9".to_string().into()],
        vec![
            ast_call("floor", SmallVec::new())
        ],
        Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                     name: "floor".to_string(),
                                                     args: vec!["42.9".to_string().into()]})))]
    #[case::del(vec![RuntimeValue::Array(vec![RuntimeValue::String("test1".to_string()), RuntimeValue::String("test2".to_string())])],
        vec![
              ast_call("del", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(0.into()))),
              ]),
        ],
        Ok(vec![RuntimeValue::Array(vec![RuntimeValue::String("test2".to_string())])]))]
    #[case::del(vec![RuntimeValue::Array(vec![RuntimeValue::String("test1".to_string()), RuntimeValue::String("test2".to_string())])],
        vec![
              ast_call("del", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
              ]),
        ],
        Ok(vec![RuntimeValue::Array(vec![RuntimeValue::String("test1".to_string())])]))]
    #[case::del(vec![RuntimeValue::String("test1".to_string())],
        vec![
              ast_call("del", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(4.into()))),
              ]),
        ],
        Ok(vec![RuntimeValue::String("test".to_string())]))]
    #[case::del(vec![RuntimeValue::Number(123.into())],
        vec![
              ast_call("del", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(4.into()))),
              ]),
        ],
        Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                     name: "del".to_string(),
                                                     args: vec!["123".to_string().into(), "4".to_string().into()]})))]
    #[case::to_code(vec![RuntimeValue::String("test1".to_string())],
        vec![
              ast_call("to_code", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("elm".into()))),
              ]),
        ],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Code(mq_markdown::Code{lang: Some("elm".to_string()), value: "test1".to_string(), fence: true, meta: None, position: None}), None)]))]
    #[case::to_code(vec![RuntimeValue::String("test1".to_string())],
        vec![
              ast_call("to_code", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("elm".into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::None)),
              ]),
        ],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Code(mq_markdown::Code{lang: None, value: "elm".to_string(), fence: true, meta: None, position: None}), None)]))]
    #[case::md_h1(vec![RuntimeValue::String("Heading 1".to_string())],
        vec![
              ast_call("to_h", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
              ]),
        ],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Heading(mq_markdown::Heading{depth: 1, values: vec!["Heading 1".to_string().into()], position: None}), None)]))]
    #[case::md_h2(vec![RuntimeValue::String("Heading 2".to_string())],
        vec![
              ast_call("to_h", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(2.into()))),
              ]),
        ],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Heading(mq_markdown::Heading{depth: 2, values: vec!["Heading 2".to_string().into()], position: None}), None)]))]
    #[case::md_h3(vec![RuntimeValue::String("Heading 3".to_string())],
        vec![
              ast_call("to_h", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(3.into()))),
              ]),
        ],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Heading(mq_markdown::Heading{depth: 3, values: vec!["Heading 3".to_string().into()], position: None}), None)]))]
    #[case::md_h3(vec![RuntimeValue::String("Heading 3".to_string())],
        vec![
              ast_call("to_h", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("3".into()))),
              ]),
        ],
        Ok(vec![RuntimeValue::NONE]))]
    #[case::md_h(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "Heading".to_string(), position: None}), None)],
        vec![
              ast_call("to_h", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::Number(2.into()))),
              ]),
        ],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Heading(mq_markdown::Heading{depth: 2, values: vec!["Heading".to_string().into()], position: None}), None)]))]
    #[case::to_math(vec![RuntimeValue::String("E=mc^2".to_string())],
        vec![
              ast_call("to_math", SmallVec::new()),
        ],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Math(mq_markdown::Math{value: "E=mc^2".to_string(), position: None}), None)]))]
    #[case::to_math_inline(vec![RuntimeValue::String("E=mc^2".to_string())],
        vec![
              ast_call("to_math_inline", SmallVec::new()),
        ],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::MathInline(mq_markdown::MathInline{value: "E=mc^2".into(), position: None}), None)]))]
    #[case::to_md_text(vec![RuntimeValue::String("This is a text".to_string())],
        vec![
              ast_call("to_md_text", SmallVec::new()),
        ],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "This is a text".to_string(), position: None}), None)]))]
    #[case::to_strong(vec![RuntimeValue::String("Bold text".to_string())],
        vec![
              ast_call("to_strong", SmallVec::new()),
        ],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Strong(mq_markdown::Strong{values: vec!["Bold text".to_string().into()], position: None}), None)]))]
    #[case::to_strong(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "Bold text".to_string(), position: None}), None)],
        vec![
              ast_call("to_strong", SmallVec::new()),
        ],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Strong(mq_markdown::Strong{values: vec![mq_markdown::Node::Text(mq_markdown::Text{value: "Bold text".to_string(), position: None})], position: None}), None)]))]
    #[case::to_em(vec![RuntimeValue::String("Italic text".to_string())],
        vec![
              ast_call("to_em", SmallVec::new()),
        ],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Emphasis(mq_markdown::Emphasis{values: vec!["Italic text".to_string().into()], position: None}), None)]))]
    #[case::to_em(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "Italic text".to_string(), position: None}), None)],
        vec![
              ast_call("to_em", SmallVec::new()),
        ],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Emphasis(mq_markdown::Emphasis{values: vec![mq_markdown::Node::Text(mq_markdown::Text{value: "Italic text".to_string(), position: None})], position: None}), None)]))]
    #[case::to_image(vec![RuntimeValue::String("Image Alt".to_string())],
        vec![
              ast_call("to_image", smallvec![
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
              ast_call("to_link", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("https://example.com".to_string()))),
                    ast_node(ast::Expr::Literal(ast::Literal::String("Link Value".to_string()))),
                    ast_node(ast::Expr::Literal(ast::Literal::String("Link Title".to_string()))),
              ]),
        ],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Link(mq_markdown::Link{
            url: mq_markdown::Url::new("https://example.com".to_string()),
            title: Some(mq_markdown::Title::new("Link Title".to_string())),
            values: vec!["Link Value".to_string().into()],
            position: None
        }), None)]))]
    #[case::to_link(vec![RuntimeValue::Number(123.into())],
        vec![
              ast_call("to_link", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("Link Title".to_string()))),
                    ast_node(ast::Expr::Literal(ast::Literal::String("Link Value".to_string()))),
              ]),
        ],
        Ok(vec![RuntimeValue::NONE]))]
    #[case::to_hr(vec![RuntimeValue::String("".to_owned())],
        vec![
              ast_call("to_hr", SmallVec::new()),
        ],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::HorizontalRule(mq_markdown::HorizontalRule{position: None}), None)]))]
    #[case::to_md_list(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "list".to_string(), position: None}), None)],
        vec![
              ast_call("to_md_list",
                       smallvec![
                             ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                       ]),
        ],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::List(
            mq_markdown::List{values: vec!["list".to_string().into()], ordered: false, index: 0, level: 1_u8, checked: None, position: None}), None)]))]
    #[case::to_md_list(vec![RuntimeValue::String("list".to_string())],
        vec![
              ast_call("to_md_list",
                       smallvec![
                             ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                       ]),
        ],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::List(
            mq_markdown::List{values: vec!["list".to_string().into()], ordered: false, index: 0, level: 1_u8, checked: None, position: None}), None)]))]
    #[case::set_check(vec![RuntimeValue::Markdown(mq_markdown::Node::List(mq_markdown::List{values: vec!["Checked Item".to_string().into()], ordered: false, level: 0, index: 0, checked: None, position: None}), None)],
        vec![
              ast_call("set_check", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(true))),
              ]),
        ],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::List(mq_markdown::List{values: vec!["Checked Item".to_string().into()], ordered: false, level: 0, index: 0, checked: Some(true), position: None}), None)]))]
    #[case::set_check(vec![RuntimeValue::Markdown(mq_markdown::Node::List(mq_markdown::List{values: vec!["Unchecked Item".to_string().into()], ordered: false, level: 0, index: 0, checked: None, position: None}), None)],
        vec![
              ast_call("set_check", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(false))),
              ]),
        ],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::List(mq_markdown::List{values: vec!["Unchecked Item".to_string().into()], ordered: false, level: 0, index: 0, checked: Some(false), position: None}), None)]))]
    #[case::compact(vec![RuntimeValue::Array(vec![
            RuntimeValue::String("test1".to_string()),
            RuntimeValue::NONE,
            RuntimeValue::String("test2".to_string()),
            RuntimeValue::NONE,
            RuntimeValue::String("test3".to_string()),
        ])],
        vec![
            ast_call("compact", SmallVec::new())
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
            ast_call("compact", SmallVec::new())
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
            ast_call("compact", SmallVec::new())
        ],
        Ok(vec![RuntimeValue::Array(Vec::new())]))]
    #[case::compact_no_none(vec![RuntimeValue::Array(vec![
            RuntimeValue::String("test1".to_string()),
            RuntimeValue::String("test2".to_string()),
        ])],
        vec![
            ast_call("compact", SmallVec::new())
        ],
        Ok(vec![RuntimeValue::Array(vec![
            RuntimeValue::String("test1".to_string()),
            RuntimeValue::String("test2".to_string()),
        ])]))]
    #[case::compact_error(vec!["test".to_string().into()],
        vec![
            ast_call("compact", SmallVec::new())
        ],
        Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                     name: "compact".to_string(),
                                                     args: vec!["test".to_string().into()]})))]
    #[case::text_selector(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "test".to_string(), position: None}), None)],
        vec![
            ast_node(ast::Expr::Selector(ast::Selector::Text)),
        ],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "test".to_string(), position: None}), None)]))]
    #[case::text_selector_heading(vec![RuntimeValue::Markdown(mq_markdown::Node::Heading(mq_markdown::Heading{depth: 1, values: vec!["Heading 1".to_string().into()], position: None}), None)],
        vec![
            ast_node(ast::Expr::Selector(ast::Selector::Text)),
        ],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Fragment(mq_markdown::Fragment { values: vec!["Heading 1".to_string().into()] }), None)]))]
    #[case::to_md_table_row(vec![RuntimeValue::Array(vec![
            RuntimeValue::String("Cell 1".to_string()),
            RuntimeValue::String("Cell 2".to_string()),
            RuntimeValue::String("Cell 3".to_string()),
        ])],
        vec![
            ast_call("to_md_table_row", SmallVec::new())
        ],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::TableRow(mq_markdown::TableRow{
            values: vec![
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
            ast_call("to_md_table_row", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::String("Cell 1".to_string()))),
                ast_node(ast::Expr::Literal(ast::Literal::String("Cell 2".to_string()))),
            ])
        ],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::TableRow(mq_markdown::TableRow{
            values: vec![
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
    #[case::get_title(vec![RuntimeValue::Markdown(mq_markdown::Node::Link(mq_markdown::Link{url: mq_markdown::Url::new("https://example.com".to_string()), title: Some(mq_markdown::Title::new("title".to_string())), values: vec!["Link".to_string().into()], position: None}), None)],
        vec![
             ast_call("get_title", SmallVec::new())
        ],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "title".to_string(), position: None}), None)]))]
    #[case::get_title(vec![RuntimeValue::Markdown(mq_markdown::Node::Link(mq_markdown::Link{url: mq_markdown::Url::new("https://example.com".to_string()), title: None, values: vec!["Link".to_string().into()], position: None}), None)],
        vec![
             ast_call("get_title", SmallVec::new())
        ],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Empty, None)]))]
    #[case::get_title(vec![RuntimeValue::Markdown(mq_markdown::Node::Image(mq_markdown::Image{url: "https://example.com/image.png".to_string(), alt: "Image Alt".to_string(), title: Some("Image Title".to_string()), position: None}), None)],
            vec![
                 ast_call("get_title", SmallVec::new())
            ],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "Image Title".to_string(), position: None}), None)]))]
    #[case::get_title(vec![RuntimeValue::Markdown(mq_markdown::Node::Image(mq_markdown::Image{url: "https://example.com/image.png".to_string(), alt: "Image Alt".to_string(), title: None, position: None}), None)],
        vec![
             ast_call("get_title", SmallVec::new())
        ],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Empty, None)]))]
    #[case::get_string(vec![RuntimeValue::String("test1".to_string())],
        vec![
            ast_call("get", smallvec![ast_node(ast::Expr::Literal(ast::Literal::Number(0.into())))])
        ],
        Ok(vec![RuntimeValue::String("t".to_string())]))]
    #[case::get_string(vec![RuntimeValue::String("test1".to_string())],
        vec![
            ast_call("get", smallvec![ast_node(ast::Expr::Literal(ast::Literal::Number(5.into())))])
        ],
        Ok(vec![RuntimeValue::NONE]))]
    #[case::get_array(vec![RuntimeValue::Array(vec!["test1".to_string().into()])],
        vec![
            ast_call("get", smallvec![ast_node(ast::Expr::Literal(ast::Literal::Number(2.into())))])
        ],
        Ok(vec![RuntimeValue::NONE]))]
    #[case::get(vec![RuntimeValue::TRUE],
        vec![
            ast_call("get", smallvec![ast_node(ast::Expr::Literal(ast::Literal::Number(0.into())))])
        ],
        Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                     name: "get".to_string(),
                                                     args: vec![true.to_string().into(), 0.to_string().into()]})))]
    #[case::to_date(vec![RuntimeValue::Number(1609459200000_i64.into())],
        vec![
            ast_call("to_date", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::String("%Y-%m-%d".to_string())))
            ])
        ],
        Ok(vec![RuntimeValue::String("2021-01-01".to_string())]))]
    #[case::to_date(vec![RuntimeValue::Number(1609459200000_i64.into())],
        vec![
            ast_call("to_date", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::String("%Y/%m/%d %H:%M:%S".to_string())))
            ])
        ],
        Ok(vec![RuntimeValue::String("2021/01/01 00:00:00".to_string())]))]
    #[case::to_date(vec![RuntimeValue::Number(1609488000000_i64.into())],
        vec![
            ast_call("to_date", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::String("%d %b %Y %H:%M".to_string())))
            ])
        ],
        Ok(vec![RuntimeValue::String("01 Jan 2021 08:00".to_string())]))]
    #[case::to_date(vec![RuntimeValue::String("test".to_string())],
        vec![
            ast_call("to_date", smallvec![
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
            ast_call("to_string", SmallVec::new())
        ],
        Ok(vec![RuntimeValue::String(r#"["test", 1, 2, false]"#.to_string())]))]
    #[case::to_string_empty_array(vec![RuntimeValue::Array(Vec::new())],
        vec![
            ast_call("to_string", SmallVec::new())
        ],
        Ok(vec![RuntimeValue::String("[]".to_string())]))]
    #[case::to_text(vec![RuntimeValue::String("test".to_string())],
        vec![
             ast_call("to_text", SmallVec::new())
        ],
        Ok(vec!["test".to_string().into()]))]
    #[case::to_text(vec![RuntimeValue::Number(42.into())],
        vec![
             ast_call("to_text", SmallVec::new())
        ],
        Ok(vec!["42".to_string().into()]))]
    #[case::to_text(vec![RuntimeValue::Bool(true)],
        vec![
             ast_call("to_text", SmallVec::new())
        ],
        Ok(vec!["true".to_string().into()]))]
    #[case::to_text(vec![RuntimeValue::Markdown(mq_markdown::Node::Heading(mq_markdown::Heading{depth: 1, values: vec!["Heading".to_string().into()], position: None}), None)],
        vec![
             ast_call("to_text", SmallVec::new())
        ],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "Heading".to_string(), position: None}), None)]))]
    #[case::to_text(vec![RuntimeValue::String("Original".to_string())],
        vec![
             ast_call("to_text",
              smallvec![ast_node(ast::Expr::Literal(ast::Literal::String("Override".to_string())))])
        ],
        Ok(vec!["Override".to_string().into()]))]
    #[case::to_text(vec![RuntimeValue::Array(vec!["val1".to_string().into(), "val2".to_string().into()])],
        vec![
             ast_call("to_text", SmallVec::new())
        ],
        Ok(vec!["val1,val2".to_string().into()]))]
    #[case::url_encode(vec![RuntimeValue::String("test string with spaces".to_string())],
        vec![
             ast_call("url_encode", SmallVec::new())
        ],
        Ok(vec![RuntimeValue::String("test%20string%20with%20spaces".to_string())]))]
    #[case::url_encode(vec![RuntimeValue::String("test!@#$%^&*()".to_string())],
        vec![
             ast_call("url_encode", SmallVec::new())
        ],
        Ok(vec![RuntimeValue::String("test%21%40%23%24%25%5E%26%2A%28%29".to_string())]))]
    #[case::url_encode(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "test string".to_string(), position: None}), None)],
        vec![
             ast_call("url_encode", SmallVec::new())
        ],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "test%20string".to_string(), position: None}), None)]))]
    #[case::url_encode(vec![RuntimeValue::Number(1.into())],
        vec![
             ast_call("url_encode", SmallVec::new())
        ],
        Ok(vec![RuntimeValue::String("1".to_string())]))]
    #[case::update(vec!["".to_string().into()],
        vec![
             ast_call("update", smallvec![
              ast_node(ast::Expr::Literal(ast::Literal::Number(0.into()))),
              ast_node(ast::Expr::Literal(ast::Literal::String("updated".to_string()))),
             ])
        ],
        Ok(vec![RuntimeValue::String("updated".to_string())]))]
    #[case::update(vec!["".to_string().into()],
        vec![
             ast_call("update", smallvec![
                ast_call("to_strong", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("text1".to_string()))),
                ]),
                ast_call("to_strong", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("text2".to_string()))),
                ])
             ])
        ],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Strong(mq_markdown::Strong{values: vec![mq_markdown::Node::Text(mq_markdown::Text{value: "text2".to_string(), position: None})], position: None}), None)]))]
    #[case::update(vec!["".to_string().into()],
        vec![
             ast_call("update", smallvec![
                ast_call("to_strong", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("text1".to_string()))),
                ]),
                ast_node(ast::Expr::Literal(ast::Literal::String("text2".to_string()))),
             ])
        ],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Strong(mq_markdown::Strong{values: vec![mq_markdown::Node::Text(mq_markdown::Text{value: "text2".to_string(), position: None})], position: None}), None)]))]
    #[case::sort_string_array(vec![RuntimeValue::Array(vec![
            RuntimeValue::String("c".to_string()),
            RuntimeValue::String("a".to_string()),
            RuntimeValue::String("b".to_string()),
        ])],
        vec![
            ast_call("sort", SmallVec::new())
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
            ast_call("sort", SmallVec::new())
        ],
        Ok(vec![RuntimeValue::Array(vec![
            RuntimeValue::Number(1.into()),
            RuntimeValue::Number(2.into()),
            RuntimeValue::Number(3.into()),
        ])]))]
    #[case::sort_empty_array(vec![RuntimeValue::Array(Vec::new())],
        vec![
            ast_call("sort", SmallVec::new())
        ],
        Ok(vec![RuntimeValue::Array(Vec::new())]))]
    #[case::sort_error(vec![RuntimeValue::Number(1.into())],
        vec![
            ast_call("sort", SmallVec::new())
        ],
        Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                     name: "sort".to_string(),
                                                     args: vec![1.to_string().into()]})))]
    #[case::uniq_string_array(vec![RuntimeValue::Array(vec![
            RuntimeValue::String("a".to_string()),
            RuntimeValue::String("b".to_string()),
            RuntimeValue::String("a".to_string()),
            RuntimeValue::String("c".to_string()),
            RuntimeValue::String("b".to_string()),
        ])],
        vec![
            ast_call("uniq", SmallVec::new())
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
            ast_call("uniq", SmallVec::new())
        ],
        Ok(vec![RuntimeValue::Array(vec![
            RuntimeValue::Number(1.into()),
            RuntimeValue::Number(2.into()),
            RuntimeValue::Number(3.into()),
        ])]))]
    #[case::uniq_error(vec![RuntimeValue::Number(1.into())],
        vec![
            ast_call("uniq", SmallVec::new())
        ],
        Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                     name: "uniq".to_string(),
                                                     args: vec![1.to_string().into()]})))]
    #[case::to_html(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "test".to_string(), position: None}), None)],
        vec![
             ast_call("to_html", SmallVec::new())
        ],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "<p>test</p>".to_string(), position: None}), None)]))]
    #[case::to_html(vec![RuntimeValue::Markdown(mq_markdown::Node::Heading(mq_markdown::Heading{depth: 1, values: vec!["Heading 1".to_string().into()], position: None}), None)],
        vec![
             ast_call("to_html", SmallVec::new())
        ],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "<h1>Heading 1</h1>".to_string(), position: None}), None)]))]
    #[case::to_html(vec![RuntimeValue::Markdown(mq_markdown::Node::Strong(mq_markdown::Strong{values: vec!["Bold".to_string().into()], position: None}), None)],
        vec![
             ast_call("to_html", SmallVec::new())
        ],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "<p><strong>Bold</strong></p>".to_string(), position: None}), None)]))]
    #[case::to_html(vec![RuntimeValue::Markdown(mq_markdown::Node::Emphasis(mq_markdown::Emphasis{values: vec!["Italic".to_string().into()], position: None}), None)],
        vec![
             ast_call("to_html", SmallVec::new())
        ],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "<p><em>Italic</em></p>".to_string(), position: None}), None)]))]
    #[case::to_html(vec![RuntimeValue::Markdown(mq_markdown::Node::Link(mq_markdown::Link{url: mq_markdown::Url::new("https://example.com".to_string()), title: Some(mq_markdown::Title::new("Link Title".to_string())), values: vec!["Link Title".to_string().into()], position: None}), None)],
        vec![
             ast_call("to_html", SmallVec::new())
        ],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "<p><a href=\"https://example.com\" title=\"Link Title\">Link Title</a></p>".to_string(), position: None}), None)]))]
    #[case::to_html(vec![RuntimeValue::Markdown(mq_markdown::Node::Code(mq_markdown::Code{lang: Some("rust".to_string()), value: "println!(\"Hello\");".to_string(), fence: true, meta: None, position: None}), None)],
        vec![
             ast_call("to_html", SmallVec::new())
        ],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "<pre><code class=\"language-rust\">println!(&quot;Hello&quot;);\n</code></pre>".to_string(), position: None}), None)]))]
    #[case::to_html(vec![RuntimeValue::String("Plain text".to_string())],
        vec![
             ast_call("to_html", SmallVec::new())
        ],
        Ok(vec!["<p>Plain text</p>".to_string().into()]))]
    #[case::to_html(vec![RuntimeValue::Number(1.into())],
        vec![
             ast_call("to_html", SmallVec::new())
        ],
        Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                     name: "to_html".to_string(),
                                                     args: vec![1.to_string().into()]})))]
    #[case::repeat_string(vec![RuntimeValue::String("abc".to_string())],
        vec![
            ast_call("repeat", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::Number(3.into())))
            ])
        ],
        Ok(vec![RuntimeValue::String("abcabcabc".to_string())]))]
    #[case::repeat_markdown(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "abc".to_string(), position: None}), None)],
        vec![
            ast_call("repeat", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::Number(3.into())))
            ])
        ],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "abcabcabc".to_string(), position: None}), None)]))]
    #[case::repeat_string_zero(vec![RuntimeValue::String("abc".to_string())],
        vec![
            ast_call("repeat", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::Number(0.into())))
            ])
        ],
        Ok(vec![RuntimeValue::String("".to_string())]))]
    #[case::repeat_array(vec![RuntimeValue::Array(vec![
            RuntimeValue::String("a".to_string()),
            RuntimeValue::String("b".to_string()),
        ])],
        vec![
            ast_call("repeat", smallvec![
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
            ast_call("repeat", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::Number(0.into())))
            ])
        ],
        Ok(vec![RuntimeValue::Array(Vec::new())]))]
    #[case::repeat_invalid_count(vec![RuntimeValue::String("abc".to_string())],
        vec![
            ast_call("repeat", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::Number((-1).into())))
            ])
        ],
        Ok(vec!["".to_string().into()]))]
    #[case::repeat_invalid_type(vec![RuntimeValue::Number(42.into())],
        vec![
            ast_call("repeat", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::Number(3.into())))
            ])
        ],
        Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                     name: "repeat".to_string(),
                                                     args: vec![42.to_string().into(), 3.to_string().into()]})))]
    #[case::debug(vec![RuntimeValue::String("test".to_string())],
        vec![
            ast_call("stderr", SmallVec::new())
        ],
        Ok(vec![RuntimeValue::String("test".to_string())]))]
    #[case::from_date(vec![RuntimeValue::String("2025-03-15T20:00:00+09:00".to_string())],
        vec![
            ast_call("from_date", SmallVec::new())
        ],
        Ok(vec![RuntimeValue::Number(1742036400000_i64.into())]))]
    #[case::from_date_invalid_format(vec![RuntimeValue::String("2021-01-01".to_string())],
        vec![
            ast_call("from_date", SmallVec::new())
        ],
        Err(InnerError::Eval(EvalError::RuntimeError(Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()}, "premature end of input".to_string()))))]
    #[case::from_date(vec![RuntimeValue::Number(1.into())],
        vec![
            ast_call("from_date", SmallVec::new())
        ],
        Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                     name: "from_date".to_string(),
                                                     args: vec![1.to_string().into()]})))]
    #[case::to_code_inline(vec![RuntimeValue::String("test1".to_string())],
        vec![
              ast_call("to_code_inline", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("elm".into()))),
              ]),
        ],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::CodeInline(mq_markdown::CodeInline{value: "elm".into(), position: None}), None)]))]
    #[case::to_md_name(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "name".to_string(), position: None}), None)],
        vec![
              ast_call("to_md_name", SmallVec::new()),
        ],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "text".to_string(), position: None}), None)]))]
    #[case::to_md_name(vec![RuntimeValue::Number(123.into())],
        vec![
              ast_call("to_md_name", SmallVec::new()),
        ],
        Ok(vec![RuntimeValue::NONE]))]
    #[case::set_ref_markdown_definition(vec![RuntimeValue::Markdown(mq_markdown::Node::Definition(mq_markdown::Definition{ident: "ident".into(), url: mq_markdown::Url::new("url".to_string()), title: None, label: None, position: None}), None)],
            vec![
                 ast_call("set_ref", smallvec![
                     ast_node(ast::Expr::Literal(ast::Literal::String("definition-ref".to_string())))
                 ])
            ],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Definition(mq_markdown::Definition{
                ident: "ident".to_string(),
                label: Some("definition-ref".to_string()),
                url: mq_markdown::Url::new("url".to_string()),
                title: None,
                position: None
            }), None)]))]
    #[case::set_ref_markdown_link_ref(vec![RuntimeValue::Markdown(mq_markdown::Node::LinkRef(mq_markdown::LinkRef{ident: "ident".into(), label: None, values: Vec::new(), position: None}), None)],
            vec![
                 ast_call("set_ref", smallvec![
                     ast_node(ast::Expr::Literal(ast::Literal::String("link-ref".to_string())))
                 ])
            ],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::LinkRef(mq_markdown::LinkRef{
                ident: "ident".to_string(),
                label: Some("link-ref".to_string()),
                values: Vec::new(),
                position: None
            }), None)]))]
    #[case::set_ref_markdown_link_ref(vec![RuntimeValue::Markdown(mq_markdown::Node::LinkRef(mq_markdown::LinkRef{ident: "ident".into(), label: None, values: Vec::new(), position: None}), None)],
            vec![
                 ast_call("set_ref", smallvec![
                     ast_node(ast::Expr::Literal(ast::Literal::String("ident".to_string())))
                 ])
            ],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::LinkRef(mq_markdown::LinkRef{
                ident: "ident".to_string(),
                label: None,
                values: Vec::new(),
                position: None
            }), None)]))]
    #[case::set_ref_markdown_image_ref(vec![RuntimeValue::Markdown(mq_markdown::Node::ImageRef(mq_markdown::ImageRef{alt: "Image Alt".to_string(), ident: "ident".into(), label: None, position: None}), None)],
            vec![
                 ast_call("set_ref", smallvec![
                     ast_node(ast::Expr::Literal(ast::Literal::String("image-ref".to_string())))
                 ])
            ],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::ImageRef(mq_markdown::ImageRef{
                ident: "ident".to_string(),
                alt: "Image Alt".to_string(),
                label: Some("image-ref".to_string()),
                position: None
            }), None)]))]
    #[case::set_ref_markdown_image_ref(vec![RuntimeValue::Markdown(mq_markdown::Node::ImageRef(mq_markdown::ImageRef{alt: "Image Alt".to_string(), ident: "ident".into(), label: None, position: None}), None)],
            vec![
                 ast_call("set_ref", smallvec![
                     ast_node(ast::Expr::Literal(ast::Literal::String("ident".to_string())))
                 ])
            ],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::ImageRef(mq_markdown::ImageRef{
                ident: "ident".to_string(),
                alt: "Image Alt".to_string(),
                label: None,
                position: None
            }), None)]))]
    #[case::set_ref_markdown_footnote_ref(vec![RuntimeValue::Markdown(mq_markdown::Node::FootnoteRef(mq_markdown::FootnoteRef{ident: "ident".into(), label: None, position: None}), None)],
            vec![
                 ast_call("set_ref", smallvec![
                     ast_node(ast::Expr::Literal(ast::Literal::String("footnote-ref".to_string())))
                 ])
            ],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::FootnoteRef(mq_markdown::FootnoteRef{
                ident: "ident".to_string(),
                label: Some("footnote-ref".to_string()),
                position: None
            }), None)]))]
    #[case::set_ref_markdown_footnote(vec![RuntimeValue::Markdown(mq_markdown::Node::Footnote(mq_markdown::Footnote{ident: "ident".into(), values: Vec::new(), position: None}), None)],
            vec![
                 ast_call("set_ref", smallvec![
                     ast_node(ast::Expr::Literal(ast::Literal::String("footnote".to_string())))
                 ])
            ],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Footnote(mq_markdown::Footnote{
               ident: "footnote".to_string(),
                values: Vec::new(),
                position: None
            }), None)]))]
    #[case::set_ref_not_link_or_image(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "Simple text".to_string(), position: None}), None)],
            vec![
                 ast_call("set_ref", smallvec![
                     ast_node(ast::Expr::Literal(ast::Literal::String("text-ref".to_string())))
                 ])
            ],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "Simple text".to_string(), position: None}), None)]))]
    #[case::set_ref_plain_string(vec![RuntimeValue::String("Not a markdown".to_string())],
            vec![
                 ast_call("set_ref", smallvec![
                     ast_node(ast::Expr::Literal(ast::Literal::String("string-ref".to_string())))
                 ])
            ],
            Ok(vec![RuntimeValue::String("Not a markdown".to_string())]))]
    #[case::set_ref_none(vec![RuntimeValue::NONE],
            vec![
                 ast_call("set_ref", smallvec![
                     ast_node(ast::Expr::Literal(ast::Literal::String("none-ref".to_string())))
                 ])
            ],
            Ok(vec![RuntimeValue::NONE]))]
    #[case::set_ref_with_empty_id(vec![RuntimeValue::Markdown(mq_markdown::Node::Link(mq_markdown::Link{url: mq_markdown::Url::new("https://example.com".to_string()), title: Some(mq_markdown::Title::new("title".to_string())), values: vec!["Link".to_string().into()], position: None}), None)],
        vec![
             ast_call("set_ref", smallvec![
                 ast_node(ast::Expr::Literal(ast::Literal::String("".to_string())))
             ])
        ],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Link(mq_markdown::Link{url: mq_markdown::Url::new("https://example.com".to_string()), title: Some(mq_markdown::Title::new("title".to_string())), values: vec!["Link".to_string().into()], position: None}), None)]))]
    #[case::get_url_link(vec![RuntimeValue::Markdown(mq_markdown::Node::Definition(mq_markdown::Definition{url: mq_markdown::Url::new("https://example.com".to_string()), ident: "ident".to_string(), label: None, title: Some(mq_markdown::Title::new("title".to_string())), position: None}), None)],
        vec![
             ast_call("get_url", SmallVec::new())
        ],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "https://example.com".to_string(), position: None}), None)]))]
    #[case::get_url_link(vec![RuntimeValue::Markdown(mq_markdown::Node::Link(mq_markdown::Link{url: mq_markdown::Url::new("https://example.com".to_string()), title: Some(mq_markdown::Title::new("title".to_string())), values: vec!["Link".to_string().into()], position: None}), None)],
        vec![
             ast_call("get_url", SmallVec::new())
        ],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "https://example.com".to_string(), position: None}), None)]))]
    #[case::get_url_image(vec![RuntimeValue::Markdown(mq_markdown::Node::Image(mq_markdown::Image{url: "https://example.com/image.png".to_string(), alt: "Image Alt".to_string(), title: Some("Image Title".to_string()), position: None}), None)],
        vec![
             ast_call("get_url", SmallVec::new())
        ],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "https://example.com/image.png".to_string(), position: None}), None)]))]
    #[case::get_url_not_link_or_image(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "Simple text".to_string(), position: None}), None)],
        vec![
             ast_call("get_url", SmallVec::new())
        ],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Empty, None)]))]
    #[case::get_url_string(vec![RuntimeValue::String("Not a markdown".to_string())],
        vec![
             ast_call("get_url", SmallVec::new())
        ],
        Ok(vec![RuntimeValue::NONE]))]
    #[case::flatten_array_of_arrays(vec![RuntimeValue::Array(vec![
                RuntimeValue::Array(vec![RuntimeValue::String("a".to_string()), RuntimeValue::String("b".to_string())]),
                RuntimeValue::Array(vec![RuntimeValue::String("c".to_string()), RuntimeValue::String("d".to_string())])
            ])],
            vec![
                ast_call("flatten", SmallVec::new())
            ],
            Ok(vec![RuntimeValue::Array(vec![
                RuntimeValue::String("a".to_string()),
                RuntimeValue::String("b".to_string()),
                RuntimeValue::String("c".to_string()),
                RuntimeValue::String("d".to_string())
            ])]))]
    #[case::flatten_array_with_nested_arrays(vec![RuntimeValue::Array(vec![
                RuntimeValue::String("a".to_string()),
                RuntimeValue::Array(vec![RuntimeValue::String("b".to_string()), RuntimeValue::String("c".to_string())]),
                RuntimeValue::String("d".to_string())
            ])],
            vec![
                ast_call("flatten", SmallVec::new())
            ],
            Ok(vec![RuntimeValue::Array(vec![
                RuntimeValue::String("a".to_string()),
                RuntimeValue::String("b".to_string()),
                RuntimeValue::String("c".to_string()),
                RuntimeValue::String("d".to_string())
            ])]))]
    #[case::flatten_deeply_nested_arrays(vec![RuntimeValue::Array(vec![
                RuntimeValue::Array(vec![
                    RuntimeValue::Array(vec![RuntimeValue::String("a".to_string()), RuntimeValue::String("b".to_string())]),
                    RuntimeValue::String("c".to_string())
                ]),
                RuntimeValue::String("d".to_string())
            ])],
            vec![
                ast_call("flatten", SmallVec::new())
            ],
            Ok(vec![RuntimeValue::Array(vec![
                RuntimeValue::String("a".to_string()),
                RuntimeValue::String("b".to_string()),
                RuntimeValue::String("c".to_string()),
                RuntimeValue::String("d".to_string())
            ])]))]
    #[case::flatten_empty_array(vec![RuntimeValue::Array(Vec::new())],
            vec![
                ast_call("flatten", SmallVec::new())
            ],
            Ok(vec![RuntimeValue::Array(Vec::new())]))]
    #[case::flatten_array_with_empty_arrays(vec![RuntimeValue::Array(vec![
                RuntimeValue::Array(Vec::new()),
                RuntimeValue::Array(Vec::new())
            ])],
            vec![
                ast_call("flatten", SmallVec::new())
            ],
            Ok(vec![RuntimeValue::Array(Vec::new())]))]
    #[case::flatten_mixed_type_arrays(vec![RuntimeValue::Array(vec![
                RuntimeValue::Array(vec![RuntimeValue::String("a".to_string()), RuntimeValue::Number(1.into())]),
                RuntimeValue::Array(vec![RuntimeValue::Bool(true), RuntimeValue::String("b".to_string())])
            ])],
            vec![
                ast_call("flatten", SmallVec::new())
            ],
            Ok(vec![RuntimeValue::Array(vec![
                RuntimeValue::String("a".to_string()),
                RuntimeValue::Number(1.into()),
                RuntimeValue::Bool(true),
                RuntimeValue::String("b".to_string())
            ])]))]
    #[case::flatten_array_with_none_values(vec![RuntimeValue::Array(vec![
                RuntimeValue::Array(vec![RuntimeValue::String("a".to_string()), RuntimeValue::NONE]),
                RuntimeValue::Array(vec![RuntimeValue::String("b".to_string()), RuntimeValue::String("c".to_string())])
            ])],
            vec![
                ast_call("flatten", SmallVec::new())
            ],
            Ok(vec![RuntimeValue::Array(vec![
                RuntimeValue::String("a".to_string()),
                RuntimeValue::NONE,
                RuntimeValue::String("b".to_string()),
                RuntimeValue::String("c".to_string())
            ])]))]
    #[case::flatten_non_array(vec![RuntimeValue::String("test".to_string())],
            vec![
                ast_call("flatten", SmallVec::new())
            ],
            Ok(vec![RuntimeValue::String("test".to_string())]))]
    #[case::flatten_none(vec![RuntimeValue::NONE],
            vec![
                ast_call("flatten", SmallVec::new())
            ],
            Ok(vec![RuntimeValue::NONE]))]
    #[case::set_array_valid_index(vec![RuntimeValue::Array(vec![
        RuntimeValue::String("item1".to_string()),
        RuntimeValue::String("item2".to_string()),
        RuntimeValue::String("item3".to_string()),
        ])],
        vec![
        ast_call("set", smallvec![
            ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
            ast_node(ast::Expr::Literal(ast::Literal::String("updated".to_string()))),
        ])
        ],
        Ok(vec![RuntimeValue::Array(vec![
        RuntimeValue::String("item1".to_string()),
        RuntimeValue::String("updated".to_string()),
        RuntimeValue::String("item3".to_string()),
        ])]))]
    #[case::set_array_first_index(vec![RuntimeValue::Array(vec![
        RuntimeValue::String("item1".to_string()),
        RuntimeValue::String("item2".to_string()),
        RuntimeValue::String("item3".to_string()),
        ])],
        vec![
        ast_call("set", smallvec![
            ast_node(ast::Expr::Literal(ast::Literal::Number(0.into()))),
            ast_node(ast::Expr::Literal(ast::Literal::String("first".to_string()))),
        ])
        ],
        Ok(vec![RuntimeValue::Array(vec![
        RuntimeValue::String("first".to_string()),
        RuntimeValue::String("item2".to_string()),
        RuntimeValue::String("item3".to_string()),
        ])]))]
    #[case::set_array_last_index(vec![RuntimeValue::Array(vec![
        RuntimeValue::String("item1".to_string()),
        RuntimeValue::String("item2".to_string()),
        RuntimeValue::String("item3".to_string()),
        ])],
        vec![
        ast_call("set", smallvec![
            ast_node(ast::Expr::Literal(ast::Literal::Number(2.into()))),
            ast_node(ast::Expr::Literal(ast::Literal::String("last".to_string()))),
        ])
        ],
        Ok(vec![RuntimeValue::Array(vec![
        RuntimeValue::String("item1".to_string()),
        RuntimeValue::String("item2".to_string()),
        RuntimeValue::String("last".to_string()),
        ])]))]
    #[case::set_array_out_of_bounds_positive(vec![RuntimeValue::Array(vec![
        RuntimeValue::String("item1".to_string()),
        RuntimeValue::String("item2".to_string()),
        ])],
        vec![
        ast_call("set", smallvec![
            ast_node(ast::Expr::Literal(ast::Literal::Number(5.into()))),
            ast_node(ast::Expr::Literal(ast::Literal::String("new".to_string()))),
        ])
        ],
        Ok(vec![RuntimeValue::Array(vec![
        RuntimeValue::String("item1".to_string()),
        RuntimeValue::String("item2".to_string()),
        RuntimeValue::NONE,
        RuntimeValue::NONE,
        RuntimeValue::NONE,
        RuntimeValue::String("new".to_string()),
        ])]))]
    #[case::set_array_negative_index(vec![RuntimeValue::Array(vec![
        RuntimeValue::String("item1".to_string()),
        RuntimeValue::String("item2".to_string()),
        RuntimeValue::String("item3".to_string()),
        ])],
        vec![
        ast_call("set", smallvec![
            ast_node(ast::Expr::Literal(ast::Literal::Number((-1).into()))),
            ast_node(ast::Expr::Literal(ast::Literal::String("negative".to_string()))),
        ])
        ],
        Ok(vec![RuntimeValue::Array(vec![
        RuntimeValue::String("negative".to_string()),
        RuntimeValue::String("item2".to_string()),
        RuntimeValue::String("item3".to_string()),
        ])]))]
    #[case::set_array_empty(vec![RuntimeValue::Array(Vec::new())],
        vec![
        ast_call("set", smallvec![
            ast_node(ast::Expr::Literal(ast::Literal::Number(0.into()))),
            ast_node(ast::Expr::Literal(ast::Literal::String("value".to_string()))),
        ])
        ],
        Ok(vec![RuntimeValue::Array(vec!["value".into()])]))]
    #[case::set_array_mixed_types(vec![RuntimeValue::Array(vec![
        RuntimeValue::String("text".to_string()),
        RuntimeValue::Number(42.into()),
        RuntimeValue::Bool(true),
        ])],
        vec![
        ast_call("set", smallvec![
            ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
            ast_node(ast::Expr::Literal(ast::Literal::String("replaced".to_string()))),
        ])
        ],
        Ok(vec![RuntimeValue::Array(vec![
        RuntimeValue::String("text".to_string()),
        RuntimeValue::String("replaced".to_string()),
        RuntimeValue::Bool(true),
        ])]))]
    #[case::set_array_with_none(vec![RuntimeValue::Array(vec![
        RuntimeValue::String("item1".to_string()),
        RuntimeValue::NONE,
        RuntimeValue::String("item3".to_string()),
        ])],
        vec![
        ast_call("set", smallvec![
            ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
            ast_node(ast::Expr::Literal(ast::Literal::String("not_none".to_string()))),
        ])
        ],
        Ok(vec![RuntimeValue::Array(vec![
        RuntimeValue::String("item1".to_string()),
        RuntimeValue::String("not_none".to_string()),
        RuntimeValue::String("item3".to_string()),
        ])]))]
    #[case::set_array_replace_with_none(vec![RuntimeValue::Array(vec![
        RuntimeValue::String("item1".to_string()),
        RuntimeValue::String("item2".to_string()),
        RuntimeValue::String("item3".to_string()),
        ])],
        vec![
        ast_call("set", smallvec![
            ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
            ast_node(ast::Expr::Literal(ast::Literal::None)),
        ])
        ],
        Ok(vec![RuntimeValue::Array(vec![
        RuntimeValue::String("item1".to_string()),
        RuntimeValue::NONE,
        RuntimeValue::String("item3".to_string()),
        ])]))]
    #[case::set_array_single_element(vec![RuntimeValue::Array(vec![
        RuntimeValue::String("only".to_string()),
        ])],
        vec![
        ast_call("set", smallvec![
            ast_node(ast::Expr::Literal(ast::Literal::Number(0.into()))),
            ast_node(ast::Expr::Literal(ast::Literal::String("changed".to_string()))),
        ])
        ],
        Ok(vec![RuntimeValue::Array(vec![
        RuntimeValue::String("changed".to_string()),
        ])]))]
    #[case::set_non_array(vec![RuntimeValue::String("not_an_array".to_string())],
        vec![
        ast_call("set", smallvec![
            ast_node(ast::Expr::Literal(ast::Literal::Number(0.into()))),
            ast_node(ast::Expr::Literal(ast::Literal::String("value".to_string()))),
        ])
        ],
        Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                             name: "set".to_string(),
                             args: vec!["not_an_array".to_string().into(), 0.to_string().into(), "value".to_string().into()]})))]
    #[case::set_array_non_number_index(vec![RuntimeValue::Array(vec![
        RuntimeValue::String("item1".to_string()),
        RuntimeValue::String("item2".to_string()),
        ])],
        vec![
        ast_call("set", smallvec![
            ast_node(ast::Expr::Literal(ast::Literal::String("not_a_number".to_string()))),
            ast_node(ast::Expr::Literal(ast::Literal::String("value".to_string()))),
        ])
        ],
        Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                             name: "set".to_string(),
                             args: vec![r#"["item1", "item2"]"#.to_string().into(), "not_a_number".to_string().into(), "value".to_string().into()]})))]
    #[case::del_dict_valid_key(vec![RuntimeValue::Dict(vec![
            ("key1".to_string(), RuntimeValue::String("value1".to_string())),
            ("key2".to_string(), RuntimeValue::String("value2".to_string())),
            ("key3".to_string(), RuntimeValue::String("value3".to_string())),
        ].into_iter().collect())],
        vec![
              ast_call("del", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("key2".to_string()))),
              ]),
        ],
        Ok(vec![RuntimeValue::Dict(vec![
            ("key1".to_string(), RuntimeValue::String("value1".to_string())),
            ("key3".to_string(), RuntimeValue::String("value3".to_string())),
        ].into_iter().collect())]))]
    #[case::del_dict_nonexistent_key(vec![RuntimeValue::Dict(vec![
            ("key1".to_string(), RuntimeValue::String("value1".to_string())),
            ("key2".to_string(), RuntimeValue::String("value2".to_string())),
        ].into_iter().collect())],
        vec![
              ast_call("del", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("nonexistent".to_string()))),
              ]),
        ],
        Ok(vec![RuntimeValue::Dict(vec![
            ("key1".to_string(), RuntimeValue::String("value1".to_string())),
            ("key2".to_string(), RuntimeValue::String("value2".to_string())),
        ].into_iter().collect())]))]
    #[case::del_dict_empty(vec![RuntimeValue::new_dict()],
        vec![
              ast_call("del", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("any_key".to_string()))),
              ]),
        ],
        Ok(vec![RuntimeValue::new_dict()]))]
    #[case::del_dict_single_key(vec![RuntimeValue::Dict(vec![
            ("only_key".to_string(), RuntimeValue::String("only_value".to_string())),
        ].into_iter().collect())],
        vec![
              ast_call("del", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("only_key".to_string()))),
              ]),
        ],
        Ok(vec![RuntimeValue::new_dict()]))]
    #[case::del_dict_mixed_value_types(vec![RuntimeValue::Dict(vec![
            ("str_key".to_string(), RuntimeValue::String("string_value".to_string())),
            ("num_key".to_string(), RuntimeValue::Number(42.into())),
            ("bool_key".to_string(), RuntimeValue::Bool(true)),
            ("array_key".to_string(), RuntimeValue::Array(vec![RuntimeValue::String("item".to_string())])),
        ].into_iter().collect())],
        vec![
              ast_call("del", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("num_key".to_string()))),
              ]),
        ],
        Ok(vec![RuntimeValue::Dict(vec![
            ("str_key".to_string(), RuntimeValue::String("string_value".to_string())),
            ("bool_key".to_string(), RuntimeValue::Bool(true)),
            ("array_key".to_string(), RuntimeValue::Array(vec![RuntimeValue::String("item".to_string())])),
        ].into_iter().collect())]))]
    #[case::del_dict_with_number_key_as_string(vec![RuntimeValue::Dict(vec![
            ("1".to_string(), RuntimeValue::String("value1".to_string())),
            ("2".to_string(), RuntimeValue::String("value2".to_string())),
        ].into_iter().collect())],
        vec![
              ast_call("del", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("1".to_string()))),
              ]),
        ],
        Ok(vec![RuntimeValue::Dict(vec![
            ("2".to_string(), RuntimeValue::String("value2".to_string())),
        ].into_iter().collect())]))]
    #[case::del_dict_with_number_index_error(vec![RuntimeValue::Dict(vec![
            ("key1".to_string(), RuntimeValue::String("value1".to_string())),
            ("key2".to_string(), RuntimeValue::String("value2".to_string())),
        ].into_iter().collect())],
        vec![
              ast_call("del", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
              ]),
        ],
        Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                     name: "del".to_string(),
                                                     args: vec![r#"{"key1": "value1", "key2": "value2"}"#.to_string().into(), "1".to_string().into()]})))]
    #[case::set_code_block_lang_string(vec![RuntimeValue::Markdown(mq_markdown::Node::Code(mq_markdown::Code {
            value: "let x = 1;".to_string(),
            lang: None,
            fence: true,
            meta: None,
            position: None,
        }), None)],
        vec![
            ast_call("set_code_block_lang", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::String("rust".to_string())))
            ])
        ],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Code(mq_markdown::Code {
            value: "let x = 1;".to_string(),
            lang: Some("rust".to_string()),
            fence: true,
            meta: None,
            position: None,
        }), None)]))]
    #[case::set_code_block_lang_empty(vec![RuntimeValue::Markdown(mq_markdown::Node::Code(mq_markdown::Code {
            value: "let x = 1;".to_string(),
            lang: Some("js".to_string()),
            fence: true,
            meta: None,
            position: None,
        }), None)],
        vec![
            ast_call("set_code_block_lang", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::String("".to_string())))
            ])
        ],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Code(mq_markdown::Code {
            value: "let x = 1;".to_string(),
            lang: None,
            fence: true,
            meta: None,
            position: None,
        }), None)]))]
    #[case::set_code_block_lang_non_code(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
            value: "not code".to_string(),
            position: None,
        }), None)],
        vec![
            ast_call("set_code_block_lang", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::String("rust".to_string())))
            ])
        ],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
            value: "not code".to_string(),
            position: None,
        }), None)]))]
    #[case::set_code_block_lang_none(vec![RuntimeValue::NONE],
        vec![
            ast_call("set_code_block_lang", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::String("rust".to_string())))
            ])
        ],
        Ok(vec![RuntimeValue::NONE]))]
    #[case::set_list_ordered_true(
        vec![RuntimeValue::Markdown(mq_markdown::Node::List(mq_markdown::List {
            values: vec!["Item 1".to_string().into(), "Item 2".to_string().into()],
            ordered: false,
            level: 1,
            index: 0,
            checked: None,
            position: None,
        }), None)],
        vec![
            ast_call("set_list_ordered", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::Bool(true))),
            ])
        ],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::List(mq_markdown::List {
            values: vec!["Item 1".to_string().into(), "Item 2".to_string().into()],
            ordered: true,
            level: 1,
            index: 0,
            checked: None,
            position: None,
        }), None)]))]
    #[case::set_list_ordered_false(
        vec![RuntimeValue::Markdown(mq_markdown::Node::List(mq_markdown::List {
            values: vec!["Item 1".to_string().into(), "Item 2".to_string().into()],
            ordered: true,
            level: 1,
            index: 0,
            checked: None,
            position: None,
        }), None)],
        vec![
            ast_call("set_list_ordered", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::Bool(false))),
            ])
        ],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::List(mq_markdown::List {
            values: vec!["Item 1".to_string().into(), "Item 2".to_string().into()],
            ordered: false,
            level: 1,
            index: 0,
            checked: None,
            position: None,
        }), None)]))]
    #[case::set_list_ordered_non_list(
        vec![RuntimeValue::String("not a list".to_string())],
        vec![
            ast_call("set_list_ordered", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::Bool(true))),
            ])
        ],
        Ok(vec![RuntimeValue::String("not a list".to_string())]))]
    #[case::range_number(vec![RuntimeValue::Number(1.into())],
            vec![
                ast_call("range", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(5.into()))),
                ])
            ],
            Ok(vec![RuntimeValue::Array(vec![
                RuntimeValue::Number(1.into()),
                RuntimeValue::Number(2.into()),
                RuntimeValue::Number(3.into()),
                RuntimeValue::Number(4.into()),
                RuntimeValue::Number(5.into()),
            ])]))]
    #[case::range_number_negative(vec![RuntimeValue::Number(5.into())],
            vec![
                ast_call("range", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(5.into()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                ])
            ],
            Ok(vec![RuntimeValue::Array(vec![
                RuntimeValue::Number(5.into()),
                RuntimeValue::Number(4.into()),
                RuntimeValue::Number(3.into()),
                RuntimeValue::Number(2.into()),
                RuntimeValue::Number(1.into()),
            ])]))]
    #[case::range_string(vec![RuntimeValue::String("a".to_string())],
            vec![
                ast_call("range", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("a".to_string()))),
                    ast_node(ast::Expr::Literal(ast::Literal::String("e".to_string()))),
                ])
            ],
            Ok(vec![RuntimeValue::Array(vec![
                RuntimeValue::String("a".to_string()),
                RuntimeValue::String("b".to_string()),
                RuntimeValue::String("c".to_string()),
                RuntimeValue::String("d".to_string()),
                RuntimeValue::String("e".to_string()),
            ])]))]
    #[case::range_string_reverse(vec![RuntimeValue::String("e".to_string())],
            vec![
                ast_call("range", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("e".to_string()))),
                    ast_node(ast::Expr::Literal(ast::Literal::String("a".to_string()))),
                ])
            ],
            Ok(vec![RuntimeValue::Array(vec![
                RuntimeValue::String("e".to_string()),
                RuntimeValue::String("d".to_string()),
                RuntimeValue::String("c".to_string()),
                RuntimeValue::String("b".to_string()),
                RuntimeValue::String("a".to_string()),
            ])]))]
    #[case::range_string_step_2(vec![RuntimeValue::String("a".to_string())],
            vec![
                ast_call("range", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("a".to_string()))),
                    ast_node(ast::Expr::Literal(ast::Literal::String("e".to_string()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(2.into()))),
                ])
            ],
            Ok(vec![RuntimeValue::Array(vec![
                RuntimeValue::String("a".to_string()),
                RuntimeValue::String("c".to_string()),
                RuntimeValue::String("e".to_string()),
            ])]))]
    #[case::range_string_step_minus_2(vec![RuntimeValue::String("e".to_string())],
            vec![
                ast_call("range", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::String("e".to_string()))),
                    ast_node(ast::Expr::Literal(ast::Literal::String("a".to_string()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number((-2).into()))),
                ])
            ],
            Ok(vec![RuntimeValue::Array(vec![
                RuntimeValue::String("e".to_string()),
                RuntimeValue::String("c".to_string()),
                RuntimeValue::String("a".to_string()),
            ])]))]
    #[case::insert_array_middle(vec![RuntimeValue::Array(vec![
                RuntimeValue::String("a".to_string()),
                RuntimeValue::String("b".to_string()),
                RuntimeValue::String("c".to_string()),
            ])],
                vec![
                    ast_call("insert", smallvec![
                        ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                        ast_node(ast::Expr::Literal(ast::Literal::String("x".to_string()))),
                    ])
                ],
                Ok(vec![RuntimeValue::Array(vec![
                    RuntimeValue::String("a".to_string()),
                    RuntimeValue::String("x".to_string()),
                    RuntimeValue::String("b".to_string()),
                    RuntimeValue::String("c".to_string()),
                ])]))]
    #[case::insert_array_start(vec![RuntimeValue::Array(vec![
                RuntimeValue::String("a".to_string()),
                RuntimeValue::String("b".to_string()),
            ])],
                vec![
                    ast_call("insert", smallvec![
                        ast_node(ast::Expr::Literal(ast::Literal::Number(0.into()))),
                        ast_node(ast::Expr::Literal(ast::Literal::String("z".to_string()))),
                    ])
                ],
                Ok(vec![RuntimeValue::Array(vec![
                    RuntimeValue::String("z".to_string()),
                    RuntimeValue::String("a".to_string()),
                    RuntimeValue::String("b".to_string()),
                ])]))]
    #[case::insert_array_end(vec![RuntimeValue::Array(vec![
                RuntimeValue::String("a".to_string()),
                RuntimeValue::String("b".to_string()),
            ])],
                vec![
                    ast_call("insert", smallvec![
                        ast_node(ast::Expr::Literal(ast::Literal::Number(2.into()))),
                        ast_node(ast::Expr::Literal(ast::Literal::String("c".to_string()))),
                    ])
                ],
                Ok(vec![RuntimeValue::Array(vec![
                    RuntimeValue::String("a".to_string()),
                    RuntimeValue::String("b".to_string()),
                    RuntimeValue::String("c".to_string()),
                ])]))]
    #[case::insert_array_out_of_bounds(vec![RuntimeValue::Array(vec![
                RuntimeValue::String("a".to_string()),
            ])],
                vec![
                    ast_call("insert", smallvec![
                        ast_node(ast::Expr::Literal(ast::Literal::Number(5.into()))),
                        ast_node(ast::Expr::Literal(ast::Literal::String("b".to_string()))),
                    ])
                ],
                Ok(vec![RuntimeValue::Array(vec![
                    RuntimeValue::String("a".to_string()),
                    RuntimeValue::NONE,
                    RuntimeValue::NONE,
                    RuntimeValue::NONE,
                    RuntimeValue::NONE,
                    RuntimeValue::String("b".to_string()),
                ])]))]
    #[case::insert_array_negative_index(vec![RuntimeValue::Array(vec![
                RuntimeValue::String("a".to_string()),
                RuntimeValue::String("b".to_string()),
            ])],
                vec![
                    ast_call("insert", smallvec![
                        ast_node(ast::Expr::Literal(ast::Literal::Number((-1).into()))),
                        ast_node(ast::Expr::Literal(ast::Literal::String("z".to_string()))),
                    ])
                ],
                Ok(vec![RuntimeValue::Array(vec![
                    RuntimeValue::String("z".to_string()),
                    RuntimeValue::String("a".to_string()),
                    RuntimeValue::String("b".to_string()),
                ])]))]
    #[case::insert_array_empty(vec![RuntimeValue::Array(Vec::new())],
                vec![
                    ast_call("insert", smallvec![
                        ast_node(ast::Expr::Literal(ast::Literal::Number(0.into()))),
                        ast_node(ast::Expr::Literal(ast::Literal::String("first".to_string()))),
                    ])
                ],
                Ok(vec![RuntimeValue::Array(vec![
                    RuntimeValue::String("first".to_string()),
                ])]))]
    #[case::insert_non_array(vec![RuntimeValue::Number(1.into())],
                vec![
                    ast_call("insert", smallvec![
                        ast_node(ast::Expr::Literal(ast::Literal::Number(0.into()))),
                        ast_node(ast::Expr::Literal(ast::Literal::String("value".to_string()))),
                    ])
                ],
                Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                     name: "insert".to_string(),
                                     args: vec![1.to_string().into(), 0.to_string().into(), "value".to_string().into()]})))]
    #[case::insert_array_non_number_index(vec![RuntimeValue::Array(vec![
                RuntimeValue::String("item1".to_string()),
                RuntimeValue::String("item2".to_string()),
            ])],
                vec![
                    ast_call("insert", smallvec![
                        ast_node(ast::Expr::Literal(ast::Literal::String("not_a_number".to_string()))),
                        ast_node(ast::Expr::Literal(ast::Literal::String("value".to_string()))),
                    ])
                ],
                Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                     name: "insert".to_string(),
                                     args: vec![r#"["item1", "item2"]"#.to_string().into(), "not_a_number".to_string().into(), "value".to_string().into()]})))]
    #[case::insert_string_middle(vec![RuntimeValue::String("ac".to_string())],
                vec![
                    ast_call("insert", smallvec![
                        ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                        ast_node(ast::Expr::Literal(ast::Literal::String("b".to_string()))),
                    ])
                ],
                Ok(vec![RuntimeValue::String("abc".to_string())]))]
    #[case::insert_string_start(vec![RuntimeValue::String("bc".to_string())],
                vec![
                    ast_call("insert", smallvec![
                        ast_node(ast::Expr::Literal(ast::Literal::Number(0.into()))),
                        ast_node(ast::Expr::Literal(ast::Literal::String("a".to_string()))),
                    ])
                ],
                Ok(vec![RuntimeValue::String("abc".to_string())]))]
    #[case::insert_string_end(vec![RuntimeValue::String("ab".to_string())],
                vec![
                    ast_call("insert", smallvec![
                        ast_node(ast::Expr::Literal(ast::Literal::Number(2.into()))),
                        ast_node(ast::Expr::Literal(ast::Literal::String("c".to_string()))),
                    ])
                ],
                Ok(vec![RuntimeValue::String("abc".to_string())]))]
    #[case::insert_string_out_of_bounds(vec![RuntimeValue::String("a".to_string())],
                vec![
                    ast_call("insert", smallvec![
                        ast_node(ast::Expr::Literal(ast::Literal::Number(5.into()))),
                        ast_node(ast::Expr::Literal(ast::Literal::String("b".to_string()))),
                    ])
                ],
                Ok(vec![RuntimeValue::String("a    b".to_string())]))]
    #[case::insert_string_negative_index(vec![RuntimeValue::String("bc".to_string())],
                vec![
                    ast_call("insert", smallvec![
                        ast_node(ast::Expr::Literal(ast::Literal::Number((-1).into()))),
                        ast_node(ast::Expr::Literal(ast::Literal::String("a".to_string()))),
                    ])
                ],
                Ok(vec![RuntimeValue::String("abc".to_string())]))]
    #[case::to_markdown_string_string(vec![RuntimeValue::String("test".to_string())],
                vec![
                    ast_call("to_markdown_string", SmallVec::new())
                ],
                Ok(vec![RuntimeValue::String("test\n".to_string())]))]
    #[case::to_markdown_string_markdown_text(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "test".to_string(), position: None}), None)],
                vec![
                    ast_call("to_markdown_string", SmallVec::new())
                ],
                Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "test\n".to_string(), position: None}), None)]))]
    #[case::to_markdown_string_markdown_heading(vec![RuntimeValue::Markdown(mq_markdown::Node::Heading(mq_markdown::Heading{depth: 2, values: vec!["Heading".to_string().into()], position: None}), None)],
                vec![
                    ast_call("to_markdown_string", SmallVec::new())
                ],
                Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "## Heading\n".to_string(), position: None}), None)]))]
    #[case::to_markdown_string_markdown_code(vec![RuntimeValue::Markdown(mq_markdown::Node::Code(mq_markdown::Code{value: "let x = 1;".to_string(), lang: Some("rust".to_string()), fence: true, meta: None, position: None}), None)],
                vec![
                    ast_call("to_markdown_string", SmallVec::new())
                ],
                Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "```rust\nlet x = 1;\n```\n".to_string(), position: None}), None)]))]
    #[case::to_markdown_string_none(vec![RuntimeValue::NONE],
                vec![
                    ast_call("to_markdown_string", SmallVec::new())
                ],
                Ok(vec![RuntimeValue::String("".to_string())]))]
    #[case::increase_header_level_h1(vec![RuntimeValue::Markdown(mq_markdown::Node::Heading(mq_markdown::Heading {
                depth: 1,
                values: vec!["Heading 1".to_string().into()],
                position: None
            }), None)],
                vec![
                    ast_call("increase_header_level", SmallVec::new())
                ],
                Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Heading(mq_markdown::Heading {
                    depth: 2,
                    values: vec!["Heading 1".to_string().into()],
                    position: None
                }), None)]))]
    #[case::increase_header_level_h6(vec![RuntimeValue::Markdown(mq_markdown::Node::Heading(mq_markdown::Heading {
                depth: 6,
                values: vec!["Heading 6".to_string().into()],
                position: None
            }), None)],
                vec![
                    ast_call("increase_header_level", SmallVec::new())
                ],
                Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Heading(mq_markdown::Heading {
                    depth: 6,
                    values: vec!["Heading 6".to_string().into()],
                    position: None
                }), None)]))]
    #[case::increase_header_level_non_heading(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
                value: "Not a heading".to_string(),
                position: None
            }), None)],
                vec![
                    ast_call("increase_header_level", SmallVec::new())
                ],
                Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
                    value: "Not a heading".to_string(),
                    position: None
                }), None)]))]
    #[case::decrease_header_level_h2(vec![RuntimeValue::Markdown(mq_markdown::Node::Heading(mq_markdown::Heading {
                depth: 2,
                values: vec!["Heading 2".to_string().into()],
                position: None
            }), None)],
                vec![
                    ast_call("decrease_header_level", SmallVec::new())
                ],
                Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Heading(mq_markdown::Heading {
                    depth: 1,
                    values: vec!["Heading 2".to_string().into()],
                    position: None
                }), None)]))]
    #[case::decrease_header_level_h1(vec![RuntimeValue::Markdown(mq_markdown::Node::Heading(mq_markdown::Heading {
                depth: 1,
                values: vec!["Heading 1".to_string().into()],
                position: None
            }), None)],
                vec![
                    ast_call("decrease_header_level", SmallVec::new())
                ],
                Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Heading(mq_markdown::Heading {
                    depth: 1,
                    values: vec!["Heading 1".to_string().into()],
                    position: None
                }), None)]))]
    #[case::decrease_header_level_non_heading(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
                value: "Not a heading".to_string(),
                position: None
            }), None)],
            vec![
                ast_call("decrease_header_level", SmallVec::new())
            ],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
                value: "Not a heading".to_string(),
                position: None
            }), None)]))]
    #[case::break_in_foreach(
       vec![RuntimeValue::Array(vec![
            RuntimeValue::Number(1.into()),
            RuntimeValue::Number(2.into()),
            RuntimeValue::Number(3.into()),
        ])],
            vec![
                ast_node(ast::Expr::Foreach(
                    ast::Ident::new("x"),
                    ast_node(ast::Expr::Self_),
                    vec![
                        ast_node(ast::Expr::If(smallvec![
                            (
                                Some(ast_node(ast::Expr::Call(
                                    ast::Ident::new("eq"),
                                    smallvec![
                                        ast_node(ast::Expr::Ident(ast::Ident::new("x"))),
                                        ast_node(ast::Expr::Literal(ast::Literal::Number(2.into()))),
                                    ],
                                ))),
                                ast_node(ast::Expr::Break),
                            ),
                            (
                                None,
                                ast_node(ast::Expr::Ident(ast::Ident::new("x"))),
                            ),
                        ])),
                    ],
                )),
            ],
            Ok(vec![RuntimeValue::Array(vec![
                RuntimeValue::Number(1.into()),
            ])])
        )]
    #[case::continue_in_foreach(
        vec![RuntimeValue::Array(vec![
            RuntimeValue::Number(1.into()),
            RuntimeValue::Number(2.into()),
            RuntimeValue::Number(3.into()),
        ])],
        vec![
            ast_node(ast::Expr::Foreach(
                ast::Ident::new("x"),
                ast_node(ast::Expr::Self_),
                vec![
                    ast_node(ast::Expr::If(smallvec![
                        (
                            Some(ast_node(ast::Expr::Call(
                                ast::Ident::new("eq"),
                                smallvec![
                                    ast_node(ast::Expr::Ident(ast::Ident::new("x"))),
                                    ast_node(ast::Expr::Literal(ast::Literal::Number(2.into()))),
                                ],
                            ))),
                            ast_node(ast::Expr::Continue),
                        ),
                        (
                            None,
                            ast_node(ast::Expr::Ident(ast::Ident::new("x"))),
                        ),
                    ])),
                ],
            )),
        ],
        Ok(vec![RuntimeValue::Array(vec![
            RuntimeValue::Number(1.into()),
            RuntimeValue::Number(3.into()),
        ])])
    )]
    #[case::foreach_string(
        vec![RuntimeValue::String("abc".to_string())],
        vec![
            ast_node(ast::Expr::Foreach(
                ast::Ident::new("c"),
                ast_node(ast::Expr::Self_),
                vec![
                    ast_node(ast::Expr::Ident(ast::Ident::new("c"))),
                ],
            )),
        ],
        Ok(vec![RuntimeValue::Array(vec![
            RuntimeValue::String("a".to_string()),
            RuntimeValue::String("b".to_string()),
            RuntimeValue::String("c".to_string()),
        ])])
    )]
    #[case::to_array_string(vec![RuntimeValue::String("test".to_string())],
        vec![
            ast_call("to_array", SmallVec::new())
        ],
        Ok(vec![RuntimeValue::Array(vec![RuntimeValue::String("t".to_string()), RuntimeValue::String("e".to_string()), RuntimeValue::String("s".to_string()), RuntimeValue::String("t".to_string())])]))]
    #[case::to_array_number(vec![RuntimeValue::Number(42.into())],
        vec![
            ast_call("to_array", SmallVec::new())
        ],
        Ok(vec![RuntimeValue::Array(vec![RuntimeValue::Number(42.into())])]))]
    #[case::to_array_bool(vec![RuntimeValue::Bool(true)],
        vec![
            ast_call("to_array", SmallVec::new())
        ],
        Ok(vec![RuntimeValue::Array(vec![RuntimeValue::Bool(true)])]))]
    #[case::to_array_array(vec![RuntimeValue::Array(vec![RuntimeValue::String("a".to_string()), RuntimeValue::String("b".to_string())])],
        vec![
            ast_call("to_array", SmallVec::new())
        ],
        Ok(vec![RuntimeValue::Array(vec![RuntimeValue::String("a".to_string()), RuntimeValue::String("b".to_string())])]))]
    #[case::to_array_empty_array(vec![RuntimeValue::Array(Vec::new())],
        vec![
            ast_call("to_array", SmallVec::new())
        ],
        Ok(vec![RuntimeValue::Array(Vec::new())]))]
    #[case::to_array_dict(vec![RuntimeValue::Dict(vec![
            ("key".to_string(), RuntimeValue::String("value".to_string())),
        ].into_iter().collect())],
        vec![
            ast_call("to_array", SmallVec::new())
        ],
        Ok(vec![RuntimeValue::Array(vec![RuntimeValue::Dict(vec![
            ("key".to_string(), RuntimeValue::String("value".to_string())),
        ].into_iter().collect())])]))]
    #[case::type_none(vec![RuntimeValue::NONE],
       vec![
            ast_call("type", SmallVec::new())
       ],
       Ok(vec![RuntimeValue::String("None".to_string())]))]
    #[case::to_text(vec![RuntimeValue::NONE],
            vec![
                 ast_call("to_text", SmallVec::new())
            ],
            Ok(vec![RuntimeValue::NONE]))]
    #[case::starts_with(vec![RuntimeValue::NONE],
       vec![
            ast_call("starts_with", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::String("te".to_string())))
            ])
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::ends_with(vec![RuntimeValue::NONE],
       vec![
            ast_call("ends_with", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::String("te".to_string())))
            ])
       ],
       Ok(vec![RuntimeValue::FALSE]))]
    #[case::rindex(vec![RuntimeValue::NONE],
       vec![
            ast_call("rindex", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::String("String".to_string())))
            ])
       ],
       Ok(vec![RuntimeValue::Number((-1).into())]))]
    #[case::utf8bytelen(vec![RuntimeValue::NONE],
       vec![
            ast_call("utf8bytelen", SmallVec::new())
       ],
       Ok(vec![RuntimeValue::Number(0.into())]))]
    #[case::index(vec![RuntimeValue::NONE],
       vec![
            ast_call("index", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::String("test".to_string())))
            ])
       ],
       Ok(vec![RuntimeValue::Number((-1).into())]))]
    #[case::del(vec![RuntimeValue::NONE],
        vec![
              ast_call("del", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Number(4.into()))),
              ]),
        ],
        Ok(vec![RuntimeValue::NONE]))]
    #[case::downcase(vec![RuntimeValue::NONE],
       vec![
            ast_call("downcase", SmallVec::new())
       ],
       Ok(vec![RuntimeValue::NONE]))]
    #[case::slice(vec![RuntimeValue::NONE],
       vec![
            ast_call("slice", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::Number(0.into()))),
                ast_node(ast::Expr::Literal(ast::Literal::Number(4.into()))),
            ])
       ],
       Ok(vec![RuntimeValue::NONE]))]
    #[case::slice(vec![RuntimeValue::NONE],
       vec![
            ast_call("len", SmallVec::new())
       ],
       Ok(vec![RuntimeValue::Number(0.into())]))]
    #[case::slice(vec![RuntimeValue::NONE],
       vec![
            ast_call("upcase", SmallVec::new())
       ],
       Ok(vec![RuntimeValue::NONE]))]
    #[case::slice_array_negative_start_index(vec![RuntimeValue::Array(vec![
            RuntimeValue::String("item1".to_string()),
            RuntimeValue::String("item2".to_string()),
            RuntimeValue::String("item3".to_string()),
            RuntimeValue::String("item4".to_string()),
            RuntimeValue::String("item5".to_string()),
        ])],
       vec![
            ast_call("slice", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::Number((-2).into()))),
                ast_node(ast::Expr::Literal(ast::Literal::Number(4.into()))),
            ])
       ],
       Ok(vec![RuntimeValue::Array(vec![
            RuntimeValue::String("item4".to_string()),
        ])]))]
    #[case::slice_array_negative_end_index(vec![RuntimeValue::Array(vec![
            RuntimeValue::String("item1".to_string()),
            RuntimeValue::String("item2".to_string()),
            RuntimeValue::String("item3".to_string()),
            RuntimeValue::String("item4".to_string()),
            RuntimeValue::String("item5".to_string()),
        ])],
       vec![
            ast_call("slice", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                ast_node(ast::Expr::Literal(ast::Literal::Number((-1).into()))),
            ])
       ],
       Ok(vec![RuntimeValue::Array(vec![
            RuntimeValue::String("item2".to_string()),
            RuntimeValue::String("item3".to_string()),
            RuntimeValue::String("item4".to_string()),
        ])]))]
    #[case::slice_array_both_negative_indices(vec![RuntimeValue::Array(vec![
            RuntimeValue::String("item1".to_string()),
            RuntimeValue::String("item2".to_string()),
            RuntimeValue::String("item3".to_string()),
            RuntimeValue::String("item4".to_string()),
            RuntimeValue::String("item5".to_string()),
        ])],
       vec![
            ast_call("slice", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::Number((-4).into()))),
                ast_node(ast::Expr::Literal(ast::Literal::Number((-2).into()))),
            ])
       ],
       Ok(vec![RuntimeValue::Array(vec![
            RuntimeValue::String("item2".to_string()),
            RuntimeValue::String("item3".to_string()),
        ])]))]
    #[case::slice_string_negative_start_index(vec![RuntimeValue::String("abcdef".to_string())],
       vec![
            ast_call("slice", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::Number((-3).into()))),
                ast_node(ast::Expr::Literal(ast::Literal::Number(6.into()))),
            ])
       ],
       Ok(vec![RuntimeValue::String("def".to_string())]))]
    #[case::slice_string_negative_end_index(vec![RuntimeValue::String("abcdef".to_string())],
       vec![
            ast_call("slice", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                ast_node(ast::Expr::Literal(ast::Literal::Number((-1).into()))),
            ])
       ],
       Ok(vec![RuntimeValue::String("bcde".to_string())]))]
    #[case::slice_string_both_negative_indices(vec![RuntimeValue::String("abcdef".to_string())],
       vec![
            ast_call("slice", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::Number((-5).into()))),
                ast_node(ast::Expr::Literal(ast::Literal::Number((-2).into()))),
            ])
       ],
       Ok(vec![RuntimeValue::String("bcd".to_string())]))]
    #[case::to_code(vec![RuntimeValue::NONE],
        vec![
              ast_call("to_code", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::None)),
              ]),
        ],
        Ok(vec![RuntimeValue::NONE]))]
    #[case::to_code(vec![RuntimeValue::NONE],
        vec![
              ast_call("update", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::None)),
              ]),
        ],
        Ok(vec![RuntimeValue::NONE]))]
    #[case::to_code_inline(vec![RuntimeValue::NONE],
        vec![
              ast_call("to_code_inline", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::None)),
              ]),
        ],
        Ok(vec![RuntimeValue::NONE]))]
    #[case::to_link(vec![RuntimeValue::NONE],
        vec![
              ast_call("to_link", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::None)),
                ast_node(ast::Expr::Literal(ast::Literal::None)),
                ast_node(ast::Expr::Literal(ast::Literal::None)),
              ]),
        ],
        Ok(vec![RuntimeValue::NONE]))]
    #[case::to_strong(vec![RuntimeValue::NONE],
        vec![
              ast_call("to_strong", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::None)),
              ]),
        ],
        Ok(vec![RuntimeValue::NONE]))]
    #[case::to_em(vec![RuntimeValue::NONE],
        vec![
              ast_call("to_em", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::None)),
              ]),
        ],
        Ok(vec![RuntimeValue::NONE]))]
    #[case::to_md_text(vec![RuntimeValue::NONE],
        vec![
              ast_call("to_md_text", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::None)),
              ]),
        ],
        Ok(vec![RuntimeValue::NONE]))]
    #[case::to_md_list(vec![RuntimeValue::NONE],
        vec![
              ast_call("to_md_list", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::None)),
                ast_node(ast::Expr::Literal(ast::Literal::None)),
              ]),
        ],
        Ok(vec![RuntimeValue::NONE]))]
    #[case::match_(vec![RuntimeValue::NONE],
       vec![
            ast_call("match", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::String(r"\d+".to_string()))),
            ])
       ],
       Ok(vec![RuntimeValue::Array(Vec::new())]))]
    #[case::gsub(vec![RuntimeValue::NONE],
       vec![
            ast_call("gsub", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::String(r"\d+".to_string()))),
                ast_node(ast::Expr::Literal(ast::Literal::String(r"1".to_string()))),
            ])
       ],
       Ok(vec![RuntimeValue::NONE]))]
    #[case::replace(vec![RuntimeValue::NONE],
       vec![
            ast_call("replace", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::String("1".to_string()))),
                ast_node(ast::Expr::Literal(ast::Literal::String("2".to_string()))),
            ])
       ],
       Ok(vec![RuntimeValue::NONE]))]
    #[case::replace(vec![RuntimeValue::NONE],
       vec![
            ast_call("repeat", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
            ])
       ],
       Ok(vec![RuntimeValue::NONE]))]
    #[case::trim(vec![RuntimeValue::NONE],
       vec![
            ast_call("trim", SmallVec::new())
       ],
       Ok(vec![RuntimeValue::NONE]))]
    #[case::split(vec![RuntimeValue::NONE],
       vec![
            ast_call("split", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::String("test".to_string()))),
            ])
       ],
       Ok(vec![RuntimeValue::EMPTY_ARRAY]))]
    #[case::to_md_name(vec![RuntimeValue::NONE],
        vec![
              ast_call("to_md_name", SmallVec::new()),
        ],
        Ok(vec![RuntimeValue::NONE]))]
    #[case::get_url_none(vec![RuntimeValue::NONE],
        vec![
             ast_call("get_url", SmallVec::new())
        ],
        Ok(vec![RuntimeValue::NONE]))]
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
                expr: Rc::new(ast::Expr::Call(ast::Ident::new("func1"), SmallVec::new())),
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

    #[rstest]
    #[case::simple_interpolated_string(
        vec![RuntimeValue::String("world".to_string())],
        vec![
            ast_node(ast::Expr::InterpolatedString(vec![
                ast::StringSegment::Text("Hello, ".to_string()),
                ast::StringSegment::Self_,
                ast::StringSegment::Text("!".to_string()),
            ])),
        ],
        Ok(vec![RuntimeValue::String("Hello, world!".to_string())])
    )]
    #[case::interpolated_string_with_number(
        vec![RuntimeValue::Number(42.into())],
        vec![
            ast_node(ast::Expr::InterpolatedString(vec![
                ast::StringSegment::Text("The answer is ".to_string()),
                ast::StringSegment::Self_,
                ast::StringSegment::Text(".".to_string()),
            ])),
        ],
        Ok(vec![RuntimeValue::String("The answer is 42.".to_string())])
    )]
    #[case::interpolated_string_with_bool(
        vec![RuntimeValue::Bool(true)],
        vec![
            ast_node(ast::Expr::InterpolatedString(vec![
                ast::StringSegment::Text("Value: ".to_string()),
                ast::StringSegment::Self_,
            ])),
        ],
        Ok(vec![RuntimeValue::String("Value: true".to_string())])
    )]
    #[case::interpolated_string_with_none(
        vec![RuntimeValue::NONE],
        vec![
            ast_node(ast::Expr::InterpolatedString(vec![
                ast::StringSegment::Text("None: ".to_string()),
                ast::StringSegment::Self_,
            ])),
        ],
        Ok(vec![RuntimeValue::String("None: ".to_string())])
    )]
    #[case::interpolated_string_with_array(
        vec![RuntimeValue::Array(vec![
            RuntimeValue::String("a".to_string()),
            RuntimeValue::String("b".to_string()),
        ])],
        vec![
            ast_node(ast::Expr::InterpolatedString(vec![
                ast::StringSegment::Text("Array: ".to_string()),
                ast::StringSegment::Self_,
            ])),
        ],
        Ok(vec![RuntimeValue::String(r#"Array: ["a", "b"]"#.to_string())])
    )]
    #[case::interpolated_string_only_literal(
        vec![RuntimeValue::String("ignored".to_string())],
        vec![
            ast_node(ast::Expr::InterpolatedString(vec![
                ast::StringSegment::Text("Just a string".to_string()),
            ])),
        ],
        Ok(vec![RuntimeValue::String("Just a string".to_string())])
    )]
    #[case::interpolated_string_empty(
        vec![RuntimeValue::String("ignored".to_string())],
        vec![
            ast_node(ast::Expr::InterpolatedString(vec![])),
        ],
        Ok(vec![RuntimeValue::String("".to_string())])
    )]
    #[case::interpolated_string_with_env_var(
        vec![RuntimeValue::String("ignored".to_string())],
        vec![
            ast_node(ast::Expr::InterpolatedString(vec![
                ast::StringSegment::Text("HOME: ".to_string()),
                ast::StringSegment::Env("HOME".into()),
            ])),
        ],
        {
            unsafe { std::env::set_var("HOME", "/home/testuser") };
            Ok(vec![RuntimeValue::String("HOME: /home/testuser".to_string())])
        }
    )]
    #[case::interpolated_string_with_missing_env_var(
        vec![RuntimeValue::String("ignored".to_string())],
        vec![
            ast_node(ast::Expr::InterpolatedString(vec![
                ast::StringSegment::Text("MISSING: ".to_string()),
                ast::StringSegment::Env("MQ_TEST_MISSING_ENV".into()),
            ])),
        ],
        {
            unsafe { std::env::remove_var("MQ_TEST_MISSING_ENV") };
            Err(EvalError::EnvNotFound(
                Token {
                    range: Range::default(),
                    kind: TokenKind::Eof,
                    module_id: 1.into(),
                },
                "MQ_TEST_MISSING_ENV".into(),
            ).into())
        }
    )]
    #[case::interpolated_string_env_and_self(
        vec![RuntimeValue::String("value".to_string())],
        vec![
            ast_node(ast::Expr::InterpolatedString(vec![
                ast::StringSegment::Env("USER".into()),
                ast::StringSegment::Text(":".to_string()),
                ast::StringSegment::Self_,
            ])),
        ],
        {
            unsafe { std::env::set_var("USER", "tester") };
            Ok(vec![RuntimeValue::String("tester:value".to_string())])
        }
    )]
    #[case::interpolated_string_env_only(
        vec![RuntimeValue::String("ignored".to_string())],
        vec![
            ast_node(ast::Expr::InterpolatedString(vec![
                ast::StringSegment::Env("USER".into()),
            ])),
        ],
        {
            unsafe { std::env::set_var("USER", "tester") };
            Ok(vec![RuntimeValue::String("tester".to_string())])
        }
    )]
    #[case::interpolated_string_env_and_literal(
        vec![RuntimeValue::String("ignored".to_string())],
        vec![
            ast_node(ast::Expr::InterpolatedString(vec![
                ast::StringSegment::Text("User: ".to_string()),
                ast::StringSegment::Env("USER".into()),
                ast::StringSegment::Text("!".to_string()),
            ])),
        ],
        {
            unsafe { std::env::set_var("USER", "tester") };
            Ok(vec![RuntimeValue::String("User: tester!".to_string())])
        }
    )]
    fn test_interpolated_string_eval(
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
}
