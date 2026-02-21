use core::f64;
#[cfg(feature = "ast-json")]
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::fmt;
use std::ops::{Add, Div, Mul, Neg, Rem, Sub};

#[cfg_attr(feature = "ast-json", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, PartialEq, Copy)]
pub struct Number(f64);

/// Represents a Not-a-Number (NaN) value.
pub const NAN: Number = Number(f64::NAN);

/// Represents positive infinity.
pub const INFINITE: Number = Number(f64::INFINITY);

impl Number {
    /// Creates a new `Number` from an `f64` value.
    pub fn new(value: f64) -> Self {
        Number(value)
    }

    /// Returns the underlying `f64` value.
    pub fn value(&self) -> f64 {
        self.0
    }

    /// Returns the underlying `i64` value, truncating any fractional part.
    pub fn to_int(self) -> i64 {
        self.0 as i64
    }

    /// Returns `true` if the number represents an integer value.
    ///
    /// Uses epsilon comparison to account for floating-point precision.
    pub fn is_int(&self) -> bool {
        (self.0 - self.0.trunc()).abs() < f64::EPSILON
    }

    /// Returns the absolute value of this number.
    pub fn abs(&self) -> Self {
        Number(self.0.abs())
    }

    /// Returns `true` if the number is zero or very close to zero.
    ///
    /// Uses epsilon comparison to account for floating-point precision.
    pub fn is_zero(&self) -> bool {
        self.0.abs() < f64::EPSILON
    }

    /// Returns `true` if the number is NaN (Not-a-Number).
    pub fn is_nan(&self) -> bool {
        self.0.is_nan()
    }
}

impl Default for Number {
    fn default() -> Self {
        Number(0.0)
    }
}

impl Neg for Number {
    type Output = Self;

    fn neg(self) -> Self::Output {
        Number(-self.0)
    }
}

impl From<i64> for Number {
    fn from(value: i64) -> Self {
        Number(value as f64)
    }
}

impl From<i32> for Number {
    fn from(value: i32) -> Self {
        Number(value as f64)
    }
}

impl From<u8> for Number {
    fn from(value: u8) -> Self {
        Number(value as f64)
    }
}

impl From<u32> for Number {
    fn from(value: u32) -> Self {
        Number(value as f64)
    }
}

impl From<u64> for Number {
    fn from(value: u64) -> Self {
        Number(value as f64)
    }
}

impl From<isize> for Number {
    fn from(value: isize) -> Self {
        Number(value as f64)
    }
}

impl From<usize> for Number {
    fn from(value: usize) -> Self {
        Number(value as f64)
    }
}

impl From<f64> for Number {
    fn from(value: f64) -> Self {
        Number(value)
    }
}

impl fmt::Display for Number {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_int() {
            write!(f, "{}", self.0 as i64)
        } else {
            let s = format!("{:.6}", self.0);
            let s = s.trim_end_matches('0').trim_end_matches('.');
            write!(f, "{}", s)
        }
    }
}

impl Add for Number {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Number(self.0 + other.0)
    }
}

impl Sub for Number {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        Number(self.0 - other.0)
    }
}

impl Mul for Number {
    type Output = Self;

    fn mul(self, other: Self) -> Self {
        Number(self.0 * other.0)
    }
}

impl Div for Number {
    type Output = Self;

    fn div(self, other: Self) -> Self {
        Number(self.0 / other.0)
    }
}

impl Rem for Number {
    type Output = Self;

    fn rem(self, other: Self) -> Self {
        Number(self.0 % other.0)
    }
}

impl PartialOrd for Number {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Eq for Number {}

impl Ord for Number {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self.0.is_nan(), other.0.is_nan()) {
            (true, true) => Ordering::Equal,
            (true, false) => Ordering::Greater,
            (false, true) => Ordering::Less,
            (false, false) => self.0.partial_cmp(&other.0).unwrap_or(Ordering::Less),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::*;

