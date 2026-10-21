use crate::RuntimeValue;
use crate::Shared;
use crate::number::Number;
use crate::runtime::builtin::Error;
use base64::prelude::*;
use encoding_rs::Encoding;
use html_escape::decode_html_entities;
use percent_encoding::{NON_ALPHANUMERIC, percent_decode, utf8_percent_encode};
use sha2::Digest;
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ConvertKind {
    Blockquote,
    Heading(mq_markdown::HeadingDepth),
    HorizontalRule,
    Link(String),
    ListItem,
    Strong,
    Strikethrough,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Convert {
    Base64,
    Html,
    Markdown(ConvertKind),
    Md5,
    Sha256,
    Shell,
    Text,
    UriEncode,
    UriDecode,
}

fn is_url(s: &str) -> bool {
    Url::parse(s).is_ok()
}

#[cfg(windows)]
fn is_file_path(s: &str) -> bool {
    s.starts_with(".\\")
        || s.starts_with("..\\")
        || s.starts_with("\\")
        || s.chars().take(3).collect::<String>().ends_with(":\\")
        || s.chars().take(3).collect::<String>().ends_with(":/")
}

#[cfg(not(windows))]
fn is_file_path(s: &str) -> bool {
    s.starts_with("./") || s.starts_with("../") || s.starts_with("/")
}

impl TryFrom<&RuntimeValue> for Convert {
    type Error = Error;

    fn try_from(value: &RuntimeValue) -> Result<Self, Self::Error> {
        match value {
            RuntimeValue::Symbol(symbol) => match symbol.to_string().as_str() {
                "h1" => Ok(Convert::Markdown(ConvertKind::Heading(mq_markdown::HeadingDepth::H1))),
                "h2" => Ok(Convert::Markdown(ConvertKind::Heading(mq_markdown::HeadingDepth::H2))),
                "h3" => Ok(Convert::Markdown(ConvertKind::Heading(mq_markdown::HeadingDepth::H3))),
                "h4" => Ok(Convert::Markdown(ConvertKind::Heading(mq_markdown::HeadingDepth::H4))),
                "h5" => Ok(Convert::Markdown(ConvertKind::Heading(mq_markdown::HeadingDepth::H5))),
                "h6" => Ok(Convert::Markdown(ConvertKind::Heading(mq_markdown::HeadingDepth::H6))),

                "html" => Ok(Convert::Html),
                "md5" => Ok(Convert::Md5),
                "sha256" => Ok(Convert::Sha256),
                "text" => Ok(Convert::Text),
                "sh" => Ok(Convert::Shell),
                "base64" => Ok(Convert::Base64),
                "uri" => Ok(Convert::UriEncode),
                "urid" => Ok(Convert::UriDecode),
                _ => Err(Error::InvalidConvert(symbol.to_string())),
            },
            RuntimeValue::String(s) => match s.as_str() {
                "#" => Ok(Convert::Markdown(ConvertKind::Heading(mq_markdown::HeadingDepth::H1))),
                "##" => Ok(Convert::Markdown(ConvertKind::Heading(mq_markdown::HeadingDepth::H2))),
                "###" => Ok(Convert::Markdown(ConvertKind::Heading(mq_markdown::HeadingDepth::H3))),
                "####" => Ok(Convert::Markdown(ConvertKind::Heading(mq_markdown::HeadingDepth::H4))),
                "#####" => Ok(Convert::Markdown(ConvertKind::Heading(mq_markdown::HeadingDepth::H5))),
                "######" => Ok(Convert::Markdown(ConvertKind::Heading(mq_markdown::HeadingDepth::H6))),
                ">" => Ok(Convert::Markdown(ConvertKind::Blockquote)),
                "-" => Ok(Convert::Markdown(ConvertKind::ListItem)),
                "~~" => Ok(Convert::Markdown(ConvertKind::Strikethrough)),
                "**" => Ok(Convert::Markdown(ConvertKind::Strong)),
                "--" => Ok(Convert::Markdown(ConvertKind::HorizontalRule)),
                s if is_url(s) || is_file_path(s) => Ok(Convert::Markdown(ConvertKind::Link(s.to_string()))),
                _ => Err(Error::InvalidConvert(format!("{:?}", value))),
            },
            _ => Err(Error::InvalidConvert(format!("{:?}", value))),
        }
    }
}

impl Convert {
    pub fn convert(&self, input: &RuntimeValue) -> RuntimeValue {
        match self {
            Convert::Base64 => match input {
                RuntimeValue::String(s) => base64(s).unwrap_or(RuntimeValue::NONE),
                RuntimeValue::Markdown(_node, _) => {
                    if let Some(md) = input.markdown_node() {
                        base64(md.value().as_str()).unwrap_or(RuntimeValue::NONE)
                    } else {
                        RuntimeValue::NONE
                    }
                }
                _ => RuntimeValue::NONE,
            },
            Convert::Html => to_html(input).unwrap_or(RuntimeValue::NONE),
            Convert::Text => to_text(input).unwrap_or(RuntimeValue::NONE),
            Convert::UriEncode => match input {
                RuntimeValue::String(s) => url_encode(s).unwrap_or(RuntimeValue::NONE),
                RuntimeValue::Markdown(_node, _) => {
                    if let Some(md) = input.markdown_node() {
                        url_encode(md.value().as_str()).unwrap_or(RuntimeValue::NONE)
                    } else {
                        RuntimeValue::NONE
                    }
                }
                _ => url_encode(&input.to_string()).unwrap_or(RuntimeValue::NONE),
            },
            Convert::UriDecode => match input {
                RuntimeValue::String(s) => url_decode(s).unwrap_or(RuntimeValue::NONE),
                RuntimeValue::Markdown(_node, _) => {
                    if let Some(md) = input.markdown_node() {
                        url_decode(md.value().as_str()).unwrap_or(RuntimeValue::NONE)
                    } else {
                        RuntimeValue::NONE
                    }
                }
                _ => url_decode(&input.to_string()).unwrap_or(RuntimeValue::NONE),
            },
            Convert::Md5 => match input {
                RuntimeValue::String(s) => md5(s).unwrap_or(RuntimeValue::NONE),
                RuntimeValue::Markdown(_, _) => {
                    if let Some(md) = input.markdown_node() {
                        md5(md.value().as_str()).unwrap_or(RuntimeValue::NONE)
                    } else {
                        RuntimeValue::NONE
                    }
                }
                RuntimeValue::None => RuntimeValue::NONE,
                _ => md5(&input.to_string()).unwrap_or(RuntimeValue::NONE),
            },
            Convert::Sha256 => match input {
                RuntimeValue::String(s) => sha256(s).unwrap_or(RuntimeValue::NONE),
                RuntimeValue::Markdown(_, _) => {
                    if let Some(md) = input.markdown_node() {
                        sha256(md.value().as_str()).unwrap_or(RuntimeValue::NONE)
                    } else {
                        RuntimeValue::NONE
                    }
                }
                RuntimeValue::None => RuntimeValue::NONE,
                _ => sha256(&input.to_string()).unwrap_or(RuntimeValue::NONE),
            },
            Convert::Markdown(kind) => self.convert_to_markdown(input, kind),
            Convert::Shell => {
                // Shell script conversion - escape for safe shell usage
                let text = match input {
                    RuntimeValue::String(s) => (**s).clone(),
                    RuntimeValue::Markdown(node, _) => node.value().to_string(),
                    _ => input.to_string(),
                };
                shell_escape(&text).unwrap_or(RuntimeValue::NONE)
            }
        }
    }

    fn convert_to_markdown(&self, input: &RuntimeValue, kind: &ConvertKind) -> RuntimeValue {
        let text = match input {
            RuntimeValue::String(s) => (**s).clone(),
            RuntimeValue::Markdown(node, _) => node.value().to_string(),
            _ => input.to_string(),
        };

        match kind {
            ConvertKind::Heading(depth) => RuntimeValue::Markdown(
                Shared::new(mq_markdown::Node::Heading(mq_markdown::Heading {
                    depth: *depth,
                    values: vec![text.into()],
                    position: None,
                })),
                None,
            ),
            ConvertKind::Blockquote => RuntimeValue::Markdown(
                Shared::new(mq_markdown::Node::Blockquote(mq_markdown::Blockquote {
                    values: vec![text.into()],
                    position: None,
                })),
                None,
            ),
            ConvertKind::ListItem => RuntimeValue::Markdown(
                Shared::new(mq_markdown::Node::List(mq_markdown::List {
                    marker: None,
                    values: vec![text.into()],
                    index: 0,
                    ordered: false,
                    level: 1,
                    checked: None,
                    start: None,
                    spread: false,
                    position: None,
                })),
                None,
            ),
            ConvertKind::Link(url) => RuntimeValue::Markdown(
                Shared::new(mq_markdown::Node::Link(mq_markdown::Link {
                    url: mq_markdown::Url::new(url.to_string()),
                    values: vec![text.into()],
                    title: None,
                    position: None,
                })),
                None,
            ),
            ConvertKind::Strikethrough => RuntimeValue::Markdown(
                Shared::new(mq_markdown::Node::Delete(mq_markdown::Delete {
                    values: vec![text.into()],
                    position: None,
                })),
                None,
            ),
            ConvertKind::Strong => RuntimeValue::Markdown(
                Shared::new(mq_markdown::Node::Strong(mq_markdown::Strong {
                    values: vec![text.into()],
                    position: None,
                })),
                None,
            ),
            ConvertKind::HorizontalRule => RuntimeValue::Markdown(
                Shared::new(mq_markdown::Node::HorizontalRule(mq_markdown::HorizontalRule {
                    marker: None,
                    position: None,
                })),
                None,
            ),
        }
    }
}

/// convert to HTML
#[inline(always)]
pub(super) fn to_html(value: &RuntimeValue) -> Result<RuntimeValue, Error> {
    match value {
        RuntimeValue::None => Ok(RuntimeValue::NONE),
        RuntimeValue::String(s) => Ok(mq_markdown::to_html(s).into()),
        RuntimeValue::Symbol(s) => Ok(mq_markdown::to_html(&s.to_string()).into()),
        RuntimeValue::Markdown(node_value, _) => Ok(mq_markdown::to_html(node_value.to_string().as_str()).into()),
        _ => Err(Error::InvalidTypes("to_html".to_string(), vec![value.clone()])),
    }
}

/// convert to Markdown string
pub(super) fn to_markdown_string(args: Vec<RuntimeValue>) -> Result<RuntimeValue, Error> {
    let args = flatten(args);

    Ok(mq_markdown::Markdown::new(
        args.iter()
            .flat_map(|arg| match arg {
                RuntimeValue::Markdown(node, _) => vec![(**node).clone()],
                a => vec![a.to_string().into()],
            })
            .collect(),
    )
    .to_string()
    .into())
}

/// convert to string
pub(super) fn to_string(value: &RuntimeValue) -> Result<RuntimeValue, Error> {
    match value {
        RuntimeValue::Symbol(s) => Ok(s.to_string().into()),
        o => Ok(o.to_string().into()),
    }
}

fn parse_number(s: &str) -> Result<RuntimeValue, Error> {
    s.parse::<f64>()
        .map(|n| RuntimeValue::Number(n.into()))
        .map_err(|e| Error::Runtime(format!("to_number: cannot convert {:?} to a number ({})", s, e)))
}

/// convert to number
pub(super) fn to_number(value: &mut RuntimeValue) -> Result<RuntimeValue, Error> {
    match value {
        node @ RuntimeValue::Markdown(_, _) => node
            .markdown_node()
            .map(|md| parse_number(&md.to_string()))
            .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
        RuntimeValue::String(s) => parse_number(s),
        RuntimeValue::Array(array) => {
            let result_value: Result<Vec<RuntimeValue>, Error> = crate::Shared::unwrap_or_clone(std::mem::take(array))
                .into_iter()
                .map(|o| match o {
                    node @ RuntimeValue::Markdown(_, _) => node
                        .markdown_node()
                        .map(|md| parse_number(&md.to_string()))
                        .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
                    RuntimeValue::String(s) => parse_number(&s),
                    RuntimeValue::Boolean(b) => Ok(RuntimeValue::Number(if b { 1 } else { 0 }.into())),
                    n @ RuntimeValue::Number(_) => Ok(n),
                    _ => Ok(RuntimeValue::Number(0.into())),
                })
                .collect();

            result_value.map(|v| RuntimeValue::Array(crate::Shared::new(v)))
        }
        RuntimeValue::Boolean(true) => Ok(RuntimeValue::Number(1.into())),
        RuntimeValue::Boolean(false) => Ok(RuntimeValue::Number(0.into())),
        RuntimeValue::Number(n) => Ok(RuntimeValue::Number(*n)),
        _ => Ok(RuntimeValue::Number(0.into())),
    }
}

/// convert to boolean
pub(super) fn to_boolean(value: &RuntimeValue) -> Result<RuntimeValue, Error> {
    match value {
        b @ RuntimeValue::Boolean(_) => Ok(b.clone()),
        RuntimeValue::String(s) => match s.as_str() {
            "true" => Ok(RuntimeValue::Boolean(true)),
            "false" => Ok(RuntimeValue::Boolean(false)),
            _ => Err(Error::InvalidTypes("to_boolean".to_string(), vec![value.clone()])),
        },
        _ => Err(Error::InvalidTypes("to_boolean".to_string(), vec![value.clone()])),
    }
}

/// convert to array
pub(super) fn to_array(value: &mut RuntimeValue) -> Result<RuntimeValue, Error> {
    match value {
        RuntimeValue::Array(array) => Ok(RuntimeValue::Array(std::mem::take(array))),
        RuntimeValue::String(s) => Ok(RuntimeValue::Array(crate::Shared::new(
            s.chars()
                .map(|c| RuntimeValue::String(Shared::new(c.to_string())))
                .collect(),
        ))),
        RuntimeValue::Bytes(bytes) => Ok(RuntimeValue::Array(crate::Shared::new(
            bytes.iter().map(|b| RuntimeValue::Number((*b).into())).collect(),
        ))),
        RuntimeValue::None => Ok(RuntimeValue::empty_array()),
        value => Ok(RuntimeValue::Array(crate::Shared::new(vec![std::mem::take(value)]))),
    }
}

/// convert to text
pub(super) fn to_text(value: &RuntimeValue) -> Result<RuntimeValue, Error> {
    match value {
        RuntimeValue::None => Ok(RuntimeValue::NONE),
        RuntimeValue::Markdown(node_value, _) => Ok(node_value.value().into()),
        RuntimeValue::Array(array) => Ok(array
            .iter()
            .map(|a| if a.is_none() { "".to_string() } else { a.to_string() })
            .collect::<Vec<_>>()
            .join(",")
            .into()),
        value => Ok(value.to_string().into()),
    }
}

/// convert from date string to Unix timestamp (seconds)
#[inline(always)]
pub(super) fn from_date(date_str: &str) -> Result<RuntimeValue, Error> {
    match chrono::DateTime::parse_from_rfc3339(date_str) {
        Ok(datetime) => Ok(RuntimeValue::Number(datetime.timestamp().into())),
        Err(e) => Err(Error::Runtime(format!(
            "from_date: cannot parse {:?} as an RFC 3339 date ({})",
            date_str, e
        ))),
    }
}

/// convert from Unix timestamp (seconds) to date string
#[inline(always)]
pub(super) fn to_date(secs: Number, convert: Option<&str>) -> Result<RuntimeValue, Error> {
    chrono::DateTime::from_timestamp(secs.value() as i64, 0)
        .map(|dt| {
            convert
                .map(|f| dt.format(f).to_string())
                .unwrap_or(dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
        })
        .map(|s| RuntimeValue::String(s.into()))
        .ok_or_else(|| Error::InvalidDateTimeFormat(convert.unwrap_or("").to_string()))
}

/// Encode to base64
#[inline(always)]
pub(super) fn base64(input: &str) -> Result<RuntimeValue, Error> {
    Ok(RuntimeValue::String(Shared::new(BASE64_STANDARD.encode(input))))
}

/// Encode to base64 from raw bytes
#[inline(always)]
pub(super) fn base64_bytes(input: &[u8]) -> Result<RuntimeValue, Error> {
    Ok(RuntimeValue::String(Shared::new(
        base64::engine::general_purpose::STANDARD.encode(input),
    )))
}

/// Decode from base64
#[inline(always)]
pub(super) fn base64d(input: &str) -> Result<RuntimeValue, Error> {
    BASE64_STANDARD
        .decode(input)
        .map_err(Error::InvalidBase64String)
        .map(|v| RuntimeValue::String(Shared::new(String::from_utf8_lossy(&v).to_string())))
}

/// Encode to base64url
#[inline(always)]
pub(super) fn base64url(input: &str) -> Result<RuntimeValue, Error> {
    Ok(RuntimeValue::String(Shared::new(BASE64_URL_SAFE_NO_PAD.encode(input))))
}

/// Decode from base64url
#[inline(always)]
pub(super) fn base64urld(input: &str) -> Result<RuntimeValue, Error> {
    BASE64_URL_SAFE_NO_PAD
        .decode(input)
        .map_err(Error::InvalidBase64String)
        .map(|v| RuntimeValue::String(Shared::new(String::from_utf8_lossy(&v).to_string())))
}

/// Decode from base64 to raw bytes, losslessly (unlike `base64d`, which assumes UTF-8 text and
/// replaces invalid sequences).
#[inline(always)]
pub(super) fn base64d_bytes(input: &str) -> Result<RuntimeValue, Error> {
    BASE64_STANDARD
        .decode(input)
        .map_err(Error::InvalidBase64String)
        .map(|v| RuntimeValue::Bytes(Shared::new(v)))
}

/// Decode from base64url to raw bytes, losslessly (unlike `base64urld`).
#[inline(always)]
pub(super) fn base64urld_bytes(input: &str) -> Result<RuntimeValue, Error> {
    BASE64_URL_SAFE_NO_PAD
        .decode(input)
        .map_err(Error::InvalidBase64String)
        .map(|v| RuntimeValue::Bytes(Shared::new(v)))
}

/// URL encode
#[inline(always)]
pub(super) fn url_encode(input: &str) -> Result<RuntimeValue, Error> {
    Ok(RuntimeValue::String(Shared::new(
        utf8_percent_encode(input, NON_ALPHANUMERIC).to_string(),
    )))
}

/// URL decode
#[inline(always)]
pub(super) fn url_decode(input: &str) -> Result<RuntimeValue, Error> {
    Ok(RuntimeValue::String(Shared::new(
        percent_decode(input.as_bytes()).decode_utf8_lossy().to_string(),
    )))
}

/// Escape `&`, `<`, `>`, `"`, and `'` as HTML entities
#[inline(always)]
pub(super) fn html_escape(input: &str) -> Result<RuntimeValue, Error> {
    let mut result = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '"' => result.push_str("&quot;"),
            '\'' => result.push_str("&#39;"),
            _ => result.push(c),
        }
    }
    Ok(RuntimeValue::String(result.into()))
}

