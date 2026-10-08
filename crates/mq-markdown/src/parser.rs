//! Native Markdown parser that builds [`Node`] values directly, without going through `mdast`.
//!
//! Parsing runs in phases: [`block`] resolves the block structure line by line into a tree,
//! [`resolve`] collects the definitions and turns the tree into nodes, and [`inline`] parses the raw
//! text of paragraphs, headings and table cells along the way.
//!
//! The output, positions included, is the same as that of `markdown-rs` for CommonMark, GFM, frontmatter,
//! math and MDX (without a JavaScript parser, so expressions only need balanced braces). This holds for
//! every example of the CommonMark and GFM specifications, and for randomly generated documents, except:
//!
//! - lazy continuation lines after definitions, footnotes or thematic breaks in containers, in containers
//!   nested more than two deep, or with several container markers on a line, and unclosed fences in
//!   containers whose end depends on the line that follows
//! - documents that start with a `---` or `+++` line that never closes, and inputs on which
//!   `markdown-rs` panics
//! - a few tab quirks of `markdown-rs`: a tab that a container only partly consumes keeps its rest in
//!   text values
//! - character references in an image destination that contains an email address
//! - MDX text expressions and tags that span lines interrupted by container markers
mod block;
mod code;
mod definition;
mod html_flow;
mod inline;
mod line;
mod mdx;
mod mdx_flow;
mod resolve;
mod table;
mod tree;

use crate::node::Node;

/// Parses `content` into a flat list of nodes.
pub(crate) fn parse(content: &str) -> miette::Result<Vec<Node>> {
    resolve::resolve(block::parse(content, false), false).map_err(|message| miette::miette!(message))
}

