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
#[case::task_after_tab("- [ ]\tfoo", 0, (1, 9), (1, 12))]
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

/// Nodes whose positions overlap, as the items of a list inside a quote, must not break rendering.
#[rstest]
#[case::nested_list_in_quote("> - - {x}\n>   }\n/>\n")]
fn rendering_overlapping_positions_does_not_panic(#[case] input: &str) {
    let markdown = Markdown::from_markdown_str(input).unwrap();
    assert!(!markdown.to_string().is_empty());
}

/// A thematic rule of dashes at the start would turn into frontmatter with the next one.
#[rstest]
#[case::dashes_at_start("-----\n\ntext\n\n-----\n\nmore\n")]
fn rendering_dashes_at_start_keeps_the_rules(#[case] input: &str) {
    let rendered = Markdown::from_markdown_str(input).unwrap().to_string();
    let nodes = Markdown::from_markdown_str(&rendered).unwrap().nodes;
    let rules = |nodes: &[Node]| nodes.iter().filter(|n| matches!(n, Node::HorizontalRule(_))).count();
    assert_eq!(
        rules(&nodes),
        rules(&Markdown::from_markdown_str(input).unwrap().nodes),
        "{rendered:?}"
    );
}

/// What `to_string` writes must read back as the same document, in HTML.
#[rstest]
#[case::url_with_underscores("see https://a.b/c_d_e/ end\n")]
#[case::nested_list_in_task_item("- [ ] one\n  - sub\n- two\n")]
#[case::paragraph_in_task_item("- [x] one\n\n  more\n- two\n")]
#[case::reference_label_with_spaces("see [Code of Conduct].\n\n[Code of Conduct]: https://a.b/c\n")]
#[case::reference_with_code_text("type [`Options`][options].\n\n[options]: #options\n")]
#[case::table_code_with_pipe("| a | b |\n|---|---|\n| `x\\|y` | z\\|w |\n")]
#[case::link_text_is_a_url("[www.W3.org](http://www.w3.org) site\n")]
#[case::strong_with_emphasis("**_NOTE:_** text\n")]
#[case::second_paragraph_after_escape("- \\[a] x\n\n  b\n")]
#[case::table_in_quote("> | a | b |\n> |---|---|\n> | 1 | 2 |\n")]
#[case::table_in_list_item("- **P:**\n  | Name | Type |\n  |------|------|\n  | a | b |\n- next\n")]
#[case::table_row_with_more_cells("| a | b |\n|---|---|\n| 1 | 2 | 3 |\n")]
#[case::paragraph_after_footnote("[^1]: Note text.\n\nParagraph after.\n")]
#[case::list_in_empty_item("- - a\n  - b\n")]
#[case::empty_code_with_blank_line("```\n\n```\n")]
#[case::paragraph_after_list_in_empty_item("- - a\n\n  Own Id: X\n")]
#[case::paragraph_after_nested_list("- a\n  - s\n\n  b\n")]
#[case::code_after_nested_list("- a\n  - s\n  ```\n  code\n  ```\n")]
#[case::nested_lists_around_paragraph("- a\n  - s\n\n  b\n  - t\n")]
#[case::ordered_item_with_nested_list_and_paragraph("1. x\n   - s\n\n   para\n2. y\n")]
#[case::escaped_reference("\\&ouml; x\n")]
#[case::escaped_numeric_reference("\\&#35; x \\&#x23; y\n")]
#[case::ampersand_without_a_reference("a & b &c d &amp\n")]
#[case::bang_before_a_link("\\![foo]\n\n[foo]: /url\n")]
#[case::bang_before_an_inline_link("wow\\![a](b)\n")]
#[case::bang_before_a_full_reference("\\![a][foo]\n\n[foo]: /url\n")]
#[case::escaped_email("<foo\\+@bar.example.com>\n")]
#[case::escaped_email_domain("<a\\_b@c.de> x\n")]
#[case::heading_with_a_trailing_line_ending("# a&#10;\n")]
#[case::heading_with_a_leading_line_ending("# &#10;a\n")]
#[case::heading_with_a_line_ending_inside("# a&#10;b\n")]
#[case::link_text_that_looks_like_a_url("[https://a.com/x\\_y](u)\n")]
#[case::link_text_with_a_reference_in_a_url("[https://a.com/x&amp;y](u)\n")]
#[case::code_ending_in_whitespace("    brew install x\n     \n# h\n")]
fn rendering_reads_back_as_the_same_html(#[case] input: &str) {
    let rendered = Markdown::from_markdown_str(input).unwrap().to_string();
    assert_eq!(
        mq_markdown::to_html(&rendered),
        mq_markdown::to_html(input),
        "{rendered:?}"
    );
}

