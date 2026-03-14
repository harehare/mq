use ropey::Rope;

use crate::{
    Lexer, Module, Shared, Token,
    cst::{
        node::Node,
        parser::{ErrorReporter, ParseWithRangesResult, Parser},
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
#[derive(Debug)]
pub struct IncrementalParser {
    source: Rope,
    tokens: Vec<Shared<Token>>,
    nodes: Vec<Shared<Node>>,
    /// Token index ranges `[start, end)` for each top-level node group.
    node_token_ranges: Vec<(usize, usize)>,
    /// Node index ranges `[start, end)` for each top-level node group,
    /// parallel to `node_token_ranges`. Used to splice the correct node
    /// sub-slice during incremental updates (a group may contain more than
    /// one node, e.g. an expression node followed by a Pipe or Eof node).
    node_index_ranges: Vec<(usize, usize)>,
    errors: ErrorReporter,
}

impl IncrementalParser {
    /// Creates a new `IncrementalParser` by doing a full parse of `source`.
    pub fn new(source: &str) -> Self {
        let tokens = Self::lex(source);
        let ParseWithRangesResult {
            nodes,
            token_ranges: node_token_ranges,
            node_ranges: node_index_ranges,
            errors,
        } = Self::parse_tokens(&tokens);
        Self {
            source: Rope::from_str(source),
            tokens,
            nodes,
            node_token_ranges,
            node_index_ranges,
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

        let raw_suffix = self
            .tokens
            .iter()
            .rev()
            .zip(new_tokens.iter().rev())
            .take_while(|(a, b)| tokens_same_kind(a, b))
            .count();

        // Prevent the suffix from overlapping with the already-matched prefix.
        // Without this clamp, tokens could be counted in both the prefix AND the
        // suffix, causing the changed region to appear empty when it is not.
        let old_suffix = raw_suffix.min(self.tokens.len().saturating_sub(prefix));
        let new_suffix = raw_suffix.min(new_tokens.len().saturating_sub(prefix));

        let old_changed_end = self.tokens.len().saturating_sub(old_suffix);
        let new_changed_end = new_tokens.len().saturating_sub(new_suffix);

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

        // Fall back to a full parse when too many node groups are affected.
        let total_groups = self.node_token_ranges.len();
        let affected_groups = last_affected.saturating_sub(first_affected);
        let should_full_parse =
            total_groups == 0 || (affected_groups as f64 / total_groups as f64) >= FULL_PARSE_THRESHOLD;

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
        let ParseWithRangesResult {
            nodes: new_nodes,
            token_ranges: new_token_ranges,
            node_ranges: new_node_index_ranges,
            ..
        } = Self::parse_tokens_range(&new_tokens, reparse_new_start, reparse_new_end);

        // Determine the node sub-slice covered by the affected groups so we
        // splice exactly the right entries (a group can have >1 node).
        let node_splice_start = self
            .node_index_ranges
            .get(first_affected)
            .map(|r| r.0)
            .unwrap_or(self.nodes.len());
        let node_splice_end = if last_affected > 0 {
            self.node_index_ranges
                .get(last_affected - 1)
                .map(|r| r.1)
                .unwrap_or(self.nodes.len())
        } else {
            node_splice_start
        };

        // Adjust suffix node-group token ranges by the net token delta.
        for range in &mut self.node_token_ranges[last_affected..] {
            range.0 = ((range.0 as isize) + delta) as usize;
            range.1 = ((range.1 as isize) + delta) as usize;
        }

        // Adjust suffix node index ranges by the node count delta.
        let new_group_node_count: usize = new_node_index_ranges.last().map(|r| r.1).unwrap_or(0);
        let old_group_node_count = node_splice_end - node_splice_start;
        let node_delta = new_group_node_count as isize - old_group_node_count as isize;
        for range in &mut self.node_index_ranges[last_affected..] {
            range.0 = ((range.0 as isize) + node_delta) as usize;
            range.1 = ((range.1 as isize) + node_delta) as usize;
        }

        // Adjust absolute node index ranges for newly-inserted groups.
        let adjusted_new_node_ranges: Vec<(usize, usize)> = new_node_index_ranges
            .into_iter()
            .map(|(s, e)| (s + node_splice_start, e + node_splice_start))
            .collect();

        // Splice in the new nodes and ranges.
        self.nodes.splice(node_splice_start..node_splice_end, new_nodes);
        self.node_token_ranges
            .splice(first_affected..last_affected, new_token_ranges);
        self.node_index_ranges
            .splice(first_affected..last_affected, adjusted_new_node_ranges);

        self.source = Rope::from_str(new_source);
        self.tokens = new_tokens;

        // Re-collect errors for the whole program after a structural change.
        self.errors = Self::parse_tokens(&self.tokens).errors;

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
        let ParseWithRangesResult {
            nodes,
            token_ranges: node_token_ranges,
            node_ranges: node_index_ranges,
            errors,
        } = Self::parse_tokens(&new_tokens);
        self.source = Rope::from_str(new_source);
        self.tokens = new_tokens;
        self.nodes = nodes;
        self.node_token_ranges = node_token_ranges;
        self.node_index_ranges = node_index_ranges;
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

    fn parse_tokens(tokens: &[Shared<Token>]) -> ParseWithRangesResult {
        let mut parser = Parser::new(tokens);
        parser.parse_with_ranges()
    }

    /// Parses the token sub-slice `tokens[start..end]` and returns nodes with
    /// absolute token index ranges (relative to the full `tokens` slice).
    /// Node index ranges are returned sub-slice-relative (starting from 0).
    fn parse_tokens_range(tokens: &[Shared<Token>], start: usize, end: usize) -> ParseWithRangesResult {
        let end = end.min(tokens.len());
        if start >= end {
            return ParseWithRangesResult::default();
        }

        let sub_slice = &tokens[start..end];
        let mut parser = Parser::new(sub_slice);
        let ParseWithRangesResult {
            nodes,
            token_ranges,
            node_ranges,
            errors,
        } = parser.parse_with_ranges();

        // Adjust token ranges from sub-slice-relative to absolute token indices.
        let token_ranges = token_ranges.into_iter().map(|(s, e)| (s + start, e + start)).collect();

        // Node index ranges are kept sub-slice-relative (caller offsets them as needed).
        ParseWithRangesResult {
            nodes,
            token_ranges,
            node_ranges,
            errors,
        }
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

#[cfg(test)]
mod prop_tests {
    use proptest::collection::vec;
    use proptest::prelude::*;

    use super::*;

    /// Reconstructs the source text from a slice of CST nodes by walking the
    /// node tree and concatenating trivia and token text in order.
    ///
    /// This mirrors how the lexer/parser consume tokens, so the result should
    /// round-trip back to the original source for any well-formed input.
    fn nodes_to_source(nodes: &[Shared<Node>]) -> String {
        use crate::lexer::token::TokenKind;

        fn node_text(node: &Node, out: &mut String) {
            for trivia in &node.leading_trivia {
                out.push_str(&trivia.to_string());
            }
            if let Some(token) = &node.token {
                // `StringLiteral` stores only the inner value; re-add the quotes.
                if let TokenKind::StringLiteral(s) = &token.kind {
                    out.push('"');
                    out.push_str(s);
                    out.push('"');
                } else {
                    out.push_str(&token.kind.to_string());
                }
            }
            for child in &node.children {
                node_text(child, out);
            }
            for trivia in &node.trailing_trivia {
                out.push_str(&trivia.to_string());
            }
        }

        let mut out = String::new();
        for node in nodes {
            node_text(node, &mut out);
        }
        out
    }

    // Strategies
    /// Generates a string literal value (the text *inside* the quotes), including
    /// ASCII words, multibyte Japanese text, emoji, and Greek letters.
    fn string_value() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("hello"),
            Just("world"),
            Just("foo"),
            Just("bar"),
            // multibyte
            Just("こんにちは"),
            Just("世界"),
            Just("日本語"),
            Just("🦀"),
            Just("αβγδ"),
            Just("résumé"),
            Just("中文"),
            Just("한국어"),
        ]
        .prop_map(|s| s.to_string())
    }

    /// Generates a quoted string literal token, e.g. `"hello"` or `"こんにちは"`.
    fn string_literal() -> impl Strategy<Value = String> {
        string_value().prop_map(|s| format!("\"{}\"", s))
    }

    /// Generates one of the well-known zero-argument built-in function calls.
    fn zero_arg_builtin() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("upcase()"),
            Just("downcase()"),
            Just("ltrim()"),
            Just("rtrim()"),
            Just("length()"),
            Just("to_number()"),
            Just("to_string()"),
            Just("keys()"),
            Just("values()"),
            Just("flatten()"),
            Just("not()"),
            Just("ascii_downcase()"),
            Just("ascii_upcase()"),
        ]
        .prop_map(|s| s.to_string())
    }

    /// Generates built-in calls that take a single string argument, including
    /// multibyte string arguments.
    fn one_string_arg_builtin() -> impl Strategy<Value = String> {
        (
            prop_oneof![Just("starts_with"), Just("ends_with"), Just("contains"), Just("split"),],
            string_literal(),
        )
            .prop_map(|(func, arg)| format!("{}({})", func, arg))
    }

    /// Generates built-in calls that take two string arguments.
    fn two_string_arg_builtin() -> impl Strategy<Value = String> {
        (string_literal(), string_literal()).prop_map(|(s1, s2)| format!("replace({}, {})", s1, s2))
    }

    /// Generates a `starts_with` or `ends_with` call with a multibyte argument.
    fn multibyte_arg_builtin() -> impl Strategy<Value = String> {
        (
            prop_oneof![Just("starts_with"), Just("ends_with"), Just("contains"),],
            prop_oneof![
                Just("\"こんにちは\""),
                Just("\"世界\""),
                Just("\"🦀\""),
                Just("\"αβγ\""),
                Just("\"日本語\""),
            ],
        )
            .prop_map(|(func, arg)| format!("{}({})", func, arg))
    }

    /// Generates any kind of single built-in call (zero-arg, one-arg, two-arg, multibyte).
    fn builtin_call() -> impl Strategy<Value = String> {
        prop_oneof![
            3 => zero_arg_builtin(),
            3 => one_string_arg_builtin(),
            1 => two_string_arg_builtin(),
            2 => multibyte_arg_builtin(),
        ]
    }

    /// Generates a simple `def` expression with no parameters and an ASCII body,
    /// e.g. `def greet(): upcase() end`.
    fn def_expr() -> impl Strategy<Value = String> {
        (
            prop_oneof![Just("greet"), Just("process"), Just("transform"), Just("my_func"),],
            zero_arg_builtin(),
        )
            .prop_map(|(name, body)| format!("def {}(): {} end", name, body))
    }

    /// Generates a `def` with a single string-arg body.
    fn def_with_arg_expr() -> impl Strategy<Value = String> {
        (
            prop_oneof![Just("check"), Just("test_fn"), Just("my_check"),],
            one_string_arg_builtin(),
        )
            .prop_map(|(name, body)| format!("def {}(): {} end", name, body))
    }

    /// Generates a simple `if` expression (condition is a builtin call that
    /// returns a boolean, branch is another builtin call).
    fn if_expr() -> impl Strategy<Value = String> {
        (
            prop_oneof![
                Just("starts_with(\"a\")"),
                Just("ends_with(\"z\")"),
                Just("contains(\"x\")"),
                Just("starts_with(\"こ\")"),
                Just("ends_with(\"界\")"),
            ],
            zero_arg_builtin(),
            zero_arg_builtin(),
        )
            .prop_map(|(cond, then_branch, else_branch)| {
                format!("if ({}): {} else: {} end", cond, then_branch, else_branch)
            })
    }

    /// Generates a pipe-chain of 1–4 built-in calls, e.g. `"upcase() | ltrim()"`.
    fn pipe_chain() -> impl Strategy<Value = String> {
        vec(builtin_call(), 1..=4).prop_map(|calls| calls.join(" | "))
    }

    /// Generates a program that may include def, if, and pipe expressions.
    fn mixed_program() -> impl Strategy<Value = String> {
        prop_oneof![
            2 => pipe_chain(),
            1 => def_expr(),
            1 => def_with_arg_expr(),
            1 => if_expr(),
            // def followed by a pipe chain
            1 => (def_expr(), pipe_chain())
                .prop_map(|(d, p)| format!("{}\n| {}", d, p)),
        ]
    }

    // source() always reflects the last source passed to update()
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(200))]
        #[test]
        fn prop_source_preserved_after_update(
            src1 in pipe_chain(),
            src2 in pipe_chain(),
        ) {
            let mut parser = IncrementalParser::new(&src1);
            parser.update(&src2);
            prop_assert_eq!(parser.source(), src2);
        }
    }

    // multiple sequential updates: source() == the last one
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(150))]
        #[test]
        fn prop_multiple_updates_preserve_last_source(
            s1 in pipe_chain(),
            s2 in pipe_chain(),
            s3 in pipe_chain(),
        ) {
            let mut parser = IncrementalParser::new(&s1);
            parser.update(&s2);
            parser.update(&s3);
            prop_assert_eq!(parser.source(), s3);
        }
    }

    // incremental update produces same node count as a fresh parse
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(200))]
        #[test]
        fn prop_incremental_node_count_matches_fresh(
            src1 in pipe_chain(),
            src2 in pipe_chain(),
        ) {
            let mut incremental = IncrementalParser::new(&src1);
            let (inc_nodes, _) = incremental.update(&src2);
            let inc_count = inc_nodes.len();

            let fresh = IncrementalParser::new(&src2);
            let (fresh_nodes, _) = fresh.result();
            prop_assert_eq!(
                inc_count,
                fresh_nodes.len(),
                "incremental node count {} != fresh parse node count {} for src={:?}",
                inc_count,
                fresh_nodes.len(),
                src2,
            );
        }
    }

    // updating with the same source is idempotent
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(200))]
        #[test]
        fn prop_idempotent_update(src in pipe_chain()) {
            let mut parser = IncrementalParser::new(&src);
            let (nodes_first, _) = parser.result();
            let count_first = nodes_first.len();

            parser.update(&src);
            let (nodes_second, _) = parser.result();
            prop_assert_eq!(nodes_second.len(), count_first);
            prop_assert_eq!(parser.source(), src);
        }
    }

    // incremental parse never produces more errors than a fresh parse
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(200))]
        #[test]
        fn prop_incremental_error_status_matches_fresh(
            src1 in pipe_chain(),
            src2 in pipe_chain(),
        ) {
            let mut incremental = IncrementalParser::new(&src1);
            let (_, inc_errors) = incremental.update(&src2);
            let inc_has_errors = inc_errors.has_errors();

            let fresh = IncrementalParser::new(&src2);
            let (_, fresh_errors) = fresh.result();
            prop_assert_eq!(
                inc_has_errors,
                fresh_errors.has_errors(),
                "error mismatch after incremental update for src={:?}",
                src2
            );
        }
    }

    // apply_edit (full replace) produces correct source
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(200))]
        #[test]
        fn prop_apply_edit_full_replace_source(
            base in pipe_chain(),
            replacement in pipe_chain(),
        ) {
            let mut parser = IncrementalParser::new(&base);
            let char_len = parser.char_len();
            let edit = TextEdit::new(0, char_len, replacement.clone());
            let result = parser.apply_edit(&edit);
            prop_assert!(result.is_ok(), "apply_edit failed: {:?}", result.err());
            prop_assert_eq!(parser.source(), replacement);
        }
    }

    // two sequential apply_edits: source reflects both
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(150))]
        #[test]
        fn prop_sequential_apply_edits(
            base   in pipe_chain(),
            mid    in pipe_chain(),
            target in pipe_chain(),
        ) {
            let mut parser = IncrementalParser::new(&base);

            // First edit: replace entire source with `mid`
            let len1 = parser.char_len();
            parser.apply_edit(&TextEdit::new(0, len1, mid)).unwrap();

            // Second edit: replace entire source with `target`
            let len2 = parser.char_len();
            parser.apply_edit(&TextEdit::new(0, len2, target.clone())).unwrap();

            prop_assert_eq!(parser.source(), target);
        }
    }

    // ---------------------------------------------------------------------------
    // Property 8 – nodes_to_source round-trip for fresh parse
    //
    // Verifies that the CST faithfully preserves all token and trivia text so
    // that reconstructing the source from nodes gives back the original string.
    // ---------------------------------------------------------------------------

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(200))]
        #[test]
        fn prop_nodes_to_source_roundtrip(src in pipe_chain()) {
            let parser = IncrementalParser::new(&src);
            let (nodes, _) = parser.result();
            let reconstructed = nodes_to_source(nodes);
            prop_assert_eq!(
                reconstructed,
                src,
                "nodes_to_source did not reproduce the original source"
            );
        }
    }

    // ---------------------------------------------------------------------------
    // Property 9 – no panic on arbitrary (possibly invalid) source strings
    // ---------------------------------------------------------------------------

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(300))]
        #[test]
        fn prop_no_panic_on_arbitrary_source(src in ".*", updated in ".*") {
            let mut parser = IncrementalParser::new(&src);
            let _ = parser.result();
            // update with another arbitrary string also must not panic
            let _ = parser.update(&updated);
        }
    }

    // ---------------------------------------------------------------------------
    // Property 10 – apply_edit with multibyte replacement: source is correct
    // ---------------------------------------------------------------------------

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(150))]
        #[test]
        fn prop_apply_edit_multibyte_replacement(
            base in pipe_chain(),
            // Replace the whole source with a short multibyte string
            insert in prop_oneof![
                Just("こんにちは".to_string()),
                Just("世界".to_string()),
                Just("🦀".to_string()),
                Just("αβγ".to_string()),
            ],
        ) {
            let mut parser = IncrementalParser::new(&base);
            let char_len = parser.char_len();
            let edit = TextEdit::new(0, char_len, insert.clone());
            let result = parser.apply_edit(&edit);
            prop_assert!(result.is_ok());
            prop_assert_eq!(parser.source(), insert);
        }
    }

    // ---------------------------------------------------------------------------
    // Property 11 – builtin_call with multibyte string arguments: no parse error
    // ---------------------------------------------------------------------------

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(200))]
        #[test]
        fn prop_multibyte_arg_builtin_no_error(src in multibyte_arg_builtin()) {
            let parser = IncrementalParser::new(&src);
            let (_, errors) = parser.result();
            prop_assert!(
                !errors.has_errors(),
                "unexpected parse error for src={:?}",
                src
            );
        }
    }

    // ---------------------------------------------------------------------------
    // Property 12 – one_string_arg_builtin: no parse error
    // ---------------------------------------------------------------------------

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(200))]
        #[test]
        fn prop_one_string_arg_builtin_no_error(src in one_string_arg_builtin()) {
            let parser = IncrementalParser::new(&src);
            let (_, errors) = parser.result();
            prop_assert!(
                !errors.has_errors(),
                "unexpected parse error for src={:?}",
                src
            );
        }
    }

    // ---------------------------------------------------------------------------
    // Property 13 – two_string_arg_builtin: no parse error
    // ---------------------------------------------------------------------------

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(150))]
        #[test]
        fn prop_two_string_arg_builtin_no_error(src in two_string_arg_builtin()) {
            let parser = IncrementalParser::new(&src);
            let (_, errors) = parser.result();
            prop_assert!(
                !errors.has_errors(),
                "unexpected parse error for src={:?}",
                src
            );
        }
    }

    // ---------------------------------------------------------------------------
    // Property 14 – def_expr: no parse error, source preserved after update
    // ---------------------------------------------------------------------------

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(150))]
        #[test]
        fn prop_def_expr_no_error(src in def_expr()) {
            let parser = IncrementalParser::new(&src);
            let (_, errors) = parser.result();
            prop_assert!(
                !errors.has_errors(),
                "unexpected parse error for def src={:?}",
                src
            );
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(150))]
        #[test]
        fn prop_def_with_arg_no_error(src in def_with_arg_expr()) {
            let parser = IncrementalParser::new(&src);
            let (_, errors) = parser.result();
            prop_assert!(
                !errors.has_errors(),
                "unexpected parse error for def-with-arg src={:?}",
                src
            );
        }
    }

    // ---------------------------------------------------------------------------
    // Property 15 – if_expr: no parse error
    // ---------------------------------------------------------------------------

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(150))]
        #[test]
        fn prop_if_expr_no_error(src in if_expr()) {
            let parser = IncrementalParser::new(&src);
            let (_, errors) = parser.result();
            prop_assert!(
                !errors.has_errors(),
                "unexpected parse error for if src={:?}",
                src
            );
        }
    }

    // ---------------------------------------------------------------------------
    // Property 16 – mixed_program: incremental node count matches fresh parse
    // ---------------------------------------------------------------------------

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(200))]
        #[test]
        fn prop_mixed_program_incremental_matches_fresh(
            src1 in mixed_program(),
            src2 in mixed_program(),
        ) {
            let mut incremental = IncrementalParser::new(&src1);
            let (inc_nodes, _) = incremental.update(&src2);
            let inc_count = inc_nodes.len();

            let fresh = IncrementalParser::new(&src2);
            let (fresh_nodes, _) = fresh.result();
            prop_assert_eq!(
                inc_count,
                fresh_nodes.len(),
                "node count mismatch: incremental={} fresh={} src={:?}",
                inc_count,
                fresh_nodes.len(),
                src2,
            );
        }
    }

    // ---------------------------------------------------------------------------
    // Property 17 – mixed_program: error status matches fresh parse
    // ---------------------------------------------------------------------------

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(200))]
        #[test]
        fn prop_mixed_program_error_status_matches_fresh(
            src1 in mixed_program(),
            src2 in mixed_program(),
        ) {
            let mut incremental = IncrementalParser::new(&src1);
            let (_, inc_errors) = incremental.update(&src2);
            let inc_has_errors = inc_errors.has_errors();

            let fresh = IncrementalParser::new(&src2);
            let (_, fresh_errors) = fresh.result();
            prop_assert_eq!(
                inc_has_errors,
                fresh_errors.has_errors(),
                "error status mismatch after incremental update for src={:?}",
                src2,
            );
        }
    }

    // ---------------------------------------------------------------------------
    // Property 18 – pipe_chain (with string args): nodes_to_source round-trip
    //
    // `nodes_to_source` faithfully reconstructs sources composed of built-in
    // calls (including those with string arguments).  Structural expressions
    // such as `def`/`if` are exercised in separate consistency properties.
    // ---------------------------------------------------------------------------

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(200))]
        #[test]
        fn prop_pipe_chain_with_args_roundtrip(src in pipe_chain()) {
            let parser = IncrementalParser::new(&src);
            let (nodes, _) = parser.result();
            let reconstructed = nodes_to_source(nodes);
            prop_assert_eq!(
                reconstructed,
                src,
                "nodes_to_source did not reproduce the original source"
            );
        }
    }

    // ---------------------------------------------------------------------------
    // Property 19 – apply_edit with multibyte builtin-arg source: source correct
    // ---------------------------------------------------------------------------

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(150))]
        #[test]
        fn prop_apply_edit_on_multibyte_arg_source(
            base  in multibyte_arg_builtin(),
            target in multibyte_arg_builtin(),
        ) {
            let mut parser = IncrementalParser::new(&base);
            let char_len = parser.char_len();
            let edit = TextEdit::new(0, char_len, target.clone());
            let result = parser.apply_edit(&edit);
            prop_assert!(result.is_ok(), "apply_edit failed: {:?}", result.err());
            prop_assert_eq!(parser.source(), target);
        }
    }

    // ---------------------------------------------------------------------------
    // Property 20 – partial edit inside a multibyte string argument
    //
    // Replaces only the inner string value of a `starts_with("X")` call while
    // leaving the surrounding syntax intact.  Verifies that the source is correct
    // and that the parser does not panic.
    // ---------------------------------------------------------------------------

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(150))]
        #[test]
        fn prop_partial_edit_multibyte_string_arg(
            prefix in prop_oneof![
                Just("starts_with"),
                Just("ends_with"),
                Just("contains"),
            ],
            old_val in prop_oneof![
                Just("hello"),
                Just("world"),
                Just("abc"),
            ],
            new_val in prop_oneof![
                Just("こんにちは"),
                Just("世界"),
                Just("🦀"),
                Just("αβγ"),
                Just("日本語"),
            ],
        ) {
            // Build e.g. `starts_with("hello")`
            let src = format!("{}(\"{}\")", prefix, old_val);
            let mut parser = IncrementalParser::new(&src);
            let (_, errors) = parser.result();
            prop_assert!(!errors.has_errors(), "initial parse error for src={:?}", src);

            // Locate the char offsets of `old_val` inside the quotes.
            // Layout: prefix ( " old_val " )
            //          0..n   n  n+1  ...
            let quote_start = parser.source().chars().position(|c| c == '"').unwrap() + 1;
            let quote_end = quote_start + old_val.chars().count();

            let edit = TextEdit::new(quote_start, quote_end, new_val);
            let result = parser.apply_edit(&edit);
            prop_assert!(result.is_ok(), "apply_edit failed: {:?}", result.err());

            let expected = format!("{}(\"{}\")", prefix, new_val);
            prop_assert_eq!(parser.source(), expected);
        }
    }
}
