//! Conversion between mq positions (1-based line, column in Unicode scalar values) and LSP
//! positions (0-based line, column in UTF-16 code units).

use std::sync::Arc;

use tower_lsp_server::ls_types::{Position, Range};

const BOM: char = '\u{FEFF}';

#[derive(Debug, Clone, Copy)]
struct Line {
    start: usize,
    end: usize,
    /// Scalar and UTF-16 columns coincide.
    ascii: bool,
    /// UTF-16 units the lexer does not count before the first column (a leading BOM).
    lead: usize,
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
                let bom = start == 0 && line.starts_with(BOM);
                let skip = if bom { BOM.len_utf8() } else { 0 };
                let content = &line[skip..];
                let entry = Line {
                    start: start + skip,
                    end: start + line.len(),
                    ascii: content.is_ascii(),
                    lead: if bom { BOM.len_utf16() } else { 0 },
                };
                start += raw.len() + 1;
                entry
            })
            .collect();
        Self { text, lines }
    }

    /// Line text without the BOM, whether it is ASCII, and the UTF-16 units of the BOM.
    fn line_text(&self, line: u32) -> Option<(&str, bool, usize)> {
        let line = self.lines.get(line as usize)?;
        Some((&self.text[line.start..line.end], line.ascii, line.lead))
    }

    /// Converts an mq position. Columns past the end of the line clamp to its end.
    pub(crate) fn to_lsp(&self, position: mq_lang::Position) -> Position {
        let line = position.line.saturating_sub(1);
        let column = position.column.saturating_sub(1);
        let character = match self.line_text(line) {
            Some((text, true, lead)) => column.min(text.len()) + lead,
            Some((text, false, lead)) => text.chars().take(column).map(char::len_utf16).sum::<usize>() + lead,
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
            None => position.character as usize,
            Some((_, true, lead)) => (position.character as usize).saturating_sub(lead),
            Some((text, false, lead)) => {
                let mut units = 0;
                let target = (position.character as usize).saturating_sub(lead) as u32;
                text.chars()
                    .take_while(|c| {
                        let keep = units < target;
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
    #[case::bom("\u{FEFF}def f()", mq(1, 5), Position::new(0, 5))]
    #[case::bom_non_ascii("\u{FEFF}😀|x", mq(1, 3), Position::new(0, 4))]
    #[case::bom_second_line("\u{FEFF}a\nb", mq(2, 2), Position::new(1, 1))]
    fn test_to_lsp(#[case] text: &str, #[case] position: mq_lang::Position, #[case] expected: Position) {
        assert_eq!(index(text).to_lsp(position), expected);
    }

    #[rstest::rstest]
    #[case::ascii("abc", Position::new(0, 2), mq(1, 3))]
    #[case::non_bmp("😀|x", Position::new(0, 3), mq(1, 3))]
    #[case::inside_pair("😀|x", Position::new(0, 1), mq(1, 2))]
    #[case::second_line("a\n😀b", Position::new(1, 2), mq(2, 2))]
    #[case::unknown_line("a", Position::new(3, 1), mq(4, 2))]
    #[case::bom("\u{FEFF}def f()", Position::new(0, 5), mq(1, 5))]
    #[case::bom_before_text("\u{FEFF}def f()", Position::new(0, 0), mq(1, 1))]
    #[case::bom_non_ascii("\u{FEFF}😀|x", Position::new(0, 4), mq(1, 3))]
    #[case::bom_second_line("\u{FEFF}a\nb", Position::new(1, 1), mq(2, 2))]
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
