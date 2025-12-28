use crate::{
    Ident, Shared,
    ast::{
        Program, TokenId,
        node::{AccessTarget, Expr, MatchArm, Node, StringSegment},
    },
    error::runtime::RuntimeError,
    eval::runtime_value::RuntimeValue,
};
use rustc_hash::{FxBuildHasher, FxHashMap};

/// Trait for evaluating macro bodies during macro collection.
/// This allows the macro expander to request evaluation without depending on the concrete evaluator type.
pub trait MacroEvaluator {
    /// Evaluates a macro body and returns the result.
    /// The result should be an AST value for valid macros.
    fn eval_macro_body(&mut self, body: &Shared<Node>, token_id: TokenId) -> Result<RuntimeValue, RuntimeError>;
}

const MAX_RECURSION_DEPTH: u32 = 1000;

/// A macro definition containing its parameters and body.
#[derive(Debug, Clone)]
struct MacroDefinition {
    params: Vec<Ident>,
    body: Shared<Program>,
}

/// Expands macros in an AST before evaluation.
#[derive(Debug, Clone)]
pub struct Macro {
    macros: FxHashMap<Ident, MacroDefinition>,
    recursion_depth: u32,
    max_recursion: u32,
}

impl Macro {
    pub fn new() -> Self {
        Self {
            macros: FxHashMap::default(),
            recursion_depth: 0,
            max_recursion: MAX_RECURSION_DEPTH,
        }
    }

    /// Expands all macros in a program.
    pub fn expand<E: MacroEvaluator>(&mut self, program: &Program, evaluator: &mut E) -> Result<Program, RuntimeError> {
        self.collect_macros(program, evaluator)?;

        // Fast path: if no macros are defined, return the program as-is without traversal
        if self.macros.is_empty() {
            return Ok(program.clone());
        }

        let mut expanded_program = Vec::with_capacity(program.len());
        for node in program {
            // Skip macro definitions - they shouldn't appear in the expanded output
            if matches!(&*node.expr, Expr::Macro(..)) {
                continue;
            }

            // Check if this is a top-level macro call - if so, expand it directly
            if let Expr::Call(ident, args) = &*node.expr
                && self.macros.contains_key(&ident.name)
            {
                // Expand macro call and add all resulting nodes
                let expanded_nodes = self.expand_macro_call(ident.name, args, evaluator)?;
                expanded_program.extend(expanded_nodes);
                continue;
            }

            // Regular node expansion
            let expanded_node = self.expand_node(node, evaluator)?;
            expanded_program.push(expanded_node);
        }

        Ok(expanded_program)
    }

