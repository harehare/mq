use nom_locate::LocatedSpan;

use crate::module::ModuleId;
#[cfg(feature = "ast-json")]
use serde::{Deserialize, Serialize};

/// A span representing a location in source code with module context.
///
/// This type combines nom's `LocatedSpan` with a module identifier,
/// enabling accurate source location tracking across multiple modules.
pub type Span<'a> = LocatedSpan<&'a str, ModuleId>;

/// A position in source code, representing a line and column.
#[cfg_attr(feature = "ast-json", derive(Serialize, Deserialize))]
#[derive(Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Hash)]
pub struct Position {
    pub line: u32,
    pub column: usize,
}

impl Default for Position {
    fn default() -> Self {
        Position { line: 1, column: 1 }
    }
}

impl Position {
    /// Creates a new position with the specified line and column.
    pub fn new(line: u32, column: usize) -> Self {
        Position { line, column }
    }
}

/// A range in source code, spanning from a start position to an end position.
#[cfg_attr(feature = "ast-json", derive(Serialize, Deserialize))]
#[derive(Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Default, Hash)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

impl Range {
    /// Returns `true` if the specified position falls within this range (inclusive).
    pub fn contains(&self, position: &Position) -> bool {
        (self.start.line < position.line || (self.start.line == position.line && self.start.column <= position.column))
            && (self.end.line > position.line || (self.end.line == position.line && self.end.column >= position.column))
    }
}

impl<'a> From<Span<'a>> for Range {
    fn from(span: Span<'a>) -> Self {
        let fragment = span.fragment();
        let fragment = if !fragment.starts_with(" ") && fragment.ends_with(" ") {
            fragment.trim()
        } else {
            fragment
        };

        Range {
            start: Position {
                line: span.location_line(),
                column: span.get_utf8_column(),
            },
            end: Position {
                line: span.location_line(),
                column: span.get_utf8_column() + fragment.chars().count(),
            },
        }
    }
}

impl<'a> From<Span<'a>> for Position {
    fn from(span: Span<'a>) -> Self {
        Position {
            line: span.location_line(),
            column: span.get_utf8_column(),
        }
    }
}