#[rstest]
#[case::escape("[https://a.com/x\\_y](u)\n", "https://a.com/x_y")]
#[case::reference("[https://a.com/x&amp;y](u)\n", "https://a.com/x&y")]
#[case::www("[www.a.com/x\\_y](u)\n", "www.a.com/x_y")]
fn link_text_that_looks_like_a_url_is_decoded(#[case] input: &str, #[case] expected: &str) {
    let nodes = Markdown::from_markdown_str(input).unwrap().nodes;
    assert_eq!(nodes[0].children()[0].value(), expected, "{nodes:?}");
}

#[test]
fn indented_code_drops_trailing_lines_of_whitespace() {
    let nodes = Markdown::from_markdown_str("    brew install x\n     \n\t\n# h\n")
        .unwrap()
        .nodes;
    assert_eq!(nodes[0].value(), "brew install x");
}

#[rstest]
#[case::line_endings_at_the_ends("``\nfoo\n``\n", "foo")]
#[case::line_ending_and_space("`\nfoo `\n", "foo")]
#[case::inner_line_endings_stay("``\nfoo\nbar  \nbaz\n``\n", "foo\nbar  \nbaz")]
#[case::only_spaces_and_line_endings("` \n`\n", " \n")]
fn code_span_strips_one_space_or_line_ending(#[case] input: &str, #[case] expected: &str) {
    let nodes = Markdown::from_markdown_str(input).unwrap().nodes;
    assert_eq!(nodes[0].value(), expected, "{nodes:?}");
}

#[rstest]
#[case::title_lines_lose_their_indent("[a](b \"\n c\")\n", "\nc")]
#[case::definition_title("[a]\n\n[a]: b \"\n  c\"\n", "\nc")]
fn titles_across_lines_lose_the_indent_of_the_lines(#[case] input: &str, #[case] expected: &str) {
    let nodes = Markdown::from_markdown_str(input).unwrap().nodes;
    let title = nodes.iter().find_map(|node| match node {
        Node::Link(link) => link.title.as_ref().map(|title| title.to_string()),
        Node::Definition(definition) => definition.title.as_ref().map(|title| title.to_string()),
        _ => None,
    });
    assert_eq!(title.as_deref(), Some(expected), "{nodes:?}");
}

#[rstest]
#[case::tab_after_an_indented_fence(" ```\n\tx", "   x")]
#[case::tab_after_a_deeper_fence("  ```\n\tx", "  x")]
#[case::spaces_after_an_item_marker("- ```\n   \n  ```", " ")]
#[case::tab_in_an_item("- ```\n\t\n  ```", "  ")]
fn fenced_code_keeps_the_whitespace_left_after_the_indent(#[case] input: &str, #[case] expected: &str) {
    let nodes = Markdown::from_markdown_str(input).unwrap().nodes;
    let code = match &nodes[0] {
        Node::List(list) => &list.values[0],
        node => node,
    };
    assert_eq!(code.value(), expected, "{nodes:?}");
}

