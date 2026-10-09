//! What the parser makes of each input: the nodes with their positions and, for Markdown, the HTML.
//!
//! Nodes are written one per line, children indented under their parent, as the kind, the fields that
//! are not empty or default, and `@line:column-line:column`.

use mq_markdown::{
    HorizontalRuleMarker, Markdown, MdxAttributeContent, MdxAttributeValue, Node, Position, TableAlignKind,
};
use rstest::rstest;

fn position(position: &Option<Position>) -> String {
    position.as_ref().map_or(String::new(), |position| {
        format!(
            " @{}:{}-{}:{}",
            position.start.line, position.start.column, position.end.line, position.end.column
        )
    })
}

fn field<T: std::fmt::Debug>(name: &str, value: &Option<T>) -> String {
    value
        .as_ref()
        .map_or(String::new(), |value| format!(" {name}={value:?}"))
}

fn flag(name: &str, set: bool) -> String {
    if set { format!(" {name}") } else { String::new() }
}

fn attributes(attributes: &[MdxAttributeContent]) -> String {
    attributes
        .iter()
        .map(|attribute| match attribute {
            MdxAttributeContent::Expression(value) => format!(" {{{value}}}"),
            MdxAttributeContent::Property(property) => match &property.value {
                None => format!(" {}", property.name),
                Some(MdxAttributeValue::Literal(value)) => format!(" {}={value:?}", property.name),
                Some(MdxAttributeValue::Expression(value)) => format!(" {}={{{value}}}", property.name),
            },
        })
        .collect()
}

/// The line of `node` and the children under it.
fn node_line(node: &Node) -> (String, &[Node]) {
    match node {
        Node::Text(text) => (format!("text {:?}{}", text.value, position(&text.position)), &[]),
        Node::Heading(heading) => (
            format!("heading {}{}", heading.depth, position(&heading.position)),
            &heading.values,
        ),
        Node::Blockquote(quote) => (format!("blockquote{}", position(&quote.position)), &quote.values),
        Node::List(list) => {
            let marker = list.marker.map(|marker| marker.as_char().to_string());
            (
                format!(
                    "list level={} index={}{}{}{}{}{}{}",
                    list.level,
                    list.index,
                    flag("ordered", list.ordered),
                    field("start", &list.start),
                    field("checked", &list.checked),
                    flag("spread", list.spread),
                    marker.map_or(String::new(), |marker| format!(" marker={marker}")),
                    position(&list.position)
                ),
                &list.values,
            )
        }
        Node::Code(code) => (
            format!(
                "code {:?}{}{}{}{}",
                code.value,
                field("lang", &code.lang),
                field("meta", &code.meta),
                flag("fence", code.fence),
                position(&code.position)
            ),
            &[],
        ),
        Node::Html(html) => (format!("html {:?}{}", html.value, position(&html.position)), &[]),
        Node::Yaml(yaml) => (format!("yaml {:?}{}", yaml.value, position(&yaml.position)), &[]),
        Node::Toml(toml) => (format!("toml {:?}{}", toml.value, position(&toml.position)), &[]),
        Node::Math(math) => (format!("math {:?}{}", math.value, position(&math.position)), &[]),
        Node::MathInline(math) => (format!("math_inline {:?}{}", math.value, position(&math.position)), &[]),
        Node::CodeInline(code) => (format!("code_inline {:?}{}", code.value, position(&code.position)), &[]),
        Node::Break(node) => (format!("break{}", position(&node.position)), &[]),
        Node::HorizontalRule(rule) => {
            let marker = rule.marker.map(HorizontalRuleMarker::as_char);
            (
                format!("hr{}{}", field("marker", &marker), position(&rule.position)),
                &[],
            )
        }
        Node::Emphasis(node) => (format!("emphasis{}", position(&node.position)), &node.values),
        Node::Strong(node) => (format!("strong{}", position(&node.position)), &node.values),
        Node::Delete(node) => (format!("delete{}", position(&node.position)), &node.values),
        Node::Link(link) => (
            format!(
                "link {:?}{}{}",
                link.url.as_str(),
                field("title", &link.title.as_ref().map(|title| title.to_value())),
                position(&link.position)
            ),
            &link.values,
        ),
        Node::Image(image) => (
            format!(
                "image {:?} alt={:?}{}{}",
                image.url,
                image.alt,
                field("title", &image.title),
                position(&image.position)
            ),
            &[],
        ),
        Node::Definition(definition) => (
            format!(
                "definition {:?} {:?}{}{}{}",
                definition.ident,
                definition.url.as_str(),
                field("label", &definition.label),
                field("title", &definition.title.as_ref().map(|title| title.to_value())),
                position(&definition.position)
            ),
            &[],
        ),
        Node::LinkRef(reference) => (
            format!(
                "link_ref {:?}{}{}",
                reference.ident,
                field("label", &reference.label),
                position(&reference.position)
            ),
            &reference.values,
        ),
        Node::ImageRef(reference) => (
            format!(
                "image_ref {:?} alt={:?}{}{}",
                reference.ident,
                reference.alt,
                field("label", &reference.label),
                position(&reference.position)
            ),
            &[],
        ),
        Node::Footnote(footnote) => (
            format!("footnote {:?}{}", footnote.ident, position(&footnote.position)),
            &footnote.values,
        ),
        Node::FootnoteRef(reference) => (
            format!(
                "footnote_ref {:?}{}{}",
                reference.ident,
                field("label", &reference.label),
                position(&reference.position)
            ),
            &[],
        ),
        Node::TableRow(row) => (format!("row{}", position(&row.position)), &row.values),
        Node::TableCell(cell) => (
            format!("cell {}:{}{}", cell.row, cell.column, position(&cell.position)),
            &cell.values,
        ),
        Node::TableAlign(align) => {
            let kinds = align
                .align
                .iter()
                .map(|kind| match kind {
                    TableAlignKind::Left => "left",
                    TableAlignKind::Right => "right",
                    TableAlignKind::Center => "center",
                    TableAlignKind::None => "none",
                })
                .collect::<Vec<_>>();
            (format!("align {}{}", kinds.join(","), position(&align.position)), &[])
        }
        Node::MdxJsxFlowElement(element) => (
            format!(
                "jsx_flow <{}{}>{}",
                element.name.as_deref().unwrap_or_default(),
                attributes(&element.attributes),
                position(&element.position)
            ),
            &element.children,
        ),
        Node::MdxJsxTextElement(element) => (
            format!(
                "jsx_text <{}{}>{}",
                element.name.as_deref().unwrap_or_default(),
                attributes(&element.attributes),
                position(&element.position)
            ),
            &element.children,
        ),
        Node::MdxFlowExpression(expression) => (
            format!(
                "expression_flow {:?}{}",
                expression.value,
                position(&expression.position)
            ),
            &[],
        ),
        Node::MdxTextExpression(expression) => (
            format!(
                "expression_text {:?}{}",
                expression.value,
                position(&expression.position)
            ),
            &[],
        ),
        Node::MdxJsEsm(esm) => (format!("esm {:?}{}", esm.value, position(&esm.position)), &[]),
        Node::Fragment(fragment) => ("fragment".to_string(), &fragment.values),
        Node::Empty => ("empty".to_string(), &[]),
        #[allow(unreachable_patterns)]
        node => panic!("unexpected node {}", node.name()),
    }
}

fn write_tree(nodes: &[Node], depth: usize, out: &mut Vec<String>) {
    for node in nodes {
        let (line, children) = node_line(node);
        out.push(format!("{}{line}", "  ".repeat(depth)));
        write_tree(children, depth + 1, out);
    }
}

fn tree(nodes: &[Node]) -> String {
    let mut out = Vec::new();
    write_tree(nodes, 0, &mut out);
    out.join("\n")
}

#[rstest]
#[case::empty("", "", "")]
#[case::blank_lines("\n\n  \n", "", "")]
#[case::paragraph("hello\n", "<p>hello</p>\n", r#"text "hello" @1:1-1:6"#)]
#[case::paragraph_no_eol("hello", "<p>hello</p>", r#"text "hello" @1:1-1:6"#)]
#[case::paragraph_multiline("a\nb\nc\n", "<p>a\nb\nc</p>\n", r#"text "a\nb\nc" @1:1-3:2"#)]
#[case::paragraph_indented_continuation("a\n    b\n", "<p>a\nb</p>\n", r#"text "a\nb" @1:1-2:6"#)]
#[case::paragraph_trailing_space("あい\n  うえ  \n", "<p>あい\nうえ</p>\n", r#"text "あい\nうえ" @1:1-2:9"#)]
#[case::two_paragraphs(
    "a\n\nb\n",
    "<p>a</p>\n<p>b</p>\n",
    r#"
text "a" @1:1-1:2
text "b" @3:1-3:2
"#
)]
#[case::crlf(
    "a\r\nb\r\n\r\nc\r\n",
    "<p>a\r\nb</p>\r\n<p>c</p>\r\n",
    r#"
text "a\r\nb" @1:1-2:2
text "c" @4:1-4:2
"#
)]
#[case::crlf_fence(
    "```\r\na\r\nb\r\n```\r\n",
    "<pre><code>a\r\nb\r\n</code></pre>\r\n",
    r#"code "a\r\nb" fence @1:1-4:4"#
)]
#[case::cr_only("a\rb\r", "<p>a\rb</p>\r", r#"text "a\rb" @1:1-2:2"#)]
#[case::atx_h1(
    "# title\n",
    "<h1>title</h1>\n",
    r#"
heading 1 @1:1-1:8
  text "title" @1:3-1:8
"#
)]
#[case::atx_h6(
    "###### title\n",
    "<h6>title</h6>\n",
    r#"
heading 6 @1:1-1:13
  text "title" @1:8-1:13
"#
)]
#[case::atx_seven(
    "####### title\n",
    "<p>####### title</p>\n",
    r########"text "####### title" @1:1-1:14"########
)]
#[case::atx_no_space("#title\n", "<p>#title</p>\n", r##"text "#title" @1:1-1:7"##)]
#[case::atx_empty("#\n", "<h1></h1>\n", "heading 1 @1:1-1:2")]
#[case::atx_closing(
    "## title ##\n",
    "<h2>title</h2>\n",
    r#"
heading 2 @1:1-1:12
  text "title" @1:4-1:9
"#
)]
#[case::atx_closing_no_space(
    "## title##\n",
    "<h2>title##</h2>\n",
    r#"
heading 2 @1:1-1:11
  text "title##" @1:4-1:11
"#
)]
#[case::atx_indent(
    "  ## title ##  \n",
    "<h2>title</h2>\n",
    r#"
heading 2 @1:1-1:16
  text "title" @1:6-1:11
"#
)]
#[case::atx_multibyte(
    "# あ h #\n",
    "<h1>あ h</h1>\n",
    r#"
heading 1 @1:1-1:10
  text "あ h" @1:3-1:8
"#
)]
#[case::atx_interrupts_paragraph(
    "a\n# b\n",
    "<p>a</p>\n<h1>b</h1>\n",
    r#"
text "a" @1:1-1:2
heading 1 @2:1-2:4
  text "b" @2:3-2:4
"#
)]
#[case::setext_h1(
    "Title\n===\n",
    "<h1>Title</h1>\n",
    r#"
heading 1 @1:1-2:4
  text "Title" @1:1-1:6
"#
)]
#[case::setext_h2(
    "a\n  ---\n",
    "<h2>a</h2>\n",
    r#"
heading 2 @1:1-2:6
  text "a" @1:1-1:2
"#
)]
#[case::setext_multiline(
    "a\nb\n---\n",
    "<h2>a\nb</h2>\n",
    r#"
heading 2 @1:1-3:4
  text "a\nb" @1:1-2:2
"#
)]
#[case::thematic_star("***\n", "<hr />\n", "hr marker='*' @1:1-1:4")]
#[case::thematic_dash_spaced("- - -\n", "<hr />\n", "hr marker='-' @1:1-1:6")]
#[case::thematic_underscore("  ___  \n", "<hr />\n", "hr marker='_' @1:1-1:8")]
#[case::thematic_two("**\n", "<p>**</p>\n", r#"text "**" @1:1-1:3"#)]
#[case::fence_lang_meta(
    "```rust title\nlet a;\n```\n",
    "<pre><code class=\"language-rust\">let a;\n</code></pre>\n",
    r#"code "let a;" lang="rust" meta="title" fence @1:1-3:4"#
)]
#[case::fence_indented(
    "  ```\n  a\n   b\n  ```\n",
    "<pre><code>a\n b\n</code></pre>\n",
    r#"code "a\n b" fence @1:3-4:6"#
)]
#[case::fence_tilde("~~~\na\n~~~\n", "<pre><code>a\n</code></pre>\n", r#"code "a" fence @1:1-3:4"#)]
#[case::fence_unclosed("```\na\n", "<pre><code>a\n</code></pre>\n", r#"code "a" fence @1:1-3:1"#)]
#[case::fence_empty(
    "para\n\n\n```\n```\n",
    "<p>para</p>\n<pre><code></code></pre>\n",
    r#"
text "para" @1:1-1:5
code "" fence @4:1-5:4
"#
)]
#[case::fence_longer_close("```\na\n`````\n", "<pre><code>a\n</code></pre>\n", r#"code "a" fence @1:1-3:6"#)]
#[case::fence_shorter_close(
    "````\na\n```\nb\n````\n",
    "<pre><code>a\n```\nb\n</code></pre>\n",
    r#"code "a\n```\nb" fence @1:1-5:5"#
)]
#[case::fence_blank_inside(
    "```\na\n\nb\n```\n",
    "<pre><code>a\n\nb\n</code></pre>\n",
    r#"code "a\n\nb" fence @1:1-5:4"#
)]
#[case::fence_backtick_info("``` a`b\nc\n", "<p>``` a`b\nc</p>\n", r#"text "``` a`b\nc" @1:1-2:2"#)]
#[case::fence_interrupts_paragraph(
    "a\n```\nb\n```\n",
    "<p>a</p>\n<pre><code>b\n</code></pre>\n",
    r#"
text "a" @1:1-1:2
code "b" fence @2:1-4:4
"#
)]
#[case::indented_code(
    "    code\n\n      more\n\nx\n",
    "<pre><code>code\n\n  more\n</code></pre>\n<p>x</p>\n",
    r#"
code "code\n\n  more" @1:1-3:11
text "x" @5:1-5:2
"#
)]
#[case::quote(
    "> a\n> b\n",
    "<blockquote>\n<p>a\nb</p>\n</blockquote>\n",
    r#"
blockquote @1:1-2:4
  text "a\nb" @1:3-2:4
"#
)]
#[case::quote_indented_lazy(
    "  > a\nb\n",
    "<blockquote>\n<p>a\nb</p>\n</blockquote>\n",
    r#"
blockquote @1:1-2:2
  text "a\nb" @1:5-2:2
"#
)]
#[case::quote_heading(
    "> # h\n>\n> c\n",
    "<blockquote>\n<h1>h</h1>\n<p>c</p>\n</blockquote>\n",
    r#"
blockquote @1:1-3:4
  heading 1 @1:3-1:6
    text "h" @1:5-1:6
  text "c" @3:3-3:4
"#
)]
#[case::quote_no_space(
    ">a\n",
    "<blockquote>\n<p>a</p>\n</blockquote>\n",
    r#"
blockquote @1:1-1:3
  text "a" @1:2-1:3
"#
)]
#[case::quote_empty(">\n", "<blockquote>\n</blockquote>\n", "blockquote @1:1-1:2")]
#[case::quote_nested(
    "> > a\n> b\n",
    "<blockquote>\n<blockquote>\n<p>a\nb</p>\n</blockquote>\n</blockquote>\n",
    r#"
blockquote @1:1-2:4
  blockquote @1:3-2:4
    text "a\nb" @1:5-2:4
"#
)]
#[case::quote_lazy_nested(
    "> > a\nb\n",
    "<blockquote>\n<blockquote>\n<p>a\nb</p>\n</blockquote>\n</blockquote>\n",
    r#"
blockquote @1:1-2:2
  blockquote @1:3-2:2
    text "a\nb" @1:5-2:2
"#
)]
#[case::quote_blank_ends(
    "> a\n\n> b\n",
    "<blockquote>\n<p>a</p>\n</blockquote>\n<blockquote>\n<p>b</p>\n</blockquote>\n",
    r#"
blockquote @1:1-1:4
  text "a" @1:3-1:4
blockquote @3:1-3:4
  text "b" @3:3-3:4
