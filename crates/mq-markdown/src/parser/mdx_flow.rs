//! MDX flow content: JSX tags and expressions that make up whole lines.

use super::error::{Located, MdxError, MdxErrorKind};
use super::line::Line;
use super::mdx::{self, Fallback, Parsed};
use super::tree::{Block, InlineSource, JsxTag};
use crate::node::{MdxFlowExpression, Node, Position};

/// What the items of a line of MDX flow content add up to.
pub(super) enum Flow {
    /// Not flow content. When some items were parsed, the offset where the last one ends.
    Nok(Option<usize>),
    More(Fallback),
    Error(Located),
    /// The items, and the offset where the last one ends.
    Done(Vec<FlowItem>, usize),
}

pub(super) enum FlowItem {
    Tag(mdx::Tag, usize),
    Expression(smol_str::SmolStr, usize, usize),
}

/// Parses JSX tags and expressions that make up whole lines: they may only be separated by spaces.
fn flow_items(source: &InlineSource) -> Flow {
    let text = source.text.as_str();
    let mut index = text.len() - text.trim_start_matches([' ', '\t']).len();
    let mut items = Vec::new();

    loop {
        let start = index;
        let after_expression = match text[index..].chars().next() {
            Some('<') => match mdx::tag(text, index, None) {
                Parsed::Ok(tag) => {
                    index = tag.end;
                    items.push(FlowItem::Tag(tag, start));
                    false
                }
                Parsed::Nok => return Flow::Nok(items.last().map(item_end)),
                Parsed::More(fallback) => return Flow::More(fallback),
                Parsed::Error(error) => return Flow::Error(error),
            },
            Some('{') => match mdx::expression(text, index, None) {
                Parsed::Ok((end, value)) => {
                    index = end;
                    items.push(FlowItem::Expression(value, start, end));
                    true
                }
                Parsed::Nok => return Flow::Nok(items.last().map(item_end)),
                Parsed::More(fallback) => return Flow::More(fallback),
                Parsed::Error(error) => return Flow::Error(error),
            },
            _ => return Flow::Nok(items.last().map(item_end)),
        };

        let end = index;
        index += text[index..].len() - text[index..].trim_start_matches([' ', '\t']).len();
        match text[index..].chars().next() {
            None | Some('\n' | '\r') => return Flow::Done(items, end),
            Some('<') => {}
            Some('{') if !after_expression => {}
            Some(_) => return Flow::Nok(Some(end)),
        }
    }
}

fn item_end(item: &FlowItem) -> usize {
    match item {
        FlowItem::Tag(tag, _) => tag.end,
        FlowItem::Expression(_, _, end) => *end,
    }
}

/// What a line of MDX makes of flow content.
pub(super) enum FlowOutcome {
    /// Flow content that uses the lines up to the given index.
    Flow(usize),
    /// Not flow content. Lines that its items span up to the given index still belong together.
    Nok(Option<usize>),
    /// The document ends inside a construct that could have been flow content. It is not, but a
    /// paragraph is not continued lazily with such a line.
    Pending,
}

/// Parses MDX flow content that starts at `lines[start]` and pushes its blocks.
pub(super) fn mdx_flow(lines: &[Line<'_>], start: usize, blocks: &mut Vec<Block>) -> FlowOutcome {
    let mut count = 1;
    loop {
        let end = (start + count).min(lines.len());
        let mut source = InlineSource::new(
            lines[start..end]
                .iter()
                .map(|line| (line.text, line.eol, line.point(0))),
        );
        // The last line has its line ending, unless the document ends without one.
        source.text.push_str(lines[end - 1].eol);
        // The container ends but the document goes on after it.
        let container_end = end == lines.len() && lines.last().is_some_and(|line| !line.eof);
        match flow_items(&source) {
            // The construct goes on: read more lines, unless the container has none left.
            Flow::More(_) if end < lines.len() => count *= 2,
            Flow::More(_) if container_end => {
                blocks.push(Block::Error(MdxError::new(
                    MdxErrorKind::LazyLine,
                    Some(lines[start].point(0)),
                )));
                return FlowOutcome::Flow(lines.len());
            }
            Flow::More(Fallback::Nok) => return FlowOutcome::Pending,
            Flow::More(Fallback::Error(error)) => {
                blocks.push(Block::Error(error.in_source(&source)));
                return FlowOutcome::Flow(lines.len());
            }
            Flow::Nok(end) => {
                let last_line =
                    end.map(|end| start + source.lines.partition_point(|line| line.offset < end).saturating_sub(1));
                return FlowOutcome::Nok(last_line);
            }
            Flow::Error(error) => {
                blocks.push(Block::Error(error.in_source(&source)));
                return FlowOutcome::Flow(lines.len());
            }
            Flow::Done(items, last) => {
                let last_line = start
                    + source
                        .lines
                        .partition_point(|line| line.offset < last)
                        .saturating_sub(1);
                if let Some(lazy) = lines[start + 1..=last_line].iter().find(|line| line.lazy) {
                    blocks.push(Block::Error(MdxError::new(MdxErrorKind::LazyLine, Some(lazy.point(0)))));
                    return FlowOutcome::Flow(lines.len());
                }
                for item in items {
                    blocks.push(match item {
                        FlowItem::Tag(tag, from) => Block::Jsx(JsxTag {
                            name: tag.name,
                            attributes: tag.attributes,
                            kind: tag.kind,
                            position: Position {
                                start: source.point(from),
                                end: source.point(tag.end),
                            },
                        }),
                        FlowItem::Expression(value, from, to) => {
                            Block::Node(Node::MdxFlowExpression(MdxFlowExpression {
                                value,
                                position: Some(Position {
                                    start: source.point(from),
                                    end: source.point(to),
                                }),
                            }))
                        }
                    });
                }
                return FlowOutcome::Flow(last_line + 1);
            }
        }
    }
}

/// Whether `lines[index]` cannot continue a paragraph lazily because MDX flow content starts there,
/// or could have started there if the document had gone on.
pub(super) fn blocks_lazy_continuation(lines: &[Line<'_>], index: usize) -> bool {
    matches!(
        probe_mdx_flow(lines, index),
        Some(FlowOutcome::Flow(_) | FlowOutcome::Pending)
    )
}

pub(super) fn probe_mdx_flow(lines: &[Line<'_>], index: usize) -> Option<FlowOutcome> {
    let line = &lines[index];
    (line.flavor.has_jsx()
        && !line.lazy
        && matches!(
            line.text.trim_start_matches([' ', '\t']).as_bytes().first(),
            Some(b'<' | b'{')
        ))
    .then(|| mdx_flow(lines, index, &mut Vec::new()))
}

/// Whether a single MDX line looks like flow content, for tracking what a paragraph can continue.
pub(super) fn looks_like_mdx_flow(line: &Line<'_>) -> bool {
    if !line.flavor.has_jsx()
        || !matches!(
            line.text.trim_start_matches([' ', '\t']).as_bytes().first(),
            Some(b'<' | b'{')
        )
    {
        return false;
    }
    // The line ending counts, unless the document ends here.
    let mut source = InlineSource::new(std::iter::once((line.text, "", line.point(0))));
    source.text.push_str(line.eol);
    !matches!(flow_items(&source), Flow::Nok(_))
}

/// The last line of `first..=last` that a paragraph takes when flow content spans them: it stops
/// before a blank line, which ends the paragraph whatever a construct that spans it is.
pub(super) fn absorbed_until(lines: &[Line<'_>], first: usize, last: usize) -> usize {
    (first + 1..=last)
        .find(|&index| lines[index].is_blank())
        .map_or(last, |blank| blank - 1)
}