#[rstest]
#[case::in_a_paragraph("a <b>x\nc</b>", "x\nc")]
#[case::after_the_tag("a <b>\nc</b> d", "\nc")]
#[case::before_the_closing_tag("a <b>c\n</b> d", "c\n")]
#[case::in_a_quote("> a <b>\n> c </b> d.", "\nc ")]
fn line_endings_in_text_elements_are_kept(#[case] input: &str, #[case] expected: &str) {
    let nodes = Markdown::from_mdx_str(input).unwrap().nodes;
    let children = nodes
        .iter()
        .flat_map(|node| match node {
            Node::Blockquote(quote) => quote.values.clone(),
            node => vec![node.clone()],
        })
        .find_map(|node| match node {
            Node::MdxJsxTextElement(element) => Some(element.children),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        children.iter().map(Node::value).collect::<String>(),
        expected,
        "{nodes:?}"
    );
}

/// What `to_string` writes for these reads back as the same nodes.
#[rstest]
#[case::math_with_a_dollar("$$ foo $ bar $$\n", "$$foo $ bar$$")]
#[case::math_of_dollars("$ $$ $\n", "$ $$ $")]
#[case::math_block_with_a_fence_inside("$$$\naaa\n$$\n$$$$\n", "$$$\naaa\n$$\n$$$")]
#[case::definition_with_an_escape("[+]: a\n[\\;]: b\n", "[+]: a\n[\\;]: b")]
#[case::definition_with_a_reference("[&amp;]: a\n\n[&amp;]\n", "[&amp;]: a\n\n[&][&amp;]")]
#[case::footnote_with_a_reference("Call.[^a&copy;b].\n\n[^a&copy;b]: y\n", "Call.[^a&copy;b].\n\n[^a&copy;b]: y")]
#[case::table_after_a_footnote(
    "[^f0]: a\n\n| 1 | 2 |\n|-|-|\n| a | b |\n",
    "[^f0]: a\n\n| 1 | 2 |\n| - | - |\n| a | b |"
)]
#[case::table_after_a_quote("> a\n\n| 1 | 2 |\n|-|-|\n| a | b |\n", "> a\n\n| 1 | 2 |\n| - | - |\n| a | b |")]
#[case::emphasis_after_a_letter_in_strong("**a*b c***\n", "**a*b c***")]
#[case::emphasis_before_a_letter_in_strong("***a b*c**\n", "***a b*c**")]
#[case::emphasis_at_an_end_of_strong("**a *b***\n", "**a _b_**")]
#[case::email_that_is_not_an_autolink("<asd@-example.com>\n", "\\<[asd\\@-example.com](mailto:asd@-example.com)\\>")]
fn writing_keeps_what_is_read_back(#[case] input: &str, #[case] expected: &str) {
    let markdown = Markdown::from_markdown_str(input).unwrap();
    let written = markdown.to_string();
    assert_eq!(written.trim_end(), expected, "{:?}", markdown.nodes);
    let reparsed = Markdown::from_markdown_str(&written).unwrap();
    assert_eq!(
        mq_markdown::to_html(&written),
        mq_markdown::to_html(input),
        "{written:?} {:?}",
        reparsed.nodes
    );
}

#[rstest]
#[case::lone_pipe_header("|\n|-|\n|d|\n", false)]
#[case::empty_cell_header("||\n|-|\n|d|\n", true)]
#[case::space_cell_header("| |\n|-|\n|d|\n", true)]
fn a_header_of_a_lone_pipe_has_no_cells(#[case] input: &str, #[case] table: bool) {
    let nodes = Markdown::from_markdown_str(input).unwrap().nodes;
    assert_eq!(
        nodes.iter().any(|node| matches!(node, Node::TableCell(_))),
        table,
        "{nodes:?}"
    );
}

