//! OpTree evaluator - executes flattened AST instructions.
//!
//! This module provides the execution engine for OpTree instructions.
//! It mirrors the functionality of the recursive evaluator but operates
//! on the flattened instruction representation.

use super::{AccessTarget, MatchArm, Op, OpPool, OpRef, SourceMap, StringSegment};
use crate::{
    Ident, LocalFsModuleResolver, ModuleResolver, Shared, SharedCell, Token,
    arena::Arena,
    ast::node::{Expr, IdentWithToken, Literal, Node, Pattern},
    error::runtime::RuntimeError,
    eval::{
        env::{Env, EnvError},
        runtime_value::{ModuleEnv, RuntimeValue},
    },
    get_token,
    module::{self, ModuleLoader},
};

#[cfg(feature = "debugger")]
use crate::Debugger;

/// OpTree evaluator - executes flattened AST instructions.
///
/// OpTreeEvaluator maintains execution state and evaluates OpTree instructions
/// by dispatching on Op variants and recursively evaluating child OpRefs.
///
/// # Example
///
/// ```rust,ignore
/// let mut evaluator = OpTreeEvaluator::new(
///     pool,
///     source_map,
///     env,
///     token_arena,
///     module_loader,
///     max_call_stack_depth,
/// );
///
/// let result = evaluator.eval(root, input_value)?;
/// ```
pub struct OpTreeEvaluator<T: ModuleResolver = LocalFsModuleResolver> {
    pool: OpPool,
    source_map: SourceMap,
    env: Shared<SharedCell<Env>>,
    token_arena: Shared<SharedCell<Arena<Shared<Token>>>>,
    call_stack_depth: u32,
    max_call_stack_depth: u32,
    module_loader: ModuleLoader<T>,

    #[cfg(feature = "debugger")]
    _debugger: Shared<SharedCell<Debugger>>,
}

impl<T: ModuleResolver> OpTreeEvaluator<T> {
    /// Creates a new OpTree evaluator.
    #[allow(unused)]
    pub fn new(
        pool: OpPool,
        source_map: SourceMap,
        env: Shared<SharedCell<Env>>,
        token_arena: Shared<SharedCell<Arena<Shared<Token>>>>,
        module_loader: ModuleLoader<T>,
        max_call_stack_depth: u32,
    ) -> Self {
        Self {
            pool,
            source_map,
            env,
            token_arena,
            call_stack_depth: 0,
            max_call_stack_depth,
            module_loader,
            #[cfg(feature = "debugger")]
            _debugger: Shared::new(SharedCell::new(Debugger::new())),
        }
    }

    /// Evaluates an OpTree instruction with the given runtime value.
    pub fn eval_single(&mut self, root: OpRef, runtime_value: RuntimeValue) -> Result<RuntimeValue, RuntimeError> {
        self.eval_op(root, &runtime_value, &Shared::clone(&self.env))
    }

    /// Evaluates an OpTree with multiple input values, handling nodes keyword.
    pub fn eval<I>(&mut self, root: OpRef, input: I) -> Result<Vec<RuntimeValue>, RuntimeError>
    where
        I: Iterator<Item = RuntimeValue>,
    {
        // Check if the OpTree contains a Nodes operation
        let nodes_info = self.find_nodes_in_sequence(root);

        if let Some((before_nodes, after_nodes_ops)) = nodes_info {
            // Split evaluation: before nodes and after nodes
            let values: Result<Vec<RuntimeValue>, RuntimeError> = input
                .map(|runtime_value| {
                    match &runtime_value {
                        RuntimeValue::Markdown(node, _) if !before_nodes.is_empty() => {
                            // For markdown nodes, map over child nodes
                            self.eval_markdown_node(&before_nodes, node)
                        }
                        _ if !before_nodes.is_empty() => {
                            // Evaluate the sequence before nodes
                            self.eval_sequence(&before_nodes, runtime_value)
                        }
                        _ => Ok(runtime_value),
                    }
                })
                .collect();

            let collected_values = values?;

            // If there are operations after nodes, evaluate them with the collected array
            if !after_nodes_ops.is_empty() {
                let array_value = RuntimeValue::Array(collected_values);
                let result = self.eval_sequence(&after_nodes_ops, array_value)?;

                // Return as array if not already
                if let RuntimeValue::Array(values) = result {
                    Ok(values)
                } else {
                    Ok(vec![result])
                }
            } else {
                Ok(collected_values)
            }
        } else {
            // No nodes keyword, evaluate each input independently
            input
                .map(|runtime_value| match &runtime_value {
                    RuntimeValue::Markdown(node, _) => self.eval_markdown_node_single(root, node),
                    _ => self.eval_op(root, &runtime_value, &Shared::clone(&self.env)),
                })
                .collect()
        }
    }

