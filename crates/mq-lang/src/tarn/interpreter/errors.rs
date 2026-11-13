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

/// One active VM frame captured for an opt-in uncaught-error stack trace.
#[derive(Debug, Clone)]
pub(crate) struct StackTraceFrame {
    pub(crate) function_name: Option<Ident>,
    pub(crate) token_id: Option<TokenId>,
}

/// An uncaught error together with the call stack it unwound through.
#[derive(Debug)]
pub(crate) struct TracedError {
    pub(crate) error: VmError,
    pub(crate) frames: Box<[StackTraceFrame]>,
}

/// VM error. Payloads are boxed so that `VmResult<T>` stays small on the hot path;
/// see `vm_error_stays_compact`.
#[derive(Debug)]
pub(crate) enum VmError {
    StackTrace(Box<TracedError>),
    Builtin(Box<builtin::Error>),
    Host(Ident, Box<str>),
    ZeroDivision,
    /// Carries the type name of the value that was called.
    NotCallable(Box<str>),
    /// `next`/`send` was given a value that is not a coroutine.
    InvalidResumeTarget {
        /// `true` for `send`, `false` for `next`.
        send: bool,
        actual: Box<str>,
    },
    EnvNotFound(Box<str>),
    EnvNotAllowed(Box<str>),
    UndefinedGlobal(Box<str>),
    #[cfg(feature = "debugger")]
    Debugger(Box<str>),
    Corrupt(&'static str),
    ArityMismatch {
        /// Callee's source-level name, `None` for anonymous functions.
        name: Option<Ident>,
        expected: u32,
        actual: u32,
    },
    /// Internal control flow emitted by a `break` inside a nested `try` chunk.
    FlowBreak(Option<RuntimeValue>),
    /// Internal control flow emitted by a `continue` inside a nested `try` chunk.
    FlowContinue,
    DestructuringFailed,
    InvalidForeachTarget(Box<str>),
    Timeout(Duration),
    RecursionError(u32),
    OperandStackLimit,
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
            VmError::StackTrace(traced) => write!(f, "{}", traced.error),
            VmError::Builtin(e) => write!(f, "{e}"),
            VmError::Host(name, msg) => write!(f, "error in host function \"{name}\": {msg}"),
            VmError::ZeroDivision => write!(f, "division by zero"),
            VmError::NotCallable(actual) => write!(f, "cannot call a value of type {actual}"),
            VmError::InvalidResumeTarget { send, actual } => {
                write!(f, "invalid types for \"{}\", got {actual}", resume_name(*send))
            }
            VmError::EnvNotFound(name) => write!(f, "environment variable not found: {name}"),
            VmError::EnvNotAllowed(name) => write!(f, "environment variable not allowed: {name}"),
            VmError::UndefinedGlobal(name) => write!(f, "undefined identifier `{name}`"),
            #[cfg(feature = "debugger")]
            VmError::Debugger(message) => write!(f, "debugger expression failed: {message}"),
            VmError::Corrupt(what) => write!(f, "corrupt bytecode: {what}"),
            VmError::ArityMismatch { expected, actual, .. } => {
                write!(f, "expected {expected} argument(s), got {actual}")
            }
            VmError::FlowBreak(_) => write!(f, "break outside a loop"),
            VmError::FlowContinue => write!(f, "continue outside a loop"),
            VmError::DestructuringFailed => write!(f, "destructuring pattern did not match value"),
            VmError::InvalidForeachTarget(repr) => write!(f, "invalid types for \"foreach\", got {repr}"),
            VmError::Timeout(d) => write!(f, "execution timed out after {:.3}s", d.as_secs_f64()),
            VmError::RecursionError(max) => write!(f, "maximum recursion depth exceeded ({max})"),
            VmError::OperandStackLimit => write!(f, "operand stack exceeds the VM limit"),
            VmError::CoroutineReentrant => write!(f, "coroutine is already running"),
            VmError::CoroutineFailed(inner, _) => write!(f, "{inner}"),
            VmError::Located(inner, _) => write!(f, "{inner}"),
        }
    }
}

fn resume_name(send: bool) -> &'static str {
    use crate::ast::constants::builtins;
    if send { builtins::SEND } else { builtins::NEXT }
}

impl VmError {
    /// Kept out of line so the `next`/`send` fast path carries no error-construction code.
    #[cold]
    #[inline(never)]
    pub(crate) fn invalid_resume_target(send: bool, actual: &RuntimeValue) -> Self {
        VmError::InvalidResumeTarget {
            send,
            actual: actual.name().into(),
        }
    }

