use mq_lang::{CstNode, Shared};
use tower_lsp_server::ls_types::{Position, Range, SelectionRange};

fn to_range(range: mq_lang::Range) -> Range {
    Range::new(
        Position::new(range.start.line - 1, (range.start.column - 1) as u32),
        Position::new(range.end.line - 1, (range.end.column - 1) as u32),
    )
}

fn contains(range: &Range, position: Position) -> bool {
    range.start <= position && position <= range.end
}

/// Collects the ranges of the nodes containing `position`, outermost first.
fn enclosing(nodes: &[Shared<CstNode>], position: Position, out: &mut Vec<Range>) {
    let Some(node) = nodes
        .iter()
        .find(|node| contains(&to_range(node.node_range()), position))
    else {
        return;
    };

    let range = to_range(node.node_range());
    if out.last() != Some(&range) {
        out.push(range);
    }
    let children = node.children().cloned().collect::<Vec<_>>();
    enclosing(&children, position, out);
}

/// Returns, for each position, the nested ranges of the syntax nodes around it.
pub(crate) fn response(nodes: &[Shared<CstNode>], positions: &[Position]) -> Vec<SelectionRange> {
    positions
        .iter()
        .map(|&position| {
            let mut ranges = Vec::new();
            enclosing(nodes, position, &mut ranges);
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
    use super::*;

    fn chain(code: &str, position: Position) -> Vec<Range> {
        let (nodes, _) = mq_lang::parse_recovery(code);
        let mut selection = Some(response(&nodes, &[position]).remove(0));
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
    fn test_position_outside_any_node_is_an_empty_range() {
        let ranges = chain("1", Position::new(5, 0));
        assert_eq!(ranges, vec![Range::new(Position::new(5, 0), Position::new(5, 0))]);
    }
}
