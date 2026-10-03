#[cfg(feature = "ast-json")]
use serde::{Deserialize, Serialize};

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