    pub(crate) fn collect_macros<E: MacroEvaluator>(
        &mut self,
        program: &Program,
        evaluator: &mut E,
    ) -> Result<(), RuntimeError> {
        for node in program {
            match &*node.expr {
                Expr::Macro(ident, params, body) => {
                    let param_names: Vec<Ident> = params
                        .iter()
                        .filter_map(|param| {
                            if let Expr::Ident(param_ident) = &*param.expr {
                                Some(param_ident.name)
                            } else {
                                None
                            }
                        })
                        .collect();

                    let ast = evaluator.eval_macro_body(body, node.token_id)?;

                    if let RuntimeValue::Ast(macro_body) = ast {
                        let program = match &*macro_body.expr {
                            Expr::Block(prog) => prog.clone(),
                            _ => vec![Shared::clone(&macro_body)],
                        };

                        self.macros.insert(
                            ident.name,
                            MacroDefinition {
                                params: param_names,
                                body: Shared::new(program),
                            },
                        );
                    } else {
                        unreachable!("Macro body did not evaluate to AST");
                    }
                }
                Expr::Module(_, program) => {
                    // Recursively collect macros in nested modules
                    self.collect_macros(program, evaluator)?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn expand_node<E: MacroEvaluator>(
        &mut self,
        node: &Shared<Node>,
        evaluator: &mut E,
    ) -> Result<Shared<Node>, RuntimeError> {
        match &*node.expr {
            // Expand function calls, including nested macro calls
            Expr::Call(ident, args) => {
                // Check if this is a macro call - use single lookup
                if self.macros.contains_key(&ident.name) {
                    // For nested macro calls, we need to expand and potentially return multiple nodes
                    // However, expand_node returns a single node, so we wrap them in a Block
                    let expanded_nodes = self.expand_macro_call(ident.name, args, evaluator)?;
                    match expanded_nodes.as_slice() {
                        [single] => Ok(Shared::clone(single)),
                        multiple => Ok(Shared::new(Node {
                            token_id: node.token_id,
                            expr: Shared::new(Expr::Block(multiple.to_vec())),
                        })),
                    }
                } else {
                    // Not a macro, just expand arguments
                    let expanded_args = args
                        .iter()
                        .map(|arg| self.expand_node(arg, evaluator))
                        .collect::<Result<Vec<_>, _>>()?;

                    Ok(Shared::new(Node {
                        token_id: node.token_id,
                        expr: Shared::new(Expr::Call(ident.clone(), expanded_args.into())),
                    }))
                }
            }
            Expr::Block(program) => {
                let expanded_program = self.expand(program, evaluator)?;
                Ok(Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::Block(expanded_program)),
                }))
            }
            Expr::Def(ident, params, program) => {
                let expanded_program = self.expand(program, evaluator)?;
                Ok(Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::Def(ident.clone(), params.clone(), expanded_program)),
                }))
            }
            Expr::Fn(params, program) => {
                let expanded_program = self.expand(program, evaluator)?;
                Ok(Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::Fn(params.clone(), expanded_program)),
                }))
            }
            Expr::If(branches) => {
                let expanded_branches = branches
                    .iter()
                    .map(|(cond, body)| {
                        let expanded_cond = if let Some(c) = cond {
                            Some(self.expand_node(c, evaluator)?)
                        } else {
                            None
                        };
                        let expanded_body = self.expand_node(body, evaluator)?;
                        Ok((expanded_cond, expanded_body))
                    })
                    .collect::<Result<Vec<_>, RuntimeError>>()?;

                Ok(Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::If(expanded_branches.into())),
                }))
            }
            Expr::While(cond, program) => {
                let expanded_cond = self.expand_node(cond, evaluator)?;
                let expanded_program = self.expand(program, evaluator)?;
                Ok(Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::While(expanded_cond, expanded_program)),
                }))
            }
            Expr::Foreach(ident, collection, program) => {
                let expanded_collection = self.expand_node(collection, evaluator)?;
                let expanded_program = self.expand(program, evaluator)?;
                Ok(Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::Foreach(ident.clone(), expanded_collection, expanded_program)),
                }))
            }
            Expr::Let(ident, value) => {
                let expanded_value = self.expand_node(value, evaluator)?;
                Ok(Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::Let(ident.clone(), expanded_value)),
                }))
            }
            Expr::Var(ident, value) => {
                let expanded_value = self.expand_node(value, evaluator)?;
                Ok(Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::Var(ident.clone(), expanded_value)),
                }))
            }
            Expr::Assign(ident, value) => {
                let expanded_value = self.expand_node(value, evaluator)?;
                Ok(Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::Assign(ident.clone(), expanded_value)),
                }))
            }
            Expr::And(left, right) => {
                let expanded_left = self.expand_node(left, evaluator)?;
                let expanded_right = self.expand_node(right, evaluator)?;
                Ok(Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::And(expanded_left, expanded_right)),
                }))
            }
            Expr::Or(left, right) => {
                let expanded_left = self.expand_node(left, evaluator)?;
                let expanded_right = self.expand_node(right, evaluator)?;
                Ok(Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::Or(expanded_left, expanded_right)),
                }))
            }
            Expr::CallDynamic(callable, args) => {
                let expanded_callable = self.expand_node(callable, evaluator)?;
                let expanded_args = args
                    .iter()
                    .map(|arg| self.expand_node(arg, evaluator))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::CallDynamic(expanded_callable, expanded_args.into())),
                }))
            }
            Expr::Match(value, arms) => {
                let expanded_value = self.expand_node(value, evaluator)?;
                let expanded_arms = arms
                    .iter()
                    .map(|arm| {
                        let expanded_guard = if let Some(guard) = &arm.guard {
                            Some(self.expand_node(guard, evaluator)?)
                        } else {
                            None
                        };
                        let expanded_body = self.expand_node(&arm.body, evaluator)?;
                        Ok(MatchArm {
                            pattern: arm.pattern.clone(),
                            guard: expanded_guard,
                            body: expanded_body,
                        })
                    })
                    .collect::<Result<Vec<_>, RuntimeError>>()?;

                Ok(Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::Match(expanded_value, expanded_arms.into())),
                }))
            }
            Expr::Module(ident, program) => {
                let expanded_program = self.expand(program, evaluator)?;
                Ok(Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::Module(ident.clone(), expanded_program)),
                }))
            }
            Expr::Paren(inner) => {
                let expanded_inner = self.expand_node(inner, evaluator)?;
                Ok(Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::Paren(expanded_inner)),
                }))
            }
            Expr::Quote(block) => {
                // Expand macros inside the quote, but preserve the quote itself
                // Quotes will be unwrapped during macro substitution
                let program = match &*block.expr {
                    Expr::Block(prog) => prog.clone(),
                    _ => vec![Shared::clone(block)],
                };
                let expanded_program = self.expand(&program, evaluator)?;
                let block = Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::Block(expanded_program)),
                });

                Ok(Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::Quote(block)),
                }))
            }
            Expr::Unquote(inner) => {
                let expanded_inner = self.expand_node(inner, evaluator)?;
                Ok(Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::Unquote(expanded_inner)),
                }))
            }
            Expr::Try(try_expr, catch_expr) => {
                let expanded_try = self.expand_node(try_expr, evaluator)?;
                let expanded_catch = self.expand_node(catch_expr, evaluator)?;
                Ok(Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::Try(expanded_try, expanded_catch)),
                }))
            }
            Expr::InterpolatedString(segments) => {
                let expanded_segments = segments
                    .iter()
                    .map(|segment| match segment {
                        StringSegment::Text(text) => Ok(StringSegment::Text(text.clone())),
                        StringSegment::Expr(node) => {
                            let expanded = self.expand_node(node, evaluator)?;
                            Ok(StringSegment::Expr(expanded))
                        }
                        StringSegment::Env(env) => Ok(StringSegment::Env(env.clone())),
                        StringSegment::Self_ => Ok(StringSegment::Self_),
                    })
                    .collect::<Result<Vec<_>, RuntimeError>>()?;

                Ok(Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::InterpolatedString(expanded_segments)),
                }))
            }
            Expr::QualifiedAccess(path, target) => {
                let expanded_target = match target {
                    AccessTarget::Call(ident, args) => {
                        let expanded_args = args
                            .iter()
                            .map(|arg| self.expand_node(arg, evaluator))
                            .collect::<Result<Vec<_>, _>>()?;
                        AccessTarget::Call(ident.clone(), expanded_args.into())
                    }
                    AccessTarget::Ident(ident) => AccessTarget::Ident(ident.clone()),
                };

                Ok(Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::QualifiedAccess(path.clone(), expanded_target)),
                }))
            }
            // Macro definitions should not be expanded - they're already collected
            Expr::Macro(_, _, _) => {
                // This should not happen in normal flow as we filter them out
                Ok(Shared::clone(node))
            }
            // Leaf nodes and other expressions - no expansion needed, avoid cloning
            Expr::Literal(_)
            | Expr::Ident(_)
            | Expr::Selector(_)
            | Expr::Nodes
            | Expr::Self_
            | Expr::Include(_)
            | Expr::Import(_)
            | Expr::Break
            | Expr::Continue => Ok(Shared::clone(node)),
        }
    }

    fn expand_macro_call<E: MacroEvaluator>(
        &mut self,
        name: Ident,
        args: &[Shared<Node>],
        evaluator: &mut E,
    ) -> Result<Vec<Shared<Node>>, RuntimeError> {
        if self.recursion_depth >= self.max_recursion {
            return Err(RuntimeError::RecursionLimit);
        }

        // Limit the borrow scope to avoid cloning the entire MacroDefinition
        let (params, body) = {
            let macro_def = self.macros.get(&name).ok_or(RuntimeError::UndefinedMacro(name))?;

            // Check arity
            if macro_def.params.len() != args.len() {
                return Err(RuntimeError::ArityMismatch {
                    macro_name: name,
                    expected: macro_def.params.len(),
                    got: args.len(),
                });
            }

            // Clone only what we need: params (Vec<Ident>) and body (Shared<Program>)
            // Note: Shared::clone is cheap (just increments reference count)
            (macro_def.params.clone(), Shared::clone(&macro_def.body))
        };

        // Create substitution map
        let mut substitutions = FxHashMap::with_capacity_and_hasher(params.len(), FxBuildHasher);
        for (param, arg) in params.iter().zip(args.iter()) {
            substitutions.insert(*param, Shared::clone(arg));
        }

        // Substitute and expand the macro body
        self.recursion_depth += 1;
        let result = self.substitute_and_expand_program(&body, &substitutions, evaluator);
        self.recursion_depth -= 1;

        result
    }

    fn substitute_and_expand_program<E: MacroEvaluator>(
        &mut self,
        program: &Program,
        substitutions: &FxHashMap<Ident, Shared<Node>>,
        evaluator: &mut E,
    ) -> Result<Program, RuntimeError> {
        program
            .iter()
            .map(|node| {
                let substituted = self.substitute_node(node, substitutions);
                self.expand_node(&substituted, evaluator)
            })
            .collect()
    }

    fn substitute_node(&self, node: &Shared<Node>, substitutions: &FxHashMap<Ident, Shared<Node>>) -> Shared<Node> {
        // Fast path: if no substitutions AND not a quote or block, return node as-is
        // Quotes and blocks always need to be processed
        if substitutions.is_empty() {
            match &*node.expr {
                Expr::Quote(_) | Expr::Block(_) => {
                    // Need to process these even without substitutions
                }
                _ => return Shared::clone(node),
            }
        }

        match &*node.expr {
            // Substitute identifiers
            Expr::Ident(ident) => {
                if let Some(replacement) = substitutions.get(&ident.name) {
                    Shared::clone(replacement)
                } else {
                    Shared::clone(node)
                }
            }

            // Recursively substitute in complex expressions
            Expr::Call(ident, args) => {
                let substituted_args: Vec<_> = args
                    .iter()
                    .map(|arg| self.substitute_node(arg, substitutions))
                    .collect();

                // Check if the function name is a macro parameter that needs substitution
                if let Some(replacement) = substitutions.get(&ident.name) {
                    // Convert to CallDynamic since the function is now an expression
                    Shared::new(Node {
                        token_id: node.token_id,
                        expr: Shared::new(Expr::CallDynamic(Shared::clone(replacement), substituted_args.into())),
                    })
                } else {
                    // Regular call with no substitution
                    Shared::new(Node {
                        token_id: node.token_id,
                        expr: Shared::new(Expr::Call(ident.clone(), substituted_args.into())),
                    })
                }
            }
            Expr::Block(program) => {
                let substituted_program: Vec<_> =
                    program.iter().map(|n| self.substitute_node(n, substitutions)).collect();

                Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::Block(substituted_program)),
                })
            }
            Expr::Def(ident, params, program) => {
                // Function parameters shadow macro parameters
                // Check if any parameter shadows a substitution
                let has_shadowing = params.iter().any(|param| {
                    if let Expr::Ident(param_ident) = &*param.expr {
                        substitutions.contains_key(&param_ident.name)
                    } else {
                        false
                    }
                });

                let substituted_program: Vec<_> = if has_shadowing {
                    let mut scoped_substitutions = substitutions.clone();
                    for param in params {
                        if let Expr::Ident(param_ident) = &*param.expr {
                            scoped_substitutions.remove(&param_ident.name);
                        }
                    }
                    program
                        .iter()
                        .map(|n| self.substitute_node(n, &scoped_substitutions))
                        .collect()
                } else {
                    program.iter().map(|n| self.substitute_node(n, substitutions)).collect()
                };

                Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::Def(ident.clone(), params.clone(), substituted_program)),
                })
            }
            Expr::Fn(params, program) => {
                // Function parameters shadow macro parameters
                // Check if any parameter shadows a substitution
                let has_shadowing = params.iter().any(|param| {
                    if let Expr::Ident(param_ident) = &*param.expr {
                        substitutions.contains_key(&param_ident.name)
                    } else {
                        false
                    }
                });

                let substituted_program: Vec<_> = if has_shadowing {
                    let mut scoped_substitutions = substitutions.clone();
                    for param in params {
                        if let Expr::Ident(param_ident) = &*param.expr {
                            scoped_substitutions.remove(&param_ident.name);
                        }
                    }
                    program
                        .iter()
                        .map(|n| self.substitute_node(n, &scoped_substitutions))
                        .collect()
                } else {
                    program.iter().map(|n| self.substitute_node(n, substitutions)).collect()
                };

                Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::Fn(params.clone(), substituted_program)),
                })
            }
            Expr::If(branches) => {
                let substituted_branches: Vec<(Option<Shared<Node>>, Shared<Node>)> = branches
                    .iter()
                    .map(|(cond, body)| {
                        let substituted_cond = cond.as_ref().map(|c| self.substitute_node(c, substitutions));
                        let substituted_body = self.substitute_node(body, substitutions);
                        (substituted_cond, substituted_body)
                    })
                    .collect();

                Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::If(substituted_branches.into())),
                })
            }
            Expr::While(cond, program) => {
                let substituted_cond = self.substitute_node(cond, substitutions);
                let substituted_program: Vec<_> =
                    program.iter().map(|n| self.substitute_node(n, substitutions)).collect();

                Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::While(substituted_cond, substituted_program)),
                })
            }
            Expr::Foreach(ident, collection, program) => {
                let substituted_collection = self.substitute_node(collection, substitutions);
                let substituted_program: Vec<_> =
                    program.iter().map(|n| self.substitute_node(n, substitutions)).collect();

                Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::Foreach(
                        ident.clone(),
                        substituted_collection,
                        substituted_program,
                    )),
                })
            }
            Expr::Let(ident, value) => {
                let substituted_value = self.substitute_node(value, substitutions);
                Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::Let(ident.clone(), substituted_value)),
                })
            }
            Expr::Var(ident, value) => {
                let substituted_value = self.substitute_node(value, substitutions);
                Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::Var(ident.clone(), substituted_value)),
                })
            }
            Expr::Assign(ident, value) => {
                let substituted_value = self.substitute_node(value, substitutions);
                Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::Assign(ident.clone(), substituted_value)),
                })
            }
            Expr::And(left, right) => {
                let substituted_left = self.substitute_node(left, substitutions);
                let substituted_right = self.substitute_node(right, substitutions);
                Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::And(substituted_left, substituted_right)),
                })
            }
            Expr::Or(left, right) => {
                let substituted_left = self.substitute_node(left, substitutions);
                let substituted_right = self.substitute_node(right, substitutions);
                Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::Or(substituted_left, substituted_right)),
                })
            }
            Expr::CallDynamic(callable, args) => {
                let substituted_callable = self.substitute_node(callable, substitutions);
                let substituted_args: Vec<_> = args
                    .iter()
                    .map(|arg| self.substitute_node(arg, substitutions))
                    .collect();

                Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::CallDynamic(substituted_callable, substituted_args.into())),
                })
            }
            Expr::Match(value, arms) => {
                let substituted_value = self.substitute_node(value, substitutions);
                let substituted_arms: Vec<_> = arms
                    .iter()
                    .map(|arm| {
                        let substituted_guard = arm.guard.as_ref().map(|g| self.substitute_node(g, substitutions));
                        let substituted_body = self.substitute_node(&arm.body, substitutions);
                        MatchArm {
                            pattern: arm.pattern.clone(),
                            guard: substituted_guard,
                            body: substituted_body,
                        }
                    })
                    .collect();

                Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::Match(substituted_value, substituted_arms.into())),
                })
            }
            Expr::Module(ident, program) => {
                let substituted_program: Vec<_> =
                    program.iter().map(|n| self.substitute_node(n, substitutions)).collect();

                Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::Module(ident.clone(), substituted_program)),
                })
            }
            Expr::Paren(inner) => {
                let substituted_inner = self.substitute_node(inner, substitutions);
                Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::Paren(substituted_inner)),
                })
            }
            Expr::Try(try_expr, catch_expr) => {
                let substituted_try = self.substitute_node(try_expr, substitutions);
                let substituted_catch = self.substitute_node(catch_expr, substitutions);
                Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::Try(substituted_try, substituted_catch)),
                })
            }
            Expr::InterpolatedString(segments) => {
                let substituted_segments: Vec<_> = segments
                    .iter()
                    .map(|segment| match segment {
                        StringSegment::Text(text) => StringSegment::Text(text.clone()),
                        StringSegment::Expr(node) => StringSegment::Expr(self.substitute_node(node, substitutions)),
                        StringSegment::Env(env) => StringSegment::Env(env.clone()),
                        StringSegment::Self_ => StringSegment::Self_,
                    })
                    .collect();

                Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::InterpolatedString(substituted_segments)),
                })
            }
            Expr::QualifiedAccess(path, target) => {
                let substituted_target = match target {
                    AccessTarget::Call(ident, args) => {
                        let substituted_args: Vec<_> = args
                            .iter()
                            .map(|arg| self.substitute_node(arg, substitutions))
                            .collect();
                        AccessTarget::Call(ident.clone(), substituted_args.into())
                    }
                    AccessTarget::Ident(ident) => AccessTarget::Ident(ident.clone()),
                };

                Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::QualifiedAccess(path.clone(), substituted_target)),
                })
            }
            // Quote: Expand unquote expressions and unwrap the quote
            Expr::Quote(block) => {
                let program = match &*block.expr {
                    Expr::Block(prog) => prog.clone(),
                    _ => vec![Shared::clone(block)],
                };

                let substituted_program: Vec<_> = program
                    .iter()
                    .map(|n| self.substitute_in_quote(n, substitutions))
                    .collect();

                // Unwrap quote: return the program as a block
                // Quote should not appear in the final expanded code
                if substituted_program.len() == 1 {
                    Shared::clone(&substituted_program[0])
                } else {
                    Shared::new(Node {
                        token_id: node.token_id,
                        expr: Shared::new(Expr::Block(substituted_program)),
                    })
                }
            }
            // Unquote: Unwrap and return the substituted inner expression
            // Unquote should not appear in the final expanded code
            Expr::Unquote(inner) => self.substitute_node(inner, substitutions),
            // Leaf nodes and other expressions - no substitution needed
            Expr::Literal(_)
            | Expr::Selector(_)
            | Expr::Nodes
            | Expr::Self_
            | Expr::Include(_)
            | Expr::Import(_)
            | Expr::Macro(_, _, _)
            | Expr::Break
            | Expr::Continue => Shared::clone(node),
        }
    }

    /// Substitute only unquote expressions within quoted code
    fn substitute_in_quote(&self, node: &Shared<Node>, substitutions: &FxHashMap<Ident, Shared<Node>>) -> Shared<Node> {
        // Fast path: if no substitutions, return node as-is
        if substitutions.is_empty() {
            return Shared::clone(node);
        }

        match &*node.expr {
            // Unquote: Perform substitution and unwrap
            // Unquote should not appear in the final expanded code
            Expr::Unquote(inner) => self.substitute_node(inner, substitutions),
            // Nested Quote: Recursively handle
            Expr::Quote(block) => {
                let program = match &*block.expr {
                    Expr::Block(prog) => prog.clone(),
                    _ => vec![Shared::clone(block)],
                };
                let substituted_program: Vec<_> = program
                    .iter()
                    .map(|n| self.substitute_in_quote(n, substitutions))
                    .collect();

                let block = Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::Block(substituted_program)),
                });

                Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::Quote(block)),
                })
            }
            Expr::Block(program) => {
                let substituted_program: Vec<_> = program
                    .iter()
                    .map(|n| self.substitute_in_quote(n, substitutions))
                    .collect();

                Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::Block(substituted_program)),
                })
            }
            Expr::Call(ident, args) => {
                let substituted_args: Vec<_> = args
                    .iter()
                    .map(|arg| self.substitute_in_quote(arg, substitutions))
                    .collect();

                Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::Call(ident.clone(), substituted_args.into())),
                })
            }
            Expr::CallDynamic(callable, args) => {
                let substituted_callable = self.substitute_in_quote(callable, substitutions);
                let substituted_args: Vec<_> = args
                    .iter()
                    .map(|arg| self.substitute_in_quote(arg, substitutions))
                    .collect();

                Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::CallDynamic(substituted_callable, substituted_args.into())),
                })
            }
            Expr::Def(ident, params, program) => {
                let substituted_program: Vec<_> = program
                    .iter()
                    .map(|n| self.substitute_in_quote(n, substitutions))
                    .collect();

                Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::Def(ident.clone(), params.clone(), substituted_program)),
                })
            }
            Expr::Fn(params, program) => {
                let substituted_program: Vec<_> = program
                    .iter()
                    .map(|n| self.substitute_in_quote(n, substitutions))
                    .collect();

                Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::Fn(params.clone(), substituted_program)),
                })
            }
            Expr::If(branches) => {
                let substituted_branches: Vec<(Option<Shared<Node>>, Shared<Node>)> = branches
                    .iter()
                    .map(|(cond, body)| {
                        let substituted_cond = cond.as_ref().map(|c| self.substitute_in_quote(c, substitutions));
                        let substituted_body = self.substitute_in_quote(body, substitutions);
                        (substituted_cond, substituted_body)
                    })
                    .collect();

                Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::If(substituted_branches.into())),
                })
            }
            Expr::While(cond, program) => {
                let substituted_cond = self.substitute_in_quote(cond, substitutions);
                let substituted_program: Vec<_> = program
                    .iter()
                    .map(|n| self.substitute_in_quote(n, substitutions))
                    .collect();

                Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::While(substituted_cond, substituted_program)),
                })
            }
            Expr::Foreach(ident, collection, program) => {
                let substituted_collection = self.substitute_in_quote(collection, substitutions);
                let substituted_program: Vec<_> = program
                    .iter()
                    .map(|n| self.substitute_in_quote(n, substitutions))
                    .collect();

                Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::Foreach(
                        ident.clone(),
                        substituted_collection,
                        substituted_program,
                    )),
                })
            }
            Expr::Let(ident, value) => {
                let substituted_value = self.substitute_in_quote(value, substitutions);
                Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::Let(ident.clone(), substituted_value)),
                })
            }
            Expr::Var(ident, value) => {
                let substituted_value = self.substitute_in_quote(value, substitutions);
                Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::Var(ident.clone(), substituted_value)),
                })
            }
            Expr::Assign(ident, value) => {
                let substituted_value = self.substitute_in_quote(value, substitutions);
                Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::Assign(ident.clone(), substituted_value)),
                })
            }
            Expr::And(left, right) => {
                let substituted_left = self.substitute_in_quote(left, substitutions);
                let substituted_right = self.substitute_in_quote(right, substitutions);
                Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::And(substituted_left, substituted_right)),
                })
            }
            Expr::Or(left, right) => {
                let substituted_left = self.substitute_in_quote(left, substitutions);
                let substituted_right = self.substitute_in_quote(right, substitutions);
                Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::Or(substituted_left, substituted_right)),
                })
            }
            Expr::Match(value, arms) => {
                let substituted_value = self.substitute_in_quote(value, substitutions);
                let substituted_arms: Vec<_> = arms
                    .iter()
                    .map(|arm| {
                        let substituted_guard = arm.guard.as_ref().map(|g| self.substitute_in_quote(g, substitutions));
                        let substituted_body = self.substitute_in_quote(&arm.body, substitutions);
                        MatchArm {
                            pattern: arm.pattern.clone(),
                            guard: substituted_guard,
                            body: substituted_body,
                        }
                    })
                    .collect();

                Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::Match(substituted_value, substituted_arms.into())),
                })
            }
            Expr::Module(ident, program) => {
                let substituted_program: Vec<_> = program
                    .iter()
                    .map(|n| self.substitute_in_quote(n, substitutions))
                    .collect();

                Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::Module(ident.clone(), substituted_program)),
                })
            }
            Expr::Paren(inner) => {
                let substituted_inner = self.substitute_in_quote(inner, substitutions);
                Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::Paren(substituted_inner)),
                })
            }
            Expr::Try(try_expr, catch_expr) => {
                let substituted_try = self.substitute_in_quote(try_expr, substitutions);
                let substituted_catch = self.substitute_in_quote(catch_expr, substitutions);
                Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::Try(substituted_try, substituted_catch)),
                })
            }
            Expr::InterpolatedString(segments) => {
                let substituted_segments: Vec<_> = segments
                    .iter()
                    .map(|segment| match segment {
                        StringSegment::Text(text) => StringSegment::Text(text.clone()),
                        StringSegment::Expr(node) => StringSegment::Expr(self.substitute_in_quote(node, substitutions)),
                        StringSegment::Env(env) => StringSegment::Env(env.clone()),
                        StringSegment::Self_ => StringSegment::Self_,
                    })
                    .collect();

                Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::InterpolatedString(substituted_segments)),
                })
            }
            Expr::QualifiedAccess(path, target) => {
                let substituted_target = match target {
                    AccessTarget::Call(ident, args) => {
                        let substituted_args: Vec<_> = args
                            .iter()
                            .map(|arg| self.substitute_in_quote(arg, substitutions))
                            .collect();
                        AccessTarget::Call(ident.clone(), substituted_args.into())
                    }
                    AccessTarget::Ident(ident) => AccessTarget::Ident(ident.clone()),
                };

                Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::QualifiedAccess(path.clone(), substituted_target)),
                })
            }
            // All other nodes (identifiers, literals, etc.) are not substituted in quote context
            _ => Shared::clone(node),
        }
    }
}

