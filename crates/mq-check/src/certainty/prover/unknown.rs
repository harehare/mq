//! Operations whose outcome does not depend on an operand of unknown type.
//!
//! A parameter, a loop variable or the result of a call the checker cannot follow has no known
//! type, but an operation on it can still be settled: `x - "a"` fails whatever `x` is, and
//! `to_string(x)` is a string whatever `x` is. The operand is tried as a value of each kind of
//! type the overloads tell apart.

use std::collections::BTreeMap;

use mq_hir::{Hir, SymbolId, SymbolKind};
use rustc_hash::{FxHashMap, FxHashSet};
use smol_str::SmolStr;

use super::{Prover, is_audited_builtin, is_expression};
use crate::builtin::PARTIAL;
use crate::constraint::{ChildrenIndex, build_children_index};
use crate::inline::{assigned_variables, doomed_operations};
use crate::probe::{combinations, representatives};
use crate::types::Type;

/// The most operands of unknown type that one operation is tried with.
const MAX_UNKNOWN: usize = 3;

/// The most arguments for which the overloads are tried against every kind of type.
const MAX_ARITY: usize = 3;

/// An operand of an operation.
#[derive(Clone)]
enum Operand {
    Known(Type),
    /// An array or dict literal: its kind is fixed, its elements are not
    Container(Vec<Type>),
    Unknown,
}

/// What every overload of a builtin of some arity gives back.
#[derive(Clone, PartialEq)]
enum Returns {
    /// The same type, whatever the arguments
    Always(Type),
    /// No overload takes that many arguments
    Nothing,
    Varies,
}

/// Memoised results of the analysis.
#[derive(Default)]
pub(super) struct UnknownOperands {
    returns: FxHashMap<(SmolStr, usize), Returns>,
    uniform: FxHashMap<SymbolId, Option<Type>>,
    /// For the inline analysis: the children of the symbols, and the variables that are assigned
    inline: Option<(ChildrenIndex, FxHashSet<SymbolId>)>,
    /// The operations that make each function fail, when no type of its parameters lets it run
    doomed: FxHashMap<SymbolId, Option<FxHashSet<SymbolId>>>,
}

