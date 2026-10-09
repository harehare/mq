//! The nodes mq reads from MDX and Markdown, flattened to tokens. The expected tokens are those of the
//! mdast that the official packages (`mdast-util-from-markdown` with `mdast-util-mdx`, `mdast-util-gfm`,
//! `mdast-util-math` and `mdast-util-frontmatter`) make of the same input, except where a comment says
//! how mq differs. The inputs come from the tests of markdown-rs and micromark and from hand-written cases.

use mq_markdown::{Markdown, MdxAttributeContent, MdxAttributeValue, Node, TableAlignKind};
use rstest::rstest;

enum Token {
    Text(String),
    Other(String),
}

/// Quotes `value` as `JSON.stringify` does.
fn quote(value: &str) -> String {
    let mut out = String::from('"');
    for char in value.chars() {
        match char {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            char if char < ' ' => out.push_str(&format!("\\u{:04x}", char as u32)),
            char => out.push(char),
        }
    }
    out.push('"');
    out
}

fn other(token: impl Into<String>) -> Token {
    Token::Other(token.into())
}

fn container(head: String, inner: Vec<Token>) -> Vec<Token> {
    let mut tokens = vec![other(format!("{head}("))];
    tokens.extend(inner);
    tokens.push(other(")"));
    tokens
}

/// The tokens of `nodes`, with adjacent text joined when `inline`.
fn children(nodes: &[Node], inline: bool) -> Vec<Token> {
    let mut tokens: Vec<Token> = Vec::new();
    for token in nodes.iter().flat_map(node_tokens) {
        match (tokens.last_mut(), token) {
            (Some(Token::Text(last)), Token::Text(text)) if inline => last.push_str(&text),
            (_, token) => tokens.push(token),
        }
    }
    tokens
}

fn attributes(attributes: &[MdxAttributeContent]) -> String {
    attributes
        .iter()
        .map(|attribute| match attribute {
            MdxAttributeContent::Expression(value) => format!(" ...{{{value}}}"),
            MdxAttributeContent::Property(property) => match &property.value {
                None => format!(" {}", property.name),
                Some(MdxAttributeValue::Literal(value)) => format!(" {}=lit:{}", property.name, quote(value)),
                Some(MdxAttributeValue::Expression(value)) => format!(" {}=expr:{{{value}}}", property.name),
            },
        })
        .collect()
}

fn or_empty<T: ToString>(value: &Option<T>) -> String {
    value.as_ref().map(ToString::to_string).unwrap_or_default()
}

/// Paragraphs have no token, since mq has none, and list items are flat, as in mq.
fn node_tokens(node: &Node) -> Vec<Token> {
    match node {
        Node::Heading(heading) => container(format!("heading {}", heading.depth), children(&heading.values, true)),
        Node::HorizontalRule(_) => vec![other("hr")],
        Node::Code(code) => vec![other(format!(
            "code {} {} {}",
            or_empty(&code.lang),
            or_empty(&code.meta),
            quote(&code.value)
        ))],
        Node::Html(html) => vec![other(format!("html {}", quote(&html.value)))],
        Node::Blockquote(quote) => container("blockquote".into(), children(&quote.values, false)),
        Node::List(list) => {
            let number = if list.ordered {
                format!("o{}", list.start.unwrap_or(1) as usize + list.index)
            } else {
                "u".into()
            };
            let checked = match list.checked {
                Some(true) => "x",
                Some(false) => "_",
                None => "-",
            };
            let spread = if list.spread { "s" } else { "t" };
            container(
                format!("item {} {number} {checked} {spread}", list.level),
                children(&list.values, false),
            )
        }
        Node::TableCell(cell) => container(
            format!("cell {} {}", cell.row, cell.column),
            children(&cell.values, true),
        ),
        Node::TableAlign(align) => {
            let align = align
                .align
                .iter()
                .map(|kind| match kind {
                    TableAlignKind::Left => "\"left\"",
                    TableAlignKind::Right => "\"right\"",
                    TableAlignKind::Center => "\"center\"",
                    TableAlignKind::None => "\"none\"",
                })
                .collect::<Vec<_>>();
            vec![other(format!("align [{}]", align.join(",")))]
        }
        Node::Definition(definition) => vec![other(format!(
            "definition {} {} {}",
            definition.ident,
            definition.url.as_str(),
            or_empty(&definition.title)
        ))],
        Node::Footnote(footnote) => container(
            format!("footnote {}", footnote.ident),
            children(&footnote.values, false),
        ),
        Node::FootnoteRef(reference) => vec![other(format!("footnoteref {}", reference.ident))],
        Node::Math(math) => vec![other(format!("math {}", quote(&math.value)))],
        Node::MathInline(math) => vec![other(format!("imath {}", quote(&math.value)))],
        Node::Yaml(yaml) => vec![other(format!("yaml {}", quote(&yaml.value)))],
        Node::Toml(toml) => vec![other(format!("toml {}", quote(&toml.value)))],
        Node::Text(text) => vec![Token::Text(text.value.clone())],
        Node::Emphasis(emphasis) => container("em".into(), children(&emphasis.values, true)),
        Node::Strong(strong) => container("strong".into(), children(&strong.values, true)),
        Node::Delete(delete) => container("del".into(), children(&delete.values, true)),
        Node::CodeInline(code) => vec![other(format!("code_inline {}", quote(&code.value)))],
        Node::Break(_) => vec![other("br")],
        Node::Link(link) => container(
            format!("link {} {}", link.url.as_str(), or_empty(&link.title)),
            children(&link.values, true),
        ),
        Node::Image(image) => vec![other(format!(
            "image {} {} {}",
            image.url,
            or_empty(&image.title),
            quote(&image.alt)
        ))],
        Node::LinkRef(reference) => container(
            format!("linkref {}", reference.ident),
            children(&reference.values, true),
        ),
        Node::ImageRef(reference) => vec![other(format!("imageref {} {}", reference.ident, quote(&reference.alt)))],
        Node::MdxJsxFlowElement(element) => container(
            format!("flow <{}{}>", or_empty(&element.name), attributes(&element.attributes)),
            children(&element.children, false),
        ),
        Node::MdxJsxTextElement(element) => container(
            format!("text <{}{}>", or_empty(&element.name), attributes(&element.attributes)),
            children(&element.children, true),
        ),
        Node::MdxFlowExpression(expression) => vec![other(format!("flowexpr {{{}}}", expression.value))],
        Node::MdxTextExpression(expression) => vec![other(format!("textexpr {{{}}}", expression.value))],
        Node::MdxJsEsm(esm) => vec![other(format!("esm {}", quote(&esm.value)))],
        Node::Empty => Vec::new(),
        node => panic!("unexpected node {}", node.name()),
    }
}

fn tokens(markdown: &Markdown) -> Vec<String> {
    children(&markdown.nodes, false)
        .into_iter()
        .map(|token| match token {
            Token::Text(text) => format!("T{}", quote(&text)),
            Token::Other(token) => token,
        })
        .collect()
}

/// Checks the tokens of `input`, and that what mq writes of it reads back as the same tokens.
fn check(input: &str, expected: &[&str], parse: fn(&str) -> miette::Result<Markdown>) {
    let markdown = parse(input).unwrap_or_else(|error| panic!("{input:?}: {error}"));
    assert_eq!(tokens(&markdown), expected, "{input:?}");
    let written = markdown.to_string();
    let reparsed = parse(&written).unwrap_or_else(|error| panic!("{input:?} written as {written:?}: {error}"));
    assert_eq!(tokens(&reparsed), expected, "{input:?} written as {written:?}");
}

