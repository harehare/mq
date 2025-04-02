use std::{
    fmt::{self, Display, Formatter},
    hash::{Hash, Hasher},
    rc::Rc,
};

use compact_str::CompactString;

use crate::{
    Token,
    arena::{Arena, ArenaId},
    lexer,
    number::Number,
    range::Range,
};

use super::{IdentName, Params, Program};

type Depth = u8;
type Index = usize;
type Optional = bool;
type Lang = CompactString;
pub type Args = Vec<Rc<Node>>;
pub type Cond = (Option<Rc<Node>>, Rc<Node>);
pub type TokenId = ArenaId<Rc<Token>>;

#[derive(PartialEq, PartialOrd, Debug, Clone)]
pub struct Node {
    pub token_id: TokenId,
    pub expr: Rc<Expr>,
}

impl Node {
    pub fn range(&self, arena: Rc<Arena<Rc<Token>>>) -> Range {
        match &*self.expr {
            Expr::Def(_, _, program)
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
            Expr::Call(_, args, _) => {
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
                let start = nodes.first().unwrap().1.range(Rc::clone(&arena));
                let end = nodes.last().unwrap().1.range(Rc::clone(&arena));
                Range {
                    start: start.start,
                    end: end.end,
                }
            }
            Expr::Literal(_)
            | Expr::Ident(_)
            | Expr::Selector(_)
            | Expr::Include(_)
            | Expr::InterpolatedString(_)
            | Expr::Self_ => arena[self.token_id].range.clone(),
        }
    }
}

#[derive(PartialEq, Debug, Eq, Clone)]
pub struct Ident {
    pub name: IdentName,
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

#[derive(Debug, Clone, PartialOrd, PartialEq, Eq)]
pub enum StringSegment {
    Text(String),
    Ident(Ident),
}

impl From<&lexer::token::StringSegment> for StringSegment {
    fn from(segment: &lexer::token::StringSegment) -> Self {
        match segment {
            lexer::token::StringSegment::Text(text, _) => StringSegment::Text(text.to_owned()),
            lexer::token::StringSegment::Ident(ident, _) => StringSegment::Ident(Ident::new(ident)),
        }
    }
}

#[derive(PartialEq, PartialOrd, Debug, Clone)]
pub enum Literal {
    String(String),
    Number(Number),
    Bool(bool),
    None,
}

#[derive(PartialEq, PartialOrd, Debug, Clone)]
pub enum Expr {
    Call(Ident, Args, Optional),
    Def(Ident, Params, Program),
    Let(Ident, Rc<Node>),
    Literal(Literal),
    Ident(Ident),
    InterpolatedString(Vec<StringSegment>),
    Selector(Selector),
    While(Rc<Node>, Program),
    Until(Rc<Node>, Program),
    Foreach(Ident, Rc<Node>, Program),
    If(Vec<Cond>),
    Include(Literal),
    Self_,
}
#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::{Position, TokenKind};

    use super::*;

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
                Vec::new(),
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
            expr: Rc::new(Expr::Call(Ident::new("test_func"), vec![arg1, arg2], false)),
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
            expr: Rc::new(Expr::If(vec![
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
}
