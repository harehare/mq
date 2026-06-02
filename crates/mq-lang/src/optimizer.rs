use rustc_hash::FxHashMap;
use smallvec::SmallVec;

use crate::{
    Ident, IdentWithToken, Shared,
    ast::{
        Program, TokenId,
        node::{self as ast, Args, Branches, Literal, MatchArm, MatchArms, Params, Pattern, StringSegment},
    },
    selector::Selector,
};

/// Stack-allocated map from `Ident` to `Literal` used during let-literal propagation.
///
/// Up to 8 entries live on the stack with no heap allocation; larger programs fall back
/// to the heap automatically (SmallVec's spill behaviour).
type LiteralEnv = SmallVec<[(Ident, Literal); 8]>;

fn env_get(env: &LiteralEnv, key: Ident) -> Option<&Literal> {
    env.iter().rev().find(|(k, _)| *k == key).map(|(_, v)| v)
}

fn env_insert(env: &mut LiteralEnv, key: Ident, val: Literal) {
    if let Some(entry) = env.iter_mut().find(|(k, _)| *k == key) {
        entry.1 = val;
    } else {
        env.push((key, val));
    }
}

fn env_remove(env: &mut LiteralEnv, key: Ident) {
    env.retain(|(k, _)| *k != key);
}

/// Pointer-equality helper that works for both `Rc` and `Arc` (the `sync` feature).
#[inline]
fn ptr_eq<T: ?Sized>(a: &Shared<T>, b: &Shared<T>) -> bool {
    #[cfg(not(feature = "sync"))]
    {
        std::rc::Rc::ptr_eq(a, b)
    }
    #[cfg(feature = "sync")]
    {
        std::sync::Arc::ptr_eq(a, b)
    }
}

/// Controls which optimization passes are applied by the [`Optimizer`].
///
/// - `None`: no transformations; the AST is returned unchanged.
/// - `Basic`: constant folding, dead-branch elimination, and selector-chain merging.
/// - `Full` (default): all passes — `Basic` plus let-literal propagation, function
///   inlining, and tail-call optimization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OptimizationLevel {
    None,
    Basic,
    #[default]
    Full,
}

/// AST optimizer that applies safe, semantics-preserving transformations before evaluation.
#[derive(Default)]
pub struct Optimizer {
    level: OptimizationLevel,
}

impl Optimizer {
    /// Creates an `Optimizer` that runs only the passes enabled by `level`.
    pub fn with_level(level: OptimizationLevel) -> Self {
        Self { level }
    }

    pub fn optimize(&self, program: Program) -> Program {
        match self.level {
            OptimizationLevel::None => program,
            OptimizationLevel::Basic => {
                let optimized: Program = program.into_iter().map(|node| self.optimize_node(node)).collect();
                self.merge_selector_chains(optimized)
            }
            OptimizationLevel::Full => {
                // Pass 1: constant folding + let-literal propagation in a single traversal.
                let program = self.propagate_and_fold(program);
                let program = self.merge_selector_chains(program);

                // Passes 2-4 are only worthwhile when Def nodes are present.
                if !program.iter().any(|n| matches!(&*n.expr, ast::Expr::Def(..))) {
                    return program;
                }

                let inlinable = collect_inlinable(&program);
                let program: Program = if inlinable.is_empty() {
                    program
                } else {
                    program.into_iter().map(|n| self.apply_inline(n, &inlinable)).collect()
                };
                let program = apply_tco_transforms(program);
                let refolded: Program = program.into_iter().map(|n| self.optimize_node(n)).collect();
                self.merge_selector_chains(refolded)
            }
        }
    }

    /// Single-pass constant folding + let-literal propagation.
    ///
    /// Processes top-level nodes left-to-right:
    /// - `let x = <foldable-expr>`: optimises the RHS, registers `x` in the substitution
    ///   map if the result is a literal.
    /// - All other nodes: substitute known literals, then fold constants.
    ///
    /// This replaces the original two-pass sequence (optimize_node → propagate_let_literals
    /// → optimize_node) with a single traversal.
    fn propagate_and_fold(&self, program: Program) -> Program {
        // Fast path: if no top-level let-literal bindings exist, skip the propagation
        // machinery entirely and just fold constants. If nothing folds either, return
        // the original program to avoid any allocation.
        let has_let_literal = program.iter().any(|n| {
            matches!(&*n.expr, ast::Expr::Let(Pattern::Ident(_), rhs) if matches!(&*rhs.expr, ast::Expr::Literal(_)))
        });

        if !has_let_literal {
            let mut changed = false;
            let result: Program = program
                .iter()
                .map(|n| {
                    let opt = self.optimize_node(Shared::clone(n));
                    if !ptr_eq(&opt, n) {
                        changed = true;
                    }
                    opt
                })
                .collect();
            return if changed { result } else { program };
        }

        let mut env: LiteralEnv = LiteralEnv::new();
        let mut result: Program = Vec::with_capacity(program.len());

        for node in program {
            let token_id = node.token_id;
            match &*node.expr {
                ast::Expr::Let(Pattern::Ident(ident), rhs) => {
                    let opt_rhs = self.optimize_node(Shared::clone(rhs));
                    if let ast::Expr::Literal(lit) = &*opt_rhs.expr {
                        env_insert(&mut env, ident.name, lit.clone());
                    } else {
                        env_remove(&mut env, ident.name);
                    }
                    // Reuse the original node when the RHS didn't change (e.g., already a literal).
                    if ptr_eq(&opt_rhs, rhs) {
                        result.push(node);
                    } else {
                        result.push(Shared::new(ast::Node {
                            token_id,
                            expr: Shared::new(ast::Expr::Let(Pattern::Ident(ident.clone()), opt_rhs)),
                        }));
                    }
                }
                _ => {
                    let optimized = if env.is_empty() {
                        self.optimize_node(node)
                    } else {
                        self.optimize_node(self.substitute_literals(node, &env))
                    };
                    result.push(optimized);
                }
            }
        }

        result
    }

    fn merge_selector_chains(&self, program: Program) -> Program {
        // Fast path: skip allocation when no consecutive Selector nodes exist.
        let has_consecutive = program
            .windows(2)
            .any(|w| matches!(&*w[0].expr, ast::Expr::Selector(_)) && matches!(&*w[1].expr, ast::Expr::Selector(_)));
        if !has_consecutive {
            return program;
        }

        let mut result: Program = Vec::with_capacity(program.len());
        let mut iter = program.into_iter().peekable();

        while let Some(node) = iter.next() {
            if let ast::Expr::Selector(sel) = &*node.expr {
                let token_id = node.token_id;
                let mut chain: SmallVec<[Selector; 4]> = SmallVec::new();
                chain.push(sel.clone());

                while let Some(next) = iter.peek() {
                    if let ast::Expr::Selector(next_sel) = &*next.expr {
                        chain.push(next_sel.clone());
                        iter.next();
                    } else {
                        break;
                    }
                }

                if chain.len() == 1 {
                    result.push(node);
                } else {
                    result.push(Shared::new(ast::Node {
                        token_id,
                        expr: Shared::new(ast::Expr::SelectorChain(chain)),
                    }));
                }
            } else {
                result.push(node);
            }
        }

        result
    }