    /// Evaluates operations on a markdown node by mapping over its children.
    fn eval_markdown_node(&mut self, ops: &[OpRef], node: &mq_markdown::Node) -> Result<RuntimeValue, RuntimeError> {
        let result_node = node
            .map_values(&mut |child_node| {
                let value = self.eval_sequence(ops, RuntimeValue::Markdown(child_node.clone(), None))?;

                Ok(match value {
                    RuntimeValue::None => child_node.to_fragment(),
                    RuntimeValue::Function(_, _, _)
                    | RuntimeValue::OpTreeFunction { .. }
                    | RuntimeValue::NativeFunction(_)
                    | RuntimeValue::Module(_) => mq_markdown::Node::Empty,
                    RuntimeValue::Array(_)
                    | RuntimeValue::Dict(_)
                    | RuntimeValue::Boolean(_)
                    | RuntimeValue::Number(_)
                    | RuntimeValue::String(_) => value.to_string().into(),
                    RuntimeValue::Symbol(i) => i.as_str().into(),
                    RuntimeValue::Markdown(node, _) => node,
                })
            })
            .map_err(|e| match e {
                crate::error::InnerError::Runtime(r) => r,
                _ => RuntimeError::InternalError((*self.get_token(OpRef::new(0))).clone()),
            })?;

        Ok(RuntimeValue::Markdown(result_node, None))
    }

    /// Evaluates a single operation on a markdown node.
    fn eval_markdown_node_single(
        &mut self,
        op_ref: OpRef,
        node: &mq_markdown::Node,
    ) -> Result<RuntimeValue, RuntimeError> {
        self.eval_markdown_node(&[op_ref], node)
    }

    /// Finds the Nodes operation in a Sequence and splits it.
    /// Returns (operations_before_nodes, operations_after_nodes)
    fn find_nodes_in_sequence(&self, op_ref: OpRef) -> Option<(Vec<OpRef>, Vec<OpRef>)> {
        if let Op::Sequence(ops) = self.pool.get(op_ref).as_ref() {
            // Find the position of Nodes in the sequence
            if let Some(nodes_pos) = ops
                .iter()
                .position(|&op| matches!(self.pool.get(op).as_ref(), Op::Nodes))
            {
                let before_nodes: Vec<OpRef> = ops[..nodes_pos].to_vec();
                let after_nodes: Vec<OpRef> = ops[nodes_pos + 1..].to_vec();

                return Some((before_nodes, after_nodes));
            }
        }

        None
    }

    /// Evaluates a sequence of operations
    fn eval_sequence(&mut self, ops: &[OpRef], runtime_value: RuntimeValue) -> Result<RuntimeValue, RuntimeError> {
        let mut value = runtime_value;
        for &op in ops {
            value = self.eval_op(op, &value, &Shared::clone(&self.env))?;
        }
        Ok(value)
    }

    /// Core evaluation method - dispatches on Op variant.
    fn eval_op(
        &mut self,
        op_ref: OpRef,
        runtime_value: &RuntimeValue,
        env: &Shared<SharedCell<Env>>,
    ) -> Result<RuntimeValue, RuntimeError> {
        // Get the operation from the pool
        let op = Shared::clone(self.pool.get(op_ref));

        #[cfg(feature = "debugger")]
        self.check_breakpoint(op_ref, &runtime_value, env);

        match op.as_ref() {
            // ===== Literals & Values =====
            Op::Literal(lit) => self.eval_literal(lit),

            Op::Ident(ident) => self.eval_ident(*ident, op_ref, env),

            Op::Self_ | Op::Nodes => Ok(runtime_value.clone()),

            // ===== Variables =====
            Op::Let { name, value } => {
                let val = self.eval_op(*value, runtime_value, env)?;
                self.define(env, *name, val);
                Ok(runtime_value.clone())
            }

            Op::Var { name, value } => {
                let val = self.eval_op(*value, runtime_value, env)?;
                self.define_mutable(env, *name, val);
                Ok(runtime_value.clone())
            }

            Op::Assign { name, value } => {
                let val = self.eval_op(*value, runtime_value, env)?;
                self.assign(env, *name, val, op_ref)?;
                Ok(runtime_value.clone())
            }

            // ===== Control Flow =====
            Op::If { branches } => self.eval_if(runtime_value, branches, env),

            Op::While { condition, body } => self.eval_while(runtime_value, *condition, *body, env),

            Op::Foreach { name, iterator, body } => self.eval_foreach(runtime_value, *name, *iterator, *body, env),

            Op::Match { value, arms } => self.eval_match(runtime_value, *value, arms, env),

            Op::Break => Err(RuntimeError::Break),

            Op::Continue => Err(RuntimeError::Continue),

            // ===== Functions =====
            Op::Def { name, params, body } => {
                self.define(
                    env,
                    *name,
                    RuntimeValue::OpTreeFunction {
                        params: params.clone(),
                        body: *body,
                        env: Shared::clone(env),
                    },
                );
                Ok(runtime_value.clone())
            }

            Op::Fn { params, body } => Ok(RuntimeValue::OpTreeFunction {
                params: params.clone(),
                body: *body,
                env: Shared::clone(env),
            }),

            Op::Call { name, args } => self.eval_call(runtime_value, *name, args, env, op_ref),

            Op::CallDynamic { callable, args } => self.eval_call_dynamic(runtime_value, *callable, args, env, op_ref),

            // ===== Blocks & Sequences =====
            Op::Block(body) => {
                let block_env = Shared::new(SharedCell::new(Env::with_parent(Shared::downgrade(env))));
                self.eval_op(*body, runtime_value, &block_env)
            }

            Op::Sequence(ops) => {
                let mut value = runtime_value.clone();
                for &op in ops.iter() {
                    value = self.eval_op(op, &value, env)?;
                }
                Ok(value)
            }

            // ===== Operators =====
            Op::And(left, right) => self.eval_and(runtime_value, *left, *right, env),

            Op::Or(left, right) => self.eval_or(runtime_value, *left, *right, env),

            Op::Paren(expr) => self.eval_op(*expr, runtime_value, env),

            // ===== String Operations =====
            Op::InterpolatedString(segments) => self.eval_interpolated_string(runtime_value, segments, env),

            // ===== Selectors =====
            Op::Selector(selector) => self.eval_selector(runtime_value, selector),

            Op::QualifiedAccess { module_path, target } => {
                self.eval_qualified_access(runtime_value, module_path, target, env, op_ref)
            }

            // ===== Modules =====
            Op::Module { name, body } => {
                let module_env = Shared::new(SharedCell::new(Env::with_parent(Shared::downgrade(env))));
                self.eval_op(*body, &RuntimeValue::None, &module_env)?;

                let module_runtime_value =
                    RuntimeValue::Module(ModuleEnv::new(&name.as_str(), Shared::clone(&module_env)));
                self.define(env, *name, module_runtime_value);
                Ok(runtime_value.clone())
            }

            Op::Include(lit) => {
                self.eval_include(lit.clone(), env)?;
                Ok(runtime_value.clone())
            }

            Op::Import(lit) => {
                self.eval_import(lit.clone(), env)?;
                Ok(runtime_value.clone())
            }

            // ===== Error Handling =====
            Op::Try { try_expr, catch_expr } => match self.eval_op(*try_expr, runtime_value, env) {
                Ok(value) => Ok(value),
                Err(_) => self.eval_op(*catch_expr, runtime_value, env),
            },
        }
    }

