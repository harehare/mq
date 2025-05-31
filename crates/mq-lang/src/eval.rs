// This module is responsible for evaluating a parsed mq program.
// It takes an Abstract Syntax Tree (AST) representation of the program
// and an input iterator of RuntimeValues, producing a vector of RuntimeValues as output.
// The evaluation process involves managing environments (scopes), handling function calls
// (both user-defined and built-in), and executing control flow structures.
use std::{cell::RefCell, rc::Rc};

use crate::{
    Program,
    Token,
    TokenKind,
    arena::Arena,
    ast::node::{self as ast}, // Alias for easier access to AST node types.
    error::InnerError,
};

// Submodules for specific functionalities within the evaluator.
pub mod builtin; // Handles built-in functions.
pub mod env; // Manages environments and scopes.
pub mod error; // Defines evaluation-specific errors.
pub mod module; // Handles module loading and management.
pub mod runtime_value; // Defines the types of values manipulated during runtime.

use env::Env;
use error::EvalError;
use runtime_value::RuntimeValue;
use smallvec::SmallVec; // Efficient small vector optimization.

/// Configuration options for the evaluator.
#[derive(Debug, Clone)]
pub struct Options {
    /// If true, `None` values encountered during chained operations will short-circuit
    /// and result in `None` immediately, rather than causing errors.
    pub filter_none: bool,
    /// Maximum depth of the call stack to prevent infinite recursion.
    pub max_call_stack_depth: u32,
}

#[cfg(debug_assertions)]
// Default options for debug builds (e.g., smaller call stack for easier debugging).
impl Default for Options {
    fn default() -> Self {
        Self {
            filter_none: true,        // Enable filtering of None by default.
            max_call_stack_depth: 32, // Lower call stack depth for debug builds.
        }
    }
}

#[cfg(not(debug_assertions))]
// Default options for release builds.
impl Default for Options {
    fn default() -> Self {
        Self {
            filter_none: true,         // Enable filtering of None by default.
            max_call_stack_depth: 192, // Higher call stack depth for release builds.
        }
    }
}

/// The `Evaluator` is responsible for executing an mq program.
/// It maintains the current execution environment, manages module loading,
/// and handles the evaluation of expressions and statements.
#[derive(Debug, Clone)]
pub struct Evaluator {
    /// The current execution environment, holding variables and function definitions.
    /// Wrapped in `Rc<RefCell<...>>` to allow shared mutable access.
    env: Rc<RefCell<Env>>,
    /// Arena for storing tokens, used for error reporting to get token details.
    token_arena: Rc<RefCell<Arena<Rc<Token>>>>,
    /// Current depth of the function call stack, used to prevent recursion errors.
    call_stack_depth: u32,
    /// Configuration options for the evaluator.
    pub(crate) options: Options,
    /// Handles loading of mq modules.
    pub(crate) module_loader: module::ModuleLoader,
}

impl Evaluator {
    /// Creates a new `Evaluator`.
    ///
    /// # Arguments
    /// * `module_loader` - A `ModuleLoader` instance for handling module imports.
    /// * `token_arena` - An `Arena` containing all tokens from parsing, for error reporting.
    pub(crate) fn new(
        module_loader: module::ModuleLoader,
        token_arena: Rc<RefCell<Arena<Rc<Token>>>>,
    ) -> Self {
        Self {
            env: Rc::new(RefCell::new(Env::default())), // Initialize with a default, empty environment.
            module_loader,
            call_stack_depth: 0,
            token_arena,
            options: Options::default(),
        }
    }

