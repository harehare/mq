
use std::vec;

use crate::{lexer::token::StringSegment, range::Range};

use super::*;
use crate::cst::node::BinaryOp;
use rstest::rstest;
use smallvec::smallvec;

fn error_node(kinds: Vec<TokenKind>) -> Shared<Node> {
    Shared::new(Node {
        kind: NodeKind::Error {
            children: kinds
                .into_iter()
                .map(|kind| {
                    Shared::new(Node {
                        kind: NodeKind::Token,
                        token: Some(Shared::new(token(kind))),
                        leading_trivia: TriviaList::new(),
                        trailing_trivia: TriviaList::new(),
                    })
                })
                .collect(),
        },
        token: None,
        leading_trivia: TriviaList::new(),
        trailing_trivia: TriviaList::new(),
    })
}

fn missing_node(expected: &'static str) -> Shared<Node> {
    Shared::new(Node {
        kind: NodeKind::Missing {
            expected,
            position: Default::default(),
        },
        token: None,
        leading_trivia: TriviaList::new(),
        trailing_trivia: TriviaList::new(),
    })
}

fn eof_node() -> Shared<Node> {
    Shared::new(Node {
        kind: NodeKind::Eof,
        token: Some(Shared::new(token(TokenKind::Eof))),
        leading_trivia: TriviaList::new(),
        trailing_trivia: TriviaList::new(),
    })
}

fn token(token_kind: TokenKind) -> Token {
    Token {
        range: Range::default(),
        kind: token_kind,
        module_id: 1.into(),
    }
}

#[rstest]
#[case::def(
        vec![
            Shared::new(token(TokenKind::Whitespace(1))),
            Shared::new(token(TokenKind::Comment("test comment".into()))),
            Shared::new(token(TokenKind::Def)),
            Shared::new(token(TokenKind::NewLine)),
            Shared::new(token(TokenKind::Comment("test comment2".into()))),
            Shared::new(token(TokenKind::Ident("foo".into()))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::StringLiteral("test".into()))),
            Shared::new(token(TokenKind::End)),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Def { name: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("foo".into())))),
                            leading_trivia: smallvec![Trivia::NewLine, Trivia::Comment(Shared::new(token(TokenKind::Comment("test comment2".into()))))],
                            trailing_trivia: TriviaList::new(),
                        }), params: Some(vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into()), colon_or_do: Some(Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Colon))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        })), program: vec![
Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::StringLiteral("test".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::End,
                            token: Some(Shared::new(token(TokenKind::End))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Def))),
                    leading_trivia: smallvec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(1)))),
                                         Trivia::Comment(Shared::new(token(TokenKind::Comment("test comment".into()))))],
                    trailing_trivia: TriviaList::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::Eof,
                    token: Some(Shared::new(token(TokenKind::Eof))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::def_without_colon1(
        vec![
            Shared::new(token(TokenKind::Def)),
            Shared::new(token(TokenKind::Ident("bar".into()))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::StringLiteral("test".into()))),
            Shared::new(token(TokenKind::End)),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Def { name: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("bar".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }), params: Some(vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into()), colon_or_do: None, program: vec![
Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::StringLiteral("test".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::End,
                            token: Some(Shared::new(token(TokenKind::End))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Def))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::Eof,
                    token: Some(Shared::new(token(TokenKind::Eof))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::def_without_colon2(
        vec![
            Shared::new(token(TokenKind::Def)),
            Shared::new(token(TokenKind::Ident("baz".into()))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::NumberLiteral(42.into()))),
            Shared::new(token(TokenKind::SemiColon)),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Def { name: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("baz".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }), params: Some(vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into()), colon_or_do: None, program: vec![
Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::NumberLiteral(42.into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::SemiColon))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Def))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::Eof,
                    token: Some(Shared::new(token(TokenKind::Eof))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::def_without_colon_with_args(
        vec![
            Shared::new(token(TokenKind::Def)),
            Shared::new(token(TokenKind::Ident("func".into()))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("x".into()))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Ident("x".into()))),
            Shared::new(token(TokenKind::End)),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Def { name: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("func".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }), params: Some(vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Param { asterisk: None, eq_token: None, default: None },
                            token: Some(Shared::new(token(TokenKind::Ident("x".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into()), colon_or_do: None, program: vec![
Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("x".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::End,
                            token: Some(Shared::new(token(TokenKind::End))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Def))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::Eof,
                    token: Some(Shared::new(token(TokenKind::Eof))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::let_(
        vec![
            Shared::new(token(TokenKind::Whitespace(4))),
            Shared::new(token(TokenKind::Let)),
            Shared::new(token(TokenKind::Whitespace(4))),
            Shared::new(token(TokenKind::Ident("x".into()))),
            Shared::new(token(TokenKind::Equal)),
            Shared::new(token(TokenKind::NumberLiteral(42.into()))),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Let { lhs: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("x".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }), eq_token: Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Equal))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }), rhs: Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::NumberLiteral(42.into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }) },
                    token: Some(Shared::new(token(TokenKind::Let))),
                    leading_trivia: smallvec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(4))))],
                    trailing_trivia: smallvec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(4))))],
                }),
                Shared::new(Node {
                    kind: NodeKind::Eof,
                    token: Some(Shared::new(token(TokenKind::Eof))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::unexpected_token(
        vec![
            Shared::new(token(TokenKind::Whitespace(1))),
            Shared::new(token(TokenKind::Comment("test comment".into()))),
            Shared::new(token(TokenKind::Def)),
            Shared::new(token(TokenKind::NewLine)),
            Shared::new(token(TokenKind::Comment("test comment2".into()))),
            Shared::new(token(TokenKind::Ident("foo".into()))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::StringLiteral("test".into()))),
            Shared::new(token(TokenKind::Comma)),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Def { name: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("foo".into())))),
                            leading_trivia: smallvec![Trivia::NewLine, Trivia::Comment(Shared::new(token(TokenKind::Comment("test comment2".into()))))],
                            trailing_trivia: TriviaList::new(),
                        }), params: Some(vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into()), colon_or_do: Some(Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Colon))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        })), program: vec![
Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::StringLiteral("test".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Def))),
                    leading_trivia: smallvec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(1)))),
                                         Trivia::Comment(Shared::new(token(TokenKind::Comment("test comment".into())))) ],
                    trailing_trivia: TriviaList::new(),
                }),
                error_node(vec![TokenKind::Comma]),
                eof_node(),
            ],
            ErrorReporter::with_error(vec![ParseError::UnexpectedToken(Shared::new(token(TokenKind::Comma)))], 100)
        )
    )]
#[case::unexpected_eof(
        vec![
            Shared::new(token(TokenKind::Whitespace(4))),
            Shared::new(token(TokenKind::Let)),
            Shared::new(token(TokenKind::Whitespace(4))),
            Shared::new(token(TokenKind::Ident("x".into()))),
            Shared::new(token(TokenKind::Equal)),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Error {
                        children: smallvec![
                            Shared::new(Node {
                                kind: NodeKind::Token,
                                token: Some(Shared::new(token(TokenKind::Let))),
                                leading_trivia: TriviaList::new(),
                                trailing_trivia: smallvec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(4))))],
                            }),
                            Shared::new(Node {
                                kind: NodeKind::Token,
                                token: Some(Shared::new(token(TokenKind::Ident("x".into())))),
                                leading_trivia: TriviaList::new(),
                                trailing_trivia: TriviaList::new(),
                            }),
                            Shared::new(Node {
                                kind: NodeKind::Token,
                                token: Some(Shared::new(token(TokenKind::Equal))),
                                leading_trivia: TriviaList::new(),
                                trailing_trivia: TriviaList::new(),
                            }),
                        ],
                    },
                    token: None,
                    leading_trivia: smallvec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(4))))],
                    trailing_trivia: TriviaList::new(),
                }),
                eof_node(),
            ],
            ErrorReporter::with_error(vec![ParseError::UnexpectedEOFDetected], 100)
        )
    )]
