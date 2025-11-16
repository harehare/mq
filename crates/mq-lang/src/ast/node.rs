use super::{Program, TokenId, error::ParseError};
#[cfg(feature = "ast-json")]
use crate::arena::ArenaId;
use crate::{Ident, Shared, Token, TokenKind, arena::Arena, lexer, number::Number, range::Range};
#[cfg(feature = "ast-json")]
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use smol_str::SmolStr;
use std::{
    convert::TryFrom,
    fmt::{self, Display, Formatter},
    hash::{Hash, Hasher},
};

type Depth = u8;
type Index = usize;
pub type Params = SmallVec<[Shared<Node>; 4]>;
pub type Args = SmallVec<[Shared<Node>; 4]>;
pub type Cond = (Option<Shared<Node>>, Shared<Node>);
pub type Branches = SmallVec<[Cond; 4]>;
pub type MatchArms = SmallVec<[MatchArm; 4]>;

#[derive(PartialEq, PartialOrd, Debug, Clone)]
#[cfg_attr(feature = "ast-json", derive(Serialize, Deserialize))]
pub struct Node {
    #[cfg_attr(
        feature = "ast-json",
        serde(skip_serializing, skip_deserializing, default = "default_token_id")
    )]
    pub token_id: TokenId,
    pub expr: Shared<Expr>,
}

#[cfg(feature = "ast-json")]
fn default_token_id() -> TokenId {
    ArenaId::new(0)
}

impl Node {
    #[cfg(feature = "ast-json")]
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    #[cfg(feature = "ast-json")]
    pub fn from_json(json_str: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json_str)
    }

    pub fn range(&self, arena: Shared<Arena<Shared<Token>>>) -> Range {
        match &*self.expr {
            Expr::Block(program)
            | Expr::Def(_, _, program)
            | Expr::Fn(_, program)
            | Expr::While(_, program)
            | Expr::Until(_, program)
            | Expr::Module(_, program)
            | Expr::Foreach(_, _, program) => {
                let start = program
                    .first()
                    .map(|node| node.range(Shared::clone(&arena)).start)
                    .unwrap_or_default();
                let end = program
                    .last()
                    .map(|node| node.range(Shared::clone(&arena)).end)
                    .unwrap_or_default();
                Range { start, end }
            }
            Expr::Call(_, args) => {
                let start = args
                    .first()
                    .map(|node| node.range(Shared::clone(&arena)).start)
                    .unwrap_or_default();
                let end = args
                    .last()
                    .map(|node| node.range(Shared::clone(&arena)).end)
                    .unwrap_or_default();
                Range { start, end }
            }
            Expr::CallDynamic(callable, args) => {
                let start = callable.range(Shared::clone(&arena)).start;
                let end = args
                    .last()
                    .map(|node| node.range(Shared::clone(&arena)).end)
                    .unwrap_or_else(|| callable.range(Shared::clone(&arena)).end);
                Range { start, end }
            }
            Expr::Let(_, node) => node.range(Shared::clone(&arena)),
            Expr::If(nodes) => {
                if let (Some(first), Some(last)) = (nodes.first(), nodes.last()) {
                    let start = first.1.range(Shared::clone(&arena));
                    let end = last.1.range(Shared::clone(&arena));
                    Range {
                        start: start.start,
                        end: end.end,
                    }
                } else {
                    // Fallback to token range if no branches exist
                    arena[self.token_id].range.clone()
                }
            }
            Expr::Match(value, arms) => {
                let start = value.range(Shared::clone(&arena)).start;
                let end = arms
                    .last()
                    .map(|arm| arm.body.range(Shared::clone(&arena)).end)
                    .unwrap_or_else(|| arena[self.token_id].range.end.clone());
                Range { start, end }
            }
            Expr::Paren(node) => node.range(Shared::clone(&arena)),
            Expr::Try(try_expr, catch_expr) => {
                let start = try_expr.range(Shared::clone(&arena)).start;
                let end = catch_expr.range(Shared::clone(&arena)).end;
                Range { start, end }
            }
            Expr::Literal(_)
            | Expr::Ident(_)
            | Expr::Selector(_)
            | Expr::Include(_)
            | Expr::Import(_)
            | Expr::InterpolatedString(_)
            | Expr::QualifiedAccess(_, _)
            | Expr::Nodes
            | Expr::Self_
            | Expr::Break
            | Expr::Continue => arena[self.token_id].range.clone(),
        }
    }

    pub fn is_nodes(&self) -> bool {
        matches!(*self.expr, Expr::Nodes)
    }
}

