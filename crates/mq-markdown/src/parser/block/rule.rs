//! Block constructs as rules that [`parse_blocks`](super::parse_blocks) tries in order.
//!
//! A rule starts at the current line, pushes its blocks and tells where parsing resumes. The rules are
//! types chained in [`Rules`], so the dispatch is static and the order is the precedence of the
//! constructs. The paragraph is not a rule: it is what is left when no rule starts.

use super::{
    Interrupt, ItemMarker, atx_depth, atx_heading, blockquote, blockquote_marker, footnote, footnote_marker,
    html_block, html_start, is_thematic_break, join_lines, list, paragraph,
};
use crate::node::{HorizontalRule, HorizontalRuleMarker, MdxJsEsm, Node, Position};
use crate::parser::code::{Fence, fenced_code, indented_code};
use crate::parser::line::Line;
use crate::parser::mdx_flow::{FlowOutcome, mdx_flow};
use crate::parser::table;
use crate::parser::tree::Block;

/// The line a rule starts at and what is known about the lines before it.
pub(super) struct Cx<'a, 'l> {
    pub(super) lines: &'l [Line<'a>],
    pub(super) index: usize,
    pub(super) line: &'l Line<'a>,
    /// Columns and byte length of the indentation of `line`, and the text after it.
    pub(super) columns: usize,
    pub(super) indent: usize,
    pub(super) rest: &'a str,
    pub(super) depth: usize,
    /// Whether containers can still be nested.
    pub(super) containers: bool,
    pub(super) interrupting: Interrupt,
    /// Whether the previous block is a container.
    pub(super) closed_container: bool,
    /// Whether a blank line or an empty list item preceded `line`.
    pub(super) separated: bool,
}

/// Where parsing resumes after a rule and what the block restricts on the next line.
pub(super) struct Step {
    pub(super) next: usize,
    pub(super) container: bool,
    pub(super) interrupt: Interrupt,
}

impl Step {
    fn to(next: usize) -> Self {
        Self {
            next,
            container: false,
            interrupt: Interrupt::default(),
        }
    }

    fn container(next: usize) -> Self {
        Self {
            container: true,
            ..Self::to(next)
        }
    }

    fn interrupt((next, interrupt): (usize, Interrupt)) -> Self {
        Self {
            interrupt,
            ..Self::to(next)
        }
    }
}

/// A block construct. `parse` pushes to `blocks` only when it returns a step.
pub(super) trait BlockRule {
    fn parse(cx: &Cx<'_, '_>, blocks: &mut Vec<Block>) -> Option<Step>;
}

impl<A: BlockRule, B: BlockRule> BlockRule for (A, B) {
    fn parse(cx: &Cx<'_, '_>, blocks: &mut Vec<Block>) -> Option<Step> {
        A::parse(cx, blocks).or_else(|| B::parse(cx, blocks))
    }
}

/// Lines a paragraph is made of when no rule starts.
pub(super) fn fallback(cx: &Cx<'_, '_>, blocks: &mut Vec<Block>) -> Step {
    Step::interrupt(paragraph(cx.lines, cx.index, blocks))
}

macro_rules! chain {
    ($rule:ty) => { $rule };
    ($rule:ty, $($rest:ty),+) => { ($rule, chain!($($rest),+)) };
}

/// The rules in the order they are tried.
pub(super) type Rules = chain!(
    IndentedCode,
    FencedCode,
    AtxHeading,
    ThematicBreak,
    Blockquote,
    List,
    Html,
    Footnote,
    Table,
    Esm,
    MdxFlow
);

/// Indented code, which cannot interrupt a paragraph and is part of it then.
pub(super) struct IndentedCode;

impl BlockRule for IndentedCode {
    fn parse(cx: &Cx<'_, '_>, blocks: &mut Vec<Block>) -> Option<Step> {
        if cx.columns < cx.line.code_indent() {
            return None;
        }
        Some(if cx.interrupting.code {
            fallback(cx, blocks)
        } else {
            Step::to(indented_code(cx.lines, cx.index, blocks))
        })
    }
}

pub(super) struct FencedCode;

impl BlockRule for FencedCode {
    fn parse(cx: &Cx<'_, '_>, blocks: &mut Vec<Block>) -> Option<Step> {
        let fence = Fence::open(cx.rest, cx.line.flavor.has_math())?;
        // The end of an unclosed fence without content is quirky right after a container.
        let own_end = cx.closed_container
            && !(cx.separated
                && matches!(blocks.last(), Some(Block::List(l)) if l.items.last().is_some_and(|i| i.children.is_empty())));
        Some(Step::to(fenced_code(
            cx.lines, cx.index, cx.indent, &fence, own_end, blocks,
        )))
    }
}

pub(super) struct AtxHeading;

impl BlockRule for AtxHeading {
    fn parse(cx: &Cx<'_, '_>, blocks: &mut Vec<Block>) -> Option<Step> {
        let depth = atx_depth(cx.rest)?;
        blocks.push(atx_heading(cx.line, cx.indent, depth));
        Some(Step::to(cx.index + 1))
    }
}

pub(super) struct ThematicBreak;

