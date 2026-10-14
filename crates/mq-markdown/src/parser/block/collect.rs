//! The lines of a container, collected for as long as they belong to it.

use super::leaf::LeafState;
use super::{Interrupt, blocks_lazy_continuation};
use crate::parser::line::Line;

/// The lines of a container without its prefix, which a quote, an item or a footnote takes one by
/// one, and the blank lines that wait after them.
pub(super) struct Collector<'a> {
    /// The lines that belong to the container.
    pub(super) lines: Vec<Line<'a>>,
    /// The blank lines after the last one. They end the container unless a line that belongs to it
    /// follows.
    pub(super) blanks: Vec<Line<'a>>,
    state: LeafState<'a>,
}

impl<'a> Collector<'a> {
    /// `depth` is the depth of the container.
    pub(super) fn new(depth: usize, interrupting: Interrupt) -> Self {
        Self {
            lines: Vec::new(),
            blanks: Vec::new(),
            state: LeafState::new(interrupting, depth),
        }
    }

    /// Takes `line`, which belongs to the container, together with the blank lines before it.
    pub(super) fn push(&mut self, line: Line<'a>) {
        for blank in &self.blanks {
            self.state.feed(blank);
        }
        self.lines.append(&mut self.blanks);
        self.state.feed(&line);
        self.lines.push(line);
    }

    /// Holds a blank line back until a line that belongs to the container follows it.
    pub(super) fn blank(&mut self, line: Line<'a>) {
        self.blanks.push(line);
    }

    /// Takes `lines[index]` as a lazy continuation of the paragraph that is open, if it can be one.
    pub(super) fn lazy(&mut self, lines: &[Line<'a>], index: usize) -> bool {
        let line = &lines[index];
        if !self.blanks.is_empty() || !self.state.continues_with(line) || blocks_lazy_continuation(lines, index) {
            return false;
        }
        self.push(Line { lazy: true, ..*line });
        true
    }

    /// Keeps the blank lines that end an open fence as code, and not as the end of the container.
    pub(super) fn keep_fence_blanks(&mut self) {
        if self.blanks.first().is_some_and(|blank| self.state.fence_holds(blank)) {
            self.lines.append(&mut self.blanks);
        }
    }
}