#[cfg_attr(feature = "ast-json", derive(Serialize, Deserialize))]
#[derive(PartialEq, Debug, Eq, Clone)]
pub struct IdentWithToken {
    pub name: Ident,
    #[cfg_attr(feature = "ast-json", serde(skip_serializing_if = "Option::is_none", default))]
    pub token: Option<Shared<Token>>,
}

impl Hash for IdentWithToken {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name.hash(state);
    }
}

impl Ord for IdentWithToken {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.name.cmp(&other.name)
    }
}

impl PartialOrd for IdentWithToken {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl IdentWithToken {
    pub fn new(name: &str) -> Self {
        Self::new_with_token(name, None)
    }

    pub fn new_with_token(name: &str, token: Option<Shared<Token>>) -> Self {
        Self {
            name: name.into(),
            token,
        }
    }
}

impl Display for IdentWithToken {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "{}", self.name)
    }
}

#[cfg_attr(feature = "ast-json", derive(Serialize, Deserialize))]
#[derive(PartialEq, PartialOrd, Debug, Eq, Clone)]
pub enum Selector {
    Blockquote,
    Footnote,
    List(Option<Index>, Option<bool>),
    Toml,
    Yaml,
    Break,
    InlineCode,
    InlineMath,
    Delete,
    Emphasis,
    FootnoteRef,
    Html,
    Image,
    ImageRef,
    MdxJsxTextElement,
    Link,
    LinkRef,
    Strong,
    Code,
    Math,
    Heading(Option<Depth>),
    Table(Option<usize>, Option<usize>),
    Text,
    HorizontalRule,
    Definition,
    MdxFlowExpression,
    MdxTextExpression,
    MdxJsEsm,
    MdxJsxFlowElement,
}

impl TryFrom<&Token> for Selector {
    type Error = ParseError;

    fn try_from(token: &Token) -> Result<Self, Self::Error> {
        if let TokenKind::Selector(s) = &token.kind {
            match s.as_str() {
                // Heading selectors
                ".h" => Ok(Selector::Heading(None)),
                ".h1" => Ok(Selector::Heading(Some(1))),
                ".h2" => Ok(Selector::Heading(Some(2))),
                ".h3" => Ok(Selector::Heading(Some(3))),
                ".h4" => Ok(Selector::Heading(Some(4))),
                ".h5" => Ok(Selector::Heading(Some(5))),
                ".h6" => Ok(Selector::Heading(Some(6))),

                // Blockquote
                ".>" | ".blockquote" => Ok(Selector::Blockquote),

                // Footnote
                ".^" | ".footnote" => Ok(Selector::Footnote),

                // MDX JSX Flow Element
                ".<" | ".mdx_jsx_flow_element" => Ok(Selector::MdxJsxFlowElement),

                // Emphasis
                ".**" | ".emphasis" => Ok(Selector::Emphasis),

                // Math
                ".$$" | ".math" => Ok(Selector::Math),

                // Horizontal Rule
                ".horizontal_rule" | ".---" | ".***" | ".___" => Ok(Selector::HorizontalRule),

                // MDX Text Expression
                ".{}" | ".mdx_text_expression" => Ok(Selector::MdxTextExpression),

                // Footnote Reference
                ".[^]" | ".footnote_ref" => Ok(Selector::FootnoteRef),

                // Definition
                ".definition" => Ok(Selector::Definition),

                // Break
                ".break" => Ok(Selector::Break),

                // Delete
                ".delete" => Ok(Selector::Delete),

                // HTML
                ".<>" | ".html" => Ok(Selector::Html),

                // Image
                ".image" => Ok(Selector::Image),

                // Image Reference
                ".image_ref" => Ok(Selector::ImageRef),

                // Inline Code
                ".code_inline" => Ok(Selector::InlineCode),

                // Inline Math
                ".math_inline" => Ok(Selector::InlineMath),

                // Link
                ".link" => Ok(Selector::Link),

                // Link Reference
                ".link_ref" => Ok(Selector::LinkRef),

                // List
                ".list" => Ok(Selector::List(None, None)),

                // TOML
                ".toml" => Ok(Selector::Toml),

                // Strong
                ".strong" => Ok(Selector::Strong),

                // YAML
                ".yaml" => Ok(Selector::Yaml),

                // Code
                ".code" => Ok(Selector::Code),

                // MDX JS ESM
                ".mdx_js_esm" => Ok(Selector::MdxJsEsm),

                // MDX JSX Text Element
                ".mdx_jsx_text_element" => Ok(Selector::MdxJsxTextElement),

                // MDX Flow Expression
                ".mdx_flow_expression" => Ok(Selector::MdxFlowExpression),

                // Text
                ".text" => Ok(Selector::Text),

                // Table
                ".table" => Ok(Selector::Table(None, None)),

                _ => Err(ParseError::UnknownSelector(token.clone())),
            }
        } else {
            Err(ParseError::UnexpectedToken(token.clone()))
        }
    }
}

