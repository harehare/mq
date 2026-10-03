//! Reference implementation of the lexer built on `nom`.
//!
//! Kept only to check that the hand-written lexer produces identical tokens and errors.

use super::Options;
use super::token::{StringSegment, Token, TokenKind};
use nom::Parser;
use nom::bytes::complete::{is_not, take_until, take_while1};
use nom::character::complete::{digit1, line_ending};
use nom::combinator::{cut, opt};
use nom::{
    IResult,
    branch::alt,
    bytes::complete::{escaped_transform, tag, take_while, take_while_m_n},
    character::complete::{alpha1, alphanumeric1, anychar, char, multispace0, none_of, satisfy},
    combinator::{map, map_opt, map_res, recognize, value},
    multi::{fold_many0, many0, many1},
    sequence::{delimited, pair, preceded},
};
use nom_locate::{LocatedSpan, position};
use smol_str::SmolStr;

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

pub struct NomLexer {
    options: Options,
}

impl NomLexer {
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

/// Returns the byte-string body's encoded length without including a following query.
fn byte_string_capacity(input: &str) -> usize {
    let mut escaped = false;
    for (index, byte) in input.bytes().enumerate() {
        if escaped {
            escaped = false;
        } else if byte == b'\\' {
            escaped = true;
        } else if byte == b'"' {
            return index;
        }
    }
    0
}

fn byte_string_literal(input: Span) -> IResult<Span, Token> {
    let (span, start) = position(input)?;
    let (span, _) = tag("b\"")(span)?;
    let capacity = byte_string_capacity(span.fragment());

    let (span, bytes) = fold_many0(
        alt((
            byte_escape_seq,
            // Only plain ASCII characters are allowed unescaped; non-ASCII must
            // use \xNN escapes to avoid silent UTF-8 multi-byte encoding.
            map(satisfy(|c: char| c.is_ascii() && c != '"' && c != '\\'), |c| c as u8),
        )),
        || Vec::with_capacity(capacity),
        |mut bytes, byte| {
            bytes.push(byte);
            bytes
        },
    )
    .parse(span)?;

    let (span, _) = char('"').parse(span)?;
    let (span, end) = position(span)?;
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
    while !current.fragment().is_empty() {
        match string_segment(current) {
            Ok((remaining, segment)) => {
                segments.push(segment);
                current = remaining;
            }
            Err(_) => break,
        }
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
#[cfg(feature = "debugger")]
pub(crate) fn nom_parse_interpolation_segments(
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
            "match" => Some(TokenKind::Match),
            "module" => Some(TokenKind::Module),
            "nodes" => Some(TokenKind::Nodes),
            "None" => Some(TokenKind::None),
            "self" => Some(TokenKind::Self_),
            "try" => Some(TokenKind::Try),
            "unless" => Some(TokenKind::Unless),
            "until" => Some(TokenKind::Until),
            "var" => Some(TokenKind::Var),
            "while" => Some(TokenKind::While),
            "yield" => Some(TokenKind::Yield),
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

fn token_slow(input: Span) -> IResult<Span, Token> {
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

fn dispatch_by_first_char(input: Span) -> IResult<Span, Token> {
    let Some(c) = input.fragment().chars().next() else {
        return token_slow(input);
    };
    match c {
        '$' => env(input),
        '"' => alt((string_literal, empty_string)).parse(input),
        '0'..='9' => number_literal(input),
        '-' => alt((number_literal, arrow, minus_equal, minus)).parse(input),
        '.' => alt((number_literal, spread_op, range_op, selector)).parse(input),
        'b' => alt((byte_string_literal, ident_or_keyword)).parse(input),
        's' => alt((interpolated_string, ident_or_keyword)).parse(input),
        '(' => l_paren(input),
        ')' => r_paren(input),
        '{' => l_brace(input),
        '}' => r_brace(input),
        '[' => l_bracket(input),
        ']' => r_bracket(input),
        ',' => comma(input),
        ';' => semi_colon(input),
        ':' => alt((double_colon, colon)).parse(input),
        '?' => alt((coalesce, question)).parse(input),
        '|' => alt((pipe_equal, or, pipe)).parse(input),
        '!' => alt((ne_eq, not_tilde_equal, not)).parse(input),
        '<' => alt((left_shift, lte, lt)).parse(input),
        '>' => alt((right_shift, gte, gt)).parse(input),
        '=' => alt((eq_eq, tilde_equal, equal)).parse(input),
        '+' => alt((number_literal, plus_equal, plus)).parse(input),
        '*' => alt((star_equal, asterisk)).parse(input),
        '/' => alt((slash_equal, double_slash_equal, slash)).parse(input),
        '%' => alt((percent_equal, percent)).parse(input),
        '&' => and(input),
        '@' => convert_op(input),
        c if c.is_ascii_alphabetic() || c == '_' => ident_or_keyword(input),
        _ => token_slow(input),
    }
}

fn token(input: Span) -> IResult<Span, Token> {
    dispatch_by_first_char(input)
}

fn token_include_spaces(input: Span) -> IResult<Span, Token> {
    match input.fragment().chars().next() {
        Some('\n') | Some('\r') => newline(input),
        Some(' ') => spaces(input),
        Some('\t') => tab(input),
        Some('#') => inline_comment(input),
        _ => dispatch_by_first_char(input),
    }
}

/// Consumes up to the next whitespace or delimiter as a single `Unknown` token so lexing can resume.
fn unknown(input: Span) -> IResult<Span, Token> {
    map(
        recognize(pair(
            anychar,
            take_while(|c: char| !c.is_whitespace() && !",()[]{}|;".contains(c)),
        )),
        |span: Span| Token {
            range: span.into(),
            kind: TokenKind::Unknown(span.fragment().to_string()),
            module_id: span.extra,
        },
    )
    .parse(input)
}

fn tokens<'a>(input: Span<'a>, options: &'a Options) -> IResult<Span<'a>, Vec<Token>> {
    let estimated_capacity = input.fragment().len() / 5;
    let mut tokens = Vec::with_capacity(estimated_capacity.max(16));
    let mut current = input;

    if options.include_spaces {
        loop {
            match token_include_spaces(current) {
                Ok((remaining, token)) => {
                    tokens.push(token);
                    current = remaining;
                }
                Err(_) if options.ignore_errors && !current.fragment().is_empty() => {
                    let (remaining, token) = unknown(current)?;
                    tokens.push(token);
                    current = remaining;
                }
                Err(_) => break,
            }
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
