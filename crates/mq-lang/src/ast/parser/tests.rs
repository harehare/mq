use crate::ast::node::StringSegment;
use crate::{Module, ast::node::MatchArm, range::Range, selector};

use super::*;
use rstest::rstest;
use smallvec::smallvec;

fn token(token_kind: TokenKind) -> Token {
    Token {
        range: Range::default(),
        kind: token_kind,
        module_id: 1.into(),
    }
}

#[rstest]
#[case::ident1(
        vec![
            token(TokenKind::Ident(SmolStr::new("and"))),
            token(TokenKind::LParen),
            token(TokenKind::Ident(SmolStr::new("contains"))),
            token(TokenKind::LParen),
            token(TokenKind::StringLiteral("test".to_owned())),
            token(TokenKind::RParen),
            token(TokenKind::Comma),
            token(TokenKind::Ident(SmolStr::new("startswith"))),
            token(TokenKind::LParen),
            token(TokenKind::StringLiteral("test2".to_owned())),
            token(TokenKind::RParen),
            token(TokenKind::RParen),
            token(TokenKind::Eof)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 4.into(),
                expr: Expr::Call(
                    IdentWithToken::new_with_token("and", Some(4.into())),
                    smallvec![
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Expr::Call(
                                IdentWithToken::new_with_token("contains", Some(1.into())),
                                smallvec![Shared::new(Node {
                                    token_id: 0.into(),
                                    expr: Expr::Literal(Literal::String("test".to_owned()))
                                })],
                            )
                        }),
                        Shared::new(Node {
                            token_id: 3.into(),
                            expr: Expr::Call(
                                IdentWithToken::new_with_token("startswith", Some(3.into())),
                                smallvec![Shared::new(Node {
                                    token_id: 2.into(),
                                    expr: Expr::Literal(Literal::String("test2".to_owned()))
                                })],
                            )
                        })
                    ],
                )
            })
        ]))]
#[case::ident2(
        vec![
            token(TokenKind::Ident(SmolStr::new("and"))),
            token(TokenKind::LParen),
            token(TokenKind::Selector(SmolStr::new(".h1"))),
            token(TokenKind::Comma),
            token(TokenKind::Selector(SmolStr::new("."))),
            token(TokenKind::LBracket),
            token(TokenKind::NumberLiteral(2.into())),
            token(TokenKind::RBracket),
            token(TokenKind::LBracket),
            token(TokenKind::RBracket),
            token(TokenKind::RParen),
            token(TokenKind::Eof)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 8.into(),
                expr: Expr::Call(
                    IdentWithToken::new_with_token("and", Some(6.into())),
                    smallvec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Expr::Selector(Selector::Heading(Some(1))),
                        }),
                        Shared::new(Node {
                            token_id: 5.into(),
                            expr: Expr::Selector(Selector::Table(Some(2), None)),
                        }),
                    ],
                )
            })
        ]))]
#[case::ident3(
        vec![
            token(TokenKind::Def),
            token(TokenKind::Ident(SmolStr::new("filter"))),
            token(TokenKind::LParen),
            token(TokenKind::Ident(SmolStr::new("arg1"))),
            token(TokenKind::Comma),
            token(TokenKind::Ident(SmolStr::new("arg2"))),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::Ident(SmolStr::new("contains"))),
            token(TokenKind::LParen),
            token(TokenKind::StringLiteral("arg1".to_owned())),
            token(TokenKind::Comma),
            token(TokenKind::StringLiteral("arg2".to_owned())),
            token(TokenKind::RParen),
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Expr::Def(
                    IdentWithToken::new_with_token("filter", Some(7.into())),
                    vec![
                        Param::new(IdentWithToken::new_with_token("arg1", Some(1.into()))),
                        Param::new(IdentWithToken::new_with_token("arg2", Some(2.into()))),
                    ],
                    vec![Shared::new(Node {
                        token_id: 6.into(),
                        expr: Expr::Call(
                            IdentWithToken::new_with_token("contains", Some(6.into())),
                            smallvec![
                                Shared::new(Node {
                                    token_id: 4.into(),
                                    expr: Expr::Literal(Literal::String("arg1".to_owned())),
                                }),
                                Shared::new(Node {
                                    token_id: 5.into(),
                                    expr: Expr::Literal(Literal::String("arg2".to_owned())),
                                }),
                            ],
                        ),
                    })],
                ),
            }),
        ]))]
#[case::ident4(
        vec![
            token(TokenKind::Ident(SmolStr::new("and"))),
            token(TokenKind::LParen),
            token(TokenKind::None),
            token(TokenKind::Comma),
            token(TokenKind::Self_),
            token(TokenKind::RParen),
            token(TokenKind::Eof)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 2.into(),
                expr: Expr::Call(
                    IdentWithToken::new_with_token("and", Some(2.into())),
                    smallvec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Expr::Literal(Literal::None),
                        }),
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Expr::Self_,
                        }),
                    ],
                )
            })
        ]))]
#[case::ident5(
        vec![
            token(TokenKind::Ident(SmolStr::new("and"))),
            token(TokenKind::LParen),
            token(TokenKind::None),
            token(TokenKind::Comma),
            token(TokenKind::Self_),
            token(TokenKind::RParen),
            token(TokenKind::Ident(SmolStr::new("and"))),
        ],
        Err(SyntaxError::UnexpectedToken(token(TokenKind::Ident(SmolStr::new("and"))))))]
#[case::ident5(
        vec![
            token(TokenKind::Ident(SmolStr::new("and"))),
            token(TokenKind::LParen),
            token(TokenKind::None),
            token(TokenKind::Comma),
            token(TokenKind::Self_),
            token(TokenKind::RParen),
            token(TokenKind::Def),
        ],
        Err(SyntaxError::UnexpectedToken(token(TokenKind::Def))))]
#[case::ident6(
        vec![
            token(TokenKind::Ident(SmolStr::new("and"))),
            token(TokenKind::Def),
        ],
        Err(SyntaxError::UnexpectedToken(token(TokenKind::Ident(SmolStr::new("and"))))))]
#[case::ident_attribute_access(
        vec![
            token(TokenKind::Ident(SmolStr::new("c"))),
            token(TokenKind::Selector(SmolStr::new(".lang"))),
            token(TokenKind::Eof),
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 2.into(),
                expr: Expr::Call(
                    IdentWithToken::new_with_token(constants::builtins::ATTR, Some(1.into())),
                    smallvec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Expr::Ident(IdentWithToken::new_with_token("c", Some(0.into()))),
                        }),
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Expr::Literal(Literal::String("lang".to_owned())),
                        }),
                    ],
                ),
            })
        ]))]
#[case::error(
        vec![
            token(TokenKind::Ident(SmolStr::new("contains"))),
            token(TokenKind::LParen),
            token(TokenKind::Selector(SmolStr::new("inline_code"))),
            token(TokenKind::Eof)
        ],
        Err(SyntaxError::UnknownSelector(selector::UnknownSelector::new(token(TokenKind::Selector(SmolStr::new("inline_code")))))))]
#[case::def1(
        vec![
            token(TokenKind::Def),
            token(TokenKind::Ident(SmolStr::new("name"))),
            token(TokenKind::LParen),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::StringLiteral("value".to_owned())),
            token(TokenKind::SemiColon)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Expr::Def(
                        IdentWithToken::new_with_token("name", Some(3.into())),
                        Vec::new(),
                        vec![Shared::new(Node {
                            token_id: 2.into(),
                            expr: Expr::Literal(Literal::String("value".to_owned())),
                        })],
                ),
            }),
        ]))]
#[case::def_with_end(
        vec![
            token(TokenKind::Def),
            token(TokenKind::Ident(SmolStr::new("name"))),
            token(TokenKind::LParen),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::StringLiteral("value".to_owned())),
            token(TokenKind::End)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Expr::Def(
                        IdentWithToken::new_with_token("name", Some(3.into())),
                        Vec::new(),
                        vec![Shared::new(Node {
                            token_id: 2.into(),
                            expr: Expr::Literal(Literal::String("value".to_owned())),
                        })],
                ),
            }),
        ]))]
#[case::def2(
        vec![
            token(TokenKind::Def),
            token(TokenKind::Ident(SmolStr::new("name"))),
            token(TokenKind::LParen),
            token(TokenKind::Comma),
            token(TokenKind::RParen),
        ],
        Err(SyntaxError::UnexpectedToken(Token{range: Range::default(), kind: TokenKind::Comma, module_id: 1.into()})))]
#[case::def3(
        vec![
            token(TokenKind::Def),
            token(TokenKind::Ident(SmolStr::new("name"))),
            token(TokenKind::LParen),
            token(TokenKind::StringLiteral("value".to_owned())),
            token(TokenKind::Comma),
            token(TokenKind::RParen),
        ],
        Err(SyntaxError::UnexpectedToken(Token{range: Range::default(), kind: TokenKind::StringLiteral("value".to_string()), module_id: 1.into()})))]
#[case::def4(
        vec![
            token(TokenKind::Def),
            token(TokenKind::Ident(SmolStr::new("name"))),
            token(TokenKind::LParen),
            token(TokenKind::StringLiteral("value".to_owned())),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
        ],
        Err(SyntaxError::UnexpectedToken(Token{range: Range::default(), kind: TokenKind::StringLiteral("value".to_string()), module_id: 1.into()})))]
#[case::def5(
        vec![
            token(TokenKind::Def),
            token(TokenKind::Ident(SmolStr::new("name"))),
            token(TokenKind::LParen),
            token(TokenKind::StringLiteral("value".to_owned())),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::Pipe),
        ],
        Err(SyntaxError::UnexpectedToken(Token{range: Range::default(), kind: TokenKind::StringLiteral("value".to_string()), module_id: 1.into()})))]
#[case::def6(
        vec![
            token(TokenKind::Def),
            token(TokenKind::Ident(SmolStr::new("name"))),
            token(TokenKind::LParen),
            token(TokenKind::StringLiteral("value".to_owned())),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::SemiColon),
        ],
        Err(SyntaxError::UnexpectedToken(Token{range: Range::default(), kind: TokenKind::StringLiteral("value".to_string()), module_id: 1.into()})))]
#[case::def7(
        vec![
            token(TokenKind::Def),
            token(TokenKind::Ident(SmolStr::new("name"))),
            token(TokenKind::LParen),
            token(TokenKind::StringLiteral("value".to_owned())),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
        ],
        Err(SyntaxError::UnexpectedToken(Token{range: Range::default(), kind: TokenKind::StringLiteral("value".to_string()), module_id: 1.into()})))]
#[case::def7(
        vec![
            token(TokenKind::Def),
            token(TokenKind::LParen),
            token(TokenKind::StringLiteral("value".to_owned())),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
        ],
        Err(SyntaxError::UnexpectedToken(Token{range: Range::default(), kind:TokenKind::LParen, module_id: 1.into()})))]
#[case::def_without_colon1(
        vec![
            token(TokenKind::Def),
            token(TokenKind::Ident(SmolStr::new("name"))),
            token(TokenKind::LParen),
            token(TokenKind::RParen),
            token(TokenKind::StringLiteral("value".to_owned())),
            token(TokenKind::SemiColon)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Expr::Def(
                        IdentWithToken::new_with_token("name", Some(2.into())),
                        Vec::new(),
                        vec![Shared::new(Node {
                            token_id: 1.into(),
                            expr: Expr::Literal(Literal::String("value".to_owned())),
                        })],
                ),
            }),
        ]))]
#[case::def_without_colon2(
        vec![
            token(TokenKind::Def),
            token(TokenKind::Ident(SmolStr::new("name"))),
            token(TokenKind::LParen),
            token(TokenKind::RParen),
            token(TokenKind::StringLiteral("value".to_owned())),
            token(TokenKind::End)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Expr::Def(
                        IdentWithToken::new_with_token("name", Some(2.into())),
                        Vec::new(),
                        vec![Shared::new(Node {
                            token_id: 1.into(),
                            expr: Expr::Literal(Literal::String("value".to_owned())),
                        })],
                ),
            }),
        ]))]
#[case::def_without_colon_with_args(
        vec![
            token(TokenKind::Def),
            token(TokenKind::Ident(SmolStr::new("name"))),
            token(TokenKind::LParen),
            token(TokenKind::Ident(SmolStr::new("x"))),
            token(TokenKind::RParen),
            token(TokenKind::Ident(SmolStr::new("x"))),
            token(TokenKind::SemiColon)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Expr::Def(
                        IdentWithToken::new_with_token("name", Some(3.into())),
                        vec![
                          Param::new(IdentWithToken::new_with_token("x", Some(1.into()))),
                        ],
                        vec![Shared::new(Node {
                            token_id: 2.into(),
                            expr: Expr::Ident(
                                IdentWithToken::new_with_token("x", Some(2.into())),
                            ),
                        })],
                ),
            }),
        ]))]
#[case::unmatched_end_at_root(
        // `end` at root level followed by non-EOF: def foo: if (.): . end end | foo
        vec![
            token(TokenKind::Def),
            token(TokenKind::Ident(SmolStr::new("foo"))),
            token(TokenKind::Colon),
            token(TokenKind::If),
            token(TokenKind::LParen),
            token(TokenKind::Selector(SmolStr::new("."))),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::Selector(SmolStr::new("."))),
            token(TokenKind::End),    // closes def body
            token(TokenKind::End),    // unmatched – no open block
            token(TokenKind::Pipe),   // something after (like `| foo`)
            token(TokenKind::Ident(SmolStr::new("foo"))),
            token(TokenKind::Eof),
        ],
        Err(SyntaxError::UnmatchedEnd(token(TokenKind::End))))]
#[case::unmatched_end_standalone_if(
        // `end` after a standalone single-line if (no open block): if (.): . end end
        vec![
            token(TokenKind::If),
            token(TokenKind::LParen),
            token(TokenKind::Selector(SmolStr::new("."))),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::Selector(SmolStr::new("."))),
            token(TokenKind::End),    // unmatched
            token(TokenKind::End),
            token(TokenKind::Eof),
        ],
        Err(SyntaxError::UnmatchedEnd(token(TokenKind::End))))]
#[case::unmatched_end_at_start(
        // `end` as the very first token at root level
        vec![
            token(TokenKind::End),
            token(TokenKind::Eof),
        ],
        Err(SyntaxError::UnmatchedEnd(token(TokenKind::End))))]
#[case::let_1(
            vec![
                token(TokenKind::Let),
                token(TokenKind::Ident(SmolStr::new("x"))),
                token(TokenKind::Equal),
                token(TokenKind::NumberLiteral(42.into())),
                token(TokenKind::Eof)
            ],
            Ok(vec![
                Shared::new(Node {
                    token_id: 0.into(),
                    expr: Expr::Let(
                        Pattern::Ident(IdentWithToken::new_with_token("x", Some(1.into()))),
                        Shared::new(Node {
                            token_id: 3.into(),
                            expr: Expr::Literal(Literal::Number(42.into())),
                        }),
                    ),
                })
            ]))]
#[case::let_2(
            vec![
                token(TokenKind::Let),
                token(TokenKind::Ident(SmolStr::new("y"))),
                token(TokenKind::Equal),
                token(TokenKind::StringLiteral("hello".to_owned())),
                token(TokenKind::Eof)
            ],
            Ok(vec![
                Shared::new(Node {
                    token_id: 0.into(),
                    expr: Expr::Let(
                        Pattern::Ident(IdentWithToken::new_with_token("y", Some(1.into()))),
                        Shared::new(Node {
                            token_id: 3.into(),
                            expr: Expr::Literal(Literal::String("hello".to_owned())),
                        }),
                    ),
                })
            ]))]
#[case::let_3(
            vec![
                token(TokenKind::Let),
                token(TokenKind::Ident(SmolStr::new("flag"))),
                token(TokenKind::Equal),
                token(TokenKind::BoolLiteral(true)),
                token(TokenKind::Eof)
            ],
            Ok(vec![
                Shared::new(Node {
                    token_id: 0.into(),
                    expr: Expr::Let(
                        Pattern::Ident(IdentWithToken::new_with_token("flag", Some(1.into()))),
                        Shared::new(Node {
                            token_id: 3.into(),
                            expr: Expr::Literal(Literal::Bool(true)),
                        }),
                    ),
                })
            ]))]
#[case::let_4(
            vec![
                token(TokenKind::Let),
                token(TokenKind::Ident(SmolStr::new("z"))),
                token(TokenKind::Equal),
                token(TokenKind::Ident(SmolStr::new("some_var"))),
                token(TokenKind::Eof)
            ],
            Ok(vec![
                Shared::new(Node {
                    token_id: 0.into(),
                    expr: Expr::Let(
                        Pattern::Ident(IdentWithToken::new_with_token("z", Some(1.into()))),
                        Shared::new(Node {
                            token_id: 3.into(),
                            expr:
                                Expr::Ident(IdentWithToken::new_with_token("some_var",
                                                 Some(3.into())))
                        }),
                    ),
                })
            ]))]
#[case::let_5(
            vec![
                token(TokenKind::Let),
                token(TokenKind::Ident(SmolStr::new("z"))),
                token(TokenKind::Equal),
                token(TokenKind::Ident(SmolStr::new("some_var"))),
                token(TokenKind::Pipe),
            ],
            Err(SyntaxError::UnexpectedEOFAfterToken(token(TokenKind::Pipe))))]
#[case::let_6(
            vec![
                token(TokenKind::Let),
                token(TokenKind::Ident(SmolStr::new("z"))),
                token(TokenKind::Equal),
                token(TokenKind::Ident(SmolStr::new("some_var"))),
            ],
            Ok(vec![
                Shared::new(Node {
                    token_id: 0.into(),
                    expr: Expr::Let(
                        Pattern::Ident(IdentWithToken::new_with_token("z", Some(1.into()))),
                        Shared::new(Node {
                            token_id: 3.into(),
                            expr:
                                Expr::Ident(IdentWithToken::new_with_token("some_var", Some(3.into()))),
                        }),
                    ),
                })
            ]))]
#[case::var_1(
            vec![
                token(TokenKind::Var),
                token(TokenKind::Ident(SmolStr::new("x"))),
                token(TokenKind::Equal),
                token(TokenKind::NumberLiteral(42.into())),
                token(TokenKind::Eof)
            ],
            Ok(vec![
                Shared::new(Node {
                    token_id: 0.into(),
                    expr: Expr::Var(
                        Pattern::Ident(IdentWithToken::new_with_token("x", Some(1.into()))),
                        Shared::new(Node {
                            token_id: 3.into(),
                            expr: Expr::Literal(Literal::Number(42.into())),
                        }),
                    ),
                })
            ]))]
#[case::var_2(
            vec![
                token(TokenKind::Var),
                token(TokenKind::Ident(SmolStr::new("count"))),
                token(TokenKind::Equal),
                token(TokenKind::NumberLiteral(0.into())),
                token(TokenKind::Eof)
            ],
            Ok(vec![
                Shared::new(Node {
                    token_id: 0.into(),
                    expr: Expr::Var(
                        Pattern::Ident(IdentWithToken::new_with_token("count", Some(1.into()))),
                        Shared::new(Node {
                            token_id: 3.into(),
                            expr: Expr::Literal(Literal::Number(0.into())),
                        }),
                    ),
                })
            ]))]
#[case::assign_1(
            vec![
                token(TokenKind::Ident(SmolStr::new("x"))),
                token(TokenKind::Equal),
                token(TokenKind::NumberLiteral(100.into())),
                token(TokenKind::Eof)
            ],
            Ok(vec![
                Shared::new(Node {
                    token_id: 1.into(),
                    expr: Expr::Assign(
                        IdentWithToken::new_with_token("x", Some(0.into())),
                        Shared::new(Node {
                            token_id: 2.into(),
                            expr: Expr::Literal(Literal::Number(100.into())),
                        }),
                    ),
                })
            ]))]
#[case::assign_2(
            vec![
                token(TokenKind::Ident(SmolStr::new("name"))),
                token(TokenKind::Equal),
                token(TokenKind::StringLiteral("Alice".to_owned())),
                token(TokenKind::Eof)
            ],
            Ok(vec![
                Shared::new(Node {
                    token_id: 1.into(),
                    expr: Expr::Assign(
                        IdentWithToken::new_with_token("name", Some(0.into())),
                        Shared::new(Node {
                            token_id: 2.into(),
                            expr: Expr::Literal(Literal::String("Alice".to_owned())),
                        }),
                    ),
                })
            ]))]
#[case::index_assign(
            vec![
                token(TokenKind::Ident(SmolStr::new("arr"))),
                token(TokenKind::LBracket),
                token(TokenKind::NumberLiteral(0.into())),
                token(TokenKind::RBracket),
                token(TokenKind::Equal),
                token(TokenKind::NumberLiteral(10.into())),
                token(TokenKind::Eof)
            ],
            Ok(vec![
                Shared::new(Node {
                    token_id: 3.into(),
                    expr: Expr::IndexAssign(
IdentWithToken::new_with_token("arr", Some(0.into())),
Shared::new(Node {
                                        token_id: 1.into(),
                                        expr: Expr::Literal(Literal::Number(0.into())),
                                    }),
Shared::new(Node {
                                        token_id: 4.into(),
                                        expr: Expr::Literal(Literal::Number(10.into())),
                                    }),
),
                })
            ]))]
#[case::index_compound_assign(
            vec![
                token(TokenKind::Ident(SmolStr::new("arr"))),
                token(TokenKind::LBracket),
                token(TokenKind::NumberLiteral(0.into())),
                token(TokenKind::RBracket),
                token(TokenKind::PlusEqual),
                token(TokenKind::NumberLiteral(1.into())),
                token(TokenKind::Eof)
            ],
            Ok(vec![
                Shared::new(Node {
                    token_id: 3.into(),
                    expr: Expr::IndexAssign(
IdentWithToken::new_with_token("arr", Some(0.into())),
Shared::new(Node {
                                        token_id: 1.into(),
                                        expr: Expr::Literal(Literal::Number(0.into())),
                                    }),
Shared::new(Node {
                                        token_id: 3.into(),
                                        expr: Expr::BinaryOp(
    BinaryOp::Add,
                                                Shared::new(Node {
                                                    token_id: 2.into(),
                                                    expr: Expr::Call(
                                                        IdentWithToken::new_with_token(constants::builtins::GET, Some(0.into())),
                                                        smallvec![
                                                            Shared::new(Node {
                                                                token_id: 0.into(),
                                                                expr: Expr::Ident(IdentWithToken::new_with_token("arr", Some(0.into()))),
                                                            }),
                                                            Shared::new(Node {
                                                                token_id: 1.into(),
                                                                expr: Expr::Literal(Literal::Number(0.into())),
                                                            }),
                                                        ],
                                                    ),
                                                }),
                                                Shared::new(Node {
                                                    token_id: 4.into(),
                                                    expr: Expr::Literal(Literal::Number(1.into())),
                                                }),
                                            ),
                                    }),
),
                })
            ]))]
#[case::index_double_slash_equal(
            vec![
                token(TokenKind::Ident(SmolStr::new("arr"))),
                token(TokenKind::LBracket),
                token(TokenKind::NumberLiteral(0.into())),
                token(TokenKind::RBracket),
                token(TokenKind::DoubleSlashEqual),
                token(TokenKind::NumberLiteral(2.into())),
                token(TokenKind::Eof)
            ],
            // arr[0] //= 2  →  arr[0] = floor(div(get(arr, 0), 2))
            Ok(vec![
                Shared::new(Node {
                    token_id: 3.into(),
                    expr: Expr::IndexAssign(
IdentWithToken::new_with_token("arr", Some(0.into())),
Shared::new(Node {
                                        token_id: 1.into(),
                                        expr: Expr::Literal(Literal::Number(0.into())),
                                    }),
Shared::new(Node {
                                        token_id: 3.into(),
                                        expr: Expr::Call(
                                            IdentWithToken::new_with_token(constants::builtins::FLOOR, Some(3.into())),
                                            smallvec![Shared::new(Node {
                                                token_id: 3.into(),
                                                expr: Expr::BinaryOp(
    BinaryOp::Div,
                                                        Shared::new(Node {
                                                            token_id: 2.into(),
                                                            expr: Expr::Call(
                                                                IdentWithToken::new_with_token(constants::builtins::GET, Some(0.into())),
                                                                smallvec![
                                                                    Shared::new(Node {
                                                                        token_id: 0.into(),
                                                                        expr: Expr::Ident(IdentWithToken::new_with_token("arr", Some(0.into()))),
                                                                    }),
                                                                    Shared::new(Node {
                                                                        token_id: 1.into(),
                                                                        expr: Expr::Literal(Literal::Number(0.into())),
                                                                    }),
                                                                ],
                                                            ),
                                                        }),
                                                        Shared::new(Node {
                                                            token_id: 4.into(),
                                                            expr: Expr::Literal(Literal::Number(2.into())),
                                                        }),
                                                    ),
                                            })],
                                        ),
                                    }),
),
                })
            ]))]
#[case::root_semicolon_error(
            vec![
                token(TokenKind::Ident(SmolStr::new("x"))),
                token(TokenKind::SemiColon),
                token(TokenKind::Ident(SmolStr::new("y"))),
                token(TokenKind::Eof)
            ],
            Err(SyntaxError::UnexpectedToken(token(TokenKind::Ident(SmolStr::new("y"))))))]
#[case::if_1(
            vec![
                token(TokenKind::If),
                token(TokenKind::LParen),
                token(TokenKind::BoolLiteral(true)),
                token(TokenKind::RParen),
                token(TokenKind::Colon),
                token(TokenKind::StringLiteral("true branch".to_owned())),
                token(TokenKind::Else),
                token(TokenKind::Colon),
                token(TokenKind::StringLiteral("false branch".to_owned())),
                token(TokenKind::Eof)
            ],
            Ok(vec![
                Shared::new(Node {
                    token_id: 7.into(),
                    expr: Expr::If(vec![
                        (
                            Some(Shared::new(Node {
                                token_id: 1.into(),
                                expr: Expr::Literal(Literal::Bool(true)),
                            })),
                            Shared::new(Node {
                                token_id: 3.into(),
                                expr: Expr::Literal(Literal::String("true branch".to_owned())),
                            })
                        ),
                        (
                            None,
                            Shared::new(Node {
                                token_id: 6.into(),
                                expr: Expr::Literal(Literal::String("false branch".to_owned())),
                            })
                        )
                    ]),
                })
            ]))]
#[case::if_elif_else(
            vec![
                token(TokenKind::If),
                token(TokenKind::LParen),
                token(TokenKind::BoolLiteral(true)),
                token(TokenKind::RParen),
                token(TokenKind::Colon),
                token(TokenKind::StringLiteral("true branch".to_owned())),
                token(TokenKind::Elif),
                token(TokenKind::LParen),
                token(TokenKind::BoolLiteral(false)),
                token(TokenKind::RParen),
                token(TokenKind::Colon),
                token(TokenKind::StringLiteral("elif branch".to_owned())),
                token(TokenKind::Else),
                token(TokenKind::Colon),
                token(TokenKind::StringLiteral("else branch".to_owned())),
                token(TokenKind::Eof)
            ],
            Ok(vec![
                Shared::new(Node {
                    token_id: 11.into(),
                    expr: Expr::If(vec![
                        (
                            Some(Shared::new(Node {
                                token_id: 1.into(),
                                expr: Expr::Literal(Literal::Bool(true)),
                            })),
                            Shared::new(Node {
                                token_id: 3.into(),
                                expr: Expr::Literal(Literal::String("true branch".to_owned())),
                            })
                        ),
                        (
                            Some(Shared::new(Node {
                                token_id: 5.into(),
                                expr: Expr::Literal(Literal::Bool(false)),
                            })),
                            Shared::new(Node {
                                token_id: 7.into(),
                                expr: Expr::Literal(Literal::String("elif branch".to_owned())),
                            })
                        ),
                        (
                            None,
                            Shared::new(Node {
                                token_id: 10.into(),
                                expr: Expr::Literal(Literal::String("else branch".to_owned())),
                            })
                        )
                    ]),
                })
            ]))]
#[case::if_only(
            vec![
                token(TokenKind::If),
                token(TokenKind::LParen),
                token(TokenKind::BoolLiteral(true)),
                token(TokenKind::RParen),
                token(TokenKind::Colon),
                token(TokenKind::StringLiteral("true branch".to_owned())),
                token(TokenKind::Eof)
            ],
            Ok(vec![
                Shared::new(Node {
                    token_id: 4.into(),
                    expr: Expr::If(vec![
                        (
                            Some(Shared::new(Node {
                                token_id: 1.into(),
                                expr: Expr::Literal(Literal::Bool(true)),
                            })),
                            Shared::new(Node {
                                token_id: 3.into(),
                                expr: Expr::Literal(Literal::String("true branch".to_owned())),
                            })
                        ),
                    ]),
                })
            ]))]
#[case::if_elif(
            vec![
                token(TokenKind::If),
                token(TokenKind::LParen),
                token(TokenKind::BoolLiteral(true)),
                token(TokenKind::RParen),
                token(TokenKind::Colon),
                token(TokenKind::StringLiteral("true branch".to_owned())),
                token(TokenKind::Elif),
                token(TokenKind::LParen),
                token(TokenKind::BoolLiteral(true)),
                token(TokenKind::RParen),
                token(TokenKind::Colon),
                token(TokenKind::StringLiteral("true branch".to_owned())),
                token(TokenKind::Eof)
            ],
            Ok(vec![
                Shared::new(Node {
                    token_id: 8.into(),
                    expr: Expr::If(vec![
                        (
                            Some(Shared::new(Node {
                                token_id: 1.into(),
                                expr: Expr::Literal(Literal::Bool(true)),
                            })),
                            Shared::new(Node {
                                token_id: 3.into(),
                                expr: Expr::Literal(Literal::String("true branch".to_owned())),
                            })
                        ),
                        (
                            Some(Shared::new(Node {
                                token_id: 5.into(),
                                expr: Expr::Literal(Literal::Bool(true)),
                            })),
                            Shared::new(Node {
                                token_id: 7.into(),
                                expr: Expr::Literal(Literal::String("true branch".to_owned())),
                            })
                        ),
                    ]),
                })
            ]))]