impl Display for Selector {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            Selector::Heading(None) => write!(f, ".h"),
            Selector::Heading(Some(1)) => write!(f, ".h1"),
            Selector::Heading(Some(2)) => write!(f, ".h2"),
            Selector::Heading(Some(3)) => write!(f, ".h3"),
            Selector::Heading(Some(4)) => write!(f, ".h4"),
            Selector::Heading(Some(5)) => write!(f, ".h5"),
            Selector::Heading(Some(6)) => write!(f, ".h6"),
            Selector::Heading(Some(n)) => write!(f, ".h{}", n),
            Selector::Blockquote => write!(f, ".blockquote"),
            Selector::Footnote => write!(f, ".footnote"),
            Selector::List(None, None) => write!(f, ".list"),
            Selector::List(Some(idx), None) => write!(f, ".list({})", idx),
            Selector::List(idx, ordered) => write!(f, ".list({:?}, {:?})", idx, ordered),
            Selector::Toml => write!(f, ".toml"),
            Selector::Yaml => write!(f, ".yaml"),
            Selector::Break => write!(f, ".break"),
            Selector::InlineCode => write!(f, ".code_inline"),
            Selector::InlineMath => write!(f, ".math_inline"),
            Selector::Delete => write!(f, ".delete"),
            Selector::Emphasis => write!(f, ".emphasis"),
            Selector::FootnoteRef => write!(f, ".footnote_ref"),
            Selector::Html => write!(f, ".html"),
            Selector::Image => write!(f, ".image"),
            Selector::ImageRef => write!(f, ".image_ref"),
            Selector::MdxJsxTextElement => write!(f, ".mdx_jsx_text_element"),
            Selector::Link => write!(f, ".link"),
            Selector::LinkRef => write!(f, ".link_ref"),
            Selector::Strong => write!(f, ".strong"),
            Selector::Code => write!(f, ".code"),
            Selector::Math => write!(f, ".math"),
            Selector::Table(None, None) => write!(f, ".table"),
            Selector::Table(Some(row), None) => write!(f, ".[{}]", row),
            Selector::Table(Some(row), Some(col)) => write!(f, ".[{}][{}]", row, col),
            Selector::Table(None, Some(col)) => write!(f, ".[][{}]", col),
            Selector::Text => write!(f, ".text"),
            Selector::HorizontalRule => write!(f, ".horizontal_rule"),
            Selector::Definition => write!(f, ".definition"),
            Selector::MdxFlowExpression => write!(f, ".mdx_flow_expression"),
            Selector::MdxTextExpression => write!(f, ".mdx_text_expression"),
            Selector::MdxJsEsm => write!(f, ".mdx_js_esm"),
            Selector::MdxJsxFlowElement => write!(f, ".mdx_jsx_flow_element"),
        }
    }
}

#[cfg_attr(feature = "ast-json", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, PartialOrd, PartialEq, Eq)]
pub enum StringSegment {
    Text(String),
    Ident(Ident),
    Env(SmolStr),
    Self_,
}

