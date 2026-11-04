//! Byte-level helpers shared by the block and inline parsers.

/// A byte range `start..end` of a source text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Span {
    pub(super) start: usize,
    pub(super) end: usize,
}

impl Span {
    pub(super) fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// The text of the range in `src`.
    pub(super) fn of(self, src: &str) -> &str {
        &src[self.start..self.end]
    }
}

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

#[cfg(kani)]
mod proofs {
    use super::*;

    #[kani::proof]
    fn eol_len_matches_the_bytes_at_index() {
        let bytes: [u8; 4] = kani::any();
        let index: usize = kani::any();
        let len = eol_len(&bytes, index);
        assert!(len <= 2);
        if len == 2 {
            assert_eq!(&bytes[index..index + 2], b"\r\n");
        }
        if index >= bytes.len() {
            assert_eq!(len, 0);
        }
    }

    #[kani::proof]
    #[kani::unwind(6)]
    fn skip_blanks_returns_the_end_of_a_blank_run() {
        let bytes: [u8; 4] = kani::any();
        let index: usize = kani::any();
        let end = skip_blanks(&bytes, index);
        assert!(end >= index);
        if index <= bytes.len() {
            assert!(end <= bytes.len());
        }
        assert!((index..end).all(|i| matches!(bytes[i], b' ' | b'\t')));
    }

    #[kani::proof]
    #[kani::unwind(6)]
    fn skip_whitespace_returns_the_end_of_a_whitespace_run() {
        let bytes: [u8; 4] = kani::any();
        let index: usize = kani::any();
        let end = skip_whitespace(&bytes, index);
        assert!(end >= index);
        if index <= bytes.len() {
            assert!(end <= bytes.len());
        }
        assert!((index..end).all(|i| is_whitespace(bytes[i])));
    }

    /// Past one line ending, only blanks are skipped, so at most one line ending is crossed.
    #[kani::proof]
    #[kani::unwind(6)]
    fn skip_blanks_and_eol_crosses_at_most_one_line_ending() {
        let bytes: [u8; 4] = kani::any();
        let index: usize = kani::any();
        let end = skip_blanks_and_eol(&bytes, index);
        assert!(end >= index);
        if index <= bytes.len() {
            assert!(end <= bytes.len());
        }
        let crossed = (index..end)
            .filter(|i| bytes[*i] == b'\n' || (bytes[*i] == b'\r' && !bytes[*i + 1..].starts_with(b"\n")))
            .count();
        assert!(crossed <= 1);
    }
}
