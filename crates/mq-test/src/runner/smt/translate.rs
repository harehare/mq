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
    /// The sorts mq-check inferred for each function, or why it has none.
    sigs: &'a HashMap<String, Result<Signature>>,
    defs: Vec<Define>,
    done: HashSet<String>,
    visiting: Vec<String>,
    /// Every sort of a translated expression, for the datatypes the script must declare.
    seen: Vec<Sort>,
    /// Optional variables known not to be `None` at this point, read as their value.
    narrowed: HashMap<String, Typed>,
}

impl<'a> Translator<'a> {
    pub(super) fn new(funcs: &'a HashMap<String, Func>, sigs: &'a HashMap<String, Result<Signature>>) -> Self {
        Self {
            funcs,
            sigs,
            defs: Vec::new(),
            done: HashSet::new(),
            visiting: Vec::new(),
            seen: Vec::new(),
            narrowed: HashMap::new(),
        }
    }

    /// The sorts of the expressions translated so far.
    pub(super) fn sorts_seen(&self) -> &[Sort] {
        &self.seen
    }

    /// The definitions produced so far, callees before callers.
    pub(super) fn defs(&self) -> &[Define] {
        &self.defs
    }

    fn signature(&self, name: &str) -> Result<Signature> {
        self.sigs
            .get(name)
            .cloned()
            .unwrap_or_else(|| Err(SmtError::Unsupported(format!("function `{name}`"))))
    }

    /// Defines `name` and the functions it calls, returning its signature.
    pub(super) fn define(&mut self, name: &str) -> Result<Signature> {
        let sig = self.signature(name)?;
        if self.done.contains(name) {
            return Ok(sig);
        }
        let func = self
            .funcs
            .get(name)
            .ok_or_else(|| SmtError::Unsupported(format!("function `{name}`")))?;
        if self.visiting.iter().any(|v| v == name) {
            return Err(SmtError::Unsupported(format!("mutual recursion through `{name}`")));
        }
        self.visiting.push(name.to_string());
        let result = self.define_inner(name, func, &sig);
        self.visiting.pop();
        result.map(|()| sig)
    }

    fn define_inner(&mut self, name: &str, func: &Func, sig: &Signature) -> Result<()> {
        let mut callees = Vec::new();
        func.body.iter().for_each(|n| collect_calls(n, &mut callees));
        let recursive = callees.iter().any(|c| c == name);
        for callee in callees.iter().filter(|c| *c != name && self.funcs.contains_key(*c)) {
            self.define(callee)?;
        }

        let params: Vec<(String, Sort)> = func.params.iter().cloned().zip(sig.params.iter().cloned()).collect();
        let env: Env = params.iter().cloned().collect();
        let body = self.program(&func.body, &env)?.expect(sig.ret.clone(), name)?;
        self.defs.push(Define {
            name: name.to_string(),
            params,
            ret: sig.ret.clone(),
            body: body.term,
            recursive,
        });
        self.done.insert(name.to_string());
        Ok(())
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
        let typed = self.expr_inner(n, env)?;
        if !self.seen.contains(&typed.sort) {
            self.seen.push(typed.sort.clone());
        }
        Ok(typed)
    }