    /// Substitute `Ident` references with their bound literals, without crossing scope boundaries.
    fn substitute_literals(&self, node: Shared<ast::Node>, env: &LiteralEnv) -> Shared<ast::Node> {
        if env.is_empty() {
            return node;
        }
        let token_id = node.token_id;

        match &*node.expr {
            ast::Expr::Ident(ident) => {
                if let Some(lit) = env_get(env, ident.name) {
                    return Shared::new(ast::Node {
                        token_id,
                        expr: Shared::new(ast::Expr::Literal(lit.clone())),
                    });
                }
                node
            }

            ast::Expr::Call(ident, args) => {
                let subst_args: Args = args
                    .iter()
                    .map(|a| self.substitute_literals(Shared::clone(a), env))
                    .collect();
                Shared::new(ast::Node {
                    token_id,
                    expr: Shared::new(ast::Expr::Call(ident.clone(), subst_args)),
                })
            }

            ast::Expr::CallDynamic(callable, args) => {
                let subst_callable = self.substitute_literals(Shared::clone(callable), env);
                let subst_args: Args = args
                    .iter()
                    .map(|a| self.substitute_literals(Shared::clone(a), env))
                    .collect();
                Shared::new(ast::Node {
                    token_id,
                    expr: Shared::new(ast::Expr::CallDynamic(subst_callable, subst_args)),
                })
            }

            ast::Expr::SelectorCall(selector, args) => {
                let subst_args: Args = args
                    .iter()
                    .map(|a| self.substitute_literals(Shared::clone(a), env))
                    .collect();
                Shared::new(ast::Node {
                    token_id,
                    expr: Shared::new(ast::Expr::SelectorCall(selector.clone(), subst_args)),
                })
            }

            ast::Expr::If(branches) => {
                let subst_branches: Branches = branches
                    .iter()
                    .map(|(cond, body)| {
                        (
                            cond.as_ref().map(|c| self.substitute_literals(Shared::clone(c), env)),
                            self.substitute_literals(Shared::clone(body), env),
                        )
                    })
                    .collect();
                Shared::new(ast::Node {
                    token_id,
                    expr: Shared::new(ast::Expr::If(subst_branches)),
                })
            }

            ast::Expr::And(operands) => {
                let subst: Vec<_> = operands
                    .iter()
                    .map(|o| self.substitute_literals(Shared::clone(o), env))
                    .collect();
                Shared::new(ast::Node {
                    token_id,
                    expr: Shared::new(ast::Expr::And(subst)),
                })
            }

            ast::Expr::Or(operands) => {
                let subst: Vec<_> = operands
                    .iter()
                    .map(|o| self.substitute_literals(Shared::clone(o), env))
                    .collect();
                Shared::new(ast::Node {
                    token_id,
                    expr: Shared::new(ast::Expr::Or(subst)),
                })
            }

            ast::Expr::Paren(inner) => Shared::new(ast::Node {
                token_id,
                expr: Shared::new(ast::Expr::Paren(self.substitute_literals(Shared::clone(inner), env))),
            }),

            ast::Expr::Try(try_expr, catch_expr) => Shared::new(ast::Node {
                token_id,
                expr: Shared::new(ast::Expr::Try(
                    self.substitute_literals(Shared::clone(try_expr), env),
                    self.substitute_literals(Shared::clone(catch_expr), env),
                )),
            }),

            ast::Expr::Break(Some(val)) => Shared::new(ast::Node {
                token_id,
                expr: Shared::new(ast::Expr::Break(Some(
                    self.substitute_literals(Shared::clone(val), env),
                ))),
            }),

            // Substitute into Expr segments of interpolated strings so that
            // `let x = "hi" | s"${x}!"` can later be folded to `"hi!"`.
            ast::Expr::InterpolatedString(segments) => {
                let subst_segs: Vec<StringSegment> = segments
                    .iter()
                    .map(|seg| match seg {
                        StringSegment::Expr(inner) => {
                            StringSegment::Expr(self.substitute_literals(Shared::clone(inner), env))
                        }
                        other => other.clone(),
                    })
                    .collect();
                Shared::new(ast::Node {
                    token_id,
                    expr: Shared::new(ast::Expr::InterpolatedString(subst_segs)),
                })
            }

            // Scope-creating or leaf nodes: stop substitution here.
            ast::Expr::Block(_)
            | ast::Expr::Def(_, _, _)
            | ast::Expr::Fn(_, _)
            | ast::Expr::While(_, _)
            | ast::Expr::Loop(_)
            | ast::Expr::Foreach(_, _, _)
            | ast::Expr::Let(_, _)
            | ast::Expr::Var(_, _)
            | ast::Expr::As(_, _)
            | ast::Expr::Assign(_, _)
            | ast::Expr::Match(_, _)
            | ast::Expr::Literal(_)
            | ast::Expr::Selector(_)
            | ast::Expr::SelectorChain(_)
            | ast::Expr::Self_
            | ast::Expr::Nodes
            | ast::Expr::Break(None)
            | ast::Expr::Continue
            | ast::Expr::Include(_)
            | ast::Expr::Import(_)
            | ast::Expr::Module(_, _)
            | ast::Expr::Macro(_, _, _)
            | ast::Expr::Quote(_)
            | ast::Expr::Unquote(_)
            | ast::Expr::QualifiedAccess(_, _) => node,
        }
    }

    fn apply_inline(&self, node: Shared<ast::Node>, fns: &FxHashMap<Ident, InlinableFn>) -> Shared<ast::Node> {
        if fns.is_empty() {
            return node;
        }
        let token_id = node.token_id;

        match &*node.expr {
            ast::Expr::Call(ident, args) => {
                let opt_args: Args = args.iter().map(|a| self.apply_inline(Shared::clone(a), fns)).collect();

                if let Some(f) = fns.get(&ident.name).filter(|f| opt_args.len() == f.params.len()) {
                    return substitute_params(Shared::clone(&f.body), &f.params, &opt_args, token_id);
                }

                Shared::new(ast::Node {
                    token_id,
                    expr: Shared::new(ast::Expr::Call(ident.clone(), opt_args)),
                })
            }

            // Recurse into sub-expressions — but not across scope-creating nodes (Def, Fn, Block).
            ast::Expr::If(branches) => {
                let branches: ast::Branches = branches
                    .iter()
                    .map(|(cond, body)| {
                        (
                            cond.as_ref().map(|c| self.apply_inline(Shared::clone(c), fns)),
                            self.apply_inline(Shared::clone(body), fns),
                        )
                    })
                    .collect();
                Shared::new(ast::Node {
                    token_id,
                    expr: Shared::new(ast::Expr::If(branches)),
                })
            }

            ast::Expr::And(ops) => Shared::new(ast::Node {
                token_id,
                expr: Shared::new(ast::Expr::And(
                    ops.iter().map(|o| self.apply_inline(Shared::clone(o), fns)).collect(),
                )),
            }),

            ast::Expr::Or(ops) => Shared::new(ast::Node {
                token_id,
                expr: Shared::new(ast::Expr::Or(
                    ops.iter().map(|o| self.apply_inline(Shared::clone(o), fns)).collect(),
                )),
            }),

            ast::Expr::Try(try_expr, catch_expr) => Shared::new(ast::Node {
                token_id,
                expr: Shared::new(ast::Expr::Try(
                    self.apply_inline(Shared::clone(try_expr), fns),
                    self.apply_inline(Shared::clone(catch_expr), fns),
                )),
            }),

            ast::Expr::SelectorCall(sel, args) => {
                let opt_args: Args = args.iter().map(|a| self.apply_inline(Shared::clone(a), fns)).collect();
                Shared::new(ast::Node {
                    token_id,
                    expr: Shared::new(ast::Expr::SelectorCall(sel.clone(), opt_args)),
                })
            }

            // Scope-creating and leaf nodes are left unchanged.
            _ => node,
        }
    }