impl BlockRule for ThematicBreak {
    fn parse(cx: &Cx<'_, '_>, blocks: &mut Vec<Block>) -> Option<Step> {
        if !is_thematic_break(cx.line, cx.indent) {
            return None;
        }
        blocks.push(Block::Node(Node::HorizontalRule(HorizontalRule {
            marker: cx.rest.chars().next().and_then(HorizontalRuleMarker::from_char),
            position: Some(Position {
                start: cx.line.point(0),
                end: cx.line.end(),
            }),
        })));
        Some(Step::to(cx.index + 1))
    }
}

pub(super) struct Blockquote;

impl BlockRule for Blockquote {
    fn parse(cx: &Cx<'_, '_>, blocks: &mut Vec<Block>) -> Option<Step> {
        if !cx.containers || blockquote_marker(cx.line).is_none() {
            return None;
        }
        Some(Step::container(blockquote(
            cx.lines,
            cx.index,
            cx.depth,
            cx.interrupting,
            blocks,
        )))
    }
}

pub(super) struct List;

impl BlockRule for List {
    fn parse(cx: &Cx<'_, '_>, blocks: &mut Vec<Block>) -> Option<Step> {
        let marker = ItemMarker::parse(cx.line)
            .filter(|marker| cx.containers && (!cx.interrupting.list || marker.interrupts_paragraph()))?;
        Some(Step::container(list(
            cx.lines,
            cx.index,
            &marker,
            cx.depth,
            cx.interrupting,
            blocks,
        )))
    }
}

pub(super) struct Html;

impl BlockRule for Html {
    fn parse(cx: &Cx<'_, '_>, blocks: &mut Vec<Block>) -> Option<Step> {
        let kind = html_start(cx.line, cx.rest)?;
        Some(Step::to(html_block(cx.lines, cx.index, kind, blocks)))
    }
}

pub(super) struct Footnote;

impl BlockRule for Footnote {
    fn parse(cx: &Cx<'_, '_>, blocks: &mut Vec<Block>) -> Option<Step> {
        let marker = footnote_marker(cx.line).filter(|_| cx.containers)?;
        Some(Step::container(footnote(
            cx.lines,
            cx.index,
            &marker,
            cx.depth,
            cx.interrupting,
            blocks,
        )))
    }
}

pub(super) struct Table;

impl BlockRule for Table {
    fn parse(cx: &Cx<'_, '_>, blocks: &mut Vec<Block>) -> Option<Step> {
        let (items, next) = table::parse(cx.lines, cx.index, super::interrupts_paragraph, super::ends_table)?;
        blocks.push(Block::Table(items));
        Some(Step::to(next))
    }
}

/// How many brackets are open after `text`, given the `depth` before it. Strings and a comment to the end
/// of the line are skipped.
fn open_brackets(text: &str, mut depth: i32) -> i32 {
    let mut chars = text.chars().peekable();
    while let Some(char) = chars.next() {
        match char {
            '{' | '(' | '[' => depth += 1,
            '}' | ')' | ']' => depth -= 1,
            '\'' | '"' | '`' => {
                while let Some(next) = chars.next() {
                    if next == '\\' {
                        chars.next();
                    } else if next == char {
                        break;
                    }
                }
            }
            '/' if chars.peek() == Some(&'/') => break,
            _ => {}
        }
    }
    depth
}

/// MDX `import` and `export`, which go on to the next blank line.
pub(super) struct Esm;

impl BlockRule for Esm {
    fn parse(cx: &Cx<'_, '_>, blocks: &mut Vec<Block>) -> Option<Step> {
        let line = cx.line;
        if !(line.flavor.has_jsx()
            && cx.depth == 0
            && cx.indent == 0
            && (line.text.starts_with("import ") || line.text.starts_with("export ")))
        {
            return None;
        }
        // Without a JavaScript parser the module goes on to the next blank line, or past it when a bracket
        // is still open.
        let mut depth = 0i32;
        let mut end = cx.lines.len();
        for (offset, line) in cx.lines[cx.index..].iter().enumerate() {
            if line.is_blank() && depth <= 0 {
                end = cx.index + offset;
                break;
            }
            depth = open_brackets(line.text, depth);
        }
        let parts = cx.lines[cx.index..end]
            .iter()
            .map(|line| (line.text, line.eol))
            .collect::<Vec<_>>();
        blocks.push(Block::Node(Node::MdxJsEsm(MdxJsEsm {
            value: join_lines(&parts).into(),
            position: Some(Position {
                start: line.point(0),
                end: cx.lines[end - 1].end(),
            }),
        })));
        Some(Step::to(end))
    }
}

pub(super) struct MdxFlow;

impl BlockRule for MdxFlow {
    fn parse(cx: &Cx<'_, '_>, blocks: &mut Vec<Block>) -> Option<Step> {
        if !(cx.line.flavor.has_jsx() && matches!(cx.rest.as_bytes().first(), Some(b'<' | b'{'))) {
            return None;
        }
        let FlowOutcome::Flow(next) = mdx_flow(cx.lines, cx.index, blocks) else {
            return None;
        };
        // Flow content that interrupted a paragraph keeps the restrictions that came with it.
        Some(Step {
            interrupt: cx.interrupting,
            ..Step::to(next)
        })
    }
}
