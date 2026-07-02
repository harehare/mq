//! Randomness backing `rand`, `rand_int`, `shuffle`, `sample`, `uuid`, and `uuid_v4`/`uuid_v7`.
//!
//! Delegates to the [`rand`] crate (OS-seeded via `getrandom`, exposed through
//! `rand::rng()`/`ThreadRng`) rather than a hand-rolled generator, so output is backed by a
//! well-reviewed CSPRNG instead of a predictable time+counter seed. On `wasm32-unknown-unknown`
//! this requires the `getrandom` crate's `wasm_js` feature (see `Cargo.toml`), which sources
//! entropy from the browser's `crypto.getRandomValues`.
//!
//! These functions are still **not** intended for generating secrets or authentication tokens —
//! use a purpose-built secret-generation API for that.

use rand::RngCore;

/// Returns 16 pseudo-random bytes.
pub(super) fn next_bytes_16() -> [u8; 16] {
    let mut bytes = [0u8; 16];
    rand::rng().fill_bytes(&mut bytes);
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_next_bytes_16_diverges_across_calls() {
        let a = next_bytes_16();
        let b = next_bytes_16();
        assert_ne!(a, b);
    }
}
