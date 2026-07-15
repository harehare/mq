pub mod token;

use nom::Parser;
use nom::bytes::complete::{is_not, take_until, take_while1};
use nom::character::complete::{digit1, line_ending};
use nom::combinator::{cut, opt};
use nom::{
    IResult,
    branch::alt,
    bytes::complete::{escaped_transform, tag, take_while_m_n},
    character::complete::{alpha1, alphanumeric1, anychar, char, multispace0, none_of, satisfy},
    combinator::{map, map_opt, map_res, recognize, value},
    multi::{many0, many1},
    sequence::{delimited, pair, preceded},
};
use nom_locate::{LocatedSpan, position};
use smol_str::SmolStr;
use token::{StringSegment, Token, TokenKind};

use crate::error::syntax::SyntaxError;
use crate::module::ModuleId;
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

    pub fn tokenize(&self, input: &str, module_id: ModuleId) -> Result<Vec<Token>, SyntaxError> {
        match tokens(Span::new_extra(input, module_id), &self.options) {
            Ok((span, mut tokens)) => {
                let eof: Range = span.into();

                if eof.start == eof.end || self.options.ignore_errors {
                    tokens.push(Token {
                        range: eof,
                        kind: TokenKind::Eof,
                        module_id,
                    });
                    Ok(tokens)
                } else {
                    Err(SyntaxError::UnexpectedToken(Token {
                        range: eof,
                        kind: TokenKind::Eof,
                        module_id,
                    }))
                }
            }
            Err(nom::Err::Error(e)) | Err(nom::Err::Failure(e)) => Err(SyntaxError::UnexpectedToken(Token {
                range: e.input.into(),
                kind: TokenKind::Eof,
                module_id,
            })),
            Err(_) => Err(SyntaxError::UnexpectedToken(Token {
                range: Range::default(),
                kind: TokenKind::Eof,
                module_id,
            })),
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

/// Parses a 4-digit Unicode escape sequence `\uXXXX`.
fn unicode4(input: Span) -> IResult<Span, char> {
    map_opt(
        map_res(
            preceded(char('u'), take_while_m_n(4, 4, |c: char| c.is_ascii_hexdigit())),
            |span: Span| u32::from_str_radix(span.fragment(), 16),
        ),
        char::from_u32,
    )
    .parse(input)
}

fn hex_escape(input: Span) -> IResult<Span, char> {
    map_opt(
        map_res(
            preceded(char('x'), take_while_m_n(2, 2, |c: char| c.is_ascii_hexdigit())),
            |span: Span| u8::from_str_radix(span.fragment(), 16),
        ),
        |byte| char::from_u32(byte as u32),
    )
    .parse(input)
}

fn inline_comment(input: Span) -> IResult<Span, Token> {
    let (span, _) = char('#')(input)?;
    let (span, start) = position(span)?;
    let (span, comment_text) = opt(is_not("\n\r")).parse(span)?;
    let (span, end) = position(span)?;

    let module_id = start.extra;
    let comment_str = comment_text.map(|s: Span| s.fragment().to_string()).unwrap_or_default();

    Ok((
        span,
        Token {
            range: Range {
                start: start.into(),
                end: end.into(),
            },
            kind: TokenKind::Comment(comment_str),
            module_id,
        },
    ))
}

/// Skips a `# ...` comment without allocating a String.
fn skip_comment(input: Span) -> IResult<Span, ()> {
    let (span, _) = char('#')(input)?;
    let (span, _) = opt(is_not("\n\r")).parse(span)?;
    Ok((span, ()))
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
    map(take_while1(|c| c == '\t'), |span: Span| {
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
    map(take_while1(|c| c == ' '), |span: Span| {
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
define_token_parser!(double_colon, "::", TokenKind::DoubleColon);
define_token_parser!(empty_string, "\"\"", TokenKind::StringLiteral(String::new()));
define_token_parser!(eq_eq, "==", TokenKind::EqEq);
define_token_parser!(equal, "=", TokenKind::Equal);
define_token_parser!(l_bracket, "[", TokenKind::LBracket);
define_token_parser!(l_paren, "(", TokenKind::LParen);
define_token_parser!(l_brace, "{", TokenKind::LBrace);
define_token_parser!(asterisk, "*", TokenKind::Asterisk);
define_token_parser!(minus, "-", TokenKind::Minus);
define_token_parser!(slash, "/", TokenKind::Slash);
define_token_parser!(ne_eq, "!=", TokenKind::NeEq);
define_token_parser!(plus, "+", TokenKind::Plus);
define_token_parser!(pipe, "|", TokenKind::Pipe);
define_token_parser!(percent, "%", TokenKind::Percent);
define_token_parser!(spread_op, "...", TokenKind::DotDotDot);
define_token_parser!(range_op, "..", TokenKind::DoubleDot);
define_token_parser!(r_bracket, "]", TokenKind::RBracket);
define_token_parser!(r_paren, ")", TokenKind::RParen);
define_token_parser!(r_brace, "}", TokenKind::RBrace);
define_token_parser!(semi_colon, ";", TokenKind::SemiColon);
define_token_parser!(lt, "<", TokenKind::Lt);
define_token_parser!(lte, "<=", TokenKind::Lte);
define_token_parser!(gt, ">", TokenKind::Gt);
define_token_parser!(gte, ">=", TokenKind::Gte);
define_token_parser!(and, "&&", TokenKind::And);
define_token_parser!(or, "||", TokenKind::Or);
define_token_parser!(not, "!", TokenKind::Not);
define_token_parser!(question, "?", TokenKind::Question);
define_token_parser!(coalesce, "??", TokenKind::Coalesce);
define_token_parser!(plus_equal, "+=", TokenKind::PlusEqual);
define_token_parser!(minus_equal, "-=", TokenKind::MinusEqual);
define_token_parser!(star_equal, "*=", TokenKind::StarEqual);
define_token_parser!(slash_equal, "/=", TokenKind::SlashEqual);
define_token_parser!(percent_equal, "%=", TokenKind::PercentEqual);
define_token_parser!(double_slash_equal, "//=", TokenKind::DoubleSlashEqual);
define_token_parser!(pipe_equal, "|=", TokenKind::PipeEqual);
define_token_parser!(tilde_equal, "=~", TokenKind::TildeEqual);
define_token_parser!(not_tilde_equal, "!~", TokenKind::NotTildeEqual);
define_token_parser!(left_shift, "<<", TokenKind::LeftShift);
define_token_parser!(right_shift, ">>", TokenKind::RightShift);
define_token_parser!(convert_op, "@", TokenKind::Convert);
define_token_parser!(arrow, "->", TokenKind::Arrow);

fn punctuations(input: Span) -> IResult<Span, Token> {
    alt((
        and,
        or,
        l_paren,
        r_paren,
        l_brace,
        r_brace,
        comma,
        double_colon,
        colon,
        semi_colon,
        l_bracket,
        r_bracket,
        coalesce,
        question,
        pipe,
    ))
    .parse(input)
}

fn lambda_op(input: Span) -> IResult<Span, Token> {
    alt((arrow,)).parse(input)
}

fn assignment_op(input: Span) -> IResult<Span, Token> {
    alt((
        plus_equal,
        minus_equal,
        star_equal,
        slash_equal,
        percent_equal,
        double_slash_equal,
        pipe_equal,
    ))
    .parse(input)
}

fn binary_op(input: Span) -> IResult<Span, Token> {
    alt((
        convert_op,
        assignment_op,
        eq_eq,
        ne_eq,
        left_shift,
        right_shift,
        tilde_equal,
        not_tilde_equal,
        lte,
        gte,
        lt,
        gt,
        equal,
        plus,
        minus,
        asterisk,
        slash,
        percent,
        spread_op,
        range_op,
    ))
    .parse(input)
}

fn unary_op(input: Span) -> IResult<Span, Token> {
    alt((not,)).parse(input)
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

fn interpolation_expr(input: Span) -> IResult<Span, Span> {
    delimited(tag("${"), take_until("}"), char('}')).parse(input)
}

fn string_segment<'a>(input: Span<'a>) -> IResult<Span<'a>, StringSegment> {
    alt((
        map(
            |input: Span<'a>| {
                let (span, start) = position(input)?;
                let (span, expr) = interpolation_expr(span)?;
                let (span, end) = position(span)?;
                Ok((
                    span,
                    (
                        expr,
                        Range {
                            start: start.into(),
                            end: end.into(),
                        },
                    ),
                ))
            },
            |(expr, range)| StringSegment::Expr(expr.to_string().into(), range),
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
                        value('{', char('{')),
                        value('}', char('}')),
                        hex_escape,
                        unicode,
                        unicode4,
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

fn byte_escape_seq(input: Span) -> IResult<Span, u8> {
    preceded(
        char('\\'),
        alt((
            preceded(
                char('x'),
                map_res(take_while_m_n(2, 2, |c: char| c.is_ascii_hexdigit()), |hex: Span| {
                    u8::from_str_radix(hex.fragment(), 16)
                }),
            ),
            value(b'\\', char('\\')),
            value(b'"', char('"')),
            value(b'\n', char('n')),
            value(b'\r', char('r')),
            value(b'\t', char('t')),
            value(b'\0', char('0')),
        )),
    )
    .parse(input)
}

fn byte_string_literal(input: Span) -> IResult<Span, Token> {
    let (span, start) = position(input)?;
    let (span, _) = tag("b\"")(span)?;

    let (span, byte_segments) = many0(alt((
        map(byte_escape_seq, |b| vec![b]),
        // Only plain ASCII characters are allowed unescaped; non-ASCII must
        // use \xNN escapes to avoid silent UTF-8 multi-byte encoding.
        map(satisfy(|c: char| c.is_ascii() && c != '"' && c != '\\'), |c| {
            vec![c as u8]
        }),
    )))
    .parse(span)?;

    let (span, _) = char('"').parse(span)?;
    let (span, end) = position(span)?;
    let bytes: Vec<u8> = byte_segments.into_iter().flatten().collect();

    Ok((
        span,
        Token {
            range: Range {
                start: start.into(),
                end: end.into(),
            },
            kind: TokenKind::BytesLiteral(bytes),
            module_id: start.extra,
        },
    ))
}

fn interpolated_string(input: Span) -> IResult<Span, Token> {
    let (span, start) = position(input)?;
    let (span, _) = tag("s\"")(span)?;

    let mut segments = Vec::with_capacity(4);
    let mut current = span;

    // Parse at least one segment
    let (remaining, segment) = string_segment(current)?;
    segments.push(segment);
    current = remaining;

    // Parse remaining segments
    while let Ok((remaining, segment)) = string_segment(current) {
        segments.push(segment);
        current = remaining;
    }

    let (span, _) = char('"')(current)?;
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

/// Parses `input` as an interpolated string body (like between `s"` and `"`) without requiring
/// the surrounding quotes, e.g. for DAP logpoint messages.
pub(crate) fn parse_interpolation_segments(
    input: &str,
    module_id: ModuleId,
) -> Result<Vec<StringSegment>, SyntaxError> {
    if input.is_empty() {
        return Ok(Vec::new());
    }

    let mut segments = Vec::with_capacity(4);
    let mut current = Span::new_extra(input, module_id);

    // escaped_transform matches zero-length on empty input, so stop at EOF ourselves.
    while !current.fragment().is_empty() {
        match string_segment(current) {
            Ok((remaining, segment)) => {
                segments.push(segment);
                current = remaining;
            }
            Err(_) => break,
        }
    }

    if current.fragment().is_empty() {
        Ok(segments)
    } else {
        Err(SyntaxError::UnexpectedToken(Token {
            range: current.into(),
            kind: TokenKind::Eof,
            module_id,
        }))
    }
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
                    unicode4,
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
        byte_string_literal,
        string_literal,
        interpolated_string,
        empty_string,
        number_literal,
    ))
    .parse(input)
}

/// Parses a selector token starting with `.`.
///
/// Handles both regular selectors (`.h`, `.p`, `.**`) and special-character
/// selectors that cannot be parsed as identifiers, such as `.>` (blockquote)
/// and `.^` (footnote).
fn selector(input: Span) -> IResult<Span, Token> {
    map(
        recognize(pair(
            tag(MARKDOWN),
            alt((
                tag(">"),
                tag("^"),
                // Quoted property selector: ."key" or ."key with spaces"
                recognize(pair(
                    char('"'),
                    pair(
                        many0(alt((recognize(pair(char('\\'), anychar)), recognize(none_of("\"\\"))))),
                        char('"'),
                    ),
                )),
                recognize(many0(alt((alphanumeric1, tag("_"), tag("-"), tag("*"))))),
            )),
        )),
        |span: Span| {
            let module_id = span.extra;
            Token {
                range: span.into(),
                kind: TokenKind::Selector(SmolStr::new(span.fragment())),
                module_id,
            }
        },
    )
    .parse(input)
}

/// Parses an identifier or keyword in a single pass.
///
/// The ASCII base `[A-Za-z0-9_]+` is parsed first. Keywords are only matched
/// at a word boundary (next char is not alphanumeric and not `_`). When the
/// next char after the base is `-` or `*`, a second parse extends the span to
/// cover the full identifier; otherwise `base_span` is used directly, avoiding
/// a redundant re-parse.
fn ident_or_keyword(input: Span) -> IResult<Span, Token> {
    let (after_base, base_span) =
        recognize(pair(alt((alpha1, tag("_"))), many0(alt((alphanumeric1, tag("_")))))).parse(input)?;

    let module_id = base_span.extra;
    let base_frag = *base_span.fragment();

    let next_char = after_base.fragment().chars().next();
    // A word boundary means the identifier cannot be extended by an alphanumeric
    // or underscore character (including non-ASCII Unicode letters/digits).
    let at_word_boundary = next_char.map(|c| !c.is_alphanumeric() && c != '_').unwrap_or(true);

    if at_word_boundary {
        let keyword_kind = match base_frag {
            "as" => Some(TokenKind::As),
            "break" => Some(TokenKind::Break),
            "catch" => Some(TokenKind::Catch),
            "continue" => Some(TokenKind::Continue),
            "def" => Some(TokenKind::Def),
            "do" => Some(TokenKind::Do),
            "elif" => Some(TokenKind::Elif),
            "else" => Some(TokenKind::Else),
            "end" => Some(TokenKind::End),
            "fn" => Some(TokenKind::Fn),
            "foreach" => Some(TokenKind::Foreach),
            "if" => Some(TokenKind::If),
            "import" => Some(TokenKind::Import),
            "include" => Some(TokenKind::Include),
            "let" => Some(TokenKind::Let),
            "loop" => Some(TokenKind::Loop),
            "macro" => Some(TokenKind::Macro),
            "match" => Some(TokenKind::Match),
            "module" => Some(TokenKind::Module),
            "nodes" => Some(TokenKind::Nodes),
            "None" => Some(TokenKind::None),
            "quote" => Some(TokenKind::Quote),
            "self" => Some(TokenKind::Self_),
            "try" => Some(TokenKind::Try),
            "unquote" => Some(TokenKind::Unquote),
            "var" => Some(TokenKind::Var),
            "while" => Some(TokenKind::While),
            _ => None,
        };

        if let Some(kind) = keyword_kind {
            return Ok((
                after_base,
                Token {
                    range: base_span.into(),
                    kind,
                    module_id,
                },
            ));
        }
    }

    // When the next character can extend the identifier (`-` or `*`), re-parse
    // from the original input to capture the full span. Otherwise `base_span`
    // already covers the complete identifier, so we reuse it directly.
    if next_char == Some('-') || next_char == Some('*') {
        let (after_full, full_span) = recognize(pair(
            alt((alpha1, tag("_"))),
            many0(alt((alphanumeric1, tag("_"), tag("-"), tag("*")))),
        ))
        .parse(input)?;

        let full_frag = *full_span.fragment();
        let kind = match full_frag {
            "true" => TokenKind::BoolLiteral(true),
            "false" => TokenKind::BoolLiteral(false),
            s => TokenKind::Ident(SmolStr::new(s)),
        };

        return Ok((
            after_full,
            Token {
                range: full_span.into(),
                kind,
                module_id: full_span.extra,
            },
        ));
    }

    let kind = match base_frag {
        "true" => TokenKind::BoolLiteral(true),
        "false" => TokenKind::BoolLiteral(false),
        s => TokenKind::Ident(SmolStr::new(s)),
    };

    Ok((
        after_base,
        Token {
            range: base_span.into(),
            kind,
            module_id,
        },
    ))
}

fn env(input: Span) -> IResult<Span, Token> {
    preceded(
        tag("$"),
        map(recognize(many1(alt((alphanumeric1, tag("_"))))), |span: Span| {
            let kind = TokenKind::Env(SmolStr::new(span.fragment()));
            let module_id = span.extra;
            Token {
                range: span.into(),
                kind,
                module_id,
            }
        }),
    )
    .parse(input)
}

fn skip_whitespace_and_comments(input: Span) -> IResult<Span, ()> {
    let mut current = input;
    loop {
        let (remaining, _) = multispace0(current)?;
        if let Ok((after_comment, ())) = skip_comment(remaining) {
            current = after_comment;
        } else {
            return Ok((remaining, ()));
        }
    }
}

fn token(input: Span) -> IResult<Span, Token> {
    alt((
        env,
        literals,
        lambda_op,
        binary_op,
        punctuations,
        unary_op,
        selector,
        ident_or_keyword,
    ))
    .parse(input)
}

fn token_include_spaces(input: Span) -> IResult<Span, Token> {
    alt((
        newline,
        spaces,
        tab,
        inline_comment,
        env,
        literals,
        lambda_op,
        binary_op,
        punctuations,
        unary_op,
        selector,
        ident_or_keyword,
    ))
    .parse(input)
}

fn tokens<'a>(input: Span<'a>, options: &'a Options) -> IResult<Span<'a>, Vec<Token>> {
    let estimated_capacity = input.fragment().len() / 5;
    let mut tokens = Vec::with_capacity(estimated_capacity.max(16));
    let mut current = input;

    if options.include_spaces {
        while let Ok((remaining, token)) = token_include_spaces(current) {
            tokens.push(token);
            current = remaining;
        }
    } else {
        loop {
            let (remaining, _) = skip_whitespace_and_comments(current)?;
            match token(remaining) {
                Ok((remaining, tok)) => {
                    tokens.push(tok);
                    current = remaining;
                }
                Err(_) => {
                    current = remaining;
                    break;
                }
            }
        }
    }

    Ok((current, tokens))
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
          Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 4} }, kind: TokenKind::Ident(SmolStr::new("and")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 4}, end: Position {line: 1, column: 5} }, kind: TokenKind::LParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 5}, end: Position {line: 1, column: 13} }, kind: TokenKind::Ident(SmolStr::new("contains")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 13}, end: Position {line: 1, column: 14} }, kind: TokenKind::LParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 14}, end: Position {line: 1, column: 20} }, kind: TokenKind::StringLiteral("test".to_string()), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 20}, end: Position {line: 1, column: 21} }, kind: TokenKind::RParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 21}, end: Position {line: 1, column: 22} }, kind: TokenKind::RParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 22}, end: Position {line: 1, column: 22} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case("and(contains(\"test\")) | or(endswith(\"test\"))",
        Options::default(),
        Ok(vec![
          Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 4} }, kind: TokenKind::Ident(SmolStr::new("and")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 4}, end: Position {line: 1, column: 5} }, kind: TokenKind::LParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 5}, end: Position {line: 1, column: 13} }, kind: TokenKind::Ident(SmolStr::new("contains")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 13}, end: Position {line: 1, column: 14} }, kind: TokenKind::LParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 14}, end: Position {line: 1, column: 20} }, kind: TokenKind::StringLiteral("test".to_string()), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 20}, end: Position {line: 1, column: 21} }, kind: TokenKind::RParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 21}, end: Position {line: 1, column: 22} }, kind: TokenKind::RParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 23}, end: Position {line: 1, column: 24} }, kind: TokenKind::Pipe, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 25}, end: Position {line: 1, column: 27} }, kind: TokenKind::Ident(SmolStr::new("or")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 27}, end: Position {line: 1, column: 28} }, kind: TokenKind::LParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 28}, end: Position {line: 1, column: 36} }, kind: TokenKind::Ident(SmolStr::new("endswith")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 36}, end: Position {line: 1, column: 37} }, kind: TokenKind::LParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 37}, end: Position {line: 1, column: 43} }, kind: TokenKind::StringLiteral("test".to_string()), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 43}, end: Position {line: 1, column: 44} }, kind: TokenKind::RParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 44}, end: Position {line: 1, column: 45} }, kind: TokenKind::RParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 45}, end: Position {line: 1, column: 45} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case("eq(length(), 10)",
        Options::default(),
        Ok(vec![
          Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 3} }, kind: TokenKind::Ident(SmolStr::new("eq")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 3}, end: Position {line: 1, column: 4} }, kind: TokenKind::LParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 4}, end: Position {line: 1, column: 10} }, kind: TokenKind::Ident(SmolStr::new("length")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 10}, end: Position {line: 1, column: 11} }, kind: TokenKind::LParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 11}, end: Position {line: 1, column: 12} }, kind: TokenKind::RParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 12}, end: Position {line: 1, column: 13} }, kind: TokenKind::Comma, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 14}, end: Position {line: 1, column: 16} }, kind: TokenKind::NumberLiteral(10.into()), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 16}, end: Position {line: 1, column: 17} }, kind: TokenKind::RParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 17}, end: Position {line: 1, column: 17} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case("or(.h1, .**)",
        Options::default(),
        Ok(vec![
          Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 3} }, kind: TokenKind::Ident(SmolStr::new("or")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 3}, end: Position {line: 1, column: 4} }, kind: TokenKind::LParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 4}, end: Position {line: 1, column: 7} }, kind: TokenKind::Selector(SmolStr::new(".h1")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 7}, end: Position {line: 1, column: 8} }, kind: TokenKind::Comma, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 9}, end: Position {line: 1, column: 12} }, kind: TokenKind::Selector(SmolStr::new(".**")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 12}, end: Position {line: 1, column: 13} }, kind: TokenKind::RParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 13}, end: Position {line: 1, column: 13} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case("or(.[][], .[])",
        Options::default(),
        Ok(vec![
          Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 3} }, kind: TokenKind::Ident(SmolStr::new("or")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 3}, end: Position {line: 1, column: 4} }, kind: TokenKind::LParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 4}, end: Position {line: 1, column: 5} }, kind: TokenKind::Selector(SmolStr::new(".")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 5}, end: Position {line: 1, column: 6} }, kind: TokenKind::LBracket, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 6}, end: Position {line: 1, column: 7} }, kind: TokenKind::RBracket, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 7}, end: Position {line: 1, column: 8} }, kind: TokenKind::LBracket, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 8}, end: Position {line: 1, column: 9} }, kind: TokenKind::RBracket, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 9}, end: Position {line: 1, column: 10} }, kind: TokenKind::Comma, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 11}, end: Position {line: 1, column: 12} }, kind: TokenKind::Selector(SmolStr::new(".")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 12}, end: Position {line: 1, column: 13} }, kind: TokenKind::LBracket, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 13}, end: Position {line: 1, column: 14} }, kind: TokenKind::RBracket, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 14}, end: Position {line: 1, column: 15} }, kind: TokenKind::RParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 15}, end: Position {line: 1, column: 15} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case("startswith(\"\\u{0061}\")",
        Options::default(),
        Ok(vec![
          Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 11} }, kind: TokenKind::Ident(SmolStr::new("startswith")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 11}, end: Position {line: 1, column: 12} }, kind: TokenKind::LParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 12}, end: Position {line: 1, column: 22} }, kind: TokenKind::StringLiteral("a".to_string()), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 22}, end: Position {line: 1, column: 23} }, kind: TokenKind::RParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 23}, end: Position {line: 1, column: 23} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case("endswith($ENV)",
        Options::default(),
        Ok(vec![
          Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 9} }, kind: TokenKind::Ident(SmolStr::new("endswith")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 9}, end: Position {line: 1, column: 10} }, kind: TokenKind::LParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 11}, end: Position {line: 1, column: 14} }, kind: TokenKind::Env(SmolStr::new("ENV")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 14}, end: Position {line: 1, column: 15} }, kind: TokenKind::RParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 15}, end: Position {line: 1, column: 15} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case("def check(arg1, arg2): startswith(\"\\u{0061}\")",
        Options::default(),
        Ok(vec![
          Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 4} }, kind: TokenKind::Def, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 5}, end: Position {line: 1, column: 10} }, kind: TokenKind::Ident(SmolStr::new("check")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 10}, end: Position {line: 1, column: 11} }, kind: TokenKind::LParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 11}, end: Position {line: 1, column: 15} }, kind: TokenKind::Ident(SmolStr::new("arg1")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 15}, end: Position {line: 1, column: 16} }, kind: TokenKind::Comma, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 17}, end: Position {line: 1, column: 21} }, kind: TokenKind::Ident(SmolStr::new("arg2")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 21}, end: Position {line: 1, column: 22} }, kind: TokenKind::RParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 22}, end: Position {line: 1, column: 23} }, kind: TokenKind::Colon, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 24}, end: Position {line: 1, column: 34} }, kind: TokenKind::Ident(SmolStr::new("startswith")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 34}, end: Position {line: 1, column: 35} }, kind: TokenKind::LParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 35}, end: Position {line: 1, column: 45} }, kind: TokenKind::StringLiteral("a".to_string()), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 45}, end: Position {line: 1, column: 46} }, kind: TokenKind::RParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 46}, end: Position {line: 1, column: 46} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case("\"test",
          Options::default(),
          Err(SyntaxError::UnexpectedToken(Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 6} }, kind: TokenKind::Eof, module_id: 1.into()})))]
    #[case::new_line("and(\ncontains(\"test\"))",
            Options{include_spaces: true, ignore_errors: true},
            Ok(vec![
              Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 4} }, kind: TokenKind::Ident(SmolStr::new("and")), module_id: 1.into()},
              Token{range: Range { start: Position {line: 1, column: 4}, end: Position {line: 1, column: 5} }, kind: TokenKind::LParen, module_id: 1.into()},
              Token{range: Range { start: Position {line: 1, column: 5}, end: Position {line: 1, column: 6} }, kind: TokenKind::NewLine, module_id: 1.into()},
              Token{range: Range { start: Position {line: 2, column: 1}, end: Position {line: 2, column: 9} }, kind: TokenKind::Ident(SmolStr::new("contains")), module_id: 1.into()},
              Token{range: Range { start: Position {line: 2, column: 9}, end: Position {line: 2, column: 10} }, kind: TokenKind::LParen, module_id: 1.into()},
              Token{range: Range { start: Position {line: 2, column: 10}, end: Position {line: 2, column: 16} }, kind: TokenKind::StringLiteral("test".to_string()), module_id: 1.into()},
              Token{range: Range { start: Position {line: 2, column: 16}, end: Position {line: 2, column: 17} }, kind: TokenKind::RParen, module_id: 1.into()},
              Token{range: Range { start: Position {line: 2, column: 17}, end: Position {line: 2, column: 18} }, kind: TokenKind::RParen, module_id: 1.into()},
              Token{range: Range { start: Position {line: 2, column: 18}, end: Position {line: 2, column: 18} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case("and(\ncontains(\"test\")) | or(\nendswith(\"test\"))",
            Options{include_spaces: true, ignore_errors: true},
            Ok(vec![
              Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 4} }, kind: TokenKind::Ident(SmolStr::new("and")), module_id: 1.into()},
              Token{range: Range { start: Position {line: 1, column: 4}, end: Position {line: 1, column: 5} }, kind: TokenKind::LParen, module_id: 1.into()},
              Token{range: Range { start: Position {line: 1, column: 5}, end: Position {line: 1, column: 6} }, kind: TokenKind::NewLine, module_id: 1.into()},
              Token{range: Range { start: Position {line: 2, column: 1}, end: Position {line: 2, column: 9} }, kind: TokenKind::Ident(SmolStr::new("contains")), module_id: 1.into()},
              Token{range: Range { start: Position {line: 2, column: 9}, end: Position {line: 2, column: 10} }, kind: TokenKind::LParen, module_id: 1.into()},
              Token{range: Range { start: Position {line: 2, column: 10}, end: Position {line: 2, column: 16} }, kind: TokenKind::StringLiteral("test".to_string()), module_id: 1.into()},
              Token{range: Range { start: Position {line: 2, column: 16}, end: Position {line: 2, column: 17} }, kind: TokenKind::RParen, module_id: 1.into()},
              Token{range: Range { start: Position {line: 2, column: 17}, end: Position {line: 2, column: 18} }, kind: TokenKind::RParen, module_id: 1.into()},
              Token{range: Range { start: Position {line: 2, column: 18}, end: Position {line: 2, column: 19} }, kind: TokenKind::Whitespace(1), module_id: 1.into()},
              Token{range: Range { start: Position {line: 2, column: 19}, end: Position {line: 2, column: 20} }, kind: TokenKind::Pipe, module_id: 1.into()},
              Token{range: Range { start: Position {line: 2, column: 20}, end: Position {line: 2, column: 21} }, kind: TokenKind::Whitespace(1), module_id: 1.into()},
              Token{range: Range { start: Position {line: 2, column: 21}, end: Position {line: 2, column: 23} }, kind: TokenKind::Ident(SmolStr::new("or")), module_id: 1.into()},
              Token{range: Range { start: Position {line: 2, column: 23}, end: Position {line: 2, column: 24} }, kind: TokenKind::LParen, module_id: 1.into()},
              Token{range: Range { start: Position {line: 2, column: 24}, end: Position {line: 2, column: 25} }, kind: TokenKind::NewLine, module_id: 1.into()},
              Token{range: Range { start: Position {line: 3, column: 1}, end: Position {line: 3, column: 9} }, kind: TokenKind::Ident(SmolStr::new("endswith")), module_id: 1.into()},
              Token{range: Range { start: Position {line: 3, column: 9}, end: Position {line: 3, column: 10} }, kind: TokenKind::LParen, module_id: 1.into()},
              Token{range: Range { start: Position {line: 3, column: 10}, end: Position {line: 3, column: 16} }, kind: TokenKind::StringLiteral("test".to_string()), module_id: 1.into()},
              Token{range: Range { start: Position {line: 3, column: 16}, end: Position {line: 3, column: 17} }, kind: TokenKind::RParen, module_id: 1.into()},
              Token{range: Range { start: Position {line: 3, column: 17}, end: Position {line: 3, column: 18} }, kind: TokenKind::RParen, module_id: 1.into()},
              Token{range: Range { start: Position {line: 3, column: 18}, end: Position {line: 3, column: 18} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case::tab("and(\tcontains(\"test\"))",
            Options{include_spaces: true, ignore_errors: true},
            Ok(vec![
              Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 4} }, kind: TokenKind::Ident(SmolStr::new("and")), module_id: 1.into()},
              Token{range: Range { start: Position {line: 1, column: 4}, end: Position {line: 1, column: 5} }, kind: TokenKind::LParen, module_id: 1.into()},
              Token{range: Range { start: Position {line: 1, column: 5}, end: Position {line: 1, column: 6} }, kind: TokenKind::Tab(1), module_id: 1.into()},
              Token{range: Range { start: Position {line: 1, column: 6}, end: Position {line: 1, column: 14} }, kind: TokenKind::Ident(SmolStr::new("contains")), module_id: 1.into()},
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
                            StringSegment::Expr("val1".to_string().into(), Range { start: Position {line: 1, column: 7}, end: Position {line: 1, column: 14} }),
                            StringSegment::Text("test\n".to_string(), Range { start: Position {line: 1, column: 14}, end: Position {line: 2, column: 1 }})
                          ]), module_id: 1.into()},
                   Token{range: Range { start: Position {line: 2, column: 2}, end: Position {line: 2, column: 2} }, kind: TokenKind::Eof, module_id: 1.into()}]
                ))]
    #[case::error("\"test",
            Options{include_spaces: false, ignore_errors: false},
            Err(SyntaxError::UnexpectedToken(Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 6} }, kind: TokenKind::Eof, module_id: 1.into()})))]
    #[case::error("s\"$$${test}$$\"",
            Options{include_spaces: false, ignore_errors: false},
            Ok(vec![Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 15} },
                          kind: TokenKind::InterpolatedString(vec![
                            StringSegment::Text("$".to_string(), Range { start: Position {line: 1, column: 3}, end: Position {line: 1, column: 5} }),
                            StringSegment::Expr("test".to_string().into(), Range { start: Position {line: 1, column: 5}, end: Position {line: 1, column: 12} }),
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
              Token{range: Range { start: Position {line: 1, column: 7}, end: Position {line: 1, column: 14} }, kind: TokenKind::Ident(SmolStr::new("program")), module_id: 1.into()},
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
              Token{range: Range { start: Position {line: 1, column: 7}, end: Position {line: 1, column: 14} }, kind: TokenKind::Ident(SmolStr::new("program")), module_id: 1.into()},
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
                Token{range: Range { start: Position {line: 1, column: 2}, end: Position {line: 1, column: 5} }, kind: TokenKind::Ident(SmolStr::new("key")), module_id: 1.into()},
                Token{range: Range { start: Position {line: 1, column: 5}, end: Position {line: 1, column: 6} }, kind: TokenKind::Colon, module_id: 1.into()},
                Token{range: Range { start: Position {line: 1, column: 7}, end: Position {line: 1, column: 12} }, kind: TokenKind::Ident(SmolStr::new("value")), module_id: 1.into()},
                Token{range: Range { start: Position {line: 1, column: 12}, end: Position {line: 1, column: 13} }, kind: TokenKind::RBrace, module_id: 1.into()},
                Token{range: Range { start: Position {line: 1, column: 13}, end: Position {line: 1, column: 13} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case::selector_with_dot_h_text(".h.text",
            Options::default(),
            Ok(vec![
                    Token {
                        range: Range { start: Position { line: 1, column: 1 }, end: Position { line: 1, column: 3 } },
                        kind: TokenKind::Selector(SmolStr::new(".h")),
                        module_id: 1.into(),
                    },
                    Token {
                        range: Range { start: Position { line: 1, column: 3 }, end: Position { line: 1, column: 8 } },
                        kind: TokenKind::Selector(SmolStr::new(".text")),
                        module_id: 1.into(),
                    },
                    Token {
                        range: Range { start: Position { line: 1, column: 8 }, end: Position { line: 1, column: 8 } },
                        kind: TokenKind::Eof,
                        module_id: 1.into(),
                    }
                ])
            )]
    #[case::selector_with_dot_h_level(".h.level",
            Options::default(),
            Ok(vec![
                    Token {
                        range: Range { start: Position { line: 1, column: 1 }, end: Position { line: 1, column: 3 } },
                        kind: TokenKind::Selector(SmolStr::new(".h")),
                        module_id: 1.into(),
                    },
                    Token {
                        range: Range { start: Position { line: 1, column: 3 }, end: Position { line: 1, column: 9 } },
                        kind: TokenKind::Selector(SmolStr::new(".level")),
                        module_id: 1.into(),
                    },
                    Token {
                        range: Range { start: Position { line: 1, column: 9 }, end: Position { line: 1, column: 9 } },
                        kind: TokenKind::Eof,
                        module_id: 1.into(),
                    }
                ])
            )]
    #[case::selector_blockquote_alias(".>",
            Options::default(),
            Ok(vec![
                    Token {
                        range: Range { start: Position { line: 1, column: 1 }, end: Position { line: 1, column: 3 } },
                        kind: TokenKind::Selector(SmolStr::new(".>")),
                        module_id: 1.into(),
                    },
                    Token {
                        range: Range { start: Position { line: 1, column: 3 }, end: Position { line: 1, column: 3 } },
                        kind: TokenKind::Eof,
                        module_id: 1.into(),
                    }
                ])
            )]
    #[case::selector_footnote_alias(".^",
            Options::default(),
            Ok(vec![
                    Token {
                        range: Range { start: Position { line: 1, column: 1 }, end: Position { line: 1, column: 3 } },
                        kind: TokenKind::Selector(SmolStr::new(".^")),
                        module_id: 1.into(),
                    },
                    Token {
                        range: Range { start: Position { line: 1, column: 3 }, end: Position { line: 1, column: 3 } },
                        kind: TokenKind::Eof,
                        module_id: 1.into(),
                    }
                ])
            )]
    #[case::selector_blockquote_in_expression("select(.>)",
            Options::default(),
            Ok(vec![
                    Token {
                        range: Range { start: Position { line: 1, column: 1 }, end: Position { line: 1, column: 7 } },
                        kind: TokenKind::Ident(SmolStr::new("select")),
                        module_id: 1.into(),
                    },
                    Token {
                        range: Range { start: Position { line: 1, column: 7 }, end: Position { line: 1, column: 8 } },
                        kind: TokenKind::LParen,
                        module_id: 1.into(),
                    },
                    Token {
                        range: Range { start: Position { line: 1, column: 8 }, end: Position { line: 1, column: 10 } },
                        kind: TokenKind::Selector(SmolStr::new(".>")),
                        module_id: 1.into(),
                    },
                    Token {
                        range: Range { start: Position { line: 1, column: 10 }, end: Position { line: 1, column: 11 } },
                        kind: TokenKind::RParen,
                        module_id: 1.into(),
                    },
                    Token {
                        range: Range { start: Position { line: 1, column: 11 }, end: Position { line: 1, column: 11 } },
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
                        kind: TokenKind::Ident(SmolStr::new("print")),
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
            Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 11} }, kind: TokenKind::Ident(SmolStr::new("definition")), module_id: 1.into()},
            Token{range: Range { start: Position {line: 1, column: 11}, end: Position {line: 1, column: 11} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case::keyword_boundary_end("ending",
        Options::default(),
        Ok(vec![
            Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 7} }, kind: TokenKind::Ident(SmolStr::new("ending")), module_id: 1.into()},
            Token{range: Range { start: Position {line: 1, column: 7}, end: Position {line: 1, column: 7} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case::keyword_boundary_if("ifconfig",
        Options::default(),
        Ok(vec![
            Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 9} }, kind: TokenKind::Ident(SmolStr::new("ifconfig")), module_id: 1.into()},
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
    // Non-ASCII alphanumeric after an ASCII keyword base must block the keyword match.
    // "defä" must not lex as keyword Def; the ASCII portion becomes Ident("def") instead.
    #[case::keyword_boundary_non_ascii_def("defä",
        Options{ignore_errors: true, include_spaces: false},
        Ok(vec![
            Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 4} }, kind: TokenKind::Ident(SmolStr::new("def")), module_id: 1.into()},
            Token{range: Range { start: Position {line: 1, column: 4}, end: Position {line: 1, column: 5} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case::keyword_boundary_non_ascii_if("ifé",
        Options{ignore_errors: true, include_spaces: false},
        Ok(vec![
            Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 3} }, kind: TokenKind::Ident(SmolStr::new("if")), module_id: 1.into()},
            Token{range: Range { start: Position {line: 1, column: 3}, end: Position {line: 1, column: 4} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
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
                range: Range { start: Position { line: 1, column: 22 }, end: Position { line: 1, column: 22 } },
                kind: TokenKind::Eof,
                module_id: 1.into(),
            }
        ])
    )]
    #[case::comment_with_empty_line("#\n# test",
        Options::default(),
        Ok(vec![
            Token {
                range: Range { start: Position { line: 2, column: 7 }, end: Position { line: 2, column: 7 } },
                kind: TokenKind::Eof,
                module_id: 1.into(),
            }
        ])
    )]
    #[case::comment_hash_only("#",
        Options::default(),
        Ok(vec![
            Token {
                range: Range { start: Position { line: 1, column: 2 }, end: Position { line: 1, column: 2 } },
                kind: TokenKind::Eof,
                module_id: 1.into(),
            }
        ])
    )]
    #[case::interpolated_string_with_escaped_braces("s\"test\\{escaped\\}\"",
            Options{include_spaces: false, ignore_errors: false},
            Ok(vec![Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 19} },
                          kind: TokenKind::InterpolatedString(vec![
                            StringSegment::Text("test{escaped}".to_string(), Range { start: Position {line: 1, column: 3}, end: Position {line: 1, column: 18} })
                          ]), module_id: 1.into()},
                   Token{range: Range { start: Position {line: 1, column: 19}, end: Position {line: 1, column: 19} }, kind: TokenKind::Eof, module_id: 1.into()}]
                ))]
    #[case::interpolated_string_mixed_escape_and_expr("s\"\\{${var}\\}\"",
            Options{include_spaces: false, ignore_errors: false},
            Ok(vec![Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 14} },
                          kind: TokenKind::InterpolatedString(vec![
                            StringSegment::Text("{".to_string(), Range { start: Position {line: 1, column: 3}, end: Position {line: 1, column: 5} }),
                            StringSegment::Expr("var".to_string().into(), Range { start: Position {line: 1, column: 5}, end: Position {line: 1, column: 11} }),
                            StringSegment::Text("}".to_string(), Range { start: Position {line: 1, column: 11}, end: Position {line: 1, column: 13} })
                          ]), module_id: 1.into()},
                   Token{range: Range { start: Position {line: 1, column: 14}, end: Position {line: 1, column: 14} }, kind: TokenKind::Eof, module_id: 1.into()}]
                ))]
    #[case::unicode4_hiragana("\"\\u3041\"",
        Options::default(),
        Ok(vec![
            Token {
                range: Range { start: Position { line: 1, column: 1 }, end: Position { line: 1, column: 9 } },
                kind: TokenKind::StringLiteral("ぁ".to_string()),
                module_id: 1.into(),
            },
            Token {
                range: Range { start: Position { line: 1, column: 9 }, end: Position { line: 1, column: 9 } },
                kind: TokenKind::Eof,
                module_id: 1.into(),
            }
        ])
    )]
    #[case::unicode4_katakana("\"\\u30A1\"",
        Options::default(),
        Ok(vec![
            Token {
                range: Range { start: Position { line: 1, column: 1 }, end: Position { line: 1, column: 9 } },
                kind: TokenKind::StringLiteral("ァ".to_string()),
                module_id: 1.into(),
            },
            Token {
                range: Range { start: Position { line: 1, column: 9 }, end: Position { line: 1, column: 9 } },
                kind: TokenKind::Eof,
                module_id: 1.into(),
            }
        ])
    )]
    #[case::unicode4_in_regex_char_class("\"[\\u3041-\\u3096]+\"",
        Options::default(),
        Ok(vec![
            Token {
                range: Range { start: Position { line: 1, column: 1 }, end: Position { line: 1, column: 19 } },
                kind: TokenKind::StringLiteral("[ぁ-ゖ]+".to_string()),
                module_id: 1.into(),
            },
            Token {
                range: Range { start: Position { line: 1, column: 19 }, end: Position { line: 1, column: 19 } },
                kind: TokenKind::Eof,
                module_id: 1.into(),
            }
        ])
    )]
    #[case::unterminated_string_reports_position("\"unterminated",
        Options::default(),
        Err(SyntaxError::UnexpectedToken(Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 14} }, kind: TokenKind::Eof, module_id: 1.into()})))]
    #[case::arrow("->",
        Options::default(),
        Ok(vec![
            Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 3} }, kind: TokenKind::Arrow, module_id: 1.into()},
            Token{range: Range { start: Position {line: 1, column: 3}, end: Position {line: 1, column: 3} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case::arrow_in_expression("map(->(x):upcase;)",
        Options::default(),
        Ok(vec![
            Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 4} }, kind: TokenKind::Ident(SmolStr::new("map")), module_id: 1.into()},
            Token{range: Range { start: Position {line: 1, column: 4}, end: Position {line: 1, column: 5} }, kind: TokenKind::LParen, module_id: 1.into()},
            Token{range: Range { start: Position {line: 1, column: 5}, end: Position {line: 1, column: 7} }, kind: TokenKind::Arrow, module_id: 1.into()},
            Token{range: Range { start: Position {line: 1, column: 7}, end: Position {line: 1, column: 8} }, kind: TokenKind::LParen, module_id: 1.into()},
            Token{range: Range { start: Position {line: 1, column: 8}, end: Position {line: 1, column: 9} }, kind: TokenKind::Ident(SmolStr::new("x")), module_id: 1.into()},
            Token{range: Range { start: Position {line: 1, column: 9}, end: Position {line: 1, column: 10} }, kind: TokenKind::RParen, module_id: 1.into()},
            Token{range: Range { start: Position {line: 1, column: 10}, end: Position {line: 1, column: 11} }, kind: TokenKind::Colon, module_id: 1.into()},
            Token{range: Range { start: Position {line: 1, column: 11}, end: Position {line: 1, column: 17} }, kind: TokenKind::Ident(SmolStr::new("upcase")), module_id: 1.into()},
            Token{range: Range { start: Position {line: 1, column: 17}, end: Position {line: 1, column: 18} }, kind: TokenKind::SemiColon, module_id: 1.into()},
            Token{range: Range { start: Position {line: 1, column: 18}, end: Position {line: 1, column: 19} }, kind: TokenKind::RParen, module_id: 1.into()},
            Token{range: Range { start: Position {line: 1, column: 19}, end: Position {line: 1, column: 19} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case::arrow_not_minus("- >",
        Options::default(),
        Ok(vec![
            Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 2} }, kind: TokenKind::Minus, module_id: 1.into()},
            Token{range: Range { start: Position {line: 1, column: 3}, end: Position {line: 1, column: 4} }, kind: TokenKind::Gt, module_id: 1.into()},
            Token{range: Range { start: Position {line: 1, column: 4}, end: Position {line: 1, column: 4} }, kind: TokenKind::Eof, module_id: 1.into()}]))]

    fn test_parse(#[case] input: &str, #[case] options: Options, #[case] expected: Result<Vec<Token>, SyntaxError>) {
        assert_eq!(Lexer::new(options).tokenize(input, 1.into()), expected);
    }

    #[rstest]
    #[case::basic(r#"b"abc""#,
        Options::default(),
        Ok(vec![
            Token { range: Range { start: Position { line: 1, column: 1 }, end: Position { line: 1, column: 7 } },
                kind: TokenKind::BytesLiteral(vec![97, 98, 99]), module_id: 1.into() },
            Token { range: Range { start: Position { line: 1, column: 7 }, end: Position { line: 1, column: 7 } },
                kind: TokenKind::Eof, module_id: 1.into() },
        ])
    )]
    #[case::hex_escape(r#"b"\xf0\x9f""#,
        Options::default(),
        Ok(vec![
            Token { range: Range { start: Position { line: 1, column: 1 }, end: Position { line: 1, column: 12 } },
                kind: TokenKind::BytesLiteral(vec![0xf0, 0x9f]), module_id: 1.into() },
            Token { range: Range { start: Position { line: 1, column: 12 }, end: Position { line: 1, column: 12 } },
                kind: TokenKind::Eof, module_id: 1.into() },
        ])
    )]
    #[case::standard_escapes(r#"b"\n\r\t\\""#,
        Options::default(),
        Ok(vec![
            Token { range: Range { start: Position { line: 1, column: 1 }, end: Position { line: 1, column: 12 } },
                kind: TokenKind::BytesLiteral(vec![b'\n', b'\r', b'\t', b'\\']), module_id: 1.into() },
            Token { range: Range { start: Position { line: 1, column: 12 }, end: Position { line: 1, column: 12 } },
                kind: TokenKind::Eof, module_id: 1.into() },
        ])
    )]
    #[case::empty(r#"b"""#,
        Options::default(),
        Ok(vec![
            Token { range: Range { start: Position { line: 1, column: 1 }, end: Position { line: 1, column: 4 } },
                kind: TokenKind::BytesLiteral(vec![]), module_id: 1.into() },
            Token { range: Range { start: Position { line: 1, column: 4 }, end: Position { line: 1, column: 4 } },
                kind: TokenKind::Eof, module_id: 1.into() },
        ])
    )]
    // Non-ASCII inside b"..." fails to parse as a byte literal.
    // The tokenizer falls back: `b` becomes Ident and `"é"` becomes StringLiteral.
    // Higher-level parsing then rejects the invalid expression.
    #[case::non_ascii_not_a_byte_literal(
        "b\"\u{00e9}\"",
        Options::default(),
        Ok(vec![
            Token { range: Range { start: Position { line: 1, column: 1 }, end: Position { line: 1, column: 2 } },
                kind: TokenKind::Ident(SmolStr::new("b")), module_id: 1.into() },
            Token { range: Range { start: Position { line: 1, column: 2 }, end: Position { line: 1, column: 5 } },
                kind: TokenKind::StringLiteral("\u{00e9}".to_string()), module_id: 1.into() },
            Token { range: Range { start: Position { line: 1, column: 5 }, end: Position { line: 1, column: 5 } },
                kind: TokenKind::Eof, module_id: 1.into() },
        ])
    )]
    #[case::b_ident_without_quote("b foo",
        Options::default(),
        Ok(vec![
            Token { range: Range { start: Position { line: 1, column: 1 }, end: Position { line: 1, column: 2 } },
                kind: TokenKind::Ident(SmolStr::new("b")), module_id: 1.into() },
            Token { range: Range { start: Position { line: 1, column: 3 }, end: Position { line: 1, column: 6 } },
                kind: TokenKind::Ident(SmolStr::new("foo")), module_id: 1.into() },
            Token { range: Range { start: Position { line: 1, column: 6 }, end: Position { line: 1, column: 6 } },
                kind: TokenKind::Eof, module_id: 1.into() },
        ])
    )]
    fn test_byte_string_literal(
        #[case] input: &str,
        #[case] options: Options,
        #[case] expected: Result<Vec<Token>, SyntaxError>,
    ) {
        assert_eq!(Lexer::new(options).tokenize(input, 1.into()), expected);
    }
}