"#
)]
#[case::quote_fence(
    "> ```\n> a\n> ```\n",
    "<blockquote>\n<pre><code>a\n</code></pre>\n</blockquote>\n",
    r#"
blockquote @1:1-3:6
  code "a" fence @1:3-3:6
"#
)]
#[case::quote_no_lazy_after_fence(
    "> ```\n> a\nb\n",
    "<blockquote>\n<pre><code>a\n</code></pre>\n</blockquote>\n<p>b</p>\n",
    r#"
blockquote @1:1-2:4
  code "a" fence @1:3-2:4
text "b" @3:1-3:2
"#
)]
#[case::quote_interrupts_paragraph(
    "a\n> b\n",
    "<p>a</p>\n<blockquote>\n<p>b</p>\n</blockquote>\n",
    r#"
text "a" @1:1-1:2
blockquote @2:1-2:4
  text "b" @2:3-2:4
"#
)]
#[case::quote_thematic_not_lazy(
    "> a\n---\n",
    "<blockquote>\n<p>a</p>\n</blockquote>\n<hr />\n",
    r#"
blockquote @1:1-1:4
  text "a" @1:3-1:4
hr marker='-' @2:1-2:4
"#
)]
#[case::quote_multibyte(
    "> あ\n> い\n",
    "<blockquote>\n<p>あ\nい</p>\n</blockquote>\n",
    r#"
blockquote @1:1-2:6
  text "あ\nい" @1:3-2:6
"#
)]
#[case::bullet(
    "- a\n- b\n",
    "<ul>\n<li>a</li>\n<li>b</li>\n</ul>\n",
    r#"
list level=0 index=0 marker=- @1:3-1:4
  text "a" @1:3-1:4
list level=0 index=1 marker=- @2:3-2:4
  text "b" @2:3-2:4
"#
)]
#[case::bullet_loose(
    "- a\n\n- b\n",
    "<ul>\n<li>\n<p>a</p>\n</li>\n<li>\n<p>b</p>\n</li>\n</ul>\n",
    r#"
list level=0 index=0 spread marker=- @1:3-1:4
  text "a" @1:3-1:4
list level=0 index=1 spread marker=- @3:3-3:4
  text "b" @3:3-3:4
"#
)]
#[case::bullet_indent(
    " - a\n",
    "<ul>\n<li>a</li>\n</ul>\n",
    r#"
list level=0 index=0 marker=- @1:4-1:5
  text "a" @1:4-1:5
"#
)]
#[case::bullet_mixed_markers(
    "* a\n+ b\n",
    "<ul>\n<li>a</li>\n</ul>\n<ul>\n<li>b</li>\n</ul>\n",
    r#"
list level=0 index=0 marker=* @1:3-1:4
  text "a" @1:3-1:4
list level=0 index=0 marker=+ @2:3-2:4
  text "b" @2:3-2:4
"#
)]
#[case::ordered(
    "1. a\n2. b\n",
    "<ol>\n<li>a</li>\n<li>b</li>\n</ol>\n",
    r#"
list level=0 index=0 ordered start=1 marker=. @1:4-1:5
  text "a" @1:4-1:5
list level=0 index=1 ordered start=1 marker=. @2:4-2:5
  text "b" @2:4-2:5
"#
)]
#[case::ordered_paren(
    "5) a\n",
    "<ol start=\"5\">\n<li>a</li>\n</ol>\n",
    r#"
list level=0 index=0 ordered start=5 marker=) @1:4-1:5
  text "a" @1:4-1:5
"#
)]
#[case::ordered_zero(
    "0. a\n",
    "<ol start=\"0\">\n<li>a</li>\n</ol>\n",
    r#"
list level=0 index=0 ordered start=0 marker=. @1:4-1:5
  text "a" @1:4-1:5
"#
)]
#[case::ordered_too_long("1234567890. a\n", "<p>1234567890. a</p>\n", r#"text "1234567890. a" @1:1-1:14"#)]
#[case::ordered_mixed_delimiters(
    "1. a\n2) b\n",
    "<ol>\n<li>a</li>\n</ol>\n<ol start=\"2\">\n<li>b</li>\n</ol>\n",
    r#"
list level=0 index=0 ordered start=1 marker=. @1:4-1:5
  text "a" @1:4-1:5
list level=0 index=0 ordered start=2 marker=) @2:4-2:5
  text "b" @2:4-2:5
"#
)]
#[case::task(
    "- [ ] x\n- [x] y\n- [X] z\n",
    "<ul>\n<li><input type=\"checkbox\" disabled=\"\" /> x</li>\n<li><input type=\"checkbox\" checked=\"\" disabled=\"\" /> y</li>\n<li><input type=\"checkbox\" checked=\"\" disabled=\"\" /> z</li>\n</ul>\n",
    r#"
list level=0 index=0 checked=false marker=- @1:7-1:8
  text "x" @1:7-1:8
list level=0 index=1 checked=true marker=- @2:7-2:8
  text "y" @2:7-2:8
list level=0 index=2 checked=true marker=- @3:7-3:8
  text "z" @3:7-3:8
"#
)]
#[case::task_empty(
    "- [ ]\n",
    "<ul>\n<li>[ ]</li>\n</ul>\n",
    r#"
list level=0 index=0 marker=- @1:3-1:6
  text "[ ]" @1:3-1:6
"#
)]
#[case::task_no_space(
    "- [ ]x\n",
    "<ul>\n<li>[ ]x</li>\n</ul>\n",
    r#"
list level=0 index=0 marker=- @1:3-1:7
  text "[ ]x" @1:3-1:7
"#
)]
#[case::nested(
    "- a\n  - b\n    c\n",
    "<ul>\n<li>a\n<ul>\n<li>b\nc</li>\n</ul>\n</li>\n</ul>\n",
    r#"
list level=0 index=0 marker=- @1:3-1:4
  text "a" @1:3-1:4
list level=1 index=0 marker=- @2:5-3:6
  text "b\nc" @2:5-3:6
"#
)]
#[case::nested_ordered(
    "1. a\n   1. b\n   2. c\n2. d\n",
    "<ol>\n<li>a\n<ol>\n<li>b</li>\n<li>c</li>\n</ol>\n</li>\n<li>d</li>\n</ol>\n",
    r#"
list level=0 index=0 ordered start=1 marker=. @1:4-1:5
  text "a" @1:4-1:5
list level=1 index=0 ordered start=1 marker=. @2:7-2:8
  text "b" @2:7-2:8
list level=1 index=1 ordered start=1 marker=. @3:7-3:8
  text "c" @3:7-3:8
list level=0 index=1 ordered start=1 marker=. @4:4-4:5
  text "d" @4:4-4:5
"#
)]
#[case::nested_dedent(
    "- a\n  - b\n- c\n",
    "<ul>\n<li>a\n<ul>\n<li>b</li>\n</ul>\n</li>\n<li>c</li>\n</ul>\n",
    r#"
list level=0 index=0 marker=- @1:3-1:4
  text "a" @1:3-1:4
list level=1 index=0 marker=- @2:5-2:6
  text "b" @2:5-2:6
list level=0 index=1 marker=- @3:3-3:4
  text "c" @3:3-3:4
"#
)]
#[case::list_item_two_paragraphs(
    "- a\n\n  b\n",
    "<ul>\n<li>\n<p>a</p>\n<p>b</p>\n</li>\n</ul>\n",
    r#"
list level=0 index=0 marker=- @1:3-3:4
  text "a" @1:3-1:4
  text "b" @3:3-3:4
"#
)]
#[case::two_paragraphs_then_item(
    "- a\n\n  b\n- c\n",
    "<ul>\n<li>\n<p>a</p>\n<p>b</p>\n</li>\n<li>\n<p>c</p>\n</li>\n</ul>\n",
    r#"
list level=0 index=0 marker=- @1:3-3:4
  text "a" @1:3-1:4
  text "b" @3:3-3:4
list level=0 index=1 marker=- @4:3-4:4
  text "c" @4:3-4:4
"#
)]
#[case::empty_item("-\n", "<ul>\n<li></li>\n</ul>\n", "list level=0 index=0 marker=- @1:1-1:2")]
#[case::empty_item_content(
    "-\n  foo\n",
    "<ul>\n<li>foo</li>\n</ul>\n",
    r#"
list level=0 index=0 marker=- @2:3-2:6
  text "foo" @2:3-2:6
"#
)]
#[case::empty_item_blank_then_text(
    "-\n\n  foo\n",
    "<ul>\n<li></li>\n</ul>\n<p>foo</p>\n",
    r#"
list level=0 index=0 marker=- @1:1-1:2
text "foo" @3:3-3:6
"#
)]
#[case::empty_items(
    "-\n-\n",
    "<ul>\n<li></li>\n<li></li>\n</ul>\n",
    r#"
list level=0 index=0 marker=- @1:1-1:2
list level=0 index=1 marker=- @2:1-2:2
"#
)]
#[case::wide_marker_gap(
    "-   a\n    b\n",
    "<ul>\n<li>a\nb</li>\n</ul>\n",
    r#"
list level=0 index=0 marker=- @1:5-2:6
  text "a\nb" @1:5-2:6
"#
)]
#[case::code_in_item(
    "-     a\n",
    "<ul>\n<li>\n<pre><code>a\n</code></pre>\n</li>\n</ul>\n",
    r#"
list level=0 index=0 marker=- @1:3-1:8
  code "a" @1:3-1:8
"#
)]
#[case::lazy(
    "- a\nb\n",
    "<ul>\n<li>a\nb</li>\n</ul>\n",
    r#"
list level=0 index=0 marker=- @1:3-2:2
  text "a\nb" @1:3-2:2
"#
)]
#[case::lazy_after_blank(
    "- a\n\nb\n",
    "<ul>\n<li>a</li>\n</ul>\n<p>b</p>\n",
    r#"
list level=0 index=0 marker=- @1:3-1:4
  text "a" @1:3-1:4
text "b" @3:1-3:2
"#
)]
#[case::list_interrupts_paragraph(
    "a\n- b\n",
    "<p>a</p>\n<ul>\n<li>b</li>\n</ul>\n",
    r#"
text "a" @1:1-1:2
list level=0 index=0 marker=- @2:3-2:4
  text "b" @2:3-2:4
"#
)]
#[case::ordered_two_no_interrupt("a\n2. b\n", "<p>a\n2. b</p>\n", r#"text "a\n2. b" @1:1-2:5"#)]
#[case::ordered_one_interrupts(
    "a\n1. b\n",
    "<p>a</p>\n<ol>\n<li>b</li>\n</ol>\n",
    r#"
text "a" @1:1-1:2
list level=0 index=0 ordered start=1 marker=. @2:4-2:5
  text "b" @2:4-2:5
"#
)]
#[case::empty_item_no_interrupt(
    "a\n-\n",
    "<h2>a</h2>\n",
    r#"
heading 2 @1:1-2:2
  text "a" @1:1-1:2
"#
)]
#[case::list_in_quote(
    "> - a\n> - b\n",
    "<blockquote>\n<ul>\n<li>a</li>\n<li>b</li>\n</ul>\n</blockquote>\n",
    r#"
blockquote @1:1-2:6
  list level=0 index=0 marker=- @1:5-1:6
    text "a" @1:5-1:6
  list level=0 index=1 marker=- @2:5-2:6
    text "b" @2:5-2:6
"#
)]
#[case::quote_in_list(
    "- > a\n  > b\n",
    "<ul>\n<li>\n<blockquote>\n<p>a\nb</p>\n</blockquote>\n</li>\n</ul>\n",
    r#"
list level=0 index=0 marker=- @1:3-2:6
  blockquote @1:3-2:6
    text "a\nb" @1:5-2:6
"#
)]
#[case::quote_in_list_level(
    "- a\n  > - b\n",
    "<ul>\n<li>a\n<blockquote>\n<ul>\n<li>b</li>\n</ul>\n</blockquote>\n</li>\n</ul>\n",
    r#"
list level=0 index=0 marker=- @1:3-2:8
  text "a" @1:3-1:4
  blockquote @2:3-2:8
    list level=0 index=0 marker=- @2:7-2:8
      text "b" @2:7-2:8
"#
)]
#[case::heading_in_item(
    "- # h\n  text\n",
    "<ul>\n<li>\n<h1>h</h1>\ntext</li>\n</ul>\n",
    r#"
list level=0 index=0 marker=- @1:3-2:7
  heading 1 @1:3-1:6
    text "h" @1:5-1:6
  text "text" @2:3-2:7
"#
)]
#[case::fence_in_item(
    "- ```\n  a\n  ```\n",
    "<ul>\n<li>\n<pre><code>a\n</code></pre>\n</li>\n</ul>\n",
    r#"
list level=0 index=0 marker=- @1:3-3:6
  code "a" fence @1:3-3:6
"#
)]
#[case::multibyte_item(
    "- あ\n- い\n",
    "<ul>\n<li>あ</li>\n<li>い</li>\n</ul>\n",
    r#"
list level=0 index=0 marker=- @1:3-1:6
  text "あ" @1:3-1:6
list level=0 index=1 marker=- @2:3-2:6
  text "い" @2:3-2:6
"#
)]
#[case::list_after_paragraph_blank(
    "a\n\n- b\n",
    "<p>a</p>\n<ul>\n<li>b</li>\n</ul>\n",
    r#"
text "a" @1:1-1:2
list level=0 index=0 marker=- @3:3-3:4
  text "b" @3:3-3:4
"#
)]
#[case::two_lists_split(
    "- a\n\n\n- b\n",
    "<ul>\n<li>\n<p>a</p>\n</li>\n<li>\n<p>b</p>\n</li>\n</ul>\n",
    r#"
list level=0 index=0 spread marker=- @1:3-1:4
  text "a" @1:3-1:4
list level=0 index=1 spread marker=- @4:3-4:4
  text "b" @4:3-4:4
"#
)]
#[case::deep_quote(
    ">>>>>> a\n",
    "<blockquote>\n<blockquote>\n<blockquote>\n<blockquote>\n<blockquote>\n<blockquote>\n<p>a</p>\n</blockquote>\n</blockquote>\n</blockquote>\n</blockquote>\n</blockquote>\n</blockquote>\n",
    r#"
blockquote @1:1-1:9
  blockquote @1:2-1:9
    blockquote @1:3-1:9
      blockquote @1:4-1:9
        blockquote @1:5-1:9
          blockquote @1:6-1:9
            text "a" @1:8-1:9
"#
)]
#[case::fence_unclosed_no_eol("```\na", "<pre><code>a\n</code></pre>\n", r#"code "a" fence @1:1-2:2"#)]
#[case::fence_unclosed_in_item(
    "- ```\n  a\n",
    "<ul>\n<li>\n<pre><code>a\n</code></pre>\n</li>\n</ul>",
    r#"
list level=0 index=0 marker=- @1:3-3:1
  code "a" fence @1:3-3:1
"#
)]
#[case::fence_unclosed_in_item_no_eol(
    "- ```\n  a",
    "<ul>\n<li>\n<pre><code>a\n</code></pre>\n</li>\n</ul>",
    r#"
list level=0 index=0 marker=- @1:3-2:4
  code "a" fence @1:3-2:4
"#
)]
#[case::fence_unclosed_in_quote_no_eol(
    "> ```\n> a",
    "<blockquote>\n<pre><code>a\n</code></pre>\n</blockquote>",
    r#"
blockquote @1:1-2:4
  code "a" fence @1:3-2:4
"#
)]
#[case::fence_unclosed_in_quote_eof(
    "> ```\n> a\n",
    "<blockquote>\n<pre><code>a\n</code></pre>\n</blockquote>",
    r#"
blockquote @1:1-3:1
  code "a" fence @1:3-3:1
"#
)]
#[case::fence_unclosed_trailing_blank("```\na\n\n", "<pre><code>a\n\n</code></pre>\n", r#"code "a\n" fence @1:1-4:1"#)]
#[case::fence_unclosed_item_then_para(
    "- ```\n  a\nb\n",
    "<ul>\n<li>\n<pre><code>a\n</code></pre>\n</li>\n</ul>\n<p>b</p>\n",
    r#"
list level=0 index=0 marker=- @1:3-2:4
  code "a" fence @1:3-2:4
text "b" @3:1-3:2
"#
)]
#[case::item_code_then_unindented_text(
    "- ===\n\n      a\n===",
    "<ul>\n<li>\n<p>===</p>\n<pre><code>a\n</code></pre>\n</li>\n</ul>\n<p>===</p>",
    r#"
list level=0 index=0 marker=- @1:3-3:8
  text "===" @1:3-1:6
  code "a" @3:3-3:8
