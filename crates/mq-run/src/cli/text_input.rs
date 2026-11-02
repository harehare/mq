//! Decoding of text input: UTF-8 (BOM stripped) and BOM-prefixed UTF-16.
use std::io::{self, BufRead, Read};

use miette::{IntoDiagnostic, miette};

const UTF8_BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];
const UTF16LE_BOM: [u8; 2] = [0xFF, 0xFE];
const UTF16BE_BOM: [u8; 2] = [0xFE, 0xFF];
const UTF32LE_BOM: [u8; 4] = [0xFF, 0xFE, 0x00, 0x00];
const UTF32BE_BOM: [u8; 4] = [0x00, 0x00, 0xFE, 0xFF];

/// Bytes sampled when guessing whether invalid input is UTF-16 without a BOM.
const SNIFF_LEN: usize = 256;

/// Longest BOM, i.e. how many leading bytes are needed to detect the encoding.
const MAX_BOM_LEN: u64 = 4;

type ToUnit = fn([u8; 2]) -> u16;

enum Encoding {
    Utf8,
    Utf8Bom,
    Utf16(ToUnit),
}

impl Encoding {
    fn bom_len(&self) -> usize {
        match self {
            Self::Utf8 => 0,
            Self::Utf8Bom => UTF8_BOM.len(),
            Self::Utf16(_) => UTF16LE_BOM.len(),
        }
    }
}

/// Detects the encoding from the leading bytes of the input.
fn detect(head: &[u8]) -> miette::Result<Encoding> {
    // UTF-32 BOMs are checked first because `FF FE 00 00` also starts with the UTF-16LE BOM.
    if head.starts_with(&UTF32LE_BOM) || head.starts_with(&UTF32BE_BOM) {
        return Err(miette!(
            help = "re-save the input as UTF-8, or as UTF-16 with a BOM",
            "UTF-32 input is not supported"
        ));
    }
    Ok(if head.starts_with(&UTF16LE_BOM) {
        Encoding::Utf16(u16::from_le_bytes)
    } else if head.starts_with(&UTF16BE_BOM) {
        Encoding::Utf16(u16::from_be_bytes)
    } else if head.starts_with(&UTF8_BOM) {
        Encoding::Utf8Bom
    } else {
        Encoding::Utf8
    })
}

/// Decodes `bytes` to text. A UTF-8 BOM is removed and UTF-16 is accepted only with a BOM.
pub(super) fn decode_text(mut bytes: Vec<u8>) -> miette::Result<String> {
    let encoding = detect(&bytes)?;
    let bom_len = encoding.bom_len();
    match encoding {
        Encoding::Utf16(to_unit) => decode_utf16(&bytes[bom_len..], to_unit),
        Encoding::Utf8 | Encoding::Utf8Bom => {
            bytes.drain(..bom_len);
            String::from_utf8(bytes).map_err(|e| invalid_utf8(e.as_bytes(), bom_len + e.utf8_error().valid_up_to()))
        }
    }
}

/// Returns the lines of `reader`, decoded like [`decode_text`] but without buffering the whole input.
pub(super) fn lines<'a, R: BufRead + 'a>(
    mut reader: R,
) -> miette::Result<Box<dyn Iterator<Item = miette::Result<String>> + 'a>> {
    let mut head = Vec::with_capacity(MAX_BOM_LEN as usize);
    (&mut reader)
        .take(MAX_BOM_LEN)
        .read_to_end(&mut head)
        .into_diagnostic()?;
    let encoding = detect(&head)?;
    let body = head.split_off(encoding.bom_len());
    let reader = io::Cursor::new(body).chain(reader);
    Ok(match encoding {
        Encoding::Utf8 | Encoding::Utf8Bom => Box::new(reader.lines().map(|line| line.into_diagnostic())),
        Encoding::Utf16(to_unit) => Box::new(Utf16Lines {
            reader,
            to_unit,
            done: false,
        }),
    })
}

struct Utf16Lines<R> {
    reader: R,
    to_unit: ToUnit,
    done: bool,
}

impl<R: BufRead> Utf16Lines<R> {
    fn read_unit(&mut self) -> miette::Result<Option<u16>> {
        if self.reader.fill_buf().into_diagnostic()?.is_empty() {
            return Ok(None);
        }
        let mut pair = [0u8; 2];
        self.reader.read_exact(&mut pair).map_err(|e| match e.kind() {
            io::ErrorKind::UnexpectedEof => ends_mid_character(),
            _ => miette!("{e}"),
        })?;
        Ok(Some((self.to_unit)(pair)))
    }

    fn read_char(&mut self) -> miette::Result<Option<char>> {
        let Some(unit) = self.read_unit()? else {
            return Ok(None);
        };
        let mut units = [unit, 0];
        let mut len = 1;
        if (0xD800..0xDC00).contains(&unit)
            && let Some(low) = self.read_unit()?
        {
            units[1] = low;
            len = 2;
        }
        char::decode_utf16(units[..len].iter().copied())
            .next()
            .transpose()
            .map_err(unpaired_surrogate)
    }
}

