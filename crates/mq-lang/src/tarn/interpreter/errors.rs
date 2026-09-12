//! `VmError`: Tarn's runtime error type, its `Display`/`RuntimeError` conversions, and the
//! small helpers (`locate`, `error_dict`, `error_message`, `flow_break_value`/`flow_continue`)
//! built on top of it.
use crate::DictMap;
use crate::ast::TokenId;
use crate::runtime::builtin;
use crate::runtime::runtime_value::RuntimeValue;
use crate::tarn::bytecode::Chunk;
use crate::{Ident, Shared};
use std::fmt;
use std::time::Duration;

#[derive(Debug)]
pub(crate) enum VmError {
    Builtin(builtin::Error),
    Host(Ident, String),
    ZeroDivision,
    NotCallable,
    EnvNotFound(String),
    UndefinedGlobal(String),
    #[cfg(feature = "debugger")]
    Debugger(String),
    Corrupt(&'static str),
    ArityMismatch {
        expected: usize,
        actual: usize,
    },
    /// Internal control flow emitted by a `break` inside a nested `try` chunk.
    FlowBreak(Option<RuntimeValue>),
    /// Internal control flow emitted by a `continue` inside a nested `try` chunk.
    FlowContinue,
    DestructuringFailed,
    InvalidForeachTarget(String),
    Timeout(Duration),
    RecursionError(u32),
    /// `next()` was called on a coroutine already being driven by an outer `next()` higher on
    /// the Rust call stack.
    CoroutineReentrant,
    /// `next()` on a coroutine that previously failed re-raises the same error. Carries the
    /// coroutine's own token arena, since the resuming call's arena may be a different one.
    CoroutineFailed(Shared<VmError>, crate::TokenArena),
    Located(Box<VmError>, TokenId),
}

impl fmt::Display for VmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VmError::Builtin(e) => write!(f, "{e}"),
            VmError::Host(name, msg) => write!(f, "error in host function \"{name}\": {msg}"),
            VmError::ZeroDivision => write!(f, "division by zero"),
            VmError::NotCallable => write!(f, "value is not callable"),
            VmError::EnvNotFound(name) => write!(f, "environment variable not found: {name}"),
            VmError::UndefinedGlobal(name) => write!(f, "undefined identifier `{name}`"),
            #[cfg(feature = "debugger")]
            VmError::Debugger(message) => write!(f, "debugger expression failed: {message}"),
            VmError::Corrupt(what) => write!(f, "corrupt bytecode: {what}"),
            VmError::ArityMismatch { expected, actual } => {
                write!(f, "expected {expected} argument(s), got {actual}")
            }
            VmError::FlowBreak(_) => write!(f, "break outside a loop"),
            VmError::FlowContinue => write!(f, "continue outside a loop"),
            VmError::DestructuringFailed => write!(f, "destructuring pattern did not match value"),
            VmError::InvalidForeachTarget(repr) => write!(f, "invalid types for \"foreach\", got {repr}"),
            VmError::Timeout(d) => write!(f, "execution timed out after {:.3}s", d.as_secs_f64()),
            VmError::RecursionError(max) => write!(f, "maximum recursion depth exceeded ({max})"),
            VmError::CoroutineReentrant => write!(f, "coroutine is already running"),
            VmError::CoroutineFailed(inner, _) => write!(f, "{inner}"),
            VmError::Located(inner, _) => write!(f, "{inner}"),
        }
    }
}

impl VmError {
    pub(crate) fn token_id(&self) -> Option<TokenId> {
        match self {
            VmError::Located(_, token_id) => Some(*token_id),
            _ => None,
        }
    }