#[rstest]
// Hand-written.
// should support a self-closing element
#[case("<a />", &["flow <a>(", ")"])]
#[case(r#"<a b="c&amp;d" e='f' g={h} {...i} j />"#, &[r#"flow <a b=lit:"c&d" e=lit:"f" g=expr:{h} ...{...i} j>("#, ")"])]
#[case("<a.b.c />", &["flow <a.b.c>(", ")"])]
#[case(r#"<a:b c:d="e" />"#, &[r#"flow <a:b c:d=lit:"e">("#, ")"])]
#[case("<></>", &["flow <>(", ")"])]
#[case("<a>x</a>", &["text <a>(", r#"T"x""#, ")"])]
#[case("<a>\n\nx\n\n</a>", &["flow <a>(", r#"T"x""#, ")"])]
#[case("<a>\n  x\n</a>", &["flow <a>(", r#"T"x""#, ")"])]
#[case("a <b>c</b> d", &[r#"T"a ""#, "text <b>(", r#"T"c""#, ")", r#"T" d""#])]
#[case("a <b/> d", &[r#"T"a ""#, "text <b>(", ")", r#"T" d""#])]
// should support an expression
#[case("{a}", &["flowexpr {a}"])]
// Differs from the official packages: Expressions are not parsed as JavaScript, so invalid JavaScript is accepted.
#[case("{a {b} c}", &["flowexpr {a {b} c}"])]
// Differs from the official packages: Expressions are not parsed as JavaScript, so invalid JavaScript is accepted.
#[case("{a\nb}", &["flowexpr {a\nb}"])]
// Differs from the official packages: Expressions are not parsed as JavaScript, so invalid JavaScript is accepted.
#[case("{a\n  b\n   c}", &["flowexpr {a\nb\n c}"])]
#[case("x {a} y", &[r#"T"x ""#, "textexpr {a}", r#"T" y""#])]
#[case("{a} <b/>", &["flowexpr {a}", "flow <b>(", ")"])]
#[case("<b/> {a}", &["flow <b>(", ")", "flowexpr {a}"])]
#[case("<b/> x", &["text <b>(", ")", r#"T" x""#])]
#[case("{a}x", &["textexpr {a}", r#"T"x""#])]
#[case("<a>{b}</a>", &["flow <a>(", "flowexpr {b}", ")"])]
#[case("a < b", &[r#"T"a < b""#])]
#[case("<a>\n<b>\n</b>\n</a>", &["flow <a>(", "flow <b>(", ")", ")"])]
#[case("> <a>\n> x\n> </a>", &["blockquote(", "flow <a>(", r#"T"x""#, ")", ")"])]
#[case("- <a>\n  x\n  </a>", &["item 0 u - t(", "flow <a>(", r#"T"x""#, ")", ")"])]
#[case(r#"<a b='c"d' />"#, &[r#"flow <a b=lit:"c\"d">("#, ")"])]
#[case("<a/><b/>", &["flow <a>(", ")", "flow <b>(", ")"])]
#[case("<a>x</a><b>y</b>", &["text <a>(", r#"T"x""#, ")", "text <b>(", r#"T"y""#, ")"])]
#[case("a\n<b/>", &[r#"T"a""#, "flow <b>(", ")"])]
#[case("a\n{b}", &[r#"T"a""#, "flowexpr {b}"])]
#[case(r#"<b e= "f"/>"#, &[r#"flow <b e=lit:"f">("#, ")"])]
#[case(r#"a <b e = "f"/>."#, &[r#"T"a ""#, r#"text <b e=lit:"f">("#, ")", r#"T".""#])]
#[case("<b c\n= \"x\">c</b>", &[r#"text <b c=lit:"x">("#, r#"T"c""#, ")"])]
// should support prefixed attributes
#[case("a <b xml :\tlang\n= \"de-CH\" foo:bar>c</b>.", &[r#"T"a ""#, r#"text <b xml:lang=lit:"de-CH" foo:bar>("#, r#"T"c""#, ")", r#"T".""#])]
// should support prefixed and normal attributes
#[case(r#"a <b a b : c d : e = "f" g/>."#, &[r#"T"a ""#, r#"text <b a b:c d:e=lit:"f" g>("#, ")", r#"T".""#])]
#[case("<b e= {f}/>", &["flow <b e=expr:{f}>(", ")"])]
#[case("<a>\n\n# h\n\n- l\n\n</a>", &["flow <a>(", "heading 1(", r#"T"h""#, ")", "item 0 u - t(", r#"T"l""#, ")", ")"])]
#[case("<Zoom>\n\n![a](b)\n\n</Zoom>", &["flow <Zoom>(", r#"image b  "a""#, ")"])]
#[case("<A>\n  <B>\n    x\n  </B>\n</A>", &["flow <A>(", "flow <B>(", r#"T"x""#, ")", ")"])]
#[case("import a from \"b\"\nexport const x = 1\n\nx\n", &[r#"esm "import a from \"b\"\nexport const x = 1""#, r#"T"x""#])]
#[case("export default d\n", &[r#"esm "export default d""#])]
#[case("  import a from \"b\"\n", &[r#"T"import a from \"b\"""#])]
#[case("import{a} from \"b\"\n", &[r#"T"import""#, "textexpr {a}", r#"T" from \"b\"""#])]
#[case("importx a\n", &[r#"T"importx a""#])]
#[case("- import a from \"b\"\n", &["item 0 u - t(", r#"T"import a from \"b\"""#, ")"])]
#[case("text\nimport a from \"b\"\n", &[r#"T"text\nimport a from \"b\"""#])]
// Differs from the official packages: A tag that is not closed before the end of its text is text here and in markdown-rs, and an error in the JavaScript packages.
#[case("<a", &[r#"T"<a""#])]
// Differs from the official packages: A tag that is not closed before the end of its text is text here and in markdown-rs, and an error in the JavaScript packages.
#[case("<a b", &[r#"T"<a b""#])]
// Differs from the official packages: A tag that is not closed before the end of its text is text here and in markdown-rs, and an error in the JavaScript packages.
#[case("<a b=", &[r#"T"<a b=""#])]
#[case(r#"a <b c="d" /> e"#, &[r#"T"a ""#, r#"text <b c=lit:"d">("#, ")", r#"T" e""#])]
#[case("a <b>c</b>", &[r#"T"a ""#, "text <b>(", r#"T"c""#, ")"])]
#[case("<a b=\"c\nd\" />", &[r#"flow <a b=lit:"c\nd">("#, ")"])]
#[case("<a b='&lt;' />", &[r#"flow <a b=lit:"<">("#, ")"])]
#[case(r#"<a b="&#x41;" />"#, &[r#"flow <a b=lit:"A">("#, ")"])]
#[case("<a {...b} {...c} />", &["flow <a ...{...b} ...{...c}>(", ")"])]
// should crash if not a spread
// Differs from the official packages: Expressions are not parsed as JavaScript, so invalid JavaScript is accepted.
#[case("<a {b} />", &["flow <a ...{b}>(", ")"])]
#[case("<a b={c} d={e} />", &["flow <a b=expr:{c} d=expr:{e}>(", ")"])]
#[case("<a b={{c: 1}} />", &["flow <a b=expr:{{c: 1}}>(", ")"])]
#[case("{/* comment */}", &["flowexpr {/* comment */}"])]
// should support an empty expression
#[case("{}", &["flowexpr {}"])]
// should support an empty expression (1)
#[case("a {} b", &[r#"T"a ""#, "textexpr {}", r#"T" b""#])]
#[case("a {b} {c} d", &[r#"T"a ""#, "textexpr {b}", r#"T" ""#, "textexpr {c}", r#"T" d""#])]
#[case("<a>{b}{c}</a>", &["text <a>(", "textexpr {b}", "textexpr {c}", ")"])]
#[case("# <a/>", &["heading 1(", "text <a>(", ")", ")"])]
#[case("# a {b} c", &["heading 1(", r#"T"a ""#, "textexpr {b}", r#"T" c""#, ")"])]
#[case("*<a>b</a>*", &["em(", "text <a>(", r#"T"b""#, ")", ")"])]
#[case("**a {b}**", &["strong(", r#"T"a ""#, "textexpr {b}", ")"])]
#[case("[<a/>](b)", &["link b (", "text <a>(", ")", ")"])]
#[case("`<a/>`", &[r#"code_inline "<a/>""#])]
#[case("`{a}`", &[r#"code_inline "{a}""#])]
#[case("<a/>\n<b/>", &["flow <a>(", ")", "flow <b>(", ")"])]
#[case("<a/> <b/>", &["flow <a>(", ")", "flow <b>(", ")"])]
#[case("<a/>x\n", &["text <a>(", ")", r#"T"x""#])]
#[case("x<a/>", &[r#"T"x""#, "text <a>(", ")"])]
#[case("<a>\n\nb\n\n</a>\n\n<c/>", &["flow <a>(", r#"T"b""#, ")", "flow <c>(", ")"])]
// should support an element w/ content
#[case("<a>\nb\n</a>", &["flow <a>(", r#"T"b""#, ")"])]
#[case("<a>\n  <b/>\n</a>", &["flow <a>(", "flow <b>(", ")", ")"])]
#[case("- a\n  <b/>\n", &["item 0 u - t(", r#"T"a""#, "flow <b>(", ")", ")"])]
#[case("> a\n> <b/>\n", &["blockquote(", r#"T"a""#, "flow <b>(", ")", ")"])]
// should support an element w/ containers as content
#[case("<a>\n- b\n</a>", &["flow <a>(", "item 0 u - t(", r#"T"b""#, ")", ")"])]
#[case("<a>\n\n- b\n\n</a>", &["flow <a>(", "item 0 u - t(", r#"T"b""#, ")", ")"])]
#[case("<a b=\"c\" d=\"e\">\n\nf\n\n</a>", &[r#"flow <a b=lit:"c" d=lit:"e">("#, r#"T"f""#, ")"])]
#[case("<a-b />", &["flow <a-b>(", ")"])]
#[case("<a.b-c />", &["flow <a.b-c>(", ")"])]
#[case("<_a />", &["flow <_a>(", ")"])]
#[case("<$a />", &["flow <$a>(", ")"])]
#[case("< a />", &[r#"T"< a />""#])]
#[case("<a / >", &["flow <a>(", ")"])]
#[case("<a/ >", &["flow <a>(", ")"])]
#[case(r#"<a b-c="d" />"#, &[r#"flow <a b-c=lit:"d">("#, ")"])]
#[case(r#"<a b:c-d="e" />"#, &[r#"flow <a b:c-d=lit:"e">("#, ")"])]
#[case("<a b='c' d />", &[r#"flow <a b=lit:"c" d>("#, ")"])]
#[case("<a\nb\n/>", &["flow <a b>(", ")"])]
#[case("<a\n  b=\"c\"\n/>", &[r#"flow <a b=lit:"c">("#, ")"])]
#[case("{a}\n{b}", &["flowexpr {a}", "flowexpr {b}"])]
#[case("{a}\n\n{b}", &["flowexpr {a}", "flowexpr {b}"])]
#[case("{a} {b}", &["textexpr {a}", r#"T" ""#, "textexpr {b}"])]
#[case("{a}{b}", &["textexpr {a}", "textexpr {b}"])]
#[case("x\n{a}\n", &[r#"T"x""#, "flowexpr {a}"])]
#[case("{\n}", &["flowexpr {\n}"])]
#[case("{a}\n<b/>", &["flowexpr {a}", "flow <b>(", ")"])]
#[case("<b/>\n{a}", &["flow <b>(", ")", "flowexpr {a}"])]
// should support a closed element
#[case("a <b> c </b> d", &[r#"T"a ""#, "text <b>(", r#"T" c ""#, ")", r#"T" d""#])]
#[case("<b>\n</b>", &["flow <b>(", ")"])]
#[case("<b></b>", &["flow <b>(", ")"])]
#[case("<b> </b>", &["flow <b>(", ")"])]
#[case("<>a</>", &["text <>(", r#"T"a""#, ")"])]
#[case("<>\n\na\n\n</>", &["flow <>(", r#"T"a""#, ")"])]
#[case("<a>{`}`}</a>", &["flow <a>(", "flowexpr {`}`}", ")"])]
#[case("{'}'}", &["flowexpr {'}'}"])]
#[case(r#"{"}"}"#, &[r#"flowexpr {"}"}"#])]
#[case(r#"<a b="{c}" />"#, &[r#"flow <a b=lit:"{c}">("#, ")"])]
#[case(r#"<a b={"c"} />"#, &[r#"flow <a b=expr:{"c"}>("#, ")"])]
#[case("<a b={`c`} />", &["flow <a b=expr:{`c`}>(", ")"])]
#[case(r#"a\<b/>"#, &[r#"T"a<b/>""#])]
#[case(r#"a\{b}"#, &[r#"T"a{b}""#])]
#[case("&lt;a/>", &[r#"T"<a/>""#])]
#[case("a &#123;b}", &[r#"T"a {b}""#])]
#[case("https://a.b", &[r#"T"https://a.b""#])]
#[case("www.a.b", &[r#"T"www.a.b""#])]
#[case("---\ntitle: x\n---\n\na", &[r#"yaml "title: x""#, r#"T"a""#])]
#[case("a\n---\n", &["heading 2(", r#"T"a""#, ")"])]
#[case("***\n", &["hr"])]
#[case("```\n<a/>\n```\n", &[r#"code   "<a/>""#])]
#[case("    <a/>\n", &["flow <a>(", ")"])]
#[case("<a/>\n    x\n", &["flow <a>(", ")", r#"T"x""#])]
#[case("<a>\n\n    x\n\n</a>", &["flow <a>(", r#"T"x""#, ")"])]
#[case("1. <a/>\n", &["item 0 o1 - t(", "flow <a>(", ")", ")"])]
#[case("* <a>\n  b\n  </a>\n", &["item 0 u - t(", "flow <a>(", r#"T"b""#, ")", ")"])]
// Differs from the official packages: A tag that is not closed before the end of its text is text here and in markdown-rs, and an error in the JavaScript packages.
#[case("x <a", &[r#"T"x <a""#])]
// Differs from the official packages: A tag that is not closed before the end of its text is text here and in markdown-rs, and an error in the JavaScript packages.
#[case("x <a b", &[r#"T"x <a b""#])]
// Differs from the official packages: A tag that is not closed before the end of its text is text here and in markdown-rs, and an error in the JavaScript packages.
#[case("x <a b=", &[r#"T"x <a b=""#])]
// Differs from the official packages: A tag that is not closed before the end of its text is text here and in markdown-rs, and an error in the JavaScript packages.
#[case("<a\n\nb", &[r#"T"<a""#, r#"T"b""#])]
// Differs from the official packages: A tag that is not closed before the end of its text is text here and in markdown-rs, and an error in the JavaScript packages.
#[case("<a\nb", &[r#"T"<a\nb""#])]
// Differs from the official packages: A tag that is not closed before the end of its text is text here and in markdown-rs, and an error in the JavaScript packages.
#[case("a\n<a", &[r#"T"a\n<a""#])]
// Differs from the official packages: A tag that is not closed before the end of its text is text here and in markdown-rs, and an error in the JavaScript packages.
#[case("x <", &[r#"T"x <""#])]
// Differs from the official packages: A tag that is not closed before the end of its text is text here and in markdown-rs, and an error in the JavaScript packages.
#[case("<", &[r#"T"<""#])]
// Differs from the official packages: A tag that is not closed before the end of its text is text here and in markdown-rs, and an error in the JavaScript packages.
#[case("x <a\n\nb", &[r#"T"x <a""#, r#"T"b""#])]
// Differs from the official packages: A tag that is not closed before the end of its text is text here and in markdown-rs, and an error in the JavaScript packages.
#[case("x <a b\n\nc", &[r#"T"x <a b""#, r#"T"c""#])]
// Differs from the official packages: A tag that is not closed before the end of its text is text here and in markdown-rs, and an error in the JavaScript packages.
#[case("- <a", &["item 0 u - t(", r#"T"<a""#, ")"])]
// Differs from the official packages: A tag that is not closed before the end of its text is text here and in markdown-rs, and an error in the JavaScript packages.
#[case("> <a", &["blockquote(", r#"T"<a""#, ")"])]
// Differs from the official packages: Expressions are not parsed as JavaScript, so invalid JavaScript is accepted.
#[case("{a\n\nb}", &["flowexpr {a\n\nb}"])]
#[case("{a /* } */}", &["flowexpr {a /* } */}"])]
#[case("{`${'}'}`}", &["flowexpr {`${'}'}`}"])]
#[case("<a b={'}'} />", &["flow <a b=expr:{'}'}>(", ")"])]
#[case("{a // }\n}", &["flowexpr {a // }\n}"])]
#[case(r#"{"a}" + '{'}"#, &[r#"flowexpr {"a}" + '{'}"#])]
#[case(r#"<a b={"}"} c='}' />"#, &[r#"flow <a b=expr:{"}"} c=lit:"}">("#, ")"])]
#[case("{a /* { */}", &["flowexpr {a /* { */}"])]
#[case("{`a${b}c`}", &["flowexpr {`a${b}c`}"])]
#[case("{`a${`}`}c`}", &["flowexpr {`a${`}`}c`}"])]
#[case(r#"{'\'}'}"#, &[r#"flowexpr {'\'}'}"#])]
#[case("{a // {\n}", &["flowexpr {a // {\n}"])]
#[case("x {'}'} y", &[r#"T"x ""#, "textexpr {'}'}", r#"T" y""#])]
#[case("x {`}`} y", &[r#"T"x ""#, "textexpr {`}`}", r#"T" y""#])]
#[case("{/* } */}\n", &["flowexpr {/* } */}"])]
#[case("<a>{'}'}</a>", &["flow <a>(", "flowexpr {'}'}", ")"])]
// Differs from the official packages: Expressions are not parsed as JavaScript, so invalid JavaScript is accepted.
#[case("{'a\nb'}", &["flowexpr {'a\nb'}"])]
#[case("{a / b}", &["flowexpr {a / b}"])]
#[case("{a /* x */ + 1}", &["flowexpr {a /* x */ + 1}"])]
#[case("---\ntitle: x\n---\n\n<A />\n", &[r#"yaml "title: x""#, "flow <A>(", ")"])]
#[case("+++\na = 1\n+++\n\nb\n", &[r#"toml "a = 1""#, r#"T"b""#])]
#[case("---\na\n", &["hr", r#"T"a""#])]
#[case("---\n\n---\n\nx\n", &[r#"yaml """#, r#"T"x""#])]
#[case("---\ntitle: x\n---\nimport a from \"b\"\n\n{a}\n", &[r#"yaml "title: x""#, r#"esm "import a from \"b\"""#, "flowexpr {a}"])]
// From markdown-rs, `tests/mdx_esm.rs`.
// should support an import
#[case("import a from 'b'\n\nc", &[r#"esm "import a from 'b'""#, r#"T"c""#])]
// should support an export
#[case("export default a\n\nb", &[r#"esm "export default a""#, r#"T"b""#])]
// should not support other keywords (`impossible`)
#[case("impossible", &[r#"T"impossible""#])]
// should not support other keywords (`exporting`)
#[case("exporting", &[r#"T"exporting""#])]
// should not support a non-whitespace after the keyword
#[case("import.", &[r#"T"import.""#])]
// should not support a non-whitespace after the keyword (import-as-a-function)
#[case("import('a')", &[r#"T"import('a')""#])]
// should not support an indent
#[case("  import a from 'b'\n  export default c", &[r#"T"import a from 'b'\nexport default c""#])]
// should not support keywords in containers
#[case("- import a from 'b'\n> export default c", &["item 0 u - t(", r#"T"import a from 'b'""#, ")", "blockquote(", r#"T"export default c""#, ")"])]
// should support imports and exports in the same “block”
#[case("import a from 'b'\nexport default c", &[r#"esm "import a from 'b'\nexport default c""#])]
// should support imports and exports in separate “blocks”
#[case("import a from 'b'\n\nexport default c", &[r#"esm "import a from 'b'""#, r#"esm "export default c""#])]
// should support imports and exports in between other constructs
#[case("a\n\nimport a from 'b'\n\nb\n\nexport default c", &[r#"T"a""#, r#"esm "import a from 'b'""#, r#"T"b""#, r#"esm "export default c""#])]
// should not support import/exports when interrupting paragraphs
#[case("a\nimport a from 'b'\n\nb\nexport default c", &[r#"T"a\nimport a from 'b'""#, r#"T"b\nexport default c""#])]
// should crash on invalid import/exports (1)
// Differs from the official packages: Expressions and ESM are not parsed as JavaScript, so invalid JavaScript is accepted.
#[case("import a", &[r#"esm "import a""#])]
// should crash on invalid import/exports (2)
// Differs from the official packages: Expressions and ESM are not parsed as JavaScript, so invalid JavaScript is accepted.
#[case("import 1/1", &[r#"esm "import 1/1""#])]
// should support line endings in import/exports
#[case("export {\n  a\n} from 'b'\n\nc", &[r#"esm "export {\n  a\n} from 'b'""#, r#"T"c""#])]
// should support blank lines in import/exports
#[case("export {\n\n  a\n\n} from 'b'\n\nc", &[r#"esm "export {\n\n  a\n\n} from 'b'""#, r#"T"c""#])]
// should crash on markdown after import/export w/o blank line
// Differs from the official packages: Expressions and ESM are not parsed as JavaScript, so invalid JavaScript is accepted.
#[case("import a from 'b'\n*md*?", &[r#"esm "import a from 'b'\n*md*?""#])]
// should support comments in “blocks”
#[case("export var a = 1\n// b\n/* c */\n\nd", &[r#"esm "export var a = 1\n// b\n/* c */""#, r#"T"d""#])]
// should crash on other statements in “blocks”
// Differs from the official packages: Expressions and ESM are not parsed as JavaScript, so invalid JavaScript is accepted.
#[case("export var a = 1\nvar b\n\nc", &[r#"esm "export var a = 1\nvar b""#, r#"T"c""#])]
// should crash on import-as-a-function with a space `import (x)`
// Differs from the official packages: Expressions and ESM are not parsed as JavaScript, so invalid JavaScript is accepted.
#[case("import ('a')\n\nb", &[r#"esm "import ('a')""#, r#"T"b""#])]
// should support a reexport from another import
#[case("import a from 'b'\nexport {a}\n\nc", &[r#"esm "import a from 'b'\nexport {a}""#, r#"T"c""#])]
// should support a reexport from another import w/ semicolons
#[case("import a from 'b';\nexport {a};\n\nc", &[r#"esm "import a from 'b';\nexport {a};""#, r#"T"c""#])]
// should support a reexport default from another import
#[case("import a from 'b'\nexport {a as default}\n\nc", &[r#"esm "import a from 'b'\nexport {a as default}""#, r#"T"c""#])]
// should support JSX by default
#[case("export var a = () => <b />", &[r#"esm "export var a = () => <b />""#])]
// should support EOF after EOL
// Differs from the official packages: Expressions and ESM are not parsed as JavaScript, so invalid JavaScript is accepted.
#[case("export {a}\n", &[r#"esm "export {a}""#])]
// should support a reexport from another esm block (1)
#[case("import a from 'b'\n\nexport {a}\n\nc", &[r#"esm "import a from 'b'""#, r#"esm "export {a}""#, r#"T"c""#])]
// should support a reexport from another esm block (2)
#[case("import a from 'b'\n\nexport {a}\n\n# c", &[r#"esm "import a from 'b'""#, r#"esm "export {a}""#, "heading 1(", r#"T"c""#, ")"])]
// should support mdx esm as `MdxjsEsm`s in mdast
#[case("import a from 'b'\nexport {a}", &[r#"esm "import a from 'b'\nexport {a}""#])]
// From markdown-rs, `tests/mdx_expression_flow.rs`.
// should prefer indented code over expressions if it’s enabled
#[case("    {}", &["flowexpr {}"])]
// should support indented expressions if indented code is enabled
#[case("   {}", &["flowexpr {}"])]
// should support a line ending in an expression
#[case("{\n}\na", &["flowexpr {\n}", r#"T"a""#])]
// should support expressions followed by spaces
#[case("{ a } \t\nb", &["flowexpr { a }", r#"T"b""#])]
// should support expressions preceded by spaces
#[case("  { a }\nb", &["flowexpr { a }", r#"T"b""#])]
// should support lists after non-expressions (wooorm/markdown-rs#11)
#[case("a\n\n* b", &[r#"T"a""#, "item 0 u - t(", r#"T"b""#, ")"])]
// should not support laziness (2)
#[case("> a\n{b}", &["blockquote(", r#"T"a""#, ")", "flowexpr {b}"])]
// should not support laziness (3)
#[case("> {a}\nb", &["blockquote(", "flowexpr {a}", ")", r#"T"b""#])]
// should support mdx expressions (flow) as `MdxFlowExpression`s in mdast
#[case("{alpha +\nbravo}", &["flowexpr {alpha +\nbravo}"])]
// should support indent in `MdxFlowExpression` in mdast
#[case("  {`\n    a\n  `}", &["flowexpr {`\n  a\n`}"])]
// should support expressions padded w/ parens
#[case("a{(b)}c", &[r#"T"a""#, "textexpr {(b)}", r#"T"c""#])]
// should support expressions padded w/ parens and comments
#[case("a{/* b */ ( (c) /* d */ + (e) )}f", &[r#"T"a""#, "textexpr {/* b */ ( (c) /* d */ + (e) )}", r#"T"f""#])]
// should use correct positional info when tabs are used (1, indent)
#[case("{`\n\t`}", &["flowexpr {`\n  `}"])]
// should use correct positional info when tabs are used (2, content)
#[case("{`\nalpha\t`}", &["flowexpr {`\nalpha\t`}"])]
// should support template strings in JSX (text) in block quotes
#[case(">  aaa <b c={`\n>      d\n>  `} /> eee", &["blockquote(", r#"T"aaa ""#, "text <b c=expr:{`\n   d\n`}>(", ")", r#"T" eee""#, ")"])]
// should use correct positional when there are virtual spaces due to a block quote
#[case("> ab {`\n>\t`}", &["blockquote(", r#"T"ab ""#, "textexpr {`\n`}", ")"])]
// should keep the correct number of spaces in a blockquote (flow)
#[case("> {`\n> alpha\n>  bravo\n>   charlie\n>    delta\n> `}", &["blockquote(", "flowexpr {`\nalpha\nbravo\ncharlie\n delta\n`}", ")"])]
// should support a spread
#[case("<a {...b} />", &["flow <a ...{...b}>(", ")"])]
// should crash on an incorrect spread
// Differs from the official packages: Expressions and ESM are not parsed as JavaScript, so invalid JavaScript is accepted.
#[case("<a {...?} />", &["flow <a ...{...?}>(", ")"])]
// should crash if not an identifier
// Differs from the official packages: Expressions and ESM are not parsed as JavaScript, so invalid JavaScript is accepted.
#[case("<a {b=c} />", &["flow <a ...{b=c}>(", ")"])]
// should crash on an empty spread
// Differs from the official packages: Expressions and ESM are not parsed as JavaScript, so invalid JavaScript is accepted.
#[case("<a {} />", &["flow <a ...{}>(", ")"])]
// should crash on a comment spread
// Differs from the official packages: Expressions and ESM are not parsed as JavaScript, so invalid JavaScript is accepted.
#[case("<a {/* b */} />", &["flow <a ...{/* b */}>(", ")"])]
// From markdown-rs, `tests/mdx_expression_text.rs`.
// should support an empty expression (2)
#[case("a { \t\r\n} b", &[r#"T"a ""#, "textexpr { \t\r\n}", r#"T" b""#])]
// should support a multiline comment (1)
#[case("a {/**/} b", &[r#"T"a ""#, "textexpr {/**/}", r#"T" b""#])]
// should support a multiline comment (2)
#[case("a {  /*\n*/\t} b", &[r#"T"a ""#, "textexpr {  /*\n*/\t}", r#"T" b""#])]
// should support a multiline comment (3)
#[case("a {/*b*//*c*/} d", &[r#"T"a ""#, "textexpr {/*b*//*c*/}", r#"T" d""#])]
// should support a multiline comment (4)
#[case("a {b/*c*/} d", &[r#"T"a ""#, "textexpr {b/*c*/}", r#"T" d""#])]
// should support a multiline comment (5)
#[case("a {/*b*/c} d", &[r#"T"a ""#, "textexpr {/*b*/c}", r#"T" d""#])]
// should support a line comment followed by a line ending
#[case("a {//\n} b", &[r#"T"a ""#, "textexpr {//\n}", r#"T" b""#])]
// should support a line comment followed by a line ending and an expression
#[case("a {// b\nc} d", &[r#"T"a ""#, "textexpr {// b\nc}", r#"T" d""#])]
// should support an expression followed by a line comment and a line ending
#[case("a {b// c\n} d", &[r#"T"a ""#, "textexpr {b// c\n}", r#"T" d""#])]
// should support comments
#[case("a {/*b*/ // c\n} d", &[r#"T"a ""#, "textexpr {/*b*/ // c\n}", r#"T" d""#])]
// should support expression statements (1)
#[case("a {b.c} d", &[r#"T"a ""#, "textexpr {b.c}", r#"T" d""#])]
// should support expression statements (2)
#[case("a {1 + 1} b", &[r#"T"a ""#, "textexpr {1 + 1}", r#"T" b""#])]
// should support expression statements (3)
#[case("a {function () {}} b", &[r#"T"a ""#, "textexpr {function () {}}", r#"T" b""#])]
// should crash on non-expressions
// Differs from the official packages: Expressions and ESM are not parsed as JavaScript, so invalid JavaScript is accepted.
#[case(r#"a {var b = "c"} d"#, &[r#"T"a ""#, r#"textexpr {var b = "c"}"#, r#"T" d""#])]
// should support expressions in containers
#[case("> a {\n> b} c", &["blockquote(", r#"T"a ""#, "textexpr {\nb}", r#"T" c""#, ")"])]
// should crash on incorrect expressions in containers (1)
// Differs from the official packages: Expressions and ESM are not parsed as JavaScript, so invalid JavaScript is accepted.
#[case("> a {\n> b<} c", &["blockquote(", r#"T"a ""#, "textexpr {\nb<}", r#"T" c""#, ")"])]
// should crash on incorrect expressions in containers (2)
// Differs from the official packages: Expressions and ESM are not parsed as JavaScript, so invalid JavaScript is accepted.
#[case("> a {\n> b\n> c} d", &["blockquote(", r#"T"a ""#, "textexpr {\nb\nc}", r#"T" d""#, ")"])]
// should support an expression
#[case("a {b} c", &[r#"T"a ""#, "textexpr {b}", r#"T" c""#])]
// should support a line ending in an expression
#[case("a {\n} b", &[r#"T"a ""#, "textexpr {\n}", r#"T" b""#])]
// should support just a closing brace
#[case("a } b", &[r#"T"a } b""#])]
// should support expressions as the first thing when following by other things
#[case("{ a } b", &["textexpr { a }", r#"T" b""#])]
// should support mdx expressions (text) as `MdxTextExpression`s in mdast
#[case("a {alpha} b.", &[r#"T"a ""#, "textexpr {alpha}", r#"T" b.""#])]
// should crash on an incorrect expression
// Differs from the official packages: Expressions and ESM are not parsed as JavaScript, so invalid JavaScript is accepted.
#[case("a {??} b", &[r#"T"a ""#, "textexpr {??}", r#"T" b""#])]
// should support an unbalanced opening brace (if JS permits)
#[case("a { /* { */ } b", &[r#"T"a ""#, "textexpr { /* { */ }", r#"T" b""#])]
// should support an unbalanced closing brace (if JS permits)
#[case("a { /* } */ } b", &[r#"T"a ""#, "textexpr { /* } */ }", r#"T" b""#])]
// should keep the correct number of spaces in a blockquote (text)
#[case("> alpha {`\n> bravo\n>  charlie\n>   delta\n>    echo\n> `} foxtrot.", &["blockquote(", r#"T"alpha ""#, "textexpr {`\nbravo\ncharlie\ndelta\n echo\n`}", r#"T" foxtrot.""#, ")"])]
// From markdown-rs, `tests/mdx_jsx_flow.rs`.
// should prefer indented code over jsx if it’s enabled
#[case("    <a />", &["flow <a>(", ")"])]
// should support indented jsx if indented code is enabled
#[case("   <a />", &["flow <a>(", ")"])]
// should support a closed element
#[case("<a></a>", &["flow <a>(", ")"])]
// should support attributes
// Differs from the official packages: Expressions and ESM are not parsed as JavaScript, so invalid JavaScript is accepted.
#[case(r#"<a b c:d e="" f={/* g */} {...h} />"#, &[r#"flow <a b c:d e=lit:"" f=expr:{/* g */} ...{...h}>("#, ")"])]
// should support a dangling `>` in a tag (not a block quote)
#[case("<x\n  y\n>  \nb\n  </x>", &["flow <x y>(", r#"T"b""#, ")"])]
// should support trailing initial and final whitespace around tags
#[case("<a>  \nb\n  </a>", &["flow <a>(", r#"T"b""#, ")"])]
// should support tags after tags
#[case("<a> <b>\t\nc\n  </b> </a>", &["flow <a>(", "flow <b>(", r#"T"c""#, ")", ")"])]
// should not support lazy flow (8)
#[case("> a\n<X />", &["blockquote(", r#"T"a""#, ")", "flow <X>(", ")"])]
// should support mdx jsx (flow) as `MdxJsxFlowElement`s in mdast
#[case("<>\n  * a\n</>", &["flow <>(", "item 0 u - t(", r#"T"a""#, ")", ")"])]
// should support tags and expressions (unaware)
#[case("<div>\n{1}\n</div>", &["flow <div>(", "flowexpr {1}", ")"])]
// should support tags and expressions (aware)
#[case("<div>\n{'}'}\n</div>", &["flow <div>(", "flowexpr {'}'}", ")"])]
// should support tags and expressions with text before (text)
#[case("x<em>{1}</em>", &[r#"T"x""#, "text <em>(", "textexpr {1}", ")"])]
// should support tags and expressions with text between, early (text)
#[case("<em>x{1}</em>", &["text <em>(", r#"T"x""#, "textexpr {1}", ")"])]
// should support tags and expressions with text between, late (text)
#[case("<em>{1}x</em>", &["text <em>(", "textexpr {1}", r#"T"x""#, ")"])]
// should support tags and expressions with text after (text)
#[case("<em>{1}</em>x", &["text <em>(", "textexpr {1}", ")", r#"T"x""#])]
// should support a tag and then an expression (flow)
#[case("<x/>{1}", &["flow <x>(", ")", "flowexpr {1}"])]
// should support a tag, an expression, then text (text)
#[case("<x/>{1}x", &["text <x>(", ")", "textexpr {1}", r#"T"x""#])]
// should support text, a tag, then an expression (text)
#[case("x<x/>{1}", &[r#"T"x""#, "text <x>(", ")", "textexpr {1}"])]
// should support an expression and then a tag (flow)
#[case("{1}<x/>", &["flowexpr {1}", "flow <x>(", ")"])]
// should support an expression, a tag, then text (text)
#[case("{1}<x/>x", &["textexpr {1}", "text <x>(", ")", r#"T"x""#])]
// should support text, an expression, then a tag (text)
#[case("x{1}<x/>", &[r#"T"x""#, "textexpr {1}", "text <x>(", ")"])]
// should nicely interleaf (micromark/micromark-extension-mdx-jsx#9)
#[case("<x>{[\n'',\n{c:''}\n]}</x>", &["flow <x>(", "flowexpr {[\n'',\n{c:''}\n]}", ")"])]
// should nicely interleaf (mdx-js/mdx#1945)
#[case("\n<style>{`\n  .foo {}\n  .bar {}\n`}</style>\n    ", &["flow <style>(", "flowexpr {`\n.foo {}\n.bar {}\n`}", ")"])]
// From markdown-rs, `tests/mdx_jsx_text.rs`.
// should support a self-closing element
#[case("a <b/> c.", &[r#"T"a ""#, "text <b>(", ")", r#"T" c.""#])]
// should support a closed element
#[case("a <b></b> c.", &[r#"T"a ""#, "text <b>(", ")", r#"T" c.""#])]
// should support fragments
#[case("a <></> c.", &[r#"T"a ""#, "text <>(", ")", r#"T" c.""#])]
// should support markdown inside elements
#[case("a <b>*b*</b> c.", &[r#"T"a ""#, "text <b>(", "em(", r#"T"b""#, ")", ")", r#"T" c.""#])]
// should support mdx jsx (text) with expression child
#[case("{1}<a/>", &["flowexpr {1}", "flow <a>(", ")"])]
// should support mdx jsx (text) with expression child
#[case("<a>{1}</a>", &["flow <a>(", "flowexpr {1}", ")"])]
// should support mdx jsx (text) as `MdxJsxTextElement`s in mdast (self-closing)
#[case("a <b /> c.", &[r#"T"a ""#, "text <b>(", ")", r#"T" c.""#])]
// should support mdx jsx (text) as `MdxJsxTextElement`s in mdast (matched open and close tags)
#[case("a <b>*c*</b> d.", &[r#"T"a ""#, "text <b>(", "em(", r#"T"c""#, ")", ")", r#"T" d.""#])]
// should support mdx jsx (text) as `MdxJsxTextElement`s in mdast (namespace in tag name)
#[case("<a:b />.", &["text <a:b>(", ")", r#"T".""#])]
// should support mdx jsx (text) as `MdxJsxTextElement`s in mdast (members in tag name)
#[case("<a.b.c />.", &["text <a.b.c>(", ")", r#"T".""#])]
// should support mdx jsx (text) as `MdxJsxTextElement`s in mdast (attribute expression)
#[case("<a {...b} />.", &["text <a ...{...b}>(", ")", r#"T".""#])]
// should support mdx jsx (text) as `MdxJsxTextElement`s in mdast (property names)
#[case("<a b c:d />.", &["text <a b c:d>(", ")", r#"T".""#])]
// should support mdx jsx (text) as `MdxJsxTextElement`s in mdast (attribute values)
#[case(r#"<a b='c' d="e" f={g} />."#, &[r#"text <a b=lit:"c" d=lit:"e" f=expr:{g}>("#, ")", r#"T".""#])]
// should support character references (HTML 4, named) in JSX attribute values
#[case("<a b='&nbsp; &amp; &copy; &AElig; &Dcaron; &frac34; &HilbertSpace; &DifferentialD; &ClockwiseContourIntegral; &ngE;' />.", &[r#"text <a b=lit:"  & © Æ Ď ¾ ℋ ⅆ ∲ ≧̸">("#, ")", r#"T".""#])]
// should support character references (numeric) in JSX attribute values
#[case("<a b='&#35; &#1234; &#992; &#0;' c='&#X22; &#XD06; &#xcab;' />.", &["text <a b=lit:\"# Ӓ Ϡ �\" c=lit:\"\\\" ആ ಫ\">(", ")", r#"T".""#])]
// should not support things that look like character references but aren’t
#[case("<a b='&nbsp &x; &#; &#x; &#987654321; &#abcdef0; &ThisIsNotDefined; &hi?;' />.", &[r#"text <a b=lit:"&nbsp &x; &#; &#x; � &#abcdef0; &ThisIsNotDefined; &hi?;">("#, ")", r#"T".""#])]
// should support unicode whitespace in a lot of places
#[case("<a　b 　c　 d　/>.", &["text <a b c d>(", ")", r#"T".""#])]
// should support line endings in a lot of places
#[case("<a\nb \nc\n d\n/>.", &["text <a b c d>(", ")", r#"T".""#])]
// should support a self-closing element
#[case("a <b /> c", &[r#"T"a ""#, "text <b>(", ")", r#"T" c""#])]
// should support an attribute expression
// Differs from the official packages: Expressions and ESM are not parsed as JavaScript, so invalid JavaScript is accepted.
#[case("a <b {1 + 1} /> c", &[r#"T"a ""#, "text <b ...{1 + 1}>(", ")", r#"T" c""#])]
// should support an attribute value expression
#[case("a <b c={1 + 1} /> d", &[r#"T"a ""#, "text <b c=expr:{1 + 1}>(", ")", r#"T" d""#])]
// should support an attribute expression
#[case("a <b {...c} /> d", &[r#"T"a ""#, "text <b ...{...c}>(", ")", r#"T" d""#])]
// should support more complex attribute expression (1)
#[case("a <b {...{c: 1, d: Infinity, e: false}} /> f", &[r#"T"a ""#, "text <b ...{...{c: 1, d: Infinity, e: false}}>(", ")", r#"T" f""#])]
// should support more complex attribute expression (2)
#[case("a <b {...[1, Infinity, false]} /> d", &[r#"T"a ""#, "text <b ...{...[1, Infinity, false]}>(", ")", r#"T" d""#])]
// should crash on an empty attribute value expression
// Differs from the official packages: Expressions and ESM are not parsed as JavaScript, so invalid JavaScript is accepted.
#[case("a <b c={} /> d", &[r#"T"a ""#, "text <b c=expr:{}>(", ")", r#"T" d""#])]
// should crash on invalid JS in an attribute value expression
// Differs from the official packages: Expressions and ESM are not parsed as JavaScript, so invalid JavaScript is accepted.
#[case("a <b c={?} /> d", &[r#"T"a ""#, "text <b c=expr:{?}>(", ")", r#"T" d""#])]
// should crash on invalid JS in an attribute expression
// Differs from the official packages: Expressions and ESM are not parsed as JavaScript, so invalid JavaScript is accepted.
#[case("a <b {?} /> c", &[r#"T"a ""#, "text <b ...{?}>(", ")", r#"T" c""#])]
// should support parenthesized expressions
#[case("a <b c={(2)} d={<e />} /> f", &[r#"T"a ""#, "text <b c=expr:{(2)} d=expr:{<e />}>(", ")", r#"T" f""#])]
// should support non-ascii identifier start characters
#[case("a <π /> b.", &[r#"T"a ""#, "text <π>(", ")", r#"T" b.""#])]
// should support non-ascii identifier continuation characters
#[case("a <a‌b /> b.", &[r#"T"a ""#, "text <a‌b>(", ")", r#"T" b.""#])]
// should support dashes in names
#[case("a <a-->b</a-->.", &[r#"T"a ""#, "text <a-->(", r#"T"b""#, ")", r#"T".""#])]
// should support dots in names for method names
#[case("a <abc . def.ghi>b</abc.def . ghi>.", &[r#"T"a ""#, "text <abc.def.ghi>(", r#"T"b""#, ")", r#"T".""#])]
// should support colons in names for local names
#[case("a <svg: rect>b</  svg :rect>.", &[r#"T"a ""#, "text <svg:rect>(", r#"T"b""#, ")", r#"T".""#])]
// should support attribute expressions
#[case("a <b {...props} {...rest}>c</b>.", &[r#"T"a ""#, "text <b ...{...props} ...{...rest}>(", r#"T"c""#, ")", r#"T".""#])]
// should support nested balanced braces in attribute expressions
#[case(r#"a <b {...{"a": "b"}}>c</b>."#, &[r#"T"a ""#, r#"text <b ...{...{"a": "b"}}>("#, r#"T"c""#, ")", r#"T".""#])]
// should support attribute expressions directly after a name
#[case("<a{...b}/>.", &["text <a ...{...b}>(", ")", r#"T".""#])]
// should support attribute expressions directly after a member name
#[case("<a.b{...c}/>.", &["text <a.b ...{...c}>(", ")", r#"T".""#])]
// should support attribute expressions directly after a local name
#[case("<a:b{...c}/>.", &["text <a:b ...{...c}>(", ")", r#"T".""#])]
// should support attribute expressions directly after boolean attributes
#[case("a <b c{...d}/>.", &[r#"T"a ""#, "text <b c ...{...d}>(", ")", r#"T".""#])]
// should support attribute expressions directly after boolean qualified attributes
#[case("a <b c:d{...e}/>.", &[r#"T"a ""#, "text <b c:d ...{...e}>(", ")", r#"T".""#])]
// should support attribute expressions and normal attributes
#[case("a <b a {...props} b>c</b>.", &[r#"T"a ""#, "text <b a ...{...props} b>(", r#"T"c""#, ")", r#"T".""#])]
// should support attributes
#[case("a <b c     d=\"d\"\t\tefg=\"e\">c</b>.", &[r#"T"a ""#, r#"text <b c d=lit:"d" efg=lit:"e">("#, r#"T"c""#, ")", r#"T".""#])]
// should support attribute value expressions
#[case("a <b c={1 + 1}>c</b>.", &[r#"T"a ""#, "text <b c=expr:{1 + 1}>(", r#"T"c""#, ")", r#"T".""#])]
// should support nested balanced braces in attribute value expressions
#[case("a <b c={1 + ({a: 1}).a}>c</b>.", &[r#"T"a ""#, "text <b c=expr:{1 + ({a: 1}).a}>(", r#"T"c""#, ")", r#"T".""#])]
// should support an attribute directly after a value
#[case(r#"<a b=""c/>."#, &[r#"text <a b=lit:"" c>("#, ")", r#"T".""#])]
// should support an attribute directly after an attribute expression
#[case("<a{...b}c/>.", &["text <a ...{...b} c>(", ")", r#"T".""#])]
// should support whitespace directly after closing slash
#[case("<a/ \t>.", &["text <a>(", ")", r#"T".""#])]
// should *not* crash on closing angle in text
#[case("a > c.", &[r#"T"a > c.""#])]
// should *not* crash on opening angle in tick code in an element
#[case("a <>`<`</> c.", &[r#"T"a ""#, "text <>(", r#"code_inline "<""#, ")", r#"T" c.""#])]
// should *not* crash on ticks in tick code in an element
#[case("a <>`` ``` ``</>", &[r#"T"a ""#, "text <>(", r#"code_inline "```""#, ")"])]
// should support nested tags
#[case("a <>b <>c</> d</>.", &[r#"T"a ""#, "text <>(", r#"T"b ""#, "text <>(", r#"T"c""#, ")", r#"T" d""#, ")", r#"T".""#])]
// should support character references in attribute values
#[case(r#"<x y="Character references can be used: &quot;, &apos;, &lt;, &gt;, &#x7B;, and &#x7D;, they can be named, decimal, or hexadecimal: &copy; &#8800; &#x1D306;" />."#, &[r#"text <x y=lit:"Character references can be used: \", ', <, >, {, and }, they can be named, decimal, or hexadecimal: © ≠ 𝌆">("#, ")", r#"T".""#])]
// should support character references in text
#[case("<x>Character references can be used: &quot;, &apos;, &lt;, &gt;, &#x7B;, and &#x7D;, they can be named, decimal, or hexadecimal: &copy; &#8800; &#x1D306;</x>.", &["text <x>(", r#"T"Character references can be used: \", ', <, >, {, and }, they can be named, decimal, or hexadecimal: © ≠ 𝌆""#, ")", r#"T".""#])]
// should support as text if the closing tag is not the last thing
#[case("<x />.", &["text <x>(", ")", r#"T".""#])]
// should support as text if the opening is not the first thing
#[case("a <x />", &[r#"T"a ""#, "text <x>(", ")"])]
// should support line endings in elements
#[case("> a <b>\n> c </b> d.", &["blockquote(", r#"T"a ""#, "text <b>(", r#"T"\nc ""#, ")", r#"T" d.""#, ")"])]
// should support line endings in attribute values
#[case("> a <b c=\"d\ne\" /> f", &["blockquote(", r#"T"a ""#, r#"text <b c=lit:"d\ne">("#, ")", r#"T" f""#, ")"])]
// should support line endings in attribute value expressions
// Differs from the official packages: Expressions and ESM are not parsed as JavaScript, so invalid JavaScript is accepted.
#[case("> a <b c={d\ne} /> f", &["blockquote(", r#"T"a ""#, "text <b c=expr:{d\ne}>(", ")", r#"T" f""#, ")"])]
// should support line endings in attribute expressions
// Differs from the official packages: Expressions and ESM are not parsed as JavaScript, so invalid JavaScript is accepted.
#[case("> a <b {c\nd} /> e", &["blockquote(", r#"T"a ""#, "text <b ...{c\nd}>(", ")", r#"T" e""#, ")"])]
// should support lazy text (1)
#[case("> a <b\n/> c", &["blockquote(", r#"T"a ""#, "text <b>(", ")", r#"T" c""#, ")"])]
// should support lazy text (2)
#[case("> a <b c='\nd'/> e", &["blockquote(", r#"T"a ""#, r#"text <b c=lit:"\nd">("#, ")", r#"T" e""#, ")"])]
// should support lazy text (3)
#[case("> a <b c='d\n'/> e", &["blockquote(", r#"T"a ""#, r#"text <b c=lit:"d\n">("#, ")", r#"T" e""#, ")"])]
// should support lazy text (4)
#[case("> a <b c='d\ne'/> f", &["blockquote(", r#"T"a ""#, r#"text <b c=lit:"d\ne">("#, ")", r#"T" f""#, ")"])]
// should support lazy text (5)
// Differs from the official packages: Expressions and ESM are not parsed as JavaScript, so invalid JavaScript is accepted.
#[case("> a <b c={d\ne}/> f", &["blockquote(", r#"T"a ""#, "text <b c=expr:{d\ne}>(", ")", r#"T" f""#, ")"])]
// should allow `<` followed by markdown whitespace as text in markdown
#[case("1 < 3", &[r#"T"1 < 3""#])]
// From `micromark-extension-mdx-jsx`.
// should support attributes
#[case("a <b c     d=\"d\"\t\tefg='e'>c</b>.", &[r#"T"a ""#, r#"text <b c d=lit:"d" efg=lit:"e">("#, r#"T"c""#, ")", r#"T".""#])]
#[case("> <a b={`\n>\t`}/>", &["blockquote(", "flow <a b=expr:{`\n`}>(", ")", ")"])]
#[case("> <a b={`\n> alpha\n>  bravo\n>   charlie\n>    delta\n> `}/>", &["blockquote(", "flow <a b=expr:{`\nalpha\nbravo\ncharlie\n delta\n`}>(", ")", ")"])]
// From `micromark-extension-mdx-expression`.
// should not support JSX by default
#[case("a {<b />} c", &[r#"T"a ""#, "textexpr {<b />}", r#"T" c""#])]
// should support `acornOptions` (1)
#[case("a {(() => {})()} c", &[r#"T"a ""#, "textexpr {(() => {})()}", r#"T" c""#])]
// should support `acornOptions` (2)
#[case("a {(function () {})()} c", &[r#"T"a ""#, "textexpr {(function () {})()}", r#"T" c""#])]
#[case("a {} c", &[r#"T"a ""#, "textexpr {}", r#"T" c""#])]
// should support `\0` and `\r` in expressions
// Differs from the official packages: A null character is kept in values, as in markdown-rs, and replaced with U+FFFD only in HTML; the JavaScript packages replace it in values too.
#[case("{`a\0b\rc\nd\r\ne`}", &["flowexpr {`a\0b\rc\nd\r\ne`}"])]
// should support a spread
// Differs from the official packages: Expressions and ESM are not parsed as JavaScript, so invalid JavaScript is accepted.
#[case("a {...b} c", &[r#"T"a ""#, "textexpr {...b}", r#"T" c""#])]
// should crash on an incorrect spread
// Differs from the official packages: Expressions and ESM are not parsed as JavaScript, so invalid JavaScript is accepted.
#[case("a {...?} c", &[r#"T"a ""#, "textexpr {...?}", r#"T" c""#])]
#[case("a {b=c}={} d", &[r#"T"a ""#, "textexpr {b=c}", r#"T"=""#, "textexpr {}", r#"T" d""#])]
// should crash if a spread and other things
// Differs from the official packages: Expressions and ESM are not parsed as JavaScript, so invalid JavaScript is accepted.
#[case("a {...b,c} d", &[r#"T"a ""#, "textexpr {...b,c}", r#"T" d""#])]
// should crash if not an identifier
#[case("a {b=c} d", &[r#"T"a ""#, "textexpr {b=c}", r#"T" d""#])]
#[case("a {/* b */} c", &[r#"T"a ""#, "textexpr {/* b */}", r#"T" c""#])]
#[case("{a=b}", &["flowexpr {a=b}"])]
// From `micromark-extension-mdxjs-esm`.
// should throw if `acorn` is not passed in
#[case("import a from \"b\"\n\nc", &[r#"esm "import a from \"b\"""#, r#"T"c""#])]
#[case(r#"import("a")"#, &[r#"T"import(\"a\")""#])]
// should not support an indent
#[case("  import a from \"b\"\n  export default c", &[r#"T"import a from \"b\"\nexport default c""#])]
// should not support keywords in containers
#[case("- import a from \"b\"\n> export default c", &["item 0 u - t(", r#"T"import a from \"b\"""#, ")", "blockquote(", r#"T"export default c""#, ")"])]
#[case("import a from \"b\"\nexport default c", &[r#"esm "import a from \"b\"\nexport default c""#])]
#[case("import a from \"b\"\n\nexport default c", &[r#"esm "import a from \"b\"""#, r#"esm "export default c""#])]
#[case("a\n\nimport a from \"b\"\n\nb\n\nexport default c", &[r#"T"a""#, r#"esm "import a from \"b\"""#, r#"T"b""#, r#"esm "export default c""#])]
#[case("a\nimport a from \"b\"\n\nb\nexport default c", &[r#"T"a\nimport a from \"b\"""#, r#"T"b\nexport default c""#])]
#[case("export {\n  a\n} from \"b\"\n\nc", &[r#"esm "export {\n  a\n} from \"b\"""#, r#"T"c""#])]
#[case("export {\n\n  a\n\n} from \"b\"\n\nc", &[r#"esm "export {\n\n  a\n\n} from \"b\"""#, r#"T"c""#])]
// Differs from the official packages: Expressions and ESM are not parsed as JavaScript, so invalid JavaScript is accepted.
#[case("import a from \"b\"\n*md*?", &[r#"esm "import a from \"b\"\n*md*?""#])]
// Differs from the official packages: Expressions and ESM are not parsed as JavaScript, so invalid JavaScript is accepted.
#[case("import (\"a\")\n\nb", &[r#"esm "import (\"a\")""#, r#"T"b""#])]
#[case("import a from \"b\"\nexport {a}\n\nc", &[r#"esm "import a from \"b\"\nexport {a}""#, r#"T"c""#])]
#[case("import a from \"b\";\nexport {a};\n\nc", &[r#"esm "import a from \"b\";\nexport {a};""#, r#"T"c""#])]
#[case("import a from \"b\"\nexport {a as default}\n\nc", &[r#"esm "import a from \"b\"\nexport {a as default}""#, r#"T"c""#])]
#[case("export var a = () => <b />\n\nc", &[r#"esm "export var a = () => <b />""#, r#"T"c""#])]
// should support `acornOptions` (1)
#[case("export var a = () => {}\n\nb", &[r#"esm "export var a = () => {}""#, r#"T"b""#])]
#[case("import a from \"b\"\n\nexport {a}\n\nc", &[r#"esm "import a from \"b\"""#, r#"esm "export {a}""#, r#"T"c""#])]
#[case("import a from \"b\"\n\nexport {a}\n\n# c", &[r#"esm "import a from \"b\"""#, r#"esm "export {a}""#, "heading 1(", r#"T"c""#, ")"])]
fn mdx(#[case] input: &str, #[case] expected: &[&str]) {
    check(input, expected, Markdown::from_mdx_str);
}

