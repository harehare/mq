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

// Exclusive upper bound for safe `as i64` casts. `i64::MAX as f64` rounds up to 2^63.
const I64_MIN_F64: f64 = i64::MIN as f64;
const I64_MAX_BOUND_F64: f64 = 9223372036854775808.0;

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

    /// Returns `true` if the number is exactly an integer.
    ///
    /// A tiny non-zero value such as `1e-17` is not an integer. NaN and the infinities are not
    /// either.
    pub fn is_int(&self) -> bool {
        self.0.fract() == 0.0
    }

    /// Returns the absolute value of this number.
    pub fn abs(&self) -> Self {
        Number(self.0.abs())
    }

    /// Returns `true` if the number is exactly zero (`0.0` or `-0.0`).
    ///
    /// A tiny non-zero value such as `1e-17` is not zero, so dividing by it is valid.
    pub fn is_zero(&self) -> bool {
        self.0 == 0.0
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

/// Magnitudes from here up, and non-zero magnitudes below `EXPONENT_BELOW`, are written in
/// exponent notation (`1e21`, `1.5e-7`), the same cut-offs as JavaScript's `Number#toString`.
const EXPONENT_FROM: f64 = 1e21;
const EXPONENT_BELOW: f64 = 1e-6;

impl fmt::Display for Number {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = self.0;
        let magnitude = value.abs();
        if (magnitude >= EXPONENT_FROM && magnitude.is_finite()) || (magnitude > 0.0 && magnitude < EXPONENT_BELOW) {
            // Shortest representation that round-trips, so no precision is lost.
            write!(f, "{value:e}")
        } else if value.fract() == 0.0 && (I64_MIN_F64..I64_MAX_BOUND_F64).contains(&value) {
            // `as i64` saturates beyond i64's range, so guard against it here.
            write!(f, "{}", value as i64)
        } else {
            write!(f, "{value}")
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
        if self.is_int()
            && other.is_int()
            && other.0 != 0.0
            && self.0 >= I64_MIN_F64
            && self.0 < I64_MAX_BOUND_F64
            && other.0 >= I64_MIN_F64
            && other.0 < I64_MAX_BOUND_F64
        {
            // `i64::MIN % -1` overflows and panics in debug builds; the true remainder is 0.
            return Number((self.0 as i64).checked_rem(other.0 as i64).unwrap_or(0) as f64);
        }
        Number(self.0 % other.0)
    }
}

#[allow(clippy::non_canonical_partial_ord_impl)]
impl PartialOrd for Number {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.0.partial_cmp(&other.0)
    }
}

// `==` follows IEEE 754 (NaN != NaN) while `Ord` sorts NaN last and equal to itself, so `Eq` is not reflexive for NaN.
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

#[cfg(kani)]
mod proofs {
    use super::*;

    #[kani::proof]
    fn rem_never_panics() {
        let (a, b): (f64, f64) = (kani::any(), kani::any());
        // Kani flags the NaN that `inf - inf` yields in `is_int`, which is not a panic.
        kani::assume(!a.is_infinite() && !b.is_infinite());
        let _ = Number::new(a) % Number::new(b);
    }

