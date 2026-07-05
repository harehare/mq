//! Textual auto-fixes for lint diagnostics.
//!
//! Rules only see the HIR, not raw source text, so a [`Fix`] records ranges rather than strings
//! and is resolved against the source later, wherever it's available (the CLI or the LSP).

/// The core replacement text for a [`Fix`], before `prefix`/`suffix` are applied.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Core {
    Literal(String),
    Verbatim(mq_lang::Range),
    Concat(Vec<Core>),
}

impl Core {
    fn resolve(&self, source: &str) -> Option<String> {
        match self {
            Core::Literal(text) => Some(text.clone()),
            Core::Verbatim(range) => Some(slice(source, *range)?.trim().to_string()),
            Core::Concat(parts) => parts
                .iter()
                .map(|part| part.resolve(source))
                .collect::<Option<Vec<_>>>()
                .map(|parts| parts.concat()),
        }
    }
}

/// A suggested rewrite for the span of a diagnostic: `range` gets replaced with `prefix` +
/// resolved core text + `suffix`.
#[derive(Debug, Clone, PartialEq)]
pub struct Fix {
    pub range: mq_lang::Range,
    core: Core,
    pub prefix: String,
    pub suffix: String,
}

impl Fix {
    /// A fix that replaces `range` with the source text spanned by `verbatim`, unchanged.
    pub fn verbatim(range: mq_lang::Range, verbatim: mq_lang::Range) -> Self {
        Self {
            range,
            core: Core::Verbatim(verbatim),
            prefix: String::new(),
            suffix: String::new(),
        }
    }

    /// A fix that replaces `range` with fixed `text` known at rule-check time.
    pub fn literal(range: mq_lang::Range, text: impl Into<String>) -> Self {
        Self {
            range,
            core: Core::Literal(text.into()),
            prefix: String::new(),
            suffix: String::new(),
        }
    }

    /// A fix that replaces `range` with several literal/verbatim `parts` concatenated together,
    /// for rewrites that reorder or splice more than one sub-expression (see
    /// [`crate::fix::Core`]).
    pub(crate) fn concat(range: mq_lang::Range, parts: Vec<Core>) -> Self {
        Self {
            range,
            core: Core::Concat(parts),
            prefix: String::new(),
            suffix: String::new(),
        }
    }

    pub fn with_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.prefix = prefix.into();
        self
    }

    pub fn with_suffix(mut self, suffix: impl Into<String>) -> Self {
        self.suffix = suffix.into();
        self
    }

    /// Resolves this fix against `source`, returning the range to replace and its replacement
    /// text.
    ///
    /// A `Verbatim` core is trimmed, since rules can often only bound one side of such a range by
    /// a sibling token rather than the expression's own true end (see [`crate::LintContext::full_range`]).
    pub fn resolve(&self, source: &str) -> Option<(mq_lang::Range, String)> {
        let core = self.core.resolve(source)?;
        Some((self.range, format!("{}{}{}", self.prefix, core, self.suffix)))
    }
}

/// Converts a 1-based line/column [`mq_lang::Position`] (column counted in `char`s) into a byte
/// offset into `source`.
fn position_to_byte_offset(source: &str, position: mq_lang::Position) -> Option<usize> {
    let mut offset = 0;
    let mut lines = source.split_inclusive('\n');

    for _ in 1..position.line {
        offset += lines.next()?.len();
    }
    let line = lines.next().unwrap_or("");

    let mut column = 1;
    for (i, _) in line.char_indices() {
        if column == position.column {
            return Some(offset + i);
        }
        column += 1;
    }
    if column == position.column {
        return Some(offset + line.trim_end_matches('\n').len());
    }

    None
}

/// The smallest range spanning both `a` and `b`.
pub fn union(a: mq_lang::Range, b: mq_lang::Range) -> mq_lang::Range {
    let start = if (a.start.line, a.start.column) <= (b.start.line, b.start.column) {
        a.start
    } else {
        b.start
    };
    let end = if (a.end.line, a.end.column) >= (b.end.line, b.end.column) {
        a.end
    } else {
        b.end
    };
    mq_lang::Range { start, end }
}

