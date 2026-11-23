//! Generates mutants of a module's source by replacing single tokens.

use mq_syntax::CstNode;
use mq_syntax::{Shared, Token, TokenKind};
use rustc_hash::FxHashSet;

/// One single-token replacement of a module's source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Mutant {
    /// One-based line of the replaced token.
    pub(crate) line: usize,
    /// One-based column of the replaced token.
    pub(crate) column: usize,
    /// The original token text.
    pub(crate) original: String,
    /// The replacement text.
    pub(crate) replacement: String,
    /// Byte range of the original token in the module source.
    from: usize,
    to: usize,
}

impl Mutant {
    /// Returns `source` with the replacement applied.
    pub(crate) fn apply(&self, source: &str) -> String {
        format!("{}{}{}", &source[..self.from], self.replacement, &source[self.to..])
    }

    /// Short description such as `` `==` -> `!=` ``.
    pub(crate) fn describe(&self) -> String {
        if self.replacement.is_empty() {
            format!("remove `{}`", self.original)
        } else {
            format!("`{}` -> `{}`", self.original, self.replacement)
        }
    }
}

/// Returns the replacement for `token` whose source text is `text`, if it is mutable.
fn replacement_for(kind: &TokenKind, text: &str) -> Option<String> {
    let swapped = match kind {
        TokenKind::EqEq => "!=",
        TokenKind::NeEq => "==",
        TokenKind::Lt => ">=",
        TokenKind::Lte => ">",
        TokenKind::Gt => "<=",
        TokenKind::Gte => "<",
        TokenKind::Plus => "-",
        TokenKind::Minus => "+",
        TokenKind::Asterisk => "/",
        TokenKind::Slash => "*",
        TokenKind::And => "||",
        TokenKind::Or => "&&",
        TokenKind::Not => "",
        TokenKind::BoolLiteral(true) => "false",
        TokenKind::BoolLiteral(false) => "true",
        TokenKind::NumberLiteral(_) => return text.parse::<i64>().ok().map(|n| n.saturating_add(1).to_string()),
        _ => return None,
    };
    Some(swapped.to_string())
}

/// Comment that excludes the code it annotates from mutation.
const SKIP_ANNOTATION: &str = "@mutate(skip)";

/// Tokens of the tree, and the `(line, last line of the annotated node)` of each skip comment.
#[derive(Default)]
struct Collected {
    tokens: Vec<Shared<Token>>,
    skips: Vec<(usize, usize)>,
}

fn collect(nodes: &[Shared<CstNode>], out: &mut Collected) {
    for node in nodes {
        if let Some(token) = &node.token {
            out.tokens.push(Shared::clone(token));
        }
        let end = node.node_range().end.line as usize;
        for trivia in node.leading_trivia.iter().chain(node.trailing_trivia.iter()) {
            if trivia.comment().is_some_and(|c| c.trim() == SKIP_ANNOTATION) {
                out.skips.push((trivia.range().start.line as usize, end));
            }
        }
        collect(&node.all_children(), out);
    }
}

/// Byte offset of the one-based `(line, column)` in `source`; columns count characters.
fn offset_of(source: &str, line: u32, column: u32) -> Option<usize> {
    let mut start = 0;
    for _ in 1..line {
        start += source[start..].find('\n')? + 1;
    }
    let rest = &source[start..];
    let skip = (column as usize).checked_sub(1)?;
    let offset = rest.char_indices().nth(skip).map_or(rest.len(), |(i, _)| i);
    Some(start + offset)
}

/// Generates every single-token mutant of `source` that still parses.
/// Lines for which `is_candidate` returns false are skipped.
pub(crate) fn generate(source: &str, is_candidate: impl Fn(usize) -> bool) -> Vec<Mutant> {
    let (nodes, errors) = mq_syntax::parse_recovery(source);
    if errors.has_errors() {
        return Vec::new();
    }
    let mut collected = Collected::default();
    collect(&nodes, &mut collected);
    let mut tokens = collected.tokens;
    let token_lines: FxHashSet<usize> = tokens.iter().map(|t| t.range.start.line as usize).collect();
    // A comment after code skips its own line; one on its own line skips the next node.
    let skipped: FxHashSet<usize> = collected
        .skips
        .iter()
        .flat_map(|&(line, end)| {
            if token_lines.contains(&line) {
                line..=line
            } else {
                line + 1..=end.max(line + 1)
            }
        })
        .collect();
    tokens.sort_by_key(|token| (token.range.start.line, token.range.start.column));
    tokens.dedup_by_key(|token| (token.range.start.line, token.range.start.column));

    let mut mutants = Vec::new();
    for token in tokens {
        let (start, end) = (token.range.start, token.range.end);
        if start.line != end.line || skipped.contains(&(start.line as usize)) || !is_candidate(start.line as usize) {
            continue;
        }
        let (Some(from), Some(to)) = (
            offset_of(source, start.line, start.column),
            offset_of(source, end.line, end.column),
        ) else {
            continue;
        };
        let Some(text) = source.get(from..to) else { continue };
        let Some(replacement) = replacement_for(&token.kind, text) else {
            continue;
        };
        let mutant = Mutant {
            line: start.line as usize,
            column: start.column as usize,
            original: text.to_string(),
            replacement,
            from,
            to,
        };
        if mq_syntax::parse_recovery(&mutant.apply(source)).1.has_errors() {
            continue;
        }
        mutants.push(mutant);
    }
    mutants
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_replaces_operators_and_literals() {
        let mutants = generate("def f(x): if (x == 1): true else: false;", |_| true);
        let described: Vec<String> = mutants.iter().map(Mutant::describe).collect();
        assert_eq!(
            described,
            vec!["`==` -> `!=`", "`1` -> `2`", "`true` -> `false`", "`false` -> `true`"]
        );
        assert_eq!(
            mutants[0].apply("def f(x): if (x == 1): true else: false;"),
            "def f(x): if (x != 1): true else: false;"
        );
    }

    #[test]
    fn test_generate_ignores_strings_and_comments() {
        assert!(generate("# a == b\n\"x == y\"", |_| true).is_empty());
    }

    #[test]
    fn test_generate_skips_unselected_lines_and_invalid_mutants() {
        assert!(generate("1 + 2", |_| false).is_empty());
        let mutants = generate("def f(a, b): a + b;", |_| true);
        assert_eq!(mutants.len(), 1);
    }

    #[test]
    fn test_generate_skips_annotated_code() {
        let source = "# @mutate(skip)\ndef f(x): x == 1;\ndef g(x): x == 2; # @mutate(skip)\ndef h(x): x == 3;";
        let mutants = generate(source, |_| true);
        let lines: Vec<usize> = mutants.iter().map(|m| m.line).collect();
        assert_eq!(lines, vec![4, 4]);

        let multiline = "# @mutate(skip)\ndef f(x) do\n  x == 1\nend\ndef g(x): x == 2;";
        assert!(generate(multiline, |_| true).iter().all(|m| m.line == 5));
    }

    #[test]
    fn test_generate_uses_character_columns() {
        let mutants = generate("\"あ\" | 1 == 1", |_| true);
        let source = "\"あ\" | 1 == 1";
        assert!(mutants.iter().any(|m| m.apply(source) == "\"あ\" | 1 != 1"));
    }
}
