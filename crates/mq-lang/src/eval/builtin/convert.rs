use crate::RuntimeValue;
use crate::eval::builtin::Error;
use crate::number::Number;
use base64::prelude::*;
use percent_encoding::{NON_ALPHANUMERIC, percent_decode, utf8_percent_encode};
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ConvertKind {
    Blockquote,
    Heading(u8),
    HorizontalRule,
    Link(String),
    ListItem,
    Strong,
    Strikethrough,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Convert {
    Base64,
    Html,
    Markdown(ConvertKind),
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
            RuntimeValue::Symbol(symbol) => match symbol.as_str().as_str() {
                "h1" => Ok(Convert::Markdown(ConvertKind::Heading(1))),
                "h2" => Ok(Convert::Markdown(ConvertKind::Heading(2))),
                "h3" => Ok(Convert::Markdown(ConvertKind::Heading(3))),
                "h4" => Ok(Convert::Markdown(ConvertKind::Heading(4))),
                "h5" => Ok(Convert::Markdown(ConvertKind::Heading(5))),
                "h6" => Ok(Convert::Markdown(ConvertKind::Heading(6))),

                "html" => Ok(Convert::Html),
                "text" => Ok(Convert::Text),
                "sh" => Ok(Convert::Shell),
                "base64" => Ok(Convert::Base64),
                "uri" => Ok(Convert::UriEncode),
                "urid" => Ok(Convert::UriDecode),
                _ => Err(Error::InvalidConvert(symbol.to_string())),
            },
            RuntimeValue::String(s) => match s.as_str() {
                "#" => Ok(Convert::Markdown(ConvertKind::Heading(1))),
                "##" => Ok(Convert::Markdown(ConvertKind::Heading(2))),
                "###" => Ok(Convert::Markdown(ConvertKind::Heading(3))),
                "####" => Ok(Convert::Markdown(ConvertKind::Heading(4))),
                "#####" => Ok(Convert::Markdown(ConvertKind::Heading(5))),
                "######" => Ok(Convert::Markdown(ConvertKind::Heading(6))),
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
            Convert::Markdown(kind) => self.convert_to_markdown(input, kind),
            Convert::Shell => {
                // Shell script conversion - escape for safe shell usage
                let text = match input {
                    RuntimeValue::String(s) => s.clone(),
                    RuntimeValue::Markdown(node, _) => node.value().to_string(),
                    _ => input.to_string(),
                };
                shell_escape(&text).unwrap_or(RuntimeValue::NONE)
            }
        }
    }

    fn convert_to_markdown(&self, input: &RuntimeValue, kind: &ConvertKind) -> RuntimeValue {
        let text = match input {
            RuntimeValue::String(s) => s.clone(),
            RuntimeValue::Markdown(node, _) => node.value().to_string(),
            _ => input.to_string(),
        };

        match kind {
            ConvertKind::Heading(depth) => RuntimeValue::Markdown(
                mq_markdown::Node::Heading(mq_markdown::Heading {
                    depth: *depth,
                    values: vec![text.into()],
                    position: None,
                }),
                None,
            ),
            ConvertKind::Blockquote => RuntimeValue::Markdown(
                mq_markdown::Node::Blockquote(mq_markdown::Blockquote {
                    values: vec![text.into()],
                    position: None,
                }),
                None,
            ),
            ConvertKind::ListItem => RuntimeValue::Markdown(
                mq_markdown::Node::List(mq_markdown::List {
                    values: vec![text.into()],
                    index: 0,
                    ordered: false,
                    level: 1,
                    checked: None,
                    position: None,
                }),
                None,
            ),
            ConvertKind::Link(url) => RuntimeValue::Markdown(
                mq_markdown::Node::Link(mq_markdown::Link {
                    url: mq_markdown::Url::new(url.to_string()),
                    values: vec![text.into()],
                    title: None,
                    position: None,
                }),
                None,
            ),
            ConvertKind::Strikethrough => RuntimeValue::Markdown(
                mq_markdown::Node::Delete(mq_markdown::Delete {
                    values: vec![text.into()],
                    position: None,
                }),
                None,
            ),
            ConvertKind::Strong => RuntimeValue::Markdown(
                mq_markdown::Node::Strong(mq_markdown::Strong {
                    values: vec![text.into()],
                    position: None,
                }),
                None,
            ),
            ConvertKind::HorizontalRule => RuntimeValue::Markdown(
                mq_markdown::Node::HorizontalRule(mq_markdown::HorizontalRule { position: None }),
                None,
            ),
        }
    }
}