    fn optimize_node(&self, node: Shared<ast::Node>) -> Shared<ast::Node> {
        let token_id = node.token_id;

        match &*node.expr {
            ast::Expr::Paren(inner) => self.optimize_node(Shared::clone(inner)),

            ast::Expr::Call(ident, args) => {
                let opt_args: Args = args.iter().map(|a| self.optimize_node(Shared::clone(a))).collect();
                if let Some(folded) = self.try_fold_call(token_id, &ident.name, &opt_args) {
                    return folded;
                }
                // Return the original node when no argument changed (avoids allocation).
                if args.iter().zip(opt_args.iter()).all(|(orig, opt)| ptr_eq(orig, opt)) {
                    return node;
                }
                Shared::new(ast::Node {
                    token_id,
                    expr: Shared::new(ast::Expr::Call(ident.clone(), opt_args)),
                })
            }

            ast::Expr::If(branches) => self.optimize_if(token_id, branches),
            ast::Expr::And(operands) => self.optimize_and(token_id, operands),
            ast::Expr::Or(operands) => self.optimize_or(token_id, operands),

            ast::Expr::Block(program) => {
                let opt = self.optimize(program.clone());
                if program.iter().zip(opt.iter()).all(|(a, b)| ptr_eq(a, b)) {
                    return node;
                }
                Shared::new(ast::Node {
                    token_id,
                    expr: Shared::new(ast::Expr::Block(opt)),
                })
            }

            ast::Expr::Def(ident, params, program) => Shared::new(ast::Node {
                token_id,
                expr: Shared::new(ast::Expr::Def(
                    ident.clone(),
                    self.optimize_params(params),
                    self.optimize(program.clone()),
                )),
            }),

            ast::Expr::Fn(params, program) => Shared::new(ast::Node {
                token_id,
                expr: Shared::new(ast::Expr::Fn(
                    self.optimize_params(params),
                    self.optimize(program.clone()),
                )),
            }),

            ast::Expr::While(cond, program) => {
                let opt_cond = self.optimize_node(Shared::clone(cond));
                let opt_body = self.optimize(program.clone());
                if ptr_eq(&opt_cond, cond) && program.iter().zip(opt_body.iter()).all(|(a, b)| ptr_eq(a, b)) {
                    return node;
                }
                Shared::new(ast::Node {
                    token_id,
                    expr: Shared::new(ast::Expr::While(opt_cond, opt_body)),
                })
            }

            ast::Expr::Loop(program) => {
                let opt = self.optimize(program.clone());
                if program.iter().zip(opt.iter()).all(|(a, b)| ptr_eq(a, b)) {
                    return node;
                }
                Shared::new(ast::Node {
                    token_id,
                    expr: Shared::new(ast::Expr::Loop(opt)),
                })
            }

            ast::Expr::Foreach(ident, values, program) => {
                let opt_values = self.optimize_node(Shared::clone(values));
                let opt_body = self.optimize(program.clone());
                if ptr_eq(&opt_values, values) && program.iter().zip(opt_body.iter()).all(|(a, b)| ptr_eq(a, b)) {
                    return node;
                }
                Shared::new(ast::Node {
                    token_id,
                    expr: Shared::new(ast::Expr::Foreach(ident.clone(), opt_values, opt_body)),
                })
            }

            ast::Expr::As(ident, inner) => {
                let opt_inner = self.optimize_node(Shared::clone(inner));
                if ptr_eq(&opt_inner, inner) {
                    return node;
                }
                Shared::new(ast::Node {
                    token_id,
                    expr: Shared::new(ast::Expr::As(ident.clone(), opt_inner)),
                })
            }

            ast::Expr::Let(pattern, inner) => {
                let opt_inner = self.optimize_node(Shared::clone(inner));
                if ptr_eq(&opt_inner, inner) {
                    return node;
                }
                Shared::new(ast::Node {
                    token_id,
                    expr: Shared::new(ast::Expr::Let(pattern.clone(), opt_inner)),
                })
            }

            ast::Expr::Var(pattern, inner) => {
                let opt_inner = self.optimize_node(Shared::clone(inner));
                if ptr_eq(&opt_inner, inner) {
                    return node;
                }
                Shared::new(ast::Node {
                    token_id,
                    expr: Shared::new(ast::Expr::Var(pattern.clone(), opt_inner)),
                })
            }

            ast::Expr::Assign(ident, inner) => {
                let opt_inner = self.optimize_node(Shared::clone(inner));
                if ptr_eq(&opt_inner, inner) {
                    return node;
                }
                Shared::new(ast::Node {
                    token_id,
                    expr: Shared::new(ast::Expr::Assign(ident.clone(), opt_inner)),
                })
            }

            ast::Expr::Try(try_expr, catch_expr) => {
                let opt_try = self.optimize_node(Shared::clone(try_expr));
                let opt_catch = self.optimize_node(Shared::clone(catch_expr));
                if ptr_eq(&opt_try, try_expr) && ptr_eq(&opt_catch, catch_expr) {
                    return node;
                }
                Shared::new(ast::Node {
                    token_id,
                    expr: Shared::new(ast::Expr::Try(opt_try, opt_catch)),
                })
            }

            ast::Expr::Break(Some(val)) => {
                let opt_val = self.optimize_node(Shared::clone(val));
                if ptr_eq(&opt_val, val) {
                    return node;
                }
                Shared::new(ast::Node {
                    token_id,
                    expr: Shared::new(ast::Expr::Break(Some(opt_val))),
                })
            }

            ast::Expr::Match(value_node, arms) => {
                let opt_value = self.optimize_node(Shared::clone(value_node));
                let opt_arms: MatchArms = arms
                    .iter()
                    .map(|arm| MatchArm {
                        pattern: arm.pattern.clone(),
                        guard: arm.guard.as_ref().map(|g| self.optimize_node(Shared::clone(g))),
                        body: self.optimize_node(Shared::clone(&arm.body)),
                    })
                    .collect();
                Shared::new(ast::Node {
                    token_id,
                    expr: Shared::new(ast::Expr::Match(opt_value, opt_arms)),
                })
            }

            ast::Expr::CallDynamic(callable, args) => {
                let opt_callable = self.optimize_node(Shared::clone(callable));
                let opt_args: Args = args.iter().map(|a| self.optimize_node(Shared::clone(a))).collect();
                if ptr_eq(&opt_callable, callable)
                    && args.iter().zip(opt_args.iter()).all(|(orig, opt)| ptr_eq(orig, opt))
                {
                    return node;
                }
                Shared::new(ast::Node {
                    token_id,
                    expr: Shared::new(ast::Expr::CallDynamic(opt_callable, opt_args)),
                })
            }

            ast::Expr::SelectorCall(selector, args) => {
                let opt_args: Args = args.iter().map(|a| self.optimize_node(Shared::clone(a))).collect();
                if args.iter().zip(opt_args.iter()).all(|(orig, opt)| ptr_eq(orig, opt)) {
                    return node;
                }
                Shared::new(ast::Node {
                    token_id,
                    expr: Shared::new(ast::Expr::SelectorCall(selector.clone(), opt_args)),
                })
            }

            ast::Expr::InterpolatedString(segments) => {
                // Optimize each Expr segment; also promote string-literal Expr segments to
                // Text so that fully-constant interpolated strings can be folded.
                let opt_segs: Vec<StringSegment> = segments
                    .iter()
                    .map(|seg| match seg {
                        StringSegment::Expr(n) => {
                            let opt = self.optimize_node(Shared::clone(n));
                            if let ast::Expr::Literal(Literal::String(s)) = &*opt.expr {
                                StringSegment::Text(s.clone())
                            } else {
                                StringSegment::Expr(opt)
                            }
                        }
                        other => other.clone(),
                    })
                    .collect();

                if opt_segs.iter().all(|s| matches!(s, StringSegment::Text(_))) {
                    let folded = opt_segs.iter().fold(String::new(), |mut acc, s| {
                        if let StringSegment::Text(t) = s {
                            acc.push_str(t);
                        }
                        acc
                    });
                    return Shared::new(ast::Node {
                        token_id,
                        expr: Shared::new(ast::Expr::Literal(Literal::String(folded))),
                    });
                }

                Shared::new(ast::Node {
                    token_id,
                    expr: Shared::new(ast::Expr::InterpolatedString(opt_segs)),
                })
            }

            ast::Expr::Module(ident, program) => Shared::new(ast::Node {
                token_id,
                expr: Shared::new(ast::Expr::Module(ident.clone(), self.optimize(program.clone()))),
            }),

            ast::Expr::Unquote(inner) => Shared::new(ast::Node {
                token_id,
                expr: Shared::new(ast::Expr::Unquote(self.optimize_node(Shared::clone(inner)))),
            }),

            ast::Expr::Literal(_)
            | ast::Expr::Ident(_)
            | ast::Expr::Selector(_)
            | ast::Expr::SelectorChain(_)
            | ast::Expr::Self_
            | ast::Expr::Nodes
            | ast::Expr::Break(None)
            | ast::Expr::Continue
            | ast::Expr::Include(_)
            | ast::Expr::Import(_)
            | ast::Expr::Macro(_, _, _)
            | ast::Expr::Quote(_)
            | ast::Expr::QualifiedAccess(_, _) => node,
        }
    }

