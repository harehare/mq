//! Parameterized tests for the HTML that `to_html` renders: every construct, the line endings, the
//! escaping, and the URLs. Each case lists the input and the exact output that is expected.

use mq_markdown::{Markdown, to_html};
use rstest::rstest;

#[rstest]
#[case::paragraph("a\n", "<p>a</p>\n")]
#[case::paragraph_without_line_ending("a", "<p>a</p>")]
#[case::two_paragraphs("a\n\nb\n", "<p>a</p>\n<p>b</p>\n")]
#[case::soft_break("a\nb\n", "<p>a\nb</p>\n")]
#[case::hard_break_spaces("a  \nb\n", "<p>a<br />\nb</p>\n")]
#[case::hard_break_backslash("a\\\nb\n", "<p>a<br />\nb</p>\n")]
#[case::atx_heading("# a\n", "<h1>a</h1>\n")]
#[case::atx_closing_sequence("## a ##\n", "<h2>a</h2>\n")]
#[case::setext_one("a\n===\n", "<h1>a</h1>\n")]
#[case::setext_two("a\n---\n", "<h2>a</h2>\n")]
#[case::heading_with_inline("# a *b* `c`\n", "<h1>a <em>b</em> <code>c</code></h1>\n")]
#[case::thematic_break("---\n", "<hr />\n")]
#[case::thematic_break_after_paragraph("a\n\n***\n", "<p>a</p>\n<hr />\n")]
#[case::empty("", "")]
#[case::blank_lines("\n\n  \n", "")]
#[case::code_keeps_rest_of_tab_in_list(
    "- foo\n\n\t\tbar\n",
    "<ul>\n<li>\n<p>foo</p>\n<pre><code>  bar\n</code></pre>\n</li>\n</ul>\n"
)]
#[case::code_keeps_rest_of_tab_in_quote(">\t\tfoo\n", "<blockquote>\n<pre><code>  foo\n</code></pre>\n</blockquote>\n")]
#[case::heading_keeps_a_leading_hash("# #[allow(dead_code)]\n", "<h1>#[allow(dead_code)]</h1>\n")]
#[case::heading_keeps_a_hash_and_a_space("# # a\n", "<h1># a</h1>\n")]
#[case::code_span_over_lines_loses_indent("a `b\n    c` d\n", "<p>a <code>b c</code> d</p>\n")]
#[case::lazy_line_of_backticks_after_quote("> q\n``", "<blockquote>\n<p>q\n``</p>\n</blockquote>")]
#[case::quote_can_start_with_code_after_paragraph(
    "text\n>     code\n",
    "<p>text</p>\n<blockquote>\n<pre><code>code\n</code></pre>\n</blockquote>\n"
)]
#[case::quote_can_start_with_ordered_item_after_paragraph(
    "text\n> 2. a\n",
    "<p>text</p>\n<blockquote>\n<ol start=\"2\">\n<li>a</li>\n</ol>\n</blockquote>\n"
)]
#[case::ordered_item_does_not_interrupt_after_quote(
    "> q\n\nfoo\n2. a\n",
    "<blockquote>\n<p>q</p>\n</blockquote>\n<p>foo\n2. a</p>\n"
)]
#[case::dashes_after_setext_heading_are_a_rule("a\n---\n---\nb\n", "<h2>a</h2>\n<hr />\n<p>b</p>\n")]
#[case::text_after_setext_heading_in_item_is_a_paragraph(
    "- a\n  ---\nb\n",
    "<ul>\n<li>\n<h2>a</h2>\n</li>\n</ul>\n<p>b</p>\n"
)]
#[case::code_of_an_item_is_one_block(
    "a\n-       b\n        c\n",
    "<p>a</p>\n<ul>\n<li>\n<pre><code>  b\n  c\n</code></pre>\n</li>\n</ul>\n"
)]
#[case::task_without_text("- [ ] \n", "<ul>\n<li><input type=\"checkbox\" disabled=\"\" /> </li>\n</ul>\n")]
#[case::task_without_text_keeps_the_list(
    "- [x] \n- b\n",
    "<ul>\n<li><input type=\"checkbox\" checked=\"\" disabled=\"\" /> </li>\n<li>b</li>\n</ul>\n"
)]
#[case::task_text_is_not_a_list(
    "- [x] - a\n",
    "<ul>\n<li><input type=\"checkbox\" checked=\"\" disabled=\"\" /> - a</li>\n</ul>\n"
)]
#[case::task_text_is_not_a_quote(
    "- [ ] > a\n",
    "<ul>\n<li><input type=\"checkbox\" disabled=\"\" /> &gt; a</li>\n</ul>\n"
)]
#[case::task_on_the_next_line(
    "- [ ] \n  b\n",
    "<ul>\n<li><input type=\"checkbox\" disabled=\"\" /> b</li>\n</ul>\n"
)]
#[case::task_after_empty_marker(
    "-\n  [ ] b\n",
    "<ul>\n<li><input type=\"checkbox\" disabled=\"\" /> b</li>\n</ul>\n"
)]
#[case::checked_task_after_empty_marker(
    "-\n  [x] b\n",
    "<ul>\n<li><input type=\"checkbox\" checked=\"\" disabled=\"\" /> b</li>\n</ul>\n"
)]
#[case::raw_html_ignores_other_closing_tags(
    "<script>\n</style>\n*hello*\n</script>\n",
    "&lt;script>\n&lt;/style>\n*hello*\n&lt;/script>\n"
)]
#[case::footnote_with_blank_line_after_marker(
    "a[^1]\n\n[^1]:\n\n    text\n",
    "<p>a<sup><a href=\"#user-content-fn-1\" id=\"user-content-fnref-1\" data-footnote-ref=\"\" aria-describedby=\"footnote-label\">1</a></sup></p>\n<section data-footnotes=\"\" class=\"footnotes\"><h2 id=\"footnote-label\" class=\"sr-only\">Footnotes</h2>\n<ol>\n<li id=\"user-content-fn-1\">\n<p>text <a href=\"#user-content-fnref-1\" data-footnote-backref=\"\" aria-label=\"Back to content\" class=\"data-footnote-backref\">↩</a></p>\n</li>\n</ol>\n</section>\n"
)]
#[case::tag_filter_inline("a <xmp> b <b>c</b>\n", "<p>a &lt;xmp> b <b>c</b></p>\n")]
#[case::tag_filter_end_tag_and_case("a </SCRIPT> b\n", "<p>a &lt;/SCRIPT> b</p>\n")]
#[case::tag_filter_block("<script>\nx\n</script>\n", "&lt;script>\nx\n&lt;/script>\n")]
#[case::tag_filter_keeps_other_tags("<scripts>\n", "<scripts>\n")]
#[case::autolink_email_after_a_slash(
    "x@y.com/z@w.org\n",
    "<p><a href=\"mailto:x@y.com\">x@y.com</a>/<a href=\"mailto:z@w.org\">z@w.org</a></p>\n"
)]
#[case::autolink_domain_with_emoji(
    "http://x\u{1F344}.ga/\n",
    "<p><a href=\"http://x%F0%9F%8D%84.ga/\">http://x\u{1F344}.ga/</a></p>\n"
)]
#[case::autolink_email_before_an_at_sign("x@y.com@\n", "<p>x@y.com@</p>\n")]
#[case::emphasis_after_a_word_before_punctuation("r*_q* x\n", "<p>r*_q* x</p>\n")]
#[case::emphasis_next_to_other_runs("]_**é日*\n", "<p>]_*<em>é日</em></p>\n")]
#[case::hard_break_in_image_alt("![a\\\nb](/u)\n", "<p><img src=\"/u\" alt=\"a\nb\" /></p>\n")]
#[case::image_in_image_alt("![foo ![bar](/url)](/url2)\n", "<p><img src=\"/url2\" alt=\"foo bar\" /></p>\n")]
fn blocks(#[case] input: &str, #[case] expected: &str) {
    assert_eq!(to_html(input), expected);
}

