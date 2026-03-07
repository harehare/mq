use ropey::Rope;

use crate::{
    Lexer, Module, Shared, Token,
    cst::{
        node::Node,
        parser::{ErrorReporter, Parser},
    },
    lexer,
};

/// Fraction of top-level nodes that must be affected to trigger a full re-parse.
const FULL_PARSE_THRESHOLD: f64 = 0.7;

/// Represents a text edit using **character** (Unicode scalar value) offsets.
///
/// Use this type when working with multi-byte source text, where character
/// counts differ from byte counts (e.g. Japanese, emoji, accented characters).
/// The offsets count Unicode scalar values (Rust `char`s), not bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEdit {
    /// Start character offset (inclusive).
    pub start: usize,
    /// End character offset (exclusive).
    pub end: usize,
    /// Replacement text.
    pub new_text: String,
}

impl TextEdit {
    /// Creates a new `TextEdit` with character offsets.
    pub fn new(start: usize, end: usize, new_text: impl Into<String>) -> Self {
        Self {
            start,
            end,
            new_text: new_text.into(),
        }
    }
}

/// An incremental CST parser that caches the previous parse result and
/// re-parses only the affected top-level statements when the source changes.
///
/// The source text is stored internally as a [`Rope`], which enables:
///
/// - **O(log n)** character-level insertions and deletions.
/// - **O(log n)** conversion between byte offsets and character offsets,
///   which is critical for correct handling of multi-byte (Unicode) text.
///
/// When the fraction of top-level nodes affected by a change exceeds
/// [`FULL_PARSE_THRESHOLD`], the parser falls back to a complete re-parse to
/// avoid overhead from splicing many incremental pieces together.
///
/// # Example
/// ```
/// use mq_lang::{IncrementalParser, TextEdit};
///
/// let mut parser = IncrementalParser::new("upcase() | downcase()");
/// let (nodes, errors) = parser.result();
/// // ... use nodes ...
///
/// // Apply a byte-offset edit
/// let edit = TextEdit::new(11, 19, "ltrim");
/// parser.apply_edit(&edit).unwrap();
/// assert_eq!(parser.source(), "upcase() | ltrim()");
///
/// // Apply a character-offset edit (safe for multi-byte text)
/// let mut parser2 = IncrementalParser::new("\"こんにちは\" | upcase()");
/// let edit2 = TextEdit::new(1, 6, "世界");
/// parser2.apply_edit(&edit2).unwrap();
/// assert_eq!(parser2.source(), "\"世界\" | upcase()");
/// ```
pub struct IncrementalParser {
    source: Rope,
    tokens: Vec<Shared<Token>>,
    nodes: Vec<Shared<Node>>,
    /// Token index ranges `[start, end)` for each top-level node group.
    /// Parallel to `nodes`: one entry per statement group.
    node_token_ranges: Vec<(usize, usize)>,
    errors: ErrorReporter,
}

impl IncrementalParser {
    /// Creates a new `IncrementalParser` by doing a full parse of `source`.
    pub fn new(source: &str) -> Self {
        let tokens = Self::lex(source);
        let (nodes, node_token_ranges, errors) = Self::parse_tokens(&tokens);
        Self {
            source: Rope::from_str(source),
            tokens,
            nodes,
            node_token_ranges,
            errors,
        }
    }

    /// Returns the current source text as an owned `String`.
    ///
    /// This is O(n) in the size of the source.
    pub fn source(&self) -> String {
        self.source.to_string()
    }

    /// Returns the total number of **bytes** in the source.
    pub fn byte_len(&self) -> usize {
        self.source.len_bytes()
    }

    /// Returns the total number of Unicode scalar values (characters) in the source.
    pub fn char_len(&self) -> usize {
        self.source.len_chars()
    }

    /// Converts a **byte** offset to a **character** offset.
    ///
    /// Returns `None` if `byte_offset` is out of range or does not fall on a
    /// UTF-8 character boundary (i.e. it points into the middle of a multi-byte
    /// character).
    pub fn byte_to_char_offset(&self, byte_offset: usize) -> Option<usize> {
        let len = self.source.len_bytes();
        if byte_offset > len {
            return None;
        }
        if !is_rope_char_boundary(&self.source, byte_offset) {
            return None;
        }
        Some(self.source.byte_to_char(byte_offset))
    }