    fn try_fold_call(&self, token_id: TokenId, name: &crate::Ident, args: &Args) -> Option<Shared<ast::Node>> {
        use crate::ast::constants::builtins;

        let make_lit = |lit: Literal| {
            Shared::new(ast::Node {
                token_id,
                expr: Shared::new(ast::Expr::Literal(lit)),
            })
        };

        if args.len() == 2 {
            let lhs = literal_of(&args[0])?;
            let rhs = literal_of(&args[1])?;

            match name.as_str().as_str() {
                n @ (builtins::ADD | builtins::SUB | builtins::MUL | builtins::DIV | builtins::MOD) => {
                    match (lhs, rhs) {
                        (Literal::Number(a), Literal::Number(b)) => {
                            if (n == builtins::DIV || n == builtins::MOD) && b.is_zero() {
                                return None;
                            }
                            let result = match n {
                                builtins::ADD => a + b,
                                builtins::SUB => a - b,
                                builtins::MUL => a * b,
                                builtins::DIV => a / b,
                                builtins::MOD => a % b,
                                _ => unreachable!(),
                            };
                            return Some(make_lit(Literal::Number(result)));
                        }
                        (Literal::String(a), Literal::String(b)) if n == builtins::ADD => {
                            return Some(make_lit(Literal::String(a + &b)));
                        }
                        _ => {}
                    }
                }

                builtins::EQ => return Some(make_lit(Literal::Bool(literal_eq(lhs, rhs)))),
                builtins::NE => return Some(make_lit(Literal::Bool(!literal_eq(lhs, rhs)))),

                n @ (builtins::LT | builtins::LTE | builtins::GT | builtins::GTE) => {
                    if let (Literal::Number(a), Literal::Number(b)) = (lhs, rhs) {
                        if a.is_nan() || b.is_nan() {
                            return None;
                        }
                        let result = match n {
                            builtins::LT => a < b,
                            builtins::LTE => a <= b,
                            builtins::GT => a > b,
                            builtins::GTE => a >= b,
                            _ => unreachable!(),
                        };
                        return Some(make_lit(Literal::Bool(result)));
                    }
                }

                _ => {}
            }
        }

        if args.len() == 1 {
            let arg = literal_of(&args[0])?;
            match name.as_str().as_str() {
                builtins::NOT => {
                    if let Literal::Bool(b) = arg {
                        return Some(make_lit(Literal::Bool(!b)));
                    }
                }
                builtins::NEGATE => {
                    if let Literal::Number(n) = arg {
                        return Some(make_lit(Literal::Number(-n)));
                    }
                }
                _ => {}
            }
        }

        None
    }

    fn optimize_if(&self, token_id: TokenId, branches: &Branches) -> Shared<ast::Node> {
        let mut remaining: Branches = SmallVec::new();

        for (cond_node, body_node) in branches {
            let opt_body = self.optimize_node(Shared::clone(body_node));

            match cond_node {
                None => {
                    remaining.push((None, opt_body));
                    break;
                }
                Some(cond) => {
                    let opt_cond = self.optimize_node(Shared::clone(cond));
                    match &*opt_cond.expr {
                        ast::Expr::Literal(Literal::Bool(true)) => {
                            remaining.push((None, opt_body));
                            break;
                        }
                        ast::Expr::Literal(Literal::Bool(false)) => {
                            continue;
                        }
                        _ => {
                            remaining.push((Some(opt_cond), opt_body));
                        }
                    }
                }
            }
        }

        match remaining.len() {
            0 => Shared::new(ast::Node {
                token_id,
                expr: Shared::new(ast::Expr::Literal(Literal::None)),
            }),
            1 if remaining[0].0.is_none() => Shared::clone(&remaining[0].1),
            _ => Shared::new(ast::Node {
                token_id,
                expr: Shared::new(ast::Expr::If(remaining)),
            }),
        }
    }

    fn optimize_and(&self, token_id: TokenId, operands: &[Shared<ast::Node>]) -> Shared<ast::Node> {
        let mut remaining: Vec<Shared<ast::Node>> = Vec::with_capacity(operands.len());

        for op in operands {
            let opt = self.optimize_node(Shared::clone(op));
            match &*opt.expr {
                ast::Expr::Literal(lit) if !literal_is_truthy(lit) => {
                    return Shared::new(ast::Node {
                        token_id,
                        expr: Shared::new(ast::Expr::Literal(Literal::Bool(false))),
                    });
                }
                ast::Expr::Literal(lit) if literal_is_truthy(lit) => continue,
                _ => remaining.push(opt),
            }
        }

        match remaining.len() {
            0 => Shared::new(ast::Node {
                token_id,
                expr: Shared::new(ast::Expr::Literal(Literal::Bool(true))),
            }),
            _ => Shared::new(ast::Node {
                token_id,
                expr: Shared::new(ast::Expr::And(remaining)),
            }),
        }
    }

    fn optimize_or(&self, token_id: TokenId, operands: &[Shared<ast::Node>]) -> Shared<ast::Node> {
        let mut remaining: Vec<Shared<ast::Node>> = Vec::with_capacity(operands.len());

        for op in operands {
            let opt = self.optimize_node(Shared::clone(op));
            match &*opt.expr {
                ast::Expr::Literal(lit) if literal_is_truthy(lit) => return opt,
                ast::Expr::Literal(lit) if !literal_is_truthy(lit) => continue,
                _ => remaining.push(opt),
            }
        }

        match remaining.len() {
            0 => Shared::new(ast::Node {
                token_id,
                expr: Shared::new(ast::Expr::Literal(Literal::Bool(false))),
            }),
            _ => Shared::new(ast::Node {
                token_id,
                expr: Shared::new(ast::Expr::Or(remaining)),
            }),
        }
    }

    fn optimize_params(&self, params: &Params) -> Params {
        params
            .iter()
            .map(|p| ast::Param {
                ident: p.ident.clone(),
                default: p.default.as_ref().map(|d| self.optimize_node(Shared::clone(d))),
                is_variadic: p.is_variadic,
            })
            .collect()
    }
}

struct InlinableFn {
    params: Vec<Ident>,
    body: Shared<ast::Node>,
}

