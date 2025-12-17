use crate::{
    Ident, Shared,
    ast::{
        Program,
        node::{AccessTarget, Expr, MatchArm, Node, StringSegment},
    },
};
use rustc_hash::{FxBuildHasher, FxHashMap};
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
    pub fn expand_program(&mut self, program: &Program) -> Result<Program, MacroExpansionError> {
        self.collect_macros(program);

        let mut expanded_program = Vec::with_capacity(program.len());
        for node in program {
            // Skip macro definitions - they shouldn't appear in the expanded output
            if matches!(&*node.expr, Expr::Macro(_, _, _)) {
                continue;
            }

            // Check if this is a macro call - if so, expand it directly
            if let Expr::Call(ident, args) = &*node.expr
                && self.macros.contains_key(&ident.name)
            {
                // Expand macro call and add all resulting nodes
                let expanded_nodes = self.expand_macro_call(ident.name, args)?;
                expanded_program.extend(expanded_nodes);
                continue;
            }

            // Regular node expansion
            let expanded_node = self.expand_node(node)?;
            expanded_program.push(expanded_node);
        }

        Ok(expanded_program)
    }

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

    fn expand_node(&mut self, node: &Shared<Node>) -> Result<Shared<Node>, MacroExpansionError> {
        match &*node.expr {
            // Expand function calls, including nested macro calls
            Expr::Call(ident, args) => {
                // Check if this is a macro call
                if self.macros.contains_key(&ident.name) {
                    // For nested macro calls, we need to expand and potentially return multiple nodes
                    // However, expand_node returns a single node, so we wrap them in a Block
                    let expanded_nodes = self.expand_macro_call(ident.name, args)?;
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
                        .map(|arg| self.expand_node(arg))
                        .collect::<Result<Vec<_>, _>>()?;

                    Ok(Shared::new(Node {
                        token_id: node.token_id,
                        expr: Shared::new(Expr::Call(ident.clone(), expanded_args.into())),
                    }))
                }
            }
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

    fn expand_macro_call(
        &mut self,
        name: Ident,
        args: &[Shared<Node>],
    ) -> Result<Vec<Shared<Node>>, MacroExpansionError> {
        if self.recursion_depth >= self.max_recursion {
            return Err(MacroExpansionError::RecursionLimit);
        }

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
        let mut substitutions = FxHashMap::with_capacity_and_hasher(macro_def.params.len(), FxBuildHasher);
        for (param, arg) in macro_def.params.iter().zip(args.iter()) {
            substitutions.insert(*param, Shared::clone(arg));
        }

        // Substitute and expand the macro body
        self.recursion_depth += 1;
        let result = self.substitute_and_expand_program(&macro_def.body, &substitutions);
        self.recursion_depth -= 1;

        // Return all nodes from the macro body directly
        result
    }

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
                // Function parameters shadow macro parameters
                let mut scoped_substitutions = substitutions.clone();
                for param in params {
                    if let Expr::Ident(param_ident) = &*param.expr {
                        scoped_substitutions.remove(&param_ident.name);
                    }
                }

                let substituted_program: Vec<_> = program
                    .iter()
                    .map(|n| self.substitute_node(n, &scoped_substitutions))
                    .collect();

                Shared::new(Node {
                    token_id: node.token_id,
                    expr: Shared::new(Expr::Def(ident.clone(), params.clone(), substituted_program)),
                })
            }
            Expr::Fn(params, program) => {
                let mut scoped_substitutions = substitutions.clone();
                for param in params {
                    if let Expr::Ident(param_ident) = &*param.expr {
                        scoped_substitutions.remove(&param_ident.name);
                    }
                }

                let substituted_program: Vec<_> = program
                    .iter()
                    .map(|n| self.substitute_node(n, &scoped_substitutions))
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

impl Default for Macro {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SharedCell, Token, TokenKind, arena::Arena, parse};
    use rstest::rstest;

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

    #[rstest]
    #[case::basic_macro(
        "macro double(x): x + x; | double(5)",
        "double(5)",
        Ok(())
    )]
    #[case::multiple_params(
        "macro add_three(a, b, c): a + b + c; | add_three(1, 2, 3)",
        "add_three(1, 2, 3)",
        Ok(())
    )]
    #[case::nested_macro_calls(
        "macro double(x): x + x; | macro quad(x): double(double(x)); | quad(3)",
        "quad(3)",
        Ok(())
    )]
    #[case::macro_with_string(
        r#"macro greet(name): s"Hello, ${name}!"; | greet("World")"#,
        r#"greet("World")"#,
        Ok(())
    )]
    #[case::macro_with_let(
        "macro let_double(x): let y = x | y + y; | let_double(7)",
        "let_double(7)",
        Ok(())
    )]
    #[case::macro_with_if(
        "macro max(a, b): if(a > b): a else: b; | max(10, 5)",
        "max(10, 5)",
        Ok(())
    )]
    #[case::macro_with_function_call(
        "macro apply_twice(f, x): f(f(x)); | def inc(n): n + 1; | apply_twice(inc, 5)",
        "apply_twice(inc, 5)",
        Ok(())
    )]
    fn test_macro_expansion_success(#[case] input: &str, #[case] _description: &str, #[case] expected: Result<(), ()>) {
        let program = parse_program(input).expect("Failed to parse program");
        let mut macro_expander = Macro::new();
        let result = macro_expander.expand_program(&program);

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
        let input = "macro add_two(a, b): a + b; | add_two(1)";
        let program = parse_program(input).expect("Failed to parse program");
        let mut macro_expander = Macro::new();
        let result = macro_expander.expand_program(&program);

        assert!(matches!(result, Err(MacroExpansionError::ArityMismatch { .. })));
    }

    #[test]
    fn test_macro_expansion_arity_mismatch_too_many() {
        let input = "macro double(x): x + x; | double(1, 2, 3)";
        let program = parse_program(input).expect("Failed to parse program");
        let mut macro_expander = Macro::new();
        let result = macro_expander.expand_program(&program);

        assert!(matches!(result, Err(MacroExpansionError::ArityMismatch { .. })));
    }

    #[test]
    fn test_macro_recursion_limit() {
        let input = "macro recurse(x): recurse(x); | recurse(1)";
        let program = parse_program(input).expect("Failed to parse program");
        let mut macro_expander = Macro::new();
        macro_expander.max_recursion = 10;
        let result = macro_expander.expand_program(&program);

        assert!(matches!(result, Err(MacroExpansionError::RecursionLimit)));
    }

    #[rstest]
    #[case::no_params("macro const(): 42;", 0)]
    #[case::one_param("macro double(x): x + x;", 1)]
    #[case::two_params("macro add(a, b): a + b;", 2)]
    #[case::three_params("macro add_three(a, b, c): a + b + c;", 3)]
    fn test_macro_definition_collection(#[case] input: &str, #[case] expected_param_count: usize) {
        let program = parse_program(input).expect("Failed to parse program");
        let mut macro_expander = Macro::new();
        macro_expander.collect_macros(&program);

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
            macro double(x): x + x;
            | macro triple(x): x + x + x;
            | double(5) + triple(3)
        "#;
        let program = parse_program(input).expect("Failed to parse program");
        let mut macro_expander = Macro::new();
        let result = macro_expander.expand_program(&program);

        assert!(result.is_ok(), "Expected successful expansion");
        assert_eq!(macro_expander.macros.len(), 2, "Expected two macro definitions");
    }

    #[test]
    fn test_macro_parameter_shadowing() {
        let input = "let x = 100 | macro use_param(x): x * 2; | use_param(5)";
        let program = parse_program(input).expect("Failed to parse program");
        let mut macro_expander = Macro::new();
        let result = macro_expander.expand_program(&program);

        assert!(result.is_ok(), "Expected successful expansion with parameter shadowing");
    }

    #[test]
    fn test_macro_not_included_in_output() {
        let input = "macro double(x): x + x; | 42";
        let program = parse_program(input).expect("Failed to parse program");
        let mut macro_expander = Macro::new();
        let expanded = macro_expander
            .expand_program(&program)
            .expect("Failed to expand program");

        // The expanded program should not contain the macro definition
        for node in &expanded {
            assert!(
                !matches!(&*node.expr, Expr::Macro(_, _, _)),
                "Macro definition should not be in expanded output"
            );
        }
    }
}
