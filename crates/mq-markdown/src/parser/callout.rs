//! Obsidian-style callouts: a block quote whose first line is `[!TYPE]`, an optional fold marker and an
//! optional title.

use super::tree::{Block, InlineBlock, InlineKind};

/// The header line of a callout.
pub(super) struct Header {
    pub(super) kind: String,
    pub(super) fold: Option<char>,
    pub(super) title: Option<String>,
}

/// Splits `[!TYPE]` and the fold marker off the start of `text`, returning the type, the marker and
/// what follows. `TYPE` is letters, digits, `-` and `_`, and a `+` or `-` right after it is the marker.
fn split(text: &str) -> Option<(&str, Option<char>, &str)> {
    let rest = text.strip_prefix("[!")?;
    let end = rest.find(']')?;
    let kind = &rest[..end];
    if kind.is_empty() || !kind.chars().all(|c| c.is_alphanumeric() || matches!(c, '-' | '_')) {
        return None;
    }
    let after = &rest[end + 1..];
    let fold = after.chars().next().filter(|c| matches!(c, '+' | '-'));
    Some((kind, fold, &after[fold.map_or(0, char::len_utf8)..]))
}

/// Takes the header line off the first paragraph of a quote, if it is one. The title is the rest of that
/// line as written, so markup in it never mixes with the body, which starts on the next line.
pub(super) fn take_header(children: &mut Vec<Block>) -> Option<Header> {
    let Some(Block::Inline(InlineBlock {
        source,
        kind: InlineKind::Paragraph,
    })) = children.first_mut()
    else {
        return None;
    };
    split(&source.text)?;

    let line = source.take_first_line();
    let (kind, fold, title) = split(&line)?;
    let title = title.trim();
    let header = Header {
        kind: kind.to_string(),
        fold,
        title: (!title.is_empty()).then(|| title.to_string()),
    };
    if source.text.is_empty() {
        children.remove(0);
    }
    Some(header)
}
