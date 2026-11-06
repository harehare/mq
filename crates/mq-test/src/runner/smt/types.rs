//! The sorts of each function: `mq-check`'s inferred types, completed by type predicates in the
//! contracts.
//!
//! Only settled types are used. A parameter or result `mq-check` left open (`'a`), or one that is
//! a union, a dict or anything else without an SMT-LIB counterpart, is taken from a predicate
//! asserted at the top of a `@requires`/`@ensures` (`is_number(x)`, `is_string(x)`, `is_bool(x)`
//! or `all(xs, is_number)`); without one the function is `skipped` instead of being guessed.

use std::collections::HashMap;

use mq_check::TypeChecker;
use mq_check::types::Type;
use mq_hir::{Hir, SymbolKind};
use mq_lang::AstExpr;

use super::term::{Result, Signature, SmtError, Sort};
use super::translate::{Func, Node};

/// What `mq-check` settled for one function; each part is a sort or why there is none.
#[derive(Clone)]
pub(super) struct Partial {
    pub(super) params: Vec<Result<Sort>>,
    pub(super) ret: Result<Sort>,
}

impl Partial {
    /// A signature with every sort known.
    #[cfg(test)]
    pub(super) fn known(sig: &Signature) -> Self {
        Self {
            params: sig.params.iter().cloned().map(Ok).collect(),
            ret: Ok(sig.ret.clone()),
        }
    }

    /// The signature, taking sorts `mq-check` left open from `declared` (by parameter name, and
    /// `result`). A declaration that disagrees with a settled type is an error.
    pub(super) fn complete(&self, func: &Func, declared: &HashMap<String, Sort>) -> Result<Signature> {
        let pick = |inferred: &Result<Sort>, name: &str, what: &str, hint: &str| -> Result<Sort> {
            match (inferred, declared.get(name)) {
                (Ok(sort), Some(declared)) if sort != declared => Err(SmtError::Invalid(format!(
                    "{what} is declared {declared} but mq-check inferred {sort}"
                ))),
                (Ok(sort), _) => Ok(sort.clone()),
                (Err(_), Some(declared)) => Ok(declared.clone()),
                (Err(e), None) => Err(SmtError::Invalid(format!("{what}: {e}; declare it with `{hint}`"))),
            }
        };
        let params = func
            .params
            .iter()
            .zip(&self.params)
            .map(|(name, inferred)| {
                pick(
                    inferred,
                    name,
                    &format!("parameter `{name}`"),
                    &format!("# @requires(is_number({name}))"),
                )
            })
            .collect::<Result<Vec<_>>>()?;
        let ret = pick(&self.ret, "result", "the result", "# @ensures(is_number(result))")?;
        Ok(Signature { params, ret })
    }
}

/// The types `mq-check` inferred for every top-level `def` in `funcs`.
pub(super) fn inferred(content: &str, funcs: &HashMap<String, Func>) -> HashMap<String, Partial> {
    let mut hir = Hir::default();
    let (_, scope, _) = hir.add_code(None, content);
    let mut checker = TypeChecker::new();
    // A type error elsewhere in the file does not stop the other functions from being typed.
    let _ = checker.check(&hir);

    let mut out = HashMap::new();
    for (id, symbol) in hir.symbols() {
        if !matches!(symbol.kind, SymbolKind::Function(_)) || symbol.scope != scope || hir.is_builtin_symbol(symbol) {
            continue;
        }
        let Some(name) = symbol.value.as_ref().map(|n| n.to_string()) else {
            continue;
        };
        let Some(func) = funcs.get(&name) else {
            continue;
        };
        let partial = match checker.symbol_types().get(&id).map(|scheme| &scheme.ty) {
            Some(Type::Function(args, ret)) if args.len() == func.params.len() => Partial {
                params: args.iter().map(open_or_sort).collect(),
                ret: open_or_sort(ret),
            },
            _ => {
                let why = || Err(SmtError::Invalid(format!("mq-check has no function type for `{name}`")));
                Partial {
                    params: func.params.iter().map(|_| why()).collect(),
                    ret: why(),
                }
            }
        };
        out.insert(name, partial);
    }
    out
}

fn open_or_sort(ty: &Type) -> Result<Sort> {
    sort(ty).map_err(|why| SmtError::Invalid(format!("mq-check inferred `{ty}`, {why}")))
}

/// The SMT-LIB sort of `ty`, or why it has none. mq numbers are modeled as integers.
fn sort(ty: &Type) -> std::result::Result<Sort, String> {
    match ty {
        Type::Int | Type::Number => Ok(Sort::Int),
        Type::Bool => Ok(Sort::Bool),
        Type::String => Ok(Sort::Str),
        Type::Array(elem) => sort(elem).map(|s| Sort::Seq(Box::new(s))),
        Type::Float => Err("floating point is not modeled".to_string()),
        Type::Var(_) => Err("which the function's own use does not settle".to_string()),
        _ => Err("which has no SMT-LIB sort".to_string()),
    }
}

/// The sorts a contract declares, from predicates in its top-level `&&` chain:
/// `is_number(x)`, `is_string(x)`, `is_bool(x)` and `all(xs, is_number)` (an array of numbers).
pub(super) fn declared(contracts: &[Node]) -> Result<HashMap<String, Sort>> {
    fn element(pred: &str) -> Option<Sort> {
        match pred {
            "is_number" => Some(Sort::Int),
            "is_string" => Some(Sort::Str),
            "is_bool" => Some(Sort::Bool),
            _ => None,
        }
    }
    fn ident(node: &Node) -> Option<String> {
        match &node.expr {
            AstExpr::Ident(id) => Some(id.name.to_string()),
            AstExpr::Paren(inner) => ident(inner),
            _ => None,
        }
    }
    fn walk(node: &Node, out: &mut Vec<(String, Sort)>) {
        match &node.expr {
            AstExpr::And(xs) => xs.iter().for_each(|x| walk(x, out)),
            AstExpr::Paren(x) => walk(x, out),
            AstExpr::Call(id, args) => match (id.name.to_string().as_str(), args.as_slice()) {
                (pred, [x]) => {
                    if let (Some(sort), Some(name)) = (element(pred), ident(x)) {
                        out.push((name, sort));
                    }
                }
                ("all", [xs, p]) => {
                    if let (Some(name), Some(sort)) = (ident(xs), ident(p).as_deref().and_then(element)) {
                        out.push((name, Sort::Seq(Box::new(sort))));
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }

    let mut found = Vec::new();
    contracts.iter().for_each(|c| walk(c, &mut found));
    let mut out: HashMap<String, Sort> = HashMap::new();
    for (name, sort) in found {
        if let Some(previous) = out.insert(name.clone(), sort.clone())
            && previous != sort
        {
            return Err(SmtError::Invalid(format!(
                "`{name}` is declared both {previous} and {sort}"
            )));
        }
    }
    Ok(out)
}
