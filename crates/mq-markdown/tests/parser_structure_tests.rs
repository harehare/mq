//! Parameterized tests for the nodes that the parser builds: their kinds and nesting, their
//! attributes, and where they are. Positions count lines from 1 and columns in bytes from 1, so the
//! multibyte cases show what a column is.

use mq_markdown::{AttrValue, Markdown, Node};
use rstest::rstest;

fn parse(input: &str) -> Vec<Node> {
    input.parse::<Markdown>().unwrap().nodes
}

/// The kinds of the nodes with their nesting, and the value of the leaves.
fn outline(nodes: &[Node]) -> String {
    fn one(node: &Node) -> String {
        let name = node.name();
        let children = node.children();
        if children.is_empty() {
            format!("{name}\"{}\"", node.value().replace('\n', "\\n"))
        } else {
            format!("{name}({})", children.iter().map(one).collect::<Vec<_>>().join(","))
        }
    }
    nodes.iter().map(one).collect::<Vec<_>>().join(" | ")
}

#[rstest]
#[case::paragraph("a", r#"text"a""#)]
#[case::paragraph_lines("a\nb", r#"text"a\nb""#)]
#[case::two_paragraphs("a\n\nb", r#"text"a" | text"b""#)]
#[case::heading("# a *b*", r#"h1(text"a ",emphasis(text"b"))"#)]
#[case::heading_depths("###### a", r#"h6(text"a")"#)]
#[case::setext("a\n===", r#"h1(text"a")"#)]
#[case::strong_in_emphasis("*a **b***", r#"emphasis(text"a ",strong(text"b"))"#)]
#[case::strikethrough("~~a~~", r#"delete(text"a")"#)]
#[case::code_span("`a`", r#"code_inline"a""#)]
#[case::math_inline("$a$", r#"math_inline"a""#)]
#[case::hard_break("a  \nb", r#"text"a" | break"" | text"b""#)]
#[case::inline_html("<b>x</b>", r#"html"<b>" | text"x" | html"</b>""#)]
#[case::html_block("<div>\nx\n</div>", r#"html"<div>\nx\n</div>""#)]
#[case::link("[a *b*](c)", r#"link(text"a ",emphasis(text"b"))"#)]
#[case::link_reference("[a][b]\n\n[b]: /u", r#"link_ref"b" | definition"/u""#)]
#[case::image("![a](b)", r#"image"b""#)]
#[case::autolink("<http://a.b>", r#"link(text"http://a.b")"#)]
#[case::autolink_literal("see www.a.com.", r#"text"see " | link(text"www.a.com") | text".""#)]
#[case::quote("> a\n> b", r#"blockquote(text"a\nb")"#)]
#[case::quote_nested("> > a", r#"blockquote(blockquote(text"a"))"#)]
#[case::quote_with_blocks("> # a\n> b", r#"blockquote(h1(text"a"),text"b")"#)]
#[case::code("```rs\nx\n```", r#"code"x""#)]
#[case::indented_code("    x", r#"code"x""#)]
#[case::math("$$\nx\n$$", r#"math"x""#)]
#[case::horizontal_rule("***", r#"Horizontal_rule"""#)]
#[case::frontmatter("---\na: 1\n---\n# b", r#"yaml"a: 1" | h1(text"b")"#)]
#[case::toml_frontmatter("+++\na = 1\n+++", r#"toml"a = 1""#)]
#[case::footnote("a[^1]\n\n[^1]: b", r#"text"a" | footnoteref"1" | footnote(text"b")"#)]
#[case::footnote_without_definition("a[^1]", r#"text"a[^1]""#)]
#[case::list_is_flat("- a\n  - b\n- c", r#"list(text"a") | list(text"b") | list(text"c")"#)]
#[case::list_item_with_blocks("- a\n\n  b", r#"list(text"a",text"b")"#)]
#[case::table(
    "| a | b |\n|:-|-:|\n| c |",
    r#"table_cell(text"a") | table_cell(text"b") | table_align"" | table_cell(text"c")"#
)]
#[case::definition_only("[a]: /u", r#"definition"/u""#)]
#[case::empty("", "")]
#[case::blank_lines("\n\n \n", "")]
fn nodes(#[case] input: &str, #[case] expected: &str) {
    assert_eq!(outline(&parse(input)), expected);
}

fn attr(node: &Node, name: &str) -> Option<AttrValue> {
    node.attr(name)
}

/// The attributes of the last item.
#[rstest]
#[case::top("- a", 0, false, None)]
#[case::second_item("- a\n- b", 0, false, None)]
#[case::nested("- a\n  - b", 1, false, None)]
#[case::nested_twice("- a\n  - b\n    - c", 2, false, None)]
#[case::ordered("1. a", 0, true, None)]
#[case::checked("- [x] a", 0, false, Some(true))]
#[case::unchecked("- [ ] a", 0, false, Some(false))]
fn list_items(#[case] input: &str, #[case] level: i64, #[case] ordered: bool, #[case] checked: Option<bool>) {
    let nodes = parse(input);
    let item = nodes.last().unwrap();
    assert_eq!(attr(item, "level"), Some(AttrValue::Integer(level)));
    assert_eq!(attr(item, "ordered"), Some(AttrValue::Boolean(ordered)));
    assert_eq!(attr(item, "checked"), checked.map(AttrValue::Boolean));
}

