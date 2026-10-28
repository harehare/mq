//! Checks a call of a function of the program by running its body on the types of the arguments.
//!
//! The type of a parameter is inferred from the whole body, which the checker cannot tell apart
//! from a part that is run only sometimes. Here only the stages that always run are followed: an
//! operation whose operands are the arguments, literals or the results of such operations is
//! looked up in the overloads, and one that nothing takes is reported at the call.

use mq_hir::{Hir, SymbolId, SymbolKind};
use rustc_hash::{FxHashMap, FxHashSet};
use smol_str::SmolStr;

use crate::builtin::PARTIAL;
use crate::certainty::UNAUDITED_BUILTINS;
use crate::constraint::{ChildrenIndex, get_children};
use crate::infer::{DeferredUserCall, InferenceContext};
use crate::probe::{ELEMENT_BUILTINS, combinations, representatives};
use crate::types::Type;
use crate::{is_module_range, symbol_range};

/// How many functions deep a call is followed.
const MAX_DEPTH: usize = 3;

/// The most parameters of a function that are tried against every type.
const MAX_PARAMS: usize = 2;

/// Operators that do not always evaluate their right operand.
const SHORT_CIRCUIT: &[&str] = &["&&", "||", "and", "or", "??", "coalesce"];

/// An operation of a body that nothing takes.
struct Failure {
    op: SymbolId,
    name: SmolStr,
    args: Vec<Type>,
    /// The first argument is the piped input, which the call does not write
    implicit_input: bool,
    range: Option<mq_lang::Range>,
    /// Raised in a lambda run on the elements of a collection, which may have none
    in_element_callback: bool,
}

/// A call with the types of its arguments, and the lambda written for each argument that is one.
type SettledCall<'a> = (&'a DeferredUserCall, Vec<Type>, Vec<Option<SymbolId>>);

/// Checks each call of a function whose arguments have settled types.
pub(crate) fn check_inlined_calls(
    hir: &Hir,
    ctx: &mut InferenceContext,
    children: &ChildrenIndex,
    calls: &[DeferredUserCall],
) {
    let assigned = assigned_variables(hir, children);
    let settled: Vec<SettledCall> = calls
        .iter()
        .filter_map(|call| {
            let mut args = Vec::with_capacity(call.arg_tys.len());
            let mut lambdas = Vec::with_capacity(call.arg_tys.len());
            for (index, ty) in call.arg_tys.iter().enumerate() {
                let lambda =
                    call.arg_symbol_ids
                        .get(index)
                        .copied()
                        .and_then(|id| match hir.symbol(id).map(|s| &s.kind) {
                            Some(SymbolKind::Function(params)) => Some((id, params.len())),
                            _ => None,
                        });
                match lambda {
                    Some((id, arity)) => {
                        args.push(Type::function(vec![Type::Dynamic; arity], Type::Dynamic));
                        lambdas.push(Some(id));
                    }
                    None => {
                        let ty = ctx.resolve_type(ty);
                        if !ctx.is_settled(&ty) {
                            return None;
                        }
                        args.push(ty);
                        lambdas.push(None);
                    }
                }
            }
            Some((call, args, lambdas))
        })
        .collect();
    // A stage that is threaded twice makes the same call twice
    let mut seen = FxHashSet::default();
    let settled: Vec<SettledCall> = settled
        .into_iter()
        .filter(|(call, args, lambdas)| seen.insert((call.def_id, call.range, args.clone(), lambdas.clone())))
        .collect();
    // A call with settled arguments is checked on its own, wherever it is
    let own_calls: FxHashSet<mq_lang::Range> = settled.iter().filter_map(|(call, _, _)| call.range).collect();
    let mut failures = Vec::new();
    for (call, args, lambdas) in &settled {
        let mut inliner = Inliner {
            hir,
            ctx,
            children,
            assigned: &assigned,
            own_calls: &own_calls,
            stack: Vec::new(),
            lambdas: FxHashMap::default(),
            failures: Vec::new(),
            element_depth: 0,
            ran: false,
        };
        inliner.run(call.def_id, args, lambdas);
        failures.extend(inliner.failures.into_iter().map(|f| (f, call.range)));
    }
    for (failure, call_range) in failures {
        // One that fails whatever the arguments are is reported where it is written
        if ctx.has_overload_error_at(&failure.name, failure.range) {
            continue;
        }
        let range = failure.range.filter(is_module_range).or(call_range);
        ctx.report_inlined_overload(
            &failure.name,
            &failure.args,
            failure.implicit_input,
            range,
            failure.in_element_callback,
        );
    }
}