#[rstest]
#[case::emphasis("*a*\n", "<p><em>a</em></p>\n")]
#[case::emphasis_underscore("_a_\n", "<p><em>a</em></p>\n")]
#[case::strong("**a**\n", "<p><strong>a</strong></p>\n")]
#[case::strong_emphasis("***a***\n", "<p><em><strong>a</strong></em></p>\n")]
#[case::strikethrough("~~a~~\n", "<p><del>a</del></p>\n")]
#[case::strikethrough_single_tilde("~a~\n", "<p><del>a</del></p>\n")]
#[case::code_span("`a`\n", "<p><code>a</code></p>\n")]
#[case::code_span_trim("` a `\n", "<p><code>a</code></p>\n")]
#[case::code_span_keeps_inner_spaces("`a  b`\n", "<p><code>a  b</code></p>\n")]
#[case::code_span_line_ending("`a\nb`\n", "<p><code>a b</code></p>\n")]
#[case::code_span_escapes_html("`<a>&`\n", "<p><code>&lt;a&gt;&amp;</code></p>\n")]
#[case::math_inline("$a$\n", "<p><code class=\"language-math math-inline\">a</code></p>\n")]
#[case::escape("\\*a\\*\n", "<p>*a*</p>\n")]
#[case::entity("&copy; &#35; &#x41;\n", "<p>© # A</p>\n")]
#[case::unknown_entity("&nope;\n", "<p>&amp;nope;</p>\n")]
#[case::control_character_reference("&#0; &#1; &#x7f;\n", "<p>\u{fffd} \u{fffd} \u{fffd}</p>\n")]
#[case::special_characters("a < b > c & \"d\"\n", "<p>a &lt; b &gt; c &amp; &quot;d&quot;</p>\n")]
#[case::nul("a\0b\n", "<p>a\u{fffd}b</p>\n")]
#[case::raw_html_inline("a <b>c</b> d\n", "<p>a <b>c</b> d</p>\n")]
#[case::raw_html_inline_multiline("<a\n   b=\"c\">x</a>\n", "<p><a\nb=\"c\">x</a></p>\n")]
fn inline(#[case] input: &str, #[case] expected: &str) {
    assert_eq!(to_html(input), expected);
}