#[rstest]
#[case::index_first("- a\n- b\n- c", 0, 0)]
#[case::index_second("- a\n- b\n- c", 1, 1)]
#[case::index_third("- a\n- b\n- c", 2, 2)]
fn list_item_index(#[case] input: &str, #[case] item: usize, #[case] index: i64) {
    assert_eq!(attr(&parse(input)[item], "index"), Some(AttrValue::Integer(index)));
}

#[rstest]
#[case::link("[a](b \"c\")", "url", "b")]
#[case::link_title("[a](b \"c\")", "title", "c")]
#[case::link_entity("[a](b&amp;c)", "url", "b&c")]
#[case::link_escape("[a](b\\)c)", "url", "b)c")]
#[case::link_angle("[a](<b c>)", "url", "b c")]
#[case::autolink("<http://a.b>", "url", "http://a.b")]
#[case::autolink_email("<a@b.c>", "url", "mailto:a@b.c")]
#[case::autolink_literal_www("www.a.com", "url", "http://www.a.com")]
#[case::image_url("![a *b*](c)", "url", "c")]
#[case::image_alt("![a *b*](c)", "alt", "a b")]
#[case::image_title("![a](b 'c')", "title", "c")]
#[case::code_lang("```rs x\ny\n```", "lang", "rs")]
#[case::code_meta("```rs x\ny\n```", "meta", "x")]
#[case::definition_url("[a]: <b> \"t\"", "url", "b")]
#[case::definition_title("[a]: b \"t\"", "title", "t")]
#[case::definition_ident("[ A  b ]: c", "ident", "a b")]
#[case::footnote_ident("a[^B]\n\n[^b]: c", "ident", "b")]
fn attributes(#[case] input: &str, #[case] name: &str, #[case] expected: &str) {
    let nodes = parse(input);
    let found = nodes
        .iter()
        .flat_map(|node| std::iter::once(node.clone()).chain(node.children()))
        .find_map(|node| node.attr(name))
        .unwrap_or_else(|| panic!("no `{name}` in {input:?}"));
    assert_eq!(found, AttrValue::String(expected.to_string()));
}

/// `(input, index of the node, start line, start column, end line, end column)`.
#[rstest]
#[case::paragraph("a", 0, (1, 1), (1, 2))]
#[case::paragraph_second_line("a\nbc", 0, (1, 1), (2, 3))]
#[case::second_paragraph("a\n\nb", 1, (3, 1), (3, 2))]
#[case::heading("## a", 0, (1, 1), (1, 5))]
#[case::quote("> a", 0, (1, 1), (1, 4))]
#[case::quote_second_line("> a\n> b", 0, (1, 1), (2, 4))]
#[case::code("```\nx\n```", 0, (1, 1), (3, 4))]
#[case::code_indented_fence("  ```\n  x\n  ```", 0, (1, 3), (3, 6))]
// The position of an item is the one of its content.
#[case::list_item("- a", 0, (1, 3), (1, 4))]
#[case::emphasis_position("a *b*", 1, (1, 3), (1, 6))]
#[case::crlf("a\r\nb", 0, (1, 1), (2, 2))]
#[case::cr("a\rb", 0, (1, 1), (2, 2))]
#[case::second_line_after_crlf("a\r\n\r\nb", 1, (3, 1), (3, 2))]
#[case::multibyte_paragraph("あ", 0, (1, 1), (1, 4))]
#[case::multibyte_second_node("あ\n\nい", 1, (3, 1), (3, 4))]
#[case::multibyte_emphasis("あ*い*う", 1, (1, 4), (1, 9))]
#[case::multibyte_link("あ[い](u)", 1, (1, 4), (1, 12))]
#[case::emoji("🎉 *a*", 1, (1, 6), (1, 9))]
#[case::combining_mark("e\u{301}*a*", 1, (1, 4), (1, 7))]
#[case::multibyte_heading("# 見出し", 0, (1, 1), (1, 12))]
#[case::multibyte_quote("> あ", 0, (1, 1), (1, 6))]
#[case::multibyte_list("- あ", 0, (1, 3), (1, 6))]
#[case::multibyte_code("```\nあ\n```", 0, (1, 1), (3, 4))]
#[case::multibyte_fence_info("```あ\nx\n```", 0, (1, 1), (3, 4))]
#[case::ideographic_space("あ\u{3000}い", 0, (1, 1), (1, 10))]
#[case::table_cell("| あ | い |\n|---|---|", 0, (1, 1), (1, 7))]
#[case::bom("\u{feff}# a", 0, (1, 4), (1, 7))]
#[case::after_footnote("a[^1]\n\n[^1]: b", 1, (1, 2), (1, 6))]
fn positions(#[case] input: &str, #[case] index: usize, #[case] start: (usize, usize), #[case] end: (usize, usize)) {
    let nodes = parse(input);
    let position = nodes[index]
        .position()
        .unwrap_or_else(|| panic!("no position in {nodes:?}"));
    assert_eq!(
        (
            (position.start.line, position.start.column),
            (position.end.line, position.end.column)
        ),
        (start, end),
        "{:?}",
        nodes[index]
    );
}