    /// Converts a **character** offset to a **byte** offset.
    ///
    /// Returns `None` if `char_offset` is out of range.
    pub fn char_to_byte_offset(&self, char_offset: usize) -> Option<usize> {
        if char_offset > self.source.len_chars() {
            return None;
        }
        Some(self.source.char_to_byte(char_offset))
    }

    /// Returns the current CST nodes and error reporter.
    pub fn result(&self) -> (&[Shared<Node>], &ErrorReporter) {
        (&self.nodes, &self.errors)
    }

    /// Updates the parser with a new source string (full replacement).
    ///
    /// Internally performs an incremental update: the source is re-lexed,
    /// only the token range that changed is identified, and only the affected
    /// top-level node groups are re-parsed. All unaffected nodes are reused.
    ///
    /// If the proportion of affected nodes exceeds [`FULL_PARSE_THRESHOLD`], a
    /// complete re-parse is performed instead.
    ///
    /// Returns references to the updated nodes and errors.
    pub fn update(&mut self, new_source: &str) -> (&[Shared<Node>], &ErrorReporter) {
        let new_tokens = Self::lex(new_source);

        // Compute the unchanged prefix/suffix lengths by comparing token kinds.
        // Positions are ignored because they shift on every edit.
        let prefix = self
            .tokens
            .iter()
            .zip(new_tokens.iter())
            .take_while(|(a, b)| tokens_same_kind(a, b))
            .count();

        let old_suffix = self
            .tokens
            .iter()
            .rev()
            .zip(new_tokens.iter().rev())
            .take_while(|(a, b)| tokens_same_kind(a, b))
            .count();

        let old_changed_end = self.tokens.len().saturating_sub(old_suffix);
        let new_changed_end = new_tokens.len().saturating_sub(old_suffix);

        // Token structure is identical — just update the source rope.
        if prefix >= old_changed_end && prefix >= new_changed_end {
            self.source = Rope::from_str(new_source);
            self.tokens = new_tokens;
            return (&self.nodes, &self.errors);
        }

        // Identify the affected top-level node groups.
        let first_affected = self.node_token_ranges.partition_point(|&(_, end)| end <= prefix);
        let last_affected = self
            .node_token_ranges
            .partition_point(|&(start, _)| start < old_changed_end);

        // Fall back to a full parse when too many nodes are affected.
        let total_nodes = self.nodes.len();
        let affected_nodes = last_affected.saturating_sub(first_affected);
        let should_full_parse =
            total_nodes == 0 || (affected_nodes as f64 / total_nodes as f64) >= FULL_PARSE_THRESHOLD;

        if should_full_parse {
            return self.full_parse(new_source, new_tokens);
        }

        // Compute the token range in old tokens that the affected groups cover.
        let reparse_old_start = self
            .node_token_ranges
            .get(first_affected)
            .map(|r| r.0)
            .unwrap_or(prefix);
        let reparse_old_end = if last_affected > 0 {
            self.node_token_ranges
                .get(last_affected - 1)
                .map(|r| r.1)
                .unwrap_or(old_changed_end)
        } else {
            old_changed_end
        };

        // Compute the corresponding range in new tokens.
        // Tokens before `reparse_old_start` are unchanged → same index in new.
        let reparse_new_start = reparse_old_start;
        let delta = new_tokens.len() as isize - self.tokens.len() as isize;
        let reparse_new_end = ((reparse_old_end as isize) + delta).max(reparse_new_start as isize) as usize;

        // Re-parse only the affected token slice.
        let (new_nodes, new_ranges, _) = Self::parse_tokens_range(&new_tokens, reparse_new_start, reparse_new_end);

        // Adjust suffix node-group token ranges by the net token delta.
        for range in &mut self.node_token_ranges[last_affected..] {
            range.0 = ((range.0 as isize) + delta) as usize;
            range.1 = ((range.1 as isize) + delta) as usize;
        }

        // Splice in the new nodes and ranges.
        self.nodes.splice(first_affected..last_affected, new_nodes);
        self.node_token_ranges.splice(first_affected..last_affected, new_ranges);

        self.source = Rope::from_str(new_source);
        self.tokens = new_tokens;

        // Re-collect errors for the whole program after a structural change.
        let (_, _, errors) = Self::parse_tokens(&self.tokens);
        self.errors = errors;

        (&self.nodes, &self.errors)
    }