#[rstest]
#[case::link("[a](b)\n", "<p><a href=\"b\">a</a></p>\n")]
#[case::link_title("[a](b \"c\")\n", "<p><a href=\"b\" title=\"c\">a</a></p>\n")]
#[case::link_title_escaped("[a](b \"<&>\")\n", "<p><a href=\"b\" title=\"&lt;&amp;&gt;\">a</a></p>\n")]
#[case::link_emphasis("[*a*](b)\n", "<p><a href=\"b\"><em>a</em></a></p>\n")]
#[case::link_angle_destination("[a](<b c>)\n", "<p><a href=\"b%20c\">a</a></p>\n")]
#[case::link_parentheses("[a](b(c))\n", "<p><a href=\"b(c)\">a</a></p>\n")]
#[case::link_non_ascii(
    "[a](http://あ.jp/パス)\n",
    "<p><a href=\"http://%E3%81%82.jp/%E3%83%91%E3%82%B9\">a</a></p>\n"
)]
#[case::link_keeps_percent_encoding("[a](b%20c)\n", "<p><a href=\"b%20c\">a</a></p>\n")]
#[case::link_encodes_quote("[a](b\"c)\n", "<p><a href=\"b%22c\">a</a></p>\n")]
#[case::link_ampersand("[a](b?c=1&d=2)\n", "<p><a href=\"b?c=1&amp;d=2\">a</a></p>\n")]
#[case::link_entity("[a](b&amp;c)\n", "<p><a href=\"b&amp;c\">a</a></p>\n")]
#[case::link_empty("[a]()\n", "<p><a href=\"\">a</a></p>\n")]
#[case::link_relative("[a](../b#c)\n", "<p><a href=\"../b#c\">a</a></p>\n")]
#[case::link_http("[a](HTTP://b)\n", "<p><a href=\"HTTP://b\">a</a></p>\n")]
#[case::link_mailto("[a](mailto:b@c)\n", "<p><a href=\"mailto:b@c\">a</a></p>\n")]
#[case::link_javascript_dropped("[a](javascript:b)\n", "<p><a href=\"\">a</a></p>\n")]
#[case::link_javascript_mixed_case_dropped("[a](JaVaScRiPt:b)\n", "<p><a href=\"\">a</a></p>\n")]
#[case::link_data_dropped("[a](data:text/html,b)\n", "<p><a href=\"\">a</a></p>\n")]
#[case::link_colon_after_path_is_relative("[a](b/c:d)\n", "<p><a href=\"b/c:d\">a</a></p>\n")]
#[case::link_colon_after_query_is_relative("[a](b?c:d)\n", "<p><a href=\"b?c:d\">a</a></p>\n")]
#[case::image("![a](b)\n", "<p><img src=\"b\" alt=\"a\" /></p>\n")]
#[case::image_title("![a](b \"c\")\n", "<p><img src=\"b\" alt=\"a\" title=\"c\" /></p>\n")]
#[case::image_alt_is_plain_text("![a *b* `c`](d)\n", "<p><img src=\"d\" alt=\"a b c\" /></p>\n")]
#[case::image_alt_escaped("![a\"b](c)\n", "<p><img src=\"c\" alt=\"a&quot;b\" /></p>\n")]
#[case::image_data_dropped("![a](data:b)\n", "<p><img src=\"\" alt=\"a\" /></p>\n")]
#[case::image_mailto_dropped("![a](mailto:b)\n", "<p><img src=\"\" alt=\"a\" /></p>\n")]
#[case::reference("[a]\n\n[a]: /u \"t\"\n", "<p><a href=\"/u\" title=\"t\">a</a></p>\n")]
#[case::reference_full("[a][b]\n\n[b]: /u\n", "<p><a href=\"/u\">a</a></p>\n")]
#[case::reference_collapsed("[a][]\n\n[a]: /u\n", "<p><a href=\"/u\">a</a></p>\n")]
#[case::reference_case_insensitive("[A]\n\n[a]: /u\n", "<p><a href=\"/u\">A</a></p>\n")]
#[case::reference_first_definition_wins("[a]\n\n[a]: /one\n[a]: /two\n", "<p><a href=\"/one\">a</a></p>\n")]
#[case::reference_before_definition("[a]: /u\n\n[a]\n", "<p><a href=\"/u\">a</a></p>\n")]
#[case::reference_undefined("[a]\n", "<p>[a]</p>\n")]
#[case::image_reference("![a]\n\n[a]: /u\n", "<p><img src=\"/u\" alt=\"a\" /></p>\n")]
#[case::definition_alone("[a]: /u\n", "")]
#[case::autolink("<http://a.b>\n", "<p><a href=\"http://a.b\">http://a.b</a></p>\n")]
#[case::autolink_email("<a@b.c>\n", "<p><a href=\"mailto:a@b.c\">a@b.c</a></p>\n")]
#[case::autolink_escapes(
    "<http://a.b/?c=1&d=2>\n",
    "<p><a href=\"http://a.b/?c=1&amp;d=2\">http://a.b/?c=1&amp;d=2</a></p>\n"
)]
#[case::autolink_literal_www("www.a.com\n", "<p><a href=\"http://www.a.com\">www.a.com</a></p>\n")]
#[case::autolink_literal_http(
    "see http://a.com/b.\n",
    "<p>see <a href=\"http://a.com/b\">http://a.com/b</a>.</p>\n"
)]
#[case::autolink_literal_email("a@b.com\n", "<p><a href=\"mailto:a@b.com\">a@b.com</a></p>\n")]
#[case::autolink_literal_in_link_is_text(
    "[http://a.b](http://c.d)\n",
    "<p><a href=\"http://c.d\">http://a.b</a></p>\n"
)]
#[case::autolink_literal_non_ascii(
    "http://あ.jp/パス\n",
    "<p><a href=\"http://%E3%81%82.jp/%E3%83%91%E3%82%B9\">http://あ.jp/パス</a></p>\n"
)]
fn links_and_images(#[case] input: &str, #[case] expected: &str) {
    assert_eq!(to_html(input), expected);
}