    #[cold]
    #[inline(never)]
    pub(crate) fn not_callable(actual: &RuntimeValue) -> Self {
        VmError::NotCallable(actual.name().into())
    }

    /// Builds an arity error, saturating counts that do not fit `u32`.
    pub(crate) fn arity_mismatch(name: Option<Ident>, expected: usize, actual: usize) -> Self {
        VmError::ArityMismatch {
            name,
            expected: u32::try_from(expected).unwrap_or(u32::MAX),
            actual: u32::try_from(actual).unwrap_or(u32::MAX),
        }
    }

    pub(crate) fn token_id(&self) -> Option<TokenId> {
        match self {
            VmError::StackTrace(traced) => traced.error.token_id(),
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
            VmError::StackTrace(traced) => crate::error::runtime::RuntimeError::WithStackTrace {
                source: Box::new(
                    traced
                        .error
                        .to_runtime_error(token, token_id, Shared::clone(&token_arena)),
                ),
                trace: format_stack_trace(&traced.frames, token_arena).into_boxed_str(),
            },
            VmError::Builtin(e) => e.to_runtime_error(token_id, token_arena),
            VmError::Host(name, msg) => {
                RuntimeError::HostFunctionError(token, name.to_string().into_boxed_str(), msg.clone())
            }
            VmError::ZeroDivision => RuntimeError::ZeroDivision(token),
            VmError::NotCallable(actual) => RuntimeError::NotCallable(token, actual.to_string()),
            VmError::InvalidResumeTarget { send, actual } => RuntimeError::InvalidTypes {
                token,
                name: resume_name(*send).to_string(),
                args: vec![actual.to_string().into()],
            },
            VmError::EnvNotFound(name) => RuntimeError::EnvNotFound(token, name.as_ref().into()),
            VmError::EnvNotAllowed(name) => RuntimeError::EnvNotAllowed(token, name.as_ref().into()),
            VmError::UndefinedGlobal(name) => RuntimeError::UndefinedReference(token, name.to_string(), Box::new([])),
            #[cfg(feature = "debugger")]
            VmError::Debugger(message) => RuntimeError::Runtime(token, message.to_string()),
            VmError::ArityMismatch { name, expected, actual } => RuntimeError::InvalidNumberOfArguments {
                token,
                name: name.map_or_else(|| "<anonymous>".to_string(), |name| name.to_string()),
                expected: *expected as usize,
                actual: *actual as usize,
            },
            VmError::FlowBreak(_) => RuntimeError::Runtime(token, "break outside a loop".to_string()),
            VmError::FlowContinue => RuntimeError::Runtime(token, "continue outside a loop".to_string()),
            VmError::DestructuringFailed => RuntimeError::DestructuringFailed(token),
            VmError::InvalidForeachTarget(repr) => RuntimeError::InvalidTypes {
                token,
                name: crate::TokenKind::Foreach.to_string(),
                args: vec![repr.to_string().into()],
            },
            VmError::Timeout(d) => RuntimeError::Timeout(*d),
            VmError::RecursionError(max) => RuntimeError::RecursionError(*max),
            VmError::OperandStackLimit => {
                RuntimeError::Runtime(token, "operand stack exceeds the VM limit".to_string())
            }
            VmError::Corrupt(what) => RuntimeError::Runtime(token, format!("corrupt bytecode: {what}")),
            VmError::CoroutineReentrant => RuntimeError::Runtime(token, "coroutine is already running".to_string()),
            VmError::CoroutineFailed(inner, origin_arena) => {
                inner.to_runtime_error(token, token_id, Shared::clone(origin_arena))
            }
            VmError::Located(inner, token_id) => {
                let token_id = *token_id;
                let token = crate::get_token(&token_arena, token_id);
                inner.to_runtime_error(token, token_id, token_arena)
            }
        }
    }
}

/// Frames shown at each end of a stack trace. A runaway recursion can be a million frames deep,
/// so the middle of a longer trace is replaced by a count.
const STACK_TRACE_EDGE_FRAMES: usize = 25;

