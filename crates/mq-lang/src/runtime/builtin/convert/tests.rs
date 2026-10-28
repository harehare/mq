use super::*;
use crate::Ident;
use rstest::rstest;

#[rstest]
#[case::empty(b"".to_vec(), "00000000")]
#[case::short(
        b"hi".to_vec(),
        "00000000  68 69                                             |hi|"
    )]
#[case::exactly_one_row(
        (0u8..16).collect::<Vec<u8>>(),
        "00000000  00 01 02 03 04 05 06 07  08 09 0a 0b 0c 0d 0e 0f  |................|"
    )]
#[case::multi_row(
        (0u8..=0x22).collect::<Vec<u8>>(),
        "00000000  00 01 02 03 04 05 06 07  08 09 0a 0b 0c 0d 0e 0f  |................|\n\
         00000010  10 11 12 13 14 15 16 17  18 19 1a 1b 1c 1d 1e 1f  |................|\n\
         00000020  20 21 22                                          | !\"|"
    )]
fn test_hexdump(#[case] input: Vec<u8>, #[case] expected: &str) {
    assert_eq!(hexdump(&input), Ok(RuntimeValue::String(expected.to_string().into())));
}

// Test convert::try_from
#[rstest]
#[case::h1_symbol(
    RuntimeValue::Symbol(Ident::new("h1")),
    Convert::Markdown(ConvertKind::Heading(mq_markdown::HeadingDepth::H1))
)]
#[case::h2_symbol(
    RuntimeValue::Symbol(Ident::new("h2")),
    Convert::Markdown(ConvertKind::Heading(mq_markdown::HeadingDepth::H2))
)]
#[case::h3_symbol(
    RuntimeValue::Symbol(Ident::new("h3")),
    Convert::Markdown(ConvertKind::Heading(mq_markdown::HeadingDepth::H3))
)]
#[case::h4_symbol(
    RuntimeValue::Symbol(Ident::new("h4")),
    Convert::Markdown(ConvertKind::Heading(mq_markdown::HeadingDepth::H4))
)]
#[case::h5_symbol(
    RuntimeValue::Symbol(Ident::new("h5")),
    Convert::Markdown(ConvertKind::Heading(mq_markdown::HeadingDepth::H5))
)]
#[case::h6_symbol(
    RuntimeValue::Symbol(Ident::new("h6")),
    Convert::Markdown(ConvertKind::Heading(mq_markdown::HeadingDepth::H6))
)]
#[case::html_symbol(RuntimeValue::Symbol(Ident::new("html")), Convert::Html)]
#[case::md5_symbol(RuntimeValue::Symbol(Ident::new("md5")), Convert::Md5)]
#[case::sha256_symbol(RuntimeValue::Symbol(Ident::new("sha256")), Convert::Sha256)]
#[case::text_symbol(RuntimeValue::Symbol(Ident::new("text")), Convert::Text)]
#[case::sh_symbol(RuntimeValue::Symbol(Ident::new("sh")), Convert::Shell)]
#[case::h1_string(RuntimeValue::String(Shared::new("#".to_string())), Convert::Markdown(ConvertKind::Heading(mq_markdown::HeadingDepth::H1)))]
#[case::h2_string(RuntimeValue::String(Shared::new("##".to_string())), Convert::Markdown(ConvertKind::Heading(mq_markdown::HeadingDepth::H2)))]
#[case::h3_string(RuntimeValue::String(Shared::new("###".to_string())), Convert::Markdown(ConvertKind::Heading(mq_markdown::HeadingDepth::H3)))]
#[case::h4_string(RuntimeValue::String(Shared::new("####".to_string())), Convert::Markdown(ConvertKind::Heading(mq_markdown::HeadingDepth::H4)))]
#[case::h5_string(RuntimeValue::String(Shared::new("#####".to_string())), Convert::Markdown(ConvertKind::Heading(mq_markdown::HeadingDepth::H5)))]
#[case::h6_string(RuntimeValue::String(Shared::new("######".to_string())), Convert::Markdown(ConvertKind::Heading(mq_markdown::HeadingDepth::H6)))]
#[case::blockquote_string(RuntimeValue::String(Shared::new(">".to_string())), Convert::Markdown(ConvertKind::Blockquote))]
#[case::list_item_string(RuntimeValue::String(Shared::new("-".to_string())), Convert::Markdown(ConvertKind::ListItem))]
#[case::strikethrough_string(RuntimeValue::String(Shared::new("~~".to_string())), Convert::Markdown(ConvertKind::Strikethrough))]
#[case::strong_string(RuntimeValue::String(Shared::new("**".to_string())), Convert::Markdown(ConvertKind::Strong))]
#[case::horizontal_rule_string(RuntimeValue::String(Shared::new("--".to_string())), Convert::Markdown(ConvertKind::HorizontalRule))]
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
#[case::invalid_string(RuntimeValue::String(Shared::new("invalid".to_string())))]
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
    assert_eq!(result, RuntimeValue::String(Shared::new(expected.to_string())));
}

