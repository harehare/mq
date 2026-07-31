//! Ergonomic sugar on top of the raw [`super::HostFunction`] API:
//! [`ValueAdapter`] converts between [`RuntimeValue`] and plain Rust types, and
//! [`IntoHostFunction`] lets [`crate::Engine::register_fn`] accept a typed closure like
//! `|n: i64| Ok(n * 2)` directly, without the host ever touching a `&[RuntimeValue]` slice.

use super::{HostFnResult, HostFunction, HostFunctionError, HostFunctionSyncBound};
use crate::{RuntimeValue, Shared, number::Number};

/// Converts a Rust type to and from [`RuntimeValue`], powering the typed-argument overload of
/// [`crate::Engine::register_fn`]. Implemented for the primitive types a host function typically
/// wants to take or return; implement it for your own types to use them directly as arguments or
/// return values too.
pub trait ValueAdapter: Sized {
    /// Converts a [`RuntimeValue`] argument to `Self`, or fails with a message describing the
    /// expected type.
    fn from_runtime_value(value: &RuntimeValue) -> Result<Self, HostFunctionError>;

    /// Converts `self` into the [`RuntimeValue`] returned to mq code.
    fn into_runtime_value(self) -> RuntimeValue;
}

fn type_mismatch(expected: &str, value: &RuntimeValue) -> HostFunctionError {
    HostFunctionError::new(format!("expected {expected}, got {}", value.name()))
}

impl ValueAdapter for RuntimeValue {
    fn from_runtime_value(value: &RuntimeValue) -> Result<Self, HostFunctionError> {
        Ok(value.clone())
    }

    fn into_runtime_value(self) -> RuntimeValue {
        self
    }
}

impl ValueAdapter for i64 {
    fn from_runtime_value(value: &RuntimeValue) -> Result<Self, HostFunctionError> {
        match value {
            RuntimeValue::Number(n) => Ok(n.to_int()),
            other => Err(type_mismatch("a number", other)),
        }
    }

    fn into_runtime_value(self) -> RuntimeValue {
        RuntimeValue::from(Number::from(self))
    }
}

impl ValueAdapter for f64 {
    fn from_runtime_value(value: &RuntimeValue) -> Result<Self, HostFunctionError> {
        match value {
            RuntimeValue::Number(n) => Ok(n.value()),
            other => Err(type_mismatch("a number", other)),
        }
    }

    fn into_runtime_value(self) -> RuntimeValue {
        RuntimeValue::from(Number::from(self))
    }
}

impl ValueAdapter for bool {
    fn from_runtime_value(value: &RuntimeValue) -> Result<Self, HostFunctionError> {
        match value {
            RuntimeValue::Boolean(b) => Ok(*b),
            other => Err(type_mismatch("a bool", other)),
        }
    }

    fn into_runtime_value(self) -> RuntimeValue {
        RuntimeValue::Boolean(self)
    }
}

impl ValueAdapter for String {
    fn from_runtime_value(value: &RuntimeValue) -> Result<Self, HostFunctionError> {
        match value {
            RuntimeValue::String(s) => Ok(s.clone()),
            other => Err(type_mismatch("a string", other)),
        }
    }

    fn into_runtime_value(self) -> RuntimeValue {
        RuntimeValue::String(self)
    }
}

impl<T: ValueAdapter> ValueAdapter for Vec<T> {
    fn from_runtime_value(value: &RuntimeValue) -> Result<Self, HostFunctionError> {
        match value {
            RuntimeValue::Array(items) => items.iter().map(T::from_runtime_value).collect(),
            other => Err(type_mismatch("an array", other)),
        }
    }

    fn into_runtime_value(self) -> RuntimeValue {
        RuntimeValue::Array(Shared::new(self.into_iter().map(T::into_runtime_value).collect()))
    }
}

impl<T: ValueAdapter> ValueAdapter for Option<T> {
    fn from_runtime_value(value: &RuntimeValue) -> Result<Self, HostFunctionError> {
        match value {
            RuntimeValue::None => Ok(None),
            other => T::from_runtime_value(other).map(Some),
        }
    }

    fn into_runtime_value(self) -> RuntimeValue {
        match self {
            Some(v) => v.into_runtime_value(),
            None => RuntimeValue::NONE,
        }
    }
}

/// Converts a Rust closure into a boxed [`HostFunction`], as the target of
/// [`crate::Engine::register_fn`]. `Marker` distinguishes the raw `&[RuntimeValue]` form from
/// each typed arity below so a single closure can only ever match one implementation — hosts
/// never need to name `Marker` or this trait themselves.
pub trait IntoHostFunction<Marker> {
    fn into_host_fn(self) -> Shared<dyn HostFunction>;
}

/// Marker for the raw form: `Fn(&[RuntimeValue]) -> HostFnResult`.
pub struct RawArgs;

impl<F> IntoHostFunction<RawArgs> for F
where
    F: Fn(&[RuntimeValue]) -> HostFnResult + HostFunctionSyncBound + 'static,
{
    fn into_host_fn(self) -> Shared<dyn HostFunction> {
        Shared::new(self)
    }
}

macro_rules! impl_into_host_fn {
    ($n:expr; $($T:ident),*) => {
        impl<Func, R, $($T),*> IntoHostFunction<($($T,)*)> for Func
        where
            Func: Fn($($T),*) -> Result<R, HostFunctionError> + HostFunctionSyncBound + 'static,
            R: ValueAdapter,
            $($T: ValueAdapter,)*
        {
            #[allow(non_snake_case)]
            fn into_host_fn(self) -> Shared<dyn HostFunction> {
                Shared::new(move |args: &[RuntimeValue]| -> HostFnResult {
                    if args.len() != $n {
                        return Err(HostFunctionError::new(format!(
                            "expected {} argument(s), got {}",
                            $n,
                            args.len()
                        )));
                    }
                    #[allow(unused_mut, unused_variables)]
                    let mut iter = args.iter();
                    $(let $T = $T::from_runtime_value(iter.next().unwrap())?;)*
                    (self)($($T),*).map(ValueAdapter::into_runtime_value)
                })
            }
        }
    };
}