#[rstest]
#[case::fenced("```\na\n```\n", "<pre><code>a\n</code></pre>\n")]
#[case::fenced_language("```rust\na\n```\n", "<pre><code class=\"language-rust\">a\n</code></pre>\n")]
#[case::fenced_meta_is_dropped("```rust title=x\na\n```\n", "<pre><code class=\"language-rust\">a\n</code></pre>\n")]
#[case::fenced_language_escaped("```a\"b\nc\n```\n", "<pre><code class=\"language-a&quot;b\">c\n</code></pre>\n")]
#[case::fenced_empty("```\n```\n", "<pre><code></code></pre>\n")]
#[case::fenced_blank_line("```\n\n```\n", "<pre><code>\n</code></pre>\n")]
#[case::fenced_blank_lines_kept("```\na\n\n\n```\n", "<pre><code>a\n\n\n</code></pre>\n")]
#[case::fenced_escapes_html("```\n<a>&\n```\n", "<pre><code>&lt;a&gt;&amp;\n</code></pre>\n")]
#[case::fenced_tilde("~~~\na\n~~~\n", "<pre><code>a\n</code></pre>\n")]
#[case::fenced_indent_removed("  ```\n  a\n   b\n  ```\n", "<pre><code>a\n b\n</code></pre>\n")]
#[case::fenced_unclosed("```\na", "<pre><code>a\n</code></pre>\n")]
#[case::fenced_unclosed_empty("```\n", "<pre><code></code></pre>\n")]
#[case::fenced_without_final_line_ending("```\na\n```", "<pre><code>a\n</code></pre>")]
#[case::indented("    a\n", "<pre><code>a\n</code></pre>\n")]
#[case::indented_blank_line_inside("    a\n\n    b\n", "<pre><code>a\n\nb\n</code></pre>\n")]
#[case::indented_trailing_blank_lines_dropped("    a\n\n\n", "<pre><code>a\n</code></pre>\n")]
#[case::math("$$\na\n$$\n", "<pre><code class=\"language-math math-display\">a\n</code></pre>\n")]
#[case::html_block("<div>\na\n</div>\n", "<div>\na\n</div>\n")]
#[case::html_block_then_paragraph("<div>\n\na\n", "<div>\n<p>a</p>\n")]
#[case::html_comment("<!-- a -->\n", "<!-- a -->\n")]
#[case::frontmatter_is_dropped("---\na: b\n---\n# c\n", "<h1>c</h1>\n")]
#[case::toml_frontmatter_is_dropped("+++\na = 1\n+++\n# c\n", "<h1>c</h1>\n")]
fn code_and_html(#[case] input: &str, #[case] expected: &str) {
    assert_eq!(to_html(input), expected);
}

