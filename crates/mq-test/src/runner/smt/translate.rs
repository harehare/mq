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
}

impl<'a> Translator<'a> {
    pub(super) fn new(funcs: &'a HashMap<String, Func>, sigs: &'a HashMap<String, Result<Signature>>) -> Self {
        Self {
            funcs,
            sigs,
            defs: Vec::new(),
            done: HashSet::new(),
            visiting: Vec::new(),
        }
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
                if let Some(holds) = self.element_predicate(&name, args, env)? {
                    return Ok(holds);
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
        // Type predicates are decided by the sort the value has.
        ("is_number", Some(x), None, None) => Ok(Typed::bool(x.sort == Sort::Int)),
        ("is_string", Some(x), None, None) => Ok(Typed::bool(x.sort == Sort::Str)),
        ("is_bool", Some(x), None, None) => Ok(Typed::bool(x.sort == Sort::Bool)),
        ("is_array", Some(x), None, None) => Ok(Typed::bool(matches!(x.sort, Sort::Seq(_)))),
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