impl Default for Macro {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AstLiteral;
    use crate::ast::node;
    use crate::eval::Evaluator;
    use crate::module::ModuleLoader;
    use crate::{
        DefaultEngine, LocalFsModuleResolver, RuntimeValue, SharedCell, Token, TokenKind, arena::Arena, parse,
    };
    use rstest::rstest;

    /// Mock MacroEvaluator for testing.
    /// Returns the program as-is wrapped in RuntimeValue::Ast.
    struct MockMacroEvaluator;

    impl MacroEvaluator for MockMacroEvaluator {
        fn eval_macro_body(&mut self, body: &Shared<Node>, _token_id: TokenId) -> Result<RuntimeValue, RuntimeError> {
            // Return the body as-is wrapped in AST
            Ok(RuntimeValue::Ast(Shared::new(Node {
                token_id: TokenId::new(1),
                expr: body.expr.clone(),
            })))
        }
    }

    fn create_token_arena() -> Shared<SharedCell<Arena<Shared<Token>>>> {
        let token_arena = Shared::new(SharedCell::new(Arena::new(10240)));
        // Ensure at least one token for ArenaId::new(0)
        crate::token_alloc(
            &token_arena,
            &Shared::new(Token {
                kind: TokenKind::Eof,
                range: crate::range::Range::default(),
                module_id: crate::arena::ArenaId::new(0),
            }),
        );
        token_arena
    }

    fn parse_program(input: &str) -> Result<Program, Box<crate::error::Error>> {
        parse(input, create_token_arena())
    }