    /// Evaluates an entire mq program with a given input stream.
    ///
    /// This is the main entry point for program execution.
    /// It first processes top-level definitions (functions and includes)
    /// and then evaluates the rest of the program against each item from the input iterator.
    ///
    /// # Arguments
    /// * `program` - The parsed `Program` (a sequence of AST nodes) to evaluate.
    /// * `input` - An iterator yielding `RuntimeValue` items that will be processed by the program.
    ///
    /// # Returns
    /// A `Result` containing a `Vec<RuntimeValue>` with the output of the program for each input,
    /// or an `InnerError` if an error occurs during evaluation.
    pub(crate) fn eval<I>(
        &mut self,
        program: &Program,
        input: I,
    ) -> Result<Vec<RuntimeValue>, InnerError>
    where
        I: Iterator<Item = RuntimeValue>,
    {
        // First pass: Handle top-level definitions (def and include)
        // and collect other expressions into a new program vector.
        let mut program_without_defs_and_includes = program.iter().try_fold(
            Vec::with_capacity(program.len()),
            |mut nodes: Vec<Rc<ast::Node>>, node: &Rc<ast::Node>| -> Result<_, InnerError> {
                match &*node.expr {
                    ast::Expr::Def(ident, params, def_program_body) => {
                        // Define the function in the global environment.
                        // The function captures the current environment (self.env) at definition time.
                        self.env.borrow_mut().define(
                            ident,
                            RuntimeValue::Function(
                                params.clone(),
                                def_program_body.clone(),
                                Rc::clone(&self.env), // Closure: captures the environment at definition.
                            ),
                        );
                    }
                    ast::Expr::Include(module_id) => {
                        // Process an include directive.
                        self.eval_include(module_id.to_owned())?;
                    }
                    _ => {
                        // Collect other expressions to be evaluated later.
                        nodes.push(Rc::clone(node))
                    }
                };
                Ok(nodes)
            },
        )?;

        // Check if there's a `nodes` expression in the program.
        // This expression alters how the program processes input, typically by operating on the
        // structure of Markdown content directly.
        let nodes_index = &program_without_defs_and_includes
            .iter()
            .position(|node| node.is_nodes());

        if let Some(index) = nodes_index {
            // If `nodes` is present, split the program.
            // `main_program_part` is everything before `nodes`.
            // `nodes_program_part` is `nodes` and everything after it.
            let (main_program_part, nodes_program_part) =
                program_without_defs_and_includes.split_at_mut(*index);
            let main_program_part = main_program_part.to_vec();
            let nodes_program_part = nodes_program_part.to_vec();

            // Evaluate the main part of the program for each input item.
            let values_after_main_part: Result<Vec<RuntimeValue>, InnerError> = input
                .map(|runtime_value| match &runtime_value {
                    RuntimeValue::Markdown(node, _) => {
                        // If input is Markdown, use special Markdown node evaluation.
                        self.eval_markdown_node(&main_program_part, node)
                    }
                    _ => {
                        // For other input types, evaluate normally.
                        self.eval_program(&main_program_part, runtime_value, Rc::clone(&self.env))
                            .map_err(InnerError::Eval)
                    }
                })
                .collect();

            if nodes_program_part.is_empty() {
                // If there's nothing after `nodes` (or `nodes` was the last thing), return the collected values.
                values_after_main_part
            } else {
                // If there's a program part including and after `nodes`,
                // evaluate it with the collected results from the first part as its input.
                self.eval_program(
                    &nodes_program_part,
                    values_after_main_part?.into(), // Convert Vec<RuntimeValue> into a single Array RuntimeValue.
                    Rc::clone(&self.env),
                )
                .map(|value| {
                    // Ensure the final output is a flat Vec<RuntimeValue>.
                    if let RuntimeValue::Array(values) = value {
                        values
                    } else {
                        vec![value]
                    }
                })
                .map_err(InnerError::Eval)
            }
        } else {
            // No `nodes` expression found, evaluate the entire (modified) program for each input item.
            input
                .map(|runtime_value| match &runtime_value {
                    RuntimeValue::Markdown(node, _) => {
                        self.eval_markdown_node(&program_without_defs_and_includes, node)
                    }
                    _ => self
                        .eval_program(
                            &program_without_defs_and_includes,
                            runtime_value,
                            Rc::clone(&self.env),
                        )
                        .map_err(InnerError::Eval),
                })
                .collect()
        }
    }

    /// Evaluates a program specifically for a given Markdown node.
    /// This function is used to apply mq transformations within the structure of a Markdown document.
    /// It recursively processes child nodes if the program transforms them.
    fn eval_markdown_node(
        &mut self,
        program: &Program,
        node: &mq_markdown::Node,
    ) -> Result<RuntimeValue, InnerError> {
        // `map_values` traverses the Markdown node. For each child text/value component,
        // it calls the provided closure. The closure should evaluate the `program`
        // with that child component as input.
        node.map_values(&mut |child_node| {
            // Evaluate the main program part using the current child_node as input.
            let value = self
                .eval_program(
                    program,
                    RuntimeValue::Markdown(child_node.clone(), None), // Wrap child_node as RuntimeValue.
                    Rc::clone(&self.env), // Use the current environment.
                )
                .map_err(InnerError::Eval)?;

            // Determine how to incorporate the result of the evaluation back into the Markdown structure.
            Ok(match value {
                RuntimeValue::None => child_node.to_fragment(), // If program returns None, keep original child as a fragment.
                RuntimeValue::Function(_, _, _) | RuntimeValue::NativeFunction(_) => {
                    // Functions themselves aren't directly inserted; effectively results in Empty.
                    mq_markdown::Node::Empty
                }
                // For simple values, convert them to a Markdown text node.
                RuntimeValue::Array(_)
                | RuntimeValue::Bool(_)
                | RuntimeValue::Number(_)
                | RuntimeValue::String(_) => value.to_string().into(),
                // If the program returned a (potentially modified) Markdown node, use that.
                RuntimeValue::Markdown(node, _) => node,
            })
        })
        .map(|node| RuntimeValue::Markdown(node, None)) // Wrap the final mapped node back into a RuntimeValue.
    }

    /// Defines a string variable in the global environment.
    /// Useful for setting up predefined string constants.
    pub fn define_string_value(&self, name: &str, value: &str) {
        self.env.borrow_mut().define(
            &ast::Ident::new(name),
            RuntimeValue::String(value.to_string()),
        );
    }

    /// Loads the built-in module, making its functions available in the global environment.
    pub(crate) fn load_builtin_module(&mut self) -> Result<(), EvalError> {
        let module = self
            .module_loader
            .load_builtin(Rc::clone(&self.token_arena)) // Use module_loader to get the built-in module.
            .map_err(EvalError::ModuleLoadError)?;
        self.load_module(module) // Populate the environment with contents of the loaded module.
    }

