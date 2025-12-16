use crate::arena::ArenaId;
use crate::{
    Ident, Shared,
    ast::{
        Program,
        node::{AccessTarget, Expr, MatchArm, Node, StringSegment},
    },
};
use rustc_hash::FxHashMap;
use thiserror::Error;

const MAX_RECURSION_DEPTH: u32 = 1000;

#[derive(Error, Debug, Clone, PartialEq)]
pub enum MacroExpansionError {
    /// A macro was called but not defined
    #[error("Undefined macro: {0}")]
    UndefinedMacro(Ident),
    /// Wrong number of arguments passed to a macro
    #[error("Macro {macro_name} expects {expected} arguments, got {got}")]
    ArityMismatch {
        macro_name: Ident,
        expected: usize,
        got: usize,
    },
    /// Maximum recursion depth exceeded
    #[error("Maximum macro recursion depth exceeded")]
    RecursionLimit,
}

/// A macro definition containing its parameters and body.
#[derive(Debug, Clone)]
struct MacroDefinition {
    params: Vec<Ident>,
    body: Program,
}

/// Expands macros in an AST before evaluation.
#[derive(Debug, Clone)]
pub struct MacroExpander {
    macros: FxHashMap<Ident, MacroDefinition>,
    recursion_depth: u32,
    max_recursion: u32,
}

impl MacroExpander {
    pub fn new() -> Self {
        Self {
            macros: FxHashMap::default(),
            recursion_depth: 0,
            max_recursion: MAX_RECURSION_DEPTH,
        }
    }

    /// Expands all macros in a program.
    pub fn expand_program(&mut self, program: &Program) -> Result<Program, MacroExpansionError> {
        // Collect macro definitions
        self.collect_macros(program);

        // Expand all nodes and filter out macro definitions
        let mut expanded_program = Vec::with_capacity(program.len());
        for node in program {
            // Skip macro definitions - they shouldn't appear in the expanded output
            if matches!(&*node.expr, Expr::Macro(_, _, _)) {
                continue;
            }

            let expanded_node = self.expand_node(node)?;
            expanded_program.push(expanded_node);
        }

        Ok(expanded_program)
    }

    /// Collects all macro definitions from the program.
    fn collect_macros(&mut self, program: &Program) {
        for node in program {
            if let Expr::Macro(ident, params, body) = &*node.expr {
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

                self.macros.insert(
                    ident.name,
                    MacroDefinition {
                        params: param_names,
                        body: body.clone(),
                    },
                );
            }
        }
    }

