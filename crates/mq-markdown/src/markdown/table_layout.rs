use crate::node::{ColorTheme, Node, RenderOptions, TableAlign, TableAlignKind, TableCell, render_values};
use unicode_width::UnicodeWidthStr;

/// Per-column widths and alignment of a single contiguous table, computed by
/// scanning ahead over its `TableCell`/`TableAlign` nodes before the first
/// row is emitted, so every row (including the header) pads to the same
/// column widths.
pub(super) struct TableLayout {
    widths: Vec<usize>,
    align: Vec<TableAlignKind>,
    /// Plain (color-free) display width of every cell, keyed by `(row, column)`,
    /// computed once during the scan so the main render pass never has to
    /// re-render a cell just to measure it.
    cell_widths: rustc_hash::FxHashMap<(usize, usize), usize>,
    /// Index (exclusive) of the node following this table's last node.
    pub(super) end: usize,
}

impl TableLayout {
    pub(super) fn compute(nodes: &[Node], options: &RenderOptions, start: usize) -> Self {
        let mut widths: Vec<usize> = Vec::new();
        let mut align: Vec<TableAlignKind> = Vec::new();
        let mut cell_widths = rustc_hash::FxHashMap::default();
        let mut i = start;

        while let Some(node) = nodes.get(i) {
            match node {
                Node::TableCell(TableCell {
                    row, column, values, ..
                }) => {
                    let width = cell_display_width(values, options);
                    cell_widths.insert((*row, *column), width);
                    if *column >= widths.len() {
                        widths.resize(*column + 1, 0);
                    }
                    widths[*column] = widths[*column].max(width);
                    i += 1;
                }
                Node::TableAlign(TableAlign { align: a, .. }) => {
                    align = a.clone();
                    i += 1;
                }
                _ => break,
            }
        }

        if widths.len() < align.len() {
            widths.resize(align.len(), 0);
        }

        for (idx, width) in widths.iter_mut().enumerate() {
            let a = align.get(idx).unwrap_or(&TableAlignKind::None);
            *width = (*width).max(column_min_width(a));
        }

        Self {
            widths,
            align,
            cell_widths,
            end: i,
        }
    }

    pub(super) fn align_for(&self, column: usize) -> TableAlignKind {
        self.align.get(column).cloned().unwrap_or(TableAlignKind::None)
    }

    pub(super) fn width_for(&self, column: usize) -> usize {
        self.widths
            .get(column)
            .copied()
            .unwrap_or(column_min_width(&self.align_for(column)))
    }

    /// Plain display width of the cell at `(row, column)`, precomputed during `compute`.
    pub(super) fn plain_width_at(&self, row: usize, column: usize) -> usize {
        self.cell_widths.get(&(row, column)).copied().unwrap_or(0)
    }

    pub(super) fn render_separator(&self) -> String {
        let segments = self
            .widths
            .iter()
            .enumerate()
            .map(|(idx, &width)| {
                let align = self.align_for(idx);
                match align {
                    TableAlignKind::None => "-".repeat(width),
                    TableAlignKind::Left => format!(":{}", "-".repeat(width - 1)),
                    TableAlignKind::Right => format!("{}:", "-".repeat(width - 1)),
                    TableAlignKind::Center => format!(":{}:", "-".repeat(width - 2)),
                }
            })
            .collect::<Vec<_>>()
            .join(" | ");

        format!("| {} |", segments)
    }
}

/// Minimum width a column must have to render a valid alignment marker
/// (e.g. `:-:` needs at least 3 characters for the two colons and a dash).
fn column_min_width(align: &TableAlignKind) -> usize {
    match align {
        TableAlignKind::None => 1,
        TableAlignKind::Left | TableAlignKind::Right => 2,
        TableAlignKind::Center => 3,
    }
}

/// Display width (in terminal columns) of a table cell's plain-rendered
/// content, ignoring any ANSI color codes so padding stays correct with
/// `--color` output.
fn cell_display_width(values: &[Node], options: &RenderOptions) -> usize {
    UnicodeWidthStr::width(render_values(values, options, &ColorTheme::PLAIN).as_str())
}

/// Pads an already-rendered (possibly colored) cell value to `width`
/// columns, using `plain_width` — the color-free display width — to compute
/// how much padding is needed.
pub(super) fn pad_cell(content: &str, plain_width: usize, width: usize, align: &TableAlignKind) -> String {
    let pad = width.saturating_sub(plain_width);

    match align {
        TableAlignKind::Right => format!("{}{}", " ".repeat(pad), content),
        TableAlignKind::Center => {
            let left = pad / 2;
            let right = pad - left;
            format!("{}{}{}", " ".repeat(left), content, " ".repeat(right))
        }
        TableAlignKind::Left | TableAlignKind::None => format!("{}{}", content, " ".repeat(pad)),
    }
}
