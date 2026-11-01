//! Decoding of text input: UTF-8 (BOM stripped) and BOM-prefixed UTF-16.
use miette::miette;

const UTF8_BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];
const UTF16LE_BOM: [u8; 2] = [0xFF, 0xFE];
const UTF16BE_BOM: [u8; 2] = [0xFE, 0xFF];
const UTF32LE_BOM: [u8; 4] = [0xFF, 0xFE, 0x00, 0x00];

/// Bytes sampled when guessing whether invalid input is UTF-16 without a BOM.
const SNIFF_LEN: usize = 256;

/// Decodes `bytes` to text. A UTF-8 BOM is removed and UTF-16 is accepted only with a BOM.
pub(super) fn decode_text(mut bytes: Vec<u8>) -> miette::Result<String> {
    if !bytes.starts_with(&UTF32LE_BOM) {
        if let Some(rest) = bytes.strip_prefix(&UTF16LE_BOM) {
            return decode_utf16(rest, u16::from_le_bytes);
        }
        if let Some(rest) = bytes.strip_prefix(&UTF16BE_BOM) {
            return decode_utf16(rest, u16::from_be_bytes);
        }
    }
    if bytes.starts_with(&UTF8_BOM) {
        bytes.drain(..UTF8_BOM.len());
    }
    String::from_utf8(bytes).map_err(|e| invalid_utf8(e.as_bytes(), e.utf8_error().valid_up_to()))
}

fn decode_utf16(bytes: &[u8], to_unit: fn([u8; 2]) -> u16) -> miette::Result<String> {
    let (pairs, rest) = bytes.as_chunks::<2>();
    if !rest.is_empty() {
        return Err(miette!(
            "input is not valid UTF-16: it ends in the middle of a character"
        ));
    }
    let units = pairs.iter().map(|&pair| to_unit(pair));
    let mut out = String::with_capacity(bytes.len() / 2);
    for c in char::decode_utf16(units) {
        out.push(c.map_err(|e| {
            miette!(
                "input is not valid UTF-16: unpaired surrogate U+{:04X}",
                e.unpaired_surrogate()
            )
        })?);
    }
    Ok(out)
}

fn invalid_utf8(bytes: &[u8], offset: usize) -> miette::Report {
    let sample = &bytes[..bytes.len().min(SNIFF_LEN)];
    let help = if sample.iter().filter(|&&b| b == 0).count() * 4 >= sample.len().max(1) {
        "this looks like UTF-16 without a BOM. Convert it first, e.g. `iconv -f UTF-16LE -t UTF-8 file | mq ...` (UTF-16BE for big endian)"
    } else {
        "this may be UTF-16 or another encoding. Convert it first, e.g. `iconv -f <encoding> -t UTF-8 file | mq ...`"
    };
    miette!(
        help = help,
        "input is not valid UTF-8 (first invalid byte at offset {offset})"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    fn utf16(text: &str, to_bytes: fn(u16) -> [u8; 2]) -> Vec<u8> {
        text.encode_utf16().flat_map(to_bytes).collect()
    }

    #[rstest]
    #[case::plain(b"# h\n".to_vec(), "# h\n")]
    #[case::utf8_bom([&UTF8_BOM[..], b"# h"].concat(), "# h")]
    #[case::utf16le([&UTF16LE_BOM[..], &utf16("# あ😀", u16::to_le_bytes)].concat(), "# あ😀")]
    #[case::utf16be([&UTF16BE_BOM[..], &utf16("# あ😀", u16::to_be_bytes)].concat(), "# あ😀")]
    #[case::utf16_empty(UTF16LE_BOM.to_vec(), "")]
    fn test_decode_text(#[case] bytes: Vec<u8>, #[case] expected: &str) {
        assert_eq!(decode_text(bytes).unwrap(), expected);
    }

    #[rstest]
    #[case::odd_length(vec![0xFF, 0xFE, 0x41], "ends in the middle")]
    #[case::unpaired_surrogate(vec![0xFF, 0xFE, 0x00, 0xD8, 0x41, 0x00], "unpaired surrogate U+D800")]
    fn test_decode_text_invalid_utf16(#[case] bytes: Vec<u8>, #[case] expected: &str) {
        assert!(decode_text(bytes).unwrap_err().to_string().contains(expected));
    }

    #[rstest]
    #[case::no_bom_utf16(utf16("# é\n", u16::to_le_bytes), "UTF-16 without a BOM")]
    #[case::utf32_bom(vec![0xFF, 0xFE, 0x00, 0x00, 0x41, 0x00, 0x00, 0x00], "UTF-16 without a BOM")]
    #[case::other_encoding(vec![b'a', 0x83, 0x41], "another encoding")]
    fn test_decode_text_invalid_utf8_hint(#[case] bytes: Vec<u8>, #[case] hint: &str) {
        let err = decode_text(bytes).unwrap_err();
        assert!(err.to_string().contains("not valid UTF-8"));
        assert!(err.help().unwrap().to_string().contains(hint));
    }
}