    // ===== Helper Methods =====

    fn eval_literal(&self, literal: &Literal) -> Result<RuntimeValue, RuntimeError> {
        Ok(match literal {
            Literal::None => RuntimeValue::None,
            Literal::Bool(b) => RuntimeValue::Boolean(*b),
            Literal::String(s) => RuntimeValue::String(s.clone()),
            Literal::Symbol(i) => RuntimeValue::Symbol(*i),
            Literal::Number(n) => RuntimeValue::Number(*n),
        })
    }

    fn eval_ident(
        &self,
        ident: Ident,
        op_ref: OpRef,
        env: &Shared<SharedCell<Env>>,
    ) -> Result<RuntimeValue, RuntimeError> {
        #[cfg(not(feature = "sync"))]
        let env_borrow = env.borrow();
        #[cfg(feature = "sync")]
        let env_borrow = env.read().unwrap();

        env_borrow
            .resolve(ident)
            .map_err(|_| RuntimeError::UndefinedVariable((*self.get_token(op_ref)).clone(), ident.to_string()))
    }

    fn eval_if(
        &mut self,
        runtime_value: &RuntimeValue,
        branches: &[(Option<OpRef>, OpRef)],
        env: &Shared<SharedCell<Env>>,
    ) -> Result<RuntimeValue, RuntimeError> {
        for (cond, body) in branches {
            if let Some(cond_ref) = cond {
                let cond_value = self.eval_op(*cond_ref, runtime_value, env)?;
                if cond_value.is_truthy() {
                    return self.eval_op(*body, runtime_value, env);
                }
            } else {
                // Else clause
                return self.eval_op(*body, runtime_value, env);
            }
        }
        Ok(RuntimeValue::None)
    }

    fn eval_while(
        &mut self,
        runtime_value: &RuntimeValue,
        condition: OpRef,
        body: OpRef,
        env: &Shared<SharedCell<Env>>,
    ) -> Result<RuntimeValue, RuntimeError> {
        let mut result = runtime_value.clone();
        loop {
            let cond_value = self.eval_op(condition, &result, env)?;
            if !cond_value.is_truthy() {
                break;
            }

            match self.eval_op(body, &result, env) {
                Ok(value) => result = value,
                Err(RuntimeError::Break) => break,
                Err(RuntimeError::Continue) => continue,
                Err(e) => return Err(e),
            }
        }
        Ok(result)
    }