    /// Recursively expands a single node.
    ///
    /// If the node is a macro call, it expands the macro.
    /// Otherwise, it recursively processes child nodes.
    fn expand_node(&mut self, node: &Shared<Node>) -> Result<Shared<Node>, MacroExpansionError> {
        match &*node.expr {
            // Expand macro calls
            Expr::Call(ident, args) => {
                // Check if this is a macro call
                if self.macros.contains_key(&ident.name) {
                    self.expand_macro_call(ident.name, args)
                } else {
                    // Not a macro, just expand arguments
                    let expanded_args = args
                        .iter()
                        .map(|arg| self.expand_node(arg))
                        .collect::<Result<Vec<_>, _>>()?;

                    Ok(Shared::new(Node {
                        token_id: node.token_id,
                        expr: Shared::new(Expr::Call(ident.clone(), expanded_args.into())),
                    }))
                }
            }

            // Recursively expand other expressions
            Expr::Block(program) => {
                let expanded_program = self.expand_program(program)?;
                Ok(Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::Block(expanded_program)),
                }))
            }

            Expr::Def(ident, params, program) => {
                let expanded_program = self.expand_program(program)?;
                Ok(Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::Def(ident.clone(), params.clone(), expanded_program)),
                }))
            }

            Expr::Fn(params, program) => {
                let expanded_program = self.expand_program(program)?;
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
                            Some(self.expand_node(c)?)
                        } else {
                            None
                        };
                        let expanded_body = self.expand_node(body)?;
                        Ok((expanded_cond, expanded_body))
                    })
                    .collect::<Result<Vec<_>, MacroExpansionError>>()?;

                Ok(Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::If(expanded_branches.into())),
                }))
            }

            Expr::While(cond, program) => {
                let expanded_cond = self.expand_node(cond)?;
                let expanded_program = self.expand_program(program)?;
                Ok(Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::While(expanded_cond, expanded_program)),
                }))
            }

            Expr::Foreach(ident, collection, program) => {
                let expanded_collection = self.expand_node(collection)?;
                let expanded_program = self.expand_program(program)?;
                Ok(Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::Foreach(ident.clone(), expanded_collection, expanded_program)),
                }))
            }

            Expr::Let(ident, value) => {
                let expanded_value = self.expand_node(value)?;
                Ok(Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::Let(ident.clone(), expanded_value)),
                }))
            }

            Expr::Var(ident, value) => {
                let expanded_value = self.expand_node(value)?;
                Ok(Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::Var(ident.clone(), expanded_value)),
                }))
            }

            Expr::Assign(ident, value) => {
                let expanded_value = self.expand_node(value)?;
                Ok(Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::Assign(ident.clone(), expanded_value)),
                }))
            }

            Expr::And(left, right) => {
                let expanded_left = self.expand_node(left)?;
                let expanded_right = self.expand_node(right)?;
                Ok(Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::And(expanded_left, expanded_right)),
                }))
            }

            Expr::Or(left, right) => {
                let expanded_left = self.expand_node(left)?;
                let expanded_right = self.expand_node(right)?;
                Ok(Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::Or(expanded_left, expanded_right)),
                }))
            }

            Expr::CallDynamic(callable, args) => {
                let expanded_callable = self.expand_node(callable)?;
                let expanded_args = args
                    .iter()
                    .map(|arg| self.expand_node(arg))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::CallDynamic(expanded_callable, expanded_args.into())),
                }))
            }

            Expr::Match(value, arms) => {
                let expanded_value = self.expand_node(value)?;
                let expanded_arms = arms
                    .iter()
                    .map(|arm| {
                        let expanded_guard = if let Some(guard) = &arm.guard {
                            Some(self.expand_node(guard)?)
                        } else {
                            None
                        };
                        let expanded_body = self.expand_node(&arm.body)?;
                        Ok(MatchArm {
                            pattern: arm.pattern.clone(),
                            guard: expanded_guard,
                            body: expanded_body,
                        })
                    })
                    .collect::<Result<Vec<_>, MacroExpansionError>>()?;

                Ok(Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::Match(expanded_value, expanded_arms.into())),
                }))
            }

            Expr::Module(ident, program) => {
                let expanded_program = self.expand_program(program)?;
                Ok(Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::Module(ident.clone(), expanded_program)),
                }))
            }

            Expr::Paren(inner) => {
                let expanded_inner = self.expand_node(inner)?;
                Ok(Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::Paren(expanded_inner)),
                }))
            }

            Expr::Try(try_expr, catch_expr) => {
                let expanded_try = self.expand_node(try_expr)?;
                let expanded_catch = self.expand_node(catch_expr)?;
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
                            let expanded = self.expand_node(node)?;
                            Ok(StringSegment::Expr(expanded))
                        }
                        StringSegment::Env(env) => Ok(StringSegment::Env(env.clone())),
                        StringSegment::Self_ => Ok(StringSegment::Self_),
                    })
                    .collect::<Result<Vec<_>, MacroExpansionError>>()?;

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
                            .map(|arg| self.expand_node(arg))
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

            // Leaf nodes - no expansion needed
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

    /// Expands a macro call by substituting arguments into the macro body.
    fn expand_macro_call(&mut self, name: Ident, args: &[Shared<Node>]) -> Result<Shared<Node>, MacroExpansionError> {
        // Check recursion depth
        if self.recursion_depth >= self.max_recursion {
            return Err(MacroExpansionError::RecursionLimit);
        }

        // Get macro definition
        let macro_def = self
            .macros
            .get(&name)
            .ok_or(MacroExpansionError::UndefinedMacro(name))?
            .clone();

        // Check arity
        if macro_def.params.len() != args.len() {
            return Err(MacroExpansionError::ArityMismatch {
                macro_name: name,
                expected: macro_def.params.len(),
                got: args.len(),
            });
        }

        // Create substitution map
        let mut substitutions = FxHashMap::default();
        for (param, arg) in macro_def.params.iter().zip(args.iter()) {
            substitutions.insert(*param, Shared::clone(arg));
        }

        // Substitute and expand the macro body
        self.recursion_depth += 1;
        let result = self.substitute_and_expand_program(&macro_def.body, &substitutions);
        self.recursion_depth -= 1;

        // If the body has only one expression, return it directly
        // Otherwise, wrap in a block
        match result?.as_slice() {
            [single] => Ok(Shared::clone(single)),
            multiple => Ok(Shared::new(Node {
                token_id: ArenaId::new(0),
                expr: Shared::new(Expr::Block(multiple.to_vec())),
            })),
        }
    }

    /// Substitutes parameters in a program and expands the result.
    fn substitute_and_expand_program(
        &mut self,
        program: &Program,
        substitutions: &FxHashMap<Ident, Shared<Node>>,
    ) -> Result<Program, MacroExpansionError> {
        program
            .iter()
            .map(|node| {
                let substituted = self.substitute_node(node, substitutions);
                self.expand_node(&substituted)
            })
            .collect()
    }

    /// Recursively substitutes identifiers in a node with their corresponding values.
    fn substitute_node(&self, node: &Shared<Node>, substitutions: &FxHashMap<Ident, Shared<Node>>) -> Shared<Node> {
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
                let substituted_program: Vec<_> =
                    program.iter().map(|n| self.substitute_node(n, substitutions)).collect();

                Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::Def(ident.clone(), params.clone(), substituted_program)),
                })
            }

            Expr::Fn(params, program) => {
                let substituted_program: Vec<_> =
                    program.iter().map(|n| self.substitute_node(n, substitutions)).collect();

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
}

impl Default for MacroExpander {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    // TODO: Add unit tests for macro expansion
}