#[cfg_attr(feature = "ast-json", derive(Serialize, Deserialize))]
#[derive(PartialEq, PartialOrd, Debug, Clone)]
pub enum Pattern {
    Literal(Literal),
    Ident(IdentWithToken),
    Wildcard,
    Array(Vec<Pattern>),
    ArrayRest(Vec<Pattern>, IdentWithToken), // patterns before .., rest binding
    Dict(Vec<(IdentWithToken, Pattern)>),
    Type(Ident), // :string, :number, etc.
}

#[cfg_attr(feature = "ast-json", derive(Serialize, Deserialize))]
#[derive(PartialEq, PartialOrd, Debug, Clone)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub guard: Option<Shared<Node>>,
    pub body: Shared<Node>,
}

impl From<&lexer::token::StringSegment> for StringSegment {
    fn from(segment: &lexer::token::StringSegment) -> Self {
        match segment {
            lexer::token::StringSegment::Text(text, _) => StringSegment::Text(text.to_owned()),
            lexer::token::StringSegment::Ident(ident, _) if ident == "self" => StringSegment::Self_,
            lexer::token::StringSegment::Ident(ident, _) if ident.starts_with("$") => {
                StringSegment::Env(SmolStr::from(&ident[1..]))
            }
            lexer::token::StringSegment::Ident(ident, _) => StringSegment::Ident(Ident::new(ident)),
        }
    }
}

#[cfg_attr(feature = "ast-json", derive(Serialize, Deserialize))]
#[derive(PartialEq, PartialOrd, Debug, Clone)]
pub enum Literal {
    String(String),
    Number(Number),
    Symbol(Ident),
    Bool(bool),
    None,
}

impl From<&str> for Literal {
    fn from(s: &str) -> Self {
        Literal::String(s.to_owned())
    }
}

#[cfg_attr(feature = "ast-json", derive(Serialize, Deserialize))]
#[derive(PartialEq, PartialOrd, Debug, Clone)]
pub enum AccessTarget {
    Call(IdentWithToken, Args),
    Ident(IdentWithToken),
}

impl Display for Literal {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            Literal::String(s) => write!(f, "{}", s),
            Literal::Number(n) => write!(f, "{}", n),
            Literal::Symbol(i) => write!(f, "{}", i),
            Literal::Bool(b) => write!(f, "{}", b),
            Literal::None => write!(f, "none"),
        }
    }
}

#[cfg_attr(feature = "ast-json", derive(Serialize, Deserialize))]
#[derive(PartialEq, PartialOrd, Debug, Clone)]
pub enum Expr {
    Block(Program),
    Call(IdentWithToken, Args),
    CallDynamic(Shared<Node>, Args),
    Def(IdentWithToken, Params, Program),
    Fn(Params, Program),
    Let(IdentWithToken, Shared<Node>),
    Literal(Literal),
    Ident(IdentWithToken),
    InterpolatedString(Vec<StringSegment>),
    Selector(Selector),
    While(Shared<Node>, Program),
    Until(Shared<Node>, Program),
    Foreach(IdentWithToken, Shared<Node>, Program),
    If(Branches),
    Match(Shared<Node>, MatchArms),
    Include(Literal),
    Import(Literal),
    Module(IdentWithToken, Program),
    QualifiedAccess(Vec<IdentWithToken>, AccessTarget),
    Self_,
    Nodes,
    Paren(Shared<Node>),
    Try(Shared<Node>, Shared<Node>),
    Break,
    Continue,
}