/// Scan `program` for `Def` nodes whose bodies are safe to inline.
///
/// A function is inlineable when:
/// - Its body has exactly one node.
/// - It has no variadic or default-valued parameters.
/// - It is not self-recursive.
/// - Its body contains no free variables (all `Ident` refs are parameter names).
fn collect_inlinable(program: &Program) -> FxHashMap<Ident, InlinableFn> {
    let mut map = FxHashMap::default();
    for node in program {
        let ast::Expr::Def(ident, params, body) = &*node.expr else {
            continue;
        };
        if body.len() != 1 {
            continue;
        }
        if params.iter().any(|p| p.is_variadic || p.default.is_some()) {
            continue;
        }
        let body_node = &body[0];
        let param_names: Vec<Ident> = params.iter().map(|p| p.ident.name).collect();
        if has_recursion(body_node, ident.name) || has_free_vars(body_node, &param_names) {
            continue;
        }
        map.insert(
            ident.name,
            InlinableFn {
                params: param_names,
                body: Shared::clone(body_node),
            },
        );
    }
    map
}

/// Returns `true` if `node` contains a direct or indirect call to `fn_name`.
fn has_recursion(node: &Shared<ast::Node>, fn_name: Ident) -> bool {
    match &*node.expr {
        ast::Expr::Call(ident, args) => ident.name == fn_name || args.iter().any(|a| has_recursion(a, fn_name)),
        ast::Expr::Ident(ident) => ident.name == fn_name,
        ast::Expr::And(ops) | ast::Expr::Or(ops) => ops.iter().any(|o| has_recursion(o, fn_name)),
        ast::Expr::If(branches) => branches.iter().any(|(cond, body)| {
            cond.as_ref().is_some_and(|c| has_recursion(c, fn_name)) || has_recursion(body, fn_name)
        }),
        ast::Expr::Try(t, c) => has_recursion(t, fn_name) || has_recursion(c, fn_name),
        ast::Expr::SelectorCall(_, args) => args.iter().any(|a| has_recursion(a, fn_name)),
        ast::Expr::Paren(inner) => has_recursion(inner, fn_name),
        _ => false,
    }
}

/// Returns `true` if `node` contains any `Ident` reference that is not in `params`,
/// OR if any `Call` node uses a parameter name as its callee (those cannot be substituted
/// at the AST level because the callee position is an `IdentWithToken`, not an `Expr::Ident`).
///
/// Conservative: complex sub-expressions (blocks, lambdas, loops, let/var) cause
/// the function to return `true` immediately so that inlining is skipped.
fn has_free_vars(node: &Shared<ast::Node>, params: &[Ident]) -> bool {
    match &*node.expr {
        ast::Expr::Ident(ident) => !params.contains(&ident.name),
        ast::Expr::Literal(_) | ast::Expr::Self_ | ast::Expr::Selector(_) | ast::Expr::SelectorChain(_) => false,
        ast::Expr::Call(callee, args) => {
            // A parameter used as a callee cannot be inlined (callee is IdentWithToken, not Expr).
            params.contains(&callee.name) || args.iter().any(|a| has_free_vars(a, params))
        }
        ast::Expr::SelectorCall(_, args) => args.iter().any(|a| has_free_vars(a, params)),
        ast::Expr::And(ops) | ast::Expr::Or(ops) => ops.iter().any(|o| has_free_vars(o, params)),
        ast::Expr::If(branches) => branches
            .iter()
            .any(|(cond, body)| cond.as_ref().is_some_and(|c| has_free_vars(c, params)) || has_free_vars(body, params)),
        ast::Expr::Try(t, c) => has_free_vars(t, params) || has_free_vars(c, params),
        ast::Expr::Paren(inner) => has_free_vars(inner, params),
        _ => true,
    }
}

/// Substitute parameter references in `body` with the supplied arguments.
///
/// `Ident(param_name)` → corresponding `arg` node.
/// All other expression types are cloned unchanged.
fn substitute_params(
    node: Shared<ast::Node>,
    params: &[Ident],
    args: &Args,
    call_token_id: TokenId,
) -> Shared<ast::Node> {
    let token_id = call_token_id;
    match &*node.expr {
        ast::Expr::Ident(ident) => {
            if let Some(pos) = params.iter().position(|p| *p == ident.name) {
                return Shared::clone(&args[pos]);
            }
            node
        }
        ast::Expr::Call(ident, call_args) => {
            let subst: Args = call_args
                .iter()
                .map(|a| substitute_params(Shared::clone(a), params, args, call_token_id))
                .collect();
            Shared::new(ast::Node {
                token_id,
                expr: Shared::new(ast::Expr::Call(ident.clone(), subst)),
            })
        }
        ast::Expr::SelectorCall(sel, call_args) => {
            let subst: Args = call_args
                .iter()
                .map(|a| substitute_params(Shared::clone(a), params, args, call_token_id))
                .collect();
            Shared::new(ast::Node {
                token_id,
                expr: Shared::new(ast::Expr::SelectorCall(sel.clone(), subst)),
            })
        }
        ast::Expr::And(ops) => Shared::new(ast::Node {
            token_id,
            expr: Shared::new(ast::Expr::And(
                ops.iter()
                    .map(|o| substitute_params(Shared::clone(o), params, args, call_token_id))
                    .collect(),
            )),
        }),
        ast::Expr::Or(ops) => Shared::new(ast::Node {
            token_id,
            expr: Shared::new(ast::Expr::Or(
                ops.iter()
                    .map(|o| substitute_params(Shared::clone(o), params, args, call_token_id))
                    .collect(),
            )),
        }),
        ast::Expr::If(branches) => {
            let branches: ast::Branches = branches
                .iter()
                .map(|(cond, body)| {
                    (
                        cond.as_ref()
                            .map(|c| substitute_params(Shared::clone(c), params, args, call_token_id)),
                        substitute_params(Shared::clone(body), params, args, call_token_id),
                    )
                })
                .collect();
            Shared::new(ast::Node {
                token_id,
                expr: Shared::new(ast::Expr::If(branches)),
            })
        }
        ast::Expr::Try(t, c) => Shared::new(ast::Node {
            token_id,
            expr: Shared::new(ast::Expr::Try(
                substitute_params(Shared::clone(t), params, args, call_token_id),
                substitute_params(Shared::clone(c), params, args, call_token_id),
            )),
        }),
        ast::Expr::Paren(inner) => substitute_params(Shared::clone(inner), params, args, call_token_id),
        _ => node,
    }
}

fn literal_of(node: &Shared<ast::Node>) -> Option<Literal> {
    match &*node.expr {
        ast::Expr::Literal(lit) => Some(lit.clone()),
        _ => None,
    }
}

fn literal_eq(a: Literal, b: Literal) -> bool {
    match (a, b) {
        (Literal::Number(x), Literal::Number(y)) => x == y,
        (Literal::Bool(x), Literal::Bool(y)) => x == y,
        (Literal::String(x), Literal::String(y)) => x == y,
        (Literal::Symbol(x), Literal::Symbol(y)) => x == y,
        (Literal::Bytes(x), Literal::Bytes(y)) => x == y,
        (Literal::None, Literal::None) => true,
        _ => false,
    }
}

/// Scan `program` and rewrite self-tail-recursive `def` functions to use a loop.
fn apply_tco_transforms(program: Program) -> Program {
    program
        .into_iter()
        .map(|node| {
            let ast::Expr::Def(ident, params, body) = &*node.expr else {
                return node;
            };
            let param_names: Vec<Ident> = params.iter().map(|p| p.ident.name).collect();
            match try_tco_transform(ident.name, &param_names, body, node.token_id) {
                Some(new_body) => Shared::new(ast::Node {
                    token_id: node.token_id,
                    expr: Shared::new(ast::Expr::Def(ident.clone(), params.clone(), new_body)),
                }),
                None => node,
            }
        })
        .collect()
}

