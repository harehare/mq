use super::{IdentName, Program, TokenId};
#[cfg(feature = "ast-json")]
use crate::arena::ArenaId;
use crate::{Token, arena::Arena, lexer, number::Number, range::Range};
use compact_str::CompactString;
#[cfg(feature = "ast-json")]
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use std::{
    fmt::{self, Display, Formatter},
    hash::{Hash, Hasher},
    rc::Rc,
};

type Depth = u8;
type Index = usize;
type Lang = CompactString;
pub type Params = SmallVec<[Rc<Node>; 4]>;
pub type Args = SmallVec<[Rc<Node>; 4]>;
pub type Cond = (Option<Rc<Node>>, Rc<Node>);
pub type Branches = SmallVec<[Cond; 4]>;

#[derive(PartialEq, PartialOrd, Debug, Clone)]
#[cfg_attr(feature = "ast-json", derive(Serialize, Deserialize))]
pub struct Node {
    #[cfg_attr(
        feature = "ast-json",
        serde(skip_serializing, skip_deserializing, default = "default_token_id")
    )]
    pub token_id: TokenId,
    pub expr: Rc<Expr>,
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

    pub fn range(&self, arena: Rc<Arena<Rc<Token>>>) -> Range {
        match &*self.expr {
            Expr::Def(_, _, program)
            | Expr::Fn(_, program)
            | Expr::While(_, program)
            | Expr::Until(_, program)
            | Expr::Foreach(_, _, program) => {
                let start = program
                    .first()
                    .map(|node| node.range(Rc::clone(&arena)).start)
                    .unwrap_or_default();
                let end = program
                    .last()
                    .map(|node| node.range(Rc::clone(&arena)).end)
                    .unwrap_or_default();
                Range { start, end }
            }
            Expr::Call(_, args) => {
                let start = args
                    .first()
                    .map(|node| node.range(Rc::clone(&arena)).start)
                    .unwrap_or_default();
                let end = args
                    .last()
                    .map(|node| node.range(Rc::clone(&arena)).end)
                    .unwrap_or_default();
                Range { start, end }
            }
            Expr::Let(_, node) => node.range(Rc::clone(&arena)),
            Expr::If(nodes) => {
                if let (Some(first), Some(last)) = (nodes.first(), nodes.last()) {
                    let start = first.1.range(Rc::clone(&arena));
                    let end = last.1.range(Rc::clone(&arena));
                    Range {
                        start: start.start,
                        end: end.end,
                    }
                } else {
                    // Fallback to token range if no branches exist
                    arena[self.token_id].range.clone()
                }
            }
            Expr::Paren(node) => node.range(Rc::clone(&arena)),
            Expr::Literal(_)
            | Expr::Ident(_)
            | Expr::Selector(_)
            | Expr::Include(_)
            | Expr::InterpolatedString(_)
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
pub struct Ident {
    pub name: IdentName,
    #[cfg_attr(
        feature = "ast-json",
        serde(skip_serializing_if = "Option::is_none", default)
    )]
    pub token: Option<Rc<Token>>,
}

impl Hash for Ident {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name.hash(state);
    }
}

impl Ord for Ident {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.name.cmp(&other.name)
    }
}

impl PartialOrd for Ident {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ident {
    pub fn new(name: &str) -> Self {
        Self::new_with_token(name, None)
    }

