//! Invoking a callee: binding arguments and producing the `Frame` for the trampoline to push,
//! instead of calling back into the dispatch loop directly.
use super::errors::{VmError, VmResult, locate};
use super::frame::{Continuation, ExecutionContext, Frame, PendingCall};
use super::{current_self, into_runtime_value};
use crate::Shared;
use crate::ast::constants::builtins;
use crate::runtime::builtin::{self, Args};
use crate::runtime::host::HostFunctions;
use crate::runtime::runtime_value::RuntimeValue;
use crate::tarn::VmEnv;
use crate::tarn::bytecode::{Chunk, ParamBinding, ParamShape, SELF_SLOT, UpvalueSource};
use crate::tarn::value::{Cell, Closure, Locals, StackValue};
use std::collections::VecDeque;

pub(super) struct CallSite<'a> {
    pub(super) locals: &'a Locals,
    pub(super) chunk: &'a Chunk,
    pub(super) ip: usize,
    /// `None` represents the root chunk pool held by the active trampoline.
    pub(super) frame_chunks: Option<Shared<Vec<Chunk>>>,
}

/// Static properties of a direct fixed-arity closure call.
pub(super) struct FixedClosureCall<'a> {
    pub(super) closure: &'a Closure,
    pub(super) argc: u16,
    pub(super) remove_callee: bool,
}

/// Resolved target metadata shared by closure and static fixed-arity calls.
struct FixedChunkCall {
    chunk_index: u16,
    upvalues: Option<Shared<Vec<Cell>>>,
    argc: u16,
    remove_callee: bool,
}

/// Metadata embedded in an exact or implicit-self direct-call opcode.
pub(super) struct KnownFixedChunkCall {
    pub(super) chunk_index: u16,
    pub(super) upvalues: Option<Shared<Vec<Cell>>>,
    pub(super) argc: u16,
    pub(super) uses_implicit_self: bool,
    pub(super) remove_callee: bool,
}

/// Metadata already available to a direct exact-call opcode or its active self frame.
pub(super) struct ExactCallTarget<'a> {
    pub(super) chunk_index: u16,
    pub(super) local_count: u16,
    pub(super) captured_local_slots: &'a [u16],
}

/// Chunk/pool access shared by parameter binding and default-value evaluation.
struct ParameterContext<'chunks, 'execution> {
    chunks: &'chunks Shared<Vec<Chunk>>,
    frame_chunks: Option<Shared<Vec<Chunk>>>,
    limits: &'execution mut super::frame::ExecutionLimits,
}

pub(super) fn capture_upvalues(sources: &[UpvalueSource], locals: &Locals, upvalues: &[Cell]) -> Vec<Cell> {
    sources
        .iter()
        .map(|source| match source {
            UpvalueSource::Local(slot) => Shared::clone(locals.cell(*slot)),
            UpvalueSource::Upvalue(idx) => Shared::clone(&upvalues[*idx as usize]),
        })
        .collect()
}

/// A resolved call: either an already-computed value (a native builtin), or a `Frame` to push.
pub(super) enum CallStep {
    Value(StackValue),
    Enter(Frame),
}