#[case::if_error(
            vec![
                token(TokenKind::If),
                token(TokenKind::LParen),
                token(TokenKind::BoolLiteral(true)),
                token(TokenKind::RParen),
                token(TokenKind::Colon),
                token(TokenKind::StringLiteral("true branch".to_owned())),
                token(TokenKind::Elif),
                token(TokenKind::LParen),
                token(TokenKind::BoolLiteral(false)),
                token(TokenKind::RParen),
                token(TokenKind::Colon),
                token(TokenKind::StringLiteral("elif branch".to_owned())),
                token(TokenKind::Else),
                token(TokenKind::Colon),
                token(TokenKind::Eof)
            ],
            Err(SyntaxError::UnexpectedEOFDetected(0.into())))]
#[case::if_error(
            vec![
                token(TokenKind::If),
                token(TokenKind::LParen),
                token(TokenKind::RParen),
                token(TokenKind::Colon),
                token(TokenKind::StringLiteral("true branch".to_owned())),
                token(TokenKind::Elif),
                token(TokenKind::LParen),
                token(TokenKind::BoolLiteral(false)),
                token(TokenKind::RParen),
                token(TokenKind::Colon),
                token(TokenKind::StringLiteral("elif branch".to_owned())),
                token(TokenKind::Else),
                token(TokenKind::Colon),
                token(TokenKind::Eof)
            ],
            Err(SyntaxError::UnexpectedToken(Token{range: Range::default(), kind:TokenKind::If, module_id: 1.into()})))]
#[case::elif_error(
            vec![
                token(TokenKind::If),
                token(TokenKind::LParen),
                token(TokenKind::BoolLiteral(true)),
                token(TokenKind::RParen),
                token(TokenKind::Colon),
                token(TokenKind::StringLiteral("true branch".to_owned())),
                token(TokenKind::Elif),
                token(TokenKind::LParen),
                token(TokenKind::RParen),
                token(TokenKind::Colon),
                token(TokenKind::StringLiteral("elif branch".to_owned())),
                token(TokenKind::Else),
                token(TokenKind::Colon),
                token(TokenKind::Eof)
            ],
            Err(SyntaxError::UnexpectedToken(Token{range: Range::default(), kind:TokenKind::Elif, module_id: 1.into()})))]
#[case::h_selector(
        vec![
            token(TokenKind::Selector(SmolStr::new(".h"))),
            token(TokenKind::Eof)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 2.into(),
                expr: Expr::Selector(Selector::Heading(None)),
            })
        ]))]
#[case::h_selector_without_number(
        vec![
            token(TokenKind::Selector(SmolStr::new(".h"))),
            token(TokenKind::Eof)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 1.into(),
                expr: Expr::Selector(Selector::Heading(None)),
            })
        ]))]
#[case::while_(
        vec![
            token(TokenKind::While),
            token(TokenKind::LParen),
            token(TokenKind::BoolLiteral(true)),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::StringLiteral("loop body".to_owned())),
            token(TokenKind::SemiColon),
        ],
        Ok(vec![Shared::new(Node {
            token_id: 0.into(),
            expr: Expr::While(
                Shared::new(Node {
                    token_id: 1.into(),
                    expr: Expr::Literal(Literal::Bool(true)),
                }),
                vec![Shared::new(Node {
                    token_id: 3.into(),
                    expr: Expr::Literal(Literal::String("loop body".to_owned())),
                })],
            ),
        })]))]
#[case::while_error(
        vec![
            token(TokenKind::While),
            token(TokenKind::LParen),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::StringLiteral("loop body".to_owned())),
            token(TokenKind::SemiColon),
        ],
        Err(SyntaxError::UnexpectedToken(Token{range: Range::default(), kind:TokenKind::While, module_id: 1.into()})))]
#[case::while_error(
        vec![
            token(TokenKind::While),
            token(TokenKind::LParen),
            token(TokenKind::BoolLiteral(true)),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
        ],
        Err(SyntaxError::UnexpectedToken(Token{range: Range::default(), kind:TokenKind::While, module_id: 1.into()})))]
#[case::while_do_end(
        vec![
            token(TokenKind::While),
            token(TokenKind::LParen),
            token(TokenKind::BoolLiteral(true)),
            token(TokenKind::RParen),
            token(TokenKind::Do),
            token(TokenKind::StringLiteral("loop body".to_owned())),
            token(TokenKind::End),
        ],
        Ok(vec![Shared::new(Node {
            token_id: 0.into(),
            expr: Expr::While(
                Shared::new(Node {
                    token_id: 1.into(),
                    expr: Expr::Literal(Literal::Bool(true)),
                }),
                vec![Shared::new(Node {
                    token_id: 3.into(),
                    expr: Expr::Literal(Literal::String("loop body".to_owned())),
                })],
            ),
        })]))]
#[case::loop_(
        vec![
            token(TokenKind::Loop),
            token(TokenKind::Colon),
            token(TokenKind::StringLiteral("loop body".to_owned())),
            token(TokenKind::SemiColon),
        ],
        Ok(vec![Shared::new(Node {
            token_id: 1.into(),
            expr: Expr::Loop(
                vec![Shared::new(Node {
                    token_id: 2.into(),
                    expr: Expr::Literal(Literal::String("loop body".to_owned())),
                })],
            ),
        })]))]
#[case::loop_error_no_body(
        vec![
            token(TokenKind::Loop),
            token(TokenKind::Colon),
        ],
        Err(SyntaxError::UnexpectedToken(Token{range: Range::default(), kind:TokenKind::Loop, module_id: 1.into()})))]
#[case::until_(
        vec![
            token(TokenKind::Until),
            token(TokenKind::LParen),
            token(TokenKind::BoolLiteral(true)),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::StringLiteral("loop body".to_owned())),
            token(TokenKind::SemiColon),
        ],
        Ok(vec![Shared::new(Node {
            token_id: 0.into(),
            expr: Expr::Until(
                Shared::new(Node {
                    token_id: 1.into(),
                    expr: Expr::Literal(Literal::Bool(true)),
                }),
                vec![Shared::new(Node {
                    token_id: 3.into(),
                    expr: Expr::Literal(Literal::String("loop body".to_owned())),
                })],
            ),
        })]))]
#[case::until_error(
        vec![
            token(TokenKind::Until),
            token(TokenKind::LParen),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::StringLiteral("loop body".to_owned())),
            token(TokenKind::SemiColon),
        ],
        Err(SyntaxError::UnexpectedToken(Token{range: Range::default(), kind:TokenKind::Until, module_id: 1.into()})))]
#[case::unless_(
        vec![
            token(TokenKind::Unless),
            token(TokenKind::LParen),
            token(TokenKind::BoolLiteral(false)),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::StringLiteral("branch".to_owned())),
            token(TokenKind::Eof)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 4.into(),
                expr: Expr::Unless(vec![
                    (
                        Some(Shared::new(Node {
                            token_id: 1.into(),
                            expr: Expr::Literal(Literal::Bool(false)),
                        })),
                        Shared::new(Node {
                            token_id: 3.into(),
                            expr: Expr::Literal(Literal::String("branch".to_owned())),
                        })
                    ),
                ]),
            })
        ]))]
#[case::unless_error(
        vec![
            token(TokenKind::Unless),
            token(TokenKind::LParen),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::StringLiteral("branch".to_owned())),
            token(TokenKind::Eof)
        ],
        Err(SyntaxError::UnexpectedToken(Token{range: Range::default(), kind:TokenKind::Unless, module_id: 1.into()})))]
#[case::try_catch(
        vec![
            token(TokenKind::Try),
            token(TokenKind::Colon),
            token(TokenKind::Ident(SmolStr::new("error_expr"))),
            token(TokenKind::Catch),
            token(TokenKind::Colon),
            token(TokenKind::StringLiteral("fallback".to_owned())),
            token(TokenKind::Eof),
        ],
        Ok(vec![Shared::new(Node {
            token_id: 2.into(),
            expr: Expr::Try(
                Shared::new(Node {
                    token_id: 2.into(),
                    expr: Expr::Ident(IdentWithToken::new_with_token("error_expr", Some(2.into()))),
                }),
                None,
                Shared::new(Node {
                    token_id: 5.into(),
                    expr: Expr::Literal(Literal::String("fallback".to_owned())),
                }),
            ),
        })]))]
#[case::try_catch_with_binder(
        vec![
            token(TokenKind::Try),
            token(TokenKind::Colon),
            token(TokenKind::Ident(SmolStr::new("error_expr"))),
            token(TokenKind::Catch),
            token(TokenKind::LParen),
            token(TokenKind::Ident(SmolStr::new("e"))),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::Ident(SmolStr::new("e"))),
            token(TokenKind::Eof),
        ],
        Ok(vec![Shared::new(Node {
            token_id: 2.into(),
            expr: Expr::Try(
                Shared::new(Node {
                    token_id: 2.into(),
                    expr: Expr::Ident(IdentWithToken::new_with_token("error_expr", Some(2.into()))),
                }),
                Some(IdentWithToken::new_with_token("e", Some(4.into()))),
                Shared::new(Node {
                    token_id: 6.into(),
                    expr: Expr::Ident(IdentWithToken::new_with_token("e", Some(6.into()))),
                }),
            ),
        })]))]
#[case::foreach(
        vec![
            token(TokenKind::Foreach),
            token(TokenKind::LParen),
            token(TokenKind::Ident(SmolStr::new("item"))),
            token(TokenKind::Comma),
            token(TokenKind::StringLiteral("array".to_owned())),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::Ident(SmolStr::new("print"))),
            token(TokenKind::LParen),
            token(TokenKind::Ident(SmolStr::new("item"))),
            token(TokenKind::RParen),
            token(TokenKind::SemiColon),
        ],
        Ok(vec![Shared::new(Node {
            token_id: 6.into(),
            expr: Expr::Foreach(
                IdentWithToken::new_with_token(
                    "item",
                    Some(0.into()),
                ),
                Shared::new(Node {
                    token_id: 1.into(),
                    expr: Expr::Literal(Literal::String("array".to_owned())),
                }),
                vec![Shared::new(Node {
                    token_id: 4.into(),
                    expr: Expr::Call(
                        IdentWithToken::new_with_token(
                            "print",
                            Some(4.into()),
                        ),
                        smallvec![Shared::new(Node {
                            token_id: 3.into(),
                            expr: Expr::Ident(IdentWithToken::new_with_token(
                                "item",
                                Some(3.into()),
                            )),
                        })],
                    ),
                })],
            ),
        })]))]
#[case::foreach(
        vec![
            token(TokenKind::Foreach),
            token(TokenKind::LParen),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::Ident(SmolStr::new("print"))),
            token(TokenKind::LParen),
            token(TokenKind::Ident(SmolStr::new("item"))),
            token(TokenKind::RParen),
            token(TokenKind::SemiColon),
        ],
        Err(SyntaxError::UnexpectedToken(Token{range: Range::default(), kind:TokenKind::Foreach, module_id: 1.into()})))]
#[case::foreach_do_end(
        vec![
            token(TokenKind::Foreach),
            token(TokenKind::LParen),
            token(TokenKind::Ident(SmolStr::new("item"))),
            token(TokenKind::Comma),
            token(TokenKind::StringLiteral("array".to_owned())),
            token(TokenKind::RParen),
            token(TokenKind::Do),
            token(TokenKind::Ident(SmolStr::new("print"))),
            token(TokenKind::LParen),
            token(TokenKind::Ident(SmolStr::new("item"))),
            token(TokenKind::RParen),
            token(TokenKind::End),
        ],
        Ok(vec![Shared::new(Node {
            token_id: 6.into(),
            expr: Expr::Foreach(
                IdentWithToken::new_with_token(
                    "item",
                    Some(0.into()),
                ),
                Shared::new(Node {
                    token_id: 1.into(),
                    expr: Expr::Literal(Literal::String("array".to_owned())),
                }),
                vec![Shared::new(Node {
                    token_id: 4.into(),
                    expr: Expr::Call(
                        IdentWithToken::new_with_token(
                            "print",
                            Some(4.into()),
                        ),
                        smallvec![Shared::new(Node {
                            token_id: 3.into(),
                            expr: Expr::Ident(IdentWithToken::new_with_token(
                                "item",
                                Some(3.into()),
                            )),
                        })],
                    ),
                })],
            ),
        })]))]
#[case::self_(
        vec![token(TokenKind::Self_), token(TokenKind::Eof)],
        Ok(vec![Shared::new(Node {
            token_id: 0.into(),
            expr: Expr::Self_,
        })]))]
#[case::include(
        vec![
            token(TokenKind::Include),
            token(TokenKind::StringLiteral("module_name".to_owned())),
            token(TokenKind::Eof),
        ],
        Ok(vec![Shared::new(Node {
            token_id: 0.into(),
            expr: Expr::Include(Literal::String("module_name".to_owned())),
        })]))]
#[case::code_selector_with_language(
        vec![
            token(TokenKind::Selector(SmolStr::new(".code"))),
            token(TokenKind::Eof),
        ],
        Ok(vec![Shared::new(Node {
            token_id: 2.into(),
            expr: Expr::Selector(Selector::Code),
        })]))]
#[case::selector_call_heading_single_arg(
        vec![
            token(TokenKind::Selector(SmolStr::new(".h"))),
            token(TokenKind::LParen),
            token(TokenKind::NumberLiteral(1.into())),
            token(TokenKind::RParen),
            token(TokenKind::Eof),
        ],
        Ok(vec![Shared::new(Node {
            token_id: 1.into(),
            expr: Expr::SelectorCall(
                Selector::Heading(None),
                // arg literal is allocated first (id=0), then the selector token (id=1)
                Box::new(smallvec![Shared::new(Node {
                    token_id: 0.into(),
                    expr: Expr::Literal(Literal::Number(1.into())),
                })]),
            ),
        })]))]
#[case::selector_call_heading_multi_arg(
        vec![
            token(TokenKind::Selector(SmolStr::new(".h"))),
            token(TokenKind::LParen),
            token(TokenKind::NumberLiteral(1.into())),
            token(TokenKind::Comma),
            token(TokenKind::NumberLiteral(2.into())),
            token(TokenKind::RParen),
            token(TokenKind::Eof),
        ],
        Ok(vec![Shared::new(Node {
            token_id: 2.into(),
            expr: Expr::SelectorCall(
                Selector::Heading(None),
                // args are allocated first (id=0, id=1), then the selector token (id=2)
                Box::new(smallvec![
                    Shared::new(Node {
                        token_id: 0.into(),
                        expr: Expr::Literal(Literal::Number(1.into())),
                    }),
                    Shared::new(Node {
                        token_id: 1.into(),
                        expr: Expr::Literal(Literal::Number(2.into())),
                    }),
                ]),
            ),
        })]))]
#[case::selector_call_code_lang(
        vec![
            token(TokenKind::Selector(SmolStr::new(".code"))),
            token(TokenKind::LParen),
            token(TokenKind::StringLiteral("rust".to_owned())),
            token(TokenKind::RParen),
            token(TokenKind::Eof),
        ],
        Ok(vec![Shared::new(Node {
            token_id: 1.into(),
            expr: Expr::SelectorCall(
                Selector::Code,
                // arg literal is allocated first (id=0), then the selector token (id=1)
                Box::new(smallvec![Shared::new(Node {
                    token_id: 0.into(),
                    expr: Expr::Literal(Literal::String("rust".to_owned())),
                })]),
            ),
        })]))]
#[case::selector_call_with_attribute(
        vec![
            token(TokenKind::Selector(SmolStr::new(".h"))),
            token(TokenKind::LParen),
            token(TokenKind::NumberLiteral(1.into())),
            token(TokenKind::RParen),
            token(TokenKind::Selector(SmolStr::new(".level"))),
            token(TokenKind::Eof),
        ],
        Ok(vec![Shared::new(Node {
            // attr() Call: arg literal id=0, SelectorCall id=1, attr_literal id=2, Call id=3
            token_id: 3.into(),
            expr: Expr::Call(
                IdentWithToken::new_with_token(
                    constants::builtins::ATTR,
                    Some(1.into()),
                ),
                smallvec![
                    Shared::new(Node {
                        token_id: 1.into(),
                        expr: Expr::SelectorCall(
                            Selector::Heading(None),
                            Box::new(smallvec![Shared::new(Node {
                                token_id: 0.into(),
                                expr: Expr::Literal(Literal::Number(1.into())),
                            })]),
                        ),
                    }),
                    Shared::new(Node {
                        token_id: 2.into(),
                        expr: Expr::Literal(Literal::String("level".to_owned())),
                    }),
                ],
            ),
        })]))]
#[case::selector_call_code_with_lang_attribute(
        vec![
            token(TokenKind::Selector(SmolStr::new(".code"))),
            token(TokenKind::LParen),
            token(TokenKind::StringLiteral("rust".to_owned())),
            token(TokenKind::RParen),
            token(TokenKind::Selector(SmolStr::new(".lang"))),
            token(TokenKind::Eof),
        ],
        Ok(vec![Shared::new(Node {
            // attr() Call: arg literal id=0, SelectorCall id=1, attr_literal id=2, Call id=3
            token_id: 3.into(),
            expr: Expr::Call(
                IdentWithToken::new_with_token(
                    constants::builtins::ATTR,
                    Some(1.into()),
                ),
                smallvec![
                    Shared::new(Node {
                        token_id: 1.into(),
                        expr: Expr::SelectorCall(
                            Selector::Code,
                            Box::new(smallvec![Shared::new(Node {
                                token_id: 0.into(),
                                expr: Expr::Literal(Literal::String("rust".to_owned())),
                            })]),
                        ),
                    }),
                    Shared::new(Node {
                        token_id: 2.into(),
                        expr: Expr::Literal(Literal::String("lang".to_owned())),
                    }),
                ],
            ),
        })]))]
#[case::table_selector(
        vec![
            token(TokenKind::Selector(SmolStr::new("."))),
            token(TokenKind::LBracket),
            token(TokenKind::NumberLiteral(1.into())),
            token(TokenKind::RBracket),
            token(TokenKind::LBracket),
            token(TokenKind::NumberLiteral(2.into())),
            token(TokenKind::RBracket),
            token(TokenKind::Eof),
        ],
        Ok(vec![Shared::new(Node {
            token_id: 8.into(),
            expr: Expr::Selector(Selector::Table(Some(1), Some(2))),
        })]))]
#[case::selector_call_list_bracket_single_arg(
        vec![
            token(TokenKind::Selector(SmolStr::new("."))),
            token(TokenKind::LBracket),
            token(TokenKind::RBracket),
            token(TokenKind::LParen),
            token(TokenKind::NumberLiteral(2.into())),
            token(TokenKind::RParen),
            token(TokenKind::Eof),
        ],
        Ok(vec![Shared::new(Node {
            token_id: 2.into(),
            expr: Expr::SelectorCall(
                Selector::List(None, None),
                Box::new(smallvec![Shared::new(Node {
                    token_id: 1.into(),
                    expr: Expr::Literal(Literal::Number(2.into())),
                })]),
            ),
        })]))]
#[case::selector_call_table_bracket_single_arg(
        vec![
            token(TokenKind::Selector(SmolStr::new("."))),
            token(TokenKind::LBracket),
            token(TokenKind::RBracket),
            token(TokenKind::LBracket),
            token(TokenKind::RBracket),
            token(TokenKind::LParen),
            token(TokenKind::NumberLiteral(1.into())),
            token(TokenKind::RParen),
            token(TokenKind::Eof),
        ],
        Ok(vec![Shared::new(Node {
            token_id: 3.into(),
            expr: Expr::SelectorCall(
                Selector::Table(None, None),
                Box::new(smallvec![Shared::new(Node {
                    token_id: 2.into(),
                    expr: Expr::Literal(Literal::Number(1.into())),
                })]),
            ),
        })]))]
#[case::selector_call_list_bracket_variable(
        vec![
            token(TokenKind::Selector(SmolStr::new("."))),
            token(TokenKind::LBracket),
            token(TokenKind::Ident(SmolStr::new("v"))),
            token(TokenKind::RBracket),
            token(TokenKind::Eof),
        ],
        Ok(vec![Shared::new(Node {
            token_id: 3.into(),
            expr: Expr::SelectorCall(
                Selector::List(None, None),
                Box::new(smallvec![Shared::new(Node {
                    token_id: 1.into(),
                    expr: Expr::Ident(IdentWithToken::new_with_token(
                        "v",
                        Some(1.into()),
                    )),
                })]),
            ),
        })]))]
#[case::selector_call_table_bracket_column_variable(
        vec![
            token(TokenKind::Selector(SmolStr::new("."))),
            token(TokenKind::LBracket),
            token(TokenKind::RBracket),
            token(TokenKind::LBracket),
            token(TokenKind::Ident(SmolStr::new("v"))),
            token(TokenKind::RBracket),
            token(TokenKind::Eof),
        ],
        Ok(vec![Shared::new(Node {
            token_id: 5.into(),
            expr: Expr::SelectorCall(
                Selector::Table(None, None),
                Box::new(smallvec![
                    Shared::new(Node {
                        token_id: 4.into(),
                        expr: Expr::Literal(Literal::None),
                    }),
                    Shared::new(Node {
                        token_id: 2.into(),
                        expr: Expr::Ident(IdentWithToken::new_with_token(
                            "v",
                            Some(2.into()),
                        )),
                    }),
                ]),
            ),
        })]))]
#[case::selector_call_table_bracket_variable(
        vec![
            token(TokenKind::Selector(SmolStr::new("."))),
            token(TokenKind::LBracket),
            token(TokenKind::Ident(SmolStr::new("v"))),
            token(TokenKind::RBracket),
            token(TokenKind::LBracket),
            token(TokenKind::RBracket),
            token(TokenKind::Eof),
        ],
        Ok(vec![Shared::new(Node {
            token_id: 4.into(),
            expr: Expr::SelectorCall(
                Selector::Table(None, None),
                Box::new(smallvec![Shared::new(Node {
                    token_id: 1.into(),
                    expr: Expr::Ident(IdentWithToken::new_with_token(
                        "v",
                        Some(1.into()),
                    )),
                })]),
            ),
        })]))]
#[case::selector_call_table_bracket_row_col_args(
        vec![
            token(TokenKind::Selector(SmolStr::new("."))),
            token(TokenKind::LBracket),
            token(TokenKind::RBracket),
            token(TokenKind::LBracket),
            token(TokenKind::RBracket),
            token(TokenKind::LParen),
            token(TokenKind::NumberLiteral(1.into())),
            token(TokenKind::Comma),
            token(TokenKind::NumberLiteral(2.into())),
            token(TokenKind::RParen),
            token(TokenKind::Eof),
        ],
        Ok(vec![Shared::new(Node {
            token_id: 4.into(),
            expr: Expr::SelectorCall(
                Selector::Table(None, None),
                Box::new(smallvec![
                    Shared::new(Node {
                        token_id: 2.into(),
                        expr: Expr::Literal(Literal::Number(1.into())),
                    }),
                    Shared::new(Node {
                        token_id: 3.into(),
                        expr: Expr::Literal(Literal::Number(2.into())),
                    }),
                ]),
            ),
        })]))]
#[case::foreach_error(
        vec![
            token(TokenKind::Foreach),
            token(TokenKind::LParen),
            token(TokenKind::Ident(SmolStr::new("item"))),
            token(TokenKind::Comma),
            token(TokenKind::StringLiteral("array".to_owned())),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::Eof),
        ],
        Err(SyntaxError::UnexpectedEOFDetected(Module::TOP_LEVEL_MODULE_ID)))]
#[case::while_error(
        vec![
            token(TokenKind::While),
            token(TokenKind::LParen),
            token(TokenKind::BoolLiteral(true)),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::Eof),
        ],
        Err(SyntaxError::UnexpectedEOFDetected(Module::TOP_LEVEL_MODULE_ID)))]
#[case::using_reserved_keyword_let(
            vec![
                token(TokenKind::Let),
                token(TokenKind::If),  // Using "if" as a variable name (should error)
                token(TokenKind::Equal),
                token(TokenKind::NumberLiteral(42.into())),
                token(TokenKind::Eof)
            ],
            Err(SyntaxError::UnexpectedToken(Token{range: Range::default(), kind: TokenKind::If, module_id: 1.into()})))]
#[case::using_reserved_keyword_while(
            vec![
                token(TokenKind::Let),
                token(TokenKind::While),  // Using "while" as a variable name (should error)
                token(TokenKind::Equal),
                token(TokenKind::NumberLiteral(42.into())),
                token(TokenKind::Eof)
            ],
            Err(SyntaxError::UnexpectedToken(Token{range: Range::default(), kind: TokenKind::While, module_id: 1.into()})))]
#[case::using_reserved_keyword_def(
            vec![
                token(TokenKind::Let),
                token(TokenKind::Def),  // Using "def" as a variable name (should error)
                token(TokenKind::Equal),
                token(TokenKind::NumberLiteral(42.into())),
                token(TokenKind::Eof)
            ],
            Err(SyntaxError::UnexpectedToken(Token{range: Range::default(), kind: TokenKind::Def, module_id: 1.into()})))]
#[case::using_reserved_keyword_include(
            vec![
                token(TokenKind::Let),
                token(TokenKind::Include),  // Using "include" as a variable name (should error)
                token(TokenKind::Equal),
                token(TokenKind::NumberLiteral(42.into())),
                token(TokenKind::Eof)
            ],
            Err(SyntaxError::UnexpectedToken(Token{range: Range::default(), kind: TokenKind::Include, module_id: 1.into()})))]
#[case::nodes(
        vec![
            token(TokenKind::Nodes),
            token(TokenKind::Eof)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Expr::Nodes,
            })
        ]))]
#[case::nodes_error_in_subprogram(
        vec![
            token(TokenKind::Def),
            token(TokenKind::Ident(SmolStr::new("test"))),
            token(TokenKind::LParen),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::Nodes),
            token(TokenKind::SemiColon)
        ],
        Err(SyntaxError::UnexpectedToken(token(TokenKind::Nodes))))]
#[case::nodes_then_selector(
        vec![
            token(TokenKind::Nodes),
            token(TokenKind::Pipe),
            token(TokenKind::Selector(SmolStr::new(".h1"))),
            token(TokenKind::Eof)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Expr::Nodes,
            }),
            Shared::new(Node {
                token_id: 1.into(),
                expr: Expr::Selector(Selector::Heading(Some(1))),
            })
        ]))]
#[case::root_level_with_multiple_pipes(
        vec![
            token(TokenKind::Nodes),
            token(TokenKind::Pipe),
            token(TokenKind::Nodes),
            token(TokenKind::Pipe),
            token(TokenKind::Selector(SmolStr::new(".h1"))),
            token(TokenKind::Pipe),
            token(TokenKind::Selector(SmolStr::new(".text"))),
            token(TokenKind::Eof)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Expr::Nodes,
            }),
            Shared::new(Node {
                token_id: 1.into(),
                expr: Expr::Nodes,
            }),
            Shared::new(Node {
                token_id: 2.into(),
                expr: Expr::Selector(Selector::Heading(Some(1))),
            }),
            Shared::new(Node {
                token_id: 3.into(),
                expr: Expr::Selector(Selector::Text),
            })
        ]))]
#[case::fn_simple(
        vec![
            token(TokenKind::Fn),
            token(TokenKind::LParen),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::StringLiteral("result".to_owned())),
            token(TokenKind::SemiColon),
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Expr::Fn(
                    Vec::new(),
                    vec![
                        Shared::new(Node {
                            token_id: 2.into(),
                            expr: Expr::Literal(Literal::String("result".to_owned())),
                        })
                    ],
                ),
            })
        ]))]
#[case::fn_with_args(
        vec![
            token(TokenKind::Fn),
            token(TokenKind::LParen),
            token(TokenKind::Ident(SmolStr::new("x"))),
            token(TokenKind::Comma),
            token(TokenKind::Ident(SmolStr::new("y"))),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::Ident(SmolStr::new("contains"))),
            token(TokenKind::LParen),
            token(TokenKind::Ident(SmolStr::new("x"))),
            token(TokenKind::Comma),
            token(TokenKind::Ident(SmolStr::new("y"))),
            token(TokenKind::RParen),
            token(TokenKind::SemiColon),
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Expr::Fn(
                    vec![
                        Param::new(IdentWithToken::new_with_token("x", Some(1.into()))),
                        Param::new(IdentWithToken::new_with_token("y", Some(2.into()))),
                    ],
                    vec![
                        Shared::new(Node {
                            token_id: 6.into(),
                            expr: Expr::Call(
                                IdentWithToken::new_with_token("contains", Some(6.into())),
                                smallvec![
                                    Shared::new(Node {
                                        token_id: 4.into(),
                                        expr: Expr::Ident(IdentWithToken::new_with_token("x", Some(4.into()))),
                                    }),
                                    Shared::new(Node {
                                        token_id: 5.into(),
                                        expr: Expr::Ident(IdentWithToken::new_with_token("y", Some(5.into()))),
                                    }),
                                ],
                            ),
                        })
                    ],
                ),
            })
        ]))]
#[case::fn_with_multiple_statements(
        vec![
            token(TokenKind::Fn),
            token(TokenKind::LParen),
            token(TokenKind::Ident(SmolStr::new("x"))),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::StringLiteral("first".to_owned())),
            token(TokenKind::Pipe),
            token(TokenKind::StringLiteral("second".to_owned())),
            token(TokenKind::SemiColon),
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Expr::Fn(
                    vec![
                        Param::new(IdentWithToken::new_with_token("x", Some(1.into()))),
                    ],
                    vec![
                        Shared::new(Node {
                            token_id: 3.into(),
                            expr: Expr::Literal(Literal::String("first".to_owned())),
                        }),
                        Shared::new(Node {
                            token_id: 4.into(),
                            expr: Expr::Literal(Literal::String("second".to_owned())),
                        })
                    ],
                ),
            })
        ]))]
#[case::fn_with_invalid_args(
        vec![
            token(TokenKind::Fn),
            token(TokenKind::LParen),
            token(TokenKind::StringLiteral("invalid".to_owned())),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::StringLiteral("result".to_owned())),
            token(TokenKind::SemiColon),
        ],
        Err(SyntaxError::UnexpectedToken(token(TokenKind::StringLiteral("invalid".to_owned())))))]
#[case::fn_without_body(
        vec![
            token(TokenKind::Fn),
            token(TokenKind::LParen),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::SemiColon),
        ],
        Err(SyntaxError::UnexpectedToken(token(TokenKind::SemiColon))))]