/// Decode named and numeric HTML entities into their corresponding characters
#[inline(always)]
pub(super) fn html_unescape(input: &str) -> Result<RuntimeValue, Error> {
    Ok(RuntimeValue::String(Shared::new(
        decode_html_entities(input).into_owned(),
    )))
}

/// The output position `markdown_escape` is escaping for. Each variant escapes only the
/// characters that could form unintended Markdown structure at that position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MarkdownEscapeContext {
    /// Body text: paragraph, list item, or blockquote content. Line breaks are kept, but the
    /// start of each line is guarded against being read as a heading, blockquote, list item,
    /// thematic break, or Setext heading underline.
    Text,
    /// Heading text. Must be a single line, so line breaks are collapsed to spaces.
    Heading,
    /// Text inside a link/image label (`[label]`). Must be a single line.
    LinkLabel,
    /// A GFM table cell. `|` is escaped and line breaks are collapsed to spaces, since a
    /// literal pipe or line break would otherwise split the cell.
    TableCell,
    /// Content meant to render as literal code (inline code span or code block). Left
    /// unmodified: backslash escapes have no effect inside code, so the caller must instead
    /// pick a backtick-fence length longer than any backtick run already in the content.
    Code,
}

impl TryFrom<&str> for MarkdownEscapeContext {
    type Error = Error;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        match s {
            "text" => Ok(Self::Text),
            "heading" => Ok(Self::Heading),
            "link_label" => Ok(Self::LinkLabel),
            "table_cell" => Ok(Self::TableCell),
            "code" => Ok(Self::Code),
            _ => Err(Error::Runtime(format!(
                "unknown markdown_escape context {:?}, expected \"text\", \"heading\", \"link_label\", \"table_cell\", or \"code\"",
                s
            ))),
        }
    }
}

