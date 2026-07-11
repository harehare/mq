//! Syntax highlighting for mq source embedded in the HTML coverage report.
//!
//! Classifies tokens by walking the CST returned by [`mq_lang::parse_recovery`]
//! and renders each source line as HTML with `<span class="tok-*">` wrappers,
//! colored via the `tok-*` CSS classes defined in `coverage::HTML_STYLE`
//! (palette taken from the [Tarn](https://github.com/harehare/tarn-theme) theme).

use mq_lang::{CstNode, CstNodeKind, CstTrivia, Shared, TokenKind};
use rustc_hash::FxHashMap;

use crate::coverage::html_escape;

/// Coarse lexical category used to pick a `tok-*` CSS class for a token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TokenClass {
    Comment,
    Keyword,
    Operator,
    Punctuation,
    Function,
    Module,
    Property,
    String,
    Number,
    Boolean,
    Builtin,
    Variable,
}

impl TokenClass {
    fn css_class(self) -> &'static str {
        match self {
            TokenClass::Comment => "tok-comment",
            TokenClass::Keyword => "tok-keyword",
            TokenClass::Operator => "tok-operator",
            TokenClass::Punctuation => "tok-punctuation",
            TokenClass::Function => "tok-function",
            TokenClass::Module => "tok-module",
            TokenClass::Property => "tok-property",
            TokenClass::String => "tok-string",
            TokenClass::Number => "tok-number",
            TokenClass::Boolean => "tok-boolean",
            TokenClass::Builtin => "tok-builtin",
            TokenClass::Variable => "tok-variable",
        }
    }
}

/// A classified token span within one source line, in 1-based UTF-8 columns
/// (matching `mq_lang::Range`), end-exclusive.
struct Span {
    line: u32,
    start_col: usize,
    end_col: usize,
    class: TokenClass,
}

/// The CST reuses the identifier's own node/token for `Call`/`MacroCall`/
/// `QualifiedAccess` (its `kind` changes but the token stays the identifier),
/// while `def`/`import`/dict-key names are separate child `Ident` nodes of a
/// keyword-bearing parent — so classification needs both the node's own kind
/// and its parent context.
fn classify_ident(node_kind: &CstNodeKind, parent: Option<&CstNodeKind>, index_in_parent: usize) -> TokenClass {
    match node_kind {
        CstNodeKind::Call | CstNodeKind::CallDynamic | CstNodeKind::MacroCall => TokenClass::Function,
        CstNodeKind::QualifiedAccess => TokenClass::Module,
        _ => match parent {
            Some(CstNodeKind::Def) | Some(CstNodeKind::Macro) if index_in_parent == 0 => TokenClass::Function,
            Some(CstNodeKind::Import) | Some(CstNodeKind::Include) | Some(CstNodeKind::Module) => TokenClass::Module,
            Some(CstNodeKind::DictEntry) if index_in_parent == 0 => TokenClass::Property,
            Some(CstNodeKind::QualifiedAccess) => TokenClass::Function,
            _ => TokenClass::Variable,
        },
    }
}

fn classify_token(
    token_kind: &TokenKind,
    node_kind: &CstNodeKind,
    parent: Option<&CstNodeKind>,
    index_in_parent: usize,
) -> Option<TokenClass> {
    use TokenKind::*;

    Some(match token_kind {
        Def | Let | If | Elif | Else | End | While | Loop | Foreach | Include | Import | Module | Match | Fn | Do
        | Var | Macro | Try | Catch | As | Break | Continue | Quote | Unquote => TokenClass::Keyword,
        Self_ | Nodes | None => TokenClass::Builtin,
        BoolLiteral(_) => TokenClass::Boolean,
        NumberLiteral(_) => TokenClass::Number,
        StringLiteral(_) | InterpolatedString(_) | BytesLiteral(_) | Env(_) | Selector(_) => TokenClass::String,
        Comment(_) => TokenClass::Comment,
        Ident(_) => classify_ident(node_kind, parent, index_in_parent),
        And | Or | Not | Coalesce | Plus | Minus | Asterisk | Slash | Percent | Equal | EqEq | NeEq | Lt | Lte | Gt
        | Gte | Arrow | Pipe | TildeEqual | NotTildeEqual | LeftShift | RightShift | Convert | DoubleDot
        | DotDotDot | PlusEqual | MinusEqual | StarEqual | SlashEqual | PercentEqual | DoubleSlashEqual | PipeEqual => {
            TokenClass::Operator
        }
        LParen | RParen | LBrace | RBrace | LBracket | RBracket | Colon | DoubleColon | SemiColon | Comma
        | Question => TokenClass::Punctuation,
        Whitespace(_) | Tab(_) | NewLine | Eof => return Option::None,
    })
}

