//! Tracks the containers and the leaf block that are open in the lines a container has collected, to
//! tell whether a following line without the container prefix continues a paragraph.

use super::{
    FOOTNOTE_INDENT, Interrupt, ItemMarker, LineStart, MAX_DEPTH, after_blockquote_marker, interrupts_paragraph,
    is_lazy_continuation, setext_depth,
};
use crate::parser::code::Fence;
use crate::parser::definition::only_definitions;
use crate::parser::html_flow::{self, Kind as HtmlKind};
use crate::parser::line::{Indent, Line};
use crate::parser::mdx_flow::{FlowEnd, FlowStart, flow_end, flow_start};
use crate::parser::table;

/// MDX flow content that spans lines is followed for this many lines, which bounds the work.
const MAX_FLOW_LINES: usize = 64;

/// A container that is open in the collected lines, inside the one that collects them.
enum Open {
    Quote,
    /// A list item, whose content is indented by `width` columns.
    Item {
        width: usize,
        /// Whether the item started empty and has no content yet: a blank line then ends it.
        empty: bool,
    },
    Footnote,
}

/// MDX flow content that goes on in the next line.
struct Pending {
    text: String,
    lines: usize,
}

pub(super) struct LeafState<'a> {
    /// The containers that are open, outermost first. The leaf blocks below belong to the innermost.
    open: Vec<Open>,
    fence: Option<Fence<'a>>,
    /// An HTML block that has not ended yet.
    html: Option<HtmlKind>,
    /// A table that has not ended yet. Its rows cannot be continued lazily.
    table: bool,
    /// The number of cells of the last line when it was paragraph text with a pipe, which a delimiter
    /// row with as many cells can turn into a header.
    header: Option<usize>,
    paragraph: bool,
    /// The text of the open paragraph when it starts like a definition, which a setext underline does
    /// not turn into a heading when there is nothing else in it.
    definitions: Option<String>,
    flow: Option<Pending>,
    /// Restrictions on the next fed line, the first line of a container that interrupted a paragraph.
    restricted: Interrupt,
    /// How many containers can still be open before the depth limit, which no longer open any.
    budget: usize,
}

impl<'a> LeafState<'a> {
    /// `depth` is the depth of the container that collects the lines.
    pub(super) fn new(restricted: Interrupt, depth: usize) -> Self {
        Self {
            open: Vec::new(),
            fence: None,
            html: None,
            table: false,
            header: None,
            paragraph: false,
            definitions: None,
            flow: None,
            restricted,
            budget: MAX_DEPTH.saturating_sub(depth + 1),
        }
    }

    /// Whether `line` without its container prefix continues the paragraph of the collected lines.
    pub(super) fn continues_with(&self, line: &Line<'_>) -> bool {
        self.paragraph && is_lazy_continuation(line)
    }

    pub(super) fn feed(&mut self, line: &Line<'a>) {
        self.feed_line(*line);
        if !self.paragraph {
            self.definitions = None;
        }
    }

    /// Removes the prefixes of the open containers from `line`, and returns how many of them it has.
    fn strip_prefixes(&self, mut line: Line<'a>) -> (usize, Line<'a>) {
        for (index, open) in self.open.iter().enumerate() {
            let width = match open {
                Open::Quote => {
                    let indent = line.indent();
                    if !matches!(LineStart::of(&line, indent), Some(LineStart::Blockquote)) {
                        return (index, line);
                    }
                    line = after_blockquote_marker(line, indent);
                    continue;
                }
                // Another blank line ends an item that started empty.
                Open::Item { empty: true, .. } if line.is_blank() => return (index, line),
                Open::Item { width, .. } => *width,
                Open::Footnote => FOOTNOTE_INDENT,
            };
            // A blank line has the prefix of an item, whatever its indentation.
            if !line.is_blank() && line.indent().columns < width {
                return (index, line);
            }
            line = line.skip_columns(width);
        }
        (self.open.len(), line)
    }

    /// Ends the containers from the `keep`th on, and with them what they hold.
    fn close_from(&mut self, keep: usize) {
        self.open.truncate(keep);
        self.fence = None;
        self.html = None;
        self.table = false;
        self.header = None;
        self.paragraph = false;
        self.flow = None;
    }