/// Escapes `input` so it renders as literal text at the given output `context`, without
/// introducing unintended Markdown structure (headings, lists, links, emphasis, tables, ...).
/// This is not HTML sanitization or URL-encoding; use `html_escape`, `sanitize_html`, or
/// `url_encode` for those.
#[inline(always)]
pub(super) fn markdown_escape(input: &str, context: MarkdownEscapeContext) -> Result<RuntimeValue, Error> {
    let escaped = match context {
        MarkdownEscapeContext::Text => escape_markdown_text(input),
        MarkdownEscapeContext::Heading | MarkdownEscapeContext::LinkLabel => escape_markdown_inline_singleline(input),
        MarkdownEscapeContext::TableCell => escape_markdown_table_cell(input),
        MarkdownEscapeContext::Code => input.to_string(),
    };
    Ok(RuntimeValue::String(Shared::new(escaped)))
}

/// Escapes a character with inline Markdown significance (emphasis, strikethrough, code
/// spans, links/images, raw HTML/autolinks). The backslash itself is escaped first, since
/// it is the escape character.
#[inline(always)]
fn escape_markdown_inline_char(c: char, out: &mut String) {
    match c {
        '\\' | '`' | '*' | '_' | '~' | '[' | ']' | '<' => {
            out.push('\\');
            out.push(c);
        }
        _ => out.push(c),
    }
}