    /// Loads functions and variables from a given `module::Module` into the current environment.
    pub(crate) fn load_module(&mut self, module: Option<module::Module>) -> Result<(), EvalError> {
        if let Some(module) = module {
            // Iterate over function definitions in the module.
            module.modules.iter().for_each(|node| {
                if let ast::Expr::Def(ident, params, program) = &*node.expr {
                    // Define each function in the current (likely global) environment.
                    // Functions from modules also capture the environment they are defined in (which is the current self.env).
                    self.env.borrow_mut().define(
                        ident,
                        RuntimeValue::Function(
                            params.clone(),
                            program.clone(),
                            Rc::clone(&self.env), // Capture current environment for the function from module.
                        ),
                    );
                }
            });

            // Iterate over variable definitions (let statements) in the module.
            module.vars.iter().try_for_each(|node| {
                if let ast::Expr::Let(ident, value_node) = &*node.expr {
                    // Evaluate the variable's value in the context of the current environment.
                    // Note: This evaluation uses RuntimeValue::NONE as initial input, which is typical for `let` RHS.
                    let val = self.eval_expr(
                        &RuntimeValue::NONE,
                        Rc::clone(value_node),
                        Rc::clone(&self.env),
                    )?;
                    self.env.borrow_mut().define(ident, val); // Define the variable in the current environment.
                    Ok(())
                } else {
                    // Should not happen if module parsing is correct.
                    Err(EvalError::InternalError(
                        (*self.token_arena.borrow()[node.token_id]).clone(),
                    ))
                }
            })
        } else {
            Ok(()) // No module provided, nothing to load.
        }
    }

    /// Evaluates a sequence of expressions (a `Program`) using a given initial `runtime_value` and `env`.
    /// Each expression's output becomes the input for the next, forming a pipeline.
    fn eval_program(
        &mut self,
        program: &Program,           // The sequence of AST nodes to evaluate.
        runtime_value: RuntimeValue, // The initial input value for the first expression.
        env: Rc<RefCell<Env>>,       // The environment to use for this evaluation.
    ) -> Result<RuntimeValue, EvalError> {
        // Fold over the program, passing the result of one expression to the next.
        program
            .iter()
            .try_fold(runtime_value, |current_value, expr_node| {
                // Optional chaining: if `filter_none` is on and current_value is None,
                // skip further evaluation and propagate None.
                if self.options.filter_none && current_value.is_none() {
                    return Ok(RuntimeValue::NONE);
                }
                // Evaluate the current expression node with the current_value as its input.
                self.eval_expr(&current_value, Rc::clone(expr_node), Rc::clone(&env))
            })
    }

    /// Evaluates an identifier (variable name) within a given environment.
    fn eval_ident(
        &self,
        ident: &ast::Ident,    // The identifier to resolve.
        node: Rc<ast::Node>, // The AST node corresponding to this identifier (for error location).
        env: Rc<RefCell<Env>>, // The environment in which to resolve the identifier.
    ) -> Result<RuntimeValue, EvalError> {
        env.borrow()
            .resolve(ident) // Attempt to find the identifier in the environment chain.
            .map_err(|e| e.to_eval_error((*node).clone(), Rc::clone(&self.token_arena))) // Convert EnvError to EvalError.
    }

    /// Handles an `include` directive by loading the specified module.
    fn eval_include(&mut self, module_literal: ast::Literal) -> Result<(), EvalError> {
        match module_literal {
            ast::Literal::String(module_name) => {
                // Load the module from a file using the module_loader.
                let module_content = self
                    .module_loader
                    .load_from_file(&module_name, Rc::clone(&self.token_arena))
                    .map_err(EvalError::ModuleLoadError)?;
                // Load the definitions from the parsed module content into the current environment.
                self.load_module(module_content)
            }
            _ => {
                // Module name must be a string literal.
                Err(EvalError::ModuleLoadError(
                    module::ModuleError::InvalidModule,
                ))
            }
        }
    }

    /// Evaluates a selector expression (e.g., `.tag`, `#id`) against a runtime value.
    /// Selectors are typically used with Markdown nodes.
    fn eval_selector_expr(runtime_value: RuntimeValue, selector: &ast::Selector) -> RuntimeValue {
        match &runtime_value {
            RuntimeValue::Markdown(node_value, _) => {
                // Apply the selector to a single Markdown node.
                if builtin::eval_selector(node_value, selector) {
                    runtime_value // Return the original node if the selector matches.
                } else {
                    RuntimeValue::NONE // Return None if it doesn't match.
                }
            }
            RuntimeValue::Array(values) => {
                // If the input is an array, apply the selector to each Markdown node within the array.
                let selected_values = values
                    .iter()
                    .map(|value| match value {
                        RuntimeValue::Markdown(node_value, _) => {
                            if builtin::eval_selector(node_value, selector) {
                                value.clone()
                            } else {
                                RuntimeValue::NONE
                            }
                        }
                        _ => RuntimeValue::NONE, // Non-Markdown items in the array result in None.
                    })
                    .collect::<Vec<_>>();
                RuntimeValue::Array(selected_values)
            }
            _ => RuntimeValue::NONE, // Selectors on non-Markdown, non-Array types result in None.
        }
    }

    /// Evaluates an interpolated string, replacing placeholders with their values.
    fn eval_interpolated_string(
        &self,
        runtime_value: &RuntimeValue, // The current context value, accessible via `.` in the string.
        node: Rc<ast::Node>,          // The AST node for the interpolated string.
        env: Rc<RefCell<Env>>,        // The environment for resolving identifiers.
    ) -> Result<RuntimeValue, EvalError> {
        if let ast::Expr::InterpolatedString(segments) = &*node.expr {
            // Iterate over segments (text literals or expressions to evaluate).
            segments
                .iter()
                .try_fold(String::with_capacity(100), |mut acc, segment| {
                    match segment {
                        ast::StringSegment::Text(s) => acc.push_str(s), // Append literal text.
                        ast::StringSegment::Ident(ident) => {
                            // Evaluate the identifier and append its string representation.
                            let value =
                                self.eval_ident(ident, Rc::clone(&node), Rc::clone(&env))?;
                            acc.push_str(&value.to_string());
                        }
                        ast::StringSegment::Self_ => {
                            // Substitute `.` with the string representation of the current runtime_value.
                            acc.push_str(&runtime_value.to_string());
                        }
                    }
                    Ok(acc)
                })
                .map(|acc| acc.into()) // Convert the accumulated String into a RuntimeValue::String.
        } else {
            unreachable!() // This function should only be called with InterpolatedString nodes.
        }
    }

