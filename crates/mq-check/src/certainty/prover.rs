//! Decides which expressions have a type that is known for certain.

use mq_hir::{Hir, SymbolId, SymbolKind};
use rustc_hash::{FxHashMap, FxHashSet};

use super::{Certainty, UNAUDITED_BUILTINS};
use crate::constraint::ChildrenIndex;
use crate::infer::{InferenceContext, InlinedError};
use crate::probe::ELEMENT_BUILTINS;

mod unknown;

/// Decides which expressions have a type that is known for certain.
///
/// An expression is exact when it is
///
/// - a literal, or an interpolated string
/// - an array or dict literal of exact elements
/// - an operator on exact operands
/// - a call of an audited builtin, or of a function of the program whose body is exact when its
///   parameters are, with exact arguments and an exact pipe stage before it
/// - an operation that gives the same type whatever its operands are (`len(x)`, `x == 1`)
/// - an `if` whose branches are exact, or a block whose last expression is exact
/// - an exact container that is indexed
/// - a selector on the input document, when its type was declared
/// - a reference to a `let` or `var` whose value is exact and that keeps one type, even when a
///   type test narrows it
/// - a parameter of a function that is called with exact arguments, or that is passed to `map`
///   and the like over an exact collection
///
/// Loops, `match`, `try` and anything else are not.
pub(super) struct Prover<'a> {
    ctx: &'a InferenceContext,
    children: FxHashMap<SymbolId, Vec<SymbolId>>,
    /// The assignments to each variable that is the target of one
    assignments: FxHashMap<SymbolId, Vec<SymbolId>>,
    /// Variables that an element is assigned to (`x[0] = 1`)
    element_assigned: FxHashSet<SymbolId>,
    /// Whether each variable that is assigned again keeps one exact type
    reassigned: FxHashMap<SymbolId, bool>,
    /// The stage of a pipe that feeds each stage, when there is one. Definitions and assignments
    /// pass their input on, so they are skipped.
    previous: FxHashMap<SymbolId, SymbolId>,
    memo: FxHashMap<SymbolId, bool>,
    /// Whether the parameters of the function being proven are taken to be exact, which holds when
    /// it is called with exact arguments
    params_exact: bool,
    /// Whether the body of a function is exact when its parameters are
    bodies: FxHashMap<SymbolId, bool>,
    /// Whether the type of the input document was declared (`--input-type`)
    declared_input: bool,
    unknown: unknown::UnknownOperands,
}

impl<'a> Prover<'a> {
    pub(super) fn new(hir: &Hir, ctx: &'a InferenceContext, declared_input: bool) -> Self {
        let mut children: FxHashMap<SymbolId, Vec<SymbolId>> = FxHashMap::default();
        for (id, symbol) in hir.symbols() {
            if let Some(parent) = symbol.parent {
                children.entry(parent).or_default().push(id);
            }
        }
        for list in children.values_mut() {
            list.sort_by_key(|&id| hir.symbol_insertion_order(id));
        }

        let mut assignments: FxHashMap<SymbolId, Vec<SymbolId>> = FxHashMap::default();
        let mut element_assigned = FxHashSet::default();
        for (id, symbol) in hir.symbols() {
            if symbol.kind != SymbolKind::Assign {
                continue;
            }
            let target = children.get(&id).and_then(|c| c.first()).copied();
            if let Some(def) = target.and_then(|t| hir.resolve_reference_symbol(t)) {
                assignments.entry(def).or_default().push(id);
                if target
                    .and_then(|t| hir.symbol(t))
                    .is_some_and(|t| t.kind != SymbolKind::Ref)
                {
                    element_assigned.insert(def);
                }
            }
        }

        let mut previous = FxHashMap::default();
        let mut link = |stages: &[SymbolId]| {
            let mut last: Option<SymbolId> = None;
            for &stage in stages {
                let Some(symbol) = hir.symbol(stage) else {
                    continue;
                };
                if !is_stage_kind(&symbol.kind) {
                    continue;
                }
                if let Some(before) = last {
                    previous.insert(stage, before);
                }
                if !passes_input_on(&symbol.kind) {
                    last = Some(stage);
                }
            }
        };
        for (parent, list) in &children {
            if hir
                .symbol(*parent)
                .is_some_and(|s| matches!(s.kind, SymbolKind::Block | SymbolKind::Function(_)))
            {
                link(list);
            }
        }
        // The top-level stages of each source
        let mut roots: FxHashMap<Option<mq_hir::SourceId>, Vec<SymbolId>> = FxHashMap::default();
        for (id, symbol) in hir.symbols() {
            if symbol.parent.is_none() && !hir.is_builtin_symbol(symbol) {
                roots.entry(symbol.source.source_id).or_default().push(id);
            }
        }
        for list in roots.values_mut() {
            list.sort_by_key(|&id| hir.symbol_insertion_order(id));
            link(list);
        }

        Self {
            children,
            assignments,
            element_assigned,
            reassigned: FxHashMap::default(),
            ctx,
            previous,
            memo: FxHashMap::default(),
            params_exact: false,
            bodies: FxHashMap::default(),
            declared_input,
            unknown: unknown::UnknownOperands::default(),
        }
    }

