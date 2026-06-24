//! Property-based regression coverage for `Node::Fragment` rendering.
//!
//! `select()` on a non-matching container node is implemented via
//! `to_fragment()`, which turns the container into a `Fragment` wrapping its
//! (now `Empty`) children instead of collapsing straight to `Node::Empty`.
//! The renderer must treat any such recursively-empty `Fragment` the same as
//! a literal `Node::Empty`, no matter how deeply it is nested, or stray
//! blank lines leak into the output (the 0.6.2 regression this guards).

use mq_markdown::{Fragment, Node, RenderOptions, Text};
use proptest::prelude::*;

fn leaf() -> impl Strategy<Value = Node> {
    prop_oneof![
        Just(Node::Empty),
        "[a-z]{1,5}".prop_map(|s| Node::Text(Text {
            value: s,
            position: None
        })),
    ]
}

fn fragment_tree() -> impl Strategy<Value = Node> {
    leaf().prop_recursive(4, 64, 6, |inner| {
        prop::collection::vec(inner, 0..6).prop_map(|values| Node::Fragment(Fragment { values }))
    })
}

fn all_empty_fragment_tree() -> impl Strategy<Value = Node> {
    Just(Node::Empty).prop_recursive(4, 64, 6, |inner| {
        prop::collection::vec(inner, 0..6).prop_map(|values| Node::Fragment(Fragment { values }))
    })
}

/// Independently re-derives the expected output: walk the tree collecting
/// the value of every `Text` leaf in document order. `Empty` contributes
/// nothing and `Fragment` is transparent. Does not call into
/// `render_with_theme`/`is_empty_fragment`, so it can't share their bugs.
fn expected_leaf_values(node: &Node, out: &mut Vec<String>) {
    match node {
        Node::Empty => {}
        Node::Fragment(Fragment { values }) => {
            for v in values {
                expected_leaf_values(v, out);
            }
        }
        Node::Text(Text { value, .. }) => out.push(value.clone()),
        other => out.push(other.to_string_with(&RenderOptions::default())),
    }
}

proptest! {
    /// A fragment tree built only from `Empty` and `Fragment` must render to
    /// an empty string at any nesting depth/shape. This is exactly the
    /// shape produced for a fully non-matching container.
    #[test]
    fn all_empty_fragment_tree_renders_empty(tree in all_empty_fragment_tree()) {
        prop_assert_eq!(tree.to_string_with(&RenderOptions::default()), String::new());
    }

    /// Rendering a fragment tree must equal joining the in-order rendered
    /// values of its `Text` leaves with "\n", regardless of how deeply
    /// `Empty`/`Fragment` nesting separates them. `Empty` contributes
    /// nothing; `Fragment` is transparent; `Text` (including `Text("")`,
    /// used by `br()`) always survives and contributes its own line.
    #[test]
    fn fragment_render_matches_leaf_order(tree in fragment_tree()) {
        let mut expected = Vec::new();
        expected_leaf_values(&tree, &mut expected);
        prop_assert_eq!(tree.to_string_with(&RenderOptions::default()), expected.join("\n"));
    }
}
