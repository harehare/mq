pub mod error;
pub mod token;

use compact_str::CompactString;
use error::LexerError;
use nom::Parser;
use nom::bytes::complete::{is_not, take_until};
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
use token::{StringSegment, Token, TokenKind};

use crate::eval::module::ModuleId;
use crate::number::Number;
use crate::range::Range;

const MARKDOWN: &str = ".";

type Span<'a> = LocatedSpan<&'a str, ModuleId>;

macro_rules! define_token_parser {
    ($name:ident, $tag:expr, $kind:expr) => {
        fn $name(input: Span) -> IResult<Span, Token> {
            map(tag($tag), |span: Span| {
                let module_id = span.extra;
                Token {
                    range: span.into(),
                    kind: $kind,
                    module_id,
                }
            })
            .parse(input)
        }
    };
}

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
                    Err(LexerError::UnexpectedEOFDetected(module_id))
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
    )
    .parse(input)
}

fn inline_comment(input: Span) -> IResult<Span, Token> {
    map(preceded(char('#'), is_not("\n\r|")), |span: Span| {
        let module_id = span.extra;
        let kind = TokenKind::Comment(span.fragment().to_string());
        Token {
            range: span.into(),
            kind,
            module_id,
        }
    })
    .parse(input)
}

fn newline(input: Span) -> IResult<Span, Token> {
    map(line_ending, |span: Span| {
        let module_id = span.extra;
        Token {
            range: span.into(),
            kind: TokenKind::NewLine,
            module_id,
        }
    })
    .parse(input)
}

fn tab(input: Span) -> IResult<Span, Token> {
    map(recognize(many1(char('\t'))), |span: Span| {
        let module_id = span.extra;
        let num = span.fragment().len();
        Token {
            range: span.into(),
            kind: TokenKind::Tab(num),
            module_id,
        }
    })
    .parse(input)
}

fn spaces(input: Span) -> IResult<Span, Token> {
    map(recognize(many1(char(' '))), |span: Span| {
        let module_id = span.extra;
        let num = span.fragment().len();
        Token {
            range: span.into(),
            kind: TokenKind::Whitespace(num),
            module_id,
        }
    })
    .parse(input)
}

define_token_parser!(comma, ",", TokenKind::Comma);
define_token_parser!(question, "?", TokenKind::Question);
define_token_parser!(l_paren, "(", TokenKind::LParen);
define_token_parser!(r_paren, ")", TokenKind::RParen);
define_token_parser!(l_bracket, "[", TokenKind::LBracket);
define_token_parser!(r_bracket, "]", TokenKind::RBracket);
define_token_parser!(pipe, "|", TokenKind::Pipe);
define_token_parser!(colon, ":", TokenKind::Colon);
define_token_parser!(semi_colon, ";", TokenKind::SemiColon);
define_token_parser!(equal, "=", TokenKind::Equal);
define_token_parser!(def, "def ", TokenKind::Def);
define_token_parser!(let_, "let ", TokenKind::Let);
define_token_parser!(self_, "self", TokenKind::Self_);
define_token_parser!(while_, "while", TokenKind::While);
define_token_parser!(until, "until", TokenKind::Until);
define_token_parser!(if_, "if", TokenKind::If);
define_token_parser!(else_, "else", TokenKind::Else);
define_token_parser!(elif, "elif", TokenKind::Elif);
define_token_parser!(none, "None", TokenKind::None);
define_token_parser!(include, "include", TokenKind::Include);
define_token_parser!(foreach, "foreach", TokenKind::Foreach);
define_token_parser!(nodes, "nodes", TokenKind::Nodes);
define_token_parser!(fn_, "fn", TokenKind::Fn);
define_token_parser!(map_start, "Map {", TokenKind::MapStart);
define_token_parser!(map_end, "}", TokenKind::MapEnd);
define_token_parser!(
    empty_string,
    "\"\"",
    TokenKind::StringLiteral(String::new())
);

fn punctuations(input: Span) -> IResult<Span, Token> {
    alt((
        l_paren, r_paren, comma, colon, semi_colon, l_bracket, r_bracket, map_end, equal, pipe, question,
    ))
    .parse(input)
}