/// The operations that fail for some type of the parameters of the function, when no type of the
/// parameters lets its body run through. `None` when some type does, or the function is not run.
pub(crate) fn doomed_operations(
    hir: &Hir,
    ctx: &InferenceContext,
    children: &ChildrenIndex,
    assigned: &FxHashSet<SymbolId>,
    def_id: SymbolId,
) -> Option<FxHashSet<SymbolId>> {
    let Some(SymbolKind::Function(params)) = hir.symbol(def_id).map(|s| &s.kind) else {
        return None;
    };
    if params.is_empty() || params.len() > MAX_PARAMS || params.iter().any(|p| p.has_default || p.is_variadic) {
        return None;
    }
    // A value always has a type of its own, which `dynamic` stands in for
    let types: Vec<Type> = representatives().into_iter().filter(|ty| !ty.is_dynamic()).collect();
    let choices = vec![types.as_slice(); params.len()];
    let no_ranges = FxHashSet::default();
    let mut blamed = FxHashSet::default();
    for args in combinations(&choices) {
        let mut inliner = Inliner {
            hir,
            ctx,
            children,
            assigned,
            own_calls: &no_ranges,
            stack: Vec::new(),
            lambdas: FxHashMap::default(),
            failures: Vec::new(),
            element_depth: 0,
            ran: false,
        };
        inliner.run(def_id, &args, &[]);
        if !inliner.ran || inliner.failures.is_empty() {
            return None;
        }
        blamed.extend(inliner.failures.iter().map(|f| f.op));
    }
    Some(blamed)
}

/// The variables and parameters that some assignment writes to.
pub(crate) fn assigned_variables(hir: &Hir, children: &ChildrenIndex) -> FxHashSet<SymbolId> {
    hir.symbols()
        .filter(|(_, symbol)| symbol.kind == SymbolKind::Assign)
        .filter_map(|(id, _)| get_children(children, id).first().copied())
        .filter_map(|target| hir.resolve_reference_symbol(target))
        .collect()
}

struct Inliner<'a> {
    hir: &'a Hir,
    ctx: &'a InferenceContext,
    children: &'a ChildrenIndex,
    assigned: &'a FxHashSet<SymbolId>,
    own_calls: &'a FxHashSet<mq_lang::Range>,
    /// The functions being followed
    stack: Vec<SymbolId>,
    /// The function passed for each parameter that gets a lambda
    lambdas: FxHashMap<SymbolId, SymbolId>,
    failures: Vec<Failure>,
    /// How many lambdas run on the elements of a collection enclose the operation being run
    element_depth: usize,
    /// Whether the body of the function asked for was run
    ran: bool,
}

/// The values of the parameters and of the variables defined so far.
type Scope = FxHashMap<SymbolId, Type>;