text "===" @4:1-4:4
"#
)]
#[case::quote_setext_dashes(
    "> a\n> ---\n> ---\n> a",
    "<blockquote>\n<h2>a</h2>\n<hr />\n<p>a</p>\n</blockquote>",
    r#"
blockquote @1:1-4:4
  heading 2 @1:3-2:6
    text "a" @1:3-1:4
  hr marker='-' @3:3-3:6
  text "a" @4:3-4:4
"#
)]
#[case::table_basic(
    "| a | b |\n|---|:-:|\n| 1 | 2 |\n",
    "<table>\n<thead>\n<tr>\n<th>a</th>\n<th align=\"center\">b</th>\n</tr>\n</thead>\n<tbody>\n<tr>\n<td>1</td>\n<td align=\"center\">2</td>\n</tr>\n</tbody>\n</table>\n",
    r#"
cell 0:0 @1:1-1:5
  text "a" @1:3-1:4
cell 0:1 @1:5-1:10
  text "b" @1:7-1:8
align none,center @2:1-2:1
cell 1:0 @3:1-3:5
  text "1" @3:3-3:4
cell 1:1 @3:5-3:10
  text "2" @3:7-3:8
"#
)]
#[case::table_no_edge_pipes(
    "a|b\n-|-\n1|2|3\n",
    "<table>\n<thead>\n<tr>\n<th>a</th>\n<th>b</th>\n</tr>\n</thead>\n<tbody>\n<tr>\n<td>1</td>\n<td>2</td>\n</tr>\n</tbody>\n</table>\n",
    r#"
cell 0:0 @1:1-1:2
  text "a" @1:1-1:2
cell 0:1 @1:2-1:4
  text "b" @1:3-1:4
align none,none @2:1-2:1
cell 1:0 @3:1-3:2
  text "1" @3:1-3:2
cell 1:1 @3:2-3:4
  text "2" @3:3-3:4
cell 1:2 @3:4-3:6
  text "3" @3:5-3:6
"#
)]
#[case::table_short_row(
    "|a|b|\n|-|-|\n|1|\n",
    "<table>\n<thead>\n<tr>\n<th>a</th>\n<th>b</th>\n</tr>\n</thead>\n<tbody>\n<tr>\n<td>1</td>\n<td></td>\n</tr>\n</tbody>\n</table>\n",
    r#"
cell 0:0 @1:1-1:3
  text "a" @1:2-1:3
cell 0:1 @1:3-1:6
  text "b" @1:4-1:5
align none,none @2:1-2:1
cell 1:0 @3:1-3:4
  text "1" @3:2-3:3
"#
)]
#[case::table_escaped_pipe(
    "| a \\| b | c |\n|--|--|\n",
    "<table>\n<thead>\n<tr>\n<th>a | b</th>\n<th>c</th>\n</tr>\n</thead>\n</table>\n",
    r#"
cell 0:0 @1:1-1:10
  text "a | b" @1:3-1:9
cell 0:1 @1:10-1:15
  text "c" @1:12-1:13
align none,none @2:1-2:1
"#
)]
#[case::table_indented(
    "  | a |\n  |---|\n  | b |\n",
    "<table>\n<thead>\n<tr>\n<th>a</th>\n</tr>\n</thead>\n<tbody>\n<tr>\n<td>b</td>\n</tr>\n</tbody>\n</table>\n",
    r#"
cell 0:0 @1:1-1:8
  text "a" @1:5-1:6
align none @2:1-2:1
cell 1:0 @3:1-3:8
  text "b" @3:5-3:6
"#
)]
#[case::table_text_after(
    "| a |\n|---|\ntext after\n",
    "<table>\n<thead>\n<tr>\n<th>a</th>\n</tr>\n</thead>\n<tbody>\n<tr>\n<td>text after</td>\n</tr>\n</tbody>\n</table>\n",
    r#"
cell 0:0 @1:1-1:6
  text "a" @1:3-1:4
align none @2:1-2:1
cell 1:0 @3:1-3:11
  text "text after" @3:1-3:11
"#
)]
#[case::table_in_quote(
    "> | a |\n> |---|\n> | b |\n",
    "<blockquote>\n<table>\n<thead>\n<tr>\n<th>a</th>\n</tr>\n</thead>\n<tbody>\n<tr>\n<td>b</td>\n</tr>\n</tbody>\n</table>\n</blockquote>\n",
    r#"
blockquote @1:1-3:8
  cell 0:0 @1:3-1:8
    text "a" @1:5-1:6
  align none @2:1-2:1
  cell 1:0 @3:3-3:8
    text "b" @3:5-3:6
"#
)]
#[case::table_after_paragraph(
    "x\n| a |\n|---|\n",
    "<p>x</p>\n<table>\n<thead>\n<tr>\n<th>a</th>\n</tr>\n</thead>\n</table>\n",
    r#"
text "x" @1:1-1:2
cell 0:0 @2:1-2:6
  text "a" @2:3-2:4
align none @3:1-3:1
"#
)]
#[case::table_column_mismatch(
    "| a | b |\n|---|\n",
    "<p>| a | b |\n|---|</p>\n",
    r#"text "| a | b |\n|---|" @1:1-2:6"#
)]
#[case::table_bad_delimiter("|a|\n|:|\n", "<p>|a|\n|:|</p>\n", r#"text "|a|\n|:|" @1:1-2:4"#)]
#[case::table_then_blank_para(
    "|a|\n|-|\n\nnext\n",
    "<table>\n<thead>\n<tr>\n<th>a</th>\n</tr>\n</thead>\n</table>\n<p>next</p>\n",
    r#"
cell 0:0 @1:1-1:4
  text "a" @1:2-1:3
align none @2:1-2:1
text "next" @4:1-4:5
"#
)]
#[case::table_then_quote(
    "|a|\n|-|\n> q\n",
    "<table>\n<thead>\n<tr>\n<th>a</th>\n</tr>\n</thead>\n</table>\n<blockquote>\n<p>q</p>\n</blockquote>\n",
    r#"
cell 0:0 @1:1-1:4
  text "a" @1:2-1:3
align none @2:1-2:1
blockquote @3:1-3:4
  text "q" @3:3-3:4
"#
)]
#[case::table_then_heading(
    "|a|\n|-|\n# h\n",
    "<table>\n<thead>\n<tr>\n<th>a</th>\n</tr>\n</thead>\n</table>\n<h1>h</h1>\n",
    r#"
cell 0:0 @1:1-1:4
  text "a" @1:2-1:3
align none @2:1-2:1
heading 1 @3:1-3:4
  text "h" @3:3-3:4
"#
)]
#[case::table_then_fence(
    "|a|\n|-|\n```\nx\n```\n",
    "<table>\n<thead>\n<tr>\n<th>a</th>\n</tr>\n</thead>\n</table>\n<pre><code>x\n</code></pre>\n",
    r#"
cell 0:0 @1:1-1:4
  text "a" @1:2-1:3
align none @2:1-2:1
code "x" fence @3:1-5:4
"#
)]
#[case::table_then_list(
    "|a|\n|-|\n- x\n",
    "<table>\n<thead>\n<tr>\n<th>a</th>\n</tr>\n</thead>\n</table>\n<ul>\n<li>x</li>\n</ul>\n",
    r#"
cell 0:0 @1:1-1:4
  text "a" @1:2-1:3
align none @2:1-2:1
list level=0 index=0 marker=- @3:3-3:4
  text "x" @3:3-3:4
"#
)]
#[case::table_two(
    "|a|\n|-|\n\n|b|\n|-|\n",
    "<table>\n<thead>\n<tr>\n<th>a</th>\n</tr>\n</thead>\n</table>\n<table>\n<thead>\n<tr>\n<th>b</th>\n</tr>\n</thead>\n</table>\n",
    r#"
cell 0:0 @1:1-1:4
  text "a" @1:2-1:3
align none @2:1-2:1
cell 0:0 @4:1-4:4
  text "b" @4:2-4:3
align none @5:1-5:1
"#
)]
#[case::table_align_spaces(
    "|a|b|\n|:-|-:|\n|  x  |  y|\n",
    "<table>\n<thead>\n<tr>\n<th align=\"left\">a</th>\n<th align=\"right\">b</th>\n</tr>\n</thead>\n<tbody>\n<tr>\n<td align=\"left\">x</td>\n<td align=\"right\">y</td>\n</tr>\n</tbody>\n</table>\n",
    r#"
cell 0:0 @1:1-1:3
  text "a" @1:2-1:3
cell 0:1 @1:3-1:6
  text "b" @1:4-1:5
align left,right @2:1-2:1
cell 1:0 @3:1-3:7
  text "x" @3:4-3:5
cell 1:1 @3:7-3:12
  text "y" @3:10-3:11
"#
)]
#[case::table_indented_code_row(
    "|a|\n|-|\n|b|\n    |c|\n",
    "<table>\n<thead>\n<tr>\n<th>a</th>\n</tr>\n</thead>\n<tbody>\n<tr>\n<td>b</td>\n</tr>\n</tbody>\n</table>\n<pre><code>|c|\n</code></pre>\n",
    r#"
cell 0:0 @1:1-1:4
  text "a" @1:2-1:3
align none @2:1-2:1
cell 1:0 @3:1-3:4
  text "b" @3:2-3:3
code "|c|" @4:1-4:8
"#
)]
#[case::table_no_body(
    "| a |\n| - |\n",
    "<table>\n<thead>\n<tr>\n<th>a</th>\n</tr>\n</thead>\n</table>\n",
    r#"
cell 0:0 @1:1-1:6
  text "a" @1:3-1:4
align none @2:1-2:1
"#
)]
#[case::table_empty_cells(
    "||\n|-|\n",
    "<table>\n<thead>\n<tr>\n<th></th>\n</tr>\n</thead>\n</table>\n",
    r#"
cell 0:0 @1:1-1:3
align none @2:1-2:1
"#
)]
#[case::table_lone_pipe_row(
    "|a|\n|-|\n|\n",
    "<table>\n<thead>\n<tr>\n<th>a</th>\n</tr>\n</thead>\n<tbody>\n<tr>\n<td></td>\n</tr>\n</tbody>\n</table>\n",
    r#"
cell 0:0 @1:1-1:4
  text "a" @1:2-1:3
align none @2:1-2:1
cell 1:0 @3:1-3:2
"#
)]
#[case::table_no_trailing_pipe(
    "| a | b\n|-|-\n| c | d\n",
    "<table>\n<thead>\n<tr>\n<th>a</th>\n<th>b</th>\n</tr>\n</thead>\n<tbody>\n<tr>\n<td>c</td>\n<td>d</td>\n</tr>\n</tbody>\n</table>\n",
    r#"
cell 0:0 @1:1-1:5
  text "a" @1:3-1:4
cell 0:1 @1:5-1:8
  text "b" @1:7-1:8
align none,none @2:1-2:1
cell 1:0 @3:1-3:5
  text "c" @3:3-3:4
cell 1:1 @3:5-3:8
  text "d" @3:7-3:8
"#
)]
#[case::table_multibyte(
    "| あ | い |\n|---|---|\n| う | え |\n",
    "<table>\n<thead>\n<tr>\n<th>あ</th>\n<th>い</th>\n</tr>\n</thead>\n<tbody>\n<tr>\n<td>う</td>\n<td>え</td>\n</tr>\n</tbody>\n</table>\n",
    r#"
cell 0:0 @1:1-1:7
  text "あ" @1:3-1:6
cell 0:1 @1:7-1:14
  text "い" @1:9-1:12
align none,none @2:1-2:1
cell 1:0 @3:1-3:7
  text "う" @3:3-3:6
cell 1:1 @3:7-3:14
  text "え" @3:9-3:12
"#
)]
#[case::not_table_setext(
    "Title\n---\n",
    "<h2>Title</h2>\n",
    r#"
heading 2 @1:1-2:4
  text "Title" @1:1-1:6
"#
)]
#[case::not_table_no_pipe_delim(
    "a|b\n---\n",
    "<h2>a|b</h2>\n",
    r#"
heading 2 @1:1-2:4
  text "a|b" @1:1-1:4
"#
)]
#[case::table_header_no_pipe(
    "a\n|-|\n",
    "<table>\n<thead>\n<tr>\n<th>a</th>\n</tr>\n</thead>\n</table>\n",
    r#"
cell 0:0 @1:1-1:2
  text "a" @1:1-1:2
align none @2:1-2:1
"#
)]
#[case::table_one_col_dash(
    "|a|\n-\n",
    "<h2>|a|</h2>\n",
    r#"
heading 2 @1:1-2:2
  text "|a|" @1:1-1:4
"#
)]
#[case::inline_escape("a\\*b", "<p>a*b</p>", r#"text "a*b" @1:1-1:5"#)]
#[case::inline_escape_nonpunct("a\\qb", "<p>a\\qb</p>", r#"text "a\\qb" @1:1-1:5"#)]
#[case::inline_entities(
    "a&amp;b &copy; &#35; &#x41; &unknown; &",
    "<p>a&amp;b © # A &amp;unknown; &amp;</p>",
    r#"text "a&b © # A &unknown; &" @1:1-1:40"#
)]
#[case::inline_emphasis_all(
    "a *b* **c** _d_ __e__ ***f***",
    "<p>a <em>b</em> <strong>c</strong> <em>d</em> <strong>e</strong> <em><strong>f</strong></em></p>",
    r#"
text "a " @1:1-1:3
emphasis @1:3-1:6
  text "b" @1:4-1:5
text " " @1:6-1:7
strong @1:7-1:12
  text "c" @1:9-1:10
text " " @1:12-1:13
emphasis @1:13-1:16
  text "d" @1:14-1:15
text " " @1:16-1:17
strong @1:17-1:22
  text "e" @1:19-1:20
text " " @1:22-1:23
emphasis @1:23-1:30
  strong @1:24-1:29
    text "f" @1:26-1:27
"#
)]
#[case::inline_code(
    "`code` `` a`b `` ` a `",
    "<p><code>code</code> <code>a`b</code> <code>a</code></p>",
    r#"
code_inline "code" @1:1-1:7
text " " @1:7-1:8
code_inline "a`b" @1:8-1:17
text " " @1:17-1:18
code_inline "a" @1:18-1:23
"#
)]
#[case::inline_code_unclosed("`a ``b`", "<p><code>a ``b</code></p>", r#"code_inline "a ``b" @1:1-1:8"#)]
#[case::inline_hard_break_spaces(
    "a  \nb",
    "<p>a<br />\nb</p>",
    r#"
text "a" @1:1-1:2
break @1:2-2:1
text "b" @2:1-2:2
"#
)]
#[case::inline_hard_break_backslash(
    "a\\\nb",
    "<p>a<br />\nb</p>",
    r#"
text "a" @1:1-1:2
break @1:2-2:1
text "b" @2:1-2:2
"#
)]
#[case::inline_soft_break("a\nb", "<p>a\nb</p>", r#"text "a\nb" @1:1-2:2"#)]
#[case::inline_soft_break_trailing_space("a \nb", "<p>a\nb</p>", r#"text "a\nb" @1:1-2:2"#)]
#[case::inline_emphasis_multiline(
    "*a\nb*",
    "<p><em>a\nb</em></p>",
    r#"
emphasis @1:1-2:3
  text "a\nb" @1:2-2:2
"#
)]
#[case::inline_strike(
    "a ~b~ ~~c~~ ~~~d~~~",
    "<p>a <del>b</del> <del>c</del> ~~~d~~~</p>",
    r#"
text "a " @1:1-1:3
delete @1:3-1:6
  text "b" @1:4-1:5
text " " @1:6-1:7
delete @1:7-1:12
  text "c" @1:9-1:10
text " ~~~d~~~" @1:12-1:20
"#
)]
#[case::inline_math(
    "$x$ $$y$$ $ a",
    "<p><code class=\"language-math math-inline\">x</code> <code class=\"language-math math-inline\">y</code> $ a</p>",
    r#"
math_inline "x" @1:1-1:4
text " " @1:4-1:5
math_inline "y" @1:5-1:10
text " $ a" @1:10-1:14
"#
)]
#[case::inline_autolinks(
    "<http://a.b> <a@b.c> <span> <!-- c --> <br/>",
    "<p><a href=\"http://a.b\">http://a.b</a> <a href=\"mailto:a@b.c\">a@b.c</a> <span> <!-- c --> <br/></p>",
    r#"
link "http://a.b" @1:1-1:13
  text "http://a.b" @1:2-1:12
text " " @1:13-1:14
link "mailto:a@b.c" @1:14-1:21
  text "a@b.c" @1:15-1:20
