//! Errors in MDX, the only content that can be invalid.

use super::tree::InlineSource;
use crate::node::Point;
use std::fmt;

/// What a JSX tag has where something else must be.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum MdxFound {
    Char(char),
    EndOfFile,
}

impl fmt::Display for MdxFound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MdxFound::Char(char) => write!(f, "character `{char}` (U+{:04X})", *char as u32),
            MdxFound::EndOfFile => write!(f, "end of file"),
        }
    }
}

/// The part of a JSX tag where an unexpected character is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum MdxPlace {
    BeforeName,
    AfterName,
    BeforeMemberName,
    BeforeLocalName,
    BeforeAttributeName,
    InAttributeName,
    BeforeLocalAttributeName,
    InLocalAttributeName,
    AfterAttributeName,
    BeforeAttributeValue,
    AfterSelfClosingSlash,
}

impl fmt::Display for MdxPlace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            MdxPlace::BeforeName => "before name",
            MdxPlace::AfterName => "after name",
            MdxPlace::BeforeMemberName => "before member name",
            MdxPlace::BeforeLocalName => "before local name",
            MdxPlace::BeforeAttributeName => "before attribute name",
            MdxPlace::InAttributeName => "in attribute name",
            MdxPlace::BeforeLocalAttributeName => "before local attribute name",
            MdxPlace::InLocalAttributeName => "in local attribute name",
            MdxPlace::AfterAttributeName => "after attribute name",
            MdxPlace::BeforeAttributeValue => "before attribute value",
            MdxPlace::AfterSelfClosingSlash => "after self-closing slash",
        })
    }
}

/// What is wrong with invalid MDX.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum MdxErrorKind {
    /// A character, or the end of the input, where a JSX tag cannot have it.
    Unexpected { found: MdxFound, place: MdxPlace },
    /// An expression without the `}` that closes its `{`.
    UnclosedExpression,
    /// An attribute value without its closing quote.
    UnclosedAttributeValue { quote: char },
    /// A closing tag that ends with `/>`.
    SelfClosingSlashInClosingTag,
    /// A closing tag with attributes.
    AttributeInClosingTag,
    /// A line of an expression or a tag inside a container that lacks the prefix of the container.
    LazyLine,
    /// Elements nested deeper than the parser follows.
    TooDeep { limit: usize },
    /// A closing tag without an element open.
    UnopenedClosingTag,
    /// A closing tag whose name is not that of the open element. Elements on lines of their own say
    /// where the open one starts.
    MismatchedClosingTag {
        closing: Option<String>,
        opening: Option<String>,
        opened_at: Option<Point>,
    },
    /// An element on lines of its own that is not closed in its container.
    UnclosedFlowElement { name: Option<String>, opened_at: Point },
    /// An element in text that is not closed in its paragraph.
    UnclosedTextElement { name: Option<String> },
}

impl fmt::Display for MdxErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = |name: &Option<String>| name.clone().unwrap_or_default();
        match self {
            MdxErrorKind::Unexpected { found, place } => write!(f, "Unexpected {found} {place}"),
            MdxErrorKind::UnclosedExpression => {
                write!(
                    f,
                    "Unexpected end of file in expression, expected a corresponding closing brace for `{{`"
                )
            }
            MdxErrorKind::UnclosedAttributeValue { quote } => write!(
                f,
                "Unexpected end of file in attribute value, expected a corresponding closing quote `{quote}`"
            ),
            MdxErrorKind::SelfClosingSlashInClosingTag => write!(
                f,
                "Unexpected self-closing slash `/` in closing tag, expected the end of the tag"
            ),
            MdxErrorKind::AttributeInClosingTag => {
                write!(f, "Unexpected attribute in closing tag, expected the end of the tag")
            }
            MdxErrorKind::LazyLine => write!(
                f,
                "Unexpected lazy line in expression in container, expected line to be prefixed with `>` when in a block quote, whitespace when in a list, etc"
            ),
            MdxErrorKind::TooDeep { limit } => write!(f, "Elements are nested deeper than {limit} levels"),
            MdxErrorKind::UnopenedClosingTag => {
                write!(f, "Unexpected closing slash `/` in tag, expected an open tag first")
            }
            MdxErrorKind::MismatchedClosingTag {
                closing,
                opening,
                opened_at,
            } => {
                write!(
                    f,
                    "Unexpected closing tag `</{}>`, expected corresponding closing tag for `<{}>`",
                    name(closing),
                    name(opening)
                )?;
                match opened_at {
                    Some(at) => write!(f, " ({}:{})", at.line, at.column),
                    None => Ok(()),
                }
            }
            MdxErrorKind::UnclosedFlowElement {
                name: element,
                opened_at,
            } => write!(
                f,
                "Expected a closing tag for `<{}>` ({}:{}) before the end of its container",
                name(element),
                opened_at.line,
                opened_at.column
            ),
            MdxErrorKind::UnclosedTextElement { name: element } => write!(
                f,
                "Expected a closing tag for `<{}>` before the end of the content",
                name(element)
            ),
        }
    }
}

