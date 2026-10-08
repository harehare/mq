//! Byte-level helpers shared by the block and inline parsers.

/// The length of the line ending at `index`, or 0 when there is none.
pub(super) fn eol_len(bytes: &[u8], index: usize) -> usize {
    match bytes.get(index..) {
        Some([b'\r', b'\n', ..]) => 2,
        Some([b'\r' | b'\n', ..]) => 1,
        _ => 0,
    }
}

/// Whether `byte` is a space, a tab or a line ending.
pub(super) fn is_whitespace(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r')
}

/// The offset after the spaces and tabs at `index`.
pub(super) fn skip_blanks(bytes: &[u8], mut index: usize) -> usize {
    while matches!(bytes.get(index), Some(b' ' | b'\t')) {
        index += 1;
    }
    index
}

/// The offset after the spaces, tabs and line endings at `index`.
pub(super) fn skip_whitespace(bytes: &[u8], mut index: usize) -> usize {
    while bytes.get(index).copied().is_some_and(is_whitespace) {
        index += 1;
    }
    index
}

/// The offset after the spaces and tabs at `index`, including at most one line ending among them.
pub(super) fn skip_blanks_and_eol(bytes: &[u8], index: usize) -> usize {
    let index = skip_blanks(bytes, index);
    match eol_len(bytes, index) {
        0 => index,
        eol => skip_blanks(bytes, index + eol),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case::none("a", 0, 0)]
    #[case::lf("a\nb", 1, 1)]
    #[case::cr("a\rb", 1, 1)]
    #[case::crlf("a\r\nb", 1, 2)]
    #[case::past_end("a", 5, 0)]
    fn eol_len_at(#[case] text: &str, #[case] index: usize, #[case] expected: usize) {
        assert_eq!(eol_len(text.as_bytes(), index), expected);
    }

    #[rstest]
    #[case::blanks(" \t a", 0, 3)]
    #[case::one_eol(" \n a", 0, 3)]
    #[case::crlf(" \r\n\ta", 0, 4)]
    #[case::two_eols(" \n\n a", 0, 2)]
    #[case::nothing("a", 0, 0)]
    fn skip_blanks_and_eol_at(#[case] text: &str, #[case] index: usize, #[case] expected: usize) {
        assert_eq!(skip_blanks_and_eol(text.as_bytes(), index), expected);
    }

    #[test]
    fn skip_whitespace_crosses_line_endings() {
        assert_eq!(skip_whitespace(b" \n\r\n\ta", 0), 5);
    }
}