/// Extracts the substring of `source` spanned by `range`.
pub fn slice(source: &str, range: mq_lang::Range) -> Option<&str> {
    let start = position_to_byte_offset(source, range.start)?;
    let end = position_to_byte_offset(source, range.end)?;
    if start > end {
        return None;
    }
    source.get(start..end)
}

/// Applies a set of resolved `(range, replacement)` edits to `source`, returning the new text.
///
/// Edits are applied from the end of the source towards the start so earlier byte offsets stay
/// valid; if two edits overlap, the one starting earlier wins and the other is dropped.
pub fn apply_edits(source: &str, edits: &[(mq_lang::Range, String)]) -> String {
    let mut spans: Vec<(usize, usize, &str)> = edits
        .iter()
        .filter_map(|(range, text)| {
            let start = position_to_byte_offset(source, range.start)?;
            let end = position_to_byte_offset(source, range.end)?;
            (start <= end).then_some((start, end, text.as_str()))
        })
        .collect();
    spans.sort_by_key(|(start, ..)| std::cmp::Reverse(*start));

    let mut result = source.to_string();
    let mut applied_start = usize::MAX;
    for (start, end, text) in spans {
        if end > applied_start {
            continue;
        }
        result.replace_range(start..end, text);
        applied_start = start;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn range(start_line: u32, start_col: usize, end_line: u32, end_col: usize) -> mq_lang::Range {
        mq_lang::Range {
            start: mq_lang::Position {
                line: start_line,
                column: start_col,
            },
            end: mq_lang::Position {
                line: end_line,
                column: end_col,
            },
        }
    }

    #[test]
    fn slice_extracts_single_line_span() {
        let source = ".checked == false";
        assert_eq!(slice(source, range(1, 1, 1, 9)), Some(".checked"));
    }

    #[test]
    fn slice_extracts_across_lines() {
        let source = "let x =\n  .h1\n| x";
        assert_eq!(slice(source, range(2, 3, 2, 6)), Some(".h1"));
    }

    #[test]
    fn slice_handles_multibyte_columns() {
        let source = r#"s"${あ}""#;
        // The interpolated expr `あ` starts at char column 5 (after `s`, `"`, `$`, `{`).
        assert_eq!(slice(source, range(1, 5, 1, 6)), Some("あ"));
    }

    #[test]
    fn resolve_applies_prefix_and_suffix() {
        let source = ".checked == false";
        let fix = Fix::verbatim(range(1, 1, 1, 18), range(1, 1, 1, 9)).with_prefix("!");
        assert_eq!(fix.resolve(source), Some((range(1, 1, 1, 18), "!.checked".to_string())));
    }

    #[test]
    fn resolve_uses_literal_core() {
        let source = r#"s"${x}""#;
        let fix = Fix::literal(range(1, 1, 1, 8), "x");
        assert_eq!(fix.resolve(source), Some((range(1, 1, 1, 8), "x".to_string())));
    }

    #[test]
    fn apply_edits_splices_single_edit() {
        let source = ".checked == false";
        let edits = vec![(range(1, 1, 1, 18), "!.checked".to_string())];
        assert_eq!(apply_edits(source, &edits), "!.checked");
    }

    #[test]
    fn apply_edits_handles_multiple_non_overlapping_edits() {
        let source = "try: get(\"x\") catch: none\n| .checked == true";
        let edits = vec![
            (range(1, 1, 1, 26), "get(\"x\")?".to_string()),
            (range(2, 3, 2, 19), ".checked".to_string()),
        ];
        assert_eq!(apply_edits(source, &edits), "get(\"x\")?\n| .checked");
    }

    #[test]
    fn apply_edits_drops_overlapping_edit() {
        let source = ".checked == false";
        let edits = vec![
            (range(1, 1, 1, 18), "!.checked".to_string()),
            (range(1, 1, 1, 9), ".checked".to_string()),
        ];
        // The two edits overlap; only the one starting first (lowest offset) is kept.
        assert_eq!(apply_edits(source, &edits), "!.checked");
    }
}