    #[rstest]
    #[case(42.0, "42")]
    #[case(42.123, "42.123")]
    #[case(42.100, "42.1")]
    #[case(42.0000001, "42")]
    #[case(-42.0, "-42")]
    #[case(-42.123, "-42.123")]
    #[case(0.0, "0")]
    #[case(0.1, "0.1")]
    fn test_display_formatting(#[case] input: f64, #[case] expected: &str) {
        let num = Number::new(input);
        assert_eq!(format!("{}", num), expected);
    }

    #[rstest]
    #[case(5.0, 2.0, "7", "3", "10", "2.5", "1")]
    #[case(10.0, 3.0, "13", "7", "30", "3.333333", "1")]
    #[case(-5.0, 2.0, "-3", "-7", "-10", "-2.5", "-1")]
    #[case(0.0, 1.0, "1", "-1", "0", "0", "0")]
    fn test_operations(
        #[case] a: f64,
        #[case] b: f64,
        #[case] add_result: &str,
        #[case] sub_result: &str,
        #[case] mul_result: &str,
        #[case] div_result: &str,
        #[case] rem_result: &str,
    ) {
        let num_a = Number::new(a);
        let num_b = Number::new(b);

        assert_eq!(format!("{}", num_a + num_b), add_result);
        assert_eq!(format!("{}", num_a - num_b), sub_result);
        assert_eq!(format!("{}", num_a * num_b), mul_result);
        assert_eq!(format!("{}", num_a / num_b), div_result);
        assert_eq!(format!("{}", num_a % num_b), rem_result);
    }

    #[rstest]
    #[case(42.5f64, "42.5")]
    #[case(42.0f64, "42")]
    #[case(-42.5f64, "-42.5")]
    fn test_from_f64(#[case] input: f64, #[case] expected: &str) {
        let num = Number::from(input);
        assert_eq!(format!("{}", num), expected);
    }

    #[rstest]
    #[case(42i64, "42")]
    #[case(-42i64, "-42")]
    #[case(0i64, "0")]
    fn test_from_i32(#[case] input: i64, #[case] expected: &str) {
        let num = Number::from(input);
        assert_eq!(format!("{}", num), expected);
    }

    #[rstest]
    #[case(5.0, 2.0, true, false, true, false)]
    #[case(2.0, 5.0, false, true, false, true)]
    #[case(5.0, 5.0, false, false, true, true)]
    fn test_comparisons(
        #[case] a: f64,
        #[case] b: f64,
        #[case] greater: bool,
        #[case] less: bool,
        #[case] greater_equal: bool,
        #[case] less_equal: bool,
    ) {
        let num_a = Number::new(a);
        let num_b = Number::new(b);

        assert_eq!(num_a > num_b, greater);
        assert_eq!(num_a < num_b, less);
        assert_eq!(num_a >= num_b, greater_equal);
        assert_eq!(num_a <= num_b, less_equal);
    }

    #[rstest]
    #[case(5.0)]
    #[case(0.0)]
    #[case(-5.0)]
    fn test_equality(#[case] value: f64) {
        let a = Number::new(value);
        let b = Number::new(value);
        assert_eq!(a, b);
    }

    #[rstest]
    #[case(0.0, true)]
    #[case(0.1, false)]
    #[case(-0.0, true)]
    #[case(1e-16, true)]
    fn test_is_zero(#[case] value: f64, #[case] expected: bool) {
        let num = Number::new(value);
        assert_eq!(num.is_zero(), expected);
    }

    #[rstest]
    #[case(5.0, 5.0)]
    #[case(-5.0, 5.0)]
    #[case(0.0, 0.0)]
    #[case(-0.0, 0.0)]
    #[case(1e-16, 1e-16)]
    #[case(-1e-16, 1e-16)]
    fn test_abs(#[case] input: f64, #[case] expected: f64) {
        let num = Number::new(input);
        let abs_num = num.abs();
        assert!(
            (abs_num.value() - expected).abs() < f64::EPSILON,
            "abs({}) = {}, expected {}",
            input,
            abs_num.value(),
            expected
        );
    }
}