    #[kani::proof]
    fn cmp_is_total_order() {
        let a = Number::new(kani::any());
        let b = Number::new(kani::any());
        let c = Number::new(kani::any());
        assert_eq!(a.cmp(&a), Ordering::Equal);
        assert_eq!(a.cmp(&b), b.cmp(&a).reverse());
        if a.cmp(&b) != Ordering::Greater && b.cmp(&c) != Ordering::Greater {
            assert!(a.cmp(&c) != Ordering::Greater);
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
    #[case(42.0000001, "42.0000001")]
    #[case(0.1234567, "0.1234567")]
    #[case(0.000001, "0.000001")]
    #[case(1.0e-7, "1e-7")]
    #[case(-1.5e-10, "-1.5e-10")]
    #[case(1.0e-17, "1e-17")]
    #[case(5e-324, "5e-324")]
    #[case(f64::MIN_POSITIVE, "2.2250738585072014e-308")]
    #[case(123456789.12345679, "123456789.12345679")]
    #[case(100000000000000000.0, "100000000000000000")]
    #[case(9007199254740993.0, "9007199254740992")]
    #[case(999999999999999900000.0, "999999999999999900000")]
    #[case(1.0e21, "1e21")]
    #[case(-1.0e21, "-1e21")]
    #[case(1.5e300, "1.5e300")]
    #[case(f64::MAX, "1.7976931348623157e308")]
    #[case(f64::INFINITY, "inf")]
    #[case(f64::NEG_INFINITY, "-inf")]
    #[case(f64::NAN, "NaN")]
    #[case(-0.0, "0")]
    #[case(-42.0, "-42")]
    #[case(-42.123, "-42.123")]
    #[case(0.0, "0")]
    #[case(0.1, "0.1")]
    fn test_display_formatting(#[case] input: f64, #[case] expected: &str) {
        let num = Number::new(input);
        assert_eq!(format!("{}", num), expected);
    }

    #[test]
    fn test_display_beyond_i64_range_does_not_saturate() {
        let big = Number::new(2f64.powf(100.0));
        assert_eq!(format!("{}", big), "1.2676506002282294e30");

        let small = Number::new(-(2f64.powf(100.0)));
        assert_eq!(format!("{}", small), "-1.2676506002282294e30");

        let between = Number::new(2f64.powf(66.0));
        assert_eq!(format!("{}", between), "73786976294838210000");
    }

    #[rstest]
    #[case(1.0e-7)]
    #[case(1.0e-17)]
    #[case(5e-324)]
    #[case(1.0e21)]
    #[case(1.5e300)]
    #[case(f64::MAX)]
    #[case(-f64::MIN_POSITIVE)]
    #[case(0.1 + 0.2 - 0.3)]
    fn test_display_round_trips_through_parsing(#[case] input: f64) {
        let text = Number::new(input).to_string();
        assert_eq!(text.parse::<f64>().unwrap(), input, "{text}");
    }

    #[rstest]
    #[case(5.0, 2.0, "7", "3", "10", "2.5", "1")]
    #[case(10.0, 3.0, "13", "7", "30", "3.3333333333333335", "1")]
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
    #[case(5.5, 2.0, 1.5)]
    #[case(5.0, 2.5, 0.0)]
    #[case(-5.5, 2.0, -1.5)]
    fn test_rem_fractional_operands(#[case] a: f64, #[case] b: f64, #[case] expected: f64) {
        assert_eq!((Number::new(a) % Number::new(b)).value(), expected);
    }

    #[rstest]
    #[case(f64::NAN, 0.0)]
    #[case(0.0, f64::NAN)]
    #[case(f64::NAN, f64::NAN)]
    fn test_nan_partial_cmp_is_none(#[case] a: f64, #[case] b: f64) {
        assert_eq!(Number::new(a).partial_cmp(&Number::new(b)), None);
    }

    #[test]
    fn test_nan_sorts_last_in_total_order() {
        assert_eq!(Number::new(f64::NAN).cmp(&Number::new(0.0)), Ordering::Greater);
    }

    #[rstest]
    #[case(5.0, 1e-17)]
    #[case(5.0, 3e-17)]
    #[case(-5.0, 1e-300)]
    #[case(1e-17, 3.0)]
    #[case(1e-17, 1e-18)]
    fn test_rem_with_tiny_operands_matches_fmod(#[case] a: f64, #[case] b: f64) {
        assert_eq!((Number::new(a) % Number::new(b)).value(), a % b);
    }

    #[rstest]
    #[case(0.0, true)]
    #[case(-0.0, true)]
    #[case(3.0, true)]
    #[case(-3.0, true)]
    #[case(1e300, true)]
    #[case(0.5, false)]
    #[case(1e-17, false)]
    #[case(5e-324, false)]
    #[case(f64::NAN, false)]
    #[case(f64::INFINITY, false)]
    #[case(f64::NEG_INFINITY, false)]
    fn test_is_int(#[case] value: f64, #[case] expected: bool) {
        assert_eq!(Number::new(value).is_int(), expected);
    }

    #[test]
    fn test_rem_by_zero_is_nan() {
        assert!((Number::new(5.0) % Number::new(0.0)).value().is_nan());
    }

    #[test]
    fn test_rem_beyond_i64_range_matches_fmod() {
        // Both operands are integer-valued but exceed i64::MAX, so the fast `as i64` path
        // must not be taken; otherwise both saturate to i64::MAX and give the same wrong
        // result instead of their true (and distinct) fmod remainders.
        let a = Number::new(100_000_000_000_000_000_000.0);
        let b = Number::new(200_000_000_000_000_000_000.0);
        let c = Number::new(3.0);

        assert_eq!((a % c).value(), 100_000_000_000_000_000_000.0f64 % 3.0);
        assert_eq!((b % c).value(), 200_000_000_000_000_000_000.0f64 % 3.0);
        assert_ne!((a % c).value(), (b % c).value());
    }

    #[test]
    fn test_rem_at_i64_max_boundary() {
        let a = Number::new(9_223_372_036_854_775_808.0); // 2^63
        let b = Number::new(3.0);

        assert_eq!((a % b).value(), 2.0);
    }

    #[test]
    fn test_rem_min_integer_by_negative_one_does_not_panic() {
        let a = Number::new(i64::MIN as f64);
        let b = Number::new(-1.0);

        assert_eq!((a % b).value(), 0.0);
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
    #[case(1e-16, false)]
    #[case(1e-300, false)]
    #[case(f64::MIN_POSITIVE, false)]
    #[case(f64::NAN, false)]
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
