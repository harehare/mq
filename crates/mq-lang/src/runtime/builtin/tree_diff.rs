//! Structural diff of two Markdown documents.
//!
//! The parsed document is a flat block sequence, so the diff works on "units"
//! (one node, or one table row) located by the chain of headings above them.

use crate::text_diff::{self, ChangeTag};
use mq_markdown::Node;

/// Minimum character similarity for a deleted and an inserted unit to count as one edited unit.
const UPDATE_THRESHOLD: f64 = 0.5;

/// What happened to a unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EditKind {
    Insert,
    Delete,
    Update,
}

impl EditKind {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Insert => "insert",
            Self::Delete => "delete",
            Self::Update => "update",
        }
    }
}

/// One structural change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Edit {
    pub kind: EditKind,
    /// Node kind, e.g. `h2`, `list`, `table_row`.
    pub node: String,
    /// Titles of the headings above the unit, joined with `" > "`.
    pub path: String,
    pub old: Option<String>,
    pub new: Option<String>,
}

#[derive(Debug)]
struct Unit {
    node: String,
    path: String,
    text: String,
}

impl Unit {
    fn fingerprint(&self) -> String {
        format!("{}\0{}", self.node, self.text)
    }
}

/// Diffs two documents given as their top-level nodes.
pub(super) fn diff(old: &[Node], new: &[Node]) -> Vec<Edit> {
    let (old_units, new_units) = (units(old), units(new));
    let old_keys: Vec<String> = old_units.iter().map(Unit::fingerprint).collect();
    let new_keys: Vec<String> = new_units.iter().map(Unit::fingerprint).collect();
    let changes = text_diff::changes(
        &old_keys.iter().map(String::as_str).collect::<Vec<_>>(),
        &new_keys.iter().map(String::as_str).collect::<Vec<_>>(),
    );

    let mut edits = Vec::new();
    let (mut deleted, mut inserted): (Vec<usize>, Vec<usize>) = (Vec::new(), Vec::new());
    for change in &changes {
        match (change.tag(), change.old_index(), change.new_index()) {
            (ChangeTag::Delete, Some(i), _) => deleted.push(i),
            (ChangeTag::Insert, _, Some(j)) => inserted.push(j),
            _ => flush(&mut edits, &old_units, &new_units, &mut deleted, &mut inserted),
        }
    }
    flush(&mut edits, &old_units, &new_units, &mut deleted, &mut inserted);
    edits
}

/// Emits the edits for one run of consecutive deletes and inserts, pairing similar units as updates.
fn flush(edits: &mut Vec<Edit>, old: &[Unit], new: &[Unit], deleted: &mut Vec<usize>, inserted: &mut Vec<usize>) {
    let mut cursor = 0;
    for &i in deleted.iter() {
        let pair = (cursor..inserted.len())
            .filter(|&k| new[inserted[k]].node == old[i].node)
            .map(|k| (k, similarity(&old[i].text, &new[inserted[k]].text)))
            .filter(|&(_, score)| score >= UPDATE_THRESHOLD)
            .max_by(|a, b| a.1.total_cmp(&b.1).then(b.0.cmp(&a.0)));
        match pair {
            Some((k, _)) => {
                for &j in &inserted[cursor..k] {
                    edits.push(edit(EditKind::Insert, None, Some(&new[j])));
                }
                edits.push(edit(EditKind::Update, Some(&old[i]), Some(&new[inserted[k]])));
                cursor = k + 1;
            }
            None => edits.push(edit(EditKind::Delete, Some(&old[i]), None)),
        }
    }
    for &j in &inserted[cursor..] {
        edits.push(edit(EditKind::Insert, None, Some(&new[j])));
    }
    deleted.clear();
    inserted.clear();
}

fn edit(kind: EditKind, old: Option<&Unit>, new: Option<&Unit>) -> Edit {
    let anchor = new.or(old).expect("an edit has an old or a new unit");
    Edit {
        kind,
        node: anchor.node.clone(),
        path: anchor.path.clone(),
        old: old.map(|u| u.text.clone()),
        new: new.map(|u| u.text.clone()),
    }
}

/// Share of characters kept between `a` and `b`, from 0.0 to 1.0.
fn similarity(a: &str, b: &str) -> f64 {
    let total = a.chars().count() + b.chars().count();
    if total == 0 {
        return 1.0;
    }
    let kept = text_diff::changes(&text_diff::chars(a), &text_diff::chars(b))
        .iter()
        .filter(|c| c.tag() == ChangeTag::Equal)
        .count();
    (2 * kept) as f64 / total as f64
}