#[rstest]
#[case::quote("> a\n", "<blockquote>\n<p>a</p>\n</blockquote>\n")]
#[case::quote_lazy("> a\nb\n", "<blockquote>\n<p>a\nb</p>\n</blockquote>\n")]
#[case::quote_empty(">\n", "<blockquote>\n</blockquote>\n")]
#[case::quote_nested("> > a\n", "<blockquote>\n<blockquote>\n<p>a</p>\n</blockquote>\n</blockquote>\n")]
#[case::quote_with_heading_and_code(
    "> # a\n> ```\n> b\n> ```\n",
    "<blockquote>\n<h1>a</h1>\n<pre><code>b\n</code></pre>\n</blockquote>\n"
)]
#[case::quote_then_paragraph("> a\n\nb\n", "<blockquote>\n<p>a</p>\n</blockquote>\n<p>b</p>\n")]
#[case::quote_with_list("> - a\n", "<blockquote>\n<ul>\n<li>a</li>\n</ul>\n</blockquote>\n")]
#[case::unordered("- a\n- b\n", "<ul>\n<li>a</li>\n<li>b</li>\n</ul>\n")]
#[case::ordered("1. a\n2. b\n", "<ol>\n<li>a</li>\n<li>b</li>\n</ol>\n")]
#[case::ordered_start("3. a\n4. b\n", "<ol start=\"3\">\n<li>a</li>\n<li>b</li>\n</ol>\n")]
#[case::ordered_start_zero("0. a\n", "<ol start=\"0\">\n<li>a</li>\n</ol>\n")]
#[case::ordered_paren("1) a\n", "<ol>\n<li>a</li>\n</ol>\n")]
#[case::list_loose("- a\n\n- b\n", "<ul>\n<li>\n<p>a</p>\n</li>\n<li>\n<p>b</p>\n</li>\n</ul>\n")]
#[case::list_loose_item_with_two_blocks(
    "- a\n\n  b\n- c\n",
    "<ul>\n<li>\n<p>a</p>\n<p>b</p>\n</li>\n<li>\n<p>c</p>\n</li>\n</ul>\n"
)]
#[case::list_tight_with_nested("- a\n  - b\n", "<ul>\n<li>a\n<ul>\n<li>b</li>\n</ul>\n</li>\n</ul>\n")]
#[case::list_tight_with_code(
    "- a\n  ```\n  b\n  ```\n",
    "<ul>\n<li>a\n<pre><code>b\n</code></pre>\n</li>\n</ul>\n"
)]
#[case::list_tight_with_heading("- # a\n- b\n", "<ul>\n<li>\n<h1>a</h1>\n</li>\n<li>b</li>\n</ul>\n")]
#[case::list_empty_item("-\n- a\n", "<ul>\n<li></li>\n<li>a</li>\n</ul>\n")]
#[case::list_changed_marker_is_new_list("- a\n* b\n", "<ul>\n<li>a</li>\n</ul>\n<ul>\n<li>b</li>\n</ul>\n")]
#[case::list_after_paragraph("a\n- b\n", "<p>a</p>\n<ul>\n<li>b</li>\n</ul>\n")]
#[case::list_blank_lines_after_are_not_loose("- a\n\nb\n", "<ul>\n<li>a</li>\n</ul>\n<p>b</p>\n")]
#[case::list_quote_in_item("- > a\n", "<ul>\n<li>\n<blockquote>\n<p>a</p>\n</blockquote>\n</li>\n</ul>\n")]
#[case::task_unchecked("- [ ] a\n", "<ul>\n<li><input type=\"checkbox\" disabled=\"\" /> a</li>\n</ul>\n")]
#[case::task_checked(
    "- [x] a\n",
    "<ul>\n<li><input type=\"checkbox\" checked=\"\" disabled=\"\" /> a</li>\n</ul>\n"
)]
#[case::task_loose(
    "- [ ] a\n\n- b\n",
    "<ul>\n<li>\n<p><input type=\"checkbox\" disabled=\"\" /> a</p>\n</li>\n<li>\n<p>b</p>\n</li>\n</ul>\n"
)]
fn containers(#[case] input: &str, #[case] expected: &str) {
    assert_eq!(to_html(input), expected);
}

