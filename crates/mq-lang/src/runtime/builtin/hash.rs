//! A seeded hash that returns an integer a number holds exactly, and the bit length of an integer.
//!
//! The hash is SipHash-2-4, whose output is fixed by its specification, so a value hashed today
//! gives the same result on any platform and in any later release. Summaries built on it, such as a
//! Bloom filter or a HyperLogLog counter, can be stored and merged later. SipHash is a keyed hash
//! with strong mixing of every output bit; it is not meant to resist an attacker who picks the input.

use siphasher::sip::SipHasher24;
use std::hash::Hasher;

/// Bits of a hash. An `f64` holds every integer below `2^53` exactly.
pub(super) const HASH_BITS: u32 = 53;

/// Seeds are the integers below this bound, so every seed is a distinct, exactly held `f64`.
pub(super) const SEED_LIMIT: f64 = (1u64 << HASH_BITS) as f64;

/// Hashes `bytes` to an integer below `2^53`. The seed is the first word of the SipHash key, so
/// each seed gives an independent hash of the same bytes.
pub(super) fn hash53(bytes: &[u8], seed: u64) -> u64 {
    let mut hasher = SipHasher24::new_with_keys(seed, 0);
    hasher.write(bytes);
    hasher.finish() >> (u64::BITS - HASH_BITS)
}

/// Number of bits needed to write the non-negative integer `n` in binary, 0 for 0. Read from the
/// binary exponent, so it is exact for every finite `f64`, however large.
pub(super) fn bit_length(n: f64) -> u32 {
    if n < 1.0 {
        0
    } else {
        ((n.to_bits() >> 52) & 0x7ff) as u32 - 1022
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    /// The 64-bit SipHash-2-4 of `bytes` under the key `00 01 .. 0f`, as in the reference test vectors.
    fn reference_siphash(bytes: &[u8]) -> u64 {
        let mut hasher = SipHasher24::new_with_keys(0x0706_0504_0302_0100, 0x0f0e_0d0c_0b0a_0908);
        hasher.write(bytes);
        hasher.finish()
    }

    /// Vectors 0, 1 and 15 of the SipHash reference implementation, to show the crate is called with
    /// the key and the bytes the way the specification means.
    #[rstest]
    #[case::empty(&[], 0x726f_db47_dd0e_0e31)]
    #[case::one_byte(&[0], 0x74f8_39c5_93dc_67fd)]
    #[case::fifteen_bytes(&[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14], 0xa129_ca61_49be_45e5)]
    fn test_siphash_matches_the_reference_vectors(#[case] bytes: &[u8], #[case] expected: u64) {
        assert_eq!(reference_siphash(bytes), expected);
    }

    /// Pinned results, computed with an independent SipHash-2-4. A change here breaks every stored
    /// summary built on the hash.
    #[rstest]
    #[case::empty("", 0, 1_075_637_845_389_024)]
    #[case::short("a", 0, 5_304_323_468_276_340)]
    #[case::word("hello", 0, 4_952_387_935_952_618)]
    #[case::seeded("hello", 1, 3_919_086_197_784_246)]
    #[case::non_ascii("日本語", 0, 7_491_283_311_083_573)]
    fn test_hash53_is_pinned(#[case] text: &str, #[case] seed: u64, #[case] expected: u64) {
        assert_eq!(hash53(text.as_bytes(), seed), expected);
    }

    #[test]
    fn test_hash53_depends_on_the_seed_and_the_bytes() {
        assert_ne!(hash53(b"a", 0), hash53(b"a", 1));
        assert_ne!(hash53(b"a", 0), hash53(b"b", 0));
        assert_ne!(hash53(b"ab", 0), hash53(b"ba", 0));
        assert_ne!(hash53(b"", 0), hash53(b"\0", 0));
    }

    /// Items spread over the buckets the way a uniform hash would, whichever end of the hash picks the
    /// bucket. With 1024 buckets the chi-square statistic of uniform data has mean 1023 and standard
    /// deviation about 45, so a value within five of them shows neither clumping nor an unnatural evenness.
    #[rstest]
    #[case::high_bits(HASH_BITS - 10)]
    #[case::low_bits(0)]
    fn test_hash53_spreads_like_a_uniform_hash(#[case] shift: u32) {
        let (items, buckets) = (200_000usize, 1024usize);
        let mut counts = vec![0f64; buckets];
        for i in 0..items {
            counts[((hash53(format!("item-{i}").as_bytes(), 0) >> shift) as usize) % buckets] += 1.0;
        }
        let expected = items as f64 / buckets as f64;
        let chi_square: f64 = counts.iter().map(|c| (c - expected).powi(2) / expected).sum();
        let (mean, deviation) = ((buckets - 1) as f64, (2.0 * (buckets - 1) as f64).sqrt());
        assert!(
            (chi_square - mean).abs() < 5.0 * deviation,
            "chi-square {chi_square}, expected about {mean} +- {}",
            5.0 * deviation
        );
    }

    #[rstest]
    #[case::zero(0.0, 0)]
    #[case::one(1.0, 1)]
    #[case::two(2.0, 2)]
    #[case::three(3.0, 2)]
    #[case::byte_max(255.0, 8)]
    #[case::just_over_a_byte(256.0, 9)]
    #[case::largest_exact_float(9_007_199_254_740_991.0, 53)]
    #[case::two_to_the_53(9_007_199_254_740_992.0, 54)]
    #[case::two_to_the_63(9_223_372_036_854_775_808.0, 64)]
    #[case::two_to_the_64(18_446_744_073_709_551_616.0, 65)]
    #[case::huge(1e30, 100)]
    #[case::two_to_the_1000(1.0715086071862673e301, 1001)]
    #[case::largest_float(f64::MAX, 1024)]
    fn test_bit_length(#[case] n: f64, #[case] expected: u32) {
        assert_eq!(bit_length(n), expected);
    }

    proptest::proptest! {
        #[test]
        fn test_hash53_stays_below_two_to_the_53(bytes in proptest::collection::vec(proptest::num::u8::ANY, 0..64), seed in proptest::num::u64::ANY) {
            proptest::prop_assert!(hash53(&bytes, seed) < 1 << HASH_BITS);
        }

        #[test]
        fn test_hash53_ignores_how_the_bytes_are_split_into_writes(bytes in proptest::collection::vec(proptest::num::u8::ANY, 0..64), at in 0usize..64) {
            let at = at.min(bytes.len());
            let mut split = SipHasher24::new_with_keys(7, 0);
            split.write(&bytes[..at]);
            split.write(&bytes[at..]);
            proptest::prop_assert_eq!(split.finish() >> (u64::BITS - HASH_BITS), hash53(&bytes, 7));
        }

        #[test]
        fn test_bit_length_matches_the_binary_text(n in proptest::num::u64::ANY) {
            proptest::prop_assert_eq!(bit_length(n as f64) as usize, format!("{:b}", n as f64 as u128).trim_start_matches('0').len());
        }
    }
}