text " " @1:21-1:22
html "<span>" @1:22-1:28
text " " @1:28-1:29
html "<!-- c -->" @1:29-1:39
text " " @1:39-1:40
html "<br/>" @1:40-1:45
"#
)]
#[case::inline_links(
    "[a](http://x \"t\") [b][c] [d][] [e] ![f](g) ![h][i]",
    "<p><a href=\"http://x\" title=\"t\">a</a> [b][c] [d][] [e] <img src=\"g\" alt=\"f\" /> ![h][i]</p>",
    r#"
link "http://x" title="t" @1:1-1:18
  text "a" @1:2-1:3
text " [b][c] [d][] [e] " @1:18-1:36
image "g" alt="f" @1:36-1:43
text " ![h][i]" @1:43-1:51
"#
)]
#[case::inline_link_forms(
    "[a](<b c> 'x') [d]( e ) [f](g (h)) [i](j \"k\\\"l\") [m](n&amp;o)",
    "<p><a href=\"b%20c\" title=\"x\">a</a> <a href=\"e\">d</a> <a href=\"g\" title=\"h\">f</a> <a href=\"j\" title=\"k&quot;l\">i</a> <a href=\"n&amp;o\">m</a></p>",
    r#"
link "b c" title="x" @1:1-1:15
  text "a" @1:2-1:3
text " " @1:15-1:16
link "e" @1:16-1:24
  text "d" @1:17-1:18
text " " @1:24-1:25
link "g" title="h" @1:25-1:35
  text "f" @1:26-1:27
text " " @1:35-1:36
link "j" title="k\"l" @1:36-1:49
  text "i" @1:37-1:38
text " " @1:49-1:50
link "n&o" @1:50-1:62
  text "m" @1:51-1:52
"#
)]
#[case::inline_link_multiline(
    "[a](b\n\"t\") [c](d",
    "<p><a href=\"b\" title=\"t\">a</a> [c](d</p>",
    r#"
link "b" title="t" @1:1-2:5
  text "a" @1:2-1:3
text " [c](d" @2:5-2:11
"#
)]
#[case::inline_image_alt(
    "![a *b* `c`](d) ![e ![f](g)](h)",
    "<p><img src=\"d\" alt=\"a b c\" /> <img src=\"h\" alt=\"e f\" /></p>",
    r#"
image "d" alt="a b c" @1:1-1:16
text " " @1:16-1:17
image "h" alt="e f" @1:17-1:32
"#
)]
#[case::inline_link_in_link(
    "[a [b](c) d](e)",
    "<p>[a <a href=\"c\">b</a> d](e)</p>",
    r#"
text "[a " @1:1-1:4
link "c" @1:4-1:10
  text "b" @1:5-1:6
text " d](e)" @1:10-1:16
"#
)]
#[case::inline_emphasis_link(
    "*[a](b)* **[c][a]**",
    "<p><em><a href=\"b\">a</a></em> <strong>[c][a]</strong></p>",
    r#"
emphasis @1:1-1:9
  link "b" @1:2-1:8
    text "a" @1:3-1:4
text " " @1:9-1:10
strong @1:10-1:20
  text "[c][a]" @1:12-1:18
"#
)]
#[case::inline_html(
    "<a href=\"x\">t</a> <a\nhref=x>",
    "<p><a href=\"x\">t</a> <a\nhref=x></p>",
    r#"
html "<a href=\"x\">" @1:1-1:13
text "t" @1:13-1:14
html "</a>" @1:14-1:18
text " " @1:18-1:19
html "<a\nhref=x>" @1:19-2:8
"#
)]
#[case::inline_html_misc(
    "<http://a b> <a+b@c> <ab> </a> <?x?> <![CDATA[x]]> <!X y>",
    "<p>&lt;<a href=\"http://a\">http://a</a> b&gt; <a href=\"mailto:a+b@c\">a+b@c</a> <ab> </a> <?x?> <![CDATA[x]]> <!X y></p>",
    r#"
text "<" @1:1-1:2
link "http://a" @1:2-1:10
  text "http://a" @1:2-1:10
text " b> " @1:10-1:14
link "mailto:a+b@c" @1:14-1:21
  text "a+b@c" @1:15-1:20
text " " @1:21-1:22
html "<ab>" @1:22-1:26
text " " @1:26-1:27
html "</a>" @1:27-1:31
text " " @1:31-1:32
html "<?x?>" @1:32-1:37
text " " @1:37-1:38
html "<![CDATA[x]]>" @1:38-1:51
text " " @1:51-1:52
html "<!X y>" @1:52-1:58
"#
)]
#[case::inline_nested_emphasis(
    "*a **b** c* **a *b* c**",
    "<p><em>a <strong>b</strong> c</em> <strong>a <em>b</em> c</strong></p>",
    r#"
emphasis @1:1-1:12
  text "a " @1:2-1:4
  strong @1:4-1:9
    text "b" @1:6-1:7
  text " c" @1:9-1:11
text " " @1:12-1:13
strong @1:13-1:24
  text "a " @1:15-1:17
  emphasis @1:17-1:20
    text "b" @1:18-1:19
  text " c" @1:20-1:22
"#
)]
#[case::inline_intraword(
    "a_b_c a*b*c _a_b",
    "<p>a_b_c a<em>b</em>c _a_b</p>",
    r#"
text "a_b_c a" @1:1-1:8
emphasis @1:8-1:11
  text "b" @1:9-1:10
text "c _a_b" @1:11-1:17
"#
)]
#[case::inline_unmatched(
    "*a **b _c ~d [e ![f",
    "<p>*a **b _c ~d [e ![f</p>",
    r#"text "*a **b _c ~d [e ![f" @1:1-1:20"#
)]
#[case::inline_rule_of_three(
    "*foo**bar**baz* ***a** b*",
    "<p><em>foo<strong>bar</strong>baz</em> <em><strong>a</strong> b</em></p>",
    r#"
emphasis @1:1-1:16
  text "foo" @1:2-1:5
  strong @1:5-1:12
    text "bar" @1:7-1:10
  text "baz" @1:12-1:15
text " " @1:16-1:17
emphasis @1:17-1:26
  strong @1:18-1:23
    text "a" @1:20-1:21
  text " b" @1:23-1:25
"#
)]
#[case::inline_heading(
    "# a *b* `c`\n",
    "<h1>a <em>b</em> <code>c</code></h1>\n",
    r#"
heading 1 @1:1-1:12
  text "a " @1:3-1:5
  emphasis @1:5-1:8
    text "b" @1:6-1:7
  text " " @1:8-1:9
  code_inline "c" @1:9-1:12
"#
)]
#[case::inline_setext_heading(
    "a *b*\nc\n===\n",
    "<h1>a <em>b</em>\nc</h1>\n",
    r#"
heading 1 @1:1-3:4
  text "a " @1:1-1:3
  emphasis @1:3-1:6
    text "b" @1:4-1:5
  text "\nc" @1:6-2:2
"#
)]
#[case::inline_list_item(
    "- a *b*\n  c **d**\n",
    "<ul>\n<li>a <em>b</em>\nc <strong>d</strong></li>\n</ul>\n",
    r#"
list level=0 index=0 marker=- @1:3-2:10
  text "a " @1:3-1:5
  emphasis @1:5-1:8
    text "b" @1:6-1:7
  text "\nc " @1:8-2:5
  strong @2:5-2:10
    text "d" @2:7-2:8
"#
)]
#[case::inline_table_cell(
    "| a *b* | `c\\|d` |\n|-|-|\n",
    "<table>\n<thead>\n<tr>\n<th>a <em>b</em></th>\n<th><code>c|d</code></th>\n</tr>\n</thead>\n</table>\n",
    r#"
cell 0:0 @1:1-1:9
  text "a " @1:3-1:5
  emphasis @1:5-1:8
    text "b" @1:6-1:7
cell 0:1 @1:9-1:19
  code_inline "c|d" @1:11-1:17
align none,none @2:1-2:1
"#
)]
#[case::inline_multibyte(
    "あ*い*う `え` [お](か)",
    "<p>あ<em>い</em>う <code>え</code> <a href=\"%E3%81%8B\">お</a></p>",
    r#"
text "あ" @1:1-1:4
emphasis @1:4-1:9
  text "い" @1:5-1:8
text "う " @1:9-1:13
code_inline "え" @1:13-1:18
text " " @1:18-1:19
link "か" @1:19-1:29
  text "お" @1:20-1:23
"#
)]
#[case::inline_crlf_break(
    "a  \r\nb\r\n",
    "<p>a<br />\r\nb</p>\r\n",
    r#"
text "a" @1:1-1:2
break @1:2-2:1
text "b" @2:1-2:2
"#
)]
#[case::math_multiline(
    "x$\nx$",
    "<p>x<code class=\"language-math math-inline\"> x</code></p>",
    r#"
text "x" @1:1-1:2
math_inline "\nx" @1:2-2:3
"#
)]
#[case::decl_email(
    "x<!a@b.co>",
    "<p>x<!a@b.co></p>",
    r#"
text "x" @1:1-1:2
html "<!a@b.co>" @1:2-1:11
"#
)]
#[case::code_multiline(
    "x`\nx`",
    "<p>x<code> x</code></p>",
    r#"
text "x" @1:1-1:2
code_inline "\nx" @1:2-2:3
"#
)]
#[case::def_basic("[a]: b", "", r#"definition "a" "b" label="a" @1:1-1:7"#)]
#[case::def_title("[a]: <b c> 'T'", "", r#"definition "a" "b c" label="a" title="T" @1:1-1:15"#)]
#[case::def_next_lines("[a]:\nb\n\"t\"", "", r#"definition "a" "b" label="a" title="t" @1:1-3:4"#)]
#[case::def_title_junk_next_line(
    "[a]: b\n\"t\" x",
    "<p>&quot;t&quot; x</p>",
    r#"
definition "a" "b" label="a" @1:1-1:7
text "\"t\" x" @2:1-2:6
"#
)]
#[case::def_title_junk_same_line(
    "[a]: b \"t\" x",
    "<p>[a]: b &quot;t&quot; x</p>",
    r#"text "[a]: b \"t\" x" @1:1-1:13"#
)]
#[case::def_then_paragraph(
    "[a]: b\nc",
    "<p>c</p>",
    r#"
definition "a" "b" label="a" @1:1-1:7
text "c" @2:1-2:2
"#
)]
#[case::def_two_then_text(
    "[a]: b\n[c]: d\ntext",
    "<p>text</p>",
    r#"
definition "a" "b" label="a" @1:1-1:7
definition "c" "d" label="c" @2:1-2:7
text "text" @3:1-3:5
"#
)]
#[case::def_not_at_start("text\n[a]: b", "<p>text\n[a]: b</p>", r#"text "text\n[a]: b" @1:1-2:7"#)]
#[case::def_then_setext_equals(
    "[a]: b\n=== ",
    "<p>===</p>",
    r#"
definition "a" "b" label="a" @1:1-1:7
text "===" @2:1-2:4
"#
)]
#[case::def_then_setext_dashes(
    "[a]: b\n---",
    "<hr />",
    r#"
definition "a" "b" label="a" @1:1-1:7
hr marker='-' @2:1-2:4
"#
)]
#[case::def_indent1(" [a]: b", "", r#"definition "a" "b" label="a" @1:1-1:8"#)]
#[case::def_indent3("   [a]: b", "", r#"definition "a" "b" label="a" @1:1-1:10"#)]
#[case::def_indent4("    [a]: b", "<pre><code>[a]: b\n</code></pre>", r#"code "[a]: b" @1:1-1:11"#)]
#[case::def_in_quote(
    "> [a]: b",
    "<blockquote>\n</blockquote>",
    r#"
blockquote @1:1-1:9
  definition "a" "b" label="a" @1:3-1:9
"#
)]
#[case::def_in_list(
    "- [a]: b",
    "<ul>\n<li></li>\n</ul>",
    r#"
list level=0 index=0 marker=- @1:3-1:9
  definition "a" "b" label="a" @1:3-1:9
"#
)]
#[case::def_no_dest("[a]:", "<p>[a]:</p>", r#"text "[a]:" @1:1-1:5"#)]
#[case::def_empty_label("[]: b", "<p>[]: b</p>", r#"text "[]: b" @1:1-1:6"#)]
#[case::def_blank_label("[ ]: b", "<p>[ ]: b</p>", r#"text "[ ]: b" @1:1-1:7"#)]
#[case::def_escaped_label("[a\\]b]: c", "", r#"definition "a\\]b" "c" label="a]b" @1:1-1:10"#)]
#[case::def_multiline_title("[a]: b \"t\nt\"", "", r#"definition "a" "b" label="a" title="t\nt" @1:1-2:3"#)]
#[case::def_title_indented("[a]: b\n  \"t\"", "", r#"definition "a" "b" label="a" title="t" @1:1-2:6"#)]
#[case::def_no_space("[a]:b", "", r#"definition "a" "b" label="a" @1:1-1:6"#)]
#[case::def_dest_junk("[a]: b c", "<p>[a]: b c</p>", r#"text "[a]: b c" @1:1-1:9"#)]
#[case::def_duplicate_labels(
    "[A  b]: c\n[a b]: d",
    "",
    r#"
definition "a b" "c" label="A  b" @1:1-1:10
definition "a b" "d" label="a b" @2:1-2:9
"#
)]
#[case::def_empty_angle("[a]: <>", "", r#"definition "a" "" label="a" @1:1-1:8"#)]
#[case::def_quote_in_dest("[a]: 'x", "", r#"definition "a" "'x" label="a" @1:1-1:8"#)]
#[case::def_use_shortcut(
    "[a]: /u\n\n[a] [a][] [b][a] [a][b] [A b]",
    "<p><a href=\"/u\">a</a> <a href=\"/u\">a</a> <a href=\"/u\">b</a> [a][b] [A b]</p>",
    r#"
definition "a" "/u" label="a" @1:1-1:8
link_ref "a" label="a" @3:1-3:4
  text "a" @3:2-3:3
text " " @3:4-3:5
link_ref "a" label="a" @3:5-3:10
  text "a" @3:6-3:7
text " " @3:10-3:11
link_ref "a" label="a" @3:11-3:17
  text "b" @3:12-3:13
text " [a][b] [A b]" @3:17-3:30
"#
)]
#[case::def_use_normalized(
    "[ab cd]: /u\n\n[AB   CD] [ab\ncd]",
    "<p><a href=\"/u\">AB   CD</a> <a href=\"/u\">ab\ncd</a></p>",
    r#"
definition "ab cd" "/u" label="ab cd" @1:1-1:12
link_ref "ab cd" label="AB   CD" @3:1-3:10
  text "AB   CD" @3:2-3:9
text " " @3:10-3:11
link_ref "ab cd" label="ab\ncd" @3:11-4:4
  text "ab\ncd" @3:12-4:3
"#
)]
#[case::def_use_before(
    "[a] and [b][a]\n\n[a]: /u \"T\"",
    "<p><a href=\"/u\" title=\"T\">a</a> and <a href=\"/u\" title=\"T\">b</a></p>\n",
    r#"
link_ref "a" label="a" @1:1-1:4
  text "a" @1:2-1:3
text " and " @1:4-1:9
link_ref "a" label="a" @1:9-1:15
  text "b" @1:10-1:11
definition "a" "/u" label="a" title="T" @3:1-3:12
"#
)]
#[case::def_use_in_link_text(
    "[a]: b\n\n[a](x) [a]",
    "<p><a href=\"x\">a</a> <a href=\"b\">a</a></p>",
    r#"
definition "a" "b" label="a" @1:1-1:7
link "x" @3:1-3:7
  text "a" @3:2-3:3
text " " @3:7-3:8
link_ref "a" label="a" @3:8-3:11
  text "a" @3:9-3:10
"#
)]
#[case::def_use_image(
    "[x]: /u\n\n![x] ![y][x] ![x][]",
    "<p><img src=\"/u\" alt=\"x\" /> <img src=\"/u\" alt=\"y\" /> <img src=\"/u\" alt=\"x\" /></p>",
    r#"
definition "x" "/u" label="x" @1:1-1:8
image_ref "x" alt="x" label="x" @3:1-3:5
text " " @3:5-3:6
image_ref "x" alt="y" label="x" @3:6-3:13
text " " @3:13-3:14
image_ref "x" alt="x" label="x" @3:14-3:20
"#
)]
#[case::def_link_in_emphasis(
    "[a]: b\n\n*[a]* **[c][a]**",
    "<p><em><a href=\"b\">a</a></em> <strong><a href=\"b\">c</a></strong></p>",
    r#"