/// Returns `Some(new_body)` if `body` matches the TCO pattern:
/// - Single `If` node whose branches are either non-recursive base cases or direct self-calls.
/// - At least one base case and at least one recursive call.
fn try_tco_transform(fn_name: Ident, param_names: &[Ident], body: &Program, token_id: TokenId) -> Option<Program> {
    if body.len() != 1 {
        return None;
    }
    let ast::Expr::If(branches) = &*body[0].expr else {
        return None;
    };

    let mut has_recursive = false;
    let mut has_base = false;

    for (_, branch_body) in branches {
        if is_direct_self_call(branch_body, fn_name) {
            has_recursive = true;
        } else if !contains_self_call(branch_body, fn_name) {
            has_base = true;
        } else {
            return None;
        }
    }

    if !has_recursive || !has_base {
        return None;
    }

    Some(build_tco_loop(fn_name, param_names, branches, token_id))
}

/// Returns `true` if `node` is exactly `Call(fn_name, args)`.
fn is_direct_self_call(node: &Shared<ast::Node>, fn_name: Ident) -> bool {
    matches!(&*node.expr, ast::Expr::Call(ident, _) if ident.name == fn_name)
}

/// Returns `true` if `node` contains any call to `fn_name` at any depth.
fn contains_self_call(node: &Shared<ast::Node>, fn_name: Ident) -> bool {
    match &*node.expr {
        ast::Expr::Call(ident, args) => ident.name == fn_name || args.iter().any(|a| contains_self_call(a, fn_name)),
        ast::Expr::Ident(ident) => ident.name == fn_name,
        ast::Expr::And(ops) | ast::Expr::Or(ops) => ops.iter().any(|o| contains_self_call(o, fn_name)),
        ast::Expr::If(branches) => branches.iter().any(|(cond, body)| {
            cond.as_ref().is_some_and(|c| contains_self_call(c, fn_name)) || contains_self_call(body, fn_name)
        }),
        ast::Expr::Try(t, c) => contains_self_call(t, fn_name) || contains_self_call(c, fn_name),
        ast::Expr::SelectorCall(_, args) => args.iter().any(|a| contains_self_call(a, fn_name)),
        ast::Expr::Paren(inner) | ast::Expr::Break(Some(inner)) | ast::Expr::Unquote(inner) => {
            contains_self_call(inner, fn_name)
        }
        ast::Expr::Block(prog) => prog.iter().any(|n| contains_self_call(n, fn_name)),
        _ => false,
    }
}

/// Build the loop-based body that replaces a tail-recursive function.
///
/// For `def f(a, b): if (cond): base else: f(new_a, new_b);` generates:
/// ```text
/// var __tco_a = a;
/// var __tco_b = b;
/// loop {
///   let a = __tco_a;
///   let b = __tco_b;
///   if (cond): break base
///   else: { __tco_a = new_a; __tco_b = new_b; continue }
/// }
/// ```
fn build_tco_loop(fn_name: Ident, param_names: &[Ident], branches: &Branches, token_id: TokenId) -> Program {
    let syn = |expr: ast::Expr| -> Shared<ast::Node> {
        Shared::new(ast::Node {
            token_id,
            expr: Shared::new(expr),
        })
    };

    let tco_ident = |p: Ident| IdentWithToken::new(&format!("__tco_{}", p.as_str()));

    // var __tco_p = p;
    let var_decls: Program = param_names
        .iter()
        .map(|p| {
            syn(ast::Expr::Var(
                Pattern::Ident(tco_ident(*p)),
                syn(ast::Expr::Ident(IdentWithToken::new(&p.as_str()))),
            ))
        })
        .collect();

    // let p = __tco_p;  (re-bind at the top of each loop iteration)
    let let_rebinds: Program = param_names
        .iter()
        .map(|p| {
            syn(ast::Expr::Let(
                Pattern::Ident(IdentWithToken::new(&p.as_str())),
                syn(ast::Expr::Ident(tco_ident(*p))),
            ))
        })
        .collect();

    // Transform each If branch
    let new_branches: Branches = branches
        .iter()
        .map(|(cond, body)| {
            let new_body = if is_direct_self_call(body, fn_name) {
                let ast::Expr::Call(_, rec_args) = &*body.expr else {
                    unreachable!()
                };
                // __tco_p = new_p; continue
                let mut block: Program = param_names
                    .iter()
                    .zip(rec_args.iter())
                    .map(|(p, new_val)| syn(ast::Expr::Assign(tco_ident(*p), Shared::clone(new_val))))
                    .collect();
                block.push(syn(ast::Expr::Continue));
                syn(ast::Expr::Block(block))
            } else {
                syn(ast::Expr::Break(Some(Shared::clone(body))))
            };
            (cond.clone(), new_body)
        })
        .collect();

    let mut loop_body = let_rebinds;
    loop_body.push(syn(ast::Expr::If(new_branches)));

    let mut result = var_decls;
    result.push(syn(ast::Expr::Loop(loop_body)));
    result
}

fn literal_is_truthy(lit: &Literal) -> bool {
    match lit {
        Literal::Bool(b) => *b,
        Literal::Number(n) => !n.is_zero(),
        Literal::String(s) => !s.is_empty(),
        Literal::Symbol(_) => true,
        Literal::Bytes(b) => !b.is_empty(),
        Literal::None => false,
    }
}

#[cfg(test)]
mod tests {
    use crate::{DefaultEngine, parse_text_input};
    use rstest::rstest;

    fn eval(query: &str, input: &str) -> Vec<String> {
        let mut engine = DefaultEngine::default();
        engine.load_builtin_module();
        let input = parse_text_input(input).unwrap();
        engine
            .eval(query, input.into_iter())
            .unwrap()
            .values()
            .iter()
            .map(|v| v.to_string())
            .collect()
    }

    #[rstest]
    #[case("1 + 2", "3")]
    #[case("10 - 3", "7")]
    #[case("3 * 4", "12")]
    #[case("10 / 2", "5")]
    #[case("10 % 3", "1")]
    #[case("(2 + 3) * 4", "20")]
    #[case("\"hello\" + \" world\"", "hello world")]
    #[case("negate(5)", "-5")]
    fn test_fold_arithmetic(#[case] query: &str, #[case] expected: &str) {
        assert_eq!(eval(query, "x"), vec![expected]);
    }

    #[rstest]
    #[case("1 == 1", "true")]
    #[case("1 == 2", "false")]
    #[case("1 != 2", "true")]
    #[case("1 != 1", "false")]
    #[case("1 < 2", "true")]
    #[case("2 < 1", "false")]
    #[case("2 <= 2", "true")]
    #[case("3 > 2", "true")]
    #[case("2 > 3", "false")]
    #[case("2 >= 2", "true")]
    #[case("not(false)", "true")]
    #[case("not(true)", "false")]
    fn test_fold_comparison(#[case] query: &str, #[case] expected: &str) {
        assert_eq!(eval(query, "x"), vec![expected]);
    }

    #[rstest]
    #[case("if (true): 1 else: 2", "1")]
    #[case("if (false): 1 else: 2", "2")]
    #[case("if (false): 1", "")]
    #[case("if (1 == 1): \"yes\" else: \"no\"", "yes")]
    #[case("if (1 == 2): \"yes\" else: \"no\"", "no")]
    fn test_dead_branch(#[case] query: &str, #[case] expected: &str) {
        assert_eq!(eval(query, "x"), vec![expected]);
    }

    #[rstest]
    #[case("false && true", "false")]
    #[case("true && false", "false")]
    #[case("true || false", "true")]
    #[case("false || true", "true")]
    fn test_short_circuit(#[case] query: &str, #[case] expected: &str) {
        assert_eq!(eval(query, "x"), vec![expected]);
    }

