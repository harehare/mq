use mq_lang::{CstNode, Shared};
use tower_lsp_server::ls_types::{Position, Range, SelectionRange};

use crate::position::LineIndex;

fn contains(range: &Range, position: Position) -> bool {
    range.start <= position && position <= range.end
}

/// Collects the ranges of the nodes containing `position`, outermost first.
fn enclosing(nodes: &[Shared<CstNode>], lines: &LineIndex, position: Position, out: &mut Vec<Range>) {
    let Some(node) = nodes
        .iter()
        .find(|node| contains(&lines.to_lsp_range(node.node_range()), position))
    else {
        return;
    };

    let range = lines.to_lsp_range(node.node_range());
    if out.last() != Some(&range) {
        out.push(range);
    }
    let children = node.children().cloned().collect::<Vec<_>>();
    enclosing(&children, lines, position, out);
}

/// Returns, for each position, the nested ranges of the syntax nodes around it.
pub(crate) fn response(nodes: &[Shared<CstNode>], lines: &LineIndex, positions: &[Position]) -> Vec<SelectionRange> {
    positions
        .iter()
        .map(|&position| {
            let mut ranges = Vec::new();
            enclosing(nodes, lines, position, &mut ranges);
            // Keep strictly growing ranges, innermost first.
            ranges.dedup();
            ranges
                .into_iter()
                .fold(None, |parent, range| {
                    Some(SelectionRange {
                        range,
                        parent: parent.map(Box::new),
                    })
                })
                .unwrap_or(SelectionRange {
                    range: Range::new(position, position),
                    parent: None,
                })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    fn chain(code: &str, position: Position) -> Vec<Range> {
        let (nodes, _) = mq_lang::parse_recovery(code);
        let lines = LineIndex::new(Arc::new(code.to_string()));
        let mut selection = Some(response(&nodes, &lines, &[position]).remove(0));
        let mut out = Vec::new();
        while let Some(current) = selection {
            out.push(current.range);
            selection = current.parent.map(|parent| *parent);
        }
        out
    }

    #[test]
    fn test_ranges_grow_outward() {
        let ranges = chain("def f(x): add(x, 1);", Position::new(0, 14));
        assert!(ranges.len() >= 2, "{ranges:?}");
        for pair in ranges.windows(2) {
            assert!(
                pair[1].start <= pair[0].start && pair[0].end <= pair[1].end && pair[0] != pair[1],
                "{pair:?}"
            );
        }
        assert!(ranges[0].start.character <= 14 && 14 <= ranges[0].end.character);
    }

    #[test]
    fn test_ranges_use_utf16_columns_after_non_bmp_characters() {
        // `😀` is two UTF-16 units, so the call `add(1, 2)` spans columns 7 to 16.
        let ranges = chain("\"😀\" | add(1, 2)", Position::new(0, 8));
        let call = Range::new(Position::new(0, 7), Position::new(0, 16));
        assert!(ranges.contains(&call), "{ranges:?}");
    }

    #[test]
    fn test_ranges_after_leading_bom() {
        // The call `add(1, 2)` spans columns 1 to 10 once the BOM unit is counted.
        let ranges = chain("\u{FEFF}add(1, 2)", Position::new(0, 2));
        let call = Range::new(Position::new(0, 1), Position::new(0, 10));
        assert!(ranges.contains(&call), "{ranges:?}");
    }

    #[test]
    fn test_position_outside_any_node_is_an_empty_range() {
        let ranges = chain("1", Position::new(5, 0));
        assert_eq!(ranges, vec![Range::new(Position::new(5, 0), Position::new(5, 0))]);
    }
}