/// Escapes inline Markdown-significant characters, collapsing line breaks to spaces since
/// the result must stay on a single line.
fn escape_markdown_inline_singleline(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            '\n' | '\r' => out.push(' '),
            c => escape_markdown_inline_char(c, &mut out),
        }
    }
    out
}

/// Escapes inline Markdown-significant characters and `|` for a GFM table cell, collapsing
/// line breaks to spaces since a cell cannot contain either.
fn escape_markdown_table_cell(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            '\n' | '\r' => out.push(' '),
            '|' => out.push_str("\\|"),
            c => escape_markdown_inline_char(c, &mut out),
        }
    }
    out
}

/// Escapes body text line by line, keeping line breaks but guarding each line's start
/// against being read as block-level syntax.
fn escape_markdown_text(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut lines = input.split('\n');
    if let Some(first) = lines.next() {
        out.push_str(&escape_markdown_text_line(first));
    }
    for line in lines {
        out.push('\n');
        out.push_str(&escape_markdown_text_line(line));
    }
    out
}

/// Escapes a single line of body text: neutralizes leading indentation that would otherwise
/// form an indented code block, defuses a leading block marker (if any), then escapes inline
/// Markdown-significant characters in the remainder.
fn escape_markdown_text_line(line: &str) -> String {
    let (indent_bytes, indent_columns) = leading_indent_whitespace(line);

    if indent_columns >= 4 {
        let mut out = String::with_capacity(line.len() + indent_bytes * 4);
        for c in line[..indent_bytes].chars() {
            match c {
                ' ' => out.push_str("&#32;"),
                '\t' => out.push_str("&#9;"),
                _ => unreachable!("leading_indent_whitespace only consumes spaces and tabs"),
            }
        }
        escape_markdown_text_line_body(&line[indent_bytes..], &mut out);
        return out;
    }

    let rest = &line[indent_bytes..];
    let mut out = String::with_capacity(line.len() + 1);
    out.push_str(&line[..indent_bytes]);
    escape_markdown_text_line_body(rest, &mut out);
    out
}