#[rstest]
#[case::table(
    "| a | b |\n|---|---|\n| c | d |\n",
    "<table>\n<thead>\n<tr>\n<th>a</th>\n<th>b</th>\n</tr>\n</thead>\n<tbody>\n<tr>\n<td>c</td>\n<td>d</td>\n</tr>\n</tbody>\n</table>\n"
)]
#[case::table_alignment(
    "| a | b | c |\n|:--|:-:|--:|\n| d | e | f |\n",
    "<table>\n<thead>\n<tr>\n<th align=\"left\">a</th>\n<th align=\"center\">b</th>\n<th align=\"right\">c</th>\n</tr>\n</thead>\n<tbody>\n<tr>\n<td align=\"left\">d</td>\n<td align=\"center\">e</td>\n<td align=\"right\">f</td>\n</tr>\n</tbody>\n</table>\n"
)]
#[case::table_without_body("| a |\n|---|\n", "<table>\n<thead>\n<tr>\n<th>a</th>\n</tr>\n</thead>\n</table>\n")]
#[case::table_short_row_is_filled(
    "| a | b |\n|---|---|\n| c |\n",
    "<table>\n<thead>\n<tr>\n<th>a</th>\n<th>b</th>\n</tr>\n</thead>\n<tbody>\n<tr>\n<td>c</td>\n<td></td>\n</tr>\n</tbody>\n</table>\n"
)]
#[case::table_long_row_is_cut(
    "| a |\n|---|\n| b | c |\n",
    "<table>\n<thead>\n<tr>\n<th>a</th>\n</tr>\n</thead>\n<tbody>\n<tr>\n<td>b</td>\n</tr>\n</tbody>\n</table>\n"
)]
#[case::table_without_outer_pipes(
    "a | b\n-|-\nc | d\n",
    "<table>\n<thead>\n<tr>\n<th>a</th>\n<th>b</th>\n</tr>\n</thead>\n<tbody>\n<tr>\n<td>c</td>\n<td>d</td>\n</tr>\n</tbody>\n</table>\n"
)]
#[case::table_inline_content(
    "| *a* | `b` |\n|---|---|\n| [c](d) | ~~e~~ |\n",
    "<table>\n<thead>\n<tr>\n<th><em>a</em></th>\n<th><code>b</code></th>\n</tr>\n</thead>\n<tbody>\n<tr>\n<td><a href=\"d\">c</a></td>\n<td><del>e</del></td>\n</tr>\n</tbody>\n</table>\n"
)]
#[case::table_escaped_pipe(
    "| a \\| b |\n|---|\n",
    "<table>\n<thead>\n<tr>\n<th>a | b</th>\n</tr>\n</thead>\n</table>\n"
)]
#[case::table_ended_by_blank_line(
    "| a |\n|---|\n| b |\n\nc\n",
    "<table>\n<thead>\n<tr>\n<th>a</th>\n</tr>\n</thead>\n<tbody>\n<tr>\n<td>b</td>\n</tr>\n</tbody>\n</table>\n<p>c</p>\n"
)]
#[case::table_multibyte(
    "| あ | 🎉 |\n|---|---|\n| é | ß |\n",
    "<table>\n<thead>\n<tr>\n<th>あ</th>\n<th>🎉</th>\n</tr>\n</thead>\n<tbody>\n<tr>\n<td>é</td>\n<td>ß</td>\n</tr>\n</tbody>\n</table>\n"
)]
fn tables(#[case] input: &str, #[case] expected: &str) {
    assert_eq!(to_html(input), expected);
}

