//! Main compilation logic for transforming AST to compiled expressions.

use super::call_stack::DEFAULT_MAX_CALL_STACK_DEPTH;
use super::compiled::{CompiledExpr, CompiledProgram};
use super::constant_fold::ConstantFolder;
use crate::arena::Arena;
use crate::ast::node::{Expr, Node};
use crate::error::runtime::RuntimeError;
use crate::eval::env::Env;
use crate::eval::{Evaluator, builtin};
use crate::{Program, RuntimeValue, Shared, SharedCell, Token, get_token};

/// Compiler for transforming AST nodes into compiled closures.
///
/// The compiler takes AST nodes and produces `CompiledExpr` closures that can
/// be executed more efficiently than tree-walking interpretation.
#[derive(Debug, Clone)]
pub struct Compiler {
    /// Arena for token storage and retrieval
    token_arena: Shared<SharedCell<Arena<Shared<Token>>>>,
    /// Constant folder for compile-time optimizations
    constant_folder: ConstantFolder,
    /// Maximum call stack depth to prevent stack overflow
    max_call_stack_depth: u32,
}

impl Compiler {
    /// Creates a new compiler with the given token arena.
    ///
    /// # Arguments
    ///
    /// * `token_arena` - Arena for storing and retrieving tokens (used for error reporting)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let token_arena = Shared::new(SharedCell::new(Arena::new(1024)));
    /// let compiler = Compiler::new(token_arena);
    /// ```
    pub fn new(token_arena: Shared<SharedCell<Arena<Shared<Token>>>>) -> Self {
        Self {
            token_arena,
            constant_folder: ConstantFolder::new(),
            max_call_stack_depth: DEFAULT_MAX_CALL_STACK_DEPTH,
        }
    }

    /// Compiles a program (sequence of AST nodes) into a compiled program.
    ///
    /// # Arguments
    ///
    /// * `program` - The AST program to compile
    ///
    /// # Returns
    ///
    /// A tuple of:
    /// - Optional index where `nodes` appears in the program
    /// - `CompiledProgram` (vector of compiled expressions) on success, or a `RuntimeError` on failure.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let program = parse(code, token_arena)?;
    /// let (nodes_index, compiled) = compiler.compile_program(&program)?;
    /// ```
    pub fn compile_program(&mut self, program: &Program) -> Result<(Option<usize>, CompiledProgram), RuntimeError> {
        let nodes_index = program.iter().position(|node| node.is_nodes());
        let compiled = program
            .iter()
            .map(|node| self.compile_node(node))
            .collect::<Result<_, _>>()?;
        Ok((nodes_index, compiled))
    }