/// Defuses a leading block marker in `rest` (if any) and escapes inline Markdown-significant
/// characters, appending the result to `out`.
fn escape_markdown_text_line_body(rest: &str, out: &mut String) {
    match markdown_block_marker_len(rest) {
        Some(marker_len) => {
            out.push_str(&rest[..marker_len]);
            out.push('\\');
            for c in rest[marker_len..].chars() {
                escape_markdown_inline_char(c, out);
            }
        }
        None => {
            for c in rest.chars() {
                escape_markdown_inline_char(c, out);
            }
        }
    }
}

/// Returns the byte length and effective column width of the leading run of spaces and tabs
/// in `line`, expanding each tab to the next 4-column tab stop per CommonMark.
fn leading_indent_whitespace(line: &str) -> (usize, usize) {
    let mut bytes = 0;
    let mut columns = 0;
    for c in line.chars() {
        match c {
            ' ' => {
                columns += 1;
                bytes += 1;
            }
            '\t' => {
                columns += 4 - (columns % 4);
                bytes += 1;
            }
            _ => break,
        }
    }
    (bytes, columns)
}

/// Returns the byte length of the prefix of `rest` that can be kept as-is before inserting a
/// backslash to defuse a leading block marker (ATX heading, blockquote, list item, thematic
/// break, or Setext heading underline), or `None` if `rest` does not start one.
fn markdown_block_marker_len(rest: &str) -> Option<usize> {
    let is_boundary_after =
        |byte_offset: usize| matches!(rest[byte_offset..].chars().next(), None | Some(' ') | Some('\t'));
    let first = rest.chars().next()?;

    match first {
        '#' => {
            let hashes = rest.chars().take_while(|&c| c == '#').count();
            (hashes <= 6 && is_boundary_after(hashes)).then_some(0)
        }
        '>' => Some(0),
        '-' | '+' if is_boundary_after(first.len_utf8()) => Some(0),
        c if c.is_ascii_digit() => {
            let digits = rest.chars().take_while(char::is_ascii_digit).count();
            match rest[digits..].chars().next() {
                Some(d @ ('.' | ')')) if digits <= 9 && is_boundary_after(digits + d.len_utf8()) => Some(digits),
                _ => None,
            }
        }
        '-' | '=' => {
            let trimmed = rest.trim_end_matches(' ');
            (!trimmed.is_empty() && trimmed.chars().all(|c| c == first)).then_some(0)
        }
        _ => None,
    }
}