    /// `operand_mismatch` is true for an error that is reported at an operand and is about what the
    /// operation around it expects, so the operation counts as well. `no_overload` names the
    /// operation of an error that found no overload for its operands: it is definite as well when
    /// the operation fails whatever its operands of unknown type are.
    ///
    /// An `inlined` error was found by running the body of the function on the arguments of a call,
    /// so it is definite when the arguments are exact, whatever the rest of the body is. One found
    /// in a lambda run on elements also needs the arguments to have some.
    pub(super) fn classify_at(
        &mut self,
        hir: &Hir,
        at_range: &[SymbolId],
        operand_mismatch: bool,
        no_overload: Option<&str>,
        inlined: Option<InlinedError>,
    ) -> Certainty {
        if at_range.is_empty() {
            return Certainty::Uncertain;
        }

        for &root in at_range {
            // Exact operands do not tell whether a collection has elements
            let through_elements = inlined == Some(InlinedError::ElementCallback);
            if !through_elements && self.operands_are_certain(hir, root, operand_mismatch) {
                continue;
            }
            if inlined.is_some() && self.arguments_are_exact(hir, root, through_elements) {
                continue;
            }
            if no_overload.is_some_and(|name| self.rejects_whatever(hir, root, name) || self.is_doomed(hir, root, name))
            {
                continue;
            }
            return Certainty::Uncertain;
        }
        Certainty::Definite
    }

    /// Whether the call of a function of the program is given exact arguments. With `has_elements`,
    /// an array among them must have an element.
    fn arguments_are_exact(&mut self, hir: &Hir, root: SymbolId, has_elements: bool) -> bool {
        let Some(symbol) = hir.symbol(root) else {
            return false;
        };
        symbol.kind == SymbolKind::Call
            && hir.bracket_group_count(root) == 0
            && hir
                .resolve_reference_symbol(root)
                .and_then(|def| hir.symbol(def))
                .is_some_and(|def| matches!(def.kind, SymbolKind::Function(_)) && !hir.is_builtin_symbol(def))
            && self.input_is_exact(hir, root)
            && (!has_elements
                || self
                    .previous
                    .get(&root)
                    .is_none_or(|&before| Self::is_scalar_or_nonempty_array(hir, &self.children, before)))
            && self
                .children
                .get(&root)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter(|&c| hir.symbol(c).is_some_and(|s| is_expression(&s.kind)))
                .all(|argument| {
                    self.is_exact(hir, argument)
                        && (!has_elements || Self::is_scalar_or_nonempty_array(hir, &self.children, argument))
                })
    }