    fn eval_foreach(
        &mut self,
        runtime_value: &RuntimeValue,
        name: Ident,
        iterator: OpRef,
        body: OpRef,
        env: &Shared<SharedCell<Env>>,
    ) -> Result<RuntimeValue, RuntimeError> {
        let iter_value = self.eval_op(iterator, runtime_value, env)?;

        let loop_env = Shared::new(SharedCell::new(Env::with_parent(Shared::downgrade(env))));
        let mut results = Vec::new();

        match iter_value {
            RuntimeValue::Array(items) => {
                for item in items {
                    self.define(&loop_env, name, item.clone());
                    match self.eval_op(body, &item, &loop_env) {
                        Ok(value) => results.push(value),
                        Err(RuntimeError::Break) => break,
                        Err(RuntimeError::Continue) => continue,
                        Err(e) => return Err(e),
                    }
                }
            }
            RuntimeValue::String(s) => {
                for c in s.chars() {
                    let char_value = RuntimeValue::String(c.to_string());
                    self.define(&loop_env, name, char_value.clone());
                    match self.eval_op(body, &char_value, &loop_env) {
                        Ok(value) => results.push(value),
                        Err(RuntimeError::Break) => break,
                        Err(RuntimeError::Continue) => continue,
                        Err(e) => return Err(e),
                    }
                }
            }
            _ => {
                return Err(RuntimeError::InvalidTypes {
                    token: (*self.get_token(iterator)).clone(),
                    name: "foreach".to_string(),
                    args: vec![iter_value.to_string().into()],
                });
            }
        }

        Ok(RuntimeValue::Array(results))
    }

    fn eval_match(
        &mut self,
        runtime_value: &RuntimeValue,
        value: OpRef,
        arms: &[MatchArm],
        env: &Shared<SharedCell<Env>>,
    ) -> Result<RuntimeValue, RuntimeError> {
        let match_value = self.eval_op(value, runtime_value, env)?;

        for arm in arms {
            if let Some(bindings) = self.pattern_match(&arm.pattern, &match_value) {
                // Check guard if present
                if let Some(guard_ref) = arm.guard {
                    let guard_env = Shared::new(SharedCell::new(Env::with_parent(Shared::downgrade(env))));
                    for (name, value) in &bindings {
                        self.define(&guard_env, *name, value.clone());
                    }

                    let guard_result = self.eval_op(guard_ref, runtime_value, &guard_env)?;
                    if !guard_result.is_truthy() {
                        continue;
                    }
                }

                // Execute body with bindings
                let body_env = Shared::new(SharedCell::new(Env::with_parent(Shared::downgrade(env))));
                for (name, value) in bindings {
                    self.define(&body_env, name, value);
                }

                return self.eval_op(arm.body, runtime_value, &body_env);
            }
        }

        Ok(runtime_value.clone())
    }

    fn pattern_match(&self, pattern: &Pattern, value: &RuntimeValue) -> Option<Vec<(Ident, RuntimeValue)>> {
        match pattern {
            Pattern::Wildcard => Some(vec![]),
            Pattern::Ident(ident) => Some(vec![(ident.name, value.clone())]),
            Pattern::Literal(lit) => {
                let pattern_value = match lit {
                    Literal::None => RuntimeValue::None,
                    Literal::Bool(b) => RuntimeValue::Boolean(*b),
                    Literal::String(s) => RuntimeValue::String(s.clone()),
                    Literal::Symbol(i) => RuntimeValue::Symbol(*i),
                    Literal::Number(n) => RuntimeValue::Number(*n),
                };
                if &pattern_value == value { Some(vec![]) } else { None }
            }
            Pattern::Type(type_ident) => {
                let type_name = type_ident.as_str();
                let matches = type_name == "string" && matches!(value, RuntimeValue::String(_))
                    || type_name == "number" && matches!(value, RuntimeValue::Number(_))
                    || type_name == "bool" && matches!(value, RuntimeValue::Boolean(_))
                    || type_name == "array" && matches!(value, RuntimeValue::Array(_))
                    || type_name == "dict" && matches!(value, RuntimeValue::Dict(_))
                    || type_name == "markdown" && matches!(value, RuntimeValue::Markdown(_, _))
                    || type_name == "function"
                        && matches!(
                            value,
                            RuntimeValue::Function(_, _, _)
                                | RuntimeValue::OpTreeFunction { .. }
                                | RuntimeValue::NativeFunction(_)
                        )
                    || type_name == "symbol" && matches!(value, RuntimeValue::Symbol(_))
                    || type_name == "none" && matches!(value, RuntimeValue::None);

                if matches { Some(vec![]) } else { None }
            }
            Pattern::Array(patterns) => {
                if let RuntimeValue::Array(values) = value {
                    if values.len() != patterns.len() {
                        return None;
                    }

                    let mut all_bindings = Vec::new();
                    for (pattern, value) in patterns.iter().zip(values.iter()) {
                        if let Some(bindings) = self.pattern_match(pattern, value) {
                            all_bindings.extend(bindings);
                        } else {
                            return None;
                        }
                    }
                    Some(all_bindings)
                } else {
                    None
                }
            }
            Pattern::ArrayRest(patterns, rest_binding) => {
                if let RuntimeValue::Array(values) = value {
                    if values.len() < patterns.len() {
                        return None;
                    }

                    let mut all_bindings = Vec::new();

                    // Match prefix patterns
                    for (pattern, value) in patterns.iter().zip(values.iter()) {
                        if let Some(bindings) = self.pattern_match(pattern, value) {
                            all_bindings.extend(bindings);
                        } else {
                            return None;
                        }
                    }

                    // Bind rest of array
                    let rest_values = values[patterns.len()..].to_vec();
                    all_bindings.push((rest_binding.name, RuntimeValue::Array(rest_values)));

                    Some(all_bindings)
                } else {
                    None
                }
            }
            Pattern::Dict(field_patterns) => {
                if let RuntimeValue::Dict(dict) = value {
                    let mut all_bindings = Vec::new();

                    for (key, pattern) in field_patterns {
                        if let Some(field_value) = dict.get(&key.name) {
                            if let Some(bindings) = self.pattern_match(pattern, field_value) {
                                all_bindings.extend(bindings);
                            } else {
                                return None;
                            }
                        } else {
                            // Required field is missing
                            return None;
                        }
                    }

                    Some(all_bindings)
                } else {
                    None
                }
            }
        }
    }