definition "a" "b" label="a" @1:1-1:7
emphasis @3:1-3:6
  link_ref "a" label="a" @3:2-3:5
    text "a" @3:3-3:4
text " " @3:6-3:7
strong @3:7-3:17
  link_ref "a" label="a" @3:9-3:15
    text "c" @3:10-3:11
"#
)]
#[case::fn_basic(
    "[^a]: b",
    "",
    r#"
footnote "a" @1:1-1:8
  text "b" @1:7-1:8
"#
)]
#[case::fn_lazy(
    "[^a]: b\nc",
    "",
    r#"
footnote "a" @1:1-2:2
  text "b\nc" @1:7-2:2
"#
)]
#[case::fn_indented_continuation(
    "[^a]: b\n    c",
    "",
    r#"
footnote "a" @1:1-2:6
  text "b\nc" @1:7-2:6
"#
)]
#[case::fn_two_paragraphs(
    "[^a]: b\n\n    c",
    "",
    r#"
footnote "a" @1:1-3:6
  text "b" @1:7-1:8
  text "c" @3:5-3:6
"#
)]
#[case::fn_two_paragraphs_then_text(
    "[^a]: b\n\n    c\n\nd",
    "<p>d</p>",
    r#"
footnote "a" @1:1-4:1
  text "b" @1:7-1:8
  text "c" @3:5-3:6
text "d" @5:1-5:2
"#
)]
#[case::fn_two_space_continuation(
    "[^a]: b\n  c",
    "",
    r#"
footnote "a" @1:1-2:4
  text "b\nc" @1:7-2:4
"#
)]
#[case::fn_empty("[^a]:", "", r#"footnote "a" @1:1-1:6"#)]
#[case::fn_empty_space("[^a]: ", "", r#"footnote "a" @1:1-1:7"#)]
#[case::fn_content_next_line(
    "[^a]:\n    b",
    "",
    r#"
footnote "a" @1:1-2:6
  text "b" @2:5-2:6
"#
)]
#[case::fn_two_adjacent(
    "[^a]: b\n[^c]: d",
    "",
    r#"
footnote "a" @1:1-1:8
  text "b" @1:7-1:8
footnote "c" @2:1-2:8
  text "d" @2:7-2:8
"#
)]
#[case::fn_two_separated(
    "[^a]: b\n\n[^c]: d",
    "",
    r#"
footnote "a" @1:1-2:1
  text "b" @1:7-1:8
footnote "c" @3:1-3:8
  text "d" @3:7-3:8
"#
)]
#[case::fn_label_space("[^ a]: b", "", r#"definition "^ a" "b" label="^ a" @1:1-1:9"#)]
#[case::fn_label_inner_space("[^a b]: b", "", r#"definition "^a b" "b" label="^a b" @1:1-1:10"#)]
#[case::fn_label_empty("[^]: b", "", r#"definition "^" "b" label="^" @1:1-1:7"#)]
#[case::fn_indent1(
    " [^a]: b",
    "",
    r#"
footnote "a" @1:1-1:9
  text "b" @1:8-1:9
"#
)]
#[case::fn_indent4("    [^a]: b", "<pre><code>[^a]: b\n</code></pre>", r#"code "[^a]: b" @1:1-1:12"#)]
#[case::fn_in_quote(
    "> [^a]: b",
    "<blockquote>\n</blockquote>",
    r#"
blockquote @1:1-1:10
  footnote "a" @1:3-1:10
    text "b" @1:9-1:10
"#
)]
#[case::fn_in_list(
    "- [^a]: b",
    "<ul>\n<li></li>\n</ul>",
    r#"
list level=0 index=0 marker=- @1:3-1:10
  footnote "a" @1:3-1:10
    text "b" @1:9-1:10
"#
)]
#[case::fn_then_quote(
    "[^a]: b\n> q",
    "<blockquote>\n<p>q</p>\n</blockquote>",
    r#"
footnote "a" @1:1-1:8
  text "b" @1:7-1:8
blockquote @2:1-2:4
  text "q" @2:3-2:4
"#
)]
#[case::fn_then_list(
    "[^a]: b\n- x",
    "<ul>\n<li>x</li>\n</ul>",
    r#"
footnote "a" @1:1-1:8
  text "b" @1:7-1:8
list level=0 index=0 marker=- @2:3-2:4
  text "x" @2:3-2:4
"#
)]
#[case::fn_heading(
    "[^a]: # h",
    "",
    r#"
footnote "a" @1:1-1:10
  heading 1 @1:7-1:10
    text "h" @1:9-1:10
"#
)]
#[case::fn_list(
    "[^a]: - x\n    - y",
    "",
    r#"
footnote "a" @1:1-2:8
  list level=0 index=0 marker=- @1:9-1:10
    text "x" @1:9-1:10
  list level=0 index=1 marker=- @2:7-2:8
    text "y" @2:7-2:8
"#
)]
#[case::fn_fence(
    "[^a]: b\n    ```\n    x\n    ```",
    "",
    r#"
footnote "a" @1:1-4:8
  text "b" @1:7-1:8
  code "x" fence @2:5-4:8
"#
)]
#[case::fn_ref_case(
    "[^A]: b\n\nx[^a]",
    "<p>x<sup><a href=\"#user-content-fn-a\" id=\"user-content-fnref-a\" data-footnote-ref=\"\" aria-describedby=\"footnote-label\">1</a></sup></p>\n<section data-footnotes=\"\" class=\"footnotes\"><h2 id=\"footnote-label\" class=\"sr-only\">Footnotes</h2>\n<ol>\n<li id=\"user-content-fn-a\">\n<p>b <a href=\"#user-content-fnref-a\" data-footnote-backref=\"\" aria-label=\"Back to content\" class=\"data-footnote-backref\">↩</a></p>\n</li>\n</ol>\n</section>\n",
    r#"
footnote "a" @1:1-2:1
  text "b" @1:7-1:8
text "x" @3:1-3:2
footnote_ref "a" label="a" @3:2-3:6
"#
)]
#[case::fn_no_space(
    "[^a]:b",
    "",
    r#"
footnote "a" @1:1-1:7
  text "b" @1:6-1:7
"#
)]
#[case::fn_interrupts_paragraph(
    "text\n[^a]: b",
    "<p>text</p>\n",
    r#"
text "text" @1:1-1:5
footnote "a" @2:1-2:8
  text "b" @2:7-2:8
"#
)]
#[case::fn_then_dashes(
    "[^a]: b\n---",
    "<hr />",
    r#"
footnote "a" @1:1-1:8
  text "b" @1:7-1:8
hr marker='-' @2:1-2:4
"#
)]
#[case::fn_then_equals(
    "[^a]: b\n===",
    "",
    r#"
footnote "a" @1:1-2:4
  text "b\n===" @1:7-2:4
"#
)]
#[case::fn_ref_basic(
    "[^a]\n\n[^a]: note",
    "<p><sup><a href=\"#user-content-fn-a\" id=\"user-content-fnref-a\" data-footnote-ref=\"\" aria-describedby=\"footnote-label\">1</a></sup></p>\n<section data-footnotes=\"\" class=\"footnotes\"><h2 id=\"footnote-label\" class=\"sr-only\">Footnotes</h2>\n<ol>\n<li id=\"user-content-fn-a\">\n<p>note <a href=\"#user-content-fnref-a\" data-footnote-backref=\"\" aria-label=\"Back to content\" class=\"data-footnote-backref\">↩</a></p>\n</li>\n</ol>\n</section>\n",
    r#"
footnote_ref "a" label="a" @1:1-1:5
footnote "a" @3:1-3:11
  text "note" @3:7-3:11
"#
)]
#[case::fn_ref_no_def("[^b] no def", "<p>[^b] no def</p>", r#"text "[^b] no def" @1:1-1:12"#)]
#[case::fn_ref_inline(
    "x[^a] and [^a]!\n\n[^a]: note",
    "<p>x<sup><a href=\"#user-content-fn-a\" id=\"user-content-fnref-a\" data-footnote-ref=\"\" aria-describedby=\"footnote-label\">1</a></sup> and <sup><a href=\"#user-content-fn-a\" id=\"user-content-fnref-a-2\" data-footnote-ref=\"\" aria-describedby=\"footnote-label\">1</a></sup>!</p>\n<section data-footnotes=\"\" class=\"footnotes\"><h2 id=\"footnote-label\" class=\"sr-only\">Footnotes</h2>\n<ol>\n<li id=\"user-content-fn-a\">\n<p>note <a href=\"#user-content-fnref-a\" data-footnote-backref=\"\" aria-label=\"Back to content\" class=\"data-footnote-backref\">↩</a> <a href=\"#user-content-fnref-a-2\" data-footnote-backref=\"\" aria-label=\"Back to content\" class=\"data-footnote-backref\">↩<sup>2</sup></a></p>\n</li>\n</ol>\n</section>\n",
    r#"
text "x" @1:1-1:2
footnote_ref "a" label="a" @1:2-1:6
text " and " @1:6-1:11
footnote_ref "a" label="a" @1:11-1:15
text "!" @1:15-1:16
footnote "a" @3:1-3:11
  text "note" @3:7-3:11
"#
)]
#[case::fn_ref_in_link_text(
    "[a[^a]](b)\n\n[^a]: note",
    "<p>[a<sup><a href=\"#user-content-fn-a\" id=\"user-content-fnref-a\" data-footnote-ref=\"\" aria-describedby=\"footnote-label\">1</a></sup>](b)</p>\n<section data-footnotes=\"\" class=\"footnotes\"><h2 id=\"footnote-label\" class=\"sr-only\">Footnotes</h2>\n<ol>\n<li id=\"user-content-fn-a\">\n<p>note <a href=\"#user-content-fnref-a\" data-footnote-backref=\"\" aria-label=\"Back to content\" class=\"data-footnote-backref\">↩</a></p>\n</li>\n</ol>\n</section>\n",
    r#"
text "[a" @1:1-1:3
footnote_ref "a" label="a" @1:3-1:7
text "](b)" @1:7-1:11
footnote "a" @3:1-3:11
  text "note" @3:7-3:11
"#
)]
#[case::html_div(
    "<div>\nx\n</div>\n\ny",
    "<div>\nx\n</div>\n<p>y</p>",
    r#"
html "<div>\nx\n</div>" @1:1-3:7
text "y" @5:1-5:2
"#
)]
#[case::html_indented("  <div>\n  x\n", "  <div>\n  x\n", r#"html "  <div>\n  x" @1:1-2:4"#)]
#[case::html_oneline("<div>x</div>", "<div>x</div>", r#"html "<div>x</div>" @1:1-1:13"#)]
#[case::html_script(
    "<script>\nx\n\ny\n</script>\nz",
    "&lt;script>\nx\n\ny\n&lt;/script>\n<p>z</p>",
    r#"
html "<script>\nx\n\ny\n</script>" @1:1-5:10
text "z" @6:1-6:2
"#
)]
#[case::html_pre(
    "<pre>\na\n\nb</pre>\nc",
    "<pre>\na\n\nb</pre>\n<p>c</p>",
    r#"
html "<pre>\na\n\nb</pre>" @1:1-4:8
text "c" @5:1-5:2
"#
)]
#[case::html_style_inline_end(
    "<style>x</style> y\nz",
    "&lt;style>x&lt;/style> y\n<p>z</p>",
    r#"
html "<style>x</style> y" @1:1-1:19
text "z" @2:1-2:2
"#
)]
#[case::html_comment(
    "<!-- c\n\n d -->\nx",
    "<!-- c\n\n d -->\n<p>x</p>",
    r#"
html "<!-- c\n\n d -->" @1:1-3:7
text "x" @4:1-4:2
"#
)]
#[case::html_instruction(
    "<?php\nx ?>\ny",
    "<?php\nx ?>\n<p>y</p>",
    r#"
html "<?php\nx ?>" @1:1-2:5
text "y" @3:1-3:2
"#
)]
#[case::html_declaration(
    "<!DOCTYPE html>\nx",
    "<!DOCTYPE html>\n<p>x</p>",
    r#"
html "<!DOCTYPE html>" @1:1-1:16
text "x" @2:1-2:2
"#
)]
#[case::html_cdata(
    "<![CDATA[\n\nx]]>\ny",
    "<![CDATA[\n\nx]]>\n<p>y</p>",
    r#"
html "<![CDATA[\n\nx]]>" @1:1-3:5
text "y" @4:1-4:2
"#
)]
#[case::html_complete_tag("<a href=\"x\">\ny", "<a href=\"x\">\ny", r#"html "<a href=\"x\">\ny" @1:1-2:2"#)]
#[case::html_tag_then_text(
    "<a href=\"x\">y",
    "<p><a href=\"x\">y</p>",
    r#"
html "<a href=\"x\">" @1:1-1:13
text "y" @1:13-1:14
"#
)]
#[case::html_interrupt_basic(
    "text\n<div>",
    "<p>text</p>\n<div>",
    r#"
text "text" @1:1-1:5
html "<div>" @2:1-2:6
"#
)]
#[case::html_no_interrupt_complete(
    "text\n<a href=\"x\">",
    "<p>text\n<a href=\"x\"></p>",
    r#"
text "text\n" @1:1-2:1
html "<a href=\"x\">" @2:1-2:13
"#
)]
#[case::html_custom_element("<x-y>\nz", "<x-y>\nz", r#"html "<x-y>\nz" @1:1-2:2"#)]
#[case::html_closing("</div>\nz", "</div>\nz", r#"html "</div>\nz" @1:1-2:2"#)]
#[case::html_multiline_open(
    "<div\nclass=\"a\">\nz",
    "<div\nclass=\"a\">\nz",
    r#"html "<div\nclass=\"a\">\nz" @1:1-3:2"#
)]
#[case::html_in_quote(
    "> <div>\n> x",
    "<blockquote>\n<div>\nx\n</blockquote>",
    r#"
blockquote @1:1-2:4
  html "<div>\nx" @1:3-2:4
"#
)]
#[case::html_in_list(
    "- <div>\n  x",
    "<ul>\n<li>\n<div>\nx\n</li>\n</ul>",
    r#"
list level=0 index=0 marker=- @1:3-2:4
  html "<div>\nx" @1:3-2:4
"#
)]
#[case::html_uppercase("<DIV>\nx", "<DIV>\nx", r#"html "<DIV>\nx" @1:1-2:2"#)]
#[case::html_self_closing("<div/>\nx", "<div/>\nx", r#"html "<div/>\nx" @1:1-2:2"#)]
#[case::html_ins("<ins>\nx", "<ins>\nx", r#"html "<ins>\nx" @1:1-2:2"#)]
#[case::html_br("<br>\nx", "<br>\nx", r#"html "<br>\nx" @1:1-2:2"#)]
#[case::html_textarea(
    "<textarea>\nx\n\ny</textarea>",
    "&lt;textarea>\nx\n\ny&lt;/textarea>",
    r#"html "<textarea>\nx\n\ny</textarea>" @1:1-4:13"#
)]
#[case::html_table_two_blocks(
    "<table>\n<tr>\n\n<td>",
    "<table>\n<tr>\n<td>",
    r#"
html "<table>\n<tr>" @1:1-2:5
html "<td>" @4:1-4:5
"#
)]
#[case::html_img_self_close(
    "<img src=\"x\" />\nz",
    "<img src=\"x\" />\nz",
    r#"html "<img src=\"x\" />\nz" @1:1-2:2"#
)]
#[case::html_img_then_text(
    "<img src=\"x\" /> y",
    "<p><img src=\"x\" /> y</p>",
    r#"
html "<img src=\"x\" />" @1:1-1:16
text " y" @1:16-1:18
"#
)]
#[case::html_multiline_tag_inline(
    "<a\nb>\nz",
    "<p><a\nb>\nz</p>",
    r#"
html "<a\nb>" @1:1-2:3
text "\nz" @2:3-3:2
"#
)]
#[case::html_indent4("    <div>", "<pre><code>&lt;div&gt;\n</code></pre>", r#"code "<div>" @1:1-1:10"#)]
#[case::html_comment_then_text(
    "<!--x-->y\nz",
    "<!--x-->y\n<p>z</p>",
    r#"
html "<!--x-->y" @1:1-1:10
text "z" @2:1-2:2
"#
)]
#[case::html_then_code_line("<div>\n    code", "<div>\n    code", r#"html "<div>\n    code" @1:1-2:9"#)]
#[case::html_comment_short(
    "<!-->\nx",
    "<!-->\n<p>x</p>",
    r#"