impl<R: BufRead> Iterator for Utf16Lines<R> {
    type Item = miette::Result<String>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        let mut line = String::new();
        let mut started = false;
        loop {
            match self.read_char() {
                Ok(None) => {
                    self.done = true;
                    return started.then_some(Ok(line));
                }
                Ok(Some('\n')) => {
                    if line.ends_with('\r') {
                        line.pop();
                    }
                    return Some(Ok(line));
                }
                Ok(Some(c)) => {
                    started = true;
                    line.push(c);
                }
                Err(e) => {
                    self.done = true;
                    return Some(Err(e));
                }
            }
        }
    }
}

fn ends_mid_character() -> miette::Report {
    miette!("input is not valid UTF-16: it ends in the middle of a character")
}

fn unpaired_surrogate(e: std::char::DecodeUtf16Error) -> miette::Report {
    miette!(
        "input is not valid UTF-16: unpaired surrogate U+{:04X}",
        e.unpaired_surrogate()
    )
}

fn decode_utf16(bytes: &[u8], to_unit: ToUnit) -> miette::Result<String> {
    let (pairs, rest) = bytes.as_chunks::<2>();
    if !rest.is_empty() {
        return Err(ends_mid_character());
    }
    let units = pairs.iter().map(|&pair| to_unit(pair));
    let mut out = String::with_capacity(bytes.len() / 2);
    for c in char::decode_utf16(units) {
        out.push(c.map_err(unpaired_surrogate)?);
    }
    Ok(out)
}

fn invalid_utf8(bytes: &[u8], offset: usize) -> miette::Report {
    let sample = &bytes[..bytes.len().min(SNIFF_LEN)];
    let help = if sample.iter().filter(|&&b| b == 0).count() * 4 >= sample.len().max(1) {
        "this looks like UTF-16 without a BOM. Re-save the input as UTF-8, or as UTF-16 with a BOM"
    } else {
        "this may be UTF-16 or another encoding. Re-save the input as UTF-8"
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
    #[case::other_encoding(vec![b'a', 0x83, 0x41], "another encoding")]
    fn test_decode_text_invalid_utf8_hint(#[case] bytes: Vec<u8>, #[case] hint: &str) {
        let err = decode_text(bytes).unwrap_err();
        assert!(err.to_string().contains("not valid UTF-8"));
        assert!(err.help().unwrap().to_string().contains(hint));
    }

    #[rstest]
    #[case::utf32le(vec![0xFF, 0xFE, 0x00, 0x00, 0x41, 0x00, 0x00, 0x00])]
    #[case::utf32be(vec![0x00, 0x00, 0xFE, 0xFF, 0x00, 0x00, 0x00, 0x41])]
    fn test_utf32_is_rejected(#[case] bytes: Vec<u8>) {
        assert!(decode_text(bytes.clone()).unwrap_err().to_string().contains("UTF-32"));
        assert!(
            lines(io::Cursor::new(bytes))
                .err()
                .unwrap()
                .to_string()
                .contains("UTF-32")
        );
    }

    #[test]
    fn test_utf16be_bom_followed_by_nul_is_utf16() {
        assert_eq!(decode_text(vec![0xFE, 0xFF, 0x00, 0x00]).unwrap(), "\0");
    }

    #[test]
    fn test_invalid_utf8_offset_includes_bom() {
        let err = decode_text(vec![0xEF, 0xBB, 0xBF, 0x41, 0xFF]).unwrap_err();
        assert!(err.to_string().contains("offset 4"), "{err}");
    }

    fn collect_lines(bytes: Vec<u8>) -> miette::Result<Vec<String>> {
        lines(io::Cursor::new(bytes))?.collect()
    }

    #[rstest]
    #[case::utf8_bom([&UTF8_BOM[..], b"# a\r\nb\n\nc"].concat())]
    #[case::utf16le([&UTF16LE_BOM[..], &utf16("# a\r\nb\n\nc", u16::to_le_bytes)].concat())]
    #[case::utf16be([&UTF16BE_BOM[..], &utf16("# a\r\nb\n\nc", u16::to_be_bytes)].concat())]
    #[case::plain(b"# a\r\nb\n\nc".to_vec())]
    fn test_lines(#[case] bytes: Vec<u8>) {
        assert_eq!(collect_lines(bytes).unwrap(), vec!["# a", "b", "", "c"]);
    }

    #[rstest]
    #[case::empty(vec![], 0)]
    #[case::utf16_empty(UTF16LE_BOM.to_vec(), 0)]
    #[case::trailing_newline(b"a\n".to_vec(), 1)]
    fn test_lines_count(#[case] bytes: Vec<u8>, #[case] expected: usize) {
        assert_eq!(collect_lines(bytes).unwrap().len(), expected);
    }

    #[test]
    fn test_lines_utf16_surrogate_pair() {
        let bytes = [&UTF16LE_BOM[..], &utf16("😀\n😀", u16::to_le_bytes)].concat();
        assert_eq!(collect_lines(bytes).unwrap(), vec!["😀", "😀"]);
    }

    #[rstest]
    #[case::odd_length(vec![0xFF, 0xFE, 0x41], "ends in the middle")]
    #[case::unpaired_surrogate(vec![0xFF, 0xFE, 0x00, 0xD8, 0x41, 0x00], "unpaired surrogate U+D800")]
    #[case::trailing_high_surrogate(vec![0xFF, 0xFE, 0x00, 0xD8], "unpaired surrogate U+D800")]
    fn test_lines_invalid_utf16(#[case] bytes: Vec<u8>, #[case] expected: &str) {
        assert!(collect_lines(bytes).unwrap_err().to_string().contains(expected));
    }
}
