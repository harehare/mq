//! Translation of mq AST expressions into sort-checked SMT terms.

use std::collections::{HashMap, HashSet};

use mq_lang::{AstBinaryOp as B, AstExpr, AstLiteral, AstNode, AstPattern, AstUnaryOp, Shared};

use super::term::{Define, Result, Signature, SmtError, Sort, Typed};

pub(super) type Node = Shared<AstNode>;

/// Variables in scope with their sorts.
pub(super) type Env = HashMap<String, Sort>;

/// A top-level `def` with plain parameters.
pub(super) struct Func {
    pub(super) params: Vec<String>,
    pub(super) body: Vec<Node>,
}

pub(super) struct Translator<'a> {
    funcs: &'a HashMap<String, Func>,
    sigs: HashMap<String, Signature>,
    defs: Vec<Define>,
    visiting: Vec<String>,
}

impl<'a> Translator<'a> {
    pub(super) fn new(funcs: &'a HashMap<String, Func>) -> Self {
        Self {
            funcs,
            sigs: HashMap::new(),
            defs: Vec::new(),
            visiting: Vec::new(),
        }
    }

    /// The definitions produced so far, callees before callers.
    pub(super) fn defs(&self) -> &[Define] {
        &self.defs
    }

    /// Defines `name` and the functions it calls, returning its signature.
    ///
    /// `hints` are extra expressions (the contracts) consulted to decide which parameters are
    /// Bool. The result sort is found by trying Int, then Bool, which is what lets a recursive
    /// call be typed before its own definition is complete.
    pub(super) fn define(&mut self, name: &str, hints: &[Node]) -> Result<Signature> {
        if let Some(sig) = self.sigs.get(name) {
            return Ok(sig.clone());
        }
        let func = self
            .funcs
            .get(name)
            .ok_or_else(|| SmtError::Unsupported(format!("function `{name}`")))?;
        if self.visiting.iter().any(|v| v == name) {
            return Err(SmtError::Unsupported(format!("mutual recursion through `{name}`")));
        }
        self.visiting.push(name.to_string());
        let result = self.define_inner(name, func, hints);
        self.visiting.pop();
        result
    }

    fn define_inner(&mut self, name: &str, func: &Func, hints: &[Node]) -> Result<Signature> {
        let mut callees = Vec::new();
        func.body.iter().for_each(|n| collect_calls(n, &mut callees));
        let recursive = callees.iter().any(|c| c == name);
        for callee in callees.iter().filter(|c| *c != name && self.funcs.contains_key(*c)) {
            self.define(callee, &[])?;
        }

        let evidence = param_evidence(&func.params, func.body.iter().chain(hints));
        let mut first_error = None;
        for params in evidence.assignments(&func.params) {
            let env: Env = params.iter().cloned().collect();
            for ret in [Sort::Int, Sort::Bool, Sort::Str, Sort::Seq(Box::new(Sort::Int))] {
                let sig = Signature {
                    params: params.iter().map(|(_, s)| s.clone()).collect(),
                    ret: ret.clone(),
                };
                self.sigs.insert(name.to_string(), sig.clone());
                match self.program(&func.body, &env) {
                    Ok(body) if body.sort == ret => {
                        self.defs.push(Define {
                            name: name.to_string(),
                            params,
                            ret,
                            body: body.term,
                            recursive,
                        });
                        return Ok(sig);
                    }
                    Ok(_) => {}
                    Err(e) => {
                        first_error.get_or_insert(e);
                    }
                }
            }
        }
        self.sigs.remove(name);
        Err(first_error.unwrap_or_else(|| SmtError::Invalid(format!("cannot infer the result sort of `{name}`"))))
    }

    /// Leading `let` bindings followed by one final expression.
    fn program(&mut self, prog: &[Node], env: &Env) -> Result<Typed> {
        let Some((last, lets)) = prog.split_last() else {
            return Err(SmtError::Invalid("empty body".to_string()));
        };
        let mut env = env.clone();
        let mut binds = Vec::new();
        for n in lets {
            let AstExpr::Let(AstPattern::Ident(id), value) = &n.expr else {
                return Err(SmtError::Unsupported(
                    "statement before the final expression (only `let` is allowed)".to_string(),
                ));
            };
            let value = self.expr(value, &env)?;
            let name = id.name.to_string();
            env.insert(name.clone(), value.sort.clone());
            binds.push((name, value));
        }
        let body = self.expr(last, &env)?;
        Ok(binds
            .into_iter()
            .rev()
            .fold(body, |acc, (name, value)| acc.bind(&name, value)))
    }