#[rstest]
// Hand-written.
#[case("<a>b")]
#[case("<a></b>")]
#[case("a <3")]
#[case("<a b=>")]
#[case("a {")]
#[case("{")]
#[case(r#"<a b=""#)]
#[case("<a b={")]
#[case("</a>")]
#[case("<a></a></a>")]
// should crash when building the ast on mismatched interleaving (4)
#[case("<a>")]
#[case("<a><b></a></b>")]
#[case("a <b>c")]
#[case("<a>b\n</a>")]
#[case("<a>\nb</a>")]
#[case("<a:b.c />")]
#[case("<a.b:c />")]
#[case("<1 />")]
#[case("<a b:c:d />")]
#[case("<a ={b} />")]
#[case("<a b=c />")]
#[case("</>")]
#[case("<></a>")]
#[case("<!-- a -->")]
#[case("<a>\n<!-- b -->\n</a>")]
#[case("<https://a.b>")]
#[case("<a@b.c>")]
#[case(r#"<a b="c"#)]
#[case(r#"x <a b="c"#)]
#[case("<a>\n<b")]
#[case("<a>\n\n<b")]
#[case("<a>x</a")]
#[case("<a>x</")]
#[case("<a>x<")]
#[case("a {b")]
#[case("a {b\n\nc}")]
#[case("x {a}\n\ny {")]
// From markdown-rs, `tests/mdx_expression_flow.rs`.
// should crash if no closing brace is found (1)
#[case("{a")]
// should crash if no closing brace is found (2)
#[case("{b { c }")]
// should not support laziness (1)
#[case("> {a\nb}")]
// should not support laziness (4)
#[case("> {\n> a\nb}")]
// should crash on an incorrect spread that looks like an assignment
#[case("<a {b=c}={} d>")]
// should crash if a spread and other things
#[case("<a {...b,c} d>")]
// From markdown-rs, `tests/mdx_expression_text.rs`.
// should crash on an incorrect line comment (1)
#[case("a {//} b")]
// should crash on an incorrect line comment (2)
#[case("a { // b } c")]
// should crash if no closing brace is found (1)
#[case("a {b c")]
// should crash if no closing brace is found (2)
#[case("a {b { c } d")]
// From markdown-rs, `tests/mdx_jsx_flow.rs`.
// should handle crash in containers gracefully
#[case("* <!a>\n1. b")]
// should not support lazy flow (1)
#[case("> <X\n/>")]
// should not support lazy flow (2)
#[case("> a\n> <X\n/>")]
// should not support lazy flow (3)
#[case("> <a b='\nc'/>")]
// should not support lazy flow (4)
#[case("> <a b='c\n'/>")]
// should not support lazy flow (5)
#[case("> <a b='c\nd'/>")]
// should not support lazy flow (6)
#[case("> <a b={c\nd}/>")]
// should not support lazy flow (7)
#[case("> <a {b\nc}/>")]
// From markdown-rs, `tests/mdx_jsx_text.rs`.
// should support mdx jsx (text) if enabled
#[case("a <b> c")]
// should crash when building the ast on a closing tag if none is open
#[case("a </b> c")]
// should crash when building the ast on a closing tag with a self-closing slash
#[case("a <b> c </b/> d")]
// should crash when building the ast on a closing tag with an attribute
#[case("a <b> c </b d> e")]
// should crash when building the ast on mismatched tags (1)
#[case("a <>b</c> d")]
// should crash when building the ast on mismatched tags (2)
#[case("a <b>c</> d")]
// should crash when building the ast on mismatched interleaving (1)
#[case("*a <b>c* d</b>.")]
// should crash when building the ast on mismatched interleaving (2)
#[case("<a>b *c</a> d*.")]
// should crash when building the ast on mismatched interleaving (3)
#[case("a <b>.")]
// should crash on unclosed jsx after closed jsx
#[case("<a><b></b>")]
// should crash on invalid JS in an attribute expression (2)
#[case("a <b{c=d}={}/> f")]
// should support an unclosed fragment
#[case("a <> c")]
// should *not* support whitespace in the opening tag (fragment)
#[case("a < \t>b</>")]
// should *not* support whitespace in the opening tag (named)
#[case("a < \nb\t>b</b>")]
// should crash on a nonconforming start identifier
#[case("a <!> b")]
// should crash on a nonconforming start identifier in a closing tag
#[case("a </(> b.")]
// should crash on non-conforming non-ascii identifier start characters
#[case("a <© /> b.")]
// should crash nicely on what might be a comment
#[case("a <!--b-->")]
// should crash nicely on JS line comments inside tags (1)
#[case("a <// b\nc/>")]
// should crash nicely JS line comments inside tags (2)
#[case("a <b// c\nd/>")]
// should crash nicely JS multiline comments inside tags (1)
#[case("a </*b*/c>")]
// should crash nicely JS multiline comments inside tags (2)
#[case("a <b/*c*/>")]
// should crash on non-conforming non-ascii identifier continuation characters
#[case("a <a¬ /> b.")]
// should crash nicely on what might be an email link
#[case("a <b@c.d>")]
// should crash on nonconforming identifier continuation characters
#[case("a <a?> c.")]
// should crash nicely on what might be an email link in member names
#[case("a <b.c@d.e>")]
// should crash on a nonconforming character to start a local name
#[case("a <a:+> c.")]
// should crash nicely on what might be a protocol in local names
#[case("a <http://example.com>")]
// should crash nicely on what might be a protocol in local names
#[case("a <http: >")]
// should crash on a nonconforming character in a local name
#[case("a <a:b|> c.")]
// should crash on a nonconforming character to start a member name
#[case("a <a..> c.")]
// should crash on a nonconforming character in a member name
#[case("a <a.b,> c.")]
// should crash on a nonconforming character after a local name
#[case("a <a:b .> c.")]
// should crash on a nonconforming character after a member name
#[case("a <a.b :> c.")]
// should crash on a nonconforming character after name
#[case("a <a => c.")]
// should crash on a nonconforming character before an attribute name
#[case("a <b {...p}~>c</b>.")]
// should crash on a missing closing brace in attribute expression
#[case("a <b {...")]
// should crash on a nonconforming character in attribute name
#[case("a <a b@> c.")]
// should crash on a nonconforming character after an attribute name
#[case("a <a b 1> c.")]
// should crash on a nonconforming character to start a local attribute name
#[case("a <a b:#> c.")]
// should crash on a nonconforming character in a local attribute name
#[case("a <a b:c%> c.")]
// should crash on a nonconforming character after a local attribute name
#[case("a <a b:c ^> c.")]
// should crash on a nonconforming character before an attribute value
#[case("a <a b=``> c.")]
// should crash nicely on what might be a fragment, element as prop value
#[case("a <a b=<c />> d.")]
// should crash on a missing closing quote in double quoted attribute value
#[case(r#"a <a b="> c."#)]
// should crash on a missing closing brace in an attribute value expression
#[case("a <a b={> c.")]
// should crash on a nonconforming character after an attribute value
#[case(r#"a <a b=""*> c."#)]
// should crash on a nonconforming character after a self-closing slash
#[case("a <a/b> c.")]
// should support a closing tag w/o open elements
#[case("a </> c.")]
// should support mismatched tags (1)
#[case("a <></b>")]
// should support mismatched tags (2)
#[case("a <b></>")]
// should support mismatched tags (3)
#[case("a <a.b></a>")]
// should support mismatched tags (4)
#[case("a <a></a.b>")]
// should support mismatched tags (5)
#[case("a <a.b></a.c>")]
// should support mismatched tags (6)
#[case("a <a:b></a>")]
// should support mismatched tags (7)
#[case("a <a></a:b>")]
// should support mismatched tags (8)
#[case("a <a:b></a:c>")]
// should support mismatched tags (9)
#[case("a <a:b></a.b>")]
// should support a closing self-closing tag
#[case("a <a>b</a/>")]
// should support a closing tag w/ attributes
#[case("a <a>b</a b>")]
// should not care about precedence between attention (emphasis)
#[case("a *open <b> close* </b> c.")]
// should not care about precedence between attention (strong)
#[case("a **open <b> close** </b> c.")]
// should not care about precedence between label (link)
#[case("a [open <b> close](c) </b> d.")]
// should not care about precedence between label (image)
#[case("a ![open <b> close](c) </b> d.")]
// From `micromark-extension-mdx-jsx`.
#[case("a <a></(> b.")]
#[case("a <a b='> c.")]
// should allow line endings in whitespace
#[case("a <b \n c> d.")]
fn invalid_mdx(#[case] input: &str) {
    assert!(Markdown::from_mdx_str(input).is_err(), "{input:?}");
}

