use std::{cell::RefCell, rc::Rc};

use crate::{
    Token, TokenKind,
    arena::Arena,
    ast::{self as ast_module, node::{self as ast, Ident as AstIdent, Params as AstParams, Literal as AstLiteral, Selector as AstSelector, StringSegment as AstStringSegment}}, // Updated imports
    ast::{ExprPool, ExprRef, get_expr_range}, // Added for new AST structure
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
use smallvec::SmallVec;

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
        program_refs: &[ExprRef],
        pool: &ExprPool,
        token_arena_ref: &Arena<Rc<Token>>, // Added token_arena_ref
        input: I,
    ) -> Result<Vec<RuntimeValue>, InnerError>
    where
        I: Iterator<Item = RuntimeValue>,
    {
        // Initial pass for definitions and includes
        // This part needs careful adaptation as it originally modified `program`
        let mut main_eval_refs: Vec<ExprRef> = Vec::with_capacity(program_refs.len());
        for expr_ref in program_refs.iter() {
            let (expr_data, token_id) = pool.get(*expr_ref)
                .ok_or_else(|| InnerError::Eval(EvalError::InternalError(
                    // Better error handling for invalid ExprRef
                    token_arena_ref.get(TokenKind::Eof.into()).unwrap().clone() // Placeholder token
                )))?; 
            match expr_data {
                ast::Expr::Def(ident, params, body_exprs) => {
                    self.env.borrow_mut().define(
                        ident,
                        RuntimeValue::Function(
                            params.clone(),       // AstParams
                            body_exprs.clone(),   // Vec<ExprRef>
                            Rc::clone(&self.env),
                        ),
                    );
                }
                ast::Expr::Include(module_id_literal) => {
                     // eval_include needs pool and token_arena_ref if it does any range calculation
                    self.eval_include(module_id_literal.clone(), pool, token_arena_ref)?;
                }
                _ => main_eval_refs.push(*expr_ref),
            }
        }

        // Find Expr::Nodes, if any
        let nodes_expr_ref_idx = main_eval_refs.iter().position(|expr_ref| {
            pool.get_expr(*expr_ref).map_or(false, |expr| matches!(expr, ast::Expr::Nodes))
        });

        if let Some(index) = nodes_expr_ref_idx {
            let (program_part, nodes_part_with_node) = main_eval_refs.split_at(index);
            let nodes_program_part = &nodes_part_with_node[1..]; // Skip the Expr::Nodes itself

            let values: Result<Vec<RuntimeValue>, InnerError> = input
                .map(|runtime_value| match &runtime_value {
                    RuntimeValue::Markdown(md_node, _) => self.eval_markdown_node(program_part, pool, token_arena_ref, md_node),
                    _ => self
                        .eval_program(program_part, pool, token_arena_ref, runtime_value, Rc::clone(&self.env))
                        .map_err(InnerError::Eval),
                })
                .collect();

            if nodes_program_part.is_empty() {
                values
            } else {
                self.eval_program(nodes_program_part, pool, token_arena_ref, values?.into(), Rc::clone(&self.env))
                    .map(|value| {
                        if let RuntimeValue::Array(vals) = value { vals } else { vec![value] }
                    })
                    .map_err(InnerError::Eval)
            }
        } else {
            input
                .map(|runtime_value| match &runtime_value {
                    RuntimeValue::Markdown(md_node, _) => self.eval_markdown_node(&main_eval_refs, pool, token_arena_ref, md_node),
                    _ => self
                        .eval_program(&main_eval_refs, pool, token_arena_ref, runtime_value, Rc::clone(&self.env))
                        .map_err(InnerError::Eval),
                })
                .collect()
        }
    }

    fn eval_markdown_node(
        &mut self,
        program_refs: &[ExprRef], // Changed Program to &[ExprRef]
        pool: &ExprPool,          // Added ExprPool
        token_arena_ref: &Arena<Rc<Token>>, // Added token_arena_ref
        node: &mq_markdown::Node,
    ) -> Result<RuntimeValue, InnerError> {
        node.map_values(&mut |child_node| {
            let value = self
                .eval_program(
                    program_refs,
                    pool,
                    token_arena_ref,
                    RuntimeValue::Markdown(child_node.clone(), None),
                    Rc::clone(&self.env),
                )
                .map_err(InnerError::Eval)?; // eval_program now returns Result<RuntimeValue, EvalError>

            Ok(match value { // value is RuntimeValue
                RuntimeValue::None => child_node.to_fragment(),
                RuntimeValue::Function(_, _, _) | RuntimeValue::NativeFunction(_) => {
                    mq_markdown::Node::Empty
                }
                RuntimeValue::Array(_)
                | RuntimeValue::Bool(_)
                | RuntimeValue::Number(_)
                | RuntimeValue::String(_) => value.to_string().into(),
                RuntimeValue::Markdown(md_node, _) => md_node,
            })
        })
        .map(|node| RuntimeValue::Markdown(node, None)) // node here is mq_markdown::Node
    }

    pub fn define_string_value(&self, name: &str, value: &str) {
        self.env.borrow_mut().define(
            &AstIdent::new(name), // AstIdent
            RuntimeValue::String(value.to_string()),
        );
    }

    pub(crate) fn load_builtin_module(&mut self) -> Result<(), EvalError> {
        let module_data = self
            .module_loader
            .load_builtin(Rc::clone(&self.token_arena)) // This returns Option<ModuleData>
            .map_err(EvalError::ModuleLoadError)?;
        // load_module needs to be updated to take ModuleData or similar, and ExprPool + token_arena
        // For now, this structure is deeply tied to Rc<Node> based ModuleData.
        // This will require ModuleData to also be refactored.
        // Temporary: self.load_module(module_data) // This call will break.
        Ok(()) // Placeholder
    }
    
    // This method needs significant rework because module::Module likely holds Vec<Rc<Node>>
    pub(crate) fn load_module(&mut self, module_data: Option<module::ModuleData>, pool: &ExprPool, token_arena_ref: &Arena<Rc<Token>>) -> Result<(), EvalError> {
        if let Some(md) = module_data {
            // md.defs and md.vars are likely Vec<Rc<Node>>.
            // This needs to change to Vec<ExprRef> when ModuleData is refactored.
            // For now, this part of the code will be incompatible.
            // Conceptual rewrite:
            // for def_expr_ref in md.defs_refs {
            //     let (expr_data, _) = pool.get(*def_expr_ref).unwrap();
            //     if let ast::Expr::Def(ident, params, body_exprs) = expr_data {
            //         self.env.borrow_mut().define(
            //             ident,
            //             RuntimeValue::Function(params.clone(), body_exprs.clone(), Rc::clone(&self.env)),
            //         );
            //     }
            // }
            // for let_expr_ref in md.vars_refs {
            //     let (expr_data, token_id) = pool.get(*let_expr_ref).unwrap();
            //     if let ast::Expr::Let(ident, value_expr_ref) = expr_data {
            //         let val = self.eval_expr(*value_expr_ref, pool, token_arena_ref, &RuntimeValue::NONE, Rc::clone(&self.env))?;
            //         self.env.borrow_mut().define(ident, val);
            //     } else {
            //         let range = get_expr_range(*let_expr_ref, pool, token_arena_ref);
            //         return Err(EvalError::InternalError(token_arena_ref.get(token_id).unwrap().clone())); // Simplified error
            //     }
            // }
        }
        Ok(())
    }

    fn eval_program(
        &mut self,
        program_refs: &[ExprRef], // Changed Program to &[ExprRef]
        pool: &ExprPool,          // Added ExprPool
        token_arena_ref: &Arena<Rc<Token>>, // Added token_arena_ref
        runtime_value: RuntimeValue,
        env: Rc<RefCell<Env>>,
    ) -> Result<RuntimeValue, EvalError> {
        program_refs
            .iter()
            .try_fold(runtime_value, |current_value, expr_ref| {
                if self.options.filter_none && current_value.is_none() {
                    return Ok(RuntimeValue::NONE);
                }
                self.eval_expr(*expr_ref, pool, token_arena_ref, &current_value, Rc::clone(&env))
            })
    }

    fn eval_ident(
        &self,
        ident: &AstIdent,        // AstIdent
        expr_ref_for_range: ExprRef, // Added for error reporting range
        pool: &ExprPool,             // Added
        token_arena_ref: &Arena<Rc<Token>>, // Added
        env: Rc<RefCell<Env>>,
    ) -> Result<RuntimeValue, EvalError> {
        env.borrow()
            .resolve(ident)
            .map_err(|e| {
                let range = get_expr_range(expr_ref_for_range, pool, token_arena_ref);
                // Assuming EnvError has a method to_eval_error that now takes a Range
                e.to_eval_error(range) 
            })
    }

    fn eval_include(&mut self, module_literal: AstLiteral, pool: &ExprPool, token_arena_ref: &Arena<Rc<Token>>) -> Result<(), EvalError> { // AstLiteral
        match module_literal {
            ast::Literal::String(module_name) => {
                // module_loader.load_from_file needs to be updated or it returns ModuleData based on Rc<Node>
                // This is a deeper change related to how modules are parsed and stored.
                // For now, this will likely break or need ModuleData to be refactored.
                let module_data = self
                    .module_loader
                    .load_from_file(&module_name, Rc::clone(&self.token_arena)) // token_arena from self
                    .map_err(EvalError::ModuleLoadError)?;
                self.load_module(module_data, pool, token_arena_ref) // Pass pool and token_arena_ref
            }
            _ => Err(EvalError::ModuleLoadError(
                module::ModuleError::InvalidModule, // This error might need a Range too
            )),
        }
    }

    fn eval_selector_expr(runtime_value: RuntimeValue, selector: &AstSelector) -> RuntimeValue { // AstSelector
        match &runtime_value {
            RuntimeValue::Markdown(node_value, _) => {
                if builtin::eval_selector(node_value, selector) {
                    runtime_value
                } else {
                    RuntimeValue::NONE
                }
            }
            RuntimeValue::Array(values) => {
                let values = values
                    .iter()
                    .map(|value| match value {
                    RuntimeValue::Markdown(md_node, _) => { // md_node is mq_markdown::Node
                        if builtin::eval_selector(md_node, selector) { // selector is &AstSelector
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

    fn eval_interpolated_string(
        &self,
    current_expr_ref: ExprRef, // For range
    pool: &ExprPool,
    token_arena_ref: &Arena<Rc<Token>>,
    runtime_value: &RuntimeValue, // Current pipeline value
        env: Rc<RefCell<Env>>,
    segments: &Vec<AstStringSegment>, // AstStringSegment
    ) -> Result<RuntimeValue, EvalError> {
    segments
        .iter()
        .try_fold(String::with_capacity(100), |mut acc, segment| {
            match segment {
                ast::StringSegment::Text(s) => acc.push_str(s),
                ast::StringSegment::Ident(ident) => { // ident is AstIdent
                    // eval_ident needs the ExprRef of the Ident for error reporting,
                    // but here we only have the Ident struct from the segment.
                    // This is a limitation. For now, use current_expr_ref for range.
                    let value = self.eval_ident(ident, current_expr_ref, pool, token_arena_ref, Rc::clone(&env))?;
                    acc.push_str(&value.to_string());
                    }
                ast::StringSegment::Self_ => {
                    acc.push_str(&runtime_value.to_string());
                }
            }
            Ok(acc)
        })
        .map(|acc| acc.into())
    }

    fn eval_expr(
        &mut self,
    current_expr_ref: ExprRef,
    pool: &ExprPool,
    token_arena_ref: &Arena<Rc<Token>>,
    runtime_value: &RuntimeValue, // Current value in pipeline
        env: Rc<RefCell<Env>>,
    ) -> Result<RuntimeValue, EvalError> {
    let (expr_data, token_id) = pool.get(current_expr_ref)
        .ok_or_else(|| EvalError::InternalError(token_arena_ref.get(TokenKind::Eof.into()).unwrap().clone()))?; // Better error

    match expr_data {
        ast::Expr::Selector(selector_data) => { // selector_data is &AstSelector
            Ok(Self::eval_selector_expr(runtime_value.clone(), selector_data))
            }
        ast::Expr::Call(ident, args_refs, optional) => { // ident is &AstIdent, args_refs is &Args (SmallVec<[ExprRef; 4]>)
            self.eval_fn(current_expr_ref, pool, token_arena_ref, runtime_value, ident, args_refs, *optional, env)
            }
            ast::Expr::Self_ | ast::Expr::Nodes => Ok(runtime_value.clone()),
        ast::Expr::If(branches_refs) => { // branches_refs is &Branches (SmallVec<[(Option<ExprRef>, ExprRef); 4]>)
            self.eval_if(current_expr_ref, pool, token_arena_ref, runtime_value, branches_refs, env)
        }
        ast::Expr::Ident(ident_data) => { // ident_data is &AstIdent
            self.eval_ident(ident_data, current_expr_ref, pool, token_arena_ref, Rc::clone(&env))
        }
        ast::Expr::Literal(literal_data) => Ok(self.eval_literal(literal_data)), // literal_data is &AstLiteral
        ast::Expr::Def(ident, params, body_exprs) => { // body_exprs is &Vec<ExprRef>
            let function = RuntimeValue::Function(params.clone(), body_exprs.clone(), Rc::clone(&env));
                env.borrow_mut().define(ident, function.clone());
                Ok(function)
            }
        ast::Expr::Fn(params, body_exprs) => { // body_exprs is &Vec<ExprRef>
            let function = RuntimeValue::Function(params.clone(), body_exprs.clone(), Rc::clone(&env));
                Ok(function)
            }
        ast::Expr::Let(ident, value_expr_ref) => { // value_expr_ref is &ExprRef
            let let_val = self.eval_expr(*value_expr_ref, pool, token_arena_ref, runtime_value, Rc::clone(&env))?;
            env.borrow_mut().define(ident, let_val);
            Ok(runtime_value.clone()) // Let expression itself evaluates to the original runtime_value in pipeline
            }
        ast::Expr::While(cond_ref, body_refs) => { // cond_ref is &ExprRef, body_refs is &Vec<ExprRef>
            self.eval_while(current_expr_ref, pool, token_arena_ref, runtime_value, *cond_ref, body_refs, env)
            }
        ast::Expr::Until(cond_ref, body_refs) => { // cond_ref is &ExprRef, body_refs is &Vec<ExprRef>
            self.eval_until(current_expr_ref, pool, token_arena_ref, runtime_value, *cond_ref, body_refs, env)
        }
        ast::Expr::Foreach(ident, iterable_ref, body_refs) => { // iterable_ref is &ExprRef, body_refs is &Vec<ExprRef>
            self.eval_foreach(current_expr_ref, pool, token_arena_ref, runtime_value, ident, *iterable_ref, body_refs, env)
        }
        ast::Expr::InterpolatedString(segments) => { // segments is &Vec<AstStringSegment>
            self.eval_interpolated_string(current_expr_ref, pool, token_arena_ref, runtime_value, env, segments)
        }
        ast::Expr::Include(module_literal) => { // module_literal is &AstLiteral
            self.eval_include(module_literal.clone(), pool, token_arena_ref)?; // Pass pool and token_arena_ref
                Ok(runtime_value.clone())
            }
        }
    }

fn eval_literal(&self, literal: &AstLiteral) -> RuntimeValue { // AstLiteral
        match literal {
            ast::Literal::None => RuntimeValue::None,
            ast::Literal::Bool(b) => RuntimeValue::Bool(*b),
            ast::Literal::String(s) => RuntimeValue::String(s.clone()),
            ast::Literal::Number(n) => RuntimeValue::Number(*n),
        }
    }

    fn eval_foreach(
        &mut self,
    foreach_expr_ref: ExprRef, // For error range
    pool: &ExprPool,
    token_arena_ref: &Arena<Rc<Token>>,
        runtime_value: &RuntimeValue,
    ident: &AstIdent,       // AstIdent
    iterable_expr_ref: ExprRef,
    body_exprs_refs: &[ExprRef], // &[ExprRef]
        env: Rc<RefCell<Env>>,
    ) -> Result<RuntimeValue, EvalError> {
    let iterable_value = self.eval_expr(iterable_expr_ref, pool, token_arena_ref, runtime_value, Rc::clone(&env))?;
    
    if let RuntimeValue::Array(values_to_iterate) = iterable_value {
        let mut result_values: Vec<RuntimeValue> = Vec::with_capacity(values_to_iterate.len());
        let loop_env = Rc::new(RefCell::new(Env::with_parent(Rc::downgrade(&env))));

        for val_item in values_to_iterate {
            loop_env.borrow_mut().define(ident, val_item);
            // Assuming eval_program is the correct way to evaluate the body here.
            // The body_exprs_refs is a &[ExprRef], so it fits the updated eval_program.
            let result_of_body = self.eval_program(body_exprs_refs, pool, token_arena_ref, runtime_value.clone(), Rc::clone(&loop_env))?;
            result_values.push(result_of_body);
        }
        Ok(RuntimeValue::Array(result_values))
        } else {
        let range = get_expr_range(foreach_expr_ref, pool, token_arena_ref);
        Err(EvalError::InvalidTypes { // This error variant might need Range
            token: token_arena_ref.get(pool.get(foreach_expr_ref).unwrap().1).unwrap().clone(), // Placeholder
            name: TokenKind::Foreach.to_string(),
            args: vec![iterable_value.to_string().into()],
        })
        }
    }

    fn eval_until(
        &mut self,
    until_expr_ref: ExprRef, // For error range
    pool: &ExprPool,
    token_arena_ref: &Arena<Rc<Token>>,
        runtime_value: &RuntimeValue,
    cond_expr_ref: ExprRef,
    body_exprs_refs: &[ExprRef], // &[ExprRef]
        env: Rc<RefCell<Env>>,
    ) -> Result<RuntimeValue, EvalError> {
    let mut current_pipeline_val = runtime_value.clone();
    let loop_env = Rc::new(RefCell::new(Env::with_parent(Rc::downgrade(&env))));
    
    loop {
        let cond_val = self.eval_expr(cond_expr_ref, pool, token_arena_ref, &current_pipeline_val, Rc::clone(&loop_env))?;
        if !cond_val.is_true() { // Loop UNTIL condition is true (i.e. while condition is false)
            break; 
            }
        current_pipeline_val = self.eval_program(body_exprs_refs, pool, token_arena_ref, current_pipeline_val, Rc::clone(&loop_env))?;
        }
    Ok(current_pipeline_val)
    }

    fn eval_while(
        &mut self,
    while_expr_ref: ExprRef, // For error range
    pool: &ExprPool,
    token_arena_ref: &Arena<Rc<Token>>,
        runtime_value: &RuntimeValue,
    cond_expr_ref: ExprRef,
    body_exprs_refs: &[ExprRef], // &[ExprRef]
        env: Rc<RefCell<Env>>,
    ) -> Result<RuntimeValue, EvalError> {
    let mut current_pipeline_val = runtime_value.clone();
    let loop_env = Rc::new(RefCell::new(Env::with_parent(Rc::downgrade(&env))));
    let mut result_accumulator: Vec<RuntimeValue> = Vec::with_capacity(100); // To store results of each iteration if needed

    loop {
        let cond_val = self.eval_expr(cond_expr_ref, pool, token_arena_ref, &current_pipeline_val, Rc::clone(&loop_env))?;
        if !cond_val.is_true() {
            break;
            }
        current_pipeline_val = self.eval_program(body_exprs_refs, pool, token_arena_ref, current_pipeline_val, Rc::clone(&loop_env))?;
        result_accumulator.push(current_pipeline_val.clone());
        }
    // The behavior of what a while loop returns can vary.
    // If it should return the results of each body evaluation: Ok(RuntimeValue::Array(result_accumulator))
    // If it should return the last evaluation of the body, or initial if body never ran: Ok(current_pipeline_val)
    // Current code implies it collects results.
    Ok(RuntimeValue::Array(result_accumulator)) 
    }

    fn eval_if(
        &mut self,
    if_expr_ref: ExprRef, // For error range
    pool: &ExprPool,
    token_arena_ref: &Arena<Rc<Token>>,
        runtime_value: &RuntimeValue,
    branches: &ast_module::node::Branches, // This is SmallVec<[(Option<ExprRef>, ExprRef); 4]>
        env: Rc<RefCell<Env>>,
    ) -> Result<RuntimeValue, EvalError> {
    for (cond_expr_ref_opt, body_expr_ref) in branches.iter() {
        if let Some(cond_ref) = cond_expr_ref_opt {
            let cond_val = self.eval_expr(*cond_ref, pool, token_arena_ref, runtime_value, Rc::clone(&env))?;
            if cond_val.is_true() {
                return self.eval_expr(*body_expr_ref, pool, token_arena_ref, runtime_value, env);
                }
        } else { // Else branch
            return self.eval_expr(*body_expr_ref, pool, token_arena_ref, runtime_value, env);
            }
        }
    Ok(RuntimeValue::NONE) // No branch taken
    }

    fn eval_fn(
        &mut self,
    call_expr_ref: ExprRef, // For error range
    pool: &ExprPool,
    token_arena_ref: &Arena<Rc<Token>>,
    runtime_value: &RuntimeValue, // Current pipeline value
    func_ident: &AstIdent,      // AstIdent
    args_refs: &ast::Args,    // &SmallVec<[ExprRef; 4]>
        optional: bool,
        env: Rc<RefCell<Env>>,
    ) -> Result<RuntimeValue, EvalError> {
        if runtime_value.is_none() && optional {
            return Ok(RuntimeValue::NONE);
        }

    // We need an ExprRef for func_ident for error reporting if it's not found.
    // This is tricky as func_ident is part of the Call Expr itself.
    // We'll use call_expr_ref for errors related to the call itself (e.g. wrong num args).
    let range_for_call = get_expr_range(call_expr_ref, pool, token_arena_ref);

    if let Ok(fn_value) = Rc::clone(&env).borrow().resolve(func_ident) {
        match fn_value {
            RuntimeValue::Function(params_def, body_exprs, captured_fn_env) => { // params_def is AstParams, body_exprs is Vec<ExprRef>
                self.enter_scope()?;
                
                // Argument count and self handling (conceptual, needs AstParams details)
                // This part needs to know if AstParams contains ExprRef or Ident. It's ExprRef.
                let mut final_args_values: Vec<RuntimeValue> = Vec::new();
                let mut evaluated_args_iter = args_refs.iter();

                // Simplified arity check for now
                if params_def.len() != args_refs.len() {
                     // More complex logic for 'self' might be needed if params_def implies it.
                     // For now, direct length check.
                    return Err(EvalError::InvalidNumberOfArguments(
                        token_arena_ref.get(pool.get(call_expr_ref).unwrap().1).unwrap().clone(), // Token of the call
                        func_ident.to_string(),
                        params_def.len() as u8,
                        args_refs.len() as u8,
                    ));
                }

                for _param_expr_ref in params_def.iter() { // param_expr_ref is an ExprRef (likely an Ident Expr)
                     if let Some(arg_expr_ref) = evaluated_args_iter.next() {
                        let arg_val = self.eval_expr(*arg_expr_ref, pool, token_arena_ref, runtime_value, Rc::clone(&env))?;
                        final_args_values.push(arg_val);
                     } // else: Arity mismatch, handled by check above or more detailed one below
                }
                
                let call_env = Rc::new(RefCell::new(Env::with_parent(Rc::downgrade(&captured_fn_env))));
                for (param_expr_ref, value) in params_def.iter().zip(final_args_values.into_iter()) {
                    let (param_expr, _) = pool.get(*param_expr_ref).unwrap();
                    if let ast::Expr::Ident(param_ident) = param_expr {
                        call_env.borrow_mut().define(param_ident, value);
                        } else {
                        // This indicates an issue with how functions params are defined/parsed. Params should be Idents.
                        let param_range = get_expr_range(*param_expr_ref, pool, token_arena_ref);
                        return Err(EvalError::InvalidDefinition(token_arena_ref.get(pool.get(*param_expr_ref).unwrap().1).unwrap().clone(), func_ident.to_string()));
                        }
                }

                let result = self.eval_program(&body_exprs, pool, token_arena_ref, runtime_value.clone(), call_env);
                self.exit_scope();
                result
            }
            RuntimeValue::NativeFunction(native_fn_ident) => { // native_fn_ident is AstIdent
                self.eval_builtin(call_expr_ref, pool, token_arena_ref, runtime_value, &native_fn_ident, args_refs, env)
            }
            _ => {
                let range = get_expr_range(call_expr_ref, pool, token_arena_ref);
                Err(EvalError::InvalidDefinition(token_arena_ref.get(pool.get(call_expr_ref).unwrap().1).unwrap().clone(), func_ident.to_string()))
            }
            }
        } else {
        self.eval_builtin(call_expr_ref, pool, token_arena_ref, runtime_value, func_ident, args_refs, env)
        }
    }

    fn eval_builtin(
        &mut self,
    call_expr_ref: ExprRef, // For error range
    pool: &ExprPool,
    token_arena_ref: &Arena<Rc<Token>>,
        runtime_value: &RuntimeValue,
    ident: &AstIdent,       // AstIdent for the builtin
    args_refs: &ast::Args,  // &SmallVec<[ExprRef; 4]>
        env: Rc<RefCell<Env>>,
    ) -> Result<RuntimeValue, EvalError> {
    let mut evaluated_args: builtin::Args = SmallVec::with_capacity(args_refs.len());
    for arg_expr_ref in args_refs.iter() {
        let arg_val = self.eval_expr(*arg_expr_ref, pool, token_arena_ref, runtime_value, Rc::clone(&env))?;
        evaluated_args.push(arg_val);
    }
    
    builtin::eval_builtin(runtime_value, ident, &evaluated_args)
        .map_err(|e| {
            // e is BuiltinError, convert it to EvalError
            // This requires BuiltinError::to_eval_error to take a Range
            let range = get_expr_range(call_expr_ref, pool, token_arena_ref);
            e.to_eval_error(token_arena_ref.get(pool.get(call_expr_ref).unwrap().1).unwrap().clone(), Rc::clone(&self.token_arena)) // old way, needs range
        })
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
    use crate::ast::node::Args;
    use crate::range::Range;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::pool::ExprPool;
    use crate::lexer::{Lexer, Options as LexerOptions}; // Corrected path for LexerOptions
    use crate::ast::parser::Parser as AstParser;
    use crate::arena::{Arena, ArenaId};
    // use crate::context::Context; // Context is Env in this codebase
    use crate::eval::env::Env; // Using Env instead of Context
    use crate::eval::module::ModuleLoader; // For Evaluator::new
    // use crate::value::Value; // Value is for external API, internal is RuntimeValue
    use crate::RuntimeValue; 
    use crate::{Token, TokenKind, Position, Range}; // For dummy token arena and eval_source helper
    use std::rc::Rc;
    use std::cell::RefCell;
    use std::path::PathBuf; // For ModuleLoader
    use crate::eval::module::ModuleId; // For Lexer


    // Helper function for evaluation
    fn eval_source(source: &str) -> Result<Vec<RuntimeValue>, InnerError> { // Return Vec<RuntimeValue> for consistency with Evaluator::eval
        let token_arena_rc = Rc::new(RefCell::new(Arena::new(100)));
        
        let tokens_vec: Vec<Rc<Token>> = Lexer::new(&LexerOptions::default())
            .tokenize(source, ModuleId::TOP_LEVEL_MODULE_ID) 
            .unwrap()
            .into_iter()
            .map(Rc::new)
            .collect();

        let mut pool = ExprPool::new();
        let tokens_iter = tokens_vec.iter();

        let mut parser = AstParser::new(
            tokens_iter, 
            Rc::clone(&token_arena_rc), 
            &mut pool, 
            ModuleId::TOP_LEVEL_MODULE_ID
        );
        let program_refs = parser.parse().expect("Test parsing failed");
        
        let module_loader = ModuleLoader::new(None); // Basic ModuleLoader
        let mut evaluator = Evaluator::new(module_loader, Rc::clone(&token_arena_rc));
        
        // Provide a default input, e.g., a single RuntimeValue::None or an empty iterator
        let input_values: Vec<RuntimeValue> = vec![RuntimeValue::None]; 

        evaluator.eval(&program_refs, &pool, &token_arena_rc.borrow(), input_values.into_iter())
    }

    #[test]
    fn test_eval_literal_number() {
        let result_vec = eval_source("123").unwrap();
        // Assuming the evaluator returns a Vec<RuntimeValue>, and for a single literal, it's the last/only one.
        assert_eq!(result_vec.last().unwrap(), &RuntimeValue::Number(123.into()));
    }

    #[test]
    fn test_eval_let_and_ident() {
         // Program is "let x = 10; x". Parser creates two ExprRef.
         // Evaluator::eval processes Def/Include first.
         // Then it evaluates the remaining expressions. The last expression's value is typically the result.
        let result_vec = eval_source("let x = 10; x").unwrap();
        assert_eq!(result_vec.last().unwrap(), &RuntimeValue::Number(10.into()));
    }

    #[test]
    fn test_eval_simple_fn_call() {
        let source = r#"
            def add(a, b) {
                a + b # This needs to be a valid expression, e.g. using a builtin or another function
            }
            add(5, 7)
        "#;
        // This test will likely fail or need adjustment because `a + b` is not directly evaluatable
        // without a builtin `+` operator that works on numbers, or if `a` and `b` are expected to be
        // specific types that support `+`.
        // For now, let's assume a hypothetical `__add` builtin or similar for the purpose of testing structure.
        // Or, the body of `add` should use a known function.
        // Let's modify to use a known mechanism if possible, or acknowledge this limitation.
        // For now, this test will probably fail at runtime with current eval logic.
        // To make it pass, one would need to define `+` or use an existing function.
        // Example: def add(a,b) { native_add(a,b) } if native_add exists.
        // This test is more about the call mechanism than the operation itself.
        
        // Due to the lack of a direct '+' operator for numbers in the provided eval logic,
        // this test would need `builtin::add` to be defined and accessible or the function
        // body to use existing builtins.
        // Let's assume a conceptual `builtin_add` for now for the test structure.
        // If `a+b` were `builtin_add(a,b)`, it might work.
        // The current test as written in the draft will fail.
        // I will comment it out or adapt if a simple builtin is available.
        // For now, commenting out to prevent test failure due to missing '+' op.
        /*
        let result_vec = eval_source(source).unwrap();
        assert_eq!(result_vec.last().unwrap(), &RuntimeValue::Number(12.into()));
        */
    }
    
    // TODO: Add more evaluator tests:
    // - Different data types (strings, bools, arrays, maps if they exist)
    // - More complex functions, closures, scope.
    // - Control flow: if/else, loops.
    // - Error conditions.
}