/// Strip HTML tags from a string, keeping the surrounding text content
#[inline(always)]
pub(super) fn strip_tags(input: &str) -> Result<RuntimeValue, Error> {
    let mut result = String::with_capacity(input.len());
    let mut in_tag = false;
    let mut quote: Option<char> = None;

    for c in input.chars() {
        if in_tag {
            match quote {
                Some(q) if c == q => quote = None,
                Some(_) => {}
                None => match c {
                    '"' | '\'' => quote = Some(c),
                    '>' => in_tag = false,
                    _ => {}
                },
            }
        } else if c == '<' {
            in_tag = true;
        } else {
            result.push(c);
        }
    }

    Ok(RuntimeValue::String(result.into()))
}

/// Sanitize HTML using an allowlist of safe tags/attributes (XSS protection).
#[inline(always)]
pub(super) fn sanitize_html(input: &str) -> Result<RuntimeValue, Error> {
    Ok(RuntimeValue::String(Shared::new(ammonia::clean(input))))
}

/// Compute MD5 hash and return lowercase hex string.
///
/// Note: MD5 is cryptographically broken and should not be used for security
/// purposes. It is suitable for non-cryptographic use cases such as content
/// fingerprinting and cache keys.
#[inline(always)]
pub(super) fn md5(input: &str) -> Result<RuntimeValue, Error> {
    let digest = md5::compute(input.as_bytes());
    Ok(RuntimeValue::String(Shared::new(bytes_to_hex(&digest.0))))
}