    pub fn new_with_token(name: &str, token: Option<Rc<Token>>) -> Self {
        Self {
            name: CompactString::from(name),
            token,
        }
    }
}

impl Display for Ident {
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
    Code(Option<Lang>),
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

#[cfg_attr(feature = "ast-json", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, PartialOrd, PartialEq, Eq)]
pub enum StringSegment {
    Text(String),
    Ident(Ident),
    Env(CompactString),
    Self_,
}

impl From<&lexer::token::StringSegment> for StringSegment {
    fn from(segment: &lexer::token::StringSegment) -> Self {
        match segment {
            lexer::token::StringSegment::Text(text, _) => StringSegment::Text(text.to_owned()),
            lexer::token::StringSegment::Ident(ident, _) if ident == "self" => StringSegment::Self_,
            lexer::token::StringSegment::Ident(ident, _) if ident.starts_with("$") => {
                StringSegment::Env(CompactString::from(&ident[1..]))
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
    Bool(bool),
    None,
}

impl Display for Literal {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            Literal::String(s) => write!(f, "{}", s),
            Literal::Number(n) => write!(f, "{}", n),
            Literal::Bool(b) => write!(f, "{}", b),
            Literal::None => write!(f, "None"),
        }
    }
}

#[cfg_attr(feature = "ast-json", derive(Serialize, Deserialize))]
#[derive(PartialEq, PartialOrd, Debug, Clone)]
pub enum Expr {
    Call(Ident, Args),
    Def(Ident, Params, Program),
    Fn(Params, Program),
    Let(Ident, Rc<Node>),
    Literal(Literal),
    Ident(Ident),
    InterpolatedString(Vec<StringSegment>),
    Selector(Selector),
    While(Rc<Node>, Program),
    Until(Rc<Node>, Program),
    Foreach(Ident, Rc<Node>, Program),
    If(Branches),
    Include(Literal),
    Self_,
    Nodes,
    Paren(Rc<Node>),
    Break,
    Continue,
}

#[cfg(feature = "ast-json")]
impl Display for Node {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match &*self.expr {
            Expr::Call(ident, args) => {
                write!(f, "{ident}(",)?;
                let mut first = true;
                for arg in args {
                    if !first {
                        f.write_str(", ")?;
                    }
                    first = false;
                    write!(f, "{arg}")?;
                }
                f.write_str(")")
            }
            Expr::Def(ident, params, program) => {
                write!(f, "def {ident}(")?;
                let mut first = true;
                for param in params {
                    if !first {
                        f.write_str("| ")?;
                    }
                    first = false;
                    write!(f, "{param}")?;
                }
                f.write_str(") {{ ")?;
                let mut first = true;
                for stmt in program {
                    if !first {
                        f.write_str("| ")?;
                    }
                    first = false;
                    write!(f, "{stmt}")?;
                }
                f.write_str(" }}")
            }
            Expr::Fn(params, program) => {
                f.write_str("fn(")?;
                let mut first = true;
                for param in params {
                    if !first {
                        f.write_str(", ")?;
                    }
                    first = false;
                    write!(f, "{param}")?;
                }
                f.write_str("): ")?;
                let mut first = true;
                for stmt in program {
                    if !first {
                        f.write_str(" | ")?;
                    }
                    first = false;
                    write!(f, "{stmt}")?;
                }
                f.write_str(";")
            }
            Expr::Let(ident, node) => write!(f, "let {ident} = {node}"),
            Expr::Literal(lit) => match lit {
                Literal::String(s) => write!(f, "\"{s}\""),
                _ => write!(f, "{lit}"),
            },
            Expr::Ident(ident) => write!(f, "{ident}"),
            Expr::InterpolatedString(segments) => {
                f.write_str("\"")?;
                for seg in segments {
                    match seg {
                        StringSegment::Text(s) => f.write_str(s)?,
                        StringSegment::Ident(ident) => write!(f, "${{{ident}}}")?,
                        StringSegment::Env(env) => write!(f, "${{{env}}}")?,
                        StringSegment::Self_ => f.write_str("${self}")?,
                    }
                }
                f.write_str("\"")
            }
            Expr::Selector(sel) => write!(f, ":{:?}", sel),
            Expr::While(cond, program) => {
                write!(f, "while ({cond}): ")?;
                let mut first = true;
                for stmt in program {
                    if !first {
                        f.write_str(" | ")?;
                    }
                    first = false;
                    write!(f, "{stmt}")?;
                }
                f.write_str(";")
            }
            Expr::Until(cond, program) => {
                write!(f, "until ({cond}): ")?;
                let mut first = true;
                for stmt in program {
                    if !first {
                        f.write_str(" | ")?;
                    }
                    first = false;
                    write!(f, "{stmt}")?;
                }
                f.write_str(";")
            }
            Expr::Foreach(ident, iterable, program) => {
                write!(f, "foreach ({ident}, {iterable}): ")?;
                let mut first = true;
                for stmt in program {
                    if !first {
                        f.write_str(" | ")?;
                    }
                    first = false;
                    write!(f, "{stmt}")?;
                }
                f.write_str(";")
            }
            Expr::If(branches) => {
                for (i, (cond, node)) in branches.iter().enumerate() {
                    if i == 0 {
                        if let Some(cond) = cond {
                            write!(f, "if ({}): ", cond)?;
                        } else {
                            f.write_str("if ")?;
                        }
                    } else if cond.is_some() {
                        write!(f, " elif ({}): ", cond.as_ref().unwrap())?;
                    } else {
                        f.write_str(" else: ")?;
                    }
                    write!(f, "{node}")?;
                }
                Ok(())
            }
            Expr::Include(lit) => write!(f, r#"include "{lit}""#),
            Expr::Self_ => f.write_str("self"),
            Expr::Nodes => f.write_str("nodes"),
            Expr::Paren(node) => write!(f, "({node})"),
            Expr::Break => f.write_str("break"),
            Expr::Continue => f.write_str("continue"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Position, TokenKind, arena::ArenaId};
    use rstest::rstest;
    use smallvec::{SmallVec, smallvec};

    #[cfg(feature = "ast-json")]
    use std::fmt::Write;

    fn create_token(range: Range) -> Rc<Token> {
        Rc::new(Token {
            range,
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        })
    }

    #[test]
    fn test_node_range_literal() {
        let mut arena = Arena::new(10);
        let range = Range {
            start: Position::new(1, 1),
            end: Position::new(2, 2),
        };
        let token = create_token(range.clone());
        let token_id = arena.alloc(Rc::clone(&token));

        let node = Node {
            token_id,
            expr: Rc::new(Expr::Literal(Literal::String("test".to_string()))),
        };

        assert_eq!(node.range(Rc::new(arena)), range);
    }

    #[test]
    fn test_node_range_def_with_program() {
        let mut arena = Arena::new(10);

        let stmt1_range = Range {
            start: Position::new(1, 1),
            end: Position::new(1, 10),
        };
        let stmt1_token_id = arena.alloc(create_token(stmt1_range.clone()));

        let stmt2_range = Range {
            start: Position::new(2, 1),
            end: Position::new(2, 15),
        };
        let stmt2_token_id = arena.alloc(create_token(stmt2_range.clone()));

        let stmt1 = Rc::new(Node {
            token_id: stmt1_token_id,
            expr: Rc::new(Expr::Literal(Literal::String("statement1".to_string()))),
        });

        let stmt2 = Rc::new(Node {
            token_id: stmt2_token_id,
            expr: Rc::new(Expr::Literal(Literal::String("statement2".to_string()))),
        });

        let def_token_id = arena.alloc(create_token(Range::default()));
        let def_node = Node {
            token_id: def_token_id,
            expr: Rc::new(Expr::Def(
                Ident::new("test_func"),
                SmallVec::new(),
                vec![stmt1, stmt2],
            )),
        };

        assert_eq!(
            def_node.range(Rc::new(arena)),
            Range {
                start: Position::new(1, 1),
                end: Position::new(2, 15)
            }
        );
    }

    #[test]
    fn test_node_range_while_loop() {
        let mut arena = Arena::new(10);

        let stmt1_range = Range {
            start: Position::new(3, 2),
            end: Position::new(3, 8),
        };
        let stmt1_token_id = arena.alloc(create_token(stmt1_range.clone()));

        let stmt2_range = Range {
            start: Position::new(4, 2),
            end: Position::new(4, 12),
        };
        let stmt2_token_id = arena.alloc(create_token(stmt2_range.clone()));

        let cond_token_id = arena.alloc(create_token(Range::default()));
        let cond_node = Rc::new(Node {
            token_id: cond_token_id,
            expr: Rc::new(Expr::Literal(Literal::Bool(true))),
        });

        let stmt1 = Rc::new(Node {
            token_id: stmt1_token_id,
            expr: Rc::new(Expr::Literal(Literal::String("loop1".to_string()))),
        });

        let stmt2 = Rc::new(Node {
            token_id: stmt2_token_id,
            expr: Rc::new(Expr::Literal(Literal::String("loop2".to_string()))),
        });

        let while_token_id = arena.alloc(create_token(Range::default()));
        let while_node = Node {
            token_id: while_token_id,
            expr: Rc::new(Expr::While(cond_node, vec![stmt1, stmt2])),
        };

        assert_eq!(
            while_node.range(Rc::new(arena)),
            Range {
                start: Position::new(3, 2),
                end: Position::new(4, 12)
            }
        );
    }

    #[test]
    fn test_node_range_until_loop() {
        let mut arena = Arena::new(10);

        let stmt1_range = Range {
            start: Position::new(5, 4),
            end: Position::new(5, 9),
        };
        let stmt1_token_id = arena.alloc(create_token(stmt1_range.clone()));

        let stmt2_range = Range {
            start: Position::new(6, 4),
            end: Position::new(6, 15),
        };
        let stmt2_token_id = arena.alloc(create_token(stmt2_range.clone()));

        let cond_token_id = arena.alloc(create_token(Range::default()));
        let cond_node = Rc::new(Node {
            token_id: cond_token_id,
            expr: Rc::new(Expr::Literal(Literal::Bool(false))),
        });

        let stmt1 = Rc::new(Node {
            token_id: stmt1_token_id,
            expr: Rc::new(Expr::Literal(Literal::String("until1".to_string()))),
        });

        let stmt2 = Rc::new(Node {
            token_id: stmt2_token_id,
            expr: Rc::new(Expr::Literal(Literal::String("until2".to_string()))),
        });

        let until_token_id = arena.alloc(create_token(Range::default()));
        let until_node = Node {
            token_id: until_token_id,
            expr: Rc::new(Expr::Until(cond_node, vec![stmt1, stmt2])),
        };

        assert_eq!(
            until_node.range(Rc::new(arena)),
            Range {
                start: Position::new(5, 4),
                end: Position::new(6, 15)
            }
        );
    }

    #[test]
    fn test_node_range_foreach_loop() {
        let mut arena = Arena::new(10);

        let stmt1_range = Range {
            start: Position::new(10, 2),
            end: Position::new(10, 20),
        };
        let stmt1_token_id = arena.alloc(create_token(stmt1_range.clone()));

        let stmt2_range = Range {
            start: Position::new(11, 2),
            end: Position::new(11, 20),
        };
        let stmt2_token_id = arena.alloc(create_token(stmt2_range.clone()));

        let iterable_token_id = arena.alloc(create_token(Range::default()));
        let iterable_node = Rc::new(Node {
            token_id: iterable_token_id,
            expr: Rc::new(Expr::Literal(Literal::String("items".to_string()))),
        });

        let stmt1 = Rc::new(Node {
            token_id: stmt1_token_id,
            expr: Rc::new(Expr::Literal(Literal::String("foreach1".to_string()))),
        });

        let stmt2 = Rc::new(Node {
            token_id: stmt2_token_id,
            expr: Rc::new(Expr::Literal(Literal::String("foreach2".to_string()))),
        });

        let foreach_token_id = arena.alloc(create_token(Range::default()));
        let foreach_node = Node {
            token_id: foreach_token_id,
            expr: Rc::new(Expr::Foreach(
                Ident::new("item"),
                iterable_node,
                vec![stmt1, stmt2],
            )),
        };

        assert_eq!(
            foreach_node.range(Rc::new(arena)),
            Range {
                start: Position::new(10, 2),
                end: Position::new(11, 20)
            }
        );
    }

    #[test]
    fn test_node_range_call_with_args() {
        let mut arena = Arena::new(10);

        let arg1_range = Range {
            start: Position::new(2, 2),
            end: Position::new(2, 2),
        };
        let arg1_token = create_token(arg1_range.clone());
        let arg1_token_id = arena.alloc(Rc::clone(&arg1_token));

        let arg2_range = Range {
            start: Position::new(3, 3),
            end: Position::new(3, 3),
        };
        let arg2_token = create_token(arg2_range.clone());
        let arg2_token_id = arena.alloc(Rc::clone(&arg2_token));

        let arg1 = Rc::new(Node {
            token_id: arg1_token_id,
            expr: Rc::new(Expr::Literal(Literal::String("arg1".to_string()))),
        });

        let arg2 = Rc::new(Node {
            token_id: arg2_token_id,
            expr: Rc::new(Expr::Literal(Literal::String("arg2".to_string()))),
        });

        let call_token_id = arena.alloc(create_token(Range {
            start: Position::new(1, 1),
            end: Position::new(1, 1),
        }));
        let call_node = Node {
            token_id: call_token_id,
            expr: Rc::new(Expr::Call(Ident::new("test_func"), smallvec![arg1, arg2])),
        };

        assert_eq!(
            call_node.range(Rc::new(arena)),
            Range {
                start: Position::new(2, 2),
                end: Position::new(3, 3)
            }
        );
    }

    #[test]
    fn test_node_range_if_expression() {
        let mut arena = Arena::new(10);

        let cond_range = Range {
            start: Position::new(1, 1),
            end: Position::new(1, 1),
        };
        let cond_token_id = arena.alloc(create_token(cond_range.clone()));

        let then_range = Range {
            start: Position::new(2, 2),
            end: Position::new(2, 2),
        };
        let then_token_id = arena.alloc(create_token(then_range.clone()));

        let else_range = Range {
            start: Position::new(3, 3),
            end: Position::new(3, 3),
        };
        let else_token_id = arena.alloc(create_token(else_range.clone()));

        let cond_node = Rc::new(Node {
            token_id: cond_token_id,
            expr: Rc::new(Expr::Literal(Literal::Bool(true))),
        });

        let then_node = Rc::new(Node {
            token_id: then_token_id,
            expr: Rc::new(Expr::Literal(Literal::String("then".to_string()))),
        });

        let else_node = Rc::new(Node {
            token_id: else_token_id,
            expr: Rc::new(Expr::Literal(Literal::String("else".to_string()))),
        });

        let if_node = Node {
            token_id: arena.alloc(create_token(Range {
                start: Position::new(0, 0),
                end: Position::new(0, 0),
            })),
            expr: Rc::new(Expr::If(smallvec![
                (Some(cond_node), then_node),
                (None, else_node),
            ])),
        };

        assert_eq!(
            if_node.range(Rc::new(arena)),
            Range {
                start: Position::new(2, 2),
                end: Position::new(3, 3)
            }
        );
    }

    #[rstest]
    #[case("abc", "def", std::cmp::Ordering::Less)]
    #[case("def", "abc", std::cmp::Ordering::Greater)]
    #[case("abc", "abc", std::cmp::Ordering::Equal)]
    #[case("0", "abc", std::cmp::Ordering::Less)]
    #[case("xyz", "abc", std::cmp::Ordering::Greater)]
    fn test_ident_ordering(
        #[case] name1: &str,
        #[case] name2: &str,
        #[case] expected: std::cmp::Ordering,
    ) {
        assert_eq!(name1.partial_cmp(name2), Some(expected));
    }

    #[cfg(feature = "ast-json")]
    #[rstest]
    #[case(
        Node {
            token_id: ArenaId::new(0),
            expr: Rc::new(Expr::Literal(Literal::String("hello".to_string())))
        },
        "\"hello\""
    )]
    #[case(
        Node {
            token_id: ArenaId::new(0),
            expr: Rc::new(Expr::Ident(Ident::new("foo")))
        },
        "foo"
    )]
    #[case(
        Node {
            token_id: ArenaId::new(0),
            expr: Rc::new(Expr::Call(
                Ident::new("bar"),
                smallvec![
                    Rc::new(Node {
                        token_id: ArenaId::new(1),
                        expr: Rc::new(Expr::Literal(Literal::Number(Number::from(1))))
                    }),
                    Rc::new(Node {
                        token_id: ArenaId::new(2),
                        expr: Rc::new(Expr::Literal(Literal::Number(Number::from(2))))
                    })
                ]
            ))
        },
        "bar(1, 2)"
    )]
    #[case(
        Node {
            token_id: ArenaId::new(0),
            expr: Rc::new(Expr::Let(
                Ident::new("x"),
                Rc::new(Node {
                    token_id: ArenaId::new(1),
                    expr: Rc::new(Expr::Literal(Literal::Bool(true)))
                })
            ))
        },
        "let x = true"
    )]
    #[case(
        Node {
            token_id: ArenaId::new(0),
            expr: Rc::new(Expr::If(smallvec![
                (Some(Rc::new(Node {
                    token_id: ArenaId::new(1),
                    expr: Rc::new(Expr::Literal(Literal::Bool(true)))
                })), Rc::new(Node {
                    token_id: ArenaId::new(2),
                    expr: Rc::new(Expr::Literal(Literal::String("then".to_string())))
                })),
                (None, Rc::new(Node {
                    token_id: ArenaId::new(3),
                    expr: Rc::new(Expr::Literal(Literal::String("else".to_string())))
                }))
            ]))
        },
        "if (true): \"then\" else: \"else\""
    )]
    #[case(
        Node {
            token_id: ArenaId::new(0),
            expr: Rc::new(Expr::While(
                Rc::new(Node {
                    token_id: ArenaId::new(1),
                    expr: Rc::new(Expr::Literal(Literal::Bool(true)))
                }),
                vec![
                    Rc::new(Node {
                        token_id: ArenaId::new(2),
                        expr: Rc::new(Expr::Literal(Literal::String("loop1".to_string())))
                    }),
                    Rc::new(Node {
                        token_id: ArenaId::new(3),
                        expr: Rc::new(Expr::Literal(Literal::String("loop2".to_string())))
                    }),
                ]
            ))
        },
        "while (true): \"loop1\" | \"loop2\";"
    )]
    #[case(
        Node {
            token_id: ArenaId::new(0),
            expr: Rc::new(Expr::Until(
                Rc::new(Node {
                    token_id: ArenaId::new(1),
                    expr: Rc::new(Expr::Literal(Literal::Bool(false)))
                }),
                vec![
                    Rc::new(Node {
                        token_id: ArenaId::new(2),
                        expr: Rc::new(Expr::Literal(Literal::String("until1".to_string())))
                    }),
                    Rc::new(Node {
                        token_id: ArenaId::new(3),
                        expr: Rc::new(Expr::Literal(Literal::String("until2".to_string())))
                    }),
                ]
            ))
        },
        "until (false): \"until1\" | \"until2\";"
    )]
    #[case(
        Node {
            token_id: ArenaId::new(0),
            expr: Rc::new(Expr::Foreach(
                Ident::new("item"),
                Rc::new(Node {
                    token_id: ArenaId::new(1),
                    expr: Rc::new(Expr::Ident(Ident::new("items")))
                }),
                vec![
                    Rc::new(Node {
                        token_id: ArenaId::new(2),
                        expr: Rc::new(Expr::Literal(Literal::String("foreach1".to_string())))
                    }),
                    Rc::new(Node {
                        token_id: ArenaId::new(3),
                        expr: Rc::new(Expr::Literal(Literal::String("foreach2".to_string())))
                    }),
                ]
            ))
        },
        "foreach (item, items): \"foreach1\" | \"foreach2\";"
    )]
    #[case(
        Node {
            token_id: ArenaId::new(0),
            expr: Rc::new(Expr::Self_)
        },
        "self"
    )]
    #[case(
        Node {
            token_id: ArenaId::new(0),
            expr: Rc::new(Expr::Nodes)
        },
        "nodes"
    )]
    #[case(
        Node {
            token_id: ArenaId::new(0),
            expr: Rc::new(Expr::Break)
        },
        "break"
    )]
    #[case(
        Node {
            token_id: ArenaId::new(0),
            expr: Rc::new(Expr::Continue)
        },
        "continue"
    )]
    fn test_node_display(#[case] node: Node, #[case] expected: &str) {
        let mut s = String::new();
        write!(&mut s, "{node}").unwrap();
        assert_eq!(s, expected);
    }
}