    fn expr_inner(&mut self, n: &Node, env: &Env) -> Result<Typed> {
        match &n.expr {
            AstExpr::Literal(AstLiteral::Number(v)) if v.is_int() => Ok(Typed::int(v.to_int())),
            AstExpr::Literal(AstLiteral::Number(_)) => Err(SmtError::Unsupported("non-integer number".to_string())),
            AstExpr::Literal(AstLiteral::Bool(b)) => Ok(Typed::bool(*b)),
            AstExpr::Literal(AstLiteral::String(text)) => Typed::string(text),
            AstExpr::Literal(AstLiteral::None) => Ok(Typed::none()),
            AstExpr::Dict(entries) => self.dict(entries, env),
            AstExpr::Array(items) => Typed::seq(self.exprs(items, env)?),
            AstExpr::Index(target, index) => {
                let target = self.expr(target, env)?;
                match (key_of(index), &target.sort) {
                    (Some(key), Sort::Record(_)) => Typed::field(target, &key),
                    _ => Typed::index(target, self.expr(index, env)?),
                }
            }
            AstExpr::Ident(id) => {
                let name = id.name.to_string();
                if let Some(narrowed) = self.narrowed.get(&name) {
                    return Ok(narrowed.clone());
                }
                let sort = env
                    .get(&name)
                    .ok_or_else(|| SmtError::Invalid(format!("unknown variable `{name}`")))?
                    .clone();
                Ok(Typed::var(&name, sort))
            }
            AstExpr::Paren(x) => self.expr(x, env),
            AstExpr::Block(p) => self.program(p, env),
            AstExpr::And(xs) => Typed::connective("and", self.chain(xs, env, true)?),
            AstExpr::Or(xs) => Typed::connective("or", self.chain(xs, env, false)?),
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
                if let Some(holds) = self.element_predicate(&name, args, env)? {
                    return Ok(holds);
                }
                if let ("set", [rec, key, value]) = (name.as_str(), args.as_slice())
                    && let Some(key) = key_of(key)
                {
                    let (rec, value) = (self.expr(rec, env)?, self.expr(value, env)?);
                    return Typed::with_field(rec, &key, value);
                }
                let args = self.exprs(args, env)?;
                if self.funcs.contains_key(&name) {
                    if !self.done.contains(&name) && !self.visiting.contains(&name) {
                        self.define(&name)?;
                    }
                    return Typed::call(&name, &self.signature(&name)?, args);
                }
                builtin_call(&name, args)
            }
            _ => Err(SmtError::Unsupported("expression".to_string())),
        }
    }

    fn exprs<'n>(&mut self, nodes: impl IntoIterator<Item = &'n Node>, env: &Env) -> Result<Vec<Typed>> {
        nodes.into_iter().map(|n| self.expr(n, env)).collect()
    }

    /// `all(xs, is_number)` and the like: a fact about the array's item sort, decided by it.
    fn element_predicate(&mut self, name: &str, args: &[Node], env: &Env) -> Result<Option<Typed>> {
        let [xs, pred] = args else {
            return Ok(None);
        };
        let (AstExpr::Ident(pred), "all") = (&pred.expr, name) else {
            return Ok(None);
        };
        let item = match pred.name.to_string().as_str() {
            "is_number" => Sort::Int,
            "is_string" => Sort::Str,
            "is_bool" => Sort::Bool,
            _ => return Ok(None),
        };
        match self.expr(xs, env)?.sort {
            Sort::Seq(elem) => Ok(Some(Typed::bool(*elem == item))),
            other => Err(SmtError::Mismatch {
                context: "all".to_string(),
                expected: Sort::Seq(Box::new(item)),
                found: other,
            }),
        }
    }

    /// A dict literal of string or symbol keys.
    fn dict(&mut self, entries: &[Node], env: &Env) -> Result<Typed> {
        let mut fields = Vec::new();
        for entry in entries {
            let AstExpr::Array(pair) = &entry.expr else {
                return Err(SmtError::Unsupported("dict spread".to_string()));
            };
            let [key, value] = pair.as_slice() else {
                return Err(SmtError::Unsupported("dict entry".to_string()));
            };
            let key = key_of(key).ok_or_else(|| SmtError::Unsupported("dict key that is not a literal".to_string()))?;
            fields.push((key, self.expr(value, env)?));
        }
        Ok(Typed::record(fields))
    }

    /// The operands of `&&` (`holds`) or `||` (`!holds`), each translated knowing what the
    /// earlier ones imply.
    fn chain(&mut self, xs: &[Node], env: &Env, holds: bool) -> Result<Vec<Typed>> {
        let mut saved = Vec::new();
        let mut out = Vec::new();
        for x in xs {
            let typed = self.expr(x, env);
            let typed = match typed {
                Ok(t) => t,
                Err(e) => {
                    self.restore(saved);
                    return Err(e);
                }
            };
            out.push(typed);
            saved.extend(self.narrow(&facts(x, holds), env));
        }
        self.restore(saved);
        Ok(out)
    }

    /// Reads each optional variable in `names` as its value until [`Self::restore`].
    fn narrow(&mut self, names: &[String], env: &Env) -> Vec<(String, Option<Typed>)> {
        let mut saved = Vec::new();
        for name in names {
            let Some(sort @ Sort::Opt(_)) = env.get(name) else {
                continue;
            };
            if self.narrowed.contains_key(name) {
                continue;
            }
            if let Ok(value) = Typed::unwrap(Typed::var(name, sort.clone())) {
                saved.push((name.clone(), self.narrowed.insert(name.clone(), value)));
            }
        }
        saved
    }

    fn restore(&mut self, saved: Vec<(String, Option<Typed>)>) {
        for (name, previous) in saved.into_iter().rev() {
            match previous {
                Some(value) => self.narrowed.insert(name, value),
                None => self.narrowed.remove(&name),
            };
        }
    }

    /// `if`/`elif`/`else` as nested `ite`; `negate` flips the first condition (`unless`). A branch
    /// is translated knowing what its condition, and the failure of the earlier ones, imply.
    fn branches(&mut self, branches: &[(Option<Node>, Node)], env: &Env, negate: bool) -> Result<Typed> {
        let mut arms = Vec::new();
        let mut otherwise = None;
        let mut earlier_failed = Vec::new();
        let mut result = Ok(());
        for (i, (cond, body)) in branches.iter().enumerate() {
            let step = (|| -> Result<()> {
                let failed = self.narrow(&earlier_failed, env);
                let outcome = match cond {
                    Some(c) => {
                        let holds = !(negate && i == 0);
                        let typed = self.expr(c, env)?.expect(Sort::Bool, "if condition")?;
                        let inside = self.narrow(&facts(c, holds), env);
                        let body = self.expr(body, env);
                        self.restore(inside);
                        arms.push((if holds { typed } else { Typed::not(typed) }, body?));
                        earlier_failed.extend(facts(c, !holds));
                        Ok(())
                    }
                    None => self.expr(body, env).map(|b| otherwise = Some(b)),
                };
                self.restore(failed);
                outcome
            })();
            if step.is_err() {
                result = step;
                break;
            }
        }
        result?;
        let otherwise = otherwise.ok_or_else(|| SmtError::Unsupported("`if` without `else`".to_string()))?;
        arms.into_iter()
            .rev()
            .try_fold(otherwise, |acc, (cond, body)| Typed::ite(cond, body, acc))
    }
}