impl_into_host_fn!(0;);
impl_into_host_fn!(1; A);
impl_into_host_fn!(2; A, B);
impl_into_host_fn!(3; A, B, C);
impl_into_host_fn!(4; A, B, C, D);

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    /// Erases a `T: ValueAdapter`'s `from_runtime_value` + `into_runtime_value` pair to a
    /// uniform, comparable fn pointer, so every `ValueAdapter` impl's happy path and error path
    /// can share one rstest table each despite each case exercising a different Rust type.
    /// Only sound for non-capturing closures, which every case below is.
    type RoundTripFn = fn(&RuntimeValue) -> Result<RuntimeValue, HostFunctionError>;

    fn round_trip<T: ValueAdapter>(value: &RuntimeValue) -> Result<RuntimeValue, HostFunctionError> {
        T::from_runtime_value(value).map(ValueAdapter::into_runtime_value)
    }

    #[rstest]
    #[case::runtime_value(round_trip::<RuntimeValue> as RoundTripFn, RuntimeValue::from(Number::from(42_i64)))]
    #[case::i64(round_trip::<i64> as RoundTripFn, RuntimeValue::from(Number::from(7_i64)))]
    #[case::f64(round_trip::<f64> as RoundTripFn, RuntimeValue::from(Number::from(1.5)))]
    #[case::bool(round_trip::<bool> as RoundTripFn, RuntimeValue::Boolean(true))]
    #[case::string(round_trip::<String> as RoundTripFn, RuntimeValue::String("hi".to_string()))]
    #[case::vec(
        round_trip::<Vec<i64>> as RoundTripFn,
        RuntimeValue::Array(Shared::new(vec![RuntimeValue::from(Number::from(1_i64)), RuntimeValue::from(Number::from(2_i64))]))
    )]
    #[case::option_none(round_trip::<Option<i64>> as RoundTripFn, RuntimeValue::NONE)]
    #[case::option_some(round_trip::<Option<i64>> as RoundTripFn, RuntimeValue::from(Number::from(3_i64)))]
    fn test_adapter_roundtrip(#[case] convert: RoundTripFn, #[case] value: RuntimeValue) {
        assert_eq!(convert(&value).unwrap(), value);
    }

    #[rstest]
    #[case::i64_wrong_type(round_trip::<i64> as RoundTripFn, RuntimeValue::Boolean(true))]
    #[case::f64_wrong_type(round_trip::<f64> as RoundTripFn, RuntimeValue::String("x".to_string()))]
    #[case::bool_wrong_type(round_trip::<bool> as RoundTripFn, RuntimeValue::NONE)]
    #[case::string_wrong_type(round_trip::<String> as RoundTripFn, RuntimeValue::Boolean(false))]
    #[case::vec_wrong_type(round_trip::<Vec<i64>> as RoundTripFn, RuntimeValue::Boolean(true))]
    #[case::vec_wrong_element_type(
        round_trip::<Vec<i64>> as RoundTripFn,
        RuntimeValue::Array(Shared::new(vec![RuntimeValue::Boolean(true)]))
    )]
    #[case::option_wrong_inner_type(round_trip::<Option<i64>> as RoundTripFn, RuntimeValue::Boolean(true))]
    fn test_adapter_type_mismatch_errors(#[case] convert: RoundTripFn, #[case] value: RuntimeValue) {
        assert!(convert(&value).is_err());
    }

    #[test]
    fn test_register_typed_zero_arity() {
        let f = (|| Ok(true)).into_host_fn();
        assert_eq!(f.call(&[]).unwrap(), RuntimeValue::Boolean(true));
    }

    #[test]
    fn test_register_typed_one_arity() {
        let f = (|n: i64| Ok(n * 2)).into_host_fn();
        let result = f.call(&[RuntimeValue::from(Number::from(21_i64))]).unwrap();
        assert_eq!(result, RuntimeValue::from(Number::from(42_i64)));
    }

    #[test]
    fn test_register_typed_two_arity() {
        let f = (|a: i64, b: i64| Ok(a + b)).into_host_fn();
        let result = f
            .call(&[
                RuntimeValue::from(Number::from(1_i64)),
                RuntimeValue::from(Number::from(2_i64)),
            ])
            .unwrap();
        assert_eq!(result, RuntimeValue::from(Number::from(3_i64)));
    }

    #[test]
    fn test_register_typed_wrong_arity_errors() {
        let f = (|n: i64| Ok(n)).into_host_fn();
        assert!(f.call(&[]).is_err());
        assert!(
            f.call(&[
                RuntimeValue::from(Number::from(1_i64)),
                RuntimeValue::from(Number::from(2_i64))
            ])
            .is_err()
        );
    }

    #[test]
    fn test_register_typed_wrong_type_errors() {
        let f = (|n: i64| Ok(n)).into_host_fn();
        assert!(f.call(&[RuntimeValue::Boolean(true)]).is_err());
    }

    #[test]
    fn test_raw_args_still_works_via_into_host_fn() {
        let f = (|args: &[RuntimeValue]| Ok(args[0].clone())).into_host_fn();
        let result = f.call(&[RuntimeValue::Boolean(true)]).unwrap();
        assert_eq!(result, RuntimeValue::Boolean(true));
    }
}
