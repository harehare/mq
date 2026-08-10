//! `compress`/`decompress` builtins: general-purpose gzip/deflate/zstd byte compression,
//! independent of `http()` (see [`crate::compression`] for the shared codec).

use super::Error;
use crate::RuntimeValue;
use crate::compression::Algorithm;

/// A value already in memory doesn't need as tight a cap as an http() response body, but still
/// needs one so a small malicious payload can't decompress into unbounded memory.
const MAX_DECOMPRESSED_SIZE: u64 = 100 * 1024 * 1024;

fn parse_algorithm(value: &RuntimeValue) -> Result<Algorithm, Error> {
    let name = match value {
        RuntimeValue::Symbol(name) => name.as_str(),
        RuntimeValue::String(name) => name.clone(),
        other => return Err(err(format!("algorithm must be a string or symbol, got {other}"))),
    };
    Algorithm::parse(&name).ok_or_else(|| {
        err(format!(
            "unsupported algorithm {name:?}, expected gzip, deflate, or zstd"
        ))
    })
}

fn err(msg: impl std::fmt::Display) -> Error {
    Error::Runtime(format!("compress: {msg}"))
}

pub(super) fn compress(data: &[u8], algorithm: &RuntimeValue) -> Result<RuntimeValue, Error> {
    Ok(RuntimeValue::Bytes(parse_algorithm(algorithm)?.encode(data)))
}

pub(super) fn decompress(data: &[u8], algorithm: &RuntimeValue) -> Result<RuntimeValue, Error> {
    let algorithm = parse_algorithm(algorithm)?;
    algorithm
        .decode(data, MAX_DECOMPRESSED_SIZE)
        .map(RuntimeValue::Bytes)
        .map_err(|e| err(format!("failed to decompress ({algorithm:?}): {e}")))
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
