//! Invoking a callee: binding arguments into a new frame's locals and running its chunk.
//! Covers the generic (`call_stack_value`) and fixed-arity fast (`call_fixed_closure_from_stack`)
//! paths used by `CallLocal`/`CallValue`/`MaybeAutoCall`, and the shared parameter-binding logic
//! (`bind_params`) both funnel into.
use super::errors::{VmError, VmResult, locate};
use super::frame::{ExecutionContext, ExecutionLimits};
use super::{current_self, into_runtime_value, run_chunk};
use crate::ast::constants::builtins;
use crate::runtime::builtin::{self, Args};
use crate::runtime::env::Env;
use crate::runtime::host::HostFunctions;
use crate::runtime::runtime_value::RuntimeValue;
use crate::tarn::bytecode::{Chunk, ParamBinding, ParamShape, SELF_SLOT, UpvalueSource};
use crate::tarn::value::{Cell, Closure, Locals, StackValue};
use crate::{Shared, SharedCell};

#[cfg(feature = "debugger")]
use super::DebugRuntime;

pub(super) struct CallSite<'a> {
    pub(super) locals: &'a Locals,
    pub(super) chunk: &'a Chunk,
    pub(super) ip: usize,
}

/// Static properties of a direct fixed-arity closure call.
pub(super) struct FixedClosureCall<'a> {
    pub(super) closure: &'a Closure,
    pub(super) argc: u16,
    pub(super) remove_callee: bool,
}