    /// Whether the expression is a literal that is no array, or an array with an element.
    fn is_scalar_or_nonempty_array(hir: &Hir, children: &ChildrenIndex, id: SymbolId) -> bool {
        match hir.symbol(id).map(|s| &s.kind) {
            Some(SymbolKind::Array) => children
                .get(&id)
                .is_some_and(|c| c.iter().any(|&c| hir.symbol(c).is_some_and(|s| is_expression(&s.kind)))),
            Some(
                SymbolKind::Number
                | SymbolKind::String
                | SymbolKind::Boolean
                | SymbolKind::None
                | SymbolKind::Symbol
                | SymbolKind::Bytes
                | SymbolKind::InterpolatedString
                | SymbolKind::Function(_),
            ) => true,
            _ => false,
        }
    }

    fn operands_are_certain(&mut self, hir: &Hir, root: SymbolId, operand_mismatch: bool) -> bool {
        if self.has_unsettled_operand(hir, root) {
            return false;
        }
        let mut subjects = vec![root];
        if operand_mismatch
            && let Some(parent) = hir.symbol(root).and_then(|s| s.parent)
            && hir.symbol(parent).is_some_and(|s| is_operation(&s.kind))
        {
            subjects.push(parent);
        }
        subjects
            .into_iter()
            .all(|subject| self.operands_are_exact(hir, subject))
    }

    /// Whether the expression is exact, and for an operation, so are its operands.
    fn operands_are_exact(&mut self, hir: &Hir, id: SymbolId) -> bool {
        let Some(symbol) = hir.symbol(id) else {
            return false;
        };
        if is_operation(&symbol.kind) {
            // A call that takes the piped value as an argument has an operand that is not a child.
            if symbol.kind == SymbolKind::Call
                && !(self.callee_is_exact(hir, id, symbol) && self.input_is_exact(hir, id))
            {
                return false;
            }
            let operands = self.children.get(&id).cloned().unwrap_or_default();
            operands
                .into_iter()
                .filter(|&c| hir.symbol(c).is_some_and(|s| is_expression(&s.kind)))
                .all(|operand| self.is_exact(hir, operand))
        } else {
            self.is_exact(hir, id)
        }
    }

    fn is_exact(&mut self, hir: &Hir, id: SymbolId) -> bool {
        if let Some(&known) = self.memo.get(&id) {
            return known;
        }
        // A cycle is not exact
        self.memo.insert(id, false);
        let exact = self.prove(hir, id);
        self.memo.insert(id, exact);
        exact
    }

    fn prove(&mut self, hir: &Hir, id: SymbolId) -> bool {
        let Some(symbol) = hir.symbol(id) else {
            return false;
        };
        let children: Vec<SymbolId> = self.children.get(&id).cloned().unwrap_or_default();
        match &symbol.kind {
            SymbolKind::Number
            | SymbolKind::String
            | SymbolKind::Boolean
            | SymbolKind::None
            | SymbolKind::Symbol
            | SymbolKind::Bytes
            | SymbolKind::InterpolatedString => true,
            SymbolKind::Array | SymbolKind::Dict => children
                .iter()
                .filter(|&&c| hir.symbol(c).is_some_and(|s| is_expression(&s.kind)))
                .all(|&c| self.is_exact(hir, c)),
            SymbolKind::UnaryOp | SymbolKind::BinaryOp => {
                (!children.is_empty() && children.iter().all(|&c| self.is_exact(hir, c)))
                    || self.uniform_type(hir, id).is_some()
            }
            SymbolKind::Call => self.call_is_exact(hir, id, symbol, &children) || self.uniform_type(hir, id).is_some(),
            SymbolKind::If => {
                let branches: Vec<SymbolId> = children.iter().skip(1).copied().collect();
                !branches.is_empty()
                    && branches
                        .iter()
                        .all(|&branch| match hir.symbol(branch).map(|s| &s.kind) {
                            Some(SymbolKind::Elif) | Some(SymbolKind::Else) => self
                                .children
                                .get(&branch)
                                .and_then(|c| c.last())
                                .copied()
                                .is_some_and(|body| self.is_exact(hir, body)),
                            _ => self.is_exact(hir, branch),
                        })
            }
            SymbolKind::Block => children
                .iter()
                .rev()
                .find(|&&c| hir.symbol(c).is_some_and(|s| is_expression(&s.kind)))
                .copied()
                .is_some_and(|last| self.is_exact(hir, last)),
            SymbolKind::Ref => self.ref_is_exact(hir, id),
            // A selector reads the input document, which is known only when its type was declared.
            SymbolKind::Selector(_) => {
                self.declared_input
                    && match symbol.parent.and_then(|p| hir.symbol(p)) {
                        Some(parent) if matches!(parent.kind, SymbolKind::Selector(_)) => {
                            symbol.parent.is_some_and(|p| self.is_exact(hir, p))
                        }
                        Some(parent) if !matches!(parent.kind, SymbolKind::Block | SymbolKind::Function(_)) => false,
                        _ => !self.previous.contains_key(&id),
                    }
            }
            _ => false,
        }
    }