    #[inline(always)]
    fn eval_call(
        &mut self,
        runtime_value: &RuntimeValue,
        name: Ident,
        args: &[OpRef],
        env: &Shared<SharedCell<Env>>,
        op_ref: OpRef,
    ) -> Result<RuntimeValue, RuntimeError> {
        // Evaluate arguments
        let arg_values: Result<Vec<_>, _> = args
            .iter()
            .map(|&arg| self.eval_op(arg, runtime_value, env))
            .collect();
        let arg_values = arg_values?;

        // Look up function
        let function = self.eval_ident(name, op_ref, env)?;

        self.call_function(runtime_value, function, arg_values, op_ref)
    }

    #[inline(always)]
    fn eval_call_dynamic(
        &mut self,
        runtime_value: &RuntimeValue,
        callable: OpRef,
        args: &[OpRef],
        env: &Shared<SharedCell<Env>>,
        op_ref: OpRef,
    ) -> Result<RuntimeValue, RuntimeError> {
        // Evaluate callable expression
        let function = self.eval_op(callable, runtime_value, env)?;

        // Evaluate arguments
        let arg_values: Result<Vec<_>, _> = args
            .iter()
            .map(|&arg| self.eval_op(arg, runtime_value, env))
            .collect();
        let arg_values = arg_values?;

        self.call_function(runtime_value, function, arg_values, op_ref)
    }

    fn call_function(
        &mut self,
        runtime_value: &RuntimeValue,
        function: RuntimeValue,
        args: Vec<RuntimeValue>,
        op_ref: OpRef,
    ) -> Result<RuntimeValue, RuntimeError> {
        // Check call stack depth
        if self.call_stack_depth >= self.max_call_stack_depth {
            return Err(RuntimeError::RecursionError(self.max_call_stack_depth));
        }

        self.call_stack_depth += 1;
        let result = match function {
            RuntimeValue::OpTreeFunction {
                params,
                body,
                env: fn_env,
            } => {
                // Create new environment for function call
                let call_env = Shared::new(SharedCell::new(Env::with_parent(Shared::downgrade(&fn_env))));

                // Check if args is one short - if so, prepend current runtime_value
                if params.len() == args.len() + 1 {
                    // First parameter gets the current runtime_value
                    if let Some(&first_param) = params.first()
                        && let Op::Ident(name) = self.pool.get(first_param).as_ref()
                    {
                        self.define(&call_env, *name, runtime_value.clone());
                    }

                    // Remaining parameters get args
                    for (i, &param) in params.iter().skip(1).enumerate() {
                        if let Op::Ident(name) = self.pool.get(param).as_ref() {
                            let value = args.get(i).cloned().unwrap_or(RuntimeValue::None);
                            self.define(&call_env, *name, value);
                        }
                    }
                } else if params.len() == args.len() {
                    // Normal case: bind parameters to arguments
                    for (i, &param) in params.iter().enumerate() {
                        if let Op::Ident(name) = self.pool.get(param).as_ref() {
                            let value = args.get(i).cloned().unwrap_or(RuntimeValue::None);
                            self.define(&call_env, *name, value);
                        }
                    }
                } else {
                    // Argument count mismatch
                    self.call_stack_depth -= 1;
                    return Err(RuntimeError::InvalidNumberOfArguments(
                        (*self.get_token(op_ref)).clone(),
                        "<function>".to_string(),
                        params.len() as u8,
                        args.len() as u8,
                    ));
                }

                // Evaluate function body
                self.eval_op(body, runtime_value, &call_env)
            }
            RuntimeValue::Function(params, program, fn_env) => {
                // Handle AST-based functions (from modules, etc.)
                // Create new environment for function call
                let call_env = Shared::new(SharedCell::new(Env::with_parent(Shared::downgrade(&fn_env))));

                // Check if args is one short - if so, prepend current runtime_value
                if params.len() == args.len() + 1 {
                    // First parameter gets the current runtime_value
                    if let Some(first_param) = params.first()
                        && let Expr::Ident(id) = &*first_param.expr
                    {
                        self.define(&call_env, id.name, runtime_value.clone());
                    }

                    // Remaining parameters get args
                    for (i, param) in params.iter().skip(1).enumerate() {
                        if let Expr::Ident(id) = &*param.expr {
                            let value = args.get(i).cloned().unwrap_or(RuntimeValue::None);
                            self.define(&call_env, id.name, value);
                        }
                    }
                } else if params.len() == args.len() {
                    // Normal case: bind parameters to arguments
                    for (i, param) in params.iter().enumerate() {
                        if let Expr::Ident(id) = &*param.expr {
                            let value = args.get(i).cloned().unwrap_or(RuntimeValue::None);
                            self.define(&call_env, id.name, value);
                        }
                    }
                } else {
                    // Argument count mismatch
                    self.call_stack_depth -= 1;
                    return Err(RuntimeError::InvalidNumberOfArguments(
                        (*self.get_token(op_ref)).clone(),
                        "<function>".to_string(),
                        params.len() as u8,
                        args.len() as u8,
                    ));
                }

                // Use recursive evaluator for AST-based function
                let mut recursive_eval: crate::eval::Evaluator<T> =
                    crate::eval::Evaluator::with_env(Shared::clone(&self.token_arena), Shared::clone(&call_env));
                recursive_eval
                    .eval(&program, std::iter::once(runtime_value.clone()))
                    .map_err(|e| match e {
                        crate::error::InnerError::Runtime(r) => r,
                        _ => RuntimeError::InternalError((*self.get_token(op_ref)).clone()),
                    })
                    .map(|results| results.into_iter().next().unwrap_or(RuntimeValue::None))
            }
            RuntimeValue::NativeFunction(func_name) => {
                // Call builtin function
                crate::eval::builtin::eval_builtin(runtime_value, &func_name, args, &self.env).map_err(|e| {
                    // Create a dummy node for error reporting
                    let token_id = self.source_map.get(op_ref);
                    let node = Node {
                        token_id,
                        expr: Shared::new(Expr::Ident(IdentWithToken::new(&func_name.as_str()))),
                    };
                    e.to_runtime_error(node, Shared::clone(&self.token_arena))
                })
            }
            _ => Err(RuntimeError::Runtime(
                (*self.get_token(op_ref)).clone(),
                "Not callable".to_string(),
            )),
        };
        self.call_stack_depth -= 1;

        result
    }