pub(super) fn call_stack_value(
    callee: StackValue,
    args: &mut Vec<StackValue>,
    call_site: CallSite<'_>,
    chunks: &Shared<Vec<Chunk>>,
    execution: &mut ExecutionContext<'_>,
) -> VmResult<CallStep> {
    if let StackValue::Value(RuntimeValue::NativeFunction(ident)) = callee {
        // `drain` (rather than `into_iter`) leaves `args`'s allocation intact for the
        // caller to recycle, same as every other exit path below.
        // `Args` stores the common one- and two-argument cases inline, unlike `Vec`.
        let arg_values: Args = args.drain(..).map(|a| into_runtime_value(a, chunks)).collect();
        let self_value = current_self(call_site.locals, chunks);
        let result = call_builtin_args(&ident, arg_values, &self_value, execution.env, execution.host_functions)
            .map_err(|e| locate(call_site.chunk, call_site.ip, e))?;
        return Ok(CallStep::Value(StackValue::Value(result)));
    }

    let (callee_chunks, callee_chunk_index, callee_upvalues, callee_frame_chunks) = match &callee {
        StackValue::Closure(closure) => (
            chunks,
            closure.chunk_index,
            closure.upvalues.clone(),
            call_site.frame_chunks,
        ),
        StackValue::Value(RuntimeValue::VmClosure(vc)) => {
            if !vc.bound_args.is_empty() {
                // `args` is a caller-owned pooled buffer. Prepend into a second pooled buffer,
                // then immediately return the emptied original one instead of dropping its
                // allocation on every `partial` call.
                let mut combined = execution.limits.take_stack();
                combined.extend(vc.bound_args.iter().cloned().map(StackValue::Value));
                combined.append(args);
                std::mem::swap(args, &mut combined);
                execution.limits.recycle_stack(combined);
            }
            (
                &vc.chunks,
                vc.chunk_index,
                vc.upvalues.clone(),
                Some(Shared::clone(&vc.chunks)),
            )
        }
        _ => return Err(locate(call_site.chunk, call_site.ip, VmError::NotCallable)),
    };
    let callee_chunk = &callee_chunks[callee_chunk_index as usize];
    let mut callee_locals = execution
        .limits
        .take_locals(callee_chunk.local_count, callee_chunk.captured_local_slots());
    callee_locals.set(SELF_SLOT, call_site.locals.get(SELF_SLOT));

    let frame = bind_params(
        &callee_chunk.param_shape,
        args,
        callee_locals,
        callee_upvalues,
        callee_chunk_index,
        &mut ParameterContext {
            chunks: callee_chunks,
            frame_chunks: callee_frame_chunks,
            limits: execution.limits,
        },
    )
    .map_err(|e| locate(call_site.chunk, call_site.ip, e))?;
    Ok(CallStep::Enter(frame))
}

pub(super) fn call_fixed_closure_from_stack(
    call: FixedClosureCall<'_>,
    stack: &mut Vec<StackValue>,
    call_site: CallSite<'_>,
    chunks: &Shared<Vec<Chunk>>,
    execution: &mut ExecutionContext<'_>,
) -> VmResult<Frame> {
    call_fixed_chunk_from_stack(
        FixedChunkCall {
            chunk_index: call.closure.chunk_index,
            upvalues: call.closure.upvalues.clone(),
            argc: call.argc,
            remove_callee: call.remove_callee,
        },
        stack,
        call_site,
        chunks,
        execution,
    )
}

/// Builds a frame for a capture-free fixed-arity chunk. `CallStatic` uses this path so it does
/// not need to load or clone the closure stored in the defining local slot.
pub(super) fn call_static_chunk_from_stack(
    chunk_index: u16,
    argc: u16,
    stack: &mut Vec<StackValue>,
    call_site: CallSite<'_>,
    chunks: &Shared<Vec<Chunk>>,
    execution: &mut ExecutionContext<'_>,
) -> VmResult<Frame> {
    call_fixed_chunk_from_stack(
        FixedChunkCall {
            chunk_index,
            upvalues: None,
            argc,
            remove_callee: false,
        },
        stack,
        call_site,
        chunks,
        execution,
    )
}

/// Builds a recursive frame using the current frame's captured environment directly.
pub(super) fn call_self_chunk_from_stack(
    chunk_index: u16,
    upvalues: Option<Shared<Vec<Cell>>>,
    argc: u16,
    stack: &mut Vec<StackValue>,
    call_site: CallSite<'_>,
    chunks: &Shared<Vec<Chunk>>,
    execution: &mut ExecutionContext<'_>,
) -> VmResult<Frame> {
    call_fixed_chunk_from_stack(
        FixedChunkCall {
            chunk_index,
            upvalues,
            argc,
            remove_callee: false,
        },
        stack,
        call_site,
        chunks,
        execution,
    )
}