    pub(super) fn expr(&mut self, n: &Node, env: &Env) -> Result<Typed> {
        match &n.expr {
            AstExpr::Literal(AstLiteral::Number(v)) if v.is_int() => Ok(Typed::int(v.to_int())),
            AstExpr::Literal(AstLiteral::Number(_)) => Err(SmtError::Unsupported("non-integer number".to_string())),
            AstExpr::Literal(AstLiteral::Bool(b)) => Ok(Typed::bool(*b)),
            AstExpr::Literal(AstLiteral::String(text)) => Typed::string(text),
            AstExpr::Array(items) => Typed::seq(self.exprs(items, env)?),
            AstExpr::Index(target, index) => Typed::index(self.expr(target, env)?, self.expr(index, env)?),
            AstExpr::Ident(id) => {
                let name = id.name.to_string();
                let sort = env
                    .get(&name)
                    .ok_or_else(|| SmtError::Invalid(format!("unknown variable `{name}`")))?
                    .clone();
                Ok(Typed::var(&name, sort))
            }
            AstExpr::Paren(x) => self.expr(x, env),
            AstExpr::Block(p) => self.program(p, env),
            AstExpr::And(xs) => Typed::connective("and", self.exprs(xs, env)?),
            AstExpr::Or(xs) => Typed::connective("or", self.exprs(xs, env)?),
            AstExpr::UnaryOp(AstUnaryOp::Not, x) => Ok(Typed::not(self.expr(x, env)?.expect(Sort::Bool, "!")?)),
            AstExpr::UnaryOp(AstUnaryOp::Neg, x) => Typed::neg(self.expr(x, env)?),
            AstExpr::BinaryOp(op, a, b) => {
                let (a, b) = (self.expr(a, env)?, self.expr(b, env)?);
                match op {
                    B::Add => Typed::add(a, b),
                    B::Sub => Typed::arith("-", a, b),
                    B::Mul => Typed::arith("*", a, b),
                    B::Lt => Typed::compare("<", a, b),
                    B::Le => Typed::compare("<=", a, b),
                    B::Gt => Typed::compare(">", a, b),
                    B::Ge => Typed::compare(">=", a, b),
                    B::Eq => Typed::equal(a, b, false),
                    B::Ne => Typed::equal(a, b, true),
                    B::Mod => Typed::rem(a, b),
                    B::Div => Err(SmtError::Unsupported("operator `/`".to_string())),
                }
            }
            AstExpr::If(branches) => self.branches(branches, env, false),
            AstExpr::Unless(branches) => self.branches(branches, env, true),
            AstExpr::Call(id, args) => {
                let name = id.name.to_string();
                let args = self.exprs(args, env)?;
                if self.funcs.contains_key(&name) {
                    let sig = self.define(&name, &[])?;
                    return Typed::call(&name, &sig, args);
                }
                builtin_call(&name, args)
            }
            _ => Err(SmtError::Unsupported("expression".to_string())),
        }
    }

    fn exprs<'n>(&mut self, nodes: impl IntoIterator<Item = &'n Node>, env: &Env) -> Result<Vec<Typed>> {
        nodes.into_iter().map(|n| self.expr(n, env)).collect()
    }

    /// `if`/`elif`/`else` as nested `ite`; `negate` flips the first condition (`unless`).
    fn branches(&mut self, branches: &[(Option<Node>, Node)], env: &Env, negate: bool) -> Result<Typed> {
        let mut arms = Vec::new();
        let mut otherwise = None;
        for (i, (cond, body)) in branches.iter().enumerate() {
            let body = self.expr(body, env)?;
            match cond {
                Some(c) => {
                    let c = self.expr(c, env)?.expect(Sort::Bool, "if condition")?;
                    arms.push((if negate && i == 0 { Typed::not(c) } else { c }, body));
                }
                None => otherwise = Some(body),
            }
        }
        let otherwise = otherwise.ok_or_else(|| SmtError::Unsupported("`if` without `else`".to_string()))?;
        arms.into_iter()
            .rev()
            .try_fold(otherwise, |acc, (cond, body)| Typed::ite(cond, body, acc))
    }
}

