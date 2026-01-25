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
}

impl Display for Param {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "{}", self.ident)
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

    pub fn range(&self, arena: Shared<Arena<Shared<Token>>>) -> Range {
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
            Expr::Let(_, node)
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
            Expr::And(expr1, expr2) | Expr::Or(expr1, expr2) => {
                let start = expr1.range(Shared::clone(&arena)).start;
                let end = expr2.range(Shared::clone(&arena)).end;
                Range { start, end }
            }
            Expr::Break(Some(value_node)) => {
                let start = arena[self.token_id].range.start;
                let end = value_node.range(Shared::clone(&arena)).end;
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
    Type(Ident), // :string, :number, etc.
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
    Macro(IdentWithToken, Params, Shared<Node>),
    Fn(Params, Program),
    Let(IdentWithToken, Shared<Node>),
    Loop(Program),
    Var(IdentWithToken, Shared<Node>),
    Assign(IdentWithToken, Shared<Node>),
    And(Shared<Node>, Shared<Node>),
    Or(Shared<Node>, Shared<Node>),
    Literal(Literal),
    Ident(IdentWithToken),
    InterpolatedString(Vec<StringSegment>),
    Selector(Selector),
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
}
