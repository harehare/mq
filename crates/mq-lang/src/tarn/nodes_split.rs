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

/// Top-level `let`/`var` names declared before a `nodes` split (last input's value wins),
/// including every name bound by a destructuring pattern.
pub(super) fn let_names_before_nodes(before: ProgramSlice<'_>) -> Vec<crate::Ident> {
    let mut names = Vec::new();
    for node in before {
        if let Expr::Let(pattern, _) | Expr::Var(pattern, _) = &*node.expr {
            compiler::collect_pattern_idents(pattern, &mut names);
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
            _ => {}
        }
    }
    names
}