/// Parses `content` as MDX: no indented code, HTML, autolinks or GFM, but expressions and JSX.
pub(crate) fn parse_mdx(content: &str) -> miette::Result<Vec<Node>> {
    resolve::resolve(block::parse(content, true), true).map_err(|message| miette::miette!(message))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markdown::{parse_mdx_with_markdown_rs, parse_with_markdown_rs};
    use proptest::prelude::*;
    use proptest::test_runner::{RngAlgorithm, TestRng, TestRunner};
    use rstest::rstest;

    /// `Node`'s `PartialEq` compares rendered output, so compare `Debug` output to include positions.
    fn assert_same_as_markdown_rs(input: &str) {
        let expected = parse_with_markdown_rs(input).unwrap();
        let actual = parse(input).unwrap();
        assert_eq!(format!("{actual:#?}"), format!("{expected:#?}"), "input: {input:?}");
    }

    /// Compares the MDX parsers. Invalid MDX has to fail in both.
    fn assert_same_mdx(input: &str) {
        let expected = parse_mdx_with_markdown_rs(input);
        let actual = parse_mdx(input);
        match (expected, actual) {
            (Ok(expected), Ok(actual)) => {
                assert_eq!(format!("{actual:#?}"), format!("{expected:#?}"), "input: {input:?}");
            }
            (Err(_), Err(_)) => {}
            (expected, actual) => panic!("input: {input:?}\n markdown-rs: {expected:?}\n native: {actual:?}"),
        }
    }

    #[rstest]
    #[case::jsx_self("<a />")]
    #[case::jsx_attrs("<a b=\"c&amp;d\" e='f' g={h} {...i} j />")]
    #[case::jsx_member("<a.b.c />")]
    #[case::jsx_namespace("<a:b c:d=\"e\" />")]
    #[case::jsx_fragment("<></>")]
    #[case::jsx_text_pair("<a>x</a>")]
    #[case::jsx_flow_pair("<a>\n\nx\n\n</a>")]
    #[case::jsx_flow_indented_child("<a>\n  x\n</a>")]
    #[case::jsx_inline("a <b>c</b> d")]
    #[case::jsx_inline_self("a <b/> d")]
    #[case::expr_flow("{a}")]
    #[case::expr_nested("{a {b} c}")]
    #[case::expr_multiline("{a\nb}")]
    #[case::expr_multiline_indented("{a\n  b\n   c}")]
    #[case::expr_text("x {a} y")]
    #[case::expr_then_tag("{a} <b/>")]
    #[case::tag_then_expr("<b/> {a}")]
    #[case::tag_then_text("<b/> x")]
    #[case::expr_then_text("{a}x")]
    #[case::jsx_in_expr_child("<a>{b}</a>")]
    #[case::jsx_unclosed_text("<a>b")]
    #[case::jsx_mismatch("<a></b>")]
    #[case::lt_space("a < b")]
    #[case::lt_digit("a <3")]
    #[case::attr_no_value("<a b=>")]
    #[case::jsx_nested_flow("<a>\n<b>\n</b>\n</a>")]
    #[case::jsx_in_quote("> <a>\n> x\n> </a>")]
    #[case::jsx_in_list("- <a>\n  x\n  </a>")]
    #[case::attr_quote_inside("<a b='c\"d' />")]
    #[case::two_tags("<a/><b/>")]
    #[case::two_text_elements("<a>x</a><b>y</b>")]
    #[case::interrupt_paragraph_tag("a\n<b/>")]
    #[case::interrupt_paragraph_expr("a\n{b}")]
    #[case::two_expressions("{a}\n{b}")]
    #[case::expr_in_flow_element("<a>\n\n{b}\n\n</a>")]
    #[case::indented_tag("   <a/>")]
    #[case::very_indented_tag("    <a/>")]
    #[case::heading_jsx("# h <a>b</a>")]
    #[case::emphasis_crossing("*<a>x</a>*")]
    #[case::emphasis_crossing_bad("*<a>x*</a>")]
    #[case::link_jsx("[<a>x</a>](y)")]
    #[case::code_not_jsx("`<a/>` {b}")]
    #[case::escaped("\\<a/> \\{b}")]
    #[case::attr_entities("<a b=\"&lt;&#x41;\" />")]
    #[case::attr_multiline_literal("<a b='x\ny' />")]
    #[case::closing_spaces("<a  >x</a  >")]
    #[case::lt_space_name("< a>")]
    #[case::self_closing_space("<a / >")]
    #[case::attr_on_next_line("<a\nb />")]
    #[case::dashed_name("<a-b />")]
    #[case::dashed_attr("<a b-c=\"d\" />")]
    #[case::bad_name_char("<A_b$.c-d />")]
    #[case::expr_unclosed("{a")]
    #[case::expr_empty("{}")]
    #[case::expr_space("{ }")]
    #[case::empty_flow_element("<a>\n</a>")]
    #[case::text_element_unclosed_line("<a>x\n</a>")]
    #[case::stray_close("</a>")]
    #[case::stray_close_text("x </a>")]
    #[case::element_with_emphasis("<a>*b*</a>")]
    #[case::flow_element_with_emphasis("<a>\n*b*\n</a>")]
    #[case::flow_element_with_list("<a>\n- b\n</a>")]
    #[case::list_items_unbalanced("- <a>\n- </a>")]
    #[case::spaced_equals("<a b = \"c\" />")]
    #[case::heading("# h\n\ntext *a* [b](c) `d`\n")]
    #[case::indented_is_text("    not code\n")]
    #[case::indented_heading("     # h\n")]
    #[case::no_autolink("<http://a.b>")]
    #[case::no_gfm("| a |\n|-|\n\n~a~ www.a.b [^a]\n\n- [ ] a\n")]
    #[case::no_math("$a$\n\n$$\na\n$$\n")]
    #[case::no_frontmatter("---\na\n---\n")]
    #[case::esm_is_text("import a from 'b'\n\nexport const c = 1\n")]
    #[case::definition("[a]: /u\n\n[a]\n")]
    #[case::list_deep_indent("-      a\n")]
    #[case::fence("```rust\ncode\n```\n")]
    fn mdx_matches_markdown_rs(#[case] input: &str) {
        assert_same_mdx(input);
    }

    #[rstest]
    #[case::empty("")]
    #[case::blank_lines("\n\n  \n")]
    #[case::paragraph("hello\n")]
    #[case::paragraph_no_eol("hello")]
    #[case::paragraph_multiline("a\nb\nc\n")]
    #[case::paragraph_indented_continuation("a\n    b\n")]
    #[case::paragraph_trailing_space("あい\n  うえ  \n")]
    #[case::two_paragraphs("a\n\nb\n")]
    #[case::crlf("a\r\nb\r\n\r\nc\r\n")]
    #[case::crlf_fence("```\r\na\r\nb\r\n```\r\n")]
    #[case::cr_only("a\rb\r")]
    #[case::atx_h1("# title\n")]
    #[case::atx_h6("###### title\n")]
    #[case::atx_seven("####### title\n")]
    #[case::atx_no_space("#title\n")]
    #[case::atx_empty("#\n")]
    #[case::atx_closing("## title ##\n")]
    #[case::atx_closing_no_space("## title##\n")]
    #[case::atx_indent("  ## title ##  \n")]
    #[case::atx_multibyte("# あ h #\n")]
    #[case::atx_interrupts_paragraph("a\n# b\n")]
    #[case::setext_h1("Title\n===\n")]
    #[case::setext_h2("a\n  ---\n")]
    #[case::setext_multiline("a\nb\n---\n")]
    #[case::thematic_star("***\n")]
    #[case::thematic_dash_spaced("- - -\n")]
    #[case::thematic_underscore("  ___  \n")]
    #[case::thematic_two("**\n")]
    #[case::fence_lang_meta("```rust title\nlet a;\n```\n")]
    #[case::fence_indented("  ```\n  a\n   b\n  ```\n")]
    #[case::fence_tilde("~~~\na\n~~~\n")]
    #[case::fence_unclosed("```\na\n")]
    #[case::fence_empty("para\n\n\n```\n```\n")]
    #[case::fence_longer_close("```\na\n`````\n")]
    #[case::fence_shorter_close("````\na\n```\nb\n````\n")]
    #[case::fence_blank_inside("```\na\n\nb\n```\n")]
    #[case::fence_backtick_info("``` a`b\nc\n")]
    #[case::fence_interrupts_paragraph("a\n```\nb\n```\n")]
    #[case::indented_code("    code\n\n      more\n\nx\n")]
    #[case::quote("> a\n> b\n")]
    #[case::quote_indented_lazy("  > a\nb\n")]
    #[case::quote_heading("> # h\n>\n> c\n")]
    #[case::quote_no_space(">a\n")]
    #[case::quote_empty(">\n")]
    #[case::quote_nested("> > a\n> b\n")]
    #[case::quote_lazy_nested("> > a\nb\n")]
    #[case::quote_blank_ends("> a\n\n> b\n")]
    #[case::quote_fence("> ```\n> a\n> ```\n")]
    #[case::quote_no_lazy_after_fence("> ```\n> a\nb\n")]
    #[case::quote_interrupts_paragraph("a\n> b\n")]
    #[case::quote_thematic_not_lazy("> a\n---\n")]
    #[case::quote_multibyte("> あ\n> い\n")]
    #[case::bullet("- a\n- b\n")]
    #[case::bullet_loose("- a\n\n- b\n")]
    #[case::bullet_indent(" - a\n")]
    #[case::bullet_mixed_markers("* a\n+ b\n")]
    #[case::ordered("1. a\n2. b\n")]
    #[case::ordered_paren("5) a\n")]
    #[case::ordered_zero("0. a\n")]
    #[case::ordered_too_long("1234567890. a\n")]
    #[case::ordered_mixed_delimiters("1. a\n2) b\n")]
    #[case::task("- [ ] x\n- [x] y\n- [X] z\n")]
    #[case::task_empty("- [ ]\n")]
    #[case::task_no_space("- [ ]x\n")]
    #[case::nested("- a\n  - b\n    c\n")]
    #[case::nested_ordered("1. a\n   1. b\n   2. c\n2. d\n")]
    #[case::nested_dedent("- a\n  - b\n- c\n")]
    #[case::two_paragraphs("- a\n\n  b\n")]
    #[case::two_paragraphs_then_item("- a\n\n  b\n- c\n")]
    #[case::empty_item("-\n")]
    #[case::empty_item_content("-\n  foo\n")]
    #[case::empty_item_blank_then_text("-\n\n  foo\n")]
    #[case::empty_items("-\n-\n")]
    #[case::wide_marker_gap("-   a\n    b\n")]
    #[case::code_in_item("-     a\n")]
    #[case::lazy("- a\nb\n")]
    #[case::lazy_after_blank("- a\n\nb\n")]
    #[case::list_interrupts_paragraph("a\n- b\n")]
    #[case::ordered_two_no_interrupt("a\n2. b\n")]
    #[case::ordered_one_interrupts("a\n1. b\n")]
    #[case::empty_item_no_interrupt("a\n-\n")]
    #[case::thematic_over_list("- - -\n")]
    #[case::list_in_quote("> - a\n> - b\n")]
    #[case::quote_in_list("- > a\n  > b\n")]
    #[case::quote_in_list_level("- a\n  > - b\n")]
    #[case::heading_in_item("- # h\n  text\n")]
    #[case::fence_in_item("- ```\n  a\n  ```\n")]
    #[case::multibyte_item("- あ\n- い\n")]
    #[case::list_after_paragraph_blank("a\n\n- b\n")]
    #[case::two_lists_split("- a\n\n\n- b\n")]
    #[case::deep_quote(">>>>>> a\n")]
    #[case::fence_unclosed_no_eol("```\na")]
    #[case::fence_unclosed_in_item("- ```\n  a\n")]
    #[case::fence_unclosed_in_item_no_eol("- ```\n  a")]
    #[case::fence_unclosed_in_quote_no_eol("> ```\n> a")]
    #[case::fence_unclosed_in_quote_eof("> ```\n> a\n")]
    #[case::fence_unclosed_trailing_blank("```\na\n\n")]
    #[case::fence_unclosed_item_then_para("- ```\n  a\nb\n")]
    #[case::item_code_then_unindented_text("- ===\n\n      a\n===")]
    #[case::quote_setext_dashes("> a\n> ---\n> ---\n> a")]
    #[case::table_basic("| a | b |\n|---|:-:|\n| 1 | 2 |\n")]
    #[case::table_no_edge_pipes("a|b\n-|-\n1|2|3\n")]
    #[case::table_short_row("|a|b|\n|-|-|\n|1|\n")]
    #[case::table_escaped_pipe("| a \\| b | c |\n|--|--|\n")]
    #[case::table_indented("  | a |\n  |---|\n  | b |\n")]
    #[case::table_text_after("| a |\n|---|\ntext after\n")]
    #[case::table_in_quote("> | a |\n> |---|\n> | b |\n")]
    #[case::table_after_paragraph("x\n| a |\n|---|\n")]
    #[case::table_column_mismatch("| a | b |\n|---|\n")]
    #[case::table_bad_delimiter("|a|\n|:|\n")]
    #[case::table_then_blank_para("|a|\n|-|\n\nnext\n")]
    #[case::table_then_quote("|a|\n|-|\n> q\n")]
    #[case::table_then_heading("|a|\n|-|\n# h\n")]
    #[case::table_then_fence("|a|\n|-|\n```\nx\n```\n")]
    #[case::table_then_list("|a|\n|-|\n- x\n")]
    #[case::table_two("|a|\n|-|\n\n|b|\n|-|\n")]
    #[case::table_align_spaces("|a|b|\n|:-|-:|\n|  x  |  y|\n")]
    #[case::table_indented_code_row("|a|\n|-|\n|b|\n    |c|\n")]
    #[case::table_no_body("| a |\n| - |\n")]
    #[case::table_empty_cells("||\n|-|\n")]
    #[case::table_lone_pipe_row("|a|\n|-|\n|\n")]
    #[case::table_no_trailing_pipe("| a | b\n|-|-\n| c | d\n")]
    #[case::table_multibyte("| あ | い |\n|---|---|\n| う | え |\n")]
    #[case::not_table_setext("Title\n---\n")]
    #[case::not_table_no_pipe_delim("a|b\n---\n")]
    #[case::table_header_no_pipe("a\n|-|\n")]
    #[case::table_one_col_dash("|a|\n-\n")]
    #[case::inline_escape("a\\*b")]
    #[case::inline_escape_nonpunct("a\\qb")]
    #[case::inline_entities("a&amp;b &copy; &#35; &#x41; &unknown; &")]
    #[case::inline_emphasis_all("a *b* **c** _d_ __e__ ***f***")]
    #[case::inline_code("`code` `` a`b `` ` a `")]
    #[case::inline_code_unclosed("`a ``b`")]
    #[case::inline_hard_break_spaces("a  \nb")]
    #[case::inline_hard_break_backslash("a\\\nb")]
    #[case::inline_soft_break("a\nb")]
    #[case::inline_soft_break_trailing_space("a \nb")]
    #[case::inline_emphasis_multiline("*a\nb*")]
    #[case::inline_strike("a ~b~ ~~c~~ ~~~d~~~")]
    #[case::inline_math("$x$ $$y$$ $ a")]
    #[case::inline_autolinks("<http://a.b> <a@b.c> <span> <!-- c --> <br/>")]
    #[case::inline_links("[a](http://x \"t\") [b][c] [d][] [e] ![f](g) ![h][i]")]
    #[case::inline_link_forms("[a](<b c> 'x') [d]( e ) [f](g (h)) [i](j \"k\\\"l\") [m](n&amp;o)")]
    #[case::inline_link_multiline("[a](b\n\"t\") [c](d")]
    #[case::inline_image_alt("![a *b* `c`](d) ![e ![f](g)](h)")]
    #[case::inline_link_in_link("[a [b](c) d](e)")]
    #[case::inline_emphasis_link("*[a](b)* **[c][a]**")]
    #[case::inline_html("<a href=\"x\">t</a> <a\nhref=x>")]
    #[case::inline_html_misc("<http://a b> <a+b@c> <ab> </a> <?x?> <![CDATA[x]]> <!X y>")]
    #[case::inline_nested_emphasis("*a **b** c* **a *b* c**")]
    #[case::inline_intraword("a_b_c a*b*c _a_b")]
    #[case::inline_unmatched("*a **b _c ~d [e ![f")]
    #[case::inline_rule_of_three("*foo**bar**baz* ***a** b*")]
    #[case::inline_heading("# a *b* `c`\n")]
    #[case::inline_setext_heading("a *b*\nc\n===\n")]
    #[case::inline_list_item("- a *b*\n  c **d**\n")]
    #[case::inline_table_cell("| a *b* | `c\\|d` |\n|-|-|\n")]
    #[case::inline_multibyte("あ*い*う `え` [お](か)")]
    #[case::inline_crlf_break("a  \r\nb\r\n")]
    #[case::math_multiline("x$\nx$")]
    #[case::decl_email("x<!a@b.co>")]
    #[case::code_multiline("x`\nx`")]
    #[case::def_basic("[a]: b")]
    #[case::def_title("[a]: <b c> 'T'")]
    #[case::def_next_lines("[a]:\nb\n\"t\"")]
    #[case::def_title_junk_next_line("[a]: b\n\"t\" x")]
    #[case::def_title_junk_same_line("[a]: b \"t\" x")]
    #[case::def_then_paragraph("[a]: b\nc")]
    #[case::def_two_then_text("[a]: b\n[c]: d\ntext")]
    #[case::def_not_at_start("text\n[a]: b")]
    #[case::def_then_setext_equals("[a]: b\n=== ")]
    #[case::def_then_setext_dashes("[a]: b\n---")]
    #[case::def_indent1(" [a]: b")]
    #[case::def_indent3("   [a]: b")]
    #[case::def_indent4("    [a]: b")]
    #[case::def_in_quote("> [a]: b")]
    #[case::def_in_list("- [a]: b")]
    #[case::def_no_dest("[a]:")]
    #[case::def_empty_label("[]: b")]
    #[case::def_blank_label("[ ]: b")]
    #[case::def_escaped_label("[a\\]b]: c")]
    #[case::def_multiline_title("[a]: b \"t\nt\"")]
    #[case::def_title_indented("[a]: b\n  \"t\"")]
    #[case::def_no_space("[a]:b")]
    #[case::def_dest_junk("[a]: b c")]
    #[case::def_duplicate_labels("[A  b]: c\n[a b]: d")]
    #[case::def_empty_angle("[a]: <>")]
    #[case::def_quote_in_dest("[a]: 'x")]
    #[case::def_use_shortcut("[a]: /u\n\n[a] [a][] [b][a] [a][b] [A b]")]
    #[case::def_use_normalized("[ab cd]: /u\n\n[AB   CD] [ab\ncd]")]
    #[case::def_use_before("[a] and [b][a]\n\n[a]: /u \"T\"")]
    #[case::def_use_in_link_text("[a]: b\n\n[a](x) [a]")]
    #[case::def_use_image("[x]: /u\n\n![x] ![y][x] ![x][]")]
    #[case::def_link_in_emphasis("[a]: b\n\n*[a]* **[c][a]**")]
    #[case::fn_basic("[^a]: b")]
    #[case::fn_lazy("[^a]: b\nc")]
    #[case::fn_indented_continuation("[^a]: b\n    c")]
    #[case::fn_two_paragraphs("[^a]: b\n\n    c")]
    #[case::fn_two_paragraphs_then_text("[^a]: b\n\n    c\n\nd")]
    #[case::fn_two_space_continuation("[^a]: b\n  c")]
    #[case::fn_empty("[^a]:")]
    #[case::fn_empty_space("[^a]: ")]
    #[case::fn_content_next_line("[^a]:\n    b")]
    #[case::fn_two_adjacent("[^a]: b\n[^c]: d")]
    #[case::fn_two_separated("[^a]: b\n\n[^c]: d")]
    #[case::fn_label_space("[^ a]: b")]
    #[case::fn_label_inner_space("[^a b]: b")]
    #[case::fn_label_empty("[^]: b")]
    #[case::fn_indent1(" [^a]: b")]
    #[case::fn_indent4("    [^a]: b")]
    #[case::fn_in_quote("> [^a]: b")]
    #[case::fn_in_list("- [^a]: b")]
    #[case::fn_then_quote("[^a]: b\n> q")]
    #[case::fn_then_list("[^a]: b\n- x")]
    #[case::fn_heading("[^a]: # h")]
    #[case::fn_list("[^a]: - x\n    - y")]
    #[case::fn_fence("[^a]: b\n    ```\n    x\n    ```")]
    #[case::fn_ref_case("[^A]: b\n\nx[^a]")]
    #[case::fn_no_space("[^a]:b")]
    #[case::fn_interrupts_paragraph("text\n[^a]: b")]
    #[case::fn_then_dashes("[^a]: b\n---")]
    #[case::fn_then_equals("[^a]: b\n===")]
    #[case::fn_ref_basic("[^a]\n\n[^a]: note")]
    #[case::fn_ref_no_def("[^b] no def")]
    #[case::fn_ref_inline("x[^a] and [^a]!\n\n[^a]: note")]
    #[case::fn_ref_in_link_text("[a[^a]](b)\n\n[^a]: note")]
    #[case::html_div("<div>\nx\n</div>\n\ny")]
    #[case::html_indented("  <div>\n  x\n")]
    #[case::html_oneline("<div>x</div>")]
    #[case::html_script("<script>\nx\n\ny\n</script>\nz")]
    #[case::html_pre("<pre>\na\n\nb</pre>\nc")]
    #[case::html_style_inline_end("<style>x</style> y\nz")]
    #[case::html_comment("<!-- c\n\n d -->\nx")]
    #[case::html_instruction("<?php\nx ?>\ny")]
    #[case::html_declaration("<!DOCTYPE html>\nx")]
    #[case::html_cdata("<![CDATA[\n\nx]]>\ny")]
    #[case::html_complete_tag("<a href=\"x\">\ny")]
    #[case::html_tag_then_text("<a href=\"x\">y")]
    #[case::html_interrupt_basic("text\n<div>")]
    #[case::html_no_interrupt_complete("text\n<a href=\"x\">")]
    #[case::html_custom_element("<x-y>\nz")]
    #[case::html_closing("</div>\nz")]
    #[case::html_multiline_open("<div\nclass=\"a\">\nz")]
    #[case::html_in_quote("> <div>\n> x")]
    #[case::html_in_list("- <div>\n  x")]
    #[case::html_uppercase("<DIV>\nx")]
    #[case::html_self_closing("<div/>\nx")]
    #[case::html_ins("<ins>\nx")]
    #[case::html_br("<br>\nx")]
    #[case::html_textarea("<textarea>\nx\n\ny</textarea>")]
    #[case::html_table_two_blocks("<table>\n<tr>\n\n<td>")]
    #[case::html_img_self_close("<img src=\"x\" />\nz")]
    #[case::html_img_then_text("<img src=\"x\" /> y")]
    #[case::html_multiline_tag_inline("<a\nb>\nz")]
    #[case::html_indent4("    <div>")]
    #[case::html_comment_then_text("<!--x-->y\nz")]
    #[case::html_then_code_line("<div>\n    code")]
    #[case::html_comment_short("<!-->\nx")]
    #[case::html_instruction_short("<?>\nx")]
    #[case::html_lazy_quote("> <div>\nx")]
    #[case::html_attr_forms("<a b c=d e='f' g=\"h\">\nx")]
    #[case::html_bad_attr("<a b=>\nx")]
    #[case::fm_yaml("---\na: b\n---\ntext")]
    #[case::fm_yaml_only("---\na: b\n---")]
    #[case::fm_empty("---\n---")]
    #[case::fm_unclosed("---\na\n")]
    #[case::fm_blank_content("---\n\n---\nx")]
    #[case::fm_toml("+++\na = 1\n+++\nx")]
    #[case::fm_indented(" ---\na\n---")]
    #[case::fm_close_trailing_space("---\na\n--- \nx")]
    #[case::fm_close_longer("---\na\n----\nx")]
    #[case::fm_open_trailing_space("---  \na\n---")]
    #[case::fm_open_junk("---x\na\n---")]
    #[case::fm_not_first("text\n---\na\n---")]
    #[case::fm_blank_before("\n---\na\n---")]
    #[case::fm_multiline_content("---\na: 1\n\nb: 2\n---\n# h")]
    #[case::fm_hr_after("---\na\n---\n\n---")]
    #[case::math_basic("$$\na\n$$")]
    #[case::math_then_text("$$\na\n$$\nx")]
    #[case::math_meta("$$ meta\na\n$$")]
    #[case::math_unclosed("$$\na")]
    #[case::math_longer_open("$$$\na\n$$")]
    #[case::math_longer_close("$$\na\n$$$$")]
    #[case::math_indented("  $$\n  a\n   b\n  $$")]
    #[case::math_empty("$$\n$$")]
    #[case::math_inline_not_block("$$a$$")]
    #[case::math_info_no_close("$$a\n$$")]
    #[case::math_in_quote("> $$\n> a\n> $$")]
    #[case::math_in_list("- $$\n  a\n  $$")]
    #[case::math_close_junk("$$\na\n$$ x")]
    #[case::math_interrupts_paragraph("text\n$$\na\n$$")]
    #[case::math_blank_inside("$$\n\na\n$$")]
    #[case::math_single_dollar_line("$ $\nx")]
    #[case::crlf_frontmatter("---\r\na\r\n---\r\nx")]
    #[case::break_end_indented("a  \n  b")]
    #[case::text_end_indented_continuation("*a*\n  <x>")]
    #[case::soft_end_before_node("a\n  `b`")]
    #[case::break_in_quote("> a  \n> b")]
    #[case::text_eol_in_footnote("[^a]: x \n  [a]")]
    #[case::math_whitespace_content("  $$\n \n  $$")]
    #[case::math_whitespace_content2("  $$\n  \n  $$")]
    #[case::table_indented_no_pipe("  a|b\n|-|:-:|")]
    #[case::tab_nested_list("- a\n\t- b\n\t\t- c\n")]
    #[case::tab_nested_ordered("1. a\n\t1. b\n\t\t1. c\n")]
    #[case::tab_list_code("- a\n\n\t\tcode\n")]
    #[case::tab_in_text("a\tb\t*c*\n")]
    #[case::tab_code_block("\tcode\n\t\tmore\n")]
    #[case::tab_after_marker("-\ta\n")]
    #[case::atx_leading_sequences("# # a")]
    #[case::atx_leading_sequences2("## # a")]
    #[case::atx_only_sequences("# # #")]
    #[case::atx_no_space_sequence("# #a")]
    #[case::atx_double_sequence("# ## a")]
    #[case::atx_tab_sequence("#\t# a")]
    #[case::atx_closing_and_leading("# # a #")]
    #[case::atx_inner_hash("# a # b")]
    #[case::quote_footnote_trailing_space("- a\n  > [^a]: x\n  ")]
    #[case::indented_code_trailing_blank("    code\n\n\n")]
    fn matches_markdown_rs(#[case] input: &str) {
        assert_same_as_markdown_rs(input);
    }

    /// Lines built from container prefixes and block-level bodies only, so inline syntax (not
    /// implemented yet) never appears. Constructs whose markdown-rs positions depend on quirks of the
    /// following lines are left to the hand-written cases: empty items and quotes, unclosed fences,
    /// indented code next to containers, ordered lists that do not start at 1, and lazy continuation
    /// inside nested containers, more than one container marker on a line, and runs of setext
    /// underlines (`===` is only used by the hand-written cases).
    fn block_input() -> impl Strategy<Value = String> {
        // At most one container marker per line; deeper nesting comes from indentation.
        let indent = prop::sample::select(vec!["", " ", "  ", "   "]);
        let marker = prop::sample::select(vec!["", "> ", ">", "- ", "* ", "+ ", "1. ", "1) "]);
        let body = prop::sample::select(vec![
            "a",
            "b",
            "# a",
            "#",
            "##  b ##",
            "---",
            "***",
            "| a | b |",
            "|-|:-:|",
            "|1|2|3|",
            "a|b",
            "[a]: /u",
            "[a]: /u 'T'",
            "[^a]: x",
            "<!-- c -->",
            "x [a] [b][a] [^a]",
            "*a* `b`",
            "a  ",
            "a\\",
            "+++",
        ]);
        let block = (indent, marker, body).prop_map(|(indent, marker, body)| format!("{indent}{marker}{body}"));
        let blank = prop::sample::select(vec!["", "  "]).prop_map(str::to_string);
        // Fences are always closed here.
        let fence = (
            prop::sample::select(vec!["", " ", "  ", "   "]),
            prop::sample::select(vec![
                ("```", "```"),
                ("```rust x", "````"),
                ("~~~", "~~~"),
                ("$$", "$$"),
                ("$$ m", "$$$"),
            ]),
            prop::sample::select(vec!["a", "", "> b", "- c"]),
        )
            .prop_map(|(indent, (open, close), content)| format!("{indent}{open}\n{indent}{content}\n{indent}{close}"));
        let line = prop_oneof![8 => block, 2 => blank, 1 => fence];
        (prop::collection::vec(line, 0..8), any::<bool>()).prop_map(|(lines, trailing_eol)| {
            let mut input = lines.join("\n");
            if trailing_eol {
                input.push('\n');
            }
            input
        })
    }

    fn env_number<T: std::str::FromStr>(name: &str) -> Option<T> {
        std::env::var(name).ok()?.parse().ok()
    }

    /// Runs `strategy` with a fixed seed so the outcome never varies between runs. `PROPTEST_CASES`
    /// and `PARSER_TEST_SEED` widen the exploration, which finds more differences in the cases listed
    /// on [`block_input`] that are not covered yet.
    fn check(strategy: impl Strategy<Value = String>) {
        check_with(strategy, parse_with_markdown_rs, assert_same_as_markdown_rs);
    }

    fn check_with<R>(
        strategy: impl Strategy<Value = String>,
        reference: fn(&str) -> miette::Result<Vec<crate::node::Node>>,
        assert_same: fn(&str) -> R,
    ) {
        let config = ProptestConfig {
            cases: env_number("PROPTEST_CASES").unwrap_or(3000),
            failure_persistence: None,
            ..ProptestConfig::default()
        };
        let seed = env_number("PARSER_TEST_SEED").unwrap_or(7);
        let mut runner = TestRunner::new_with_rng(config, TestRng::from_seed(RngAlgorithm::ChaCha, &[seed; 32]));
        let result = runner.run(&strategy, |input| {
            // markdown-rs misparses what follows an opening `---` or `+++` that never closes, and even
            // panics on some inputs. Those cannot be compared.
            let mut lines = input.lines().map(str::trim_end);
            if let Some(first @ ("---" | "+++")) = lines.next() {
                prop_assume!(lines.any(|line| line == first));
            }
            let outcome = std::panic::catch_unwind(|| reference(&input));
            prop_assume!(outcome.is_ok());
            assert_same(&input);
            Ok(())
        });
        if let Err(error) = result {
            panic!("{error}");
        }
    }

    #[test]
    fn block_structure_matches_markdown_rs() {
        check(block_input());
    }

    /// Lines of inline syntax. Each line starts with `x` so that no line is a block on its own.
    fn inline_input() -> impl Strategy<Value = String> {
        let token = prop::sample::select(vec![
            "a",
            "b",
            "foo",
            " ",
            "  ",
            "*",
            "**",
            "***",
            "_",
            "__",
            "~",
            "~~",
            "`",
            "``",
            "[",
            "]",
            "(",
            ")",
            "![",
            "<",
            ">",
            "\\",
            "&",
            "#",
            "$",
            "$$",
            "http://a.b",
            "www.a.b",
            "a@b.co",
            "\"",
            "'",
            ".",
            ",",
            "!",
            "-",
            "/",
            "é",
            "あ",
            "\n",
            "](",
            "](x)",
            "][",
            "]:",
            "<a>",
            "</a>",
            "<x@y.z>",
            "<http://a>",
        ]);
        prop::collection::vec(token, 1..14).prop_map(|tokens| {
            let mut input = String::from("x");
            for token in tokens {
                input.push_str(token);
                if token == "\n" {
                    input.push('x');
                }
            }
            input
        })
    }

    /// Lines of MDX: tags, expressions and the Markdown around them.
    fn mdx_input() -> impl Strategy<Value = String> {
        let token = prop::sample::select(vec![
            "<",
            ">",
            "/",
            "{",
            "}",
            "a",
            "b",
            "c",
            " ",
            "  ",
            "=",
            "\"",
            "'",
            ".",
            ":",
            "-",
            "*",
            "_",
            "`",
            "[",
            "]",
            "(",
            ")",
            "&amp;",
            "<a>",
            "</a>",
            "<b/>",
            "<c d=\"e\">",
            "<>",
            "</>",
            "{x}",
            "{x {y}}",
            "# ",
            "- ",
            "> ",
            "\\",
            "!",
            "|",
            "\n",
            "\n\n",
            "\n  ",
            "<a.b>",
            "</a.b>",
            "<a:b/>",
            "{...c}",
            "é",
            "1. ",
        ]);
        prop::collection::vec(token, 1..16).prop_map(|tokens| tokens.concat())
    }

    #[test]
    fn mdx_syntax_matches_markdown_rs() {
        check_with(mdx_input(), parse_mdx_with_markdown_rs, assert_same_mdx);
    }

    #[test]
    fn inline_syntax_matches_markdown_rs() {
        check(inline_input());
    }

    const SPEC_URL: &str =
        "https://raw.githubusercontent.com/github/cmark-gfm/828322d1ee4facdab56f0d3edccb13e9af90dcd2/test/spec.txt";

    /// The markdown of each example of the `CommonMark` and GFM `spec.txt`, with its number.
    fn spec_examples(text: &str) -> Vec<(usize, String)> {
        let fence = "`".repeat(32);
        let open = format!("{fence} example");
        let mut examples = Vec::new();
        let mut lines = text.lines();

        while let Some(line) = lines.next() {
            if line != open {
                continue;
            }
            let markdown = lines.by_ref().take_while(|line| *line != ".").collect::<Vec<_>>();
            for line in lines.by_ref() {
                if line == fence {
                    break;
                }
            }
            // `→` stands for a tab in the spec.
            examples.push((examples.len() + 1, markdown.join("\n").replace('\u{2192}', "\t") + "\n"));
        }
        examples
    }

    /// Examples of the spec on which the native parser differs from `markdown-rs`, by number.
    const KNOWN_DIFFERENCES: &[usize] = &[];

    /// Compares every example of the spec, from `SPEC_FILE` or fetched over the network.
    #[test]
    #[ignore = "fetches spec.txt over the network; set SPEC_FILE to use a local copy"]
    fn spec_examples_match_markdown_rs() {
        let text = match std::env::var("SPEC_FILE") {
            Ok(path) => std::fs::read_to_string(path).unwrap(),
            Err(_) => ureq::get(SPEC_URL)
                .call()
                .unwrap()
                .into_body()
                .read_to_string()
                .unwrap(),
        };
        let examples = spec_examples(&text);
        assert!(examples.len() > 600, "only {} examples were found", examples.len());

        if let Ok(number) = std::env::var("SPEC_SHOW") {
            let (_, markdown) = &examples[number.parse::<usize>().unwrap() - 1];
            println!("{markdown:?}");
            println!("native: {:?}", parse(markdown).unwrap());
            println!("markdown-rs: {:?}", parse_with_markdown_rs(markdown).unwrap());
        }

        let differing = examples
            .iter()
            .filter(|(_, markdown)| {
                let Ok(expected) = std::panic::catch_unwind(|| parse_with_markdown_rs(markdown)) else {
                    return false;
                };
                format!("{:#?}", parse(markdown).unwrap()) != format!("{:#?}", expected.unwrap())
            })
            .map(|(number, _)| *number)
            .collect::<Vec<_>>();

        assert_eq!(differing, KNOWN_DIFFERENCES, "examples that differ from markdown-rs");
    }
}