    /// Evaluates a single AST expression node.
    /// This is a central dispatch function that routes to specific evaluation logic
    /// based on the type of the expression.
    ///
    /// # Arguments
    /// * `runtime_value` - The current input value for this expression (often the result of a previous one).
    /// * `node` - The AST node representing the expression to evaluate.
    /// * `env` - The environment in which to evaluate the expression.
    fn eval_expr(
        &mut self,
        runtime_value: &RuntimeValue,
        node: Rc<ast::Node>,
        env: Rc<RefCell<Env>>,
    ) -> Result<RuntimeValue, EvalError> {
        match &*node.expr {
            ast::Expr::Selector(ident) => {
                // Evaluate a selector expression (e.g., `.h1`, `.text`).
                Ok(Self::eval_selector_expr(runtime_value.clone(), ident))
            }
            ast::Expr::Call(ident, args, optional) => {
                // Evaluate a function call.
                self.eval_fn(runtime_value, Rc::clone(&node), ident, args, *optional, env)
            }
            ast::Expr::Self_ | ast::Expr::Nodes => Ok(runtime_value.clone()), // '.' or 'nodes' evaluates to the current input value.
            ast::Expr::If(_) => self.eval_if(runtime_value, node, env), // Evaluate an if/else if/else conditional expression.
            ast::Expr::Ident(ident) => self.eval_ident(ident, Rc::clone(&node), Rc::clone(&env)), // Resolve an identifier (variable).
            ast::Expr::Literal(literal) => Ok(self.eval_literal(literal)), // Evaluate a literal value (string, number, bool, none).
            ast::Expr::Def(ident, params, program_body) => {
                // Define a new function.
                // The function captures the environment (`env`) active at its definition point.
                let function =
                    RuntimeValue::Function(params.clone(), program_body.clone(), Rc::clone(&env));
                env.borrow_mut().define(ident, function.clone()); // Add function to the current environment.
                Ok(function) // The result of a 'def' expression is the function value itself.
            }
            ast::Expr::Fn(params, program_body) => {
                // Define an anonymous function (lambda).
                // Similar to 'def', it captures the current environment.
                let function =
                    RuntimeValue::Function(params.clone(), program_body.clone(), Rc::clone(&env));
                Ok(function)
            }
            ast::Expr::Let(ident, value_node) => {
                // Evaluate the expression on the right-hand side of 'let'.
                let let_value =
                    self.eval_expr(runtime_value, Rc::clone(value_node), Rc::clone(&env))?;
                // Define the identifier in the current environment with the evaluated value.
                env.borrow_mut().define(ident, let_value);
                // A 'let' expression evaluates to the original `runtime_value` (input to 'let').
                Ok(runtime_value.clone())
            }
            ast::Expr::While(_, _) => self.eval_while(runtime_value, node, env), // Evaluate a 'while' loop.
            ast::Expr::Until(_, _) => self.eval_until(runtime_value, node, env), // Evaluate an 'until' loop.
            ast::Expr::Foreach(_, _, _) => self.eval_foreach(runtime_value, node, env), // Evaluate a 'foreach' loop.
            ast::Expr::InterpolatedString(_) => {
                // Evaluate an interpolated string.
                self.eval_interpolated_string(runtime_value, node, env)
            }
            ast::Expr::Include(module_id) => {
                // Process an 'include' directive.
                self.eval_include(module_id.to_owned())?;
                // An 'include' expression evaluates to the original `runtime_value`.
                Ok(runtime_value.clone())
            }
        }
    }

    /// Evaluates a literal AST node to its corresponding `RuntimeValue`.
    fn eval_literal(&self, literal: &ast::Literal) -> RuntimeValue {
        match literal {
            ast::Literal::None => RuntimeValue::None,
            ast::Literal::Bool(b) => RuntimeValue::Bool(*b),
            ast::Literal::String(s) => RuntimeValue::String(s.clone()),
            ast::Literal::Number(n) => RuntimeValue::Number(*n),
        }
    }

    /// Evaluates a `foreach` loop.
    /// It iterates over an array, defining a loop variable for each item,
    /// and executes the loop body. Results from each iteration are collected into an array.
    fn eval_foreach(
        &mut self,
        runtime_value: &RuntimeValue, // Current input value, passed through to loop body.
        node: Rc<ast::Node>,          // The `foreach` AST node.
        env: Rc<RefCell<Env>>,        // The environment in which `foreach` is evaluated.
    ) -> Result<RuntimeValue, EvalError> {
        if let ast::Expr::Foreach(loop_var_ident, collection_node, body_program) = &*node.expr {
            // 1. Evaluate the expression that should result in the collection to iterate over.
            let collection_val =
                self.eval_expr(runtime_value, Rc::clone(collection_node), Rc::clone(&env))?;

            if let RuntimeValue::Array(items_to_iterate) = collection_val {
                let mut iteration_results: Vec<RuntimeValue> =
                    Vec::with_capacity(items_to_iterate.len());
                // 2. Create a new environment for the loop body. This environment is a child of `env`.
                let loop_body_env = Rc::new(RefCell::new(Env::with_parent(Rc::downgrade(&env))));

                for item in items_to_iterate {
                    // 3. Define the loop variable (e.g., `x` in `foreach x in arr`) in the loop's new scope.
                    loop_body_env.borrow_mut().define(loop_var_ident, item);
                    // 4. Evaluate the loop body program with the `runtime_value` (original input to foreach)
                    //    and the new loop environment.
                    let result = self.eval_program(
                        body_program,
                        runtime_value.clone(),
                        Rc::clone(&loop_body_env),
                    )?;
                    iteration_results.push(result);
                }
                Ok(RuntimeValue::Array(iteration_results)) // `foreach` evaluates to an array of results.
            } else {
                // The expression for the collection did not evaluate to an array.
                Err(EvalError::InvalidTypes {
                    token: (*self.token_arena.borrow()[node.token_id]).clone(),
                    name: TokenKind::Foreach.to_string(),
                    args: vec![collection_val.to_string().into()],
                })
            }
        } else {
            unreachable!() // Should only be called with Foreach AST nodes.
        }
    }