    fn call_is_exact(&mut self, hir: &Hir, id: SymbolId, symbol: &mq_hir::Symbol, children: &[SymbolId]) -> bool {
        self.callee_is_exact(hir, id, symbol)
            && self.input_is_exact(hir, id)
            && children.iter().all(|&c| match hir.symbol(c).map(|s| &s.kind) {
                Some(SymbolKind::Function(_)) => self.lambda_is_exact(hir, c),
                Some(kind) if is_expression(kind) => self.is_exact(hir, c),
                _ => true,
            })
    }

    /// Whether what the call applies has a known type: an audited builtin, a function of the
    /// program with an exact body, or an exact container that is indexed.
    fn callee_is_exact(&mut self, hir: &Hir, id: SymbolId, symbol: &mq_hir::Symbol) -> bool {
        if hir.bracket_group_count(id) > 0 {
            return self.ref_is_exact(hir, id);
        }
        is_audited_builtin(hir, id, symbol) || self.is_exact_user_call(hir, id)
    }

    /// Whether every expression in the body of a function passed as an argument is exact.
    fn lambda_is_exact(&mut self, hir: &Hir, lambda_id: SymbolId) -> bool {
        let stages: Vec<SymbolId> = self
            .children
            .get(&lambda_id)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|&c| hir.symbol(c).is_some_and(|s| is_expression(&s.kind)))
            .collect();
        !stages.is_empty() && stages.iter().all(|&stage| self.stage_is_exact(hir, stage))
    }

    /// Whether the call is of a function defined in the program, given every parameter and whose
    /// body is exact when its parameters are, so that exact arguments give a result of known type.
    fn is_exact_user_call(&mut self, hir: &Hir, id: SymbolId) -> bool {
        let Some(def_id) = hir.resolve_reference_symbol(id) else {
            return false;
        };
        let Some(def) = hir.symbol(def_id) else {
            return false;
        };
        let SymbolKind::Function(params) = &def.kind else {
            return false;
        };
        if hir.is_builtin_symbol(def) || params.iter().any(|p| p.is_variadic || p.has_default) {
            return false;
        }
        let given = self.children.get(&id).map_or(0, |c| {
            c.iter()
                .filter(|&&c| hir.symbol(c).is_some_and(|s| is_expression(&s.kind)))
                .count()
        });
        // A call with fewer arguments takes the piped value as the first one
        (given == params.len() || (given + 1 == params.len() && self.previous.contains_key(&id)))
            && self.body_is_exact(hir, def_id)
    }

    /// Whether every expression in the body of the function is exact if its parameters are.
    fn body_is_exact(&mut self, hir: &Hir, def_id: SymbolId) -> bool {
        if let Some(&known) = self.bodies.get(&def_id) {
            return known;
        }
        // A recursive function is not exact
        self.bodies.insert(def_id, false);
        let outer_memo = std::mem::take(&mut self.memo);
        let outer_params = std::mem::replace(&mut self.params_exact, true);

        let stages: Vec<SymbolId> = self
            .children
            .get(&def_id)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|&c| hir.symbol(c).is_some_and(|s| is_expression(&s.kind)))
            .collect();
        let exact = !stages.is_empty() && stages.iter().all(|&stage| self.stage_is_exact(hir, stage));

        self.memo = outer_memo;
        self.params_exact = outer_params;
        self.bodies.insert(def_id, exact);
        exact
    }

    /// A stage of a body: a definition is exact when its value is.
    fn stage_is_exact(&mut self, hir: &Hir, stage: SymbolId) -> bool {
        match hir.symbol(stage).map(|s| &s.kind) {
            Some(SymbolKind::Variable | SymbolKind::DestructuringBinding) => self
                .children
                .get(&stage)
                .and_then(|c| c.last())
                .copied()
                .is_some_and(|init| self.is_exact(hir, init)),
            _ => self.is_exact(hir, stage),
        }
    }

    /// Whether what the stage is piped is exact, or it has no stage before it.
    fn input_is_exact(&mut self, hir: &Hir, id: SymbolId) -> bool {
        match self.previous.get(&id).copied() {
            Some(before) => self.is_exact(hir, before),
            None => true,
        }
    }

    fn ref_is_exact(&mut self, hir: &Hir, id: SymbolId) -> bool {
        let Some(def) = hir.resolve_reference_symbol(id) else {
            return false;
        };
        let Some(def_symbol) = hir.symbol(def) else {
            return false;
        };
        if self.is_narrowed_around(hir, id, def) {
            return false;
        }
        if self.assignments.contains_key(&def) && !self.reassigned_is_exact(hir, def) {
            return false;
        }
        if def_symbol.kind == SymbolKind::Parameter {
            return self.params_exact || self.is_element_of_exact_collection(hir, def);
        }
        if def_symbol.kind != SymbolKind::Variable {
            return false;
        }
        let Some(&init) = self.children.get(&def).and_then(|c| c.last()) else {
            return false;
        };
        self.is_exact(hir, init)
    }

    /// Whether a variable that is assigned again keeps one exact type: its first value and every
    /// value assigned to it are exact and of that type. A value may read the variable itself
    /// (`s = s + "x"`), which holds when the claim does.
    fn reassigned_is_exact(&mut self, hir: &Hir, def: SymbolId) -> bool {
        if let Some(&known) = self.reassigned.get(&def) {
            return known;
        }
        if self.element_assigned.contains(&def) {
            self.reassigned.insert(def, false);
            return false;
        }
        let Some(&init) = self.children.get(&def).and_then(|c| c.last()) else {
            return false;
        };
        let outer_memo = std::mem::take(&mut self.memo);
        self.reassigned.insert(def, true);

        let type_of = |prover: &Self, id: SymbolId| {
            prover
                .ctx
                .get_symbol_type(id)
                .map(|ty| prover.ctx.resolve_type(ty))
                .filter(|ty| !ty.has_pending_var() && !ty.is_dynamic())
        };
        let own = type_of(self, init);
        let assignments = self.assignments.get(&def).cloned().unwrap_or_default();
        let mut exact = own.is_some() && self.is_exact(hir, init);
        for assignment in assignments {
            if !exact {
                break;
            }
            let value = self.children.get(&assignment).and_then(|c| c.get(1)).copied();
            exact = value.is_some_and(|value| self.is_exact(hir, value) && type_of(self, value) == own);
        }

        self.reassigned.insert(def, exact);
        self.memo = outer_memo;
        exact
    }

    /// Whether the parameter is the only one of a function passed to a builtin that calls it on the
    /// elements of its first argument (`map(xs, fn(x): ...)`), and that collection is exact.
    fn is_element_of_exact_collection(&mut self, hir: &Hir, param: SymbolId) -> bool {
        let Some(lambda_id) = hir.symbol(param).and_then(|s| s.parent) else {
            return false;
        };
        let Some(lambda) = hir.symbol(lambda_id) else {
            return false;
        };
        let SymbolKind::Function(params) = &lambda.kind else {
            return false;
        };
        let Some(call_id) = lambda.parent else {
            return false;
        };
        let Some(call) = hir.symbol(call_id) else {
            return false;
        };
        if params.len() != 1
            || call.kind != SymbolKind::Call
            || !ELEMENT_BUILTINS
                .binary_search(&call.value.as_deref().unwrap_or(""))
                .is_ok()
            || !is_audited_builtin(hir, call_id, call)
        {
            return false;
        }
        let collection = self.children.get(&call_id).and_then(|c| {
            c.iter()
                .copied()
                .find(|&c| c != lambda_id && hir.symbol(c).is_some_and(|s| is_expression(&s.kind)))
        });
        collection.is_some_and(|collection| self.is_exact(hir, collection))
    }

    /// Whether a condition above the reference tests the same variable, which narrows its type.
    fn is_narrowed_around(&self, hir: &Hir, id: SymbolId, def: SymbolId) -> bool {
        let mut current = hir.symbol(id).and_then(|s| s.parent);
        while let Some(ancestor_id) = current {
            let Some(ancestor) = hir.symbol(ancestor_id) else {
                return false;
            };
            if matches!(
                ancestor.kind,
                SymbolKind::If
                    | SymbolKind::Elif
                    | SymbolKind::Unless
                    | SymbolKind::While
                    | SymbolKind::Until
                    | SymbolKind::Match
            ) {
                let tested = self.children.get(&ancestor_id).and_then(|c| c.first()).copied();
                let is_branch = matches!(ancestor.kind, SymbolKind::If | SymbolKind::Elif | SymbolKind::Unless);
                if tested.is_some_and(|t| self.reads(hir, t, def) && !(is_branch && self.is_type_test(hir, t, def))) {
                    return true;
                }
            }
            current = ancestor.parent;
        }
        false
    }

    /// Whether the condition only tests the type of the variable, in a way that the checker narrows
    /// it for: `is_string(x)`, `type(x) == "number"`, `x != None`, or those joined with `!`, `&&`
    /// and `||`. A condition that reads the variable otherwise is not such a test.
    fn is_type_test(&self, hir: &Hir, cond: SymbolId, def: SymbolId) -> bool {
        const PREDICATES: &[&str] = &["is_string", "is_number", "is_bool", "is_array", "is_dict", "is_none"];
        let Some(symbol) = hir.symbol(cond) else {
            return false;
        };
        let children = self.children.get(&cond).cloned().unwrap_or_default();
        let is_def = |id: SymbolId| {
            hir.symbol(id).is_some_and(|s| s.kind == SymbolKind::Ref) && hir.resolve_reference_symbol(id) == Some(def)
        };
        let value = symbol.value.as_deref().unwrap_or("");
        match &symbol.kind {
            SymbolKind::Call => {
                PREDICATES.contains(&value)
                    && children.len() == 1
                    && is_def(children[0])
                    && hir
                        .resolve_reference_symbol(cond)
                        .and_then(|d| hir.symbol(d))
                        .is_some_and(|d| hir.is_builtin_symbol(d))
            }
            SymbolKind::UnaryOp => {
                matches!(value, "!" | "not") && children.len() == 1 && self.is_type_test(hir, children[0], def)
            }
            SymbolKind::BinaryOp if matches!(value, "&&" | "and" | "||" | "or") => children
                .iter()
                .all(|&c| !self.reads(hir, c, def) || self.is_type_test(hir, c, def)),
            SymbolKind::BinaryOp if matches!(value, "==" | "!=") && children.len() == 2 => {
                let kind_of = |id: SymbolId| hir.symbol(id).map(|s| s.kind.clone());
                let is_type_call = |id: SymbolId| {
                    hir.symbol(id)
                        .is_some_and(|s| s.kind == SymbolKind::Call && s.value.as_deref() == Some("type"))
                        && self.children.get(&id).is_some_and(|c| c.len() == 1 && is_def(c[0]))
                };
                (is_type_call(children[0]) && kind_of(children[1]) == Some(SymbolKind::String))
                    || (is_def(children[0]) && kind_of(children[1]) == Some(SymbolKind::None))
            }
            _ => false,
        }
    }

    fn reads(&self, hir: &Hir, root: SymbolId, def: SymbolId) -> bool {
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            if hir.symbol(id).is_some_and(|s| s.kind == SymbolKind::Ref)
                && hir.resolve_reference_symbol(id) == Some(def)
            {
                return true;
            }
            if let Some(children) = self.children.get(&id) {
                stack.extend(children.iter().copied());
            }
        }
        false
    }

    /// An operand whose type the checker could not settle.
    fn has_unsettled_operand(&mut self, hir: &Hir, root: SymbolId) -> bool {
        // A function passed as an argument has the type variables of its own parameters.
        let operands: Vec<SymbolId> = self
            .children
            .get(&root)
            .into_iter()
            .flatten()
            .copied()
            .filter(|&id| hir.symbol(id).is_some_and(|s| is_expression(&s.kind)))
            .collect();
        operands.into_iter().any(|id| {
            self.ctx.get_symbol_type(id).is_some_and(|ty| !self.ctx.is_settled(ty))
                && self.exact_type(hir, id).is_none()
        })
    }
}