/// An error at an offset of the text that is being parsed, before the offset is known as a position.
#[derive(Clone, Debug)]
pub(super) struct Located {
    pub(super) kind: MdxErrorKind,
    pub(super) offset: usize,
}

impl Located {
    /// The error at its position in the document.
    pub(super) fn in_source(self, source: &InlineSource) -> MdxError {
        MdxError {
            position: Some(source.point(self.offset)),
            kind: self.kind,
        }
    }
}

/// Invalid MDX, with where it is in the document when that is known.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MdxError {
    kind: MdxErrorKind,
    position: Option<Point>,
}

impl MdxError {
    pub(super) fn new(kind: MdxErrorKind, position: Option<Point>) -> Self {
        Self { kind, position }
    }

    pub fn kind(&self) -> &MdxErrorKind {
        &self.kind
    }

    /// Where the error is, as a line and a column that count from 1.
    pub fn position(&self) -> Option<&Point> {
        self.position.as_ref()
    }

    /// A diagnostic that points at the error in `content`, the document it was found in.
    pub(super) fn into_diagnostic(self, content: &str) -> MdxDiagnostic {
        let span = self
            .position
            .as_ref()
            .and_then(|point| offset_of(content, point))
            .map(|offset| miette::SourceSpan::from((offset, 0)));
        MdxDiagnostic {
            error: self,
            source_code: content.to_string(),
            span,
        }
    }
}

impl fmt::Display for MdxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.kind.fmt(f)
    }
}

impl std::error::Error for MdxError {}

/// An [`MdxError`] with the document, to show where in it the error is.
#[derive(Debug, miette::Diagnostic)]
#[diagnostic(code(mq_markdown::mdx))]
pub struct MdxDiagnostic {
    error: MdxError,
    #[source_code]
    source_code: String,
    #[label("here")]
    span: Option<miette::SourceSpan>,
}

impl MdxDiagnostic {
    pub fn error(&self) -> &MdxError {
        &self.error
    }
}

impl fmt::Display for MdxDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.error.fmt(f)
    }
}

impl std::error::Error for MdxDiagnostic {}

/// The byte offset of `point` in `content`, where columns count bytes and a tab goes to the next
/// multiple of four, as the positions of nodes do.
fn offset_of(content: &str, point: &Point) -> Option<usize> {
    let mut start = 0;
    for _ in 1..point.line {
        let rest = &content[start..];
        let eol = rest.find(['\n', '\r'])?;
        start += eol + if rest[eol..].starts_with("\r\n") { 2 } else { 1 };
    }
    let line = &content[start..];
    let line = &line[..line.find(['\n', '\r']).unwrap_or(line.len())];
    let mut column = 0;
    for (index, char) in line.char_indices() {
        if column + 1 >= point.column {
            return Some(start + index);
        }
        column = if char == '\t' {
            (column / 4 + 1) * 4
        } else {
            column + char.len_utf8()
        };
    }
    Some(start + line.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case::first("abc", 1, 2, Some(1))]
    #[case::second_line("a\nbc", 2, 2, Some(3))]
    #[case::crlf("a\r\nbc", 2, 1, Some(3))]
    #[case::tab("\tx", 1, 5, Some(1))]
    #[case::multibyte("éx", 1, 3, Some(2))]
    #[case::end_of_line("ab\ncd", 1, 3, Some(2))]
    #[case::missing_line("ab", 3, 1, None)]
    fn offset_of_point(
        #[case] content: &str,
        #[case] line: usize,
        #[case] column: usize,
        #[case] expected: Option<usize>,
    ) {
        assert_eq!(offset_of(content, &Point { line, column }), expected);
    }
}