#[cold]
fn format_stack_trace(frames: &[StackTraceFrame], token_arena: crate::TokenArena) -> String {
    use std::fmt::Write;

    let write_frame = |trace: &mut String, frame: &StackTraceFrame| {
        let name = frame
            .function_name
            .map(|name| name.to_string())
            .unwrap_or_else(|| "<main>".to_string());
        match frame.token_id {
            Some(token_id) => {
                let (range, _) = crate::token_location(&token_arena, token_id);
                let _ = write!(trace, "\n  at {name} ({}:{})", range.start.line, range.start.column + 1);
            }
            None => {
                let _ = write!(trace, "\n  at {name}");
            }
        }
    };

    let mut trace = String::from("stack trace:");
    if frames.len() <= 2 * STACK_TRACE_EDGE_FRAMES {
        frames.iter().for_each(|frame| write_frame(&mut trace, frame));
    } else {
        let (head, rest) = frames.split_at(STACK_TRACE_EDGE_FRAMES);
        let (omitted, tail) = rest.split_at(rest.len() - STACK_TRACE_EDGE_FRAMES);
        head.iter().for_each(|frame| write_frame(&mut trace, frame));
        let plural = if omitted.len() == 1 { "" } else { "s" };
        let _ = write!(trace, "\n  ... {} more frame{plural} ...", omitted.len());
        tail.iter().for_each(|frame| write_frame(&mut trace, frame));
    }
    trace
}

impl VmError {
    /// Fills in the callee name of an arity error raised without access to the callee chunk.
    pub(super) fn with_arity_name(self, callee: Option<Ident>) -> Self {
        match self {
            VmError::ArityMismatch {
                name: None,
                expected,
                actual,
            } => VmError::ArityMismatch {
                name: callee,
                expected,
                actual,
            },
            e => e,
        }
    }
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
        VmError::Builtin(Box::new(e))
    }
}

pub(super) type VmResult<T> = Result<T, VmError>;