    /// Evaluates an `until` loop.
    /// The loop body is executed as long as the condition evaluates to true.
    /// The input `runtime_value` is passed to the condition and body, and can be modified by the body.
    fn eval_until(
        &mut self,
        runtime_value: &RuntimeValue, // Initial input value.
        node: Rc<ast::Node>,          // The `until` AST node.
        env: Rc<RefCell<Env>>,        // Environment for evaluation.
    ) -> Result<RuntimeValue, EvalError> {
        if let ast::Expr::Until(cond_node, body_program) = &*node.expr {
            let mut current_loop_value = runtime_value.clone();
            // Create a new child environment for the loop's scope.
            let loop_env = Rc::new(RefCell::new(Env::with_parent(Rc::downgrade(&env))));

            // Evaluate the condition for the first time.
            let mut cond_result = self.eval_expr(
                &current_loop_value,
                Rc::clone(cond_node),
                Rc::clone(&loop_env),
            )?;

            // For an `until` loop, if the condition is initially false, the body is never executed.
            if !cond_result.is_true() {
                return Ok(RuntimeValue::NONE); // Or perhaps current_loop_value, depending on desired semantics for non-executed loops.
            }

            // Loop as long as the condition is true.
            while cond_result.is_true() {
                // Evaluate the loop body. The result of the body becomes the new `current_loop_value`
                // for the next condition evaluation.
                current_loop_value =
                    self.eval_program(body_program, current_loop_value, Rc::clone(&loop_env))?;
                // Re-evaluate the condition with the (potentially updated) `current_loop_value`.
                cond_result = self.eval_expr(
                    &current_loop_value,
                    Rc::clone(cond_node),
                    Rc::clone(&loop_env),
                )?;
            }
            // The result of the `until` loop is the `current_loop_value` after the last body execution
            // (or the initial value if the loop condition was false from the start and it returned current_loop_value).
            Ok(current_loop_value)
        } else {
            unreachable!()
        }
    }

    /// Evaluates a `while` loop.
    /// The loop body is executed as long as the condition evaluates to true.
    /// Results from each body execution are collected into an array.
    fn eval_while(
        &mut self,
        runtime_value: &RuntimeValue, // Initial input value.
        node: Rc<ast::Node>,          // The `while` AST node.
        env: Rc<RefCell<Env>>,        // Environment for evaluation.
    ) -> Result<RuntimeValue, EvalError> {
        if let ast::Expr::While(cond_node, body_program) = &*node.expr {
            let mut current_loop_value = runtime_value.clone();
            // Create a new child environment for the loop's scope.
            let loop_env = Rc::new(RefCell::new(Env::with_parent(Rc::downgrade(&env))));
            // Evaluate the condition for the first time.
            let mut cond_result = self.eval_expr(
                &current_loop_value,
                Rc::clone(cond_node),
                Rc::clone(&loop_env),
            )?;
            let mut iteration_results = Vec::with_capacity(100);

            // If the condition is initially false, the loop body is never executed.
            if !cond_result.is_true() {
                return Ok(RuntimeValue::NONE); // `while` with initially false condition results in None.
            }

            // Loop as long as the condition is true.
            while cond_result.is_true() {
                // Evaluate the loop body. The result of the body becomes the new `current_loop_value`.
                current_loop_value =
                    self.eval_program(body_program, current_loop_value, Rc::clone(&loop_env))?;
                // Re-evaluate the condition with the (potentially updated) `current_loop_value`.
                cond_result = self.eval_expr(
                    &current_loop_value,
                    Rc::clone(cond_node),
                    Rc::clone(&loop_env),
                )?;
                iteration_results.push(current_loop_value.clone());
            }
            // `while` loop evaluates to an array of the results from each iteration's body.
            Ok(RuntimeValue::Array(iteration_results))
        } else {
            unreachable!()
        }
    }

