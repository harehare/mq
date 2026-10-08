//! Puts the children of a list item back in order with the items nested in it.
//!
//! A flat list holds the items nested in an item right after it, even when some of the children of
//! that item come after them in the source, like a paragraph that follows a nested list. Rendering
//! the nodes as they are would write those children before the nested items.

use crate::node::{List, ListMarker, Node, Position};

/// Returns the nodes in source order, or `None` when they already are.
pub(super) fn reorder(nodes: &[Node]) -> Option<Vec<Node>> {
    // Some child of an item starts after the item nested right after it.
    let needed = nodes.windows(2).any(|pair| match (&pair[0], &pair[1]) {
        (Node::List(item), Node::List(next)) if next.level > item.level => {
            let nested = start_line(&pair[1]);
            item.values
                .iter()
                .filter_map(start_line)
                .any(|line| Some(line) > nested)
        }
        _ => false,
    });
    if !needed {
        return None;
    }
    let mut out = Vec::with_capacity(nodes.len() + 4);
    reorder_into(nodes, &mut out);
    Some(out)
}

fn start_line(node: &Node) -> Option<usize> {
    node.position().map(|position| position.start.line)
}

fn reorder_into(nodes: &[Node], out: &mut Vec<Node>) {
    let mut index = 0;
    while index < nodes.len() {
        let node = &nodes[index];
        let Node::List(item) = node else {
            out.push(node.clone());
            index += 1;
            continue;
        };
        let nested = nodes[index + 1..]
            .iter()
            .take_while(|next| matches!(next, Node::List(next) if next.level > item.level))
            .count();
        let lines: Option<Vec<usize>> = item.values.iter().map(start_line).collect();
        let (true, Some(lines)) = (!item.values.is_empty() && nested > 0, lines) else {
            out.push(node.clone());
            index += 1;
            continue;
        };

        let run = &nodes[index + 1..index + 1 + nested];
        let Node::List(first) = &run[0] else { unreachable!() };
        let segment_level = first.level;
        // Each item nested directly in this one, with the items nested in those.
        let mut segments: Vec<&[Node]> = Vec::new();
        let mut from = 0;
        for at in 1..=run.len() {
            if at == run.len() || matches!(&run[at], Node::List(next) if next.level <= segment_level) {
                segments.push(&run[from..at]);
                from = at;
            }
        }
        let segment_lines: Option<Vec<usize>> = segments.iter().map(|segment| start_line(&segment[0])).collect();
        let Some(segment_lines) = segment_lines else {
            out.push(node.clone());
            index += 1;
            continue;
        };

        // The children before the first nested item stay with the item.
        let head = lines.iter().take_while(|line| **line < segment_lines[0]).count();
        out.push(Node::List(List {
            values: item.values[..head].to_vec(),
            position: span(&item.values[..head]).or_else(|| {
                // Without children it ends where it starts, so the nested item follows the marker.
                item.position.clone().map(|position| Position {
                    end: position.start.clone(),
                    start: position.start,
                })
            }),
            ..item.clone()
        }));

        let mut child = head;
        for (segment, line) in segments.iter().zip(&segment_lines) {
            let from = child;
            while child < item.values.len() && lines[child] < *line {
                child += 1;
            }
            if child > from {
                out.push(continuation(item, &item.values[from..child]));
            }
            reorder_into(segment, out);
        }
        if child < item.values.len() {
            out.push(continuation(item, &item.values[child..]));
        }
        index += 1 + nested;
    }
}

fn continuation(item: &List, values: &[Node]) -> Node {
    Node::List(List {
        values: values.to_vec(),
        position: span(values),
        // Rendered as its content, indented to the content of the item, without a marker.
        marker: Some(ListMarker::Continuation),
        ..item.clone()
    })
}

/// The position from the start of the first node to the end of the last.
fn span(values: &[Node]) -> Option<Position> {
    Some(Position {
        start: values.first()?.position()?.start,
        end: values.last()?.position()?.end,
    })
}