html "<!-->" @1:1-1:6
text "x" @2:1-2:2
"#
)]
#[case::html_instruction_short(
    "<?>\nx",
    "<?>\n<p>x</p>",
    r#"
html "<?>" @1:1-1:4
text "x" @2:1-2:2
"#
)]
#[case::html_lazy_quote(
    "> <div>\nx",
    "<blockquote>\n<div>\n</blockquote>\n<p>x</p>",
    r#"
blockquote @1:1-1:8
  html "<div>" @1:3-1:8
text "x" @2:1-2:2
"#
)]
#[case::html_attr_forms(
    "<a b c=d e='f' g=\"h\">\nx",
    "<a b c=d e='f' g=\"h\">\nx",
    r#"html "<a b c=d e='f' g=\"h\">\nx" @1:1-2:2"#
)]
#[case::html_bad_attr("<a b=>\nx", "<p>&lt;a b=&gt;\nx</p>", r#"text "<a b=>\nx" @1:1-2:2"#)]
#[case::fm_yaml(
    "---\na: b\n---\ntext",
    "<p>text</p>",
    r#"
yaml "a: b" @1:1-3:4
text "text" @4:1-4:5
"#
)]
#[case::fm_yaml_only("---\na: b\n---", "", r#"yaml "a: b" @1:1-3:4"#)]
#[case::fm_empty("---\n---", "", r#"yaml "" @1:1-2:4"#)]
#[case::fm_unclosed(
    "---\na\n",
    "<hr />\n<p>a</p>\n",
    r#"
hr marker='-' @1:1-1:4
text "a" @2:1-2:2
"#
)]
#[case::fm_blank_content(
    "---\n\n---\nx",
    "<p>x</p>",
    r#"
yaml "" @1:1-3:4
text "x" @4:1-4:2
"#
)]
#[case::fm_toml(
    "+++\na = 1\n+++\nx",
    "<p>x</p>",
    r#"
toml "a = 1" @1:1-3:4
text "x" @4:1-4:2
"#
)]
#[case::fm_indented(
    " ---\na\n---",
    "<hr />\n<h2>a</h2>",
    r#"
hr marker='-' @1:1-1:5
heading 2 @2:1-3:4
  text "a" @2:1-2:2
"#
)]
#[case::fm_close_trailing_space(
    "---\na\n--- \nx",
    "<p>x</p>",
    r#"
yaml "a" @1:1-3:5
text "x" @4:1-4:2
"#
)]
#[case::fm_close_longer(
    "---\na\n----\nx",
    "<hr />\n<h2>a</h2>\n<p>x</p>",
    r#"
hr marker='-' @1:1-1:4
heading 2 @2:1-3:5
  text "a" @2:1-2:2
text "x" @4:1-4:2
"#
)]
#[case::fm_open_trailing_space("---  \na\n---", "", r#"yaml "a" @1:1-3:4"#)]
#[case::fm_open_junk(
    "---x\na\n---",
    "<h2>---x\na</h2>",
    r#"
heading 2 @1:1-3:4
  text "---x\na" @1:1-2:2
"#
)]
#[case::fm_not_first(
    "text\n---\na\n---",
    "<h2>text</h2>\n<h2>a</h2>",
    r#"
heading 2 @1:1-2:4
  text "text" @1:1-1:5
heading 2 @3:1-4:4
  text "a" @3:1-3:2
"#
)]
#[case::fm_blank_before(
    "\n---\na\n---",
    "<hr />\n<h2>a</h2>",
    r#"
hr marker='-' @2:1-2:4
heading 2 @3:1-4:4
  text "a" @3:1-3:2
"#
)]
#[case::fm_multiline_content(
    "---\na: 1\n\nb: 2\n---\n# h",
    "<h1>h</h1>",
    r#"
yaml "a: 1\n\nb: 2" @1:1-5:4
heading 1 @6:1-6:4
  text "h" @6:3-6:4
"#
)]
#[case::fm_hr_after(
    "---\na\n---\n\n---",
    "<hr />",
    r#"
yaml "a" @1:1-3:4
hr marker='-' @5:1-5:4
"#
)]
#[case::math_basic(
    "$$\na\n$$",
    "<pre><code class=\"language-math math-display\">a\n</code></pre>",
    r#"math "a" @1:1-3:3"#
)]
#[case::math_then_text(
    "$$\na\n$$\nx",
    "<pre><code class=\"language-math math-display\">a\n</code></pre>\n<p>x</p>",
    r#"
math "a" @1:1-3:3
text "x" @4:1-4:2
"#
)]
#[case::math_meta(
    "$$ meta\na\n$$",
    "<pre><code class=\"language-math math-display\">a\n</code></pre>",
    r#"math "a" @1:1-3:3"#
)]
#[case::math_unclosed(
    "$$\na",
    "<pre><code class=\"language-math math-display\">a\n</code></pre>\n",
    r#"math "a" @1:1-2:2"#
)]
#[case::math_longer_open(
    "$$$\na\n$$",
    "<pre><code class=\"language-math math-display\">a\n$$\n</code></pre>\n",
    r#"math "a\n$$" @1:1-3:3"#
)]
#[case::math_longer_close(
    "$$\na\n$$$$",
    "<pre><code class=\"language-math math-display\">a\n</code></pre>",
    r#"math "a" @1:1-3:5"#
)]
#[case::math_indented(
    "  $$\n  a\n   b\n  $$",
    "<pre><code class=\"language-math math-display\">a\n b\n</code></pre>",
    r#"math "a\n b" @1:3-4:5"#
)]
#[case::math_empty(
    "$$\n$$",
    "<pre><code class=\"language-math math-display\"></code></pre>",
    r#"math "" @1:1-2:3"#
)]
#[case::math_inline_not_block(
    "$$a$$",
    "<p><code class=\"language-math math-inline\">a</code></p>",
    r#"math_inline "a" @1:1-1:6"#
)]
#[case::math_info_no_close(
    "$$a\n$$",
    "<pre><code class=\"language-math math-display\"></code></pre>",
    r#"math "" @1:1-2:3"#
)]
#[case::math_in_quote(
    "> $$\n> a\n> $$",
    "<blockquote>\n<pre><code class=\"language-math math-display\">a\n</code></pre>\n</blockquote>",
    r#"
blockquote @1:1-3:5
  math "a" @1:3-3:5
"#
)]
#[case::math_in_list(
    "- $$\n  a\n  $$",
    "<ul>\n<li>\n<pre><code class=\"language-math math-display\">a\n</code></pre>\n</li>\n</ul>",
    r#"
list level=0 index=0 marker=- @1:3-3:5
  math "a" @1:3-3:5
"#
)]
#[case::math_close_junk(
    "$$\na\n$$ x",
    "<pre><code class=\"language-math math-display\">a\n$$ x\n</code></pre>\n",
    r#"math "a\n$$ x" @1:1-3:5"#
)]
#[case::math_interrupts_paragraph(
    "text\n$$\na\n$$",
    "<p>text</p>\n<pre><code class=\"language-math math-display\">a\n</code></pre>",
    r#"
text "text" @1:1-1:5
math "a" @2:1-4:3
"#
)]
#[case::math_blank_inside(
    "$$\n\na\n$$",
    "<pre><code class=\"language-math math-display\">\na\n</code></pre>",
    r#"math "\na" @1:1-4:3"#
)]
#[case::math_single_dollar_line(
    "$ $\nx",
    "<p><code class=\"language-math math-inline\"> </code>\nx</p>",
    r#"
math_inline " " @1:1-1:4
text "\nx" @1:4-2:2
"#
)]
#[case::crlf_frontmatter(
    "---\r\na\r\n---\r\nx",
    "<p>x</p>",
    r#"
yaml "a" @1:1-3:4
text "x" @4:1-4:2
"#
)]
#[case::break_end_indented(
    "a  \n  b",
    "<p>a<br />\nb</p>",
    r#"
text "a" @1:1-1:2
break @1:2-2:1
text "b" @2:3-2:4
"#
)]
#[case::text_end_indented_continuation(
    "*a*\n  <x>",
    "<p><em>a</em>\n<x></p>",
    r#"
emphasis @1:1-1:4
  text "a" @1:2-1:3
text "\n" @1:4-2:1
html "<x>" @2:3-2:6
"#
)]
#[case::soft_end_before_node(
    "a\n  `b`",
    "<p>a\n<code>b</code></p>",
    r#"
text "a\n" @1:1-2:1
code_inline "b" @2:3-2:6
"#
)]
#[case::break_in_quote(
    "> a  \n> b",
    "<blockquote>\n<p>a<br />\nb</p>\n</blockquote>",
    r#"
blockquote @1:1-2:4
  text "a" @1:3-1:4
  break @1:4-2:1
  text "b" @2:3-2:4
"#
)]
#[case::text_eol_in_footnote(
    "[^a]: x \n  [a]",
    "",
    r#"
footnote "a" @1:1-2:6
  text "x\n[a]" @1:7-2:6
"#
)]
#[case::math_whitespace_content(
    "  $$\n \n  $$",
    "<pre><code class=\"language-math math-display\">\n</code></pre>",
    r#"math "" @1:3-3:5"#
)]
#[case::math_whitespace_content2(
    "  $$\n  \n  $$",
    "<pre><code class=\"language-math math-display\">\n</code></pre>",
    r#"math "" @1:3-3:5"#
)]
#[case::table_indented_no_pipe(
    "  a|b\n|-|:-:|",
    "<table>\n<thead>\n<tr>\n<th>a</th>\n<th align=\"center\">b</th>\n</tr>\n</thead>\n</table>",
    r#"
cell 0:0 @1:1-1:4
  text "a" @1:3-1:4
cell 0:1 @1:4-1:6
  text "b" @1:5-1:6
align none,center @2:1-2:1
"#
)]
#[case::tab_nested_list(
    "- a\n\t- b\n\t\t- c\n",
    "<ul>\n<li>a\n<ul>\n<li>b\n<ul>\n<li>c</li>\n</ul>\n</li>\n</ul>\n</li>\n</ul>\n",
    r#"
list level=0 index=0 marker=- @1:3-1:4
  text "a" @1:3-1:4
list level=1 index=0 marker=- @2:7-2:8
  text "b" @2:7-2:8
list level=2 index=0 marker=- @3:11-3:12
  text "c" @3:11-3:12
"#
)]
#[case::tab_nested_ordered(
    "1. a\n\t1. b\n\t\t1. c\n",
    "<ol>\n<li>a\n<ol>\n<li>b\n<ol>\n<li>c</li>\n</ol>\n</li>\n</ol>\n</li>\n</ol>\n",
    r#"
list level=0 index=0 ordered start=1 marker=. @1:4-1:5
  text "a" @1:4-1:5
list level=1 index=0 ordered start=1 marker=. @2:8-2:9
  text "b" @2:8-2:9
list level=2 index=0 ordered start=1 marker=. @3:12-3:13
  text "c" @3:12-3:13
"#
)]
#[case::tab_list_code(
    "- a\n\n\t\tcode\n",
    "<ul>\n<li>\n<p>a</p>\n<pre><code>  code\n</code></pre>\n</li>\n</ul>\n",
    r#"
list level=0 index=0 marker=- @1:3-3:13
  text "a" @1:3-1:4
  code "  code" @3:3-3:13
"#
)]
#[case::tab_in_text(
    "a\tb\t*c*\n",
    "<p>a\tb\t<em>c</em></p>\n",
    r#"
text "a\tb\t" @1:1-1:9
emphasis @1:9-1:12
  text "c" @1:10-1:11
"#
)]
#[case::tab_code_block(
    "\tcode\n\t\tmore\n",
    "<pre><code>code\n\tmore\n</code></pre>\n",
    r#"code "code\n\tmore" @1:1-2:13"#
)]
#[case::tab_after_marker(
    "-\ta\n",
    "<ul>\n<li>a</li>\n</ul>\n",
    r#"
list level=0 index=0 marker=- @1:5-1:6
  text "a" @1:5-1:6
"#
)]
#[case::atx_leading_sequences(
    "# # a",
    "<h1># a</h1>",
    r##"
heading 1 @1:1-1:6
  text "# a" @1:3-1:6
"##
)]
#[case::atx_leading_sequences2(
    "## # a",
    "<h2># a</h2>",
    r##"
heading 2 @1:1-1:7
  text "# a" @1:4-1:7
"##
)]
#[case::atx_only_sequences(
    "# # #",
    "<h1>#</h1>",
    r##"
heading 1 @1:1-1:6
  text "#" @1:3-1:4
"##
)]
#[case::atx_no_space_sequence(
    "# #a",
    "<h1>#a</h1>",
    r##"
heading 1 @1:1-1:5
  text "#a" @1:3-1:5
"##
)]
#[case::atx_double_sequence(
    "# ## a",
    "<h1>## a</h1>",
    r###"
heading 1 @1:1-1:7
  text "## a" @1:3-1:7
"###
)]
#[case::atx_tab_sequence(
    "#\t# a",
    "<h1># a</h1>",
    r##"
heading 1 @1:1-1:8
  text "# a" @1:5-1:8
"##
)]
#[case::atx_closing_and_leading(
    "# # a #",
    "<h1># a</h1>",
    r##"
heading 1 @1:1-1:8
  text "# a" @1:3-1:6
"##
)]
#[case::atx_inner_hash(
    "# a # b",
    "<h1>a # b</h1>",
    r#"
heading 1 @1:1-1:8
  text "a # b" @1:3-1:8
"#
)]
#[case::quote_footnote_trailing_space(
    "- a\n  > [^a]: x\n  ",
    "<ul>\n<li>a\n<blockquote>\n</blockquote>\n</li>\n</ul>",
    r#"
list level=0 index=0 marker=- @1:3-3:3
  text "a" @1:3-1:4
  blockquote @2:3-3:3
    footnote "a" @2:5-3:3
      text "x" @2:11-2:12
"#
)]
#[case::indented_code_trailing_blank("    code\n\n\n", "<pre><code>code\n</code></pre>\n", r#"code "code" @1:1-1:9"#)]
#[case::bom_heading(
    "\u{feff}# a\n",
    "<h1>a</h1>\n",
    r#"
heading 1 @1:4-1:7
  text "a" @1:6-1:7
"#
)]
#[case::bom_frontmatter("\u{feff}---\na: b\n---\n", "", r#"yaml "a: b" @1:4-3:4"#)]
#[case::bom_text("\u{feff}a\n", "<p>a</p>\n", r#"text "a" @1:4-1:5"#)]
#[case::www_multibyte_after_ww("wwß x", "<p>wwß x</p>", r#"text "wwß x" @1:1-1:7"#)]
#[case::www_combining("ww\u{301}.a.com", "<p>ww\u{301}.a.com</p>", r#"text "ww\u{301}.a.com" @1:1-1:11"#)]
#[case::www_cjk(
    "www.あ.com/パス x",
    "<p><a href=\"http://www.%E3%81%82.com/%E3%83%91%E3%82%B9\">www.あ.com/パス</a> x</p>",
    r#"
link "http://www.あ.com/パス" @1:1-1:19
  text "www.あ.com/パス" @1:1-1:19
text " x" @1:19-1:21
"#
)]
#[case::http_cjk(
    "http://あ.jp/パス ok",
    "<p><a href=\"http://%E3%81%82.jp/%E3%83%91%E3%82%B9\">http://あ.jp/パス</a> ok</p>",
    r#"
link "http://あ.jp/パス" @1:1-1:21
  text "http://あ.jp/パス" @1:1-1:21
text " ok" @1:21-1:24
"#
)]
#[case::email_cjk_domain("a@あ.com", "<p>a@あ.com</p>", r#"text "a@あ.com" @1:1-1:10"#)]
#[case::email_cjk_local("あ@b.com", "<p>あ@b.com</p>", r#"text "あ@b.com" @1:1-1:10"#)]
#[case::html_inline_continuation(
    "<img a\n     b=\"c\">",
    "<p><img a\nb=\"c\"></p>",
    r#"html "<img a\nb=\"c\">" @1:1-2:12"#
)]
#[case::html_inline_continuation_tab(
    "<img a\n\t\tb=\"c\">",
    "<p><img a\nb=\"c\"></p>",
    r#"html "<img a\nb=\"c\">" @1:1-2:15"#
)]
#[case::table_trailing_space(
    "| a | b |  \n|-|-|  \n| c | d |  \n",
    "<table>\n<thead>\n<tr>\n<th>a</th>\n<th>b</th>\n</tr>\n</thead>\n<tbody>\n<tr>\n<td>c</td>\n<td>d</td>\n</tr>\n</tbody>\n</table>\n",
    r#"
