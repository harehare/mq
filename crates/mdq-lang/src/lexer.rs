pub mod error;
pub mod token;

use compact_str::CompactString;
use error::LexerError;
use nom::bytes::complete::is_not;
use nom::character::complete::line_ending;
use nom::combinator::opt;
use nom::number::complete::double;
use nom::{
    IResult,
    branch::alt,
    bytes::complete::{escaped_transform, tag, take_while_m_n},
    character::complete::{alpha1, alphanumeric1, char, multispace0, none_of},
    combinator::{map, map_opt, map_res, recognize, value},
    multi::{many0, many1},
    sequence::{delimited, pair, preceded},
};
use nom_locate::{LocatedSpan, position};
use token::{Token, TokenKind};

use crate::eval::module::ModuleId;
use crate::number::Number;
use crate::range::Range;

const MARKDOWN: &str = ".";

type Span<'a> = LocatedSpan<&'a str, ModuleId>;

#[derive(Debug, Clone, Default)]
pub struct Options {
    pub ignore_errors: bool,
    pub include_spaces: bool,
}

pub struct Lexer {
    options: Options,
}

impl Lexer {
    pub fn new(options: Options) -> Self {
        Self { options }
    }

    pub fn tokenize(&self, input: &str, module_id: ModuleId) -> Result<Vec<Token>, LexerError> {
        match tokens(Span::new_extra(input, module_id), &self.options) {
            Ok((span, tokens)) => {
                let eof: Range = span.into();

                if eof.start == eof.end || self.options.ignore_errors {
                    Ok([
                        tokens,
                        vec![Token {
                            range: eof,
                            kind: TokenKind::Eof,
                            module_id,
                        }],
                    ]
                    .concat())
                } else {
                    Err(LexerError::UnexpectedToken(Token {
                        range: eof,
                        kind: TokenKind::Eof,
                        module_id,
                    }))
                }
            }
            Err(nom::Err::Error(e)) | Err(nom::Err::Failure(e)) => {
                Err(LexerError::UnexpectedToken(Token {
                    range: e.input.into(),
                    kind: TokenKind::Eof,
                    module_id,
                }))
            }
            _ => unreachable!(),
        }
    }
}

#[inline(always)]
fn unicode(input: Span) -> IResult<Span, char> {
    map_opt(
        map_res(
            preceded(
                char('u'),
                delimited(
                    char('{'),
                    take_while_m_n(1, 6, |c: char| c.is_ascii_hexdigit()),
                    char('}'),
                ),
            ),
            |span: Span| u32::from_str_radix(span.fragment(), 16),
        ),
        char::from_u32,
    )(input)
}

#[inline(always)]
fn inline_comment(input: Span) -> IResult<Span, Token> {
    map(preceded(char('#'), is_not("\n\r|")), |span: Span| {
        let module_id = span.extra;
        let kind = TokenKind::Comment(span.fragment().to_string());
        Token {
            range: span.into(),
            kind,
            module_id,
        }
    })(input)
}

#[inline(always)]
fn newline(input: Span) -> IResult<Span, Token> {
    map(line_ending, |span: Span| {
        let module_id = span.extra;
        Token {
            range: span.into(),
            kind: TokenKind::NewLine,
            module_id,
        }
    })(input)
}

#[inline(always)]
fn tab(input: Span) -> IResult<Span, Token> {
    map(recognize(many1(char('\t'))), |span: Span| {
        let module_id = span.extra;
        let num = span.fragment().len();
        Token {
            range: span.into(),
            kind: TokenKind::Tab(num),
            module_id,
        }
    })(input)
}

#[inline(always)]
fn spaces(input: Span) -> IResult<Span, Token> {
    map(recognize(many1(char(' '))), |span: Span| {
        let module_id = span.extra;
        let num = span.fragment().len();
        Token {
            range: span.into(),
            kind: TokenKind::Whitespace(num),
            module_id,
        }
    })(input)
}

#[inline(always)]
fn comma(input: Span) -> IResult<Span, Token> {
    map(tag(","), |span: Span| {
        let module_id = span.extra;

        Token {
            range: span.into(),
            kind: TokenKind::Comma,
            module_id,
        }
    })(input)
}

#[inline(always)]
fn question(input: Span) -> IResult<Span, Token> {
    map(tag("?"), |span: Span| {
        let module_id = span.extra;
        Token {
            range: span.into(),
            kind: TokenKind::Question,
            module_id,
        }
    })(input)
}