fn keywords(input: Span) -> IResult<Span, Token> {
    alt((
        nodes, def, let_, self_, while_, until, if_, elif, else_, none, include, foreach, fn_, map_start,
    ))
    .parse(input)
}

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
    })
    .parse(input)
}

fn interpolation_ident(input: Span) -> IResult<Span, Span> {
    delimited(tag("${"), take_until("}"), char('}')).parse(input)
}

fn string_segment<'a>(input: Span<'a>) -> IResult<Span<'a>, StringSegment> {
    alt((
        map(
            |input: Span<'a>| {
                let (span, start) = position(input)?;
                let (span, ident) = interpolation_ident(span)?;
                let (span, end) = position(span)?;
                Ok((
                    span,
                    (
                        ident,
                        Range {
                            start: start.into(),
                            end: end.into(),
                        },
                    ),
                ))
            },
            |(ident, range)| StringSegment::Ident(ident.to_string().into(), range),
        ),
        map(
            |input| {
                let (span, start) = position(input)?;
                let (span, text) = escaped_transform(
                    none_of("\"\\${"),
                    '\\',
                    alt((
                        value('\\', char('\\')),
                        value('\"', char('\"')),
                        value('\r', char('r')),
                        value('\n', char('n')),
                        value('\t', char('t')),
                        unicode,
                    )),
                )(span)?;
                let (span, end) = position(span)?;
                Ok((
                    span,
                    (
                        text,
                        Range {
                            start: start.into(),
                            end: end.into(),
                        },
                    ),
                ))
            },
            |(text, range)| StringSegment::Text(text.to_string(), range),
        ),
        map(
            |input: Span<'a>| {
                let (span, start) = position(input)?;
                let (span, _) = tag("$$")(span)?;
                let (span, end) = position(span)?;
                Ok((
                    span,
                    (
                        "$".to_string(),
                        Range {
                            start: start.into(),
                            end: end.into(),
                        },
                    ),
                ))
            },
            |(text, range)| StringSegment::Text(text, range),
        ),
    ))
    .parse(input)
}

fn interpolated_string(input: Span) -> IResult<Span, Token> {
    let (span, start) = position(input)?;
    let (span, _) = tag("s\"")(span)?;
    let (span, segments) = many1(string_segment).parse(span)?;
    let (span, _) = char('"')(span)?;
    let (span, end) = position(span)?;
    let module_id = start.extra;

    Ok((
        span,
        Token {
            range: Range {
                start: start.into(),
                end: end.into(),
            },
            kind: TokenKind::InterpolatedString(segments),
            module_id,
        },
    ))
}

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
                value('\r', char('r')),
                value('\n', char('n')),
                value('\t', char('t')),
                unicode,
            )),
        ),
        char('"'),
    )
    .parse(span)?;
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

fn literals(input: Span) -> IResult<Span, Token> {
    alt((
        number_literal,
        empty_string,
        interpolated_string,
        string_literal,
    ))
    .parse(input)
}

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
    )
    .parse(input)
}

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
    )
    .parse(input)
}

fn token(input: Span) -> IResult<Span, Token> {
    alt((inline_comment, punctuations, keywords, env, literals, ident)).parse(input)
}

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
    ))
    .parse(input)
}

