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
use mq_lang::{AstExpr, AstLiteral};

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

/// A declared sort, possibly built from facts about the fields of a dict.
enum Shape {
    Leaf(Sort),
    Fields(Vec<(String, Shape)>),
}

impl Shape {
    fn into_sort(self) -> Sort {
        match self {
            Shape::Leaf(sort) => sort,
            Shape::Fields(fields) => Sort::record(fields.into_iter().map(|(k, v)| (k, v.into_sort())).collect()),
        }
    }

    fn insert(&mut self, path: &[String], sort: Sort, name: &str) -> Result<()> {
        let Shape::Fields(fields) = self else {
            return Err(SmtError::Invalid(format!(
                "`{name}` is declared both a value and a dict"
            )));
        };
        let Some((key, rest)) = path.split_first() else {
            return Ok(());
        };
        let existing = fields.iter().position(|(k, _)| k == key);
        if rest.is_empty() {
            return match existing {
                None => {
                    fields.push((key.clone(), Shape::Leaf(sort)));
                    Ok(())
                }
                Some(i) => match &fields[i].1 {
                    Shape::Leaf(previous) if *previous == sort => Ok(()),
                    _ => Err(SmtError::Invalid(format!(
                        "`{name}[:{key}]` is declared with two different types"
                    ))),
                },
            };
        }
        let i = existing.unwrap_or_else(|| {
            fields.push((key.clone(), Shape::Fields(Vec::new())));
            fields.len() - 1
        });
        fields[i].1.insert(rest, sort, name)
    }
}

/// The sorts a contract declares, from predicates in its top-level `&&` chain:
///
/// - `is_number(x)`, `is_string(x)`, `is_bool(x)`;
/// - `all(xs, is_number)`: an array of numbers (or strings, bools);
/// - `is_number(x) || is_none(x)`: a number or `None`;
/// - the same on a key, `is_number(p[:x])`: `p` is a dict with that key.
pub(super) fn declared(contracts: &[Node]) -> Result<HashMap<String, Sort>> {
    fn element(pred: &str) -> Option<Sort> {
        match pred {
            "is_number" => Some(Sort::Int),
            "is_string" => Some(Sort::Str),
            "is_bool" => Some(Sort::Bool),
            _ => None,
        }
    }
    /// The root variable and keys of `x`, `x[:a]`, `x[:a][:b]`.
    fn path(node: &Node) -> Option<(String, Vec<String>)> {
        match &node.expr {
            AstExpr::Ident(id) => Some((id.name.to_string(), Vec::new())),
            AstExpr::Paren(inner) => path(inner),
            AstExpr::Index(target, key) => {
                let (root, mut keys) = path(target)?;
                keys.push(match &key.expr {
                    AstExpr::Literal(AstLiteral::String(s)) => s.clone(),
                    AstExpr::Literal(AstLiteral::Symbol(s)) => s.to_string(),
                    _ => return None,
                });
                Some((root, keys))
            }
            _ => None,
        }
    }
    /// `is_number(p)` or `all(p, is_number)` as a path and its sort.
    fn predicate(node: &Node) -> Option<((String, Vec<String>), Sort)> {
        match &node.expr {
            AstExpr::Paren(inner) => predicate(inner),
            AstExpr::Call(id, args) => match (id.name.to_string().as_str(), args.as_slice()) {
                (pred, [x]) => Some((path(x)?, element(pred)?)),
                ("all", [xs, p]) => {
                    let AstExpr::Ident(p) = &p.expr else {
                        return None;
                    };
                    Some((path(xs)?, Sort::Seq(Box::new(element(&p.name.to_string())?))))
                }
                _ => None,
            },
            _ => None,
        }
    }
    fn is_none(node: &Node) -> Option<(String, Vec<String>)> {
        match &node.expr {
            AstExpr::Paren(inner) => is_none(inner),
            AstExpr::Call(id, args) if id.name.to_string() == "is_none" => path(args.first()?),
            _ => None,
        }
    }
    fn walk(node: &Node, out: &mut Vec<((String, Vec<String>), Sort)>) {
        match &node.expr {
            AstExpr::And(xs) => xs.iter().for_each(|x| walk(x, out)),
            AstExpr::Paren(x) => walk(x, out),
            AstExpr::Or(xs) => {
                if let [a, b] = xs.as_slice() {
                    for (none, other) in [(a, b), (b, a)] {
                        if let (Some(none), Some((target, sort))) = (is_none(none), predicate(other))
                            && none == target
                        {
                            out.push((target, Sort::Opt(Box::new(sort))));
                        }
                    }
                }
            }
            _ => out.extend(predicate(node)),
        }
    }

    let mut found = Vec::new();
    contracts.iter().for_each(|c| walk(c, &mut found));
    let mut roots: HashMap<String, Shape> = HashMap::new();
    for ((root, keys), sort) in found {
        if keys.is_empty() {
            match roots.get(&root) {
                None => {
                    roots.insert(root, Shape::Leaf(sort));
                }
                Some(Shape::Leaf(previous)) if *previous == sort => {}
                Some(Shape::Leaf(previous)) => {
                    return Err(SmtError::Invalid(format!(
                        "`{root}` is declared both {previous} and {sort}"
                    )));
                }
                Some(Shape::Fields(_)) => {
                    return Err(SmtError::Invalid(format!(
                        "`{root}` is declared both a value and a dict"
                    )));
                }
            }
        } else {
            roots
                .entry(root.clone())
                .or_insert_with(|| Shape::Fields(Vec::new()))
                .insert(&keys, sort, &root)?;
        }
    }
    Ok(roots.into_iter().map(|(k, v)| (k, v.into_sort())).collect())
}
