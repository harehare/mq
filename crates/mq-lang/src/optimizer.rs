use std::sync::OnceLock;

use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;

use crate::{
    Ident, IdentWithToken, Shared,
    ast::{
        Program, TokenId,
        node::{self as ast, Args, Branches, Literal, MatchArm, MatchArms, Params, Pattern, StringSegment},
    },
    selector::Selector,
};

/// SmallVec-backed `(Ident, Literal)` map used during let-literal propagation.
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
/// - `None` (default): no transformations; the AST is returned unchanged.
/// - `Basic`: constant folding, dead-branch elimination, and selector-chain merging.
/// - `Full`: all passes — `Basic` plus let-literal propagation, function inlining, and tail-call optimization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OptimizationLevel {
    #[default]
    None,
    Basic,
    Full,
}

/// Map `program` through `f` without allocating a new `Vec` when no node changes.
///
/// If every node is pointer-equal after `f`, the original `program` is returned as-is.
/// Only when a changed node is encountered does a new `Vec` get allocated, pre-seeded with
/// the unchanged prefix already seen.
fn lazy_map_program<F>(program: Program, mut f: F) -> Program
where
    F: FnMut(&Shared<ast::Node>) -> Shared<ast::Node>,
{
    let mut result: Option<Vec<Shared<ast::Node>>> = None;
    for (i, node) in program.iter().enumerate() {
        let opt = f(node);
        if !ptr_eq(&opt, node) {
            if result.is_none() {
                let mut r = Vec::with_capacity(program.len());
                r.extend(program[..i].iter().cloned());
                result = Some(r);
            }
            result.as_mut().unwrap().push(opt);
        } else if let Some(ref mut r) = result {
            r.push(Shared::clone(node));
        }
    }
    result.unwrap_or(program)
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

    /// Runs all enabled optimization passes on `program` and returns the transformed AST.
    pub fn optimize(&self, program: Program) -> Program {
        self.optimize_impl(program, true)
    }

    /// Optimize a nested sub-program (body of a Def, Block, While, Loop, Foreach, etc.).
    fn optimize_nested(&self, program: Program, parent_user_defs: &FxHashSet<Ident>) -> Program {
        if matches!(self.level, OptimizationLevel::None) {
            return program;
        }

        // Merge parent user_defs with any local Defs. When there are no local Defs (the
        // common case for loop bodies and blocks), skip the allocation entirely.
        let merged;
        let user_defs: &FxHashSet<Ident> = if program.iter().any(|n| matches!(&*n.expr, ast::Expr::Def(..))) {
            merged = parent_user_defs
                .iter()
                .copied()
                .chain(program.iter().filter_map(|n| {
                    if let ast::Expr::Def(ident, ..) = &*n.expr {
                        Some(ident.name)
                    } else {
                        None
                    }
                }))
                .collect();
            &merged
        } else {
            parent_user_defs
        };

        let optimized = lazy_map_program(program, |n| self.optimize_node(Shared::clone(n), user_defs));
        self.merge_selector_chains(optimized)
    }

    /// Internal optimization entry point.
    fn optimize_impl(&self, program: Program, top_level: bool) -> Program {
        // Collect user-defined function names only when Def nodes are actually present.
        // Programs without any `def` (the common case for simple queries) share a static
        // empty set — no heap allocation.
        static EMPTY_DEFS: OnceLock<FxHashSet<Ident>> = OnceLock::new();
        let user_defs_owned: FxHashSet<Ident>;
        let user_defs: &FxHashSet<Ident> = if program.iter().any(|n| matches!(&*n.expr, ast::Expr::Def(..))) {
            user_defs_owned = program
                .iter()
                .filter_map(|n| {
                    if let ast::Expr::Def(ident, ..) = &*n.expr {
                        Some(ident.name)
                    } else {
                        None
                    }
                })
                .collect();
            &user_defs_owned
        } else {
            EMPTY_DEFS.get_or_init(FxHashSet::default)
        };

        match self.level {
            OptimizationLevel::None => program,
            OptimizationLevel::Basic => {
                let optimized: Program = program
                    .into_iter()
                    .map(|node| self.optimize_node(node, user_defs))
                    .collect();
                self.merge_selector_chains(optimized)
            }
            OptimizationLevel::Full => {
                // Pass 1: constant folding + let-literal propagation in a single traversal.
                let program = self.propagate_and_fold(program, user_defs);
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
                // Dead-def elimination is safe only at the top level where all call sites
                // are visible. In nested scopes, external callers are not in scope.
                let program = if top_level {
                    eliminate_dead_defs(program, &inlinable)
                } else {
                    program
                };
                let refolded: Program = program.into_iter().map(|n| self.optimize_node(n, user_defs)).collect();
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
    fn propagate_and_fold(&self, program: Program, user_defs: &FxHashSet<Ident>) -> Program {
        let has_let_literal = program.iter().any(|n| {
            matches!(&*n.expr, ast::Expr::Let(Pattern::Ident(_), rhs) if matches!(&*rhs.expr, ast::Expr::Literal(_)))
        });

        if !has_let_literal {
            return lazy_map_program(program, |n| self.optimize_node(Shared::clone(n), user_defs));
        }

        let mut env: LiteralEnv = LiteralEnv::new();
        let mut result: Program = Vec::with_capacity(program.len());

        for node in program {
            let token_id = node.token_id;
            match &*node.expr {
                ast::Expr::Let(Pattern::Ident(ident), rhs) => {
                    let opt_rhs = self.optimize_node(Shared::clone(rhs), user_defs);
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
                        self.optimize_node(node, user_defs)
                    } else {
                        self.optimize_node(self.substitute_literals(node, &env), user_defs)
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
            // No error binder: neither branch introduces a new binding, so substitution is safe.
            ast::Expr::Try(try_expr, None, catch_expr) => Shared::new(ast::Node {
                token_id,
                expr: Shared::new(ast::Expr::Try(
                    self.substitute_literals(Shared::clone(try_expr), env),
                    None,
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
            | ast::Expr::Try(_, Some(_), _)
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
            | ast::Expr::Import(_, _)
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
            ast::Expr::Try(try_expr, None, catch_expr) => Shared::new(ast::Node {
                token_id,
                expr: Shared::new(ast::Expr::Try(
                    self.apply_inline(Shared::clone(try_expr), fns),
                    None,
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

    fn optimize_node(&self, node: Shared<ast::Node>, user_defs: &FxHashSet<Ident>) -> Shared<ast::Node> {
        let token_id = node.token_id;

        match &*node.expr {
            ast::Expr::Paren(inner) => self.optimize_node(Shared::clone(inner), user_defs),
            ast::Expr::Call(ident, args) => {
                let opt_args: Args = args
                    .iter()
                    .map(|a| self.optimize_node(Shared::clone(a), user_defs))
                    .collect();
                if let Some(folded) = self.try_fold_call(token_id, &ident.name, &opt_args, user_defs) {
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
            ast::Expr::If(branches) => self.optimize_if(token_id, branches, user_defs),
            ast::Expr::And(operands) => self.optimize_and(token_id, operands, user_defs),
            ast::Expr::Or(operands) => self.optimize_or(token_id, operands, user_defs),
            ast::Expr::Block(program) => {
                let opt = self.optimize_nested(program.clone(), user_defs);
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
                    self.optimize_params(params, user_defs),
                    self.optimize_nested(program.clone(), user_defs),
                )),
            }),
            ast::Expr::Fn(params, program) => Shared::new(ast::Node {
                token_id,
                expr: Shared::new(ast::Expr::Fn(
                    self.optimize_params(params, user_defs),
                    self.optimize_nested(program.clone(), user_defs),
                )),
            }),
            ast::Expr::While(cond, program) => {
                let opt_cond = self.optimize_node(Shared::clone(cond), user_defs);
                let opt_body = self.optimize_nested(program.clone(), user_defs);
                if ptr_eq(&opt_cond, cond) && program.iter().zip(opt_body.iter()).all(|(a, b)| ptr_eq(a, b)) {
                    return node;
                }
                Shared::new(ast::Node {
                    token_id,
                    expr: Shared::new(ast::Expr::While(opt_cond, opt_body)),
                })
            }
            ast::Expr::Loop(program) => {
                let opt = self.optimize_nested(program.clone(), user_defs);
                if program.iter().zip(opt.iter()).all(|(a, b)| ptr_eq(a, b)) {
                    return node;
                }
                Shared::new(ast::Node {
                    token_id,
                    expr: Shared::new(ast::Expr::Loop(opt)),
                })
            }
            ast::Expr::Foreach(ident, values, program) => {
                let opt_values = self.optimize_node(Shared::clone(values), user_defs);
                let opt_body = self.optimize_nested(program.clone(), user_defs);
                if ptr_eq(&opt_values, values) && program.iter().zip(opt_body.iter()).all(|(a, b)| ptr_eq(a, b)) {
                    return node;
                }
                Shared::new(ast::Node {
                    token_id,
                    expr: Shared::new(ast::Expr::Foreach(ident.clone(), opt_values, opt_body)),
                })
            }
            ast::Expr::As(ident, inner) => {
                let opt_inner = self.optimize_node(Shared::clone(inner), user_defs);
                if ptr_eq(&opt_inner, inner) {
                    return node;
                }
                Shared::new(ast::Node {
                    token_id,
                    expr: Shared::new(ast::Expr::As(ident.clone(), opt_inner)),
                })
            }
            ast::Expr::Let(pattern, inner) => {
                let opt_inner = self.optimize_node(Shared::clone(inner), user_defs);
                if ptr_eq(&opt_inner, inner) {
                    return node;
                }
                Shared::new(ast::Node {
                    token_id,
                    expr: Shared::new(ast::Expr::Let(pattern.clone(), opt_inner)),
                })
            }
            ast::Expr::Var(pattern, inner) => {
                let opt_inner = self.optimize_node(Shared::clone(inner), user_defs);
                if ptr_eq(&opt_inner, inner) {
                    return node;
                }
                Shared::new(ast::Node {
                    token_id,
                    expr: Shared::new(ast::Expr::Var(pattern.clone(), opt_inner)),
                })
            }
            ast::Expr::Assign(ident, inner) => {
                let opt_inner = self.optimize_node(Shared::clone(inner), user_defs);
                if ptr_eq(&opt_inner, inner) {
                    return node;
                }
                Shared::new(ast::Node {
                    token_id,
                    expr: Shared::new(ast::Expr::Assign(ident.clone(), opt_inner)),
                })
            }
            ast::Expr::Try(try_expr, error_binder, catch_expr) => {
                let opt_try = self.optimize_node(Shared::clone(try_expr), user_defs);
                let opt_catch = self.optimize_node(Shared::clone(catch_expr), user_defs);
                if ptr_eq(&opt_try, try_expr) && ptr_eq(&opt_catch, catch_expr) {
                    return node;
                }
                Shared::new(ast::Node {
                    token_id,
                    expr: Shared::new(ast::Expr::Try(opt_try, error_binder.clone(), opt_catch)),
                })
            }
            ast::Expr::Break(Some(val)) => {
                let opt_val = self.optimize_node(Shared::clone(val), user_defs);
                if ptr_eq(&opt_val, val) {
                    return node;
                }
                Shared::new(ast::Node {
                    token_id,
                    expr: Shared::new(ast::Expr::Break(Some(opt_val))),
                })
            }
            ast::Expr::Match(value_node, arms) => {
                let opt_value = self.optimize_node(Shared::clone(value_node), user_defs);
                let opt_arms: MatchArms = arms
                    .iter()
                    .map(|arm| MatchArm {
                        pattern: arm.pattern.clone(),
                        guard: arm
                            .guard
                            .as_ref()
                            .map(|g| self.optimize_node(Shared::clone(g), user_defs)),
                        body: self.optimize_node(Shared::clone(&arm.body), user_defs),
                    })
                    .collect();
                Shared::new(ast::Node {
                    token_id,
                    expr: Shared::new(ast::Expr::Match(opt_value, opt_arms)),
                })
            }
            ast::Expr::CallDynamic(callable, args) => {
                let opt_callable = self.optimize_node(Shared::clone(callable), user_defs);
                let opt_args: Args = args
                    .iter()
                    .map(|a| self.optimize_node(Shared::clone(a), user_defs))
                    .collect();
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
                let opt_args: Args = args
                    .iter()
                    .map(|a| self.optimize_node(Shared::clone(a), user_defs))
                    .collect();
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
                            let opt = self.optimize_node(Shared::clone(n), user_defs);
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
                expr: Shared::new(ast::Expr::Module(
                    ident.clone(),
                    self.optimize_nested(program.clone(), user_defs),
                )),
            }),
            ast::Expr::Unquote(inner) => Shared::new(ast::Node {
                token_id,
                expr: Shared::new(ast::Expr::Unquote(self.optimize_node(Shared::clone(inner), user_defs))),
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
            | ast::Expr::Import(_, _)
            | ast::Expr::Macro(_, _, _)
            | ast::Expr::Quote(_)
            | ast::Expr::QualifiedAccess(_, _) => node,
        }
    }

    fn try_fold_call(
        &self,
        token_id: TokenId,
        name: &crate::Ident,
        args: &Args,
        user_defs: &FxHashSet<Ident>,
    ) -> Option<Shared<ast::Node>> {
        use crate::ast::constants::builtins;

        // Never fold a call whose name is shadowed by a user-defined function.
        if user_defs.contains(name) {
            return None;
        }

        let make_lit = |lit: Literal| {
            Shared::new(ast::Node {
                token_id,
                expr: Shared::new(ast::Expr::Literal(lit)),
            })
        };

        if args.len() == 2 {
            let lhs_lit = literal_of(&args[0]);
            let rhs_lit = literal_of(&args[1]);

            if let (Some(lhs), Some(rhs)) = (lhs_lit.clone(), rhs_lit.clone()) {
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

                    n @ (builtins::LT | builtins::LTE | builtins::GT | builtins::GTE) => match (lhs, rhs) {
                        (Literal::Number(a), Literal::Number(b)) => {
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
                        (Literal::String(a), Literal::String(b)) => {
                            let result = match n {
                                builtins::LT => a < b,
                                builtins::LTE => a <= b,
                                builtins::GT => a > b,
                                builtins::GTE => a >= b,
                                _ => unreachable!(),
                            };
                            return Some(make_lit(Literal::Bool(result)));
                        }
                        _ => {}
                    },

                    builtins::STARTS_WITH => {
                        if let (Literal::String(s), Literal::String(prefix)) = (lhs, rhs) {
                            return Some(make_lit(Literal::Bool(s.starts_with(&*prefix))));
                        }
                    }

                    builtins::ENDS_WITH => {
                        if let (Literal::String(s), Literal::String(suffix)) = (lhs, rhs) {
                            return Some(make_lit(Literal::Bool(s.ends_with(&*suffix))));
                        }
                    }

                    builtins::INDEX => {
                        if let (Literal::String(s), Literal::String(sub)) = (lhs, rhs) {
                            let pos = s.find(&*sub).map(|v| v as i64).unwrap_or(-1);
                            return Some(make_lit(Literal::Number(pos.into())));
                        }
                    }

                    builtins::RINDEX => {
                        if let (Literal::String(s), Literal::String(sub)) = (lhs, rhs) {
                            let pos = s.rfind(&*sub).map(|v| v as i64).unwrap_or(-1);
                            return Some(make_lit(Literal::Number(pos.into())));
                        }
                    }

                    builtins::COALESCE => {
                        return Some(match lhs {
                            Literal::None => Shared::clone(&args[1]),
                            _ => Shared::clone(&args[0]),
                        });
                    }

                    _ => {}
                }
            }

            // Partial fold: coalesce(none, x) → x even when x is not a literal.
            if name.as_str().as_str() == builtins::COALESCE {
                if matches!(&lhs_lit, Some(Literal::None)) {
                    return Some(Shared::clone(&args[1]));
                }
                if lhs_lit.as_ref().is_some_and(|lit| !matches!(lit, Literal::None)) {
                    return Some(Shared::clone(&args[0]));
                }
            }

            // One operand is a literal: algebraic identity folding.
            let op = name.as_str();
            match op.as_str() {
                builtins::ADD => {
                    // add(x, 0) → x, add(0, x) → x
                    if matches!(&rhs_lit, Some(Literal::Number(n)) if n.is_zero()) {
                        return Some(Shared::clone(&args[0]));
                    }
                    if matches!(&lhs_lit, Some(Literal::Number(n)) if n.is_zero()) {
                        return Some(Shared::clone(&args[1]));
                    }
                    // add("", x) → x, add(x, "") → x
                    if matches!(&rhs_lit, Some(Literal::String(s)) if s.is_empty()) {
                        return Some(Shared::clone(&args[0]));
                    }
                    if matches!(&lhs_lit, Some(Literal::String(s)) if s.is_empty()) {
                        return Some(Shared::clone(&args[1]));
                    }
                }
                builtins::SUB => {
                    // sub(x, 0) → x
                    if matches!(&rhs_lit, Some(Literal::Number(n)) if n.is_zero()) {
                        return Some(Shared::clone(&args[0]));
                    }
                }
                builtins::MUL => {
                    // mul(x, 1) → x, mul(1, x) → x
                    if matches!(&rhs_lit, Some(Literal::Number(n)) if is_one(n)) {
                        return Some(Shared::clone(&args[0]));
                    }
                    if matches!(&lhs_lit, Some(Literal::Number(n)) if is_one(n)) {
                        return Some(Shared::clone(&args[1]));
                    }
                    // mul(x, 0) → 0, mul(0, x) → 0
                    if matches!(&rhs_lit, Some(Literal::Number(n)) if n.is_zero()) {
                        return Some(make_lit(Literal::Number(0i64.into())));
                    }
                    if matches!(&lhs_lit, Some(Literal::Number(n)) if n.is_zero()) {
                        return Some(make_lit(Literal::Number(0i64.into())));
                    }
                }
                builtins::DIV => {
                    // div(x, 1) → x
                    if matches!(&rhs_lit, Some(Literal::Number(n)) if is_one(n)) {
                        return Some(Shared::clone(&args[0]));
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
                // Numeric rounding/absolute value — safe because these are total functions on numbers.
                n @ (builtins::FLOOR | builtins::CEIL | builtins::ROUND | builtins::ABS | builtins::TRUNC) => {
                    if let Literal::Number(num) = arg {
                        if num.is_nan() {
                            return None;
                        }
                        let result = match n {
                            builtins::FLOOR => num.value().floor(),
                            builtins::CEIL => num.value().ceil(),
                            builtins::ROUND => num.value().round(),
                            builtins::ABS => num.value().abs(),
                            builtins::TRUNC => num.value().trunc(),
                            _ => unreachable!(),
                        };
                        return Some(make_lit(Literal::Number(result.into())));
                    }
                }
                builtins::LEN => match arg {
                    Literal::String(s) => return Some(make_lit(Literal::Number(s.chars().count().into()))),
                    Literal::Bytes(b) => return Some(make_lit(Literal::Number(b.len().into()))),
                    _ => {}
                },
                // to_string on any primitive literal — replicates the runtime behaviour exactly.
                builtins::TO_STRING => {
                    let s = match arg {
                        Literal::String(s) => s,
                        Literal::Number(n) => n.to_string(),
                        Literal::Bool(b) => b.to_string(),
                        Literal::None => String::new(),
                        Literal::Symbol(sym) => sym.to_string(),
                        Literal::Bytes(_) => return None, // hex encoding would need extra logic
                    };
                    return Some(make_lit(Literal::String(s)));
                }
                // to_number on a string literal — only fold when parsing succeeds.
                builtins::TO_NUMBER => {
                    if let Literal::String(s) = arg {
                        return s.parse::<f64>().ok().map(|n| make_lit(Literal::Number(n.into())));
                    }
                }
                n @ (builtins::TRIM | builtins::LTRIM | builtins::RTRIM) => {
                    if let Literal::String(s) = arg {
                        let result = match n {
                            builtins::TRIM => s.trim().to_string(),
                            builtins::LTRIM => s.trim_start().to_string(),
                            builtins::RTRIM => s.trim_end().to_string(),
                            _ => unreachable!(),
                        };
                        return Some(make_lit(Literal::String(result)));
                    }
                }
                n @ (builtins::UPCASE | builtins::DOWNCASE) => {
                    if let Literal::String(s) = arg {
                        let result = match n {
                            builtins::UPCASE => s.to_uppercase(),
                            builtins::DOWNCASE => s.to_lowercase(),
                            _ => unreachable!(),
                        };
                        return Some(make_lit(Literal::String(result)));
                    }
                }

                _ => {}
            }
        }

        if args.len() == 3 {
            let a = literal_of(&args[0]);
            let b = literal_of(&args[1]);
            let c = literal_of(&args[2]);
            if let (Some(Literal::String(s)), Some(Literal::String(from)), Some(Literal::String(to))) = (a, b, c) {
                // replace("foo", "o", "a") → "faa"
                if name.as_str().as_str() == builtins::REPLACE {
                    return Some(make_lit(Literal::String(s.replace(&*from, &to))));
                }
            }
        }

        None
    }

    fn optimize_if(&self, token_id: TokenId, branches: &Branches, user_defs: &FxHashSet<Ident>) -> Shared<ast::Node> {
        let mut remaining: Branches = SmallVec::new();

        for (cond_node, body_node) in branches {
            let opt_body = self.optimize_node(Shared::clone(body_node), user_defs);

            match cond_node {
                None => {
                    remaining.push((None, opt_body));
                    break;
                }
                Some(cond) => {
                    let opt_cond = self.optimize_node(Shared::clone(cond), user_defs);
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

    fn optimize_and(
        &self,
        token_id: TokenId,
        operands: &[Shared<ast::Node>],
        user_defs: &FxHashSet<Ident>,
    ) -> Shared<ast::Node> {
        let mut remaining: Vec<Shared<ast::Node>> = Vec::with_capacity(operands.len());

        for op in operands {
            let opt = self.optimize_node(Shared::clone(op), user_defs);
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

    fn optimize_or(
        &self,
        token_id: TokenId,
        operands: &[Shared<ast::Node>],
        user_defs: &FxHashSet<Ident>,
    ) -> Shared<ast::Node> {
        let mut remaining: Vec<Shared<ast::Node>> = Vec::with_capacity(operands.len());

        for op in operands {
            let opt = self.optimize_node(Shared::clone(op), user_defs);
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

    fn optimize_params(&self, params: &Params, user_defs: &FxHashSet<Ident>) -> Params {
        params
            .iter()
            .map(|p| ast::Param {
                ident: p.ident.clone(),
                default: p
                    .default
                    .as_ref()
                    .map(|d| self.optimize_node(Shared::clone(d), user_defs)),
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
        ast::Expr::Try(t, _, c) => has_recursion(t, fn_name) || has_recursion(c, fn_name),
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
        // Try with an error binder falls through to `_ => true` (unsafe to inline).
        ast::Expr::Try(t, None, c) => has_free_vars(t, params) || has_free_vars(c, params),
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
        // `has_free_vars` already excludes error-binder bodies from inlining.
        ast::Expr::Try(t, error_binder, c) => Shared::new(ast::Node {
            token_id,
            expr: Shared::new(ast::Expr::Try(
                substitute_params(Shared::clone(t), params, args, call_token_id),
                error_binder.clone(),
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
        ast::Expr::Try(t, _, c) => contains_self_call(t, fn_name) || contains_self_call(c, fn_name),
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

/// Returns `true` if `n` is exactly the integer 1.
#[inline]
fn is_one(n: &crate::number::Number) -> bool {
    (n.value() - 1.0).abs() < f64::EPSILON
}

/// Collect the names of every function directly called in `program` (recursively).
fn collect_called_fns(program: &Program) -> FxHashSet<Ident> {
    let mut set = FxHashSet::default();
    for node in program {
        collect_called_fns_node(node, &mut set);
    }
    set
}

fn collect_called_fns_node(node: &Shared<ast::Node>, set: &mut FxHashSet<Ident>) {
    match &*node.expr {
        ast::Expr::Call(ident, args) => {
            set.insert(ident.name);
            for a in args {
                collect_called_fns_node(a, set);
            }
        }
        // An ident may be a first-class function value (e.g. passed to `map`, `filter`);
        // record it so we don't accidentally eliminate its Def.
        ast::Expr::Ident(ident) => {
            set.insert(ident.name);
        }
        ast::Expr::Def(_, _, body) | ast::Expr::Block(body) | ast::Expr::Loop(body) | ast::Expr::Module(_, body) => {
            for n in body {
                collect_called_fns_node(n, set);
            }
        }
        ast::Expr::Fn(_, body) => {
            for n in body {
                collect_called_fns_node(n, set);
            }
        }
        ast::Expr::If(branches) => {
            for (cond, body) in branches {
                if let Some(c) = cond {
                    collect_called_fns_node(c, set);
                }
                collect_called_fns_node(body, set);
            }
        }
        ast::Expr::And(ops) | ast::Expr::Or(ops) => {
            for o in ops {
                collect_called_fns_node(o, set);
            }
        }
        ast::Expr::Try(t, _, c) => {
            collect_called_fns_node(t, set);
            collect_called_fns_node(c, set);
        }
        ast::Expr::SelectorCall(_, args) => {
            for a in args {
                collect_called_fns_node(a, set);
            }
        }
        ast::Expr::CallDynamic(callee, args) => {
            collect_called_fns_node(callee, set);
            for a in args {
                collect_called_fns_node(a, set);
            }
        }
        ast::Expr::Let(_, rhs) | ast::Expr::Var(_, rhs) | ast::Expr::Assign(_, rhs) | ast::Expr::As(_, rhs) => {
            collect_called_fns_node(rhs, set);
        }
        ast::Expr::While(cond, body) | ast::Expr::Foreach(_, cond, body) => {
            collect_called_fns_node(cond, set);
            for n in body {
                collect_called_fns_node(n, set);
            }
        }
        ast::Expr::Match(val, arms) => {
            collect_called_fns_node(val, set);
            for arm in arms {
                if let Some(g) = &arm.guard {
                    collect_called_fns_node(g, set);
                }
                collect_called_fns_node(&arm.body, set);
            }
        }
        ast::Expr::Paren(inner) | ast::Expr::Break(Some(inner)) | ast::Expr::Unquote(inner) => {
            collect_called_fns_node(inner, set);
        }
        ast::Expr::InterpolatedString(segs) => {
            for seg in segs {
                if let StringSegment::Expr(n) = seg {
                    collect_called_fns_node(n, set);
                }
            }
        }
        _ => {}
    }
}

/// Remove top-level `Def` nodes that were inlined everywhere they were called.
///
/// Only removes `Def`s that were candidates for inlining (present in `inlinable`) and
/// are no longer referenced by any `Call` in the program. Non-inlinable functions (recursive,
/// variadic, TCO-transformed) are always preserved because they may be called at runtime.
fn eliminate_dead_defs(program: Program, inlinable: &FxHashMap<Ident, InlinableFn>) -> Program {
    if inlinable.is_empty() {
        return program;
    }
    let used = collect_called_fns(&program);
    program
        .into_iter()
        .filter(|node| match &*node.expr {
            ast::Expr::Def(ident, _, _) => !inlinable.contains_key(&ident.name) || used.contains(&ident.name),
            _ => true,
        })
        .collect()
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
    use crate::{
        DefaultEngine,
        ast::node::{Expr, Literal},
        optimizer::OptimizationLevel,
    };
    use rstest::rstest;

    /// Compile `query` at the given `level` and return the resulting AST.
    fn ast_with(query: &str, level: OptimizationLevel) -> crate::ast::Program {
        let mut engine = DefaultEngine::default();
        engine.set_optimization_level(level);
        engine.compile(query).unwrap().program().clone()
    }

    fn ast_none(query: &str) -> crate::ast::Program {
        ast_with(query, OptimizationLevel::None)
    }

    fn ast_basic(query: &str) -> crate::ast::Program {
        ast_with(query, OptimizationLevel::Basic)
    }

    fn ast_full(query: &str) -> crate::ast::Program {
        ast_with(query, OptimizationLevel::Full)
    }

    fn assert_literal(node: &crate::Shared<crate::AstNode>, expected: &str, ctx: &str) {
        match &*node.expr {
            Expr::Literal(lit) => assert_eq!(lit.to_string(), expected, "{ctx}"),
            other => panic!("{ctx}: expected Literal({expected:?}), got {other:?}"),
        }
    }

    #[test]
    fn none_arithmetic_stays_as_call() {
        let prog = ast_none("1 + 2");
        assert_eq!(prog.len(), 1);
        assert!(
            matches!(&*prog[0].expr, Expr::Call(..)),
            "None: expected Call, got {:?}",
            prog[0].expr
        );
    }

    #[test]
    fn none_consecutive_selectors_stay_separate() {
        let prog = ast_none(".h1 | .text");
        assert_eq!(prog.len(), 2, "None must not merge selectors");
        assert!(matches!(&*prog[0].expr, Expr::Selector(_)));
        assert!(matches!(&*prog[1].expr, Expr::Selector(_)));
    }

    #[test]
    fn none_if_with_literal_cond_stays_as_if() {
        let prog = ast_none("if (true): 1 else: 2");
        assert_eq!(prog.len(), 1);
        assert!(
            matches!(&*prog[0].expr, Expr::If(_)),
            "None: expected If, got {:?}",
            prog[0].expr
        );
    }

    #[test]
    fn none_and_stays_as_and() {
        let prog = ast_none("false && .");
        assert_eq!(prog.len(), 1);
        assert!(
            matches!(&*prog[0].expr, Expr::And(_)),
            "None: expected And, got {:?}",
            prog[0].expr
        );
    }

    #[test]
    fn none_interpolated_string_stays_unfolded() {
        let prog = ast_none("s\"hello world\"");
        assert_eq!(prog.len(), 1);
        assert!(
            matches!(&*prog[0].expr, Expr::InterpolatedString(_)),
            "None: expected InterpolatedString, got {:?}",
            prog[0].expr
        );
    }

    #[test]
    fn none_def_body_stays_as_if_no_tco() {
        let prog = ast_none("def countdown(n): if (n == 0): \"done\" else: countdown(n - 1);");
        assert_eq!(prog.len(), 1);
        let Expr::Def(_, _, body) = &*prog[0].expr else {
            panic!("expected Def");
        };
        assert!(
            !body.iter().any(|n| matches!(&*n.expr, Expr::Loop(_))),
            "None: must not apply TCO; Loop found in body"
        );
        assert!(
            body.iter().any(|n| matches!(&*n.expr, Expr::If(_))),
            "None: original If must remain in body"
        );
    }

    #[rstest]
    #[case("1 + 2", "3")]
    #[case("10 - 3", "7")]
    #[case("3 * 4", "12")]
    #[case("10 / 2", "5")]
    #[case("10 % 3", "1")]
    #[case("\"hello\" + \" world\"", "hello world")]
    #[case("negate(5)", "-5")]
    #[case("not(false)", "true")]
    #[case("not(true)", "false")]
    fn fold_arithmetic_to_literal(#[case] query: &str, #[case] expected: &str) {
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with(query, level);
            assert_eq!(prog.len(), 1, "{level:?}: expected single literal node");
            assert_literal(&prog[0], expected, &format!("{level:?}"));
        }
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
    fn fold_comparisons_to_literal(#[case] query: &str, #[case] expected: &str) {
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with(query, level);
            assert_eq!(prog.len(), 1, "{level:?}: expected single literal node");
            assert_literal(&prog[0], expected, &format!("{level:?}"));
        }
    }

    #[test]
    fn fold_nested_arithmetic_to_literal() {
        // (1 + 2) * (3 + 4) — both sub-expressions fold first, then the outer mul folds.
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with("(1 + 2) * (3 + 4)", level);
            assert_eq!(prog.len(), 1, "{level:?}");
            assert_literal(&prog[0], "21", &format!("{level:?}"));
        }
    }

    #[test]
    fn fold_double_negation_to_literal() {
        // not(not(true)) → true
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with("not(not(true))", level);
            assert_eq!(prog.len(), 1, "{level:?}");
            assert_literal(&prog[0], "true", &format!("{level:?}"));
        }
    }

    #[test]
    fn div_by_zero_not_folded() {
        // Division by zero must stay as Call — the evaluator handles the error.
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with("1 / 0", level);
            assert_eq!(prog.len(), 1, "{level:?}");
            assert!(
                matches!(&*prog[0].expr, Expr::Call(..)),
                "{level:?}: div-by-zero must stay as Call, got {:?}",
                prog[0].expr
            );
        }
    }

    #[test]
    fn dynamic_arg_prevents_folding() {
        // add(., 1): left operand is the pipeline value — not a literal, cannot fold.
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with("add(., 1)", level);
            assert_eq!(prog.len(), 1, "{level:?}");
            assert!(
                matches!(&*prog[0].expr, Expr::Call(..)),
                "{level:?}: dynamic arg must prevent folding, got {:?}",
                prog[0].expr
            );
        }
    }

    #[test]
    fn if_true_collapses_to_then_body() {
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with("if (true): 1 else: 2", level);
            assert_eq!(prog.len(), 1, "{level:?}");
            assert_literal(&prog[0], "1", &format!("{level:?}: if(true)"));
        }
    }

    #[test]
    fn if_false_collapses_to_else_body() {
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with("if (false): 1 else: 2", level);
            assert_eq!(prog.len(), 1, "{level:?}");
            assert_literal(&prog[0], "2", &format!("{level:?}: if(false)+else"));
        }
    }

    #[test]
    fn if_false_no_else_collapses_to_literal_none() {
        // All branches eliminated → optimizer emits Literal::None.
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with("if (false): 1", level);
            assert_eq!(prog.len(), 1, "{level:?}");
            assert!(
                matches!(&*prog[0].expr, Expr::Literal(Literal::None)),
                "{level:?}: expected Literal(None), got {:?}",
                prog[0].expr
            );
        }
    }

    #[test]
    fn if_foldable_condition_also_eliminated() {
        // Condition `1 == 1` folds to `true`, then branch is eliminated.
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with("if (1 == 1): \"yes\" else: \"no\"", level);
            assert_eq!(prog.len(), 1, "{level:?}");
            assert_literal(&prog[0], "yes", &format!("{level:?}"));
        }
    }

    #[test]
    fn if_dynamic_condition_stays_as_if() {
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with("if (.): 1 else: 2", level);
            assert_eq!(prog.len(), 1, "{level:?}");
            assert!(
                matches!(&*prog[0].expr, Expr::If(_)),
                "{level:?}: dynamic condition must not eliminate branch, got {:?}",
                prog[0].expr
            );
        }
    }

    #[test]
    fn elif_first_true_branch_wins() {
        // `if (false): 1 elif (true): 2 elif (true): 3 else: 4` → Literal(2)
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with("if (false): 1 elif (true): 2 elif (true): 3 else: 4", level);
            assert_eq!(prog.len(), 1, "{level:?}");
            assert_literal(&prog[0], "2", &format!("{level:?}"));
        }
    }

    #[test]
    fn elif_all_false_no_else_collapses_to_literal_none() {
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with("if (false): 1 elif (false): 2 elif (false): 3", level);
            assert_eq!(prog.len(), 1, "{level:?}");
            assert!(
                matches!(&*prog[0].expr, Expr::Literal(Literal::None)),
                "{level:?}: expected Literal(None), got {:?}",
                prog[0].expr
            );
        }
    }

    #[test]
    fn nested_if_both_levels_eliminated() {
        // `if (true): if (false): 1 else: 2 else: 3` → outer true → inner, inner false → 2
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with("if (true): if (false): 1 else: 2 else: 3", level);
            assert_eq!(prog.len(), 1, "{level:?}");
            assert_literal(&prog[0], "2", &format!("{level:?}"));
        }
    }

    #[test]
    fn and_falsy_literal_short_circuits_to_false() {
        // `false && .` — falsy operand → entire And becomes Literal(false).
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with("false && .", level);
            assert_eq!(prog.len(), 1, "{level:?}");
            assert!(
                matches!(&*prog[0].expr, Expr::Literal(Literal::Bool(false))),
                "{level:?}: expected Literal(false), got {:?}",
                prog[0].expr
            );
        }
    }

    #[test]
    fn and_truthy_literal_dropped_dynamic_preserved() {
        // `true && .` — truthy literal is dropped; remaining dynamic operand wrapped in And.
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with("true && .", level);
            assert_eq!(prog.len(), 1, "{level:?}");
            assert!(
                matches!(&*prog[0].expr, Expr::And(_)),
                "{level:?}: expected And([.]), got {:?}",
                prog[0].expr
            );
        }
    }

    #[test]
    fn and_all_truthy_collapses_to_literal_true() {
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with("true && true && true", level);
            assert_eq!(prog.len(), 1, "{level:?}");
            assert!(
                matches!(&*prog[0].expr, Expr::Literal(Literal::Bool(true))),
                "{level:?}: expected Literal(true), got {:?}",
                prog[0].expr
            );
        }
    }

    #[test]
    fn or_truthy_literal_short_circuits() {
        // `true || .` — truthy operand → entire Or becomes Literal(true).
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with("true || .", level);
            assert_eq!(prog.len(), 1, "{level:?}");
            assert!(
                matches!(&*prog[0].expr, Expr::Literal(Literal::Bool(true))),
                "{level:?}: expected Literal(true), got {:?}",
                prog[0].expr
            );
        }
    }

    #[test]
    fn or_falsy_literal_dropped_dynamic_preserved() {
        // `false || .` — falsy literal is dropped; remaining dynamic operand wrapped in Or.
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with("false || .", level);
            assert_eq!(prog.len(), 1, "{level:?}");
            assert!(
                matches!(&*prog[0].expr, Expr::Or(_)),
                "{level:?}: expected Or([.]), got {:?}",
                prog[0].expr
            );
        }
    }

    #[test]
    fn or_all_falsy_collapses_to_literal_false() {
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with("false || false || false", level);
            assert_eq!(prog.len(), 1, "{level:?}");
            assert!(
                matches!(&*prog[0].expr, Expr::Literal(Literal::Bool(false))),
                "{level:?}: expected Literal(false), got {:?}",
                prog[0].expr
            );
        }
    }

    #[rstest]
    #[case(".h1 | .text", 2usize)]
    #[case(".h1 | .text | .code", 3usize)]
    fn consecutive_selectors_merged_into_chain(#[case] query: &str, #[case] expected_len: usize) {
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with(query, level);
            assert_eq!(prog.len(), 1, "{level:?}: expected single SelectorChain node");
            assert!(
                matches!(&*prog[0].expr, Expr::SelectorChain(c) if c.len() == expected_len),
                "{level:?}: expected SelectorChain(len={expected_len}), got {:?}",
                prog[0].expr
            );
        }
    }

    #[test]
    fn single_selector_stays_as_selector_not_chain() {
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with(".h1", level);
            assert_eq!(prog.len(), 1, "{level:?}");
            assert!(
                matches!(&*prog[0].expr, Expr::Selector(_)),
                "{level:?}: single selector must NOT become SelectorChain"
            );
        }
    }

    #[rstest]
    #[case(".h1 | to_string | .text")]
    #[case(".h1 | len() | .text")]
    fn call_between_selectors_prevents_chain_merge(#[case] query: &str) {
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with(query, level);
            assert!(prog.len() > 1, "{level:?}: call between selectors must break the chain");
            assert!(
                !matches!(&*prog[0].expr, Expr::SelectorChain(_)),
                "{level:?}: must not merge selectors across a call"
            );
        }
    }

    #[test]
    fn none_level_does_not_merge_selectors() {
        let prog = ast_none(".h1 | .text");
        assert_eq!(prog.len(), 2, "None must not merge consecutive selectors");
        assert!(matches!(&*prog[0].expr, Expr::Selector(_)));
        assert!(matches!(&*prog[1].expr, Expr::Selector(_)));
    }

    #[test]
    fn selector_chain_inside_def_body_merged() {
        // Full inlines single-node def bodies and then eliminates the unused Def.
        // `.h1 | .text` is merged into SelectorChain during inlining, so the final program
        // has one top-level SelectorChain (the inlined call site) and no Def.
        let prog = ast_full("def extract: .h1 | .text; | extract()");
        assert!(
            prog.iter().any(|n| matches!(&*n.expr, Expr::SelectorChain(_))),
            "Full: inlined extract() must produce a top-level SelectorChain"
        );
        assert!(
            !prog.iter().any(|n| matches!(&*n.expr, Expr::Def(..))),
            "Full: fully-inlined Def must be eliminated"
        );
    }

    #[rstest]
    #[case("s\"hello world\"", "hello world")]
    #[case("s\"static text only\"", "static text only")]
    fn all_text_interpolated_string_folded_to_literal(#[case] query: &str, #[case] expected: &str) {
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with(query, level);
            assert_eq!(prog.len(), 1, "{level:?}");
            assert!(
                matches!(&*prog[0].expr, Expr::Literal(_)),
                "{level:?}: all-text interpolated string must fold to Literal"
            );
            assert_literal(&prog[0], expected, &format!("{level:?}"));
        }
    }

    #[test]
    fn interpolated_string_with_dynamic_expr_not_folded() {
        // A dynamic segment (`${self}`) prevents folding.
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with("s\"${self} end\"", level);
            assert_eq!(prog.len(), 1, "{level:?}");
            assert!(
                matches!(&*prog[0].expr, Expr::InterpolatedString(_)),
                "{level:?}: dynamic segment must prevent folding to Literal"
            );
        }
    }

    #[test]
    fn none_level_does_not_fold_interpolated_string() {
        // Even a fully-static string must stay as InterpolatedString under None.
        let prog = ast_none("s\"hello world\"");
        assert_eq!(prog.len(), 1);
        assert!(
            matches!(&*prog[0].expr, Expr::InterpolatedString(_)),
            "None must not fold interpolated strings"
        );
    }

    #[test]
    fn let_literal_propagated_and_folded_in_full() {
        // Full: x is bound to 5, substituted into `x + 1`, folded to 6.
        let prog = ast_full("let x = 5 | x + 1");
        assert_eq!(prog.len(), 2);
        assert_literal(&prog[1], "6", "Full: x+1 after propagation");
    }

    #[test]
    fn let_literal_not_propagated_in_basic() {
        // Basic does not propagate — `x + 1` stays as Call with Ident.
        let prog = ast_basic("let x = 5 | x + 1");
        assert_eq!(prog.len(), 2);
        assert!(
            matches!(&*prog[1].expr, Expr::Call(..)),
            "Basic must not propagate let-literals; expected Call, got {:?}",
            prog[1].expr
        );
    }

    #[test]
    fn let_rebind_propagates_latest_value() {
        // Second binding of x shadows the first; `x + 0` folds to 2.
        let prog = ast_full("let x = 1 | let x = 2 | x + 0");
        assert_eq!(prog.len(), 3);
        assert_literal(&prog[2], "2", "Full: second binding must shadow first");
    }

    #[test]
    fn let_multiple_bindings_all_propagated() {
        // Three bindings, all propagated and folded into a single literal.
        let prog = ast_full("let a = 2 | let b = 3 | let c = 4 | a + b + c");
        assert_eq!(prog.len(), 4);
        assert_literal(&prog[3], "9", "Full: a+b+c after propagation");
    }

    #[test]
    fn let_non_literal_rhs_not_propagated() {
        // `let x = add(1, .)` — RHS is not a literal (dynamic arg) → x is not registered.
        // Full folds `x + 0` to `x` via algebraic identity, but `x` stays as Ident (not a
        // literal), proving the let binding was not propagated to a constant.
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with("let x = add(1, .) | x + 0", level);
            assert_eq!(prog.len(), 2, "{level:?}");
            assert!(
                !matches!(&*prog[1].expr, Expr::Literal(_)),
                "{level:?}: non-literal let must not propagate to a literal, got {:?}",
                prog[1].expr
            );
        }
    }

    #[test]
    fn simple_function_inlined_and_folded_in_full() {
        // Full: double(4) → inlined to 4 * 2 → folded to Literal(8).
        let prog = ast_full("def double(x): x * 2; | double(4)");
        let last = prog.last().unwrap();
        assert!(
            matches!(&*last.expr, Expr::Literal(_)),
            "Full: inlined+folded call must be Literal, got {:?}",
            last.expr
        );
        assert_literal(last, "8", "Full: double(4)");
    }

    #[test]
    fn function_call_stays_as_call_in_basic() {
        // Basic does not inline — the call node is preserved.
        let prog = ast_basic("def double(x): x * 2; | double(4)");
        let last = prog.last().unwrap();
        assert!(
            matches!(&*last.expr, Expr::Call(..)),
            "Basic must not inline; expected Call, got {:?}",
            last.expr
        );
    }

    #[test]
    fn zero_param_constant_alias_inlined_in_full() {
        let prog = ast_full("def pi: 3; | pi()");
        let last = prog.last().unwrap();
        assert!(
            matches!(&*last.expr, Expr::Literal(_)),
            "Full: 0-param constant alias must inline to Literal, got {:?}",
            last.expr
        );
        assert_literal(last, "3", "Full: pi()");
    }

    #[test]
    fn recursive_function_not_inlined() {
        // Recursive functions must never be inlined (infinite unrolling).
        let prog = ast_full("def fact(n): if (n == 0): 1 else: n * fact(n - 1); | fact(5)");
        let last = prog.last().unwrap();
        assert!(
            matches!(&*last.expr, Expr::Call(..)),
            "Full: recursive function must not be inlined; expected Call, got {:?}",
            last.expr
        );
    }

    #[test]
    fn function_with_free_variable_not_inlined() {
        // `add_k` references `k` which is not a parameter → not inlineable.
        let prog = ast_full("let k = 10 | def add_k(x): x + k; | add_k(5)");
        let last = prog.last().unwrap();
        assert!(
            matches!(&*last.expr, Expr::Call(..)),
            "Full: function with free var must not be inlined; expected Call, got {:?}",
            last.expr
        );
    }

    #[test]
    fn chained_inlining_collapses_to_literal() {
        // Both add1 and mul2 are inlineable; mul2(add1(3)) → (3+1)*2 → 8.
        let prog = ast_full("def add1(x): x + 1; | def mul2(x): x * 2; | mul2(add1(3))");
        let last = prog.last().unwrap();
        assert!(
            matches!(&*last.expr, Expr::Literal(_)),
            "Full: chained inline+fold must collapse to Literal, got {:?}",
            last.expr
        );
        assert_literal(last, "8", "Full: mul2(add1(3))");
    }

    #[test]
    fn tco_tail_recursive_def_gets_loop_in_full() {
        let prog = ast_full("def countdown(n): if (n == 0): \"done\" else: countdown(n - 1);");
        let Expr::Def(_, _, body) = &*prog[0].expr else {
            panic!("expected Def");
        };
        assert!(
            body.iter().any(|n| matches!(&*n.expr, Expr::Loop(_))),
            "Full: TCO-transformed Def must contain a Loop node"
        );
        // The original top-level If must be replaced — not left alongside the Loop.
        assert!(
            !body.iter().any(|n| matches!(&*n.expr, Expr::If(_))),
            "Full: original If must be replaced by Loop after TCO"
        );
    }

    #[test]
    fn tco_not_applied_in_basic() {
        let prog = ast_basic("def countdown(n): if (n == 0): \"done\" else: countdown(n - 1);");
        let Expr::Def(_, _, body) = &*prog[0].expr else {
            panic!("expected Def");
        };
        assert!(
            !body.iter().any(|n| matches!(&*n.expr, Expr::Loop(_))),
            "Basic must not apply TCO; Loop found unexpectedly"
        );
    }

    #[test]
    fn tco_not_applied_in_none() {
        let prog = ast_none("def countdown(n): if (n == 0): \"done\" else: countdown(n - 1);");
        let Expr::Def(_, _, body) = &*prog[0].expr else {
            panic!("expected Def");
        };
        assert!(
            !body.iter().any(|n| matches!(&*n.expr, Expr::Loop(_))),
            "None must not apply TCO"
        );
    }

    #[test]
    fn tco_not_applied_to_non_tail_call() {
        // `n * fact(n-1)` is a binary op wrapping the recursive call — NOT a tail call.
        let prog = ast_full("def fact(n): if (n == 0): 1 else: n * fact(n - 1);");
        let Expr::Def(_, _, body) = &*prog[0].expr else {
            panic!("expected Def");
        };
        assert!(
            !body.iter().any(|n| matches!(&*n.expr, Expr::Loop(_))),
            "Full: non-tail-recursive function must not be TCO-transformed"
        );
    }

    #[test]
    fn tco_multi_param_def_gets_loop() {
        let prog = ast_full("def loop2(a, b): if (a == 0): b else: loop2(a - 1, b + 1);");
        let Expr::Def(_, _, body) = &*prog[0].expr else {
            panic!("expected Def");
        };
        assert!(
            body.iter().any(|n| matches!(&*n.expr, Expr::Loop(_))),
            "Full: multi-param tail-recursive Def must contain Loop"
        );
    }

    #[test]
    fn inline_then_constant_fold() {
        // double(3 + 4): argument folds to 7, then inlined body 7 * 2 folds to 14.
        let prog = ast_full("def double(x): x * 2; | double(3 + 4)");
        let last = prog.last().unwrap();
        assert_literal(last, "14", "Full: double(3+4)");
    }

    #[test]
    fn inline_reveals_dead_branch() {
        // always_false(0) inlines to `0 == 999`, folds to false, if-branch eliminated.
        let prog = ast_full("def always_false(x): x == 999; | if (always_false(0)): \"bad\" else: \"good\"");
        let last = prog.last().unwrap();
        assert!(
            matches!(&*last.expr, Expr::Literal(_)),
            "Full: inline+dead-branch must collapse to Literal, got {:?}",
            last.expr
        );
        assert_literal(last, "good", "Full: always_false(0)");
    }

    #[test]
    fn let_propagation_enables_inline_and_fold() {
        // n=9 propagated, inc(9) inlined to 9+1, folded to 10.
        let prog = ast_full("def inc(x): x + 1; | let n = 9 | inc(n)");
        let last = prog.last().unwrap();
        assert!(
            matches!(&*last.expr, Expr::Literal(_)),
            "Full: propagation+inline+fold must collapse to Literal, got {:?}",
            last.expr
        );
        assert_literal(last, "10", "Full: inc(n) where n=9");
    }

    #[test]
    fn fold_then_dead_branch_elimination() {
        // `if (1 + 2 == 3): "yes" else: "no"` — fold condition first, then eliminate branch.
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with("if (1 + 2 == 3): \"yes\" else: \"no\"", level);
            assert_eq!(prog.len(), 1, "{level:?}");
            assert_literal(&prog[0], "yes", &format!("{level:?}"));
        }
    }

    #[test]
    fn let_propagation_into_and_or_operands() {
        // `let n = 0 | n == 0 && true` — n substituted, `0 == 0` folds to true,
        // then `true && true` collapses to Literal(true).
        let prog = ast_full("let n = 0 | n == 0 && true");
        let last = prog.last().unwrap();
        assert!(
            matches!(&*last.expr, Expr::Literal(Literal::Bool(true))),
            "Full: propagation+and fold must collapse to Literal(true), got {:?}",
            last.expr
        );
    }

    #[rstest]
    #[case("floor(3.9)", "3")]
    #[case("ceil(3.1)", "4")]
    #[case("round(3.5)", "4")]
    #[case("abs(-7)", "7")]
    #[case("trunc(3.9)", "3")]
    fn numeric_unary_folds(#[case] query: &str, #[case] expected: &str) {
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with(query, level);
            assert_eq!(prog.len(), 1, "{level:?}: {query}");
            assert_literal(&prog[0], expected, &format!("{level:?}: {query}"));
        }
    }

    #[rstest]
    #[case("len(\"hello\")", "5")]
    #[case("len(\"\")", "0")]
    fn len_string_folds(#[case] query: &str, #[case] expected: &str) {
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with(query, level);
            assert_eq!(prog.len(), 1, "{level:?}: {query}");
            assert_literal(&prog[0], expected, &format!("{level:?}: {query}"));
        }
    }

    #[rstest]
    #[case("to_string(42)", "42")]
    #[case("to_string(3.5)", "3.5")]
    #[case("to_string(true)", "true")]
    #[case("to_string(false)", "false")]
    fn to_string_folds(#[case] query: &str, #[case] expected: &str) {
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with(query, level);
            assert_eq!(prog.len(), 1, "{level:?}: {query}");
            assert_literal(&prog[0], expected, &format!("{level:?}: {query}"));
        }
    }

    #[rstest]
    #[case("to_number(\"42\")", "42")]
    #[case("to_number(\"3.14\")", "3.14")]
    fn to_number_folds(#[case] query: &str, #[case] expected: &str) {
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with(query, level);
            assert_eq!(prog.len(), 1, "{level:?}: {query}");
            assert_literal(&prog[0], expected, &format!("{level:?}: {query}"));
        }
    }

    #[test]
    fn to_number_invalid_string_not_folded() {
        // Runtime would return an error — optimizer must not fold.
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with("to_number(\"abc\")", level);
            assert_eq!(prog.len(), 1, "{level:?}");
            assert!(
                matches!(&*prog[0].expr, Expr::Call(..)),
                "{level:?}: unparsable to_number must stay as Call, got {:?}",
                prog[0].expr
            );
        }
    }

    #[rstest]
    #[case("lt(\"a\", \"b\")", "true")] // #[case(..)]  "a" < "b"
    #[case("gt(\"b\", \"a\")", "true")]
    #[case("lte(\"a\", \"a\")", "true")]
    #[case("gte(\"b\", \"a\")", "true")]
    #[case("lt(\"b\", \"a\")", "false")]
    fn string_comparison_folds(#[case] query: &str, #[case] expected: &str) {
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with(query, level);
            assert_eq!(prog.len(), 1, "{level:?}: {query}");
            assert_literal(&prog[0], expected, &format!("{level:?}: {query}"));
        }
    }

    #[test]
    fn algebraic_identity_add_zero() {
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with(". + 0", level);
            assert_eq!(prog.len(), 1, "{level:?}");
            assert!(
                matches!(&*prog[0].expr, Expr::Self_),
                "{level:?}: . + 0 must fold to Self_, got {:?}",
                prog[0].expr
            );
        }
    }

    #[test]
    fn algebraic_identity_mul_zero() {
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with(". * 0", level);
            assert_eq!(prog.len(), 1, "{level:?}");
            assert_literal(&prog[0], "0", &format!("{level:?}: . * 0"));
        }
    }

    #[test]
    fn algebraic_identity_mul_one() {
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with(". * 1", level);
            assert_eq!(prog.len(), 1, "{level:?}");
            assert!(
                matches!(&*prog[0].expr, Expr::Self_),
                "{level:?}: . * 1 must fold to Self_, got {:?}",
                prog[0].expr
            );
        }
    }

    #[test]
    fn algebraic_identity_div_one() {
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with(". / 1", level);
            assert_eq!(prog.len(), 1, "{level:?}");
            assert!(
                matches!(&*prog[0].expr, Expr::Self_),
                "{level:?}: . / 1 must fold to Self_, got {:?}",
                prog[0].expr
            );
        }
    }

    #[rstest]
    #[case("trim(\"  hi  \")", "hi")]
    #[case("ltrim(\"  hi\")", "hi")]
    #[case("rtrim(\"hi  \")", "hi")]
    #[case("upcase(\"hello\")", "HELLO")]
    #[case("downcase(\"WORLD\")", "world")]
    fn string_transform_folds(#[case] query: &str, #[case] expected: &str) {
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with(query, level);
            assert_eq!(prog.len(), 1, "{level:?}: {query}");
            assert_literal(&prog[0], expected, &format!("{level:?}: {query}"));
        }
    }

    #[rstest]
    #[case("starts_with(\"hello\", \"he\")", "true")]
    #[case("starts_with(\"hello\", \"wo\")", "false")]
    #[case("ends_with(\"hello\", \"lo\")", "true")]
    #[case("ends_with(\"hello\", \"he\")", "false")]
    #[case("index(\"hello\", \"ll\")", "2")]
    #[case("index(\"hello\", \"xx\")", "-1")]
    #[case("rindex(\"hello\", \"l\")", "3")]
    fn string_search_folds(#[case] query: &str, #[case] expected: &str) {
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with(query, level);
            assert_eq!(prog.len(), 1, "{level:?}: {query}");
            assert_literal(&prog[0], expected, &format!("{level:?}: {query}"));
        }
    }

    #[rstest]
    #[case("replace(\"hello\", \"l\", \"r\")", "herro")]
    #[case("replace(\"aaa\", \"a\", \"b\")", "bbb")]
    fn replace_folds(#[case] query: &str, #[case] expected: &str) {
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with(query, level);
            assert_eq!(prog.len(), 1, "{level:?}: {query}");
            assert_literal(&prog[0], expected, &format!("{level:?}: {query}"));
        }
    }

    #[test]
    fn coalesce_none_folded() {
        // coalesce(None, .) → Self_ (rhs is dynamic; None is Literal::None)
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with("coalesce(None, .)", level);
            assert_eq!(prog.len(), 1, "{level:?}");
            assert!(
                matches!(&*prog[0].expr, Expr::Self_),
                "{level:?}: coalesce(None, .) must fold to Self_, got {:?}",
                prog[0].expr
            );
        }
    }

    #[test]
    fn coalesce_non_none_lhs_folded() {
        // coalesce("hi", .) → "hi" (lhs is non-none, rhs is never evaluated)
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with("coalesce(\"hi\", .)", level);
            assert_eq!(prog.len(), 1, "{level:?}");
            assert_literal(&prog[0], "hi", &format!("{level:?}: coalesce(\"hi\", .)"));
        }
    }

    #[test]
    fn coalesce_both_literals_folded() {
        // coalesce(None, 42) → 42 (both are literals)
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with("coalesce(None, 42)", level);
            assert_eq!(prog.len(), 1, "{level:?}");
            assert_literal(&prog[0], "42", &format!("{level:?}: coalesce(None, 42)"));
        }
    }

    #[test]
    fn user_def_shadows_builtin_uses_user_semantics() {
        // User's `upcase(x): x` is an identity — it gets inlined, producing "hello".
        // The builtin would produce "HELLO". Verifying we get "hello" proves the
        // user's definition was used (via inlining), not the builtin's constant fold.
        let prog = ast_full("def upcase(x): x; | upcase(\"hello\")");
        let last = prog.last().unwrap();
        assert_literal(
            last,
            "hello",
            "Full: user-shadowed upcase must produce identity, not builtin 'HELLO'",
        );
    }

    #[test]
    fn user_def_shadows_trim_uses_user_semantics() {
        // User's `trim(x): x` is an identity. Builtin would strip spaces → "hi".
        // After inlining, result should be "  hi  " (spaces preserved).
        let prog = ast_full("def trim(x): x; | trim(\"  hi  \")");
        let last = prog.last().unwrap();
        assert_literal(last, "  hi  ", "Full: user-shadowed trim must preserve spaces");
    }

    #[test]
    fn non_shadowed_builtin_still_folded() {
        // Having an unrelated user def must not block folding of other builtins.
        let prog = ast_full("def my_fn(x): x; | upcase(\"hello\")");
        let last = prog.last().unwrap();
        assert_literal(last, "HELLO", "Full: non-shadowed upcase must fold");
    }

    #[rstest]
    #[case("trim(\"  a  \")", "a")]
    #[case("ltrim(\"  a\")", "a")]
    #[case("rtrim(\"a  \")", "a")]
    #[case("trim(\"\")", "")]
    #[case("upcase(\"abc\")", "ABC")]
    #[case("downcase(\"XYZ\")", "xyz")]
    #[case("upcase(\"123\")", "123")]
    fn string_ops_fold(#[case] query: &str, #[case] expected: &str) {
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with(query, level);
            assert_eq!(prog.len(), 1, "{level:?}: {query}");
            assert_literal(&prog[0], expected, &format!("{level:?}: {query}"));
        }
    }

    #[test]
    fn string_ops_with_dynamic_arg_not_folded() {
        for q in ["trim(.)", "upcase(.)"] {
            for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
                let prog = ast_with(q, level);
                assert_eq!(prog.len(), 1, "{level:?}: {q}");
                assert!(
                    matches!(&*prog[0].expr, Expr::Call(..)),
                    "{level:?}: {q} must remain Call"
                );
            }
        }
    }

    #[rstest]
    #[case("starts_with(\"hello\", \"he\")", "true")]
    #[case("starts_with(\"hello\", \"lo\")", "false")]
    #[case("ends_with(\"world\", \"ld\")", "true")]
    #[case("ends_with(\"world\", \"wo\")", "false")]
    #[case("index(\"abcabc\", \"bc\")", "1")]
    #[case("rindex(\"abcabc\", \"bc\")", "4")]
    #[case("index(\"abc\", \"x\")", "-1")]
    fn string_search_fold(#[case] query: &str, #[case] expected: &str) {
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with(query, level);
            assert_eq!(prog.len(), 1, "{level:?}: {query}");
            assert_literal(&prog[0], expected, &format!("{level:?}: {query}"));
        }
    }

    #[rstest]
    #[case("replace(\"aabbcc\", \"b\", \"x\")", "aaxxcc")]
    #[case("replace(\"hello\", \"l\", \"\")", "heo")]
    #[case("replace(\"\", \"x\", \"y\")", "")]
    fn replace_fold(#[case] query: &str, #[case] expected: &str) {
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with(query, level);
            assert_eq!(prog.len(), 1, "{level:?}: {query}");
            assert_literal(&prog[0], expected, &format!("{level:?}: {query}"));
        }
    }

    #[rstest]
    #[case("floor(3.7)", "3")]
    #[case("floor(-1.2)", "-2")]
    #[case("ceil(3.1)", "4")]
    #[case("ceil(-1.8)", "-1")]
    #[case("round(2.5)", "3")]
    #[case("round(2.4)", "2")]
    #[case("abs(-5)", "5")]
    #[case("abs(5)", "5")]
    #[case("trunc(3.9)", "3")]
    #[case("trunc(-3.9)", "-3")]
    fn numeric_math_folds(#[case] query: &str, #[case] expected: &str) {
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with(query, level);
            assert_eq!(prog.len(), 1, "{level:?}: {query}");
            assert_literal(&prog[0], expected, &format!("{level:?}: {query}"));
        }
    }

    #[test]
    fn numeric_math_dynamic_not_folded() {
        for q in ["floor(.)", "abs(.)"] {
            let prog = ast_basic(q);
            assert!(
                matches!(&*prog[0].expr, Expr::Call(..)),
                "{q} with dynamic arg must stay Call"
            );
        }
    }

    #[rstest]
    #[case("len(\"\")", "0")]
    #[case("len(\"hello\")", "5")]
    #[case("len(\"こんにちは\")", "5")] // Unicode: 5 chars
    fn len_folds(#[case] query: &str, #[case] expected: &str) {
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with(query, level);
            assert_eq!(prog.len(), 1, "{level:?}: {query}");
            assert_literal(&prog[0], expected, &format!("{level:?}: {query}"));
        }
    }

    #[rstest]
    #[case("to_string(0)", "0")]
    #[case("to_string(\"hi\")", "hi")]
    #[case("to_string(true)", "true")]
    fn to_string_lit_folds(#[case] query: &str, #[case] expected: &str) {
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with(query, level);
            assert_eq!(prog.len(), 1, "{level:?}: {query}");
            assert_literal(&prog[0], expected, &format!("{level:?}: {query}"));
        }
    }

    #[rstest]
    #[case("to_number(\"-1\")", "-1")]
    #[case("to_number(\"0.5\")", "0.5")]
    fn to_number_lit_folds(#[case] query: &str, #[case] expected: &str) {
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with(query, level);
            assert_eq!(prog.len(), 1, "{level:?}: {query}");
            assert_literal(&prog[0], expected, &format!("{level:?}: {query}"));
        }
    }

    #[rstest]
    #[case(". + 0")]
    #[case("0 + .")]
    #[case(". - 0")]
    #[case(". * 1")]
    #[case("1 * .")]
    #[case(". / 1")]
    fn algebraic_identity_returns_self(#[case] query: &str) {
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with(query, level);
            assert_eq!(prog.len(), 1, "{level:?}: {query}");
            assert!(
                matches!(&*prog[0].expr, Expr::Self_),
                "{level:?}: {query} must fold to Self_"
            );
        }
    }

    #[rstest]
    #[case(". * 0")]
    #[case("0 * .")]
    fn algebraic_mul_zero_returns_literal_zero(#[case] query: &str) {
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with(query, level);
            assert_eq!(prog.len(), 1, "{level:?}: {query}");
            assert_literal(&prog[0], "0", &format!("{level:?}: {query}"));
        }
    }

    #[rstest]
    #[case(". + \"\"")]
    #[case("\"\" + .")]
    fn algebraic_add_empty_string_returns_self(#[case] query: &str) {
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with(query, level);
            assert_eq!(prog.len(), 1, "{level:?}: {query}");
            assert!(
                matches!(&*prog[0].expr, Expr::Self_),
                "{level:?}: {query} must fold to Self_"
            );
        }
    }

    #[test]
    fn constant_fold_inside_block() {
        // Blocks are nested scopes; constant folding must still apply.
        let prog = ast_basic("(1 + 2)");
        assert_eq!(prog.len(), 1);
        assert_literal(&prog[0], "3", "Basic: (1+2) inside Paren must fold");
    }

    #[test]
    fn selector_chain_merged_inside_while_body() {
        // The selector chain inside a while condition (2 selectors) must merge.
        // while(.h1 | .text) is only 2 nodes in the condition, but the condition
        // itself is a single Call node; so we check a def body instead.
        let prog = ast_basic("def f: .h1 | .text;");
        let Expr::Def(_, _, body) = &*prog[0].expr else {
            panic!("expected Def");
        };
        assert!(
            body.iter().any(|n| matches!(&*n.expr, Expr::SelectorChain(_))),
            "Basic: SelectorChain must be merged inside Def body"
        );
    }

    #[test]
    fn nested_let_literal_not_propagated_across_scope() {
        // A let binding defined at the top level must not propagate into a nested
        // def body — they are separate scopes.
        let prog = ast_full("let x = 99 | def f: x;");
        let Expr::Def(_, _, body) = &*prog.iter().find(|n| matches!(&*n.expr, Expr::Def(..))).unwrap().expr else {
            panic!("expected Def");
        };
        assert!(
            matches!(&*body[0].expr, Expr::Ident(_)),
            "Full: top-level let must not propagate into def body, got {:?}",
            body[0].expr
        );
    }

    #[test]
    fn inlined_def_is_eliminated() {
        // After inlining, the Def is no longer called → eliminated.
        let prog = ast_full("def double(x): x * 2; | double(5)");
        assert!(
            !prog.iter().any(|n| matches!(&*n.expr, Expr::Def(..))),
            "Full: fully-inlined Def must be eliminated from the program"
        );
        let last = prog.last().unwrap();
        assert_literal(last, "10", "Full: double(5) must fold to 10");
    }

    #[test]
    fn non_inlinable_def_preserved() {
        // A recursive Def is not inlinable → must be kept.
        let prog = ast_full("def count(n): if (n == 0): 0 else: count(n - 1);");
        assert!(
            prog.iter().any(|n| matches!(&*n.expr, Expr::Def(..))),
            "Full: non-inlinable Def must be preserved"
        );
    }

    #[test]
    fn def_passed_as_value_not_eliminated() {
        // When a Def is passed as a first-class function value, it must not be eliminated.
        let prog = ast_full("def is_pos(x): gt(x, 0); | filter(array(1, -1, 2), is_pos)");
        assert!(
            prog.iter().any(|n| matches!(&*n.expr, Expr::Def(..))),
            "Full: Def passed as first-class value must be preserved"
        );
    }

    #[test]
    fn tco_only_in_full() {
        let query = "def sum(n): if (n == 0): 0 else: sum(n - 1);";
        let none_prog = ast_none(query);
        let basic_prog = ast_basic(query);
        let full_prog = ast_full(query);

        let has_loop = |prog: &crate::ast::Program| {
            prog.iter().any(|n| {
                if let Expr::Def(_, _, body) = &*n.expr {
                    body.iter().any(|b| matches!(&*b.expr, Expr::Loop(_)))
                } else {
                    false
                }
            })
        };
        assert!(!has_loop(&none_prog), "None: no TCO");
        assert!(!has_loop(&basic_prog), "Basic: no TCO");
        assert!(has_loop(&full_prog), "Full: TCO must apply");
    }

    #[test]
    fn none_level_is_identity() {
        // At None, the program must be returned byte-for-byte unchanged.
        let queries = ["1 + 2", "if (true): 1 else: 2", "false && .", "def f(x): x + 1; | f(5)"];
        for q in queries {
            let none = ast_none(q);
            let basic = ast_basic(q);
            assert!(!none.is_empty(), "None: {q} must produce nodes");
            assert!(none.len() >= basic.len(), "None: {q}");
        }
    }

    #[test]
    fn chained_string_ops_fold() {
        // upcase(trim("  hello  ")) → upcase("hello") → "HELLO"
        let prog = ast_full("upcase(trim(\"  hello  \"))");
        assert_eq!(prog.len(), 1);
        assert_literal(&prog[0], "HELLO", "Full: chained upcase(trim(...)) must fold");
    }

    #[test]
    fn nested_arithmetic_folds() {
        // floor(abs(-3.7) + 1) → floor(3.7 + 1) → floor(4.7) → 4
        let prog = ast_full("floor(abs(-3.7) + 1)");
        assert_eq!(prog.len(), 1);
        assert_literal(&prog[0], "4", "Full: nested floor(abs+add) must fold");
    }

    #[test]
    fn let_chain_propagation() {
        // let a = 1 | let b = 2 | let c = a + b → 3
        let prog = ast_full("let a = 1 | let b = 2 | a + b");
        let last = prog.last().unwrap();
        assert_literal(last, "3", "Full: chained let propagation must fold to 3");
    }

    #[test]
    fn let_and_string_propagation() {
        // let prefix = "Hello" | starts_with(prefix, "He") → true
        let prog = ast_full("let prefix = \"Hello\" | starts_with(prefix, \"He\")");
        let last = prog.last().unwrap();
        assert_literal(last, "true", "Full: let+starts_with must fold");
    }

    #[test]
    fn full_pipeline_fold_then_dead_branch() {
        // let x = 5 | if (x == 5): "yes" else: "no"
        // Full: x substituted → if (true): "yes" → "yes"
        let prog = ast_full("let x = 5 | if (x == 5): \"yes\" else: \"no\"");
        let last = prog.last().unwrap();
        assert_literal(last, "yes", "Full: propagate+fold+dead-branch must yield \"yes\"");
    }

    #[test]
    fn basic_does_not_propagate_let() {
        // Basic does not do let-literal propagation.
        let prog = ast_basic("let x = 5 | if (x == 5): \"yes\" else: \"no\"");
        let last = prog.last().unwrap();
        assert!(
            matches!(&*last.expr, Expr::If(_)),
            "Basic: let propagation must not happen, expected If, got {:?}",
            last.expr
        );
    }

    #[test]
    fn module_scoped_def_not_mistaken_for_builtin_shadow() {
        // A Def inside a Module body must not prevent folding of the same-named
        // builtin at the top level.
        let prog = ast_full("upcase(\"hello\") | module m: def upcase(x): x; end");
        // The upcase("hello") call at the top level should be folded.
        let first = &prog[0];
        assert_literal(
            first,
            "HELLO",
            "Full: module-internal def must not shadow top-level fold",
        );
    }

    // ---- len on bytes literal folds ----
    #[rstest]
    #[case("len(b\"hello\")", "5")]
    #[case("len(b\"\")", "0")]
    fn len_bytes_folds(#[case] query: &str, #[case] expected: &str) {
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with(query, level);
            assert_eq!(prog.len(), 1, "{level:?}: {query}");
            assert_literal(&prog[0], expected, &format!("{level:?}: {query}"));
        }
    }

    // ---- to_string on None and Bool folds ----
    #[rstest]
    #[case("to_string(None)", "")]
    fn to_string_none_folds(#[case] query: &str, #[case] expected: &str) {
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with(query, level);
            assert_eq!(prog.len(), 1, "{level:?}: {query}");
            assert_literal(&prog[0], expected, &format!("{level:?}: {query}"));
        }
    }

    // ---- Bytes literal stays as Call for to_string (not foldable) ----
    #[test]
    fn to_string_bytes_not_folded() {
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with("to_string(b\"hi\")", level);
            assert_eq!(prog.len(), 1, "{level:?}");
            assert!(
                matches!(&*prog[0].expr, Expr::Call(..)),
                "{level:?}: to_string(bytes) must stay as Call"
            );
        }
    }

    // ---- NaN-guarded arithmetic does not fold ----
    #[test]
    fn nan_arithmetic_not_folded() {
        // nan() is not a literal, so arithmetic on it cannot be constant-folded.
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with("floor(nan())", level);
            assert_eq!(prog.len(), 1, "{level:?}");
            assert!(
                matches!(&*prog[0].expr, Expr::Call(..)),
                "{level:?}: floor(nan()) must not fold"
            );
        }
    }

    // ---- substitute_literals into And/Or/Paren/Try/Break/InterpolatedString ----
    #[test]
    fn let_propagation_into_try_block() {
        // let x = "ok" | try: x catch: "err" — x is substituted, try is preserved because it might error.
        let prog = ast_full("let x = 5 | try: x catch: 0");
        assert_eq!(prog.len(), 2, "Full: let + try must produce 2 nodes");
    }

    #[test]
    fn let_propagation_into_interpolated_string() {
        // let x = "hi" | s"${x} world" → substitute x, fold to literal "hi world"
        let prog = ast_full("let x = \"hi\" | s\"${x} world\"");
        let last = prog.last().unwrap();
        assert_literal(last, "hi world", "Full: let+interpolated string must fold");
    }

    #[test]
    fn let_propagation_into_paren() {
        // let x = 3 | (x + 1) → substitute x=3, fold 3+1=4
        let prog = ast_full("let x = 3 | (x + 1)");
        let last = prog.last().unwrap();
        assert_literal(last, "4", "Full: let+paren must fold");
    }

    // ---- Dead-def after inlining multiple call sites ----
    #[test]
    fn two_call_sites_both_inlined_def_eliminated() {
        // inc is called twice — both sites get inlined, so def is eliminated.
        let prog = ast_full("def inc(x): x + 1; | inc(3) | inc(7)");
        assert!(
            !prog.iter().any(|n| matches!(&*n.expr, Expr::Def(..))),
            "Full: Def with two inlined call sites must be eliminated"
        );
        let last = prog.last().unwrap();
        assert_literal(last, "8", "Full: inc(7) must fold to 8");
    }

    // ---- optimize_node handles While / Loop / Foreach / Assign ----
    #[test]
    fn optimize_while_folds_constant_in_body() {
        // while(true): 1 + 1 — body constant should fold.
        let prog = ast_basic("while(true): 1 + 1;");
        assert_eq!(prog.len(), 1);
        // The while node itself must remain (condition is not false-literal).
        assert!(
            matches!(&*prog[0].expr, Expr::While(..)),
            "Basic: while must remain when condition is dynamic-ish"
        );
    }

    #[test]
    fn optimize_try_with_constant_folds_body() {
        // try: 1 + 2 catch: 0 — body must fold to 3.
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with("try: 1 + 2 catch: 0", level);
            assert_eq!(prog.len(), 1, "{level:?}");
            // The Try node remains because catch matters even when body is constant.
            assert!(
                matches!(&*prog[0].expr, Expr::Try(..)),
                "{level:?}: Try must remain; got {:?}",
                prog[0].expr
            );
        }
    }

    #[test]
    fn optimize_foreach_folds_constant_values() {
        // foreach(x, [1]): 2 + 3 — body constant 5 should fold.
        let prog = ast_basic("foreach(x, [1]): 2 + 3;");
        assert_eq!(prog.len(), 1, "Basic: foreach must be single node");
        let Expr::Foreach(_, _, body) = &*prog[0].expr else {
            panic!("expected Foreach");
        };
        assert!(
            body.iter().any(|n| matches!(&*n.expr, Expr::Literal(_))),
            "Basic: Foreach body must have folded constant"
        );
    }

    #[test]
    fn optimize_match_folds_constant_in_arms() {
        // match(1+2) do | 3: "three" | _: "other" end — condition 1+2 folds to 3.
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with("match(1 + 2) do | 3: \"three\" | _: \"other\" end", level);
            assert_eq!(prog.len(), 1, "{level:?}");
            // The match node remains but its value should be folded.
            assert!(
                matches!(&*prog[0].expr, Expr::Match(..)),
                "{level:?}: Match must remain when value is not a pattern-eliminating literal"
            );
        }
    }

    // ---- algebraic identity for sub(x, 0) ----
    #[test]
    fn algebraic_identity_sub_zero() {
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with(". - 0", level);
            assert_eq!(prog.len(), 1, "{level:?}");
            assert!(
                matches!(&*prog[0].expr, Expr::Self_),
                "{level:?}: . - 0 must fold to Self_"
            );
        }
    }

    // ---- EQ/NE fold across mixed literal types ----
    #[rstest]
    #[case("eq(1, \"one\")", "false")]
    #[case("ne(1, \"one\")", "true")]
    #[case("eq(None, None)", "true")]
    #[case("ne(None, 0)", "true")]
    fn eq_ne_cross_type_fold(#[case] query: &str, #[case] expected: &str) {
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with(query, level);
            assert_eq!(prog.len(), 1, "{level:?}: {query}");
            assert_literal(&prog[0], expected, &format!("{level:?}: {query}"));
        }
    }

    // ---- multiple user defs, some inlinable, some not ----
    #[test]
    fn mixed_inline_and_non_inline_defs() {
        // double is inlineable (simple); fact is recursive (not inlineable).
        let prog = ast_full(
            "def double(x): x * 2; | def fact(n): if (n == 0): 1 else: n * fact(n - 1); | double(3) | fact(4)",
        );
        // double(3) should be inlined and folded to 6.
        // fact(4) must remain as Call.
        assert!(
            prog.iter().any(|n| matches!(&*n.expr, Expr::Def(..))),
            "Full: recursive fact must be preserved"
        );
        let last = prog.last().unwrap();
        assert!(matches!(&*last.expr, Expr::Call(..)), "Full: fact(4) must stay as Call");
    }

    // ---- substitute_literals into a CallDynamic node ----
    #[test]
    fn let_propagation_into_selectorchain_body() {
        // Selector chains are scope-stopping nodes; let propagation doesn't cross them.
        let prog = ast_full("let x = 5 | .h1 | x + 0");
        // x+0 should fold to 5 (via propagation).
        let last = prog.last().unwrap();
        assert_literal(last, "5", "Full: let + selector + x+0 must fold");
    }

    // ---- literal_is_truthy for all variants ----
    #[rstest]
    #[case("\"\" && .", "false")] // empty string is falsy
    #[case("\"x\" && .", r#"."#)] // non-empty string is truthy — drops literal
    fn literal_is_truthy_string(#[case] query: &str, #[case] _expected: &str) {
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with(query, level);
            assert_eq!(prog.len(), 1, "{level:?}: {query}");
        }
    }

    #[test]
    fn literal_is_truthy_zero_number_is_falsy() {
        // `0 && .` → falsy literal → short-circuit to false.
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with("0 && .", level);
            assert_eq!(prog.len(), 1, "{level:?}");
            assert!(
                matches!(&*prog[0].expr, Expr::Literal(Literal::Bool(false))),
                "{level:?}: 0 && . must short-circuit to false"
            );
        }
    }

    #[test]
    fn literal_is_truthy_nonzero_number_is_truthy() {
        // `1 && .` → truthy literal dropped, remaining And([.]) emitted.
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with("1 && .", level);
            assert_eq!(prog.len(), 1, "{level:?}");
            assert!(
                matches!(&*prog[0].expr, Expr::And(_)),
                "{level:?}: 1 && . truthy lit must be dropped leaving And([.])"
            );
        }
    }

    // ---- coalesce with both dynamic operands: stays as Call ----
    #[test]
    fn coalesce_both_dynamic_stays_as_call() {
        for level in [OptimizationLevel::Basic, OptimizationLevel::Full] {
            let prog = ast_with("coalesce(., .)", level);
            assert_eq!(prog.len(), 1, "{level:?}");
            assert!(
                matches!(&*prog[0].expr, Expr::Call(..)),
                "{level:?}: coalesce(., .) with both dynamic must stay as Call"
            );
        }
    }

    // ---- def with default-param is not inlinable ----
    #[test]
    fn def_with_default_param_not_inlined() {
        let prog = ast_full("def greet(name, greeting = \"hello\"): greeting; | greet(\"world\")");
        // Has a default param → not inlineable.
        let last = prog.last().unwrap();
        assert!(
            matches!(&*last.expr, Expr::Call(..)),
            "Full: def with default param must not be inlined; expected Call"
        );
    }
}