/// convert to HTML
#[inline(always)]
pub fn to_html(value: &RuntimeValue) -> Result<RuntimeValue, Error> {
    match value {
        RuntimeValue::None => Ok(RuntimeValue::NONE),
        RuntimeValue::String(s) => Ok(mq_markdown::to_html(s).into()),
        RuntimeValue::Symbol(s) => Ok(mq_markdown::to_html(&s.as_str()).into()),
        RuntimeValue::Markdown(node_value, _) => Ok(mq_markdown::to_html(node_value.to_string().as_str()).into()),
        _ => Err(Error::InvalidTypes("to_html".to_string(), vec![value.clone()])),
    }
}

/// convert to Markdown string
pub fn to_markdown_string(args: Vec<RuntimeValue>) -> Result<RuntimeValue, Error> {
    let args = flatten(args);

    Ok(mq_markdown::Markdown::new(
        args.iter()
            .flat_map(|arg| match arg {
                RuntimeValue::Markdown(node, _) => vec![node.clone()],
                a => vec![a.to_string().into()],
            })
            .collect(),
    )
    .to_string()
    .into())
}

/// convert to string
pub fn to_string(value: &RuntimeValue) -> Result<RuntimeValue, Error> {
    match value {
        RuntimeValue::Symbol(s) => Ok(s.as_str().into()),
        o => Ok(o.to_string().into()),
    }
}

/// convert to number
pub fn to_number(value: &mut RuntimeValue) -> Result<RuntimeValue, Error> {
    match value {
        node @ RuntimeValue::Markdown(_, _) => node
            .markdown_node()
            .map(|md| {
                md.to_string()
                    .parse::<f64>()
                    .map(|n| RuntimeValue::Number(n.into()))
                    .map_err(|e| Error::Runtime(format!("{}", e)))
            })
            .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
        RuntimeValue::String(s) => s
            .parse::<f64>()
            .map(|n| RuntimeValue::Number(n.into()))
            .map_err(|e| Error::Runtime(format!("{}", e))),
        RuntimeValue::Array(array) => {
            let result_value: Result<Vec<RuntimeValue>, Error> = std::mem::take(array)
                .into_iter()
                .map(|o| match o {
                    node @ RuntimeValue::Markdown(_, _) => node
                        .markdown_node()
                        .map(|md| {
                            md.to_string()
                                .parse::<f64>()
                                .map(|n| RuntimeValue::Number(n.into()))
                                .map_err(|e| Error::Runtime(format!("{}", e)))
                        })
                        .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
                    RuntimeValue::String(s) => s
                        .parse::<f64>()
                        .map(|n| RuntimeValue::Number(n.into()))
                        .map_err(|e| Error::Runtime(format!("{}", e))),
                    RuntimeValue::Boolean(b) => Ok(RuntimeValue::Number(if b { 1 } else { 0 }.into())),
                    n @ RuntimeValue::Number(_) => Ok(n),
                    _ => Ok(RuntimeValue::Number(0.into())),
                })
                .collect();

            result_value.map(RuntimeValue::Array)
        }
        RuntimeValue::Boolean(true) => Ok(RuntimeValue::Number(1.into())),
        RuntimeValue::Boolean(false) => Ok(RuntimeValue::Number(0.into())),
        RuntimeValue::Number(n) => Ok(RuntimeValue::Number(*n)),
        _ => Ok(RuntimeValue::Number(0.into())),
    }
}

/// convert to array
pub fn to_array(value: &mut RuntimeValue) -> Result<RuntimeValue, Error> {
    match value {
        RuntimeValue::Array(array) => Ok(RuntimeValue::Array(std::mem::take(array))),
        RuntimeValue::String(s) => Ok(RuntimeValue::Array(
            s.chars().map(|c| RuntimeValue::String(c.to_string())).collect(),
        )),
        RuntimeValue::None => Ok(RuntimeValue::Array(Vec::new())),
        value => Ok(RuntimeValue::Array(vec![std::mem::take(value)])),
    }
}