    #[rstest]
    #[case("add(\"hello\", .)", "world", "helloworld")]
    fn test_non_literal_unchanged(#[case] query: &str, #[case] input: &str, #[case] expected: &str) {
        assert_eq!(eval(query, input), vec![expected]);
    }

    #[test]
    fn test_div_zero_not_folded() {
        let mut engine = DefaultEngine::default();
        engine.load_builtin_module();
        let input = parse_text_input("x").unwrap();
        assert!(engine.eval("1 / 0", input.into_iter()).is_err());
    }

    #[rstest]
    #[case(".h1 | .text", 2usize)]
    fn test_selector_chain_merged(#[case] query: &str, #[case] chain_len: usize) {
        use crate::ast::node::Expr;

        let mut engine = DefaultEngine::default();
        let compiled = engine.compile(query).unwrap();
        let program = compiled.program();
        assert_eq!(program.len(), 1, "consecutive selectors must collapse to one node");
        assert!(
            matches!(&*program[0].expr, Expr::SelectorChain(c) if c.len() == chain_len),
            "SelectorChain length must be {chain_len}"
        );
        assert!(
            engine
                .eval_compiled(&compiled, parse_text_input("# Hello").unwrap().into_iter())
                .is_ok()
        );
    }

    #[test]
    fn test_selector_single_stays_selector() {
        use crate::ast::node::Expr;

        let mut engine = DefaultEngine::default();
        let compiled = engine.compile(".h1").unwrap();
        let program = compiled.program();
        assert_eq!(program.len(), 1);
        assert!(matches!(&*program[0].expr, Expr::Selector(_)));
    }

    #[rstest]
    #[case(".h1 | to_string | .text")]
    #[case(".h1 | len() | .text")]
    fn test_selector_chain_not_merged_across_call(#[case] query: &str) {
        use crate::ast::node::Expr;

        let mut engine = DefaultEngine::default();
        engine.load_builtin_module();
        let compiled = engine.compile(query).unwrap();
        let program = compiled.program();
        assert!(program.len() > 1, "call between selectors must break the chain");
        assert!(!matches!(&*program[0].expr, Expr::SelectorChain(_)));
    }

    #[rstest]
    #[case("s\"hello world\"", "hello world")]
    #[case("s\"static text only\"", "static text only")]
    fn test_interpolated_string_folded_to_literal(#[case] query: &str, #[case] expected: &str) {
        use crate::ast::node::Expr;

        let mut engine = DefaultEngine::default();
        let compiled = engine.compile(query).unwrap();
        assert!(
            matches!(&*compiled.program()[0].expr, Expr::Literal(_)),
            "all-text interpolated string must be folded to Literal"
        );
        assert_eq!(eval(query, "x"), vec![expected]);
    }

    #[rstest]
    #[case("s\"${self} end\"")]
    fn test_interpolated_string_not_folded(#[case] query: &str) {
        use crate::ast::node::Expr;

        let mut engine = DefaultEngine::default();
        let compiled = engine.compile(query).unwrap();
        assert!(matches!(&*compiled.program()[0].expr, Expr::InterpolatedString(_)));
    }

    #[rstest]
    #[case("let x = 10 | add(x, 5)", "15")]
    #[case("let a = 3 | let b = 4 | a + b", "7")]
    #[case("let n = 5 | if (n == 5): \"yes\" else: \"no\"", "yes")]
    #[case("let n = 0 | if (n == 5): \"yes\" else: \"no\"", "no")]
    fn test_let_propagation(#[case] query: &str, #[case] expected: &str) {
        assert_eq!(eval(query, "x"), vec![expected]);
    }

    #[rstest]
    #[case("let x = add(1, 2) | add(x, 1)", "4")]
    fn test_let_non_literal_not_propagated(#[case] query: &str, #[case] expected: &str) {
        assert_eq!(eval(query, "x"), vec![expected]);
    }

    #[rstest]
    // 1-param: arithmetic body — inlined + constant-folded
    #[case("def double(x): x * 2; | double(5)", "10")]
    #[case("def add1(x): x + 1; | add1(9)", "10")]
    #[case("def neg(x): negate(x); | neg(3)", "-3")]
    // 1-param: comparison body
    #[case("def is_zero(x): x == 0; | is_zero(0)", "true")]
    #[case("def is_zero(x): x == 0; | is_zero(1)", "false")]
    // 0-param: constant alias
    #[case("def pi: 3; | pi()", "3")]
    // Chained inlining: double(double(2)) → 2*2*2 = 8? no: double(2)=4, double(4)=8
    #[case("def double(x): x * 2; | double(double(2))", "8")]
    // Inline + let propagation: should all collapse to a single literal
    #[case("def inc(x): x + 1; | let n = 4 | inc(n)", "5")]
    fn test_inline_small_functions(#[case] query: &str, #[case] expected: &str) {
        assert_eq!(eval(query, "x"), vec![expected]);
    }

    #[rstest]
    // Recursive function must NOT be inlined (would cause infinite unrolling)
    #[case("def fact(n): if (n == 0): 1 else: n * fact(n - 1); | fact(5)", "120")]
    // Function with free variable must NOT be inlined
    #[case("let k = 10 | def add_k(x): x + k; | add_k(5)", "15")]
    fn test_non_inlinable_functions_still_evaluate(#[case] query: &str, #[case] expected: &str) {
        assert_eq!(eval(query, "x"), vec![expected]);
    }

    #[test]
    fn test_inline_removes_call_node() {
        use crate::DefaultEngine;
        use crate::ast::node::Expr;

        let mut engine = DefaultEngine::default();
        let compiled = engine.compile("def double(x): x * 2; | double(4)").unwrap();
        let last = compiled.program().last().unwrap();
        assert!(
            matches!(&*last.expr, Expr::Literal(_)),
            "inlined + folded call must collapse to a Literal, got: {:?}",
            last.expr
        );
    }