    #[inline(always)]
    fn eval_and(
        &mut self,
        runtime_value: &RuntimeValue,
        left: OpRef,
        right: OpRef,
        env: &Shared<SharedCell<Env>>,
    ) -> Result<RuntimeValue, RuntimeError> {
        let left_value = self.eval_op(left, runtime_value, env)?;
        if !left_value.is_truthy() {
            return Ok(left_value);
        }
        self.eval_op(right, runtime_value, env)
    }

    #[inline(always)]
    fn eval_or(
        &mut self,
        runtime_value: &RuntimeValue,
        left: OpRef,
        right: OpRef,
        env: &Shared<SharedCell<Env>>,
    ) -> Result<RuntimeValue, RuntimeError> {
        let left_value = self.eval_op(left, runtime_value, env)?;
        if left_value.is_truthy() {
            return Ok(left_value);
        }
        self.eval_op(right, runtime_value, env)
    }

    fn eval_interpolated_string(
        &mut self,
        runtime_value: &RuntimeValue,
        segments: &[StringSegment],
        env: &Shared<SharedCell<Env>>,
    ) -> Result<RuntimeValue, RuntimeError> {
        // Pre-allocate capacity based on segment content for better performance
        let estimated_capacity = segments
            .iter()
            .map(|segment| match segment {
                StringSegment::Text(s) => s.len(),
                StringSegment::Expr(_) => 32, // Estimated size for expression result
                StringSegment::Env(_) => 32,  // Estimated size for environment variable
                StringSegment::Self_ => 64,   // Estimated size for self reference
            })
            .sum();
        let mut result = String::with_capacity(estimated_capacity);

        for segment in segments {
            match segment {
                StringSegment::Text(text) => result.push_str(text),
                StringSegment::Expr(expr_ref) => {
                    let value = self.eval_op(*expr_ref, runtime_value, env)?;
                    result.push_str(&value.to_string());
                }
                StringSegment::Env(env_var) => {
                    if let Ok(value) = std::env::var(env_var.as_str()) {
                        result.push_str(&value);
                    }
                }
                StringSegment::Self_ => {
                    result.push_str(&runtime_value.to_string());
                }
            }
        }

        Ok(RuntimeValue::String(result))
    }

