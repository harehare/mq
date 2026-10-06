//! Parser for type expressions such as `h1 | h2`, `markdown - code` and `[number]`.
//!
//! ```text
//! type   = union
//! union  = diff { "|" diff }
//! diff   = atom { "-" atom }          (node types only)
//! atom   = name | "[" type "]" | "(" type ")"
//! name   = "markdown" | "h" | a node kind (h1, code, list, ...)
//!        | "number" | "string" | "bool" | "none" | "symbol" | "bytes" | "dynamic"
//! ```

use miette::{Diagnostic, SourceSpan};
use mq_markdown::NodeKind;
use thiserror::Error;

use crate::{kind_set::KindSet, types::Type};

/// An error in a type expression.
#[derive(Debug, Clone, Error, Diagnostic, PartialEq, Eq)]
#[error("Invalid type `{source_text}`: {message}")]
#[diagnostic(code(typechecker::invalid_type_expression))]
pub struct TypeExprError {
    source_text: String,
    message: String,
    #[label("here")]
    span: SourceSpan,
    #[help]
    help: Option<String>,
}

impl TypeExprError {
    /// The message followed by the hint on valid names, for command line output.
    pub fn with_help(&self) -> String {
        match &self.help {
            Some(help) => format!("{self}\n{help}"),
            None => self.to_string(),
        }
    }
}

/// Parses a type expression.
pub fn parse_type(src: &str) -> Result<Type, TypeExprError> {
    let mut parser = Parser { src, pos: 0 };
    let ty = parser.parse_union()?;
    parser.skip_whitespace();
    match parser.peek() {
        None => Ok(ty),
        Some(c) => Err(parser.error(format!("unexpected `{c}`"), 1, None)),
    }
}

struct Parser<'a> {
    src: &'a str,
    pos: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<char> {
        self.src[self.pos..].chars().next()
    }

    fn skip_whitespace(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.pos += 1;
        }
    }

    /// Consumes `expected` after optional whitespace.
    fn eat(&mut self, expected: char) -> bool {
        self.skip_whitespace();
        if self.peek() == Some(expected) {
            self.pos += expected.len_utf8();
            true
        } else {
            false
        }
    }

    fn error(&self, message: String, len: usize, help: Option<String>) -> TypeExprError {
        TypeExprError {
            source_text: self.src.to_string(),
            message,
            span: (self.pos.min(self.src.len()), len).into(),
            help,
        }
    }

    fn parse_union(&mut self) -> Result<Type, TypeExprError> {
        let mut members = vec![self.parse_difference()?];
        while self.eat('|') {
            members.push(self.parse_difference()?);
        }
        Ok(Type::union(members))
    }

    fn parse_difference(&mut self) -> Result<Type, TypeExprError> {
        let start = self.pos;
        let mut ty = self.parse_atom()?;
        while self.eat('-') {
            let rhs_start = self.pos;
            let rhs = self.parse_atom()?;
            let (Type::Node(have), Type::Node(removed)) = (&ty, &rhs) else {
                self.pos = rhs_start;
                return Err(self.error(
                    "`-` removes node kinds and needs node types on both sides".to_string(),
                    1,
                    None,
                ));
            };
            let rest = have.difference(*removed);
            if rest.is_empty() {
                self.pos = start;
                return Err(self.error("no node kind is left after `-`".to_string(), rhs_start - start, None));
            }
            ty = Type::Node(rest);
        }
        Ok(ty)
    }

    fn parse_atom(&mut self) -> Result<Type, TypeExprError> {
        self.skip_whitespace();
        match self.peek() {
            Some('[') => {
                self.pos += 1;
                let elem = self.parse_union()?;
                if !self.eat(']') {
                    return Err(self.error("expected `]`".to_string(), 1, None));
                }
                Ok(Type::array(elem))
            }
            Some('(') => {
                self.pos += 1;
                let ty = self.parse_union()?;
                if !self.eat(')') {
                    return Err(self.error("expected `)`".to_string(), 1, None));
                }
                Ok(ty)
            }
            Some(c) if c.is_alphanumeric() || c == '_' => self.parse_name(),
            Some(c) => Err(self.error(format!("unexpected `{c}`"), c.len_utf8(), None)),
            None => Err(self.error("expected a type".to_string(), 1, None)),
        }
    }

    fn parse_name(&mut self) -> Result<Type, TypeExprError> {
        let start = self.pos;
        while self.peek().is_some_and(|c| c.is_alphanumeric() || c == '_') {
            self.pos += 1;
        }
        let name = &self.src[start..self.pos];
        named_type(name).ok_or_else(|| {
            self.pos = start;
            self.error(
                format!("unknown type `{name}`"),
                name.len(),
                Some(format!("expected one of: {}", known_names().join(", "))),
            )
        })
    }
}