#[case::fn_nested_in_call(
        vec![
            token(TokenKind::Ident(SmolStr::new("apply"))),
            token(TokenKind::LParen),
            token(TokenKind::Fn),
            token(TokenKind::LParen),
            token(TokenKind::Ident(SmolStr::new("x"))),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::StringLiteral("processed".to_owned())),
            token(TokenKind::SemiColon),
            token(TokenKind::RParen),
            token(TokenKind::Eof),
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 4.into(),
                expr: Expr::Call(
                    IdentWithToken::new_with_token("apply", Some(4.into())),
                    smallvec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Expr::Fn(
                                vec![
                                  Param::new(IdentWithToken::new_with_token("x", Some(1.into()))),
                                ],
                                vec![
                                    Shared::new(Node {
                                        token_id: 3.into(),
                                        expr: Expr::Literal(Literal::String("processed".to_owned())),
                                    })
                                ],
                            ),
                        })
                    ],
                ),
            })
        ]))]
#[case::empty_array(
                vec![
                    token(TokenKind::LBracket),
                    token(TokenKind::RBracket),
                    token(TokenKind::Eof)
                ],
                Ok(vec![
                    Shared::new(Node {
                        token_id: 0.into(),
                        expr: Expr::Array(SmallVec::new()),
                    })
                ]))]
#[case::array_with_elements(
                vec![
                    token(TokenKind::LBracket),
                    token(TokenKind::StringLiteral("first".to_owned())),
                    token(TokenKind::Comma),
                    token(TokenKind::NumberLiteral(42.into())),
                    token(TokenKind::RBracket),
                    token(TokenKind::Eof)
                ],
                Ok(vec![
                    Shared::new(Node {
                        token_id: 0.into(),
                        expr: Expr::Array(
                            smallvec![
                                Shared::new(Node {
                                    token_id: 1.into(),
                                    expr: Expr::Literal(Literal::String("first".to_owned())),
                                }),
                                Shared::new(Node {
                                    token_id: 2.into(),
                                    expr: Expr::Literal(Literal::Number(42.into())),
                                }),
                            ],
                        ),
                    })
                ]))]
#[case::array_with_mixed_elements(
                vec![
                    token(TokenKind::LBracket),
                    token(TokenKind::StringLiteral("text".to_owned())),
                    token(TokenKind::Comma),
                    token(TokenKind::BoolLiteral(true)),
                    token(TokenKind::Comma),
                    token(TokenKind::None),
                    token(TokenKind::RBracket),
                    token(TokenKind::Eof)
                ],
                Ok(vec![
                    Shared::new(Node {
                        token_id: 0.into(),
                        expr: Expr::Array(
                            smallvec![
                                Shared::new(Node {
                                    token_id: 1.into(),
                                    expr: Expr::Literal(Literal::String("text".to_owned())),
                                }),
                                Shared::new(Node {
                                    token_id: 2.into(),
                                    expr: Expr::Literal(Literal::Bool(true)),
                                }),
                                Shared::new(Node {
                                    token_id: 3.into(),
                                    expr: Expr::Literal(Literal::None),
                                }),
                            ],
                        ),
                    })
                ]))]
#[case::array_with_nested_array(
                vec![
                    token(TokenKind::LBracket),
                    token(TokenKind::LBracket),
                    token(TokenKind::NumberLiteral(1.into())),
                    token(TokenKind::RBracket),
                    token(TokenKind::Comma),
                    token(TokenKind::LBracket),
                    token(TokenKind::NumberLiteral(2.into())),
                    token(TokenKind::RBracket),
                    token(TokenKind::RBracket),
                    token(TokenKind::Eof)
                ],
                Ok(vec![
                    Shared::new(Node {
                        token_id: 0.into(),
                        expr: Expr::Array(
                            smallvec![
                                Shared::new(Node {
                                    token_id: 1.into(),
                                    expr: Expr::Array(
                                        smallvec![
                                            Shared::new(Node {
                                                token_id: 2.into(),
                                                expr: Expr::Literal(Literal::Number(1.into())),
                                            }),
                                        ],
                                    ),
                                }),
                                Shared::new(Node {
                                    token_id: 3.into(),
                                    expr: Expr::Array(
                                        smallvec![
                                            Shared::new(Node {
                                                token_id: 4.into(),
                                                expr: Expr::Literal(Literal::Number(2.into())),
                                            }),
                                        ],
                                    ),
                                }),
                            ],
                        ),
                    })
                ]))]
#[case::array_with_trailing_comma(
                vec![
                    token(TokenKind::LBracket),
                    token(TokenKind::StringLiteral("value".to_owned())),
                    token(TokenKind::Comma),
                    token(TokenKind::RBracket),
                    token(TokenKind::Eof)
                ],
                Ok(vec![
                    Shared::new(Node {
                        token_id: 0.into(),
                        expr: Expr::Array(
                            smallvec![
                                Shared::new(Node {
                                    token_id: 1.into(),
                                    expr: Expr::Literal(Literal::String("value".to_owned())),
                                }),
                            ],
                        ),
                    })
                ]))]
#[case::array_unclosed(
                    vec![
                        token(TokenKind::LBracket),
                        token(TokenKind::StringLiteral("value".to_owned())),
                        token(TokenKind::Eof)
                    ],
                    Err(SyntaxError::ExpectedClosingBracket(token(TokenKind::Eof), Some(Box::new(token(TokenKind::LBracket))))))]
#[case::array_invalid_token(
                    vec![
                        token(TokenKind::LBracket),
                        token(TokenKind::Pipe),
                        token(TokenKind::RBracket),
                        token(TokenKind::Eof)
                    ],
                    Err(SyntaxError::UnexpectedToken(token(TokenKind::Pipe))))]
#[case::array_nested_unclosed(
                    vec![
                        token(TokenKind::LBracket),
                        token(TokenKind::LBracket),
                        token(TokenKind::StringLiteral("inner".to_owned())),
                        token(TokenKind::RBracket),
                        token(TokenKind::Eof)
                    ],
                    Err(SyntaxError::ExpectedClosingBracket(token(TokenKind::Eof), Some(Box::new(token(TokenKind::LBracket))))))]
#[case::array_with_ident(
                    vec![
                        token(TokenKind::LBracket),
                        token(TokenKind::Ident(SmolStr::new("foo"))),
                        token(TokenKind::Comma),
                        token(TokenKind::Ident(SmolStr::new("bar"))),
                        token(TokenKind::RBracket),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Expr::Array(
                                smallvec![
                                    Shared::new(Node {
                                        token_id: 1.into(),
                                        expr: Expr::Ident(IdentWithToken::new_with_token("foo", Some(1.into()))),
                                    }),
                                    Shared::new(Node {
                                        token_id: 2.into(),
                                        expr: Expr::Ident(IdentWithToken::new_with_token("bar", Some(2.into()))),
                                    }),
                                ],
                            ),
                        })
                    ]))]
#[case::equality_simple(
                    vec![
                        token(TokenKind::StringLiteral("hello".to_owned())),
                        token(TokenKind::EqEq),
                        token(TokenKind::StringLiteral("world".to_owned())),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Expr::BinaryOp(
    BinaryOp::Eq,
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Expr::Literal(Literal::String("hello".to_owned())),
                                    }),
                                    Shared::new(Node {
                                        token_id: 2.into(),
                                        expr: Expr::Literal(Literal::String("world".to_owned())),
                                    }),
                                ),
                        })
                    ]))]
#[case::equality_numbers(
                    vec![
                        token(TokenKind::NumberLiteral(42.into())),
                        token(TokenKind::EqEq),
                        token(TokenKind::NumberLiteral(42.into())),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Expr::BinaryOp(
    BinaryOp::Eq,
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Expr::Literal(Literal::Number(42.into())),
                                    }),
                                    Shared::new(Node {
                                        token_id: 2.into(),
                                        expr: Expr::Literal(Literal::Number(42.into())),
                                    }),
                                ),
                        })
                    ]))]
#[case::equality_booleans(
                    vec![
                        token(TokenKind::BoolLiteral(true)),
                        token(TokenKind::EqEq),
                        token(TokenKind::BoolLiteral(false)),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Expr::BinaryOp(
    BinaryOp::Eq,
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Expr::Literal(Literal::Bool(true)),
                                    }),
                                    Shared::new(Node {
                                        token_id: 2.into(),
                                        expr: Expr::Literal(Literal::Bool(false)),
                                    }),
                                ),
                        })
                    ]))]
#[case::equality_with_identifiers(
                    vec![
                        token(TokenKind::Ident(SmolStr::new("x"))),
                        token(TokenKind::EqEq),
                        token(TokenKind::Ident(SmolStr::new("y"))),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Expr::BinaryOp(
    BinaryOp::Eq,
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Expr::Ident(IdentWithToken::new_with_token("x", Some(0.into()))),
                                    }),
                                    Shared::new(Node {
                                        token_id: 2.into(),
                                        expr: Expr::Ident(IdentWithToken::new_with_token("y", Some(2.into()))),
                                    }),
                                ),
                        })
                    ]))]
#[case::equality_with_function_call(
                    vec![
                        token(TokenKind::Ident(SmolStr::new("foo"))),
                        token(TokenKind::LParen),
                        token(TokenKind::StringLiteral("arg".to_owned())),
                        token(TokenKind::RParen),
                        token(TokenKind::EqEq),
                        token(TokenKind::StringLiteral("result".to_owned())),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 2.into(),
                            expr: Expr::BinaryOp(
    BinaryOp::Eq,
                                    Shared::new(Node {
                                        token_id: 1.into(),
                                        expr: Expr::Call(
                                            IdentWithToken::new_with_token("foo", Some(1.into())),
                                            smallvec![
                                                Shared::new(Node {
                                                    token_id: 0.into(),
                                                    expr: Expr::Literal(Literal::String("arg".to_owned())),
                                                }),
                                            ],
                                        ),
                                    }),
                                    Shared::new(Node {
                                        token_id: 3.into(),
                                        expr: Expr::Literal(Literal::String("result".to_owned())),
                                    }),
                                ),
                        })
                    ]))]
#[case::equality_with_selectors(
                    vec![
                        token(TokenKind::Selector(SmolStr::new(".h1"))),
                        token(TokenKind::EqEq),
                        token(TokenKind::Selector(SmolStr::new(".text"))),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Expr::BinaryOp(
    BinaryOp::Eq,
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Expr::Selector(Selector::Heading(Some(1))),
                                    }),
                                    Shared::new(Node {
                                        token_id: 2.into(),
                                        expr: Expr::Selector(Selector::Text),
                                    }),
                                ),
                        })
                    ]))]
#[case::equality_with_none(
                    vec![
                        token(TokenKind::None),
                        token(TokenKind::EqEq),
                        token(TokenKind::None),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Expr::BinaryOp(
    BinaryOp::Eq,
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Expr::Literal(Literal::None),
                                    }),
                                    Shared::new(Node {
                                        token_id: 2.into(),
                                        expr: Expr::Literal(Literal::None),
                                    }),
                                ),
                        })
                    ]))]
#[case::equality_error_missing_rhs(
                    vec![
                        token(TokenKind::StringLiteral("hello".to_owned())),
                        token(TokenKind::EqEq),
                        token(TokenKind::Eof)
                    ],
                    Err(SyntaxError::UnexpectedEOFAfterToken(token(TokenKind::EqEq))))]
#[case::equality_in_if_condition(
                    vec![
                        token(TokenKind::If),
                        token(TokenKind::LParen),
                        token(TokenKind::Ident(SmolStr::new("x"))),
                        token(TokenKind::EqEq),
                        token(TokenKind::NumberLiteral(5.into())),
                        token(TokenKind::RParen),
                        token(TokenKind::Colon),
                        token(TokenKind::StringLiteral("equal".to_owned())),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 6.into(),
                            expr: Expr::If(vec![
                                (
                                    Some(Shared::new(Node {
                                        token_id: 2.into(),
                                        expr: Expr::BinaryOp(
    BinaryOp::Eq,
                                                Shared::new(Node {
                                                    token_id: 1.into(),
                                                    expr: Expr::Ident(IdentWithToken::new_with_token("x", Some(1.into()))),
                                                }),
                                                Shared::new(Node {
                                                    token_id: 3.into(),
                                                    expr: Expr::Literal(Literal::Number(5.into())),
                                                }),
                                            ),
                                    })),
                                    Shared::new(Node {
                                        token_id: 5.into(),
                                        expr: Expr::Literal(Literal::String("equal".to_owned())),
                                    })
                                ),
                            ]),
                        })
                    ]))]
#[case::not_equality_simple(
                    vec![
                        token(TokenKind::StringLiteral("hello".to_owned())),
                        token(TokenKind::NeEq),
                        token(TokenKind::StringLiteral("world".to_owned())),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Expr::BinaryOp(
    BinaryOp::Ne,
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Expr::Literal(Literal::String("hello".to_owned())),
                                    }),
                                    Shared::new(Node {
                                        token_id: 2.into(),
                                        expr: Expr::Literal(Literal::String("world".to_owned())),
                                    }),
                                ),
                        })
                    ]))]
#[case::not_equality_numbers(
                    vec![
                        token(TokenKind::NumberLiteral(42.into())),
                        token(TokenKind::NeEq),
                        token(TokenKind::NumberLiteral(24.into())),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Expr::BinaryOp(
    BinaryOp::Ne,
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Expr::Literal(Literal::Number(42.into())),
                                    }),
                                    Shared::new(Node {
                                        token_id: 2.into(),
                                        expr: Expr::Literal(Literal::Number(24.into())),
                                    }),
                                ),
                        })
                    ]))]
#[case::not_equality_booleans(
                    vec![
                        token(TokenKind::BoolLiteral(true)),
                        token(TokenKind::NeEq),
                        token(TokenKind::BoolLiteral(false)),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Expr::BinaryOp(
    BinaryOp::Ne,
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Expr::Literal(Literal::Bool(true)),
                                    }),
                                    Shared::new(Node {
                                        token_id: 2.into(),
                                        expr: Expr::Literal(Literal::Bool(false)),
                                    }),
                                ),
                        })
                    ]))]
#[case::not_equality_with_identifiers(
                    vec![
                        token(TokenKind::Ident(SmolStr::new("x"))),
                        token(TokenKind::NeEq),
                        token(TokenKind::Ident(SmolStr::new("y"))),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Expr::BinaryOp(
    BinaryOp::Ne,
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Expr::Ident(IdentWithToken::new_with_token("x", Some(0.into()))),
                                    }),
                                    Shared::new(Node {
                                        token_id: 2.into(),
                                        expr: Expr::Ident(IdentWithToken::new_with_token("y", Some(2.into()))),
                                    }),
                                ),
                        })
                    ]))]
#[case::not_equality_with_function_call(
                    vec![
                        token(TokenKind::Ident(SmolStr::new("foo"))),
                        token(TokenKind::LParen),
                        token(TokenKind::StringLiteral("arg".to_owned())),
                        token(TokenKind::RParen),
                        token(TokenKind::NeEq),
                        token(TokenKind::StringLiteral("result".to_owned())),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 2.into(),
                            expr: Expr::BinaryOp(
    BinaryOp::Ne,
                                    Shared::new(Node {
                                        token_id: 1.into(),
                                        expr: Expr::Call(
                                            IdentWithToken::new_with_token("foo", Some(1.into())),
                                            smallvec![
                                                Shared::new(Node {
                                                    token_id: 0.into(),
                                                    expr: Expr::Literal(Literal::String("arg".to_owned())),
                                                }),
                                            ],
                                        ),
                                    }),
                                    Shared::new(Node {
                                        token_id: 3.into(),
                                        expr: Expr::Literal(Literal::String("result".to_owned())),
                                    }),
                                ),
                        })
                    ]))]
#[case::not_equality_with_selectors(
                    vec![
                        token(TokenKind::Selector(SmolStr::new(".h1"))),
                        token(TokenKind::NeEq),
                        token(TokenKind::Selector(SmolStr::new(".text"))),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Expr::BinaryOp(
    BinaryOp::Ne,
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Expr::Selector(Selector::Heading(Some(1))),
                                    }),
                                    Shared::new(Node {
                                        token_id: 2.into(),
                                        expr: Expr::Selector(Selector::Text),
                                    }),
                                ),
                        })
                    ]))]
#[case::not_equality_with_none(
                    vec![
                        token(TokenKind::None),
                        token(TokenKind::NeEq),
                        token(TokenKind::StringLiteral("something".to_owned())),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Expr::BinaryOp(
    BinaryOp::Ne,
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Expr::Literal(Literal::None),
                                    }),
                                    Shared::new(Node {
                                        token_id: 2.into(),
                                        expr: Expr::Literal(Literal::String("something".to_owned())),
                                    }),
                                ),
                        })
                    ]))]
#[case::not_equality_error_missing_rhs(
                    vec![
                        token(TokenKind::StringLiteral("hello".to_owned())),
                        token(TokenKind::NeEq),
                        token(TokenKind::Eof)
                    ],
                    Err(SyntaxError::UnexpectedEOFAfterToken(token(TokenKind::NeEq))))]
#[case::not_equality_in_if_condition(
                    vec![
                        token(TokenKind::If),
                        token(TokenKind::LParen),
                        token(TokenKind::Ident(SmolStr::new("x"))),
                        token(TokenKind::NeEq),
                        token(TokenKind::NumberLiteral(5.into())),
                        token(TokenKind::RParen),
                        token(TokenKind::Colon),
                        token(TokenKind::StringLiteral("not equal".to_owned())),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 6.into(),
                            expr: Expr::If(vec![
                                (
                                    Some(Shared::new(Node {
                                        token_id: 2.into(),
                                        expr: Expr::BinaryOp(
    BinaryOp::Ne,
                                                Shared::new(Node {
                                                    token_id: 1.into(),
                                                    expr: Expr::Ident(IdentWithToken::new_with_token("x", Some(1.into()))),
                                                }),
                                                Shared::new(Node {
                                                    token_id: 3.into(),
                                                    expr: Expr::Literal(Literal::Number(5.into())),
                                                }),
                                            ),
                                    })),
                                    Shared::new(Node {
                                        token_id: 5.into(),
                                        expr: Expr::Literal(Literal::String("not equal".to_owned())),
                                    })
                                ),
                            ]),
                        })
                    ]))]
#[case::plus_simple(
                    vec![
                        token(TokenKind::NumberLiteral(1.into())),
                        token(TokenKind::Plus),
                        token(TokenKind::NumberLiteral(2.into())),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Expr::BinaryOp(
    BinaryOp::Add,
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Expr::Literal(Literal::Number(1.into())),
                                    }),
                                    Shared::new(Node {
                                        token_id: 2.into(),
                                        expr: Expr::Literal(Literal::Number(2.into())),
                                    }),
                                ),
                        })
                    ]))]
#[case::plus_with_identifiers(
                    vec![
                        token(TokenKind::Ident(SmolStr::new("x"))),
                        token(TokenKind::Plus),
                        token(TokenKind::Ident(SmolStr::new("y"))),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Expr::BinaryOp(
    BinaryOp::Add,
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Expr::Ident(IdentWithToken::new_with_token("x", Some(0.into()))),
                                    }),
                                    Shared::new(Node {
                                        token_id: 2.into(),
                                        expr: Expr::Ident(IdentWithToken::new_with_token("y", Some(2.into()))),
                                    }),
                                ),
                        })
                    ]))]
#[case::plus_error_missing_rhs(
                    vec![
                        token(TokenKind::NumberLiteral(1.into())),
                        token(TokenKind::Plus),
                        token(TokenKind::Eof)
                    ],
                    Err(SyntaxError::UnexpectedEOFAfterToken(token(TokenKind::Plus))))]
#[case::lt_simple(
                    vec![
                        token(TokenKind::NumberLiteral(1.into())),
                        token(TokenKind::Lt),
                        token(TokenKind::NumberLiteral(2.into())),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Expr::BinaryOp(
    BinaryOp::Lt,
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Expr::Literal(Literal::Number(1.into())),
                                    }),
                                    Shared::new(Node {
                                        token_id: 2.into(),
                                        expr: Expr::Literal(Literal::Number(2.into())),
                                    }),
                                ),
                        })
                    ]))]
#[case::lte_simple(
                    vec![
                        token(TokenKind::NumberLiteral(1.into())),
                        token(TokenKind::Lte),
                        token(TokenKind::NumberLiteral(2.into())),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Expr::BinaryOp(
    BinaryOp::Le,
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Expr::Literal(Literal::Number(1.into())),
                                    }),
                                    Shared::new(Node {
                                        token_id: 2.into(),
                                        expr: Expr::Literal(Literal::Number(2.into())),
                                    }),
                                ),
                        })
                    ]))]
#[case::gt_simple(
                    vec![
                        token(TokenKind::NumberLiteral(3.into())),
                        token(TokenKind::Gt),
                        token(TokenKind::NumberLiteral(2.into())),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Expr::BinaryOp(
    BinaryOp::Gt,
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Expr::Literal(Literal::Number(3.into())),
                                    }),
                                    Shared::new(Node {
                                        token_id: 2.into(),
                                        expr: Expr::Literal(Literal::Number(2.into())),
                                    }),
                                ),
                        })
                    ]))]
#[case::gte_simple(
                    vec![
                        token(TokenKind::NumberLiteral(3.into())),
                        token(TokenKind::Gte),
                        token(TokenKind::NumberLiteral(2.into())),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Expr::BinaryOp(
    BinaryOp::Ge,
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Expr::Literal(Literal::Number(3.into())),
                                    }),
                                    Shared::new(Node {
                                        token_id: 2.into(),
                                        expr: Expr::Literal(Literal::Number(2.into())),
                                    }),
                                ),
                        })
                    ]))]
#[case::dict_empty(
                        vec![
                            token(TokenKind::LBrace),
                            token(TokenKind::RBrace),
                            token(TokenKind::Eof)
                        ],
                        Ok(vec![
                            Shared::new(Node {
                                token_id: 0.into(),
                                expr: Expr::Dict(SmallVec::new()),
                            })
                        ]))]
#[case::dict_single_pair(
                        vec![
                            token(TokenKind::LBrace),
                            token(TokenKind::Ident(SmolStr::new("key"))),
                            token(TokenKind::Colon),
                            token(TokenKind::StringLiteral("value".to_owned())),
                            token(TokenKind::RBrace),
                            token(TokenKind::Eof)
                        ],
                        Ok(vec![
                            Shared::new(Node {
                                token_id: 0.into(),
                                expr: Expr::Dict(
                                    smallvec![
                                        Shared::new(Node {
                                            token_id: 0.into(),
                                            expr: Expr::Array(
                                                smallvec![
                                                    Shared::new(Node {
                                                        token_id: 1.into(),
                                                        expr: Expr::Literal(Literal::Symbol(Ident::new("key"))),
                                                    }),
                                                    Shared::new(Node {
                                                        token_id: 2.into(),
                                                        expr: Expr::Literal(Literal::String("value".to_owned())),
                                                    }),
                                                ],
                                            ),
                                        }),
                                    ],
                                ),
                            })
                        ]))]
#[case::dict_multiple_pairs(
                        vec![
                            token(TokenKind::LBrace),
                            token(TokenKind::Ident(SmolStr::new("a"))),
                            token(TokenKind::Colon),
                            token(TokenKind::NumberLiteral(1.into())),
                            token(TokenKind::Comma),
                            token(TokenKind::StringLiteral("b".to_owned())),
                            token(TokenKind::Colon),
                            token(TokenKind::BoolLiteral(true)),
                            token(TokenKind::RBrace),
                            token(TokenKind::Eof)
                        ],
                        Ok(vec![
                            Shared::new(Node {
                                token_id: 0.into(),
                                expr: Expr::Dict(
                                    smallvec![
                                        Shared::new(Node {
                                            token_id: 0.into(),
                                            expr: Expr::Array(
                                                smallvec![
                                                    Shared::new(Node {
                                                        token_id: 1.into(),
                                                        expr: Expr::Literal(Literal::Symbol(Ident::new("a"))),
                                                    }),
                                                    Shared::new(Node {
                                                        token_id: 2.into(),
                                                        expr: Expr::Literal(Literal::Number(1.into())),
                                                    }),
                                                ],
                                            ),
                                        }),
                                        Shared::new(Node {
                                            token_id: 0.into(),
                                            expr: Expr::Array(
                                                smallvec![
                                                    Shared::new(Node {
                                                        token_id: 3.into(),
                                                        expr: Expr::Literal(Literal::String("b".to_owned())),
                                                    }),
                                                    Shared::new(Node {
                                                        token_id: 4.into(),
                                                        expr: Expr::Literal(Literal::Bool(true)),
                                                    }),
                                                ],
                                            ),
                                        }),
                                    ],
                                ),
                            })
                        ]))]
#[case::dict_trailing_comma(
                        vec![
                            token(TokenKind::LBrace),
                            token(TokenKind::Ident(SmolStr::new("x"))),
                            token(TokenKind::Colon),
                            token(TokenKind::NumberLiteral(10.into())),
                            token(TokenKind::Comma),
                            token(TokenKind::RBrace),
                            token(TokenKind::Eof)
                        ],
                        Ok(vec![
                            Shared::new(Node {
                                token_id: 0.into(),
                                expr: Expr::Dict(
                                    smallvec![
                                        Shared::new(Node {
                                            token_id: 0.into(),
                                            expr: Expr::Array(
                                                smallvec![
                                                    Shared::new(Node {
                                                        token_id: 1.into(),
                                                        expr: Expr::Literal(Literal::Symbol(Ident::new("x"))),
                                                    }),
                                                    Shared::new(Node {
                                                        token_id: 2.into(),
                                                        expr: Expr::Literal(Literal::Number(10.into())),
                                                    }),
                                                ],
                                            ),
                                        }),
                                    ],
                                ),
                            })
                        ]))]
#[case::dict_unclosed(
                        vec![
                            token(TokenKind::LBrace),
                            token(TokenKind::Ident(SmolStr::new("k"))),
                            token(TokenKind::Colon),
                            token(TokenKind::NumberLiteral(1.into())),
                            token(TokenKind::Eof)
                        ],
                        Err(SyntaxError::ExpectedClosingBrace(token(TokenKind::Eof), Some(Box::new(token(TokenKind::LBrace))))))]
#[case::dict_missing_colon(
                        vec![
                            token(TokenKind::LBrace),
                            token(TokenKind::Ident(SmolStr::new("k"))),
                            token(TokenKind::NumberLiteral(1.into())),
                            token(TokenKind::RBrace),
                            token(TokenKind::Eof)
                        ],
                        Err(SyntaxError::UnexpectedToken(token(TokenKind::NumberLiteral(1.into())))))]
#[case::dict_invalid_key(
                        vec![
                            token(TokenKind::LBrace),
                            token(TokenKind::NumberLiteral(1.into())),
                            token(TokenKind::Colon),
                            token(TokenKind::StringLiteral("v".to_owned())),
                            token(TokenKind::RBrace),
                            token(TokenKind::Eof)
                        ],
                        Err(SyntaxError::UnexpectedToken(token(TokenKind::NumberLiteral(1.into())))))]
#[case::attr_h_value(
        vec![
            token(TokenKind::Selector(".h".into())),
            token(TokenKind::Selector(".value".into())),
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 2.into(),
                expr: Expr::Call(IdentWithToken::new_with_token(constants::builtins::ATTR, Some(0.into())),
                    smallvec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Expr::Selector(Selector::Heading(None)),
                        }),
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Expr::Literal(Literal::String("value".to_owned())),
                        }),

                    ],
                )})]))]
#[case::attr(
        vec![
            token(TokenKind::Selector(".list".into())),
            token(TokenKind::Selector(".checked".into())),
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 2.into(),
                expr: Expr::Call(IdentWithToken::new_with_token(constants::builtins::ATTR, Some(0.into())),
                    smallvec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Expr::Selector(Selector::List(None, None)),
                        }),
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Expr::Literal(Literal::String("checked".to_owned())),
                        }),

                    ],
                )})]))]
#[case::paren(
        vec![
            token(TokenKind::LParen),
            token(TokenKind::NumberLiteral(1.into())),
            token(TokenKind::Plus),
            token(TokenKind::NumberLiteral(2.into())),
            token(TokenKind::RParen),
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Expr::Paren(
                    Shared::new(Node {
                        token_id: 2.into(),
                        expr: Expr::BinaryOp(
    BinaryOp::Add,
                                Shared::new(Node {
                                    token_id: 1.into(),
                                    expr: Expr::Literal(Literal::Number(1.into())),
                                }),
                                Shared::new(Node {
                                    token_id: 3.into(),
                                    expr: Expr::Literal(Literal::Number(2.into())),
                                }),
                            ),
                    })
                ),
            })
        ]))]
#[case::minus_simple(
        vec![
            token(TokenKind::NumberLiteral(5.into())),
            token(TokenKind::Minus),
            token(TokenKind::NumberLiteral(3.into())),
            token(TokenKind::Eof)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 1.into(),
                expr: Expr::BinaryOp(
    BinaryOp::Sub,
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Expr::Literal(Literal::Number(5.into())),
                        }),
                        Shared::new(Node {
                            token_id: 2.into(),
                            expr: Expr::Literal(Literal::Number(3.into())),
                        }),
                    ),
            })
        ]))]
#[case::minus_with_identifiers(
        vec![
            token(TokenKind::Ident(SmolStr::new("a"))),
            token(TokenKind::Minus),
            token(TokenKind::Ident(SmolStr::new("b"))),
            token(TokenKind::Eof)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 1.into(),
                expr: Expr::BinaryOp(
    BinaryOp::Sub,
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Expr::Ident(IdentWithToken::new_with_token("a", Some(0.into()))),
                        }),
                        Shared::new(Node {
                            token_id: 2.into(),
                            expr: Expr::Ident(IdentWithToken::new_with_token("b", Some(2.into()))),
                        }),
                    ),
            })
        ]))]
#[case::slash_simple(
        vec![
            token(TokenKind::NumberLiteral(6.into())),
            token(TokenKind::Slash),
            token(TokenKind::NumberLiteral(2.into())),
            token(TokenKind::Eof)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 1.into(),
                expr: Expr::BinaryOp(
    BinaryOp::Div,
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Expr::Literal(Literal::Number(6.into())),
                        }),
                        Shared::new(Node {
                            token_id: 2.into(),
                            expr: Expr::Literal(Literal::Number(2.into())),
                        }),
                    ),
            })
        ]))]