const NOTE_OPEN: &str = "<section data-footnotes=\"\" class=\"footnotes\"><h2 id=\"footnote-label\" class=\"sr-only\">Footnotes</h2>\n<ol>\n";

fn call(id: &str, fnref: &str, number: usize) -> String {
    format!(
        "<sup><a href=\"#user-content-fn-{id}\" id=\"user-content-fnref-{fnref}\" data-footnote-ref=\"\" aria-describedby=\"footnote-label\">{number}</a></sup>"
    )
}

fn back(fnref: &str, sup: Option<usize>) -> String {
    let sup = sup.map(|n| format!("<sup>{n}</sup>")).unwrap_or_default();
    format!(
        "<a href=\"#user-content-fnref-{fnref}\" data-footnote-backref=\"\" aria-label=\"Back to content\" class=\"data-footnote-backref\">\u{21a9}{sup}</a>"
    )
}

#[test]
fn footnote() {
    let expected = format!(
        "<p>a{}</p>\n{NOTE_OPEN}<li id=\"user-content-fn-1\">\n<p>b {}</p>\n</li>\n</ol>\n</section>\n",
        call("1", "1", 1),
        back("1", None)
    );
    assert_eq!(to_html("a[^1]\n\n[^1]: b\n"), expected);
}

#[test]
fn footnote_referenced_twice_has_two_backreferences() {
    let expected = format!(
        "<p>a{} b{}</p>\n{NOTE_OPEN}<li id=\"user-content-fn-1\">\n<p>c {} {}</p>\n</li>\n</ol>\n</section>\n",
        call("1", "1", 1),
        call("1", "1-2", 1),
        back("1", None),
        back("1-2", Some(2))
    );
    assert_eq!(to_html("a[^1] b[^1]\n\n[^1]: c\n"), expected);
}

#[test]
fn footnotes_are_numbered_in_the_order_of_their_references() {
    let html = to_html("a[^b] c[^a]\n\n[^a]: x\n[^b]: y\n");
    let first = html.find("<li id=\"user-content-fn-b\">").unwrap();
    let second = html.find("<li id=\"user-content-fn-a\">").unwrap();
    assert!(first < second, "{html}");
    assert!(
        html.contains(&call("b", "b", 1)) && html.contains(&call("a", "a", 2)),
        "{html}"
    );
}