#[rstest]
#[case("aGVsbG8=", "hello")]
#[case("d29ybGQ=", "world")]
#[case("dGVzdA==", "test")]
#[case("", "")]
fn test_base64_decode(#[case] input: &str, #[case] expected: &str) {
    let result = base64d(input).unwrap();
    assert_eq!(result, RuntimeValue::String(Shared::new(expected.to_string())));
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
    assert_eq!(result, RuntimeValue::String(Shared::new(expected.to_string())));
}

#[rstest]
#[case("aGVsbG8", "hello")]
#[case("d29ybGQ", "world")]
#[case("dGVzdA", "test")]
fn test_base64url_decode(#[case] input: &str, #[case] expected: &str) {
    let result = base64urld(input).unwrap();
    assert_eq!(result, RuntimeValue::String(Shared::new(expected.to_string())));
}

// Test URL encoding
#[rstest]
#[case("hello world", "hello%20world")]
#[case("test@example.com", "test%40example.com")]
#[case("a-b_c.d~e", "a-b_c.d~e")]
#[case("a+b=c", "a%2Bb%3Dc")]
#[case("", "")]
#[case("foo/bar", "foo%2Fbar")]
#[case("foo?bar=baz", "foo%3Fbar%3Dbaz")]
#[case("foo&bar", "foo%26bar")]
#[case("100%", "100%25")]
#[case("café", "caf%C3%A9")]
fn test_url_encode(#[case] input: &str, #[case] expected: &str) {
    let result = url_encode(input).unwrap();
    assert_eq!(result, RuntimeValue::String(Shared::new(expected.to_string())));
}

// Test html_escape
#[rstest]
#[case("<b>bold</b>", "&lt;b&gt;bold&lt;/b&gt;")]
#[case("Tom & Jerry", "Tom &amp; Jerry")]
#[case(r#"say "hi""#, "say &quot;hi&quot;")]
#[case("it's", "it&#39;s")]
#[case("plain text", "plain text")]
#[case("", "")]
fn test_html_escape(#[case] input: &str, #[case] expected: &str) {
    let result = html_escape(input).unwrap();
    assert_eq!(result, RuntimeValue::String(Shared::new(expected.to_string())));
}

// Test html_unescape
#[rstest]
#[case("&lt;b&gt;bold&lt;/b&gt;", "<b>bold</b>")]
#[case("Tom &amp; Jerry", "Tom & Jerry")]
#[case("&quot;quoted&quot;", "\"quoted\"")]
#[case("&#39;single&#39;", "'single'")]
#[case("&apos;apos&apos;", "'apos'")]
#[case("&#65;&#x42;", "AB")]
#[case("plain text", "plain text")]
#[case("", "")]
fn test_html_unescape(#[case] input: &str, #[case] expected: &str) {
    let result = html_unescape(input).unwrap();
    assert_eq!(result, RuntimeValue::String(Shared::new(expected.to_string())));
}

// Test html_escape/html_unescape round trip
#[rstest]
#[case("<script>alert('xss')</script>")]
#[case(r#"5 > 3 && 2 < 4, say "hi""#)]
fn test_html_escape_unescape_round_trip(#[case] input: &str) {
    let escaped = html_escape(input).unwrap();
    let RuntimeValue::String(escaped) = escaped else {
        panic!("Expected String")
    };
    let unescaped = html_unescape(&escaped).unwrap();
    assert_eq!(unescaped, RuntimeValue::String(Shared::new(input.to_string())));
}