    /// Evaluates an `if` expression, including `else if` and `else` branches.
    fn eval_if(
        &mut self,
        runtime_value: &RuntimeValue, // Input value for condition and body evaluation.
        node: Rc<ast::Node>,          // The `if` AST node.
        env: Rc<RefCell<Env>>,        // Environment for evaluation.
    ) -> Result<RuntimeValue, EvalError> {
        if let ast::Expr::If(conditional_branches) = &*node.expr {
            // Iterate through each branch (condition_expression_option, body_expression).
            // `condition_expression_option` is `Some(expr)` for `if` and `else if`, and `None` for `else`.
            for (condition_expr_opt, body_expr) in conditional_branches {
                match condition_expr_opt {
                    Some(actual_condition_expr) => {
                        // This is an 'if' or 'else if' branch. Evaluate its condition.
                        let condition_value = self.eval_expr(
                            runtime_value,
                            Rc::clone(actual_condition_expr),
                            Rc::clone(&env),
                        )?;
                        // If the condition is true, evaluate this branch's body and return the result.
                        if condition_value.is_true() {
                            return self.eval_expr(runtime_value, Rc::clone(body_expr), env);
                        }
                        // If false, proceed to the next branch.
                    }
                    None => {
                        // This is an 'else' branch (condition is implicitly true if reached).
                        // Evaluate its body and return the result.
                        return self.eval_expr(runtime_value, Rc::clone(body_expr), env);
                    }
                }
            }
            // If no 'if' or 'else if' condition was true, and there was no 'else' branch.
            Ok(RuntimeValue::NONE)
        } else {
            unreachable!()
        }
    }

    /// Evaluates a function call, which can be either a user-defined function or a built-in one.
    fn eval_fn(
        &mut self,
        runtime_value: &RuntimeValue, // The current value `.` passed to the function context.
        node: Rc<ast::Node>,          // The AST node for the function call (for error reporting).
        ident: &ast::Ident,           // The identifier (name) of the function being called.
        args: &ast::Args,             // The arguments provided in the function call (AST nodes).
        optional: bool,               // True if this is an optional call (e.g., `foo?()`).
        env: Rc<RefCell<Env>>, // The environment in which the function call occurs (caller's environment).
    ) -> Result<RuntimeValue, EvalError> {
        // Handle optional chaining: if input is None and call is optional, return None immediately.
        if runtime_value.is_none() && optional {
            return Ok(RuntimeValue::NONE);
        }

        // Try to resolve the function identifier in the current (caller's) environment.
        if let Ok(fn_value_from_env) = Rc::clone(&env).borrow().resolve(ident) {
            // A binding for `ident` was found. Check if it's a function.
            match &fn_value_from_env {
                RuntimeValue::Function(
                    param_names_nodes,
                    function_body_program,
                    definition_env,
                ) => {
                    // It's a user-defined function.
                    self.enter_scope()?; // Increment call stack depth and check for recursion limits.

                    // --- Argument Handling ---
                    // mq functions can implicitly take the current value `.` as the first argument
                    // if the number of declared parameters is one more than provided arguments.
                    let mut final_arg_nodes: ast::Args = SmallVec::with_capacity(args.len());
                    let final_arg_nodes = if param_names_nodes.len() == args.len() + 1 {
                        // Case: Implicit 'self' or context argument (current `runtime_value`).
                        // Create a synthetic AST node for `.` to be the first argument.
                        final_arg_nodes.insert(
                            0,
                            Rc::new(ast::Node {
                                token_id: node.token_id, // Use call site token for the synthetic 'self' node.
                                expr: Rc::new(ast::Expr::Self_),
                            }),
                        );
                        final_arg_nodes.extend(args.clone()); // Add the explicitly provided arguments after 'self'.
                        final_arg_nodes
                    } else if args.len() != param_names_nodes.len() {
                        // Case: Argument count mismatch.
                        self.exit_scope(); // Ensure scope is exited before erroring.
                        return Err(EvalError::InvalidNumberOfArguments(
                            (*self.token_arena.borrow()[node.token_id]).clone(),
                            ident.to_string(),
                            param_names_nodes.len() as u8,
                            args.len() as u8,
                        ));
                    } else {
                        // Case: Number of arguments matches number of parameters directly.
                        args.clone()
                    };

                    // --- Environment Setup for Function Execution ---
                    // Create a new environment for the function's execution.
                    // This new environment's parent is `definition_env` (the environment where the function was defined),
                    // enabling lexical scoping (closures).
                    let function_execution_env = Rc::new(RefCell::new(Env::with_parent(
                        Rc::downgrade(definition_env),
                    )));

                    // --- Evaluate Arguments and Define Parameters ---
                    // The provided argument *expressions* (from `final_arg_nodes`) are evaluated in the *caller's environment* (`env`).
                    // The resulting *values* are then bound to the parameter names in the *function's new execution environment* (`function_execution_env`).
                    final_arg_nodes
                        .iter()
                        .zip(param_names_nodes.iter())
                        .try_for_each(|(arg_expr_node, param_name_node)| {
                            // param_name_node is an AST Node whose expr should be an Ident.
                            if let ast::Expr::Ident(param_name_ident) = &*param_name_node.expr {
                                // Evaluate the argument expression in the caller's scope (`env`).
                                // The `runtime_value` for argument evaluation is the `.` from the call site.
                                let arg_value = self.eval_expr(
                                    runtime_value,
                                    Rc::clone(arg_expr_node),
                                    Rc::clone(&env),
                                )?;
                                // Define the parameter in the function's new scope.
                                function_execution_env
                                    .borrow_mut()
                                    .define(param_name_ident, arg_value);
                                Ok(())
                            } else {
                                // This should not happen with a valid AST where params are Idents.
                                Err(EvalError::InvalidDefinition(
                                    (*self.token_arena.borrow()[param_name_node.token_id]).clone(),
                                    ident.to_string(), // The function being called.
                                ))
                            }
                        })?;

                    // --- Evaluate Function Body ---
                    // Evaluate the function's program (body) using the new `function_execution_env`.
                    // The `runtime_value` (value of `.` inside the function) is the one passed to `eval_fn` (the caller's context).
                    let result = self.eval_program(
                        function_body_program,
                        runtime_value.clone(),
                        function_execution_env,
                    );

                    self.exit_scope(); // Decrement call stack depth.
                    result
                }
                RuntimeValue::NativeFunction(native_fn_actual_ident) => {
                    // It's a built-in (native) function.
                    // `ident` is the name used at call site, `native_fn_actual_ident` is its canonical name.
                    self.eval_builtin(runtime_value, node, native_fn_actual_ident, args, env)
                }
                _ => {
                    // The resolved identifier is not a callable function type.
                    Err(EvalError::InvalidDefinition(
                        // TODO: Better error, e.g., "NotAFunction"
                        (*self.token_arena.borrow()[node.token_id]).clone(),
                        ident.to_string(),
                    ))
                }
            }
        } else {
            // Function identifier not found in user-defined scope, attempt to call as a built-in.
            self.eval_builtin(runtime_value, node, ident, args, env)
        }
    }

