use crate::{
    Lexer, Module, Shared, Token,
    cst::{
        node::Node,
        parser::{ErrorReporter, Parser},
    },
    lexer,
};

/// Represents a text edit as a byte offset range and replacement text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEdit {
    /// Start byte offset (inclusive).
    pub start: usize,
    /// End byte offset (exclusive).
    pub end: usize,
    /// Replacement text.
    pub new_text: String,
}

impl TextEdit {
    /// Creates a new `TextEdit`.
    pub fn new(start: usize, end: usize, new_text: impl Into<String>) -> Self {
        Self {
            start,
            end,
            new_text: new_text.into(),
        }
    }
}

/// An incremental CST parser that caches the previous parse result and re-parses
/// only the affected top-level statements when the source changes.
///
/// # Example
/// ```
/// use mq_lang::IncrementalParser;
///
/// let mut parser = IncrementalParser::new("upcase() | downcase()");
/// let (nodes, errors) = parser.result();
/// // ... use nodes ...
///
/// // Apply a text edit
/// parser.update("upcase() | ltrim()");
/// let (nodes, errors) = parser.result();
/// ```
pub struct IncrementalParser {
    source: String,
    tokens: Vec<Shared<Token>>,
    nodes: Vec<Shared<Node>>,
    /// Token index ranges `[start, end)` for each top-level node group.
    /// Parallel to `nodes` in terms of grouping: one range per statement group.
    node_token_ranges: Vec<(usize, usize)>,
    errors: ErrorReporter,
}

impl IncrementalParser {
    /// Creates a new `IncrementalParser` by doing a full parse of `source`.
    pub fn new(source: &str) -> Self {
        let tokens = Self::lex(source);
        let (nodes, node_token_ranges, errors) = Self::parse_tokens(&tokens);
        Self {
            source: source.to_string(),
            tokens,
            nodes,
            node_token_ranges,
            errors,
        }
    }

    /// Returns the current source text.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Returns the current CST nodes and error reporter.
    pub fn result(&self) -> (&[Shared<Node>], &ErrorReporter) {
        (&self.nodes, &self.errors)
    }

    /// Updates the parser with a new source string.
    ///
    /// This is an incremental update: it re-lexes the entire source (fast),
    /// finds the changed token range by comparing old and new token sequences,
    /// identifies the affected top-level node groups, re-parses only those
    /// groups, and reuses all other nodes.
    ///
    /// Returns references to the updated nodes and errors.
    pub fn update(&mut self, new_source: &str) -> (&[Shared<Node>], &ErrorReporter) {
        let new_tokens = Self::lex(new_source);

        // Find the changed region by comparing token kinds.
        // We compare kinds (not positions) because positions shift with every edit.
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

        // If token structure is identical, just update positions and source.
        if prefix >= old_changed_end && prefix >= new_changed_end {
            self.source = new_source.to_string();
            self.tokens = new_tokens;
            return (&self.nodes, &self.errors);
        }

        // Find the affected top-level node groups.
        // A group is affected if its token range overlaps [prefix, old_changed_end).
        let first_affected = self.node_token_ranges.partition_point(|&(_, end)| end <= prefix);
        let last_affected = self
            .node_token_ranges
            .partition_point(|&(start, _)| start < old_changed_end);

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
        // Tokens before `reparse_old_start` are unchanged → same count in new tokens.
        let reparse_new_start = reparse_old_start;
        // Tokens at old_suffix distance from the end are unchanged.
        let delta = new_tokens.len() as isize - self.tokens.len() as isize;
        let reparse_new_end = ((reparse_old_end as isize) + delta).max(reparse_new_start as isize) as usize;

        // Re-parse the affected token slice using the new full token array.
        // We start parsing from reparse_new_start and parse until reparse_new_end.
        let (new_nodes, new_ranges, _) = Self::parse_tokens_range(&new_tokens, reparse_new_start, reparse_new_end);

        // Adjust suffix group token ranges by the delta.
        for range in &mut self.node_token_ranges[last_affected..] {
            range.0 = ((range.0 as isize) + delta) as usize;
            range.1 = ((range.1 as isize) + delta) as usize;
        }

        // Splice in the new nodes and ranges.
        self.nodes.splice(first_affected..last_affected, new_nodes);
        self.node_token_ranges.splice(first_affected..last_affected, new_ranges);

        self.source = new_source.to_string();
        self.tokens = new_tokens;

        // Re-run error collection on the affected region by doing a lightweight
        // full re-parse of errors only (re-use existing errors for unaffected regions).
        // For simplicity, we re-parse the entire source for errors when the structure changed.
        let (_, _, errors) = Self::parse_tokens(&self.tokens);
        self.errors = errors;

        (&self.nodes, &self.errors)
    }

    /// Applies a byte-offset-based [`TextEdit`] to the source and updates the parse.
    pub fn apply_edit(&mut self, edit: &TextEdit) -> (&[Shared<Node>], &ErrorReporter) {
        let mut new_source = self.source.clone();
        new_source.replace_range(edit.start..edit.end, &edit.new_text);
        self.update(&new_source)
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

    /// Parses tokens in the range `[start, end)` within `tokens`, treating this
    /// sub-slice as a root-level program. Returns the parsed nodes with their
    /// absolute token index ranges (relative to `tokens`, not the sub-slice).
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

        // Adjust ranges to be absolute (relative to `tokens`).
        let adjusted_ranges: Vec<(usize, usize)> = ranges.into_iter().map(|(s, e)| (s + start, e + start)).collect();

        (nodes, adjusted_ranges, errors)
    }
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
        let source = "upcase | downcase";
        let mut parser = IncrementalParser::new(source);
        let (nodes_before, _) = parser.result();
        let count_before = nodes_before.len();

        parser.update(source);
        let (nodes_after, _) = parser.result();
        assert_eq!(nodes_after.len(), count_before);
    }

    #[test]
    fn test_incremental_append() {
        let source = "upcase";
        let mut parser = IncrementalParser::new(source);

        let (nodes, errors) = parser.update("upcase | downcase");
        assert!(!errors.has_errors());
        assert!(!nodes.is_empty());
    }

    #[test]
    fn test_apply_edit() {
        let source = "upcase | downcase";
        let mut parser = IncrementalParser::new(source);

        // Replace "downcase" with "ltrim"
        let edit = TextEdit::new(9, 17, "ltrim");
        let (nodes, errors) = parser.apply_edit(&edit);
        assert!(!errors.has_errors());
        assert!(!nodes.is_empty());
        assert_eq!(parser.source(), "upcase | ltrim");
    }

    #[test]
    fn test_incremental_def_change() {
        let source = "def foo(): \"hello\" end\n| upcase";
        let mut parser = IncrementalParser::new(source);
        let (_, errors) = parser.result();
        assert!(!errors.has_errors());

        // Change the def body
        let (_, errors) = parser.update("def foo(): \"world\" end\n| upcase");
        assert!(!errors.has_errors());
    }
}