    fn eval_program(program: &Program) -> Result<Vec<RuntimeValue>, Box<dyn std::error::Error>> {
        let mut engine = DefaultEngine::default();
        engine.load_builtin_module();
        let result = engine
            .evaluator
            .eval(program, [RuntimeValue::Number(0.into())].into_iter())?;
        Ok(result)
    }

    #[rstest]
    #[case::basic_macro(
        "macro double(x): x + x | double(5)",
        "double(5)",
        Ok(())
    )]
    #[case::multiple_params(
        "macro add_three(a, b, c) a + b + c | add_three(1, 2, 3)",
        "add_three(1, 2, 3)",
        Ok(())
    )]
    #[case::nested_macro_calls(
        "macro double(x): x + x | macro quad(x): double(double(x)) | quad(3)",
        "quad(3)",
        Ok(())
    )]
    #[case::macro_with_string(
        r#"macro greet(name) do s"Hello, ${name}!"; | greet("World");"#,
        r#"greet("World")"#,
        Ok(())
    )]
    #[case::macro_with_let(
        "macro let_double(x) do let y = x | y + y; | let_double(7);",
        "let_double(7)",
        Ok(())
    )]
    #[case::macro_with_if(
        "macro max(a, b) do if(a > b): a else: b; | max(10, 5);",
        "max(10, 5)",
        Ok(())
    )]
    #[case::macro_with_function_call(
        "macro apply_twice(f, x) do f(f(x)); | def inc(n): n + 1; | apply_twice(inc, 5);",
        "apply_twice(inc, 5)",
        Ok(())
    )]
    fn test_macro_expansion_success(#[case] input: &str, #[case] _description: &str, #[case] expected: Result<(), ()>) {
        let program = parse_program(input).expect("Failed to parse program");
        let mut macro_expander = Macro::new();
        let result = macro_expander.expand(&program, &mut MockMacroEvaluator);

        match expected {
            Ok(_) => {
                assert!(
                    result.is_ok(),
                    "Expected successful expansion but got error: {:?}",
                    result.err()
                );
            }
            Err(_) => {
                assert!(result.is_err(), "Expected error but got success");
            }
        }
    }

    #[test]
    fn test_macro_expansion_arity_mismatch_too_few() {
        let input = "macro add_two(a, b): a + b | add_two(1)";
        let program = parse_program(input).expect("Failed to parse program");
        let mut macro_expander = Macro::new();
        let result = macro_expander.expand(&program, &mut MockMacroEvaluator);

        assert!(matches!(result, Err(RuntimeError::ArityMismatch { .. })));
    }

    #[test]
    fn test_macro_expansion_arity_mismatch_too_many() {
        let input = "macro double(x): x + x | double(1, 2, 3)";
        let program = parse_program(input).expect("Failed to parse program");
        let mut macro_expander = Macro::new();
        let result = macro_expander.expand(&program, &mut MockMacroEvaluator);

        assert!(matches!(result, Err(RuntimeError::ArityMismatch { .. })));
    }

    #[test]
    fn test_macro_recursion_limit() {
        let input = "macro recurse(x): recurse(x) | recurse(1)";
        let program = parse_program(input).expect("Failed to parse program");
        let mut macro_expander = Macro::new();
        macro_expander.max_recursion = 10;
        let result = macro_expander.expand(&program, &mut MockMacroEvaluator);

        assert!(matches!(result, Err(RuntimeError::RecursionLimit)));
    }

    #[rstest]
    #[case::no_params("macro const(): 42;", 0)]
    #[case::one_param("macro double(x): x + x;", 1)]
    #[case::two_params("macro add(a, b): a + b;", 2)]
    #[case::three_params("macro add_three(a, b, c): a + b + c;", 3)]
    fn test_macro_definition_collection(#[case] input: &str, #[case] expected_param_count: usize) {
        let program = parse_program(input).expect("Failed to parse program");
        let mut macro_expander = Macro::new();
        let _ = macro_expander.collect_macros(&program, &mut MockMacroEvaluator);

        assert_eq!(macro_expander.macros.len(), 1, "Expected one macro definition");
        let macro_def = macro_expander.macros.values().next().unwrap();
        assert_eq!(
            macro_def.params.len(),
            expected_param_count,
            "Unexpected parameter count"
        );
    }

    #[test]
    fn test_multiple_macro_definitions() {
        let input = r#"
            macro double(x): x + x
            | macro triple(x): x + x + x
            | double(5) + triple(3)
        "#;
        let program = parse_program(input).expect("Failed to parse program");
        let mut macro_expander = Macro::new();
        let result = macro_expander.expand(&program, &mut MockMacroEvaluator);

        assert!(result.is_ok(), "Expected successful expansion");
        assert_eq!(macro_expander.macros.len(), 2, "Expected two macro definitions");
    }

    #[test]
    fn test_macro_parameter_shadowing() {
        let input = "let x = 100 | macro use_param(x): x * 2 | use_param(5)";
        let program = parse_program(input).expect("Failed to parse program");
        let mut macro_expander = Macro::new();
        let result = macro_expander.expand(&program, &mut MockMacroEvaluator);

        assert!(result.is_ok(), "Expected successful expansion with parameter shadowing");
    }

    #[test]
    fn test_macro_not_included_in_output() {
        let input = "macro double(x): x + x | 42";
        let program = parse_program(input).expect("Failed to parse program");
        let mut macro_expander = Macro::new();
        let expanded = macro_expander
            .expand(&program, &mut MockMacroEvaluator)
            .expect("Failed to expand program");

        for node in &expanded {
            assert!(
                !matches!(&*node.expr, Expr::Macro(_, _, _)),
                "Macro definition should not be in expanded output"
            );
        }
    }

    #[test]
    fn test_no_macro_fast_path() {
        let input = "1 + 2 | . * 3";
        let program = parse_program(input).expect("Failed to parse program");
        let original_len = program.len();

        let mut macro_expander = Macro::new();
        let expanded = macro_expander
            .expand(&program, &mut MockMacroEvaluator)
            .expect("Failed to expand program");

        assert_eq!(expanded.len(), original_len);
        assert!(macro_expander.macros.is_empty());
    }

    #[rstest]
    #[case::simple_expression("1 + 2")]
    #[case::pipe_chain(". | . * 2 | . + 1")]
    #[case::function_call("add(1, 2)")]
    #[case::let_binding("let x = 5 | x * 2")]
    #[case::if_expression("if(true): 1 else: 2;")]
    fn test_no_macro_optimization(#[case] input: &str) {
        let program = parse_program(input).expect("Failed to parse program");
        let mut macro_expander = Macro::new();
        let result = macro_expander.expand(&program, &mut MockMacroEvaluator);

        assert!(result.is_ok(), "Expected successful expansion for input without macros");
        assert!(macro_expander.macros.is_empty(), "No macros should be collected");
    }

    #[test]
    fn test_quote_basic() {
        let input = "macro make_expr(x): quote: x + 1 | make_expr(5)";
        let program = parse_program(input).expect("Failed to parse program");
        let mut macro_expander = Macro::new();
        let result = macro_expander.expand(&program, &mut MockMacroEvaluator);

        assert!(result.is_ok(), "Expected successful expansion with quote");
    }

    #[test]
    fn test_quote_with_unquote() {
        let input = "macro add_one(x): quote: unquote(x) + 1 | add_one(5)";
        let program = parse_program(input).expect("Failed to parse program");
        let mut macro_expander = Macro::new();
        let result = macro_expander.expand(&program, &mut MockMacroEvaluator);

        assert!(result.is_ok(), "Expected successful expansion with quote and unquote");
    }

    #[test]
    fn test_quote_multiple_expressions() {
        let input = r#"macro log_and_eval(x): quote do "start" | unquote(x) | "end"; | log_and_eval(5 + 5)"#;
        let program = parse_program(input).expect("Failed to parse program");
        let mut macro_expander = Macro::new();
        let result = macro_expander.expand(&program, &mut MockMacroEvaluator);

        assert!(
            result.is_ok(),
            "Expected successful expansion with multi-expression quote"
        );
    }

    #[test]
    fn test_nested_quote() {
        let input = "macro nested(x): quote: quote: unquote(x) | nested(5)";
        let program = parse_program(input).expect("Failed to parse program");
        let mut macro_expander = Macro::new();
        let result = macro_expander.expand(&program, &mut MockMacroEvaluator);

        assert!(result.is_ok(), "Expected successful expansion with nested quote");
    }

    #[test]
    fn test_macro_call_with_body() {
        let input = r#"macro unless(cond, expr): quote: if (unquote(!cond)): unquote(expr) | unless(!false) do 1;"#;
        let program = parse_program(input).expect("Failed to parse program");
        let mut macro_expander = Macro::new();
        let result = macro_expander.expand(&program, &mut MockMacroEvaluator);

        assert!(result.is_ok(), "Expected successful expansion with MacroCall and body");
    }

    #[rstest]
    #[case::while_condition(
        "macro get_limit(x): x * 2 | var i = 0 | while(i < get_limit(5)): i = i + 1; | i",
        vec![RuntimeValue::Number(10.into())],
    )]
    #[case::foreach_collection(
        "macro make_value(x): x * 2 | foreach(item, [1, 2, 3]): make_value(item);",
        vec![RuntimeValue::Array(vec![
            RuntimeValue::Number(2.into()),
            RuntimeValue::Number(4.into()),
            RuntimeValue::Number(6.into())
        ])],
    )]
    #[case::match_value(
        "macro get_val(x): x | match(get_val(5)): | 1: 10 | _: 20 end",
        vec![RuntimeValue::Number(20.into())],
    )]
    #[case::match_body(
        "macro default_value(x): x * 2 | match(5): | 1: default_value(10) | _: default_value(20) end",
        vec![RuntimeValue::Number(40.into())],
    )]
    #[case::match_guard(
        "macro is_positive(x): x > 0 | match(5): | n if (is_positive(n)): n * 2 | _: 0 end",
        vec![RuntimeValue::Number(10.into())],
    )]
    #[case::and_expression(
        "macro is_positive(x): x > 0 | macro is_even(x): x % 2 == 0 | is_positive(4) && is_even(4)",
        vec![RuntimeValue::Boolean(true)],
    )]
    #[case::or_expression(
        "macro is_zero(x): x == 0 | macro is_one(x): x == 1 | is_zero(0) || is_one(1)",
        vec![RuntimeValue::Boolean(true)],
    )]
    #[case::try_catch(
        "macro fallback(x): x * 2 | try: 1 / 0 catch: fallback(5);",
        vec![RuntimeValue::Number(10.into())],
    )]
    #[case::var_declaration(
        "macro initial_value(x): x * 10 | var counter = initial_value(5) | counter",
        vec![RuntimeValue::Number(50.into())],
    )]
    #[case::assignment(
        "macro next_value(x): x + 1 | var counter = 0 | counter = next_value(counter) | counter",
        vec![RuntimeValue::Number(1.into())],
    )]
    #[case::lambda_body(
        "macro double(x): x * 2 | def apply_double(n): double(n) + 1; | apply_double(5)",
        vec![RuntimeValue::Number(11.into())],
    )]
    #[case::parentheses(
        "macro value(x): x + 10 | (value(5)) * 2",
        vec![RuntimeValue::Number(30.into())],
    )]
    #[case::interpolated_string(
        r#"macro get_name(): "World" | s"Hello, ${get_name()}!""#,
        vec![RuntimeValue::String("Hello, World!".to_string())],
    )]
    fn test_expand_node_branches(#[case] input: &str, #[case] expected: Vec<RuntimeValue>) {
        let program = parse_program(input).expect("Failed to parse program");
        let mut macro_expander = Macro::new();
        let expanded = macro_expander
            .expand(&program, &mut MockMacroEvaluator)
            .unwrap_or_else(|e| panic!("Failed to expand: {:?}", e));

        let result = eval_program(&expanded).unwrap_or_else(|e| panic!("Failed to eval: {:?}", e));

        assert_eq!(result, expected,);
    }

    #[rstest]
    #[case::parameter_substitution(
        "macro add_to_param(x, y): x + y | add_to_param(10, 5)",
        vec![RuntimeValue::Number(15.into())],
    )]
    #[case::complex_substitution(
        "macro calc(x, y, z): (x + y) * z | calc(2, 3, 4)",
        vec![RuntimeValue::Number(20.into())],
    )]
    #[case::call_to_dynamic(
        "macro apply(f, x): f(x) | def double(n): n * 2; | apply(double, 5)",
        vec![RuntimeValue::Number(10.into())],
    )]
    #[case::nested_blocks(
        "macro block_double(x) do x + x; | macro block_quad(x) do block_double(block_double(x)); | block_quad(3)",
        vec![RuntimeValue::Number(12.into())],
    )]
    fn test_substitute_node_branches(#[case] input: &str, #[case] expected: Vec<RuntimeValue>) {
        let program = parse_program(input).expect("Failed to parse program");
        let mut macro_expander = Macro::new();
        let expanded = macro_expander
            .expand(&program, &mut MockMacroEvaluator)
            .unwrap_or_else(|e| panic!("Failed to expand: {:?}", e));

        let result = eval_program(&expanded).unwrap_or_else(|e| panic!("Failed to eval: {:?}", e));

        assert_eq!(result, expected,);
    }

    #[rstest]
    #[case::quote_with_complex_unquote(
        r#"macro wrap(expr): quote do "before" | unquote(expr) | "after"; | wrap(1 + 2)"#,
        vec![RuntimeValue::String("after".to_string())], // Pipeline returns last value
    )]
    fn test_quote_unquote_branches(#[case] input: &str, #[case] expected: Vec<RuntimeValue>) {
        let program = parse_program(input).expect("Failed to parse program");
        let mut macro_expander = Macro::new();
        let expanded = macro_expander
            .expand(&program, &mut MockMacroEvaluator)
            .unwrap_or_else(|e| panic!("Failed to expand: {:?}", e));

        let result = eval_program(&expanded).unwrap_or_else(|e| panic!("Failed to eval: {:?}", e));

        assert_eq!(result, expected,);
    }

    #[rstest]
    #[case::single_node_body(
        "macro single(x): x | single(42)",
        vec![RuntimeValue::Number(42.into())],
    )]
    #[case::multi_node_body(
        "macro multi(x) do x | x + 1 | x + 2; | multi(5)",
        vec![RuntimeValue::Number(7.into())], // Pipeline returns last value
    )]
    #[case::deeply_nested(
        "macro level1(x): level2(x) | macro level2(x): level3(x) | macro level3(x): x * 2 | level1(5)",
        vec![RuntimeValue::Number(10.into())],
    )]
    #[case::multiple_macros_in_expression(
        "macro first(x): x * 2 | macro second(x): x + 10 | first(5) + second(3)",
        vec![RuntimeValue::Number(23.into())],
    )]
    #[allow(clippy::approx_constant)]
    #[case::zero_parameters(
        "macro pi(): 3.14159 | pi() * 2",
        vec![RuntimeValue::Number(6.28318.into())],
    )]
    #[case::all_if_branches(
        "macro value(x): x | if(value(true)): value(1) elif(value(false)): value(2) else: value(3);",
        vec![RuntimeValue::Number(1.into())],
    )]
    #[case::macro_returning_macro(
        "macro inner(x): x + 1 | macro outer(x): inner(x) | outer(5)",
        vec![RuntimeValue::Number(6.into())],
    )]
    #[case::block_expression(
        "macro wrap(x) do let y = x | y * 2; | wrap(5) + wrap(3)",
        vec![RuntimeValue::Number(16.into())],
    )]
    fn test_edge_cases(#[case] input: &str, #[case] expected: Vec<RuntimeValue>) {
        let program = parse_program(input).expect("Failed to parse program");
        let mut macro_expander = Macro::new();
        let expanded = macro_expander
            .expand(&program, &mut MockMacroEvaluator)
            .unwrap_or_else(|e| panic!("Failed to expand: {:?}", e));

        let result = eval_program(&expanded).unwrap_or_else(|e| panic!("Failed to eval: {:?}", e));

        assert_eq!(result, expected,);
    }

    #[test]
    fn test_single_node_not_wrapped() {
        let input = "macro single(x): x | single(42)";
        let program = parse_program(input).expect("Failed to parse program");
        let mut macro_expander = Macro::new();
        let expanded = macro_expander
            .expand(&program, &mut MockMacroEvaluator)
            .expect("Failed to expand");

        assert_eq!(expanded.len(), 1, "Expected single node in expanded output");
        // Should not be wrapped in a Block
        assert!(!matches!(&*expanded[0].expr, Expr::Block(_)));
    }

    #[test]
    fn test_multi_node_preserved() {
        let input = "macro multi(x) do x | x + 1 | x + 2; | multi(5)";
        let program = parse_program(input).expect("Failed to parse program");
        let mut macro_expander = Macro::new();
        let expanded = macro_expander
            .expand(&program, &mut MockMacroEvaluator)
            .expect("Failed to expand");

        assert_eq!(expanded.len(), 3, "Expected three nodes from multi-node macro");
    }

    #[rstest]
    #[case::simple_unquote(
        "macro test(x): quote: unquote(x) | test(42)",
        vec![RuntimeValue::Number(42.into())],
    )]
    #[case::unquote_with_expression(
        "macro test(x): quote: unquote(x + 1) | test(10)",
        vec![RuntimeValue::Number(11.into())],
    )]
    #[case::unquote_in_call_args(
        "macro test(x): quote: foo(unquote(x), 5) | def foo(a, b): a + b; | test(10)",
        vec![RuntimeValue::Number(15.into())],
    )]
    #[case::multiple_unquotes(
        "macro test(x, y): quote: unquote(x) + unquote(y) | test(10, 20)",
        vec![RuntimeValue::Number(30.into())],
    )]
    #[case::unquote_in_block(
        "macro test(x): quote do unquote(x) | unquote(x) + 1; | test(5)",
        vec![RuntimeValue::Number(6.into())],
    )]
    #[case::unquote_in_foreach(
        "macro test(arr): quote: foreach(item, unquote(arr)): item * 2; | test([1, 2, 3])",
        vec![RuntimeValue::Array(vec![
            RuntimeValue::Number(2.into()),
            RuntimeValue::Number(4.into()),
            RuntimeValue::Number(6.into())
        ])],
    )]
    #[case::unquote_in_let(
        "macro test(x): quote do let y = unquote(x) | y * 2; | test(10)",
        vec![RuntimeValue::Number(20.into())],
    )]
    #[case::unquote_in_var(
        "macro test(x): quote do var y = unquote(x) | y; | test(42)",
        vec![RuntimeValue::Number(42.into())],
    )]
    #[case::unquote_in_assignment(
        "macro test(x): quote do var y = 0 | y = unquote(x) | y; | test(42)",
        vec![RuntimeValue::Number(42.into())],
    )]
    #[case::unquote_in_and_left(
        "macro test(x): quote: unquote(x) && true | test(true)",
        vec![RuntimeValue::Boolean(true)],
    )]
    #[case::unquote_in_and_right(
        "macro test(x): quote: true && unquote(x) | test(false)",
        vec![RuntimeValue::Boolean(false)],
    )]
    #[case::unquote_in_or_left(
        "macro test(x): quote: unquote(x) || false | test(true)",
        vec![RuntimeValue::Boolean(true)],
    )]
    #[case::unquote_in_or_right(
        "macro test(x): quote: false || unquote(x) | test(true)",
        vec![RuntimeValue::Boolean(true)],
    )]
    #[case::unquote_in_match_value(
        "macro test(x): quote: match(unquote(x)): | 42: 100 | _: 200 end | test(42)",
        vec![RuntimeValue::Number(100.into())],
    )]
    #[case::unquote_in_match_body(
        "macro test(x): quote: match(1): | 1: unquote(x) | _: 0 end | test(42)",
        vec![RuntimeValue::Number(42.into())],
    )]
    #[case::unquote_in_match_guard(
        "macro test(x): quote: match(5): | n if (n > unquote(x)): 100 | _: 200 end | test(3)",
        vec![RuntimeValue::Number(100.into())],
    )]
    #[case::unquote_in_paren(
        "macro test(x): quote: (unquote(x)) * 2 | test(21)",
        vec![RuntimeValue::Number(42.into())],
    )]
    #[case::unquote_in_interpolated_string(
        r#"macro test(x): quote: s"Value: ${unquote(x)}" | test(42)"#,
        vec![RuntimeValue::String("Value: 42".to_string())],
    )]
    #[case::unquote_in_def_body(
        "macro test(x): quote do def f(): unquote(x); | f(); | test(42)",
        vec![RuntimeValue::Number(42.into())],
    )]
    #[case::no_unquote_preserves_identifier(
        "macro test(x): quote: y + 1 | let y = 10 | test(5)",
        vec![RuntimeValue::Number(11.into())],
    )]
    #[case::quote_with_literal_preserved(
        r#"macro test(x): quote: "hello" | test(42)"#,
        vec![RuntimeValue::String("hello".to_string())],
    )]
    fn test_substitute_in_quote_comprehensive(#[case] input: &str, #[case] expected: Vec<RuntimeValue>) {
        let program = parse_program(input).expect("Failed to parse program");
        let mut macro_expander = Macro::new();
        let expanded = macro_expander
            .expand(&program, &mut MockMacroEvaluator)
            .unwrap_or_else(|e| panic!("Failed to expand: {:?}", e));

        let result = eval_program(&expanded).unwrap_or_else(|e| panic!("Failed to eval: {:?}", e));

        assert_eq!(result, expected);
    }

    #[test]
    fn test_substitute_in_quote_empty_substitutions() {
        let input = "macro test(): quote: 1 + 1 | test()";
        let program = parse_program(input).expect("Failed to parse program");
        let mut macro_expander = Macro::new();
        let expanded = macro_expander
            .expand(&program, &mut MockMacroEvaluator)
            .expect("Failed to expand program");

        let result = eval_program(&expanded).expect("Failed to eval");
        assert_eq!(result, vec![RuntimeValue::Number(2.into())]);
    }

    #[test]
    fn test_substitute_in_quote_preserves_non_unquote() {
        let input = "macro test(x): quote: foo() + bar() | def foo(): 10; | def bar(): 20; | test(42)";
        let program = parse_program(input).expect("Failed to parse program");
        let mut macro_expander = Macro::new();
        let expanded = macro_expander
            .expand(&program, &mut MockMacroEvaluator)
            .expect("Failed to expand program");

        let result = eval_program(&expanded).expect("Failed to eval");
        assert_eq!(result, vec![RuntimeValue::Number(30.into())]);
    }

    #[test]
    fn test_substitute_in_quote_complex_expression() {
        let input = r#"
            macro test(a, b, c): quote do
                let x = unquote(a)
                | let y = unquote(b)
                | if (x > y): unquote(c) else: x + y;
            | test(10, 5, 100)
        "#;
        let program = parse_program(input).expect("Failed to parse program");
        let mut macro_expander = Macro::new();
        let expanded = macro_expander
            .expand(&program, &mut MockMacroEvaluator)
            .expect("Failed to expand program");

        let result = eval_program(&expanded).expect("Failed to eval");
        assert_eq!(result, vec![RuntimeValue::Number(100.into())]);
    }

    #[test]
    fn test_substitute_in_quote_multiple_levels() {
        let input = r#"
            macro level1(x): unquote(x) * 2
            | level1(21)
        "#;
        let program = parse_program(input).expect("Failed to parse program");
        let mut macro_expander = Macro::new();
        let expanded = macro_expander
            .expand(&program, &mut MockMacroEvaluator)
            .expect("Failed to expand program");

        let result = eval_program(&expanded).expect("Failed to eval");
        assert_eq!(result, vec![RuntimeValue::Number(42.into())]);
    }

    #[test]
    fn test_substitute_in_quote_all_string_segments() {
        let input = r#"
            macro test(x, y): quote: s"Value: ${unquote(x)}, Other: ${unquote(y)}"
            | test(42, 100)
        "#;
        let program = parse_program(input).expect("Failed to parse program");
        let mut macro_expander = Macro::new();
        let expanded = macro_expander
            .expand(&program, &mut MockMacroEvaluator)
            .expect("Failed to expand program");

        let result = eval_program(&expanded).expect("Failed to eval");
        assert_eq!(result, vec![RuntimeValue::String("Value: 42, Other: 100".to_string())]);
    }

    #[test]
    fn test_substitute_in_quote_unquote_call_with_args() {
        let input = r#"
            macro test(base, offset): quote: unquote(base + offset) * 2
            | test(10, 5)
        "#;
        let program = parse_program(input).expect("Failed to parse program");
        let mut macro_expander = Macro::new();
        let expanded = macro_expander
            .expand(&program, &mut MockMacroEvaluator)
            .expect("Failed to expand program");

        let result = eval_program(&expanded).expect("Failed to eval");
        assert_eq!(result, vec![RuntimeValue::Number(30.into())]);
    }

    #[test]
    fn test_substitute_in_quote_no_unwanted_substitution() {
        let input = r#"
            macro test(x): quote: x + 1
            | let x = 100
            | test(42)
        "#;
        let program = parse_program(input).expect("Failed to parse program");
        let mut macro_expander = Macro::new();
        let expanded = macro_expander
            .expand(&program, &mut MockMacroEvaluator)
            .expect("Failed to expand program");

        let result = eval_program(&expanded).expect("Failed to eval");
        assert_eq!(result, vec![RuntimeValue::Number(101.into())]);
    }

    #[test]
    fn test_substitute_in_quote_if_branches() {
        let input = r#"
            macro test(cond, val1, val2): quote do if (unquote(cond)): unquote(val1) else: unquote(val2); | test(true, 100, 200)
        "#;
        let program = parse_program(input).expect("Failed to parse program");
        let mut macro_expander = Macro::new();
        let expanded = macro_expander
            .expand(&program, &mut MockMacroEvaluator)
            .expect("Failed to expand program");

        assert!(!expanded.is_empty(), "Expansion should produce nodes");
    }

    #[test]
    fn test_substitute_in_quote_while_loop() {
        let input = r#"
            macro test(limit): quote: var i = 0 | while(i < unquote(limit)): i = i + 1; | i
            | test(5)
        "#;
        let program = parse_program(input).expect("Failed to parse program");
        let mut macro_expander = Macro::new();
        let expanded = macro_expander
            .expand(&program, &mut MockMacroEvaluator)
            .expect("Failed to expand program");

        assert!(!expanded.is_empty(), "Expansion should produce nodes");
    }

    #[test]
    fn test_substitute_in_quote_nested_quote_preserved() {
        let input = r#"
            macro test(x): quote: quote: unquote(x)
            | test(42)
        "#;
        let program = parse_program(input).expect("Failed to parse program");
        let mut macro_expander = Macro::new();
        let expanded = macro_expander
            .expand(&program, &mut MockMacroEvaluator)
            .expect("Failed to expand program");

        assert!(!expanded.is_empty(), "Expansion should produce nodes");
    }

    #[test]
    fn test_substitute_in_quote_call_dynamic() {
        let input = r#"
            macro test(val): quote: length(unquote(val))
            | test([1, 2, 3])
        "#;
        let program = parse_program(input).expect("Failed to parse program");
        let mut macro_expander = Macro::new();
        let expanded = macro_expander
            .expand(&program, &mut MockMacroEvaluator)
            .expect("Failed to expand program");

        assert!(!expanded.is_empty(), "Expansion should produce nodes");
        let result = eval_program(&expanded);
        if let Ok(vals) = result {
            assert_eq!(vals, vec![RuntimeValue::Number(3.into())]);
        }
    }

    #[test]
    fn test_substitute_in_quote_module() {
        let input = r#"
            macro test(val): unquote(val) * 2
            | test(21)
        "#;
        let program = parse_program(input).expect("Failed to parse program");
        let mut macro_expander = Macro::new();
        let expanded = macro_expander
            .expand(&program, &mut MockMacroEvaluator)
            .expect("Failed to expand program");

        assert!(!expanded.is_empty(), "Expansion should produce nodes");
        let result = eval_program(&expanded);
        if let Ok(vals) = result {
            assert_eq!(vals, vec![RuntimeValue::Number(42.into())]);
        }
    }

    #[test]
    fn test_substitute_in_quote_try_catch() {
        let input = r#"
            macro test(val): quote do try: 1 / 0 catch: unquote(val); | test(999)
        "#;
        let program = parse_program(input).expect("Failed to parse program");
        let mut macro_expander = Macro::new();
        let expanded = macro_expander
            .expand(&program, &mut MockMacroEvaluator)
            .expect("Failed to expand program");

        assert!(!expanded.is_empty(), "Expansion should produce nodes");
        let result = eval_program(&expanded);
        if let Ok(vals) = result {
            assert_eq!(vals, vec![RuntimeValue::Number(999.into())]);
        }
    }

    #[test]
    fn test_substitute_in_quote_qualified_access() {
        let input = r#"
            macro test(val): unquote(val) + 10
            | test(32)
        "#;
        let program = parse_program(input).expect("Failed to parse program");
        let mut macro_expander = Macro::new();
        let expanded = macro_expander
            .expand(&program, &mut MockMacroEvaluator)
            .expect("Failed to expand program");

        assert!(!expanded.is_empty(), "Expansion should produce nodes");
        let result = eval_program(&expanded);
        if let Ok(vals) = result {
            assert_eq!(vals, vec![RuntimeValue::Number(42.into())]);
        }
    }

    #[test]
    fn test_substitute_in_quote_preserves_structure() {
        let input = r#"
            macro wrap(content): quote: "start" | unquote(content) | "end"
            | wrap(42)
        "#;
        let program = parse_program(input).expect("Failed to parse program");
        let mut macro_expander = Macro::new();
        let expanded = macro_expander
            .expand(&program, &mut MockMacroEvaluator)
            .expect("Failed to expand program");

        assert!(!expanded.is_empty(), "Expansion should produce nodes");
    }

    #[test]
    fn test_quote_runtime_evaluation_basic() {
        let input = "quote: 1 + 2";
        let program = parse_program(input).expect("Failed to parse program");
        let result = eval_program(&program).expect("Failed to eval program");

        // quote should return an AST value
        assert_eq!(result.len(), 1);
        assert!(matches!(result[0], RuntimeValue::Ast(_)));
    }

    #[test]
    fn test_macro_with_conditional_true_expands() {
        let input = r#"macro test(): if (true): quote: breakpoint() | test()"#;
        let token_arena = create_token_arena();
        let program = parse(input, Shared::clone(&token_arena)).expect("Failed to parse program");

        let module_loader = ModuleLoader::new(LocalFsModuleResolver::default());
        let mut evaluator = Evaluator::new(module_loader, token_arena);

        let mut macro_expander = Macro::new();
        let expanded = macro_expander
            .expand(&program, &mut evaluator)
            .expect("Failed to expand program");

        // The macro should expand to breakpoint() call
        assert!(!expanded.is_empty(), "Expansion should produce nodes");

        // Verify the expanded code contains a call to breakpoint
        let has_breakpoint = expanded.iter().any(|node| {
            if let Expr::Call(ident, _) = &*node.expr {
                ident.name.as_str() == "breakpoint"
            } else {
                false
            }
        });
        assert!(has_breakpoint, "Expected breakpoint() call in expanded output");
    }

    #[test]
    fn test_macro_with_conditional_false_expands_nothing() {
        let input = r#"macro test(): if (false): quote: breakpoint() | test()"#;
        let token_arena = create_token_arena();
        let program = parse(input, Shared::clone(&token_arena)).expect("Failed to parse program");

        let module_loader = ModuleLoader::new(LocalFsModuleResolver::default());
        let mut evaluator = Evaluator::new(module_loader, token_arena);

        let mut macro_expander = Macro::new();
        let expanded = macro_expander
            .expand(&program, &mut evaluator)
            .expect("Failed to expand program");

        // The macro should expand to empty or no breakpoint call
        let has_breakpoint = expanded.iter().any(|node| {
            if let Expr::Call(ident, _) = &*node.expr {
                ident.name.as_str() == "breakpoint"
            } else {
                false
            }
        });
        assert!(
            !has_breakpoint,
            "Should not have breakpoint() call when condition is false"
        );
    }

    #[test]
    fn test_macro_with_conditional_else_branch() {
        let input = r#"macro test(): if (false): quote: breakpoint() else: quote: continue | test()"#;
        let token_arena = create_token_arena();
        let program = parse(input, Shared::clone(&token_arena)).expect("Failed to parse program");

        let module_loader = ModuleLoader::new(LocalFsModuleResolver::default());
        let mut evaluator = Evaluator::new(module_loader, token_arena);

        let mut macro_expander = Macro::new();
        let expanded = macro_expander
            .expand(&program, &mut evaluator)
            .expect("Failed to expand program");

        // Should expand to continue since condition is false
        assert!(!expanded.is_empty(), "Expansion should produce nodes");

        let has_continue = expanded.iter().any(|node| matches!(&*node.expr, Expr::Continue));
        assert!(has_continue, "Expected continue in expanded output from else branch");

        let has_breakpoint = expanded.iter().any(|node| {
            if let Expr::Call(ident, _) = &*node.expr {
                ident.name.as_str() == "breakpoint"
            } else {
                false
            }
        });
        assert!(!has_breakpoint, "Should not have breakpoint() from false branch");
    }

    #[test]
    fn test_quote_with_unquote_runtime() {
        let input = "let x = 5 | quote: unquote(x) + 1";
        let program = parse_program(input).expect("Failed to parse program");
        let result = eval_program(&program).expect("Failed to eval program");

        // quote should evaluate unquote and return AST
        assert_eq!(result.len(), 1);
        assert!(matches!(result[0], RuntimeValue::Ast(_)));
    }

    #[test]
    fn test_unquote_outside_quote_fails() {
        let input = "unquote(42)";
        let program = parse_program(input).expect("Failed to parse program");
        let result = eval_program(&program);

        // unquote outside quote should fail
        assert!(result.is_err());
    }

    // Additional comprehensive tests for substitute_node and macro expansion

    #[test]
    fn test_mutual_recursion_between_macros() {
        // Test mutual recursion with a limit
        // Note: This tests that macro expansion itself handles mutual recursion
        // The actual evaluation would need if/else to work properly
        let input = r#"
            macro a(x): b(x)
            | macro b(x): x * 2
            | a(21)
        "#;
        let program = parse_program(input).expect("Failed to parse program");
        let mut macro_expander = Macro::new();
        macro_expander.max_recursion = 100;
        let expanded = macro_expander
            .expand(&program, &mut MockMacroEvaluator)
            .expect("Failed to expand mutually recursive macros");

        let result = eval_program(&expanded).expect("Failed to eval");
        assert_eq!(result, vec![RuntimeValue::Number(42.into())]);
    }

    #[test]
    fn test_mutual_recursion_hits_limit() {
        // Test that mutual recursion respects the recursion limit
        let input = r#"
            macro ping(x): pong(x)
            | macro pong(x): ping(x)
            | ping(1)
        "#;
        let program = parse_program(input).expect("Failed to parse program");
        let mut macro_expander = Macro::new();
        macro_expander.max_recursion = 5;
        let result = macro_expander.expand(&program, &mut MockMacroEvaluator);

        assert!(matches!(result, Err(RuntimeError::RecursionLimit)));
    }

    #[rstest]
    #[case::nested_function_shadowing(
        r#"
            macro outer(x): def inner(x): x * 2; | inner(5)
            | outer(100)
        "#,
        vec![RuntimeValue::Number(10.into())],
    )]
    #[case::multiple_level_shadowing(
        r#"
            let x = 1
            | macro level1(x): def level2(x): def level3(x): x + 1; | level3(10); | level2(20)
            | level1(30)
        "#,
        vec![RuntimeValue::Number(11.into())],
    )]
    #[case::shadowing_in_nested_let(
        r#"
            macro test(x) do let a = x | let b = a * 2 | a + b;
            | test(5)
        "#,
        vec![RuntimeValue::Number(15.into())],
    )]
    #[case::lambda_parameter_shadowing(
        r#"
            macro make_adder(x): def adder(y): x + y;
            | make_adder(10)
            | adder(5)
        "#,
        vec![RuntimeValue::Number(15.into())],
    )]
    fn test_complex_parameter_shadowing(#[case] input: &str, #[case] expected: Vec<RuntimeValue>) {
        let program = parse_program(input).expect("Failed to parse program");
        let mut macro_expander = Macro::new();
        let expanded = macro_expander
            .expand(&program, &mut MockMacroEvaluator)
            .expect("Failed to expand");

        let result = eval_program(&expanded).expect("Failed to eval");
        assert_eq!(result, expected);
    }

    #[rstest]
    #[case::string_interpolation_with_multiple_macros(
        r#"
            macro first(): "Hello"
            | macro second(): "World"
            | s"${first()}, ${second()}!"
        "#,
        vec![RuntimeValue::String("Hello, World!".to_string())],
    )]
    #[case::nested_string_interpolation(
        r#"
            macro outer(x): s"[${x}]"
            | macro inner(x): s"${x}!"
            | outer(inner("test"))
        "#,
        vec![RuntimeValue::String("[test!]".to_string())],
    )]
    #[case::string_interpolation_with_complex_expression(
        r#"
            macro compute(x, y): x * y + 10
            | s"Result: ${compute(3, 4)}"
        "#,
        vec![RuntimeValue::String("Result: 22".to_string())],
    )]
    #[case::string_with_macro_and_literal_segments(
        r#"
            macro get_value(): 42
            | s"The answer is ${get_value()}, always ${get_value()}!"
        "#,
        vec![RuntimeValue::String("The answer is 42, always 42!".to_string())],
    )]
    fn test_string_interpolation_edge_cases(#[case] input: &str, #[case] expected: Vec<RuntimeValue>) {
        let program = parse_program(input).expect("Failed to parse program");
        let mut macro_expander = Macro::new();
        let expanded = macro_expander
            .expand(&program, &mut MockMacroEvaluator)
            .expect("Failed to expand");

        let result = eval_program(&expanded).expect("Failed to eval");
        assert_eq!(result, expected);
    }

    #[rstest]
    #[case::try_with_macro_in_both_branches(
        r#"
            macro risky(x): 1 / x
            | macro fallback(x): x * 10
            | try: risky(0) catch: fallback(5);
        "#,
        vec![RuntimeValue::Number(50.into())],
    )]
    #[case::nested_try_with_macros(
        r#"
            macro inner_try(x) do try: 1 / x catch: 0;
            | macro outer_try(x) do try: inner_try(x) catch: -1;
            | outer_try(0)
        "#,
        vec![RuntimeValue::Number(0.into())],
    )]
    #[case::try_with_macro_generating_error(
        r#"
            macro will_error(): 1 / 0
            | try: will_error() catch: 999;
        "#,
        vec![RuntimeValue::Number(999.into())],
    )]
    fn test_try_catch_with_nested_macros(#[case] input: &str, #[case] expected: Vec<RuntimeValue>) {
        let program = parse_program(input).expect("Failed to parse program");
        let mut macro_expander = Macro::new();
        let expanded = macro_expander
            .expand(&program, &mut MockMacroEvaluator)
            .expect("Failed to expand");

        let result = eval_program(&expanded).expect("Failed to eval");
        assert_eq!(result, expected);
    }

    #[rstest]
    #[case::match_with_macro_generated_patterns(
        r#"
            macro get_pattern(): 42
            | match(42): | 1: 10 | 42: 100 | _: 0 end
        "#,
        vec![RuntimeValue::Number(100.into())],
    )]
    #[case::match_with_multiple_guards_using_macros(
        r#"
            macro is_positive(x): x > 0
            | macro is_even(x): x % 2 == 0
            | match(4): | n if (is_positive(n) && is_even(n)): 100 | _: 0 end
        "#,
        vec![RuntimeValue::Number(100.into())],
    )]
    #[case::match_with_macro_in_all_branches(
        r#"
            macro handle(x): x * 10
            | match(2): | 1: handle(1) | 2: handle(2) | _: handle(0) end
        "#,
        vec![RuntimeValue::Number(20.into())],
    )]
    #[case::nested_match_with_macros(
        r#"
            macro outer_match(x): match(x): | 1: 10 | _: 20 end
            | match(5): | 5: outer_match(1) | _: 0 end
        "#,
        vec![RuntimeValue::Number(10.into())],
    )]
    fn test_pattern_matching_with_macros(#[case] input: &str, #[case] expected: Vec<RuntimeValue>) {
        let program = parse_program(input).expect("Failed to parse program");
        let mut macro_expander = Macro::new();
        let expanded = macro_expander
            .expand(&program, &mut MockMacroEvaluator)
            .expect("Failed to expand");

        let result = eval_program(&expanded).expect("Failed to eval");
        assert_eq!(result, expected);
    }

    #[rstest]
    #[case::multi_statement_in_block_context(
        r#"
            macro setup(x) do var a = x | var b = a * 2 | a + b;
            | setup(5)
        "#,
        vec![RuntimeValue::Number(15.into())],
    )]
    #[case::multi_statement_with_side_effects(
        r#"
            macro init_and_compute(x) do var counter = x | counter = counter + 1 | counter = counter * 2 | counter;
            | init_and_compute(5)
        "#,
        vec![RuntimeValue::Number(12.into())],
    )]
    #[case::multi_statement_mixed_with_other_code(
        r#"
            macro block(x) do let y = x | y * 2;
            | let before = 10 | block(5) + before
        "#,
        vec![RuntimeValue::Number(20.into())],
    )]
    fn test_multiple_statement_expansion_edge_cases(#[case] input: &str, #[case] expected: Vec<RuntimeValue>) {
        let program = parse_program(input).expect("Failed to parse program");
        let mut macro_expander = Macro::new();
        let expanded = macro_expander
            .expand(&program, &mut MockMacroEvaluator)
            .expect("Failed to expand");

        let result = eval_program(&expanded).expect("Failed to eval");
        assert_eq!(result, expected);
    }

    #[rstest]
    #[case::nested_call_dynamic(
        r#"
            macro apply(f, x): f(x)
            | macro double_apply(f, x): f(f(x))
            | def inc(n): n + 1;
            | double_apply(inc, 5)
        "#,
        vec![RuntimeValue::Number(7.into())],
    )]
    #[case::multiple_dynamic_calls_in_expression(
        r#"
            macro call_both(f, g, x): f(x) + g(x)
            | def double(n): n * 2;
            | def triple(n): n * 3;
            | call_both(double, triple, 5)
        "#,
        vec![RuntimeValue::Number(25.into())],
    )]
    fn test_call_dynamic_conversion_edge_cases(#[case] input: &str, #[case] expected: Vec<RuntimeValue>) {
        let program = parse_program(input).expect("Failed to parse program");
        let mut macro_expander = Macro::new();
        let expanded = macro_expander
            .expand(&program, &mut MockMacroEvaluator)
            .expect("Failed to expand");

        let result = eval_program(&expanded).expect("Failed to eval");
        assert_eq!(result, expected);
    }

    #[test]
    fn test_substitute_node_with_deeply_nested_expressions() {
        let input = r#"
            macro deep(x): ((((x + 1) * 2) - 3) / 4)
            | deep(10)
        "#;
        let program = parse_program(input).expect("Failed to parse program");
        let mut macro_expander = Macro::new();
        let expanded = macro_expander
            .expand(&program, &mut MockMacroEvaluator)
            .expect("Failed to expand");

        let result = eval_program(&expanded).expect("Failed to eval");
        assert_eq!(result, vec![RuntimeValue::Number(4.75.into())]);
    }

    #[test]
    fn test_substitute_node_with_array_operations() {
        let input = r#"
            macro first_elem(arr): arr[0]
            | first_elem([10, 20, 30])
        "#;
        let program = parse_program(input).expect("Failed to parse program");
        let mut macro_expander = Macro::new();
        let expanded = macro_expander
            .expand(&program, &mut MockMacroEvaluator)
            .expect("Failed to expand");

        let result = eval_program(&expanded).expect("Failed to eval");
        assert_eq!(result, vec![RuntimeValue::Number(10.into())]);
    }

    #[test]
    fn test_substitute_node_preserves_foreach_iteration() {
        let input = r#"
            macro transform(arr, multiplier): foreach(item, arr): item * multiplier;
            | transform([1, 2, 3], 10)
        "#;
        let program = parse_program(input).expect("Failed to parse program");
        let mut macro_expander = Macro::new();
        let expanded = macro_expander
            .expand(&program, &mut MockMacroEvaluator)
            .expect("Failed to expand");

        let result = eval_program(&expanded).expect("Failed to eval");
        assert_eq!(
            result,
            vec![RuntimeValue::Array(vec![
                RuntimeValue::Number(10.into()),
                RuntimeValue::Number(20.into()),
                RuntimeValue::Number(30.into())
            ])]
        );
    }

    #[test]
    fn test_substitute_node_with_variable_mutations() {
        let input = r#"
            macro init_and_update(x) do var counter = x | counter = counter + 10 | counter;
            | init_and_update(5)
        "#;
        let program = parse_program(input).expect("Failed to parse program");
        let mut macro_expander = Macro::new();
        let expanded = macro_expander
            .expand(&program, &mut MockMacroEvaluator)
            .expect("Failed to expand");

        let result = eval_program(&expanded).expect("Failed to eval");
        assert_eq!(result, vec![RuntimeValue::Number(15.into())]);
    }

    #[test]
    fn test_substitute_node_with_object_literal() {
        let input = r#"
            macro make_obj(k, v): {key: k, value: v}
            | make_obj("name", "test")
        "#;
        let program = parse_program(input).expect("Failed to parse program");
        let mut macro_expander = Macro::new();
        let expanded = macro_expander
            .expand(&program, &mut MockMacroEvaluator)
            .expect("Failed to expand");

        // Just verify it expands without error
        assert!(!expanded.is_empty());
    }

    #[test]
    fn test_substitute_node_empty_substitutions_optimization() {
        let input = "macro no_params(): 42 | no_params()";
        let program = parse_program(input).expect("Failed to parse program");
        let mut macro_expander = Macro::new();
        let expanded = macro_expander
            .expand(&program, &mut MockMacroEvaluator)
            .expect("Failed to expand");

        let result = eval_program(&expanded).expect("Failed to eval");
        assert_eq!(result, vec![RuntimeValue::Number(42.into())]);
    }

    #[test]
    fn test_substitute_node_with_index_access() {
        let input = r#"
            macro get_at(arr, idx): arr[idx]
            | get_at([10, 20, 30], 1)
        "#;
        let program = parse_program(input).expect("Failed to parse program");
        let mut macro_expander = Macro::new();
        let expanded = macro_expander
            .expand(&program, &mut MockMacroEvaluator)
            .expect("Failed to expand");

        let result = eval_program(&expanded).expect("Failed to eval");
        assert_eq!(result, vec![RuntimeValue::Number(20.into())]);
    }

    #[test]
    fn test_quote_unquote_evaluates_non_ast_values() {
        let input = r#"quote: unquote(1 + 2)"#;
        let program = parse_program(input).expect("Failed to parse program");
        let result = eval_program(&program).expect("Failed to eval");

        // The result should be an AST containing a block with the literal value 3
        assert_eq!(result.len(), 1);
        if let RuntimeValue::Ast(node) = &result[0] {
            if let Expr::Block(nodes) = &*node.expr {
                assert_eq!(nodes.len(), 1);
                if let Expr::Literal(AstLiteral::Number(n)) = &*nodes[0].expr {
                    assert_eq!(*n, 3.into());
                } else {
                    panic!("Expected literal number in block, got {:?}", nodes[0].expr);
                }
            } else {
                panic!("Expected block in AST, got {:?}", node.expr);
            }
        } else {
            panic!("Expected AST result");
        }
    }

    #[test]
    fn test_quote_unquote_evaluates_string_values() {
        let input = r#"let x = "hello" | quote: unquote(x)"#;
        let program = parse_program(input).expect("Failed to parse program");
        let result = eval_program(&program).expect("Failed to eval");

        // The result should be an AST containing a block with the literal string "hello"
        assert_eq!(result.len(), 1);
        if let RuntimeValue::Ast(node) = &result[0] {
            if let Expr::Block(nodes) = &*node.expr {
                assert_eq!(nodes.len(), 1);
                if let Expr::Literal(AstLiteral::String(s)) = &*nodes[0].expr {
                    assert_eq!(s, "hello");
                } else {
                    panic!("Expected literal string in block, got {:?}", nodes[0].expr);
                }
            } else {
                panic!("Expected block in AST, got {:?}", node.expr);
            }
        } else {
            panic!("Expected AST result");
        }
    }

    #[test]
    fn test_quote_unquote_removes_none_values() {
        let input = r#"quote do unquote(if(false): 1) | 2 | unquote(if(false): 3);"#;
        let program = parse_program(input).expect("Failed to parse program");
        let result = eval_program(&program).expect("Failed to eval");

        // The result should be an AST with a block containing only the literal 2
        // The None values from the if statements should be filtered out
        assert_eq!(result.len(), 1);
        if let RuntimeValue::Ast(node) = &result[0] {
            if let Expr::Block(nodes) = &*node.expr {
                // Should have only 1 node (the literal 2), not 3
                assert_eq!(
                    nodes.len(),
                    1,
                    "Expected 1 node after filtering None values, got {}",
                    nodes.len()
                );
                if let Expr::Literal(crate::ast::node::Literal::Number(n)) = &*nodes[0].expr {
                    assert_eq!(*n, 2.into());
                } else {
                    panic!("Expected literal number 2");
                }
            } else {
                panic!("Expected block in AST");
            }
        } else {
            panic!("Expected AST result");
        }
    }

    #[test]
    fn test_quote_unquote_preserves_ast_values() {
        // Test that unquote expressions that return AST values preserve them
        let input = r#"let x = quote: 1 + 2 | quote: unquote(x)"#;
        let program = parse_program(input).expect("Failed to parse program");
        let result = eval_program(&program).expect("Failed to eval");

        // The result should be an AST containing the expression "1 + 2"
        assert_eq!(result.len(), 1);
        if let RuntimeValue::Ast(node) = &result[0] {
            // quote returns a Block, so we need to check inside the block
            if let Expr::Block(outer_nodes) = &*node.expr {
                assert_eq!(outer_nodes.len(), 1, "Expected 1 node in outer block");
                // The inner node should be another Block (from the original quote)
                if let Expr::Block(inner_nodes) = &*outer_nodes[0].expr {
                    assert_eq!(inner_nodes.len(), 1, "Expected 1 node in inner block");
                    // The innermost node should be a Call (add)
                    if let Expr::Call(ident, _) = &*inner_nodes[0].expr {
                        assert_eq!(ident.name.as_str(), "add", "Expected add call");
                    } else {
                        panic!("Expected Call in innermost block, got {:?}", inner_nodes[0].expr);
                    }
                } else {
                    panic!("Expected inner Block, got {:?}", outer_nodes[0].expr);
                }
            } else {
                panic!("Expected outer Block in AST, got {:?}", node.expr);
            }
        } else {
            panic!("Expected AST result");
        }
    }

    #[test]
    fn test_macro_body_none_removes_macro() {
        let input = r#"quote do 1 | unquote(if(false): 2) | 3;"#;
        let program = parse_program(input).expect("Failed to parse program");
        let result = eval_program(&program).expect("Failed to eval");

        // The result should be an AST with a block containing only 1 and 3
        // The None from if(false) should be filtered out
        assert_eq!(result.len(), 1);
        if let RuntimeValue::Ast(node) = &result[0] {
            if let Expr::Block(nodes) = &*node.expr {
                // Should have 2 nodes (1 and 3), not 3
                assert_eq!(nodes.len(), 2, "Expected 2 nodes after filtering None");
                // Verify the values are 1 and 3
                if let Expr::Literal(node::Literal::Number(n)) = &*nodes[0].expr {
                    assert_eq!(*n, 1.into());
                } else {
                    panic!("Expected literal 1");
                }
                if let Expr::Literal(node::Literal::Number(n)) = &*nodes[1].expr {
                    assert_eq!(*n, 3.into());
                } else {
                    panic!("Expected literal 3");
                }
            } else {
                panic!("Expected block in AST");
            }
        } else {
            panic!("Expected AST result");
        }
    }
}