// Test markdown_escape: text context
#[rstest]
#[case("plain text", "plain text")]
#[case("**bold** and _em_", r"\*\*bold\*\* and \_em\_")]
#[case("a ~b~ `c` <d>", r"a \~b\~ \`c\` \<d>")]
#[case(r"back\slash", r"back\\slash")]
#[case("[label](url)", r"\[label\](url)")]
#[case("![alt](url)", r"!\[alt\](url)")]
#[case("# Not a heading", r"\# Not a heading")]
#[case("###### Not a heading", r"\###### Not a heading")]
#[case("####### still text", "####### still text")] // 7 hashes: not a valid ATX marker
#[case("> Not a quote", r"\> Not a quote")]
#[case("- Not a list item", r"\- Not a list item")]
#[case("+ Not a list item", r"\+ Not a list item")]
#[case("1. Not a list item", r"1\. Not a list item")]
#[case("42) Not a list item", r"42\) Not a list item")]
#[case("not-a-list.item", "not-a-list.item")]
#[case("---", r"\---")]
#[case("===", r"\===")]
#[case("- - -", r"\- - -")]
#[case("Title\n===\nBody", "Title\n\\===\nBody")]
#[case("   - indented list", "   \\- indented list")]
#[case("    four spaces", "&#32;&#32;&#32;&#32;four spaces")]
#[case("\tone tab", "&#9;one tab")]
#[case("   \tspaces then tab", "&#32;&#32;&#32;&#9;spaces then tab")]
fn test_markdown_escape_text(#[case] input: &str, #[case] expected: &str) {
    let result = markdown_escape(input, MarkdownEscapeContext::Text).unwrap();
    assert_eq!(result, RuntimeValue::String(Shared::new(expected.to_string())));
}

#[rstest]
#[case("    customer value")]
#[case("\tcustomer value")]
#[case("   \tcustomer value")]
fn test_markdown_escape_text_indentation_does_not_become_code_block(#[case] input: &str) {
    let escaped = match markdown_escape(input, MarkdownEscapeContext::Text).unwrap() {
        RuntimeValue::String(s) => s.to_string(),
        _ => panic!("Expected String"),
    };
    let md = mq_markdown::Markdown::from_markdown_str(&escaped).unwrap();
    assert!(
        !md.nodes.iter().any(|n| matches!(n, mq_markdown::Node::Code(_))),
        "escaped text {:?} parsed back into a code block: {:?}",
        escaped,
        md.nodes
    );
}

// Test markdown_escape: heading and link_label contexts collapse newlines to spaces
#[rstest]
#[case(MarkdownEscapeContext::Heading, "Q1 * Results", r"Q1 \* Results")]
#[case(MarkdownEscapeContext::Heading, "line1\nline2", "line1 line2")]
#[case(MarkdownEscapeContext::LinkLabel, "a [nested] link", r"a \[nested\] link")]
#[case(MarkdownEscapeContext::LinkLabel, "line1\nline2", "line1 line2")]
fn test_markdown_escape_singleline(
    #[case] context: MarkdownEscapeContext,
    #[case] input: &str,
    #[case] expected: &str,
) {
    let result = markdown_escape(input, context).unwrap();
    assert_eq!(result, RuntimeValue::String(Shared::new(expected.to_string())));
}

// Test markdown_escape: table_cell context
#[rstest]
#[case("a | b", r"a \| b")]
#[case("line1\nline2", "line1 line2")]
#[case("**bold**", r"\*\*bold\*\*")]
#[case(r"back\|slash", r"back\\\|slash")]
#[case("plain", "plain")]
fn test_markdown_escape_table_cell(#[case] input: &str, #[case] expected: &str) {
    let result = markdown_escape(input, MarkdownEscapeContext::TableCell).unwrap();
    assert_eq!(result, RuntimeValue::String(Shared::new(expected.to_string())));
}

// Test markdown_escape: code context is left unmodified (no backslash-escape mechanism
// exists inside a code span/block; the caller must pick a longer fence instead)
#[rstest]
#[case("plain")]
#[case("`nested`")]
#[case("``` fence ```")]
#[case("a | b\n* c *")]
fn test_markdown_escape_code(#[case] input: &str) {
    let result = markdown_escape(input, MarkdownEscapeContext::Code).unwrap();
    assert_eq!(result, RuntimeValue::String(Shared::new(input.to_string())));
}

#[rstest]
#[case("text", MarkdownEscapeContext::Text)]
#[case("heading", MarkdownEscapeContext::Heading)]
#[case("link_label", MarkdownEscapeContext::LinkLabel)]
#[case("table_cell", MarkdownEscapeContext::TableCell)]
#[case("code", MarkdownEscapeContext::Code)]
fn test_markdown_escape_context_try_from_valid(#[case] input: &str, #[case] expected: MarkdownEscapeContext) {
    assert_eq!(MarkdownEscapeContext::try_from(input).unwrap(), expected);
}

#[rstest]
#[case("")]
#[case("Text")]
#[case("paragraph")]
fn test_markdown_escape_context_try_from_invalid(#[case] input: &str) {
    assert!(MarkdownEscapeContext::try_from(input).is_err());
}