    /// Applies a **character-offset** [`TextEdit`] to the source and updates
    /// the parse.
    ///
    /// Character offsets count Unicode scalar values (Rust `char`s), so this
    /// method is always safe for multi-byte source text without any manual
    /// byte-offset arithmetic.
    ///
    /// Returns `Err` if the character offsets are out of range.
    pub fn apply_edit(&mut self, edit: &TextEdit) -> Result<(&[Shared<Node>], &ErrorReporter), String> {
        if edit.start > edit.end {
            return Err(format!("TextEdit: start ({}) > end ({})", edit.start, edit.end));
        }
        let char_len = self.source.len_chars();
        if edit.end > char_len {
            return Err(format!(
                "TextEdit: end ({}) out of range (source is {} chars)",
                edit.end, char_len
            ));
        }

        // Apply the edit on the rope (O(log n)).
        self.source.remove(edit.start..edit.end);
        self.source.insert(edit.start, &edit.new_text);

        let new_source = self.source.to_string();
        Ok(self.update(&new_source))
    }

    /// Performs a complete re-parse from `new_source` using `new_tokens`.
    fn full_parse(&mut self, new_source: &str, new_tokens: Vec<Shared<Token>>) -> (&[Shared<Node>], &ErrorReporter) {
        let (nodes, node_token_ranges, errors) = Self::parse_tokens(&new_tokens);
        self.source = Rope::from_str(new_source);
        self.tokens = new_tokens;
        self.nodes = nodes;
        self.node_token_ranges = node_token_ranges;
        self.errors = errors;
        (&self.nodes, &self.errors)
    }

    fn lex(source: &str) -> Vec<Shared<Token>> {
        Lexer::new(lexer::Options {
            ignore_errors: true,
            include_spaces: true,
        })
        .tokenize(source, Module::TOP_LEVEL_MODULE_ID)
        .unwrap_or_default()
        .into_iter()
        .map(Shared::new)
        .collect()
    }

    fn parse_tokens(tokens: &[Shared<Token>]) -> (Vec<Shared<Node>>, Vec<(usize, usize)>, ErrorReporter) {
        let mut parser = Parser::new(tokens);
        parser.parse_with_ranges()
    }

    /// Parses the token sub-slice `tokens[start..end]` and returns nodes with
    /// absolute token index ranges (relative to the full `tokens` slice).
    fn parse_tokens_range(
        tokens: &[Shared<Token>],
        start: usize,
        end: usize,
    ) -> (Vec<Shared<Node>>, Vec<(usize, usize)>, ErrorReporter) {
        let end = end.min(tokens.len());
        if start >= end {
            return (Vec::new(), Vec::new(), ErrorReporter::default());
        }

        let sub_slice = &tokens[start..end];
        let mut parser = Parser::new(sub_slice);
        let (nodes, ranges, errors) = parser.parse_with_ranges();

        // Adjust ranges from sub-slice-relative to absolute token indices.
        let adjusted: Vec<(usize, usize)> = ranges.into_iter().map(|(s, e)| (s + start, e + start)).collect();

        (nodes, adjusted, errors)
    }
}

/// Returns `true` if `byte_idx` falls on a UTF-8 character boundary within `rope`.
///
/// A byte is on a character boundary when it is either 0, `len_bytes()`, or
/// its value is not a UTF-8 continuation byte (continuation bytes have the
/// bit pattern `10xxxxxx`, i.e. `byte & 0xC0 == 0x80`).
fn is_rope_char_boundary(rope: &Rope, byte_idx: usize) -> bool {
    let len = rope.len_bytes();
    if byte_idx == 0 || byte_idx == len {
        return true;
    }
    if byte_idx > len {
        return false;
    }
    // Continuation bytes start with 10xxxxxx.
    rope.byte(byte_idx) & 0xC0 != 0x80
}

