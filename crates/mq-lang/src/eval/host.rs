//! Host-registered native functions: the embedding API that lets a Rust host expose its own
//! functions to mq code.
//!
//! A [`HostFunctions`] registry is owned per [`Engine`](crate::Engine)/[`Evaluator`](crate::eval::Evaluator)
//! and consulted by the evaluator whenever a called identifier isn't a local binding, right
//! before falling back to the built-in function table. This lets host functions be called from
//! mq code without any special syntax, exactly like builtins, while still being shadowable by a
//! `def` of the same name.

use crate::{Ident, RuntimeValue, Shared};
use rustc_hash::FxHashMap;

pub mod adapter;
pub use adapter::{IntoHostFunction, ValueAdapter};

/// Marker trait bounding host function closures to what's safe to store behind [`Shared`].
/// `Send + Sync` is only required when the `sync` feature (which backs [`Shared`] with `Arc`)
/// is enabled; under the default `Rc`-backed build, closures may freely capture non-`Send` state.
#[cfg(feature = "sync")]
pub trait HostFunctionSyncBound: Send + Sync {}
#[cfg(feature = "sync")]
impl<T: Send + Sync> HostFunctionSyncBound for T {}

#[cfg(not(feature = "sync"))]
pub trait HostFunctionSyncBound {}
#[cfg(not(feature = "sync"))]
impl<T> HostFunctionSyncBound for T {}

/// Error returned by a host-registered function, or produced while marshalling arguments and
/// return values (see the `ValueAdapter` trait). Carries only a message, so it stays
/// `PartialEq`-comparable and cheap to embed in [`RuntimeError`](crate::error::runtime::RuntimeError).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostFunctionError(String);

impl HostFunctionError {
    /// Creates a new host function error with the given message.
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }

    /// Returns the error message.
    pub fn message(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for HostFunctionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for HostFunctionError {}

impl From<&str> for HostFunctionError {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl From<String> for HostFunctionError {
    fn from(s: String) -> Self {
        Self::new(s)
    }
}

/// Result type returned by a host function call.
pub type HostFnResult = Result<RuntimeValue, HostFunctionError>;

/// Extracts a human-readable message from a caught panic payload.
pub(crate) fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic".to_string()
    }
}

/// A native Rust function callable from mq code, registered via [`HostFunctions::insert`].
///
/// Receives the already-evaluated call arguments and returns a single [`RuntimeValue`]. Blanket-
/// implemented for any matching closure, so hosts never need to name this trait directly.
///
/// Dispatch goes through [`Self::call`] rather than calling `dyn HostFunction` values directly:
/// the standard library only provides `Fn`-trait impls for `Box<dyn Fn..>`, not for `Rc`/`Arc`
/// (which back [`Shared`] depending on the `sync` feature), so an explicit method keeps
/// invocation through `Shared<dyn HostFunction>` unambiguous.
pub trait HostFunction: HostFunctionSyncBound + 'static {
    fn call(&self, args: &[RuntimeValue]) -> HostFnResult;
}

impl<F> HostFunction for F
where
    F: Fn(&[RuntimeValue]) -> HostFnResult + HostFunctionSyncBound + 'static,
{
    fn call(&self, args: &[RuntimeValue]) -> HostFnResult {
        (self)(args)
    }
}

/// A registry of native Rust functions exposed to mq code under a given name.
///
/// Cloning is O(1): registered functions are stored behind [`Shared`], so a clone shares the
/// same underlying entries (matching how [`Engine`](crate::Engine) itself is `Clone`).
#[derive(Clone, Default)]
pub struct HostFunctions(FxHashMap<Ident, Shared<dyn HostFunction>>);

impl std::fmt::Debug for HostFunctions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostFunctions")
            .field("names", &self.0.keys().map(Ident::to_string).collect::<Vec<_>>())
            .finish()
    }
}

impl HostFunctions {
    /// Registers a raw host function under `name`, callable from mq code as `name(...)`.
    ///
    /// If a function is already registered under `name`, it is replaced.
    pub fn insert(&mut self, name: impl Into<Ident>, f: impl HostFunction) {
        self.0.insert(name.into(), Shared::new(f));
    }

    /// Like [`Self::insert`], but takes an already-boxed function; used by
    /// [`crate::Engine::register_fn`]'s typed-argument overload.
    pub(crate) fn insert_shared(&mut self, name: impl Into<Ident>, f: Shared<dyn HostFunction>) {
        self.0.insert(name.into(), f);
    }

    /// Removes the function registered under `name`, if any. Returns whether one was removed.
    pub fn remove(&mut self, name: impl Into<Ident>) -> bool {
        self.0.remove(&name.into()).is_some()
    }

    /// Whether a function is registered under `name`.
    pub fn contains(&self, name: &Ident) -> bool {
        self.0.contains_key(name)
    }

    /// Looks up the function registered under `name`, cloning the [`Shared`] handle.
    pub(crate) fn get(&self, name: &Ident) -> Option<Shared<dyn HostFunction>> {
        self.0.get(name).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_and_get() {
        let mut fns = HostFunctions::default();
        assert!(!fns.contains(&Ident::new("double")));

        fns.insert("double", |args: &[RuntimeValue]| match args {
            [RuntimeValue::Number(n)] => Ok(RuntimeValue::from(crate::number::Number::from(n.value() * 2.0))),
            _ => Err(HostFunctionError::new("expected one number")),
        });

        assert!(fns.contains(&Ident::new("double")));
        let f = fns.get(&Ident::new("double")).unwrap();
        let result = f
            .call(&[RuntimeValue::from(crate::number::Number::from(21.0))])
            .unwrap();
        assert_eq!(result, RuntimeValue::from(crate::number::Number::from(42.0)));
    }

    #[test]
    fn test_insert_replaces_existing() {
        let mut fns = HostFunctions::default();
        fns.insert("f", |_: &[RuntimeValue]| {
            Ok(RuntimeValue::from(crate::number::Number::from(1_i64)))
        });
        fns.insert("f", |_: &[RuntimeValue]| Ok(RuntimeValue::Boolean(true)));

        let f = fns.get(&Ident::new("f")).unwrap();
        assert_eq!(f.call(&[]).unwrap(), RuntimeValue::Boolean(true));
    }

    #[test]
    fn test_remove() {
        let mut fns = HostFunctions::default();
        fns.insert("f", |_: &[RuntimeValue]| Ok(RuntimeValue::NONE));

        assert!(fns.remove("f"));
        assert!(!fns.contains(&Ident::new("f")));
        assert!(!fns.remove("f"));
    }

    #[test]
    fn test_get_missing_returns_none() {
        let fns = HostFunctions::default();
        assert!(fns.get(&Ident::new("missing")).is_none());
    }

    #[test]
    fn test_host_function_error_display_and_message() {
        let err = HostFunctionError::new("boom");
        assert_eq!(err.message(), "boom");
        assert_eq!(err.to_string(), "boom");

        let err: HostFunctionError = "from str".into();
        assert_eq!(err.message(), "from str");

        let err: HostFunctionError = String::from("from string").into();
        assert_eq!(err.message(), "from string");
    }

    #[test]
    fn test_debug_lists_names() {
        let mut fns = HostFunctions::default();
        fns.insert("alpha", |_: &[RuntimeValue]| Ok(RuntimeValue::NONE));
        let debug = format!("{:?}", fns);
        assert!(debug.contains("alpha"));
    }
}