fn call_fixed_chunk_from_stack(
    call: FixedChunkCall,
    stack: &mut Vec<StackValue>,
    call_site: CallSite<'_>,
    chunks: &Shared<Vec<Chunk>>,
    execution: &mut ExecutionContext<'_>,
) -> VmResult<Frame> {
    let callee_chunk = &chunks[call.chunk_index as usize];
    let Some(arity) = callee_chunk.param_shape.fixed_required_arity() else {
        return Err(locate(
            call_site.chunk,
            call_site.ip,
            VmError::Corrupt("fixed call has non-fixed parameters"),
        ));
    };
    let argc = call.argc as usize;
    let uses_implicit_self = arity > 0 && argc + 1 == arity;
    if argc != arity && !uses_implicit_self {
        return Err(locate(
            call_site.chunk,
            call_site.ip,
            VmError::ArityMismatch {
                expected: arity,
                actual: argc,
            },
        ));
    }
    if stack.len() < argc + usize::from(call.remove_callee) {
        return Err(locate(
            call_site.chunk,
            call_site.ip,
            VmError::Corrupt("stack underflow in fixed closure call"),
        ));
    }

    call_known_fixed_chunk_from_stack(
        KnownFixedChunkCall {
            chunk_index: call.chunk_index,
            upvalues: call.upvalues,
            argc: call.argc,
            uses_implicit_self,
            remove_callee: call.remove_callee,
        },
        stack,
        call_site,
        chunks,
        execution,
    )
}

/// Builds a frame for a direct fixed-arity call whose parameter form was established while
/// compiling bytecode. The verifier ensures the opcode agrees with the target chunk.
pub(super) fn call_known_fixed_chunk_from_stack(
    call: KnownFixedChunkCall,
    stack: &mut Vec<StackValue>,
    call_site: CallSite<'_>,
    chunks: &Shared<Vec<Chunk>>,
    execution: &mut ExecutionContext<'_>,
) -> VmResult<Frame> {
    let callee_chunk = &chunks[call.chunk_index as usize];
    let argc = call.argc as usize;
    if stack.len() < argc + usize::from(call.remove_callee) {
        return Err(locate(
            call_site.chunk,
            call_site.ip,
            VmError::Corrupt("stack underflow in fixed closure call"),
        ));
    }

    let arity = argc + usize::from(call.uses_implicit_self);
    let initialized_slots = SELF_SLOT as usize + 1 + arity;
    let mut callee_locals = execution.limits.take_locals_with_initialized_prefix(
        callee_chunk.local_count,
        initialized_slots,
        callee_chunk.captured_local_slots(),
    );
    let self_value = call_site.locals.get(SELF_SLOT);
    let first_arg_slot = if call.uses_implicit_self {
        callee_locals.set(SELF_SLOT, self_value.clone());
        callee_locals.set(SELF_SLOT + 1, self_value);
        SELF_SLOT as usize + 2
    } else {
        callee_locals.set(SELF_SLOT, self_value);
        SELF_SLOT as usize + 1
    };
    for offset in (0..argc).rev() {
        let Some(value) = stack.pop() else {
            recycle_locals_if_possible(execution.limits, callee_locals, callee_chunk.captures_local_slots());
            return Err(locate(
                call_site.chunk,
                call_site.ip,
                VmError::Corrupt("stack underflow while binding fixed-call arguments"),
            ));
        };
        callee_locals.set((first_arg_slot + offset) as u16, value);
    }
    if call.remove_callee && stack.pop().is_none() {
        recycle_locals_if_possible(execution.limits, callee_locals, callee_chunk.captures_local_slots());
        return Err(locate(
            call_site.chunk,
            call_site.ip,
            VmError::Corrupt("stack underflow while removing fixed-call callee"),
        ));
    }
    Ok(Frame::new(
        call.chunk_index,
        call_site.frame_chunks,
        callee_locals,
        call.upvalues,
        !callee_chunk.captures_local_slots(),
        Continuation::Push,
    ))
}

/// Builds a frame for a verified zero-argument direct call without entering the generic
/// fixed-call binder.
pub(super) fn call_exact_fixed_chunk_0(
    target: ExactCallTarget<'_>,
    upvalues: Option<Shared<Vec<Cell>>>,
    call_site: CallSite<'_>,
    execution: &mut ExecutionContext<'_>,
) -> Frame {
    let mut callee_locals = execution.limits.take_locals_with_initialized_prefix(
        target.local_count,
        SELF_SLOT as usize + 1,
        target.captured_local_slots,
    );
    callee_locals.set(SELF_SLOT, call_site.locals.get(SELF_SLOT));
    exact_fixed_frame(target, call_site.frame_chunks, callee_locals, upvalues)
}

