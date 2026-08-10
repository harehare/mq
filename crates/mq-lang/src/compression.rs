//! Shared gzip/deflate/zstd (de)compression primitives, backing both the `compress`/
//! `decompress` builtins and the `http()` response path's `deflate`/`zstd` decoding
//! (`gzip` there is decoded by ureq itself; see [`crate::io::native`]).

use std::io::{self, Read, Write};

/// Reads `reader` into a `Vec`, erroring once more than `limit` bytes have been produced
/// instead of buffering an unbounded amount — protects against decompression bombs.
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

pub(crate) fn encode_gzip(data: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder
        .write_all(data)
        .expect("writing to an in-memory Vec cannot fail");
    encoder.finish().expect("writing to an in-memory Vec cannot fail")
}

pub(crate) fn decode_gzip(reader: impl Read, limit: u64) -> io::Result<Vec<u8>> {
    read_bounded_to_vec(flate2::read::MultiGzDecoder::new(reader), limit)
}

pub(crate) fn encode_deflate(data: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    encoder
        .write_all(data)
        .expect("writing to an in-memory Vec cannot fail");
    encoder.finish().expect("writing to an in-memory Vec cannot fail")
}

/// Some servers send raw deflate instead of the zlib wrapper `Content-Encoding: deflate` implies.
pub(crate) fn decode_deflate(compressed: &[u8], limit: u64) -> io::Result<Vec<u8>> {
    read_bounded_to_vec(flate2::read::ZlibDecoder::new(compressed), limit)
        .or_else(|_| read_bounded_to_vec(flate2::read::DeflateDecoder::new(compressed), limit))
}

pub(crate) fn encode_zstd(data: &[u8]) -> Vec<u8> {
    ruzstd::encoding::compress_to_vec(data, ruzstd::encoding::CompressionLevel::Fastest)
}

pub(crate) fn decode_zstd(reader: impl Read, limit: u64) -> io::Result<Vec<u8>> {
    let decoder = ruzstd::decoding::StreamingDecoder::new(reader).map_err(io::Error::other)?;
    read_bounded_to_vec(decoder, limit)
}

#[cfg(test)]
mod tests {
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

    #[test]
    fn test_gzip_round_trip() {
        let original = b"gzip payload";
        let compressed = encode_gzip(original);
        assert_eq!(decode_gzip(compressed.as_slice(), 1024).unwrap(), original);
    }

    #[test]
    fn test_gzip_rejects_garbage() {
        assert!(decode_gzip(&b"not gzip data"[..], 1024).is_err());
    }

    #[test]
    fn test_gzip_enforces_decompressed_limit() {
        let original = vec![0u8; 1024 * 1024];
        let compressed = encode_gzip(&original);
        assert!(compressed.len() < original.len() / 100);

        assert!(decode_gzip(compressed.as_slice(), 1024).is_err());
    }

    #[test]
    fn test_deflate_round_trip() {
        let original = b"deflate payload";
        let compressed = encode_deflate(original);
        assert_eq!(decode_deflate(&compressed, 1024).unwrap(), original);
    }

    #[test]
    fn test_deflate_raw_fallback() {
        // Some servers send raw deflate under `Content-Encoding: deflate`, without the zlib wrapper.
        let original = b"deflate payload";
        let mut encoder = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(original).unwrap();
        let compressed = encoder.finish().unwrap();

        assert_eq!(decode_deflate(&compressed, 1024).unwrap(), original);
    }

    #[test]
    fn test_deflate_rejects_garbage() {
        assert!(decode_deflate(b"not compressed data at all", 1024).is_err());
    }

    #[test]
    fn test_deflate_enforces_decompressed_limit() {
        let original = vec![0u8; 1024 * 1024];
        let compressed = encode_deflate(&original);
        assert!(compressed.len() < original.len() / 100);

        assert!(decode_deflate(&compressed, 1024).is_err());
    }

    #[test]
    fn test_zstd_round_trip() {
        let original = b"zstd payload";
        let compressed = encode_zstd(original);
        assert_eq!(decode_zstd(compressed.as_slice(), 1024).unwrap(), original);
    }

    #[test]
    fn test_zstd_rejects_garbage() {
        assert!(decode_zstd(&b"not zstd data"[..], 1024).is_err());
    }

    #[test]
    fn test_zstd_enforces_decompressed_limit() {
        let original = vec![0u8; 1024 * 1024];
        let compressed = encode_zstd(&original);
        assert!(compressed.len() < original.len() / 100);

        assert!(decode_zstd(compressed.as_slice(), 1024).is_err());
    }
}