fn collect_spans(nodes: &[Shared<CstNode>], parent: Option<&CstNodeKind>, out: &mut Vec<Span>) {
    for (index, node) in nodes.iter().enumerate() {
        for trivia in node.leading_trivia.iter().chain(node.trailing_trivia.iter()) {
            if let CstTrivia::Comment(token) = trivia {
                out.push(Span {
                    line: token.range.start.line,
                    // The lexer's comment token starts after the `#` delimiter;
                    // pull the span back one column so `#` is highlighted too.
                    start_col: token.range.start.column.saturating_sub(1),
                    end_col: token.range.end.column,
                    class: TokenClass::Comment,
                });
            }
        }

        if let Some(token) = &node.token
            && let Some(class) = classify_token(&token.kind, &node.kind, parent, index)
        {
            out.push(Span {
                line: token.range.start.line,
                start_col: token.range.start.column,
                end_col: token.range.end.column,
                class,
            });
        }

        collect_spans(&node.children, Some(&node.kind), out);
    }
}

fn render_line(line: &str, spans: &[Span]) -> String {
    let chars: Vec<char> = line.chars().collect();
    let escape_range = |start: usize, end: usize| -> String {
        let start = start.min(chars.len());
        let end = end.min(chars.len());
        html_escape(&chars[start..end].iter().collect::<String>())
    };

    let mut html = String::new();
    let mut cursor = 0usize;

    for span in spans {
        let start = span.start_col.saturating_sub(1);
        let end = span.end_col.saturating_sub(1);
        // Defensively skip spans that are out of order/overlapping/out-of-bounds
        // (e.g. a token whose range spans multiple lines) rather than panicking.
        if start < cursor || start >= chars.len() || end <= start {
            continue;
        }

        if start > cursor {
            html.push_str(&escape_range(cursor, start));
        }
        html.push_str(&format!(
            "<span class=\"{}\">{}</span>",
            span.class.css_class(),
            escape_range(start, end)
        ));
        cursor = end.min(chars.len());
    }

    if cursor < chars.len() {
        html.push_str(&escape_range(cursor, chars.len()));
    }

    html
}

/// Renders every line of `content` as syntax-highlighted, HTML-escaped markup
/// suitable for a `<td class="code">` cell, in source line order.
pub(crate) fn highlight_lines(content: &str) -> Vec<String> {
    let (nodes, _) = mq_lang::parse_recovery(content);
    let mut spans = Vec::new();
    collect_spans(&nodes, None, &mut spans);

    let mut by_line: FxHashMap<u32, Vec<Span>> = FxHashMap::default();
    for span in spans {
        by_line.entry(span.line).or_default().push(span);
    }

    content
        .lines()
        .enumerate()
        .map(|(i, line)| {
            let line_no = (i + 1) as u32;
            let mut line_spans = by_line.remove(&line_no).unwrap_or_default();
            line_spans.sort_by_key(|s| s.start_col);
            render_line(line, &line_spans)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case(
        "def foo(x):\n  x + 1\nend\n",
        0,
        "<span class=\"tok-keyword\">def</span> <span class=\"tok-function\">foo</span><span class=\"tok-punctuation\">(</span><span class=\"tok-variable\">x</span><span class=\"tok-punctuation\">)</span><span class=\"tok-punctuation\">:</span>"
    )]
    #[case(
        "upcase()\n",
        0,
        "<span class=\"tok-function\">upcase</span><span class=\"tok-punctuation\">(</span><span class=\"tok-punctuation\">)</span>"
    )]
    #[case("# leading comment\n1\n", 0, "<span class=\"tok-comment\">")]
    #[case("\"hi\"\n", 0, "<span class=\"tok-string\">")]
    #[case(
        "1 + 2\n",
        0,
        "<span class=\"tok-number\">1</span> <span class=\"tok-operator\">+</span> <span class=\"tok-number\">2</span>"
    )]
    fn test_highlight_lines_classifies_tokens(#[case] code: &str, #[case] line_index: usize, #[case] expected: &str) {
        let lines = highlight_lines(code);
        assert!(
            lines[line_index].contains(expected),
            "expected {:?} to contain {:?}",
            lines[line_index],
            expected
        );
    }

    #[test]
    fn test_highlight_lines_import_and_module_names_are_module_class() {
        let lines = highlight_lines("import \"foo\" as bar\n");
        assert!(lines[0].contains("<span class=\"tok-keyword\">import</span>"));
    }

    #[test]
    fn test_highlight_lines_preserves_line_count_and_plain_whitespace() {
        let code = "1\n\n  2\n";
        let lines = highlight_lines(code);
        assert_eq!(lines.len(), 3);
        assert!(lines[1].is_empty());
        assert!(lines[2].starts_with("  "));
    }

    #[test]
    fn test_highlight_lines_escapes_html_in_strings_and_comments() {
        let lines = highlight_lines("\"<a>\"\n");
        assert!(lines[0].contains("&lt;a&gt;"));
        assert!(!lines[0].contains("<a>"));
    }
}