/// Splits nodes into units, grouping table cells by row and tracking the heading chain.
fn units(nodes: &[Node]) -> Vec<Unit> {
    let mut out = Vec::new();
    let mut headings: Vec<(u8, String)> = Vec::new();
    let mut row: Option<(usize, Vec<String>, String)> = None;
    walk(nodes, &mut |node| {
        if let Node::TableCell(cell) = node {
            match &mut row {
                Some((index, cells, _)) if *index == cell.row => cells.push(node.value()),
                _ => {
                    if let Some((_, cells, path)) = row.take() {
                        out.push(table_row(cells, path));
                    }
                    row = Some((cell.row, vec![node.value()], path_of(&headings)));
                }
            }
            return;
        }
        if let Some((_, cells, path)) = row.take() {
            out.push(table_row(cells, path));
        }
        let heading_depth = match node {
            Node::Heading(heading) => Some(heading.depth.get()),
            _ => None,
        };
        if let Some(depth) = heading_depth {
            headings.retain(|(d, _)| *d < depth);
        }
        let path = path_of(&headings);
        if let Some(depth) = heading_depth {
            headings.push((depth, node.value()));
        }
        out.push(Unit {
            node: node.name().to_string(),
            path,
            text: node.to_string().trim_end().to_string(),
        });
    });
    if let Some((_, cells, path)) = row {
        out.push(table_row(cells, path));
    }
    out
}

fn table_row(cells: Vec<String>, path: String) -> Unit {
    Unit {
        node: "table_row".to_string(),
        path,
        text: format!("| {} |", cells.join(" | ")),
    }
}

fn path_of(headings: &[(u8, String)]) -> String {
    headings.iter().map(|(_, t)| t.as_str()).collect::<Vec<_>>().join(" > ")
}

/// Visits nodes in order, flattening fragments.
fn walk(nodes: &[Node], visit: &mut impl FnMut(&Node)) {
    for node in nodes {
        match node {
            Node::Fragment(fragment) => walk(&fragment.values, visit),
            _ => visit(node),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    fn parse(s: &str) -> Vec<Node> {
        s.parse::<mq_markdown::Markdown>().unwrap().nodes
    }

    fn run(old: &str, new: &str) -> Vec<(EditKind, String, String)> {
        diff(&parse(old), &parse(new))
            .into_iter()
            .map(|e| (e.kind, e.node, e.path))
            .collect()
    }

    #[rstest]
    #[case::identical("# A\n\ntext\n", "# A\n\ntext\n", vec![])]
    #[case::heading_renamed(
        "# Install\n\ntext\n",
        "# Installation\n\ntext\n",
        vec![(EditKind::Update, "h1", "")]
    )]
    #[case::paragraph_added(
        "# A\n\none\n",
        "# A\n\none\n\ntwo\n",
        vec![(EditKind::Insert, "text", "A")]
    )]
    #[case::paragraph_removed(
        "# A\n\none\n\ntwo\n",
        "# A\n\none\n",
        vec![(EditKind::Delete, "text", "A")]
    )]
    #[case::unrelated_paragraph_is_not_an_update(
        "# A\n\nalpha beta gamma\n",
        "# A\n\n0123456789\n",
        vec![(EditKind::Delete, "text", "A"), (EditKind::Insert, "text", "A")]
    )]
    #[case::table_row_added(
        "# A\n\n## B\n\n| x | y |\n|---|---|\n| 1 | 2 |\n",
        "# A\n\n## B\n\n| x | y |\n|---|---|\n| 1 | 2 |\n| 3 | 4 |\n",
        vec![(EditKind::Insert, "table_row", "A > B")]
    )]
    #[case::table_cell_changed(
        "| x | y |\n|---|---|\n| 1 | 2 |\n",
        "| x | y |\n|---|---|\n| 1 | 20 |\n",
        vec![(EditKind::Update, "table_row", "")]
    )]
    #[case::list_item_added(
        "- a\n- b\n",
        "- a\n- b\n- c\n",
        vec![(EditKind::Insert, "list", "")]
    )]
    fn test_diff(#[case] old: &str, #[case] new: &str, #[case] expected: Vec<(EditKind, &str, &str)>) {
        let expected: Vec<_> = expected
            .into_iter()
            .map(|(k, n, p)| (k, n.to_string(), p.to_string()))
            .collect();
        assert_eq!(run(old, new), expected);
    }

    #[test]
    fn test_update_carries_old_and_new_text() {
        let edits = diff(&parse("# Install\n"), &parse("# Installation\n"));
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].old.as_deref(), Some("# Install"));
        assert_eq!(edits[0].new.as_deref(), Some("# Installation"));
    }
}