#[inline(always)]
fn l_paren(input: Span) -> IResult<Span, Token> {
    map(tag("("), |span: Span| {
        let module_id = span.extra;
        Token {
            range: span.into(),
            kind: TokenKind::LParen,
            module_id,
        }
    })(input)
}

#[inline(always)]
fn r_paren(input: Span) -> IResult<Span, Token> {
    map(tag(")"), |span: Span| {
        let module_id = span.extra;
        Token {
            range: span.into(),
            kind: TokenKind::RParen,
            module_id,
        }
    })(input)
}

#[inline(always)]
fn l_bracket(input: Span) -> IResult<Span, Token> {
    map(tag("["), |span: Span| {
        let module_id = span.extra;
        Token {
            range: span.into(),
            kind: TokenKind::LBracket,
            module_id,
        }
    })(input)
}

#[inline(always)]
fn r_bracket(input: Span) -> IResult<Span, Token> {
    map(tag("]"), |span: Span| {
        let module_id = span.extra;
        Token {
            range: span.into(),
            kind: TokenKind::RBracket,
            module_id,
        }
    })(input)
}

#[inline(always)]
fn pipe(input: Span) -> IResult<Span, Token> {
    map(tag("|"), |span: Span| {
        let module_id = span.extra;
        Token {
            range: span.into(),
            kind: TokenKind::Pipe,
            module_id,
        }
    })(input)
}

#[inline(always)]
fn colon(input: Span) -> IResult<Span, Token> {
    map(tag(":"), |span: Span| {
        let module_id = span.extra;
        Token {
            range: span.into(),
            kind: TokenKind::Colon,
            module_id,
        }
    })(input)
}

#[inline(always)]
fn semi_colon(input: Span) -> IResult<Span, Token> {
    map(tag(";"), |span: Span| {
        let module_id = span.extra;
        Token {
            range: span.into(),
            kind: TokenKind::SemiColon,
            module_id,
        }
    })(input)
}

#[inline(always)]
fn equal(input: Span) -> IResult<Span, Token> {
    map(tag("="), |span: Span| {
        let module_id = span.extra;
        Token {
            range: span.into(),
            kind: TokenKind::Equal,
            module_id,
        }
    })(input)
}

#[inline(always)]
fn punctuations(input: Span) -> IResult<Span, Token> {
    alt((
        l_paren, r_paren, comma, colon, semi_colon, l_bracket, r_bracket, equal, pipe, question,
    ))(input)
}

#[inline(always)]
fn def(input: Span) -> IResult<Span, Token> {
    map(tag("def"), |span: Span| {
        let module_id = span.extra;
        Token {
            range: span.into(),
            kind: TokenKind::Def,
            module_id,
        }
    })(input)
}

#[inline(always)]
fn let_(input: Span) -> IResult<Span, Token> {
    map(tag("let"), |span: Span| {
        let module_id = span.extra;
        Token {
            range: span.into(),
            kind: TokenKind::Let,
            module_id,
        }
    })(input)
}

#[inline(always)]
fn self_(input: Span) -> IResult<Span, Token> {
    map(tag("self"), |span: Span| {
        let module_id = span.extra;
        Token {
            range: span.into(),
            kind: TokenKind::Self_,
            module_id,
        }
    })(input)
}

#[inline(always)]
fn while_(input: Span) -> IResult<Span, Token> {
    map(tag("while"), |span: Span| {
        let module_id = span.extra;
        Token {
            range: span.into(),
            kind: TokenKind::While,
            module_id,
        }
    })(input)
}

#[inline(always)]
fn until(input: Span) -> IResult<Span, Token> {
    map(tag("until"), |span: Span| {
        let module_id = span.extra;
        Token {
            range: span.into(),
            kind: TokenKind::Until,
            module_id,
        }
    })(input)
}

#[inline(always)]
fn if_(input: Span) -> IResult<Span, Token> {
    map(tag("if"), |span: Span| {
        let module_id = span.extra;
        Token {
            range: span.into(),
            kind: TokenKind::If,
            module_id,
        }
    })(input)
}

#[inline(always)]
fn else_(input: Span) -> IResult<Span, Token> {
    map(tag("else"), |span: Span| {
        let module_id = span.extra;
        Token {
            range: span.into(),
            kind: TokenKind::Else,
            module_id,
        }
    })(input)
}

#[inline(always)]
fn elif(input: Span) -> IResult<Span, Token> {
    map(tag("elif"), |span: Span| {
        let module_id = span.extra;
        Token {
            range: span.into(),
            kind: TokenKind::Elif,
            module_id,
        }
    })(input)
}