    /// Compiles a single AST node into a compiled expression.
    ///
    /// # Arguments
    ///
    /// * `node` - The AST node to compile
    ///
    /// # Returns
    ///
    /// A `CompiledExpr` on success, or a `RuntimeError` on failure.
    fn compile_node(&mut self, node: &Shared<Node>) -> Result<CompiledExpr, RuntimeError> {
        let token_id = node.token_id;
        let token_arena = Shared::clone(&self.token_arena);

        // Try constant folding first (for literals)
        if let Expr::Literal(literal) = &*node.expr {
            let const_val = self.constant_folder.fold_literal(literal);
            return Ok(Box::new(move |_input, _stack, _env| Ok(const_val.clone())));
        }

        // Compile based on expression type
        match &*node.expr {
            // Self and Nodes: return the input value unchanged (identity function)
            Expr::Self_ | Expr::Nodes => Ok(Box::new(|input, _stack, _env| Ok(input))),

            // Identifier: resolve variable from environment
            Expr::Ident(ident) => {
                let ident_name = ident.name;
                Ok(Box::new(move |_input, _stack, env: Shared<SharedCell<_>>| {
                    #[cfg(not(feature = "sync"))]
                    let result = env.borrow().resolve(ident_name);
                    #[cfg(feature = "sync")]
                    let result = env.read().unwrap().resolve(ident_name);

                    result.map_err(|e| e.to_runtime_error(token_id, Shared::clone(&token_arena)))
                }))
            }

            // Selector: filter markdown nodes by selector
            Expr::Selector(selector) => {
                let selector_clone = selector.clone();
                Ok(Box::new(move |input, _stack, _env| {
                    Ok(Evaluator::<crate::LocalFsModuleResolver>::eval_selector_expr(
                        &input,
                        &selector_clone,
                    ))
                }))
            }

            // InterpolatedString: build string from segments
            Expr::InterpolatedString(segments) => {
                use crate::ast::node::StringSegment;

                // Compile expression segments
                let compiled_segments: Result<Vec<(u8, String, Option<CompiledExpr>)>, RuntimeError> = segments
                    .iter()
                    .map(|segment| -> Result<(u8, String, Option<CompiledExpr>), RuntimeError> {
                        match segment {
                            StringSegment::Text(s) => Ok((0, s.clone(), None)),
                            StringSegment::Expr(expr_node) => {
                                let compiled = self.compile_node(expr_node)?;
                                Ok((1, String::new(), Some(compiled)))
                            }
                            StringSegment::Env(env_var) => Ok((2, env_var.to_string(), None)),
                            StringSegment::Self_ => Ok((3, String::new(), None)),
                        }
                    })
                    .collect();
                let compiled_segments = compiled_segments?;

                Ok(Box::new(move |input, stack, env| {
                    let estimated_capacity = compiled_segments.len() * 32;
                    compiled_segments
                        .iter()
                        .try_fold(
                            String::with_capacity(estimated_capacity),
                            |mut acc, (seg_type, text, compiled_opt)| {
                                match seg_type {
                                    0 => acc.push_str(text), // Text
                                    1 => {
                                        // Expr
                                        let value = compiled_opt.as_ref().unwrap()(input.clone(), stack, env.clone())?;
                                        acc.push_str(&value.to_string());
                                    }
                                    2 => {
                                        // Env
                                        if let Ok(val) = std::env::var(text) {
                                            acc.push_str(&val);
                                        } else {
                                            return Err(RuntimeError::EnvNotFound(
                                                (*get_token(Shared::clone(&token_arena), token_id)).clone(),
                                                text.clone().into(),
                                            ));
                                        }
                                    }
                                    3 => acc.push_str(&input.to_string()), // Self
                                    _ => {}
                                }
                                Ok(acc)
                            },
                        )
                        .map(RuntimeValue::String)
                }))
            }

            // And expression: short-circuit evaluation
            Expr::And(left, right) => {
                let left_compiled = self.compile_node(left)?;
                let right_compiled = self.compile_node(right)?;

                Ok(Box::new(move |input, stack, env| {
                    let left_val = left_compiled(input.clone(), stack, env.clone())?;
                    if !left_val.is_truthy() {
                        return Ok(RuntimeValue::Boolean(false));
                    }
                    let right_val = right_compiled(input, stack, env)?;
                    if !right_val.is_truthy() {
                        return Ok(RuntimeValue::Boolean(false));
                    }
                    Ok(right_val)
                }))
            }

            // Or expression: short-circuit evaluation
            Expr::Or(left, right) => {
                let left_compiled = self.compile_node(left)?;
                let right_compiled = self.compile_node(right)?;

                Ok(Box::new(move |input, stack, env| {
                    let left_val = left_compiled(input.clone(), stack, env.clone())?;
                    if left_val.is_truthy() {
                        return Ok(left_val);
                    }
                    let right_val = right_compiled(input, stack, env)?;
                    Ok(right_val)
                }))
            }

            // If expression: conditional branches
            Expr::If(branches) => {
                let compiled_branches: Result<Vec<(Option<CompiledExpr>, CompiledExpr)>, RuntimeError> = branches
                    .iter()
                    .map(|(cond_opt, body)| {
                        let cond_compiled = match cond_opt {
                            Some(cond) => Some(self.compile_node(cond)?),
                            None => None,
                        };
                        let body_compiled = self.compile_node(body)?;
                        Ok((cond_compiled, body_compiled))
                    })
                    .collect();
                let compiled_branches = compiled_branches?;

                Ok(Box::new(move |input, stack, env| {
                    for (cond_opt, body) in &compiled_branches {
                        match cond_opt {
                            Some(cond) => {
                                let cond_result = cond(input.clone(), stack, env.clone())?;
                                if cond_result.is_truthy() {
                                    return body(input, stack, env);
                                }
                            }
                            None => return body(input, stack, env),
                        }
                    }
                    Ok(RuntimeValue::None)
                }))
            }

            // Break: return special error
            Expr::Break => Ok(Box::new(|_input, _stack, _env| Err(RuntimeError::Break))),

            // Continue: return special error
            Expr::Continue => Ok(Box::new(|_input, _stack, _env| Err(RuntimeError::Continue))),

            // While loop
            Expr::While(cond, body) => {
                let cond_compiled = self.compile_node(cond)?;
                let (_nodes_idx, body_compiled) = self.compile_program(body)?;

                Ok(Box::new(move |mut input, stack, env| {
                    let loop_env = Shared::new(SharedCell::new(Env::with_parent(Shared::downgrade(&env))));

                    let mut cond_value = cond_compiled(input.clone(), stack, loop_env.clone())?;
                    if !cond_value.is_truthy() {
                        return Ok(RuntimeValue::None);
                    }

                    let mut first = true;
                    while cond_value.is_truthy() {
                        let mut value = input.clone();
                        for expr in &body_compiled {
                            match expr(value.clone(), stack, loop_env.clone()) {
                                Ok(new_val) => value = new_val,
                                Err(RuntimeError::Break) if first => {
                                    return Ok(RuntimeValue::None);
                                }
                                Err(RuntimeError::Break) => return Ok(input),
                                Err(RuntimeError::Continue) if first => {
                                    input = RuntimeValue::None;
                                    break;
                                }
                                Err(RuntimeError::Continue) => break,
                                Err(e) => return Err(e),
                            }
                        }
                        if !matches!(value, RuntimeValue::None) || !first {
                            input = value;
                        }
                        cond_value = cond_compiled(input.clone(), stack, loop_env.clone())?;
                        first = false;
                    }

                    Ok(input)
                }))
            }

            // Foreach loop
            Expr::Foreach(ident, values_node, body) => {
                let ident_name = ident.name;
                let values_compiled = self.compile_node(values_node)?;
                let (_nodes_idx, body_compiled) = self.compile_program(body)?;

                Ok(Box::new(move |input, stack, env| {
                    let values = values_compiled(input, stack, env.clone())?;

                    match values {
                        RuntimeValue::Array(arr) => {
                            let loop_env = Shared::new(SharedCell::new(Env::with_parent(Shared::downgrade(&env))));
                            let mut results = Vec::with_capacity(arr.len());

                            for value in arr {
                                #[cfg(not(feature = "sync"))]
                                loop_env.borrow_mut().define(ident_name, value.clone());
                                #[cfg(feature = "sync")]
                                loop_env.write().unwrap().define(ident_name, value.clone());

                                let mut loop_value = value;
                                let mut should_break = false;
                                let mut should_continue = false;
                                for expr in &body_compiled {
                                    match expr(loop_value.clone(), stack, loop_env.clone()) {
                                        Ok(new_val) => loop_value = new_val,
                                        Err(RuntimeError::Break) => {
                                            should_break = true;
                                            break;
                                        }
                                        Err(RuntimeError::Continue) => {
                                            should_continue = true;
                                            break;
                                        }
                                        Err(e) => return Err(e),
                                    }
                                }
                                if should_break {
                                    return Ok(RuntimeValue::Array(results));
                                }
                                if !should_continue {
                                    results.push(loop_value);
                                }
                            }

                            Ok(RuntimeValue::Array(results))
                        }
                        RuntimeValue::String(s) => {
                            let loop_env = Shared::new(SharedCell::new(Env::with_parent(Shared::downgrade(&env))));
                            let mut results = Vec::with_capacity(s.len());

                            for c in s.chars() {
                                let char_val = RuntimeValue::String(c.to_string());
                                #[cfg(not(feature = "sync"))]
                                loop_env.borrow_mut().define(ident_name, char_val.clone());
                                #[cfg(feature = "sync")]
                                loop_env.write().unwrap().define(ident_name, char_val.clone());

                                let mut loop_value = char_val;
                                let mut should_break = false;
                                let mut should_continue = false;
                                for expr in &body_compiled {
                                    match expr(loop_value.clone(), stack, loop_env.clone()) {
                                        Ok(new_val) => loop_value = new_val,
                                        Err(RuntimeError::Break) => {
                                            should_break = true;
                                            break;
                                        }
                                        Err(RuntimeError::Continue) => {
                                            should_continue = true;
                                            break;
                                        }
                                        Err(e) => return Err(e),
                                    }
                                }
                                if should_break {
                                    return Ok(RuntimeValue::Array(results));
                                }
                                if !should_continue {
                                    results.push(loop_value);
                                }
                            }

                            Ok(RuntimeValue::Array(results))
                        }
                        _ => {
                            let token = get_token(Shared::clone(&token_arena), token_id);
                            Err(RuntimeError::InvalidTypes {
                                token: (*token).clone(),
                                name: crate::TokenKind::Foreach.to_string(),
                                args: vec![values.to_string().into()],
                            })
                        }
                    }
                }))
            }

            // Try expression: error handling
            Expr::Try(try_expr, catch_expr) => {
                let try_compiled = self.compile_node(try_expr)?;
                let catch_compiled = self.compile_node(catch_expr)?;

                Ok(Box::new(move |input, stack, env| {
                    match try_compiled(input.clone(), stack, env.clone()) {
                        Ok(result) => Ok(result),
                        Err(_) => catch_compiled(input, stack, env),
                    }
                }))
            }

            // Def: function definition
            Expr::Def(ident, params, program) => {
                let ident_name = ident.name;
                let params_clone = params.clone();
                let program_clone = program.clone();

                Ok(Box::new(move |_input, _stack, env| {
                    let function = RuntimeValue::Function(params_clone.clone(), program_clone.clone(), env.clone());
                    #[cfg(not(feature = "sync"))]
                    env.borrow_mut().define(ident_name, function.clone());
                    #[cfg(feature = "sync")]
                    env.write().unwrap().define(ident_name, function.clone());
                    Ok(function)
                }))
            }

            // Fn: lambda/anonymous function
            Expr::Fn(params, program) => {
                let params_clone = params.clone();
                let program_clone = program.clone();

                Ok(Box::new(move |_input, _stack, env| {
                    Ok(RuntimeValue::Function(
                        params_clone.clone(),
                        program_clone.clone(),
                        env.clone(),
                    ))
                }))
            }

            // Let: immutable variable binding
            Expr::Let(ident, value_node) => {
                let ident_name = ident.name;
                let value_compiled = self.compile_node(value_node)?;

                Ok(Box::new(move |input, stack, env| {
                    let val = value_compiled(input.clone(), stack, env.clone())?;
                    #[cfg(not(feature = "sync"))]
                    env.borrow_mut().define(ident_name, val);
                    #[cfg(feature = "sync")]
                    env.write().unwrap().define(ident_name, val);
                    Ok(input)
                }))
            }

            // Var: mutable variable binding
            Expr::Var(ident, value_node) => {
                let ident_name = ident.name;
                let value_compiled = self.compile_node(value_node)?;

                Ok(Box::new(move |input, stack, env| {
                    let val = value_compiled(input.clone(), stack, env.clone())?;
                    #[cfg(not(feature = "sync"))]
                    env.borrow_mut().define_mutable(ident_name, val);
                    #[cfg(feature = "sync")]
                    env.write().unwrap().define_mutable(ident_name, val);
                    Ok(input)
                }))
            }

            // Assign: assignment to mutable variable
            Expr::Assign(ident, value_node) => {
                let ident_name = ident.name;
                let value_compiled = self.compile_node(value_node)?;

                Ok(Box::new(move |input, stack, env| {
                    let val = value_compiled(input.clone(), stack, env.clone())?;
                    #[cfg(not(feature = "sync"))]
                    let result = env.borrow_mut().assign(ident_name, val);
                    #[cfg(feature = "sync")]
                    let result = env.write().unwrap().assign(ident_name, val);

                    result.map_err(|e| e.to_runtime_error(token_id, Shared::clone(&token_arena)))?;
                    Ok(input)
                }))
            }

            // Block: scoped expression
            Expr::Block(program) => {
                let (_nodes_idx, body_compiled) = self.compile_program(program)?;

                Ok(Box::new(move |input, stack, env| {
                    let block_env = Shared::new(SharedCell::new(Env::with_parent(Shared::downgrade(&env))));
                    let mut value = input;
                    for expr in &body_compiled {
                        value = expr(value, stack, block_env.clone())?;
                    }
                    Ok(value)
                }))
            }

            // Paren: just unwrap
            Expr::Paren(expr) => self.compile_node(expr),

            // Call: function call
            Expr::Call(ident, args) => {
                let func_name = ident.name;
                let ident_clone = ident.clone();
                let compiled_args: Result<Vec<CompiledExpr>, RuntimeError> =
                    args.iter().map(|arg| self.compile_node(arg)).collect();
                let compiled_args = compiled_args?;
                let max_depth = self.max_call_stack_depth;
                let token_arena = Shared::clone(&self.token_arena);

                Ok(Box::new(move |input, stack, env| {
                    // Check stack depth
                    if stack.len() >= max_depth as usize {
                        return Err(RuntimeError::RecursionError(max_depth));
                    }

                    // Resolve function from environment
                    #[cfg(not(feature = "sync"))]
                    let fn_value = env.borrow().resolve(func_name);
                    #[cfg(feature = "sync")]
                    let fn_value = env.read().unwrap().resolve(func_name);

                    match fn_value {
                        Ok(RuntimeValue::Function(params, program, fn_env)) => {
                            // User-defined function: tree-walk the body (not compiled yet)
                            stack.push(());
                            let new_env = Shared::new(SharedCell::new(Env::with_parent(Shared::downgrade(&fn_env))));

                            // Evaluate arguments
                            let arg_values: Result<Vec<_>, _> = compiled_args
                                .iter()
                                .map(|arg_expr| arg_expr(input.clone(), stack, env.clone()))
                                .collect();
                            let arg_values = arg_values?;

                            // Bind parameters
                            if params.len() == arg_values.len() + 1 {
                                // First param is implicit input
                                if let Expr::Ident(id) = &*params.first().unwrap().expr {
                                    #[cfg(not(feature = "sync"))]
                                    new_env.borrow_mut().define(id.name, input.clone());
                                    #[cfg(feature = "sync")]
                                    new_env.write().unwrap().define(id.name, input.clone());
                                }
                                for (val, param) in arg_values.iter().zip(params.iter().skip(1)) {
                                    if let Expr::Ident(id) = &*param.expr {
                                        #[cfg(not(feature = "sync"))]
                                        new_env.borrow_mut().define(id.name, val.clone());
                                        #[cfg(feature = "sync")]
                                        new_env.write().unwrap().define(id.name, val.clone());
                                    }
                                }
                            } else if params.len() == arg_values.len() {
                                for (val, param) in arg_values.iter().zip(params.iter()) {
                                    if let Expr::Ident(id) = &*param.expr {
                                        #[cfg(not(feature = "sync"))]
                                        new_env.borrow_mut().define(id.name, val.clone());
                                        #[cfg(feature = "sync")]
                                        new_env.write().unwrap().define(id.name, val.clone());
                                    }
                                }
                            } else {
                                stack.pop();
                                return Err(RuntimeError::InvalidNumberOfArguments(
                                    (*get_token(Shared::clone(&token_arena), token_id)).clone(),
                                    func_name.to_string(),
                                    params.len() as u8,
                                    arg_values.len() as u8,
                                ));
                            }

                            // Compile and execute function body recursively
                            let mut fn_compiler = Compiler::new(Shared::clone(&token_arena));
                            let compile_result = fn_compiler.compile_program(&program);

                            match compile_result {
                                Ok((_nodes_idx, compiled_fn_body)) => {
                                    // Execute compiled function body
                                    let mut value = input;
                                    for expr in &compiled_fn_body {
                                        value = expr(value, stack, new_env.clone())?;
                                    }
                                    stack.pop();
                                    Ok(value)
                                }
                                Err(e) => {
                                    stack.pop();
                                    Err(e)
                                }
                            }
                        }
                        Ok(RuntimeValue::NativeFunction(builtin_name)) => {
                            // Builtin function
                            let arg_values: Result<Vec<_>, _> = compiled_args
                                .iter()
                                .map(|arg_expr| arg_expr(input.clone(), stack, env.clone()))
                                .collect();
                            let arg_values = arg_values?;

                            builtin::eval_builtin(&input, &builtin_name, arg_values, &env).map_err(|e| {
                                let node = Node {
                                    token_id,
                                    expr: Shared::new(Expr::Ident(ident_clone.clone())),
                                };
                                e.to_runtime_error(node, Shared::clone(&token_arena))
                            })
                        }
                        Ok(_) => {
                            let token = get_token(Shared::clone(&token_arena), token_id);
                            Err(RuntimeError::InvalidDefinition((*token).clone(), func_name.to_string()))
                        }
                        Err(_) => {
                            // Try builtin
                            let arg_values: Result<Vec<_>, _> = compiled_args
                                .iter()
                                .map(|arg_expr| arg_expr(input.clone(), stack, env.clone()))
                                .collect();
                            let arg_values = arg_values?;

                            builtin::eval_builtin(&input, &func_name, arg_values, &env).map_err(|e| {
                                let node = Node {
                                    token_id,
                                    expr: Shared::new(Expr::Ident(ident_clone.clone())),
                                };
                                e.to_runtime_error(node, Shared::clone(&token_arena))
                            })
                        }
                    }
                }))
            }

            // CallDynamic: dynamic function call
            Expr::CallDynamic(callable, args) => {
                let callable_compiled = self.compile_node(callable)?;
                let compiled_args: Result<Vec<CompiledExpr>, RuntimeError> =
                    args.iter().map(|arg| self.compile_node(arg)).collect();
                let compiled_args = compiled_args?;
                let max_depth = self.max_call_stack_depth;
                let token_arena = Shared::clone(&self.token_arena);

                Ok(Box::new(move |input, stack, env| {
                    if stack.len() >= max_depth as usize {
                        return Err(RuntimeError::RecursionError(max_depth));
                    }

                    let fn_value = callable_compiled(input.clone(), stack, env.clone())?;

                    match fn_value {
                        RuntimeValue::Function(params, program, fn_env) => {
                            stack.push(());
                            let new_env = Shared::new(SharedCell::new(Env::with_parent(Shared::downgrade(&fn_env))));

                            let arg_values: Result<Vec<_>, _> = compiled_args
                                .iter()
                                .map(|arg_expr| arg_expr(input.clone(), stack, env.clone()))
                                .collect();
                            let arg_values = arg_values?;

                            if params.len() == arg_values.len() + 1 {
                                if let Expr::Ident(id) = &*params.first().unwrap().expr {
                                    #[cfg(not(feature = "sync"))]
                                    new_env.borrow_mut().define(id.name, input.clone());
                                    #[cfg(feature = "sync")]
                                    new_env.write().unwrap().define(id.name, input.clone());
                                }
                                for (val, param) in arg_values.iter().zip(params.iter().skip(1)) {
                                    if let Expr::Ident(id) = &*param.expr {
                                        #[cfg(not(feature = "sync"))]
                                        new_env.borrow_mut().define(id.name, val.clone());
                                        #[cfg(feature = "sync")]
                                        new_env.write().unwrap().define(id.name, val.clone());
                                    }
                                }
                            } else if params.len() == arg_values.len() {
                                for (val, param) in arg_values.iter().zip(params.iter()) {
                                    if let Expr::Ident(id) = &*param.expr {
                                        #[cfg(not(feature = "sync"))]
                                        new_env.borrow_mut().define(id.name, val.clone());
                                        #[cfg(feature = "sync")]
                                        new_env.write().unwrap().define(id.name, val.clone());
                                    }
                                }
                            } else {
                                stack.pop();
                                return Err(RuntimeError::InvalidNumberOfArguments(
                                    (*get_token(Shared::clone(&token_arena), token_id)).clone(),
                                    "<dynamic>".to_string(),
                                    params.len() as u8,
                                    arg_values.len() as u8,
                                ));
                            }

                            // Compile and execute function body recursively
                            let mut fn_compiler = Compiler::new(Shared::clone(&token_arena));
                            let compile_result = fn_compiler.compile_program(&program);

                            match compile_result {
                                Ok((_nodes_idx, compiled_fn_body)) => {
                                    // Execute compiled function body
                                    let mut value = input;
                                    for expr in &compiled_fn_body {
                                        value = expr(value, stack, new_env.clone())?;
                                    }
                                    stack.pop();
                                    Ok(value)
                                }
                                Err(e) => {
                                    stack.pop();
                                    Err(e)
                                }
                            }
                        }
                        _ => {
                            let token = get_token(Shared::clone(&token_arena), token_id);
                            Err(RuntimeError::InvalidDefinition(
                                (*token).clone(),
                                "<dynamic>".to_string(),
                            ))
                        }
                    }
                }))
            }

            // Literal: already handled by constant folding above
            Expr::Literal(_) => {
                unreachable!("Literals should be handled by constant folding")
            }

            // Match: fallback to tree-walking evaluator (uses input value)
            Expr::Match(_, _) => {
                let node_clone = Shared::clone(node);
                let token_arena_clone = Shared::clone(&self.token_arena);

                Ok(Box::new(move |input, _stack, env| {
                    use crate::LocalFsModuleResolver;
                    let mut evaluator: Evaluator<LocalFsModuleResolver> =
                        Evaluator::with_env(Shared::clone(&token_arena_clone), env.clone());
                    evaluator.eval_expr(&input, &node_clone, &env)
                }))
            }

            // QualifiedAccess: fallback to tree-walking evaluator
            // Module path resolution ignores input, but function calls use the pipeline input
            Expr::QualifiedAccess(_, _) => {
                let node_clone = Shared::clone(node);
                let token_arena_clone = Shared::clone(&self.token_arena);

                Ok(Box::new(move |input, _stack, env| {
                    use crate::LocalFsModuleResolver;
                    let mut evaluator: Evaluator<LocalFsModuleResolver> =
                        Evaluator::with_env(Shared::clone(&token_arena_clone), env.clone());
                    // Pass input for function calls within qualified access
                    evaluator.eval_expr(&input, &node_clone, &env)
                }))
            }

            // Module/Import/Include: fallback to tree-walking evaluator
            // These expressions completely ignore the pipeline input
            Expr::Module(_, _) | Expr::Include(_) | Expr::Import(_) => {
                let node_clone = Shared::clone(node);
                let token_arena_clone = Shared::clone(&self.token_arena);

                Ok(Box::new(move |_input, _stack, env| {
                    use crate::LocalFsModuleResolver;
                    let mut evaluator: Evaluator<LocalFsModuleResolver> =
                        Evaluator::with_env(Shared::clone(&token_arena_clone), env.clone());
                    // Use RuntimeValue::None as input since these expressions ignore pipeline input
                    evaluator.eval_expr(&RuntimeValue::None, &node_clone, &env)
                }))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::env::Env;
    use crate::number::Number;
    use crate::parse;
    use crate::token_alloc;

    fn create_test_compiler() -> Compiler {
        let token_arena = Shared::new(SharedCell::new(Arena::new(1024)));
        token_alloc(
            &token_arena,
            &Shared::new(Token {
                kind: crate::TokenKind::Eof,
                range: crate::range::Range::default(),
                module_id: crate::arena::ArenaId::new(0),
            }),
        );
        Compiler::new(token_arena)
    }

    #[test]
    fn test_compile_literal() {
        let mut compiler = create_test_compiler();
        let token_arena = Shared::clone(&compiler.token_arena);
        let program = parse("42", token_arena).unwrap();
        let (_nodes_idx, compiled) = compiler.compile_program(&program).unwrap();

        assert_eq!(compiled.len(), 1);

        let mut stack = vec![];
        let env = Shared::new(SharedCell::new(Env::default()));
        let result = compiled[0](RuntimeValue::None, &mut stack, env).unwrap();

        assert_eq!(result, RuntimeValue::Number(Number::from(42)));
    }

    #[test]
    fn test_compile_self() {
        let mut compiler = create_test_compiler();
        let token_arena = Shared::clone(&compiler.token_arena);
        let program = parse("self", token_arena).unwrap();
        let (_nodes_idx, compiled) = compiler.compile_program(&program).unwrap();

        assert_eq!(compiled.len(), 1);

        let mut stack = vec![];
        let env = Shared::new(SharedCell::new(Env::default()));
        let input = RuntimeValue::String("hello".to_string());
        let result = compiled[0](input.clone(), &mut stack, env).unwrap();

        assert_eq!(result, input);
    }

    #[test]
    fn test_compile_string_literal() {
        let mut compiler = create_test_compiler();
        let token_arena = Shared::clone(&compiler.token_arena);
        let program = parse("\"hello world\"", token_arena).unwrap();
        let (_nodes_idx, compiled) = compiler.compile_program(&program).unwrap();

        assert_eq!(compiled.len(), 1);

        let mut stack = vec![];
        let env = Shared::new(SharedCell::new(Env::default()));
        let result = compiled[0](RuntimeValue::None, &mut stack, env).unwrap();

        assert_eq!(result, RuntimeValue::String("hello world".to_string()));
    }

    #[test]
    fn test_compiler_tree_walker_equivalence() {
        use crate::DefaultEngine;

        let test_cases = vec![
            // Literals
            ("42", vec![RuntimeValue::None]),
            ("\"hello\"", vec![RuntimeValue::None]),
            ("true", vec![RuntimeValue::None]),
            // Self
            ("self", vec![RuntimeValue::String("test".to_string())]),
            // Arithmetic with builtins
            ("add(\" world\")", vec![RuntimeValue::String("hello".to_string())]),
            // Control flow
            ("if true: \"yes\" else: \"no\"", vec![RuntimeValue::None]),
            ("if false: \"yes\" else: \"no\"", vec![RuntimeValue::None]),
            // And/Or
            ("true and true", vec![RuntimeValue::None]),
            ("false or true", vec![RuntimeValue::None]),
            // Variables
            ("let x = 42; x", vec![RuntimeValue::None]),
            // Functions
            ("def foo(x): x; foo(42)", vec![RuntimeValue::None]),
            // Loops
            ("let i = 0; while i < 3: let i = add(i, 1); i", vec![RuntimeValue::None]),
        ];

        for (code, inputs) in test_cases {
            // Run with tree-walker
            let mut engine_tw = DefaultEngine::default();
            engine_tw.set_use_compiler(false);
            let result_tw = engine_tw.eval(code, inputs.clone().into_iter());

            // Run with compiler
            let mut engine_comp = DefaultEngine::default();
            engine_comp.set_use_compiler(true);
            let result_comp = engine_comp.eval(code, inputs.into_iter());

            // Compare results
            assert_eq!(
                result_tw.is_ok(),
                result_comp.is_ok(),
                "Code: {} - Both should succeed or both should fail",
                code
            );

            if result_tw.is_ok() {
                let tw_values = result_tw.unwrap_or_default().values().clone();
                let comp_values = result_comp.unwrap().values().clone();
                assert_eq!(tw_values, comp_values, "Code: {} - Results should be identical", code);
            }
        }
    }
}
