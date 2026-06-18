use super::{Program, TokenId};
#[cfg(feature = "ast-json")]
use crate::arena::ArenaId;
use crate::{Ident, Shared, Token, arena::Arena, number::Number, range::Range, selector::Selector};
#[cfg(feature = "ast-json")]
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use smol_str::SmolStr;
use std::{
    fmt::{self, Display, Formatter},
    hash::{Hash, Hasher},
};

/// Represents a function parameter with an optional default value
#[cfg_attr(feature = "ast-json", derive(Serialize, Deserialize))]
#[derive(PartialEq, PartialOrd, Debug, Clone)]
pub struct Param {
    pub ident: IdentWithToken,
    pub default: Option<Shared<Node>>,
    pub is_variadic: bool,
}

impl Display for Param {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        if self.is_variadic {
            write!(f, "*{}", self.ident)
        } else {
            write!(f, "{}", self.ident)
        }
    }
}

impl Param {
    pub fn new(name: IdentWithToken) -> Self {
        Self::with_default(name, None)
    }

    pub fn with_default(name: IdentWithToken, default_value: Option<Shared<Node>>) -> Self {
        Self {
            ident: name,
            default: default_value,
            is_variadic: false,
        }
    }

    /// Creates a variadic parameter (e.g., `*args`)
    pub fn variadic(name: IdentWithToken) -> Self {
        Self {
            ident: name,
            default: None,
            is_variadic: true,
        }
    }
}

pub type Params = SmallVec<[Param; 4]>;
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

    pub fn range(&self, arena: Shared<Arena<Token>>) -> Range {
        match &*self.expr {
            Expr::Block(program)
            | Expr::Def(_, _, program)
            | Expr::Fn(_, program)
            | Expr::While(_, program)
            | Expr::Loop(program)
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
            Expr::Macro(_, params, block) => {
                let start = params
                    .first()
                    .and_then(|param| param.ident.token.as_ref().map(|t| t.range))
                    .unwrap_or(block.range(Shared::clone(&arena)))
                    .start;
                let end = block.range(arena).end;
                Range { start, end }
            }
            Expr::As(_, node)
            | Expr::Let(_, node)
            | Expr::Var(_, node)
            | Expr::Assign(_, node)
            | Expr::Quote(node)
            | Expr::Unquote(node) => node.range(Shared::clone(&arena)),
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
                    arena[self.token_id].range
                }
            }
            Expr::Match(value, arms) => {
                let start = value.range(Shared::clone(&arena)).start;
                let end = arms
                    .last()
                    .map(|arm| arm.body.range(Shared::clone(&arena)).end)
                    .unwrap_or_else(|| arena[self.token_id].range.end);
                Range { start, end }
            }
            Expr::Paren(node) => node.range(Shared::clone(&arena)),
            Expr::Try(try_expr, catch_expr) => {
                let start = try_expr.range(Shared::clone(&arena)).start;
                let end = catch_expr.range(Shared::clone(&arena)).end;
                Range { start, end }
            }
            Expr::And(exprs) | Expr::Or(exprs) => {
                if let (Some(first), Some(last)) = (exprs.first(), exprs.last()) {
                    Range {
                        start: first.range(Shared::clone(&arena)).start,
                        end: last.range(Shared::clone(&arena)).end,
                    }
                } else {
                    arena[self.token_id].range
                }
            }
            Expr::Break(Some(value_node)) => {
                let start = arena[self.token_id].range.start;
                let end = value_node.range(Shared::clone(&arena)).end;
                Range { start, end }
            }
            Expr::SelectorCall(_, args) => {
                let start = arena[self.token_id].range.start;
                let end = args
                    .last()
                    .map(|node| node.range(Shared::clone(&arena)).end)
                    .unwrap_or_else(|| arena[self.token_id].range.end);
                Range { start, end }
            }
            Expr::Literal(_)
            | Expr::Ident(_)
            | Expr::Selector(_)
            | Expr::SelectorChain(_)
            | Expr::Include(_)
            | Expr::Import(_)
            | Expr::InterpolatedString(_)
            | Expr::QualifiedAccess(_, _)
            | Expr::Nodes
            | Expr::Self_
            | Expr::Break(None)
            | Expr::Continue => arena[self.token_id].range,
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
    pub token: Option<Token>,
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

    pub fn new_with_token(name: &str, token: Option<Token>) -> Self {
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
#[derive(Debug, Clone, PartialOrd, PartialEq)]
pub enum StringSegment {
    Text(String),
    Expr(Shared<Node>),
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
    Type(Ident),      // :string, :number, etc.
    Or(Vec<Pattern>), // p1 || p2 || p3
}

#[cfg_attr(feature = "ast-json", derive(Serialize, Deserialize))]
#[derive(PartialEq, PartialOrd, Debug, Clone)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub guard: Option<Shared<Node>>,
    pub body: Shared<Node>,
}