#[test]
fn footnote_label_is_case_insensitive_and_its_id_is_lowercase() {
    let html = to_html("a[^Ab]\n\n[^ab]: x\n");
    assert!(html.contains("href=\"#user-content-fn-ab\""), "{html}");
}

#[test]
fn footnote_id_is_percent_encoded() {
    let html = to_html("a[^é]\n\n[^é]: x\n");
    assert!(html.contains("user-content-fn-%C3%A9"), "{html}");
}

#[test]
fn footnote_without_definition_is_text() {
    assert_eq!(to_html("a[^1]\n"), "<p>a[^1]</p>\n");
}

#[test]
fn footnote_without_reference_is_not_rendered() {
    assert_eq!(to_html("[^1]: a\n"), "");
}

#[test]
fn footnote_with_a_list_puts_the_backreference_on_its_own_line() {
    let expected = format!(
        "<p>a{}</p>\n{NOTE_OPEN}<li id=\"user-content-fn-1\">\n<ul>\n<li>l</li>\n</ul>\n{}\n</li>\n</ol>\n</section>\n",
        call("1", "1", 1),
        back("1", None)
    );
    assert_eq!(to_html("a[^1]\n\n[^1]:\n    - l\n"), expected);
}

#[test]
fn footnote_with_two_paragraphs_has_the_backreference_in_the_last() {
    let html = to_html("a[^1]\n\n[^1]: p\n\n    q\n");
    assert!(
        html.contains(&format!("<p>p</p>\n<p>q {}</p>", back("1", None))),
        "{html}"
    );
}

#[test]
fn footnote_defined_before_its_reference_is_still_rendered() {
    let html = to_html("[^1]: a\n\nb[^1]\n");
    assert!(html.starts_with("<p>b"), "{html}");
    assert!(html.contains("<li id=\"user-content-fn-1\">"), "{html}");
}

#[test]
fn footnote_reference_inside_a_footnote_is_numbered_when_it_is_met() {
    let html = to_html("[^1]: x [^2]\n[^2]: y\n\nz[^1]\n");
    // `[^2]` is met first, in the definition of `[^1]`.
    assert!(html.contains(&call("2", "2", 1)), "{html}");
    assert!(html.contains(&call("1", "1", 2)), "{html}");
}

#[rstest]
#[case::lf("a\n\nb\n")]
#[case::list("- a\n- b\n")]
#[case::quote("> a\n> b\n")]
#[case::code("```\na\nb\n```\n")]
#[case::table("| a |\n|---|\n| b |\n")]
#[case::footnote("a[^1]\n\n[^1]: b\n")]
#[case::hard_break("a  \nb\n")]
fn crlf_documents_render_with_crlf(#[case] input: &str) {
    let crlf = input.replace('\n', "\r\n");
    assert_eq!(to_html(&crlf), to_html(input).replace('\n', "\r\n"));
}

#[rstest]
#[case::no_final_line_ending("a", false)]
#[case::final_line_ending("a\n", true)]
#[case::heading("# a", false)]
#[case::heading_final_line_ending("# a\n", true)]
#[case::closed_fence("```\na\n```", false)]
#[case::closed_fence_final_line_ending("```\na\n```\n", true)]
#[case::list("- a", false)]
#[case::list_final_line_ending("- a\n", true)]
#[case::table("| a |\n|---|", false)]
#[case::table_final_line_ending("| a |\n|---|\n", true)]
#[case::blank_lines_after("a\n\n\n", true)]
fn trailing_line_ending_follows_the_source(#[case] input: &str, #[case] ends_with_line_ending: bool) {
    assert_eq!(
        to_html(input).ends_with('\n'),
        ends_with_line_ending,
        "{:?}",
        to_html(input)
    );
}

#[rstest]
#[case::paragraph("a *b*\n\n- c\n")]
#[case::link("[a](b \"c\")\n")]
#[case::table("| a | b |\n|:-|-:|\n| c | d |\n")]
#[case::footnote("a[^1]\n\n[^1]: b\n")]
fn to_html_of_a_document_equals_to_html_of_its_source(#[case] input: &str) {
    let document: Markdown = input.parse().unwrap();
    assert_eq!(document.to_html(), to_html(input));
}