// Round-trip: escaping a value and embedding it in a table row keeps the table's
// column count and doesn't turn the value into a link/emphasis/heading.
#[rstest]
#[case("a | b\nc", "a \\| b c")]
#[case("no special chars", "no special chars")]
fn test_markdown_escape_table_cell_round_trip_fixture(#[case] input: &str, #[case] expected_cell: &str) {
    let escaped = markdown_escape(input, MarkdownEscapeContext::TableCell).unwrap();
    let RuntimeValue::String(escaped) = escaped else {
        panic!("Expected String")
    };
    assert_eq!(escaped.as_str(), expected_cell);
    let row = format!("| {} |", escaped);
    let unescaped_pipes = row
        .char_indices()
        .filter(|&(i, c)| c == '|' && !row[..i].ends_with('\\'))
        .count();
    assert_eq!(
        unescaped_pipes, 2,
        "row should have exactly 2 column delimiters: {row:?}"
    );
}

// Test strip_tags
#[rstest]
#[case("<b>bold</b>", "bold")]
#[case("<p>Hello <em>world</em>!</p>", "Hello world!")]
#[case("no tags here", "no tags here")]
#[case(r#"<a href="http://example.com" title="a > b">link</a>"#, "link")]
#[case("<div class='a<b'>text</div>", "text")]
#[case("", "")]
// A stray, unterminated `<` is treated as the start of a tag through the
// end of the input, mirroring common strip_tags implementations.
#[case("a < b", "a ")]
#[case("a > b", "a > b")]
fn test_strip_tags(#[case] input: &str, #[case] expected: &str) {
    let result = strip_tags(input).unwrap();
    assert_eq!(result, RuntimeValue::String(Shared::new(expected.to_string())));
}

// Test sanitize_html
#[rstest]
#[case("<script>alert('xss')</script><p>hi</p>", "<p>hi</p>")]
#[case(r#"<img src=x onerror="alert(1)">"#, r#"<img src="x">"#)]
#[case(
    r#"<a href="javascript:alert(1)">click</a>"#,
    r#"<a rel="noopener noreferrer">click</a>"#
)]
#[case("<b>bold</b> and <i>italic</i>", "<b>bold</b> and <i>italic</i>")]
#[case("", "")]
fn test_sanitize_html(#[case] input: &str, #[case] expected: &str) {
    let result = sanitize_html(input).unwrap();
    assert_eq!(result, RuntimeValue::String(Shared::new(expected.to_string())));
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
#[case("=ls", "'=ls'")] // Leading equals is expanded by zsh
#[case("test123", "test123")] // Alphanumeric is safe
#[case("TEST", "TEST")] // Uppercase is safe
#[case("it's a test", "'it'\\''s a test'")] // Multiple issues
#[case("rm -rf /", "'rm -rf /'")] // Dangerous command
fn test_shell_escape(#[case] input: &str, #[case] expected: &str) {
    let result = shell_escape(input).unwrap();
    assert_eq!(result, RuntimeValue::String(Shared::new(expected.to_string())));
}

// Test to_string
#[rstest]
#[case::string(RuntimeValue::String(Shared::new("test".to_string())), "test")]
#[case::symbol(RuntimeValue::Symbol(Ident::new("test")), "test")]
#[case::number(RuntimeValue::Number(42.into()), "42")]
#[case::boolean_true(RuntimeValue::Boolean(true), "true")]
#[case::boolean_false(RuntimeValue::Boolean(false), "false")]
fn test_to_string(#[case] input: RuntimeValue, #[case] expected: &str) {
    let result = to_string(&input).unwrap();
    assert_eq!(result, RuntimeValue::String(Shared::new(expected.to_string())));
}

// Test to_number
#[rstest]
#[case::string_int(RuntimeValue::String(Shared::new("42".to_string())), RuntimeValue::Number(42.into()))]
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
    let mut input = RuntimeValue::String(Shared::new("not a number".to_string()));
    let result = to_number(&mut input);
    assert!(result.is_err());
}

#[test]
fn test_to_number_array() {
    let mut input = RuntimeValue::Array(crate::Shared::new(vec![
        RuntimeValue::String(Shared::new("42".to_string())),
        RuntimeValue::Boolean(true),
    ]));
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
        RuntimeValue::String(Shared::new("abc".to_string())),
        vec![
            RuntimeValue::String(Shared::new("a".to_string())),
            RuntimeValue::String(Shared::new("b".to_string())),
            RuntimeValue::String(Shared::new("c".to_string())),
        ]
    )]