    #[rstest]
    // Simple countdown: recursive call directly in else branch
    #[case(
        "def countdown(n): if (n == 0): \"done\" else: countdown(n - 1); | countdown(5)",
        "done"
    )]
    // Multi-param: two counters
    #[case("def loop2(a, b): if (a == 0): b else: loop2(a - 1, b + 1); | loop2(3, 0)", "3")]
    // Accumulator pattern
    #[case(
        "def sum_to(acc, n): if (n == 0): acc else: sum_to(acc + n, n - 1); | sum_to(0, 10)",
        "55"
    )]
    fn test_tco_correct_result(#[case] query: &str, #[case] expected: &str) {
        assert_eq!(eval(query, "x"), vec![expected]);
    }

    #[rstest]
    // TCO allows deeply recursive calls without hitting the stack limit
    #[case("def go(n): if (n == 0): \"ok\" else: go(n - 1); | go(300)")]
    fn test_tco_deep_recursion_no_stack_overflow(#[case] query: &str) {
        let result = eval(query, "x");
        assert_eq!(result, vec!["ok"], "deep recursion with TCO must not overflow");
    }

    #[test]
    fn test_tco_def_contains_loop_node() {
        use crate::DefaultEngine;
        use crate::ast::node::Expr;

        let mut engine = DefaultEngine::default();
        let compiled = engine
            .compile("def countdown(n): if (n == 0): \"done\" else: countdown(n - 1);")
            .unwrap();
        let program = compiled.program();
        // The Def body should now contain a Loop node (not the original If).
        let def_node = program.first().unwrap();
        let Expr::Def(_, _, body) = &*def_node.expr else {
            panic!("expected Def");
        };
        assert!(
            body.iter().any(|n| matches!(&*n.expr, Expr::Loop(_))),
            "TCO-transformed Def must contain a Loop node"
        );
    }

    #[rstest]
    // Mutual recursion / complex body: must NOT be transformed (falls back to runtime)
    #[case("def fact(n): if (n == 0): 1 else: n * fact(n - 1); | fact(5)", "120")]
    fn test_tco_not_applied_to_non_tail_recursion(#[case] query: &str, #[case] expected: &str) {
        assert_eq!(eval(query, "x"), vec![expected]);
    }

    #[rstest]
    #[case("1 + 2 + 3 + 4", "10")]
    #[case("\"a\" + \"b\" + \"c\"", "abc")]
    #[case("(1 + 2) * (3 + 4)", "21")]
    #[case("((10 - 2) / 2) + 1", "5")]
    // Folding inside call arguments
    #[case("add(1 + 2, 3 + 4)", "10")]
    #[case("mul(negate(2), 3 + 1)", "-8")]
    // Fold within comparison operands
    #[case("(1 + 1) == (2 * 1)", "true")]
    #[case("(10 - 3) > (2 + 4)", "true")]
    #[case("not(not(true))", "true")]
    #[case("not(1 == 2)", "true")]
    fn test_fold_nested(#[case] query: &str, #[case] expected: &str) {
        assert_eq!(eval(query, "x"), vec![expected]);
    }

    #[rstest]
    // Outer true → inner branch runs; inner false → else
    #[case("if (true): if (false): 1 else: 2 else: 3", "2")]
    // Outer false → else, inner never evaluated
    #[case("if (false): if (true): 99 else: 88 else: 42", "42")]
    // All elif branches false → none
    #[case("if (false): 1 elif (false): 2 elif (false): 3", "")]
    // First elif true wins
    #[case("if (false): 1 elif (true): 2 elif (true): 3 else: 4", "2")]
    // Condition folded, body also contains foldable expression
    #[case("if (2 * 3 == 6): 10 + 5 else: 0", "15")]
    // Nested: outer fold → true, inner fold → false → inner else
    #[case(
        "if (1 + 1 == 2): if (3 - 3 == 1): \"inner_true\" else: \"inner_false\" else: \"outer_false\"",
        "inner_false"
    )]
    fn test_dead_branch_nested(#[case] query: &str, #[case] expected: &str) {
        assert_eq!(eval(query, "x"), vec![expected]);
    }

    #[rstest]
    // Truthy literal dropped from And; dynamic operand preserved
    #[case("true && .", "x", "x")]
    // Falsy literal in And → entire And short-circuits to false
    #[case("false && .", "x", "false")]
    // Falsy literal dropped from Or; dynamic operand preserved
    #[case("false || .", "x", "x")]
    // Truthy literal in Or → short-circuits to that literal
    #[case("true || .", "x", "true")]
    #[case("true && true && true", "x", "true")]
    #[case("true && false && true", "x", "false")]
    #[case("false || false || true", "x", "true")]
    fn test_and_or_mixed(#[case] query: &str, #[case] input: &str, #[case] expected: &str) {
        assert_eq!(eval(query, input), vec![expected]);
    }

    #[test]
    fn test_selector_chain_in_def_body() {
        use crate::DefaultEngine;
        use crate::ast::node::Expr;

        let mut engine = DefaultEngine::default();
        let compiled = engine.compile("def extract: .h1 | .text; | extract()").unwrap();
        let program = compiled.program();
        let def_node = program.first().unwrap();
        let Expr::Def(_, _, body) = &*def_node.expr else {
            panic!("expected Def");
        };
        assert!(body.iter().any(|n| matches!(&*n.expr, Expr::SelectorChain(_))));
    }

    #[rstest]
    // Re-binding same name: second binding wins
    #[case("let x = 1 | let x = 2 | x + 0", "2")]
    // Propagation into and/or operands
    #[case("let n = 0 | n == 0 && true", "true")]
    #[case("let n = 1 | n == 0 || true", "true")]
    // Multiple bindings, all literals
    #[case("let a = 2 | let b = 3 | let c = 4 | a + b + c", "9")]
    // Non-literal rebind clears propagation for that name
    #[case("let x = 5 | let x = add(1, 2) | add(x, 0)", "3")]
    // Same variable used twice in one expression
    #[case("let n = 7 | add(n, n)", "14")]
    fn test_let_propagation_edge_cases(#[case] query: &str, #[case] expected: &str) {
        assert_eq!(eval(query, "x"), vec![expected]);
    }

    #[rstest]
    // Nested inlining: g(f(x)) where both functions are inlineable
    #[case("def add1(x): x + 1; | def mul2(x): x * 2; | mul2(add1(3))", "8")]
    // Inline reveals constant comparison → dead branch elimination
    #[case(
        "def always_false(x): x == 999; | if (always_false(0)): \"bad\" else: \"good\"",
        "good"
    )]
    // Inline + fold: argument is itself a foldable expression
    #[case("def double(x): x * 2; | double(3 + 4)", "14")]
    // Multiple distinct calls to same function
    #[case("def inc(x): x + 1; | add(inc(3), inc(4))", "9")]
    // Inline inside if condition
    #[case("def is_pos(x): x > 0; | if (is_pos(5)): \"pos\" else: \"non-pos\"", "pos")]
    // Inline inside and/or
    #[case("def is_one(x): x == 1; | is_one(1) && is_one(1)", "true")]
    #[case("def is_one(x): x == 1; | is_one(0) || is_one(1)", "true")]
    fn test_inline_nested_and_interactions(#[case] query: &str, #[case] expected: &str) {
        assert_eq!(eval(query, "x"), vec![expected]);
    }

    #[rstest]
    #[case("let x = 3 | let y = 4 | x * y", "12")]
    #[case("if (1 + 2 == 3): \"fold_then_dead\" else: \"no\"", "fold_then_dead")]
    #[case("def inc(x): x + 1; | let n = 9 | inc(n)", "10")]
    #[case("def always_true: 1 == 1; | if (always_true()): \"yes\" else: \"no\"", "yes")]
    #[case(
        "def gt_five(x): x > 5; | let n = 6 | if (gt_five(n)): \"big\" else: \"small\"",
        "big"
    )]
    #[case("def compute(x): (x + 1) * (x - 1); | compute(4)", "15")]
    fn test_cross_optimization_chains(#[case] query: &str, #[case] expected: &str) {
        assert_eq!(eval(query, "x"), vec![expected]);
    }

    #[rstest]
    // elif chain: multiple base cases, one recursive branch
    #[case(
        "def classify(n): if (n < 0): \"neg\" elif (n == 0): \"zero\" else: classify(n - 1); | classify(3)",
        "zero"
    )]
    // Boolean accumulator with elif
    #[case(
        "def any_positive(found, n): if (found): true elif (n == 0): false else: any_positive(n > 0, n - 1); | any_positive(false, 5)",
        "true"
    )]
    #[case("def repeat(n): if (n == 0): \"done\" else: repeat(n - 1); | repeat(10)", "done")]
    fn test_tco_extended(#[case] query: &str, #[case] expected: &str) {
        assert_eq!(eval(query, "x"), vec![expected]);
    }

    #[rstest]
    // Pipeline value concatenated with literal (text input)
    #[case("add(., \" world\")", "hello", "hello world")]
    // Dynamic comparison: left side is a variable resolved at runtime
    #[case("let n = add(1, 1) | n > 1", "x", "true")]
    #[case("let n = add(1, 1) | n > 3", "x", "false")]
    #[case("let x = add(2, 3) | x * 2", "x", "10")]
    // 2-param function is not inlineable but still works
    #[case("def add2(a, b): a + b; | add2(3, 4)", "x", "7")]
    fn test_non_optimizable_correctness(#[case] query: &str, #[case] input: &str, #[case] expected: &str) {
        assert_eq!(eval(query, input), vec![expected]);
    }
}