#[inline(always)]
fn none(input: Span) -> IResult<Span, Token> {
    map(tag("None"), |span: Span| {
        let module_id = span.extra;
        Token {
            range: span.into(),
            kind: TokenKind::None,
            module_id,
        }
    })(input)
}

#[inline(always)]
fn include(input: Span) -> IResult<Span, Token> {
    map(tag("include"), |span: Span| {
        let module_id = span.extra;
        Token {
            range: span.into(),
            kind: TokenKind::Include,
            module_id,
        }
    })(input)
}

#[inline(always)]
fn foreach(input: Span) -> IResult<Span, Token> {
    map(tag("foreach"), |span: Span| {
        let module_id = span.extra;
        Token {
            range: span.into(),
            kind: TokenKind::Foreach,
            module_id,
        }
    })(input)
}

#[inline(always)]
fn keywords(input: Span) -> IResult<Span, Token> {
    alt((
        def, let_, self_, while_, until, if_, elif, else_, none, include, foreach,
    ))(input)
}

#[inline(always)]
fn number_literal(input: Span) -> IResult<Span, Token> {
    map_res(recognize(pair(opt(char('-')), double)), |span: Span| {
        str::parse(span.fragment()).map(|s| {
            let module_id = span.extra;
            Token {
                range: span.into(),
                kind: TokenKind::NumberLiteral(Number::new(s)),
                module_id,
            }
        })
    })(input)
}

#[inline(always)]
fn empty_string(input: Span) -> IResult<Span, Token> {
    map(tag("\"\""), |span: Span| Token {
        range: span.into(),
        kind: TokenKind::StringLiteral(String::new()),
        module_id: span.extra,
    })(input)
}

#[inline(always)]
fn string_literal(input: Span) -> IResult<Span, Token> {
    let (span, start) = position(input)?;
    let (span, s) = delimited(
        char('"'),
        escaped_transform(
            none_of("\"\\"),
            '\\',
            alt((
                value('\\', char('\\')),
                value('\"', char('\"')),
                value('\'', char('\'')),
                value('\r', char('r')),
                value('\n', char('n')),
                value('\t', char('t')),
                unicode,
            )),
        ),
        char('"'),
    )(span)?;
    let (span, end) = position(span)?;
    let module_id = start.extra;

    Ok((
        span,
        Token {
            range: Range {
                start: start.into(),
                end: end.into(),
            },
            kind: TokenKind::StringLiteral(s.to_string()),
            module_id,
        },
    ))
}

#[inline(always)]
fn literals(input: Span) -> IResult<Span, Token> {
    alt((number_literal, empty_string, string_literal))(input)
}

#[inline(always)]
fn ident(input: Span) -> IResult<Span, Token> {
    map(
        recognize(pair(
            alt((alpha1, tag("_"), tag(MARKDOWN))),
            many0(alt((alphanumeric1, tag("_"), tag("-"), tag("#"), tag("*")))),
        )),
        |span: Span| match *span.fragment() {
            "true" => {
                let module_id = span.extra;
                Token {
                    range: span.into(),
                    kind: TokenKind::BoolLiteral(true),
                    module_id,
                }
            }
            "false" => {
                let module_id = span.extra;
                Token {
                    range: span.into(),
                    kind: TokenKind::BoolLiteral(false),
                    module_id,
                }
            }
            _ => {
                let module_id = span.extra;
                let fragment = span.fragment();

                if fragment.starts_with(".") {
                    let kind = TokenKind::Selector(CompactString::new(span.fragment()));
                    Token {
                        range: span.into(),
                        kind,
                        module_id,
                    }
                } else {
                    let kind = TokenKind::Ident(CompactString::new(span.fragment()));
                    Token {
                        range: span.into(),
                        kind,
                        module_id,
                    }
                }
            }
        },
    )(input)
}

#[inline(always)]
fn env(input: Span) -> IResult<Span, Token> {
    preceded(
        tag("$"),
        map(
            recognize(many1(alt((alphanumeric1, tag("_"))))),
            |span: Span| {
                let kind = TokenKind::Env(CompactString::new(span.fragment()));
                let module_id = span.extra;
                Token {
                    range: span.into(),
                    kind,
                    module_id,
                }
            },
        ),
    )(input)
}

#[inline(always)]
fn token(input: Span) -> IResult<Span, Token> {
    alt((inline_comment, punctuations, keywords, env, literals, ident))(input)
}