#[case::if_(
        vec![
            Shared::new(token(TokenKind::If)),
            Shared::new(token(TokenKind::Whitespace(2))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("condition".into()))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::Ident("then_branch".into()))),
            Shared::new(token(TokenKind::Else)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::Ident("else_branch".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::If { args: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("condition".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into(), colon: Some(Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Colon))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        })), then_branch: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("then_branch".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }), elifs: Elifs::new(), else_branch: Some(Shared::new(Node {
                            kind: NodeKind::Else { colon: Some(Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::Colon))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                })), then_branch: Shared::new(Node {
                                    kind: NodeKind::Ident { attr: None },
                                    token: Some(Shared::new(token(TokenKind::Ident("else_branch".into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }) },
                            token: Some(Shared::new(token(TokenKind::Else))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        })) },
                    token: Some(Shared::new(token(TokenKind::If))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: smallvec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(2))))],
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::unless_(
        vec![
            Shared::new(token(TokenKind::Unless)),
            Shared::new(token(TokenKind::Whitespace(2))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("condition".into()))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::Ident("then_branch".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Unless { args: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("condition".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into(), colon: Some(Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Colon))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        })), then_branch: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("then_branch".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }) },
                    token: Some(Shared::new(token(TokenKind::Unless))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: smallvec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(2))))],
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::if_elif_else(
        vec![
            Shared::new(token(TokenKind::If)),
            Shared::new(token(TokenKind::Whitespace(2))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("condition1".into()))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::Ident("then_branch1".into()))),
            Shared::new(token(TokenKind::Elif)),
            Shared::new(token(TokenKind::Whitespace(2))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("condition2".into()))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::Ident("then_branch2".into()))),
            Shared::new(token(TokenKind::Else)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::Ident("else_branch".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::If { args: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("condition1".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into(), colon: Some(Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Colon))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        })), then_branch: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("then_branch1".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }), elifs: vec![
Shared::new(Node {
                            kind: NodeKind::Elif { args: vec![
Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::LParen))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }),
Shared::new(Node {
                                    kind: NodeKind::Ident { attr: None },
                                    token: Some(Shared::new(token(TokenKind::Ident("condition2".into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }),
Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::RParen))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }),
].into(), colon: Some(Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::Colon))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                })), then_branch: Shared::new(Node {
                                    kind: NodeKind::Ident { attr: None },
                                    token: Some(Shared::new(token(TokenKind::Ident("then_branch2".into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }) },
                            token: Some(Shared::new(token(TokenKind::Elif))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: smallvec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(2))))],
                        }),
].into(), else_branch: Some(Shared::new(Node {
                            kind: NodeKind::Else { colon: Some(Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::Colon))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                })), then_branch: Shared::new(Node {
                                    kind: NodeKind::Ident { attr: None },
                                    token: Some(Shared::new(token(TokenKind::Ident("else_branch".into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }) },
                            token: Some(Shared::new(token(TokenKind::Else))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        })) },
                    token: Some(Shared::new(token(TokenKind::If))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: smallvec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(2))))],
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::if_only(
        vec![
            Shared::new(token(TokenKind::If)),
            Shared::new(token(TokenKind::Whitespace(2))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("condition1".into()))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::Ident("then_branch1".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::If { args: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("condition1".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into(), colon: Some(Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Colon))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        })), then_branch: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("then_branch1".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }), elifs: Elifs::new(), else_branch: None },
                    token: Some(Shared::new(token(TokenKind::If))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: smallvec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(2))))],
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::if_elif_else(
        vec![
            Shared::new(token(TokenKind::If)),
            Shared::new(token(TokenKind::Whitespace(2))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("condition1".into()))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::Ident("then_branch1".into()))),
            Shared::new(token(TokenKind::Elif)),
            Shared::new(token(TokenKind::Whitespace(2))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("condition2".into()))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::Ident("then_branch2".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::If { args: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("condition1".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into(), colon: Some(Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Colon))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        })), then_branch: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("then_branch1".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }), elifs: vec![
Shared::new(Node {
                            kind: NodeKind::Elif { args: vec![
Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::LParen))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }),
Shared::new(Node {
                                    kind: NodeKind::Ident { attr: None },
                                    token: Some(Shared::new(token(TokenKind::Ident("condition2".into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }),
Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::RParen))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }),
].into(), colon: Some(Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::Colon))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                })), then_branch: Shared::new(Node {
                                    kind: NodeKind::Ident { attr: None },
                                    token: Some(Shared::new(token(TokenKind::Ident("then_branch2".into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }) },
                            token: Some(Shared::new(token(TokenKind::Elif))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: smallvec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(2))))],
                        }),
].into(), else_branch: None },
                    token: Some(Shared::new(token(TokenKind::If))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: smallvec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(2))))],
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::multiple_expr_with_trivia(
        vec![
            Shared::new(token(TokenKind::Whitespace(1))),
            Shared::new(token(TokenKind::Ident("x".into()))),
            Shared::new(token(TokenKind::Whitespace(1))),
            Shared::new(token(TokenKind::Pipe)),
            Shared::new(token(TokenKind::Whitespace(1))),
            Shared::new(token(TokenKind::Ident("y".into()))),
            Shared::new(token(TokenKind::Whitespace(1))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Ident { attr: None },
                    token: Some(Shared::new(token(TokenKind::Ident("x".into())))),
                    leading_trivia: smallvec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(1))))],
                    trailing_trivia: smallvec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(1))))],
                }),
                Shared::new(Node {
                    kind: NodeKind::Token,
                    token: Some(Shared::new(token(TokenKind::Pipe))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: smallvec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(1))))],
                }),
                Shared::new(Node {
                    kind: NodeKind::Ident { attr: None },
                    token: Some(Shared::new(token(TokenKind::Ident("y".into())))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: smallvec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(1))))],
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::args_with_function(
        vec![
            Shared::new(token(TokenKind::Ident("foo".into()))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("bar".into()))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::RParen)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Call { args: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Call { args: vec![
Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::LParen))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }),
Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::RParen))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }),
].into() },
                            token: Some(Shared::new(token(TokenKind::Ident("bar".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Ident("foo".into())))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::foreach(
        vec![
            Shared::new(token(TokenKind::Foreach)),
            Shared::new(token(TokenKind::Whitespace(1))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("item".into()))),
            Shared::new(token(TokenKind::Comma)),
            Shared::new(token(TokenKind::Ident("collection".into()))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::NewLine)),
            Shared::new(token(TokenKind::Comment("comment".into()))),
            Shared::new(token(TokenKind::NewLine)),
            Shared::new(token(TokenKind::Ident("body".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Foreach { args: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("item".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Comma))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("collection".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into(), colon_or_do: Some(Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Colon))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        })), program: vec![
Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("body".into())))),
                            leading_trivia: smallvec![Trivia::NewLine, Trivia::Comment(Shared::new(token(TokenKind::Comment("comment".into())))), Trivia::NewLine],
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Foreach))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: smallvec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(1))))],
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::foreach_do_end(
        vec![
            Shared::new(token(TokenKind::Foreach)),
            Shared::new(token(TokenKind::Whitespace(1))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("item".into()))),
            Shared::new(token(TokenKind::Comma)),
            Shared::new(token(TokenKind::Ident("collection".into()))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Do)),
            Shared::new(token(TokenKind::NewLine)),
            Shared::new(token(TokenKind::Ident("body".into()))),
            Shared::new(token(TokenKind::NewLine)),
            Shared::new(token(TokenKind::End)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Foreach { args: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("item".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Comma))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("collection".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into(), colon_or_do: Some(Shared::new(Node {
                            kind: NodeKind::Do,
                            token: Some(Shared::new(token(TokenKind::Do))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        })), program: vec![
Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("body".into())))),
                            leading_trivia: smallvec![Trivia::NewLine],
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::End,
                            token: Some(Shared::new(token(TokenKind::End))),
                            leading_trivia: smallvec![Trivia::NewLine],
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Foreach))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: smallvec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(1))))],
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::foreach_value_is_function_call(
        vec![
            Shared::new(token(TokenKind::Foreach)),
            Shared::new(token(TokenKind::Whitespace(1))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("item".into()))),
            Shared::new(token(TokenKind::Comma)),
            Shared::new(token(TokenKind::Ident("get_items".into()))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("arg".into()))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::NewLine)),
            Shared::new(token(TokenKind::Ident("body".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Foreach { args: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("item".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Comma))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Call { args: vec![
Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::LParen))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }),
Shared::new(Node {
                                    kind: NodeKind::Ident { attr: None },
                                    token: Some(Shared::new(token(TokenKind::Ident("arg".into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }),
Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::RParen))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }),
].into() },
                            token: Some(Shared::new(token(TokenKind::Ident("get_items".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into(), colon_or_do: Some(Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Colon))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        })), program: vec![
Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("body".into())))),
                            leading_trivia: smallvec![Trivia::NewLine],
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Foreach))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: smallvec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(1))))],
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::while_(
        vec![
            Shared::new(token(TokenKind::While)),
            Shared::new(token(TokenKind::Whitespace(1))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("condition".into()))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::NewLine)),
            Shared::new(token(TokenKind::Comment("comment".into()))),
            Shared::new(token(TokenKind::NewLine)),
            Shared::new(token(TokenKind::Ident("body".into()))),
            Shared::new(token(TokenKind::Whitespace(4))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::While { args: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("condition".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into(), colon_or_do: Some(Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Colon))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        })), program: vec![
Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("body".into())))),
                            leading_trivia: smallvec![Trivia::NewLine, Trivia::Comment(Shared::new(token(TokenKind::Comment("comment".into())))), Trivia::NewLine],
                            trailing_trivia: smallvec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(4))))],
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::While))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: smallvec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(1))))],
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::while_do_end(
        vec![
            Shared::new(token(TokenKind::While)),
            Shared::new(token(TokenKind::Whitespace(1))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("condition".into()))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Do)),
            Shared::new(token(TokenKind::NewLine)),
            Shared::new(token(TokenKind::Ident("body".into()))),
            Shared::new(token(TokenKind::NewLine)),
            Shared::new(token(TokenKind::End)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::While { args: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("condition".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into(), colon_or_do: Some(Shared::new(Node {
                            kind: NodeKind::Do,
                            token: Some(Shared::new(token(TokenKind::Do))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        })), program: vec![
Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("body".into())))),
                            leading_trivia: smallvec![Trivia::NewLine],
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::End,
                            token: Some(Shared::new(token(TokenKind::End))),
                            leading_trivia: smallvec![Trivia::NewLine],
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::While))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: smallvec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(1))))],
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::until_(
        vec![
            Shared::new(token(TokenKind::Until)),
            Shared::new(token(TokenKind::Whitespace(1))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("condition".into()))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::Ident("body".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Until { args: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("condition".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into(), colon_or_do: Some(Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Colon))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        })), program: vec![
Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("body".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Until))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: smallvec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(1))))],
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::loop_(
        vec![
            Shared::new(token(TokenKind::Loop)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::NewLine)),
            Shared::new(token(TokenKind::Comment("loop comment".into()))),
            Shared::new(token(TokenKind::NewLine)),
            Shared::new(token(TokenKind::Ident("body".into()))),
            Shared::new(token(TokenKind::Whitespace(2))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Loop { colon_or_do: Some(Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Colon))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        })), program: vec![
Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("body".into())))),
                            leading_trivia: smallvec![Trivia::NewLine, Trivia::Comment(Shared::new(token(TokenKind::Comment("loop comment".into())))), Trivia::NewLine],
                            trailing_trivia: smallvec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(2))))],
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Loop))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::selector1(
        vec![
            Shared::new(token(TokenKind::Selector(".h1".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Selector { items: ArgList::new() },
                    token: Some(Shared::new(token(TokenKind::Selector(".h1".into())))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::selector2(
        vec![
            Shared::new(token(TokenKind::Selector(".".into()))),
            Shared::new(token(TokenKind::LBracket)),
            Shared::new(token(TokenKind::NumberLiteral(2.into()))),
            Shared::new(token(TokenKind::RBracket)),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Selector { items: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LBracket))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::NumberLiteral(2.into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RBracket))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Selector(".".into())))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::Eof,
                    token: Some(Shared::new(token(TokenKind::Eof))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::selector3(
        vec![
            Shared::new(token(TokenKind::Selector(".".into()))),
            Shared::new(token(TokenKind::LBracket)),
            Shared::new(token(TokenKind::NumberLiteral(2.into()))),
            Shared::new(token(TokenKind::RBracket)),
            Shared::new(token(TokenKind::LBracket)),
            Shared::new(token(TokenKind::NumberLiteral(2.into()))),
            Shared::new(token(TokenKind::RBracket)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Selector { items: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LBracket))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::NumberLiteral(2.into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RBracket))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LBracket))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::NumberLiteral(2.into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RBracket))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Selector(".".into())))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::selector4(
        vec![
            Shared::new(token(TokenKind::Selector(".list".into()))),
            Shared::new(token(TokenKind::Selector(".checked".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Selector { items: vec![
Shared::new(Node {
                            kind: NodeKind::Selector { items: ArgList::new() },
                            token: Some(Shared::new(token(TokenKind::Selector(".checked".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Selector(".list".into())))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::ident_attribute_access(
        vec![
            Shared::new(token(TokenKind::Ident("c".into()))),
            Shared::new(token(TokenKind::Selector(".lang".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Ident { attr: Some(Shared::new(Node {
                            kind: NodeKind::Selector { items: ArgList::new() },
                            token: Some(Shared::new(token(TokenKind::Selector(".lang".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        })) },
                    token: Some(Shared::new(token(TokenKind::Ident("c".into())))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::self_attribute_access(
        vec![
            Shared::new(token(TokenKind::Self_)),
            Shared::new(token(TokenKind::Selector(".name".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Self_ { attr: Some(Shared::new(Node {
                            kind: NodeKind::Selector { items: ArgList::new() },
                            token: Some(Shared::new(token(TokenKind::Selector(".name".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        })) },
                    token: Some(Shared::new(token(TokenKind::Self_))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::standalone_attr_selector(
        vec![
            Shared::new(token(TokenKind::Selector(".lang".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::SelfAttr,
                    token: Some(Shared::new(token(TokenKind::Selector(".lang".into())))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::include(
        vec![
            Shared::new(token(TokenKind::Include)),
            Shared::new(token(TokenKind::Whitespace(1))),
            Shared::new(token(TokenKind::StringLiteral("module".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Include { path: Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::StringLiteral("module".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }) },
                    token: Some(Shared::new(token(TokenKind::Include))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: smallvec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(1))))],
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::import(
        vec![
            Shared::new(token(TokenKind::Import)),
            Shared::new(token(TokenKind::Whitespace(1))),
            Shared::new(token(TokenKind::StringLiteral("module".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Import { path: Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::StringLiteral("module".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }), as_token: None, alias: None },
                    token: Some(Shared::new(token(TokenKind::Import))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: smallvec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(1))))],
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::import_as(
        vec![
            Shared::new(token(TokenKind::Import)),
            Shared::new(token(TokenKind::Whitespace(1))),
            Shared::new(token(TokenKind::StringLiteral("module".into()))),
            Shared::new(token(TokenKind::Whitespace(1))),
            Shared::new(token(TokenKind::As)),
            Shared::new(token(TokenKind::Whitespace(1))),
            Shared::new(token(TokenKind::Ident("m".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Import { path: Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::StringLiteral("module".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: smallvec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(1))))],
                        }), as_token: Some(Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::As))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: smallvec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(1))))],
                        })), alias: Some(Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("m".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        })) },
                    token: Some(Shared::new(token(TokenKind::Import))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: smallvec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(1))))],
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::root_with_token_after_semicolon(
        vec![
            Shared::new(token(TokenKind::Ident("x".into()))),
            Shared::new(token(TokenKind::SemiColon)),
            Shared::new(token(TokenKind::Ident("y".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Ident { attr: None },
                    token: Some(Shared::new(token(TokenKind::Ident("x".into())))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::Token,
                    token: Some(Shared::new(token(TokenKind::SemiColon))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::Ident { attr: None },
                    token: Some(Shared::new(token(TokenKind::Ident("y".into())))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::with_error(vec![ParseError::UnexpectedToken(Shared::new(token(TokenKind::Ident("y".into()))))], 100)
        )
    )]
#[case::call_with_newlines(
        vec![
            Shared::new(token(TokenKind::Ident("foo".into()))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::NewLine)),
            Shared::new(token(TokenKind::Comment("param comment".into()))),
            Shared::new(token(TokenKind::NewLine)),
            Shared::new(token(TokenKind::Ident("arg1".into()))),
            Shared::new(token(TokenKind::Comma)),
            Shared::new(token(TokenKind::NewLine)),
            Shared::new(token(TokenKind::Ident("arg2".into()))),
            Shared::new(token(TokenKind::NewLine)),
            Shared::new(token(TokenKind::RParen)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Call { args: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("arg1".into())))),
                            leading_trivia: smallvec![Trivia::NewLine, Trivia::Comment(Shared::new(token(TokenKind::Comment("param comment".into())))), Trivia::NewLine],
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Comma))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("arg2".into())))),
                            leading_trivia: smallvec![Trivia::NewLine],
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: smallvec![Trivia::NewLine],
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Ident("foo".into())))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::interpolated_string(
        vec![
            Shared::new(token(TokenKind::InterpolatedString(vec![StringSegment::Expr("val".into(), Range::default()), StringSegment::Text("hello".into(), Range::default())]))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::InterpolatedString,
                    token: Some(Shared::new(token(TokenKind::InterpolatedString(vec![StringSegment::Expr("val".into(), Range::default()), StringSegment::Text("hello".into(), Range::default())])))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::nodes(
        vec![
            Shared::new(token(TokenKind::Nodes)),
            Shared::new(token(TokenKind::Whitespace(1))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Nodes,
                    token: Some(Shared::new(token(TokenKind::Nodes))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: smallvec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(1))))],
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::fn_with_program(
        vec![
            Shared::new(token(TokenKind::Fn)),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::Ident("x".into()))),
            Shared::new(token(TokenKind::SemiColon)),
            Shared::new(token(TokenKind::Pipe)),
            Shared::new(token(TokenKind::Ident("y".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Fn { params: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into(), colon_or_do: Some(Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Colon))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        })), program: vec![
Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("x".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::SemiColon))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Fn))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::Token,
                    token: Some(Shared::new(token(TokenKind::Pipe))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::Ident { attr: None },
                    token: Some(Shared::new(token(TokenKind::Ident("y".into())))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::fn_with_parameter_and_program(
        vec![
            Shared::new(token(TokenKind::Fn)),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("param".into()))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::Ident("body".into()))),
            Shared::new(token(TokenKind::SemiColon)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Fn { params: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Param { asterisk: None, eq_token: None, default: None },
                            token: Some(Shared::new(token(TokenKind::Ident("param".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into(), colon_or_do: Some(Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Colon))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        })), program: vec![
Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("body".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::SemiColon))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Fn))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),

            ],
            ErrorReporter::default()
        )
    )]
#[case::fn_with_multiple_parameters(
        vec![
            Shared::new(token(TokenKind::Fn)),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("param1".into()))),
            Shared::new(token(TokenKind::Comma)),
            Shared::new(token(TokenKind::Ident("param2".into()))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::StringLiteral("result".into()))),
            Shared::new(token(TokenKind::SemiColon)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Fn { params: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Param { asterisk: None, eq_token: None, default: None },
                            token: Some(Shared::new(token(TokenKind::Ident("param1".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Comma))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Param { asterisk: None, eq_token: None, default: None },
                            token: Some(Shared::new(token(TokenKind::Ident("param2".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into(), colon_or_do: Some(Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Colon))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        })), program: vec![
Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::StringLiteral("result".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::SemiColon))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Fn))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::array_empty(
        vec![
            Shared::new(token(TokenKind::LBracket)),
            Shared::new(token(TokenKind::RBracket)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Array { items: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LBracket))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RBracket))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: None,
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::array_with_single_element(
        vec![
            Shared::new(token(TokenKind::LBracket)),
            Shared::new(token(TokenKind::NumberLiteral(42.into()))),
            Shared::new(token(TokenKind::RBracket)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Array { items: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LBracket))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::NumberLiteral(42.into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RBracket))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: None,
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::array_with_multiple_elements(
        vec![
            Shared::new(token(TokenKind::LBracket)),
            Shared::new(token(TokenKind::NumberLiteral(1.into()))),
            Shared::new(token(TokenKind::Comma)),
            Shared::new(token(TokenKind::StringLiteral("hello".into()))),
            Shared::new(token(TokenKind::Comma)),
            Shared::new(token(TokenKind::Ident("x".into()))),
            Shared::new(token(TokenKind::RBracket)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Array { items: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LBracket))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::NumberLiteral(1.into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Comma))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::StringLiteral("hello".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Comma))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("x".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RBracket))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: None,
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::array_with_trivia(
        vec![
            Shared::new(token(TokenKind::Whitespace(2))),
            Shared::new(token(TokenKind::LBracket)),
            Shared::new(token(TokenKind::NewLine)),
            Shared::new(token(TokenKind::Comment("array element".into()))),
            Shared::new(token(TokenKind::NewLine)),
            Shared::new(token(TokenKind::NumberLiteral(42.into()))),
            Shared::new(token(TokenKind::Comma)),
            Shared::new(token(TokenKind::NewLine)),
            Shared::new(token(TokenKind::StringLiteral("test".into()))),
            Shared::new(token(TokenKind::NewLine)),
            Shared::new(token(TokenKind::RBracket)),
            Shared::new(token(TokenKind::Whitespace(1))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Array { items: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LBracket))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::NumberLiteral(42.into())))),
                            leading_trivia: smallvec![
                                Trivia::NewLine,
                                Trivia::Comment(Shared::new(token(TokenKind::Comment("array element".into())))),
                                Trivia::NewLine
                            ],
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Comma))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::StringLiteral("test".into())))),
                            leading_trivia: smallvec![Trivia::NewLine],
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RBracket))),
                            leading_trivia: smallvec![Trivia::NewLine],
                                trailing_trivia: smallvec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(1))))],
                        }),
].into() },
                    token: None,
                    leading_trivia: smallvec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(2))))],
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::array_nested(
        vec![
            Shared::new(token(TokenKind::LBracket)),
            Shared::new(token(TokenKind::LBracket)),
            Shared::new(token(TokenKind::NumberLiteral(1.into()))),
            Shared::new(token(TokenKind::RBracket)),
            Shared::new(token(TokenKind::Comma)),
            Shared::new(token(TokenKind::LBracket)),
            Shared::new(token(TokenKind::NumberLiteral(2.into()))),
            Shared::new(token(TokenKind::RBracket)),
            Shared::new(token(TokenKind::RBracket)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Array { items: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LBracket))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Array { items: vec![
Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::LBracket))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }),
Shared::new(Node {
                                    kind: NodeKind::Literal,
                                    token: Some(Shared::new(token(TokenKind::NumberLiteral(1.into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }),
Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::RBracket))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }),
].into() },
                            token: None,
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Comma))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Array { items: vec![
Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::LBracket))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }),
Shared::new(Node {
                                    kind: NodeKind::Literal,
                                    token: Some(Shared::new(token(TokenKind::NumberLiteral(2.into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }),
Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::RBracket))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }),
].into() },
                            token: None,
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RBracket))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: None,
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::array_with_spread(
        vec![
            Shared::new(token(TokenKind::LBracket)),
            Shared::new(token(TokenKind::DotDotDot)),
            Shared::new(token(TokenKind::Ident("a".into()))),
            Shared::new(token(TokenKind::RBracket)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Array { items: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LBracket))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Spread { operand: Shared::new(Node {
                                    kind: NodeKind::Ident { attr: None },
                                    token: Some(Shared::new(token(TokenKind::Ident("a".into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }) },
                            token: Some(Shared::new(token(TokenKind::DotDotDot))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RBracket))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: None,
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::dict_empty(
        vec![
            Shared::new(token(TokenKind::LBrace)),
            Shared::new(token(TokenKind::RBrace)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Dict { entries: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LBrace))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RBrace))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: None,
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::dict_with_single_pair(
        vec![
            Shared::new(token(TokenKind::LBrace)),
            Shared::new(token(TokenKind::StringLiteral("test".into()))),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::NumberLiteral(1.into()))),
            Shared::new(token(TokenKind::RBrace)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Dict { entries: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LBrace))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::DictEntry { key: Shared::new(Node {
                                    kind: NodeKind::Literal,
                                    token: Some(Shared::new(token(TokenKind::StringLiteral("test".into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }), colon: Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::Colon))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }), value: Shared::new(Node {
                                    kind: NodeKind::Literal,
                                    token: Some(Shared::new(token(TokenKind::NumberLiteral(1.into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }) },
                            token: None,
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RBrace))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: None,
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::dict_with_multiple_pairs(
        vec![
            Shared::new(token(TokenKind::LBrace)),
            Shared::new(token(TokenKind::StringLiteral("test".into()))),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::NumberLiteral(1.into()))),
            Shared::new(token(TokenKind::Comma)),
            Shared::new(token(TokenKind::StringLiteral("foo".into()))),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::NumberLiteral(2.into()))),
            Shared::new(token(TokenKind::RBrace)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Dict { entries: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LBrace))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::DictEntry { key: Shared::new(Node {
                                    kind: NodeKind::Literal,
                                    token: Some(Shared::new(token(TokenKind::StringLiteral("test".into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }), colon: Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::Colon))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }), value: Shared::new(Node {
                                    kind: NodeKind::Literal,
                                    token: Some(Shared::new(token(TokenKind::NumberLiteral(1.into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }) },
                            token: None,
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Comma))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::DictEntry { key: Shared::new(Node {
                                    kind: NodeKind::Literal,
                                    token: Some(Shared::new(token(TokenKind::StringLiteral("foo".into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }), colon: Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::Colon))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }), value: Shared::new(Node {
                                    kind: NodeKind::Literal,
                                    token: Some(Shared::new(token(TokenKind::NumberLiteral(2.into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }) },
                            token: None,
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RBrace))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: None,
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::dict_with_spread(
        vec![
            Shared::new(token(TokenKind::LBrace)),
            Shared::new(token(TokenKind::DotDotDot)),
            Shared::new(token(TokenKind::Ident("a".into()))),
            Shared::new(token(TokenKind::RBrace)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Dict { entries: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LBrace))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Spread { operand: Shared::new(Node {
                                    kind: NodeKind::Ident { attr: None },
                                    token: Some(Shared::new(token(TokenKind::Ident("a".into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }) },
                            token: Some(Shared::new(token(TokenKind::DotDotDot))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RBrace))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: None,
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::eq_eq(
        vec![
            Shared::new(token(TokenKind::Ident("a".into()))),
            Shared::new(token(TokenKind::EqEq)),
            Shared::new(token(TokenKind::Ident("b".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::BinaryOp { op: BinaryOp::Equal, lhs: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("a".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }), rhs: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("b".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }) },
                    token: Some(Shared::new(token(TokenKind::EqEq))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::ne_eq(
        vec![
            Shared::new(token(TokenKind::Ident("a".into()))),
            Shared::new(token(TokenKind::Whitespace(1))),
            Shared::new(token(TokenKind::NeEq)),
            Shared::new(token(TokenKind::Whitespace(1))),
            Shared::new(token(TokenKind::Ident("b".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::BinaryOp { op: BinaryOp::NotEqual, lhs: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("a".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: smallvec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(1))))],
                        }), rhs: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("b".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }) },
                    token: Some(Shared::new(token(TokenKind::NeEq))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: smallvec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(1))))],
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::plus(
        vec![
            Shared::new(token(TokenKind::Ident("a".into()))),
            Shared::new(token(TokenKind::Plus)),
            Shared::new(token(TokenKind::Ident("b".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::BinaryOp { op: BinaryOp::Plus, lhs: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("a".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }), rhs: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("b".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }) },
                    token: Some(Shared::new(token(TokenKind::Plus))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::coalesce(
        vec![
            Shared::new(token(TokenKind::Ident("a".into()))),
            Shared::new(token(TokenKind::Coalesce)),
            Shared::new(token(TokenKind::Ident("b".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::BinaryOp { op: BinaryOp::Coalesce, lhs: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("a".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }), rhs: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("b".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }) },
                    token: Some(Shared::new(token(TokenKind::Coalesce))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::lt(
        vec![
            Shared::new(token(TokenKind::Ident("a".into()))),
            Shared::new(token(TokenKind::Lt)),
            Shared::new(token(TokenKind::Ident("b".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::BinaryOp { op: BinaryOp::Lt, lhs: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("a".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }), rhs: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("b".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }) },
                    token: Some(Shared::new(token(TokenKind::Lt))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::lte(
        vec![
            Shared::new(token(TokenKind::Ident("a".into()))),
            Shared::new(token(TokenKind::Lte)),
            Shared::new(token(TokenKind::Ident("b".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::BinaryOp { op: BinaryOp::Lte, lhs: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("a".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }), rhs: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("b".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }) },
                    token: Some(Shared::new(token(TokenKind::Lte))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::gt(
        vec![
            Shared::new(token(TokenKind::Ident("a".into()))),
            Shared::new(token(TokenKind::Gt)),
            Shared::new(token(TokenKind::Ident("b".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::BinaryOp { op: BinaryOp::Gt, lhs: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("a".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }), rhs: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("b".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }) },
                    token: Some(Shared::new(token(TokenKind::Gt))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::gte(
        vec![
            Shared::new(token(TokenKind::Ident("a".into()))),
            Shared::new(token(TokenKind::Gte)),
            Shared::new(token(TokenKind::Ident("b".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::BinaryOp { op: BinaryOp::Gte, lhs: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("a".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }), rhs: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("b".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }) },
                    token: Some(Shared::new(token(TokenKind::Gte))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::range(
        vec![
            Shared::new(token(TokenKind::Ident("a".into()))),
            Shared::new(token(TokenKind::DoubleDot)),
            Shared::new(token(TokenKind::Ident("b".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::BinaryOp { op: BinaryOp::RangeOp, lhs: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("a".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }), rhs: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("b".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }) },
                    token: Some(Shared::new(token(TokenKind::DoubleDot))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::minus(
        vec![
            Shared::new(token(TokenKind::Ident("a".into()))),
            Shared::new(token(TokenKind::Minus)),
            Shared::new(token(TokenKind::Ident("b".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::BinaryOp { op: BinaryOp::Minus, lhs: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("a".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }), rhs: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("b".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }) },
                    token: Some(Shared::new(token(TokenKind::Minus))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::division(
        vec![
            Shared::new(token(TokenKind::Ident("a".into()))),
            Shared::new(token(TokenKind::Slash)),
            Shared::new(token(TokenKind::Ident("b".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::BinaryOp { op: BinaryOp::Division, lhs: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("a".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }), rhs: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("b".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }) },
                    token: Some(Shared::new(token(TokenKind::Slash))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::percent(
        vec![
            Shared::new(token(TokenKind::Ident("a".into()))),
            Shared::new(token(TokenKind::Percent)),
            Shared::new(token(TokenKind::Ident("b".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::BinaryOp { op: BinaryOp::Modulo, lhs: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("a".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }), rhs: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("b".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }) },
                    token: Some(Shared::new(token(TokenKind::Percent))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::multiplication(
        vec![
            Shared::new(token(TokenKind::Ident("a".into()))),
            Shared::new(token(TokenKind::Asterisk)),
            Shared::new(token(TokenKind::Ident("b".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::BinaryOp { op: BinaryOp::Multiplication, lhs: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("a".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }), rhs: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("b".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }) },
                    token: Some(Shared::new(token(TokenKind::Asterisk))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::left_shift(
        vec![
            Shared::new(token(TokenKind::Ident("a".into()))),
            Shared::new(token(TokenKind::LeftShift)),
            Shared::new(token(TokenKind::Ident("b".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::BinaryOp { op: BinaryOp::LeftShift, lhs: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("a".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }), rhs: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("b".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }) },
                    token: Some(Shared::new(token(TokenKind::LeftShift))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::right_shift(
        vec![
            Shared::new(token(TokenKind::Ident("a".into()))),
            Shared::new(token(TokenKind::RightShift)),
            Shared::new(token(TokenKind::Ident("b".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::BinaryOp { op: BinaryOp::RightShift, lhs: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("a".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }), rhs: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("b".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }) },
                    token: Some(Shared::new(token(TokenKind::RightShift))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::convert(
        vec![
            Shared::new(token(TokenKind::Ident("a".into()))),
            Shared::new(token(TokenKind::Convert)),
            Shared::new(token(TokenKind::Ident("b".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::BinaryOp { op: BinaryOp::Convert, lhs: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("a".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }), rhs: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("b".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }) },
                    token: Some(Shared::new(token(TokenKind::Convert))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::multiple_binary_ops(
        vec![
            Shared::new(token(TokenKind::Ident("a".into()))),
            Shared::new(token(TokenKind::Plus)),
            Shared::new(token(TokenKind::Ident("b".into()))),
            Shared::new(token(TokenKind::Minus)),
            Shared::new(token(TokenKind::Ident("c".into()))),
            Shared::new(token(TokenKind::Asterisk)),
            Shared::new(token(TokenKind::Ident("d".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::BinaryOp { op: BinaryOp::Minus, lhs: Shared::new(Node {
                    kind: NodeKind::BinaryOp { op: BinaryOp::Plus, lhs: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("a".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }), rhs: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("b".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }) },
                    token: Some(Shared::new(token(TokenKind::Plus))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }), rhs: Shared::new(Node {
                    kind: NodeKind::BinaryOp { op: BinaryOp::Multiplication, lhs: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("c".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }), rhs: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("d".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }) },
                    token: Some(Shared::new(token(TokenKind::Asterisk))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }) },
                    token: Some(Shared::new(token(TokenKind::Minus))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::binary_ops_with_trivia(
        vec![
            Shared::new(token(TokenKind::Ident("x".into()))),
            Shared::new(token(TokenKind::Whitespace(1))),
            Shared::new(token(TokenKind::Plus)),
            Shared::new(token(TokenKind::Whitespace(1))),
            Shared::new(token(TokenKind::Ident("y".into()))),
            Shared::new(token(TokenKind::Whitespace(1))),
            Shared::new(token(TokenKind::Minus)),
            Shared::new(token(TokenKind::Whitespace(1))),
            Shared::new(token(TokenKind::Ident("z".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::BinaryOp { op: BinaryOp::Minus, lhs: Shared::new(Node {
                            kind: NodeKind::BinaryOp { op: BinaryOp::Plus, lhs: Shared::new(Node {
                                    kind: NodeKind::Ident { attr: None },
                                    token: Some(Shared::new(token(TokenKind::Ident("x".into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: smallvec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(1))))],
                                }), rhs: Shared::new(Node {
                                    kind: NodeKind::Ident { attr: None },
                                    token: Some(Shared::new(token(TokenKind::Ident("y".into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: smallvec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(1))))],
                                }) },
                            token: Some(Shared::new(token(TokenKind::Plus))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: smallvec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(1))))],
                        }), rhs: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("z".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }) },
                    token: Some(Shared::new(token(TokenKind::Minus))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: smallvec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(1))))],
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::string_literal_with_escape_sequences(
        vec![
            Shared::new(token(TokenKind::StringLiteral("\\x1b[2J\\x1b[H".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Literal,
                    token: Some(Shared::new(token(TokenKind::StringLiteral("\\x1b[2J\\x1b[H".into())))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::symbol_with_ident(
        vec![
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::Ident("foo".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Symbol { colon: Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Colon))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }), name: Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::Ident("foo".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }) },
                    token: None,
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::symbol_with_string(
        vec![
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::StringLiteral("bar".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Symbol { colon: Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Colon))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }), name: Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::StringLiteral("bar".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }) },
                    token: None,
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::not_operation(
        vec![
            Shared::new(token(TokenKind::Not)),
            Shared::new(token(TokenKind::BoolLiteral(true))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::UnaryOp { op: UnaryOp::Not, operand: Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::BoolLiteral(true)))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }) },
                    token: Some(Shared::new(token(TokenKind::Not))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::break_in_loop(
        vec![
            Shared::new(token(TokenKind::While)),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::BoolLiteral(true))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::Break)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::While { args: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::BoolLiteral(true)))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into(), colon_or_do: Some(Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Colon))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        })), program: vec![
Shared::new(Node {
                            kind: NodeKind::Break { colon: None, value: None },
                            token: Some(Shared::new(token(TokenKind::Break))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::While))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::break_in_loop_with_value(
        vec![
            Shared::new(token(TokenKind::While)),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::BoolLiteral(true))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::Break)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::BoolLiteral(false))),
            Shared::new(token(TokenKind::SemiColon)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::While { args: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::BoolLiteral(true)))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into(), colon_or_do: Some(Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Colon))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        })), program: vec![
Shared::new(Node {
                            kind: NodeKind::Break { colon: Some(Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::Colon))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                })), value: Some(Shared::new(Node {
                                    kind: NodeKind::Literal,
                                    token: Some(Shared::new(token(TokenKind::BoolLiteral(false)))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                })) },
                            token: Some(Shared::new(token(TokenKind::Break))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::SemiColon))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::While))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::continue_in_loop(
        vec![
            Shared::new(token(TokenKind::While)),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::BoolLiteral(true))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::Continue)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::While { args: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::BoolLiteral(true)))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into(), colon_or_do: Some(Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Colon))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        })), program: vec![
Shared::new(Node {
                            kind: NodeKind::Continue,
                            token: Some(Shared::new(token(TokenKind::Continue))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::While))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::break_outside_loop(
        vec![
            Shared::new(token(TokenKind::Break)),
        ],
        (
            vec![error_node(vec![TokenKind::Break])],
            ErrorReporter::with_error(vec![ParseError::UnexpectedToken(Shared::new(token(TokenKind::Break)))], 100)
        )
    )]
#[case::continue_outside_loop(
        vec![
            Shared::new(token(TokenKind::Continue)),
        ],
        (
            vec![error_node(vec![TokenKind::Continue])],
            ErrorReporter::with_error(vec![ParseError::UnexpectedToken(Shared::new(token(TokenKind::Continue)))], 100)
        )
    )]
#[case::bracket_access_with_number(
        vec![
            Shared::new(token(TokenKind::Ident("arr".into()))),
            Shared::new(token(TokenKind::LBracket)),
            Shared::new(token(TokenKind::NumberLiteral(5.into()))),
            Shared::new(token(TokenKind::RBracket)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Call { args: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LBracket))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::NumberLiteral(5.into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RBracket))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Ident("arr".into())))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::bracket_access_with_string(
        vec![
            Shared::new(token(TokenKind::Ident("dict".into()))),
            Shared::new(token(TokenKind::LBracket)),
            Shared::new(token(TokenKind::StringLiteral("key".into()))),
            Shared::new(token(TokenKind::RBracket)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Call { args: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LBracket))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::StringLiteral("key".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RBracket))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Ident("dict".into())))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::bracket_access_error_missing_rbracket(
        vec![
            Shared::new(token(TokenKind::Ident("arr".into()))),
            Shared::new(token(TokenKind::LBracket)),
            Shared::new(token(TokenKind::NumberLiteral(5.into()))),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Call { args: vec![
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LBracket))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::NumberLiteral(5.into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
                        missing_node("`]`"),
                    ].into() },
                    token: Some(Shared::new(token(TokenKind::Ident("arr".into())))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
                eof_node(),
            ],
            ErrorReporter::with_error(vec![ParseError::Missing { expected: "`]`", found: Shared::new(token(TokenKind::Eof)) }], 100)
        )
    )]
#[case::call_with_not_ident_arg(
        vec![
            Shared::new(token(TokenKind::Ident("foo".into()))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Not)),
            Shared::new(token(TokenKind::Ident("bar".into()))),
            Shared::new(token(TokenKind::RParen)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Call { args: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::UnaryOp { op: UnaryOp::Not, operand: Shared::new(Node {
                                    kind: NodeKind::Ident { attr: None },
                                    token: Some(Shared::new(token(TokenKind::Ident("bar".into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }) },
                            token: Some(Shared::new(token(TokenKind::Not))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Ident("foo".into())))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::slice_access_with_numbers(
        vec![
            Shared::new(token(TokenKind::Ident("arr".into()))),
            Shared::new(token(TokenKind::LBracket)),
            Shared::new(token(TokenKind::NumberLiteral(1.into()))),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::NumberLiteral(3.into()))),
            Shared::new(token(TokenKind::RBracket)),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Call { args: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LBracket))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::NumberLiteral(1.into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Colon))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::NumberLiteral(3.into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RBracket))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Ident("arr".into())))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::Eof,
                    token: Some(Shared::new(token(TokenKind::Eof))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::slice_access_with_variables(
        vec![
            Shared::new(token(TokenKind::Ident("items".into()))),
            Shared::new(token(TokenKind::LBracket)),
            Shared::new(token(TokenKind::Ident("start".into()))),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::Ident("end".into()))),
            Shared::new(token(TokenKind::RBracket)),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Call { args: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LBracket))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("start".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Colon))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("end".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RBracket))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Ident("items".into())))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::Eof,
                    token: Some(Shared::new(token(TokenKind::Eof))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::group_expr_single_ident(
        vec![
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("x".into()))),
            Shared::new(token(TokenKind::RParen)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Group { lparen: Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }), expr: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("x".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }), rparen: Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }) },
                    token: None,
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::group_expr_binary_op(
        vec![
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("a".into()))),
            Shared::new(token(TokenKind::Plus)),
            Shared::new(token(TokenKind::Ident("b".into()))),
            Shared::new(token(TokenKind::RParen)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Group { lparen: Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }), expr: Shared::new(Node {
                            kind: NodeKind::BinaryOp { op: BinaryOp::Plus, lhs: Shared::new(Node {
                                    kind: NodeKind::Ident { attr: None },
                                    token: Some(Shared::new(token(TokenKind::Ident("a".into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }), rhs: Shared::new(Node {
                                    kind: NodeKind::Ident { attr: None },
                                    token: Some(Shared::new(token(TokenKind::Ident("b".into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }) },
                            token: Some(Shared::new(token(TokenKind::Plus))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }), rparen: Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }) },
                    token: None,
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::group_expr_with_trivia(
        vec![
            Shared::new(token(TokenKind::Whitespace(2))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::NewLine)),
            Shared::new(token(TokenKind::Ident("x".into()))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Whitespace(1))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Group { lparen: Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }), expr: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("x".into())))),
                            leading_trivia: smallvec![Trivia::NewLine],
                            trailing_trivia: TriviaList::new(),
                        }), rparen: Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: smallvec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(1))))],
                        }) },
                    token: None,
                    leading_trivia: smallvec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(2))))],
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::group_expr_missing_rparen(
        vec![
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("x".into()))),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Group {
                        lparen: Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
                        expr: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("x".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
                        rparen: missing_node("`)`"),
                    },
                    token: None,
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
                eof_node(),
            ],
            ErrorReporter::with_error(vec![ParseError::Missing { expected: "`)`", found: Shared::new(token(TokenKind::Eof)) }], 100)
        )
    )]
// Test group expr with index access: (x)[0]
#[case::group_expr_with_index_access(
        vec![
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("x".into()))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::LBracket)),
            Shared::new(token(TokenKind::NumberLiteral(0.into()))),
            Shared::new(token(TokenKind::RBracket)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Call { args: vec![
Shared::new(Node {
                            kind: NodeKind::Group { lparen: Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::LParen))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }), expr: Shared::new(Node {
                                    kind: NodeKind::Ident { attr: None },
                                    token: Some(Shared::new(token(TokenKind::Ident("x".into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }), rparen: Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::RParen))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }) },
                            token: None,
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LBracket))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::NumberLiteral(0.into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RBracket))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: None,
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
// Test group expr with dynamic call: (x)("test")
#[case::group_expr_with_dynamic_call(
        vec![
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("x".into()))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::StringLiteral("test".into()))),
            Shared::new(token(TokenKind::RParen)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::CallDynamic { callee: Shared::new(Node {
                            kind: NodeKind::Group { lparen: Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::LParen))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }), expr: Shared::new(Node {
                                    kind: NodeKind::Ident { attr: None },
                                    token: Some(Shared::new(token(TokenKind::Ident("x".into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }), rparen: Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::RParen))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }) },
                            token: None,
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }), args: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::StringLiteral("test".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: None,
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
// Test array literal with index access: [1,2][0]
#[case::array_literal_with_index_access(
        vec![
            Shared::new(token(TokenKind::LBracket)),
            Shared::new(token(TokenKind::NumberLiteral(1.into()))),
            Shared::new(token(TokenKind::Comma)),
            Shared::new(token(TokenKind::NumberLiteral(2.into()))),
            Shared::new(token(TokenKind::RBracket)),
            Shared::new(token(TokenKind::LBracket)),
            Shared::new(token(TokenKind::NumberLiteral(0.into()))),
            Shared::new(token(TokenKind::RBracket)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Call { args: vec![
Shared::new(Node {
                            kind: NodeKind::Array { items: vec![
Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::LBracket))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }),
Shared::new(Node {
                                    kind: NodeKind::Literal,
                                    token: Some(Shared::new(token(TokenKind::NumberLiteral(1.into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }),
Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::Comma))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }),
Shared::new(Node {
                                    kind: NodeKind::Literal,
                                    token: Some(Shared::new(token(TokenKind::NumberLiteral(2.into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }),
Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::RBracket))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }),
].into() },
                            token: None,
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LBracket))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::NumberLiteral(0.into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RBracket))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: None,
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::if_with_grouped_binary_op_condition(
        vec![
            Shared::new(token(TokenKind::If)),
            Shared::new(token(TokenKind::Whitespace(1))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("a".into()))),
            Shared::new(token(TokenKind::Plus)),
            Shared::new(token(TokenKind::Ident("b".into()))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::Ident("then_branch".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::If { args: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Group { lparen: Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::LParen))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }), expr: Shared::new(Node {
                                    kind: NodeKind::BinaryOp { op: BinaryOp::Plus, lhs: Shared::new(Node {
                                            kind: NodeKind::Ident { attr: None },
                                            token: Some(Shared::new(token(TokenKind::Ident("a".into())))),
                                            leading_trivia: TriviaList::new(),
                                            trailing_trivia: TriviaList::new(),
                                        }), rhs: Shared::new(Node {
                                            kind: NodeKind::Ident { attr: None },
                                            token: Some(Shared::new(token(TokenKind::Ident("b".into())))),
                                            leading_trivia: TriviaList::new(),
                                            trailing_trivia: TriviaList::new(),
                                        }) },
                                    token: Some(Shared::new(token(TokenKind::Plus))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }), rparen: Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::RParen))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }) },
                            token: None,
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into(), colon: Some(Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Colon))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        })), then_branch: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("then_branch".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }), elifs: Elifs::new(), else_branch: None },
                    token: Some(Shared::new(token(TokenKind::If))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: smallvec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(1))))],
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::self_bracket_access_with_number(
        vec![
            Shared::new(token(TokenKind::Self_)),
            Shared::new(token(TokenKind::LBracket)),
            Shared::new(token(TokenKind::NumberLiteral(3.into()))),
            Shared::new(token(TokenKind::RBracket)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Call { args: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LBracket))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::NumberLiteral(3.into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RBracket))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Self_))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::call_with_index_access(
        vec![
            Shared::new(token(TokenKind::Ident("foo".into()))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("bar".into()))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::LBracket)),
            Shared::new(token(TokenKind::NumberLiteral(3.into()))),
            Shared::new(token(TokenKind::RBracket)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Call { args: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("bar".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LBracket))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::NumberLiteral(3.into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RBracket))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Ident("foo".into())))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::negate(
        vec![
            Shared::new(token(TokenKind::Minus)),
            Shared::new(token(TokenKind::NumberLiteral(42.into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::UnaryOp { op: UnaryOp::Negate, operand: Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::NumberLiteral(42.into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }) },
                    token: Some(Shared::new(token(TokenKind::Minus))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::do_block_simple(
        vec![
            Shared::new(token(TokenKind::Do)),
            Shared::new(token(TokenKind::Ident("test".into()))),
            Shared::new(token(TokenKind::End)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Block { program: vec![
Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("test".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::End,
                            token: Some(Shared::new(token(TokenKind::End))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Do))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::do_block_with_whitespace(
        vec![
            Shared::new(token(TokenKind::Whitespace(2))),
            Shared::new(token(TokenKind::Do)),
            Shared::new(token(TokenKind::NewLine)),
            Shared::new(token(TokenKind::Tab(1))),
            Shared::new(token(TokenKind::Ident("expression".into()))),
            Shared::new(token(TokenKind::NewLine)),
            Shared::new(token(TokenKind::End)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Block { program: vec![
Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("expression".into())))),
                            leading_trivia: smallvec![Trivia::NewLine, Trivia::Tab(Shared::new(token(TokenKind::Tab(1))))],
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::End,
                            token: Some(Shared::new(token(TokenKind::End))),
                            leading_trivia: smallvec![Trivia::NewLine],
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Do))),
                    leading_trivia: smallvec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(2))))],
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::do_block_with_comment(
        vec![
            Shared::new(token(TokenKind::Do)),
            Shared::new(token(TokenKind::Comment("inside block".into()))),
            Shared::new(token(TokenKind::NewLine)),
            Shared::new(token(TokenKind::Ident("value".into()))),
            Shared::new(token(TokenKind::End)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Block { program: vec![
Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("value".into())))),
                            leading_trivia: smallvec![Trivia::Comment(Shared::new(token(TokenKind::Comment("inside block".into())))), Trivia::NewLine],
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::End,
                            token: Some(Shared::new(token(TokenKind::End))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Do))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::do_block_empty(
        vec![
            Shared::new(token(TokenKind::Do)),
            Shared::new(token(TokenKind::End)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Block { program: smallvec![Shared::new(Node {
                        kind: NodeKind::End,
                        token: Some(Shared::new(token(TokenKind::End))),
                        leading_trivia: TriviaList::new(),
                        trailing_trivia: TriviaList::new(),
                    })] },
                    token: Some(Shared::new(token(TokenKind::Do))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::with_error(vec![ParseError::UnexpectedToken(Shared::new(token(TokenKind::End)))], 100)
        )
    )]
#[case::do_block_nested(
        vec![
            Shared::new(token(TokenKind::Do)),
            Shared::new(token(TokenKind::Do)),
            Shared::new(token(TokenKind::Ident("inner".into()))),
            Shared::new(token(TokenKind::End)),
            Shared::new(token(TokenKind::End)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Block { program: vec![
Shared::new(Node {
                            kind: NodeKind::Block { program: vec![
Shared::new(Node {
                                    kind: NodeKind::Ident { attr: None },
                                    token: Some(Shared::new(token(TokenKind::Ident("inner".into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }),
Shared::new(Node {
                                    kind: NodeKind::End,
                                    token: Some(Shared::new(token(TokenKind::End))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }),
].into() },
                            token: Some(Shared::new(token(TokenKind::Do))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::End,
                            token: Some(Shared::new(token(TokenKind::End))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Do))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::try_catch(
        vec![
            Shared::new(token(TokenKind::Try)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::Ident("try_body".into()))),
            Shared::new(token(TokenKind::Catch)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::Ident("catch_body".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Try { colon_or_do: Some(Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Colon))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        })), body: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("try_body".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }), catch: Some(Shared::new(Node {
                            kind: NodeKind::Catch { params: None, colon_or_do: Some(Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::Colon))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                })), body: Shared::new(Node {
                                    kind: NodeKind::Ident { attr: None },
                                    token: Some(Shared::new(token(TokenKind::Ident("catch_body".into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }) },
                            token: Some(Shared::new(token(TokenKind::Catch))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        })) },
                    token: Some(Shared::new(token(TokenKind::Try))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::try_without_catch(
        vec![
            Shared::new(token(TokenKind::Try)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::Ident("try_body".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Try { colon_or_do: Some(Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Colon))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        })), body: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("try_body".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }), catch: None },
                    token: Some(Shared::new(token(TokenKind::Try))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::call_with_question_mark(
        vec![
            Shared::new(token(TokenKind::Ident("foo".into()))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("bar".into()))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Question)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Call { args: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("bar".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Question))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Ident("foo".into())))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::match_simple(
        vec![
            Shared::new(token(TokenKind::Match)),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("x".into()))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::Pipe)),
            Shared::new(token(TokenKind::NumberLiteral(1.into()))),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::StringLiteral("one".into()))),
            Shared::new(token(TokenKind::Pipe)),
            Shared::new(token(TokenKind::Ident("_".into()))),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::StringLiteral("other".into()))),
            Shared::new(token(TokenKind::End)),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Match { args: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("x".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into(), colon_or_do: Some(Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Colon))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        })), arms: vec![
Shared::new(Node {
                            kind: NodeKind::MatchArm { pipe: Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::Pipe))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }), pattern: Shared::new(Node {
                                    kind: NodeKind::Pattern { items: ArgList::new() },
                                    token: Some(Shared::new(token(TokenKind::NumberLiteral(1.into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }), if_token: None, guard_args: None, colon: Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::Colon))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }), body: Shared::new(Node {
                                    kind: NodeKind::Literal,
                                    token: Some(Shared::new(token(TokenKind::StringLiteral("one".into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }) },
                            token: None,
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::MatchArm { pipe: Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::Pipe))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }), pattern: Shared::new(Node {
                                    kind: NodeKind::Pattern { items: ArgList::new() },
                                    token: Some(Shared::new(token(TokenKind::Ident("_".into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }), if_token: None, guard_args: None, colon: Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::Colon))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }), body: Shared::new(Node {
                                    kind: NodeKind::Literal,
                                    token: Some(Shared::new(token(TokenKind::StringLiteral("other".into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }) },
                            token: None,
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into(), end_token: Some(Shared::new(Node {
                            kind: NodeKind::End,
                            token: Some(Shared::new(token(TokenKind::End))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        })) },
                    token: Some(Shared::new(token(TokenKind::Match))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::Eof,
                    token: Some(Shared::new(token(TokenKind::Eof))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::match_do_end(
        vec![
            Shared::new(token(TokenKind::Match)),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("x".into()))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Do)),
            Shared::new(token(TokenKind::NewLine)),
            Shared::new(token(TokenKind::Pipe)),
            Shared::new(token(TokenKind::NumberLiteral(1.into()))),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::StringLiteral("one".into()))),
            Shared::new(token(TokenKind::NewLine)),
            Shared::new(token(TokenKind::Pipe)),
            Shared::new(token(TokenKind::Ident("_".into()))),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::StringLiteral("other".into()))),
            Shared::new(token(TokenKind::NewLine)),
            Shared::new(token(TokenKind::End)),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Match { args: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("x".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into(), colon_or_do: Some(Shared::new(Node {
                            kind: NodeKind::Do,
                            token: Some(Shared::new(token(TokenKind::Do))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        })), arms: vec![
Shared::new(Node {
                            kind: NodeKind::MatchArm { pipe: Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::Pipe))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }), pattern: Shared::new(Node {
                                    kind: NodeKind::Pattern { items: ArgList::new() },
                                    token: Some(Shared::new(token(TokenKind::NumberLiteral(1.into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }), if_token: None, guard_args: None, colon: Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::Colon))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }), body: Shared::new(Node {
                                    kind: NodeKind::Literal,
                                    token: Some(Shared::new(token(TokenKind::StringLiteral("one".into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }) },
                            token: None,
                            leading_trivia: smallvec![Trivia::NewLine],
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::MatchArm { pipe: Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::Pipe))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }), pattern: Shared::new(Node {
                                    kind: NodeKind::Pattern { items: ArgList::new() },
                                    token: Some(Shared::new(token(TokenKind::Ident("_".into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }), if_token: None, guard_args: None, colon: Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::Colon))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }), body: Shared::new(Node {
                                    kind: NodeKind::Literal,
                                    token: Some(Shared::new(token(TokenKind::StringLiteral("other".into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }) },
                            token: None,
                            leading_trivia: smallvec![Trivia::NewLine],
                            trailing_trivia: TriviaList::new(),
                        }),
].into(), end_token: Some(Shared::new(Node {
                            kind: NodeKind::End,
                            token: Some(Shared::new(token(TokenKind::End))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        })) },
                    token: Some(Shared::new(token(TokenKind::Match))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::Eof,
                    token: Some(Shared::new(token(TokenKind::Eof))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::index_access_with_function_call(
        vec![
            Shared::new(token(TokenKind::Ident("arr".into()))),
            Shared::new(token(TokenKind::LBracket)),
            Shared::new(token(TokenKind::NumberLiteral(0.into()))),
            Shared::new(token(TokenKind::RBracket)),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::RParen)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::CallDynamic { callee: Shared::new(Node {
                            kind: NodeKind::Call { args: vec![
Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::LBracket))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }),
Shared::new(Node {
                                    kind: NodeKind::Literal,
                                    token: Some(Shared::new(token(TokenKind::NumberLiteral(0.into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }),
Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::RBracket))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }),
].into() },
                            token: Some(Shared::new(token(TokenKind::Ident("arr".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }), args: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Ident("arr".into())))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::qualified_access_simple(
        vec![
            Shared::new(token(TokenKind::Ident("mod1".into()))),
            Shared::new(token(TokenKind::DoubleColon)),
            Shared::new(token(TokenKind::Ident("value".into()))),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::QualifiedAccess { items: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::DoubleColon))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("value".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Ident("mod1".into())))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::Eof,
                    token: Some(Shared::new(token(TokenKind::Eof))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::qualified_access_multi_level(
        vec![
            Shared::new(token(TokenKind::Ident("mod1".into()))),
            Shared::new(token(TokenKind::DoubleColon)),
            Shared::new(token(TokenKind::Ident("mod2".into()))),
            Shared::new(token(TokenKind::DoubleColon)),
            Shared::new(token(TokenKind::Ident("value".into()))),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::QualifiedAccess { items: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::DoubleColon))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("mod2".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::DoubleColon))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("value".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Ident("mod1".into())))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::Eof,
                    token: Some(Shared::new(token(TokenKind::Eof))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::qualified_access_with_call(
        vec![
            Shared::new(token(TokenKind::Ident("mod1".into()))),
            Shared::new(token(TokenKind::DoubleColon)),
            Shared::new(token(TokenKind::Ident("func".into()))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::StringLiteral("arg".into()))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::QualifiedAccess { items: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::DoubleColon))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("func".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::StringLiteral("arg".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Ident("mod1".into())))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::Eof,
                    token: Some(Shared::new(token(TokenKind::Eof))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::qualified_access_multi_level_with_call(
        vec![
            Shared::new(token(TokenKind::Ident("mod1".into()))),
            Shared::new(token(TokenKind::DoubleColon)),
            Shared::new(token(TokenKind::Ident("mod2".into()))),
            Shared::new(token(TokenKind::DoubleColon)),
            Shared::new(token(TokenKind::Ident("func".into()))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::QualifiedAccess { items: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::DoubleColon))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("mod2".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::DoubleColon))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("func".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Ident("mod1".into())))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::Eof,
                    token: Some(Shared::new(token(TokenKind::Eof))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::qualified_access::empty(
        vec![
            Shared::new(token(TokenKind::Ident("mod1".into()))),
            Shared::new(token(TokenKind::DoubleColon)),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::QualifiedAccess { items: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::DoubleColon))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Ident("mod1".into())))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::Eof,
                    token: Some(Shared::new(token(TokenKind::Eof))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::module_simple(
        vec![
            Shared::new(token(TokenKind::Module)),
            Shared::new(token(TokenKind::Ident("modname1".into()))),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::Let)),
            Shared::new(token(TokenKind::Ident("x".into()))),
            Shared::new(token(TokenKind::Equal)),
            Shared::new(token(TokenKind::NumberLiteral(1.into()))),
            Shared::new(token(TokenKind::Def)),
            Shared::new(token(TokenKind::Ident("foo".into()))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::StringLiteral("bar".into()))),
            Shared::new(token(TokenKind::End)),
            Shared::new(token(TokenKind::End)),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Module { name: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("modname1".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }), colon_or_do: Some(Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Colon))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        })), program: vec![
Shared::new(Node {
                            kind: NodeKind::Let { lhs: Shared::new(Node {
                                    kind: NodeKind::Ident { attr: None },
                                    token: Some(Shared::new(token(TokenKind::Ident("x".into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }), eq_token: Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::Equal))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }), rhs: Shared::new(Node {
                                    kind: NodeKind::Literal,
                                    token: Some(Shared::new(token(TokenKind::NumberLiteral(1.into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }) },
                            token: Some(Shared::new(token(TokenKind::Let))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Def { name: Shared::new(Node {
                                    kind: NodeKind::Ident { attr: None },
                                    token: Some(Shared::new(token(TokenKind::Ident("foo".into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }), params: Some(vec![
Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::LParen))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }),
Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::RParen))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }),
].into()), colon_or_do: Some(Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::Colon))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                })), program: vec![
Shared::new(Node {
                                    kind: NodeKind::Literal,
                                    token: Some(Shared::new(token(TokenKind::StringLiteral("bar".into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }),
Shared::new(Node {
                                    kind: NodeKind::End,
                                    token: Some(Shared::new(token(TokenKind::End))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }),
].into() },
                            token: Some(Shared::new(token(TokenKind::Def))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::End,
                            token: Some(Shared::new(token(TokenKind::End))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Module))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::Eof,
                    token: Some(Shared::new(token(TokenKind::Eof))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::slice_access_with_start_only(
        vec![
            Shared::new(token(TokenKind::Ident("arr".into()))),
            Shared::new(token(TokenKind::LBracket)),
            Shared::new(token(TokenKind::NumberLiteral(1.into()))),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::RBracket)),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Call { args: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LBracket))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::NumberLiteral(1.into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Colon))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RBracket))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Ident("arr".into())))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::Eof,
                    token: Some(Shared::new(token(TokenKind::Eof))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::selector_existing_and_non_existing(
        vec![
            Shared::new(token(TokenKind::Selector(".h".into()))),
            Shared::new(token(TokenKind::Selector(".value".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Selector { items: vec![
Shared::new(Node {
                            kind: NodeKind::Selector { items: ArgList::new() },
                            token: Some(Shared::new(token(TokenKind::Selector(".value".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Selector(".h".into())))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::selector_property(
        vec![
            Shared::new(token(TokenKind::Selector(".\"notfound\"".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Selector { items: ArgList::new() },
                    token: Some(Shared::new(token(TokenKind::Selector(".\"notfound\"".into())))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::selector_property_chained(
        vec![
            Shared::new(token(TokenKind::Selector(".\"a\"".into()))),
            Shared::new(token(TokenKind::Selector(".\"b\"".into()))),
            Shared::new(token(TokenKind::Selector(".\"c\"".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Block { program: vec![
Shared::new(Node {
                            kind: NodeKind::Selector { items: ArgList::new() },
                            token: Some(Shared::new(token(TokenKind::Selector(".\"a\"".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Selector { items: ArgList::new() },
                            token: Some(Shared::new(token(TokenKind::Selector(".\"b\"".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Selector { items: ArgList::new() },
                            token: Some(Shared::new(token(TokenKind::Selector(".\"c\"".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Selector(".\"a\"".into())))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::selector_property_chained_two(
        vec![
            Shared::new(token(TokenKind::Selector(".\"foo\"".into()))),
            Shared::new(token(TokenKind::Selector(".\"bar\"".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Block { program: vec![
Shared::new(Node {
                            kind: NodeKind::Selector { items: ArgList::new() },
                            token: Some(Shared::new(token(TokenKind::Selector(".\"foo\"".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Selector { items: ArgList::new() },
                            token: Some(Shared::new(token(TokenKind::Selector(".\"bar\"".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Selector(".\"foo\"".into())))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::selector_property_iterator(
        // ."items"[] → Block([Selector(."items"), Selector(. + [[ + ]]))]
        vec![
            Shared::new(token(TokenKind::Selector(".\"items\"".into()))),
            Shared::new(token(TokenKind::LBracket)),
            Shared::new(token(TokenKind::RBracket)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Block { program: vec![
Shared::new(Node {
                            kind: NodeKind::Selector { items: ArgList::new() },
                            token: Some(Shared::new(token(TokenKind::Selector(".\"items\"".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Selector { items: vec![
Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::LBracket))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }),
Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::RBracket))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }),
].into() },
                            token: Some(Shared::new(Token {
                                kind: TokenKind::Selector(".".into()),
                                range: Range::default(),
                                module_id: 1.into(),
                            })),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Selector(".\"items\"".into())))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::selector_property_iterator_with_index(
        // ."items"[0] → Block([Selector(."items"), Selector(. + [[ + 0 + ]]))]
        vec![
            Shared::new(token(TokenKind::Selector(".\"items\"".into()))),
            Shared::new(token(TokenKind::LBracket)),
            Shared::new(token(TokenKind::NumberLiteral(0.0.into()))),
            Shared::new(token(TokenKind::RBracket)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Block { program: vec![
Shared::new(Node {
                            kind: NodeKind::Selector { items: ArgList::new() },
                            token: Some(Shared::new(token(TokenKind::Selector(".\"items\"".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Selector { items: vec![
Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::LBracket))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }),
Shared::new(Node {
                                    kind: NodeKind::Literal,
                                    token: Some(Shared::new(token(TokenKind::NumberLiteral(0.0.into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }),
Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::RBracket))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }),
].into() },
                            token: Some(Shared::new(Token {
                                kind: TokenKind::Selector(".".into()),
                                range: Range::default(),
                                module_id: 1.into(),
                            })),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Selector(".\"items\"".into())))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::call_with_selector_attribute(
        vec![
            Shared::new(token(TokenKind::Ident("foo".into()))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("bar".into()))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Selector(".level".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Call { args: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("bar".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Selector { items: ArgList::new() },
                            token: Some(Shared::new(token(TokenKind::Selector(".level".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Ident("foo".into())))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[rstest]
#[case::selector_dot_followed_by_pipe(
        vec![
            Shared::new(token(TokenKind::Selector(".".into()))),
            Shared::new(token(TokenKind::Pipe)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Self_ { attr: None },
                    token: Some(Shared::new(token(TokenKind::Selector(".".into())))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::Token,
                    token: Some(Shared::new(token(TokenKind::Pipe))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::selector_dot_followed_by_semicolon(
        vec![
            Shared::new(token(TokenKind::Selector(".".into()))),
            Shared::new(token(TokenKind::SemiColon)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Self_ { attr: None },
                    token: Some(Shared::new(token(TokenKind::Selector(".".into())))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::Token,
                    token: Some(Shared::new(token(TokenKind::SemiColon))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::selector_dot_followed_by_end(
        vec![
            Shared::new(token(TokenKind::Selector(".".into()))),
            Shared::new(token(TokenKind::End)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Self_ { attr: None },
                    token: Some(Shared::new(token(TokenKind::Selector(".".into())))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::End,
                    token: Some(Shared::new(token(TokenKind::End))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::selector_dot_followed_by_binary_op(
        vec![
            Shared::new(token(TokenKind::Selector(".".into()))),
            Shared::new(token(TokenKind::Plus)),
            Shared::new(token(TokenKind::Ident("x".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::BinaryOp { op: BinaryOp::Plus, lhs: Shared::new(Node {
                            kind: NodeKind::Self_ { attr: None },
                            token: Some(Shared::new(token(TokenKind::Selector(".".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }), rhs: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("x".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }) },
                    token: Some(Shared::new(token(TokenKind::Plus))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::let_followed_by_let_without_pipe(
        vec![
            Shared::new(token(TokenKind::Let)),
            Shared::new(token(TokenKind::Ident("x".into()))),
            Shared::new(token(TokenKind::Equal)),
            Shared::new(token(TokenKind::NumberLiteral(1.into()))),
            Shared::new(token(TokenKind::Let)),
            Shared::new(token(TokenKind::Ident("y".into()))),
            Shared::new(token(TokenKind::Equal)),
            Shared::new(token(TokenKind::NumberLiteral(2.into()))),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Let { lhs: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("x".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }), eq_token: Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Equal))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }), rhs: Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::NumberLiteral(1.into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }) },
                    token: Some(Shared::new(token(TokenKind::Let))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::Let { lhs: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("y".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }), eq_token: Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Equal))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }), rhs: Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::NumberLiteral(2.into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }) },
                    token: Some(Shared::new(token(TokenKind::Let))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
                eof_node(),
            ],
            ErrorReporter::default()
        )
    )]
#[case::call_with_do_block_argument(
        vec![
            Shared::new(token(TokenKind::Ident("foo".into()))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Do)),
            Shared::new(token(TokenKind::Ident("bar".into()))),
            Shared::new(token(TokenKind::End)),
            Shared::new(token(TokenKind::RParen)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Call { args: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Block { program: vec![
Shared::new(Node {
                                    kind: NodeKind::Ident { attr: None },
                                    token: Some(Shared::new(token(TokenKind::Ident("bar".into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }),
Shared::new(Node {
                                    kind: NodeKind::End,
                                    token: Some(Shared::new(token(TokenKind::End))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }),
].into() },
                            token: Some(Shared::new(token(TokenKind::Do))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Ident("foo".into())))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::call_with_implicit_pipeline_arg(
        vec![
            Shared::new(token(TokenKind::Ident("foo".into()))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("bar".into()))),
            Shared::new(token(TokenKind::Whitespace(1))),
            Shared::new(token(TokenKind::Pipe)),
            Shared::new(token(TokenKind::Whitespace(1))),
            Shared::new(token(TokenKind::Ident("baz".into()))),
            Shared::new(token(TokenKind::RParen)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Call { args: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Block { program: vec![
Shared::new(Node {
                                    kind: NodeKind::Ident { attr: None },
                                    token: Some(Shared::new(token(TokenKind::Ident("bar".into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: smallvec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(1))))],
                                }),
Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::Pipe))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: smallvec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(1))))],
                                }),
Shared::new(Node {
                                    kind: NodeKind::Ident { attr: None },
                                    token: Some(Shared::new(token(TokenKind::Ident("baz".into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }),
].into() },
                            token: None,
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Ident("foo".into())))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::call_with_implicit_pipeline_arg_multi_step(
        vec![
            Shared::new(token(TokenKind::Ident("foo".into()))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("a".into()))),
            Shared::new(token(TokenKind::Pipe)),
            Shared::new(token(TokenKind::Ident("b".into()))),
            Shared::new(token(TokenKind::Pipe)),
            Shared::new(token(TokenKind::Ident("c".into()))),
            Shared::new(token(TokenKind::RParen)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Call { args: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Block { program: vec![
Shared::new(Node {
                                    kind: NodeKind::Ident { attr: None },
                                    token: Some(Shared::new(token(TokenKind::Ident("a".into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }),
Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::Pipe))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }),
Shared::new(Node {
                                    kind: NodeKind::Ident { attr: None },
                                    token: Some(Shared::new(token(TokenKind::Ident("b".into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }),
Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::Pipe))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }),
Shared::new(Node {
                                    kind: NodeKind::Ident { attr: None },
                                    token: Some(Shared::new(token(TokenKind::Ident("c".into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }),
].into() },
                            token: None,
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Ident("foo".into())))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::call_with_implicit_pipeline_first_of_multiple_args(
        vec![
            Shared::new(token(TokenKind::Ident("foo".into()))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("a".into()))),
            Shared::new(token(TokenKind::Pipe)),
            Shared::new(token(TokenKind::Ident("b".into()))),
            Shared::new(token(TokenKind::Comma)),
            Shared::new(token(TokenKind::Ident("c".into()))),
            Shared::new(token(TokenKind::RParen)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Call { args: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Block { program: vec![
Shared::new(Node {
                                    kind: NodeKind::Ident { attr: None },
                                    token: Some(Shared::new(token(TokenKind::Ident("a".into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }),
Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::Pipe))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }),
Shared::new(Node {
                                    kind: NodeKind::Ident { attr: None },
                                    token: Some(Shared::new(token(TokenKind::Ident("b".into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }),
].into() },
                            token: None,
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Comma))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("c".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Ident("foo".into())))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::call_with_implicit_pipeline_second_arg(
        // foo(x, a | b): pipeline is the second argument
        vec![
            Shared::new(token(TokenKind::Ident("foo".into()))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("x".into()))),
            Shared::new(token(TokenKind::Comma)),
            Shared::new(token(TokenKind::Ident("a".into()))),
            Shared::new(token(TokenKind::Pipe)),
            Shared::new(token(TokenKind::Ident("b".into()))),
            Shared::new(token(TokenKind::RParen)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Call { args: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("x".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Comma))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Block { program: vec![
Shared::new(Node {
                                    kind: NodeKind::Ident { attr: None },
                                    token: Some(Shared::new(token(TokenKind::Ident("a".into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }),
Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::Pipe))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }),
Shared::new(Node {
                                    kind: NodeKind::Ident { attr: None },
                                    token: Some(Shared::new(token(TokenKind::Ident("b".into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }),
].into() },
                            token: None,
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Ident("foo".into())))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::call_with_implicit_pipeline_multiline(
        // foo(a\n| b): pipeline spanning two lines
        vec![
            Shared::new(token(TokenKind::Ident("foo".into()))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("a".into()))),
            Shared::new(token(TokenKind::NewLine)),
            Shared::new(token(TokenKind::Pipe)),
            Shared::new(token(TokenKind::Whitespace(1))),
            Shared::new(token(TokenKind::Ident("b".into()))),
            Shared::new(token(TokenKind::RParen)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Call { args: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Block { program: vec![
Shared::new(Node {
                                    kind: NodeKind::Ident { attr: None },
                                    token: Some(Shared::new(token(TokenKind::Ident("a".into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }),
Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::Pipe))),
                                    leading_trivia: smallvec![Trivia::NewLine],
                                    trailing_trivia: smallvec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(1))))],
                                }),
Shared::new(Node {
                                    kind: NodeKind::Ident { attr: None },
                                    token: Some(Shared::new(token(TokenKind::Ident("b".into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }),
].into() },
                            token: None,
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Ident("foo".into())))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::call_with_implicit_pipeline_call_exprs(
        // foo(bar() | baz()): function calls inside pipeline
        vec![
            Shared::new(token(TokenKind::Ident("foo".into()))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("bar".into()))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Whitespace(1))),
            Shared::new(token(TokenKind::Pipe)),
            Shared::new(token(TokenKind::Whitespace(1))),
            Shared::new(token(TokenKind::Ident("baz".into()))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::RParen)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Call { args: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Block { program: vec![
Shared::new(Node {
                                    kind: NodeKind::Call { args: vec![
Shared::new(Node {
                                            kind: NodeKind::Token,
                                            token: Some(Shared::new(token(TokenKind::LParen))),
                                            leading_trivia: TriviaList::new(),
                                            trailing_trivia: TriviaList::new(),
                                        }),
Shared::new(Node {
                                            kind: NodeKind::Token,
                                            token: Some(Shared::new(token(TokenKind::RParen))),
                                            leading_trivia: TriviaList::new(),
                                            trailing_trivia: smallvec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(1))))],
                                        }),
].into() },
                                    token: Some(Shared::new(token(TokenKind::Ident("bar".into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }),
Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::Pipe))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: smallvec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(1))))],
                                }),
Shared::new(Node {
                                    kind: NodeKind::Call { args: vec![
Shared::new(Node {
                                            kind: NodeKind::Token,
                                            token: Some(Shared::new(token(TokenKind::LParen))),
                                            leading_trivia: TriviaList::new(),
                                            trailing_trivia: TriviaList::new(),
                                        }),
Shared::new(Node {
                                            kind: NodeKind::Token,
                                            token: Some(Shared::new(token(TokenKind::RParen))),
                                            leading_trivia: TriviaList::new(),
                                            trailing_trivia: TriviaList::new(),
                                        }),
].into() },
                                    token: Some(Shared::new(token(TokenKind::Ident("baz".into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }),
].into() },
                            token: None,
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Ident("foo".into())))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::call_with_try_argument(
        vec![
            Shared::new(token(TokenKind::Ident("foo".into()))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Try)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::Ident("bar".into()))),
            Shared::new(token(TokenKind::RParen)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Call { args: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Try { colon_or_do: Some(Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::Colon))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                })), body: Shared::new(Node {
                                    kind: NodeKind::Ident { attr: None },
                                    token: Some(Shared::new(token(TokenKind::Ident("bar".into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }), catch: None },
                            token: Some(Shared::new(token(TokenKind::Try))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Ident("foo".into())))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::call_with_try_catch_argument(
        vec![
            Shared::new(token(TokenKind::Ident("foo".into()))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Try)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::Ident("bar".into()))),
            Shared::new(token(TokenKind::Catch)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::Ident("baz".into()))),
            Shared::new(token(TokenKind::RParen)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Call { args: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Try { colon_or_do: Some(Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::Colon))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                })), body: Shared::new(Node {
                                    kind: NodeKind::Ident { attr: None },
                                    token: Some(Shared::new(token(TokenKind::Ident("bar".into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }), catch: Some(Shared::new(Node {
                                    kind: NodeKind::Catch { params: None, colon_or_do: Some(Shared::new(Node {
                                            kind: NodeKind::Token,
                                            token: Some(Shared::new(token(TokenKind::Colon))),
                                            leading_trivia: TriviaList::new(),
                                            trailing_trivia: TriviaList::new(),
                                        })), body: Shared::new(Node {
                                            kind: NodeKind::Ident { attr: None },
                                            token: Some(Shared::new(token(TokenKind::Ident("baz".into())))),
                                            leading_trivia: TriviaList::new(),
                                            trailing_trivia: TriviaList::new(),
                                        }) },
                                    token: Some(Shared::new(token(TokenKind::Catch))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                })) },
                            token: Some(Shared::new(token(TokenKind::Try))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Ident("foo".into())))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::assign(
        vec![
            Shared::new(token(TokenKind::Ident("x".into()))),
            Shared::new(token(TokenKind::Equal)),
            Shared::new(token(TokenKind::NumberLiteral(10.into()))),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Assign { op: BinaryOp::Assign, lhs: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("x".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }), rhs: Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::NumberLiteral(10.into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }) },
                    token: Some(Shared::new(token(TokenKind::Equal))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::Eof,
                    token: Some(Shared::new(token(TokenKind::Eof))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::plus_equal(
        vec![
            Shared::new(token(TokenKind::Ident("x".into()))),
            Shared::new(token(TokenKind::PlusEqual)),
            Shared::new(token(TokenKind::NumberLiteral(5.into()))),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Assign { op: BinaryOp::PlusEqual, lhs: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("x".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }), rhs: Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::NumberLiteral(5.into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }) },
                    token: Some(Shared::new(token(TokenKind::PlusEqual))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::Eof,
                    token: Some(Shared::new(token(TokenKind::Eof))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::minus_equal(
        vec![
            Shared::new(token(TokenKind::Ident("y".into()))),
            Shared::new(token(TokenKind::MinusEqual)),
            Shared::new(token(TokenKind::NumberLiteral(3.into()))),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Assign { op: BinaryOp::MinusEqual, lhs: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("y".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }), rhs: Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::NumberLiteral(3.into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }) },
                    token: Some(Shared::new(token(TokenKind::MinusEqual))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::Eof,
                    token: Some(Shared::new(token(TokenKind::Eof))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::star_equal(
        vec![
            Shared::new(token(TokenKind::Ident("z".into()))),
            Shared::new(token(TokenKind::StarEqual)),
            Shared::new(token(TokenKind::NumberLiteral(2.into()))),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Assign { op: BinaryOp::MultiplicationEqual, lhs: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("z".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }), rhs: Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::NumberLiteral(2.into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }) },
                    token: Some(Shared::new(token(TokenKind::StarEqual))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::Eof,
                    token: Some(Shared::new(token(TokenKind::Eof))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::slash_equal(
        vec![
            Shared::new(token(TokenKind::Ident("a".into()))),
            Shared::new(token(TokenKind::SlashEqual)),
            Shared::new(token(TokenKind::NumberLiteral(4.into()))),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Assign { op: BinaryOp::DivisionEqual, lhs: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("a".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }), rhs: Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::NumberLiteral(4.into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }) },
                    token: Some(Shared::new(token(TokenKind::SlashEqual))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::Eof,
                    token: Some(Shared::new(token(TokenKind::Eof))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::percent_equal(
        vec![
            Shared::new(token(TokenKind::Ident("b".into()))),
            Shared::new(token(TokenKind::PercentEqual)),
            Shared::new(token(TokenKind::NumberLiteral(7.into()))),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Assign { op: BinaryOp::ModuloEqual, lhs: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("b".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }), rhs: Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::NumberLiteral(7.into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }) },
                    token: Some(Shared::new(token(TokenKind::PercentEqual))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::Eof,
                    token: Some(Shared::new(token(TokenKind::Eof))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::double_slash_equal(
        vec![
            Shared::new(token(TokenKind::Ident("c".into()))),
            Shared::new(token(TokenKind::DoubleSlashEqual)),
            Shared::new(token(TokenKind::NumberLiteral(3.into()))),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Assign { op: BinaryOp::DoubleDivisionEqual, lhs: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("c".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }), rhs: Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::NumberLiteral(3.into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }) },
                    token: Some(Shared::new(token(TokenKind::DoubleSlashEqual))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::Eof,
                    token: Some(Shared::new(token(TokenKind::Eof))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::pipe_equal_with_selector(
        vec![
            Shared::new(token(TokenKind::Selector(".h1".into()))),
            Shared::new(token(TokenKind::Selector(".value".into()))),
            Shared::new(token(TokenKind::PipeEqual)),
            Shared::new(token(TokenKind::StringLiteral("new_id".into()))),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Assign { op: BinaryOp::PipeEqual, lhs: Shared::new(Node {
                            kind: NodeKind::Selector { items: vec![
Shared::new(Node {
                                    kind: NodeKind::Selector { items: ArgList::new() },
                                    token: Some(Shared::new(token(TokenKind::Selector(".value".into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }),
].into() },
                            token: Some(Shared::new(token(TokenKind::Selector(".h1".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }), rhs: Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::StringLiteral("new_id".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }) },
                    token: Some(Shared::new(token(TokenKind::PipeEqual))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::Eof,
                    token: Some(Shared::new(token(TokenKind::Eof))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::pipe_equal_with_ident_and_attr(
        vec![
            Shared::new(token(TokenKind::Ident("obj".into()))),
            Shared::new(token(TokenKind::Selector(".value".into()))),
            Shared::new(token(TokenKind::PipeEqual)),
            Shared::new(token(TokenKind::StringLiteral("John".into()))),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Assign { op: BinaryOp::PipeEqual, lhs: Shared::new(Node {
                            kind: NodeKind::Ident { attr: Some(Shared::new(Node {
                                    kind: NodeKind::Selector { items: ArgList::new() },
                                    token: Some(Shared::new(token(TokenKind::Selector(".value".into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                })) },
                            token: Some(Shared::new(token(TokenKind::Ident("obj".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }), rhs: Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::StringLiteral("John".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }) },
                    token: Some(Shared::new(token(TokenKind::PipeEqual))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::Eof,
                    token: Some(Shared::new(token(TokenKind::Eof))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::pipe_equal_with_self(
        vec![
            Shared::new(token(TokenKind::Selector(".".into()))),
            Shared::new(token(TokenKind::PipeEqual)),
            Shared::new(token(TokenKind::NumberLiteral(42.into()))),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Assign { op: BinaryOp::PipeEqual, lhs: Shared::new(Node {
                            kind: NodeKind::Self_ { attr: None },
                            token: Some(Shared::new(token(TokenKind::Selector(".".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }), rhs: Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::NumberLiteral(42.into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }) },
                    token: Some(Shared::new(token(TokenKind::PipeEqual))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::Eof,
                    token: Some(Shared::new(token(TokenKind::Eof))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::var_then_assign(
        vec![
            Shared::new(token(TokenKind::Var)),
            Shared::new(token(TokenKind::Ident("count".into()))),
            Shared::new(token(TokenKind::Equal)),
            Shared::new(token(TokenKind::NumberLiteral(0.into()))),
            Shared::new(token(TokenKind::Pipe)),
            Shared::new(token(TokenKind::Ident("count".into()))),
            Shared::new(token(TokenKind::Equal)),
            Shared::new(token(TokenKind::NumberLiteral(10.into()))),
            Shared::new(token(TokenKind::Pipe)),
            Shared::new(token(TokenKind::Ident("count".into()))),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Var { lhs: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("count".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }), eq_token: Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Equal))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }), rhs: Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::NumberLiteral(0.into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }) },
                    token: Some(Shared::new(token(TokenKind::Var))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::Token,
                    token: Some(Shared::new(token(TokenKind::Pipe))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::Assign { op: BinaryOp::Assign, lhs: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("count".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }), rhs: Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::NumberLiteral(10.into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }) },
                    token: Some(Shared::new(token(TokenKind::Equal))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::Token,
                    token: Some(Shared::new(token(TokenKind::Pipe))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::Ident { attr: None },
                    token: Some(Shared::new(token(TokenKind::Ident("count".into())))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::Eof,
                    token: Some(Shared::new(token(TokenKind::Eof))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::index_assign(
        vec![
            Shared::new(token(TokenKind::Ident("arr".into()))),
            Shared::new(token(TokenKind::LBracket)),
            Shared::new(token(TokenKind::NumberLiteral(0.into()))),
            Shared::new(token(TokenKind::RBracket)),
            Shared::new(token(TokenKind::Equal)),
            Shared::new(token(TokenKind::NumberLiteral(10.into()))),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Assign { op: BinaryOp::Assign, lhs: Shared::new(Node {
                            kind: NodeKind::Call { args: vec![
Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::LBracket))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }),
Shared::new(Node {
                                    kind: NodeKind::Literal,
                                    token: Some(Shared::new(token(TokenKind::NumberLiteral(0.into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }),
Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::RBracket))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }),
].into() },
                            token: Some(Shared::new(token(TokenKind::Ident("arr".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }), rhs: Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::NumberLiteral(10.into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }) },
                    token: Some(Shared::new(token(TokenKind::Equal))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::Eof,
                    token: Some(Shared::new(token(TokenKind::Eof))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::index_compound_assign(
        vec![
            Shared::new(token(TokenKind::Ident("arr".into()))),
            Shared::new(token(TokenKind::LBracket)),
            Shared::new(token(TokenKind::NumberLiteral(0.into()))),
            Shared::new(token(TokenKind::RBracket)),
            Shared::new(token(TokenKind::PlusEqual)),
            Shared::new(token(TokenKind::NumberLiteral(1.into()))),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Assign { op: BinaryOp::PlusEqual, lhs: Shared::new(Node {
                            kind: NodeKind::Call { args: vec![
Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::LBracket))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }),
Shared::new(Node {
                                    kind: NodeKind::Literal,
                                    token: Some(Shared::new(token(TokenKind::NumberLiteral(0.into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }),
Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::RBracket))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }),
].into() },
                            token: Some(Shared::new(token(TokenKind::Ident("arr".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }), rhs: Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::NumberLiteral(1.into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }) },
                    token: Some(Shared::new(token(TokenKind::PlusEqual))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::Eof,
                    token: Some(Shared::new(token(TokenKind::Eof))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::def_with_default_param(
        vec![
            Shared::new(token(TokenKind::Def)),
            Shared::new(token(TokenKind::Ident("greet".into()))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("name".into()))),
            Shared::new(token(TokenKind::Comma)),
            Shared::new(token(TokenKind::Ident("greeting".into()))),
            Shared::new(token(TokenKind::Equal)),
            Shared::new(token(TokenKind::StringLiteral("Hello".into()))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::Ident("greeting".into()))),
            Shared::new(token(TokenKind::SemiColon)),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Def { name: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("greet".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }), params: Some(vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Param { asterisk: None, eq_token: None, default: None },
                            token: Some(Shared::new(token(TokenKind::Ident("name".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Comma))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Param { asterisk: None, eq_token: Some(Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::Equal))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                })), default: Some(Shared::new(Node {
                                    kind: NodeKind::Literal,
                                    token: Some(Shared::new(token(TokenKind::StringLiteral("Hello".into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                })) },
                            token: Some(Shared::new(token(TokenKind::Ident("greeting".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into()), colon_or_do: Some(Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Colon))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        })), program: vec![
Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("greeting".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::SemiColon))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Def))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::Eof,
                    token: Some(Shared::new(token(TokenKind::Eof))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::fn_with_multiple_default_params(
        vec![
            Shared::new(token(TokenKind::Fn)),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("a".into()))),
            Shared::new(token(TokenKind::Comma)),
            Shared::new(token(TokenKind::Ident("b".into()))),
            Shared::new(token(TokenKind::Equal)),
            Shared::new(token(TokenKind::NumberLiteral(2.into()))),
            Shared::new(token(TokenKind::Comma)),
            Shared::new(token(TokenKind::Ident("c".into()))),
            Shared::new(token(TokenKind::Equal)),
            Shared::new(token(TokenKind::NumberLiteral(3.into()))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::Ident("a".into()))),
            Shared::new(token(TokenKind::SemiColon)),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Fn { params: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Param { asterisk: None, eq_token: None, default: None },
                            token: Some(Shared::new(token(TokenKind::Ident("a".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Comma))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Param { asterisk: None, eq_token: Some(Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::Equal))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                })), default: Some(Shared::new(Node {
                                    kind: NodeKind::Literal,
                                    token: Some(Shared::new(token(TokenKind::NumberLiteral(2.into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                })) },
                            token: Some(Shared::new(token(TokenKind::Ident("b".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Comma))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Param { asterisk: None, eq_token: Some(Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::Equal))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                })), default: Some(Shared::new(Node {
                                    kind: NodeKind::Literal,
                                    token: Some(Shared::new(token(TokenKind::NumberLiteral(3.into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                })) },
                            token: Some(Shared::new(token(TokenKind::Ident("c".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into(), colon_or_do: Some(Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Colon))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        })), program: vec![
Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("a".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::SemiColon))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Fn))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::Eof,
                    token: Some(Shared::new(token(TokenKind::Eof))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::arrow_with_program(
        vec![
            Shared::new(token(TokenKind::Arrow)),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::Ident("x".into()))),
            Shared::new(token(TokenKind::SemiColon)),
            Shared::new(token(TokenKind::Pipe)),
            Shared::new(token(TokenKind::Ident("y".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Fn { params: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into(), colon_or_do: Some(Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Colon))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        })), program: vec![
Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("x".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::SemiColon))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Arrow))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::Token,
                    token: Some(Shared::new(token(TokenKind::Pipe))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::Ident { attr: None },
                    token: Some(Shared::new(token(TokenKind::Ident("y".into())))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::arrow_with_parameter_and_program(
        vec![
            Shared::new(token(TokenKind::Arrow)),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("param".into()))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::Ident("body".into()))),
            Shared::new(token(TokenKind::SemiColon)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Fn { params: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Param { asterisk: None, eq_token: None, default: None },
                            token: Some(Shared::new(token(TokenKind::Ident("param".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into(), colon_or_do: Some(Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Colon))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        })), program: vec![
Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("body".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::SemiColon))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Arrow))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::arrow_with_multiple_parameters(
        vec![
            Shared::new(token(TokenKind::Arrow)),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("param1".into()))),
            Shared::new(token(TokenKind::Comma)),
            Shared::new(token(TokenKind::Ident("param2".into()))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::StringLiteral("result".into()))),
            Shared::new(token(TokenKind::SemiColon)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Fn { params: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Param { asterisk: None, eq_token: None, default: None },
                            token: Some(Shared::new(token(TokenKind::Ident("param1".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Comma))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Param { asterisk: None, eq_token: None, default: None },
                            token: Some(Shared::new(token(TokenKind::Ident("param2".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into(), colon_or_do: Some(Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Colon))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        })), program: vec![
Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::StringLiteral("result".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::SemiColon))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Arrow))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::def_with_variadic_param(
        vec![
            Shared::new(token(TokenKind::Def)),
            Shared::new(token(TokenKind::Ident("f".into()))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Asterisk)),
            Shared::new(token(TokenKind::Ident("args".into()))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::Ident("args".into()))),
            Shared::new(token(TokenKind::SemiColon)),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Def { name: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("f".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }), params: Some(vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
// Variadic param: Ident node with Asterisk token as single child
                        Shared::new(Node {
                            kind: NodeKind::Param { asterisk: Some(Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::Asterisk))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                })), eq_token: None, default: None },
                            token: Some(Shared::new(token(TokenKind::Ident("args".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into()), colon_or_do: Some(Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Colon))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        })), program: vec![
Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("args".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::SemiColon))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Def))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::Eof,
                    token: Some(Shared::new(token(TokenKind::Eof))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::def_without_params(
        vec![
            Shared::new(token(TokenKind::Def)),
            Shared::new(token(TokenKind::Ident("f".into()))),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::Ident("args".into()))),
            Shared::new(token(TokenKind::SemiColon)),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Def { name: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("f".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }), params: None, colon_or_do: Some(Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Colon))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        })), program: vec![
Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("args".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::SemiColon))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Def))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::Eof,
                    token: Some(Shared::new(token(TokenKind::Eof))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::def_without_params_with_do(
        vec![
            Shared::new(token(TokenKind::Def)),
            Shared::new(token(TokenKind::Ident("f".into()))),
            Shared::new(token(TokenKind::Do)),
            Shared::new(token(TokenKind::Ident("args".into()))),
            Shared::new(token(TokenKind::SemiColon)),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Def { name: Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("f".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }), params: None, colon_or_do: Some(Shared::new(Node {
                            kind: NodeKind::Do,
                            token: Some(Shared::new(token(TokenKind::Do))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        })), program: vec![
Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("args".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::SemiColon))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Def))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::Eof,
                    token: Some(Shared::new(token(TokenKind::Eof))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::selector_call_heading_single_arg(
        vec![
            Shared::new(token(TokenKind::Selector(".h".into()))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::NumberLiteral(1.into()))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::SelectorCall { args: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::NumberLiteral(1.into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Selector(".h".into())))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::Eof,
                    token: Some(Shared::new(token(TokenKind::Eof))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::selector_call_heading_multi_arg(
        vec![
            Shared::new(token(TokenKind::Selector(".h".into()))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::NumberLiteral(1.into()))),
            Shared::new(token(TokenKind::Comma)),
            Shared::new(token(TokenKind::NumberLiteral(2.into()))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::SelectorCall { args: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::NumberLiteral(1.into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Comma))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::NumberLiteral(2.into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Selector(".h".into())))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::Eof,
                    token: Some(Shared::new(token(TokenKind::Eof))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::selector_call_code_lang(
        vec![
            Shared::new(token(TokenKind::Selector(".code".into()))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::StringLiteral("rust".into()))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::SelectorCall { args: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::StringLiteral("rust".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Selector(".code".into())))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::Eof,
                    token: Some(Shared::new(token(TokenKind::Eof))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::selector_call_heading_range(
        vec![
            Shared::new(token(TokenKind::Selector(".h".into()))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::NumberLiteral(1.into()))),
            Shared::new(token(TokenKind::DoubleDot)),
            Shared::new(token(TokenKind::NumberLiteral(3.into()))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::SelectorCall { args: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
// 1..3 is parsed as a BinaryOp(RangeOp) expression by parse_expr
                        Shared::new(Node {
                            kind: NodeKind::BinaryOp { op: BinaryOp::RangeOp, lhs: Shared::new(Node {
                                    kind: NodeKind::Literal,
                                    token: Some(Shared::new(token(TokenKind::NumberLiteral(1.into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }), rhs: Shared::new(Node {
                                    kind: NodeKind::Literal,
                                    token: Some(Shared::new(token(TokenKind::NumberLiteral(3.into())))),
                                    leading_trivia: TriviaList::new(),
                                    trailing_trivia: TriviaList::new(),
                                }) },
                            token: Some(Shared::new(token(TokenKind::DoubleDot))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Selector(".h".into())))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::Eof,
                    token: Some(Shared::new(token(TokenKind::Eof))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::selector_call_ident_arg(
        vec![
            Shared::new(token(TokenKind::Selector(".h".into()))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("n".into()))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::SelectorCall { args: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Ident { attr: None },
                            token: Some(Shared::new(token(TokenKind::Ident("n".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Selector(".h".into())))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::Eof,
                    token: Some(Shared::new(token(TokenKind::Eof))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::selector_call_with_attribute(
        vec![
            Shared::new(token(TokenKind::Selector(".h".into()))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::NumberLiteral(1.into()))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Selector(".depth".into()))),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::SelectorCall { args: vec![
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::NumberLiteral(1.into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
Shared::new(Node {
                            kind: NodeKind::Selector { items: ArgList::new() },
                            token: Some(Shared::new(token(TokenKind::Selector(".depth".into())))),
                            leading_trivia: TriviaList::new(),
                            trailing_trivia: TriviaList::new(),
                        }),
].into() },
                    token: Some(Shared::new(token(TokenKind::Selector(".h".into())))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::Eof,
                    token: Some(Shared::new(token(TokenKind::Eof))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
#[case::unmatched_end_at_root(
        // `. end end eof`: the first `end` closes a root statement, the second `end` has no
        // matching open block and should produce `UnmatchedEnd` pointing at the first `end`.
        vec![
            Shared::new(token(TokenKind::Selector(".".into()))),
            Shared::new(token(TokenKind::End)),
            Shared::new(token(TokenKind::End)),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Self_ { attr: None },
                    token: Some(Shared::new(token(TokenKind::Selector(".".into())))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::End,
                    token: Some(Shared::new(token(TokenKind::End))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::End,
                    token: Some(Shared::new(token(TokenKind::End))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
                eof_node(),
            ],
            ErrorReporter::with_error(vec![ParseError::UnmatchedEnd(Shared::new(token(TokenKind::End)))], 100)
        )
    )]
#[case::unmatched_end_at_root_pipe_follows(
        // `. end end | ident eof`: mirrors the real-world `def foo: if (.): . end end | foo`
        // scenario where the extra `end` should be flagged, not the pipe after it.
        vec![
            Shared::new(token(TokenKind::Selector(".".into()))),
            Shared::new(token(TokenKind::End)),
            Shared::new(token(TokenKind::End)),
            Shared::new(token(TokenKind::Pipe)),
            Shared::new(token(TokenKind::Ident("foo".into()))),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Self_ { attr: None },
                    token: Some(Shared::new(token(TokenKind::Selector(".".into())))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::End,
                    token: Some(Shared::new(token(TokenKind::End))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::End,
                    token: Some(Shared::new(token(TokenKind::End))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::Token,
                    token: Some(Shared::new(token(TokenKind::Pipe))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::Ident { attr: None },
                    token: Some(Shared::new(token(TokenKind::Ident("foo".into())))),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }),
                eof_node(),
            ],
            ErrorReporter::with_error(vec![ParseError::UnmatchedEnd(Shared::new(token(TokenKind::End)))], 100)
        )
    )]

fn test_parse(#[case] input: Vec<Shared<Token>>, #[case] expected: (Vec<Shared<Node>>, ErrorReporter)) {
    let (nodes, errors) = Parser::new(&input).parse();
    assert_eq!(errors, expected.1);
    assert_eq!(nodes, expected.0);
}

#[test]
fn test_module_block_can_follow_a_def() {
    let (nodes, errors) = crate::parse_recovery("def a(): 1;\nmodule m:\n  def b(): 2;\nend\n");
    assert!(!errors.has_errors());
    assert!(nodes.iter().any(|node| matches!(node.kind, NodeKind::Module { .. })));
}

#[test]
fn test_descendant_chain_has_no_synthetic_bridge_node() {
    // No synthetic DoubleDot node: it would duplicate the next selector's source range.
    let (nodes, errors) = crate::parse_recovery(".blockquote .code");
    assert!(!errors.has_errors());

    let block = nodes
        .iter()
        .find(|node| matches!(node.kind, NodeKind::Block { .. }))
        .expect("descendant chain should parse as a Block");
    let children = block.all_children();

    assert_eq!(
        children.len(),
        2,
        "expected exactly the two real selectors, got {:#?}",
        children
    );
    assert!(
        children
            .iter()
            .all(|child| !matches!(child.token.as_ref().map(|t| &t.kind), Some(TokenKind::DoubleDot))),
        "no child should carry a synthetic DoubleDot token: {:#?}",
        children
    );
}

#[test]
fn test_unmatched_end_error_message() {
    // Verify the error message text for `UnmatchedEnd`.
    let err = ParseError::UnmatchedEnd(Shared::new(token(TokenKind::End)));
    assert_eq!(err.to_string(), "Unexpected `end` keyword — no open block to close");
}

#[test]
fn test_unmatched_end_error_ranges() {
    // Verify that `error_ranges` correctly extracts the range from an `UnmatchedEnd` error.
    let end_token = Shared::new(Token {
        range: Range {
            start: Position { line: 1, column: 20 },
            end: Position { line: 1, column: 23 },
        },
        kind: TokenKind::End,
        module_id: 1.into(),
    });
    let mut reporter = ErrorReporter::new(100);
    reporter.report(ParseError::UnmatchedEnd(Shared::clone(&end_token)));

    let ranges = reporter.error_ranges("def foo: if (.): . end end");
    assert_eq!(ranges.len(), 1);
    assert_eq!(ranges[0].1.start.line, 1);
    assert_eq!(ranges[0].1.start.column, 20);
    assert!(ranges[0].0.contains("end"));
}

#[test]
fn test_error_reporter_error_ranges() {
    let text = "def foo():\n bar()\n";

    let mut reporter = ErrorReporter::new(100);

    let unexpected_token = Shared::new(Token {
        range: Range {
            start: Position { line: 1, column: 4 },
            end: Position { line: 1, column: 7 },
        },
        kind: TokenKind::Ident("foo".into()),
        module_id: 1.into(),
    });
    reporter.report(ParseError::UnexpectedToken(Shared::clone(&unexpected_token)));

    let insufficient_token = Shared::new(Token {
        range: Range {
            start: Position { line: 1, column: 4 },
            end: Position { line: 1, column: 7 },
        },
        kind: TokenKind::Ident("bar".into()),
        module_id: 1.into(),
    });
    reporter.report(ParseError::InsufficientTokens(Shared::clone(&insufficient_token)));

    reporter.report(ParseError::UnexpectedEOFDetected);

    let ranges = reporter.error_ranges(text);

    assert_eq!(ranges.len(), 3);

    assert_eq!(ranges[0].1.start.line, 1);
    assert_eq!(ranges[0].1.start.column, 4);
    assert_eq!(ranges[0].1.end.line, 1);
    assert_eq!(ranges[0].1.end.column, 7);

    assert_eq!(ranges[1].1.start.line, 1);
    assert_eq!(ranges[1].1.start.column, 4);
    assert_eq!(ranges[1].1.end.line, 1);
    assert_eq!(ranges[1].1.end.column, 7);

    assert_eq!(ranges[2].1.start.line, 2);
    assert_eq!(ranges[2].1.start.column, 6);
    assert_eq!(ranges[2].1.end.line, 2);
    assert_eq!(ranges[2].1.end.column, 6);
}

#[test]
fn test_error_reporter_diagnostics_carries_hints_and_ranges() {
    let text = "def foo():\n bar()\n";

    let mut reporter = ErrorReporter::new(100);

    let unexpected_token = Shared::new(Token {
        range: Range {
            start: Position { line: 1, column: 4 },
            end: Position { line: 1, column: 7 },
        },
        kind: TokenKind::Ident("foo".into()),
        module_id: 1.into(),
    });
    reporter.report(ParseError::UnexpectedToken(Shared::clone(&unexpected_token)));
    reporter.report(ParseError::UnexpectedEOFDetected);

    let diagnostics = reporter.diagnostics(text);

    assert_eq!(diagnostics.len(), 2);

    assert_eq!(
        diagnostics[0].message,
        ParseError::UnexpectedToken(unexpected_token).to_string()
    );
    assert_eq!(diagnostics[0].range.unwrap().start.column, 4);
    assert_eq!(diagnostics[0].hints.len(), 1);

    assert_eq!(diagnostics[1].message, ParseError::UnexpectedEOFDetected.to_string());
    assert_eq!(diagnostics[1].hints.len(), 1);
}

#[test]
fn test_error_reporter_diagnostics_unknown_selector_hint_matches_error_help() {
    let mut reporter = ErrorReporter::new(100);
    reporter.report(ParseError::UnknownSelector(crate::selector::UnknownSelector::new(
        Token {
            range: Range::default(),
            kind: TokenKind::Selector(smol_str::SmolStr::new(".hedaing")),
            module_id: 1.into(),
        },
    )));

    let diagnostics = reporter.diagnostics("");
    assert_eq!(
        diagnostics[0].hints[0],
        "Unknown selector `.hedaing`. A selector with a similar name exists: `.heading`."
    );
}

#[test]
fn test_error_reporter_has_errors() {
    let mut reporter = ErrorReporter::new(100);
    assert!(!reporter.has_errors());

    reporter.report(ParseError::UnexpectedEOFDetected);
    assert!(reporter.has_errors());
}

#[test]
fn test_error_reporter_max_errors() {
    let mut reporter = ErrorReporter::new(2);

    reporter.report(ParseError::UnexpectedToken(Shared::new(Token {
        range: Range::default(),
        kind: TokenKind::Ident("foo".into()),
        module_id: 1.into(),
    })));
    reporter.report(ParseError::UnexpectedToken(Shared::new(Token {
        range: Range::default(),
        kind: TokenKind::Ident("bar".into()),
        module_id: 1.into(),
    })));
    reporter.report(ParseError::UnexpectedEOFDetected);

    assert_eq!(reporter.errors.len(), 2);
}

#[test]
fn test_error_reporter_unexpected_eof_detected() {
    let mut reporter = ErrorReporter::new(2);

    reporter.report(ParseError::UnexpectedEOFDetected);
    reporter.report(ParseError::UnexpectedEOFDetected);

    assert_eq!(reporter.errors.len(), 1);
}

#[test]
fn test_error_reporter_display() {
    let mut reporter = ErrorReporter::new(100);

    reporter.report(ParseError::UnexpectedEOFDetected);
    let token = Shared::new(Token {
        range: Range::default(),
        kind: TokenKind::Ident("foo".into()),
        module_id: 1.into(),
    });
    reporter.report(ParseError::UnexpectedToken(token));

    let display = format!("{}", reporter);
    assert!(display.contains("Unexpected EOF detected"));
    assert!(display.contains("Unexpected token"));
}

#[rstest]
#[case::binary_op_without_rhs("1 +")]
#[case::as_without_name("1 as")]
#[case::as_with_non_ident("1 as 2")]
#[case::let_only("let")]
#[case::let_without_rhs("let x =")]
#[case::var_only("var")]
#[case::def_only("def")]
#[case::def_without_body("def f(x):")]
#[case::fn_only("fn")]
#[case::include_only("include")]
#[case::import_only("import")]
#[case::import_as_without_alias("import \"csv\" as")]
#[case::if_only("if")]
#[case::if_without_then("if (1):")]
#[case::elif_without_if("elif (1): 1")]
#[case::match_only("match")]
#[case::match_arm_without_body("match (1): | x:")]
#[case::foreach_open_paren("foreach (")]
#[case::try_only("try")]
#[case::selector_bracket_open(".[")]
#[case::dict_unclosed("{\"a\": ")]
#[case::dict_trailing_comma_unclosed("{\"a\": 1, ")]
#[case::array_unclosed("[1, ")]
#[case::call_unclosed("f(1, ")]
#[case::trailing_pipe("1 |")]
#[case::unmatched_end("end")]
fn test_truncated_input_reports_error(#[case] code: &str) {
    let (_, errors) = crate::parse_recovery(code);
    assert!(errors.has_errors(), "{code}");
}

#[rstest]
#[case::single_line("{\"a\": 1,}")]
#[case::multi_entry("{\"a\": 1, \"b\": [1],}")]
#[case::multi_line("{\n  \"a\": 1,\n  # c\n}")]
#[case::spread("{...x, \"a\": 1,}")]
fn test_dict_trailing_comma(#[case] code: &str) {
    let (nodes, errors) = crate::parse_recovery(code);
    assert!(!errors.has_errors(), "{code}: {errors:?}");
    assert!(matches!(nodes[0].kind, NodeKind::Dict { .. }), "{code}");
}

#[rstest]
#[case::string("\"abc\"[1]")]
#[case::string_slice("\"abc\"[1:2]")]
#[case::interpolated("s\"a${1}\"[0]")]
fn test_string_literal_index(#[case] code: &str) {
    let (nodes, errors) = crate::parse_recovery(code);
    assert!(!errors.has_errors(), "{code}: {errors:?}");
    assert!(matches!(nodes[0].kind, NodeKind::Call { .. }), "{code}");
}

#[rstest]
#[case::string_key("{\"a\": 1}[\"a\"]")]
#[case::symbol_key("{\"a\": 1}[:a]")]
#[case::chained("{\"a\": {\"b\": 2}}[\"a\"][\"b\"]")]
fn test_dict_literal_index(#[case] code: &str) {
    let (nodes, errors) = crate::parse_recovery(code);
    assert!(!errors.has_errors(), "{code}: {errors:?}");
    assert!(matches!(nodes[0].kind, NodeKind::Call { .. }), "{code}");
}

#[rstest]
#[case::dict_newline("{\"a\": 1}\n[\"a\"]")]
#[case::dict_comment("{\"a\": 1} # c\n[\"a\"]")]
#[case::array_newline("[1, 2]\n[0]")]
#[case::string_newline("\"abc\"\n[0]")]
#[case::ident_newline("x\n[0]")]
#[case::call_newline("foo(1)\n[0]")]
#[case::dynamic_call_newline("x\n(1)")]
fn test_postfix_across_newline(#[case] code: &str) {
    let (nodes, errors) = crate::parse_recovery(code);
    assert!(!errors.has_errors(), "{code}: {errors:?}");
    let exprs: Vec<_> = nodes.iter().filter(|n| !matches!(n.kind, NodeKind::Eof)).collect();
    assert_eq!(exprs.len(), 1, "{code}");
    assert!(
        matches!(exprs[0].kind, NodeKind::Call { .. } | NodeKind::CallDynamic { .. }),
        "{code}"
    );
}

const TOKEN_SOUP: &[&str] = &[
    "def",
    "fn",
    "let",
    "var",
    "if",
    "elif",
    "else",
    "unless",
    "while",
    "until",
    "loop",
    "foreach",
    "match",
    "try",
    "catch",
    "do",
    "end",
    "module",
    "import",
    "include",
    "as",
    "break",
    "continue",
    "yield",
    "self",
    "nodes",
    "(",
    ")",
    "[",
    "]",
    "{",
    "}",
    ":",
    ";",
    ",",
    "|",
    "=",
    "+",
    "-",
    "*",
    "/",
    "==",
    "..",
    ".",
    ".h1",
    ".[0]",
    "x",
    "f",
    "1",
    "\"s\"",
    "s\"${x}\"",
    ":sym",
    "$ENV",
    "#c\n",
    "\n",
    " ",
];

const STATEMENTS: &[&str] = &[
    "def f(x, y = 1):\n  x + y;",
    "let a = [1, 2]",
    "if (a):\t1 elif (b): 2 else: 3",
    "match (x): | [a, ..rest]: a | {k}: k | _: 0 end",
    "foreach (v, a): v;",
    "try: 1 catch(e): e",
    ".h1.value",
    ".[0][1]",
    "{\"a\": 1, ...b}",
    "fn(x): x;",
    "s\"${a} b\"",
    "import \"csv\" as c",
    "module m: def g(): 1; end",
    "while (a): break: 1;",
    "loop do continue end",
    "1 as n",
    "x += 1",
    "(a)(1)",
];

const SEPARATORS: &[&str] = &[" | ", "\n| ", " |\n  ", "\t|\t", " | # comment\n", "\n\n| "];

/// Broken input still yields a tree: the well-formed parts survive, and the
/// damage is confined to `Error`/`Missing` nodes.
#[rstest]
#[case::unclosed_call("foo(1, 2,", "foo", "`)`")]
#[case::unclosed_group("(x", "(", "`)`")]
#[case::unclosed_array("[1, 2", "[", "`]`")]
#[case::unclosed_dict("{a: 1, b: 2", "{", "`}`")]
#[case::unclosed_bracket_access("arr[5", "arr", "`]`")]
fn test_unclosed_delimiter_becomes_missing_node(#[case] code: &str, #[case] kept: &str, #[case] expected: &str) {
    let (nodes, errors) = crate::parse_recovery(code);

    assert!(errors.has_errors(), "{code}");
    assert!(
        nodes
            .iter()
            .any(|node| node.to_string() == kept || node.children().any(|c| c.to_string() == kept)),
        "{code}: {nodes:?}"
    );
    fn has_missing(node: &Shared<Node>, expected: &str) -> bool {
        matches!(&node.kind, NodeKind::Missing { expected: e, .. } if *e == expected)
            || node.children().any(|child| has_missing(child, expected))
    }
    assert!(
        nodes.iter().any(|node| has_missing(node, expected)),
        "{code}: {nodes:?}"
    );
}

#[rstest]
#[case::leading("foo(, 1)")]
#[case::consecutive("foo(1,, 2)")]
#[case::trailing_pair("foo(1, ,)")]
#[case::only_comma("foo(,)")]
#[case::array_leading("[, 1]")]
fn test_absent_item_before_separator_has_error(#[case] code: &str) {
    let (nodes, errors) = crate::parse_recovery(code);

    assert!(errors.has_errors(), "{code}");
    assert!(nodes.iter().any(|node| node.has_error()), "{code}: {nodes:?}");
    let commas: usize = nodes
        .iter()
        .flat_map(|node| node.children())
        .filter(|child| matches!(child.token.as_ref().map(|t| &t.kind), Some(TokenKind::Comma)))
        .count();
    assert_eq!(commas, code.matches(',').count(), "{code}: separators must be kept");
}

#[rstest]
#[case::unclosed_call("foo(1,", Position { line: 1, column: 7 })]
#[case::unclosed_call_next_line("foo(1,\n", Position { line: 1, column: 7 })]
#[case::cursor_on_next_line("foo(1,\n  ", Position { line: 2, column: 2 })]
fn test_unclosed_call_range_covers_cursor(#[case] code: &str, #[case] cursor: Position) {
    let (nodes, _) = crate::parse_recovery(code);
    let call = nodes
        .iter()
        .find(|node| matches!(node.kind, NodeKind::Call { .. }))
        .unwrap_or_else(|| panic!("{code}: {nodes:?}"));

    assert!(call.node_range().contains(&cursor), "{code}: {:?}", call.node_range());
}

#[rstest]
#[case::junk_between_items("foo(1 2, 3)", 3)]
#[case::invalid_item("foo(1, §, 3) | bar", 3)]
#[case::stray_closer("foo(1) ) bar(2)", 1)]
fn test_junk_is_kept_in_error_node(#[case] code: &str, #[case] valid_literals: usize) {
    let (nodes, errors) = crate::parse_recovery(code);

    fn count(node: &Shared<Node>, error_nodes: &mut usize, literals: &mut usize) {
        match node.kind {
            NodeKind::Error { .. } => *error_nodes += 1,
            NodeKind::Literal => *literals += 1,
            _ => {}
        }
        node.children().for_each(|child| count(child, error_nodes, literals));
    }

    let (mut error_nodes, mut literals) = (0, 0);
    nodes
        .iter()
        .for_each(|node| count(node, &mut error_nodes, &mut literals));

    assert!(errors.has_errors(), "{code}");
    assert!(error_nodes >= 1, "{code}");
    assert!(literals >= valid_literals.min(2), "{code}: {literals}");
}

#[rstest]
#[case::after_def("def f(): 1; let y = f()")]
#[case::let_after_let("let x = 1 let y = 2")]
fn test_statements_need_no_separator(#[case] code: &str) {
    let (_, errors) = crate::parse_recovery(code);

    assert!(!errors.has_errors(), "{code}: {errors}");
}

proptest::proptest! {
    #[test]
    fn prop_parse_recovery_never_panics_on_ascii(code in "[ -~\\n\\t]{0,120}") {
        let _ = crate::parse_recovery(&code);
    }

    #[test]
    fn prop_parse_recovery_never_panics_on_token_soup(
        tokens in proptest::collection::vec(proptest::sample::select(TOKEN_SOUP), 0..40)
    ) {
        let _ = crate::parse_recovery(&tokens.join(" "));
    }

    /// Parsing is lossless: every non-EOF token reappears in the CST, in order.
    #[test]
    fn prop_cst_keeps_every_token(
        stmts in proptest::collection::vec(proptest::sample::select(STATEMENTS), 1..8),
        seps in proptest::collection::vec(proptest::sample::select(SEPARATORS), 8),
    ) {
        let mut code = String::new();
        for (i, stmt) in stmts.iter().enumerate() {
            if i > 0 {
                code.push_str(seps[i]);
            }
            code.push_str(stmt);
        }
        let (nodes, errors) = crate::parse_recovery(&code);
        proptest::prop_assert!(!errors.has_errors(), "{}: {}", code, errors);

        let (kept, expected) = kept_and_lexed_token_counts(&code, &nodes);
        proptest::prop_assert_eq!(kept, expected, "{}", code);
    }

    /// Recovery is lossless even for broken input: no token is dropped.
    #[test]
    fn prop_cst_keeps_every_token_on_token_soup(
        tokens in proptest::collection::vec(proptest::sample::select(TOKEN_SOUP), 0..40)
    ) {
        let code = tokens.join(" ");
        let (nodes, _) = crate::parse_recovery(&code);
        // A program with no statements (e.g. only a comment) has no node to hang trivia on.
        proptest::prop_assume!(!nodes.is_empty());
        let (kept, expected) = kept_and_lexed_token_counts(&code, &nodes);
        proptest::prop_assert_eq!(kept, expected, "{}", code);
    }
}

/// Number of distinct token ranges found in `nodes`, and the number of tokens the lexer produced.
fn kept_and_lexed_token_counts(code: &str, nodes: &[Shared<Node>]) -> (usize, usize) {
    fn collect(node: &Shared<Node>, out: &mut Vec<Range>) {
        for trivia in node.leading_trivia.iter().chain(&node.trailing_trivia) {
            if !trivia.is_new_line() {
                out.push(trivia.range());
            }
        }
        if let Some(token) = node.token.as_ref().filter(|t| !matches!(t.kind, TokenKind::Eof)) {
            out.push(token.range);
        }
        node.children().for_each(|child| collect(child, out));
    }

    let mut ranges = Vec::new();
    nodes.iter().for_each(|node| collect(node, &mut ranges));
    ranges.sort();
    ranges.dedup();

    let expected = crate::Lexer::new(crate::lexer::Options {
        ignore_errors: true,
        include_spaces: true,
    })
    .tokenize(code, crate::Module::TOP_LEVEL_MODULE_ID)
    .unwrap()
    .iter()
    .filter(|t| !matches!(t.kind, TokenKind::Eof | TokenKind::NewLine))
    .count();

    (ranges.len(), expected)
}

/// Renders `node` fully parenthesized with the source symbol of each operator.
fn shape(node: &Node) -> String {
    let text = |node: &Node| node.token.as_ref().map(|t| t.to_string()).unwrap_or_default();
    match &node.kind {
        NodeKind::BinaryOp { lhs, rhs, .. } | NodeKind::Assign { lhs, rhs, .. } => {
            format!("({} {} {})", shape(lhs), text(node), shape(rhs))
        }
        NodeKind::UnaryOp { operand, .. } => format!("({} {})", text(node), shape(operand)),
        NodeKind::As { expr, name } => format!("({} as {})", shape(expr), text(name)),
        NodeKind::Group { expr, .. } => format!("<{}>", shape(expr)),
        _ => text(node),
    }
}

fn parse_shape(code: &str) -> (String, bool) {
    let (nodes, errors) = crate::parse_recovery(code);
    let expressions: Vec<_> = nodes.iter().filter(|n| !matches!(n.kind, NodeKind::Eof)).collect();
    assert_eq!(expressions.len(), 1, "expected one expression for {code:?}");
    (shape(expressions[0]), errors.has_errors())
}

/// For each argument of the top-level call, the statement count of its body if it is a lambda,
/// and whether parsing reported errors.
fn lambda_arg_body_lens(code: &str) -> (Vec<Option<usize>>, bool) {
    let (nodes, errors) = crate::parse_recovery(code);
    let expressions: Vec<_> = nodes.iter().filter(|n| !matches!(n.kind, NodeKind::Eof)).collect();
    let [call] = expressions.as_slice() else {
        panic!("expected one expression for {code:?}");
    };
    let NodeKind::Call { args } = &call.kind else {
        panic!("expected a call for {code:?}");
    };
    let lens = args
        .iter()
        .filter(|arg| !matches!(arg.kind, NodeKind::Token))
        .map(|arg| match &arg.kind {
            NodeKind::Fn { program, .. } => Some(
                program
                    .iter()
                    .filter(|node| !matches!(node.kind, NodeKind::Token | NodeKind::End))
                    .count(),
            ),
            _ => None,
        })
        .collect();
    (lens, errors.has_errors())
}

#[rstest]
#[case::only_arg("f(fn(x): x + 1)", vec![Some(1)])]
#[case::arrow("f(->(x): x + 1)", vec![Some(1)])]
#[case::last_arg("f(a, fn(x): x + 1)", vec![None, Some(1)])]
#[case::ends_at_comma("f(fn(x): x, 1)", vec![Some(1), None])]
#[case::lambda_between_args("f(a, fn(x): x, b)", vec![None, Some(1), None])]
#[case::two_lambdas("f(fn(x): x + 1, fn(x): x * 2)", vec![Some(1), Some(1)])]
#[case::pipe_stays_in_body("f(fn(x): x | g(.h))", vec![Some(2)])]
#[case::pipe_before_comma("f(fn(x): x | g(.h), 1)", vec![Some(2), None])]
#[case::call_with_commas_in_body("f(fn(x): g(x, 1, 2), 3)", vec![Some(1), None])]
#[case::array_in_body("f(fn(x): [x, 1], 2)", vec![Some(1), None])]
#[case::paren_in_body("f(fn(x): (x + 1) * 2, 3)", vec![Some(1), None])]
#[case::if_else_in_body("f(fn(x): if (x): 1 else: 2, 3)", vec![Some(1), None])]
#[case::do_end_in_body("f(fn(x): do 1 | 2 end, 3)", vec![Some(1), None])]
#[case::foreach_in_body("f(fn(x): foreach (i, x): i end, 3)", vec![Some(1), None])]
#[case::match_in_body("f(fn(x): match (x): | 1: 2 | _: 3 end, 4)", vec![Some(1), None])]
#[case::nested_call_lambda_then_comma("f(fn(x): g(x, fn(y): y + 1), 2)", vec![Some(1), None])]
#[case::multiline_body("f(fn(x):\n  let y = x + 1\n  | y * 2\n, 3)", vec![Some(2), None])]
#[case::lambda_in_param_default("f(fn(x = g(fn(y): y)): x, 1)", vec![Some(1), None])]
#[case::semicolon_form("f(fn(x): x + 1;)", vec![Some(1)])]
#[case::semicolon_form_then_arg("f(fn(x): x; , 1)", vec![Some(1), None])]
fn test_lambda_in_call_args_may_omit_terminator(#[case] code: &str, #[case] expected: Vec<Option<usize>>) {
    assert_eq!(lambda_arg_body_lens(code), (expected, false));
}

#[rstest]
#[case::only_arg("f(fn: self + 1)", vec![Some(1)])]
#[case::ends_at_comma("f(fn: self, 1)", vec![Some(1), None])]
#[case::pipe_before_comma("f(fn: g | h, 1)", vec![Some(2), None])]
#[case::call_with_commas_in_body("f(fn: g(1, 2), 3)", vec![Some(1), None])]
#[case::nested("f(fn: g(fn: self + 1), 2)", vec![Some(1), None])]
#[case::semicolon_form("f(fn: self + 1;)", vec![Some(1)])]
#[case::spaced_colon("f(fn : self)", vec![Some(1)])]
fn test_lambda_without_params(#[case] code: &str, #[case] expected: Vec<Option<usize>>) {
    assert_eq!(lambda_arg_body_lens(code), (expected, false));
}

#[test]
fn test_lambda_without_params_has_no_param_nodes() {
    let (nodes, errors) = crate::parse_recovery("f(fn: self)");
    assert!(!errors.has_errors());
    let NodeKind::Call { args } = &nodes[0].kind else {
        panic!("expected a call");
    };
    let Some(NodeKind::Fn { params, .. }) = args
        .iter()
        .map(|arg| &arg.kind)
        .find(|k| matches!(k, NodeKind::Fn { .. }))
    else {
        panic!("expected a lambda");
    };
    assert!(params.is_empty());
}

#[rstest]
#[case::arrow_has_no_paramless_form("f(->: self)")]
#[case::empty_body("f(fn: )")]
#[case::unclosed_call("f(fn: self")]
fn test_lambda_without_params_reports_malformed_input(#[case] code: &str) {
    let (_, errors) = crate::parse_recovery(code);
    assert!(errors.has_errors(), "{code:?} should report an error");
}

#[rstest]
#[case::empty_body("f(fn(x): )")]
#[case::empty_body_then_comma("f(fn(x):, 1)")]
#[case::dangling_operator("f(fn(x): x +)")]
#[case::unclosed_call("f(fn(x): x + 1")]
#[case::stray_close_paren("f(fn(x): x + 1))")]
#[case::let_value("let g = fn(x): x + 1)")]
fn test_lambda_reports_malformed_input(#[case] code: &str) {
    let (_, errors) = crate::parse_recovery(code);
    assert!(errors.has_errors(), "{code:?} should report an error");
}

#[rstest]
#[case::paren_group("(fn(x): x + 1)(2)")]
#[case::array_element("[fn(x): x, 1]")]
#[case::implicit_array_element("[fn: self, 1]")]
#[case::dict_value("{\"a\": fn(x): x, \"b\": 1}")]
#[case::lambda_body_tail("f(fn(x): fn(y): x + y)")]
#[case::after_pipe("f(1 | fn(x): x + 1, 2)")]
fn test_lambda_ends_at_the_closer_of_its_enclosing_construct(#[case] code: &str) {
    let (_, errors) = crate::parse_recovery(code);
    assert!(!errors.has_errors(), "{code:?} should not report an error");
}

#[rstest]
#[case::do_block("do 1")]
#[case::def("def f(): 1")]
#[case::def_do("def f() do 1")]
#[case::fn_params("fn(x): x")]
#[case::fn_no_params("fn: 1")]
#[case::if_body("if (true): 1")]
#[case::after_pipe("1 | fn(x): x")]
fn test_top_level_body_may_end_at_eof(#[case] code: &str) {
    let (_, errors) = crate::parse_recovery(code);
    assert!(!errors.has_errors(), "{code:?}: {errors:?}");
}

#[rstest]
#[case::call("f(do 1", "`)`")]
#[case::array("[do 1", "`]`")]
#[case::group("(do 1", "`)`")]
fn test_body_at_eof_leaves_missing_closer_to_enclosing_construct(#[case] code: &str, #[case] expected: &str) {
    let (_, errors) = crate::parse_recovery(code);
    let errors = errors.to_vec();
    assert_eq!(errors.len(), 1, "{code:?}: {errors:?}");
    assert!(
        matches!(&errors[0], ParseError::Missing { expected: e, .. } if *e == expected),
        "{code:?}: {errors:?}"
    );
}

#[rstest]
#[case::add("a + b", "(a + b)")]
#[case::sub("a - b", "(a - b)")]
#[case::mul("a * b", "(a * b)")]
#[case::div("a / b", "(a / b)")]
#[case::modulo("a % b", "(a % b)")]
#[case::eq("a == b", "(a == b)")]
#[case::ne("a != b", "(a != b)")]
#[case::lt("a < b", "(a < b)")]
#[case::lte("a <= b", "(a <= b)")]
#[case::gt("a > b", "(a > b)")]
#[case::gte("a >= b", "(a >= b)")]
#[case::and("a && b", "(a && b)")]
#[case::or("a || b", "(a || b)")]
#[case::left_shift("a << b", "(a << b)")]
#[case::right_shift("a >> b", "(a >> b)")]
#[case::convert("a @ b", "(a @ b)")]
#[case::range("a .. b", "(a .. b)")]
#[case::coalesce("a ?? b", "(a ?? b)")]
#[case::regex_match("a =~ b", "(a =~ b)")]
#[case::not_regex_match("a !~ b", "(a !~ b)")]
fn test_every_binary_operator_shape(#[case] code: &str, #[case] expected: &str) {
    assert_eq!(parse_shape(code), (expected.to_string(), false));
}

#[rstest]
#[case::and_over_or("a || b && c", "(a || (b && c))")]
#[case::or_after_and("a && b || c", "((a && b) || c)")]
#[case::comparison_over_and("a && b == c", "(a && (b == c))")]
#[case::and_after_comparison("a == b && c", "((a == b) && c)")]
#[case::additive_over_comparison("a == b + c", "(a == (b + c))")]
#[case::comparison_after_additive("a + b == c", "((a + b) == c)")]
#[case::multiplicative_over_additive("a + b * c", "(a + (b * c))")]
#[case::additive_after_multiplicative("a * b + c", "((a * b) + c)")]
#[case::range_over_multiplicative("a * b .. c", "(a * (b .. c))")]
#[case::multiplicative_after_range("a .. b * c", "((a .. b) * c)")]
#[case::coalesce_over_multiplicative("a * b ?? c", "(a * (b ?? c))")]
#[case::multiplicative_after_coalesce("a ?? b * c", "((a ?? b) * c)")]
#[case::convert_with_multiplicative("a + b @ c", "(a + (b @ c))")]
#[case::convert_is_left_associative_with_mul("a * b @ c", "((a * b) @ c)")]
#[case::shift_with_additive("a << b + c", "((a << b) + c)")]
#[case::additive_with_shift("a + b >> c", "((a + b) >> c)")]
#[case::regex_match_with_comparison("a =~ b == c", "((a =~ b) == c)")]
#[case::regex_over_and("a !~ b && c", "((a !~ b) && c)")]
#[case::three_levels("a || b && c == d + e * f .. g", "(a || (b && (c == (d + (e * (f .. g))))))")]
#[case::three_levels_reversed("a .. b * c + d == e && f || g", "((((((a .. b) * c) + d) == e) && f) || g)")]
fn test_binary_op_precedence_shape(#[case] code: &str, #[case] expected: &str) {
    assert_eq!(parse_shape(code), (expected.to_string(), false));
}

#[rstest]
#[case::or("a || b || c", "((a || b) || c)")]
#[case::and("a && b && c", "((a && b) && c)")]
#[case::comparison("a == b == c", "((a == b) == c)")]
#[case::mixed_comparison("a < b != c", "((a < b) != c)")]
#[case::sub("a - b - c", "((a - b) - c)")]
#[case::add_then_sub("a + b - c", "((a + b) - c)")]
#[case::shifts("a << b >> c", "((a << b) >> c)")]
#[case::div("a / b / c", "((a / b) / c)")]
#[case::mul_div_mod("a * b / c % d", "(((a * b) / c) % d)")]
#[case::range("a .. b .. c", "((a .. b) .. c)")]
#[case::coalesce("a ?? b ?? c", "((a ?? b) ?? c)")]
#[case::range_then_coalesce("a .. b ?? c", "((a .. b) ?? c)")]
#[case::coalesce_then_range("a ?? b .. c", "((a ?? b) .. c)")]
#[case::convert("a @ b @ c", "((a @ b) @ c)")]
fn test_binary_op_associativity_shape(#[case] code: &str, #[case] expected: &str) {
    assert_eq!(parse_shape(code), (expected.to_string(), false));
}

#[rstest]
#[case::parens_override_precedence("(a + b) * c", "(<(a + b)> * c)")]
#[case::parens_on_the_right("a * (b + c)", "(a * <(b + c)>)")]
#[case::parens_override_associativity("a - (b - c)", "(a - <(b - c)>)")]
#[case::neg_binds_tighter_than_mul("-a * b", "((- a) * b)")]
#[case::not_binds_tighter_than_eq("!a == b", "((! a) == b)")]
#[case::not_binds_tighter_than_and("!a && !b", "((! a) && (! b))")]
#[case::unary_on_the_right("a - -b", "(a - (- b))")]
#[case::not_on_the_right("a * !b", "(a * (! b))")]
#[case::as_binds_the_whole_expression("a + b * c as x", "((a + (b * c)) as x)")]
#[case::as_after_logic("a && b as x", "((a && b) as x)")]
fn test_operand_shape(#[case] code: &str, #[case] expected: &str) {
    assert_eq!(parse_shape(code), (expected.to_string(), false));
}

#[rstest]
#[case::assign("x = a", "(x = a)")]
#[case::assign_is_lowest("x = a || b && c == d + e * f", "(x = (a || (b && (c == (d + (e * f))))))")]
#[case::assign_takes_the_whole_expression("x = a + b * c", "(x = (a + (b * c)))")]
#[case::plus_assign("x += a * b", "(x += (a * b))")]
#[case::minus_assign("x -= a * b", "(x -= (a * b))")]
#[case::mul_assign("x *= a + b", "(x *= (a + b))")]
#[case::div_assign("x /= a + b", "(x /= (a + b))")]
#[case::mod_assign("x %= a + b", "(x %= (a + b))")]
#[case::floor_div_assign("x //= a + b", "(x //= (a + b))")]
fn test_assignment_shape(#[case] code: &str, #[case] expected: &str) {
    assert_eq!(parse_shape(code), (expected.to_string(), false));
}

#[rstest]
#[case::trivia_between("a +\n  b # c\n  * d", "(a + (b * d))")]
#[case::trivia_before_operator("a\n  + b", "(a + b)")]
#[case::trivia_in_assignment("x =\n  a || b", "(x = (a || b))")]
fn test_binary_op_shape_with_trivia(#[case] code: &str, #[case] expected: &str) {
    assert_eq!(parse_shape(code), (expected.to_string(), false));
}

#[rstest]
#[case::missing_right_operand("a +")]
#[case::missing_right_operand_after_assign("a =")]
#[case::missing_right_operand_after_logic("a &&")]
#[case::as_without_name("a as")]
#[case::as_with_non_identifier("a as 1")]
fn test_binary_op_errors(#[case] code: &str) {
    let (nodes, errors) = crate::parse_recovery(code);
    assert!(errors.has_errors(), "expected an error for {code:?}, got {nodes:?}");
}