    fn eval_selector(
        &self,
        runtime_value: &RuntimeValue,
        selector: &crate::selector::Selector,
    ) -> Result<RuntimeValue, RuntimeError> {
        match runtime_value {
            RuntimeValue::Markdown(node, _) => {
                if crate::eval::builtin::eval_selector(node, selector) {
                    Ok(runtime_value.clone())
                } else {
                    Ok(RuntimeValue::None)
                }
            }
            RuntimeValue::Array(values) => {
                let filtered: Vec<_> = values
                    .iter()
                    .filter_map(|value| {
                        if let RuntimeValue::Markdown(node, _) = value {
                            if crate::eval::builtin::eval_selector(node, selector) {
                                Some(value.clone())
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    })
                    .collect();
                Ok(RuntimeValue::Array(filtered))
            }
            _ => Ok(RuntimeValue::None),
        }
    }

    fn eval_qualified_access(
        &mut self,
        runtime_value: &RuntimeValue,
        module_path: &[Ident],
        target: &AccessTarget,
        env: &Shared<SharedCell<Env>>,
        op_ref: OpRef,
    ) -> Result<RuntimeValue, RuntimeError> {
        // Navigate through module path
        #[cfg(not(feature = "sync"))]
        let env_borrow = env.borrow();
        #[cfg(feature = "sync")]
        let env_borrow = env.read().unwrap();

        let mut current_value = env_borrow.resolve(module_path[0]).map_err(|_| {
            RuntimeError::UndefinedVariable((*self.get_token(op_ref)).clone(), module_path[0].to_string())
        })?;

        for &module_name in &module_path[1..] {
            if let RuntimeValue::Module(module_env) = current_value {
                #[cfg(not(feature = "sync"))]
                let module_borrow = module_env.exports().borrow();
                #[cfg(feature = "sync")]
                let module_borrow = module_env.exports().read().unwrap();

                current_value = module_borrow.resolve(module_name).map_err(|_| {
                    RuntimeError::UndefinedVariable((*self.get_token(op_ref)).clone(), module_name.to_string())
                })?;
            } else {
                return Err(RuntimeError::Runtime(
                    (*self.get_token(op_ref)).clone(),
                    "Not a module".to_string(),
                ));
            }
        }

        // Access target
        match target {
            AccessTarget::Ident(ident) => {
                if let RuntimeValue::Module(module_env) = current_value {
                    #[cfg(not(feature = "sync"))]
                    let module_borrow = module_env.exports().borrow();
                    #[cfg(feature = "sync")]
                    let module_borrow = module_env.exports().read().unwrap();

                    module_borrow.resolve(*ident).map_err(|_| {
                        RuntimeError::UndefinedVariable((*self.get_token(op_ref)).clone(), ident.to_string())
                    })
                } else {
                    Ok(current_value)
                }
            }
            AccessTarget::Call(func_name, args) => {
                if let RuntimeValue::Module(module_env) = current_value {
                    #[cfg(not(feature = "sync"))]
                    let module_borrow = module_env.exports().borrow();
                    #[cfg(feature = "sync")]
                    let module_borrow = module_env.exports().read().unwrap();

                    let function = module_borrow.resolve(*func_name).map_err(|_| {
                        RuntimeError::UndefinedVariable((*self.get_token(op_ref)).clone(), func_name.to_string())
                    })?;
                    drop(module_borrow);

                    // Evaluate arguments
                    let arg_values: Result<Vec<_>, _> = args
                        .iter()
                        .map(|&arg| self.eval_op(arg, runtime_value, env))
                        .collect();
                    let arg_values = arg_values?;

                    self.call_function(runtime_value, function, arg_values, op_ref)
                } else {
                    Err(RuntimeError::Runtime(
                        (*self.get_token(op_ref)).clone(),
                        "Not callable".to_string(),
                    ))
                }
            }
        }
    }

    fn eval_include(&mut self, module_path: Literal, env: &Shared<SharedCell<Env>>) -> Result<(), RuntimeError> {
        let path_str = match module_path {
            Literal::String(s) => s,
            _ => {
                #[cfg(not(feature = "sync"))]
                let token = self
                    .token_arena
                    .borrow()
                    .get(crate::arena::ArenaId::new(0))
                    .map(|t| (**t).clone());
                #[cfg(feature = "sync")]
                let token = self
                    .token_arena
                    .read()
                    .unwrap()
                    .get(crate::arena::ArenaId::new(0))
                    .map(|t| (**t).clone());

                return Err(RuntimeError::Runtime(
                    token.unwrap_or_else(|| crate::lexer::token::Token {
                        range: crate::Range::default(),
                        kind: crate::lexer::token::TokenKind::Eof,
                        module_id: crate::arena::ArenaId::new(0),
                    }),
                    "Invalid module path".to_string(),
                ));
            }
        };

        let module = self
            .module_loader
            .load_from_file(&path_str, Shared::clone(&self.token_arena))?;

        self.load_module_with_env(module, env)
    }

    fn eval_import(&mut self, module_path: Literal, env: &Shared<SharedCell<Env>>) -> Result<(), RuntimeError> {
        match module_path {
            Literal::String(module_name) => {
                let module = self
                    .module_loader
                    .load_from_file(&module_name, Shared::clone(&self.token_arena));

                if let Ok(module) = module {
                    // Create a new environment for the module exports
                    let module_env = Shared::new(SharedCell::new(Env::with_parent(Shared::downgrade(env))));

                    let module_name_to_use = module.name.to_string();

                    self.load_module_with_env(module, &Shared::clone(&module_env))?;

                    // Register the module in the environment
                    let module_runtime_value =
                        RuntimeValue::Module(ModuleEnv::new(&module_name_to_use, Shared::clone(&module_env)));

                    self.define(&self.env, Ident::new(&module_name_to_use), module_runtime_value);

                    Ok(())
                } else {
                    // Module loading failed or already loaded
                    Ok(())
                }
            }
            _ => Err(RuntimeError::Runtime(
                (*self.get_token(OpRef::new(0))).clone(),
                "Import requires a string literal".to_string(),
            )),
        }
    }

    fn load_module_with_env(
        &mut self,
        module: module::Module,
        env: &Shared<SharedCell<Env>>,
    ) -> Result<(), RuntimeError> {
        // For now, use the recursive evaluator to load modules
        // TODO: Eventually transform entire module to OpTree

        for module_node in &module.modules {
            match &*module_node.expr {
                Expr::Include(path) => {
                    self.eval_include(path.clone(), env)?;
                }
                Expr::Import(path) => {
                    self.eval_import(path.clone(), env)?;
                }
                Expr::Module(ident, program) => {
                    let module_env = Shared::new(SharedCell::new(Env::with_parent(Shared::downgrade(env))));

                    // Recursively evaluate module program using recursive evaluator
                    let mut recursive_eval: crate::eval::Evaluator<T> =
                        crate::eval::Evaluator::with_env(Shared::clone(&self.token_arena), Shared::clone(&module_env));
                    let _ = recursive_eval
                        .eval(program, std::iter::once(RuntimeValue::None))
                        .map_err(|e| match e {
                            crate::error::InnerError::Runtime(r) => r,
                            _ => RuntimeError::InternalError((*self.get_token(OpRef::new(0))).clone()),
                        })?;

                    let module_runtime_value =
                        RuntimeValue::Module(ModuleEnv::new(&ident.name.as_str(), Shared::clone(&module_env)));
                    self.define(env, ident.name, module_runtime_value);
                }
                _ => {}
            }
        }

        // Process module functions
        for func_node in &module.functions {
            if let Expr::Def(ident, params, program) = &*func_node.expr {
                // Store as traditional AST function for now
                // OpTree functions would require full transformation
                self.define(
                    env,
                    ident.name,
                    RuntimeValue::Function(params.clone(), program.clone(), Shared::clone(env)),
                );
            }
        }

        // Process module variables
        for var_node in &module.vars {
            if let Expr::Let(ident, value_node) = &*var_node.expr {
                // Evaluate using recursive evaluator
                let mut recursive_eval: crate::eval::Evaluator<T> =
                    crate::eval::Evaluator::with_env(Shared::clone(&self.token_arena), Shared::clone(env));
                let val = recursive_eval
                    .eval(&vec![Shared::clone(value_node)], std::iter::once(RuntimeValue::None))
                    .map_err(|e| match e {
                        crate::error::InnerError::Runtime(r) => r,
                        _ => RuntimeError::InternalError((*self.get_token(OpRef::new(0))).clone()),
                    })?
                    .into_iter()
                    .next()
                    .unwrap_or(RuntimeValue::None);

                self.define(env, ident.name, val);
            }
        }

        Ok(())
    }

    // ===== Environment Operations =====

    fn define(&self, env: &Shared<SharedCell<Env>>, name: Ident, value: RuntimeValue) {
        #[cfg(not(feature = "sync"))]
        env.borrow_mut().define(name, value);
        #[cfg(feature = "sync")]
        env.write().unwrap().define(name, value);
    }

    fn define_mutable(&self, env: &Shared<SharedCell<Env>>, name: Ident, value: RuntimeValue) {
        #[cfg(not(feature = "sync"))]
        env.borrow_mut().define_mutable(name, value);
        #[cfg(feature = "sync")]
        env.write().unwrap().define_mutable(name, value);
    }

    fn assign(
        &self,
        env: &Shared<SharedCell<Env>>,
        name: Ident,
        value: RuntimeValue,
        op_ref: OpRef,
    ) -> Result<(), RuntimeError> {
        #[cfg(not(feature = "sync"))]
        let mut env_borrow = env.borrow_mut();
        #[cfg(feature = "sync")]
        let mut env_borrow = env.write().unwrap();

        env_borrow.assign(name, value).map_err(|e| match e {
            EnvError::UndefinedVariable(_) => {
                RuntimeError::UndefinedVariable((*self.get_token(op_ref)).clone(), name.to_string())
            }
            EnvError::AssignToImmutable(_) => {
                RuntimeError::AssignToImmutable((*self.get_token(op_ref)).clone(), name.to_string())
            }
            _ => RuntimeError::Runtime((*self.get_token(op_ref)).clone(), format!("Assignment error: {:?}", e)),
        })
    }

    // ===== Utilities =====

    fn get_token(&self, op_ref: OpRef) -> Shared<Token> {
        let token_id = self.source_map.get(op_ref);
        get_token(Shared::clone(&self.token_arena), token_id)
    }

    #[cfg(feature = "debugger")]
    fn check_breakpoint(&self, _op_ref: OpRef, _runtime_value: &RuntimeValue, _env: &Shared<SharedCell<Env>>) {
        // TODO: Implement breakpoint checking for DAP support
        // This would use the existing Debugger infrastructure
    }
}
