use nom_locate::LocatedSpan;

use crate::eval::module::ModuleId;

pub type Span<'a> = LocatedSpan<&'a str, ModuleId>;

#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Hash)]
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
    pub fn new(line: u32, column: usize) -> Self {
        Position { line, column }
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Default, Hash)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

impl Range {
    pub fn contains(&self, position: &Position) -> bool {
        (self.start.line < position.line
            || (self.start.line == position.line && self.start.column <= position.column))
            && (self.end.line > position.line
                || (self.end.line == position.line && self.end.column >= position.column))
    }
}

impl<'a> From<Span<'a>> for Range {
    fn from(span: Span<'a>) -> Self {
        Range {
            start: Position {
                line: span.location_line(),
                column: span.get_utf8_column(),
            },
            end: Position {
                line: span.location_line(),
                column: span.get_utf8_column() + span.fragment().chars().count(),
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