/// Runtime services shared by parameter binding and default-value evaluation.
struct ParameterContext<'chunks, 'execution> {
    chunks: &'chunks Shared<Vec<Chunk>>,
    env: &'execution Shared<SharedCell<Env>>,
    limits: &'execution mut ExecutionLimits,
    host_functions: &'execution HostFunctions,
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

pub(super) fn call_stack_value(
    callee: StackValue,
    args: &mut Vec<StackValue>,
    call_site: CallSite<'_>,
    chunks: &Shared<Vec<Chunk>>,
    execution: &mut ExecutionContext<'_>,
    #[cfg(feature = "debugger")] debug: &mut DebugRuntime<'_>,
) -> VmResult<StackValue> {
    if let StackValue::Value(RuntimeValue::NativeFunction(ident)) = callee {
        // `drain` (rather than `into_iter`) leaves `args`'s allocation intact for the
        // caller to recycle, same as every other exit path below.
        let arg_values: Vec<RuntimeValue> = args.drain(..).map(|a| into_runtime_value(a, chunks)).collect();
        let self_value = current_self(call_site.locals, chunks);
        let result = call_builtin(
            &ident,
            &arg_values,
            &self_value,
            execution.env,
            execution.host_functions,
        )
        .map_err(|e| locate(call_site.chunk, call_site.ip, e))?;
        return Ok(StackValue::Value(result));
    }

    let (callee_chunks, callee_chunk_index, callee_upvalues): (&Shared<Vec<Chunk>>, u16, &[Cell]) = match &callee {
        StackValue::Closure(closure) => (chunks, closure.chunk_index, &closure.upvalues),
        StackValue::Value(RuntimeValue::VmClosure(vc)) => {
            if !vc.bound_args.is_empty() {
                let mut combined: Vec<StackValue> = vc.bound_args.iter().cloned().map(StackValue::Value).collect();
                combined.append(args);
                *args = combined;
            }
            (&vc.chunks, vc.chunk_index, &vc.upvalues)
        }
        _ => return Err(locate(call_site.chunk, call_site.ip, VmError::NotCallable)),
    };
    let callee_chunk = &callee_chunks[callee_chunk_index as usize];
    let callee_locals = execution
        .limits
        .take_locals(callee_chunk.local_count, callee_chunk.captures_local_slots());
    callee_locals.set(SELF_SLOT, call_site.locals.get(SELF_SLOT));
    execution
        .limits
        .enter_call()
        .map_err(|e| locate(call_site.chunk, call_site.ip, e))?;
    if let Err(e) = bind_params(
        &callee_chunk.param_shape,
        args,
        &callee_locals,
        callee_upvalues,
        &mut ParameterContext {
            chunks: callee_chunks,
            env: execution.env,
            limits: execution.limits,
            host_functions: execution.host_functions,
        },
        #[cfg(feature = "debugger")]
        debug,
    ) {
        execution.limits.exit_call();
        return Err(locate(call_site.chunk, call_site.ip, e));
    }
    #[cfg(feature = "debugger")]
    let caller_node = debug.current_node.clone();
    #[cfg(feature = "debugger")]
    let pushed_call = if let Some(node) = &caller_node {
        debug.call_stack.push(Shared::clone(node));
        true
    } else {
        false
    };
    let call_result = run_chunk(
        callee_chunk_index,
        callee_chunks,
        callee_locals,
        callee_upvalues,
        execution,
        #[cfg(feature = "debugger")]
        debug,
    );
    execution.limits.exit_call();
    #[cfg(feature = "debugger")]
    if pushed_call {
        debug.call_stack.pop();
    }
    #[cfg(feature = "debugger")]
    {
        debug.current_node = caller_node;
    }
    call_result
}

pub(super) fn call_fixed_closure_from_stack(
    call: FixedClosureCall<'_>,
    stack: &mut Vec<StackValue>,
    call_site: CallSite<'_>,
    chunks: &Shared<Vec<Chunk>>,
    execution: &mut ExecutionContext<'_>,
    #[cfg(feature = "debugger")] debug: &mut DebugRuntime<'_>,
) -> VmResult<StackValue> {
    let closure = call.closure;
    let callee_chunk = &chunks[closure.chunk_index as usize];
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
                expected: arity as u8,
                actual: argc as u8,
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

    let initialized_slots = SELF_SLOT as usize + 1 + arity;
    let callee_locals = execution.limits.take_locals_with_initialized_prefix(
        callee_chunk.local_count,
        initialized_slots,
        callee_chunk.captures_local_slots(),
    );
    let self_value = call_site.locals.get(SELF_SLOT);
    let first_arg_slot = if uses_implicit_self {
        callee_locals.set(SELF_SLOT, self_value.clone());
        callee_locals.set(SELF_SLOT + 1, self_value);
        SELF_SLOT as usize + 2
    } else {
        callee_locals.set(SELF_SLOT, self_value);
        SELF_SLOT as usize + 1
    };
    for offset in (0..argc).rev() {
        let Some(value) = stack.pop() else {
            return Err(locate(
                call_site.chunk,
                call_site.ip,
                VmError::Corrupt("stack underflow while binding fixed-call arguments"),
            ));
        };
        callee_locals.set((first_arg_slot + offset) as u16, value);
    }
    if call.remove_callee && stack.pop().is_none() {
        return Err(locate(
            call_site.chunk,
            call_site.ip,
            VmError::Corrupt("stack underflow while removing fixed-call callee"),
        ));
    }

    #[cfg(feature = "debugger")]
    let caller_node = debug.current_node.clone();
    #[cfg(feature = "debugger")]
    let pushed_call = if let Some(node) = &caller_node {
        debug.call_stack.push(Shared::clone(node));
        true
    } else {
        false
    };
    execution
        .limits
        .enter_call()
        .map_err(|e| locate(call_site.chunk, call_site.ip, e))?;
    let call_result = run_chunk(
        closure.chunk_index,
        chunks,
        callee_locals,
        &closure.upvalues,
        execution,
        #[cfg(feature = "debugger")]
        debug,
    );
    execution.limits.exit_call();
    #[cfg(feature = "debugger")]
    if pushed_call {
        debug.call_stack.pop();
    }
    #[cfg(feature = "debugger")]
    {
        debug.current_node = caller_node;
    }
    call_result
}

fn bind_params(
    shape: &ParamShape,
    args: &mut Vec<StackValue>,
    callee_locals: &Locals,
    enclosing_upvalues: &[Cell],
    context: &mut ParameterContext<'_, '_>,
    #[cfg(feature = "debugger")] debug: &mut DebugRuntime<'_>,
) -> VmResult<()> {
    if let Some(arity) = shape.fixed_required_arity() {
        return bind_fixed_required_params(arity, args, callee_locals, context.chunks);
    }

    let arg_count = args.len();
    let param_count = shape.bindings.len();
    let use_self_param = parameter_uses_implicit_self(shape, arg_count)?;

    let mut bindings = shape.bindings.iter();
    // `drain` (rather than `into_iter`) leaves `args`'s allocation for the caller to recycle.
    let mut args = args.drain(..);

    if use_self_param && let Some(binding) = bindings.next() {
        let self_value = current_self(callee_locals, context.chunks);
        callee_locals.set(binding.slot(), StackValue::Value(self_value));
    }

    for binding in bindings {
        match binding {
            ParamBinding::Variadic(slot) => {
                let collected: Vec<RuntimeValue> = args
                    .by_ref()
                    .map(|arg| into_runtime_value(arg, context.chunks))
                    .collect();
                callee_locals.set(*slot, StackValue::Value(RuntimeValue::Array(Shared::new(collected))));
            }
            ParamBinding::Required(slot) => {
                let Some(value) = args.next() else {
                    return Err(VmError::ArityMismatch {
                        expected: param_count as u8,
                        actual: arg_count as u8,
                    });
                };
                callee_locals.set(*slot, value);
            }
            ParamBinding::Optional(slot, default_chunk, default_upvalues) => {
                if let Some(value) = args.next() {
                    callee_locals.set(*slot, value);
                } else {
                    let captured = capture_upvalues(default_upvalues, callee_locals, enclosing_upvalues);
                    let default_chunk_ref = &context.chunks[*default_chunk as usize];
                    let default_locals = context
                        .limits
                        .take_locals(default_chunk_ref.local_count, default_chunk_ref.captures_local_slots());
                    default_locals.set(SELF_SLOT, callee_locals.get(SELF_SLOT));
                    context.limits.enter_call()?;
                    let result = {
                        let mut execution = ExecutionContext {
                            env: context.env,
                            limits: context.limits,
                            host_functions: context.host_functions,
                        };
                        run_chunk(
                            *default_chunk,
                            context.chunks,
                            default_locals,
                            &captured,
                            &mut execution,
                            #[cfg(feature = "debugger")]
                            debug,
                        )
                    };
                    context.limits.exit_call();
                    callee_locals.set(*slot, result?);
                }
            }
        }
    }
    Ok(())
}

fn bind_fixed_required_params(
    arity: usize,
    args: &mut Vec<StackValue>,
    callee_locals: &Locals,
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
            expected: arity as u8,
            actual: arg_count as u8,
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
            shape.required as u8
        } else {
            parameter_count as u8
        },
        actual: arg_count as u8,
    })
}