#[case::percent_simple(
            vec![
                token(TokenKind::NumberLiteral(10.into())),
                token(TokenKind::Percent),
                token(TokenKind::NumberLiteral(3.into())),
                token(TokenKind::Eof)
            ],
            Ok(vec![
                Shared::new(Node {
                    token_id: 1.into(),
                    expr: Expr::BinaryOp(
    BinaryOp::Mod,
                            Shared::new(Node {
                                token_id: 0.into(),
                                expr: Expr::Literal(Literal::Number(10.into())),
                            }),
                            Shared::new(Node {
                                token_id: 2.into(),
                                expr: Expr::Literal(Literal::Number(3.into())),
                            }),
                        ),
                })
            ]))]
#[case::percent_with_identifiers(
            vec![
                token(TokenKind::Ident(SmolStr::new("a"))),
                token(TokenKind::Percent),
                token(TokenKind::Ident(SmolStr::new("b"))),
                token(TokenKind::Eof)
            ],
            Ok(vec![
                Shared::new(Node {
                    token_id: 1.into(),
                    expr: Expr::BinaryOp(
    BinaryOp::Mod,
                            Shared::new(Node {
                                token_id: 0.into(),
                                expr: Expr::Ident(IdentWithToken::new_with_token("a", Some(0.into()))),
                            }),
                            Shared::new(Node {
                                token_id: 2.into(),
                                expr: Expr::Ident(IdentWithToken::new_with_token("b", Some(2.into()))),
                            }),
                        ),
                })
            ]))]
#[case::percent_error_missing_rhs(
            vec![
                token(TokenKind::NumberLiteral(10.into())),
                token(TokenKind::Percent),
                token(TokenKind::Eof)
            ],
            Err(SyntaxError::UnexpectedEOFAfterToken(token(TokenKind::Percent))))]
#[case::mul_simple(
            vec![
                token(TokenKind::NumberLiteral(3.into())),
                token(TokenKind::Asterisk),
                token(TokenKind::NumberLiteral(4.into())),
                token(TokenKind::Eof)
            ],
            Ok(vec![
                Shared::new(Node {
                    token_id: 1.into(),
                    expr: Expr::BinaryOp(
    BinaryOp::Mul,
                            Shared::new(Node {
                                token_id: 0.into(),
                                expr: Expr::Literal(Literal::Number(3.into())),
                            }),
                            Shared::new(Node {
                                token_id: 2.into(),
                                expr: Expr::Literal(Literal::Number(4.into())),
                            }),
                        ),
                })
            ]))]
#[case::mul_with_identifiers(
            vec![
                token(TokenKind::Ident(SmolStr::new("a"))),
                token(TokenKind::Asterisk),
                token(TokenKind::Ident(SmolStr::new("b"))),
                token(TokenKind::Eof)
            ],
            Ok(vec![
                Shared::new(Node {
                    token_id: 1.into(),
                    expr: Expr::BinaryOp(
    BinaryOp::Mul,
                            Shared::new(Node {
                                token_id: 0.into(),
                                expr: Expr::Ident(IdentWithToken::new_with_token("a", Some(0.into()))),
                            }),
                            Shared::new(Node {
                                token_id: 2.into(),
                                expr: Expr::Ident(IdentWithToken::new_with_token("b", Some(2.into()))),
                            }),
                        ),
                })
            ]))]
#[case::mul_error_missing_rhs(
            vec![
                token(TokenKind::NumberLiteral(5.into())),
                token(TokenKind::Asterisk),
                token(TokenKind::Eof)
            ],
            Err(SyntaxError::UnexpectedEOFAfterToken(token(TokenKind::Asterisk))))]
#[case::convert_simple(
            vec![
                token(TokenKind::Ident(SmolStr::new("a"))),
                token(TokenKind::Convert),
                token(TokenKind::Ident(SmolStr::new("b"))),
                token(TokenKind::Eof)
            ],
            Ok(vec![
                Shared::new(Node {
                    token_id: 1.into(),
                    expr: Expr::Call(
                        IdentWithToken::new_with_token(constants::builtins::CONVERT, Some(1.into())),
                        smallvec![
                            Shared::new(Node {
                                token_id: 0.into(),
                                expr: Expr::Ident(IdentWithToken::new_with_token("a", Some(0.into()))),
                            }),
                            Shared::new(Node {
                                token_id: 2.into(),
                                expr: Expr::Ident(IdentWithToken::new_with_token("b", Some(2.into()))),
                            }),
                        ],
                    ),
                })
            ]))]
#[case::convert_error_missing_rhs(
            vec![
                token(TokenKind::Ident(SmolStr::new("a"))),
                token(TokenKind::Convert),
                token(TokenKind::Eof)
            ],
            Err(SyntaxError::UnexpectedEOFAfterToken(token(TokenKind::Convert))))]
#[case::multiple_binary_operators(
            vec![
                token(TokenKind::NumberLiteral(1.into())),
                token(TokenKind::Asterisk),
                token(TokenKind::NumberLiteral(2.into())),
                token(TokenKind::Asterisk),
                token(TokenKind::NumberLiteral(3.into())),
                token(TokenKind::Eof)
            ],
            Ok(vec![
                Shared::new(Node {
                    token_id: 3.into(),
                    expr: Expr::BinaryOp(
    BinaryOp::Mul,
                            Shared::new(Node {
                                token_id: 1.into(),
                                expr: Expr::BinaryOp(
    BinaryOp::Mul,
                                        Shared::new(Node {
                                            token_id: 0.into(),
                                            expr: Expr::Literal(Literal::Number(1.into())),
                                        }),
                                        Shared::new(Node {
                                            token_id: 2.into(),
                                            expr: Expr::Literal(Literal::Number(2.into())),
                                        }),
                                    ),
                            }),
                            Shared::new(Node {
                                token_id: 4.into(),
                                expr: Expr::Literal(Literal::Number(3.into())),
                            }),
                        ),
                })
            ]))]
#[case::multiple_binary_operators_eq(
            vec![
                token(TokenKind::NumberLiteral(1.into())),
                token(TokenKind::Plus),
                token(TokenKind::NumberLiteral(2.into())),
                token(TokenKind::EqEq),
                token(TokenKind::NumberLiteral(3.into())),
                token(TokenKind::Eof)
            ],
            Ok(vec![
                Shared::new(Node {
                    token_id: 3.into(),
                    expr: Expr::BinaryOp(
    BinaryOp::Eq,
                            Shared::new(Node {
                                token_id: 1.into(),
                                expr: Expr::BinaryOp(
    BinaryOp::Add,
                                        Shared::new(Node {
                                            token_id: 0.into(),
                                            expr: Expr::Literal(Literal::Number(1.into())),
                                        }),
                                        Shared::new(Node {
                                            token_id: 2.into(),
                                            expr: Expr::Literal(Literal::Number(2.into())),
                                        }),
                                    ),
                            }),
                            Shared::new(Node {
                                token_id: 4.into(),
                                expr: Expr::Literal(Literal::Number(3.into())),
                            }),
                        ),
                })
            ]))]
#[case::multiple_and_operators(
            vec![
                token(TokenKind::Ident(SmolStr::new("a"))),
                token(TokenKind::And),
                token(TokenKind::Ident(SmolStr::new("b"))),
                token(TokenKind::And),
                token(TokenKind::Ident(SmolStr::new("c"))),
                token(TokenKind::Eof)
            ],
            Ok(vec![
                Shared::new(Node {
                    token_id: 3.into(),
                    expr: Expr::And(vec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Expr::Ident(IdentWithToken::new_with_token("a", Some(0.into()))),
                        }),
                        Shared::new(Node {
                            token_id: 2.into(),
                            expr: Expr::Ident(IdentWithToken::new_with_token("b", Some(2.into()))),
                        }),
                        Shared::new(Node {
                            token_id: 4.into(),
                            expr: Expr::Ident(IdentWithToken::new_with_token("c", Some(4.into()))),
                        }),
                    ]),
                })
            ]))]
#[case::multiple_or_operators(
            vec![
                token(TokenKind::Ident(SmolStr::new("x"))),
                token(TokenKind::Or),
                token(TokenKind::Ident(SmolStr::new("y"))),
                token(TokenKind::Or),
                token(TokenKind::Ident(SmolStr::new("z"))),
                token(TokenKind::Eof)
            ],
            Ok(vec![
                Shared::new(Node {
                    token_id: 3.into(),
                    expr: Expr::Or(vec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Expr::Ident(IdentWithToken::new_with_token("x", Some(0.into()))),
                        }),
                        Shared::new(Node {
                            token_id: 2.into(),
                            expr: Expr::Ident(IdentWithToken::new_with_token("y", Some(2.into()))),
                        }),
                        Shared::new(Node {
                            token_id: 4.into(),
                            expr: Expr::Ident(IdentWithToken::new_with_token("z", Some(4.into()))),
                        }),
                    ]),
                })
            ]))]
#[case::and_or_mixed(
            vec![
                token(TokenKind::Ident(SmolStr::new("a"))),
                token(TokenKind::And),
                token(TokenKind::Ident(SmolStr::new("b"))),
                token(TokenKind::Or),
                token(TokenKind::Ident(SmolStr::new("c"))),
                token(TokenKind::Eof)
            ],
            Ok(vec![
                Shared::new(Node {
                    token_id: 3.into(),
                    expr: Expr::Or(vec![
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Expr::And(vec![
                                Shared::new(Node {
                                    token_id: 0.into(),
                                    expr: Expr::Ident(IdentWithToken::new_with_token("a", Some(0.into()))),
                                }),
                                Shared::new(Node {
                                    token_id: 2.into(),
                                    expr: Expr::Ident(IdentWithToken::new_with_token("b", Some(2.into()))),
                                }),
                            ]),
                        }),
                        Shared::new(Node {
                            token_id: 4.into(),
                            expr: Expr::Ident(IdentWithToken::new_with_token("c", Some(4.into()))),
                        }),
                    ]),
                })
            ]))]
#[case::four_and_operators(
            vec![
                token(TokenKind::Ident(SmolStr::new("a"))),
                token(TokenKind::And),
                token(TokenKind::Ident(SmolStr::new("b"))),
                token(TokenKind::And),
                token(TokenKind::Ident(SmolStr::new("c"))),
                token(TokenKind::And),
                token(TokenKind::Ident(SmolStr::new("d"))),
                token(TokenKind::Eof)
            ],
            Ok(vec![
                Shared::new(Node {
                    token_id: 5.into(),
                    expr: Expr::And(vec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Expr::Ident(IdentWithToken::new_with_token("a", Some(0.into()))),
                        }),
                        Shared::new(Node {
                            token_id: 2.into(),
                            expr: Expr::Ident(IdentWithToken::new_with_token("b", Some(2.into()))),
                        }),
                        Shared::new(Node {
                            token_id: 4.into(),
                            expr: Expr::Ident(IdentWithToken::new_with_token("c", Some(4.into()))),
                        }),
                        Shared::new(Node {
                            token_id: 6.into(),
                            expr: Expr::Ident(IdentWithToken::new_with_token("d", Some(6.into()))),
                        }),
                    ]),
                })
            ]))]
#[case::four_or_operators(
            vec![
                token(TokenKind::Ident(SmolStr::new("a"))),
                token(TokenKind::Or),
                token(TokenKind::Ident(SmolStr::new("b"))),
                token(TokenKind::Or),
                token(TokenKind::Ident(SmolStr::new("c"))),
                token(TokenKind::Or),
                token(TokenKind::Ident(SmolStr::new("d"))),
                token(TokenKind::Eof)
            ],
            Ok(vec![
                Shared::new(Node {
                    token_id: 5.into(),
                    expr: Expr::Or(vec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Expr::Ident(IdentWithToken::new_with_token("a", Some(0.into()))),
                        }),
                        Shared::new(Node {
                            token_id: 2.into(),
                            expr: Expr::Ident(IdentWithToken::new_with_token("b", Some(2.into()))),
                        }),
                        Shared::new(Node {
                            token_id: 4.into(),
                            expr: Expr::Ident(IdentWithToken::new_with_token("c", Some(4.into()))),
                        }),
                        Shared::new(Node {
                            token_id: 6.into(),
                            expr: Expr::Ident(IdentWithToken::new_with_token("d", Some(6.into()))),
                        }),
                    ]),
                })
            ]))]
#[case::or_with_and_in_middle(
            vec![
                token(TokenKind::Ident(SmolStr::new("a"))),
                token(TokenKind::Or),
                token(TokenKind::Ident(SmolStr::new("b"))),
                token(TokenKind::And),
                token(TokenKind::Ident(SmolStr::new("c"))),
                token(TokenKind::Or),
                token(TokenKind::Ident(SmolStr::new("d"))),
                token(TokenKind::Eof)
            ],
            Ok(vec![
                Shared::new(Node {
                    token_id: 5.into(),
                    expr: Expr::Or(vec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Expr::Ident(IdentWithToken::new_with_token("a", Some(0.into()))),
                        }),
                        Shared::new(Node {
                            token_id: 3.into(),
                            expr: Expr::And(vec![
                                Shared::new(Node {
                                    token_id: 2.into(),
                                    expr: Expr::Ident(IdentWithToken::new_with_token("b", Some(2.into()))),
                                }),
                                Shared::new(Node {
                                    token_id: 4.into(),
                                    expr: Expr::Ident(IdentWithToken::new_with_token("c", Some(4.into()))),
                                }),
                            ]),
                        }),
                        Shared::new(Node {
                            token_id: 6.into(),
                            expr: Expr::Ident(IdentWithToken::new_with_token("d", Some(6.into()))),
                        }),
                    ]),
                })
            ]))]
#[case::and_or_and_mixed(
            // a && b || c && d  =>  Or([And([a, b]), And([c, d])])
            vec![
                token(TokenKind::Ident(SmolStr::new("a"))),
                token(TokenKind::And),
                token(TokenKind::Ident(SmolStr::new("b"))),
                token(TokenKind::Or),
                token(TokenKind::Ident(SmolStr::new("c"))),
                token(TokenKind::And),
                token(TokenKind::Ident(SmolStr::new("d"))),
                token(TokenKind::Eof)
            ],
            Ok(vec![
                Shared::new(Node {
                    token_id: 3.into(),
                    expr: Expr::Or(vec![
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Expr::And(vec![
                                Shared::new(Node {
                                    token_id: 0.into(),
                                    expr: Expr::Ident(IdentWithToken::new_with_token("a", Some(0.into()))),
                                }),
                                Shared::new(Node {
                                    token_id: 2.into(),
                                    expr: Expr::Ident(IdentWithToken::new_with_token("b", Some(2.into()))),
                                }),
                            ]),
                        }),
                        Shared::new(Node {
                            token_id: 5.into(),
                            expr: Expr::And(vec![
                                Shared::new(Node {
                                    token_id: 4.into(),
                                    expr: Expr::Ident(IdentWithToken::new_with_token("c", Some(4.into()))),
                                }),
                                Shared::new(Node {
                                    token_id: 6.into(),
                                    expr: Expr::Ident(IdentWithToken::new_with_token("d", Some(6.into()))),
                                }),
                            ]),
                        }),
                    ]),
                })
            ]))]
#[case::or_and_or_mixed(
            // a || b && c || d  =>  Or([a, And([b, c]), d])
            vec![
                token(TokenKind::Ident(SmolStr::new("a"))),
                token(TokenKind::Or),
                token(TokenKind::Ident(SmolStr::new("b"))),
                token(TokenKind::And),
                token(TokenKind::Ident(SmolStr::new("c"))),
                token(TokenKind::Or),
                token(TokenKind::Ident(SmolStr::new("d"))),
                token(TokenKind::Eof)
            ],
            Ok(vec![
                Shared::new(Node {
                    token_id: 5.into(),
                    expr: Expr::Or(vec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Expr::Ident(IdentWithToken::new_with_token("a", Some(0.into()))),
                        }),
                        Shared::new(Node {
                            token_id: 3.into(),
                            expr: Expr::And(vec![
                                Shared::new(Node {
                                    token_id: 2.into(),
                                    expr: Expr::Ident(IdentWithToken::new_with_token("b", Some(2.into()))),
                                }),
                                Shared::new(Node {
                                    token_id: 4.into(),
                                    expr: Expr::Ident(IdentWithToken::new_with_token("c", Some(4.into()))),
                                }),
                            ]),
                        }),
                        Shared::new(Node {
                            token_id: 6.into(),
                            expr: Expr::Ident(IdentWithToken::new_with_token("d", Some(6.into()))),
                        }),
                    ]),
                })
            ]))]
#[case::and_and_or_and_and_mixed(
            // a && b && c || d && e && f  =>  Or([And([a,b,c]), And([d,e,f])])
            vec![
                token(TokenKind::Ident(SmolStr::new("a"))),
                token(TokenKind::And),
                token(TokenKind::Ident(SmolStr::new("b"))),
                token(TokenKind::And),
                token(TokenKind::Ident(SmolStr::new("c"))),
                token(TokenKind::Or),
                token(TokenKind::Ident(SmolStr::new("d"))),
                token(TokenKind::And),
                token(TokenKind::Ident(SmolStr::new("e"))),
                token(TokenKind::And),
                token(TokenKind::Ident(SmolStr::new("f"))),
                token(TokenKind::Eof)
            ],
            Ok(vec![
                Shared::new(Node {
                    token_id: 5.into(),
                    expr: Expr::Or(vec![
                        Shared::new(Node {
                            token_id: 3.into(),
                            expr: Expr::And(vec![
                                Shared::new(Node {
                                    token_id: 0.into(),
                                    expr: Expr::Ident(IdentWithToken::new_with_token("a", Some(0.into()))),
                                }),
                                Shared::new(Node {
                                    token_id: 2.into(),
                                    expr: Expr::Ident(IdentWithToken::new_with_token("b", Some(2.into()))),
                                }),
                                Shared::new(Node {
                                    token_id: 4.into(),
                                    expr: Expr::Ident(IdentWithToken::new_with_token("c", Some(4.into()))),
                                }),
                            ]),
                        }),
                        Shared::new(Node {
                            token_id: 9.into(),
                            expr: Expr::And(vec![
                                Shared::new(Node {
                                    token_id: 6.into(),
                                    expr: Expr::Ident(IdentWithToken::new_with_token("d", Some(6.into()))),
                                }),
                                Shared::new(Node {
                                    token_id: 8.into(),
                                    expr: Expr::Ident(IdentWithToken::new_with_token("e", Some(8.into()))),
                                }),
                                Shared::new(Node {
                                    token_id: 10.into(),
                                    expr: Expr::Ident(IdentWithToken::new_with_token("f", Some(10.into()))),
                                }),
                            ]),
                        }),
                    ]),
                })
            ]))]
#[case::range_simple(
                vec![
                    token(TokenKind::NumberLiteral(1.into())),
                    token(TokenKind::DoubleDot),
                    token(TokenKind::NumberLiteral(5.into())),
                    token(TokenKind::Eof)
                ],
                Ok(vec![
                    Shared::new(Node {
                        token_id: 1.into(),
                        expr: Expr::Call(
                            IdentWithToken::new_with_token(constants::builtins::RANGE, Some(1.into())),
                            smallvec![
                                Shared::new(Node {
                                    token_id: 0.into(),
                                    expr: Expr::Literal(Literal::Number(1.into())),
                                }),
                                Shared::new(Node {
                                    token_id: 2.into(),
                                    expr: Expr::Literal(Literal::Number(5.into())),
                                }),
                            ],
                        ),
                    })
                ]))]
#[case::range_with_identifiers(
                vec![
                    token(TokenKind::Ident(SmolStr::new("start"))),
                    token(TokenKind::DoubleDot),
                    token(TokenKind::Ident(SmolStr::new("end"))),
                    token(TokenKind::Eof)
                ],
                Ok(vec![
                    Shared::new(Node {
                        token_id: 1.into(),
                        expr: Expr::Call(
                            IdentWithToken::new_with_token(constants::builtins::RANGE, Some(1.into())),
                            smallvec![
                                Shared::new(Node {
                                    token_id: 0.into(),
                                    expr: Expr::Ident(IdentWithToken::new_with_token("start", Some(0.into()))),
                                }),
                                Shared::new(Node {
                                    token_id: 2.into(),
                                    expr: Expr::Ident(IdentWithToken::new_with_token("end", Some(2.into()))),
                                }),
                            ],
                        ),
                    })
                ]))]
#[case::range_error_missing_rhs(
                vec![
                    token(TokenKind::NumberLiteral(1.into())),
                    token(TokenKind::DoubleDot),
                    token(TokenKind::Eof)
                ],
                Err(SyntaxError::UnexpectedEOFAfterToken(token(TokenKind::DoubleDot))))]
#[case::args_missing_rparen(
                vec![
                    token(TokenKind::Ident(SmolStr::new("foo"))),
                    token(TokenKind::LParen),
                    token(TokenKind::StringLiteral("bar".to_owned())),
                    // Missing RParen
                    token(TokenKind::Eof)
                ],
                Err(SyntaxError::ExpectedClosingParen(
                    token(TokenKind::Eof),
                    Some(Box::new(token(TokenKind::LParen))),
                ))
            )]
#[case::args_unexpected_token(
                vec![
                    token(TokenKind::Ident(SmolStr::new("foo"))),
                    token(TokenKind::LParen),
                    token(TokenKind::NumberLiteral(1.into())),
                    token(TokenKind::Colon), // Invalid token in args
                    token(TokenKind::RParen),
                    token(TokenKind::Eof)
                ],
                Err(SyntaxError::ExpectedClosingParen(
                    token(TokenKind::Colon),
                    Some(Box::new(token(TokenKind::LParen))),
                ))
            )]
#[case::args_leading_comma(
                vec![
                    token(TokenKind::Ident(SmolStr::new("foo"))),
                    token(TokenKind::LParen),
                    token(TokenKind::Comma),
                    token(TokenKind::Ident(SmolStr::new("bar"))),
                    token(TokenKind::RParen),
                    token(TokenKind::Eof)
                ],
                Err(SyntaxError::UnexpectedToken(token(TokenKind::Comma)))
            )]
#[case::args_double_comma(
                vec![
                    token(TokenKind::Ident(SmolStr::new("foo"))),
                    token(TokenKind::LParen),
                    token(TokenKind::Ident(SmolStr::new("bar"))),
                    token(TokenKind::Comma),
                    token(TokenKind::Comma),
                    token(TokenKind::Ident(SmolStr::new("baz"))),
                    token(TokenKind::RParen),
                    token(TokenKind::Eof)
                ],
                Err(SyntaxError::UnexpectedToken(token(TokenKind::Comma)))
            )]
#[case::binary_operator_chaining(
                vec![
                    token(TokenKind::NumberLiteral(2.into())),
                    token(TokenKind::Gt),
                    token(TokenKind::NumberLiteral(1.into())),
                    token(TokenKind::Or),
                    token(TokenKind::NumberLiteral(2.into())),
                    token(TokenKind::Gt),
                    token(TokenKind::NumberLiteral(1.into())),
                    token(TokenKind::Eof)
                ],
                Ok(vec![
                    Shared::new(Node {
                        token_id: 3.into(),
                        expr: Expr::Or(vec![
                            Shared::new(Node {
                                token_id: 1.into(),
                                expr: Expr::BinaryOp(
    BinaryOp::Gt,
                                        Shared::new(Node {
                                            token_id: 0.into(),
                                            expr: Expr::Literal(Literal::Number(2.into())),
                                        }),
                                        Shared::new(Node {
                                            token_id: 2.into(),
                                            expr: Expr::Literal(Literal::Number(1.into())),
                                        }),
                                    ),
                            }),
                            Shared::new(Node {
                                token_id: 5.into(),
                                expr: Expr::BinaryOp(
    BinaryOp::Gt,
                                        Shared::new(Node {
                                            token_id: 4.into(),
                                            expr: Expr::Literal(Literal::Number(2.into())),
                                        }),
                                        Shared::new(Node {
                                            token_id: 6.into(),
                                            expr: Expr::Literal(Literal::Number(1.into())),
                                        }),
                                    ),
                            }),
                        ]),
                    })
                ]))]
#[case::not_simple(
                vec![
                    token(TokenKind::Not),
                    token(TokenKind::BoolLiteral(false)),
                    token(TokenKind::Eof)
                ],
                Ok(vec![
                    Shared::new(Node {
                        token_id: 0.into(),
                        expr: Expr::UnaryOp(UnaryOp::Not, Shared::new(Node {
                                    token_id: 1.into(),
                                    expr: Expr::Literal(Literal::Bool(false)),
                                })),
                    })
                ]))]
#[case::not_with_expr(
                vec![
                    token(TokenKind::Not),
                    token(TokenKind::Ident(SmolStr::new("x"))),
                    token(TokenKind::Eof)
                ],
                Ok(vec![
                    Shared::new(Node {
                        token_id: 0.into(),
                        expr: Expr::UnaryOp(UnaryOp::Not, Shared::new(Node {
                                    token_id: 1.into(),
                                    expr: Expr::Ident(IdentWithToken::new_with_token("x", Some(1.into()))),
                                })),
                    })
                ]))]
#[case::bracket_access_with_number(
                vec![
                    token(TokenKind::Ident(SmolStr::new("arr"))),
                    token(TokenKind::LBracket),
                    token(TokenKind::NumberLiteral(5.into())),
                    token(TokenKind::RBracket),
                    token(TokenKind::Eof)
                ],
                Ok(vec![
                    Shared::new(Node {
                        token_id: 2.into(),
                        expr: Expr::Call(
                            IdentWithToken::new_with_token(constants::builtins::GET, Some(0.into())),
                            smallvec![
                                Shared::new(Node {
                                    token_id: 0.into(),
                                    expr: Expr::Ident(IdentWithToken::new_with_token("arr", Some(0.into()))),
                                }),
                                Shared::new(Node {
                                    token_id: 1.into(),
                                    expr: Expr::Literal(Literal::Number(5.into())),
                                }),
                            ],
                        ),
                    })
                ]))]
#[case::bracket_access_with_string(
                vec![
                    token(TokenKind::Ident(SmolStr::new("dict"))),
                    token(TokenKind::LBracket),
                    token(TokenKind::StringLiteral("key".to_owned())),
                    token(TokenKind::RBracket),
                    token(TokenKind::Eof)
                ],
                Ok(vec![
                    Shared::new(Node {
                        token_id: 2.into(),
                        expr: Expr::Call(
                            IdentWithToken::new_with_token(constants::builtins::GET, Some(0.into())),
                            smallvec![
                                Shared::new(Node {
                                    token_id: 0.into(),
                                    expr: Expr::Ident(IdentWithToken::new_with_token(constants::builtins::DICT, Some(0.into()))),
                                }),
                                Shared::new(Node {
                                    token_id: 1.into(),
                                    expr: Expr::Literal(Literal::String("key".to_owned())),
                                }),
                            ],
                        ),
                    })
                ]))]
#[case::bracket_access_error_missing_rbracket(
                vec![
                    token(TokenKind::Ident(SmolStr::new("arr"))),
                    token(TokenKind::LBracket),
                    token(TokenKind::NumberLiteral(5.into())),
                    token(TokenKind::Eof)
                ],
                Err(SyntaxError::ExpectedClosingBracket(
                    token(TokenKind::Eof),
                    Some(Box::new(token(TokenKind::LBracket))),
                )))]
#[case::slice_access_with_numbers(
                vec![
                    token(TokenKind::Ident(SmolStr::new("arr"))),
                    token(TokenKind::LBracket),
                    token(TokenKind::NumberLiteral(1.into())),
                    token(TokenKind::Colon),
                    token(TokenKind::NumberLiteral(3.into())),
                    token(TokenKind::RBracket),
                    token(TokenKind::Eof)
                ],
                Ok(vec![
                    Shared::new(Node {
                        token_id: 5.into(),
                        expr: Expr::Call(
                            IdentWithToken::new_with_token(constants::builtins::SLICE, Some(0.into())),
                            smallvec![
                                Shared::new(Node {
                                    token_id: 0.into(),
                                    expr: Expr::Ident(IdentWithToken::new_with_token("arr", Some(0.into()))),
                                }),
                                Shared::new(Node {
                                    token_id: 1.into(),
                                    expr: Expr::Literal(Literal::Number(1.into())),
                                }),
                                Shared::new(Node {
                                    token_id: 2.into(),
                                    expr: Expr::Literal(Literal::Number(3.into())),
                                }),
                            ],
                        ),
                    })
                ]))]
#[case::slice_access_with_variables(
                vec![
                    token(TokenKind::Ident(SmolStr::new("items"))),
                    token(TokenKind::LBracket),
                    token(TokenKind::Ident(SmolStr::new("start"))),
                    token(TokenKind::Colon),
                    token(TokenKind::Ident(SmolStr::new("end"))),
                    token(TokenKind::RBracket),
                    token(TokenKind::Eof)
                ],
                Ok(vec![
                    Shared::new(Node {
                        token_id: 5.into(),
                        expr: Expr::Call(
                            IdentWithToken::new_with_token(constants::builtins::SLICE, Some(0.into())),
                            smallvec![
                                Shared::new(Node {
                                    token_id: 0.into(),
                                    expr: Expr::Ident(IdentWithToken::new_with_token("items", Some(0.into()))),
                                }),
                                Shared::new(Node {
                                    token_id: 1.into(),
                                    expr: Expr::Ident(IdentWithToken::new_with_token("start", Some(1.into()))),
                                }),
                                Shared::new(Node {
                                    token_id: 2.into(),
                                    expr: Expr::Ident(IdentWithToken::new_with_token("end", Some(2.into()))),
                                }),
                            ],
                        ),
                    })
                ]))]
#[case::not_with_paren_expr(
                vec![
                    token(TokenKind::Not),
                    token(TokenKind::LParen),
                    token(TokenKind::BoolLiteral(false)),
                    token(TokenKind::RParen),
                    token(TokenKind::Eof)
                ],
                Ok(vec![
                    Shared::new(Node {
                        token_id: 0.into(),
                        expr: Expr::UnaryOp(UnaryOp::Not, Shared::new(Node {
                                    token_id: 1.into(),
                                    expr: Expr::Paren(
                                        Shared::new(Node {
                                            token_id: 2.into(),
                                            expr: Expr::Literal(Literal::Bool(false)),
                                        })
                                    ),
                                })),
                    })
                ]))]