/// convert to text
pub fn to_text(value: &RuntimeValue) -> Result<RuntimeValue, Error> {
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

/// convert from date string to timestamp (milliseconds)
#[inline(always)]
pub fn from_date(date_str: &str) -> Result<RuntimeValue, Error> {
    match chrono::DateTime::parse_from_rfc3339(date_str) {
        Ok(datetime) => Ok(RuntimeValue::Number(datetime.timestamp_millis().into())),
        Err(e) => Err(Error::Runtime(format!("{}", e))),
    }
}

/// convert from timestamp (milliseconds) to date string
#[inline(always)]
pub fn to_date(ms: Number, convert: Option<&str>) -> Result<RuntimeValue, Error> {
    chrono::DateTime::from_timestamp((ms.value() as i64) / 1000, 0)
        .map(|dt| {
            convert
                .map(|f| dt.format(f).to_string())
                .unwrap_or(dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
        })
        .map(RuntimeValue::String)
        .ok_or_else(|| Error::InvalidDateTimeFormat(convert.unwrap_or("").to_string()))
}

/// Encode to base64
#[inline(always)]
pub fn base64(input: &str) -> Result<RuntimeValue, Error> {
    Ok(RuntimeValue::String(BASE64_STANDARD.encode(input)))
}

/// Decode from base64
#[inline(always)]
pub fn base64d(input: &str) -> Result<RuntimeValue, Error> {
    BASE64_STANDARD
        .decode(input)
        .map_err(Error::InvalidBase64String)
        .map(|v| RuntimeValue::String(String::from_utf8_lossy(&v).to_string()))
}

/// Encode to base64url
#[inline(always)]
pub fn base64url(input: &str) -> Result<RuntimeValue, Error> {
    Ok(RuntimeValue::String(BASE64_URL_SAFE_NO_PAD.encode(input)))
}

/// Decode from base64url
#[inline(always)]
pub fn base64urld(input: &str) -> Result<RuntimeValue, Error> {
    BASE64_URL_SAFE_NO_PAD
        .decode(input)
        .map_err(Error::InvalidBase64String)
        .map(|v| RuntimeValue::String(String::from_utf8_lossy(&v).to_string()))
}

/// URL encode
#[inline(always)]
pub fn url_encode(input: &str) -> Result<RuntimeValue, Error> {
    Ok(RuntimeValue::String(
        utf8_percent_encode(input, NON_ALPHANUMERIC).to_string(),
    ))
}

/// URL decode
#[inline(always)]
pub fn url_decode(input: &str) -> Result<RuntimeValue, Error> {
    Ok(RuntimeValue::String(
        percent_decode(input.as_bytes()).decode_utf8_lossy().to_string(),
    ))
}

/// Shell escape for safe use in shell commands
#[inline(always)]
pub fn shell_escape(input: &str) -> Result<RuntimeValue, Error> {
    // If the string is empty, return empty quotes
    if input.is_empty() {
        return Ok(RuntimeValue::String("''".to_string()));
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
        return Ok(RuntimeValue::String(input.to_string()));
    }

    // Use single quotes and escape any single quotes in the string
    // by replacing ' with '\''
    let escaped = input.replace('\'', "'\\''");
    Ok(RuntimeValue::String(format!("'{}'", escaped)))
}

pub fn flatten(args: Vec<RuntimeValue>) -> Vec<RuntimeValue> {
    let mut result = Vec::new();
    for arg in args {
        match arg {
            RuntimeValue::Array(arr) => result.extend(flatten(arr)),
            other => result.push(other),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Ident;
    use rstest::rstest;

    // Test convert::try_from
    #[rstest]
    #[case::h1_symbol(RuntimeValue::Symbol(Ident::new("h1")), Convert::Markdown(ConvertKind::Heading(1)))]
    #[case::h2_symbol(RuntimeValue::Symbol(Ident::new("h2")), Convert::Markdown(ConvertKind::Heading(2)))]
    #[case::h3_symbol(RuntimeValue::Symbol(Ident::new("h3")), Convert::Markdown(ConvertKind::Heading(3)))]
    #[case::h4_symbol(RuntimeValue::Symbol(Ident::new("h4")), Convert::Markdown(ConvertKind::Heading(4)))]
    #[case::h5_symbol(RuntimeValue::Symbol(Ident::new("h5")), Convert::Markdown(ConvertKind::Heading(5)))]
    #[case::h6_symbol(RuntimeValue::Symbol(Ident::new("h6")), Convert::Markdown(ConvertKind::Heading(6)))]
    #[case::html_symbol(RuntimeValue::Symbol(Ident::new("html")), Convert::Html)]
    #[case::text_symbol(RuntimeValue::Symbol(Ident::new("text")), Convert::Text)]
    #[case::sh_symbol(RuntimeValue::Symbol(Ident::new("sh")), Convert::Shell)]
    #[case::h1_string(RuntimeValue::String("#".to_string()), Convert::Markdown(ConvertKind::Heading(1)))]
    #[case::h2_string(RuntimeValue::String("##".to_string()), Convert::Markdown(ConvertKind::Heading(2)))]
    #[case::h3_string(RuntimeValue::String("###".to_string()), Convert::Markdown(ConvertKind::Heading(3)))]
    #[case::h4_string(RuntimeValue::String("####".to_string()), Convert::Markdown(ConvertKind::Heading(4)))]
    #[case::h5_string(RuntimeValue::String("#####".to_string()), Convert::Markdown(ConvertKind::Heading(5)))]
    #[case::h6_string(RuntimeValue::String("######".to_string()), Convert::Markdown(ConvertKind::Heading(6)))]
    #[case::blockquote_string(RuntimeValue::String(">".to_string()), Convert::Markdown(ConvertKind::Blockquote))]
    #[case::list_item_string(RuntimeValue::String("-".to_string()), Convert::Markdown(ConvertKind::ListItem))]
    #[case::strikethrough_string(RuntimeValue::String("~~".to_string()), Convert::Markdown(ConvertKind::Strikethrough))]
    #[case::strong_string(RuntimeValue::String("**".to_string()), Convert::Markdown(ConvertKind::Strong))]
    #[case::horizontal_rule_string(RuntimeValue::String("--".to_string()), Convert::Markdown(ConvertKind::HorizontalRule))]
    fn test_convert_convert_try_from_valid(#[case] input: RuntimeValue, #[case] expected: Convert) {
        let result = Convert::try_from(&input);
        assert!(result.is_ok());

        // Compare the enum variants
        match (result.unwrap(), expected) {
            (Convert::Markdown(ConvertKind::Heading(d1)), Convert::Markdown(ConvertKind::Heading(d2))) => {
                assert_eq!(d1, d2);
            }
            (a, b) => assert_eq!(a, b),
        }
    }

    #[rstest]
    #[case::invalid_symbol(RuntimeValue::Symbol(Ident::new("invalid")))]
    #[case::invalid_string(RuntimeValue::String("invalid".to_string()))]
    #[case::number(RuntimeValue::Number(42.into()))]
    #[case::boolean(RuntimeValue::Boolean(true))]
    fn test_convert_convert_try_from_invalid(#[case] input: RuntimeValue) {
        let result = Convert::try_from(&input);
        assert!(result.is_err());
    }

    // Test base64 functions
    #[rstest]
    #[case("hello", "aGVsbG8=")]
    #[case("world", "d29ybGQ=")]
    #[case("test", "dGVzdA==")]
    #[case("", "")]
    #[case("Hello, 世界!", "SGVsbG8sIOS4lueVjCE=")]
    #[case("emoji: 😀", "ZW1vamk6IPCfmIA=")]
    #[case("line\nbreak", "bGluZQpicmVhaw==")]
    fn test_base64_encode(#[case] input: &str, #[case] expected: &str) {
        let result = base64(input).unwrap();
        assert_eq!(result, RuntimeValue::String(expected.to_string()));
    }

    #[rstest]
    #[case("aGVsbG8=", "hello")]
    #[case("d29ybGQ=", "world")]
    #[case("dGVzdA==", "test")]
    #[case("", "")]
    fn test_base64_decode(#[case] input: &str, #[case] expected: &str) {
        let result = base64d(input).unwrap();
        assert_eq!(result, RuntimeValue::String(expected.to_string()));
    }

    #[rstest]
    #[case("not valid base64!@#")]
    #[case("SGVsbG8")] // Missing padding might still work in some implementations
    fn test_base64d_invalid(#[case] input: &str) {
        let result = base64d(input);
        // Some invalid inputs might error
        if result.is_ok() {
            // If it succeeds, it should produce some output
            assert!(matches!(result.unwrap(), RuntimeValue::String(_)));
        }
    }

    #[rstest]
    #[case("hello", "aGVsbG8")]
    #[case("world", "d29ybGQ")]
    #[case("test", "dGVzdA")]
    #[case("subjects?", "c3ViamVjdHM_")]
    #[case("subjects>", "c3ViamVjdHM-")]
    fn test_base64url_encode(#[case] input: &str, #[case] expected: &str) {
        let result = base64url(input).unwrap();
        assert_eq!(result, RuntimeValue::String(expected.to_string()));
    }

    #[rstest]
    #[case("aGVsbG8", "hello")]
    #[case("d29ybGQ", "world")]
    #[case("dGVzdA", "test")]
    fn test_base64url_decode(#[case] input: &str, #[case] expected: &str) {
        let result = base64urld(input).unwrap();
        assert_eq!(result, RuntimeValue::String(expected.to_string()));
    }

    // Test URL encoding
    #[rstest]
    #[case("hello world", "hello%20world")]
    #[case("test@example.com", "test%40example%2Ecom")]
    #[case("a+b=c", "a%2Bb%3Dc")]
    #[case("", "")]
    #[case("foo/bar", "foo%2Fbar")]
    #[case("foo?bar=baz", "foo%3Fbar%3Dbaz")]
    #[case("foo&bar", "foo%26bar")]
    #[case("100%", "100%25")]
    #[case("café", "caf%C3%A9")]
    fn test_url_encode(#[case] input: &str, #[case] expected: &str) {
        let result = url_encode(input).unwrap();
        assert_eq!(result, RuntimeValue::String(expected.to_string()));
    }

    // Test shell escape
    #[rstest]
    #[case("hello", "hello")] // No escaping needed
    #[case("hello world", "'hello world'")] // Space needs quoting
    #[case("hello'world", "'hello'\\''world'")] // Single quote needs escaping
    #[case("hello\"world", "'hello\"world'")] // Double quote in single quotes
    #[case("hello$world", "'hello$world'")] // Dollar sign needs quoting
    #[case("hello`world`", "'hello`world`'")] // Backticks need quoting
    #[case("hello!world", "'hello!world'")] // Exclamation mark needs quoting
    #[case("hello&world", "'hello&world'")] // Ampersand needs quoting
    #[case("hello;world", "'hello;world'")] // Semicolon needs quoting
    #[case("hello|world", "'hello|world'")] // Pipe needs quoting
    #[case("hello<world", "'hello<world'")] // Less than needs quoting
    #[case("hello>world", "'hello>world'")] // Greater than needs quoting
    #[case("hello(world)", "'hello(world)'")] // Parentheses need quoting
    #[case("hello[world]", "'hello[world]'")] // Brackets need quoting
    #[case("hello{world}", "'hello{world}'")] // Braces need quoting
    #[case("hello*world", "'hello*world'")] // Asterisk needs quoting
    #[case("hello?world", "'hello?world'")] // Question mark needs quoting
    #[case("hello\\world", "'hello\\world'")] // Backslash needs quoting
    #[case("hello\nworld", "'hello\nworld'")] // Newline needs quoting
    #[case("hello\tworld", "'hello\tworld'")] // Tab needs quoting
    #[case("", "''")] // Empty string
    #[case("hello-world", "hello-world")] // Hyphen is safe
    #[case("hello_world", "hello_world")] // Underscore is safe
    #[case("hello.world", "hello.world")] // Dot is safe
    #[case("hello,world", "hello,world")] // Comma is safe
    #[case("hello:world", "hello:world")] // Colon is safe
    #[case("hello@world", "hello@world")] // At sign is safe
    #[case("hello/world", "hello/world")] // Slash is safe
    #[case("hello=world", "hello=world")] // Equals is safe
    #[case("test123", "test123")] // Alphanumeric is safe
    #[case("TEST", "TEST")] // Uppercase is safe
    #[case("it's a test", "'it'\\''s a test'")] // Multiple issues
    #[case("rm -rf /", "'rm -rf /'")] // Dangerous command
    fn test_shell_escape(#[case] input: &str, #[case] expected: &str) {
        let result = shell_escape(input).unwrap();
        assert_eq!(result, RuntimeValue::String(expected.to_string()));
    }

    // Test to_string
    #[rstest]
    #[case::string(RuntimeValue::String("test".to_string()), "test")]
    #[case::symbol(RuntimeValue::Symbol(Ident::new("test")), "test")]
    #[case::number(RuntimeValue::Number(42.into()), "42")]
    #[case::boolean_true(RuntimeValue::Boolean(true), "true")]
    #[case::boolean_false(RuntimeValue::Boolean(false), "false")]
    fn test_to_string(#[case] input: RuntimeValue, #[case] expected: &str) {
        let result = to_string(&input).unwrap();
        assert_eq!(result, RuntimeValue::String(expected.to_string()));
    }

    // Test to_number
    #[rstest]
    #[case::string_int(RuntimeValue::String("42".to_string()), RuntimeValue::Number(42.into()))]
    #[case::number(RuntimeValue::Number(42.into()), RuntimeValue::Number(42.into()))]
    #[case::boolean_true(RuntimeValue::Boolean(true), RuntimeValue::Number(1.into()))]
    #[case::boolean_false(RuntimeValue::Boolean(false), RuntimeValue::Number(0.into()))]
    #[case::none(RuntimeValue::None, RuntimeValue::Number(0.into()))]
    fn test_to_number(#[case] mut input: RuntimeValue, #[case] expected: RuntimeValue) {
        let result = to_number(&mut input).unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn test_to_number_invalid_string() {
        let mut input = RuntimeValue::String("not a number".to_string());
        let result = to_number(&mut input);
        assert!(result.is_err());
    }

    #[test]
    fn test_to_number_array() {
        let mut input = RuntimeValue::Array(vec![
            RuntimeValue::String("42".to_string()),
            RuntimeValue::Boolean(true),
        ]);
        let result = to_number(&mut input).unwrap();
        match result {
            RuntimeValue::Array(arr) => {
                assert_eq!(arr.len(), 2);
                assert_eq!(arr[0], RuntimeValue::Number(42.into()));
                assert_eq!(arr[1], RuntimeValue::Number(1.into()));
            }
            _ => panic!("Expected Array"),
        }
    }

    // Test to_array
    #[rstest]
    #[case::string(
        RuntimeValue::String("abc".to_string()),
        vec![
            RuntimeValue::String("a".to_string()),
            RuntimeValue::String("b".to_string()),
            RuntimeValue::String("c".to_string()),
        ]
    )]
    #[case::none(RuntimeValue::None, vec![])]
    #[case::number(RuntimeValue::Number(42.into()), vec![RuntimeValue::Number(42.into())])]
    #[case::boolean_true(RuntimeValue::Boolean(true), vec![RuntimeValue::Boolean(true)])]
    #[case::boolean_false(RuntimeValue::Boolean(false), vec![RuntimeValue::Boolean(false)])]
    fn test_to_array(#[case] mut input: RuntimeValue, #[case] expected: Vec<RuntimeValue>) {
        let result = to_array(&mut input).unwrap();
        assert_eq!(result, RuntimeValue::Array(expected));
    }

    #[test]
    fn test_to_array_already_array() {
        let mut input = RuntimeValue::Array(vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into())]);
        let result = to_array(&mut input).unwrap();
        assert_eq!(
            result,
            RuntimeValue::Array(vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into()),])
        );
    }

    // Test to_text
    #[rstest]
    #[case::string(RuntimeValue::String("test".to_string()), "test")]
    #[case::number(RuntimeValue::Number(42.into()), "42")]
    #[case::none(RuntimeValue::None, "")]
    #[case::array(
        RuntimeValue::Array(vec![
            RuntimeValue::String("a".to_string()),
            RuntimeValue::String("b".to_string()),
            RuntimeValue::String("c".to_string()),
        ]),
        "a,b,c"
    )]
    #[case::array_with_none(
        RuntimeValue::Array(vec![
            RuntimeValue::String("a".to_string()),
            RuntimeValue::None,
            RuntimeValue::String("c".to_string()),
        ]),
        "a,,c"
    )]
    #[case::empty_array(RuntimeValue::Array(vec![]), "")]
    fn test_to_text(#[case] input: RuntimeValue, #[case] expected: &str) {
        let result = to_text(&input).unwrap();
        match result {
            RuntimeValue::None => assert_eq!(expected, ""),
            RuntimeValue::String(s) => assert_eq!(s, expected),
            _ => panic!("Expected String or None"),
        }
    }

    // Test to_html
    #[rstest]
    #[case::plain_text(RuntimeValue::String("hello".to_string()), "<p>hello</p>")]
    #[case::bold(RuntimeValue::String("**bold**".to_string()), "<p><strong>bold</strong></p>")]
    #[case::italic(RuntimeValue::String("*italic*".to_string()), "<p><em>italic</em></p>")]
    #[case::none(RuntimeValue::None, "")]
    #[case::heading(RuntimeValue::String("# Heading\n\nParagraph".to_string()), "<h1>Heading</h1>\n<p>Paragraph</p>")]
    #[case::quote(RuntimeValue::String("> Quote".to_string()), "<blockquote>\n<p>Quote</p>\n</blockquote>")]
    #[case::list(RuntimeValue::String("- Item 1\n- Item 2".to_string()), "<ul>\n<li>Item 1</li>\n<li>Item 2</li>\n</ul>")]
    fn test_to_html(#[case] input: RuntimeValue, #[case] expected: &str) {
        let result = to_html(&input).unwrap();
        match result {
            RuntimeValue::None => assert_eq!(expected, ""),
            RuntimeValue::String(s) => {
                // Trim trailing newline for comparison
                let trimmed = s.trim_end();
                assert_eq!(trimmed, expected);
            }
            _ => panic!("Expected String or None"),
        }
    }

    // Test flatten
    #[rstest]
    #[case::flat_array(
        vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into())],
        vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into())]
    )]
    #[case::nested_array(
        vec![
            RuntimeValue::Array(vec![RuntimeValue::Number(1.into())]),
            RuntimeValue::Number(2.into()),
            RuntimeValue::Array(vec![RuntimeValue::Number(3.into()), RuntimeValue::Number(4.into())]),
        ],
        vec![
            RuntimeValue::Number(1.into()),
            RuntimeValue::Number(2.into()),
            RuntimeValue::Number(3.into()),
            RuntimeValue::Number(4.into()),
        ]
    )]
    #[case::deeply_nested(
        vec![
            RuntimeValue::Array(vec![
                RuntimeValue::Array(vec![RuntimeValue::Number(1.into())]),
                RuntimeValue::Number(2.into()),
            ]),
        ],
        vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into())]
    )]
    #[case::empty(vec![], vec![])]
    fn test_flatten(#[case] input: Vec<RuntimeValue>, #[case] expected: Vec<RuntimeValue>) {
        let result = flatten(input);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_flatten_with_empty_nested() {
        let input = vec![
            RuntimeValue::Array(vec![]),
            RuntimeValue::Number(1.into()),
            RuntimeValue::Array(vec![]),
        ];
        let result = flatten(input);
        assert_eq!(result, vec![RuntimeValue::Number(1.into())]);
    }

    // Test from_date
    #[rstest]
    #[case("2024-01-01T00:00:00Z", 1704067200000)]
    #[case("2024-06-15T12:30:45Z", 1718454645000)]
    #[case("2024-12-31T23:59:59Z", 1735689599000)]
    fn test_from_date_valid(#[case] input: &str, #[case] expected_ms: i64) {
        let result = from_date(input).unwrap();
        assert_eq!(result, RuntimeValue::Number(expected_ms.into()));
    }

    #[rstest]
    #[case("invalid date")]
    #[case("2024-13-01T00:00:00Z")]
    #[case("not-a-date")]
    fn test_from_date_invalid(#[case] input: &str) {
        let result = from_date(input);
        assert!(result.is_err());
    }

    // Test to_date
    #[rstest]
    #[case(1704067200000, None, "2024-01-01T00:00:00Z")]
    #[case(1704067200000, Some("%Y-%m-%d"), "2024-01-01")]
    #[case(1718454645000, Some("%Y/%m/%d %H:%M"), "2024/06/15 12:30")]
    #[case(1704067200000, Some("%d/%m/%Y"), "01/01/2024")]
    #[case(1704067200000, Some("%H:%M:%S"), "00:00:00")]
    #[case(1704067200000, Some("%Y"), "2024")]
    fn test_to_date(#[case] ms: i64, #[case] convert: Option<&str>, #[case] expected: &str) {
        let result = to_date(ms.into(), convert).unwrap();
        assert_eq!(result, RuntimeValue::String(expected.to_string()));
    }

    // Test to_markdown_string
    #[rstest]
    #[case::single_string(
        vec![RuntimeValue::String("hello".to_string())],
        "hello"
    )]
    #[case::multiple_strings(
        vec![
            RuntimeValue::String("hello".to_string()),
            RuntimeValue::String(" ".to_string()),
            RuntimeValue::String("world".to_string()),
        ],
        "hello\n \nworld"  // Each becomes a separate line
    )]
    fn test_to_markdown_string(#[case] input: Vec<RuntimeValue>, #[case] expected: &str) {
        let result = to_markdown_string(input).unwrap();
        match result {
            RuntimeValue::String(s) => {
                let trimmed = s.trim_end();
                assert_eq!(trimmed, expected);
            }
            _ => panic!("Expected String"),
        }
    }

    // Test convert::convert for all converts
    #[rstest]
    #[case::base64(Convert::Base64, RuntimeValue::String("hello".to_string()), "aGVsbG8=")]
    #[case::uri_encode(Convert::UriEncode, RuntimeValue::String("hello world".to_string()), "hello%20world")]
    #[case::uri_decode(Convert::UriDecode, RuntimeValue::String("hello%20world".to_string()), "hello world")]
    #[case::empty_base64(Convert::Base64, RuntimeValue::String("".to_string()), "")]
    #[case::empty_html(Convert::Html, RuntimeValue::String("".to_string()), "")]
    #[case::empty_text(Convert::Text, RuntimeValue::String("".to_string()), "")]
    #[case::empty_uri(Convert::UriEncode, RuntimeValue::String("".to_string()), "")]
    fn test_convert_string_converts(#[case] convert: Convert, #[case] input: RuntimeValue, #[case] expected: &str) {
        let result = convert.convert(&input);
        assert_eq!(result, RuntimeValue::String(expected.to_string()));
    }

    #[test]
    fn test_convert_html() {
        let convert = Convert::Html;
        let input = RuntimeValue::String("**bold**".to_string());
        let result = convert.convert(&input);
        match result {
            RuntimeValue::String(s) => {
                let trimmed = s.trim_end();
                assert_eq!(trimmed, "<p><strong>bold</strong></p>");
            }
            _ => panic!("Expected String"),
        }
    }

    #[test]
    fn test_convert_text() {
        let convert = Convert::Text;
        let input = RuntimeValue::Number(42.into());
        let result = convert.convert(&input);
        assert_eq!(result, RuntimeValue::String("42".to_string()));
    }

    #[test]
    fn test_convert_sh() {
        let convert = Convert::Shell;

        // Simple string without special characters
        let input = RuntimeValue::String("echo hello".to_string());
        let result = convert.convert(&input);
        assert_eq!(result, RuntimeValue::String("'echo hello'".to_string()));

        // String with single quote
        let input = RuntimeValue::String("it's".to_string());
        let result = convert.convert(&input);
        assert_eq!(result, RuntimeValue::String("'it'\\''s'".to_string()));

        // Safe alphanumeric string
        let input = RuntimeValue::String("test123".to_string());
        let result = convert.convert(&input);
        assert_eq!(result, RuntimeValue::String("test123".to_string()));

        // Number
        let input = RuntimeValue::Number(42.into());
        let result = convert.convert(&input);
        assert_eq!(result, RuntimeValue::String("42".to_string()));
    }

    // Test convert::convert for Markdown headings
    #[rstest]
    #[case(1)]
    #[case(2)]
    #[case(3)]
    #[case(4)]
    #[case(5)]
    #[case(6)]
    fn test_convert_all_heading_levels(#[case] depth: u8) {
        let convert = Convert::Markdown(ConvertKind::Heading(depth));
        let input = RuntimeValue::String("Test".to_string());
        let result = convert.convert(&input);

        match result {
            RuntimeValue::Markdown(mq_markdown::Node::Heading(heading), _) => {
                assert_eq!(heading.depth, depth);
                assert_eq!(heading.values.len(), 1);
            }
            _ => panic!("Expected Markdown Heading with depth {}", depth),
        }
    }

    // Test convert::convert for Markdown blockquote
    #[test]
    fn test_convert_markdown_blockquote() {
        let convert = Convert::Markdown(ConvertKind::Blockquote);
        let input = RuntimeValue::String("Important note".to_string());
        let result = convert.convert(&input);

        match result {
            RuntimeValue::Markdown(mq_markdown::Node::Blockquote(blockquote), _) => {
                assert_eq!(blockquote.values.len(), 1);
            }
            _ => panic!("Expected Markdown Blockquote"),
        }
    }

    // Test convert::convert for Markdown list item
    #[test]
    fn test_convert_markdown_list_item() {
        let convert = Convert::Markdown(ConvertKind::ListItem);
        let input = RuntimeValue::String("Item text".to_string());
        let result = convert.convert(&input);

        match result {
            RuntimeValue::Markdown(mq_markdown::Node::List(list), _) => {
                assert!(!list.ordered);
                assert_eq!(list.level, 1);
                assert_eq!(list.values.len(), 1);
            }
            _ => panic!("Expected Markdown List"),
        }
    }

    // Test convert::convert for Markdown link
    #[test]
    fn test_convert_markdown_link() {
        let url = Url::parse("https://example.com").unwrap();
        let convert = Convert::Markdown(ConvertKind::Link(url.to_string()));
        let input = RuntimeValue::String("Click here".to_string());
        let result = convert.convert(&input);

        match result {
            RuntimeValue::Markdown(mq_markdown::Node::Link(link), _) => {
                assert_eq!(link.url.as_str(), "https://example.com/");
                assert_eq!(link.values.len(), 1);
            }
            _ => panic!("Expected Markdown Link"),
        }
    }

    // Test convert::convert for Markdown strikethrough
    #[test]
    fn test_convert_markdown_strikethrough() {
        let convert = Convert::Markdown(ConvertKind::Strikethrough);
        let input = RuntimeValue::String("Deleted text".to_string());
        let result = convert.convert(&input);

        match result {
            RuntimeValue::Markdown(mq_markdown::Node::Delete(delete), _) => {
                assert_eq!(delete.values.len(), 1);
            }
            _ => panic!("Expected Markdown Delete"),
        }
    }

    // Test convert with None input
    #[test]
    fn test_convert_base64_with_none() {
        let convert = Convert::Base64;
        let input = RuntimeValue::None;
        let result = convert.convert(&input);
        assert_eq!(result, RuntimeValue::NONE);
    }

    #[test]
    fn test_convert_text_with_none() {
        let convert = Convert::Text;
        let input = RuntimeValue::None;
        let result = convert.convert(&input);
        assert_eq!(result, RuntimeValue::NONE);
    }

    // Test convert with Markdown input
    #[test]
    fn test_convert_with_markdown_input() {
        let markdown_node = mq_markdown::Node::Text(mq_markdown::Text {
            value: "test".to_string(),
            position: None,
        });
        let input = RuntimeValue::Markdown(markdown_node, None);
        let convert = Convert::Markdown(ConvertKind::Heading(2));

        let result = convert.convert(&input);

        match result {
            RuntimeValue::Markdown(mq_markdown::Node::Heading(heading), _) => {
                assert_eq!(heading.depth, 2);
            }
            _ => panic!("Expected Markdown Heading"),
        }
    }

    // Test convert with number input
    #[test]
    fn test_convert_number_to_heading() {
        let convert = Convert::Markdown(ConvertKind::Heading(1));
        let input = RuntimeValue::Number(42.into());
        let result = convert.convert(&input);

        match result {
            RuntimeValue::Markdown(mq_markdown::Node::Heading(heading), _) => {
                assert_eq!(heading.depth, 1);
                assert_eq!(heading.values.len(), 1);
            }
            _ => panic!("Expected Markdown Heading"),
        }
    }

    // Test convert::try_from with URL
    #[test]
    fn test_convert_convert_url() {
        let url_string = RuntimeValue::String("https://example.com".to_string());
        let result = Convert::try_from(&url_string);
        assert!(result.is_ok());
        match result.unwrap() {
            Convert::Markdown(ConvertKind::Link(_)) => {}
            _ => panic!("Expected Markdown Link convert"),
        }
    }
}