    /// Evaluates a call to a built-in (native) function.
    fn eval_builtin(
        &mut self,
        runtime_value: &RuntimeValue, // Current input value for the built-in.
        node: Rc<ast::Node>,          // AST node of the call, for error reporting.
        ident: &ast::Ident,           // Identifier of the built-in function.
        arg_expr_nodes: &ast::Args,   // Argument expressions (AST nodes) provided to the function.
        env: Rc<RefCell<Env>>,        // Current environment (for evaluating argument expressions).
    ) -> Result<RuntimeValue, EvalError> {
        // 1. Evaluate each argument expression in the current (caller's) environment.
        let evaluated_args: Result<builtin::Args, EvalError> = arg_expr_nodes
            .iter()
            .map(|arg_expr_node| {
                self.eval_expr(runtime_value, Rc::clone(arg_expr_node), Rc::clone(&env))
            })
            .collect(); // Collects into Result<Vec<RuntimeValue>, EvalError>, then `?` handles error.

        // 2. Call the appropriate built-in function handler with the evaluated arguments.
        builtin::eval_builtin(runtime_value, ident, &evaluated_args?)
            .map_err(|e| e.to_eval_error((*node).clone(), Rc::clone(&self.token_arena))) // Convert BuiltinError to EvalError.
    }

    /// Called before entering a new function scope to manage call stack depth.
    fn enter_scope(&mut self) -> Result<(), EvalError> {
        // Check for call stack overflow to prevent infinite recursion.
        if self.call_stack_depth >= self.options.max_call_stack_depth {
            return Err(EvalError::RecursionError(self.options.max_call_stack_depth));
        }
        self.call_stack_depth += 1;
        Ok(())
    }