#[case::not_none(
                vec![
                    token(TokenKind::Not),
                    token(TokenKind::None),
                    token(TokenKind::Eof)
                ],
                Ok(vec![
                    Shared::new(Node {
                        token_id: 0.into(),
                        expr: Expr::UnaryOp(
                            UnaryOp::Not,
                            Shared::new(Node {
                                token_id: 1.into(),
                                expr: Expr::Literal(Literal::None),
                            })
                        ),
                    })
                ]))]
#[case::not_error_missing_rhs(
                vec![
                    token(TokenKind::Not),
                    token(TokenKind::Eof)
                ],
                Err(SyntaxError::UnexpectedEOFAfterToken(token(TokenKind::Not))))]
#[case::break_(
                    vec![
                        token(TokenKind::Break),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Expr::Break(None),
                        })
                    ]))]
#[case::continue_(
                    vec![
                        token(TokenKind::Continue),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Expr::Continue,
                        })
                    ]))]
#[case::self_bracket_access_with_number(
                vec![
                    token(TokenKind::Self_),
                    token(TokenKind::LBracket),
                    token(TokenKind::NumberLiteral(5.into())),
                    token(TokenKind::RBracket),
                    token(TokenKind::Eof)
                ],
                Ok(vec![
                    Shared::new(Node {
                        token_id: 2.into(),
                        expr: Expr::Call(
                            IdentWithToken::new_with_token(constants::builtins::GET, Some(0.into())),
                            smallvec![
                                Shared::new(Node {
                                    token_id: 0.into(),
                                    expr: Expr::Self_,
                                }),
                                Shared::new(Node {
                                    token_id: 1.into(),
                                    expr: Expr::Literal(Literal::Number(5.into())),
                                }),
                            ],
                        ),
                    })
                ]))]
#[case::self_bracket_access_with_string(
                vec![
                    token(TokenKind::Self_),
                    token(TokenKind::LBracket),
                    token(TokenKind::StringLiteral("key".to_owned())),
                    token(TokenKind::RBracket),
                    token(TokenKind::Eof)
                ],
                Ok(vec![
                    Shared::new(Node {
                        token_id: 2.into(),
                        expr: Expr::Call(
                            IdentWithToken::new_with_token(constants::builtins::GET, Some(0.into())),
                            smallvec![
                                Shared::new(Node {
                                    token_id: 0.into(),
                                    expr: Expr::Self_,
                                }),
                                Shared::new(Node {
                                    token_id: 1.into(),
                                    expr: Expr::Literal(Literal::String("key".to_owned())),
                                }),
                            ],
                        ),
                    })
                ]))]
// Test function call followed by index access (e.g., foo()[0])
#[case::function_call_with_index_access(
        vec![
            token(TokenKind::Ident(SmolStr::new("foo"))),
            token(TokenKind::LParen),
            token(TokenKind::RParen),
            token(TokenKind::LBracket),
            token(TokenKind::NumberLiteral(0.into())),
            token(TokenKind::RBracket),
            token(TokenKind::Eof)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 2.into(),
                expr: Expr::Call(
                    IdentWithToken::new_with_token(constants::builtins::GET, Some(0.into())),
                    smallvec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Expr::Call(
                                IdentWithToken::new_with_token("foo", Some(0.into())),
                                SmallVec::new(),
                            ),
                        }),
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Expr::Literal(Literal::Number(0.into())),
                        }),
                    ],
                ),
            })
        ]))]
// Test function call with arguments followed by index access
#[case::function_call_with_args_and_index_access(
        vec![
            token(TokenKind::Ident(SmolStr::new("bar"))),
            token(TokenKind::LParen),
            token(TokenKind::StringLiteral("arg".to_owned())),
            token(TokenKind::RParen),
            token(TokenKind::LBracket),
            token(TokenKind::StringLiteral("key".to_owned())),
            token(TokenKind::RBracket),
            token(TokenKind::Eof)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 1.into(),
                expr: Expr::Call(
                    IdentWithToken::new_with_token(constants::builtins::GET, Some(1.into())),
                    smallvec![
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Expr::Call(
                                IdentWithToken::new_with_token("bar", Some(1.into())),
                                smallvec![
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Expr::Literal(Literal::String("arg".to_owned())),
                                    })
                                ],
                            ),
                        }),
                        Shared::new(Node {
                            token_id: 2.into(),
                            expr: Expr::Literal(Literal::String("key".to_owned())),
                        }),
                    ],
                ),
            })
        ]))]
// Test chained index access on function call result
#[case::function_call_with_chained_index_access(
        vec![
            token(TokenKind::Ident(SmolStr::new("baz"))),
            token(TokenKind::LParen),
            token(TokenKind::RParen),
            token(TokenKind::LBracket),
            token(TokenKind::NumberLiteral(0.into())),
            token(TokenKind::RBracket),
            token(TokenKind::LBracket),
            token(TokenKind::NumberLiteral(1.into())),
            token(TokenKind::RBracket),
            token(TokenKind::Eof)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 2.into(),
                expr: Expr::Call(
                    IdentWithToken::new_with_token(constants::builtins::GET, Some(0.into())),
                    smallvec![
                        Shared::new(Node {
                            token_id: 2.into(),
                            expr: Expr::Call(
                                IdentWithToken::new_with_token(constants::builtins::GET, Some(0.into())),
                                smallvec![
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Expr::Call(
                                            IdentWithToken::new_with_token("baz", Some(0.into())),
                                            SmallVec::new(),
                                        ),
                                    }),
                                    Shared::new(Node {
                                        token_id: 1.into(),
                                        expr: Expr::Literal(Literal::Number(0.into())),
                                    }),
                                ],
                            ),
                        }),
                        Shared::new(Node {
                            token_id: 3.into(),
                            expr: Expr::Literal(Literal::Number(1.into())),
                        }),
                    ],
                ),
            })
        ]))]
#[case::try_without_catch(
            vec![
                token(TokenKind::Try),
                token(TokenKind::Colon),
                token(TokenKind::Ident(SmolStr::new("error_expr"))),
                token(TokenKind::Eof),
            ],
            Ok(vec![Shared::new(Node {
                token_id: 2.into(),
                expr: Expr::Try(
                    Shared::new(Node {
                        token_id: 2.into(),
                        expr: Expr::Ident(IdentWithToken::new_with_token("error_expr", Some(2.into()))),
                    }),
                    None,
                    Shared::new(Node {
                        token_id: 0.into(),
                        expr: Expr::Literal(Literal::None),
                    }),
                ),
            })])
        )]
// Test index access followed by function call (e.g., arr[0]())
#[case::index_access_with_function_call(
        vec![
            token(TokenKind::Ident(SmolStr::new("arr"))),
            token(TokenKind::LBracket),
            token(TokenKind::NumberLiteral(0.into())),
            token(TokenKind::RBracket),
            token(TokenKind::LParen),
            token(TokenKind::RParen),
            token(TokenKind::Eof)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 2.into(),
                expr: Expr::CallDynamic(
                    Shared::new(Node {
                        token_id: 2.into(),
                        expr: Expr::Call(
                            IdentWithToken::new_with_token(constants::builtins::GET, Some(0.into())),
                            smallvec![
                                Shared::new(Node {
                                    token_id: 0.into(),
                                    expr: Expr::Ident(IdentWithToken::new_with_token("arr", Some(0.into()))),
                                }),
                                Shared::new(Node {
                                    token_id: 1.into(),
                                    expr: Expr::Literal(Literal::Number(0.into())),
                                }),
                            ],
                        ),
                    }),
                    SmallVec::new(),
                ),
            })
        ]))]
// Test index access with args followed by function call (e.g., arr[0](arg))
#[case::index_access_with_function_call_and_args(
        vec![
            token(TokenKind::Ident(SmolStr::new("arr"))),
            token(TokenKind::LBracket),
            token(TokenKind::NumberLiteral(0.into())),
            token(TokenKind::RBracket),
            token(TokenKind::LParen),
            token(TokenKind::StringLiteral("test".to_owned())),
            token(TokenKind::RParen),
            token(TokenKind::Eof)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 2.into(),
                expr: Expr::CallDynamic(
                    Shared::new(Node {
                        token_id: 2.into(),
                        expr: Expr::Call(
                            IdentWithToken::new_with_token(constants::builtins::GET, Some(0.into())),
                            smallvec![
                                Shared::new(Node {
                                    token_id: 0.into(),
                                    expr: Expr::Ident(IdentWithToken::new_with_token("arr", Some(0.into()))),
                                }),
                                Shared::new(Node {
                                    token_id: 1.into(),
                                    expr: Expr::Literal(Literal::Number(0.into())),
                                }),
                            ],
                        ),
                    }),
                    smallvec![
                        Shared::new(Node {
                            token_id: 3.into(),
                            expr: Expr::Literal(Literal::String("test".to_owned())),
                        })
                    ],
                ),
            })
        ]))]
// Test group expr with index access: (x)[0] → get(Paren(x), 0)
#[case::group_expr_with_index_access(
        vec![
            token(TokenKind::LParen),
            token(TokenKind::Ident(SmolStr::new("x"))),
            token(TokenKind::RParen),
            token(TokenKind::LBracket),
            token(TokenKind::NumberLiteral(0.into())),
            token(TokenKind::RBracket),
            token(TokenKind::Eof),
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Expr::Call(
                    IdentWithToken::new_with_token(constants::builtins::GET, Some(0.into())),
                    smallvec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Expr::Paren(
                                Shared::new(Node {
                                    token_id: 1.into(),
                                    expr: Expr::Ident(IdentWithToken::new_with_token("x", Some(1.into()))),
                                }),
                            ),
                        }),
                        Shared::new(Node {
                            token_id: 2.into(),
                            expr: Expr::Literal(Literal::Number(0.into())),
                        }),
                    ],
                ),
            })
        ]))]
// Test group expr with dynamic call: (x)("test") → CallDynamic(Paren(x), ["test"])
#[case::group_expr_with_dynamic_call(
        vec![
            token(TokenKind::LParen),
            token(TokenKind::Ident(SmolStr::new("x"))),
            token(TokenKind::RParen),
            token(TokenKind::LParen),
            token(TokenKind::StringLiteral("test".to_owned())),
            token(TokenKind::RParen),
            token(TokenKind::Eof),
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Expr::CallDynamic(
                    Shared::new(Node {
                        token_id: 0.into(),
                        expr: Expr::Paren(
                            Shared::new(Node {
                                token_id: 1.into(),
                                expr: Expr::Ident(IdentWithToken::new_with_token("x", Some(1.into()))),
                            }),
                        ),
                    }),
                    smallvec![
                        Shared::new(Node {
                            token_id: 2.into(),
                            expr: Expr::Literal(Literal::String("test".to_owned())),
                        }),
                    ],
                ),
            })
        ]))]
// Test group expr with chained call then index: (f)("a")[0] → get(CallDynamic(Paren(f), ["a"]), 0)
#[case::group_expr_with_chained_call_and_index(
        vec![
            token(TokenKind::LParen),
            token(TokenKind::Ident(SmolStr::new("f"))),
            token(TokenKind::RParen),
            token(TokenKind::LParen),
            token(TokenKind::StringLiteral("a".to_owned())),
            token(TokenKind::RParen),
            token(TokenKind::LBracket),
            token(TokenKind::NumberLiteral(0.into())),
            token(TokenKind::RBracket),
            token(TokenKind::Eof),
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Expr::Call(
                    IdentWithToken::new_with_token(constants::builtins::GET, Some(0.into())),
                    smallvec![
                        Shared::new(Node {
                            token_id: 3.into(),
                            expr: Expr::CallDynamic(
                                Shared::new(Node {
                                    token_id: 0.into(),
                                    expr: Expr::Paren(
                                        Shared::new(Node {
                                            token_id: 1.into(),
                                            expr: Expr::Ident(IdentWithToken::new_with_token("f", Some(1.into()))),
                                        }),
                                    ),
                                }),
                                smallvec![
                                    Shared::new(Node {
                                        token_id: 2.into(),
                                        expr: Expr::Literal(Literal::String("a".to_owned())),
                                    }),
                                ],
                            ),
                        }),
                        Shared::new(Node {
                            token_id: 4.into(),
                            expr: Expr::Literal(Literal::Number(0.into())),
                        }),
                    ],
                ),
            })
        ]))]
// Test array literal with index access: [1,2][0] → get(array([1,2]), 0)
#[case::array_literal_with_index_access(
        vec![
            token(TokenKind::LBracket),
            token(TokenKind::NumberLiteral(1.into())),
            token(TokenKind::Comma),
            token(TokenKind::NumberLiteral(2.into())),
            token(TokenKind::RBracket),
            token(TokenKind::LBracket),
            token(TokenKind::NumberLiteral(0.into())),
            token(TokenKind::RBracket),
            token(TokenKind::Eof),
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Expr::Call(
                    IdentWithToken::new_with_token(constants::builtins::GET, Some(0.into())),
                    smallvec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Expr::Array(
                                smallvec![
                                    Shared::new(Node {
                                        token_id: 1.into(),
                                        expr: Expr::Literal(Literal::Number(1.into())),
                                    }),
                                    Shared::new(Node {
                                        token_id: 2.into(),
                                        expr: Expr::Literal(Literal::Number(2.into())),
                                    }),
                                ],
                            ),
                        }),
                        Shared::new(Node {
                            token_id: 3.into(),
                            expr: Expr::Literal(Literal::Number(0.into())),
                        }),
                    ],
                ),
            })
        ]))]
// Test chained index access followed by function call (e.g., arr[0][1]())
#[case::chained_index_access_with_function_call(
        vec![
            token(TokenKind::Ident(SmolStr::new("arr"))),
            token(TokenKind::LBracket),
            token(TokenKind::NumberLiteral(0.into())),
            token(TokenKind::RBracket),
            token(TokenKind::LBracket),
            token(TokenKind::NumberLiteral(1.into())),
            token(TokenKind::RBracket),
            token(TokenKind::LParen),
            token(TokenKind::RParen),
            token(TokenKind::Eof)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 4.into(),
                expr: Expr::CallDynamic(
                    Shared::new(Node {
                        token_id: 4.into(),
                        expr: Expr::Call(
                            IdentWithToken::new_with_token(constants::builtins::GET, Some(0.into())),
                            smallvec![
                                Shared::new(Node {
                                    token_id: 2.into(),
                                    expr: Expr::Call(
                                        IdentWithToken::new_with_token(constants::builtins::GET, Some(0.into())),
                                        smallvec![
                                            Shared::new(Node {
                                                token_id: 0.into(),
                                                expr: Expr::Ident(IdentWithToken::new_with_token("arr", Some(0.into()))),
                                            }),
                                            Shared::new(Node {
                                                token_id: 1.into(),
                                                expr: Expr::Literal(Literal::Number(0.into())),
                                            }),
                                        ],
                                    ),
                                }),
                                Shared::new(Node {
                                    token_id: 3.into(),
                                    expr: Expr::Literal(Literal::Number(1.into())),
                                }),
                            ],
                        ),
                    }),
                    SmallVec::new(),
                ),
            })
        ]))]
#[case::function_call_with_question_mark(
            vec![
                token(TokenKind::Ident(SmolStr::new("foo"))),
                token(TokenKind::LParen),
                token(TokenKind::StringLiteral("arg".to_owned())),
                token(TokenKind::RParen),
                token(TokenKind::Question),
                token(TokenKind::Eof),
            ],
            Ok(vec![Shared::new(Node {
                token_id: 1.into(),
                expr: Expr::Try(
                    Shared::new(Node {
                        token_id: 1.into(),
                        expr: Expr::Call(
                            IdentWithToken::new_with_token("foo", Some(1.into())),
                            smallvec![
                                Shared::new(Node {
                                    token_id: 0.into(),
                                    expr: Expr::Literal(Literal::String("arg".to_owned())),
                                }),
                            ],
                        ),
                    }),
                    None,
                    Shared::new(Node {
                        token_id: 1.into(),
                        expr: Expr::Literal(Literal::None),
                    }),
                ),
            })])
        )]
#[case::question_mark_after_call(
                vec![
                    token(TokenKind::Ident(SmolStr::new("foo"))),
                    token(TokenKind::LParen),
                    token(TokenKind::RParen),
                    token(TokenKind::Question),
                    token(TokenKind::Eof),
                ],
                Ok(vec![
                    Shared::new(Node {
                        token_id: 1.into(),
                        expr: Expr::Try(
                            Shared::new(Node {
                                token_id: 0.into(),
                                expr: Expr::Call(
                                    IdentWithToken::new_with_token("foo", Some(0.into())),
                                    SmallVec::new(),
                                ),
                            }),
                            None,
                            Shared::new(Node {
                                token_id: 0.into(),
                                expr: Expr::Literal(Literal::None),
                            }),
                        ),
                    })
                ]))]
#[case::question_mark_after_call_with_args(
                vec![
                    token(TokenKind::Ident(SmolStr::new("bar"))),
                    token(TokenKind::LParen),
                    token(TokenKind::StringLiteral("arg".to_owned())),
                    token(TokenKind::RParen),
                    token(TokenKind::Question),
                    token(TokenKind::Eof),
                ],
                Ok(vec![
                    Shared::new(Node {
                        token_id: 1.into(),
                        expr: Expr::Try(
                            Shared::new(Node {
                                token_id: 1.into(),
                                expr: Expr::Call(
                                    IdentWithToken::new_with_token("bar", Some(1.into())),
                                    smallvec![
                                        Shared::new(Node {
                                            token_id: 0.into(),
                                            expr: Expr::Literal(Literal::String("arg".to_owned())),
                                        }),
                                    ],
                                ),
                            }),
                            None,
                            Shared::new(Node {
                                token_id: 1.into(),
                                expr: Expr::Literal(Literal::None),
                            }),
                        ),
                    })
                ]))]
#[case::question_mark_after_call_error(
                vec![
                    token(TokenKind::Ident(SmolStr::new("foo"))),
                    token(TokenKind::Question),
                    token(TokenKind::Eof),
                ],
                Err(SyntaxError::UnexpectedToken(token(TokenKind::Ident("foo".into())))))]
#[case::coalesce_simple(
                    vec![
                        token(TokenKind::StringLiteral("foo".to_owned())),
                        token(TokenKind::Coalesce),
                        token(TokenKind::StringLiteral("bar".to_owned())),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Expr::Call(
                                IdentWithToken::new_with_token(constants::builtins::COALESCE, Some(1.into())),
                                smallvec![
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Expr::Literal(Literal::String("foo".to_owned())),
                                    }),
                                    Shared::new(Node {
                                        token_id: 2.into(),
                                        expr: Expr::Literal(Literal::String("bar".to_owned())),
                                    }),
                                ],
                            ),
                        })
                    ]))]
#[case::coalesce_with_none(
                    vec![
                        token(TokenKind::None),
                        token(TokenKind::Coalesce),
                        token(TokenKind::StringLiteral("default".to_owned())),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Expr::Call(
                                IdentWithToken::new_with_token(constants::builtins::COALESCE, Some(1.into())),
                                smallvec![
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Expr::Literal(Literal::None),
                                    }),
                                    Shared::new(Node {
                                        token_id: 2.into(),
                                        expr: Expr::Literal(Literal::String("default".to_owned())),
                                    }),
                                ],
                            ),
                        })
                    ]))]
#[case::coalesce_with_identifiers(
                    vec![
                        token(TokenKind::Ident(SmolStr::new("x"))),
                        token(TokenKind::Coalesce),
                        token(TokenKind::Ident(SmolStr::new("y"))),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Expr::Call(
                                IdentWithToken::new_with_token(constants::builtins::COALESCE, Some(1.into())),
                                smallvec![
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Expr::Ident(IdentWithToken::new_with_token("x", Some(0.into()))),
                                    }),
                                    Shared::new(Node {
                                        token_id: 2.into(),
                                        expr: Expr::Ident(IdentWithToken::new_with_token("y", Some(2.into()))),
                                    }),
                                ],
                            ),
                        })
                    ]))]
#[case::coalesce_error_missing_rhs(
                    vec![
                        token(TokenKind::StringLiteral("foo".to_owned())),
                        token(TokenKind::Coalesce),
                        token(TokenKind::Eof)
                    ],
                    Err(SyntaxError::UnexpectedEOFAfterToken(token(TokenKind::Coalesce))))]
#[case::negate_simple(
        vec![
            token(TokenKind::Minus),
            token(TokenKind::NumberLiteral(42.into())),
            token(TokenKind::Eof)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Expr::UnaryOp(UnaryOp::Neg, Shared::new(Node {
                            token_id: 1.into(),
                            expr: Expr::Literal(Literal::Number(42.into())),
                        })),
            })
        ]))]
#[case::negate_with_identifier(
        vec![
            token(TokenKind::Minus),
            token(TokenKind::Ident(SmolStr::new("x"))),
            token(TokenKind::Eof)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Expr::UnaryOp(UnaryOp::Neg, Shared::new(Node {
                            token_id: 1.into(),
                            expr: Expr::Ident(IdentWithToken::new_with_token("x", Some(1.into()))),
                        })),
            })
        ]))]
#[case::negate_paren_expr(
        vec![
            token(TokenKind::Minus),
            token(TokenKind::LParen),
            token(TokenKind::NumberLiteral(1.into())),
            token(TokenKind::RParen),
            token(TokenKind::Eof)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Expr::UnaryOp(
                    UnaryOp::Neg,
                    Shared::new(Node {
                        token_id: 1.into(),
                        expr: Expr::Paren(Shared::new(Node {
                            token_id: 2.into(),
                            expr: Expr::Literal(Literal::Number(1.into())),
                        })),
                    })
                ),
            })
        ]))]
#[case::negate_string(
        vec![
            token(TokenKind::Minus),
            token(TokenKind::StringLiteral("a".to_owned())),
            token(TokenKind::Eof)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Expr::UnaryOp(
                    UnaryOp::Neg,
                    Shared::new(Node {
                        token_id: 1.into(),
                        expr: Expr::Literal(Literal::String("a".to_owned())),
                    })
                ),
            })
        ]))]
#[case::negate_error_missing_rhs(
        vec![
            token(TokenKind::Minus),
            token(TokenKind::Eof)
        ],
        Err(SyntaxError::UnexpectedEOFAfterToken(token(TokenKind::Minus))))]
#[case::import_simple(
            vec![
            token(TokenKind::Import),
            token(TokenKind::StringLiteral("name".to_owned())),
            token(TokenKind::Eof),
            ],
            Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Expr::Import(
                    Literal::String("name".to_owned()),
                    None,
                ),
            })
            ]))]
#[case::import_as(
            vec![
            token(TokenKind::Import),
            token(TokenKind::StringLiteral("name".to_owned())),
            token(TokenKind::As),
            token(TokenKind::Ident(SmolStr::new("alias"))),
            token(TokenKind::Eof),
            ],
            Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Expr::Import(
                    Literal::String("name".to_owned()),
                    Some(IdentWithToken::new_with_token(
                        "alias",
                        Some(1.into()),
                    )),
                ),
            })
            ]))]
#[case::import_as_missing_ident(
            vec![
            token(TokenKind::Import),
            token(TokenKind::StringLiteral("name".to_owned())),
            token(TokenKind::As),
            token(TokenKind::Eof),
            ],
            Err(SyntaxError::UnexpectedToken(token(TokenKind::Eof))))]
#[case::qualified_access(
            vec![
            token(TokenKind::Ident(SmolStr::new("test"))),
            token(TokenKind::DoubleColon),
            token(TokenKind::Ident(SmolStr::new("foo"))),
            token(TokenKind::Eof),
            ],
            Ok(vec![
                Shared::new(Node {
                    token_id: 1.into(),
                    expr: Expr::QualifiedAccess(
                        vec![IdentWithToken::new_with_token("test", Some(0.into()))],
                        AccessTarget::Ident(IdentWithToken::new_with_token("foo", Some(1.into())),
                    )),
                })
            ]))]
#[case::qualified_access_with_call(
        vec![
            token(TokenKind::Ident(SmolStr::new("mod"))),
            token(TokenKind::DoubleColon),
            token(TokenKind::Ident(SmolStr::new("func"))),
            token(TokenKind::LParen),
            token(TokenKind::StringLiteral("arg".to_owned())),
            token(TokenKind::RParen),
            token(TokenKind::Eof),
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 4.into(),
                expr: Expr::QualifiedAccess(
                    vec![IdentWithToken::new_with_token("mod", Some(0.into()))],
                    AccessTarget::Call(
                        IdentWithToken::new_with_token("func", Some(2.into())),
                        Box::new(smallvec![
                            Shared::new(Node {
                                token_id: 1.into(),
                                expr: Expr::Literal(Literal::String("arg".to_owned())),
                            }),
                        ]),
                    ),
                ),
            })
        ]))]
#[case::qualified_access_multi_level(
        vec![
            token(TokenKind::Ident(SmolStr::new("mod1"))),
            token(TokenKind::DoubleColon),
            token(TokenKind::Ident(SmolStr::new("mod2"))),
            token(TokenKind::DoubleColon),
            token(TokenKind::Ident(SmolStr::new("func"))),
            token(TokenKind::LParen),
            token(TokenKind::RParen),
            token(TokenKind::Eof),
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 4.into(),
                expr: Expr::QualifiedAccess(
                    vec![
                        IdentWithToken::new_with_token("mod1", Some(0.into())),
                        IdentWithToken::new_with_token("mod2", Some(1.into())),
                    ],
                    AccessTarget::Call(
                        IdentWithToken::new_with_token("func", Some(2.into())),
                        Box::new(smallvec![]),
                    ),
                ),
            })
        ]))]
#[case::slice_access_with_start_only(
            vec![
                token(TokenKind::Ident(SmolStr::new("arr"))),
                token(TokenKind::LBracket),
                token(TokenKind::NumberLiteral(1.into())),
                token(TokenKind::Colon),
                token(TokenKind::RBracket),
                token(TokenKind::Eof)
            ],
            Ok(vec![
                Shared::new(Node {
                    token_id: 4.into(),
                    expr: Expr::Call(
                        IdentWithToken::new_with_token(constants::builtins::SLICE, Some(0.into())),
                        smallvec![
                            Shared::new(Node {
                                token_id: 0.into(),
                                expr: Expr::Ident(IdentWithToken::new_with_token("arr", Some(0.into()))),
                            }),
                            Shared::new(Node {
                                token_id: 1.into(),
                                expr: Expr::Literal(Literal::Number(1.into())),
                            }),
                            Shared::new(Node {
                                token_id: 3.into(),
                                expr: Expr::Call(
                                    IdentWithToken::new_with_token("len", None),
                                    smallvec![
                                        Shared::new(Node {
                                            token_id: 0.into(),
                                            expr: Expr::Ident(IdentWithToken::new_with_token("arr", Some(0.into()))),
                                        }),
                                    ],
                                ),
                            }),
                        ],
                    ),
                })
            ]))]
#[case::slice_access_with_end_only(
        vec![
            token(TokenKind::Ident(SmolStr::new("arr"))),
            token(TokenKind::LBracket),
            token(TokenKind::Colon),
            token(TokenKind::NumberLiteral(2.into())),
            token(TokenKind::RBracket),
            token(TokenKind::Eof),
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 3.into(),
                expr: Expr::Call(
                    IdentWithToken::new_with_token(constants::builtins::SLICE, Some(0.into())),
                    smallvec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Expr::Ident(IdentWithToken::new_with_token("arr", Some(0.into()))),
                        }),
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Expr::Literal(Literal::Number(0.into())),
                        }),
                        Shared::new(Node {
                            token_id: 2.into(),
                            expr: Expr::Literal(Literal::Number(2.into())),
                        }),
                    ],
                ),
            })
        ]))]
#[case::qualified_access_with_call_and_slice(
        vec![
            token(TokenKind::Ident(SmolStr::new("mod"))),
            token(TokenKind::DoubleColon),
            token(TokenKind::Ident(SmolStr::new("func"))),
            token(TokenKind::LParen),
            token(TokenKind::RParen),
            token(TokenKind::LBracket),
            token(TokenKind::NumberLiteral(0.into())),
            token(TokenKind::Colon),
            token(TokenKind::NumberLiteral(2.into())),
            token(TokenKind::RBracket),
            token(TokenKind::Eof),
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 3.into(),
                expr: Expr::Call(
                    IdentWithToken::new_with_token(constants::builtins::SLICE, Some(0.into())),
                    smallvec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Expr::QualifiedAccess(
                                vec![IdentWithToken::new_with_token("mod", Some(0.into()))],
                                AccessTarget::Call(
                                    IdentWithToken::new_with_token("func", Some(1.into())),
                                    Box::new(smallvec![]),
                                ),
                            ),
                        }),
                        Shared::new(Node {
                            token_id: 2.into(),
                            expr: Expr::Literal(Literal::Number(0.into())),
                        }),
                        Shared::new(Node {
                            token_id: 3.into(),
                            expr: Expr::Literal(Literal::Number(2.into())),
                        }),
                    ],
                ),
            })
        ]))]
#[case::qualified_access_with_call_and_end_only_slice(
        vec![
            token(TokenKind::Ident(SmolStr::new("mod"))),
            token(TokenKind::DoubleColon),
            token(TokenKind::Ident(SmolStr::new("func"))),
            token(TokenKind::LParen),
            token(TokenKind::RParen),
            token(TokenKind::LBracket),
            token(TokenKind::Colon),
            token(TokenKind::NumberLiteral(1.into())),
            token(TokenKind::RBracket),
            token(TokenKind::Eof),
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 3.into(),
                expr: Expr::Call(
                    IdentWithToken::new_with_token(constants::builtins::SLICE, Some(0.into())),
                    smallvec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Expr::QualifiedAccess(
                                vec![IdentWithToken::new_with_token("mod", Some(0.into()))],
                                AccessTarget::Call(
                                    IdentWithToken::new_with_token("func", Some(1.into())),
                                    Box::new(smallvec![]),
                                ),
                            ),
                        }),
                        Shared::new(Node {
                            token_id: 2.into(),
                            expr: Expr::Literal(Literal::Number(0.into())),
                        }),
                        Shared::new(Node {
                            token_id: 3.into(),
                            expr: Expr::Literal(Literal::Number(1.into())),
                        }),
                    ],
                ),
            })
        ]))]