impl Prover<'_> {
    /// The type an expression has for certain, even where the checker left it open.
    pub(super) fn exact_type(&mut self, hir: &Hir, id: SymbolId) -> Option<Type> {
        if !self.is_exact(hir, id) {
            return None;
        }
        if let Some(ty) = self.settled_type(id) {
            return Some(ty);
        }
        match hir.symbol(id).map(|s| &s.kind) {
            Some(SymbolKind::Call | SymbolKind::BinaryOp | SymbolKind::UnaryOp) => self.uniform_type(hir, id),
            Some(SymbolKind::Ref) => {
                let def = hir.resolve_reference_symbol(id)?;
                if hir.symbol(def)?.kind != SymbolKind::Variable {
                    return None;
                }
                let init = self.children.get(&def).and_then(|c| c.last()).copied()?;
                self.exact_type(hir, init)
            }
            _ => None,
        }
    }

    fn settled_type(&self, id: SymbolId) -> Option<Type> {
        self.ctx
            .get_symbol_type(id)
            .map(|ty| self.ctx.resolve_type(ty))
            .filter(|ty| !ty.has_pending_var() && !ty.is_dynamic())
    }

    /// The type of the result of a builtin applied to anything, when every overload gives the same.
    pub(super) fn uniform_type(&mut self, hir: &Hir, id: SymbolId) -> Option<Type> {
        if let Some(known) = self.unknown.uniform.get(&id) {
            return known.clone();
        }
        let found = self.find_uniform_type(hir, id);
        self.unknown.uniform.insert(id, found.clone());
        found
    }

    fn find_uniform_type(&mut self, hir: &Hir, id: SymbolId) -> Option<Type> {
        let symbol = hir.symbol(id)?;
        let name = symbol.value.as_deref()?;
        let may_be_piped = match symbol.kind {
            SymbolKind::BinaryOp | SymbolKind::UnaryOp => false,
            SymbolKind::Call if is_audited_builtin(hir, id, symbol) && hir.bracket_group_count(id) == 0 => true,
            _ => return None,
        };
        let given = self.argument_count(hir, id)?;
        let mut result: Option<Type> = None;
        for arity in std::iter::once(given).chain(may_be_piped.then_some(given + 1)) {
            match self.returns(name, arity) {
                Returns::Always(ty) if result.as_ref().is_none_or(|seen| *seen == ty) => result = Some(ty),
                Returns::Nothing => {}
                _ => return None,
            }
        }
        result
    }

    /// What the builtin `name` gives back when it is given `arity` arguments.
    fn returns(&mut self, name: &str, arity: usize) -> Returns {
        let key = (SmolStr::new(name), arity);
        if let Some(known) = self.unknown.returns.get(&key) {
            return known.clone();
        }
        let found = self.find_returns(name, arity);
        self.unknown.returns.insert(key, found.clone());
        found
    }

    fn find_returns(&self, name: &str, arity: usize) -> Returns {
        let Some(overloads) = self.ctx.get_builtin_overloads(name) else {
            return Returns::Varies;
        };
        let mut common: Option<&Type> = None;
        for overload in overloads {
            let Type::Function(params, ret) = overload else {
                continue;
            };
            if params.len() != arity {
                continue;
            }
            let closed = !ret.has_pending_var() && !ret.is_dynamic() && !matches!(**ret, Type::Union(_) | Type::Never);
            if !closed || common.is_some_and(|seen| seen != &**ret) {
                return Returns::Varies;
            }
            common = Some(ret);
        }
        let Some(common) = common else {
            return Returns::Nothing;
        };
        // The fallbacks and refinements answer for particular arguments
        if arity > MAX_ARITY {
            return Returns::Varies;
        }
        let types = representatives();
        let choices = vec![types.as_slice(); arity];
        for args in combinations(&choices) {
            if let Some(Type::Function(_, ret)) = self.ctx.peek_signature(name, &args)
                && *ret != *common
            {
                return Returns::Varies;
            }
        }
        Returns::Always(common.clone())
    }

    /// The number of arguments written in an application, or `None` when one is not a value.
    fn argument_count(&self, hir: &Hir, id: SymbolId) -> Option<usize> {
        let mut count = 0;
        for &child in self.children.get(&id).map_or(&[][..], Vec::as_slice) {
            match hir.symbol(child).map(|s| &s.kind)? {
                SymbolKind::Function(_) => count += 1,
                kind if is_expression(kind) => count += 1,
                _ => return None,
            }
        }
        Some(count)
    }

    /// Whether the operation named `name` at `id` fails whatever its operands of unknown type are.
    pub(super) fn rejects_whatever(&mut self, hir: &Hir, id: SymbolId, name: &str) -> bool {
        let Some(symbol) = hir.symbol(id) else {
            return false;
        };
        let may_be_piped = match symbol.kind {
            SymbolKind::BinaryOp | SymbolKind::UnaryOp => false,
            SymbolKind::Call if is_audited_builtin(hir, id, symbol) && hir.bracket_group_count(id) == 0 => true,
            _ => return false,
        };
        if symbol.value.as_deref() != Some(name) || name == PARTIAL || self.ctx.get_builtin_overloads(name).is_none() {
            return false;
        }

        let children = self.children.get(&id).cloned().unwrap_or_default();
        let mut written = Vec::with_capacity(children.len());
        for child in children {
            let Some(operand) = self.operand(hir, child) else {
                return false;
            };
            written.push(operand);
        }
        let mut lists = vec![written.clone()];
        if may_be_piped {
            // The stage before it, or the value an enclosing application hands on
            let piped = match self.previous.get(&id).copied() {
                Some(before) => self.operand(hir, before).unwrap_or(Operand::Unknown),
                None => Operand::Unknown,
            };
            lists.push(std::iter::once(piped).chain(written).collect());
        }

        let types = representatives();
        lists.iter().all(|operands| {
            let choices: Vec<&[Type]> = operands
                .iter()
                .filter_map(|operand| match operand {
                    Operand::Known(_) => None,
                    Operand::Container(kinds) => Some(kinds.as_slice()),
                    Operand::Unknown => Some(types.as_slice()),
                })
                .collect();
            if choices.iter().filter(|c| c.len() == types.len()).count() > MAX_UNKNOWN {
                return false;
            }
            combinations(&choices).all(|mut tried| {
                let args: Vec<Type> = operands
                    .iter()
                    .map(|operand| match operand {
                        Operand::Known(ty) => ty.clone(),
                        _ => tried.pop().unwrap_or(Type::Dynamic),
                    })
                    .collect();
                self.ctx.peek_signature(name, &args).is_none()
            })
        })
    }

    /// Whether the function that holds the operation fails for every type of its parameters, and
    /// the operation is one of those that fail (`abs(x) | upcase(x)`).
    pub(super) fn is_doomed(&mut self, hir: &Hir, id: SymbolId, name: &str) -> bool {
        let Some(symbol) = hir.symbol(id) else {
            return false;
        };
        if symbol.value.as_deref() != Some(name) {
            return false;
        }
        let mut current = symbol.parent;
        let def_id = loop {
            let Some(parent) = current.and_then(|p| hir.symbol(p).map(|s| (p, s))) else {
                return false;
            };
            if matches!(parent.1.kind, SymbolKind::Function(_)) {
                break parent.0;
            }
            current = parent.1.parent;
        };
        if !self.unknown.doomed.contains_key(&def_id) {
            let (children, assigned) = self.unknown.inline.get_or_insert_with(|| {
                let children = build_children_index(hir);
                let assigned = assigned_variables(hir, &children);
                (children, assigned)
            });
            let blamed = doomed_operations(hir, self.ctx, children, assigned, def_id);
            self.unknown.doomed.insert(def_id, blamed);
        }
        self.unknown.doomed[&def_id]
            .as_ref()
            .is_some_and(|blamed| blamed.contains(&id))
    }

    /// What is known of the type of an argument, or `None` when it is not a value.
    fn operand(&mut self, hir: &Hir, id: SymbolId) -> Option<Operand> {
        let kind = hir.symbol(id)?.kind.clone();
        Some(match kind {
            SymbolKind::Function(params) => {
                Operand::Known(Type::function(vec![Type::Dynamic; params.len()], Type::Dynamic))
            }
            kind if is_expression(&kind) => match self.exact_type(hir, id) {
                Some(ty) => Operand::Known(ty),
                None => self.container_operand(id, &kind),
            },
            _ => return None,
        })
    }

    /// An array or dict literal, whose elements the checker may not know.
    fn container_operand(&self, id: SymbolId, kind: &SymbolKind) -> Operand {
        match kind {
            SymbolKind::Array => {
                let len = self.children.get(&id).map_or(0, Vec::len);
                Operand::Container(vec![Type::array(Type::Dynamic), Type::Tuple(vec![Type::Dynamic; len])])
            }
            SymbolKind::Dict => Operand::Container(vec![
                Type::dict(Type::Dynamic, Type::Dynamic),
                Type::Record(BTreeMap::new(), Box::new(Type::RowEmpty)),
            ]),
            _ => Operand::Unknown,
        }
    }
}