/// Builds a frame for a verified one-argument direct call without a parameter-binding loop.
pub(super) fn call_exact_fixed_chunk_1(
    target: ExactCallTarget<'_>,
    upvalues: Option<Shared<Vec<Cell>>>,
    stack: &mut Vec<StackValue>,
    call_site: CallSite<'_>,
    execution: &mut ExecutionContext<'_>,
) -> VmResult<Frame> {
    let Some(argument) = stack.pop() else {
        return Err(locate(
            call_site.chunk,
            call_site.ip,
            VmError::Corrupt("stack underflow in one-argument exact fixed call"),
        ));
    };
    let mut callee_locals = execution.limits.take_locals_with_initialized_prefix(
        target.local_count,
        SELF_SLOT as usize + 2,
        target.captured_local_slots,
    );
    callee_locals.set(SELF_SLOT, call_site.locals.get(SELF_SLOT));
    callee_locals.set(SELF_SLOT + 1, argument);
    Ok(exact_fixed_frame(
        target,
        call_site.frame_chunks,
        callee_locals,
        upvalues,
    ))
}

/// Builds a frame for a verified two-argument direct call without a parameter-binding loop.
pub(super) fn call_exact_fixed_chunk_2(
    target: ExactCallTarget<'_>,
    upvalues: Option<Shared<Vec<Cell>>>,
    stack: &mut Vec<StackValue>,
    call_site: CallSite<'_>,
    execution: &mut ExecutionContext<'_>,
) -> VmResult<Frame> {
    let Some(second_argument) = stack.pop() else {
        return Err(locate(
            call_site.chunk,
            call_site.ip,
            VmError::Corrupt("stack underflow in two-argument exact fixed call"),
        ));
    };
    let Some(first_argument) = stack.pop() else {
        stack.push(second_argument);
        return Err(locate(
            call_site.chunk,
            call_site.ip,
            VmError::Corrupt("stack underflow in two-argument exact fixed call"),
        ));
    };
    let mut callee_locals = execution.limits.take_locals_with_initialized_prefix(
        target.local_count,
        SELF_SLOT as usize + 3,
        target.captured_local_slots,
    );
    callee_locals.set(SELF_SLOT, call_site.locals.get(SELF_SLOT));
    callee_locals.set(SELF_SLOT + 1, first_argument);
    callee_locals.set(SELF_SLOT + 2, second_argument);
    Ok(exact_fixed_frame(
        target,
        call_site.frame_chunks,
        callee_locals,
        upvalues,
    ))
}

fn exact_fixed_frame(
    target: ExactCallTarget<'_>,
    frame_chunks: Option<Shared<Vec<Chunk>>>,
    callee_locals: Locals,
    upvalues: Option<Shared<Vec<Cell>>>,
) -> Frame {
    Frame::new(
        target.chunk_index,
        frame_chunks,
        callee_locals,
        upvalues,
        target.captured_local_slots.is_empty(),
        Continuation::Push,
    )
}

/// Binds `args` and returns the next `Frame` to push: the callee's body, or (if a missing
/// argument needs its default) the default-value expression to run first.
fn bind_params(
    shape: &ParamShape,
    args: &mut Vec<StackValue>,
    mut callee_locals: Locals,
    callee_upvalues: Option<Shared<Vec<Cell>>>,
    callee_chunk_index: u16,
    context: &mut ParameterContext<'_, '_>,
) -> VmResult<Frame> {
    if let Some(arity) = shape.fixed_required_arity() {
        bind_fixed_required_params(arity, args, &mut callee_locals, context.chunks)?;
        return Ok(build_callee_frame(
            callee_locals,
            callee_upvalues,
            callee_chunk_index,
            context,
        ));
    }

    let arg_count = args.len();
    let param_count = shape.bindings.len();
    let use_self_param = parameter_uses_implicit_self(shape, arg_count)?;

    // `drain` (rather than `into_iter`) leaves `args`'s allocation for the caller to recycle.
    let remaining_args: VecDeque<StackValue> = args.drain(..).collect();
    let mut start_index = 0;
    if use_self_param && let Some(binding) = shape.bindings.first() {
        let self_value = current_self(&callee_locals, context.chunks);
        callee_locals.set(binding.slot(), StackValue::Value(self_value));
        start_index = 1;
    }

    resume_bind_params(
        shape,
        remaining_args,
        callee_locals,
        callee_upvalues,
        callee_chunk_index,
        context,
        start_index,
        arg_count,
        param_count,
    )
}