#[inline(always)]
fn token_include_spaces(input: Span) -> IResult<Span, Token> {
    alt((
        newline,
        spaces,
        tab,
        inline_comment,
        punctuations,
        keywords,
        env,
        literals,
        ident,
    ))(input)
}

#[inline(always)]
fn tokens<'a>(input: Span<'a>, options: &'a Options) -> IResult<Span<'a>, Vec<Token>> {
    if options.include_spaces {
        many0(token_include_spaces)(input)
    } else {
        many0(delimited(multispace0, token, multispace0))(input)
    }
}

#[cfg(test)]
mod tests {
    use crate::range::Position;

    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case("and(contains(\"test\"))",
        Options::default(),
        Ok(vec![
          Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 4} }, kind: TokenKind::Ident(CompactString::new("and")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 4}, end: Position {line: 1, column: 5} }, kind: TokenKind::LParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 5}, end: Position {line: 1, column: 13} }, kind: TokenKind::Ident(CompactString::new("contains")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 13}, end: Position {line: 1, column: 14} }, kind: TokenKind::LParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 14}, end: Position {line: 1, column: 20} }, kind: TokenKind::StringLiteral("test".to_string()), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 20}, end: Position {line: 1, column: 21} }, kind: TokenKind::RParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 21}, end: Position {line: 1, column: 22} }, kind: TokenKind::RParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 22}, end: Position {line: 1, column: 22} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case("and(contains(\"test\")) | or(endswith(\"test\"))",
        Options::default(),
        Ok(vec![
          Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 4} }, kind: TokenKind::Ident(CompactString::new("and")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 4}, end: Position {line: 1, column: 5} }, kind: TokenKind::LParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 5}, end: Position {line: 1, column: 13} }, kind: TokenKind::Ident(CompactString::new("contains")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 13}, end: Position {line: 1, column: 14} }, kind: TokenKind::LParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 14}, end: Position {line: 1, column: 20} }, kind: TokenKind::StringLiteral("test".to_string()), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 20}, end: Position {line: 1, column: 21} }, kind: TokenKind::RParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 21}, end: Position {line: 1, column: 22} }, kind: TokenKind::RParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 23}, end: Position {line: 1, column: 24} }, kind: TokenKind::Pipe, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 25}, end: Position {line: 1, column: 27} }, kind: TokenKind::Ident(CompactString::new("or")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 27}, end: Position {line: 1, column: 28} }, kind: TokenKind::LParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 28}, end: Position {line: 1, column: 36} }, kind: TokenKind::Ident(CompactString::new("endswith")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 36}, end: Position {line: 1, column: 37} }, kind: TokenKind::LParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 37}, end: Position {line: 1, column: 43} }, kind: TokenKind::StringLiteral("test".to_string()), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 43}, end: Position {line: 1, column: 44} }, kind: TokenKind::RParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 44}, end: Position {line: 1, column: 45} }, kind: TokenKind::RParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 45}, end: Position {line: 1, column: 45} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case("eq(length(), 10)",
        Options::default(),
        Ok(vec![
          Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 3} }, kind: TokenKind::Ident(CompactString::new("eq")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 3}, end: Position {line: 1, column: 4} }, kind: TokenKind::LParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 4}, end: Position {line: 1, column: 10} }, kind: TokenKind::Ident(CompactString::new("length")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 10}, end: Position {line: 1, column: 11} }, kind: TokenKind::LParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 11}, end: Position {line: 1, column: 12} }, kind: TokenKind::RParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 12}, end: Position {line: 1, column: 13} }, kind: TokenKind::Comma, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 14}, end: Position {line: 1, column: 16} }, kind: TokenKind::NumberLiteral(10.into()), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 16}, end: Position {line: 1, column: 17} }, kind: TokenKind::RParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 17}, end: Position {line: 1, column: 17} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case("or(.##, .**)",
        Options::default(),
        Ok(vec![
          Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 3} }, kind: TokenKind::Ident(CompactString::new("or")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 3}, end: Position {line: 1, column: 4} }, kind: TokenKind::LParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 4}, end: Position {line: 1, column: 7} }, kind: TokenKind::Selector(CompactString::new(".##")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 7}, end: Position {line: 1, column: 8} }, kind: TokenKind::Comma, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 9}, end: Position {line: 1, column: 12} }, kind: TokenKind::Selector(CompactString::new(".**")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 12}, end: Position {line: 1, column: 13} }, kind: TokenKind::RParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 13}, end: Position {line: 1, column: 13} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case("or(.[][], .[])",
        Options::default(),
        Ok(vec![
          Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 3} }, kind: TokenKind::Ident(CompactString::new("or")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 3}, end: Position {line: 1, column: 4} }, kind: TokenKind::LParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 4}, end: Position {line: 1, column: 5} }, kind: TokenKind::Selector(CompactString::new(".")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 5}, end: Position {line: 1, column: 6} }, kind: TokenKind::LBracket, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 6}, end: Position {line: 1, column: 7} }, kind: TokenKind::RBracket, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 7}, end: Position {line: 1, column: 8} }, kind: TokenKind::LBracket, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 8}, end: Position {line: 1, column: 9} }, kind: TokenKind::RBracket, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 9}, end: Position {line: 1, column: 10} }, kind: TokenKind::Comma, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 11}, end: Position {line: 1, column: 12} }, kind: TokenKind::Selector(CompactString::new(".")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 12}, end: Position {line: 1, column: 13} }, kind: TokenKind::LBracket, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 13}, end: Position {line: 1, column: 14} }, kind: TokenKind::RBracket, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 14}, end: Position {line: 1, column: 15} }, kind: TokenKind::RParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 15}, end: Position {line: 1, column: 15} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case("startswith(\"\u{0061}\")",
        Options::default(),
        Ok(vec![
          Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 11} }, kind: TokenKind::Ident(CompactString::new("startswith")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 11}, end: Position {line: 1, column: 12} }, kind: TokenKind::LParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 12}, end: Position {line: 1, column: 15} }, kind: TokenKind::StringLiteral("a".to_string()), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 15}, end: Position {line: 1, column: 16} }, kind: TokenKind::RParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 16}, end: Position {line: 1, column: 16} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case("endswith($ENV)",
        Options::default(),
        Ok(vec![
          Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 9} }, kind: TokenKind::Ident(CompactString::new("endswith")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 9}, end: Position {line: 1, column: 10} }, kind: TokenKind::LParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 11}, end: Position {line: 1, column: 14} }, kind: TokenKind::Env(CompactString::new("ENV")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 14}, end: Position {line: 1, column: 15} }, kind: TokenKind::RParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 15}, end: Position {line: 1, column: 15} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case("def check(arg1, arg2): startswith(\"\u{0061}\")",
        Options::default(),
        Ok(vec![
          Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 4} }, kind: TokenKind::Def, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 5}, end: Position {line: 1, column: 10} }, kind: TokenKind::Ident(CompactString::new("check")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 10}, end: Position {line: 1, column: 11} }, kind: TokenKind::LParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 11}, end: Position {line: 1, column: 15} }, kind: TokenKind::Ident(CompactString::new("arg1")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 15}, end: Position {line: 1, column: 16} }, kind: TokenKind::Comma, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 17}, end: Position {line: 1, column: 21} }, kind: TokenKind::Ident(CompactString::new("arg2")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 21}, end: Position {line: 1, column: 22} }, kind: TokenKind::RParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 22}, end: Position {line: 1, column: 23} }, kind: TokenKind::Colon, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 24}, end: Position {line: 1, column: 34} }, kind: TokenKind::Ident(CompactString::new("startswith")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 34}, end: Position {line: 1, column: 35} }, kind: TokenKind::LParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 35}, end: Position {line: 1, column: 38} }, kind: TokenKind::StringLiteral("a".to_string()), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 38}, end: Position {line: 1, column: 39} }, kind: TokenKind::RParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 39}, end: Position {line: 1, column: 39} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case("\"test",
          Options::default(),
          Err(LexerError::UnexpectedToken(Token {
              range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 6} },
              kind: TokenKind::Eof,
              module_id: 1.into()
          })))]
    #[case::new_line("and(\ncontains(\"test\"))",
            Options{include_spaces: true, ignore_errors: true},
            Ok(vec![
              Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 4} }, kind: TokenKind::Ident(CompactString::new("and")), module_id: 1.into()},
              Token{range: Range { start: Position {line: 1, column: 4}, end: Position {line: 1, column: 5} }, kind: TokenKind::LParen, module_id: 1.into()},
              Token{range: Range { start: Position {line: 1, column: 5}, end: Position {line: 1, column: 6} }, kind: TokenKind::NewLine, module_id: 1.into()},
              Token{range: Range { start: Position {line: 2, column: 1}, end: Position {line: 2, column: 9} }, kind: TokenKind::Ident(CompactString::new("contains")), module_id: 1.into()},
              Token{range: Range { start: Position {line: 2, column: 9}, end: Position {line: 2, column: 10} }, kind: TokenKind::LParen, module_id: 1.into()},
              Token{range: Range { start: Position {line: 2, column: 10}, end: Position {line: 2, column: 16} }, kind: TokenKind::StringLiteral("test".to_string()), module_id: 1.into()},
              Token{range: Range { start: Position {line: 2, column: 16}, end: Position {line: 2, column: 17} }, kind: TokenKind::RParen, module_id: 1.into()},
              Token{range: Range { start: Position {line: 2, column: 17}, end: Position {line: 2, column: 18} }, kind: TokenKind::RParen, module_id: 1.into()},
              Token{range: Range { start: Position {line: 2, column: 18}, end: Position {line: 2, column: 18} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case("and(\ncontains(\"test\")) | or(\nendswith(\"test\"))",
            Options{include_spaces: true, ignore_errors: true},
            Ok(vec![
              Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 4} }, kind: TokenKind::Ident(CompactString::new("and")), module_id: 1.into()},
              Token{range: Range { start: Position {line: 1, column: 4}, end: Position {line: 1, column: 5} }, kind: TokenKind::LParen, module_id: 1.into()},
              Token{range: Range { start: Position {line: 1, column: 5}, end: Position {line: 1, column: 6} }, kind: TokenKind::NewLine, module_id: 1.into()},
              Token{range: Range { start: Position {line: 2, column: 1}, end: Position {line: 2, column: 9} }, kind: TokenKind::Ident(CompactString::new("contains")), module_id: 1.into()},
              Token{range: Range { start: Position {line: 2, column: 9}, end: Position {line: 2, column: 10} }, kind: TokenKind::LParen, module_id: 1.into()},
              Token{range: Range { start: Position {line: 2, column: 10}, end: Position {line: 2, column: 16} }, kind: TokenKind::StringLiteral("test".to_string()), module_id: 1.into()},
              Token{range: Range { start: Position {line: 2, column: 16}, end: Position {line: 2, column: 17} }, kind: TokenKind::RParen, module_id: 1.into()},
              Token{range: Range { start: Position {line: 2, column: 17}, end: Position {line: 2, column: 18} }, kind: TokenKind::RParen, module_id: 1.into()},
              Token{range: Range { start: Position {line: 2, column: 18}, end: Position {line: 2, column: 19} }, kind: TokenKind::Whitespace(1), module_id: 1.into()},
              Token{range: Range { start: Position {line: 2, column: 19}, end: Position {line: 2, column: 20} }, kind: TokenKind::Pipe, module_id: 1.into()},
              Token{range: Range { start: Position {line: 2, column: 20}, end: Position {line: 2, column: 21} }, kind: TokenKind::Whitespace(1), module_id: 1.into()},
              Token{range: Range { start: Position {line: 2, column: 21}, end: Position {line: 2, column: 23} }, kind: TokenKind::Ident(CompactString::new("or")), module_id: 1.into()},
              Token{range: Range { start: Position {line: 2, column: 23}, end: Position {line: 2, column: 24} }, kind: TokenKind::LParen, module_id: 1.into()},
              Token{range: Range { start: Position {line: 2, column: 24}, end: Position {line: 2, column: 25} }, kind: TokenKind::NewLine, module_id: 1.into()},
              Token{range: Range { start: Position {line: 3, column: 1}, end: Position {line: 3, column: 9} }, kind: TokenKind::Ident(CompactString::new("endswith")), module_id: 1.into()},
              Token{range: Range { start: Position {line: 3, column: 9}, end: Position {line: 3, column: 10} }, kind: TokenKind::LParen, module_id: 1.into()},
              Token{range: Range { start: Position {line: 3, column: 10}, end: Position {line: 3, column: 16} }, kind: TokenKind::StringLiteral("test".to_string()), module_id: 1.into()},
              Token{range: Range { start: Position {line: 3, column: 16}, end: Position {line: 3, column: 17} }, kind: TokenKind::RParen, module_id: 1.into()},
              Token{range: Range { start: Position {line: 3, column: 17}, end: Position {line: 3, column: 18} }, kind: TokenKind::RParen, module_id: 1.into()},
              Token{range: Range { start: Position {line: 3, column: 18}, end: Position {line: 3, column: 18} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    fn test(
        #[case] input: &str,
        #[case] options: Options,
        #[case] expected: Result<Vec<Token>, LexerError>,
    ) {
        assert_eq!(Lexer::new(options).tokenize(input, 1.into()), expected);
    }
}
