//! Markdown parser that builds [`Node`] values directly.
//!
//! Parsing runs in phases: [`block`] resolves the block structure line by line into a tree,
//! [`resolve`] collects the definitions and turns the tree into nodes, and [`inline`] parses the raw
//! text of paragraphs, headings and table cells along the way.
//!
//! It reads `CommonMark`, GFM, frontmatter, math and MDX (without a JavaScript parser, so expressions
//! only need balanced braces). HTML is rendered by [`render_html`].
mod block;
#[cfg(feature = "callout")]
mod callout;
mod code;
mod definition;
mod html_flow;
mod inline;
mod line;
mod mdx;
mod mdx_flow;
mod render_html;
mod resolve;
mod table;
mod tree;

use crate::node::Node;

/// Parses `content` into a flat list of nodes.
pub(crate) fn parse(content: &str) -> miette::Result<Vec<Node>> {
    resolve::resolve(block::parse(content, false), false).map_err(|message| miette::miette!(message))
}

/// Renders `content` as HTML.
pub(crate) fn to_html(content: &str) -> String {
    render_html::render(content)
}

/// Parses `content` as MDX: no indented code, HTML, autolinks or GFM, but expressions and JSX.
pub(crate) fn parse_mdx(content: &str) -> miette::Result<Vec<Node>> {
    resolve::resolve(block::parse(content, true), true).map_err(|message| miette::miette!(message))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use proptest::test_runner::{RngAlgorithm, TestRng, TestRunner};

    const MDX_CASES: &[(&str, &str)] = &[
        ("jsx_self", "<a />"),
        ("jsx_attrs", "<a b=\"c&amp;d\" e='f' g={h} {...i} j />"),
        ("jsx_member", "<a.b.c />"),
        ("jsx_namespace", "<a:b c:d=\"e\" />"),
        ("jsx_fragment", "<></>"),
        ("jsx_text_pair", "<a>x</a>"),
        ("jsx_flow_pair", "<a>\n\nx\n\n</a>"),
        ("jsx_flow_indented_child", "<a>\n  x\n</a>"),
        ("jsx_inline", "a <b>c</b> d"),
        ("jsx_inline_self", "a <b/> d"),
        ("expr_flow", "{a}"),
        ("expr_nested", "{a {b} c}"),
        ("expr_multiline", "{a\nb}"),
        ("expr_multiline_indented", "{a\n  b\n   c}"),
        ("expr_text", "x {a} y"),
        ("expr_then_tag", "{a} <b/>"),
        ("tag_then_expr", "<b/> {a}"),
        ("tag_then_text", "<b/> x"),
        ("expr_then_text", "{a}x"),
        ("jsx_in_expr_child", "<a>{b}</a>"),
        ("jsx_unclosed_text", "<a>b"),
        ("jsx_mismatch", "<a></b>"),
        ("lt_space", "a < b"),
        ("lt_digit", "a <3"),
        ("attr_no_value", "<a b=>"),
        ("attr_space_after_eq", "<b e= \"f\"/>"),
        ("attr_spaces_around_eq", "a <b e = \"f\"/>."),
        ("attr_line_before_eq", "<b c\n= \"x\">c</b>"),
        ("attr_namespace_spaces", "a <b xml :\tlang\n= \"de-CH\" foo:bar>c</b>."),
        ("attr_spaced_names", "a <b a b : c d : e = \"f\" g/>."),
        ("attr_expr_space_after_eq", "<b e= {f}/>"),
        ("jsx_nested_flow", "<a>\n<b>\n</b>\n</a>"),
        ("jsx_in_quote", "> <a>\n> x\n> </a>"),
        ("jsx_in_list", "- <a>\n  x\n  </a>"),
        ("attr_quote_inside", "<a b='c\"d' />"),
        ("two_tags", "<a/><b/>"),
        ("two_text_elements", "<a>x</a><b>y</b>"),
        ("interrupt_paragraph_tag", "a\n<b/>"),
        ("interrupt_paragraph_expr", "a\n{b}"),
        ("two_expressions", "{a}\n{b}"),
        ("expr_in_flow_element", "<a>\n\n{b}\n\n</a>"),
        ("indented_tag", "   <a/>"),
        ("very_indented_tag", "    <a/>"),
        ("heading_jsx", "# h <a>b</a>"),
        ("emphasis_crossing", "*<a>x</a>*"),
        ("emphasis_crossing_bad", "*<a>x*</a>"),
        ("link_jsx", "[<a>x</a>](y)"),
        ("code_not_jsx", "`<a/>` {b}"),
        ("escaped", "\\<a/> \\{b}"),
        ("attr_entities", "<a b=\"&lt;&#x41;\" />"),
        ("attr_multiline_literal", "<a b='x\ny' />"),
        ("closing_spaces", "<a  >x</a  >"),
        ("lt_space_name", "< a>"),
        ("self_closing_space", "<a / >"),
        ("attr_on_next_line", "<a\nb />"),
        ("dashed_name", "<a-b />"),
        ("dashed_attr", "<a b-c=\"d\" />"),
        ("bad_name_char", "<A_b$.c-d />"),
        ("expr_unclosed", "{a"),
        ("expr_empty", "{}"),
        ("expr_space", "{ }"),
        ("empty_flow_element", "<a>\n</a>"),
        ("text_element_unclosed_line", "<a>x\n</a>"),
        ("stray_close", "</a>"),
        ("stray_close_text", "x </a>"),
        ("element_with_emphasis", "<a>*b*</a>"),
        ("flow_element_with_emphasis", "<a>\n*b*\n</a>"),
        ("flow_element_with_list", "<a>\n- b\n</a>"),
        ("list_items_unbalanced", "- <a>\n- </a>"),
        ("spaced_equals", "<a b = \"c\" />"),
        ("heading", "# h\n\ntext *a* [b](c) `d`\n"),
        ("indented_is_text", "    not code\n"),
        ("indented_heading", "     # h\n"),
        ("no_autolink", "<http://a.b>"),
        ("no_gfm", "| a |\n|-|\n\n~a~ www.a.b [^a]\n\n- [ ] a\n"),
        ("no_math", "$a$\n\n$$\na\n$$\n"),
        ("no_frontmatter", "---\na\n---\n"),
        ("esm", "import a from 'b'\n\nexport const c = 1\n"),
        ("definition", "[a]: /u\n\n[a]\n"),
        ("list_deep_indent", "-      a\n"),
        ("fence", "```rust\ncode\n```\n"),
        ("lone_cr_after_lt", "<\r/-x"),
        ("crlf_after_lt", "<\r\n/-x"),
        ("expression_lone_cr", "{\r }"),
        ("expression_cr_indent", "{a\r   b\r\n c\n  d}"),
        ("tilde_not_delimiter", "*~*a"),
        ("tilde_strong_not_delimiter", "**~**あ"),
        ("dollar_after_name", "<a$/>"),
        ("dollar_after_space", "<a $b/>"),
        ("cjk_tag_name", "<あ b=\"c\" />"),
        ("fullwidth_underscore_name", "<a＿b />"),
        ("jsx_text_cjk", "あ <b>い</b> う"),
        ("expression_cjk", "{あ} い"),
    ];

    const CASES: &[(&str, &str)] = &[
        ("empty", ""),
        ("blank_lines", "\n\n  \n"),
        ("paragraph", "hello\n"),
        ("paragraph_no_eol", "hello"),
        ("paragraph_multiline", "a\nb\nc\n"),
        ("paragraph_indented_continuation", "a\n    b\n"),
        ("paragraph_trailing_space", "あい\n  うえ  \n"),
        ("two_paragraphs", "a\n\nb\n"),
        ("crlf", "a\r\nb\r\n\r\nc\r\n"),
        ("crlf_fence", "```\r\na\r\nb\r\n```\r\n"),
        ("cr_only", "a\rb\r"),
        ("atx_h1", "# title\n"),
        ("atx_h6", "###### title\n"),
        ("atx_seven", "####### title\n"),
        ("atx_no_space", "#title\n"),
        ("atx_empty", "#\n"),
        ("atx_closing", "## title ##\n"),
        ("atx_closing_no_space", "## title##\n"),
        ("atx_indent", "  ## title ##  \n"),
        ("atx_multibyte", "# あ h #\n"),
        ("atx_interrupts_paragraph", "a\n# b\n"),
        ("setext_h1", "Title\n===\n"),
        ("setext_h2", "a\n  ---\n"),
        ("setext_multiline", "a\nb\n---\n"),
        ("thematic_star", "***\n"),
        ("thematic_dash_spaced", "- - -\n"),
        ("thematic_underscore", "  ___  \n"),
        ("thematic_two", "**\n"),
        ("fence_lang_meta", "```rust title\nlet a;\n```\n"),
        ("fence_indented", "  ```\n  a\n   b\n  ```\n"),
        ("fence_tilde", "~~~\na\n~~~\n"),
        ("fence_unclosed", "```\na\n"),
        ("fence_empty", "para\n\n\n```\n```\n"),
        ("fence_longer_close", "```\na\n`````\n"),
        ("fence_shorter_close", "````\na\n```\nb\n````\n"),
        ("fence_blank_inside", "```\na\n\nb\n```\n"),
        ("fence_backtick_info", "``` a`b\nc\n"),
        ("fence_interrupts_paragraph", "a\n```\nb\n```\n"),
        ("indented_code", "    code\n\n      more\n\nx\n"),
        ("quote", "> a\n> b\n"),
        ("quote_indented_lazy", "  > a\nb\n"),
        ("quote_heading", "> # h\n>\n> c\n"),
        ("quote_no_space", ">a\n"),
        ("quote_empty", ">\n"),
        ("quote_nested", "> > a\n> b\n"),
        ("quote_lazy_nested", "> > a\nb\n"),
        ("quote_blank_ends", "> a\n\n> b\n"),
        ("quote_fence", "> ```\n> a\n> ```\n"),
        ("quote_no_lazy_after_fence", "> ```\n> a\nb\n"),
        ("quote_interrupts_paragraph", "a\n> b\n"),
        ("quote_thematic_not_lazy", "> a\n---\n"),
        ("quote_multibyte", "> あ\n> い\n"),
        ("bullet", "- a\n- b\n"),
        ("bullet_loose", "- a\n\n- b\n"),
        ("bullet_indent", " - a\n"),
        ("bullet_mixed_markers", "* a\n+ b\n"),
        ("ordered", "1. a\n2. b\n"),
        ("ordered_paren", "5) a\n"),
        ("ordered_zero", "0. a\n"),
        ("ordered_too_long", "1234567890. a\n"),
        ("ordered_mixed_delimiters", "1. a\n2) b\n"),
        ("task", "- [ ] x\n- [x] y\n- [X] z\n"),
        ("task_empty", "- [ ]\n"),
        ("task_no_space", "- [ ]x\n"),
        ("nested", "- a\n  - b\n    c\n"),
        ("nested_ordered", "1. a\n   1. b\n   2. c\n2. d\n"),
        ("nested_dedent", "- a\n  - b\n- c\n"),
        ("two_paragraphs", "- a\n\n  b\n"),
        ("two_paragraphs_then_item", "- a\n\n  b\n- c\n"),
        ("empty_item", "-\n"),
        ("empty_item_content", "-\n  foo\n"),
        ("empty_item_blank_then_text", "-\n\n  foo\n"),
        ("empty_items", "-\n-\n"),
        ("wide_marker_gap", "-   a\n    b\n"),
        ("code_in_item", "-     a\n"),
        ("lazy", "- a\nb\n"),
        ("lazy_after_blank", "- a\n\nb\n"),
        ("list_interrupts_paragraph", "a\n- b\n"),
        ("ordered_two_no_interrupt", "a\n2. b\n"),
        ("ordered_one_interrupts", "a\n1. b\n"),
        ("empty_item_no_interrupt", "a\n-\n"),
        ("thematic_over_list", "- - -\n"),
        ("list_in_quote", "> - a\n> - b\n"),
        ("quote_in_list", "- > a\n  > b\n"),
        ("quote_in_list_level", "- a\n  > - b\n"),
        ("heading_in_item", "- # h\n  text\n"),
        ("fence_in_item", "- ```\n  a\n  ```\n"),
        ("multibyte_item", "- あ\n- い\n"),
        ("list_after_paragraph_blank", "a\n\n- b\n"),
        ("two_lists_split", "- a\n\n\n- b\n"),
        ("deep_quote", ">>>>>> a\n"),
        ("fence_unclosed_no_eol", "```\na"),
        ("fence_unclosed_in_item", "- ```\n  a\n"),
        ("fence_unclosed_in_item_no_eol", "- ```\n  a"),
        ("fence_unclosed_in_quote_no_eol", "> ```\n> a"),
        ("fence_unclosed_in_quote_eof", "> ```\n> a\n"),
        ("fence_unclosed_trailing_blank", "```\na\n\n"),
        ("fence_unclosed_item_then_para", "- ```\n  a\nb\n"),
        ("item_code_then_unindented_text", "- ===\n\n      a\n==="),
        ("quote_setext_dashes", "> a\n> ---\n> ---\n> a"),
        ("table_basic", "| a | b |\n|---|:-:|\n| 1 | 2 |\n"),
        ("table_no_edge_pipes", "a|b\n-|-\n1|2|3\n"),
        ("table_short_row", "|a|b|\n|-|-|\n|1|\n"),
        ("table_escaped_pipe", "| a \\| b | c |\n|--|--|\n"),
        ("table_indented", "  | a |\n  |---|\n  | b |\n"),
        ("table_text_after", "| a |\n|---|\ntext after\n"),
        ("table_in_quote", "> | a |\n> |---|\n> | b |\n"),
        ("table_after_paragraph", "x\n| a |\n|---|\n"),
        ("table_column_mismatch", "| a | b |\n|---|\n"),
        ("table_bad_delimiter", "|a|\n|:|\n"),
        ("table_then_blank_para", "|a|\n|-|\n\nnext\n"),
        ("table_then_quote", "|a|\n|-|\n> q\n"),
        ("table_then_heading", "|a|\n|-|\n# h\n"),
        ("table_then_fence", "|a|\n|-|\n```\nx\n```\n"),
        ("table_then_list", "|a|\n|-|\n- x\n"),
        ("table_two", "|a|\n|-|\n\n|b|\n|-|\n"),
        ("table_align_spaces", "|a|b|\n|:-|-:|\n|  x  |  y|\n"),
        ("table_indented_code_row", "|a|\n|-|\n|b|\n    |c|\n"),
        ("table_no_body", "| a |\n| - |\n"),
        ("table_empty_cells", "||\n|-|\n"),
        ("table_lone_pipe_row", "|a|\n|-|\n|\n"),
        ("table_no_trailing_pipe", "| a | b\n|-|-\n| c | d\n"),
        ("table_multibyte", "| あ | い |\n|---|---|\n| う | え |\n"),
        ("not_table_setext", "Title\n---\n"),
        ("not_table_no_pipe_delim", "a|b\n---\n"),
        ("table_header_no_pipe", "a\n|-|\n"),
        ("table_one_col_dash", "|a|\n-\n"),
        ("inline_escape", "a\\*b"),
        ("inline_escape_nonpunct", "a\\qb"),
        ("inline_entities", "a&amp;b &copy; &#35; &#x41; &unknown; &"),
        ("inline_emphasis_all", "a *b* **c** _d_ __e__ ***f***"),
        ("inline_code", "`code` `` a`b `` ` a `"),
        ("inline_code_unclosed", "`a ``b`"),
        ("inline_hard_break_spaces", "a  \nb"),
        ("inline_hard_break_backslash", "a\\\nb"),
        ("inline_soft_break", "a\nb"),
        ("inline_soft_break_trailing_space", "a \nb"),
        ("inline_emphasis_multiline", "*a\nb*"),
        ("inline_strike", "a ~b~ ~~c~~ ~~~d~~~"),
        ("inline_math", "$x$ $$y$$ $ a"),
        ("inline_autolinks", "<http://a.b> <a@b.c> <span> <!-- c --> <br/>"),
        ("inline_links", "[a](http://x \"t\") [b][c] [d][] [e] ![f](g) ![h][i]"),
        (
            "inline_link_forms",
            "[a](<b c> 'x') [d]( e ) [f](g (h)) [i](j \"k\\\"l\") [m](n&amp;o)",
        ),
        ("inline_link_multiline", "[a](b\n\"t\") [c](d"),
        ("inline_image_alt", "![a *b* `c`](d) ![e ![f](g)](h)"),
        ("inline_link_in_link", "[a [b](c) d](e)"),
        ("inline_emphasis_link", "*[a](b)* **[c][a]**"),
        ("inline_html", "<a href=\"x\">t</a> <a\nhref=x>"),
        (
            "inline_html_misc",
            "<http://a b> <a+b@c> <ab> </a> <?x?> <![CDATA[x]]> <!X y>",
        ),
        ("inline_nested_emphasis", "*a **b** c* **a *b* c**"),
        ("inline_intraword", "a_b_c a*b*c _a_b"),
        ("inline_unmatched", "*a **b _c ~d [e ![f"),
        ("inline_rule_of_three", "*foo**bar**baz* ***a** b*"),
        ("inline_heading", "# a *b* `c`\n"),
        ("inline_setext_heading", "a *b*\nc\n===\n"),
        ("inline_list_item", "- a *b*\n  c **d**\n"),
        ("inline_table_cell", "| a *b* | `c\\|d` |\n|-|-|\n"),
        ("inline_multibyte", "あ*い*う `え` [お](か)"),
        ("inline_crlf_break", "a  \r\nb\r\n"),
        ("math_multiline", "x$\nx$"),
        ("decl_email", "x<!a@b.co>"),
        ("code_multiline", "x`\nx`"),
        ("def_basic", "[a]: b"),
        ("def_title", "[a]: <b c> 'T'"),
        ("def_next_lines", "[a]:\nb\n\"t\""),
        ("def_title_junk_next_line", "[a]: b\n\"t\" x"),
        ("def_title_junk_same_line", "[a]: b \"t\" x"),
        ("def_then_paragraph", "[a]: b\nc"),
        ("def_two_then_text", "[a]: b\n[c]: d\ntext"),
        ("def_not_at_start", "text\n[a]: b"),
        ("def_then_setext_equals", "[a]: b\n=== "),
        ("def_then_setext_dashes", "[a]: b\n---"),
        ("def_indent1", " [a]: b"),
        ("def_indent3", "   [a]: b"),
        ("def_indent4", "    [a]: b"),
        ("def_in_quote", "> [a]: b"),
        ("def_in_list", "- [a]: b"),
        ("def_no_dest", "[a]:"),
        ("def_empty_label", "[]: b"),
        ("def_blank_label", "[ ]: b"),
        ("def_escaped_label", "[a\\]b]: c"),
        ("def_multiline_title", "[a]: b \"t\nt\""),
        ("def_title_indented", "[a]: b\n  \"t\""),
        ("def_no_space", "[a]:b"),
        ("def_dest_junk", "[a]: b c"),
        ("def_duplicate_labels", "[A  b]: c\n[a b]: d"),
        ("def_empty_angle", "[a]: <>"),
        ("def_quote_in_dest", "[a]: 'x"),
        ("def_use_shortcut", "[a]: /u\n\n[a] [a][] [b][a] [a][b] [A b]"),
        ("def_use_normalized", "[ab cd]: /u\n\n[AB   CD] [ab\ncd]"),
        ("def_use_before", "[a] and [b][a]\n\n[a]: /u \"T\""),
        ("def_use_in_link_text", "[a]: b\n\n[a](x) [a]"),
        ("def_use_image", "[x]: /u\n\n![x] ![y][x] ![x][]"),
        ("def_link_in_emphasis", "[a]: b\n\n*[a]* **[c][a]**"),
        ("fn_basic", "[^a]: b"),
        ("fn_lazy", "[^a]: b\nc"),
        ("fn_indented_continuation", "[^a]: b\n    c"),
        ("fn_two_paragraphs", "[^a]: b\n\n    c"),
        ("fn_two_paragraphs_then_text", "[^a]: b\n\n    c\n\nd"),
        ("fn_two_space_continuation", "[^a]: b\n  c"),
        ("fn_empty", "[^a]:"),
        ("fn_empty_space", "[^a]: "),
        ("fn_content_next_line", "[^a]:\n    b"),
        ("fn_two_adjacent", "[^a]: b\n[^c]: d"),
        ("fn_two_separated", "[^a]: b\n\n[^c]: d"),
        ("fn_label_space", "[^ a]: b"),
        ("fn_label_inner_space", "[^a b]: b"),
        ("fn_label_empty", "[^]: b"),
        ("fn_indent1", " [^a]: b"),
        ("fn_indent4", "    [^a]: b"),
        ("fn_in_quote", "> [^a]: b"),
        ("fn_in_list", "- [^a]: b"),
        ("fn_then_quote", "[^a]: b\n> q"),
        ("fn_then_list", "[^a]: b\n- x"),
        ("fn_heading", "[^a]: # h"),
        ("fn_list", "[^a]: - x\n    - y"),
        ("fn_fence", "[^a]: b\n    ```\n    x\n    ```"),
        ("fn_ref_case", "[^A]: b\n\nx[^a]"),
        ("fn_no_space", "[^a]:b"),
        ("fn_interrupts_paragraph", "text\n[^a]: b"),
        ("fn_then_dashes", "[^a]: b\n---"),
        ("fn_then_equals", "[^a]: b\n==="),
        ("fn_ref_basic", "[^a]\n\n[^a]: note"),
        ("fn_ref_no_def", "[^b] no def"),
        ("fn_ref_inline", "x[^a] and [^a]!\n\n[^a]: note"),
        ("fn_ref_in_link_text", "[a[^a]](b)\n\n[^a]: note"),
        ("html_div", "<div>\nx\n</div>\n\ny"),
        ("html_indented", "  <div>\n  x\n"),
        ("html_oneline", "<div>x</div>"),
        ("html_script", "<script>\nx\n\ny\n</script>\nz"),
        ("html_pre", "<pre>\na\n\nb</pre>\nc"),
        ("html_style_inline_end", "<style>x</style> y\nz"),
        ("html_comment", "<!-- c\n\n d -->\nx"),
        ("html_instruction", "<?php\nx ?>\ny"),
        ("html_declaration", "<!DOCTYPE html>\nx"),
        ("html_cdata", "<![CDATA[\n\nx]]>\ny"),
        ("html_complete_tag", "<a href=\"x\">\ny"),
        ("html_tag_then_text", "<a href=\"x\">y"),
        ("html_interrupt_basic", "text\n<div>"),
        ("html_no_interrupt_complete", "text\n<a href=\"x\">"),
        ("html_custom_element", "<x-y>\nz"),
        ("html_closing", "</div>\nz"),
        ("html_multiline_open", "<div\nclass=\"a\">\nz"),
        ("html_in_quote", "> <div>\n> x"),
        ("html_in_list", "- <div>\n  x"),
        ("html_uppercase", "<DIV>\nx"),
        ("html_self_closing", "<div/>\nx"),
        ("html_ins", "<ins>\nx"),
        ("html_br", "<br>\nx"),
        ("html_textarea", "<textarea>\nx\n\ny</textarea>"),
        ("html_table_two_blocks", "<table>\n<tr>\n\n<td>"),
        ("html_img_self_close", "<img src=\"x\" />\nz"),
        ("html_img_then_text", "<img src=\"x\" /> y"),
        ("html_multiline_tag_inline", "<a\nb>\nz"),
        ("html_indent4", "    <div>"),
        ("html_comment_then_text", "<!--x-->y\nz"),
        ("html_then_code_line", "<div>\n    code"),
        ("html_comment_short", "<!-->\nx"),
        ("html_instruction_short", "<?>\nx"),
        ("html_lazy_quote", "> <div>\nx"),
        ("html_attr_forms", "<a b c=d e='f' g=\"h\">\nx"),
        ("html_bad_attr", "<a b=>\nx"),
        ("fm_yaml", "---\na: b\n---\ntext"),
        ("fm_yaml_only", "---\na: b\n---"),
        ("fm_empty", "---\n---"),
        ("fm_unclosed", "---\na\n"),
        ("fm_blank_content", "---\n\n---\nx"),
        ("fm_toml", "+++\na = 1\n+++\nx"),
        ("fm_indented", " ---\na\n---"),
        ("fm_close_trailing_space", "---\na\n--- \nx"),
        ("fm_close_longer", "---\na\n----\nx"),
        ("fm_open_trailing_space", "---  \na\n---"),
        ("fm_open_junk", "---x\na\n---"),
        ("fm_not_first", "text\n---\na\n---"),
        ("fm_blank_before", "\n---\na\n---"),
        ("fm_multiline_content", "---\na: 1\n\nb: 2\n---\n# h"),
        ("fm_hr_after", "---\na\n---\n\n---"),
        ("math_basic", "$$\na\n$$"),
        ("math_then_text", "$$\na\n$$\nx"),
        ("math_meta", "$$ meta\na\n$$"),
        ("math_unclosed", "$$\na"),
        ("math_longer_open", "$$$\na\n$$"),
        ("math_longer_close", "$$\na\n$$$$"),
        ("math_indented", "  $$\n  a\n   b\n  $$"),
        ("math_empty", "$$\n$$"),
        ("math_inline_not_block", "$$a$$"),
        ("math_info_no_close", "$$a\n$$"),
        ("math_in_quote", "> $$\n> a\n> $$"),
        ("math_in_list", "- $$\n  a\n  $$"),
        ("math_close_junk", "$$\na\n$$ x"),
        ("math_interrupts_paragraph", "text\n$$\na\n$$"),
        ("math_blank_inside", "$$\n\na\n$$"),
        ("math_single_dollar_line", "$ $\nx"),
        ("crlf_frontmatter", "---\r\na\r\n---\r\nx"),
        ("break_end_indented", "a  \n  b"),
        ("text_end_indented_continuation", "*a*\n  <x>"),
        ("soft_end_before_node", "a\n  `b`"),
        ("break_in_quote", "> a  \n> b"),
        ("text_eol_in_footnote", "[^a]: x \n  [a]"),
        ("math_whitespace_content", "  $$\n \n  $$"),
        ("math_whitespace_content2", "  $$\n  \n  $$"),
        ("table_indented_no_pipe", "  a|b\n|-|:-:|"),
        ("tab_nested_list", "- a\n\t- b\n\t\t- c\n"),
        ("tab_nested_ordered", "1. a\n\t1. b\n\t\t1. c\n"),
        ("tab_list_code", "- a\n\n\t\tcode\n"),
        ("tab_in_text", "a\tb\t*c*\n"),
        ("tab_code_block", "\tcode\n\t\tmore\n"),
        ("tab_after_marker", "-\ta\n"),
        ("atx_leading_sequences", "# # a"),
        ("atx_leading_sequences2", "## # a"),
        ("atx_only_sequences", "# # #"),
        ("atx_no_space_sequence", "# #a"),
        ("atx_double_sequence", "# ## a"),
        ("atx_tab_sequence", "#\t# a"),
        ("atx_closing_and_leading", "# # a #"),
        ("atx_inner_hash", "# a # b"),
        ("quote_footnote_trailing_space", "- a\n  > [^a]: x\n  "),
        ("indented_code_trailing_blank", "    code\n\n\n"),
        ("bom_heading", "\u{feff}# a\n"),
        ("bom_frontmatter", "\u{feff}---\na: b\n---\n"),
        ("bom_text", "\u{feff}a\n"),
        ("www_multibyte_after_ww", "wwß x"),
        ("www_combining", "ww\u{301}.a.com"),
        ("www_cjk", "www.あ.com/パス x"),
        ("http_cjk", "http://あ.jp/パス ok"),
        ("email_cjk_domain", "a@あ.com"),
        ("email_cjk_local", "あ@b.com"),
        ("html_inline_continuation", "<img a\n     b=\"c\">"),
        ("html_inline_continuation_tab", "<img a\n\t\tb=\"c\">"),
        ("table_trailing_space", "| a | b |  \n|-|-|  \n| c | d |  \n"),
        ("table_no_pipes_trailing_space", "a | b\n--|--\nd | e \n"),
        ("table_trailing_tab", "| a | b |\t\n|-|-|\n| c | d |   \n"),
        ("table_cjk", "| あ | 🎉 |\n|:-|-:|\n| é | ß |\n"),
        ("table_single_column_colon", "a\n:-\nb\n"),
        ("table_single_column_right", "a\n---:\nb\nc\n"),
        ("table_double_colon_not_delimiter", "a\n::-\n"),
        ("table_body_empty_marker", "a|b\n|-|-|\n*\n"),
        ("table_body_ordered_marker", "a|b\n|-|-|\n2. x\n"),
        ("table_body_empty_ordered", "a|b\n|-|-|\n1.\n"),
        ("paragraph_after_partial_tab", "- a\n\n\t[P](h) x\n"),
        ("fence_after_partial_tab", "- a\n\n\t```ts\n\t\tx\n\t```\n"),
        ("math_after_partial_tab", "- a\n\n\t$$\n\tm\n\t$$\n"),
        ("quote_after_partial_tab", "- a\n\n\t> q\n"),
        ("heading_after_partial_tab", "- a\n\n\t# h\n"),
        (
            "control_character_references",
            "&#1;&#x7f;&#x85;&#xB;&#xC;&#x9f;&#xa0;&#xd800;&#x110000;&#0;",
        ),
        ("footnote_escaped_bracket", "[^a\\]: x"),
        ("footnote_escaped_backslash", "[^a\\\\]: x"),
        ("footnote_escaped_open", "[^a\\[]: x"),
        ("footnote_unescaped_open", "[^a[]: x"),
        ("footnote_empty_then_blank", "[^a]: \n\n"),
        ("footnote_empty_then_spaces", "[^a]: \n    \n"),
        ("footnote_empty_then_text", "[^a]: \n\nb"),
        ("definition_nul_destination", "[a]: \0\n\n[a]\n"),
        ("link_nul_destination", "[a](x\0y)"),
        ("definition_then_empty_marker", "[a]: b\n-"),
        ("definition_then_item", "[a]: b\n- x"),
        ("definition_then_ordered", "[a]: b\n1."),
        ("emphasis_cjk", "*あ*。*い*"),
        ("strong_cjk", "**日本語**です"),
        ("underscore_accent", "_é_ _ß_"),
        ("fullwidth_markers", "＊a＊ ＿b＿"),
        ("emphasis_after_cjk_punctuation", "。*a*、**b**"),
        ("combining_mark", "e\u{301}*a*"),
        ("emoji_zwj", "👨\u{200d}👩\u{200d}👧 *a*"),
        ("fence_cjk_info", "```あ\n日本語\n```\n"),
        ("fence_in_quote_emoji", "> ```🎉\n> x\n> ```\n"),
        ("fence_in_list_cjk", "- ```あ\n  日本語\n  ```\n"),
        ("math_cjk", "$$\nあ\n$$\n\n$い$\n"),
        ("heading_cjk", "# 見出し\n\n見出し2\n===\n"),
        ("link_cjk", "[あ](http://い.jp/う \"え\")"),
        ("reference_cjk", "[あ]\n\n[あ]: /u\n"),
        ("footnote_cjk", "あ[^い]\n\n[^い]: う\n"),
        ("ideographic_space", "a\u{3000}b\n\u{3000}c"),
        ("nbsp_around_emphasis", "a\u{a0}*b*\u{a0}c"),
        ("entity_cjk_context", "あ&amp;い&#12354;&#x1F389;"),
        ("autolink_cjk", "<http://あ.jp/パス>"),
        ("code_span_cjk", "`あ`と``い`う``"),
        ("hard_break_cjk", "あ  \nい\\\nう"),
        ("nul_in_text", "a\0b"),
    ];

    /// One line with the nodes, positions written as `@line:column-line:column`.
    fn dump(nodes: &[Node]) -> String {
        format!("{nodes:?}")
            .replace("position: Some(Position { start: Point { line: ", "@")
            .replace(", column: ", ":")
            .replace(" }, end: Point { line: ", "-")
            .replace(" } })", "")
    }

    /// What the parser and the HTML renderer make of each case, to compare with `snapshots.txt`.
    /// Run the tests with `UPDATE_SNAPSHOTS=1` to write it after a change that is meant to alter it.
    fn snapshot() -> String {
        let mut out = String::new();
        for (name, input) in CASES {
            let nodes = dump(&parse(input).unwrap());
            out.push_str(&format!(
                "## {name} {input:?}\nnodes: {nodes}\nhtml: {:?}\n",
                to_html(input)
            ));
        }
        for (name, input) in MDX_CASES {
            let nodes = match parse_mdx(input) {
                Ok(nodes) => dump(&nodes),
                Err(error) => format!("error: {error}"),
            };
            out.push_str(&format!("## mdx {name} {input:?}\nnodes: {nodes}\n"));
        }
        out
    }

    #[test]
    fn snapshots_match() {
        let actual = snapshot();
        if std::env::var("UPDATE_SNAPSHOTS").is_ok() {
            std::fs::write(
                concat!(env!("CARGO_MANIFEST_DIR"), "/src/parser/snapshots.txt"),
                &actual,
            )
            .unwrap();
            return;
        }
        let expected = include_str!("parser/snapshots.txt");
        for (actual, expected) in actual.split("## ").zip(expected.split("## ")) {
            assert_eq!(actual, expected, "snapshot differs, UPDATE_SNAPSHOTS=1 rewrites it");
        }
        assert_eq!(actual.len(), expected.len(), "number of snapshots differs");
    }

    fn env_number<T: std::str::FromStr>(name: &str) -> Option<T> {
        std::env::var(name).ok()?.parse().ok()
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

    /// Markdown syntax mixed with multibyte text: CJK, emoji, combining marks, `ß`, full-width and
    /// ideographic punctuation and spaces, and characters that change case or width.
    fn unicode_input() -> impl Strategy<Value = String> {
        let word = prop::sample::select(vec![
            "あ",
            "日本語",
            "テスト",
            "🎉",
            "👨‍👩‍👧",
            "é",
            "e\u{301}",
            "ß",
            "ǅ",
            "İ",
            "ﬁ",
            "ｗｗｗ",
            "www.あ.com",
            "wwß",
            "ww\u{301}",
            "http://あ.jp/パス",
            "https://例え.jp",
            "a@あ.com",
            "あ@b.com",
            "。",
            "、",
            "「",
            "」",
            "（",
            "）",
            "！",
            "？",
            "＊",
            "＿",
            "｀",
            "＃",
            "\u{3000}",
            "\u{a0}",
            "\u{200b}",
            "\u{2028}",
            "\u{feff}",
            "\u{fe0f}",
            "ا",
            "א",
            "𠮷",
            "\u{10ffff}",
            "\u{0}",
            "x",
            "foo",
        ]);
        let syntax = prop::sample::select(vec![
            " ",
            "  ",
            "\t",
            "*",
            "**",
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
            "&",
            "&amp;",
            "&#12354;",
            "&#x1F389;",
            "&あ;",
            "\\",
            "|",
            ":",
            "-",
            "#",
            "^",
            "\"",
            "'",
            "{",
            "}",
            "/",
            "=",
            "://",
            ".",
            "\n",
            "\n\n",
            "\n> ",
            "\n- ",
            "\n1. ",
            "\n# ",
            "\n| ",
            "\n|-|-|\n",
            "\n    ",
            "\n\t",
            "\r\n",
            "\n[a]: ",
            "\n[^a]: ",
        ]);
        let token = prop_oneof![3 => word, 2 => syntax];
        prop::collection::vec(token, 1..24).prop_map(|tokens| tokens.concat())
    }

    /// Any characters at all, with extra weight on the ones that matter to Markdown.
    fn arbitrary_input() -> impl Strategy<Value = String> {
        let ch = prop_oneof![
            4 => any::<char>(),
            3 => prop::sample::select(vec!['*', '_', '`', '[', ']', '(', ')', '<', '>', '&', '|', '\\', '#', '-', '~', ' ', '\t', '\n', '!', 'w', 'h', '@', ':', '/', '.', '{', '}']),
            3 => prop::sample::select(vec!['あ', 'é', 'ß', '🎉', '\u{301}', '\u{3000}', '\u{a0}', '。', '＊']),
        ];
        prop::collection::vec(ch, 0..40).prop_map(|chars| chars.into_iter().collect())
    }

    /// Closed fences whose info string and content are multibyte, alone or inside a container.
    fn unicode_fence_input() -> impl Strategy<Value = String> {
        let text = prop::sample::select(vec![
            "あ",
            "日本語 x",
            "🎉",
            "é",
            "ß",
            "ｗｗｗ",
            "a b",
            "",
            "> あ",
            "- 🎉",
            "\u{3000}",
        ]);
        (
            prop::sample::select(vec!["", "> ", "- ", "  ", "1. "]),
            prop::sample::select(vec![("```", "```"), ("~~~", "~~~"), ("````", "````"), ("$$", "$$")]),
            text.clone(),
            text.clone(),
            text,
        )
            .prop_map(|(container, (open, close), info, first, second)| {
                let pad = if container == "> " {
                    "> ".to_string()
                } else {
                    " ".repeat(container.len())
                };
                let lead = |line: &str, first: bool| {
                    if first {
                        format!("{container}{line}")
                    } else {
                        format!("{pad}{line}")
                    }
                };
                [
                    lead(&format!("{open}{info}"), true),
                    lead(first, false),
                    lead(second, false),
                    lead(close, false),
                ]
                .join("\n")
            })
    }

    /// Checks what has to hold for any input: nothing panics, and every position lies inside the
    /// document and does not end before it starts.
    fn assert_valid(input: &str) {
        let lines = 1 + input.matches(['\n', '\r']).count() - input.matches("\r\n").count();
        let mut stack = parse(input).expect("Markdown always parses");
        stack.extend(parse_mdx(input).unwrap_or_default());
        while let Some(node) = stack.pop() {
            if let Some(position) = node.position() {
                let (start, end) = (&position.start, &position.end);
                let ordered = (start.line, start.column) <= (end.line, end.column);
                assert!(
                    ordered && start.line >= 1 && start.column >= 1 && end.line <= lines,
                    "{node:?} has a position outside of {input:?}"
                );
            }
            stack.extend(node.children());
        }
        let _ = to_html(input);
    }

    /// Runs `strategy` with a fixed seed so the outcome never varies between runs. `PROPTEST_CASES`
    /// and `PARSER_TEST_SEED` widen the exploration.
    fn check(strategy: impl Strategy<Value = String>) {
        let config = ProptestConfig {
            cases: env_number("PROPTEST_CASES").unwrap_or(3000),
            failure_persistence: None,
            ..ProptestConfig::default()
        };
        let seed = env_number("PARSER_TEST_SEED").unwrap_or(7);
        let mut runner = TestRunner::new_with_rng(config, TestRng::from_seed(RngAlgorithm::ChaCha, &[seed; 32]));
        let result = runner.run(&strategy, |input| {
            assert_valid(&input);
            Ok(())
        });
        if let Err(error) = result {
            panic!("{error}");
        }
    }

    #[test]
    fn block_structure_is_valid() {
        check(block_input());
    }

    #[test]
    fn inline_syntax_is_valid() {
        check(inline_input());
    }

    #[test]
    fn mdx_syntax_is_valid() {
        check(mdx_input());
    }

    #[test]
    fn multibyte_syntax_is_valid() {
        check(unicode_input());
    }

    #[test]
    fn multibyte_fences_are_valid() {
        check(unicode_fence_input());
    }

    #[test]
    fn arbitrary_text_is_valid() {
        check(arbitrary_input());
    }

    /// Every control character and unusual space or separator at each syntax position.
    #[test]
    fn special_characters_are_valid() {
        let chars = (0u32..=0x9f)
            .chain([
                0xa0, 0xad, 0x1680, 0x180e, 0x2000, 0x200a, 0x200b, 0x200d, 0x2028, 0x2029, 0x202f, 0x205f, 0x2060,
                0x3000, 0xfeff, 0xfffd, 0xfe0f,
            ])
            .filter_map(char::from_u32);
        let templates = [
            "[a]: {}",
            "[a]: x{}y",
            "[a]: <{}>",
            "[a]: /u '{}'",
            "[a]: /u {}",
            "[a]\n\n[a]: {}x\n\n[a]",
            "[a]({})",
            "[a](x{}y)",
            "[a](<{}>)",
            "[a](/u '{}')",
            "[a](/u \"{}\")",
            "![{}](x)",
            "[{}](x)",
            "[x{}]: y",
            "<http://a{}b>",
            "<a@b{}c>",
            "<{}a>",
            "<a{}b>",
            "<a {}b=c>",
            "<a b={}c>",
            "<a b=\"{}\">",
            "http://a{}b.c",
            "www.a{}b.c",
            "www.a.b/{}x",
            "x@y{}z.com",
            "{}x@y.com",
            "a{}b",
            "{}a",
            "a{}",
            "a {}",
            "{} a",
            "# {}",
            "# a{}",
            "#{}a",
            "{}# a",
            "> {}",
            "- {}",
            "-{}a",
            "1.{}a",
            "1. {}",
            "```{}",
            "```a{}\nx\n```",
            "```\n{}\n```",
            "~~~{}",
            "$${}",
            "$$\n{}\n$$",
            "`{}`",
            "`a{}b`",
            "*{}*",
            "*a{}*",
            "{}*a*",
            "*a*{}",
            "**{}**",
            "_a{}_",
            "_{}a_",
            "~~a{}~~",
            "~~{}~~",
            "a\\{}",
            "&{};",
            "&a{};",
            "&#{};",
            "&#x{};",
            "&amp{}",
            "{}\n---",
            "a{}\n---",
            "a\n{}---",
            "---{}",
            "***{}",
            "- - {}",
            "| a{} | b |\n|-|-|",
            "| {} |\n|-|\n| {} |",
            "|{}|\n|{}|",
            "[^a]: {}",
            "[^a{}]: x",
            "[^a]\n\n[^a]: {}x",
            "x\n\n    {}",
            "x\n\n    a{}",
            "<!-- {} -->",
            "<div>{}",
            "<div {}>",
            "<?{}?>",
            "<![CDATA[{}]]>",
            "---\n{}\n---\na",
            "+++\n{}\n+++\na",
            "{}  \nb",
            "a{}\nb",
            "a\n{}b",
        ];
        for char in chars {
            for template in templates {
                assert_valid(&template.replace("{}", &char.to_string()));
            }
        }
    }

    const SPEC_URL: &str =
        "https://raw.githubusercontent.com/github/cmark-gfm/828322d1ee4facdab56f0d3edccb13e9af90dcd2/test/spec.txt";

    const EXTENSIONS_SPEC_URL: &str = "https://raw.githubusercontent.com/github/cmark-gfm/828322d1ee4facdab56f0d3edccb13e9af90dcd2/test/extensions.txt";

    const COMMONMARK_SPEC_URL: &str = "https://raw.githubusercontent.com/commonmark/commonmark-spec/0.31.2/spec.txt";

    /// The markdown and the HTML of each example of the `CommonMark` and GFM `spec.txt`, with its number.
    fn spec_examples(text: &str) -> Vec<(usize, String, String)> {
        let fence = "`".repeat(32);
        let open = format!("{fence} example");
        let mut examples = Vec::new();
        let mut lines = text.lines();

        while let Some(line) = lines.next() {
            if line != open {
                continue;
            }
            let markdown = lines.by_ref().take_while(|line| *line != ".").collect::<Vec<_>>();
            let html = lines.by_ref().take_while(|line| *line != fence).collect::<Vec<_>>();
            // `→` stands for a tab in the spec.
            let text = |lines: Vec<&str>| lines.join("\n").replace('\u{2192}', "\t");
            let html = text(html);
            let html = if html.is_empty() { html } else { html + "\n" };
            examples.push((examples.len() + 1, text(markdown) + "\n", html));
        }
        examples
    }

    /// Examples of the GFM spec, which is based on `CommonMark` 0.29, that are not rendered as in the
    /// spec, by number:
    ///
    /// - a document that starts with `---` has frontmatter (66, 68)
    /// - nested `strong` of runs of `*` and `_`, which `CommonMark` 0.30 and later render as nested
    ///   elements where 0.29 merged them (388, 416, 424, 425, 426, 463, 464, 465, 467)
    /// - links with a protocol other than http, https, irc, ircs, mailto and xmpp have no `href`, as
    ///   `markdown-rs` does it by default (496, 594, 595, 597)
    /// - GFM autolink literals, which the spec does not have (598, 604, 607, 608)
    /// - the GFM tag filter, which writes the tags `script`, `style` and `textarea` as text (140, 141,
    ///   142, 145, 147)
    const KNOWN_DIFFERENCES: &[usize] = &[
        66, 68, 140, 141, 142, 145, 147, 388, 416, 424, 425, 426, 463, 464, 465, 467, 496, 594, 595, 597, 598, 604,
        607, 608,
    ];

    /// Examples of the `CommonMark` 0.31.2 spec that are not rendered as in the spec, by number:
    ///
    /// - a document that starts with `---` has frontmatter (96, 98)
    /// - links with a protocol other than http, https, irc, ircs, mailto and xmpp have no `href` (500,
    ///   598, 599, 601)
    /// - GFM autolink literals (602, 608, 611, 612)
    /// - the GFM tag filter, which writes the tags `script`, `style` and `textarea` as text (170, 171, 172,
    ///   173, 176, 178)
    const COMMONMARK_KNOWN_DIFFERENCES: &[usize] = &[
        96, 98, 170, 171, 172, 173, 176, 178, 500, 598, 599, 601, 602, 608, 611, 612,
    ];

    /// Examples of the GFM extensions spec that are not rendered as in the spec, by number:
    ///
    /// - input that must only not crash, whose output the spec leaves out (20)
    /// - the HTML of footnotes, which is the one of the current micromark and GitHub (23, 24, 25)
    const EXTENSIONS_KNOWN_DIFFERENCES: &[usize] = &[20, 23, 24, 25];

    /// Fetches `url`, or reads the file in the environment variable `var`.
    fn spec_text(var: &str, url: &str) -> String {
        match std::env::var(var) {
            Ok(path) => std::fs::read_to_string(path).unwrap(),
            Err(_) => ureq::get(url).call().unwrap().into_body().read_to_string().unwrap(),
        }
    }

    /// The numbers of the examples of `text` that are not rendered as in the spec.
    fn differing_examples(text: &str, minimum: usize) -> Vec<usize> {
        let examples = spec_examples(text);
        assert!(examples.len() > minimum, "only {} examples were found", examples.len());
        examples
            .iter()
            .filter(|(_, markdown, html)| to_html(markdown) != *html)
            .map(|(number, ..)| *number)
            .collect()
    }

    /// Renders every example of the spec to HTML, from `SPEC_FILE` or fetched over the network.
    #[test]
    #[ignore = "fetches spec.txt over the network; set SPEC_FILE to use a local copy"]
    fn spec_examples_render_as_in_the_spec() {
        assert_eq!(
            differing_examples(&spec_text("SPEC_FILE", SPEC_URL), 600),
            KNOWN_DIFFERENCES,
            "examples that are not rendered as in the spec"
        );
    }

    /// Renders every example of the `CommonMark` 0.31.2 spec to HTML, from `COMMONMARK_SPEC_FILE` or
    /// fetched over the network.
    #[test]
    #[ignore = "fetches spec.txt over the network; set COMMONMARK_SPEC_FILE to use a local copy"]
    fn commonmark_spec_examples_render_as_in_the_spec() {
        assert_eq!(
            differing_examples(&spec_text("COMMONMARK_SPEC_FILE", COMMONMARK_SPEC_URL), 600),
            COMMONMARK_KNOWN_DIFFERENCES,
            "examples that are not rendered as in the spec"
        );
    }

    /// Renders every example of the GFM extensions spec to HTML, from `EXTENSIONS_SPEC_FILE` or fetched
    /// over the network.
    #[test]
    #[ignore = "fetches extensions.txt over the network; set EXTENSIONS_SPEC_FILE to use a local copy"]
    fn extensions_spec_examples_render_as_in_the_spec() {
        assert_eq!(
            differing_examples(&spec_text("EXTENSIONS_SPEC_FILE", EXTENSIONS_SPEC_URL), 20),
            EXTENSIONS_KNOWN_DIFFERENCES,
            "examples that are not rendered as in the spec"
        );
    }
}