    /// Maps to the common `RuntimeError`, reusing its `Display` text instead of duplicating
    /// each variant's wording.
    pub(crate) fn to_runtime_error(
        &self,
        token: crate::Token,
        token_id: TokenId,
        token_arena: crate::TokenArena,
    ) -> crate::error::runtime::RuntimeError {
        use crate::error::runtime::RuntimeError;
        match self {
            VmError::Builtin(e) => e.to_runtime_error(token_id, token_arena),
            VmError::Host(name, msg) => {
                RuntimeError::HostFunctionError(token, name.to_string().into_boxed_str(), msg.clone().into_boxed_str())
            }
            VmError::ZeroDivision => RuntimeError::ZeroDivision(token),
            VmError::NotCallable => RuntimeError::InvalidDefinition(token, "value is not callable".to_string()),
            VmError::EnvNotFound(name) => RuntimeError::EnvNotFound(token, name.clone().into()),
            VmError::UndefinedGlobal(name) => RuntimeError::UndefinedReference(token, name.clone(), Box::new([])),
            #[cfg(feature = "debugger")]
            VmError::Debugger(message) => RuntimeError::Runtime(token, message.clone()),
            VmError::ArityMismatch { expected, actual } => RuntimeError::InvalidNumberOfArguments {
                token,
                name: String::new(),
                expected: *expected,
                actual: *actual,
            },
            VmError::FlowBreak(_) => RuntimeError::Runtime(token, "break outside a loop".to_string()),
            VmError::FlowContinue => RuntimeError::Runtime(token, "continue outside a loop".to_string()),
            VmError::DestructuringFailed => RuntimeError::DestructuringFailed(token),
            VmError::InvalidForeachTarget(repr) => RuntimeError::InvalidTypes {
                token,
                name: crate::TokenKind::Foreach.to_string(),
                args: vec![repr.clone().into()],
            },
            VmError::Timeout(d) => RuntimeError::Timeout(*d),
            VmError::RecursionError(max) => RuntimeError::RecursionError(*max),
            VmError::Corrupt(what) => RuntimeError::Runtime(token, format!("corrupt bytecode: {what}")),
            VmError::CoroutineReentrant => RuntimeError::Runtime(token, "coroutine is already running".to_string()),
            VmError::CoroutineFailed(inner, origin_arena) => {
                inner.to_runtime_error(token, token_id, Shared::clone(origin_arena))
            }
            VmError::Located(inner, token_id) => {
                let token_id = *token_id;
                let token = resolve_token(&token_arena, token_id);
                inner.to_runtime_error(token, token_id, token_arena)
            }
        }
    }
}

/// Resolves `token_id` in `token_arena`, falling back to a placeholder token if out of range.
fn resolve_token(token_arena: &crate::TokenArena, token_id: TokenId) -> crate::Token {
    #[cfg(not(feature = "sync"))]
    let found = token_arena.borrow().get(token_id).cloned();
    #[cfg(feature = "sync")]
    let found = token_arena.read().unwrap().get(token_id).cloned();

    found.map_or_else(
        || crate::Token {
            range: crate::Range::default(),
            kind: crate::TokenKind::Eof,
            module_id: crate::ArenaId::new(0),
        },
        |token| (*token).clone(),
    )
}

pub(super) fn locate(chunk: &Chunk, ip: usize, e: VmError) -> VmError {
    match chunk.token_at(ip.saturating_sub(1)) {
        Some(token_id) => VmError::Located(Box::new(e), token_id),
        None => e,
    }
}

impl std::error::Error for VmError {}

impl From<builtin::Error> for VmError {
    fn from(e: builtin::Error) -> Self {
        VmError::Builtin(e)
    }
}

pub(super) type VmResult<T> = Result<T, VmError>;

pub(super) fn error_dict(e: &VmError) -> RuntimeValue {
    let mut map = DictMap::default();
    map.insert(
        Ident::new("message"),
        RuntimeValue::String(Shared::new(error_message(e))),
    );
    RuntimeValue::Dict(Shared::new(map))
}

// Throwaway token/arena for `VmError::to_runtime_error` where no real one is available.
fn placeholder_token_context() -> (crate::Token, TokenId, crate::TokenArena) {
    let mut arena = crate::Arena::new(1);
    let token = crate::Token {
        range: crate::Range::default(),
        kind: crate::TokenKind::Eof,
        module_id: crate::ArenaId::new(0),
    };
    let token_id = arena.alloc(Shared::new(token.clone()));
    let token_arena: crate::TokenArena = Shared::new(crate::SharedCell::new(arena));
    (token, token_id, token_arena)
}

pub(super) fn error_message(e: &VmError) -> String {
    // Strip `Located`'s real token_id first: `to_runtime_error` would try to resolve it
    // against the placeholder arena below, which only holds the placeholder token.
    match e {
        VmError::Located(inner, _) => error_message(inner),
        other => {
            let (token, token_id, token_arena) = placeholder_token_context();
            other.to_runtime_error(token, token_id, token_arena).to_string()
        }
    }
}

pub(super) fn flow_break_value(e: &VmError) -> Option<Option<RuntimeValue>> {
    match e {
        VmError::FlowBreak(value) => Some(value.clone()),
        VmError::Located(inner, _) => flow_break_value(inner),
        _ => None,
    }
}