    fn feed_line(&mut self, line: Line<'a>) {
        let restricted = std::mem::take(&mut self.restricted);
        let mut line = line;
        // A lazy line was already accepted by the container that collects the lines.
        if !line.lazy {
            let (matched, rest) = self.strip_prefixes(line);
            if matched < self.open.len() {
                // Containers that do not match end, unless the line goes on with their paragraph.
                if self.paragraph && flow_start(&rest).is_none() && is_lazy_continuation(&rest) {
                    line = Line { lazy: true, ..rest };
                } else {
                    self.close_from(matched);
                    line = rest;
                }
            } else {
                line = rest;
            }
            if !line.is_blank() {
                for open in &mut self.open {
                    if let Open::Item { empty, .. } = open {
                        *empty = false;
                    }
                }
            }
        }
        // A paragraph that this line is in the container of, which markers have to interrupt.
        let in_paragraph = self.paragraph && !line.lazy;

        loop {
            let line_indent = line.indent();
            let Indent { columns, bytes: indent } = line_indent;
            let rest = &line.text[indent..];

            if let Some(fence) = &self.fence {
                if columns < line.code_indent() && fence.is_closed_by(rest) {
                    self.fence = None;
                }
                return;
            }

            if let Some(kind) = self.html {
                let ends = match kind {
                    HtmlKind::Basic | HtmlKind::Complete => line.is_blank(),
                    _ => html_flow::ends_in(kind, line.text),
                };
                if ends {
                    self.html = None;
                }
                self.paragraph = false;
                return;
            }

            if let Some(pending) = &mut self.flow {
                pending.text.push_str(line.eol);
                pending.text.push_str(line.text);
                pending.lines += 1;
                let end = if pending.lines < MAX_FLOW_LINES {
                    flow_end(&pending.text)
                } else {
                    FlowEnd::Done
                };
                if matches!(end, FlowEnd::Open) {
                    return;
                }
                self.flow = None;
                self.paragraph = matches!(end, FlowEnd::Text);
                return;
            }

            if line.is_blank() {
                self.paragraph = false;
                self.table = false;
                self.header = None;
                return;
            }
            let header = std::mem::take(&mut self.header);
            if self.table {
                if interrupts_paragraph(&line) {
                    self.table = false;
                } else {
                    self.paragraph = false;
                    return;
                }
            } else if self.paragraph
                && !line.lazy
                && header.is_some()
                && table::delimiter_cells(&line) == header
                && !interrupts_paragraph(&line)
            {
                self.table = true;
                self.paragraph = false;
                return;
            }
            if columns >= line.code_indent() {
                // Continues an open paragraph, otherwise it is indented code (or text when the
                // container interrupted a paragraph).
                if !self.paragraph {
                    self.paragraph = restricted.code;
                } else if let Some(text) = &mut self.definitions {
                    text.push('\n');
                    text.push_str(rest);
                }
                return;
            }
            let start = LineStart::of(&line, line_indent);
            if let Some(LineStart::Fence(fence)) = start {
                self.fence = Some(fence);
                self.paragraph = false;
                return;
            }
            if self.paragraph && setext_depth(&line).is_some() {
                // Under nothing but definitions an underline is text, unless it is a rule.
                if self.definitions.as_deref().is_some_and(only_definitions) {
                    self.definitions = None;
                    self.paragraph = !matches!(start, Some(LineStart::ThematicBreak));
                } else {
                    self.paragraph = false;
                }
                return;
            }
            let can_open = self.open.len() < self.budget;
            match start {
                Some(LineStart::Atx(_) | LineStart::ThematicBreak) => {
                    self.paragraph = false;
                    return;
                }
                Some(LineStart::Html(kind)) if kind != HtmlKind::Complete || !self.paragraph => {
                    let closed = match kind {
                        HtmlKind::Basic | HtmlKind::Complete => false,
                        _ => html_flow::ends_in(kind, &rest[html_flow::first_line_offset(kind)..]),
                    };
                    self.html = (!closed).then_some(kind);
                    self.paragraph = false;
                    return;
                }
                Some(LineStart::Blockquote) if can_open => {
                    line = after_blockquote_marker(line, line_indent);
                    self.open.push(Open::Quote);
                    self.paragraph = false;
                    continue;
                }
                Some(LineStart::Footnote(marker)) if can_open => {
                    line = line.skip(marker.content);
                    self.open.push(Open::Footnote);
                    self.paragraph = false;
                    continue;
                }
                // A marker in the container of the open paragraph has to be able to interrupt it.
                Some(LineStart::Item(marker))
                    if can_open && (!(restricted.list || in_paragraph) || marker.interrupts_paragraph()) =>
                {
                    self.open.push(item(&marker));
                    line = marker.content(line);
                    // The rest of the line starts a new item.
                    self.paragraph = false;
                    continue;
                }
                _ => {}
            }

            // MDX flow content is not part of a paragraph.
            if let Some(start) = flow_start(&line) {
                self.paragraph = false;
                if matches!(start, FlowStart::Pending) {
                    self.flow = Some(Pending {
                        text: line.text.to_string(),
                        lines: 1,
                    });
                }
                return;
            }
            self.header = rest.contains('|').then(|| table::row_cells(&line));
            if self.paragraph {
                if let Some(text) = &mut self.definitions {
                    text.push('\n');
                    text.push_str(rest);
                }
            } else {
                self.definitions = rest.starts_with('[').then(|| rest.to_string());
            }
            self.paragraph = true;
            return;
        }
    }
}

fn item(marker: &ItemMarker) -> Open {
    Open::Item {
        width: marker.width,
        empty: marker.empty,
    }
}