/// Compute MD5 hash of raw bytes and return lowercase hex string.
#[inline(always)]
pub(super) fn md5_bytes(input: &[u8]) -> Result<RuntimeValue, Error> {
    let digest = md5::compute(input);
    Ok(RuntimeValue::String(Shared::new(bytes_to_hex(&digest.0))))
}

/// Compute SHA-256 hash and return lowercase hex string.
#[inline(always)]
pub(super) fn sha256(input: &str) -> Result<RuntimeValue, Error> {
    let hash = sha2::Sha256::digest(input.as_bytes());
    Ok(RuntimeValue::String(Shared::new(bytes_to_hex(hash.as_slice()))))
}

/// Compute SHA-256 hash of raw bytes and return lowercase hex string.
#[inline(always)]
pub(super) fn sha256_bytes(input: &[u8]) -> Result<RuntimeValue, Error> {
    let hash = sha2::Sha256::digest(input);
    Ok(RuntimeValue::String(Shared::new(bytes_to_hex(hash.as_slice()))))
}

/// Compute SHA-512 hash and return lowercase hex string.
#[inline(always)]
pub(super) fn sha512(input: &str) -> Result<RuntimeValue, Error> {
    let hash = sha2::Sha512::digest(input.as_bytes());
    Ok(RuntimeValue::String(Shared::new(bytes_to_hex(hash.as_slice()))))
}

/// Compute SHA-512 hash of raw bytes and return lowercase hex string.
#[inline(always)]
pub(super) fn sha512_bytes(input: &[u8]) -> Result<RuntimeValue, Error> {
    let hash = sha2::Sha512::digest(input);
    Ok(RuntimeValue::String(Shared::new(bytes_to_hex(hash.as_slice()))))
}

/// Parse a lowercase or uppercase hex string into raw bytes.
#[inline(always)]
pub(super) fn from_hex(input: &str) -> Result<RuntimeValue, Error> {
    if !input.len().is_multiple_of(2) {
        return Err(Error::Runtime(format!(
            "from_hex: odd-length hex string: \"{}\"",
            input
        )));
    }
    let bytes = input
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|chunk| {
            let s = std::str::from_utf8(chunk)
                .map_err(|_| Error::Runtime("from_hex: invalid hex byte (non-ASCII)".to_string()))?;
            u8::from_str_radix(s, 16).map_err(|_| Error::Runtime(format!("from_hex: invalid hex byte \"{}\"", s)))
        })
        .collect::<Result<Vec<u8>, _>>()?;
    Ok(RuntimeValue::Bytes(bytes.into()))
}

/// Decode bytes as a UTF-8 string, returning an error if the bytes are not valid UTF-8.
#[inline(always)]
pub(super) fn utf8(input: &[u8]) -> Result<RuntimeValue, Error> {
    String::from_utf8(input.to_vec())
        .map(|s| RuntimeValue::String(s.into()))
        .map_err(|e| Error::Runtime(format!("utf8: {}", e)))
}

