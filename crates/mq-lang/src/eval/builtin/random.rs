//! Randomness backing `rand`, `rand_int`, `shuffle`, `sample`, and `random_string`.
//!
//! Delegates to the [`rand`] crate (OS-seeded via `getrandom`, exposed through
//! `rand::rng()`/`ThreadRng`) rather than a hand-rolled generator, so output is backed by a
//! well-reviewed CSPRNG instead of a predictable time+counter seed. On `wasm32-unknown-unknown`
//! this requires the `getrandom` crate's `wasm_js` feature (see `Cargo.toml`), which sources
//! entropy from the browser's `crypto.getRandomValues`.
//!
//! Every function takes an optional `seed`: when given, output is drawn from a fresh
//! `StdRng::seed_from_u64(seed)` instead, making the call a pure, reproducible function of its
//! arguments (no hidden global RNG state, safe under parallel evaluation). This backs
//! property-based tests in `mq-test`, where a failing case's seed can be reported and replayed.
//!
//! These functions are still **not** intended for generating secrets or authentication tokens —
//! use a purpose-built secret-generation API for that.

use crate::RuntimeValue;
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::{Rng, RngExt, SeedableRng};

/// Runs `f` against a seeded `StdRng` when `seed` is given, or the OS-seeded thread RNG otherwise.
fn with_rng<T>(seed: Option<u64>, f: impl FnOnce(&mut dyn Rng) -> T) -> T {
    match seed {
        Some(seed) => f(&mut StdRng::seed_from_u64(seed)),
        None => f(&mut rand::rng()),
    }
}

/// Returns a pseudo-random `f64` uniformly distributed in `[0, 1)`.
pub(super) fn next_f64(seed: Option<u64>) -> f64 {
    with_rng(seed, |rng| rng.random::<f64>())
}

/// Returns a pseudo-random integer uniformly distributed in `[min, max]` (inclusive).
/// Returns `None` if `min > max`.
pub(super) fn next_range_i64(min: i64, max: i64, seed: Option<u64>) -> Option<i64> {
    if min > max {
        return None;
    }
    Some(with_rng(seed, |rng| rng.random_range(min..=max)))
}

/// Shuffles `items` in place using a uniformly random permutation (Fisher-Yates).
pub(super) fn shuffle(items: &mut [RuntimeValue], seed: Option<u64>) {
    with_rng(seed, |rng| items.shuffle(rng));
}

/// Returns `n` elements sampled from `items` without replacement, in random order.
///
/// Panics if `n > items.len()`; callers must validate the bound first.
pub(super) fn sample(items: &[RuntimeValue], n: usize, seed: Option<u64>) -> Vec<RuntimeValue> {
    let mut items = items.to_vec();
    shuffle(&mut items, seed);
    items.truncate(n);
    items
}

/// Returns a random string of `len` characters, each chosen uniformly at random
/// (with replacement) from `charset`. Returns `None` if `charset` is empty.
pub(super) fn next_string(len: usize, charset: &[char], seed: Option<u64>) -> Option<String> {
    if charset.is_empty() {
        return None;
    }

    Some(with_rng(seed, |rng| {
        (0..len).map(|_| charset[rng.random_range(0..charset.len())]).collect()
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_next_f64_in_unit_range() {
        for _ in 0..1000 {
            let v = next_f64(None);
            assert!((0.0..1.0).contains(&v), "value {v} out of [0, 1)");
        }
    }

    #[test]
    fn test_next_f64_seeded_is_deterministic() {
        assert_eq!(next_f64(Some(42)), next_f64(Some(42)));
    }

    #[test]
    fn test_next_range_i64_within_bounds() {
        for _ in 0..1000 {
            let v = next_range_i64(-5, 5, None).unwrap();
            assert!((-5..=5).contains(&v), "value {v} out of [-5, 5]");
        }
    }

    #[test]
    fn test_next_range_i64_single_value_range() {
        for _ in 0..10 {
            assert_eq!(next_range_i64(7, 7, None), Some(7));
        }
    }

    #[test]
    fn test_next_range_i64_invalid_range() {
        assert_eq!(next_range_i64(5, 1, None), None);
    }

    #[test]
    fn test_next_range_i64_seeded_is_deterministic() {
        assert_eq!(
            next_range_i64(0, 1_000_000, Some(7)),
            next_range_i64(0, 1_000_000, Some(7))
        );
    }

    fn nums(vals: &[i64]) -> Vec<RuntimeValue> {
        vals.iter().map(|&n| RuntimeValue::Number(n.into())).collect()
    }

    #[test]
    fn test_shuffle_preserves_multiset() {
        let mut items = nums(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
        let original = items.clone();
        shuffle(&mut items, None);
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
        shuffle(&mut items, None);
        assert!(items.is_empty());
    }

    #[test]
    fn test_shuffle_seeded_is_deterministic() {
        let mut a = nums(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
        let mut b = a.clone();
        shuffle(&mut a, Some(123));
        shuffle(&mut b, Some(123));
        assert_eq!(a, b);
    }

    #[test]
    fn test_sample_returns_requested_count_without_duplicates() {
        let items = nums(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
        let sampled = sample(&items, 4, None);
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
        let sampled = sample(&items, items.len(), None);
        let mut sorted_original = items.clone();
        let mut sorted_sampled = sampled.clone();
        sorted_original.sort_by(|a, b| a.partial_cmp(b).unwrap());
        sorted_sampled.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(sorted_original, sorted_sampled);
    }

    #[test]
    fn test_sample_zero() {
        let items = nums(&[1, 2, 3]);
        assert_eq!(sample(&items, 0, None), Vec::<RuntimeValue>::new());
    }

    #[test]
    fn test_sample_seeded_is_deterministic() {
        let items = nums(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
        assert_eq!(sample(&items, 4, Some(99)), sample(&items, 4, Some(99)));
    }

    #[test]
    fn test_next_string_uses_only_charset_chars() {
        let charset: Vec<char> = "abc".chars().collect();
        let s = next_string(20, &charset, None).unwrap();
        assert_eq!(s.chars().count(), 20);
        assert!(s.chars().all(|c| charset.contains(&c)));
    }

    #[test]
    fn test_next_string_zero_length() {
        let charset: Vec<char> = "abc".chars().collect();
        assert_eq!(next_string(0, &charset, None), Some(String::new()));
    }

    #[test]
    fn test_next_string_empty_charset() {
        assert_eq!(next_string(5, &[], None), None);
    }

    #[test]
    fn test_next_string_seeded_is_deterministic() {
        let charset: Vec<char> = "abcdefghijklmnopqrstuvwxyz0123456789".chars().collect();
        assert_eq!(next_string(16, &charset, Some(7)), next_string(16, &charset, Some(7)));
    }
}