/// Stores a completed default-value expression's result and resumes binding.
pub(super) fn apply_pending(
    mut pending: PendingCall,
    value: StackValue,
    execution: &mut ExecutionContext<'_>,
) -> VmResult<Frame> {
    pending.callee_locals.set(pending.target_slot, value);
    let PendingCall {
        callee_locals,
        callee_locals_reusable: _,
        callee_upvalues,
        callee_chunk_index,
        callee_chunks,
        remaining_args,
        target_slot: _,
        next_index,
        arg_count,
        param_count,
    } = pending;
    let shape = &callee_chunks[callee_chunk_index as usize].param_shape;
    let mut context = ParameterContext {
        chunks: &callee_chunks,
        frame_chunks: Some(Shared::clone(&callee_chunks)),
        limits: execution.limits,
    };
    resume_bind_params(
        shape,
        remaining_args,
        callee_locals,
        callee_upvalues,
        callee_chunk_index,
        &mut context,
        next_index,
        arg_count,
        param_count,
    )
}

/// Resumes binding `shape.bindings[start_index..]`.
#[allow(clippy::too_many_arguments)]
fn resume_bind_params(
    shape: &ParamShape,
    mut remaining_args: VecDeque<StackValue>,
    mut callee_locals: Locals,
    callee_upvalues: Option<Shared<Vec<Cell>>>,
    callee_chunk_index: u16,
    context: &mut ParameterContext<'_, '_>,
    start_index: usize,
    arg_count: usize,
    param_count: usize,
) -> VmResult<Frame> {
    for index in start_index..shape.bindings.len() {
        match &shape.bindings[index] {
            ParamBinding::Variadic(slot) => {
                let collected: Vec<RuntimeValue> = remaining_args
                    .drain(..)
                    .map(|arg| into_runtime_value(arg, context.chunks))
                    .collect();
                callee_locals.set(*slot, StackValue::Value(RuntimeValue::Array(Shared::new(collected))));
            }
            ParamBinding::Required(slot) => {
                let Some(value) = remaining_args.pop_front() else {
                    return Err(VmError::ArityMismatch {
                        expected: param_count,
                        actual: arg_count,
                    });
                };
                callee_locals.set(*slot, value);
            }
            ParamBinding::Optional(slot, default_chunk, default_upvalues) => {
                if let Some(value) = remaining_args.pop_front() {
                    callee_locals.set(*slot, value);
                } else {
                    let captured = capture_upvalues(
                        default_upvalues,
                        &callee_locals,
                        callee_upvalues.as_deref().map_or_else(|| &[][..], Vec::as_slice),
                    );
                    let default_chunk_ref = &context.chunks[*default_chunk as usize];
                    let mut default_locals = context
                        .limits
                        .take_locals(default_chunk_ref.local_count, default_chunk_ref.captured_local_slots());
                    default_locals.set(SELF_SLOT, callee_locals.get(SELF_SLOT));
                    let callee_locals_reusable = !context.chunks[callee_chunk_index as usize].captures_local_slots();
                    let pending = PendingCall {
                        callee_locals,
                        callee_locals_reusable,
                        callee_upvalues,
                        callee_chunk_index,
                        callee_chunks: Shared::clone(context.chunks),
                        remaining_args,
                        target_slot: *slot,
                        next_index: index + 1,
                        arg_count,
                        param_count,
                    };
                    return Ok(Frame::new(
                        *default_chunk,
                        Some(Shared::clone(context.chunks)),
                        default_locals,
                        (!captured.is_empty()).then(|| Shared::new(captured)),
                        !default_chunk_ref.captures_local_slots(),
                        Continuation::ResumeBindParams(Box::new(pending)),
                    ));
                }
            }
        }
    }
    Ok(build_callee_frame(
        callee_locals,
        callee_upvalues,
        callee_chunk_index,
        context,
    ))
}