#[cfg(feature = "debugger")]
impl Display for Expr {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Expr::Call(ident, args) => {
                write!(f, "{}(", ident)?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", arg.expr)?;
                }
                write!(f, ")")
            }
            Expr::CallDynamic(callable, args) => {
                write!(f, "{}(", callable.expr)?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", arg.expr)?;
                }
                write!(f, ")")
            }
            _ => write!(f, ""),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Position, TokenKind, arena::ArenaId};
    use rstest::rstest;
    use smallvec::smallvec;

    fn create_token(range: Range) -> Shared<Token> {
        Shared::new(Token {
            range,
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        })
    }

    #[rstest]
    #[case(
        Expr::CallDynamic(
            Shared::new(Node {
                token_id: ArenaId::new(1),
                expr: Shared::new(Expr::Literal(Literal::String("callee".to_string()))),
            }),
            smallvec![
                Shared::new(Node {
                    token_id: ArenaId::new(0),
                    expr: Shared::new(Expr::Literal(Literal::String("arg1".to_string()))),
                }),
                Shared::new(Node {
                    token_id: ArenaId::new(1),
                    expr: Shared::new(Expr::Literal(Literal::String("arg2".to_string()))),
                }),
            ]
        ),
        vec![
            (0, Range { start: Position::new(1, 1), end: Position::new(1, 5) }),
            (1, Range { start: Position::new(2, 1), end: Position::new(2, 5) }),
        ],
        Range { start: Position::new(2, 1), end: Position::new(2, 5) }
    )]
    #[case(
        Expr::Match(
            Shared::new(Node {
                token_id: ArenaId::new(0),
                expr: Shared::new(Expr::Literal(Literal::String("val".to_string()))),
            }),
            smallvec![
                MatchArm {
                    pattern: Pattern::Literal(Literal::String("a".to_string())),
                    guard: None,
                    body: Shared::new(Node {
                        token_id: ArenaId::new(1),
                        expr: Shared::new(Expr::Literal(Literal::String("body1".to_string()))),
                    }),
                },
                MatchArm {
                    pattern: Pattern::Literal(Literal::String("b".to_string())),
                    guard: None,
                    body: Shared::new(Node {
                        token_id: ArenaId::new(2),
                        expr: Shared::new(Expr::Literal(Literal::String("body2".to_string()))),
                    }),
                },
            ]
        ),
        vec![
            (0, Range { start: Position::new(10, 1), end: Position::new(10, 5) }),
            (1, Range { start: Position::new(11, 1), end: Position::new(11, 5) }),
            (2, Range { start: Position::new(12, 1), end: Position::new(12, 5) }),
        ],
        Range { start: Position::new(10, 1), end: Position::new(12, 5) }
    )]
    #[case(
        Expr::Try(
            Shared::new(Node {
                token_id: ArenaId::new(0),
                expr: Shared::new(Expr::Literal(Literal::String("try".to_string()))),
            }),
            Shared::new(Node {
                token_id: ArenaId::new(1),
                expr: Shared::new(Expr::Literal(Literal::String("catch".to_string()))),
            })
        ),
        vec![
            (0, Range { start: Position::new(20, 1), end: Position::new(20, 5) }),
            (1, Range { start: Position::new(21, 1), end: Position::new(21, 5) }),
        ],
        Range { start: Position::new(20, 1), end: Position::new(21, 5) }
    )]
    #[case(
        Expr::Let(
            IdentWithToken::new("x"),
            Shared::new(Node {
                token_id: ArenaId::new(0),
                expr: Shared::new(Expr::Literal(Literal::String("letval".to_string()))),
            })
        ),
        vec![
            (0, Range { start: Position::new(30, 1), end: Position::new(30, 5) }),
        ],
        Range { start: Position::new(30, 1), end: Position::new(30, 5) }
    )]
    #[case(
        Expr::Paren(
            Shared::new(Node {
                token_id: ArenaId::new(0),
                expr: Shared::new(Expr::Literal(Literal::String("paren".to_string()))),
            })
        ),
        vec![
            (0, Range { start: Position::new(40, 1), end: Position::new(40, 5) }),
        ],
        Range { start: Position::new(40, 1), end: Position::new(40, 5) }
    )]
    #[case(
        Expr::Block(vec![
            Shared::new(Node {
                token_id: ArenaId::new(0),
                expr: Shared::new(Expr::Literal(Literal::String("block1".to_string()))),
            }),
            Shared::new(Node {
                token_id: ArenaId::new(1),
                expr: Shared::new(Expr::Literal(Literal::String("block2".to_string()))),
            }),
        ]),
        vec![
            (0, Range { start: Position::new(50, 1), end: Position::new(50, 5) }),
            (1, Range { start: Position::new(51, 1), end: Position::new(51, 5) }),
        ],
        Range { start: Position::new(50, 1), end: Position::new(51, 5) }
    )]
    #[case(
        Expr::Def(
            IdentWithToken::new("f"),
            smallvec![],
            vec![
                Shared::new(Node {
                    token_id: ArenaId::new(0),
                    expr: Shared::new(Expr::Literal(Literal::String("def1".to_string()))),
                }),
                Shared::new(Node {
                    token_id: ArenaId::new(1),
                    expr: Shared::new(Expr::Literal(Literal::String("def2".to_string()))),
                }),
            ]
        ),
        vec![
            (0, Range { start: Position::new(60, 1), end: Position::new(60, 5) }),
            (1, Range { start: Position::new(61, 1), end: Position::new(61, 5) }),
        ],
        Range { start: Position::new(60, 1), end: Position::new(61, 5) }
    )]
    #[case(
        Expr::Fn(
            smallvec![],
            vec![
                Shared::new(Node {
                    token_id: ArenaId::new(0),
                    expr: Shared::new(Expr::Literal(Literal::String("fn1".to_string()))),
                }),
                Shared::new(Node {
                    token_id: ArenaId::new(1),
                    expr: Shared::new(Expr::Literal(Literal::String("fn2".to_string()))),
                }),
            ]
        ),
        vec![
            (0, Range { start: Position::new(70, 1), end: Position::new(70, 5) }),
            (1, Range { start: Position::new(71, 1), end: Position::new(71, 5) }),
        ],
        Range { start: Position::new(70, 1), end: Position::new(71, 5) }
    )]
    #[case(
        Expr::While(
            Shared::new(Node {
                token_id: ArenaId::new(0),
                expr: Shared::new(Expr::Literal(Literal::String("cond".to_string()))),
            }),
            vec![
                Shared::new(Node {
                    token_id: ArenaId::new(1),
                    expr: Shared::new(Expr::Literal(Literal::String("while1".to_string()))),
                }),
                Shared::new(Node {
                    token_id: ArenaId::new(2),
                    expr: Shared::new(Expr::Literal(Literal::String("while2".to_string()))),
                }),
            ]
        ),
        vec![
            (0, Range { start: Position::new(81, 1), end: Position::new(81, 5) }),
            (1, Range { start: Position::new(82, 1), end: Position::new(82, 5) }),
            (2, Range { start: Position::new(82, 1), end: Position::new(82, 5) }),
        ],
        Range { start: Position::new(82, 1), end: Position::new(82, 5) }
    )]
    #[case(
        Expr::Until(
            Shared::new(Node {
                token_id: ArenaId::new(0),
                expr: Shared::new(Expr::Literal(Literal::String("cond".to_string()))),
            }),
            vec![
                Shared::new(Node {
                    token_id: ArenaId::new(1),
                    expr: Shared::new(Expr::Literal(Literal::String("until1".to_string()))),
                }),
                Shared::new(Node {
                    token_id: ArenaId::new(2),
                    expr: Shared::new(Expr::Literal(Literal::String("until2".to_string()))),
                }),
            ]
        ),
        vec![
            (0, Range { start: Position::new(91, 1), end: Position::new(91, 5) }),
            (1, Range { start: Position::new(92, 1), end: Position::new(92, 5) }),
            (2, Range { start: Position::new(92, 1), end: Position::new(92, 5) }),
        ],
        Range { start: Position::new(92, 1), end: Position::new(92, 5) }
    )]
    #[case(
        Expr::Foreach(
            IdentWithToken::new("item"),
            Shared::new(Node {
                token_id: ArenaId::new(0),
                expr: Shared::new(Expr::Literal(Literal::String("iter".to_string()))),
            }),
            vec![
                Shared::new(Node {
                    token_id: ArenaId::new(1),
                    expr: Shared::new(Expr::Literal(Literal::String("foreach1".to_string()))),
                }),
                Shared::new(Node {
                    token_id: ArenaId::new(2),
                    expr: Shared::new(Expr::Literal(Literal::String("foreach2".to_string()))),
                }),
            ]
        ),
        vec![
            (0, Range { start: Position::new(101, 1), end: Position::new(101, 5) }),
            (1, Range { start: Position::new(102, 1), end: Position::new(102, 5) }),
            (2, Range { start: Position::new(102, 1), end: Position::new(102, 5) }),
        ],
        Range { start: Position::new(102, 1), end: Position::new(102, 5) }
    )]
    #[case(
        Expr::If(smallvec![
            (
                Some(Shared::new(Node {
                    token_id: ArenaId::new(0),
                    expr: Shared::new(Expr::Literal(Literal::String("cond1".to_string()))),
                })),
                Shared::new(Node {
                    token_id: ArenaId::new(1),
                    expr: Shared::new(Expr::Literal(Literal::String("if1".to_string()))),
                })
            ),
            (
                Some(Shared::new(Node {
                    token_id: ArenaId::new(2),
                    expr: Shared::new(Expr::Literal(Literal::String("cond2".to_string()))),
                })),
                Shared::new(Node {
                    token_id: ArenaId::new(3),
                    expr: Shared::new(Expr::Literal(Literal::String("if2".to_string()))),
                })
            ),
        ]),
        vec![
            (0, Range { start: Position::new(111, 1), end: Position::new(111, 5) }),
            (1, Range { start: Position::new(113, 1), end: Position::new(113, 5) }),
            (2, Range { start: Position::new(114, 1), end: Position::new(115, 5) }),
            (3, Range { start: Position::new(116, 1), end: Position::new(117, 5) }),
        ],
        Range { start: Position::new(113, 1), end: Position::new(117, 5) }
    )]
    #[case(
        Expr::Call(
            IdentWithToken::new("func"),
            smallvec![
                Shared::new(Node {
                    token_id: ArenaId::new(0),
                    expr: Shared::new(Expr::Literal(Literal::String("arg1".to_string()))),
                }),
                Shared::new(Node {
                    token_id: ArenaId::new(1),
                    expr: Shared::new(Expr::Literal(Literal::String("arg2".to_string()))),
                }),
            ]
        ),
        vec![
            (0, Range { start: Position::new(120, 1), end: Position::new(120, 5) }),
            (1, Range { start: Position::new(121, 1), end: Position::new(121, 5) }),
        ],
        Range { start: Position::new(120, 1), end: Position::new(121, 5) }
    )]
    fn test_node_range_various_exprs(
        #[case] expr: Expr,
        #[case] token_ranges: Vec<(usize, Range)>,
        #[case] expected: Range,
    ) {
        let mut arena = Arena::new(150);
        for (_, range) in &token_ranges {
            let token = create_token(range.clone());
            let _ = arena.alloc(token);
        }
        let node = Node {
            token_id: ArenaId::new(0),
            expr: Shared::new(expr),
        };
        assert_eq!(node.range(Shared::new(arena)), expected);
    }

    #[rstest]
    // Heading selectors
    #[case::heading(".h", Selector::Heading(None), ".h")]
    #[case::heading_h1(".h1", Selector::Heading(Some(1)), ".h1")]
    #[case::heading_h2(".h2", Selector::Heading(Some(2)), ".h2")]
    #[case::heading_h3(".h3", Selector::Heading(Some(3)), ".h3")]
    #[case::heading_h4(".h4", Selector::Heading(Some(4)), ".h4")]
    #[case::heading_h5(".h5", Selector::Heading(Some(5)), ".h5")]
    #[case::heading_h6(".h6", Selector::Heading(Some(6)), ".h6")]
    // Blockquote
    #[case::blockquote(".blockquote", Selector::Blockquote, ".blockquote")]
    #[case::blockquote_alias(".>", Selector::Blockquote, ".blockquote")]
    // Footnote
    #[case::footnote(".footnote", Selector::Footnote, ".footnote")]
    #[case::footnote_alias(".^", Selector::Footnote, ".footnote")]
    // MDX JSX Flow Element
    #[case::mdx_jsx_flow_element(".mdx_jsx_flow_element", Selector::MdxJsxFlowElement, ".mdx_jsx_flow_element")]
    #[case::mdx_jsx_flow_element_alias(".<", Selector::MdxJsxFlowElement, ".mdx_jsx_flow_element")]
    // Emphasis
    #[case::emphasis(".emphasis", Selector::Emphasis, ".emphasis")]
    #[case::emphasis_alias(".**", Selector::Emphasis, ".emphasis")]
    // Math
    #[case::math(".math", Selector::Math, ".math")]
    #[case::math_alias(".$$", Selector::Math, ".math")]
    // Horizontal Rule
    #[case::horizontal_rule(".horizontal_rule", Selector::HorizontalRule, ".horizontal_rule")]
    #[case::horizontal_rule_alias_dash(".---", Selector::HorizontalRule, ".horizontal_rule")]
    #[case::horizontal_rule_alias_star(".***", Selector::HorizontalRule, ".horizontal_rule")]
    #[case::horizontal_rule_alias_underscore(".___", Selector::HorizontalRule, ".horizontal_rule")]
    // MDX Text Expression
    #[case::mdx_text_expression(".mdx_text_expression", Selector::MdxTextExpression, ".mdx_text_expression")]
    #[case::mdx_text_expression_alias(".{}", Selector::MdxTextExpression, ".mdx_text_expression")]
    // Footnote Reference
    #[case::footnote_ref(".footnote_ref", Selector::FootnoteRef, ".footnote_ref")]
    #[case::footnote_ref_alias(".[^]", Selector::FootnoteRef, ".footnote_ref")]
    // Definition
    #[case::definition(".definition", Selector::Definition, ".definition")]
    // Break
    #[case::break_selector(".break", Selector::Break, ".break")]
    // Delete
    #[case::delete(".delete", Selector::Delete, ".delete")]
    // HTML
    #[case::html(".html", Selector::Html, ".html")]
    #[case::html_alias(".<>", Selector::Html, ".html")]
    // Image
    #[case::image(".image", Selector::Image, ".image")]
    // Image Reference
    #[case::image_ref(".image_ref", Selector::ImageRef, ".image_ref")]
    // Inline Code
    #[case::code_inline(".code_inline", Selector::InlineCode, ".code_inline")]
    // Inline Math
    #[case::math_inline(".math_inline", Selector::InlineMath, ".math_inline")]
    // Link
    #[case::link(".link", Selector::Link, ".link")]
    // Link Reference
    #[case::link_ref(".link_ref", Selector::LinkRef, ".link_ref")]
    // List
    #[case::list(".list", Selector::List(None, None), ".list")]
    // TOML
    #[case::toml(".toml", Selector::Toml, ".toml")]
    // Strong
    #[case::strong(".strong", Selector::Strong, ".strong")]
    // YAML
    #[case::yaml(".yaml", Selector::Yaml, ".yaml")]
    // Code
    #[case::code(".code", Selector::Code, ".code")]
    // MDX JS ESM
    #[case::mdx_js_esm(".mdx_js_esm", Selector::MdxJsEsm, ".mdx_js_esm")]
    // MDX JSX Text Element
    #[case::mdx_jsx_text_element(".mdx_jsx_text_element", Selector::MdxJsxTextElement, ".mdx_jsx_text_element")]
    // MDX Flow Expression
    #[case::mdx_flow_expression(".mdx_flow_expression", Selector::MdxFlowExpression, ".mdx_flow_expression")]
    // Text
    #[case::text(".text", Selector::Text, ".text")]
    // Table
    #[case::table(".table", Selector::Table(None, None), ".table")]
    fn test_selector_try_from_and_display(
        #[case] input: &str,
        #[case] expected_selector: Selector,
        #[case] expected_display: &str,
    ) {
        // Test TryFrom
        let selector = Selector::try_from(&Token {
            kind: TokenKind::Selector(SmolStr::new(input)),
            range: Range {
                start: Position::new(0, 0),
                end: Position::new(0, 0),
            },
            module_id: ArenaId::new(0),
        })
        .expect("Should parse valid selector");
        assert_eq!(selector, expected_selector);

        // Test Display
        assert_eq!(selector.to_string(), expected_display);
    }

    #[test]
    fn test_selector_try_from_unknown() {
        let token = Token {
            kind: TokenKind::Selector(SmolStr::new(".unknown")),
            range: Range {
                start: Position::new(0, 0),
                end: Position::new(0, 0),
            },
            module_id: ArenaId::new(0),
        };
        let result = Selector::try_from(&token);
        assert!(result.is_err());
        if let Err(e) = result {
            assert_eq!(e, ParseError::UnknownSelector(token));
        }
    }
}
