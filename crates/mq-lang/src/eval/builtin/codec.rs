//! `compress`/`decompress` builtins: general-purpose gzip/deflate/zstd byte compression,
//! independent of `http()` (see [`crate::compression`] for the shared codec primitives).

use super::Error;
use crate::{RuntimeValue, compression};

/// Response bodies get a network-specific limit (see `io::native`); a value already sitting
/// in memory as a `RuntimeValue::Bytes` argument doesn't need as tight a cap, but still needs
/// one so a small malicious payload can't be decompressed into unbounded memory.
const MAX_DECOMPRESSED_SIZE: u64 = 100 * 1024 * 1024;

/// Accepts either a string (`"gzip"`) or a symbol (`:gzip`) algorithm name, case-insensitively.
fn parse_algorithm(value: &RuntimeValue) -> Result<&'static str, Error> {
    let name = match value {
        RuntimeValue::Symbol(name) => name.as_str(),
        RuntimeValue::String(name) => name.clone(),
        other => return Err(err(format!("algorithm must be a string or symbol, got {other}"))),
    };
    match_algorithm(&name).ok_or_else(|| {
        err(format!(
            "unsupported algorithm {name:?}, expected gzip, deflate, or zstd"
        ))
    })
}

fn match_algorithm(name: &str) -> Option<&'static str> {
    if name.eq_ignore_ascii_case("gzip") {
        Some("gzip")
    } else if name.eq_ignore_ascii_case("deflate") {
        Some("deflate")
    } else if name.eq_ignore_ascii_case("zstd") {
        Some("zstd")
    } else {
        None
    }
}

fn err(msg: impl std::fmt::Display) -> Error {
    Error::Runtime(format!("compress: {msg}"))
}

pub(super) fn compress(data: &[u8], algorithm: &RuntimeValue) -> Result<RuntimeValue, Error> {
    let bytes = match parse_algorithm(algorithm)? {
        "gzip" => compression::encode_gzip(data),
        "deflate" => compression::encode_deflate(data),
        "zstd" => compression::encode_zstd(data),
        _ => unreachable!(),
    };
    Ok(RuntimeValue::Bytes(bytes))
}

pub(super) fn decompress(data: &[u8], algorithm: &RuntimeValue) -> Result<RuntimeValue, Error> {
    let algo = parse_algorithm(algorithm)?;
    let result = match algo {
        "gzip" => compression::decode_gzip(data, MAX_DECOMPRESSED_SIZE),
        "deflate" => compression::decode_deflate(data, MAX_DECOMPRESSED_SIZE),
        "zstd" => compression::decode_zstd(data, MAX_DECOMPRESSED_SIZE),
        _ => unreachable!(),
    };
    result
        .map(RuntimeValue::Bytes)
        .map_err(|e| err(format!("failed to decompress ({algo}): {e}")))
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::Ident;

    fn symbol(name: &str) -> RuntimeValue {
        RuntimeValue::Symbol(Ident::new(name))
    }

    #[rstest]
    #[case::gzip("gzip")]
    #[case::deflate("deflate")]
    #[case::zstd("zstd")]
    fn test_compress_decompress_round_trip(#[case] algorithm: &str) {
        let original = b"hello, compression!";

        let compressed = compress(original, &symbol(algorithm)).unwrap();
        let RuntimeValue::Bytes(compressed) = compressed else {
            panic!("expected Bytes")
        };
        assert_ne!(compressed, original);

        let decompressed = decompress(&compressed, &RuntimeValue::String(algorithm.into())).unwrap();
        assert_eq!(decompressed, RuntimeValue::Bytes(original.to_vec()));
    }

    #[test]
    fn test_parse_algorithm_rejects_unknown_name() {
        assert!(matches!(compress(b"data", &symbol("brotli")), Err(Error::Runtime(_))));
    }

    #[test]
    fn test_decompress_rejects_garbage() {
        assert!(matches!(
            decompress(b"not compressed", &symbol("gzip")),
            Err(Error::Runtime(_))
        ));
    }
}
