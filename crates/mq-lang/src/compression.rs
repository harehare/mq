//! Shared gzip/deflate/zstd (de)compression, backing the `compress`/`decompress` builtins and
//! `http()`'s `deflate`/`zstd` response decoding (`gzip` there is decoded by ureq itself; see
//! [`crate::io::native`]).

use std::io::{self, Read, Write};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Algorithm {
    Gzip,
    Deflate,
    Zstd,
}

impl Algorithm {
    pub(crate) fn parse(name: &str) -> Option<Self> {
        if name.eq_ignore_ascii_case("gzip") {
            Some(Self::Gzip)
        } else if name.eq_ignore_ascii_case("deflate") {
            Some(Self::Deflate)
        } else if name.eq_ignore_ascii_case("zstd") {
            Some(Self::Zstd)
        } else {
            None
        }
    }

    pub(crate) fn encode(self, data: &[u8]) -> Vec<u8> {
        match self {
            Self::Gzip => {
                let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
                encoder
                    .write_all(data)
                    .expect("writing to an in-memory Vec cannot fail");
                encoder.finish().expect("writing to an in-memory Vec cannot fail")
            }
            Self::Deflate => {
                let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
                encoder
                    .write_all(data)
                    .expect("writing to an in-memory Vec cannot fail");
                encoder.finish().expect("writing to an in-memory Vec cannot fail")
            }
            Self::Zstd => ruzstd::encoding::compress_to_vec(data, ruzstd::encoding::CompressionLevel::Fastest),
        }
    }

    pub(crate) fn decode(self, compressed: &[u8], limit: u64) -> io::Result<Vec<u8>> {
        match self {
            Self::Gzip => read_bounded_to_vec(flate2::read::MultiGzDecoder::new(compressed), limit),
            // Some servers send raw deflate instead of the zlib wrapper this encoding implies.
            Self::Deflate => read_bounded_to_vec(flate2::read::ZlibDecoder::new(compressed), limit)
                .or_else(|_| read_bounded_to_vec(flate2::read::DeflateDecoder::new(compressed), limit)),
            Self::Zstd => {
                let decoder = ruzstd::decoding::StreamingDecoder::new(compressed).map_err(io::Error::other)?;
                read_bounded_to_vec(decoder, limit)
            }
        }
    }
}

/// Reads `reader` into a `Vec`, erroring past `limit` bytes instead of buffering without bound
/// — protects against decompression bombs.
pub(crate) fn read_bounded_to_vec(mut reader: impl Read, limit: u64) -> io::Result<Vec<u8>> {
    let limit = limit as usize;
    let mut buf = Vec::new();
    let mut chunk = [0u8; 64 * 1024];

    loop {
        let n = reader.read(&mut chunk)?;
        if n == 0 {
            return Ok(buf);
        }
        if buf.len() + n > limit {
            return Err(io::Error::other(format!(
                "decompressed data exceeds the {limit}-byte limit"
            )));
        }
        buf.extend_from_slice(&chunk[..n]);
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[test]
    fn test_read_bounded_to_vec_within_limit() {
        assert_eq!(
            read_bounded_to_vec(b"hello world".as_slice(), 11).unwrap(),
            b"hello world"
        );
        assert_eq!(read_bounded_to_vec(b"".as_slice(), 0).unwrap(), b"");
    }

    #[test]
    fn test_read_bounded_to_vec_rejects_oversized_input() {
        assert!(read_bounded_to_vec(vec![0u8; 100].as_slice(), 50).is_err());
    }

    #[rstest]
    #[case::gzip("gzip", Algorithm::Gzip)]
    #[case::deflate("DEFLATE", Algorithm::Deflate)]
    #[case::zstd("Zstd", Algorithm::Zstd)]
    fn test_parse_is_case_insensitive(#[case] name: &str, #[case] expected: Algorithm) {
        assert_eq!(Algorithm::parse(name), Some(expected));
    }

    #[test]
    fn test_parse_rejects_unknown_name() {
        assert_eq!(Algorithm::parse("brotli"), None);
    }

    #[rstest]
    #[case(Algorithm::Gzip)]
    #[case(Algorithm::Deflate)]
    #[case(Algorithm::Zstd)]
    fn test_round_trip(#[case] algorithm: Algorithm) {
        let original = b"payload";
        let compressed = algorithm.encode(original);
        assert_eq!(algorithm.decode(&compressed, 1024).unwrap(), original);
    }

    #[rstest]
    #[case(Algorithm::Gzip)]
    #[case(Algorithm::Deflate)]
    #[case(Algorithm::Zstd)]
    fn test_rejects_garbage(#[case] algorithm: Algorithm) {
        assert!(algorithm.decode(b"not compressed data at all", 1024).is_err());
    }

    #[rstest]
    #[case(Algorithm::Gzip)]
    #[case(Algorithm::Deflate)]
    #[case(Algorithm::Zstd)]
    fn test_enforces_decompressed_limit(#[case] algorithm: Algorithm) {
        let original = vec![0u8; 1024 * 1024];
        let compressed = algorithm.encode(&original);
        assert!(compressed.len() < original.len() / 100);

        assert!(algorithm.decode(&compressed, 1024).is_err());
    }

    #[test]
    fn test_deflate_decodes_raw_deflate_without_zlib_wrapper() {
        let original = b"deflate payload";
        let mut encoder = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(original).unwrap();
        let compressed = encoder.finish().unwrap();

        assert_eq!(Algorithm::Deflate.decode(&compressed, 1024).unwrap(), original);
    }
}