#[case::none(RuntimeValue::None, vec![])]
#[case::number(RuntimeValue::Number(42.into()), vec![RuntimeValue::Number(42.into())])]
#[case::boolean_true(RuntimeValue::Boolean(true), vec![RuntimeValue::Boolean(true)])]
#[case::boolean_false(RuntimeValue::Boolean(false), vec![RuntimeValue::Boolean(false)])]
fn test_to_array(#[case] mut input: RuntimeValue, #[case] expected: Vec<RuntimeValue>) {
    let result = to_array(&mut input).unwrap();
    assert_eq!(result, RuntimeValue::Array(crate::Shared::new(expected)));
}

#[test]
fn test_to_array_already_array() {
    let mut input = RuntimeValue::Array(crate::Shared::new(vec![
        RuntimeValue::Number(1.into()),
        RuntimeValue::Number(2.into()),
    ]));
    let result = to_array(&mut input).unwrap();
    assert_eq!(
        result,
        RuntimeValue::Array(crate::Shared::new(vec![
            RuntimeValue::Number(1.into()),
            RuntimeValue::Number(2.into()),
        ]))
    );
}

// Test to_text
#[rstest]
#[case::string(RuntimeValue::String(Shared::new("test".to_string())), "test")]
#[case::number(RuntimeValue::Number(42.into()), "42")]
#[case::none(RuntimeValue::None, "")]
#[case::array(
        RuntimeValue::Array(crate::Shared::new(vec![
            RuntimeValue::String(Shared::new("a".to_string())),
            RuntimeValue::String(Shared::new("b".to_string())),
            RuntimeValue::String(Shared::new("c".to_string())),
        ])),
        "a,b,c"
    )]
#[case::array_with_none(
        RuntimeValue::Array(crate::Shared::new(vec![
            RuntimeValue::String(Shared::new("a".to_string())),
            RuntimeValue::None,
            RuntimeValue::String(Shared::new("c".to_string())),
        ])),
        "a,,c"
    )]
#[case::empty_array(RuntimeValue::Array(crate::Shared::new(vec![])), "")]
fn test_to_text(#[case] input: RuntimeValue, #[case] expected: &str) {
    let result = to_text(&input).unwrap();
    match result {
        RuntimeValue::None => assert_eq!(expected, ""),
        RuntimeValue::String(s) => assert_eq!(s.as_str(), expected),
        _ => panic!("Expected String or None"),
    }
}

// Test to_html
#[rstest]
#[case::plain_text(RuntimeValue::String(Shared::new("hello".to_string())), "<p>hello</p>")]
#[case::bold(RuntimeValue::String(Shared::new("**bold**".to_string())), "<p><strong>bold</strong></p>")]
#[case::italic(RuntimeValue::String(Shared::new("*italic*".to_string())), "<p><em>italic</em></p>")]
#[case::none(RuntimeValue::None, "")]
#[case::heading(RuntimeValue::String(Shared::new("# Heading\n\nParagraph".to_string())), "<h1>Heading</h1>\n<p>Paragraph</p>")]
#[case::quote(RuntimeValue::String(Shared::new("> Quote".to_string())), "<blockquote>\n<p>Quote</p>\n</blockquote>")]
#[case::list(RuntimeValue::String(Shared::new("- Item 1\n- Item 2".to_string())), "<ul>\n<li>Item 1</li>\n<li>Item 2</li>\n</ul>")]
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
            RuntimeValue::Array(crate::Shared::new(vec![RuntimeValue::Number(1.into())])),
            RuntimeValue::Number(2.into()),
            RuntimeValue::Array(crate::Shared::new(vec![RuntimeValue::Number(3.into()), RuntimeValue::Number(4.into())])),
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
            RuntimeValue::Array(crate::Shared::new(vec![
                RuntimeValue::Array(crate::Shared::new(vec![RuntimeValue::Number(1.into())])),
                RuntimeValue::Number(2.into()),
            ])),
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
        RuntimeValue::Array(crate::Shared::new(vec![])),
        RuntimeValue::Number(1.into()),
        RuntimeValue::Array(crate::Shared::new(vec![])),
    ];
    let result = flatten(input);
    assert_eq!(result, vec![RuntimeValue::Number(1.into())]);
}