fn named_type(name: &str) -> Option<Type> {
    Some(match name {
        "markdown" => Type::markdown(),
        "h" => Type::Node(KindSet::HEADING),
        "number" => Type::Number,
        "string" => Type::String,
        "bool" => Type::Bool,
        "none" => Type::None,
        "symbol" => Type::Symbol,
        "bytes" => Type::Bytes,
        "dynamic" => Type::Dynamic,
        _ => {
            let kind = NodeKind::ALL
                .into_iter()
                .find(|kind| !kind.name().is_empty() && kind.name().eq_ignore_ascii_case(name))?;
            Type::Node(KindSet::of(kind))
        }
    })
}

fn known_names() -> Vec<&'static str> {
    let mut names = vec![
        "markdown", "h", "number", "string", "bool", "none", "symbol", "bytes", "dynamic",
    ];
    names.extend(
        NodeKind::ALL
            .into_iter()
            .map(NodeKind::name)
            .filter(|name| !name.is_empty()),
    );
    names
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    fn node(kinds: impl IntoIterator<Item = NodeKind>) -> Type {
        Type::Node(KindSet::from_kinds(kinds))
    }

    #[rstest]
    #[case::markdown("markdown", Type::markdown())]
    #[case::one_kind("code", node([NodeKind::Code]))]
    #[case::heading_alias("h", Type::Node(KindSet::HEADING))]
    #[case::union("h1 | h2", node([NodeKind::H1, NodeKind::H2]))]
    #[case::union_with_whitespace("  h1|  h2 ", node([NodeKind::H1, NodeKind::H2]))]
    #[case::difference("h - h2", node([NodeKind::H1, NodeKind::H3, NodeKind::H4, NodeKind::H5, NodeKind::H6]))]
    #[case::difference_then_union("markdown - code | none", Type::union(vec![Type::Node(KindSet::ALL.difference(KindSet::of(NodeKind::Code))), Type::None]))]
    #[case::case_insensitive("Horizontal_rule", node([NodeKind::HorizontalRule]))]
    #[case::scalar("number | string", Type::union(vec![Type::Number, Type::String]))]
    #[case::array("[h1]", Type::array(node([NodeKind::H1])))]
    #[case::parentheses("([h1 | h2])", Type::array(node([NodeKind::H1, NodeKind::H2])))]
    #[case::dynamic("dynamic", Type::Dynamic)]
    fn test_parse_type(#[case] src: &str, #[case] expected: Type) {
        assert_eq!(parse_type(src), Ok(expected));
    }

    #[rstest]
    #[case::unknown("headin", "unknown type `headin`")]
    #[case::empty("", "expected a type")]
    #[case::trailing("h1 )", "unexpected `)`")]
    #[case::unclosed_array("[h1", "expected `]`")]
    #[case::dangling_union("h1 |", "expected a type")]
    #[case::difference_of_scalars("number - string", "`-` removes node kinds")]
    #[case::nothing_left("h1 - h", "no node kind is left")]
    fn test_parse_type_errors(#[case] src: &str, #[case] message: &str) {
        let error = parse_type(src).unwrap_err();
        assert!(error.to_string().contains(message), "{error}");
    }

    #[test]
    fn test_unknown_name_lists_the_known_ones() {
        let error = parse_type("nope").unwrap_err();
        let help = miette::Diagnostic::help(&error).unwrap().to_string();
        assert!(help.contains("h1") && help.contains("markdown"), "{help}");
    }
}