impl Inliner<'_> {
    /// Runs the stages of the function that always run and gives the type of the last one.
    fn run(&mut self, def_id: SymbolId, args: &[Type], lambdas: &[Option<SymbolId>]) -> Option<Type> {
        let hir = self.hir;
        let def = hir.symbol(def_id)?;
        let SymbolKind::Function(params) = &def.kind else {
            return None;
        };
        if hir.is_builtin_symbol(def)
            || self.stack.len() >= MAX_DEPTH
            || self.stack.contains(&def_id)
            || params.iter().any(|p| p.is_variadic)
            || args.len() > params.len()
            || params.iter().skip(args.len()).any(|p| !p.has_default)
        {
            return None;
        }
        let stages = get_children(self.children, def_id);
        let param_ids: Vec<SymbolId> = stages
            .iter()
            .copied()
            .filter(|&id| hir.symbol(id).is_some_and(|s| s.is_parameter()))
            .collect();
        if param_ids.len() != params.len() || param_ids.iter().any(|id| self.assigned.contains(id)) {
            return None;
        }

        let mut scope: Scope = param_ids.iter().copied().zip(args.iter().cloned()).collect();
        self.ran = true;
        self.stack.push(def_id);
        for (param, lambda) in param_ids.iter().zip(lambdas) {
            if let Some(lambda) = lambda {
                self.lambdas.insert(*param, *lambda);
            }
        }
        // The input of the body is whatever the caller pipes in
        let mut value: Option<Type> = None;
        let mut first = true;
        // The stage after a parameter with a default is the default itself
        let mut default_of: Option<SymbolId> = None;
        for &stage in stages {
            let Some(kind) = hir.symbol(stage).map(|s| &s.kind) else {
                continue;
            };
            if let Some(param) = default_of
                && !matches!(kind, SymbolKind::Keyword)
            {
                default_of = None;
                if !scope.contains_key(&param)
                    && let Some(ty) = self.eval(stage, None, &mut scope)
                {
                    scope.insert(param, ty);
                }
                continue;
            }
            match kind {
                SymbolKind::Keyword => {}
                SymbolKind::Parameter => {
                    let index = param_ids.iter().position(|&id| id == stage);
                    default_of = index.filter(|&i| params[i].has_default).map(|_| stage);
                }
                SymbolKind::Variable | SymbolKind::DestructuringBinding => {
                    if let Some(init) = get_children(self.children, stage).last().copied()
                        && !self.assigned.contains(&stage)
                        && let Some(ty) = self.eval(init, None, &mut scope)
                    {
                        scope.insert(stage, ty);
                    }
                }
                SymbolKind::Function(_)
                | SymbolKind::Assign
                | SymbolKind::Include(_)
                | SymbolKind::Import(_)
                | SymbolKind::Module(_) => {}
                _ => {
                    let input = if first { None } else { value.clone() };
                    value = self.eval(stage, input.as_ref(), &mut scope);
                    first = false;
                }
            }
        }
        self.stack.pop();
        for param in &param_ids {
            self.lambdas.remove(param);
        }
        value
    }

    /// The type of an expression, or `None` when it is not known.
    fn eval(&mut self, id: SymbolId, piped: Option<&Type>, scope: &mut Scope) -> Option<Type> {
        let hir = self.hir;
        let symbol = hir.symbol(id)?;
        match &symbol.kind {
            SymbolKind::Number
            | SymbolKind::String
            | SymbolKind::Boolean
            | SymbolKind::None
            | SymbolKind::Symbol
            | SymbolKind::Bytes
            | SymbolKind::InterpolatedString => self.settled(id),
            SymbolKind::Ref => {
                let def = hir.resolve_reference_symbol(id)?;
                scope.get(&def).cloned()
            }
            SymbolKind::Array => {
                let elements: Vec<Option<Type>> = self.eval_all(id, scope);
                let same = elements
                    .first()
                    .cloned()
                    .flatten()
                    .filter(|first| elements.iter().all(|e| e.as_ref() == Some(first)));
                Some(Type::array(same.unwrap_or(Type::Dynamic)))
            }
            SymbolKind::Dict => {
                self.eval_all(id, scope);
                Some(Type::dict(Type::Dynamic, Type::Dynamic))
            }
            SymbolKind::BinaryOp | SymbolKind::UnaryOp => {
                let name = symbol.value.as_deref()?;
                if SHORT_CIRCUIT.contains(&name) {
                    if let Some(first) = get_children(self.children, id).first().copied() {
                        self.eval(first, None, scope);
                    }
                    return None;
                }
                let operands = self.eval_all(id, scope);
                // `self` stands for an operand that is not a symbol
                let expected = if symbol.kind == SymbolKind::BinaryOp { 2 } else { 1 };
                if operands.len() != expected {
                    return None;
                }
                self.apply(id, name, &operands, None)
            }
            SymbolKind::Call => self.eval_call(id, piped, scope),
            _ => None,
        }
    }

    fn eval_all(&mut self, id: SymbolId, scope: &mut Scope) -> Vec<Option<Type>> {
        let hir = self.hir;
        let operands: Vec<SymbolId> = get_children(self.children, id)
            .iter()
            .copied()
            .filter(|&c| hir.symbol(c).is_some_and(|s| !matches!(s.kind, SymbolKind::Keyword)))
            .collect();
        operands
            .into_iter()
            .map(|operand| match hir.symbol(operand).map(|s| &s.kind) {
                Some(SymbolKind::Function(params)) => {
                    Some(Type::function(vec![Type::Dynamic; params.len()], Type::Dynamic))
                }
                _ => self.eval(operand, None, scope),
            })
            .collect()
    }

    fn eval_call(&mut self, id: SymbolId, piped: Option<&Type>, scope: &mut Scope) -> Option<Type> {
        let hir = self.hir;
        let symbol = hir.symbol(id)?;
        let name = symbol.value.as_deref()?;
        if hir.bracket_group_count(id) > 0 {
            return None;
        }
        let operands = self.eval_all(id, scope);
        let def = hir.symbol(hir.resolve_reference_symbol(id)?)?;
        if hir.is_builtin_symbol(def) {
            // The signature of a builtin that is not audited may be narrower than the function
            if UNAUDITED_BUILTINS.binary_search(&name).is_ok() {
                return None;
            }
            let result = self.run_on_elements(id, name, &operands, piped);
            let applied = self.apply(id, name, &operands, piped);
            return result.or(applied);
        }
        // A function of the program is followed when it gets all its arguments
        let lambdas: Vec<Option<SymbolId>> = get_children(self.children, id)
            .iter()
            .copied()
            .filter(|&c| hir.symbol(c).is_some_and(|s| !matches!(s.kind, SymbolKind::Keyword)))
            .map(|c| {
                hir.symbol(c)
                    .filter(|s| matches!(s.kind, SymbolKind::Function(_)))
                    .map(|_| c)
            })
            .collect();
        let args: Vec<Type> = operands.into_iter().collect::<Option<_>>()?;
        let mut def_id = hir.resolve_reference_symbol(id)?;
        let mut lambdas = lambdas;
        // A parameter that got a lambda is called with the arguments
        if let Some(&lambda) = self.lambdas.get(&def_id) {
            def_id = lambda;
            lambdas = Vec::new();
        }
        let before = self.failures.len();
        let value = self.run(def_id, &args, &lambdas);
        if symbol_range(hir, symbol).is_some_and(|range| self.own_calls.contains(&range)) {
            self.failures.truncate(before);
        }
        value
    }

    /// Runs the lambda that a builtin calls on each element of a collection of known type.
    fn run_on_elements(
        &mut self,
        id: SymbolId,
        name: &str,
        operands: &[Option<Type>],
        piped: Option<&Type>,
    ) -> Option<Type> {
        if ELEMENT_BUILTINS.binary_search(&name).is_err() {
            return None;
        }
        let hir = self.hir;
        let written: Vec<SymbolId> = get_children(self.children, id)
            .iter()
            .copied()
            .filter(|&c| hir.symbol(c).is_some_and(|s| !matches!(s.kind, SymbolKind::Keyword)))
            .collect();
        let mut lambdas = written
            .iter()
            .filter(|&&c| hir.symbol(c).is_some_and(|s| matches!(s.kind, SymbolKind::Function(_))));
        let (Some(&lambda), None) = (lambdas.next(), lambdas.next()) else {
            return None;
        };
        let collection = written
            .iter()
            .position(|&c| c != lambda)
            .map_or(piped.cloned(), |index| operands.get(index).cloned().flatten());
        let Some(Type::Array(element)) = collection.clone() else {
            return None;
        };
        if element.is_dynamic() {
            return None;
        }
        self.element_depth += 1;
        let value = self.run(lambda, &[*element], &[]);
        self.element_depth -= 1;
        match name {
            "map" => value.map(Type::array),
            "filter" | "reject" | "sort_by" => collection,
            _ => None,
        }
    }

    /// The result of the builtin `name` applied to `operands`, reporting it when nothing takes them.
    fn apply(&mut self, id: SymbolId, name: &str, operands: &[Option<Type>], piped: Option<&Type>) -> Option<Type> {
        if name == PARTIAL || self.ctx.get_builtin_overloads(name).is_none() {
            return None;
        }
        let args: Vec<Type> = operands.iter().cloned().collect::<Option<_>>()?;
        if let Some(Type::Function(_, ret)) = self.ctx.peek_signature(name, &args) {
            return Some(*ret).filter(|ret| !ret.has_pending_var() && !ret.is_dynamic());
        }
        // The piped input comes first when the written arguments do not make a call
        let is_call = self.hir.symbol(id).is_some_and(|s| s.kind == SymbolKind::Call);
        let mut reported = args.clone();
        let mut implicit_input = false;
        if is_call {
            let completed = |this: &Self, first: Type| {
                let with_piped: Vec<Type> = std::iter::once(first).chain(args.iter().cloned()).collect();
                this.ctx.peek_signature(name, &with_piped).map(|sig| (with_piped, sig))
            };
            match piped {
                Some(piped) => match completed(self, piped.clone()) {
                    Some((_, Type::Function(_, ret))) => {
                        return Some(*ret).filter(|ret| !ret.has_pending_var() && !ret.is_dynamic());
                    }
                    Some(_) => return None,
                    None => {
                        reported.insert(0, piped.clone());
                        implicit_input = true;
                    }
                },
                None => {
                    // Whatever is piped in, nothing takes one more argument either
                    if representatives()
                        .into_iter()
                        .any(|first| completed(self, first).is_some())
                    {
                        return None;
                    }
                }
            }
        }
        self.failures.push(Failure {
            op: id,
            name: SmolStr::new(name),
            args: reported,
            implicit_input,
            range: self.hir.symbol(id).and_then(|s| symbol_range(self.hir, s)),
            in_element_callback: self.element_depth > 0,
        });
        None
    }

    fn settled(&self, id: SymbolId) -> Option<Type> {
        let ty = self.ctx.resolve_type(self.ctx.get_symbol_type(id)?);
        self.ctx.is_settled(&ty).then_some(ty)
    }
}
