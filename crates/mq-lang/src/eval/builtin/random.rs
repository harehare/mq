//! Randomness backing `rand`, `rand_int`, `shuffle`, and `sample`.
//!
//! Delegates to the [`rand`] crate (OS-seeded via `getrandom`, exposed through
//! `rand::rng()`/`ThreadRng`) rather than a hand-rolled generator, so output is backed by a
//! well-reviewed CSPRNG instead of a predictable time+counter seed. On `wasm32-unknown-unknown`
//! this requires the `getrandom` crate's `wasm_js` feature (see `Cargo.toml`), which sources
//! entropy from the browser's `crypto.getRandomValues`.
//!
//! These functions are still **not** intended for generating secrets or authentication tokens —
//! use a purpose-built secret-generation API for that.

use crate::RuntimeValue;
use rand::Rng;
use rand::seq::SliceRandom;

/// Returns a pseudo-random `f64` uniformly distributed in `[0, 1)`.
pub(super) fn next_f64() -> f64 {
    rand::rng().random::<f64>()
}

/// Returns a pseudo-random integer uniformly distributed in `[min, max]` (inclusive).
/// Returns `None` if `min > max`.
pub(super) fn next_range_i64(min: i64, max: i64) -> Option<i64> {
    if min > max {
        return None;
    }
    Some(rand::rng().random_range(min..=max))
}

/// Shuffles `items` in place using a uniformly random permutation (Fisher-Yates).
pub(super) fn shuffle(items: &mut [RuntimeValue]) {
    items.shuffle(&mut rand::rng());
}

/// Returns `n` elements sampled from `items` without replacement, in random order.
///
/// Panics if `n > items.len()`; callers must validate the bound first.
pub(super) fn sample(items: &[RuntimeValue], n: usize) -> Vec<RuntimeValue> {
    let mut items = items.to_vec();
    shuffle(&mut items);
    items.truncate(n);
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_next_f64_in_unit_range() {
        for _ in 0..1000 {
            let v = next_f64();
            assert!((0.0..1.0).contains(&v), "value {v} out of [0, 1)");
        }
    }

    #[test]
    fn test_next_range_i64_within_bounds() {
        for _ in 0..1000 {
            let v = next_range_i64(-5, 5).unwrap();
            assert!((-5..=5).contains(&v), "value {v} out of [-5, 5]");
        }
    }

    #[test]
    fn test_next_range_i64_single_value_range() {
        for _ in 0..10 {
            assert_eq!(next_range_i64(7, 7), Some(7));
        }
    }

    #[test]
    fn test_next_range_i64_invalid_range() {
        assert_eq!(next_range_i64(5, 1), None);
    }

    fn nums(vals: &[i64]) -> Vec<RuntimeValue> {
        vals.iter().map(|&n| RuntimeValue::Number(n.into())).collect()
    }

    #[test]
    fn test_shuffle_preserves_multiset() {
        let mut items = nums(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
        let original = items.clone();
        shuffle(&mut items);
        assert_eq!(items.len(), original.len());

        let mut sorted_original = original.clone();
        let mut sorted_shuffled = items.clone();
        sorted_original.sort_by(|a, b| a.partial_cmp(b).unwrap());
        sorted_shuffled.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(sorted_original, sorted_shuffled);
    }

    #[test]
    fn test_shuffle_empty_array() {
        let mut items: Vec<RuntimeValue> = vec![];
        shuffle(&mut items);
        assert!(items.is_empty());
    }

    #[test]
    fn test_sample_returns_requested_count_without_duplicates() {
        let items = nums(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
        let sampled = sample(&items, 4);
        assert_eq!(sampled.len(), 4);

        for v in &sampled {
            assert!(items.contains(v));
        }
        let mut sorted = sampled.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        sorted.dedup();
        assert_eq!(sorted.len(), 4, "sample should not contain duplicates");
    }

    #[test]
    fn test_sample_full_length_is_a_permutation() {
        let items = nums(&[1, 2, 3, 4, 5]);
        let sampled = sample(&items, items.len());
        let mut sorted_original = items.clone();
        let mut sorted_sampled = sampled.clone();
        sorted_original.sort_by(|a, b| a.partial_cmp(b).unwrap());
        sorted_sampled.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(sorted_original, sorted_sampled);
    }

    #[test]
    fn test_sample_zero() {
        let items = nums(&[1, 2, 3]);
        assert_eq!(sample(&items, 0), Vec::<RuntimeValue>::new());
    }
}