    /// Called after exiting a function scope.
    fn exit_scope(&mut self) {
        // Decrement call stack depth. Should not go below zero if enter/exit are paired correctly.
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
            expr: Rc::new(ast::Expr::Call(ast::Ident::new(name), args, false)),
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
                    ast_node(ast::Expr::Literal(ast::Literal::String("te".to_string()))),
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                ]),
       ],
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                         name: "add".to_string(),
                                                         args: vec!["te".into(), 1.to_string().into()]})))]
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
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                         name: "sub".to_string(),
                                                         args: vec!["te".to_string().into(), 1.to_string().into()]})))]
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
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                         name: "div".to_string(),
                                                         args: vec!["te".to_string().into(), 1.to_string().into()]})))]
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
                    ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                ]),
       ],
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                         name: "mul".to_string(),
                                                         args: vec!["te".to_string().into(), 1.to_string().into()]})))]
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
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                    name: "mod".to_string(),
                                                    args: vec!["te".to_string().into(), 1.to_string().into()]})))]
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
       Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "test1\ntest2".to_string(), position: None}), None)]))]
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
    #[case::match_regex(vec![RuntimeValue::String("test123".to_string())],
       vec![
            ast_call("match", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::String(r"\d+".to_string()))),
            ])
       ],
       Ok(vec![RuntimeValue::Array(vec![RuntimeValue::String("123".to_string())])]))]
    #[case::match_regex(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "test123".to_string(), position: None}), None)],
       vec![
            ast_call("match", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::String(r"\d+".to_string()))),
            ])
       ],
       Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "123".to_string(), position: None}), None)]))]
    #[case::match_regex(vec![RuntimeValue::Number(123.into())],
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
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "65\n66\n67".to_string(), position: None}), None)]))]
    #[case::range(vec![RuntimeValue::Number(1.into())],
       vec![
            ast_call("range", smallvec![
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
            ast_call("range", smallvec![
                ast_node(ast::Expr::Literal(ast::Literal::Number(0.into()))),
            ])
       ],
       Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                    name: "range".to_string(),
                                                    args: vec!["1".to_string().into(), "0".to_string().into()]})))]
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
        Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                     name: "to_link".to_string(),
                                                     args: vec![123.to_string().into(), "Link Title".to_string().into(), "Link Value".to_string().into()]})))]
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
            mq_markdown::List{values: vec!["list".to_string().into()], index: 0, level: 1_u8, checked: None, position: None}), None)]))]
    #[case::to_md_list(vec![RuntimeValue::String("list".to_string())],
        vec![
              ast_call("to_md_list",
                       smallvec![
                             ast_node(ast::Expr::Literal(ast::Literal::Number(1.into()))),
                       ]),
        ],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::List(
            mq_markdown::List{values: vec!["list".to_string().into()], index: 0, level: 1_u8, checked: None, position: None}), None)]))]
    #[case::set_check(vec![RuntimeValue::Markdown(mq_markdown::Node::List(mq_markdown::List{values: vec!["Checked Item".to_string().into()], level: 0, index: 0, checked: None, position: None}), None)],
        vec![
              ast_call("set_check", smallvec![
                    ast_node(ast::Expr::Literal(ast::Literal::Bool(true))),
              ]),
        ],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::List(mq_markdown::List{values: vec!["Checked Item".to_string().into()], level: 0, index: 0, checked: Some(true), position: None}), None)]))]
    #[case::set_check(vec![RuntimeValue::Markdown(mq_markdown::Node::List(mq_markdown::List{values: vec!["Unchecked Item".to_string().into()], level: 0, index: 0, checked: None, position: None}), None)],
        vec![
              ast_call("set_check", smallvec![
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
    #[case::to_csv(vec![RuntimeValue::Array(vec![
            RuntimeValue::String("test1".to_string()),
            RuntimeValue::String("test2".to_string()),
            RuntimeValue::String("test3".to_string()),
        ])],
        vec![
            ast_call("to_csv", SmallVec::new())
        ],
        Ok(vec![RuntimeValue::String("test1,test2,test3".to_string())]))]
    #[case::to_csv(vec![RuntimeValue::String("test1".to_string())],
        vec![
            ast_call("to_csv", SmallVec::new())
        ],
        Ok(vec![RuntimeValue::String("test1".to_string())]))]
    #[case::to_csv_empty(vec![RuntimeValue::Array(Vec::new())],
        vec![
            ast_call("to_csv", SmallVec::new())
        ],
        Ok(vec![RuntimeValue::String("".to_string())]))]
    #[case::to_csv_mixed(vec![RuntimeValue::Array(vec![
            RuntimeValue::String("test1".to_string()),
            RuntimeValue::Number(42.into()),
            RuntimeValue::Bool(true),
        ])],
        vec![
            ast_call("to_csv", SmallVec::new())
        ],
        Ok(vec![RuntimeValue::String("test1,42,true".to_string())]))]
    #[case::to_tsv(vec![RuntimeValue::Array(vec![
            RuntimeValue::String("test1".to_string()),
            RuntimeValue::String("test2".to_string()),
            RuntimeValue::String("test3".to_string()),
        ])],
        vec![
            ast_call("to_tsv", SmallVec::new())
        ],
        Ok(vec![RuntimeValue::String("test1\ttest2\ttest3".to_string())]))]
    #[case::to_tsv(vec![RuntimeValue::String("test1".to_string())],
        vec![
            ast_call("to_tsv", SmallVec::new())
        ],
        Ok(vec![RuntimeValue::String("test1".to_string())]))]
    #[case::to_tsv_empty(vec![RuntimeValue::Array(Vec::new())],
        vec![
            ast_call("to_tsv", SmallVec::new())
        ],
        Ok(vec![RuntimeValue::String("".to_string())]))]
    #[case::to_tsv_mixed(vec![RuntimeValue::Array(vec![
            RuntimeValue::String("test1".to_string()),
            RuntimeValue::Number(42.into()),
            RuntimeValue::Bool(true),
        ])],
        vec![
            ast_call("to_tsv", SmallVec::new())
        ],
        Ok(vec![RuntimeValue::String("test1\t42\ttrue".to_string())]))]
    #[case::get_md_list_level(vec![RuntimeValue::Markdown(mq_markdown::Node::List(mq_markdown::List{values: vec!["List Item".to_string().into()], level: 1, index: 0, checked: None, position: None}), None)],
        vec![
            ast_call("get_md_list_level", SmallVec::new()),
        ],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "1".to_string(), position: None}), None)]))]
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
    #[case::nth_string(vec![RuntimeValue::String("test1".to_string())],
        vec![
            ast_call("nth", smallvec![ast_node(ast::Expr::Literal(ast::Literal::Number(0.into())))])
        ],
        Ok(vec![RuntimeValue::String("t".to_string())]))]
    #[case::nth_string(vec![RuntimeValue::String("test1".to_string())],
        vec![
            ast_call("nth", smallvec![ast_node(ast::Expr::Literal(ast::Literal::Number(5.into())))])
        ],
        Ok(vec![RuntimeValue::NONE]))]
    #[case::nth_array(vec![RuntimeValue::Array(vec!["test1".to_string().into()])],
        vec![
            ast_call("nth", smallvec![ast_node(ast::Expr::Literal(ast::Literal::Number(2.into())))])
        ],
        Ok(vec![RuntimeValue::NONE]))]
    #[case::nth(vec![RuntimeValue::TRUE],
        vec![
            ast_call("nth", smallvec![ast_node(ast::Expr::Literal(ast::Literal::Number(0.into())))])
        ],
        Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                     name: "nth".to_string(),
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
        Ok(vec![RuntimeValue::Array(vec!["test".to_string().into(), "1".to_string().into(), "2".to_string().into(), "false".to_string().into()])]))]
    #[case::to_string_empty_array(vec![RuntimeValue::Array(Vec::new())],
        vec![
            ast_call("to_string", SmallVec::new())
        ],
        Ok(vec![RuntimeValue::Array(Vec::new())]))]
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
    #[case::url_encode_error(vec![RuntimeValue::Number(1.into())],
        vec![
             ast_call("url_encode", SmallVec::new())
        ],
        Err(InnerError::Eval(EvalError::InvalidTypes{token: Token { range: Range::default(), kind: TokenKind::Eof, module_id: 1.into()},
                                                     name: "url_encode".to_string(),
                                                     args: vec![1.to_string().into()]})))]
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
                expr: Rc::new(ast::Expr::Call(
                    ast::Ident::new("func1"),
                    SmallVec::new(),
                    false,
                )),
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