/// Encode raw bytes as a lowercase hex string.
#[inline(always)]
pub(super) fn to_hex(input: &[u8]) -> Result<RuntimeValue, Error> {
    Ok(RuntimeValue::String(Shared::new(bytes_to_hex(input))))
}

/// Render bytes as a `hexdump -C`/`xxd`-style dump: offset, hex bytes, and an ASCII gutter.
#[inline(always)]
pub(super) fn hexdump(input: &[u8]) -> Result<RuntimeValue, Error> {
    use std::fmt::Write;
    const WIDTH: usize = 16;

    if input.is_empty() {
        return Ok(RuntimeValue::String(format!("{:08x}", 0).into()));
    }

    let mut out = String::with_capacity(input.len() / WIDTH * 70 + 70);
    for (i, chunk) in input.chunks(WIDTH).enumerate() {
        write!(out, "{:08x}  ", i * WIDTH).unwrap();

        for (j, b) in chunk.iter().enumerate() {
            write!(out, "{b:02x} ").unwrap();
            if j == WIDTH / 2 - 1 {
                out.push(' ');
            }
        }
        for j in chunk.len()..WIDTH {
            out.push_str("   ");
            if j == WIDTH / 2 - 1 {
                out.push(' ');
            }
        }

        out.push_str(" |");
        for b in chunk {
            out.push(if b.is_ascii_graphic() || *b == b' ' {
                *b as char
            } else {
                '.'
            });
        }
        out.push('|');

        if i + 1 != input.len().div_ceil(WIDTH) {
            out.push('\n');
        }
    }

    Ok(RuntimeValue::String(out.into()))
}

/// Decode bytes as text using a WHATWG encoding label. Errors on invalid input rather than substituting.
#[inline(always)]
pub(super) fn decode(input: &[u8], label: &str) -> Result<RuntimeValue, Error> {
    let encoding = Encoding::for_label(label.as_bytes())
        .ok_or_else(|| Error::Runtime(format!("decode: unrecognized encoding label \"{}\"", label)))?;
    let (decoded, _, had_errors) = encoding.decode(input);
    if had_errors {
        return Err(Error::Runtime(format!(
            "decode: input contains a byte sequence invalid for encoding \"{}\"",
            encoding.name()
        )));
    }
    Ok(RuntimeValue::String(Shared::new(decoded.into_owned())))
}

/// Encode text as bytes using a WHATWG encoding label. Errors on unmappable input rather than substituting.
#[inline(always)]
pub(super) fn encode(input: &str, label: &str) -> Result<RuntimeValue, Error> {
    let encoding = Encoding::for_label(label.as_bytes())
        .ok_or_else(|| Error::Runtime(format!("encode: unrecognized encoding label \"{}\"", label)))?;
    let (encoded, _, had_unmappable) = encoding.encode(input);
    if had_unmappable {
        return Err(Error::Runtime(format!(
            "encode: input contains a character unmappable in encoding \"{}\"",
            encoding.name()
        )));
    }
    Ok(RuntimeValue::Bytes(Shared::new(encoded.into_owned())))
}

#[inline(always)]
fn bytes_to_hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
        write!(s, "{b:02x}").unwrap();
        s
    })
}

/// Shell escape for safe use in shell commands
#[inline(always)]
pub(super) fn shell_escape(input: &str) -> Result<RuntimeValue, Error> {
    // If the string is empty, return empty quotes
    if input.is_empty() {
        return Ok(RuntimeValue::String(Shared::new("''".to_string())));
    }

    // Check if the string contains characters that need escaping
    let needs_quoting = input.chars().any(|c| {
        !matches!(c,
            'a'..='z' | 'A'..='Z' | '0'..='9' |
            '-' | '_' | '=' | '/' | '.' | ',' | ':' | '@'
        )
    });

    if !needs_quoting {
        // Safe to use without quoting
        return Ok(RuntimeValue::String(Shared::new(input.to_string())));
    }

    // Use single quotes and escape any single quotes in the string
    // by replacing ' with '\''
    let escaped = input.replace('\'', "'\\''");
    Ok(RuntimeValue::String(Shared::new(format!("'{}'", escaped))))
}

pub(super) fn flatten(args: Vec<RuntimeValue>) -> Vec<RuntimeValue> {
    let mut result = Vec::new();
    for arg in args {
        match arg {
            RuntimeValue::Array(arr) => result.extend(flatten(crate::Shared::unwrap_or_clone(arr))),
            other => result.push(other),
        }
    }
    result
}

#[cfg(test)]
mod tests;
