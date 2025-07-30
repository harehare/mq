pub mod error;
pub mod token;

use compact_str::CompactString;
use error::LexerError;
use nom::Parser;
use nom::bytes::complete::{is_not, take_until};
use nom::character::complete::{digit1, line_ending};
use nom::combinator::{cut, opt};
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

macro_rules! define_keyword_parser {
    ($name:ident, $keyword:expr, $kind:expr) => {
        fn $name(input: Span) -> IResult<Span, Token> {
            let (remaining, matched) = tag($keyword)(input)?;

            if !remaining.fragment().is_empty() {
                let c = remaining.fragment().chars().next().unwrap_or('\0');
                if c.is_alphanumeric() || c == '_' {
                    return Err(nom::Err::Error(nom::error::Error::new(
                        input,
                        nom::error::ErrorKind::Tag,
                    )));
                }
            }

            let module_id = matched.extra;

            Ok((
                remaining,
                Token {
                    range: matched.into(),
                    kind: $kind,
                    module_id,
                },
            ))
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

fn hex_escape(input: Span) -> IResult<Span, char> {
    map_opt(
        map_res(
            preceded(
                char('x'),
                take_while_m_n(2, 2, |c: char| c.is_ascii_hexdigit()),
            ),
            |span: Span| u8::from_str_radix(span.fragment(), 16),
        ),
        |byte| char::from_u32(byte as u32),
    )
    .parse(input)
}

fn inline_comment(input: Span) -> IResult<Span, Token> {
    map(preceded(char('#'), is_not("\n\r")), |span: Span| {
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

define_token_parser!(colon, ":", TokenKind::Colon);
define_token_parser!(comma, ",", TokenKind::Comma);
define_keyword_parser!(def, "def", TokenKind::Def);
define_keyword_parser!(elif, "elif", TokenKind::Elif);
define_keyword_parser!(else_, "else", TokenKind::Else);
define_keyword_parser!(end, "end", TokenKind::End);
define_token_parser!(
    empty_string,
    "\"\"",
    TokenKind::StringLiteral(String::new())
);
define_token_parser!(eq_eq, "==", TokenKind::EqEq);
define_token_parser!(equal, "=", TokenKind::Equal);
define_keyword_parser!(break_, "break", TokenKind::Break);
define_keyword_parser!(continue_, "continue", TokenKind::Continue);
define_keyword_parser!(fn_, "fn", TokenKind::Fn);
define_keyword_parser!(foreach, "foreach", TokenKind::Foreach);
define_keyword_parser!(if_, "if", TokenKind::If);
define_keyword_parser!(include, "include", TokenKind::Include);
define_token_parser!(l_bracket, "[", TokenKind::LBracket);
define_token_parser!(l_paren, "(", TokenKind::LParen);
define_token_parser!(l_brace, "{", TokenKind::LBrace);
define_keyword_parser!(let_, "let", TokenKind::Let);
define_token_parser!(asterisk, "*", TokenKind::Asterisk);
define_token_parser!(minus, "-", TokenKind::Minus);
define_token_parser!(slash, "/", TokenKind::Slash);
define_token_parser!(ne_eq, "!=", TokenKind::NeEq);
define_keyword_parser!(nodes, "nodes", TokenKind::Nodes);
define_keyword_parser!(none, "None", TokenKind::None);
define_token_parser!(plus, "+", TokenKind::Plus);
define_token_parser!(pipe, "|", TokenKind::Pipe);
define_token_parser!(percent, "%", TokenKind::Percent);
define_token_parser!(question, "?", TokenKind::Question);
define_token_parser!(range_op, "..", TokenKind::RangeOp);
define_token_parser!(r_bracket, "]", TokenKind::RBracket);
define_token_parser!(r_paren, ")", TokenKind::RParen);
define_token_parser!(r_brace, "}", TokenKind::RBrace);
define_keyword_parser!(self_, "self", TokenKind::Self_);
define_token_parser!(semi_colon, ";", TokenKind::SemiColon);
define_keyword_parser!(until, "until", TokenKind::Until);
define_keyword_parser!(while_, "while", TokenKind::While);
define_token_parser!(lt, "<", TokenKind::Lt);
define_token_parser!(lte, "<=", TokenKind::Lte);
define_token_parser!(gt, ">", TokenKind::Gt);
define_token_parser!(gte, ">=", TokenKind::Gte);
define_token_parser!(and, "&&", TokenKind::And);
define_token_parser!(or, "||", TokenKind::Or);
define_token_parser!(not, "!", TokenKind::Not);

fn punctuations(input: Span) -> IResult<Span, Token> {
    alt((
        and, or, l_paren, r_paren, l_brace, r_brace, comma, colon, semi_colon, l_bracket,
        r_bracket, pipe, question,
    ))
    .parse(input)
}

fn binary_op(input: Span) -> IResult<Span, Token> {
    alt((
        eq_eq, ne_eq, lte, gte, lt, gt, equal, plus, minus, asterisk, slash, percent, range_op,
    ))
    .parse(input)
}

fn unary_op(input: Span) -> IResult<Span, Token> {
    alt((not,)).parse(input)
}

fn keywords(input: Span) -> IResult<Span, Token> {
    alt((
        nodes, def, let_, self_, while_, until, if_, elif, else_, end, none, include, foreach, fn_,
        break_, continue_,
    ))
    .parse(input)
}

fn number_literal(input: Span) -> IResult<Span, Token> {
    map_res(
        recognize(pair(
            opt(char('-')),
            recognize((
                opt(alt((char('+'), char('-')))),
                alt((
                    map((digit1, opt(pair(char('.'), digit1))), |_| ()),
                    map((char('.'), digit1), |_| ()),
                )),
                opt((
                    alt((char('e'), char('E'))),
                    opt(alt((char('+'), char('-')))),
                    cut(digit1),
                )),
            )),
        )),
        |span: Span| {
            str::parse(span.fragment()).map(|s| {
                let module_id = span.extra;
                Token {
                    range: span.into(),
                    kind: TokenKind::NumberLiteral(Number::new(s)),
                    module_id,
                }
            })
        },
    )
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
                        hex_escape,
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
            |(text, range)| StringSegment::Text(text, range),
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
                alt((
                    value('\\', char('\\')),
                    value('\"', char('\"')),
                    value('\r', char('r')),
                    value('\n', char('n')),
                    value('\t', char('t')),
                    value('/', char('/')),
                    value('[', char('[')),
                    value(']', char(']')),
                    value('(', char('(')),
                    value(')', char(')')),
                    value('{', char('{')),
                    value('}', char('}')),
                )),
                alt((
                    value('+', char('+')),
                    value('*', char('*')),
                    value('?', char('?')),
                    value('^', char('^')),
                    value('$', char('$')),
                    value('|', char('|')),
                    value('-', char('-')),
                    value('.', char('.')),
                    value('s', char('s')), // \s (whitespace)
                    value('S', char('S')), // \S (non-whitespace)
                    value('d', char('d')), // \d (digit)
                    value('D', char('D')), // \D (non-digit)
                    value('w', char('w')), // \w (word character)
                    value('W', char('W')), // \W (non-word character)
                    hex_escape,
                    unicode,
                )),
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
            kind: TokenKind::StringLiteral(s),
            module_id,
        },
    ))
}

fn literals(input: Span) -> IResult<Span, Token> {
    alt((
        string_literal,
        interpolated_string,
        empty_string,
        number_literal,
    ))
    .parse(input)
}

fn ident(input: Span) -> IResult<Span, Token> {
    map(
        recognize(pair(
            alt((alpha1, tag("_"), tag(MARKDOWN))),
            many0(alt((alphanumeric1, tag("_"), tag("-"), tag("."), tag("*")))),
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
    alt((
        inline_comment,
        keywords,
        env,
        literals,
        punctuations,
        binary_op,
        unary_op,
        ident,
    ))
    .parse(input)
}

fn token_include_spaces(input: Span) -> IResult<Span, Token> {
    alt((
        newline,
        spaces,
        tab,
        inline_comment,
        keywords,
        env,
        literals,
        punctuations,
        binary_op,
        unary_op,
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
    #[case("or(.h1, .**)",
        Options::default(),
        Ok(vec![
          Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 3} }, kind: TokenKind::Ident(CompactString::new("or")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 3}, end: Position {line: 1, column: 4} }, kind: TokenKind::LParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 4}, end: Position {line: 1, column: 7} }, kind: TokenKind::Selector(CompactString::new(".h1")), module_id: 1.into()},
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
    #[case::end_keyword("end",
            Options::default(),
            Ok(vec![
              Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 4} }, kind: TokenKind::End, module_id: 1.into()},
              Token{range: Range { start: Position {line: 1, column: 4}, end: Position {line: 1, column: 4} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case::function_declaration_with_end("fn(): program end",
            Options::default(),
            Ok(vec![
              Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 3} }, kind: TokenKind::Fn, module_id: 1.into()},
              Token{range: Range { start: Position {line: 1, column: 3}, end: Position {line: 1, column: 4} }, kind: TokenKind::LParen, module_id: 1.into()},
              Token{range: Range { start: Position {line: 1, column: 4}, end: Position {line: 1, column: 5} }, kind: TokenKind::RParen, module_id: 1.into()},
              Token{range: Range { start: Position {line: 1, column: 5}, end: Position {line: 1, column: 6} }, kind: TokenKind::Colon, module_id: 1.into()},
              Token{range: Range { start: Position {line: 1, column: 7}, end: Position {line: 1, column: 14} }, kind: TokenKind::Ident(CompactString::new("program")), module_id: 1.into()},
              Token{range: Range { start: Position {line: 1, column: 15}, end: Position {line: 1, column: 18} }, kind: TokenKind::End, module_id: 1.into()},
              Token{range: Range { start: Position {line: 1, column: 18}, end: Position {line: 1, column: 18} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case::eq_eq1("==",
              Options::default(),
              Ok(vec![
                  Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 3} }, kind: TokenKind::EqEq, module_id: 1.into()},
                  Token{range: Range { start: Position {line: 1, column: 3}, end: Position {line: 1, column: 3} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case::eq_eq2("=",
              Options::default(),
              Ok(vec![
                  Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 2} }, kind: TokenKind::Equal, module_id: 1.into()},
                  Token{range: Range { start: Position {line: 1, column: 2}, end: Position {line: 1, column: 2} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case::eq_eq3("===",
              Options::default(),
              Ok(vec![
                  Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 3} }, kind: TokenKind::EqEq, module_id: 1.into()},
                  Token{range: Range { start: Position {line: 1, column: 3}, end: Position {line: 1, column: 4} }, kind: TokenKind::Equal, module_id: 1.into()},
                  Token{range: Range { start: Position {line: 1, column: 4}, end: Position {line: 1, column: 4} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case::eq_eq4("== =",
              Options{include_spaces: true, ignore_errors: false},
              Ok(vec![
                  Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 3} }, kind: TokenKind::EqEq, module_id: 1.into()},
                  Token{range: Range { start: Position {line: 1, column: 3}, end: Position {line: 1, column: 4} }, kind: TokenKind::Whitespace(1), module_id: 1.into()},
                  Token{range: Range { start: Position {line: 1, column: 4}, end: Position {line: 1, column: 5} }, kind: TokenKind::Equal, module_id: 1.into()},
                  Token{range: Range { start: Position {line: 1, column: 5}, end: Position {line: 1, column: 5} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case::eq_eq5("== =",
              Options{include_spaces: false, ignore_errors: false}, // Default options ignore spaces between tokens
              Ok(vec![
                  Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 3} }, kind: TokenKind::EqEq, module_id: 1.into()},
                  Token{range: Range { start: Position {line: 1, column: 4}, end: Position {line: 1, column: 5} }, kind: TokenKind::Equal, module_id: 1.into()},
                  Token{range: Range { start: Position {line: 1, column: 5}, end: Position {line: 1, column: 5} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case::ne_eq1("!=",
              Options::default(),
              Ok(vec![
                  Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 3} }, kind: TokenKind::NeEq, module_id: 1.into()},
                  Token{range: Range { start: Position {line: 1, column: 3}, end: Position {line: 1, column: 3} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case::ne_eq2("!==",
              Options::default(),
              Ok(vec![
                  Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 3} }, kind: TokenKind::NeEq, module_id: 1.into()},
                  Token{range: Range { start: Position {line: 1, column: 3}, end: Position {line: 1, column: 4} }, kind: TokenKind::Equal, module_id: 1.into()},
                  Token{range: Range { start: Position {line: 1, column: 4}, end: Position {line: 1, column: 4} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case::ne_eq3("!= =",
              Options{include_spaces: true, ignore_errors: false},
              Ok(vec![
                  Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 3} }, kind: TokenKind::NeEq, module_id: 1.into()},
                  Token{range: Range { start: Position {line: 1, column: 3}, end: Position {line: 1, column: 4} }, kind: TokenKind::Whitespace(1), module_id: 1.into()},
                  Token{range: Range { start: Position {line: 1, column: 4}, end: Position {line: 1, column: 5} }, kind: TokenKind::Equal, module_id: 1.into()},
                  Token{range: Range { start: Position {line: 1, column: 5}, end: Position {line: 1, column: 5} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case::ne_eq4("!= =",
              Options{include_spaces: false, ignore_errors: false}, // Default options ignore spaces between tokens
              Ok(vec![
                  Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 3} }, kind: TokenKind::NeEq, module_id: 1.into()},
                  Token{range: Range { start: Position {line: 1, column: 4}, end: Position {line: 1, column: 5} }, kind: TokenKind::Equal, module_id: 1.into()},
                  Token{range: Range { start: Position {line: 1, column: 5}, end: Position {line: 1, column: 5} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case("{}",
            Options::default(),
            Ok(vec![
                Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 2} }, kind: TokenKind::LBrace, module_id: 1.into()},
                Token{range: Range { start: Position {line: 1, column: 2}, end: Position {line: 1, column: 3} }, kind: TokenKind::RBrace, module_id: 1.into()},
                Token{range: Range { start: Position {line: 1, column: 3}, end: Position {line: 1, column: 3} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case(" { } ",
            Options::default(),
            Ok(vec![
                Token{range: Range { start: Position {line: 1, column: 2}, end: Position {line: 1, column: 3} }, kind: TokenKind::LBrace, module_id: 1.into()},
                Token{range: Range { start: Position {line: 1, column: 4}, end: Position {line: 1, column: 5} }, kind: TokenKind::RBrace, module_id: 1.into()},
                Token{range: Range { start: Position {line: 1, column: 6}, end: Position {line: 1, column: 6} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case("{key: value}", // Adjusted to match LBrace/RBrace being {{ and }}
            Options::default(),
            Ok(vec![
                Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 2} }, kind: TokenKind::LBrace, module_id: 1.into()},
                Token{range: Range { start: Position {line: 1, column: 2}, end: Position {line: 1, column: 5} }, kind: TokenKind::Ident(CompactString::new("key")), module_id: 1.into()},
                Token{range: Range { start: Position {line: 1, column: 5}, end: Position {line: 1, column: 6} }, kind: TokenKind::Colon, module_id: 1.into()},
                Token{range: Range { start: Position {line: 1, column: 7}, end: Position {line: 1, column: 12} }, kind: TokenKind::Ident(CompactString::new("value")), module_id: 1.into()},
                Token{range: Range { start: Position {line: 1, column: 12}, end: Position {line: 1, column: 13} }, kind: TokenKind::RBrace, module_id: 1.into()},
                Token{range: Range { start: Position {line: 1, column: 13}, end: Position {line: 1, column: 13} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case::selector_with_dot_h_text(".h.text",
            Options::default(),
            Ok(vec![
                    Token {
                        range: Range { start: Position { line: 1, column: 1 }, end: Position { line: 1, column: 8 } },
                        kind: TokenKind::Selector(CompactString::new(".h.text")),
                        module_id: 1.into(),
                    },
                    Token {
                        range: Range { start: Position { line: 1, column: 8 }, end: Position { line: 1, column: 8 } },
                        kind: TokenKind::Eof,
                        module_id: 1.into(),
                    }
                ])
            )]
    #[case::hex_escape_sequence("print(\"\\x1b[2J\\x1b[H\")",
            Options::default(),
            Ok(vec![
                    Token {
                        range: Range { start: Position { line: 1, column: 1 }, end: Position { line: 1, column: 6 } },
                        kind: TokenKind::Ident(CompactString::new("print")),
                        module_id: 1.into(),
                    },
                    Token {
                        range: Range { start: Position { line: 1, column: 6 }, end: Position { line: 1, column: 7 } },
                        kind: TokenKind::LParen,
                        module_id: 1.into(),
                    },
                    Token {
                        range: Range { start: Position { line: 1, column: 7 }, end: Position { line: 1, column: 22 } },
                        kind: TokenKind::StringLiteral("\x1b[2J\x1b[H".to_string()),
                        module_id: 1.into(),
                    },
                    Token {
                        range: Range { start: Position { line: 1, column: 22 }, end: Position { line: 1, column: 23 } },
                        kind: TokenKind::RParen,
                        module_id: 1.into(),
                    },
                    Token {
                        range: Range { start: Position { line: 1, column: 23 }, end: Position { line: 1, column: 23 } },
                        kind: TokenKind::Eof,
                        module_id: 1.into(),
                    }
                ])
            )]
    #[case::keyword_boundary_def("definition",
        Options::default(),
        Ok(vec![
            Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 11} }, kind: TokenKind::Ident(CompactString::new("definition")), module_id: 1.into()},
            Token{range: Range { start: Position {line: 1, column: 11}, end: Position {line: 1, column: 11} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case::keyword_boundary_end("ending",
        Options::default(),
        Ok(vec![
            Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 7} }, kind: TokenKind::Ident(CompactString::new("ending")), module_id: 1.into()},
            Token{range: Range { start: Position {line: 1, column: 7}, end: Position {line: 1, column: 7} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case::keyword_boundary_if("ifconfig",
        Options::default(),
        Ok(vec![
            Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 9} }, kind: TokenKind::Ident(CompactString::new("ifconfig")), module_id: 1.into()},
            Token{range: Range { start: Position {line: 1, column: 9}, end: Position {line: 1, column: 9} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case::keyword_proper_def("def ",
        Options::default(),
        Ok(vec![
            Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 4} }, kind: TokenKind::Def, module_id: 1.into()},
            Token{range: Range { start: Position {line: 1, column: 5}, end: Position {line: 1, column: 5} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case::keyword_proper_end("end ",
        Options::default(),
        Ok(vec![
            Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 4} }, kind: TokenKind::End, module_id: 1.into()},
            Token{range: Range { start: Position {line: 1, column: 5}, end: Position {line: 1, column: 5} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case::number_regex("\"^(-?(?:0|[1-9]\\\\d*)(?:\\\\.\\\\d+)?(?:[eE][+-]?\\\\d+)?)\"",
        Options::default(),
        Ok(vec![
            Token {
                range: Range { start: Position { line: 1, column: 1 }, end: Position { line: 1, column: 53 } },
                kind: TokenKind::StringLiteral("^(-?(?:0|[1-9]\\d*)(?:\\.\\d+)?(?:[eE][+-]?\\d+)?)".to_string()),
                module_id: 1.into(),
            },
            Token {
                range: Range { start: Position { line: 1, column: 53 }, end: Position { line: 1, column: 53 } },
                kind: TokenKind::Eof,
                module_id: 1.into(),
            }
        ])
    )]
    #[case::regex_with_brackets("\"[a-zA-Z0-9]+\"",
        Options::default(),
        Ok(vec![
            Token {
                range: Range { start: Position { line: 1, column: 1 }, end: Position { line: 1, column: 15 } },
                kind: TokenKind::StringLiteral("[a-zA-Z0-9]+".to_string()),
                module_id: 1.into(),
            },
            Token {
                range: Range { start: Position { line: 1, column: 15 }, end: Position { line: 1, column: 15 } },
                kind: TokenKind::Eof,
                module_id: 1.into(),
            }
        ])
    )]
    #[case::regex_with_escaped_chars("\"\\\\[\\\\(\\\\)\\\\{\\\\}\\\\+\\\\*\\\\?\\\\^\\\\$\\\\|\"",
        Options::default(),
        Ok(vec![
            Token {
                range: Range { start: Position { line: 1, column: 1 }, end: Position { line: 1, column: 36 } },
                kind: TokenKind::StringLiteral("\\[\\(\\)\\{\\}\\+\\*\\?\\^\\$\\|".to_string()),
                module_id: 1.into(),
            },
            Token {
                range: Range { start: Position { line: 1, column: 36 }, end: Position { line: 1, column: 36 } },
                kind: TokenKind::Eof,
                module_id: 1.into(),
            }
        ])
    )]
    #[case::regex_character_classes("\"\\s\\S\\d\\D\\w\\W\"",
        Options::default(),
        Ok(vec![
            Token {
                range: Range { start: Position { line: 1, column: 1 }, end: Position { line: 1, column: 15 } },
                kind: TokenKind::StringLiteral("sSdDwW".to_string()),
                module_id: 1.into(),
            },
            Token {
                range: Range { start: Position { line: 1, column: 15 }, end: Position { line: 1, column: 15 } },
                kind: TokenKind::Eof,
                module_id: 1.into(),
            }
        ])
    )]
    #[case::regex_mixed_with_character_classes("\"[a-z]\\d+\\s*\"",
        Options::default(),
        Ok(vec![
            Token {
                range: Range { start: Position { line: 1, column: 1 }, end: Position { line: 1, column: 14 } },
                kind: TokenKind::StringLiteral("[a-z]d+s*".to_string()),
                module_id: 1.into(),
            },
            Token {
                range: Range { start: Position { line: 1, column: 14 }, end: Position { line: 1, column: 14 } },
                kind: TokenKind::Eof,
                module_id: 1.into(),
            }
        ])
    )]
    #[case::pipe_with_comment("| \"test\" # comment",
        Options::default(),
        Ok(vec![
            Token {
                range: Range { start: Position { line: 1, column: 1 }, end: Position { line: 1, column: 2 } },
                kind: TokenKind::Pipe,
                module_id: 1.into(),
            },
            Token {
                range: Range { start: Position { line: 1, column: 3 }, end: Position { line: 1, column: 9 } },
                kind: TokenKind::StringLiteral("test".to_string()),
                module_id: 1.into(),
            },
            Token {
                range: Range { start: Position { line: 1, column: 11 }, end: Position { line: 1, column: 19 } },
                kind: TokenKind::Comment(" comment".to_string()),
                module_id: 1.into(),
            },
            Token {
                range: Range { start: Position { line: 1, column: 19 }, end: Position { line: 1, column: 19 } },
                kind: TokenKind::Eof,
                module_id: 1.into(),
            }
        ])
    )]
    #[case::comment_with_pipe_character("# comment with | pipe",
        Options::default(),
        Ok(vec![
            Token {
                range: Range { start: Position { line: 1, column: 2 }, end: Position { line: 1, column: 22 } },
                kind: TokenKind::Comment(" comment with | pipe".to_string()),
                module_id: 1.into(),
            },
            Token {
                range: Range { start: Position { line: 1, column: 22 }, end: Position { line: 1, column: 22 } },
                kind: TokenKind::Eof,
                module_id: 1.into(),
            }
        ])
    )]
    fn test_parse(
        #[case] input: &str,
        #[case] options: Options,
        #[case] expected: Result<Vec<Token>, LexerError>,
    ) {
        assert_eq!(Lexer::new(options).tokenize(input, 1.into()), expected);
    }
}