cell 0:0 @1:1-1:5
  text "a" @1:3-1:4
cell 0:1 @1:5-1:12
  text "b" @1:7-1:8
align none,none @2:1-2:1
cell 1:0 @3:1-3:5
  text "c" @3:3-3:4
cell 1:1 @3:5-3:12
  text "d" @3:7-3:8
"#
)]
#[case::table_no_pipes_trailing_space(
    "a | b\n--|--\nd | e \n",
    "<table>\n<thead>\n<tr>\n<th>a</th>\n<th>b</th>\n</tr>\n</thead>\n<tbody>\n<tr>\n<td>d</td>\n<td>e</td>\n</tr>\n</tbody>\n</table>\n",
    r#"
cell 0:0 @1:1-1:3
  text "a" @1:1-1:2
cell 0:1 @1:3-1:6
  text "b" @1:5-1:6
align none,none @2:1-2:1
cell 1:0 @3:1-3:3
  text "d" @3:1-3:2
cell 1:1 @3:3-3:7
  text "e" @3:5-3:6
"#
)]
#[case::table_trailing_tab(
    "| a | b |\t\n|-|-|\n| c | d |   \n",
    "<table>\n<thead>\n<tr>\n<th>a</th>\n<th>b</th>\n</tr>\n</thead>\n<tbody>\n<tr>\n<td>c</td>\n<td>d</td>\n</tr>\n</tbody>\n</table>\n",
    r#"
cell 0:0 @1:1-1:5
  text "a" @1:3-1:4
cell 0:1 @1:5-1:13
  text "b" @1:7-1:8
align none,none @2:1-2:1
cell 1:0 @3:1-3:5
  text "c" @3:3-3:4
cell 1:1 @3:5-3:13
  text "d" @3:7-3:8
"#
)]
#[case::table_cjk(
    "| あ | 🎉 |\n|:-|-:|\n| é | ß |\n",
    "<table>\n<thead>\n<tr>\n<th align=\"left\">あ</th>\n<th align=\"right\">🎉</th>\n</tr>\n</thead>\n<tbody>\n<tr>\n<td align=\"left\">é</td>\n<td align=\"right\">ß</td>\n</tr>\n</tbody>\n</table>\n",
    r#"
cell 0:0 @1:1-1:7
  text "あ" @1:3-1:6
cell 0:1 @1:7-1:15
  text "🎉" @1:9-1:13
align left,right @2:1-2:1
cell 1:0 @3:1-3:6
  text "é" @3:3-3:5
cell 1:1 @3:6-3:12
  text "ß" @3:8-3:10
"#
)]
#[case::table_single_column_colon(
    "a\n:-\nb\n",
    "<table>\n<thead>\n<tr>\n<th align=\"left\">a</th>\n</tr>\n</thead>\n<tbody>\n<tr>\n<td align=\"left\">b</td>\n</tr>\n</tbody>\n</table>\n",
    r#"
cell 0:0 @1:1-1:2
  text "a" @1:1-1:2
align left @2:1-2:1
cell 1:0 @3:1-3:2
  text "b" @3:1-3:2
"#
)]
#[case::table_single_column_right(
    "a\n---:\nb\nc\n",
    "<table>\n<thead>\n<tr>\n<th align=\"right\">a</th>\n</tr>\n</thead>\n<tbody>\n<tr>\n<td align=\"right\">b</td>\n</tr>\n<tr>\n<td align=\"right\">c</td>\n</tr>\n</tbody>\n</table>\n",
    r#"
cell 0:0 @1:1-1:2
  text "a" @1:1-1:2
align right @2:1-2:1
cell 1:0 @3:1-3:2
  text "b" @3:1-3:2
cell 2:0 @4:1-4:2
  text "c" @4:1-4:2
"#
)]
#[case::table_double_colon_not_delimiter("a\n::-\n", "<p>a\n::-</p>\n", r#"text "a\n::-" @1:1-2:4"#)]
#[case::table_body_empty_marker(
    "a|b\n|-|-|\n*\n",
    "<table>\n<thead>\n<tr>\n<th>a</th>\n<th>b</th>\n</tr>\n</thead>\n</table>\n<ul>\n<li></li>\n</ul>\n",
    r#"
cell 0:0 @1:1-1:2
  text "a" @1:1-1:2
cell 0:1 @1:2-1:4
  text "b" @1:3-1:4
align none,none @2:1-2:1
list level=0 index=0 marker=* @3:1-3:2
"#
)]
#[case::table_body_ordered_marker(
    "a|b\n|-|-|\n2. x\n",
    "<table>\n<thead>\n<tr>\n<th>a</th>\n<th>b</th>\n</tr>\n</thead>\n</table>\n<ol start=\"2\">\n<li>x</li>\n</ol>\n",
    r#"
cell 0:0 @1:1-1:2
  text "a" @1:1-1:2
cell 0:1 @1:2-1:4
  text "b" @1:3-1:4
align none,none @2:1-2:1
list level=0 index=0 ordered start=2 marker=. @3:4-3:5
  text "x" @3:4-3:5
"#
)]
#[case::table_body_empty_ordered(
    "a|b\n|-|-|\n1.\n",
    "<table>\n<thead>\n<tr>\n<th>a</th>\n<th>b</th>\n</tr>\n</thead>\n</table>\n<ol>\n<li></li>\n</ol>\n",
    r#"
cell 0:0 @1:1-1:2
  text "a" @1:1-1:2
cell 0:1 @1:2-1:4
  text "b" @1:3-1:4
align none,none @2:1-2:1
list level=0 index=0 ordered start=1 marker=. @3:1-3:3
"#
)]
#[case::paragraph_after_partial_tab(
    "- a\n\n\t[P](h) x\n",
    "<ul>\n<li>\n<p>a</p>\n<p><a href=\"h\">P</a> x</p>\n</li>\n</ul>\n",
    r#"
list level=0 index=0 marker=- @1:3-3:13
  text "a" @1:3-1:4
  link "h" @3:5-3:11
    text "P" @3:6-3:7
  text " x" @3:11-3:13
"#
)]
#[case::fence_after_partial_tab(
    "- a\n\n\t```ts\n\t\tx\n\t```\n",
    "<ul>\n<li>\n<p>a</p>\n<pre><code class=\"language-ts\">\tx\n</code></pre>\n</li>\n</ul>\n",
    r#"
list level=0 index=0 marker=- @1:3-5:8
  text "a" @1:3-1:4
  code "\tx" lang="ts" fence @3:5-5:8
"#
)]
#[case::math_after_partial_tab(
    "- a\n\n\t$$\n\tm\n\t$$\n",
    "<ul>\n<li>\n<p>a</p>\n<pre><code class=\"language-math math-display\">m\n</code></pre>\n</li>\n</ul>\n",
    r#"
list level=0 index=0 marker=- @1:3-5:7
  text "a" @1:3-1:4
  math "m" @3:5-5:7
"#
)]
#[case::quote_after_partial_tab(
    "- a\n\n\t> q\n",
    "<ul>\n<li>\n<p>a</p>\n<blockquote>\n<p>q</p>\n</blockquote>\n</li>\n</ul>\n",
    r#"
list level=0 index=0 marker=- @1:3-3:8
  text "a" @1:3-1:4
  blockquote @3:3-3:8
    text "q" @3:7-3:8
"#
)]
#[case::heading_after_partial_tab(
    "- a\n\n\t# h\n",
    "<ul>\n<li>\n<p>a</p>\n<h1>h</h1>\n</li>\n</ul>\n",
    r#"
list level=0 index=0 marker=- @1:3-3:8
  text "a" @1:3-1:4
  heading 1 @3:3-3:8
    text "h" @3:7-3:8
"#
)]
#[case::control_character_references(
    "&#1;&#x7f;&#x85;&#xB;&#xC;&#x9f;&#xa0;&#xd800;&#x110000;&#0;",
    "<p>����\u{c}�\u{a0}���</p>",
    r#"text "����\u{c}�\u{a0}���" @1:1-1:61"#
)]
#[case::footnote_escaped_bracket("[^a\\]: x", "<p>[^a]: x</p>", r#"text "[^a]: x" @1:1-1:9"#)]
#[case::footnote_escaped_backslash(
    "[^a\\\\]: x",
    "",
    r#"
footnote "a\\\\" @1:1-1:10
  text "x" @1:9-1:10
"#
)]
#[case::footnote_escaped_open(
    "[^a\\[]: x",
    "",
    r#"
footnote "a\\[" @1:1-1:10
  text "x" @1:9-1:10
"#
)]
#[case::footnote_unescaped_open("[^a[]: x", "<p>[^a[]: x</p>", r#"text "[^a[]: x" @1:1-1:9"#)]
#[case::footnote_empty_then_blank("[^a]: \n\n", "", r#"footnote "a" @1:1-2:1"#)]
#[case::footnote_empty_then_spaces("[^a]: \n    \n", "", r#"footnote "a" @1:1-2:5"#)]
#[case::footnote_empty_then_text(
    "[^a]: \n\nb",
    "<p>b</p>",
    r#"
footnote "a" @1:1-2:1
text "b" @3:1-3:2
"#
)]
#[case::definition_nul_destination(
    "[a]: \0\n\n[a]\n",
    "<p><a href=\"%EF%BF%BD\">a</a></p>\n",
    r#"
definition "a" "\0" label="a" @1:1-1:7
link_ref "a" label="a" @3:1-3:4
  text "a" @3:2-3:3
"#
)]
#[case::link_nul_destination(
    "[a](x\0y)",
    "<p><a href=\"x%EF%BF%BDy\">a</a></p>",
    r#"
link "x\0y" @1:1-1:9
  text "a" @1:2-1:3
"#
)]
#[case::definition_then_empty_marker(
    "[a]: b\n-",
    "<p>-</p>",
    r#"
definition "a" "b" label="a" @1:1-1:7
text "-" @2:1-2:2
"#
)]
#[case::definition_then_item(
    "[a]: b\n- x",
    "<ul>\n<li>x</li>\n</ul>",
    r#"
definition "a" "b" label="a" @1:1-1:7
list level=0 index=0 marker=- @2:3-2:4
  text "x" @2:3-2:4
"#
)]
#[case::definition_then_ordered(
    "[a]: b\n1.",
    "<p>1.</p>",
    r#"
definition "a" "b" label="a" @1:1-1:7
text "1." @2:1-2:3
"#
)]
#[case::emphasis_cjk(
    "*あ*。*い*",
    "<p><em>あ</em>。<em>い</em></p>",
    r#"
emphasis @1:1-1:6
  text "あ" @1:2-1:5
text "。" @1:6-1:9
emphasis @1:9-1:14
  text "い" @1:10-1:13
"#
)]
#[case::strong_cjk(
    "**日本語**です",
    "<p><strong>日本語</strong>です</p>",
    r#"
strong @1:1-1:14
  text "日本語" @1:3-1:12
text "です" @1:14-1:20
"#
)]
#[case::underscore_accent(
    "_é_ _ß_",
    "<p><em>é</em> <em>ß</em></p>",
    r#"
emphasis @1:1-1:5
  text "é" @1:2-1:4
text " " @1:5-1:6
emphasis @1:6-1:10
  text "ß" @1:7-1:9
"#
)]
#[case::fullwidth_markers("＊a＊ ＿b＿", "<p>＊a＊ ＿b＿</p>", r#"text "＊a＊ ＿b＿" @1:1-1:16"#)]
#[case::emphasis_after_cjk_punctuation(
    "。*a*、**b**",
    "<p>。<em>a</em>、<strong>b</strong></p>",
    r#"
text "。" @1:1-1:4
emphasis @1:4-1:7
  text "a" @1:5-1:6
text "、" @1:7-1:10
strong @1:10-1:15
  text "b" @1:12-1:13
"#
)]
#[case::combining_mark(
    "e\u{301}*a*",
    "<p>e\u{301}<em>a</em></p>",
    r#"
text "e\u{301}" @1:1-1:4
emphasis @1:4-1:7
  text "a" @1:5-1:6
"#
)]
#[case::emoji_zwj(
    "👨\u{200d}👩\u{200d}👧 *a*",
    "<p>👨\u{200d}👩\u{200d}👧 <em>a</em></p>",
    r#"
text "👨\u{200d}👩\u{200d}👧 " @1:1-1:20
emphasis @1:20-1:23
  text "a" @1:21-1:22
"#
)]
#[case::fence_cjk_info(
    "```あ\n日本語\n```\n",
    "<pre><code class=\"language-あ\">日本語\n</code></pre>\n",
    r#"code "日本語" lang="あ" fence @1:1-3:4"#
)]
#[case::fence_in_quote_emoji(
    "> ```🎉\n> x\n> ```\n",
    "<blockquote>\n<pre><code class=\"language-🎉\">x\n</code></pre>\n</blockquote>\n",
    r#"
blockquote @1:1-3:6
  code "x" lang="🎉" fence @1:3-3:6
"#
)]
#[case::fence_in_list_cjk(
    "- ```あ\n  日本語\n  ```\n",
    "<ul>\n<li>\n<pre><code class=\"language-あ\">日本語\n</code></pre>\n</li>\n</ul>\n",
    r#"
list level=0 index=0 marker=- @1:3-3:6
  code "日本語" lang="あ" fence @1:3-3:6
"#
)]
#[case::math_cjk(
    "$$\nあ\n$$\n\n$い$\n",
    "<pre><code class=\"language-math math-display\">あ\n</code></pre>\n<p><code class=\"language-math math-inline\">い</code></p>\n",
    r#"
math "あ" @1:1-3:3
math_inline "い" @5:1-5:6
"#
)]
#[case::heading_cjk(
    "# 見出し\n\n見出し2\n===\n",
    "<h1>見出し</h1>\n<h1>見出し2</h1>\n",
    r#"
heading 1 @1:1-1:12
  text "見出し" @1:3-1:12
heading 1 @3:1-4:4
  text "見出し2" @3:1-3:11
"#
)]
#[case::link_cjk(
    "[あ](http://い.jp/う \"え\")",
    "<p><a href=\"http://%E3%81%84.jp/%E3%81%86\" title=\"え\">あ</a></p>",
    r#"
link "http://い.jp/う" title="え" @1:1-1:31
  text "あ" @1:2-1:5
"#
)]
#[case::reference_cjk(
    "[あ]\n\n[あ]: /u\n",
    "<p><a href=\"/u\">あ</a></p>\n",
    r#"
link_ref "あ" label="あ" @1:1-1:6
  text "あ" @1:2-1:5
definition "あ" "/u" label="あ" @3:1-3:10
"#
)]
#[case::footnote_cjk(
    "あ[^い]\n\n[^い]: う\n",
    "<p>あ<sup><a href=\"#user-content-fn-%E3%81%84\" id=\"user-content-fnref-%E3%81%84\" data-footnote-ref=\"\" aria-describedby=\"footnote-label\">1</a></sup></p>\n<section data-footnotes=\"\" class=\"footnotes\"><h2 id=\"footnote-label\" class=\"sr-only\">Footnotes</h2>\n<ol>\n<li id=\"user-content-fn-%E3%81%84\">\n<p>う <a href=\"#user-content-fnref-%E3%81%84\" data-footnote-backref=\"\" aria-label=\"Back to content\" class=\"data-footnote-backref\">↩</a></p>\n</li>\n</ol>\n</section>\n",
    r#"
text "あ" @1:1-1:4
footnote_ref "い" label="い" @1:4-1:10
footnote "い" @3:1-3:12
  text "う" @3:9-3:12
"#
)]
#[case::ideographic_space(
    "a\u{3000}b\n\u{3000}c",
    "<p>a\u{3000}b\n\u{3000}c</p>",
    r#"text "a\u{3000}b\n\u{3000}c" @1:1-2:5"#
)]
#[case::nbsp_around_emphasis(
    "a\u{a0}*b*\u{a0}c",
    "<p>a\u{a0}<em>b</em>\u{a0}c</p>",
    r#"
text "a\u{a0}" @1:1-1:4
emphasis @1:4-1:7
  text "b" @1:5-1:6
text "\u{a0}c" @1:7-1:10
"#
)]
#[case::entity_cjk_context(
    "あ&amp;い&#12354;&#x1F389;",
    "<p>あ&amp;いあ🎉</p>",
    r#"text "あ&いあ🎉" @1:1-1:29"#
)]
#[case::autolink_cjk(
    "<http://あ.jp/パス>",
    "<p><a href=\"http://%E3%81%82.jp/%E3%83%91%E3%82%B9\">http://あ.jp/パス</a></p>",
    r#"
