//! The sorts of each function, taken from `mq-check`'s inferred types.
//!
//! Only types the checker settled are used. A parameter or result it left open (`'a`), or one
//! that is a union, a dict or anything else without an SMT-LIB counterpart, makes the function
//! `skipped` instead of being guessed.

use std::collections::HashMap;

use mq_check::TypeChecker;
use mq_check::types::Type;
use mq_hir::{Hir, SymbolKind};

use super::term::{Result, Signature, SmtError, Sort};
use super::translate::Func;

/// The signature of every top-level `def` in `funcs`, or why it has none.
pub(super) fn signatures(content: &str, funcs: &HashMap<String, Func>) -> HashMap<String, Result<Signature>> {
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
        let sig = match checker.symbol_types().get(&id) {
            Some(scheme) => signature(&scheme.ty, func),
            None => Err(SmtError::Invalid(format!("mq-check has no type for `{name}`"))),
        };
        out.insert(name, sig);
    }
    out
}

fn signature(ty: &Type, func: &Func) -> Result<Signature> {
    let Type::Function(args, ret) = ty else {
        return Err(SmtError::Invalid(format!("mq-check typed the function as `{ty}`")));
    };
    if args.len() != func.params.len() {
        return Err(SmtError::Invalid(format!(
            "mq-check sees {} parameters, expected {}",
            args.len(),
            func.params.len()
        )));
    }
    let params = args
        .iter()
        .zip(&func.params)
        .map(|(ty, name)| sort(ty).map_err(|why| open(&format!("parameter `{name}`"), ty, &why)))
        .collect::<Result<Vec<_>>>()?;
    let ret = sort(ret).map_err(|why| open("the result", ret, &why))?;
    Ok(Signature { params, ret })
}

fn open(what: &str, ty: &Type, why: &str) -> SmtError {
    SmtError::Unsupported(format!("type of {what}: mq-check inferred `{ty}`, {why}"))
}

/// The SMT-LIB sort of `ty`, or why it has none. mq numbers are modeled as integers.
fn sort(ty: &Type) -> std::result::Result<Sort, String> {
    match ty {
        Type::Int | Type::Number => Ok(Sort::Int),
        Type::Bool => Ok(Sort::Bool),
        Type::String => Ok(Sort::Str),
        Type::Array(elem) => sort(elem).map(|s| Sort::Seq(Box::new(s))),
        Type::Float => Err("floating point is not modeled".to_string()),
        Type::Var(_) => Err("which is not settled by how the function uses it".to_string()),
        _ => Err("which has no SMT-LIB sort".to_string()),
    }
}
