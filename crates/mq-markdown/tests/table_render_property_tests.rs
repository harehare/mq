//! Property-based regression coverage for GFM table column-width padding.
//!
//! `Markdown::to_string()` pads every cell in a table to its column's
//! computed width (see `TableLayout` in `src/markdown.rs`), so that tables
//! render the way markdownlint/Prettier/GitHub do. That padding logic must
//! hold for arbitrary column counts, row counts, and alignments — not just
//! the hand-picked shapes covered by the `#[rstest]` cases in
//! `src/markdown.rs`. This file checks the invariants a correct padding
//! implementation can never violate, regardless of table shape.

use mq_markdown::{Markdown, TableAlignKind};
use proptest::prelude::*;

fn align_kind() -> impl Strategy<Value = TableAlignKind> {
    prop_oneof![
        Just(TableAlignKind::None),
        Just(TableAlignKind::Left),
        Just(TableAlignKind::Right),
        Just(TableAlignKind::Center),
    ]
}

// Plain alphanumerics only: keeps unicode-width == char count trivial and
// avoids characters (`|`, `*`, backslash, newlines) that would change
// markdown semantics rather than exercise table padding.
fn cell_text() -> impl Strategy<Value = String> {
    "[a-zA-Z0-9]{0,8}"
}

#[derive(Debug)]
struct TableSpec {
    aligns: Vec<TableAlignKind>,
    header: Vec<String>,
    rows: Vec<Vec<String>>,
}

fn table_spec() -> impl Strategy<Value = TableSpec> {
    (1usize..6).prop_flat_map(|cols| {
        (
            prop::collection::vec(align_kind(), cols),
            prop::collection::vec(cell_text(), cols),
            prop::collection::vec(prop::collection::vec(cell_text(), cols), 0..6),
        )
            .prop_map(|(aligns, header, rows)| TableSpec { aligns, header, rows })
    })
}

fn render_table_source(spec: &TableSpec) -> String {
    let mut lines = vec![
        format!("| {} |", spec.header.join(" | ")),
        format!(
            "| {} |",
            spec.aligns
                .iter()
                .map(|a| a.to_string())
                .collect::<Vec<_>>()
                .join(" | ")
        ),
    ];
    for row in &spec.rows {
        lines.push(format!("| {} |", row.join(" | ")));
    }
    lines.join("\n") + "\n"
}

proptest! {
    /// Every line of a rendered table — header, separator, and every data
    /// row — must have identical display width. A regression that drops
    /// the column-min-width floor or miscomputes a single cell's padding
    /// would desync the columns.
    #[test]
    fn table_rows_have_uniform_width(spec in table_spec()) {
        let source = render_table_source(&spec);
        let md: Markdown = source.parse().unwrap();
        let rendered = md.to_string();
        let widths: Vec<usize> = rendered.lines().map(|l| l.chars().count()).collect();

        prop_assert!(
            widths.windows(2).all(|w| w[0] == w[1]),
            "rendered table lines have unequal width {widths:?}:\n{rendered}"
        );
    }

    /// Formatting must be a fixed point: rendering already-padded markdown
    /// must reproduce it exactly, not grow/shrink the padding on each pass.
    #[test]
    fn table_rendering_is_idempotent(spec in table_spec()) {
        let source = render_table_source(&spec);
        let md: Markdown = source.parse().unwrap();
        let first = md.to_string();
        let second: Markdown = first.parse().unwrap();

        prop_assert_eq!(second.to_string(), first);
    }

    /// Padding must never add or drop rows: header + separator + every
    /// data row must each become exactly one line of output.
    #[test]
    fn table_row_count_is_preserved(spec in table_spec()) {
        let source = render_table_source(&spec);
        let md: Markdown = source.parse().unwrap();
        let rendered = md.to_string();

        prop_assert_eq!(rendered.lines().count(), 2 + spec.rows.len());
    }
}