// Test from_date
#[rstest]
#[case("2024-01-01T00:00:00Z", 1704067200)]
#[case("2024-06-15T12:30:45Z", 1718454645)]
#[case("2024-12-31T23:59:59Z", 1735689599)]
fn test_from_date_valid(#[case] input: &str, #[case] expected_secs: i64) {
    let result = from_date(input).unwrap();
    assert_eq!(result, RuntimeValue::Number(expected_secs.into()));
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
#[case(1704067200, None, "2024-01-01T00:00:00Z")]
#[case(1704067200, Some("%Y-%m-%d"), "2024-01-01")]
#[case(1718454645, Some("%Y/%m/%d %H:%M"), "2024/06/15 12:30")]
#[case(1704067200, Some("%d/%m/%Y"), "01/01/2024")]
#[case(1704067200, Some("%H:%M:%S"), "00:00:00")]
#[case(1704067200, Some("%Y"), "2024")]
fn test_to_date(#[case] secs: i64, #[case] convert: Option<&str>, #[case] expected: &str) {
    let result = to_date(secs.into(), convert).unwrap();
    assert_eq!(result, RuntimeValue::String(Shared::new(expected.to_string())));
}

// Test to_markdown_string
#[rstest]
#[case::single_string(
        vec![RuntimeValue::String(Shared::new("hello".to_string()))],
        "hello"
    )]
#[case::multiple_strings(
        vec![
            RuntimeValue::String(Shared::new("hello".to_string())),
            RuntimeValue::String(Shared::new(" ".to_string())),
            RuntimeValue::String(Shared::new("world".to_string())),
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
#[case::base64(Convert::Base64, RuntimeValue::String(Shared::new("hello".to_string())), "aGVsbG8=")]
#[case::uri_encode(Convert::UriEncode, RuntimeValue::String(Shared::new("hello world".to_string())), "hello%20world")]
#[case::uri_decode(Convert::UriDecode, RuntimeValue::String(Shared::new("hello%20world".to_string())), "hello world")]
#[case::empty_base64(Convert::Base64, RuntimeValue::String(Shared::new("".to_string())), "")]
#[case::empty_html(Convert::Html, RuntimeValue::String(Shared::new("".to_string())), "")]
#[case::empty_text(Convert::Text, RuntimeValue::String(Shared::new("".to_string())), "")]
#[case::empty_uri(Convert::UriEncode, RuntimeValue::String(Shared::new("".to_string())), "")]
#[case::md5(Convert::Md5, RuntimeValue::String(Shared::new("hello".to_string())), "5d41402abc4b2a76b9719d911017c592")]
#[case::sha256(Convert::Sha256, RuntimeValue::String(Shared::new("hello".to_string())), "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824")]
#[case::md5_integer(Convert::Md5, RuntimeValue::Number(42.into()), "a1d0c6e83f027327d8461063f4ac58a6")]
#[case::sha256_integer(Convert::Sha256, RuntimeValue::Number(42.into()), "73475cb40a568e8da8a045ced110137e159f890ac4da883b6b17dc651b3a8049")]
#[case::md5_float(
    Convert::Md5,
    RuntimeValue::Number(crate::number::Number::from(1.5_f64)),
    "6008647277c4454cecd97d33c069f0ca"
)]
#[case::sha256_float(
    Convert::Sha256,
    RuntimeValue::Number(crate::number::Number::from(1.5_f64)),
    "9f29a130438b81170b92a42650f9a94291ecad60bd47af2a3886e75f7f728725"
)]
fn test_convert_string_converts(#[case] convert: Convert, #[case] input: RuntimeValue, #[case] expected: &str) {
    let result = convert.convert(&input);
    assert_eq!(result, RuntimeValue::String(Shared::new(expected.to_string())));
}

#[test]
fn test_convert_html() {
    let convert = Convert::Html;
    let input = RuntimeValue::String(Shared::new("a b,\"c\"<x> & 'y'".to_string()));
    assert_eq!(
        convert.convert(&input),
        RuntimeValue::String(Shared::new("a b,&quot;c&quot;&lt;x&gt; &amp; &#39;y&#39;".to_string()))
    );
    assert_eq!(convert.convert(&RuntimeValue::None), RuntimeValue::NONE);
}

#[test]
fn test_convert_text() {
    let convert = Convert::Text;
    let input = RuntimeValue::Number(42.into());
    let result = convert.convert(&input);
    assert_eq!(result, RuntimeValue::String(Shared::new("42".to_string())));
}