/// The variables known not to be `None` when `cond` evaluates to `holds`.
fn facts(cond: &Node, holds: bool) -> Vec<String> {
    fn ident(node: &Node) -> Option<String> {
        match &node.expr {
            AstExpr::Ident(id) => Some(id.name.to_string()),
            AstExpr::Paren(x) => ident(x),
            _ => None,
        }
    }
    fn is_none_literal(node: &Node) -> bool {
        match &node.expr {
            AstExpr::Literal(AstLiteral::None) => true,
            AstExpr::Paren(x) => is_none_literal(x),
            _ => false,
        }
    }
    match &cond.expr {
        AstExpr::Paren(x) => facts(x, holds),
        AstExpr::UnaryOp(AstUnaryOp::Not, x) => facts(x, !holds),
        AstExpr::Call(id, args) if !holds && id.name.to_string() == "is_none" => {
            args.first().and_then(ident).into_iter().collect()
        }
        AstExpr::BinaryOp(op @ (B::Eq | B::Ne), a, b) if holds == matches!(op, B::Ne) => {
            if is_none_literal(b) {
                ident(a).into_iter().collect()
            } else if is_none_literal(a) {
                ident(b).into_iter().collect()
            } else {
                Vec::new()
            }
        }
        AstExpr::And(xs) if holds => xs.iter().flat_map(|x| facts(x, true)).collect(),
        AstExpr::Or(xs) if !holds => xs.iter().flat_map(|x| facts(x, false)).collect(),
        _ => Vec::new(),
    }
}

/// A string or symbol literal used as a dict key.
fn key_of(node: &Node) -> Option<String> {
    match &node.expr {
        AstExpr::Literal(AstLiteral::String(s)) => Some(s.clone()),
        AstExpr::Literal(AstLiteral::Symbol(s)) => Some(s.to_string()),
        _ => None,
    }
}

/// Builtins with a direct integer translation.
fn builtin_call(name: &str, args: Vec<Typed>) -> Result<Typed> {
    let int = |t: Typed| t.expect(Sort::Int, name);
    let ite = |cmp: &'static str, a: Typed, b: Typed| Typed::ite(Typed::compare(cmp, a.clone(), b.clone())?, a, b);
    let mut args = args.into_iter();
    match (name, args.next(), args.next(), args.next()) {
        ("len", Some(x), None, None) => Typed::len(x),
        ("is_none", Some(x), None, None) => Ok(Typed::is_none(x)),
        // Type predicates are decided by the sort the value has.
        ("is_number", Some(x), None, None) => Ok(Typed::kind_test(x, |s| *s == Sort::Int)),
        ("is_string", Some(x), None, None) => Ok(Typed::kind_test(x, |s| *s == Sort::Str)),
        ("is_bool", Some(x), None, None) => Ok(Typed::kind_test(x, |s| *s == Sort::Bool)),
        ("is_array", Some(x), None, None) => Ok(Typed::kind_test(x, |s| matches!(s, Sort::Seq(_)))),
        ("is_dict", Some(x), None, None) => Ok(Typed::kind_test(x, |s| matches!(s, Sort::Record(_)))),
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