#[case::selector_dot_is_self(
                vec![
                    token(TokenKind::Selector(SmolStr::new("."))),
                    token(TokenKind::Eof)
                ],
                Ok(vec![
                    Shared::new(Node {
                        token_id: 0.into(),
                        expr: Expr::Self_,
                    })
                ]))]
#[case::ident_with_single_attr(
                    vec![
                        token(TokenKind::Ident(SmolStr::new("obj"))),
                        token(TokenKind::Selector(SmolStr::new(".name"))),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Expr::Call(
                                IdentWithToken::new_with_token(constants::builtins::ATTR, Some(1.into())),
                                smallvec![
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Expr::Ident(IdentWithToken::new_with_token("obj", Some(0.into()))),
                                    }),
                                    Shared::new(Node {
                                        token_id: 1.into(),
                                        expr: Expr::Literal(Literal::String("name".to_owned())),
                                    }),
                                ],
                            ),
                        })
                    ]))]
#[case::function_call_result_with_attr(
                    vec![
                        token(TokenKind::Ident(SmolStr::new("get_user"))),
                        token(TokenKind::LParen),
                        token(TokenKind::RParen),
                        token(TokenKind::Selector(SmolStr::new(".name"))),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 3.into(),
                            expr: Expr::Call(
                                IdentWithToken::new_with_token(constants::builtins::ATTR, Some(1.into())),
                                smallvec![
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Expr::Call(
                                            IdentWithToken::new_with_token("get_user", Some(0.into())),
                                            SmallVec::new(),
                                        ),
                                    }),
                                    Shared::new(Node {
                                        token_id: 1.into(),
                                        expr: Expr::Literal(Literal::String("name".to_owned())),
                                    }),
                                ],
                            ),
                        })
                    ]))]
#[case::ident_with_attr_in_pipe(
                    vec![
                        token(TokenKind::Ident(SmolStr::new("data"))),
                        token(TokenKind::Pipe),
                        token(TokenKind::Ident(SmolStr::new("obj"))),
                        token(TokenKind::Selector(SmolStr::new(".value"))),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Expr::Ident(IdentWithToken::new_with_token("data", Some(0.into()))),
                        }),
                        Shared::new(Node {
                            token_id: 3.into(),
                            expr: Expr::Call(
                                IdentWithToken::new_with_token(constants::builtins::ATTR, Some(2.into())),
                                smallvec![
                                    Shared::new(Node {
                                        token_id: 1.into(),
                                        expr: Expr::Ident(IdentWithToken::new_with_token("obj", Some(1.into()))),
                                    }),
                                    Shared::new(Node {
                                        token_id: 2.into(),
                                        expr: Expr::Literal(Literal::String("value".to_owned())),
                                    }),
                                ],
                            ),
                        })
                    ]))]
#[case::self_with_attr(
                    vec![
                        token(TokenKind::Self_),
                        token(TokenKind::Selector(SmolStr::new(".value"))),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Expr::Call(
                                IdentWithToken::new_with_token(constants::builtins::ATTR, Some(1.into())),
                                smallvec![
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Expr::Self_,
                                    }),
                                    Shared::new(Node {
                                        token_id: 1.into(),
                                        expr: Expr::Literal(Literal::String("value".to_owned())),
                                    }),
                                ],
                            ),
                        })
                    ]))]
#[case::pipe_equal_with_selector(
                    vec![
                        token(TokenKind::Selector(SmolStr::new(".h1"))),
                        token(TokenKind::Selector(SmolStr::new(".value"))),
                        token(TokenKind::PipeEqual),
                        token(TokenKind::StringLiteral("new_id".to_owned())),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 3.into(),
                            expr: Expr::Call(
                                IdentWithToken::new_with_token(constants::builtins::SET_ATTR, Some(2.into())),
                                smallvec![
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Expr::Selector(selector::Selector::Heading(Some(1))),
                                    }),
                                    Shared::new(Node {
                                        token_id: 1.into(),
                                        expr: Expr::Literal(Literal::String("value".to_owned())),
                                    }),
                                    Shared::new(Node {
                                        token_id: 2.into(),
                                        expr: Expr::Literal(Literal::String("new_id".to_owned())),
                                    }),
                                ],
                            ),
                        })
                    ]))]
#[case::pipe_equal_with_ident_and_attr(
                    vec![
                        token(TokenKind::Ident(SmolStr::new("obj"))),
                        token(TokenKind::Selector(SmolStr::new(".value"))),
                        token(TokenKind::PipeEqual),
                        token(TokenKind::StringLiteral("John".to_owned())),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 3.into(),
                            expr: Expr::Call(
                                IdentWithToken::new_with_token(constants::builtins::SET_ATTR, Some(2.into())),
                                smallvec![
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Expr::Ident(IdentWithToken::new_with_token("obj", Some(0.into()))),
                                    }),
                                    Shared::new(Node {
                                        token_id: 1.into(),
                                        expr: Expr::Literal(Literal::String("value".to_owned())),
                                    }),
                                    Shared::new(Node {
                                        token_id: 2.into(),
                                        expr: Expr::Literal(Literal::String("John".to_owned())),
                                    }),
                                ],
                            ),
                        })
                    ]))]
#[case::pipe_equal_with_self_and_attr(
                    vec![
                        token(TokenKind::Self_),
                        token(TokenKind::Selector(SmolStr::new(".value"))),
                        token(TokenKind::PipeEqual),
                        token(TokenKind::NumberLiteral(42.into())),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 2.into(),
                            expr: Expr::Call(
                                IdentWithToken::new_with_token(constants::builtins::SET_ATTR, Some(2.into())),
                                smallvec![
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Expr::Self_,
                                    }),
                                    Shared::new(Node {
                                        token_id: 1.into(),
                                        expr: Expr::Literal(Literal::String("value".to_owned())),
                                    }),
                                    Shared::new(Node {
                                        token_id: 2.into(),
                                        expr: Expr::Literal(Literal::Number(42.into())),
                                    }),
                                ],
                            ),
                        })
                    ]))]
#[case::let_with_reserved_keyword_as_value(
                        vec![
                            token(TokenKind::Let),
                            token(TokenKind::Ident(SmolStr::new("aaa"))),
                            token(TokenKind::Equal),
                            token(TokenKind::Let), // Using "let" as a value (should error)
                            token(TokenKind::Eof)
                        ],
                        Err(SyntaxError::UnexpectedToken(Token {
                            range: Range::default(),
                            kind: TokenKind::Let,
                            module_id: 1.into(),
                        }))
                    )]
#[case::let_with_reserved_keyword_as_variable_and_value(
                        vec![
                            token(TokenKind::Let),
                            token(TokenKind::Let), // Using "let" as a variable name (should error)
                            token(TokenKind::Equal),
                            token(TokenKind::Ident(SmolStr::new("vvv"))),
                            token(TokenKind::Eof)
                        ],
                        Err(SyntaxError::UnexpectedToken(Token {
                            range: Range::default(),
                            kind: TokenKind::Let,
                            module_id: 1.into(),
                        }))
                    )]
#[case::let_with_reserved_keyword_as_variable_and_value2(
                        vec![
                            token(TokenKind::Let),
                            token(TokenKind::Let), // Using "let" as a variable name (should error)
                            token(TokenKind::Equal),
                            token(TokenKind::Let), // Using "let" as a value (should error)
                            token(TokenKind::Eof)
                        ],
                        Err(SyntaxError::UnexpectedToken(Token {
                            range: Range::default(),
                            kind: TokenKind::Let,
                            module_id: 1.into(),
                        }))
                    )]
#[case::def_with_variadic_param(
        vec![
            token(TokenKind::Def),
            token(TokenKind::Ident(SmolStr::new("f"))),
            token(TokenKind::LParen),
            token(TokenKind::Asterisk),
            token(TokenKind::Ident(SmolStr::new("args"))),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::Ident(SmolStr::new("args"))),
            token(TokenKind::SemiColon)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Expr::Def(
                        IdentWithToken::new_with_token("f", Some(4.into())),
                        vec![
                            Param::variadic(IdentWithToken::new_with_token("args", Some(1.into()))),
                        ],
                        vec![Shared::new(Node {
                            token_id: 3.into(),
                            expr: Expr::Ident(IdentWithToken::new_with_token("args", Some(3.into()))),
                        })],
                ),
            }),
        ]))]
#[case::def_with_regular_and_variadic_param(
        vec![
            token(TokenKind::Def),
            token(TokenKind::Ident(SmolStr::new("f"))),
            token(TokenKind::LParen),
            token(TokenKind::Ident(SmolStr::new("a"))),
            token(TokenKind::Comma),
            token(TokenKind::Asterisk),
            token(TokenKind::Ident(SmolStr::new("rest"))),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::Ident(SmolStr::new("rest"))),
            token(TokenKind::SemiColon)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Expr::Def(
                        IdentWithToken::new_with_token("f", Some(5.into())),
                        vec![
                            Param::new(IdentWithToken::new_with_token("a", Some(1.into()))),
                            Param::variadic(IdentWithToken::new_with_token("rest", Some(2.into()))),
                        ],
                        vec![Shared::new(Node {
                            token_id: 4.into(),
                            expr: Expr::Ident(IdentWithToken::new_with_token("rest", Some(4.into()))),
                        })],
                ),
            }),
        ]))]
#[case::def_variadic_param_not_last(
        vec![
            token(TokenKind::Def),
            token(TokenKind::Ident(SmolStr::new("f"))),
            token(TokenKind::LParen),
            token(TokenKind::Asterisk),
            token(TokenKind::Ident(SmolStr::new("a"))),
            token(TokenKind::Comma),
            token(TokenKind::Ident(SmolStr::new("b"))),
            token(TokenKind::RParen),
        ],
        Err(SyntaxError::VariadicParameterMustBeLast(Token{range: Range::default(), kind: TokenKind::Ident(SmolStr::new("b")), module_id: 1.into()})))]
#[case::def_multiple_variadic_params(
        vec![
            token(TokenKind::Def),
            token(TokenKind::Ident(SmolStr::new("f"))),
            token(TokenKind::LParen),
            token(TokenKind::Asterisk),
            token(TokenKind::Ident(SmolStr::new("a"))),
            token(TokenKind::Comma),
            token(TokenKind::Asterisk),
            token(TokenKind::Ident(SmolStr::new("b"))),
            token(TokenKind::RParen),
        ],
        Err(SyntaxError::MultipleVariadicParameters(Token{range: Range::default(), kind: TokenKind::Asterisk, module_id: 1.into()})))]
#[case::def_without_params(
        vec![
            token(TokenKind::Def),
            token(TokenKind::Ident(SmolStr::new("f"))),
            token(TokenKind::Colon),
            token(TokenKind::Ident(SmolStr::new("args"))),
            token(TokenKind::SemiColon)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Expr::Def(
                        IdentWithToken::new_with_token("f", Some(3.into())),
                        vec![],
                        vec![Shared::new(Node {
                            token_id: 2.into(),
                            expr: Expr::Ident(IdentWithToken::new_with_token("args", Some(2.into()))),
                        })],
                ),
            }),
        ]))]
#[case::def_without_params_with_do(
        vec![
            token(TokenKind::Def),
            token(TokenKind::Ident(SmolStr::new("f"))),
            token(TokenKind::Do),
            token(TokenKind::Ident(SmolStr::new("args"))),
            token(TokenKind::SemiColon)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Expr::Def(
                        IdentWithToken::new_with_token("f", Some(3.into())),
                        vec![],
                        vec![Shared::new(Node {
                            token_id: 2.into(),
                            expr: Expr::Ident(IdentWithToken::new_with_token("args", Some(2.into()))),
                        })],
                ),
            }),
        ]))]
#[case::arrow_simple(
        vec![
            token(TokenKind::Arrow),
            token(TokenKind::LParen),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::StringLiteral("result".to_owned())),
            token(TokenKind::SemiColon),
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Expr::Fn(
                    Vec::new(),
                    vec![
                        Shared::new(Node {
                            token_id: 2.into(),
                            expr: Expr::Literal(Literal::String("result".to_owned())),
                        })
                    ],
                ),
            })
        ]))]
#[case::arrow_with_args(
        vec![
            token(TokenKind::Arrow),
            token(TokenKind::LParen),
            token(TokenKind::Ident(SmolStr::new("x"))),
            token(TokenKind::Comma),
            token(TokenKind::Ident(SmolStr::new("y"))),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::Ident(SmolStr::new("contains"))),
            token(TokenKind::LParen),
            token(TokenKind::Ident(SmolStr::new("x"))),
            token(TokenKind::Comma),
            token(TokenKind::Ident(SmolStr::new("y"))),
            token(TokenKind::RParen),
            token(TokenKind::SemiColon),
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Expr::Fn(
                    vec![
                        Param::new(IdentWithToken::new_with_token("x", Some(1.into()))),
                        Param::new(IdentWithToken::new_with_token("y", Some(2.into()))),
                    ],
                    vec![
                        Shared::new(Node {
                            token_id: 6.into(),
                            expr: Expr::Call(
                                IdentWithToken::new_with_token("contains", Some(6.into())),
                                smallvec![
                                    Shared::new(Node {
                                        token_id: 4.into(),
                                        expr: Expr::Ident(IdentWithToken::new_with_token("x", Some(4.into()))),
                                    }),
                                    Shared::new(Node {
                                        token_id: 5.into(),
                                        expr: Expr::Ident(IdentWithToken::new_with_token("y", Some(5.into()))),
                                    }),
                                ],
                            ),
                        })
                    ],
                ),
            })
        ]))]
#[case::arrow_nested_in_call(
        vec![
            token(TokenKind::Ident(SmolStr::new("apply"))),
            token(TokenKind::LParen),
            token(TokenKind::Arrow),
            token(TokenKind::LParen),
            token(TokenKind::Ident(SmolStr::new("x"))),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::StringLiteral("processed".to_owned())),
            token(TokenKind::SemiColon),
            token(TokenKind::RParen),
            token(TokenKind::Eof),
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 4.into(),
                expr: Expr::Call(
                    IdentWithToken::new_with_token("apply", Some(4.into())),
                    smallvec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Expr::Fn(
                                vec![
                                  Param::new(IdentWithToken::new_with_token("x", Some(1.into()))),
                                ],
                                vec![
                                    Shared::new(Node {
                                        token_id: 3.into(),
                                        expr: Expr::Literal(Literal::String("processed".to_owned())),
                                    })
                                ],
                            ),
                        })
                    ],
                ),
            })
        ]))]
fn test_parse(#[case] input: Vec<Token>, #[case] expected: Result<Program, SyntaxError>) {
    let mut arena = Arena::new(10);
    let result = Parser::new(input.iter(), &mut arena, Module::TOP_LEVEL_MODULE_ID).parse();

    match (&result, &expected) {
        (Ok(actual), Ok(expected)) => {
            assert_eq!(actual.len(), expected.len());
            let actual_exprs: Vec<_> = actual.iter().map(|a| &a.expr).collect();
            let expected_exprs: Vec<_> = expected.iter().map(|e| &e.expr).collect();
            assert_eq!(actual_exprs, expected_exprs);
        }
        (Err(actual), Err(expected)) => {
            assert_eq!(actual, expected);
        }
        _ => {
            panic!("Mismatch: actual = {:?}, expected = {:?}", result, expected)
        }
    }
}

#[rstest]
#[case::heading(".h1", Selector::Heading(Some(1)))]
#[case::heading_h3(".h3", Selector::Heading(Some(3)))]
#[case::blockquote(".>", Selector::Blockquote)]
#[case::blockquote_full(".blockquote", Selector::Blockquote)]
#[case::footnote(".^", Selector::Footnote)]
#[case::footnote_full(".footnote", Selector::Footnote)]
#[case::mdx_jsx_flow(".mdx_jsx_flow_element", Selector::MdxJsxFlowElement)]
#[case::mdx_jsx_flow_short(".<", Selector::MdxJsxFlowElement)]
#[case::emphasis(".**", Selector::Emphasis)]
#[case::emphasis_full(".emphasis", Selector::Emphasis)]
#[case::math(".$$", Selector::Math)]
#[case::math_full(".math", Selector::Math)]
#[case::horizontal_rule(".---", Selector::HorizontalRule)]
#[case::horizontal_rule_alt(".***", Selector::HorizontalRule)]
#[case::horizontal_rule_full(".horizontal_rule", Selector::HorizontalRule)]
#[case::mdx_text_expression(".{}", Selector::MdxTextExpression)]
#[case::mdx_text_expression_full(".mdx_text_expression", Selector::MdxTextExpression)]
#[case::footnote_ref(".[^]", Selector::FootnoteRef)]
#[case::footnote_ref_full(".footnote_ref", Selector::FootnoteRef)]
#[case::definition(".definition", Selector::Definition)]
#[case::break_selector(".break", Selector::Break)]
#[case::delete(".delete", Selector::Delete)]
#[case::html(".<>", Selector::Html)]
#[case::html_full(".html", Selector::Html)]
#[case::image(".image", Selector::Image)]
#[case::image_ref(".image_ref", Selector::ImageRef)]
#[case::code(".code", Selector::Code)]
#[case::code_inline(".code_inline", Selector::InlineCode)]
#[case::math_inline(".math_inline", Selector::InlineMath)]
#[case::link(".link", Selector::Link)]
#[case::link_ref(".link_ref", Selector::LinkRef)]
#[case::list(".list", Selector::List(None, None))]
#[case::toml(".toml", Selector::Toml)]
#[case::strong(".strong", Selector::Strong)]
#[case::yaml(".yaml", Selector::Yaml)]
#[case::text(".text", Selector::Text)]
#[case::mdx_js_esm(".mdx_js_esm", Selector::MdxJsEsm)]
#[case::mdx_jsx_text_element(".mdx_jsx_text_element", Selector::MdxJsxTextElement)]
#[case::mdx_flow_expression(".mdx_flow_expression", Selector::MdxFlowExpression)]
fn test_parse_selector(#[case] selector_str: &str, #[case] expected_selector: Selector) {
    let mut arena = Arena::new(10);
    let token = Token {
        range: Range::default(),
        kind: TokenKind::Selector(SmolStr::new(selector_str)),
        module_id: 1.into(),
    };

    let tokens = [
        token.clone(),
        Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: 1.into(),
        },
    ];

    let result = Parser::new(tokens.iter(), &mut arena, Module::TOP_LEVEL_MODULE_ID).parse();

    match result {
        Ok(program) => {
            assert_eq!(program.len(), 1);
            if let Expr::Selector(selector) = &program[0].expr {
                assert_eq!(*selector, expected_selector);
            } else {
                panic!("Expected Selector expression, got {:?}", program[0].expr);
            }
        }
        Err(err) => panic!("Parse error: {:?}", err),
    }
}

#[rstest]
#[case(".", Some(1), None, Selector::List(Some(1), None))]
#[case(".", Some(2), Some(3), Selector::Table(Some(2), Some(3)))]
fn test_parse_array_selector(
    #[case] selector_str: &str,
    #[case] first_idx: Option<usize>,
    #[case] second_idx: Option<usize>,
    #[case] expected_selector: Selector,
) {
    let mut arena = Arena::new(10);
    let mut tokens = vec![
        Token {
            range: Range::default(),
            kind: TokenKind::Selector(SmolStr::new(selector_str)),
            module_id: 1.into(),
        },
        Token {
            range: Range::default(),
            kind: TokenKind::LBracket,
            module_id: 1.into(),
        },
    ];

    if let Some(idx) = first_idx {
        tokens.push(Token {
            range: Range::default(),
            kind: TokenKind::NumberLiteral(idx.into()),
            module_id: 1.into(),
        });
    }

    tokens.push(Token {
        range: Range::default(),
        kind: TokenKind::RBracket,
        module_id: 1.into(),
    });

    if second_idx.is_some() {
        tokens.push(Token {
            range: Range::default(),
            kind: TokenKind::LBracket,
            module_id: 1.into(),
        });

        if let Some(idx) = second_idx {
            tokens.push(Token {
                range: Range::default(),
                kind: TokenKind::NumberLiteral(idx.into()),
                module_id: 1.into(),
            });
        }

        tokens.push(Token {
            range: Range::default(),
            kind: TokenKind::RBracket,
            module_id: 1.into(),
        });
    }

    tokens.push(Token {
        range: Range::default(),
        kind: TokenKind::Eof,
        module_id: 1.into(),
    });

    let result = Parser::new(tokens.iter(), &mut arena, Module::TOP_LEVEL_MODULE_ID).parse();

    match result {
        Ok(program) => {
            assert_eq!(program.len(), 1);
            if let Expr::Selector(selector) = &program[0].expr {
                assert_eq!(*selector, expected_selector);
            } else {
                panic!("Expected Selector expression, got {:?}", program[0].expr);
            }
        }
        Err(err) => panic!("Parse error: {:?}", err),
    }
}

// ."key"[] or ."key"[n]: single property selector followed by array iterator/index
#[rstest]
#[case::all_elements(".\"items\"", None, vec![Selector::Property("items".into()), Selector::List(None, None)])]
#[case::index_0(".\"items\"", Some(0usize), vec![Selector::Property("items".into()), Selector::List(Some(0), None)])]
#[case::index_2(".\"items\"", Some(2usize), vec![Selector::Property("items".into()), Selector::List(Some(2), None)])]
fn test_parse_property_selector_iterator(
    #[case] selector_str: &str,
    #[case] index: Option<usize>,
    #[case] expected: Vec<Selector>,
) {
    let mut arena = Arena::new(10);
    let mut tokens = vec![
        Token {
            range: Range::default(),
            kind: TokenKind::Selector(SmolStr::new(selector_str)),
            module_id: 1.into(),
        },
        Token {
            range: Range::default(),
            kind: TokenKind::LBracket,
            module_id: 1.into(),
        },
    ];
    if let Some(idx) = index {
        tokens.push(Token {
            range: Range::default(),
            kind: TokenKind::NumberLiteral(idx.into()),
            module_id: 1.into(),
        });
    }
    tokens.push(Token {
        range: Range::default(),
        kind: TokenKind::RBracket,
        module_id: 1.into(),
    });
    tokens.push(Token {
        range: Range::default(),
        kind: TokenKind::Eof,
        module_id: 1.into(),
    });

    let result = Parser::new(tokens.iter(), &mut arena, Module::TOP_LEVEL_MODULE_ID)
        .parse()
        .expect("parse error");

    assert_eq!(result.len(), 1);
    let Expr::Block(nodes) = &result[0].expr else {
        panic!("expected Block, got {:?}", result[0].expr);
    };
    assert_eq!(nodes.len(), expected.len());
    for (node, sel) in nodes.iter().zip(expected.iter()) {
        let Expr::Selector(actual) = &node.expr else {
            panic!("expected Selector, got {:?}", node.expr);
        };
        assert_eq!(actual, sel);
    }
}

// ."a"."b"[] or ."a"."b"[n]: chained property selectors followed by array iterator/index
#[rstest]
#[case::all_elements(".\"a\"", ".\"b\"", None, vec![Selector::Property("a".into()), Selector::Property("b".into()), Selector::List(None, None)])]
#[case::index_0(".\"a\"", ".\"b\"", Some(0usize), vec![Selector::Property("a".into()), Selector::Property("b".into()), Selector::List(Some(0), None)])]
#[case::index_1(".\"a\"", ".\"b\"", Some(1usize), vec![Selector::Property("a".into()), Selector::Property("b".into()), Selector::List(Some(1), None)])]
fn test_parse_chained_property_selector_iterator(
    #[case] first: &str,
    #[case] second: &str,
    #[case] index: Option<usize>,
    #[case] expected: Vec<Selector>,
) {
    let mut arena = Arena::new(10);
    let mut tokens = vec![
        Token {
            range: Range::default(),
            kind: TokenKind::Selector(SmolStr::new(first)),
            module_id: 1.into(),
        },
        Token {
            range: Range::default(),
            kind: TokenKind::Selector(SmolStr::new(second)),
            module_id: 1.into(),
        },
        Token {
            range: Range::default(),
            kind: TokenKind::LBracket,
            module_id: 1.into(),
        },
    ];
    if let Some(idx) = index {
        tokens.push(Token {
            range: Range::default(),
            kind: TokenKind::NumberLiteral(idx.into()),
            module_id: 1.into(),
        });
    }
    tokens.push(Token {
        range: Range::default(),
        kind: TokenKind::RBracket,
        module_id: 1.into(),
    });
    tokens.push(Token {
        range: Range::default(),
        kind: TokenKind::Eof,
        module_id: 1.into(),
    });

    let result = Parser::new(tokens.iter(), &mut arena, Module::TOP_LEVEL_MODULE_ID)
        .parse()
        .expect("parse error");

    assert_eq!(result.len(), 1);
    let Expr::Block(nodes) = &result[0].expr else {
        panic!("expected Block, got {:?}", result[0].expr);
    };
    assert_eq!(nodes.len(), expected.len());
    for (node, sel) in nodes.iter().zip(expected.iter()) {
        let Expr::Selector(actual) = &node.expr else {
            panic!("expected Selector, got {:?}", node.expr);
        };
        assert_eq!(actual, sel);
    }
}

#[test]
fn test_parse_as_binding() {
    let mut arena = Arena::new(10);
    let tokens = [
        Token {
            range: Range::default(),
            kind: TokenKind::NumberLiteral(42.into()),
            module_id: 1.into(),
        },
        Token {
            range: Range::default(),
            kind: TokenKind::As,
            module_id: 1.into(),
        },
        Token {
            range: Range::default(),
            kind: TokenKind::Ident("x".into()),
            module_id: 1.into(),
        },
        Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: 1.into(),
        },
    ];
    let result = Parser::new(tokens.iter(), &mut arena, Module::TOP_LEVEL_MODULE_ID).parse();
    assert!(result.is_ok());
    let program = result.unwrap();
    assert_eq!(program.len(), 1);
    let Expr::As(ident, inner) = &program[0].expr else {
        panic!("expected Expr::As, got {:?}", program[0].expr);
    };
    assert_eq!(ident.name.to_string(), "x");
    assert!(matches!(&inner.expr, Expr::Literal(Literal::Number(_))));
}

#[test]
fn test_parse_env() {
    unsafe { std::env::set_var("MQ_TEST_VAR", "test_value") };
    let _guard = io_context::scoped(Shared::new(
        crate::io::SandboxedIo::new(crate::io::NativeIo::default()).allow_env(true),
    ));

    let mut arena = Arena::new(10);
    let tokens = [
        Token {
            range: Range::default(),
            kind: TokenKind::Env("MQ_TEST_VAR".into()),
            module_id: 1.into(),
        },
        Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: 1.into(),
        },
    ];

    let result = Parser::new(tokens.iter(), &mut arena, Module::TOP_LEVEL_MODULE_ID).parse();

    match result {
        Ok(program) => {
            assert_eq!(program.len(), 1);
            if let Expr::Literal(Literal::String(value)) = &program[0].expr {
                assert_eq!(value, "test_value");
            } else {
                panic!("Expected String literal, got {:?}", program[0].expr);
            }
        }
        Err(err) => panic!("Parse error: {:?}", err),
    }
}

#[test]
fn test_parse_env_not_found() {
    let _guard = io_context::scoped(Shared::new(
        crate::io::SandboxedIo::new(crate::io::NativeIo::default()).allow_env(true),
    ));

    let mut arena = Arena::new(10);
    let token = Token {
        range: Range::default(),
        kind: TokenKind::Env("MQ_NONEXISTENT_VAR".into()),
        module_id: 1.into(),
    };

    let tokens = [
        token.clone(),
        Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: 1.into(),
        },
    ];

    let result = Parser::new(tokens.iter(), &mut arena, Module::TOP_LEVEL_MODULE_ID).parse();

    assert!(matches!(
        result,
        Err(SyntaxError::EnvNotFound(_, var)) if var == "MQ_NONEXISTENT_VAR"
    ));
}

#[test]
fn test_parse_env_in_arguments() {
    unsafe { std::env::set_var("MQ_ARG_TEST", "env_arg_value") };
    let _guard = io_context::scoped(Shared::new(
        crate::io::SandboxedIo::new(crate::io::NativeIo::default()).allow_env(true),
    ));

    let mut arena = Arena::new(10);
    let tokens = [
        Token {
            range: Range::default(),
            kind: TokenKind::Ident(SmolStr::new("function")),
            module_id: 1.into(),
        },
        Token {
            range: Range::default(),
            kind: TokenKind::LParen,
            module_id: 1.into(),
        },
        Token {
            range: Range::default(),
            kind: TokenKind::Env("MQ_ARG_TEST".into()),
            module_id: 1.into(),
        },
        Token {
            range: Range::default(),
            kind: TokenKind::RParen,
            module_id: 1.into(),
        },
        Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: 1.into(),
        },
    ];

    let result = Parser::new(tokens.iter(), &mut arena, Module::TOP_LEVEL_MODULE_ID).parse();

    match result {
        Ok(program) => {
            assert_eq!(program.len(), 1);
            if let Expr::Call(ident, args) = &program[0].expr {
                assert_eq!(ident.name, "function".into());
                assert_eq!(args.len(), 1);
                if let Expr::Literal(Literal::String(value)) = &args[0].expr {
                    assert_eq!(value, "env_arg_value");
                } else {
                    panic!("Expected String literal in argument, got {:?}", args[0].expr);
                }
            } else {
                panic!("Expected Call expression, got {:?}", program[0].expr);
            }
        }
        Err(err) => panic!("Parse error: {:?}", err),
    }
}

#[rstest]
#[case::lang(".lang", "lang")]
#[case::value(".value", "value")]
#[case::depth(".depth", "depth")]
fn test_parse_standalone_attr_selector(#[case] selector: &str, #[case] attribute: &str) {
    let mut arena = Arena::new(10);
    let tokens = [
        Token {
            range: Range::default(),
            kind: TokenKind::Selector(SmolStr::new(selector)),
            module_id: 1.into(),
        },
        Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: 1.into(),
        },
    ];

    let result = Parser::new(tokens.iter(), &mut arena, Module::TOP_LEVEL_MODULE_ID).parse();

    match result {
        Ok(program) => {
            assert_eq!(program.len(), 1);
            if let Expr::Call(ident, args) = &program[0].expr {
                assert_eq!(ident.name, "attr".into());
                assert_eq!(args.len(), 2);
                assert!(matches!(&args[0].expr, Expr::Self_));
                if let Expr::Literal(Literal::String(attr_str)) = &args[1].expr {
                    assert_eq!(attr_str, attribute);
                } else {
                    panic!("Expected String literal in second argument, got {:?}", args[1].expr);
                }
            } else {
                panic!("Expected Call expression, got {:?}", program[0].expr);
            }
        }
        Err(err) => panic!("Parse error: {:?}", err),
    }
}

