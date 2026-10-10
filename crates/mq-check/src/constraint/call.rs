//! The one place where an application of a builtin (a call, a piped reference or an operator) is
//! resolved against the overloads of that builtin.
//!
//! The steps are always the same: wait while an argument is not settled enough to pick an
//! overload, pick the best overload, tie the arguments to its parameters, and report the call
//! when nothing matches. Callers only decide what the call looks like (its constraint origin and
//! when it may be given piped input) and where the result type goes.

use mq_hir::SymbolId;
use smol_str::SmolStr;

use crate::{
    builtin::PARTIAL,
    constraint::{Constraint, ConstraintOrigin},
    infer::{DeferredOverload, InferenceContext},
    types::Type,
};

/// What the application looks like in the source, which decides how its constraints are labelled
/// and when it waits for its arguments.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum CallKind {
    /// `f(a, b)`: waits only when `f` has several overloads to choose from
    Function,
    /// `x | f`: the piped value is the only argument
    Piped,
    /// `a + b`, `-a`, `a += b`: waits whenever an operand is not settled
    Operator,
}

pub(super) struct BuiltinCall<'a> {
    /// The symbol the result belongs to, and which a deferred resolution fills in later
    pub symbol_id: SymbolId,
    pub name: &'a str,
    /// The arguments as written, already including a piped input that was prepended
    pub args: &'a [Type],
    pub range: Option<mq_lang::Range>,
    pub kind: CallKind,
    /// The call may still be given a piped input, as `join(",")` in `map(xs, join(","))` is. It
    /// is then reported only if no piped input could make it match.
    pub may_get_piped_input: bool,
    /// Whether a call that matches no overload is reported
    pub report: bool,
    /// For `partial`: the piped input, when it is not yet known whether it is the function
    pub unsettled_piped: Option<Type>,
}

impl<'a> BuiltinCall<'a> {
    pub(super) fn new(
        symbol_id: SymbolId,
        name: &'a str,
        args: &'a [Type],
        range: Option<mq_lang::Range>,
        kind: CallKind,
    ) -> Self {
        Self {
            symbol_id,
            name,
            args,
            range,
            kind,
            may_get_piped_input: false,
            report: true,
            unsettled_piped: None,
        }
    }
}

/// The outcome of resolving a call. Each carries the type of the result.
pub(super) enum Resolution {
    /// An overload was picked and the arguments tied to its parameters
    Resolved(Type),
    /// An argument is not settled yet; a fresh type stands for the result until the deferred pass
    Deferred(Type),
    /// No overload matched (reported unless the call may still get piped input)
    Failed(Type),
}

impl Resolution {
    pub(super) fn into_type(self) -> Type {
        match self {
            Resolution::Resolved(ty) | Resolution::Deferred(ty) | Resolution::Failed(ty) => ty,
        }
    }
}

/// Whether adding one more argument in front (the piped input) would make the call match an
/// overload of the builtin `name`.
fn completed_by_piped_input(ctx: &mut InferenceContext, name: &str, args: &[Type]) -> bool {
    // `partial(f, ...)` keeps its own function: the piped input only stands in for one that is
    // missing, or is the function when a single argument is given.
    if name == PARTIAL && args.len() > 1 && matches!(ctx.resolve_type(&args[0]), Type::Function(..)) {
        return false;
    }
    let piped = Type::Var(ctx.fresh_var());
    let with_piped: Vec<Type> = std::iter::once(piped).chain(args.iter().cloned()).collect();
    ctx.resolve_overload(name, &with_piped).is_some()
}

fn constraint_origin(call: &BuiltinCall<'_>, arg_index: usize) -> ConstraintOrigin {
    let name = SmolStr::new(call.name);
    match call.kind {
        CallKind::Function => ConstraintOrigin::Argument {
            fn_name: name,
            arg_index,
        },
        CallKind::Piped => ConstraintOrigin::PipedInput { fn_name: name },
        CallKind::Operator => ConstraintOrigin::Operator { op: name },
    }
}

pub(super) fn resolve_builtin(ctx: &mut InferenceContext, call: &BuiltinCall<'_>) -> Resolution {
    let resolved_args: Vec<Type> = call.args.iter().map(|ty| ctx.resolve_type(ty)).collect();
    let is_builtin = ctx.get_builtin_overloads(call.name).is_some();

    // Pick no overload for an argument that is not settled yet: it could be pinned to whatever
    // the first matching overload accepts. `partial` is typed from its function argument alone.
    let by_function_arg = call.name == PARTIAL;
    let pending = if by_function_arg {
        resolved_args.first().is_some_and(Type::is_pending_operand)
    } else {
        resolved_args.iter().any(Type::is_pending_operand)
    };
    let waits = pending
        && match call.kind {
            CallKind::Operator => true,
            CallKind::Function | CallKind::Piped => {
                is_builtin
                    && (by_function_arg
                        || ctx
                            .get_builtin_overloads(call.name)
                            .is_some_and(|overloads| overloads.len() > 1))
            }
        };
    if waits {
        let result = Type::Var(ctx.fresh_var());
        ctx.add_deferred_overload(DeferredOverload {
            symbol_id: call.symbol_id,
            op_name: SmolStr::new(call.name),
            operand_tys: call.args.to_vec(),
            unsettled_piped: call.unsettled_piped.clone(),
            range: call.range,
        });
        return Resolution::Deferred(result);
    }

    if let Some(Type::Function(params, ret)) = ctx.resolve_overload(call.name, &resolved_args) {
        for (index, (arg, param)) in call.args.iter().zip(params.iter()).enumerate() {
            ctx.add_constraint(Constraint::Equal(
                arg.clone(),
                param.clone(),
                call.range,
                constraint_origin(call, index),
            ));
        }
        return Resolution::Resolved(*ret);
    }

    let reportable = is_builtin
        && call.report
        && (!call.may_get_piped_input || !completed_by_piped_input(ctx, call.name, &resolved_args));
    if reportable {
        ctx.report_no_matching_overload(call.name, &resolved_args, call.range);
    }
    Resolution::Failed(Type::Var(ctx.fresh_var()))
}
