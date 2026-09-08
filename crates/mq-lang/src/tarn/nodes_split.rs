//! Splits a program around its `nodes` call (per-input part vs. aggregate part).
use super::compiler;
use crate::Shared;
use crate::ast::Program;
use crate::ast::node::{Expr, Node};

pub(super) type ProgramSlice<'a> = &'a [Shared<Node>];

pub(super) fn split_at_nodes(program: &Program) -> Option<(ProgramSlice<'_>, ProgramSlice<'_>)> {
    let index = program.iter().position(|node| node.is_nodes())?;
    Some(program.split_at(index))
}

pub(super) fn program_after_nodes(before: ProgramSlice<'_>, after: ProgramSlice<'_>) -> Program {
    before
        .iter()
        .filter(|node| {
            matches!(
                *node.expr,
                Expr::Def(..) | Expr::Include(..) | Expr::Import(..) | Expr::Module(..)
            )
        })
        .cloned()
        .chain(after.iter().cloned())
        .collect()
}

/// Top-level bindings declared before a `nodes` split (the last input's value wins),
/// including every name bound by a destructuring pattern and `as` bindings.
pub(super) fn let_names_before_nodes(before: ProgramSlice<'_>) -> Vec<crate::Ident> {
    let mut names = Vec::new();
    for node in before {
        match &*node.expr {
            Expr::Let(pattern, _) | Expr::Var(pattern, _) => compiler::collect_pattern_idents(pattern, &mut names),
            Expr::As(ident, _) => names.push(ident.name),
            _ => {}
        }
    }
    names
}

/// Top-level immutable bindings declared before a `nodes` split.
pub(super) fn immutable_let_names_before_nodes(before: ProgramSlice<'_>) -> Vec<crate::Ident> {
    let mut names = Vec::new();
    let mut shadowed = std::collections::HashSet::new();
    for node in before.iter().rev() {
        let (mut declared, immutable) = match &*node.expr {
            Expr::Let(pattern, _) => {
                let mut declared = Vec::new();
                compiler::collect_pattern_idents(pattern, &mut declared);
                (declared, true)
            }
            Expr::Var(pattern, _) => {
                let mut declared = Vec::new();
                compiler::collect_pattern_idents(pattern, &mut declared);
                (declared, false)
            }
            Expr::As(ident, _) => (vec![ident.name], true),
            _ => continue,
        };
        for name in declared.drain(..) {
            if shadowed.insert(name) && immutable {
                names.push(name);
            }
        }
    }
    names
}

/// Like [`let_names_before_nodes`], but for the whole program and including `def`.
pub(super) fn top_level_binding_names(program: &Program) -> Vec<crate::Ident> {
    let mut names = Vec::new();
    for node in program {
        match &*node.expr {
            Expr::Let(pattern, _) | Expr::Var(pattern, _) => compiler::collect_pattern_idents(pattern, &mut names),
            Expr::Def(ident, ..) => names.push(ident.name),
            Expr::As(ident, _) => names.push(ident.name),
            _ => {}
        }
    }
    names
}