#[rstest]
#[case::trailing_space("| a | b |  \n|-|-|  \n| c | d |  \n")]
#[case::no_outer_pipes("a | b\n--|--\nd | e \n")]
fn table_rows_include_trailing_whitespace(#[case] input: &str) {
    for node in parse(input) {
        let position = node.position().unwrap();
        let line = input.lines().nth(position.end.line - 1).unwrap();
        assert!(position.end.column <= line.len() + 1, "{node:?}");
    }
    let last_cell = parse(input)
        .into_iter()
        .rfind(|node| node.name() == "table_cell")
        .unwrap();
    let end = last_cell.position().unwrap().end;
    assert_eq!(
        end.column,
        input.lines().nth(end.line - 1).unwrap().len() + 1,
        "{last_cell:?}"
    );
}

#[rstest]
#[case::bom_heading("\u{feff}# a", "h1")]
#[case::bom_frontmatter("\u{feff}---\na: b\n---", "yaml")]
#[case::bom_text("\u{feff}a", "text")]
fn byte_order_mark_is_not_content(#[case] input: &str, #[case] first: &str) {
    let nodes = parse(input);
    assert_eq!(nodes[0].name(), first);
    assert!(!outline(&nodes).contains('\u{feff}'));
}

#[rstest]
#[case::wwß("wwß x")]
#[case::www_combining("ww\u{301}.a.com")]
#[case::http_multibyte("http://あ.jp/パス ok")]
#[case::email_multibyte("あ@い.com")]
#[case::www_multibyte("www.あ.com/パス")]
#[case::www_emoji("www.🎉.com")]
#[case::emphasis_multibyte("*あ*。**い**です")]
#[case::underscore_accent("_é_ _ß_")]
#[case::fullwidth_markers("＊a＊ ＿b＿")]
#[case::table_multibyte("| あ | 🎉 |\n|-|-|\n| é | ß |")]
#[case::reference_multibyte("[あ]\n\n[あ]: /u")]
#[case::footnote_multibyte("あ[^い]\n\n[^い]: う")]
fn multibyte_input_parses(#[case] input: &str) {
    let nodes = parse(input);
    assert!(!nodes.is_empty());
    for node in &nodes {
        let position = node.position().unwrap();
        assert!(position.start.line >= 1 && position.start.column >= 1, "{node:?}");
    }
}

#[rstest]
#[case::emphasis_cjk("*あ*。", r#"emphasis(text"あ") | text"。""#)]
#[case::strong_cjk("**日本語**です", r#"strong(text"日本語") | text"です""#)]
#[case::emphasis_between_cjk("あ*い*う", r#"text"あ" | emphasis(text"い") | text"う""#)]
#[case::underscore_intraword_cjk("あ_い_う", r#"text"あ_い_う""#)]
#[case::emphasis_next_to_cjk_punctuation("。*a*、", r#"text"。" | emphasis(text"a") | text"、""#)]
#[case::emphasis_next_to_ideographic_space(
    "\u{3000}*a*\u{3000}",
    "text\"\u{3000}\" | emphasis(text\"a\") | text\"\u{3000}\""
)]
#[case::emphasis_next_to_nbsp("a\u{a0}*b*\u{a0}c", "text\"a\u{a0}\" | emphasis(text\"b\") | text\"\u{a0}c\"")]
#[case::code_span_multibyte("`あ`と``い`う``", r#"code_inline"あ" | text"と" | code_inline"い`う""#)]
#[case::hard_break_multibyte("あ  \nい", r#"text"あ" | break"" | text"い""#)]
#[case::entity_multibyte("あ&amp;い&#12354;", r#"text"あ&いあ""#)]
#[case::emoji_zwj(
    "👨\u{200d}👩\u{200d}👧 *a*",
    "text\"👨\u{200d}👩\u{200d}👧 \" | emphasis(text\"a\")"
)]
fn multibyte_inline_syntax(#[case] input: &str, #[case] expected: &str) {
    assert_eq!(outline(&parse(input)), expected);
}

#[rstest]
#[case::paragraph_then_paragraph("a\n\nb", 2)]
#[case::heading_then_paragraph("# a\nb", 2)]
#[case::definition_between("a\n\n[b]: c\n\nd", 2)]
fn blocks_are_separate_nodes(#[case] input: &str, #[case] count: usize) {
    let nodes = parse(input)
        .into_iter()
        .filter(|node| node.name() != "definition")
        .count();
    assert_eq!(nodes, count);
}