#[cfg_attr(feature = "ast-json", derive(Serialize, Deserialize))]
#[derive(PartialEq, PartialOrd, Debug, Clone)]
pub enum Literal {
    String(String),
    Bytes(Vec<u8>),
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
            Literal::Bytes(b) => {
                write!(f, "b\"")?;
                for byte in b {
                    if byte.is_ascii_graphic() && *byte != b'"' && *byte != b'\\' {
                        write!(f, "{}", *byte as char)?;
                    } else {
                        write!(f, "\\x{:02x}", byte)?;
                    }
                }
                write!(f, "\"")
            }
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
    As(IdentWithToken, Shared<Node>),
    Block(Program),
    Call(IdentWithToken, Args),
    CallDynamic(Shared<Node>, Args),
    Def(IdentWithToken, Params, Program),
    Macro(IdentWithToken, Params, Shared<Node>),
    Fn(Params, Program),
    Let(Pattern, Shared<Node>),
    Loop(Program),
    Var(Pattern, Shared<Node>),
    Assign(IdentWithToken, Shared<Node>),
    And(Vec<Shared<Node>>),
    Or(Vec<Shared<Node>>),
    Literal(Literal),
    Ident(IdentWithToken),
    InterpolatedString(Vec<StringSegment>),
    Selector(Selector),
    /// A sequence of selectors merged by the optimizer to reduce pipeline overhead.
    ///
    /// Equivalent to applying each selector in order, but evaluated in a single
    /// `eval_expr` call instead of N separate pipeline steps.
    SelectorChain(SmallVec<[Selector; 4]>),
    /// A selector with runtime-evaluated arguments for filtered matching.
    ///
    /// Supports `.h(1..2)`, `.h(1, 2)`, `.code("rust")`, etc.
    SelectorCall(Selector, Args),
    While(Shared<Node>, Program),
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
    Quote(Shared<Node>),
    Unquote(Shared<Node>),
    Try(Shared<Node>, Shared<Node>),
    Break(Option<Shared<Node>>),
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