#[rstest]
// Hand-written.
#[case("# a\n\n- x\n  - y\n- [ ] z\n\n1. q\n", &["heading 1(", r#"T"a""#, ")", "item 0 u - t(", r#"T"x""#, ")", "item 1 u - t(", r#"T"y""#, ")", "item 0 u _ t(", r#"T"z""#, ")", "item 0 o1 - t(", r#"T"q""#, ")"])]
#[case("| a | b |\n|:-|-:|\n| 1 | 2 |\n", &["cell 0 0(", r#"T"a""#, ")", "cell 0 1(", r#"T"b""#, ")", r#"align ["left","right"]"#, "cell 1 0(", r#"T"1""#, ")", "cell 1 1(", r#"T"2""#, ")"])]
#[case("> t *e* **s** ~~d~~ `c` [l](u \"t\") ![i](u) [r][d] <b> a  \nb\n\n[d]: /u\n", &["blockquote(", r#"T"t ""#, "em(", r#"T"e""#, ")", r#"T" ""#, "strong(", r#"T"s""#, ")", r#"T" ""#, "del(", r#"T"d""#, ")", r#"T" ""#, r#"code_inline "c""#, r#"T" ""#, "link u t(", r#"T"l""#, ")", r#"T" ""#, r#"image u  "i""#, r#"T" ""#, "linkref d(", r#"T"r""#, ")", r#"T" ""#, r#"html "<b>""#, r#"T" a""#, "br", r#"T"b""#, ")", "definition d /u "])]
#[case("[^1]\n\n[^1]: note\n", &["footnoteref 1", "footnote 1(", r#"T"note""#, ")"])]
#[case("---\ntitle: x\n---\n\n# a\n", &[r#"yaml "title: x""#, "heading 1(", r#"T"a""#, ")"])]
#[case("+++\ntitle = 1\n+++\n\na\n", &[r#"toml "title = 1""#, r#"T"a""#])]
#[case("$$\nm\n$$\n\n$i$\n", &[r#"math "m""#, r#"imath "i""#])]
#[case("```rs x\nc\n```\n", &[r#"code rs x "c""#])]
#[case("a\n\nb\n", &[r#"T"a""#, r#"T"b""#])]
#[case("*a*\n\n*b*\n", &["em(", r#"T"a""#, ")", "em(", r#"T"b""#, ")"])]
fn markdown(#[case] input: &str, #[case] expected: &[&str]) {
    check(input, expected, Markdown::from_markdown_str);
}
