use mq_lang::{CstNode, CstNodeKind, CstTrivia, Shared};
use tower_lsp_server::ls_types::{FoldingRange, FoldingRangeKind};

/// Computes folding ranges by parsing the document into a CST and walking it directly,
/// the same approach `signature_help.rs` uses — `mq_hir::Symbol` ranges are too coarse
/// (block delimiters like `end`/`;` aren't lowered into HIR at all), but the CST's
/// `node_range()` gives an exact span for every block construct.
pub(crate) fn response(source_text: Option<&str>) -> Option<Vec<FoldingRange>> {
    let source_text = source_text?;
    let (nodes, _) = mq_lang::parse_recovery(source_text);

    let mut ranges = Vec::new();
    for node in &nodes {
        visit(node, &mut ranges);
    }

    if ranges.is_empty() { None } else { Some(ranges) }
}

fn visit(node: &Shared<CstNode>, ranges: &mut Vec<FoldingRange>) {
    collect_comment_folds(node, ranges);

    if is_foldable(&node.kind) {
        let span = node.node_range();
        if span.end.line > span.start.line {
            ranges.push(FoldingRange {
                start_line: span.start.line - 1,
                start_character: None,
                end_line: span.end.line - 1,
                end_character: None,
                kind: Some(FoldingRangeKind::Region),
                collapsed_text: None,
            });
        }
    }

    for child in &node.children {
        visit(child, ranges);
    }
}

/// Block-like constructs worth collapsing: function/macro/module bodies, control-flow
/// blocks, and multi-line array/dict literals. Deliberately excludes leaf/expression kinds
/// (`Call`, `BinaryOp`, ...) so folding stays limited to structural blocks.
fn is_foldable(kind: &CstNodeKind) -> bool {
    matches!(
        kind,
        CstNodeKind::Def
            | CstNodeKind::Macro
            | CstNodeKind::Module
            | CstNodeKind::If
            | CstNodeKind::Elif
            | CstNodeKind::Else
            | CstNodeKind::Match
            | CstNodeKind::MatchArm
            | CstNodeKind::Foreach
            | CstNodeKind::While
            | CstNodeKind::Loop
            | CstNodeKind::Try
            | CstNodeKind::Catch
            | CstNodeKind::Array
            | CstNodeKind::Dict
    )
}

/// Folds runs of 2+ consecutive `#`-comment lines immediately preceding a node, so a
/// multi-line doc comment or a `# Section:`-style banner can be collapsed like in other
/// languages' "region"/"comment" folding.
fn collect_comment_folds(node: &Shared<CstNode>, ranges: &mut Vec<FoldingRange>) {
    let comment_lines = node
        .leading_trivia
        .iter()
        .filter_map(|trivia| match trivia {
            CstTrivia::Comment(token) => Some(token.range.start.line),
            _ => None,
        })
        .collect::<Vec<_>>();

    let mut run_start = None;
    let mut run_end = None;

    for line in comment_lines {
        match run_end {
            Some(end) if line == end + 1 => run_end = Some(line),
            _ => {
                push_comment_fold(ranges, run_start, run_end);
                run_start = Some(line);
                run_end = Some(line);
            }
        }
    }
    push_comment_fold(ranges, run_start, run_end);
}

fn push_comment_fold(ranges: &mut Vec<FoldingRange>, start: Option<u32>, end: Option<u32>) {
    if let (Some(start), Some(end)) = (start, end)
        && end > start
    {
        ranges.push(FoldingRange {
            start_line: start - 1,
            start_character: None,
            end_line: end - 1,
            end_character: None,
            kind: Some(FoldingRangeKind::Comment),
            collapsed_text: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_source_text() {
        assert!(response(None).is_none());
    }

    #[test]
    fn test_no_foldable_ranges_for_single_line() {
        let result = response(Some("let x = 1"));
        assert!(result.is_none());
    }

    #[test]
    fn test_folds_multiline_function_body() {
        let code = "def foo(a):\n  let b = a + 1\n  | b;\n| foo(1)";
        let result = response(Some(code)).unwrap();

        let def_fold = result
            .iter()
            .find(|r| r.kind == Some(FoldingRangeKind::Region) && r.start_line == 0);
        assert!(
            def_fold.is_some(),
            "expected a folding range starting at the `def` line"
        );
        assert_eq!(def_fold.unwrap().end_line, 2);
    }

    #[test]
    fn test_folds_multiline_array() {
        let code = "let xs = [\n  1,\n  2,\n  3\n] | xs";
        let result = response(Some(code)).unwrap();

        let array_fold = result.iter().find(|r| r.kind == Some(FoldingRangeKind::Region));
        assert!(array_fold.is_some());
        assert_eq!(array_fold.unwrap().start_line, 0);
        assert_eq!(array_fold.unwrap().end_line, 4);
    }

    #[test]
    fn test_folds_consecutive_comment_lines() {
        let code = "# First line\n# Second line\n# Third line\ndef foo(): 1;";
        let result = response(Some(code)).unwrap();

        let comment_fold = result.iter().find(|r| r.kind == Some(FoldingRangeKind::Comment));
        assert!(comment_fold.is_some());
        let comment_fold = comment_fold.unwrap();
        assert_eq!(comment_fold.start_line, 0);
        assert_eq!(comment_fold.end_line, 2);
    }

    #[test]
    fn test_single_comment_line_not_folded() {
        let code = "# Just one line\ndef foo(): 1;";
        let result = response(Some(code));

        let has_comment_fold = result
            .map(|ranges| ranges.iter().any(|r| r.kind == Some(FoldingRangeKind::Comment)))
            .unwrap_or(false);
        assert!(!has_comment_fold);
    }
}