/// Builtins with a direct integer translation.
fn builtin_call(name: &str, args: Vec<Typed>) -> Result<Typed> {
    let int = |t: Typed| t.expect(Sort::Int, name);
    let ite = |cmp: &'static str, a: Typed, b: Typed| Typed::ite(Typed::compare(cmp, a.clone(), b.clone())?, a, b);
    let mut args = args.into_iter();
    match (name, args.next(), args.next(), args.next()) {
        ("len", Some(x), None, None) => Typed::len(x),
        ("not", Some(x), None, None) => Ok(Typed::not(x.expect(Sort::Bool, "not")?)),
        ("contains" | "starts_with" | "ends_with", Some(a), Some(b), None) => Typed::str_predicate(name, a, b),
        ("abs", Some(x), None, None) => {
            let x = int(x)?;
            Typed::ite(
                Typed::compare(">=", x.clone(), Typed::int(0))?,
                x.clone(),
                Typed::neg(x)?,
            )
        }
        ("min", Some(a), Some(b), None) => ite("<=", int(a)?, int(b)?),
        ("max", Some(a), Some(b), None) => ite(">=", int(a)?, int(b)?),
        _ => Err(SmtError::Unsupported(format!("function `{name}`"))),
    }
}

/// What the body and contracts reveal about the sort of each parameter.
#[derive(Default)]
struct Evidence {
    bools: HashSet<String>,
    strs: HashSet<String>,
    seqs: HashSet<String>,
    /// Used with `len`, which fits both strings and arrays.
    either: HashSet<String>,
    /// Indexed with `[i]`: more likely an array than a string.
    indexed: HashSet<String>,
}

impl Evidence {
    /// Every plausible sort assignment, most likely first. Parameters that could be a string
    /// or an array are tried as a string first.
    fn assignments(&self, params: &[String]) -> Vec<Vec<(String, Sort)>> {
        let seq = || Sort::Seq(Box::new(Sort::Int));
        let mut out: Vec<Vec<(String, Sort)>> = vec![Vec::new()];
        for p in params {
            let options = if self.bools.contains(p) {
                vec![Sort::Bool]
            } else if self.strs.contains(p) {
                vec![Sort::Str]
            } else if self.seqs.contains(p) {
                vec![seq()]
            } else if self.indexed.contains(p) {
                vec![seq(), Sort::Str]
            } else if self.either.contains(p) {
                vec![Sort::Str, seq()]
            } else {
                vec![Sort::Int]
            };
            out = out
                .into_iter()
                .flat_map(|prefix| {
                    options.iter().map(move |s| {
                        let mut next = prefix.clone();
                        next.push((p.clone(), s.clone()));
                        next
                    })
                })
                .collect();
            // Keeps the search small when many parameters are ambiguous.
            out.truncate(8);
        }
        out
    }
}

fn param_evidence<'n>(params: &[String], nodes: impl IntoIterator<Item = &'n Node>) -> Evidence {
    let mut ev = Evidence::default();
    for n in nodes {
        collect_evidence(n, params, &mut ev);
    }
    ev
}

/// The parameter `node` names, looking through parentheses.
fn param_of(node: &Node, params: &[String]) -> Option<String> {
    let mut n = node;
    while let AstExpr::Paren(inner) = &n.expr {
        n = inner;
    }
    match &n.expr {
        AstExpr::Ident(id) => Some(id.name.to_string()).filter(|s| params.contains(s)),
        _ => None,
    }
}

/// Whether `node` is syntactically a Bool expression.
fn is_boolish(node: &Node) -> bool {
    match &node.expr {
        AstExpr::Literal(AstLiteral::Bool(_)) | AstExpr::And(_) | AstExpr::Or(_) => true,
        AstExpr::Paren(x) => is_boolish(x),
        AstExpr::UnaryOp(op, _) => matches!(op, AstUnaryOp::Not),
        AstExpr::BinaryOp(op, ..) => !matches!(op, B::Add | B::Sub | B::Mul | B::Div | B::Mod),
        _ => false,
    }
}

/// Whether `node` is syntactically a string.
fn is_strish(node: &Node) -> bool {
    match &node.expr {
        AstExpr::Literal(AstLiteral::String(_)) => true,
        AstExpr::Paren(x) => is_strish(x),
        AstExpr::BinaryOp(B::Add, a, b) => is_strish(a) || is_strish(b),
        _ => false,
    }
}

/// Whether `node` is syntactically an array.
fn is_seqish(node: &Node) -> bool {
    match &node.expr {
        AstExpr::Array(_) => true,
        AstExpr::Paren(x) => is_seqish(x),
        AstExpr::BinaryOp(B::Add, a, b) => is_seqish(a) || is_seqish(b),
        _ => false,
    }
}

