//! Splits a program around its `nodes` call (per-input part vs. aggregate part).
use super::{Error, compiler};
use crate::ast::Program;
use crate::ast::node::{Expr, Node};
use crate::runtime::runtime_value::RuntimeValue;
use crate::{Ident, ModuleLoader, ModuleResolver, Shared, TokenArena};

pub(super) type ProgramSlice<'a> = &'a [Shared<Node>];

pub(super) fn split_at_nodes(program: &Program) -> Option<(ProgramSlice<'_>, ProgramSlice<'_>)> {
    let index = program.iter().position(|node| node.is_nodes())?;
    Some(program.split_at(index))
}

/// Whether `node` only declares names, leaving the piped value untouched.
pub(super) fn is_declaration(node: &Node) -> bool {
    matches!(
        &node.expr,
        Expr::Def(..) | Expr::Include(..) | Expr::Import(..) | Expr::Module(..)
    )
}

pub(super) fn program_after_nodes(before: ProgramSlice<'_>, after: ProgramSlice<'_>) -> Program {
    before
        .iter()
        .filter(|node| is_declaration(node))
        .cloned()
        .chain(after.iter().cloned())
        .collect()
}

/// Top-level bindings declared before a `nodes` split (the last input's value wins),
/// including every name bound by a destructuring pattern and `as` bindings.
pub(super) fn let_names_before_nodes(before: ProgramSlice<'_>) -> Vec<crate::Ident> {
    let mut names = Vec::new();
    for node in before {
        match &node.expr {
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
        let (mut declared, immutable) = match &node.expr {
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
        match &node.expr {
            Expr::Let(pattern, _) | Expr::Var(pattern, _) => compiler::collect_pattern_idents(pattern, &mut names),
            Expr::Def(ident, ..) => names.push(ident.name),
            Expr::As(ident, _) => names.push(ident.name),
            _ => {}
        }
    }
    names
}

/// Compiles the per-input part and, for a `nodes` query, the aggregate part, without debugger
/// instrumentation. Returns the programs and the `let` names carried from the per-input part.
pub(super) fn compile_split_parts<R: ModuleResolver>(
    program: &Program,
    token_arena: TokenArena,
    module_loader: ModuleLoader<R>,
    global_bindings: &[(Ident, RuntimeValue)],
    preresolved_module_vars: &compiler::ResolvedModuleVars,
) -> Result<(compiler::CompiledProgram, Option<compiler::CompiledProgram>, Vec<Ident>), Error> {
    let mut global_names: Vec<Ident> = global_bindings.iter().map(|(name, _)| *name).collect();
    global_names.sort_unstable();
    let Some((before, after)) = split_at_nodes(program) else {
        let program = compiler::compile_uninstrumented_program_for_engine(
            program,
            token_arena,
            module_loader,
            &[],
            &[],
            &global_names,
            preresolved_module_vars,
        )?;
        return Ok((program, None, Vec::new()));
    };
    let let_names = let_names_before_nodes(before);
    let immutable_let_names = immutable_let_names_before_nodes(before);
    // `after` re-declares these, so skip them per input.
    let per_input = if before.iter().all(|node| is_declaration(node)) {
        Program::new()
    } else {
        before.to_vec()
    };
    let program_before = compiler::compile_uninstrumented_program_for_engine(
        &per_input,
        Shared::clone(&token_arena),
        module_loader.clone(),
        &[],
        &[],
        &global_names,
        preresolved_module_vars,
    )?;
    let program_after = compiler::compile_uninstrumented_program_for_engine(
        &program_after_nodes(before, after),
        token_arena,
        module_loader,
        &let_names,
        &immutable_let_names,
        &global_names,
        preresolved_module_vars,
    )?;
    Ok((program_before, Some(program_after), let_names))
}