#[rstest]
#[case::next_line("- [ ]\n  a\n", Some(false))]
#[case::checked_next_line("* [x]\r\n  a\n", Some(true))]
#[case::alone("- [ ]\n", None)]
#[case::before_a_blank_line("- [ ]\n\npara\n", None)]
#[case::before_an_item("- [x]\n- b\n", None)]
#[case::with_a_space("- [x] a\n", Some(true))]
fn a_task_marker_is_followed_by_whitespace(#[case] input: &str, #[case] checked: Option<bool>) {
    let nodes = Markdown::from_markdown_str(input).unwrap().nodes;
    let Node::List(list) = &nodes[0] else {
        panic!("{nodes:?}")
    };
    assert_eq!(list.checked, checked, "{nodes:?}");
}

#[test]
fn mdx_attribute_references_of_any_length_are_decoded() {
    let markdown = Markdown::from_mdx_str("<a b='&#987654321; &#x110000; &#65;' />").unwrap();
    assert_eq!(markdown.to_string().trim_end(), "<a b=\"\u{FFFD} \u{FFFD} A\" />");
}

#[test]
fn mdx_expression_keeps_what_is_left_of_a_tab_at_the_start_of_a_line() {
    let nodes = Markdown::from_mdx_str("{`\n\t`}").unwrap().nodes;
    assert!(
        matches!(&nodes[..], [Node::MdxFlowExpression(e)] if e.value == "`\n  `"),
        "{nodes:?}"
    );
}

#[rstest]
#[case::bracket_open_over_blank_lines("export {\n\n  a\n\n} from 'b'\n\nc", 2)]
#[case::paren_open("import (\n\n'a'\n\n)\n\nb", 2)]
#[case::string_with_a_bracket("import a from \"{\"\n\nb", 2)]
#[case::closed("import a from 'b'\n\nc", 2)]
fn mdx_esm_goes_on_while_a_bracket_is_open(#[case] input: &str, #[case] blocks: usize) {
    let nodes = Markdown::from_mdx_str(input).unwrap().nodes;
    assert_eq!(nodes.len(), blocks, "{nodes:?}");
    assert!(matches!(nodes[0], Node::MdxJsEsm(_)), "{nodes:?}");
    assert!(
        matches!(&nodes[1], Node::Text(t) if t.value.trim().len() == 1),
        "{nodes:?}"
    );
}

#[rstest]
#[case::strikethrough_in_emphasis("a*~b~*c\n", "<p>a<em><del>b</del></em>c</p>\n")]
#[case::strikethrough_not_next_to_a_word("a ~*b*~ c\n", "<p>a <del><em>b</em></del> c</p>\n")]
#[case::tildes_in_a_word("a~*b*~c\n", "<p>a~<em>b</em>~c</p>\n")]
#[case::strong_around_strikethrough("a**~b~**c\n", "<p>a<strong><del>b</del></strong>c</p>\n")]
#[case::underscore_next_to_tildes("a_~b~_c\n", "<p>a_<del>b</del>_c</p>\n")]
#[case::punctuation_that_is_not_a_tilde("a*.b.*c\n", "<p>a*.b.*c</p>\n")]
fn emphasis_next_to_strikethrough(#[case] input: &str, #[case] expected: &str) {
    assert_eq!(mq_markdown::to_html(input), expected);
}

#[rstest]
#[case::more_spaces("- [ ]   b\n", "  b")]
#[case::one_space("- [ ] b\n", "b")]
#[case::tab("- [x]\tb\n", "b")]
#[case::next_line("- [ ]\n  b\n", "b")]
#[case::space_and_next_line("- [ ] \n  b\n", "b")]
#[case::crlf("* [x]\r\n  a\n", "a")]
fn a_task_marker_takes_one_whitespace_character(#[case] input: &str, #[case] expected: &str) {
    let nodes = Markdown::from_markdown_str(input).unwrap().nodes;
    let Node::List(list) = &nodes[0] else {
        panic!("{nodes:?}")
    };
    assert_eq!(
        list.values.iter().map(Node::value).collect::<String>(),
        expected,
        "{nodes:?}"
    );
}