/// Compares two tokens by their `kind` only (ignoring positions and module IDs).
fn tokens_same_kind(a: &Shared<Token>, b: &Shared<Token>) -> bool {
    a.kind == b.kind
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_incremental_no_change() {
        let source = "upcase() | downcase()";
        let mut parser = IncrementalParser::new(source);
        let (nodes_before, _) = parser.result();
        let count_before = nodes_before.len();

        parser.update(source);
        let (nodes_after, _) = parser.result();
        assert_eq!(nodes_after.len(), count_before);
    }

    #[test]
    fn test_incremental_append() {
        let source = "upcase()";
        let mut parser = IncrementalParser::new(source);

        let (nodes, errors) = parser.update("upcase() | downcase()");
        assert!(!errors.has_errors());
        assert!(!nodes.is_empty());
    }

    #[test]
    fn test_incremental_def_change() {
        let source = "def foo(): \"hello\" end\n| upcase()";
        let mut parser = IncrementalParser::new(source);
        let (_, errors) = parser.result();
        assert!(!errors.has_errors());

        let (_, errors) = parser.update("def foo(): \"world\" end\n| upcase()");
        assert!(!errors.has_errors());
    }

    #[test]
    fn test_multibyte_char_edit() {
        // Source: `"こんにちは" | upcase()`
        // char offsets: 0='"', 1='こ', 2='ん', 3='に', 4='ち', 5='は', 6='"', ...
        let source = "\"こんにちは\" | upcase()";
        let mut parser = IncrementalParser::new(source);
        let (_, errors) = parser.result();
        assert!(!errors.has_errors());

        // Replace chars 1..6 ("こんにちは") with "世界" → `"世界" | upcase()`
        let edit = TextEdit::new(1, 6, "世界");
        let (_, errors) = parser.apply_edit(&edit).unwrap();
        assert!(!errors.has_errors());
        assert_eq!(parser.source(), "\"世界\" | upcase()");
    }

    #[test]
    fn test_multibyte_byte_to_char_offset() {
        // "# こんにちは": "# " = 2 bytes; "こ" starts at byte 2.
        let source = "# こんにちは";
        let parser = IncrementalParser::new(source);

        assert_eq!(parser.byte_to_char_offset(0), Some(0));
        assert_eq!(parser.byte_to_char_offset(2), Some(2)); // start of "こ"
        // Byte 3 is inside "こ" (3-byte char) → not a boundary.
        assert_eq!(parser.byte_to_char_offset(3), None);
        assert_eq!(parser.byte_to_char_offset(5), Some(3)); // start of "ん"
    }

    #[test]
    fn test_multibyte_char_to_byte_offset() {
        // "# こんにちは": "# " = 2 bytes; each Japanese char = 3 bytes.
        let source = "# こんにちは";
        let parser = IncrementalParser::new(source);

        assert_eq!(parser.char_to_byte_offset(0), Some(0));
        assert_eq!(parser.char_to_byte_offset(2), Some(2)); // byte start of "こ"
        assert_eq!(parser.char_to_byte_offset(3), Some(5)); // byte start of "ん"
        assert_eq!(parser.char_to_byte_offset(7), Some(17)); // past end
    }

    #[test]
    fn test_apply_char_edit_out_of_range() {
        let source = "abc";
        let mut parser = IncrementalParser::new(source);

        let edit = TextEdit::new(0, 10, "x"); // only 3 chars
        assert!(parser.apply_edit(&edit).is_err());
    }

    #[test]
    fn test_full_parse_fallback_triggered() {
        // Source with several distinct statements so we have multiple nodes.
        let source = "upcase() | downcase() | ltrim() | rtrim() | length()";
        let mut parser = IncrementalParser::new(source);
        let (_, errors) = parser.result();
        assert!(!errors.has_errors());

        // Completely different source — should exceed threshold and full-parse.
        let new_source = "starts_with(\"x\") | ends_with(\"y\") | contains(\"z\")";
        let (nodes, errors) = parser.update(new_source);
        assert!(!errors.has_errors());
        assert!(!nodes.is_empty());
        assert_eq!(parser.source(), new_source);
    }

    #[test]
    fn test_full_parse_on_empty_nodes() {
        // When there are no existing nodes, full parse should be used.
        let mut parser = IncrementalParser::new("");
        let (nodes, errors) = parser.update("upcase()");
        assert!(!errors.has_errors());
        assert!(!nodes.is_empty());
    }
}