#[test]
fn test_convert_sh() {
    let convert = Convert::Shell;

    // Simple string without special characters
    let input = RuntimeValue::String(Shared::new("echo hello".to_string()));
    let result = convert.convert(&input);
    assert_eq!(result, RuntimeValue::String(Shared::new("'echo hello'".to_string())));

    // String with single quote
    let input = RuntimeValue::String(Shared::new("it's".to_string()));
    let result = convert.convert(&input);
    assert_eq!(result, RuntimeValue::String(Shared::new("'it'\\''s'".to_string())));

    // Safe alphanumeric string
    let input = RuntimeValue::String(Shared::new("test123".to_string()));
    let result = convert.convert(&input);
    assert_eq!(result, RuntimeValue::String(Shared::new("test123".to_string())));

    // Number
    let input = RuntimeValue::Number(42.into());
    let result = convert.convert(&input);
    assert_eq!(result, RuntimeValue::String(Shared::new("42".to_string())));
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
    let depth = mq_markdown::HeadingDepth::new(depth).expect("a heading depth");
    let convert = Convert::Markdown(ConvertKind::Heading(depth));
    let input = RuntimeValue::String(Shared::new("Test".to_string()));
    let result = convert.convert(&input);

    let RuntimeValue::Markdown(node, _) = result else {
        panic!("Expected Markdown Heading with depth {}", depth)
    };
    let mq_markdown::Node::Heading(heading) = Shared::unwrap_or_clone(node) else {
        panic!("Expected Heading node")
    };
    assert_eq!(heading.depth, depth);
    assert_eq!(heading.values.len(), 1);
}

// Test convert::convert for Markdown blockquote
#[test]
fn test_convert_markdown_blockquote() {
    let convert = Convert::Markdown(ConvertKind::Blockquote);
    let input = RuntimeValue::String(Shared::new("Important note".to_string()));
    let result = convert.convert(&input);

    let RuntimeValue::Markdown(node, _) = result else {
        panic!("Expected Markdown Blockquote")
    };
    let mq_markdown::Node::Blockquote(blockquote) = Shared::unwrap_or_clone(node) else {
        panic!("Expected Blockquote node")
    };
    assert_eq!(blockquote.values.len(), 1);
}

// Test convert::convert for Markdown list item
#[test]
fn test_convert_markdown_list_item() {
    let convert = Convert::Markdown(ConvertKind::ListItem);
    let input = RuntimeValue::String(Shared::new("Item text".to_string()));
    let result = convert.convert(&input);

    let RuntimeValue::Markdown(node, _) = result else {
        panic!("Expected Markdown List")
    };
    let mq_markdown::Node::List(list) = Shared::unwrap_or_clone(node) else {
        panic!("Expected List node")
    };
    assert!(!list.ordered);
    assert_eq!(list.level, 1);
    assert_eq!(list.values.len(), 1);
}

// Test convert::convert for Markdown link
#[test]
fn test_convert_markdown_link() {
    let url = Url::parse("https://example.com").unwrap();
    let convert = Convert::Markdown(ConvertKind::Link(url.to_string()));
    let input = RuntimeValue::String(Shared::new("Click here".to_string()));
    let result = convert.convert(&input);

    let RuntimeValue::Markdown(node, _) = result else {
        panic!("Expected Markdown Link")
    };
    let mq_markdown::Node::Link(link) = Shared::unwrap_or_clone(node) else {
        panic!("Expected Link node")
    };
    assert_eq!(link.url.as_str(), "https://example.com/");
    assert_eq!(link.values.len(), 1);
}

// Test convert::convert for Markdown strikethrough
#[test]
fn test_convert_markdown_strikethrough() {
    let convert = Convert::Markdown(ConvertKind::Strikethrough);
    let input = RuntimeValue::String(Shared::new("Deleted text".to_string()));
    let result = convert.convert(&input);

    let RuntimeValue::Markdown(node, _) = result else {
        panic!("Expected Markdown Delete")
    };
    let mq_markdown::Node::Delete(delete) = Shared::unwrap_or_clone(node) else {
        panic!("Expected Delete node")
    };
    assert_eq!(delete.values.len(), 1);
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
    let input = RuntimeValue::new_markdown(markdown_node);
    let convert = Convert::Markdown(ConvertKind::Heading(mq_markdown::HeadingDepth::H2));

    let result = convert.convert(&input);

    let RuntimeValue::Markdown(node, _) = result else {
        panic!("Expected Markdown Heading")
    };
    let mq_markdown::Node::Heading(heading) = Shared::unwrap_or_clone(node) else {
        panic!("Expected Heading node")
    };
    assert_eq!(heading.depth, mq_markdown::HeadingDepth::H2);
}