fn build_callee_frame(
    callee_locals: Locals,
    callee_upvalues: Option<Shared<Vec<Cell>>>,
    callee_chunk_index: u16,
    context: &mut ParameterContext<'_, '_>,
) -> Frame {
    let reusable = !context.chunks[callee_chunk_index as usize].captures_local_slots();
    Frame::new(
        callee_chunk_index,
        context.frame_chunks.take(),
        callee_locals,
        callee_upvalues,
        reusable,
        Continuation::Push,
    )
}

/// Returns a frame to the allocation pool when no closure can retain its local cells.
fn recycle_locals_if_possible(limits: &mut super::frame::ExecutionLimits, locals: Locals, captures_local_slots: bool) {
    if !captures_local_slots {
        limits.recycle_locals(locals);
    }
}

fn bind_fixed_required_params(
    arity: usize,
    args: &mut Vec<StackValue>,
    callee_locals: &mut Locals,
    chunks: &Shared<Vec<Chunk>>,
) -> VmResult<()> {
    let arg_count = args.len();
    let first_arg_slot = if arg_count == arity {
        SELF_SLOT as usize + 1
    } else if arity > 0 && arg_count + 1 == arity {
        callee_locals.set(SELF_SLOT + 1, StackValue::Value(current_self(callee_locals, chunks)));
        SELF_SLOT as usize + 2
    } else {
        return Err(VmError::ArityMismatch {
            expected: arity,
            actual: arg_count,
        });
    };

    // `drain` (rather than `into_iter`) leaves `args`'s allocation for the caller to recycle.
    for (offset, value) in args.drain(..).enumerate() {
        callee_locals.set((first_arg_slot + offset) as u16, value);
    }
    Ok(())
}

fn parameter_uses_implicit_self(shape: &ParamShape, arg_count: usize) -> VmResult<bool> {
    let parameter_count = shape.bindings.len();
    let accepts_explicit_args = arg_count >= shape.required && (shape.has_variadic || arg_count <= parameter_count);
    if accepts_explicit_args {
        return Ok(false);
    }

    let accepts_implicit_self = arg_count.saturating_add(1) >= shape.required && arg_count < parameter_count;
    if accepts_implicit_self {
        return Ok(true);
    }

    Err(VmError::ArityMismatch {
        expected: if shape.has_variadic {
            shape.required
        } else {
            parameter_count
        },
        actual: arg_count,
    })
}

pub(super) fn call_builtin(
    ident: &crate::Ident,
    args: &[RuntimeValue],
    self_value: &RuntimeValue,
    env: &VmEnv,
    host_functions: &HostFunctions,
) -> VmResult<RuntimeValue> {
    call_builtin_args(ident, args.iter().cloned().collect(), self_value, env, host_functions)
}

pub(super) fn call_builtin_args(
    ident: &crate::Ident,
    args: Args,
    self_value: &RuntimeValue,
    env: &VmEnv,
    host_functions: &HostFunctions,
) -> VmResult<RuntimeValue> {
    let host_args = host_functions.get(ident).map(|_| args.clone());
    match builtin::eval_builtin(self_value, ident, args, env) {
        Ok(v) => Ok(v),
        Err(builtin::Error::NotDefined(_, _)) => match host_functions.get(ident) {
            Some(host_fn) => {
                let Some(host_args) = host_args.as_deref() else {
                    return Err(VmError::Corrupt("host function arguments were not retained"));
                };
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| host_fn.call(host_args)))
                    .unwrap_or_else(|payload| {
                        Err(crate::runtime::host::HostFunctionError::new(format!(
                            "panic: {}",
                            crate::runtime::host::panic_message(&*payload)
                        )))
                    })
                    .map_err(|e| VmError::Host(*ident, e.message().to_string()))
            }
            None => Err(VmError::Builtin(builtin::Error::NotDefined(
                ident.to_string(),
                Vec::new(),
            ))),
        },
        Err(e) => Err(VmError::Builtin(e)),
    }
}

pub(super) fn negate_ident() -> &'static crate::Ident {
    use std::sync::LazyLock;
    static NEGATE: LazyLock<crate::Ident> = LazyLock::new(|| crate::Ident::new(builtins::NEGATE));
    &NEGATE
}
