//! Conversion between mq positions (1-based line, column in Unicode scalar values) and LSP
//! positions (0-based line, column in UTF-16 code units).

use std::sync::Arc;

use tower_lsp_server::ls_types::{Position, Range};

#[derive(Debug, Clone, Copy)]
struct Line {
    start: usize,
    end: usize,
    /// Scalar and UTF-16 columns coincide.
    ascii: bool,
}

/// Line boundaries of a document, built once per text.
#[derive(Debug, Clone)]
pub(crate) struct LineIndex {
    text: Arc<String>,
    lines: Vec<Line>,
}

impl LineIndex {
    pub(crate) fn new(text: Arc<String>) -> Self {
        let mut start = 0;
        let lines = text
            .split('\n')
            .map(|raw| {
                let line = raw.strip_suffix('\r').unwrap_or(raw);
                let entry = Line {
                    start,
                    end: start + line.len(),
                    ascii: line.is_ascii(),
                };
                start += raw.len() + 1;
                entry
            })
            .collect();
        Self { text, lines }
    }

    fn line_text(&self, line: u32) -> Option<(&str, bool)> {
        let line = self.lines.get(line as usize)?;
        Some((&self.text[line.start..line.end], line.ascii))
    }

    /// Converts an mq position. Columns past the end of the line clamp to its end.
    pub(crate) fn to_lsp(&self, position: mq_lang::Position) -> Position {
        let line = position.line.saturating_sub(1);
        let column = position.column.saturating_sub(1);
        let character = match self.line_text(line) {
            Some((text, true)) => column.min(text.len()),
            Some((text, false)) => text.chars().take(column).map(char::len_utf16).sum(),
            None => column,
        };
        Position::new(line, character as u32)
    }

    pub(crate) fn to_lsp_range(&self, range: mq_lang::Range) -> Range {
        Range::new(self.to_lsp(range.start), self.to_lsp(range.end))
    }

    /// Converts an LSP position. A position inside a surrogate pair rounds up to the next character.
    pub(crate) fn to_mq(&self, position: Position) -> mq_lang::Position {
        let column = match self.line_text(position.line) {
            Some((_, true)) | None => position.character as usize,
            Some((text, false)) => {
                let mut units = 0;
                text.chars()
                    .take_while(|c| {
                        let keep = units < position.character;
                        units += c.len_utf16() as u32;
                        keep
                    })
                    .count()
            }
        };
        mq_lang::Position::new(position.line + 1, column + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index(text: &str) -> LineIndex {
        LineIndex::new(Arc::new(text.to_string()))
    }

    fn mq(line: u32, column: usize) -> mq_lang::Position {
        mq_lang::Position::new(line, column)
    }

    #[rstest::rstest]
    #[case::ascii("abc", mq(1, 3), Position::new(0, 2))]
    #[case::bmp_wide("あい|x", mq(1, 4), Position::new(0, 3))]
    #[case::non_bmp("😀|x", mq(1, 3), Position::new(0, 3))]
    #[case::second_line("a\n😀b", mq(2, 2), Position::new(1, 2))]
    #[case::crlf("a\r\n😀b", mq(2, 2), Position::new(1, 2))]
    #[case::past_line_end("😀", mq(1, 9), Position::new(0, 2))]
    #[case::past_last_line("a", mq(4, 2), Position::new(3, 1))]
    fn test_to_lsp(#[case] text: &str, #[case] position: mq_lang::Position, #[case] expected: Position) {
        assert_eq!(index(text).to_lsp(position), expected);
    }

    #[rstest::rstest]
    #[case::ascii("abc", Position::new(0, 2), mq(1, 3))]
    #[case::non_bmp("😀|x", Position::new(0, 3), mq(1, 3))]
    #[case::inside_pair("😀|x", Position::new(0, 1), mq(1, 2))]
    #[case::second_line("a\n😀b", Position::new(1, 2), mq(2, 2))]
    #[case::unknown_line("a", Position::new(3, 1), mq(4, 2))]
    fn test_to_mq(#[case] text: &str, #[case] position: Position, #[case] expected: mq_lang::Position) {
        assert_eq!(index(text).to_mq(position), expected);
    }

    #[test]
    fn test_round_trip_after_non_bmp() {
        let index = index("\"😀\" | import \"csv\"");
        let lsp = Position::new(0, 14);
        assert_eq!(index.to_lsp(index.to_mq(lsp)), lsp);
    }
}