/// Records which parameters appear where a Bool, string or array is expected inside `node`.
fn collect_evidence(node: &Node, params: &[String], ev: &mut Evidence) {
    let mark = |n: &Node, set: fn(&mut Evidence) -> &mut HashSet<String>, ev: &mut Evidence| {
        if let Some(p) = param_of(n, params) {
            set(ev).insert(p);
        }
    };
    fn bools(ev: &mut Evidence) -> &mut HashSet<String> {
        &mut ev.bools
    }
    fn strs(ev: &mut Evidence) -> &mut HashSet<String> {
        &mut ev.strs
    }
    fn seqs(ev: &mut Evidence) -> &mut HashSet<String> {
        &mut ev.seqs
    }
    fn either(ev: &mut Evidence) -> &mut HashSet<String> {
        &mut ev.either
    }
    fn indexed(ev: &mut Evidence) -> &mut HashSet<String> {
        &mut ev.indexed
    }
    match &node.expr {
        AstExpr::And(xs) | AstExpr::Or(xs) => {
            for x in xs {
                mark(x, bools, ev);
                collect_evidence(x, params, ev);
            }
        }
        AstExpr::UnaryOp(op, x) => {
            if matches!(op, AstUnaryOp::Not) {
                mark(x, bools, ev);
            }
            collect_evidence(x, params, ev);
        }
        AstExpr::If(branches) | AstExpr::Unless(branches) => {
            let boolish = branches.iter().any(|(_, body)| is_boolish(body));
            let strish = branches.iter().any(|(_, body)| is_strish(body));
            for (cond, body) in branches {
                if let Some(c) = cond {
                    mark(c, bools, ev);
                    collect_evidence(c, params, ev);
                }
                if boolish {
                    mark(body, bools, ev);
                }
                if strish {
                    mark(body, strs, ev);
                }
                collect_evidence(body, params, ev);
            }
        }
        AstExpr::BinaryOp(op, a, b) => {
            if matches!(op, B::Eq | B::Ne | B::Add) {
                for (this, other) in [(a, b), (b, a)] {
                    if is_boolish(other) && !matches!(op, B::Add) {
                        mark(this, bools, ev);
                    }
                    if is_strish(other) {
                        mark(this, strs, ev);
                    }
                    if is_seqish(other) {
                        mark(this, seqs, ev);
                    }
                }
            }
            collect_evidence(a, params, ev);
            collect_evidence(b, params, ev);
        }
        AstExpr::Index(target, index) => {
            mark(target, indexed, ev);
            collect_evidence(target, params, ev);
            collect_evidence(index, params, ev);
        }
        AstExpr::Call(id, args) => {
            match (id.name.to_string().as_str(), args.as_slice()) {
                ("len", [x]) => mark(x, either, ev),
                ("starts_with" | "ends_with" | "contains", [a, b]) => {
                    mark(a, strs, ev);
                    mark(b, strs, ev);
                }
                ("not", [x]) => mark(x, bools, ev),
                _ => {}
            }
            args.iter().for_each(|a| collect_evidence(a, params, ev));
        }
        AstExpr::Array(items) => items.iter().for_each(|a| collect_evidence(a, params, ev)),
        AstExpr::Block(p) => p.iter().for_each(|n| collect_evidence(n, params, ev)),
        AstExpr::Paren(x) | AstExpr::Let(_, x) => collect_evidence(x, params, ev),
        _ => {}
    }
}

/// Names of every function called inside `node`.
fn collect_calls(node: &Node, out: &mut Vec<String>) {
    match &node.expr {
        AstExpr::Call(id, args) => {
            out.push(id.name.to_string());
            args.iter().for_each(|a| collect_calls(a, out));
        }
        AstExpr::And(xs) | AstExpr::Or(xs) => xs.iter().for_each(|x| collect_calls(x, out)),
        AstExpr::UnaryOp(_, x) | AstExpr::Let(_, x) | AstExpr::Paren(x) => collect_calls(x, out),
        AstExpr::BinaryOp(_, a, b) => {
            collect_calls(a, out);
            collect_calls(b, out);
        }
        AstExpr::If(bs) | AstExpr::Unless(bs) => {
            for (c, b) in bs {
                if let Some(c) = c {
                    collect_calls(c, out);
                }
                collect_calls(b, out);
            }
        }
        AstExpr::Block(p) => p.iter().for_each(|x| collect_calls(x, out)),
        _ => {}
    }
}