    fn create_token(range: Range) -> Token {
        Token {
            range,
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        }
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
            Pattern::Ident(IdentWithToken::new("x")),
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
            let token = create_token(*range);
            let _ = arena.alloc(token);
        }
        let node = Node {
            token_id: ArenaId::new(0),
            expr: Shared::new(expr),
        };
        assert_eq!(node.range(Shared::new(arena)), expected);
    }

    fn make_node(token_id: u32) -> Shared<Node> {
        Shared::new(Node {
            token_id: ArenaId::new(token_id),
            expr: Shared::new(Expr::Literal(Literal::None)),
        })
    }

    fn single_token_arena(range: Range) -> Shared<Arena<Token>> {
        let mut arena = Arena::new(10);
        arena.alloc(create_token(range));
        Shared::new(arena)
    }

    #[test]
    fn test_range_loop_uses_program() {
        let r0 = Range {
            start: Position::new(1, 1),
            end: Position::new(1, 5),
        };
        let r1 = Range {
            start: Position::new(2, 1),
            end: Position::new(2, 5),
        };
        let mut arena = Arena::new(10);
        arena.alloc(create_token(r0));
        arena.alloc(create_token(r1));
        let expr = Expr::Loop(vec![make_node(0), make_node(1)]);
        let node = Node {
            token_id: ArenaId::new(0),
            expr: Shared::new(expr),
        };
        let got = node.range(Shared::new(arena));
        assert_eq!(got.start, r0.start);
        assert_eq!(got.end, r1.end);
    }

    #[test]
    fn test_range_module_uses_program() {
        let r0 = Range {
            start: Position::new(10, 1),
            end: Position::new(10, 5),
        };
        let r1 = Range {
            start: Position::new(11, 1),
            end: Position::new(11, 5),
        };
        let mut arena = Arena::new(10);
        arena.alloc(create_token(r0));
        arena.alloc(create_token(r1));
        let expr = Expr::Module(IdentWithToken::new("m"), vec![make_node(0), make_node(1)]);
        let node = Node {
            token_id: ArenaId::new(0),
            expr: Shared::new(expr),
        };
        let got = node.range(Shared::new(arena));
        assert_eq!(got.start, r0.start);
        assert_eq!(got.end, r1.end);
    }

    #[test]
    fn test_range_empty_block_uses_default() {
        let r0 = Range {
            start: Position::new(5, 1),
            end: Position::new(5, 5),
        };
        let arena = single_token_arena(r0);
        let expr = Expr::Block(vec![]);
        let node = Node {
            token_id: ArenaId::new(0),
            expr: Shared::new(expr),
        };
        let got = node.range(arena);
        assert_eq!(got, Range::default());
    }

    #[test]
    fn test_range_as_delegates_to_inner() {
        let r0 = Range {
            start: Position::new(20, 1),
            end: Position::new(20, 8),
        };
        let arena = single_token_arena(r0);
        let expr = Expr::As(IdentWithToken::new("x"), make_node(0));
        let node = Node {
            token_id: ArenaId::new(0),
            expr: Shared::new(expr),
        };
        assert_eq!(node.range(arena), r0);
    }

    #[test]
    fn test_range_var_delegates_to_inner() {
        let r0 = Range {
            start: Position::new(21, 1),
            end: Position::new(21, 8),
        };
        let arena = single_token_arena(r0);
        let expr = Expr::Var(Pattern::Wildcard, make_node(0));
        let node = Node {
            token_id: ArenaId::new(0),
            expr: Shared::new(expr),
        };
        assert_eq!(node.range(arena), r0);
    }

    #[test]
    fn test_range_assign_delegates_to_inner() {
        let r0 = Range {
            start: Position::new(22, 1),
            end: Position::new(22, 8),
        };
        let arena = single_token_arena(r0);
        let expr = Expr::Assign(IdentWithToken::new("v"), make_node(0));
        let node = Node {
            token_id: ArenaId::new(0),
            expr: Shared::new(expr),
        };
        assert_eq!(node.range(arena), r0);
    }

    #[test]
    fn test_range_quote_delegates_to_inner() {
        let r0 = Range {
            start: Position::new(23, 1),
            end: Position::new(23, 8),
        };
        let arena = single_token_arena(r0);
        let expr = Expr::Quote(make_node(0));
        let node = Node {
            token_id: ArenaId::new(0),
            expr: Shared::new(expr),
        };
        assert_eq!(node.range(arena), r0);
    }

    #[test]
    fn test_range_unquote_delegates_to_inner() {
        let r0 = Range {
            start: Position::new(24, 1),
            end: Position::new(24, 8),
        };
        let arena = single_token_arena(r0);
        let expr = Expr::Unquote(make_node(0));
        let node = Node {
            token_id: ArenaId::new(0),
            expr: Shared::new(expr),
        };
        assert_eq!(node.range(arena), r0);
    }

    #[test]
    fn test_range_and_empty_falls_back_to_token() {
        let r0 = Range {
            start: Position::new(30, 1),
            end: Position::new(30, 5),
        };
        let arena = single_token_arena(r0);
        let expr = Expr::And(vec![]);
        let node = Node {
            token_id: ArenaId::new(0),
            expr: Shared::new(expr),
        };
        assert_eq!(node.range(arena), r0);
    }

    #[test]
    fn test_range_or_empty_falls_back_to_token() {
        let r0 = Range {
            start: Position::new(31, 1),
            end: Position::new(31, 5),
        };
        let arena = single_token_arena(r0);
        let expr = Expr::Or(vec![]);
        let node = Node {
            token_id: ArenaId::new(0),
            expr: Shared::new(expr),
        };
        assert_eq!(node.range(arena), r0);
    }

    #[test]
    fn test_range_and_non_empty() {
        let r0 = Range {
            start: Position::new(40, 1),
            end: Position::new(40, 5),
        };
        let r1 = Range {
            start: Position::new(41, 1),
            end: Position::new(41, 5),
        };
        let mut arena = Arena::new(10);
        arena.alloc(create_token(r0));
        arena.alloc(create_token(r1));
        let expr = Expr::And(vec![make_node(0), make_node(1)]);
        let node = Node {
            token_id: ArenaId::new(0),
            expr: Shared::new(expr),
        };
        let got = node.range(Shared::new(arena));
        assert_eq!(got.start, r0.start);
        assert_eq!(got.end, r1.end);
    }

    #[test]
    fn test_range_break_with_value() {
        let r0 = Range {
            start: Position::new(50, 1),
            end: Position::new(50, 3),
        };
        let r1 = Range {
            start: Position::new(50, 5),
            end: Position::new(50, 8),
        };
        let mut arena = Arena::new(10);
        arena.alloc(create_token(r0));
        arena.alloc(create_token(r1));
        let expr = Expr::Break(Some(make_node(1)));
        let node = Node {
            token_id: ArenaId::new(0),
            expr: Shared::new(expr),
        };
        let got = node.range(Shared::new(arena));
        assert_eq!(got.start, r0.start);
        assert_eq!(got.end, r1.end);
    }

    #[test]
    fn test_range_selector_call_with_args() {
        use crate::selector::Selector;
        let r0 = Range {
            start: Position::new(60, 1),
            end: Position::new(60, 3),
        };
        let r1 = Range {
            start: Position::new(60, 4),
            end: Position::new(60, 6),
        };
        let mut arena = Arena::new(10);
        arena.alloc(create_token(r0));
        arena.alloc(create_token(r1));
        let expr = Expr::SelectorCall(Selector::Heading(None), smallvec![make_node(1)]);
        let node = Node {
            token_id: ArenaId::new(0),
            expr: Shared::new(expr),
        };
        let got = node.range(Shared::new(arena));
        assert_eq!(got.start, r0.start);
        assert_eq!(got.end, r1.end);
    }

    #[test]
    fn test_range_terminal_exprs_use_token() {
        let r0 = Range {
            start: Position::new(70, 1),
            end: Position::new(70, 5),
        };

        for expr in [
            Expr::Literal(Literal::None),
            Expr::Nodes,
            Expr::Self_,
            Expr::Break(None),
            Expr::Continue,
        ] {
            let arena = single_token_arena(r0);
            let node = Node {
                token_id: ArenaId::new(0),
                expr: Shared::new(expr),
            };
            assert_eq!(node.range(arena), r0, "terminal expr should use token range");
        }
    }

    #[test]
    fn test_range_macro_with_params() {
        let r0 = Range {
            start: Position::new(80, 1),
            end: Position::new(80, 5),
        };
        let r1 = Range {
            start: Position::new(81, 1),
            end: Position::new(81, 5),
        };
        let mut arena = Arena::new(10);
        arena.alloc(create_token(r0));
        arena.alloc(create_token(r1));
        let param = Param::new(IdentWithToken::new_with_token("p", Some(create_token(r0))));
        let body = make_node(1);
        let expr = Expr::Macro(IdentWithToken::new("m"), smallvec![param], body);
        let node = Node {
            token_id: ArenaId::new(0),
            expr: Shared::new(expr),
        };
        let got = node.range(Shared::new(arena));
        assert_eq!(got.start, r0.start);
        assert_eq!(got.end, r1.end);
    }

    #[test]
    fn test_range_macro_no_params_uses_body() {
        let r0 = Range {
            start: Position::new(90, 1),
            end: Position::new(90, 5),
        };
        let arena = single_token_arena(r0);
        let body = make_node(0);
        let expr = Expr::Macro(IdentWithToken::new("m"), smallvec![], body);
        let node = Node {
            token_id: ArenaId::new(0),
            expr: Shared::new(expr),
        };
        assert_eq!(node.range(arena), r0);
    }

    #[test]
    fn test_range_call_empty_args_is_default() {
        let r0 = Range {
            start: Position::new(1, 1),
            end: Position::new(1, 5),
        };
        let arena = single_token_arena(r0);
        let expr = Expr::Call(IdentWithToken::new("f"), smallvec![]);
        let node = Node {
            token_id: ArenaId::new(0),
            expr: Shared::new(expr),
        };
        assert_eq!(node.range(arena), Range::default());
    }

    #[test]
    fn test_is_nodes() {
        let r = Range::default();
        let arena = single_token_arena(r);
        let nodes_node = Node {
            token_id: ArenaId::new(0),
            expr: Shared::new(Expr::Nodes),
        };
        assert!(nodes_node.is_nodes());
        let other = Node {
            token_id: ArenaId::new(0),
            expr: Shared::new(Expr::Self_),
        };
        assert!(!other.is_nodes());
        let _ = arena;
    }

    #[test]
    fn test_param_new_and_display() {
        let p = Param::new(IdentWithToken::new("x"));
        assert_eq!(p.to_string(), "x");
        assert!(!p.is_variadic);
        assert!(p.default.is_none());
    }

    #[test]
    fn test_param_variadic_display() {
        let p = Param::variadic(IdentWithToken::new("args"));
        assert!(p.is_variadic);
        assert_eq!(p.to_string(), "*args");
    }

    #[test]
    fn test_param_with_default() {
        let default_node = make_node(0);
        let p = Param::with_default(IdentWithToken::new("x"), Some(default_node));
        assert!(p.default.is_some());
    }

    #[rstest]
    #[case(Literal::String("hello".to_string()), "hello")]
    #[case(Literal::Number(crate::number::Number::from(42.0)), "42")]
    #[case(Literal::Bool(true), "true")]
    #[case(Literal::Bool(false), "false")]
    #[case(Literal::None, "none")]
    fn test_literal_display(#[case] lit: Literal, #[case] expected: &str) {
        assert_eq!(lit.to_string(), expected);
    }

    #[test]
    fn test_literal_bytes_display_ascii() {
        let lit = Literal::Bytes(b"hello".to_vec());
        assert_eq!(lit.to_string(), "b\"hello\"");
    }

    #[test]
    fn test_literal_bytes_display_non_ascii() {
        let lit = Literal::Bytes(vec![0x00, 0xff]);
        assert_eq!(lit.to_string(), "b\"\\x00\\xff\"");
    }

    #[test]
    fn test_ident_with_token_display() {
        let ident = IdentWithToken::new("foo");
        assert_eq!(ident.to_string(), "foo");
    }

    #[test]
    fn test_ident_with_token_ord() {
        let a = IdentWithToken::new("a");
        let b = IdentWithToken::new("b");
        assert!(a < b);
    }

    #[cfg(feature = "debugger")]
    #[test]
    fn test_expr_display_call() {
        let call = Expr::Call(IdentWithToken::new("func"), smallvec![make_node(0), make_node(1)]);
        let s = format!("{call}");
        assert!(s.starts_with("func("), "expected func(...), got: {s}");
    }

    #[cfg(feature = "debugger")]
    #[test]
    fn test_expr_display_call_dynamic() {
        let callee = Shared::new(Node {
            token_id: ArenaId::new(0),
            expr: Shared::new(Expr::Literal(Literal::None)),
        });
        let dynamic = Expr::CallDynamic(callee, smallvec![]);
        let s = format!("{dynamic}");
        assert!(s.contains('('), "expected parens in: {s}");
    }

    #[cfg(feature = "debugger")]
    #[test]
    fn test_expr_display_other_is_empty() {
        let lit = Expr::Literal(Literal::None);
        assert_eq!(format!("{lit}"), "");
    }
}