link "http://あ.jp/パス" @1:1-1:23
  text "http://あ.jp/パス" @1:2-1:22
"#
)]
#[case::code_span_cjk(
    "`あ`と``い`う``",
    "<p><code>あ</code>と<code>い`う</code></p>",
    r#"
code_inline "あ" @1:1-1:6
text "と" @1:6-1:9
code_inline "い`う" @1:9-1:20
"#
)]
#[case::hard_break_cjk(
    "あ  \nい\\\nう",
    "<p>あ<br />\nい<br />\nう</p>",
    r#"
text "あ" @1:1-1:4
break @1:4-2:1
text "い" @2:1-2:4
break @2:4-3:1
text "う" @3:1-3:4
"#
)]
#[case::nul_in_text("a\0b", "<p>a�b</p>", r#"text "a\0b" @1:1-1:4"#)]
fn markdown(#[case] input: &str, #[case] html: &str, #[case] expected: &str) {
    let nodes = Markdown::from_markdown_str(input).unwrap().nodes;
    assert_eq!(tree(&nodes), expected.trim_matches('\n'), "{input:?}");
    assert_eq!(mq_markdown::to_html(input), html, "{input:?}");
}

#[rstest]
#[case::jsx_self("<a />", "jsx_flow <a> @1:1-1:6")]
#[case::jsx_attrs(
    "<a b=\"c&amp;d\" e='f' g={h} {...i} j />",
    r#"jsx_flow <a b="c&d" e="f" g={h} {...i} j> @1:1-1:39"#
)]
#[case::jsx_member("<a.b.c />", "jsx_flow <a.b.c> @1:1-1:10")]
#[case::jsx_namespace("<a:b c:d=\"e\" />", r#"jsx_flow <a:b c:d="e"> @1:1-1:16"#)]
#[case::jsx_fragment("<></>", "jsx_flow <> @1:1-1:6")]
#[case::jsx_text_pair(
    "<a>x</a>",
    r#"
jsx_text <a> @1:1-1:9
  text "x" @1:4-1:5
"#
)]
#[case::jsx_flow_pair(
    "<a>\n\nx\n\n</a>",
    r#"
jsx_flow <a> @1:1-5:5
  text "x" @3:1-3:2
"#
)]
#[case::jsx_flow_indented_child(
    "<a>\n  x\n</a>",
    r#"
jsx_flow <a> @1:1-3:5
  text "x" @2:3-2:4
"#
)]
#[case::jsx_inline(
    "a <b>c</b> d",
    r#"
text "a " @1:1-1:3
jsx_text <b> @1:3-1:11
  text "c" @1:6-1:7
text " d" @1:11-1:13
"#
)]
#[case::jsx_inline_self(
    "a <b/> d",
    r#"
text "a " @1:1-1:3
jsx_text <b> @1:3-1:7
text " d" @1:7-1:9
"#
)]
#[case::expr_flow("{a}", r#"expression_flow "a" @1:1-1:4"#)]
#[case::expr_nested("{a {b} c}", r#"expression_flow "a {b} c" @1:1-1:10"#)]
#[case::expr_multiline("{a\nb}", r#"expression_flow "a\nb" @1:1-2:3"#)]
#[case::expr_multiline_indented("{a\n  b\n   c}", r#"expression_flow "a\nb\n c" @1:1-3:6"#)]
#[case::expr_text(
    "x {a} y",
    r#"
text "x " @1:1-1:3
expression_text "a" @1:3-1:6
text " y" @1:6-1:8
"#
)]
#[case::expr_then_tag(
    "{a} <b/>",
    r#"
expression_flow "a" @1:1-1:4
jsx_flow <b> @1:5-1:9
"#
)]
#[case::tag_then_expr(
    "<b/> {a}",
    r#"
jsx_flow <b> @1:1-1:5
expression_flow "a" @1:6-1:9
"#
)]
#[case::tag_then_text(
    "<b/> x",
    r#"
jsx_text <b> @1:1-1:5
text " x" @1:5-1:7
"#
)]
#[case::expr_then_text(
    "{a}x",
    r#"
expression_text "a" @1:1-1:4
text "x" @1:4-1:5
"#
)]
#[case::jsx_in_expr_child(
    "<a>{b}</a>",
    r#"
jsx_flow <a> @1:1-1:11
  expression_flow "b" @1:4-1:7
"#
)]
#[case::jsx_unclosed_text("<a>b", "error: Expected a closing tag for `<a>` before the end of the content")]
#[case::jsx_mismatch(
    "<a></b>",
    "error: Unexpected closing tag `</b>`, expected corresponding closing tag for `<a>` (1:1)"
)]
#[case::lt_space("a < b", r#"text "a < b" @1:1-1:6"#)]
#[case::lt_digit("a <3", "error: Unexpected character `3` (U+0033) before name")]
#[case::attr_no_value("<a b=>", "error: Unexpected character `>` (U+003E) before attribute value")]
#[case::attr_space_after_eq("<b e= \"f\"/>", r#"jsx_flow <b e="f"> @1:1-1:12"#)]
#[case::attr_spaces_around_eq(
    "a <b e = \"f\"/>.",
    r#"
text "a " @1:1-1:3
jsx_text <b e="f"> @1:3-1:15
text "." @1:15-1:16
"#
)]
#[case::attr_line_before_eq(
    "<b c\n= \"x\">c</b>",
    r#"
jsx_text <b c="x"> @1:1-2:12
  text "c" @2:7-2:8
"#
)]
#[case::attr_namespace_spaces(
    "a <b xml :\tlang\n= \"de-CH\" foo:bar>c</b>.",
    r#"
text "a " @1:1-1:3
jsx_text <b xml:lang="de-CH" foo:bar> @1:3-2:24
  text "c" @2:19-2:20
text "." @2:24-2:25
"#
)]
#[case::attr_spaced_names(
    "a <b a b : c d : e = \"f\" g/>.",
    r#"
text "a " @1:1-1:3
jsx_text <b a b:c d:e="f" g> @1:3-1:29
text "." @1:29-1:30
"#
)]
#[case::attr_expr_space_after_eq("<b e= {f}/>", "jsx_flow <b e={f}> @1:1-1:12")]
#[case::jsx_nested_flow(
    "<a>\n<b>\n</b>\n</a>",
    r#"
jsx_flow <a> @1:1-4:5
  jsx_flow <b> @2:1-3:5
"#
)]
#[case::jsx_in_quote(
    "> <a>\n> x\n> </a>",
    r#"
blockquote @1:1-3:7
  jsx_flow <a> @1:3-3:7
    text "x" @2:3-2:4
"#
)]
#[case::jsx_in_list(
    "- <a>\n  x\n  </a>",
    r#"
list level=0 index=0 marker=- @1:3-3:7
  jsx_flow <a> @1:3-3:7
    text "x" @2:3-2:4
"#
)]
#[case::attr_quote_inside("<a b='c\"d' />", r#"jsx_flow <a b="c\"d"> @1:1-1:14"#)]
#[case::two_tags(
    "<a/><b/>",
    r#"
jsx_flow <a> @1:1-1:5
jsx_flow <b> @1:5-1:9
"#
)]
#[case::two_text_elements(
    "<a>x</a><b>y</b>",
    r#"
jsx_text <a> @1:1-1:9
  text "x" @1:4-1:5
jsx_text <b> @1:9-1:17
  text "y" @1:12-1:13
"#
)]
#[case::interrupt_paragraph_tag(
    "a\n<b/>",
    r#"
text "a" @1:1-1:2
jsx_flow <b> @2:1-2:5
"#
)]
#[case::interrupt_paragraph_expr(
    "a\n{b}",
    r#"
text "a" @1:1-1:2
expression_flow "b" @2:1-2:4
"#
)]
#[case::two_expressions(
    "{a}\n{b}",
    r#"
expression_flow "a" @1:1-1:4
expression_flow "b" @2:1-2:4
"#
)]
#[case::expr_in_flow_element(
    "<a>\n\n{b}\n\n</a>",
    r#"
jsx_flow <a> @1:1-5:5
  expression_flow "b" @3:1-3:4
"#
)]
#[case::indented_tag("   <a/>", "jsx_flow <a> @1:4-1:8")]
#[case::very_indented_tag("    <a/>", "jsx_flow <a> @1:5-1:9")]
#[case::heading_jsx(
    "# h <a>b</a>",
    r#"
heading 1 @1:1-1:13
  text "h " @1:3-1:5
  jsx_text <a> @1:5-1:13
    text "b" @1:8-1:9
"#
)]
#[case::emphasis_crossing(
    "*<a>x</a>*",
    r#"
emphasis @1:1-1:11
  jsx_text <a> @1:2-1:10
    text "x" @1:5-1:6
"#
)]
#[case::emphasis_crossing_bad(
    "*<a>x*</a>",
    "error: Expected a closing tag for `<a>` before the end of the content"
)]
#[case::link_jsx(
    "[<a>x</a>](y)",
    r#"
link "y" @1:1-1:14
  jsx_text <a> @1:2-1:10
    text "x" @1:5-1:6
"#
)]
#[case::code_not_jsx(
    "`<a/>` {b}",
    r#"
code_inline "<a/>" @1:1-1:7
text " " @1:7-1:8
expression_text "b" @1:8-1:11
"#
)]
#[case::escaped("\\<a/> \\{b}", r#"text "<a/> {b}" @1:2-1:11"#)]
#[case::attr_entities("<a b=\"&lt;&#x41;\" />", r#"jsx_flow <a b="<A"> @1:1-1:21"#)]
#[case::attr_multiline_literal("<a b='x\ny' />", r#"jsx_flow <a b="x\ny"> @1:1-2:6"#)]
#[case::closing_spaces(
    "<a  >x</a  >",
    r#"
jsx_text <a> @1:1-1:13
  text "x" @1:6-1:7
"#
)]
#[case::lt_space_name("< a>", r#"text "< a>" @1:1-1:5"#)]
#[case::self_closing_space("<a / >", "jsx_flow <a> @1:1-1:7")]
#[case::attr_on_next_line("<a\nb />", "jsx_flow <a b> @1:1-2:5")]
#[case::dashed_name("<a-b />", "jsx_flow <a-b> @1:1-1:8")]
#[case::dashed_attr("<a b-c=\"d\" />", r#"jsx_flow <a b-c="d"> @1:1-1:14"#)]
#[case::bad_name_char("<A_b$.c-d />", "error: Unexpected character `$` (U+0024) after name")]
#[case::expr_unclosed(
    "{a",
    "error: Unexpected end of file in expression, expected a corresponding closing brace for `{`"
)]
#[case::expr_empty("{}", r#"expression_flow "" @1:1-1:3"#)]
#[case::expr_space("{ }", r#"expression_flow " " @1:1-1:4"#)]
#[case::empty_flow_element("<a>\n</a>", "jsx_flow <a> @1:1-2:5")]
#[case::text_element_unclosed_line(
    "<a>x\n</a>",
    "error: Expected a closing tag for `<a>` before the end of the content"
)]
#[case::stray_close("</a>", "error: Unexpected closing slash `/` in tag, expected an open tag first")]
#[case::stray_close_text("x </a>", "error: Unexpected closing slash `/` in tag, expected an open tag first")]
#[case::element_with_emphasis(
    "<a>*b*</a>",
    r#"
jsx_text <a> @1:1-1:11
  emphasis @1:4-1:7
    text "b" @1:5-1:6
"#
)]
#[case::flow_element_with_emphasis(
    "<a>\n*b*\n</a>",
    r#"
jsx_flow <a> @1:1-3:5
  emphasis @2:1-2:4
    text "b" @2:2-2:3
"#
)]
#[case::flow_element_with_list(
    "<a>\n- b\n</a>",
    r#"
jsx_flow <a> @1:1-3:5
  list level=0 index=0 marker=- @2:3-2:4
    text "b" @2:3-2:4
"#
)]
#[case::list_items_unbalanced(
    "- <a>\n- </a>",
    "error: Expected a closing tag for `<a>` (1:3) before the end of its container"
)]
#[case::spaced_equals("<a b = \"c\" />", r#"jsx_flow <a b="c"> @1:1-1:14"#)]
#[case::heading(
    "# h\n\ntext *a* [b](c) `d`\n",
    r#"
heading 1 @1:1-1:4
  text "h" @1:3-1:4
text "text " @3:1-3:6
emphasis @3:6-3:9
  text "a" @3:7-3:8
text " " @3:9-3:10
link "c" @3:10-3:16
  text "b" @3:11-3:12
text " " @3:16-3:17
code_inline "d" @3:17-3:20
"#
)]
#[case::indented_is_text("    not code\n", r#"text "not code" @1:5-1:13"#)]
#[case::indented_heading(
    "     # h\n",
    r#"
heading 1 @1:1-1:9
  text "h" @1:8-1:9
"#
)]
#[case::no_autolink("<http://a.b>", "error: Unexpected character `/` (U+002F) before local name")]
#[case::no_gfm(
    "| a |\n|-|\n\n~a~ www.a.b [^a]\n\n- [ ] a\n",
    r#"
text "| a |\n|-|" @1:1-2:4
text "~a~ www.a.b [^a]" @4:1-4:17
list level=0 index=0 marker=- @6:3-6:8
  text "[ ] a" @6:3-6:8
"#
)]
#[case::no_math(
    "$a$\n\n$$\na\n$$\n",
    r#"
text "$a$" @1:1-1:4
text "$$\na\n$$" @3:1-5:3
"#
)]
#[case::frontmatter("---\na\n---\n", r#"yaml "a" @1:1-3:4"#)]
#[case::frontmatter_toml(
    "+++\na = 1\n+++\n\n<A />",
    r#"
toml "a = 1" @1:1-3:4
jsx_flow <A> @5:1-5:6
"#
)]
#[case::frontmatter_unclosed(
    "---\na\n",
    r#"
hr marker='-' @1:1-1:4
text "a" @2:1-2:2
"#
)]
#[case::esm(
    "import a from 'b'\n\nexport const c = 1\n",
    r#"
esm "import a from 'b'" @1:1-1:18
esm "export const c = 1" @3:1-3:19
"#
)]
#[case::definition(
    "[a]: /u\n\n[a]\n",
    r#"
definition "a" "/u" label="a" @1:1-1:8
link_ref "a" label="a" @3:1-3:4
  text "a" @3:2-3:3
"#
)]
#[case::list_deep_indent(
    "-      a\n",
    r#"
list level=0 index=0 marker=- @1:8-1:9
  text "a" @1:8-1:9
"#
)]
#[case::fence("```rust\ncode\n```\n", r#"code "code" lang="rust" fence @1:1-3:4"#)]
#[case::lone_cr_after_lt("<\r/-x", r#"text "<\r/-x" @1:1-2:4"#)]
#[case::crlf_after_lt("<\r\n/-x", r#"text "<\r\n/-x" @1:1-2:4"#)]
#[case::expression_lone_cr("{\r }", r#"expression_flow "\r" @1:1-2:3"#)]
#[case::expression_cr_indent("{a\r   b\r\n c\n  d}", r#"expression_flow "a\r b\r\nc\nd" @1:1-4:5"#)]
#[case::tilde_not_delimiter("*~*a", r#"text "*~*a" @1:1-1:5"#)]
#[case::tilde_strong_not_delimiter("**~**あ", r#"text "**~**あ" @1:1-1:9"#)]
#[case::dollar_after_name("<a$/>", "error: Unexpected character `$` (U+0024) after name")]
#[case::dollar_after_space("<a $b/>", "jsx_flow <a $b> @1:1-1:8")]
#[case::cjk_tag_name("<あ b=\"c\" />", r#"jsx_flow <あ b="c"> @1:1-1:14"#)]
#[case::fullwidth_underscore_name("<a＿b />", "jsx_flow <a＿b> @1:1-1:10")]
#[case::jsx_text_cjk(
    "あ <b>い</b> う",
    r#"
text "あ " @1:1-1:5
jsx_text <b> @1:5-1:15
  text "い" @1:8-1:11
text " う" @1:15-1:19
"#
)]
#[case::expression_cjk(
    "{あ} い",
    r#"
expression_text "あ" @1:1-1:6
text " い" @1:6-1:10
"#
)]
fn mdx(#[case] input: &str, #[case] expected: &str) {
    let actual = match Markdown::from_mdx_str(input) {
        Ok(markdown) => tree(&markdown.nodes),
        Err(error) => format!("error: {error}"),
    };
    assert_eq!(actual, expected.trim_matches('\n'), "{input:?}");
}