pub(super) fn error_dict(e: &VmError) -> RuntimeValue {
    let message = error_message(e);
    let value = thrown_value(e).unwrap_or_else(|| RuntimeValue::String(Shared::new(message.clone())));
    let mut map = DictMap::default();
    map.insert(Ident::new("message"), RuntimeValue::String(Shared::new(message)));
    map.insert(Ident::new("value"), value);
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
    let token_id = arena.alloc(token.clone());
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

/// The value passed to `error` when it was not a string.
fn thrown_value(e: &VmError) -> Option<RuntimeValue> {
    match e {
        VmError::Builtin(e) => match &**e {
            builtin::Error::Thrown(value) => Some(value.clone()),
            _ => None,
        },
        VmError::Located(inner, _) => thrown_value(inner),
        VmError::StackTrace(traced) => thrown_value(&traced.error),
        VmError::CoroutineFailed(inner, _) => thrown_value(inner),
        _ => None,
    }
}

pub(super) fn flow_break_value(e: &VmError) -> Option<Option<RuntimeValue>> {
    match e {
        VmError::FlowBreak(value) => Some(value.clone()),
        VmError::Located(inner, _) => flow_break_value(inner),
        _ => None,
    }
}

/// `halt` and a timeout must reach the host, so `try` never catches them. Catching a timeout
/// would let a query keep running past its deadline.
pub(super) fn is_uncatchable(e: &VmError) -> bool {
    match e {
        VmError::Builtin(e) => matches!(**e, builtin::Error::Halt(_)),
        VmError::Timeout(_) => true,
        VmError::Located(inner, _) => is_uncatchable(inner),
        VmError::StackTrace(traced) => is_uncatchable(&traced.error),
        VmError::CoroutineFailed(inner, _) => is_uncatchable(inner),
        _ => false,
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
    use crate::parse;

    // One case per `builtin::Error` variant; see `all_builtin_error_variants_are_covered`.
    #[rstest::rstest]
    #[case::user_defined(builtin::Error::UserDefined("boom".to_string()), "boom")]
    #[case::thrown(builtin::Error::Thrown(RuntimeValue::Number(1.into())), "1")]
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
    #[case::invalid_convert(
        builtin::Error::InvalidConvert("bogus".to_string()),
        "Invalid convert: bogus"
    )]
    #[case::halt(builtin::Error::Halt(2), "Halted with exit code 2")]
    fn builtin_error_message_matches_runtime_error_display(#[case] error: builtin::Error, #[case] expected: &str) {
        assert_eq!(error_message(&VmError::from(error)), expected);
    }

    /// Forces a compile error if a `builtin::Error` variant is missing a case above.
    #[test]
    fn all_builtin_error_variants_are_covered() {
        let _ = |e: builtin::Error| match e {
            builtin::Error::UserDefined(_)
            | builtin::Error::Thrown(_)
            | builtin::Error::InvalidBase64String(_)
            | builtin::Error::NotDefined(_, _)
            | builtin::Error::InvalidDateTimeFormat(_)
            | builtin::Error::InvalidTypes(_, _)
            | builtin::Error::InvalidNumberOfArguments(_, _, _)
            | builtin::Error::InvalidRegularExpression(_)
            | builtin::Error::Runtime(_)
            | builtin::Error::ZeroDivision
            | builtin::Error::InvalidConvert(_)
            | builtin::Error::Halt(_) => {}
        };
    }

    // One case per bare `VmError` variant; see `all_vm_error_variants_are_covered`.
    #[rstest::rstest]
    #[case::host(VmError::Host(Ident::new("f"), "boom".into()), "Error in host function \"f\": boom")]
    #[case::zero_division(VmError::ZeroDivision, "Division by zero")]
    #[case::not_callable(VmError::NotCallable("number".into()), "Cannot call a value of type number")]
    #[case::invalid_resume_target(
        VmError::InvalidResumeTarget { send: false, actual: "number".into() },
        "Invalid types for \"next\", got number"
    )]
    #[case::env_not_found(VmError::EnvNotFound("HOME".into()), "Environment variable `HOME` not found")]
    #[case::env_not_allowed(
        VmError::EnvNotAllowed("HOME".into()),
        "Access to environment variable `HOME` is not allowed"
    )]
    #[case::undefined_global(VmError::UndefinedGlobal("x".into()), "\"x\" is not defined")]
    #[cfg_attr(
        feature = "debugger",
        case::debugger(VmError::Debugger("bad condition".into()), "Runtime error: bad condition")
    )]
    #[case::arity_mismatch(
        VmError::ArityMismatch { name: None, expected: 2, actual: 1 },
        "Invalid number of arguments in \"<anonymous>\", expected 2, got 1"
    )]
    #[case::flow_break(VmError::FlowBreak(None), "Runtime error: break outside a loop")]
    #[case::flow_continue(VmError::FlowContinue, "Runtime error: continue outside a loop")]
    #[case::destructuring_failed(VmError::DestructuringFailed, "Destructuring pattern did not match value")]
    #[case::invalid_foreach_target(
        VmError::InvalidForeachTarget("number".into()),
        "Invalid types for \"foreach\", got number"
    )]
    #[case::timeout(VmError::Timeout(Duration::from_secs(1)), "Execution timed out after 1.000s")]
    #[case::recursion_error(VmError::RecursionError(100), "Maximum recursion depth exceeded (100)")]
    #[case::operand_stack_limit(VmError::OperandStackLimit, "Runtime error: operand stack exceeds the VM limit")]
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
    #[test]
    fn all_vm_error_variants_are_covered() {
        let _ = |e: VmError| match e {
            VmError::StackTrace(_) => {}
            #[cfg(feature = "debugger")]
            VmError::Debugger(_) => {}
            VmError::Builtin(_)
            | VmError::Host(_, _)
            | VmError::ZeroDivision
            | VmError::NotCallable(_)
            | VmError::InvalidResumeTarget { .. }
            | VmError::EnvNotFound(_)
            | VmError::EnvNotAllowed(_)
            | VmError::UndefinedGlobal(_)
            | VmError::Corrupt(_)
            | VmError::ArityMismatch { .. }
            | VmError::FlowBreak(_)
            | VmError::FlowContinue
            | VmError::DestructuringFailed
            | VmError::InvalidForeachTarget(_)
            | VmError::Timeout(_)
            | VmError::RecursionError(_)
            | VmError::OperandStackLimit
            | VmError::CoroutineReentrant
            | VmError::CoroutineFailed(_, _)
            | VmError::Located(_, _) => {}
        };
    }

    /// Guards against the two error-message paths drifting apart again: the VM's own
    /// `1 / 0` fast path used to report "division by zero" (lowercase) via `VmError`'s own
    /// `Display`, while the user-facing `RuntimeError` reports "Division by zero".
    fn run_query(query: &str) -> RuntimeValue {
        let token_arena = Shared::new(crate::SharedCell::new(crate::arena::Arena::new(100)));
        let program = parse(query, Shared::clone(&token_arena)).unwrap();
        let compiled = super::super::super::compiler::compile_program(
            &program,
            token_arena,
            crate::ModuleLoader::new(crate::module::resolver::std_resolver::StdModuleResolver),
        )
        .unwrap();
        super::super::run_with_globals(
            &compiled,
            RuntimeValue::None,
            &crate::runtime::host::HostFunctions::default(),
            None,
            super::super::super::Options::default().max_call_stack_depth,
            &[],
        )
        .unwrap()
    }

    #[test]
    fn zero_division_message_matches_through_a_real_try_catch() {
        assert_eq!(
            run_query("try: 1 / 0 catch(e): get(e, \"message\");"),
            RuntimeValue::String(Shared::new("Division by zero".to_string()))
        );
    }

    #[rstest::rstest]
    #[case::string("error(\"boom\")", "\"boom\"")]
    #[case::number("error(42)", "42")]
    #[case::none("error(None)", "None")]
    #[case::array("error([1, \"a\"])", "[1, \"a\"]")]
    #[case::dict("error({\"code\": 7})", "{\"code\": 7}")]
    #[case::nested_in_function("(fn(x): error({\"x\": x});)(3)", "{\"x\": 3}")]
    fn catch_binder_exposes_the_value_passed_to_error(#[case] raise: &str, #[case] expected: &str) {
        let caught = run_query(&format!("try: {raise} catch(e): get(e, \"value\");"));
        assert_eq!(caught, run_query(expected));
    }

    #[test]
    fn catch_binder_value_of_a_non_user_error_is_its_message() {
        assert_eq!(
            run_query("try: 1 / 0 catch(e): get(e, \"value\");"),
            RuntimeValue::String(Shared::new("Division by zero".to_string()))
        );
    }

    #[test]
    fn thrown_value_message_is_its_display_form() {
        assert_eq!(
            run_query("try: error([1, 2]) catch(e): get(e, \"message\");"),
            RuntimeValue::String(Shared::new("[1, 2]".to_string()))
        );
    }

    fn trace_of(frame_count: usize) -> String {
        let frames: Vec<StackTraceFrame> = (0..frame_count)
            .map(|index| StackTraceFrame {
                function_name: Some(Ident::new(&format!("f{index}"))),
                token_id: None,
            })
            .collect();
        format_stack_trace(
            &frames,
            Shared::new(crate::SharedCell::new(crate::arena::Arena::new(1))),
        )
    }

    #[rstest::rstest]
    #[case::empty(0)]
    #[case::one(1)]
    #[case::exactly_the_edges(2 * STACK_TRACE_EDGE_FRAMES)]
    fn a_short_stack_trace_lists_every_frame(#[case] frame_count: usize) {
        let trace = trace_of(frame_count);
        assert_eq!(trace.lines().count(), frame_count + 1);
        assert!(!trace.contains("more frames"));
    }

    #[test]
    fn a_long_stack_trace_keeps_both_ends_and_counts_the_rest() {
        let total = 1_000_000;
        let trace = trace_of(total);
        let lines: Vec<&str> = trace.lines().collect();

        assert_eq!(lines.len(), 1 + 2 * STACK_TRACE_EDGE_FRAMES + 1);
        assert_eq!(lines[0], "stack trace:");
        assert_eq!(lines[1], "  at f0");
        assert_eq!(
            lines[STACK_TRACE_EDGE_FRAMES],
            format!("  at f{}", STACK_TRACE_EDGE_FRAMES - 1)
        );
        assert_eq!(
            lines[STACK_TRACE_EDGE_FRAMES + 1],
            format!("  ... {} more frames ...", total - 2 * STACK_TRACE_EDGE_FRAMES)
        );
        assert_eq!(
            lines[STACK_TRACE_EDGE_FRAMES + 2],
            format!("  at f{}", total - STACK_TRACE_EDGE_FRAMES)
        );
        assert_eq!(lines.last().copied(), Some(format!("  at f{}", total - 1).as_str()));
    }

    #[test]
    fn a_stack_trace_one_frame_over_the_edges_omits_exactly_one() {
        let trace = trace_of(2 * STACK_TRACE_EDGE_FRAMES + 1);
        assert!(trace.contains("... 1 more frame ..."), "{trace}");
        assert_eq!(trace.lines().count(), 1 + 2 * STACK_TRACE_EDGE_FRAMES + 1);
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
                crate::Token {
                    range: crate::Range {
                        start: crate::Position { line, column: 1 },
                        end: crate::Position { line, column: 1 },
                    },
                    kind: crate::TokenKind::Eof,
                    module_id: crate::ArenaId::new(0),
                },
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

    #[test]
    fn vm_error_stays_compact() {
        // Every fallible VM function returns `VmResult<T>`; keep the error small.
        assert!(
            std::mem::size_of::<VmError>() <= 24,
            "VmError grew to {} bytes; box the new variant's payload",
            std::mem::size_of::<VmError>()
        );
    }
}