pub(super) fn flow_continue(e: &VmError) -> bool {
    match e {
        VmError::FlowContinue => true,
        VmError::Located(inner, _) => flow_continue(inner),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // One case per `builtin::Error` variant; see `all_builtin_error_variants_are_covered`.
    #[rstest::rstest]
    #[case::user_defined(builtin::Error::UserDefined("boom".to_string()), "boom")]
    #[case::invalid_base64_string(
        builtin::Error::InvalidBase64String({
            use base64::Engine;
            base64::prelude::BASE64_STANDARD.decode("not valid base64!!!").unwrap_err()
        }),
        "Invalid base64 string"
    )]
    #[case::not_defined(
        builtin::Error::NotDefined("f".to_string(), vec!["g".to_string()]),
        "\"f\" is not defined"
    )]
    #[case::undefined_reference(
        builtin::Error::UndefinedReference("r".to_string(), vec![]),
        "\"r\" is not defined"
    )]
    #[case::invalid_date_time_format(
        builtin::Error::InvalidDateTimeFormat("%Q".to_string()),
        "Unable to format date time, %Q"
    )]
    #[case::invalid_types(
        builtin::Error::InvalidTypes("f".to_string(), vec![RuntimeValue::Number(1.into())]),
        "Invalid types for \"f\", got number"
    )]
    #[case::invalid_types_multiple_args(
        builtin::Error::InvalidTypes(
            "f".to_string(),
            vec![RuntimeValue::Number(1.into()), RuntimeValue::Boolean(true)],
        ),
        "Invalid types for \"f\", got number, bool"
    )]
    #[case::invalid_number_of_arguments(
        builtin::Error::InvalidNumberOfArguments("f".to_string(), 2, 1),
        "Invalid number of arguments in \"f\", expected 2, got 1"
    )]
    #[case::invalid_regular_expression(
        builtin::Error::InvalidRegularExpression("(".to_string()),
        "Invalid regular expression \"(\""
    )]
    #[case::runtime(
        builtin::Error::Runtime("something went wrong".to_string()),
        "Runtime error: something went wrong"
    )]
    #[case::zero_division(builtin::Error::ZeroDivision, "Division by zero")]
    #[case::assign_to_immutable(
        builtin::Error::AssignToImmutable("x".to_string()),
        "Cannot assign to immutable variable \"x\""
    )]
    #[case::undefined_variable(
        builtin::Error::UndefinedVariable("x".to_string()),
        "Undefined variable \"x\""
    )]
    #[case::invalid_convert(
        builtin::Error::InvalidConvert("bogus".to_string()),
        "Invalid convert: bogus"
    )]
    fn builtin_error_message_matches_runtime_error_display(#[case] error: builtin::Error, #[case] expected: &str) {
        assert_eq!(error_message(&VmError::Builtin(error)), expected);
    }

    /// Forces a compile error if a `builtin::Error` variant is missing a case above.
    #[allow(dead_code)]
    fn all_builtin_error_variants_are_covered(e: builtin::Error) {
        match e {
            builtin::Error::UserDefined(_)
            | builtin::Error::InvalidBase64String(_)
            | builtin::Error::NotDefined(_, _)
            | builtin::Error::UndefinedReference(_, _)
            | builtin::Error::InvalidDateTimeFormat(_)
            | builtin::Error::InvalidTypes(_, _)
            | builtin::Error::InvalidNumberOfArguments(_, _, _)
            | builtin::Error::InvalidRegularExpression(_)
            | builtin::Error::Runtime(_)
            | builtin::Error::ZeroDivision
            | builtin::Error::AssignToImmutable(_)
            | builtin::Error::UndefinedVariable(_)
            | builtin::Error::InvalidConvert(_) => {}
        }
    }

    // One case per bare `VmError` variant; see `all_vm_error_variants_are_covered`.
    #[rstest::rstest]
    #[case::host(VmError::Host(Ident::new("f"), "boom".to_string()), "Error in host function \"f\": boom")]
    #[case::zero_division(VmError::ZeroDivision, "Division by zero")]
    #[case::not_callable(VmError::NotCallable, "Invalid definition for \"value is not callable\"")]
    #[case::env_not_found(VmError::EnvNotFound("HOME".to_string()), "Environment variable `HOME` not found")]
    #[case::undefined_global(VmError::UndefinedGlobal("x".to_string()), "\"x\" is not defined")]
    #[cfg_attr(
        feature = "debugger",
        case::debugger(VmError::Debugger("bad condition".to_string()), "Runtime error: bad condition")
    )]
    #[case::arity_mismatch(
        VmError::ArityMismatch { expected: 2, actual: 1 },
        "Invalid number of arguments in \"\", expected 2, got 1"
    )]
    #[case::flow_break(VmError::FlowBreak(None), "Runtime error: break outside a loop")]
    #[case::flow_continue(VmError::FlowContinue, "Runtime error: continue outside a loop")]
    #[case::destructuring_failed(VmError::DestructuringFailed, "Destructuring pattern did not match value")]
    #[case::invalid_foreach_target(
        VmError::InvalidForeachTarget("number".to_string()),
        "Invalid types for \"foreach\", got number"
    )]
    #[case::timeout(VmError::Timeout(Duration::from_secs(1)), "Execution timed out after 1.000s")]
    #[case::recursion_error(VmError::RecursionError(100), "Maximum recursion depth exceeded (100)")]
    #[case::corrupt(VmError::Corrupt("bad opcode"), "Runtime error: corrupt bytecode: bad opcode")]
    #[case::located_unwraps_to_the_inner_message(
        VmError::Located(Box::new(VmError::ZeroDivision), TokenId::new(0)),
        "Division by zero"
    )]
    #[case::coroutine_reentrant(VmError::CoroutineReentrant, "Runtime error: coroutine is already running")]
    #[case::coroutine_failed_unwraps_to_the_inner_message(
        VmError::CoroutineFailed(
            Shared::new(VmError::ZeroDivision),
            Shared::new(crate::SharedCell::new(crate::arena::Arena::new(1))),
        ),
        "Division by zero"
    )]
    fn vm_error_message_matches_runtime_error_display(#[case] error: VmError, #[case] expected: &str) {
        assert_eq!(error_message(&error), expected);
    }

    /// Forces a compile error if a `VmError` variant is missing a case above.
    #[allow(dead_code)]
    fn all_vm_error_variants_are_covered(e: VmError) {
        match e {
            #[cfg(feature = "debugger")]
            VmError::Debugger(_) => {}
            VmError::Builtin(_)
            | VmError::Host(_, _)
            | VmError::ZeroDivision
            | VmError::NotCallable
            | VmError::EnvNotFound(_)
            | VmError::UndefinedGlobal(_)
            | VmError::Corrupt(_)
            | VmError::ArityMismatch { .. }
            | VmError::FlowBreak(_)
            | VmError::FlowContinue
            | VmError::DestructuringFailed
            | VmError::InvalidForeachTarget(_)
            | VmError::Timeout(_)
            | VmError::RecursionError(_)
            | VmError::CoroutineReentrant
            | VmError::CoroutineFailed(_, _)
            | VmError::Located(_, _) => {}
        }
    }

    /// Guards against the two error-message paths drifting apart again: the VM's own
    /// `1 / 0` fast path used to report "division by zero" (lowercase) via `VmError`'s own
    /// `Display`, while the user-facing `RuntimeError` reports "Division by zero".
    #[test]
    fn zero_division_message_matches_through_a_real_try_catch() {
        let token_arena = Shared::new(crate::SharedCell::new(crate::arena::Arena::new(100)));
        let program = crate::parse("try: 1 / 0 catch(e): get(e, \"message\");", Shared::clone(&token_arena)).unwrap();
        let compiled = super::super::super::compiler::compile_program(
            &program,
            token_arena,
            crate::ModuleLoader::new(crate::module::resolver::std_resolver::StdModuleResolver),
        )
        .unwrap();
        let result = super::super::run_with_globals(
            &compiled,
            RuntimeValue::None,
            &crate::runtime::host::HostFunctions::default(),
            None,
            super::super::super::Options::default().max_call_stack_depth,
            &[],
        )
        .unwrap();
        assert_eq!(
            result,
            RuntimeValue::String(Shared::new("Division by zero".to_string()))
        );
    }

    /// A coroutine's failure must resolve against its own (carried) arena, not whichever
    /// unrelated, smaller arena happens to be resuming it. The latter used to panic by indexing
    /// past its end.
    #[test]
    fn coroutine_failed_resolves_against_its_own_arena_not_the_resuming_ones() {
        let origin_arena = Shared::new(crate::SharedCell::new(crate::arena::Arena::new(50)));
        let mut origin_token_id = TokenId::new(0);
        for line in 1..=40 {
            origin_token_id = crate::token_alloc(
                &origin_arena,
                &Shared::new(crate::Token {
                    range: crate::Range {
                        start: crate::Position { line, column: 1 },
                        end: crate::Position { line, column: 1 },
                    },
                    kind: crate::TokenKind::Eof,
                    module_id: crate::ArenaId::new(0),
                }),
            );
        }
        let coroutine_failed = VmError::CoroutineFailed(
            Shared::new(VmError::Located(Box::new(VmError::ZeroDivision), origin_token_id)),
            origin_arena,
        );

        // Deliberately tiny and unrelated: `origin_token_id` (40) is out of range here.
        let (token, token_id, resuming_arena) = placeholder_token_context();
        let result = coroutine_failed.to_runtime_error(token, token_id, resuming_arena);
        assert_eq!(
            result.token().unwrap().range.start.line,
            40,
            "must resolve against origin_arena"
        );
    }
}