/// References keep the notation they were written in, as long as it resolves to the same definition.
#[rstest]
#[case::full_keeps_case_and_spaces("[x][Foo   Bar]\n\n[foo bar]: /u\n", "[x][Foo   Bar]")]
#[case::full_image("![x][Foo   Bar]\n\n[foo bar]: /u\n", "![x][Foo   Bar]")]
#[case::full_escaped_label("[x][a\\]b]\n\n[a\\]b]: /u\n", "[x][a\\]b]")]
#[case::shortcut_sharp_s("[Straße]\n\n[strasse]: /u\n", "[Straße]")]
#[case::shortcut_capital_sharp_s("[ẞ]\n\n[ss]: /u\n", "[ẞ]")]
#[case::collapsed("[Straße][]\n\n[strasse]: /u\n", "[Straße]")]
fn references_are_written_as_they_were(#[case] input: &str, #[case] expected: &str) {
    let rendered = Markdown::from_markdown_str(input).unwrap().to_string();
    assert!(rendered.starts_with(expected), "{rendered:?}");
    assert_eq!(
        mq_markdown::to_html(&rendered),
        mq_markdown::to_html(input),
        "{rendered:?}"
    );
}

/// Braces in strings, template literals and comments do not end an expression.
#[rstest]
#[case::string("{'}'}", "'}'")]
#[case::double_quoted("{\"}\" + '{'}", "\"}\" + '{'")]
#[case::escaped_quote("{'\\'}'}", "'\\'}'")]
#[case::block_comment("{a /* } */}", "a /* } */")]
#[case::line_comment("{a // }\n}", "a // }\n")]
#[case::template("{`a${`}`}c`}", "`a${`}`}c`")]
#[case::division("{a / b}", "a / b")]
fn mdx_expression_braces(#[case] input: &str, #[case] expected: &str) {
    let nodes = Markdown::from_mdx_str(input).unwrap().nodes;
    assert!(
        matches!(&nodes[..], [Node::MdxFlowExpression(expression)] if expression.value == expected),
        "{nodes:?}"
    );
}

#[test]
fn mdx_attribute_expression_braces() {
    let nodes = Markdown::from_mdx_str("<a b={'}'} />").unwrap().nodes;
    assert!(matches!(&nodes[..], [Node::MdxJsxFlowElement(_)]), "{nodes:?}");
}

/// What an MDX document starts its blocks with: `import` and `export` lines are one block up to the
/// next blank line.
#[rstest]
#[case::import_and_export("import a from \"b\"\nexport const x = 1\n\nx\n", &["esm:import a from \"b\"\nexport const x = 1", "text:x"])]
#[case::export_default("export default d\n", &["esm:export default d"])]
#[case::indented_is_text("  import a from \"b\"\n", &["text:import a from \"b\""])]
#[case::no_space_is_text("import{a} from \"b\"\n", &["text:import", "expr:a", "text: from \"b\""])]
#[case::a_word_is_text("importx a\n", &["text:importx a"])]
#[case::in_a_list_is_text("- import a from \"b\"\n", &["item"])]
#[case::does_not_interrupt_a_paragraph("text\nimport a from \"b\"\n", &["text:text\nimport a from \"b\""])]
fn mdx_esm(#[case] input: &str, #[case] expected: &[&str]) {
    let nodes = Markdown::from_mdx_str(input).unwrap().nodes;
    let described = nodes
        .iter()
        .map(|node| match node {
            Node::MdxJsEsm(esm) => format!("esm:{}", esm.value),
            Node::MdxTextExpression(expression) => format!("expr:{}", expression.value),
            Node::Text(text) => format!("text:{}", text.value),
            Node::List(_) => "item".to_string(),
            other => format!("{other:?}"),
        })
        .collect::<Vec<_>>();
    assert_eq!(described, expected);
}

/// Without MDX, `import` is text.
#[test]
fn import_is_text_in_markdown() {
    let nodes = Markdown::from_markdown_str("import a from \"b\"\n").unwrap().nodes;
    assert!(matches!(nodes.as_slice(), [Node::Text(_)]));
}