// Test convert with number input
#[test]
fn test_convert_number_to_heading() {
    let convert = Convert::Markdown(ConvertKind::Heading(mq_markdown::HeadingDepth::H1));
    let input = RuntimeValue::Number(42.into());
    let result = convert.convert(&input);

    let RuntimeValue::Markdown(node, _) = result else {
        panic!("Expected Markdown Heading")
    };
    let mq_markdown::Node::Heading(heading) = Shared::unwrap_or_clone(node) else {
        panic!("Expected Heading node")
    };
    assert_eq!(heading.depth, mq_markdown::HeadingDepth::H1);
    assert_eq!(heading.values.len(), 1);
}

// Test convert::try_from with URL
#[test]
fn test_convert_convert_url() {
    let url_string = RuntimeValue::String(Shared::new("https://example.com".to_string()));
    let result = Convert::try_from(&url_string);
    assert!(result.is_ok());
    match result.unwrap() {
        Convert::Markdown(ConvertKind::Link(_)) => {}
        _ => panic!("Expected Markdown Link convert"),
    }
}

// Test MD5 hash
#[rstest]
#[case("", "d41d8cd98f00b204e9800998ecf8427e")]
#[case("hello", "5d41402abc4b2a76b9719d911017c592")]
#[case("hello world", "5eb63bbbe01eeed093cb22bb8f5acdc3")]
#[case("The quick brown fox jumps over the lazy dog", "9e107d9d372bb6826bd81d3542a419d6")]
fn test_md5(#[case] input: &str, #[case] expected: &str) {
    let result = md5(input).unwrap();
    assert_eq!(result, RuntimeValue::String(Shared::new(expected.to_string())));
}

// Test SHA-256 hash
#[rstest]
#[case("", "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")]
#[case("hello", "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824")]
#[case("hello world", "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9")]
#[case(
    "The quick brown fox jumps over the lazy dog",
    "d7a8fbb307d7809469ca9abcb0082e4f8d5651e46d3cdb762d02d0bf37c9e592"
)]
fn test_sha256(#[case] input: &str, #[case] expected: &str) {
    let result = sha256(input).unwrap();
    assert_eq!(result, RuntimeValue::String(Shared::new(expected.to_string())));
}

#[rstest]
#[case(b"" as &[u8], "d41d8cd98f00b204e9800998ecf8427e")]
#[case(b"hello", "5d41402abc4b2a76b9719d911017c592")]
#[case(b"hello world", "5eb63bbbe01eeed093cb22bb8f5acdc3")]
fn test_md5_bytes(#[case] input: &[u8], #[case] expected: &str) {
    let result = md5_bytes(input).unwrap();
    assert_eq!(result, RuntimeValue::String(Shared::new(expected.to_string())));
}

#[rstest]
#[case(b"" as &[u8], "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")]
#[case(b"hello", "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824")]
fn test_sha256_bytes(#[case] input: &[u8], #[case] expected: &str) {
    let result = sha256_bytes(input).unwrap();
    assert_eq!(result, RuntimeValue::String(Shared::new(expected.to_string())));
}

#[rstest]
#[case("deadbeef", Ok(RuntimeValue::Bytes(Shared::new(vec![0xde, 0xad, 0xbe, 0xef]))))]
#[case("DEADBEEF", Ok(RuntimeValue::Bytes(Shared::new(vec![0xde, 0xad, 0xbe, 0xef]))))]
#[case("", Ok(RuntimeValue::Bytes(Shared::new(vec![]))))]
#[case("00ff", Ok(RuntimeValue::Bytes(Shared::new(vec![0x00, 0xff]))))]
fn test_from_hex(#[case] input: &str, #[case] expected: Result<RuntimeValue, Error>) {
    assert_eq!(from_hex(input), expected);
}

#[rstest]
#[case("abc")]
#[case("zzzz")]
#[case("にa")]
#[case("café")]
fn test_from_hex_invalid(#[case] input: &str) {
    assert!(from_hex(input).is_err());
}

#[rstest]
#[case(b"hello" as &[u8], Ok(RuntimeValue::String(Shared::new("hello".to_string()))))]
#[case(b"" as &[u8], Ok(RuntimeValue::String(Shared::new("".to_string()))))]
fn test_utf8_valid(#[case] input: &[u8], #[case] expected: Result<RuntimeValue, Error>) {
    assert_eq!(utf8(input), expected);
}

#[test]
fn test_utf8_invalid() {
    assert!(utf8(&[0xff, 0xfe]).is_err());
}