#[rstest]
#[case::h_value(vec![".h", ".value"], "h", "value")]
#[case::h1_value(vec![".h1", ".value"], "h1", "value")]
#[case::code_lang(vec![".code", ".lang"], "code", "lang")]
#[case::text_value(vec![".text", ".value"], "text", "value")]
fn test_parse_selector_with_attribute(
    #[case] selectors: Vec<&str>,
    #[case] base_selector: &str,
    #[case] attribute: &str,
) {
    let mut arena = Arena::new(10);
    let mut tokens = selectors
        .iter()
        .map(|selector| Token {
            range: Range::default(),
            kind: TokenKind::Selector(SmolStr::new(selector)),
            module_id: 1.into(),
        })
        .collect::<Vec<_>>();

    tokens.push(Token {
        range: Range::default(),
        kind: TokenKind::Eof,
        module_id: 1.into(),
    });

    let result = Parser::new(tokens.iter(), &mut arena, Module::TOP_LEVEL_MODULE_ID).parse();

    match result {
        Ok(program) => {
            assert_eq!(program.len(), 1);
            if let Expr::Call(ident, args) = &program[0].expr {
                // Should be transformed to attr(base_selector, "attribute")
                assert_eq!(ident.name, "attr".into());
                assert_eq!(args.len(), 2);

                // First argument should be the base selector
                if let Expr::Selector(selector) = &args[0].expr {
                    match base_selector {
                        "h" => assert_eq!(*selector, Selector::Heading(None)),
                        "h1" => assert_eq!(*selector, Selector::Heading(Some(1))),
                        "code" => assert_eq!(*selector, Selector::Code),
                        "text" => assert_eq!(*selector, Selector::Text),
                        _ => panic!("Unexpected base selector: {}", base_selector),
                    }
                } else {
                    panic!("Expected Selector expression in first argument, got {:?}", args[0].expr);
                }

                // Second argument should be the attribute string
                if let Expr::Literal(Literal::String(attr_str)) = &args[1].expr {
                    assert_eq!(attr_str, attribute);
                } else {
                    panic!("Expected String literal in second argument, got {:?}", args[1].expr);
                }
            } else {
                panic!("Expected Call expression, got {:?}", program[0].expr);
            }
        }
        Err(err) => panic!("Parse error: {:?}", err),
    }
}

#[rstest]
// Two selectors: base, then a single descendant hop.
#[case::two_levels(vec![".blockquote", ".code"], vec![Selector::Blockquote, Selector::Code])]
// Three selectors: two descendant hops.
#[case::three_levels(
        vec![".blockquote", ".list", ".code"],
        vec![Selector::Blockquote, Selector::List(None, None), Selector::Code]
    )]
fn test_parse_selector_descendant_chain(#[case] selectors: Vec<&str>, #[case] expected_selectors: Vec<Selector>) {
    let mut arena = Arena::new(10);
    let mut tokens = selectors
        .iter()
        .map(|selector| Token {
            range: Range::default(),
            kind: TokenKind::Selector(SmolStr::new(selector)),
            module_id: 1.into(),
        })
        .collect::<Vec<_>>();
    tokens.push(Token {
        range: Range::default(),
        kind: TokenKind::Eof,
        module_id: 1.into(),
    });

    let result = Parser::new(tokens.iter(), &mut arena, Module::TOP_LEVEL_MODULE_ID).parse();

    match result {
        Ok(program) => {
            assert_eq!(program.len(), 1);
            if let Expr::Block(nodes) = &program[0].expr {
                // Desugars to `base | .. | step1 | .. | step2 | ...`, i.e. a plain
                // selector interleaved with a `Recursive` selector between each hop.
                let expected: Vec<Selector> = expected_selectors
                    .into_iter()
                    .enumerate()
                    .flat_map(|(i, sel)| {
                        if i == 0 {
                            vec![sel]
                        } else {
                            vec![Selector::Recursive, sel]
                        }
                    })
                    .collect();
                assert_eq!(nodes.len(), expected.len());
                for (node, expected_sel) in nodes.iter().zip(expected.iter()) {
                    if let Expr::Selector(sel) = &node.expr {
                        assert_eq!(sel, expected_sel);
                    } else {
                        panic!("Expected Selector expression, got {:?}", node.expr);
                    }
                }
            } else {
                panic!("Expected Block expression, got {:?}", program[0].expr);
            }
        }
        Err(err) => panic!("Parse error: {:?}", err),
    }
}

#[test]
fn test_parse_selector_descendant_chain_trailing_attribute() {
    // `.blockquote .code.lang` → attr(Block([.blockquote, .., .code]), "lang")
    let mut arena = Arena::new(10);
    let selectors = [".blockquote", ".code", ".lang"];
    let mut tokens = selectors
        .iter()
        .map(|selector| Token {
            range: Range::default(),
            kind: TokenKind::Selector(SmolStr::new(*selector)),
            module_id: 1.into(),
        })
        .collect::<Vec<_>>();
    tokens.push(Token {
        range: Range::default(),
        kind: TokenKind::Eof,
        module_id: 1.into(),
    });

    let result = Parser::new(tokens.iter(), &mut arena, Module::TOP_LEVEL_MODULE_ID).parse();

    match result {
        Ok(program) => {
            assert_eq!(program.len(), 1);
            if let Expr::Call(ident, args) = &program[0].expr {
                assert_eq!(ident.name, "attr".into());
                assert_eq!(args.len(), 2);
                assert!(matches!(&args[0].expr, Expr::Block(nodes) if nodes.len() == 3));
                assert!(matches!(&args[1].expr, Expr::Literal(Literal::String(s)) if s == "lang"));
            } else {
                panic!("Expected Call expression, got {:?}", program[0].expr);
            }
        }
        Err(err) => panic!("Parse error: {:?}", err),
    }
}

#[rstest]
#[case::match_simple_literal(
        vec![
            token(TokenKind::Match),
            token(TokenKind::LParen),
            token(TokenKind::Ident(SmolStr::new("x"))),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::Pipe),
            token(TokenKind::NumberLiteral(1.into())),
            token(TokenKind::Colon),
            token(TokenKind::StringLiteral("one".to_owned())),
            token(TokenKind::Pipe),
            token(TokenKind::NumberLiteral(2.into())),
            token(TokenKind::Colon),
            token(TokenKind::StringLiteral("two".to_owned())),
            token(TokenKind::Pipe),
            token(TokenKind::Ident(SmolStr::new("_"))),
            token(TokenKind::Colon),
            token(TokenKind::StringLiteral("other".to_owned())),
            token(TokenKind::End),
            token(TokenKind::Eof)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Expr::Match(
                    Shared::new(Node {
                        token_id: 1.into(),
                        expr: Expr::Ident(IdentWithToken::new_with_token("x", Some(1.into())))
                    }),
                    vec![
                        MatchArm {
                            pattern: Pattern::Literal(Literal::Number(1.into())),
                            guard: None,
                            body: Shared::new(Node {
                                token_id: 5.into(),
                                expr: Expr::Literal(Literal::String("one".to_owned()))
                            })
                        },
                        MatchArm {
                            pattern: Pattern::Literal(Literal::Number(2.into())),
                            guard: None,
                            body: Shared::new(Node {
                                token_id: 8.into(),
                                expr: Expr::Literal(Literal::String("two".to_owned()))
                            })
                        },
                        MatchArm {
                            pattern: Pattern::Wildcard,
                            guard: None,
                            body: Shared::new(Node {
                                token_id: 11.into(),
                                expr: Expr::Literal(Literal::String("other".to_owned()))
                            })
                        }
                    ]
                )
            })
        ]))]
#[case::match_type_pattern(
        vec![
            token(TokenKind::Match),
            token(TokenKind::LParen),
            token(TokenKind::Ident(SmolStr::new("value"))),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::Pipe),
            token(TokenKind::Colon),
            token(TokenKind::Ident(SmolStr::new("string"))),
            token(TokenKind::Colon),
            token(TokenKind::StringLiteral("is string".to_owned())),
            token(TokenKind::Pipe),
            token(TokenKind::Colon),
            token(TokenKind::Ident(SmolStr::new("number"))),
            token(TokenKind::Colon),
            token(TokenKind::StringLiteral("is number".to_owned())),
            token(TokenKind::End),
            token(TokenKind::Eof)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Expr::Match(
                    Shared::new(Node {
                        token_id: 1.into(),
                        expr: Expr::Ident(IdentWithToken::new_with_token("value", Some(1.into())))
                    }),
                    vec![
                        MatchArm {
                            pattern: Pattern::Type(Ident::new("string")),
                            guard: None,
                            body: Shared::new(Node {
                                token_id: 5.into(),
                                expr: Expr::Literal(Literal::String("is string".to_owned()))
                            })
                        },
                        MatchArm {
                            pattern: Pattern::Type(Ident::new("number")),
                            guard: None,
                            body: Shared::new(Node {
                                token_id: 8.into(),
                                expr: Expr::Literal(Literal::String("is number".to_owned()))
                            })
                        }
                    ]
                )
            })
        ]))]
#[case::match_array_pattern(
        vec![
            token(TokenKind::Match),
            token(TokenKind::LParen),
            token(TokenKind::Ident(SmolStr::new("arr"))),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::Pipe),
            token(TokenKind::LBracket),
            token(TokenKind::Ident(SmolStr::new("x"))),
            token(TokenKind::Comma),
            token(TokenKind::Ident(SmolStr::new("y"))),
            token(TokenKind::RBracket),
            token(TokenKind::Colon),
            token(TokenKind::StringLiteral("two elements".to_owned())),
            token(TokenKind::End),
            token(TokenKind::Eof)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Expr::Match(
                    Shared::new(Node {
                        token_id: 1.into(),
                        expr: Expr::Ident(IdentWithToken::new_with_token("arr", Some(1.into())))
                    }),
                    vec![
                        MatchArm {
                            pattern: Pattern::Array(vec![
                                Pattern::Ident(IdentWithToken::new("x")),
                                Pattern::Ident(IdentWithToken::new("y"))
                            ]),
                            guard: None,
                            body: Shared::new(Node {
                                token_id: 5.into(),
                                expr: Expr::Literal(Literal::String("two elements".to_owned()))
                            })
                        }
                    ]
                )
            })
        ]))]
#[case::match_array_rest_pattern(
        vec![
            token(TokenKind::Match),
            token(TokenKind::LParen),
            token(TokenKind::Ident(SmolStr::new("arr"))),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::Pipe),
            token(TokenKind::LBracket),
            token(TokenKind::Ident(SmolStr::new("first"))),
            token(TokenKind::Comma),
            token(TokenKind::DoubleDot),
            token(TokenKind::Ident(SmolStr::new("rest"))),
            token(TokenKind::RBracket),
            token(TokenKind::Colon),
            token(TokenKind::Ident(SmolStr::new("first"))),
            token(TokenKind::End),
            token(TokenKind::Eof)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Expr::Match(
                    Shared::new(Node {
                        token_id: 1.into(),
                        expr: Expr::Ident(IdentWithToken::new_with_token("arr", Some(1.into())))
                    }),
                    vec![
                        MatchArm {
                            pattern: Pattern::ArrayRest(
                                vec![Pattern::Ident(IdentWithToken::new("first"))],
                                IdentWithToken::new("rest")
                            ),
                            guard: None,
                            body: Shared::new(Node {
                                token_id: 5.into(),
                                expr: Expr::Ident(IdentWithToken::new_with_token("first", Some(5.into())))
                            })
                        }
                    ]
                )
            })
        ]))]
#[case::match_dict_pattern(
        vec![
            token(TokenKind::Match),
            token(TokenKind::LParen),
            token(TokenKind::Ident(SmolStr::new("obj"))),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::Pipe),
            token(TokenKind::LBrace),
            token(TokenKind::Ident(SmolStr::new("name"))),
            token(TokenKind::Comma),
            token(TokenKind::Ident(SmolStr::new("age"))),
            token(TokenKind::RBrace),
            token(TokenKind::Colon),
            token(TokenKind::Ident(SmolStr::new("name"))),
            token(TokenKind::End),
            token(TokenKind::Eof)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Expr::Match(
                    Shared::new(Node {
                        token_id: 1.into(),
                        expr: Expr::Ident(IdentWithToken::new_with_token("obj", Some(1.into())))
                    }),
                    vec![
                        MatchArm {
                            pattern: Pattern::Dict(vec![
                                (IdentWithToken::new("name"), Pattern::Ident(IdentWithToken::new("name"))),
                                (IdentWithToken::new("age"), Pattern::Ident(IdentWithToken::new("age")))
                            ]),
                            guard: None,
                            body: Shared::new(Node {
                                token_id: 5.into(),
                                expr: Expr::Ident(IdentWithToken::new_with_token("name", Some(5.into())))
                            })
                        }
                    ]
                )
            })
        ]))]
#[case::match_with_guard(
        vec![
            token(TokenKind::Match),
            token(TokenKind::LParen),
            token(TokenKind::Ident(SmolStr::new("n"))),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::Pipe),
            token(TokenKind::Ident(SmolStr::new("x"))),
            token(TokenKind::If),
            token(TokenKind::LParen),
            token(TokenKind::Ident(SmolStr::new("x"))),
            token(TokenKind::Gt),
            token(TokenKind::NumberLiteral(0.into())),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::StringLiteral("positive".to_owned())),
            token(TokenKind::Pipe),
            token(TokenKind::Ident(SmolStr::new("x"))),
            token(TokenKind::If),
            token(TokenKind::LParen),
            token(TokenKind::Ident(SmolStr::new("x"))),
            token(TokenKind::Lt),
            token(TokenKind::NumberLiteral(0.into())),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::StringLiteral("negative".to_owned())),
            token(TokenKind::Pipe),
            token(TokenKind::Ident(SmolStr::new("_"))),
            token(TokenKind::Colon),
            token(TokenKind::StringLiteral("zero".to_owned())),
            token(TokenKind::End),
            token(TokenKind::Eof)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Expr::Match(
                    Shared::new(Node {
                        token_id: 1.into(),
                        expr: Expr::Ident(IdentWithToken::new_with_token("n", Some(1.into())))
                    }),
                    vec![
                        MatchArm {
                            pattern: Pattern::Ident(IdentWithToken::new("x")),
                            guard: Some(Shared::new(Node {
                                token_id: 5.into(),
                                expr: Expr::BinaryOp(
    BinaryOp::Gt,
                                        Shared::new(Node {
                                            token_id: 4.into(),
                                            expr: Expr::Ident(IdentWithToken::new_with_token("x", Some(4.into())))
                                        }),
                                        Shared::new(Node {
                                            token_id: 6.into(),
                                            expr: Expr::Literal(Literal::Number(0.into()))
                                        })
                                    )
                            })),
                            body: Shared::new(Node {
                                token_id: 8.into(),
                                expr: Expr::Literal(Literal::String("positive".to_owned()))
                            })
                        },
                        MatchArm {
                            pattern: Pattern::Ident(IdentWithToken::new("x")),
                            guard: Some(Shared::new(Node {
                                token_id: 11.into(),
                                expr: Expr::BinaryOp(
    BinaryOp::Lt,
                                        Shared::new(Node {
                                            token_id: 10.into(),
                                            expr: Expr::Ident(IdentWithToken::new_with_token("x", Some(10.into())))
                                        }),
                                        Shared::new(Node {
                                            token_id: 12.into(),
                                            expr: Expr::Literal(Literal::Number(0.into()))
                                        })
                                    )
                            })),
                            body: Shared::new(Node {
                                token_id: 14.into(),
                                expr: Expr::Literal(Literal::String("negative".to_owned()))
                            })
                        },
                        MatchArm {
                            pattern: Pattern::Wildcard,
                            guard: None,
                            body: Shared::new(Node {
                                token_id: 17.into(),
                                expr: Expr::Literal(Literal::String("zero".to_owned()))
                            })
                        }
                    ]
                )
            })
        ]))]
#[case::match_do_end(
        vec![
            token(TokenKind::Match),
            token(TokenKind::LParen),
            token(TokenKind::NumberLiteral(2.into())),
            token(TokenKind::RParen),
            token(TokenKind::Do),
            token(TokenKind::Pipe),
            token(TokenKind::NumberLiteral(1.into())),
            token(TokenKind::Colon),
            token(TokenKind::StringLiteral("one".to_owned())),
            token(TokenKind::Pipe),
            token(TokenKind::NumberLiteral(2.into())),
            token(TokenKind::Colon),
            token(TokenKind::StringLiteral("two".to_owned())),
            token(TokenKind::Pipe),
            token(TokenKind::Ident(SmolStr::new("_"))),
            token(TokenKind::Colon),
            token(TokenKind::StringLiteral("other".to_owned())),
            token(TokenKind::End),
            token(TokenKind::Eof)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Expr::Match(
                    Shared::new(Node {
                        token_id: 1.into(),
                        expr: Expr::Literal(Literal::Number(2.into()))
                    }),
                    vec![
                        MatchArm {
                            pattern: Pattern::Literal(Literal::Number(1.into())),
                            guard: None,
                            body: Shared::new(Node {
                                token_id: 5.into(),
                                expr: Expr::Literal(Literal::String("one".to_owned()))
                            })
                        },
                        MatchArm {
                            pattern: Pattern::Literal(Literal::Number(2.into())),
                            guard: None,
                            body: Shared::new(Node {
                                token_id: 8.into(),
                                expr: Expr::Literal(Literal::String("two".to_owned()))
                            })
                        },
                        MatchArm {
                            pattern: Pattern::Wildcard,
                            guard: None,
                            body: Shared::new(Node {
                                token_id: 11.into(),
                                expr: Expr::Literal(Literal::String("other".to_owned()))
                            })
                        }
                    ]
                )
            })
        ]))]
// --- or-pattern parser tests ---
// Arena allocation order for match(x): COLON | pattern COLON body ...
//   id 0: Match, id 1: Ident("x"), id 2: Colon (consume_colon_or_do)
//   per arm: id N: Pipe, id N+1: Colon, id N+2: body literal
#[case::match_or_two_literal_numbers(
        vec![
            token(TokenKind::Match),
            token(TokenKind::LParen),
            token(TokenKind::Ident(SmolStr::new("x"))),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::Pipe),
            token(TokenKind::NumberLiteral(1.into())),
            token(TokenKind::Or),
            token(TokenKind::NumberLiteral(2.into())),
            token(TokenKind::Colon),
            token(TokenKind::StringLiteral("matched".to_owned())),
            token(TokenKind::Pipe),
            token(TokenKind::Ident(SmolStr::new("_"))),
            token(TokenKind::Colon),
            token(TokenKind::StringLiteral("other".to_owned())),
            token(TokenKind::End),
            token(TokenKind::Eof),
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Expr::Match(
                    Shared::new(Node {
                        token_id: 1.into(),
                        expr: Expr::Ident(IdentWithToken::new_with_token("x", Some(1.into())))
                    }),
                    vec![
                        MatchArm {
                            pattern: Pattern::Or(vec![
                                Pattern::Literal(Literal::Number(1.into())),
                                Pattern::Literal(Literal::Number(2.into())),
                            ]),
                            guard: None,
                            body: Shared::new(Node {
                                token_id: 5.into(),
                                expr: Expr::Literal(Literal::String("matched".to_owned()))
                            })
                        },
                        MatchArm {
                            pattern: Pattern::Wildcard,
                            guard: None,
                            body: Shared::new(Node {
                                token_id: 8.into(),
                                expr: Expr::Literal(Literal::String("other".to_owned()))
                            })
                        },
                    ]
                )
            })
        ]))]
#[case::match_or_string_literals(
        vec![
            token(TokenKind::Match),
            token(TokenKind::LParen),
            token(TokenKind::Ident(SmolStr::new("x"))),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::Pipe),
            token(TokenKind::StringLiteral("a".to_owned())),
            token(TokenKind::Or),
            token(TokenKind::StringLiteral("b".to_owned())),
            token(TokenKind::Colon),
            token(TokenKind::StringLiteral("matched".to_owned())),
            token(TokenKind::Pipe),
            token(TokenKind::Ident(SmolStr::new("_"))),
            token(TokenKind::Colon),
            token(TokenKind::StringLiteral("other".to_owned())),
            token(TokenKind::End),
            token(TokenKind::Eof),
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Expr::Match(
                    Shared::new(Node {
                        token_id: 1.into(),
                        expr: Expr::Ident(IdentWithToken::new_with_token("x", Some(1.into())))
                    }),
                    vec![
                        MatchArm {
                            pattern: Pattern::Or(vec![
                                Pattern::Literal(Literal::String("a".to_owned())),
                                Pattern::Literal(Literal::String("b".to_owned())),
                            ]),
                            guard: None,
                            body: Shared::new(Node {
                                token_id: 5.into(),
                                expr: Expr::Literal(Literal::String("matched".to_owned()))
                            })
                        },
                        MatchArm {
                            pattern: Pattern::Wildcard,
                            guard: None,
                            body: Shared::new(Node {
                                token_id: 8.into(),
                                expr: Expr::Literal(Literal::String("other".to_owned()))
                            })
                        },
                    ]
                )
            })
        ]))]
#[case::match_or_type_patterns(
        vec![
            token(TokenKind::Match),
            token(TokenKind::LParen),
            token(TokenKind::Ident(SmolStr::new("x"))),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::Pipe),
            token(TokenKind::Colon),
            token(TokenKind::Ident(SmolStr::new("string"))),
            token(TokenKind::Or),
            token(TokenKind::Colon),
            token(TokenKind::Ident(SmolStr::new("number"))),
            token(TokenKind::Colon),
            token(TokenKind::StringLiteral("str or num".to_owned())),
            token(TokenKind::End),
            token(TokenKind::Eof),
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Expr::Match(
                    Shared::new(Node {
                        token_id: 1.into(),
                        expr: Expr::Ident(IdentWithToken::new_with_token("x", Some(1.into())))
                    }),
                    vec![
                        MatchArm {
                            pattern: Pattern::Or(vec![
                                Pattern::Type(Ident::new("string")),
                                Pattern::Type(Ident::new("number")),
                            ]),
                            guard: None,
                            body: Shared::new(Node {
                                token_id: 5.into(),
                                expr: Expr::Literal(Literal::String("str or num".to_owned()))
                            })
                        },
                    ]
                )
            })
        ]))]
#[case::match_or_three_alternatives(
        vec![
            token(TokenKind::Match),
            token(TokenKind::LParen),
            token(TokenKind::Ident(SmolStr::new("x"))),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::Pipe),
            token(TokenKind::NumberLiteral(1.into())),
            token(TokenKind::Or),
            token(TokenKind::NumberLiteral(2.into())),
            token(TokenKind::Or),
            token(TokenKind::NumberLiteral(3.into())),
            token(TokenKind::Colon),
            token(TokenKind::StringLiteral("matched".to_owned())),
            token(TokenKind::Pipe),
            token(TokenKind::Ident(SmolStr::new("_"))),
            token(TokenKind::Colon),
            token(TokenKind::StringLiteral("other".to_owned())),
            token(TokenKind::End),
            token(TokenKind::Eof),
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Expr::Match(
                    Shared::new(Node {
                        token_id: 1.into(),
                        expr: Expr::Ident(IdentWithToken::new_with_token("x", Some(1.into())))
                    }),
                    vec![
                        MatchArm {
                            pattern: Pattern::Or(vec![
                                Pattern::Literal(Literal::Number(1.into())),
                                Pattern::Literal(Literal::Number(2.into())),
                                Pattern::Literal(Literal::Number(3.into())),
                            ]),
                            guard: None,
                            body: Shared::new(Node {
                                token_id: 5.into(),
                                expr: Expr::Literal(Literal::String("matched".to_owned()))
                            })
                        },
                        MatchArm {
                            pattern: Pattern::Wildcard,
                            guard: None,
                            body: Shared::new(Node {
                                token_id: 8.into(),
                                expr: Expr::Literal(Literal::String("other".to_owned()))
                            })
                        },
                    ]
                )
            })
        ]))]
#[case::match_or_bool_literals(
        vec![
            token(TokenKind::Match),
            token(TokenKind::LParen),
            token(TokenKind::Ident(SmolStr::new("x"))),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::Pipe),
            token(TokenKind::BoolLiteral(true)),
            token(TokenKind::Or),
            token(TokenKind::BoolLiteral(false)),
            token(TokenKind::Colon),
            token(TokenKind::StringLiteral("bool".to_owned())),
            token(TokenKind::End),
            token(TokenKind::Eof),
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Expr::Match(
                    Shared::new(Node {
                        token_id: 1.into(),
                        expr: Expr::Ident(IdentWithToken::new_with_token("x", Some(1.into())))
                    }),
                    vec![
                        MatchArm {
                            pattern: Pattern::Or(vec![
                                Pattern::Literal(Literal::Bool(true)),
                                Pattern::Literal(Literal::Bool(false)),
                            ]),
                            guard: None,
                            body: Shared::new(Node {
                                token_id: 5.into(),
                                expr: Expr::Literal(Literal::String("bool".to_owned()))
                            })
                        },
                    ]
                )
            })
        ]))]
fn test_parse_match(#[case] input: Vec<Token>, #[case] expected: Result<Program, SyntaxError>) {
    let mut arena = Arena::new(10);
    let result = Parser::new(input.iter(), &mut arena, Module::TOP_LEVEL_MODULE_ID).parse();
    assert_eq!(result, expected);
}

#[rstest]
#[case::expr_literal(
        vec![
            lexer::token::StringSegment::Text("Value: ".to_string(), Range::default()),
            lexer::token::StringSegment::Expr("42".into(), Range::default()),
        ],
        2,
        |segments: &[StringSegment]| {
            matches!(&segments[0], StringSegment::Text(s) if s == "Value: ") &&
            matches!(&segments[1], StringSegment::Expr(node) if matches!(&node.expr, Expr::Literal(Literal::Number(_))))
        }
    )]
#[case::expr_string(
        vec![
            lexer::token::StringSegment::Text("Result: ".to_string(), Range::default()),
            lexer::token::StringSegment::Expr("\"hello\"".into(), Range::default()),
        ],
        2,
        |segments: &[StringSegment]| {
            matches!(&segments[0], StringSegment::Text(s) if s == "Result: ") &&
            matches!(&segments[1], StringSegment::Expr(node) if matches!(&node.expr, Expr::Literal(Literal::String(s)) if s == "hello"))
        }
    )]
#[case::expr_call(
        vec![
            lexer::token::StringSegment::Text("Result: ".to_string(), Range::default()),
            lexer::token::StringSegment::Expr("add(1, 2)".into(), Range::default()),
        ],
        2,
        |segments: &[StringSegment]| {
            matches!(&segments[0], StringSegment::Text(s) if s == "Result: ") &&
            if let StringSegment::Expr(node) = &segments[1] {
                if let Expr::Call(ident, args) = &node.expr {
                    ident.name == "add".into() && args.len() == 2
                } else {
                    false
                }
            } else {
                false
            }
        }
    )]
#[case::expr_self(
        vec![
            lexer::token::StringSegment::Text("Value: ".to_string(), Range::default()),
            lexer::token::StringSegment::Expr("self".into(), Range::default()),
        ],
        2,
        |segments: &[StringSegment]| {
            matches!(&segments[0], StringSegment::Text(s) if s == "Value: ") &&
            matches!(&segments[1], StringSegment::Self_)
        }
    )]
#[case::expr_env(
        vec![
            lexer::token::StringSegment::Text("Env: ".to_string(), Range::default()),
            lexer::token::StringSegment::Expr("$MQ_TEST_INTERPOLATION".into(), Range::default()),
        ],
        2,
        |segments: &[StringSegment]| {
            unsafe { std::env::set_var("MQ_TEST_INTERPOLATION", "test_value") };
            matches!(&segments[0], StringSegment::Text(s) if s == "Env: ") &&
            matches!(&segments[1], StringSegment::Env(var) if var == "MQ_TEST_INTERPOLATION")
        }
    )]
#[case::multiple_exprs(
        vec![
            lexer::token::StringSegment::Text("A: ".to_string(), Range::default()),
            lexer::token::StringSegment::Expr("1".into(), Range::default()),
            lexer::token::StringSegment::Text(", B: ".to_string(), Range::default()),
            lexer::token::StringSegment::Expr("2".into(), Range::default()),
        ],
        4,
        |segments: &[StringSegment]| {
            matches!(&segments[0], StringSegment::Text(s) if s == "A: ") &&
            matches!(&segments[1], StringSegment::Expr(_)) &&
            matches!(&segments[2], StringSegment::Text(s) if s == ", B: ") &&
            matches!(&segments[3], StringSegment::Expr(_))
        }
    )]
#[case::mixed_segments(
        vec![
            lexer::token::StringSegment::Text("Literal, ".to_string(), Range::default()),
            lexer::token::StringSegment::Expr("self".into(), Range::default()),
            lexer::token::StringSegment::Text(", ".to_string(), Range::default()),
            lexer::token::StringSegment::Expr("$MQ_MIXED_TEST".into(), Range::default()),
            lexer::token::StringSegment::Text(", ".to_string(), Range::default()),
            lexer::token::StringSegment::Expr("42".into(), Range::default()),
        ],
        6,
        |segments: &[StringSegment]| {
            unsafe { std::env::set_var("MQ_MIXED_TEST", "env_value") };
            matches!(&segments[0], StringSegment::Text(s) if s == "Literal, ") &&
            matches!(&segments[1], StringSegment::Self_) &&
            matches!(&segments[2], StringSegment::Text(s) if s == ", ") &&
            matches!(&segments[3], StringSegment::Env(var) if var == "MQ_MIXED_TEST") &&
            matches!(&segments[4], StringSegment::Text(s) if s == ", ") &&
            matches!(&segments[5], StringSegment::Expr(_))
        }
    )]