pub(super) fn call_builtin(
    ident: &crate::Ident,
    args: &[RuntimeValue],
    self_value: &RuntimeValue,
    env: &Shared<SharedCell<Env>>,
    host_functions: &HostFunctions,
) -> VmResult<RuntimeValue> {
    call_builtin_args(ident, args.iter().cloned().collect(), self_value, env, host_functions)
}

pub(super) fn call_builtin_args(
    ident: &crate::Ident,
    args: Args,
    self_value: &RuntimeValue,
    env: &Shared<SharedCell<Env>>,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::runtime_value::RuntimeValue;

    fn number(value: i64) -> StackValue {
        StackValue::Value(RuntimeValue::Number(value.into()))
    }

    fn value_at(locals: &Locals, slot: u16) -> RuntimeValue {
        match locals.get(slot) {
            StackValue::Value(value) => value,
            StackValue::Closure(_) => panic!("expected a runtime value"),
        }
    }

    #[test]
    fn fixed_required_binder_handles_explicit_and_implicit_self_arguments() {
        let chunks = Shared::new(Vec::new());
        let explicit_locals = Locals::boxed(3);
        bind_fixed_required_params(2, &mut vec![number(3), number(4)], &explicit_locals, &chunks).unwrap();
        assert_eq!(value_at(&explicit_locals, 1), RuntimeValue::Number(3.into()));
        assert_eq!(value_at(&explicit_locals, 2), RuntimeValue::Number(4.into()));

        let implicit_locals = Locals::boxed(3);
        implicit_locals.set(0, number(10));
        bind_fixed_required_params(2, &mut vec![number(4)], &implicit_locals, &chunks).unwrap();
        assert_eq!(value_at(&implicit_locals, 1), RuntimeValue::Number(10.into()));
        assert_eq!(value_at(&implicit_locals, 2), RuntimeValue::Number(4.into()));
    }

    #[test]
    fn fixed_required_binder_rejects_invalid_arity() {
        let chunks = Shared::new(Vec::new());
        let locals = Locals::boxed(1);
        assert!(matches!(
            bind_fixed_required_params(0, &mut vec![number(1)], &locals, &chunks),
            Err(VmError::ArityMismatch { expected: 0, actual: 1 })
        ));
    }
}