/// Whether the call is of a builtin whose signature has been audited.
fn is_audited_builtin(hir: &Hir, id: SymbolId, symbol: &mq_hir::Symbol) -> bool {
    let Some(name) = symbol.value.as_deref() else {
        return false;
    };
    UNAUDITED_BUILTINS.binary_search(&name).is_err()
        && hir
            .resolve_reference_symbol(id)
            .and_then(|def| hir.symbol(def))
            .is_some_and(|def| hir.is_builtin_symbol(def))
}

/// Whether the symbol applies something to operands.
fn is_operation(kind: &SymbolKind) -> bool {
    matches!(kind, SymbolKind::Call | SymbolKind::BinaryOp | SymbolKind::UnaryOp)
}

/// Whether the symbol is an expression with a value (not a keyword or a parameter).
fn is_expression(kind: &SymbolKind) -> bool {
    !matches!(
        kind,
        SymbolKind::Keyword | SymbolKind::Parameter | SymbolKind::Selector(_) | SymbolKind::Function(_)
    )
}

fn is_stage_kind(kind: &SymbolKind) -> bool {
    !matches!(kind, SymbolKind::Keyword | SymbolKind::Parameter)
}

/// Whether a stage hands its own input to the next stage instead of its value.
fn passes_input_on(kind: &SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::Variable
            | SymbolKind::DestructuringBinding
            | SymbolKind::Function(_)
            | SymbolKind::Assign
            | SymbolKind::Include(_)
            | SymbolKind::Import(_)
            | SymbolKind::Module(_)
    )
}

/// The failure of a `try:` body is handled by its `catch`.
pub(super) fn in_try_body(hir: &Hir, id: SymbolId) -> bool {
    let mut child = id;
    let mut current = hir.symbol(id).and_then(|s| s.parent);
    while let Some(parent_id) = current {
        let Some(parent) = hir.symbol(parent_id) else {
            return false;
        };
        if parent.kind == SymbolKind::Try {
            let is_handler = hir.symbol(child).is_some_and(|s| s.kind == SymbolKind::Catch);
            return !is_handler;
        }
        child = parent_id;
        current = parent.parent;
    }
    false
}