fn tokens<'a>(input: Span<'a>, options: &'a Options) -> IResult<Span<'a>, Vec<Token>> {
    if options.include_spaces {
        many0(token_include_spaces).parse(input)
    } else {
        many0(delimited(multispace0, token, multispace0)).parse(input)
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
    #[case("startswith(\"\\u{0061}\")",
        Options::default(),
        Ok(vec![
          Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 11} }, kind: TokenKind::Ident(CompactString::new("startswith")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 11}, end: Position {line: 1, column: 12} }, kind: TokenKind::LParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 12}, end: Position {line: 1, column: 22} }, kind: TokenKind::StringLiteral("a".to_string()), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 22}, end: Position {line: 1, column: 23} }, kind: TokenKind::RParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 23}, end: Position {line: 1, column: 23} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case("endswith($ENV)",
        Options::default(),
        Ok(vec![
          Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 9} }, kind: TokenKind::Ident(CompactString::new("endswith")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 9}, end: Position {line: 1, column: 10} }, kind: TokenKind::LParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 11}, end: Position {line: 1, column: 14} }, kind: TokenKind::Env(CompactString::new("ENV")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 14}, end: Position {line: 1, column: 15} }, kind: TokenKind::RParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 15}, end: Position {line: 1, column: 15} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case("def check(arg1, arg2): startswith(\"\\u{0061}\")",
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
          Token{range: Range { start: Position {line: 1, column: 35}, end: Position {line: 1, column: 45} }, kind: TokenKind::StringLiteral("a".to_string()), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 45}, end: Position {line: 1, column: 46} }, kind: TokenKind::RParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 46}, end: Position {line: 1, column: 46} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case("\"test",
          Options::default(),
          Err(LexerError::UnexpectedEOFDetected(1.into())))]
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
    #[case::tab("and(\tcontains(\"test\"))",
            Options{include_spaces: true, ignore_errors: true},
            Ok(vec![
              Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 4} }, kind: TokenKind::Ident(CompactString::new("and")), module_id: 1.into()},
              Token{range: Range { start: Position {line: 1, column: 4}, end: Position {line: 1, column: 5} }, kind: TokenKind::LParen, module_id: 1.into()},
              Token{range: Range { start: Position {line: 1, column: 5}, end: Position {line: 1, column: 6} }, kind: TokenKind::Tab(1), module_id: 1.into()},
              Token{range: Range { start: Position {line: 1, column: 6}, end: Position {line: 1, column: 14} }, kind: TokenKind::Ident(CompactString::new("contains")), module_id: 1.into()},
              Token{range: Range { start: Position {line: 1, column: 14}, end: Position {line: 1, column: 15} }, kind: TokenKind::LParen, module_id: 1.into()},
              Token{range: Range { start: Position {line: 1, column: 15}, end: Position {line: 1, column: 21} }, kind: TokenKind::StringLiteral("test".to_string()), module_id: 1.into()},
              Token{range: Range { start: Position {line: 1, column: 21}, end: Position {line: 1, column: 22} }, kind: TokenKind::RParen, module_id: 1.into()},
              Token{range: Range { start: Position {line: 1, column: 22}, end: Position {line: 1, column: 23} }, kind: TokenKind::RParen, module_id: 1.into()},
              Token{range: Range { start: Position {line: 1, column: 23}, end: Position {line: 1, column: 23} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case::interpolated_string("s\"test${val1}test\n\"",
            Options{include_spaces: true, ignore_errors: true},
            Ok(vec![Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 2, column: 2} },
                          kind: TokenKind::InterpolatedString(vec![
                            StringSegment::Text("test".to_string(), Range { start: Position {line: 1, column: 3}, end: Position {line: 1, column: 7} }),
                            StringSegment::Ident("val1".to_string().into(), Range { start: Position {line: 1, column: 7}, end: Position {line: 1, column: 14} }),
                            StringSegment::Text("test\n".to_string(), Range { start: Position {line: 1, column: 14}, end: Position {line: 2, column: 1 }})
                          ]), module_id: 1.into()},
                   Token{range: Range { start: Position {line: 2, column: 2}, end: Position {line: 2, column: 2} }, kind: TokenKind::Eof, module_id: 1.into()}]
                ))]
    #[case::error("\"test",
            Options{include_spaces: false, ignore_errors: false},
            Err(LexerError::UnexpectedEOFDetected(1.into())))]
    #[case::error("s\"$$${test}$$\"",
            Options{include_spaces: false, ignore_errors: false},
            Ok(vec![Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 15} },
                          kind: TokenKind::InterpolatedString(vec![
                            StringSegment::Text("$".to_string(), Range { start: Position {line: 1, column: 3}, end: Position {line: 1, column: 5} }),
                            StringSegment::Ident("test".to_string().into(), Range { start: Position {line: 1, column: 5}, end: Position {line: 1, column: 12} }),
                            StringSegment::Text("$".to_string(), Range { start: Position {line: 1, column: 12}, end: Position {line: 1, column: 14 }})
                          ]), module_id: 1.into()},
                   Token{range: Range { start: Position {line: 1, column: 15}, end: Position {line: 1, column: 15} }, kind: TokenKind::Eof, module_id: 1.into()}]
                ))]
    #[case::function_declaration("fn(): program;",
            Options::default(),
            Ok(vec![
              Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 3} }, kind: TokenKind::Fn, module_id: 1.into()},
              Token{range: Range { start: Position {line: 1, column: 3}, end: Position {line: 1, column: 4} }, kind: TokenKind::LParen, module_id: 1.into()},
              Token{range: Range { start: Position {line: 1, column: 4}, end: Position {line: 1, column: 5} }, kind: TokenKind::RParen, module_id: 1.into()},
              Token{range: Range { start: Position {line: 1, column: 5}, end: Position {line: 1, column: 6} }, kind: TokenKind::Colon, module_id: 1.into()},
              Token{range: Range { start: Position {line: 1, column: 7}, end: Position {line: 1, column: 14} }, kind: TokenKind::Ident(CompactString::new("program")), module_id: 1.into()},
              Token{range: Range { start: Position {line: 1, column: 14}, end: Position {line: 1, column: 15} }, kind: TokenKind::SemiColon, module_id: 1.into()},
              Token{range: Range { start: Position {line: 1, column: 15}, end: Position {line: 1, column: 15} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case::map_simple("Map { \"key\": 123 }",
        Options::default(),
        Ok(vec![
            Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 6} }, kind: TokenKind::MapStart, module_id: 1.into()},
            Token{range: Range { start: Position {line: 1, column: 7}, end: Position {line: 1, column: 12} }, kind: TokenKind::StringLiteral("key".to_string()), module_id: 1.into()},
            Token{range: Range { start: Position {line: 1, column: 12}, end: Position {line: 1, column: 13} }, kind: TokenKind::Colon, module_id: 1.into()},
            Token{range: Range { start: Position {line: 1, column: 14}, end: Position {line: 1, column: 17} }, kind: TokenKind::NumberLiteral(123.into()), module_id: 1.into()},
            Token{range: Range { start: Position {line: 1, column: 18}, end: Position {line: 1, column: 19} }, kind: TokenKind::MapEnd, module_id: 1.into()},
            Token{range: Range { start: Position {line: 1, column: 19}, end: Position {line: 1, column: 19} }, kind: TokenKind::Eof, module_id: 1.into()},
        ]))]
    #[case::map_empty("Map {}",
        Options::default(),
        Ok(vec![
            Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 6} }, kind: TokenKind::MapStart, module_id: 1.into()},
            Token{range: Range { start: Position {line: 1, column: 6}, end: Position {line: 1, column: 7} }, kind: TokenKind::MapEnd, module_id: 1.into()},
            Token{range: Range { start: Position {line: 1, column: 7}, end: Position {line: 1, column: 7} }, kind: TokenKind::Eof, module_id: 1.into()},
        ]))]
    #[case::map_with_whitespace(" Map { } ",
        Options::default(),
        Ok(vec![
            Token{range: Range { start: Position {line: 1, column: 2}, end: Position {line: 1, column: 7} }, kind: TokenKind::MapStart, module_id: 1.into()},
            Token{range: Range { start: Position {line: 1, column: 8}, end: Position {line: 1, column: 9} }, kind: TokenKind::MapEnd, module_id: 1.into()},
            Token{range: Range { start: Position {line: 1, column: 10}, end: Position {line: 1, column: 10} }, kind: TokenKind::Eof, module_id: 1.into()},
        ]))]
    #[case::map_start_only("Map {",
        Options::default(),
        Ok(vec![
            Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 6} }, kind: TokenKind::MapStart, module_id: 1.into()},
            Token{range: Range { start: Position {line: 1, column: 6}, end: Position {line: 1, column: 6} }, kind: TokenKind::Eof, module_id: 1.into()},
        ]))]
    #[case::map_end_only("}",
        Options::default(),
        Ok(vec![
            Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 2} }, kind: TokenKind::MapEnd, module_id: 1.into()},
            Token{range: Range { start: Position {line: 1, column: 2}, end: Position {line: 1, column: 2} }, kind: TokenKind::Eof, module_id: 1.into()},
        ]))]
    fn test_parse(
        #[case] input: &str,
        #[case] options: Options,
        #[case] expected: Result<Vec<Token>, LexerError>,
    ) {
        assert_eq!(Lexer::new(options).tokenize(input, 1.into()), expected);
    }
}