#[case::expr_bool(
        vec![
            lexer::token::StringSegment::Text("Bool: ".to_string(), Range::default()),
            lexer::token::StringSegment::Expr("true".into(), Range::default()),
        ],
        2,
        |segments: &[StringSegment]| {
            matches!(&segments[0], StringSegment::Text(s) if s == "Bool: ") &&
            matches!(&segments[1], StringSegment::Expr(node) if matches!(&node.expr, Expr::Literal(Literal::Bool(true))))
        }
    )]
fn test_parse_interpolated_string_with_expr(
    #[case] input_segments: Vec<lexer::token::StringSegment>,
    #[case] expected_len: usize,
    #[case] validator: fn(&[StringSegment]) -> bool,
) {
    let mut arena = Arena::new(10);
    let tokens = [
        Token {
            range: Range::default(),
            kind: TokenKind::InterpolatedString(input_segments),
            module_id: 1.into(),
        },
        Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: 1.into(),
        },
    ];

    let result = Parser::new(tokens.iter(), &mut arena, Module::TOP_LEVEL_MODULE_ID).parse();

    match result {
        Ok(program) => {
            assert_eq!(program.len(), 1);
            if let Expr::InterpolatedString(segments) = &program[0].expr {
                assert_eq!(segments.len(), expected_len);
                assert!(validator(segments), "Validator failed for segments");
            } else {
                panic!("Expected InterpolatedString, got {:?}", program[0].expr);
            }
        }
        Err(err) => panic!("Parse error: {:?}", err),
    }
}

#[test]
fn test_interpolated_string_inner_syntax_error_is_propagated() {
    let result = parse_source("let a = 1\n| s\"a ${ 1 + } b\"");
    match result {
        Err(SyntaxError::UnexpectedEOFAfterToken(token)) => {
            assert_eq!(token.kind, TokenKind::Plus);
            assert_eq!(token.range.start, Position { line: 2, column: 12 });
        }
        other => panic!("expected the inner UnexpectedEOFAfterToken, got {:?}", other),
    }
}

/// For each argument of the top-level call, the statement count of its body if it is a lambda.
fn lambda_arg_body_lens(source: &str) -> Vec<Option<usize>> {
    let program = parse_source(source).expect("source should parse");
    let [node] = program.as_slice() else {
        panic!("expected a single top-level expression");
    };
    let Expr::Call(_, args) = &node.expr else {
        panic!("expected a call, got {:?}", node.expr);
    };
    args.iter()
        .map(|arg| match &arg.expr {
            Expr::Fn(_, body) => Some(body.len()),
            _ => None,
        })
        .collect()
}

#[rstest]
#[case::only_arg("f(fn(x): x + 1)", vec![Some(1)])]
#[case::arrow("f(->(x): x + 1)", vec![Some(1)])]
#[case::last_arg("f(a, fn(x): x + 1)", vec![None, Some(1)])]
#[case::multiple_params("f(a, 0, fn(acc, x): acc + x)", vec![None, None, Some(1)])]
#[case::ends_at_comma("f(fn(x): x, 1)", vec![Some(1), None])]
#[case::lambda_between_args("f(a, fn(x): x, b)", vec![None, Some(1), None])]
#[case::two_lambdas("f(fn(x): x + 1, fn(x): x * 2)", vec![Some(1), Some(1)])]
#[case::pipe_stays_in_body("f(fn(x): x | g(.h))", vec![Some(2)])]
#[case::pipe_before_comma("f(fn(x): x | g(.h), 1)", vec![Some(2), None])]
#[case::call_with_commas_in_body("f(fn(x): g(x, 1, 2), 3)", vec![Some(1), None])]
#[case::array_in_body("f(fn(x): [x, 1], 2)", vec![Some(1), None])]
#[case::dict_in_body("f(fn(x): {\"a\": x, \"b\": 1}, 2)", vec![Some(1), None])]
#[case::paren_in_body("f(fn(x): (x + 1) * 2, 3)", vec![Some(1), None])]
#[case::if_else_in_body("f(fn(x): if (x): 1 else: 2, 3)", vec![Some(1), None])]
#[case::elif_in_body("f(fn(x): if (x): 1 elif (y): 2 else: 3)", vec![Some(1)])]
#[case::do_end_in_body("f(fn(x): do 1 | 2 end, 3)", vec![Some(1), None])]
#[case::foreach_in_body("f(fn(x): foreach (i, x): i end, 3)", vec![Some(1), None])]
#[case::match_in_body("f(fn(x): match (x): | 1: 2 | _: 3 end, 4)", vec![Some(1), None])]
#[case::nested_call_lambda("f(fn(x): g(x, fn(y): y + 1))", vec![Some(1)])]
#[case::nested_call_lambda_then_comma("f(fn(x): g(x, fn(y): y + 1), 2)", vec![Some(1), None])]
#[case::let_in_body("f(fn(x): let y = x + 1 | y, 2)", vec![Some(2), None])]
#[case::lambda_in_param_default("f(fn(x = g(fn(y): y)): x, 1)", vec![Some(1), None])]
#[case::semicolon_form("f(fn(x): x + 1;)", vec![Some(1)])]
#[case::semicolon_form_then_arg("f(fn(x): x; , 1)", vec![Some(1), None])]
#[case::end_form("f(fn(x): do x end; , 1)", vec![Some(1), None])]
#[case::inner_end_then_semicolon("f(fn(x): foreach (i, x): i end;)", vec![Some(1)])]
fn test_lambda_in_call_args_may_omit_terminator(#[case] source: &str, #[case] expected: Vec<Option<usize>>) {
    assert_eq!(lambda_arg_body_lens(source), expected);
}

// Body lengths exclude the leading `fn:` argument statement.
#[rstest]
#[case::only_arg("f(fn: self + 1)", vec![Some(1)])]
#[case::last_arg("f(a, fn: self + 1)", vec![None, Some(1)])]
#[case::ends_at_comma("f(fn: self, 1)", vec![Some(1), None])]
#[case::pipe_stays_in_body("f(fn: g(1) | h(2))", vec![Some(2)])]
#[case::pipe_before_comma("f(fn: g | h, 1)", vec![Some(2), None])]
#[case::call_with_commas_in_body("f(fn: g(1, 2), 3)", vec![Some(1), None])]
#[case::nested("f(fn: g(fn: self + 1))", vec![Some(1)])]
#[case::nested_then_comma("f(fn: g(fn: self + 1), 2)", vec![Some(1), None])]
#[case::if_else_in_body("f(fn: if (self): 1 else: 2, 3)", vec![Some(1), None])]
#[case::semicolon_form("f(fn: self + 1;)", vec![Some(1)])]
#[case::end_form("f(fn: do self end;, 1)", vec![Some(1), None])]
fn test_lambda_without_params(#[case] source: &str, #[case] expected: Vec<Option<usize>>) {
    let expected: Vec<_> = expected.into_iter().map(|len| len.map(|len| len + 1)).collect();
    assert_eq!(lambda_arg_body_lens(source), expected);
}

#[test]
fn test_lambda_without_params_takes_one_argument() {
    let program = parse_source("f(fn: self, fn(): 1)").expect("source should parse");
    let Expr::Call(_, args) = &program[0].expr else {
        panic!("expected a call");
    };
    let Expr::Fn(params, body) = &args[0].expr else {
        panic!("expected a function");
    };
    let [param] = params.as_slice() else {
        panic!("expected one parameter");
    };
    assert!(matches!(&body[0].expr, Expr::Ident(ident) if ident.name == param.ident.name));
    assert!(matches!(&args[1].expr, Expr::Fn(params, _) if params.is_empty()));
}

#[test]
fn test_lambda_without_params_outside_call_args_with_terminator() {
    let program = parse_source("let g = fn: self + 1;").expect("source should parse");
    assert!(matches!(&program[0].expr, Expr::Let(..)));
}

#[rstest]
#[case::let_value_without_terminator("let g = fn: self + 1)")]
#[case::empty_body("f(fn: )")]
#[case::empty_body_then_comma("f(fn:, 1)")]
#[case::unclosed_call("f(fn: self")]
#[case::arrow_has_no_paramless_form("f(->: self)")]
#[case::missing_colon("f(fn self)")]
fn test_lambda_without_params_reports_malformed_input(#[case] source: &str) {
    assert!(parse_source(source).is_err(), "{source} should not parse");
}

#[rstest]
#[case::let_value("let g = fn(x): x + 1)")]
#[case::empty_body("f(fn(x): )")]
#[case::empty_body_then_comma("f(fn(x):, 1)")]
#[case::dangling_operator("f(fn(x): x +)")]
#[case::unclosed_call("f(fn(x): x + 1")]
#[case::stray_close_paren("f(fn(x): x + 1))")]
fn test_lambda_reports_malformed_input(#[case] source: &str) {
    assert!(parse_source(source).is_err(), "{source} should not parse");
}

#[rstest]
#[case::paren_group("(fn(x): x + 1)(2)")]
#[case::array_element("[fn(x): x, 1]")]
#[case::implicit_array_element("[fn: self, 1]")]
#[case::dict_value("{\"a\": fn(x): x, \"b\": 1}")]
#[case::lambda_body_tail("f(fn(x): fn(y): x + y)")]
#[case::after_pipe("f(1 | fn(x): x + 1, 2)")]
fn test_lambda_ends_at_the_closer_of_its_enclosing_construct(#[case] source: &str) {
    parse_source(source).expect("source should parse");
}

#[test]
fn test_lambda_array_element_ends_before_comma() {
    let program = parse_source("[fn(x): x + 1, 2]").expect("source should parse");
    let Expr::Array(elements) = &program[0].expr else {
        panic!("expected an array");
    };
    assert_eq!(elements.len(), 2);
    assert!(matches!(&elements[0].expr, Expr::Fn(..)));
}

#[rstest]
#[case::empty_body("f(fn(x): )", TokenKind::RParen)]
#[case::empty_body_then_comma("f(fn(x):, 1)", TokenKind::Comma)]
fn test_lambda_in_call_args_with_empty_body_reports_the_delimiter(#[case] source: &str, #[case] kind: TokenKind) {
    match parse_source(source) {
        Err(SyntaxError::UnexpectedToken(token)) => assert_eq!(token.kind, kind),
        other => panic!("expected UnexpectedToken, got {other:?}"),
    }
}

#[test]
fn test_unclosed_call_after_unterminated_lambda_reports_missing_paren() {
    assert!(matches!(
        parse_source("f(fn(x): x + 1"),
        Err(SyntaxError::ExpectedClosingParen(..))
    ));
}

const STACK_KIB: usize = 2048;

fn parse_source(source: &str) -> Result<Program, SyntaxError> {
    let tokens = Lexer::new(lexer::Options::default())
        .tokenize(source, Module::TOP_LEVEL_MODULE_ID)
        .expect("source should tokenize");
    let mut arena = Arena::new(16);
    Parser::new(tokens.iter(), &mut arena, Module::TOP_LEVEL_MODULE_ID).parse()
}

/// Runs `f` on a 2 MiB stack (the default for spawned and tokio worker threads) in release
/// builds. Debug frames are much larger, so debug builds get more room.
fn on_small_stack<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    let stack_size = if cfg!(debug_assertions) { 64 * 1024 } else { STACK_KIB } * 1024;
    std::thread::Builder::new()
        .stack_size(stack_size)
        .spawn(f)
        .unwrap()
        .join()
        .expect("parser must not overflow the stack")
}

#[rstest]
#[case::paren("(", "1", ")")]
#[case::array("[", "1", "]")]
#[case::dict("{\"a\": ", "1", "}")]
#[case::not("!", "true", "")]
#[case::block("do ", "1", " end")]
#[case::if_else("if (true): ", "1", " else: 2")]
#[case::fn_body("fn(): ", "1", "; ")]
#[case::call("f(", "1", ")")]
fn test_nesting_within_limit_is_accepted(#[case] open: &str, #[case] core: &str, #[case] close: &str) {
    let source = format!(
        "{}{}{}",
        open.repeat(MAX_PARSE_DEPTH - 1),
        core,
        close.repeat(MAX_PARSE_DEPTH - 1)
    );
    let result = on_small_stack(move || parse_source(&source).map(|_| ()));
    assert!(
        !matches!(result, Err(SyntaxError::TooDeeplyNested(..))),
        "nesting of {} should be accepted, got {:?}",
        MAX_PARSE_DEPTH - 1,
        result
    );
}

#[rstest]
#[case::paren("(", "1", ")")]
#[case::array("[", "1", "]")]
#[case::dict("{\"a\": ", "1", "}")]
#[case::not("!", "true", "")]
#[case::block("do ", "1", " end")]
#[case::if_else("if (true): ", "1", " else: 2")]
#[case::fn_body("fn(): ", "1", "; ")]
#[case::call("f(", "1", ")")]
fn test_nesting_beyond_limit_returns_error(#[case] open: &str, #[case] core: &str, #[case] close: &str) {
    let n = MAX_PARSE_DEPTH * 2;
    let source = format!("{}{}{}", open.repeat(n), core, close.repeat(n));
    let result = on_small_stack(move || parse_source(&source).map(|_| ()));
    assert!(
        matches!(result, Err(SyntaxError::TooDeeplyNested(_, MAX_PARSE_DEPTH))),
        "expected TooDeeplyNested, got {:?}",
        result
    );
}

#[rstest]
#[case::let_array("let ", "[", "x", "]", " = 1")]
#[case::let_dict("let ", "{a: ", "x", "}", " = 1")]
#[case::match_array("match (1): | ", "[", "x", "]", ": 1 end")]
fn test_pattern_nesting_beyond_limit_returns_error(
    #[case] prefix: &str,
    #[case] open: &str,
    #[case] core: &str,
    #[case] close: &str,
    #[case] suffix: &str,
) {
    let n = MAX_PARSE_DEPTH * 2;
    let source = format!("{prefix}{}{core}{}{suffix}", open.repeat(n), close.repeat(n));
    let result = on_small_stack(move || parse_source(&source).map(|_| ()));
    assert!(
        matches!(result, Err(SyntaxError::TooDeeplyNested(_, MAX_PARSE_DEPTH))),
        "expected TooDeeplyNested, got {:?}",
        result
    );
}

#[test]
fn test_long_index_chain_returns_error() {
    let source = format!("a{}", "[0]".repeat(MAX_PARSE_DEPTH * 2));
    let result = on_small_stack(move || parse_source(&source).map(|_| ()));
    assert!(
        matches!(result, Err(SyntaxError::TooDeeplyNested(_, MAX_PARSE_DEPTH))),
        "expected TooDeeplyNested, got {:?}",
        result
    );
}

#[test]
fn test_interpolated_expr_nesting_reports_nesting_error() {
    let n = MAX_PARSE_DEPTH * 2;
    let source = format!("s\"${{{}1{}}}\"", "(".repeat(n), ")".repeat(n));
    let prefix_len = "s\"".len();
    let result = on_small_stack(move || parse_source(&source).map(|_| ()));
    match result {
        Err(SyntaxError::TooDeeplyNested(token, _)) => {
            assert!(
                token.range.start.column >= prefix_len,
                "range should point into the original source, got {:?}",
                token.range
            );
        }
        other => panic!("expected TooDeeplyNested, got {:?}", other),
    }
}

#[test]
fn test_nesting_limit_boundary() {
    let at_limit = format!(
        "{}1{}",
        "(".repeat(MAX_PARSE_DEPTH - 1),
        ")".repeat(MAX_PARSE_DEPTH - 1)
    );
    let over_limit = format!("{}1{}", "(".repeat(MAX_PARSE_DEPTH), ")".repeat(MAX_PARSE_DEPTH));
    let (at_limit_ok, over_limit_err) = on_small_stack(move || {
        (
            parse_source(&at_limit).is_ok(),
            matches!(
                parse_source(&over_limit),
                Err(SyntaxError::TooDeeplyNested(_, MAX_PARSE_DEPTH))
            ),
        )
    });
    assert!(at_limit_ok);
    assert!(over_limit_err);
}

#[test]
fn test_depth_is_restored_after_each_expression() {
    let nested = format!(
        "{}1{}",
        "(".repeat(MAX_PARSE_DEPTH - 1),
        ")".repeat(MAX_PARSE_DEPTH - 1)
    );
    let source = format!("{nested} | {nested} | {nested}");
    assert!(on_small_stack(move || parse_source(&source).is_ok()));
}

/// Renders `node` fully parenthesized with the source symbol of each operator.
fn shape(node: &Node, arena: &Arena<Token>) -> String {
    fn compound_rhs(node: &Node) -> &Node {
        match &node.expr {
            Expr::BinaryOp(_, _, rhs) => rhs,
            Expr::Call(_, args) if args.len() == 2 => &args[1],
            Expr::Call(_, args) if args.len() == 1 => compound_rhs(&args[0]),
            _ => node,
        }
    }

    let operator = arena[node.token_id].to_string();
    let fold = |operands: &[Shared<Node>], symbol: &str| {
        operands
            .iter()
            .map(|operand| shape(operand, arena))
            .reduce(|lhs, rhs| format!("({lhs} {symbol} {rhs})"))
            .unwrap()
    };

    match &node.expr {
        Expr::Ident(ident) => ident.name.to_string(),
        Expr::Literal(literal) => literal.to_string(),
        Expr::Paren(inner) => format!("<{}>", shape(inner, arena)),
        Expr::UnaryOp(op, operand) => format!("({op} {})", shape(operand, arena)),
        Expr::BinaryOp(_, lhs, rhs) => format!("({} {operator} {})", shape(lhs, arena), shape(rhs, arena)),
        Expr::Call(_, args) if args.len() == 2 && arena[node.token_id].kind.binary_op_precedence().is_some() => {
            format!("({} {operator} {})", shape(&args[0], arena), shape(&args[1], arena))
        }
        Expr::And(operands) => fold(operands, "&&"),
        Expr::Or(operands) => fold(operands, "||"),
        Expr::Assign(ident, rhs) if operator == "=" => format!("({} = {})", ident.name, shape(rhs, arena)),
        Expr::Assign(ident, rhs) => format!("({} {operator} {})", ident.name, shape(compound_rhs(rhs), arena)),
        Expr::As(ident, expr) => format!("({} as {})", shape(expr, arena), ident.name),
        other => panic!("shape does not support {other:?}"),
    }
}

fn parse_with_arena(code: &str) -> (Program, Arena<Token>) {
    let tokens = Lexer::new(lexer::Options::default())
        .tokenize(code, Module::TOP_LEVEL_MODULE_ID)
        .expect("code should tokenize");
    let mut arena = Arena::new(16);
    let program = Parser::new(tokens.iter(), &mut arena, Module::TOP_LEVEL_MODULE_ID)
        .parse()
        .expect("code should parse");
    (program, arena)
}

#[test]
fn test_ident_token_resolves_to_its_lexer_token() {
    let (program, arena) = parse_with_arena("def foo(a): a;");
    let Expr::Def(name, params, body) = &program[0].expr else {
        panic!("expected def");
    };

    let name_token = &arena[name.token.expect("def name keeps its token")];
    assert_eq!(name_token.kind, TokenKind::Ident("foo".into()));
    assert_eq!(name_token.range.start.column, 5);

    let param_token = &arena[params[0].ident.token.expect("param keeps its token")];
    assert_eq!(param_token.kind, TokenKind::Ident("a".into()));
    assert_eq!(param_token.range.start.column, 9);

    let Expr::Ident(ident) = &body[0].expr else {
        panic!("expected ident");
    };
    assert_eq!(ident.token, Some(body[0].token_id), "an ident shares its node's entry");
}

#[test]
fn test_nodes_built_from_one_lexer_token_have_distinct_ids() {
    let (program, _) = parse_with_arena("arr[0](1)");
    let Expr::CallDynamic(callee, _) = &program[0].expr else {
        panic!("expected dynamic call");
    };
    assert_ne!(program[0].token_id, callee.token_id);
}

#[rstest]
#[case::self_("self.url")]
#[case::ident("v.url")]
#[case::call("f().url")]
fn test_attr_call_and_its_literal_have_distinct_ids(#[case] code: &str) {
    let (program, arena) = parse_with_arena(code);
    let Expr::Call(attr, args) = &program[0].expr else {
        panic!("expected attr call");
    };
    assert_eq!(attr.name, Ident::new(constants::builtins::ATTR));
    assert_ne!(program[0].token_id, args[1].token_id);
    assert_eq!(
        attr.token,
        Some(args[1].token_id),
        "attr resolves to the selector token"
    );
    assert_eq!(arena[args[1].token_id].kind, TokenKind::Selector(".url".into()));
}

fn parse_shape(code: &str) -> Result<String, SyntaxError> {
    let tokens = Lexer::new(lexer::Options::default())
        .tokenize(code, Module::TOP_LEVEL_MODULE_ID)
        .expect("code should tokenize");
    let mut arena = Arena::new(16);
    let program = Parser::new(tokens.iter(), &mut arena, Module::TOP_LEVEL_MODULE_ID).parse()?;
    assert_eq!(program.len(), 1, "expected one expression for {code:?}");
    Ok(shape(&program[0], &arena))
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
    assert_eq!(parse_shape(code), Ok(expected.to_string()));
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
#[case::coalesce_under_multiplicative("a * b ?? c", "((a * b) ?? c)")]
#[case::multiplicative_under_coalesce("a ?? b * c", "(a ?? (b * c))")]
#[case::coalesce_under_additive("a ?? b + c", "(a ?? (b + c))")]
#[case::coalesce_under_comparison("a ?? b == c", "(a ?? (b == c))")]
#[case::coalesce_over_or("a || b ?? c", "(a || (b ?? c))")]
#[case::coalesce_under_and("a && b ?? c", "((a && b) ?? c)")]
#[case::convert_over_multiplicative("a * b @ c", "(a * (b @ c))")]
#[case::multiplicative_under_convert("a @ b * c", "((a @ b) * c)")]
#[case::convert_over_additive("a + b @ c", "(a + (b @ c))")]
#[case::range_over_convert("a @ b .. c", "(a @ (b .. c))")]
#[case::convert_after_range("a .. b @ c", "((a .. b) @ c)")]
#[case::shift_with_additive("a << b + c", "((a << b) + c)")]
#[case::additive_with_shift("a + b >> c", "((a + b) >> c)")]
#[case::regex_match_with_comparison("a =~ b == c", "((a =~ b) == c)")]
#[case::regex_over_and("a !~ b && c", "((a !~ b) && c)")]
#[case::three_levels("a || b && c == d + e * f .. g", "(a || (b && (c == (d + (e * (f .. g))))))")]
#[case::coalesce_between_or_and_and("a || b ?? c && d", "(a || (b ?? (c && d)))")]
#[case::three_levels_reversed("a .. b * c + d == e && f || g", "((((((a .. b) * c) + d) == e) && f) || g)")]
fn test_binary_op_precedence_shape(#[case] code: &str, #[case] expected: &str) {
    assert_eq!(parse_shape(code), Ok(expected.to_string()));
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
#[case::coalesce_then_range("a ?? b .. c", "(a ?? (b .. c))")]
#[case::convert("a @ b @ c", "((a @ b) @ c)")]
fn test_binary_op_associativity_shape(#[case] code: &str, #[case] expected: &str) {
    assert_eq!(parse_shape(code), Ok(expected.to_string()));
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
    assert_eq!(parse_shape(code), Ok(expected.to_string()));
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
    assert_eq!(parse_shape(code), Ok(expected.to_string()));
}

#[rstest]
#[case::chained_assignment("a = b = c")]
#[case::assign_to_expression("a + b = c")]
#[case::assign_to_logic("a || b = c")]
#[case::pipe_assign_outside_a_binding("a |= b")]
#[case::missing_right_operand("a +")]
#[case::missing_right_operand_after_assign("a =")]
#[case::missing_right_operand_after_logic("a &&")]
#[case::operator_only("+")]
#[case::as_without_name("a as")]
#[case::as_with_non_identifier("a as 1")]
fn test_binary_op_errors(#[case] code: &str) {
    assert!(parse_shape(code).is_err(), "expected an error for {code:?}");
}

#[test]
fn test_code_generation_precedence_matches_the_parser() {
    let kinds = [
        TokenKind::Plus,
        TokenKind::Minus,
        TokenKind::Asterisk,
        TokenKind::Slash,
        TokenKind::Percent,
        TokenKind::EqEq,
        TokenKind::NeEq,
        TokenKind::Lt,
        TokenKind::Lte,
        TokenKind::Gt,
        TokenKind::Gte,
    ];
    for x in &kinds {
        for y in &kinds {
            let (op_x, op_y) = (Parser::binary_op_kind(x).unwrap(), Parser::binary_op_kind(y).unwrap());
            assert_eq!(
                crate::ast::code::binary_op_precedence(op_x).cmp(&crate::ast::code::binary_op_precedence(op_y)),
                x.binary_op_precedence().cmp(&y.binary_op_precedence()),
                "{x} vs {y}"
            );
        }
    }
}

#[rstest]
#[case::after_literal("1 |=")]
#[case::after_literal_with_operand("1 |= b")]
#[case::after_paren("(a) |= b")]
#[case::after_string("\"s\" |= b")]
fn test_pipe_assign_is_not_an_infix_operator(#[case] code: &str) {
    assert!(
        matches!(parse_shape(code), Err(SyntaxError::UnexpectedToken(token)) if token.kind == TokenKind::PipeEqual),
        "expected `|=` to be rejected as an unexpected token for {code:?}"
    );
}

fn parse_program_source(code: &str) -> Result<Program, SyntaxError> {
    let tokens = Lexer::new(lexer::Options::default())
        .tokenize(code, Module::TOP_LEVEL_MODULE_ID)
        .expect("code should tokenize");
    let mut arena = Arena::new(16);
    Parser::new(tokens.iter(), &mut arena, Module::TOP_LEVEL_MODULE_ID).parse()
}

#[rstest]
#[case::literal("1 7")]
#[case::array("[1] 7")]
#[case::dict("{\"a\": 1} 7")]
#[case::paren("(1) 7")]
#[case::index("x[0] 7")]
#[case::self_("self 7")]
#[case::qualified("a::b 7")]
#[case::as_binding("x as y 7")]
#[case::selector_chain(".blockquote .list code")]
#[case::recursive_name("..h code")]
#[case::in_body("def f(): [1] 7;")]
fn test_adjacent_statements_without_pipe_are_rejected(#[case] code: &str) {
    assert!(
        matches!(parse_program_source(code), Err(SyntaxError::UnexpectedToken(_))),
        "expected an unexpected token error for {code:?}"
    );
}

#[rstest]
#[case::piped("1 | 7", 2)]
#[case::piped_array("[1] | 7", 2)]
#[case::after_def("def f(): 1; f()", 2)]
#[case::after_do("do 1 end 7", 2)]
#[case::after_import("import \"m\" def f(): 1;", 2)]
#[case::self_selector_chain(". .li", 1)]
#[case::call_with_trailing_do_block("f(x) do 1 | 2 end", 1)]
#[case::recursive_name("..h", 1)]
#[case::recursive_name_attr("..h.level", 1)]
#[case::recursive_name_chain("..h .code", 1)]
#[case::nested_chain_in_selector_args(".value .code(.value .h)", 1)]
#[case::recursive_key("..\"key\"", 1)]
#[case::qualified_index("x::y[0]", 1)]
#[case::literal_index("1[0]", 1)]
#[case::interpolated_pipeline("s\"${a | b}\"", 1)]
#[case::index_attr("x[0].row", 1)]
#[case::index_attr_chain("x[0].row[1].column", 1)]
fn test_statement_boundaries_that_remain_valid(#[case] code: &str, #[case] len: usize) {
    assert_eq!(parse_program_source(code).map(|p| p.len()), Ok(len), "{code:?}");
}

#[rstest]
#[case::symbol("[1 :b]")]
#[case::after_ident("[x :b]")]
#[case::after_dict("[{\"a\": 1} :b]")]
fn test_array_elements_need_a_comma(#[case] code: &str) {
    assert!(
        matches!(parse_program_source(code), Err(SyntaxError::ExpectedClosingBracket(..))),
        "expected a missing delimiter error for {code:?}"
    );
}

#[rstest]
#[case::leading("[, 1]")]
#[case::doubled("[1,, 2]")]
#[case::only_comma("[,]")]
fn test_array_rejects_misplaced_commas(#[case] code: &str) {
    assert!(
        matches!(parse_program_source(code), Err(SyntaxError::UnexpectedToken(_))),
        "expected an unexpected token error for {code:?}"
    );
}

#[rstest]
#[case::trailing("[1, 2,]", 1)]
#[case::empty("[]", 1)]
fn test_array_comma_forms_that_remain_valid(#[case] code: &str, #[case] len: usize) {
    assert_eq!(parse_program_source(code).map(|p| p.len()), Ok(len), "{code:?}");
}

#[rstest]
#[case::simple("match (x): | 2 \"two\" end")]
#[case::with_guard("match (x): | 2 if (true) \"two\" end")]
#[case::second_arm("match (x): | 1: \"one\" | 2 \"two\" end")]
fn test_match_arm_requires_a_colon(#[case] code: &str) {
    assert!(
        matches!(parse_program_source(code), Err(SyntaxError::UnexpectedToken(_))),
        "expected an unexpected token error for {code:?}"
    );
}

#[test]
fn test_match_arm_body_may_be_a_do_block() {
    assert_eq!(
        parse_program_source("match (x): | 1: do \"one\" end | _: 2 end").map(|p| p.len()),
        Ok(1)
    );
}

#[rstest]
#[case::root("1 | | 2")]
#[case::in_body("def f(): 1 | | 2;")]
fn test_consecutive_pipes_are_rejected(#[case] code: &str) {
    assert!(
        matches!(parse_program_source(code), Err(SyntaxError::UnexpectedToken(token)) if token.kind == TokenKind::Pipe),
        "expected an unexpected pipe error for {code:?}"
    );
}

#[rstest]
#[case::closing_bracket("s\"${x]}\"")]
#[case::comma("s\"${x, y}\"")]
#[case::two_expressions("s\"${x; y}\"")]
fn test_interpolation_holds_a_single_expression(#[case] code: &str) {
    assert!(
        matches!(parse_program_source(code), Err(SyntaxError::UnexpectedToken(_))),
        "expected an unexpected token error for {code:?}"
    );
}
