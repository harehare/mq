#[cfg(feature = "html-to-markdown")]
use mq_markdown::{convert_html_to_markdown, HtmlToMarkdownError};

#[cfg(feature = "html-to-markdown")]
fn assert_conversion_with_options(html: &str, expected_markdown: &str, extract_scripts: bool) {
    match convert_html_to_markdown(html, extract_scripts) {
        // Trailing newline is often added by formatters or part of block structure,
        // so trim it for comparison if the expected value doesn't explicitly include it.
        // Or ensure all expected values for blocks end with \n.
        // For now, let's trim trailing newlines from actual for block comparisons.
        Ok(markdown) => assert_eq!(markdown.trim_end_matches('\n'), expected_markdown.trim_end_matches('\n')),
        Err(e) => panic!("Conversion failed for HTML '{}': {:?}", html, e),
    }
}

#[cfg(feature = "html-to-markdown")]
fn assert_conversion(html: &str, expected_markdown: &str) {
    // Default for existing tests: don't extract scripts.
    assert_conversion_with_options(html, expected_markdown, false);
}

// --- <br> Tests ---

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_table_with_alignments() {
    let html = concat!(
        "<table><thead>",
        "<tr><th style=\"text-align:left\">Left</th>",
        "<th style=\"text-align:center\">Center</th>",
        "<th style=\"text-align:right\">Right</th>",
        "<th>Default</th></tr>",
        "</thead><tbody><tr><td>1</td><td>2</td><td>3</td><td>4</td></tr></tbody></table>"
    );
    let expected = concat!(
        "| Left | Center | Right | Default |\n",
        "|:---|:---:|---:|---|\n", // Default is ---
        "| 1 | 2 | 3 | 4 |"
    );
    assert_conversion(html, expected);
}

// --- <script> Tag Conversion Tests ---

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_script_tag_ignored_when_option_is_false() {
    let html = "<script>alert('ignored');</script>";
    assert_conversion_with_options(html, "", false); // Expect empty if script is ignored

    let html_ext = "<script src=\"ext.js\"></script>";
    assert_conversion_with_options(html_ext, "", false);
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_script_tag_inline_javascript_default_type() {
    let html = "<script>alert('Hello');</script>";
    let expected = "```\nalert('Hello');\n```";
    assert_conversion_with_options(html, expected, true);
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_script_tag_inline_javascript_text_javascript() {
    let html = "<script type=\"text/javascript\">console.log(1);</script>";
    let expected = "```javascript\nconsole.log(1);\n```";
    assert_conversion_with_options(html, expected, true);
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_script_tag_inline_javascript_application_javascript() {
    let html = "<script type=\"application/javascript\">let a = 1;</script>";
    let expected = "```javascript\nlet a = 1;\n```";
    assert_conversion_with_options(html, expected, true);
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_script_tag_inline_javascript_module() {
    let html = "<script type=\"module\">import { B } from './mod.js';</script>";
    let expected = "```javascript\nimport { B } from './mod.js';\n```";
    assert_conversion_with_options(html, expected, true);
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_script_tag_inline_json_ld() {
    let html = "<script type=\"application/ld+json\">{\"@context\":\"schema.org\"}</script>";
    let expected = "```json\n{\"@context\":\"schema.org\"}\n```";
    assert_conversion_with_options(html, expected, true);
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_script_tag_inline_json() {
    let html = "<script type=\"application/json\">{\"key\":\"value\"}</script>";
    let expected = "```json\n{\"key\":\"value\"}\n```";
    assert_conversion_with_options(html, expected, true);
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_script_tag_unknown_type() {
    let html = "<script type=\"text/custom\">content</script>";
    let expected = "```\ncontent\n```";
    assert_conversion_with_options(html, expected, true);
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_script_tag_empty_content() {
    let html = "<script></script>";
    let expected = "```\n\n```";
    assert_conversion_with_options(html, expected, true);
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_script_tag_external_src_ignored_when_option_true() {
    let html = "<script src=\"app.js\"></script>";
    assert_conversion_with_options(html, "", true); // Still ignored
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_script_tag_with_html_comments_and_cdata() {
    let html = "<script><!-- alert(1); // --><![CDATA[\nalert(2);\n//]]></script>";
    let expected_content = "<!-- alert(1); // --><![CDATA[\nalert(2);\n//]]>";
    let expected_markdown = format!("```\n{}\n```", expected_content);
    assert_conversion_with_options(html, &expected_markdown, true);
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_script_tag_leading_newline_stripping() {
    let html = "<script>\n  var x = 1;\n</script>";
    // extract_text_from_pre_children removes leading \n from its direct input if script_content.starts_with('\n')
    // However, the parser might produce a Text node "\n  var x = 1;\n" for the script content.
    // extract_text_from_pre_children on this would be "\n  var x = 1;\n".
    // Then the script handler: if script_content.starts_with('\n') { script_content.remove(0); }
    // This makes it "  var x = 1;\n".
    // Then trim_end_matches('\n') makes it "  var x = 1;".
    // Then format adds outer newlines.
    let expected = "```\n  var x = 1;\n```";
    assert_conversion_with_options(html, expected, true);
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_table_with_align_attribute() {
    let html = "<table><thead><tr><th align=\"right\">H1</th><th align=\"center\">H2</th></tr></thead><tbody><tr><td>c1</td><td>c2</td></tr></tbody></table>";
    let expected = "| H1 | H2 |\n|---:|:---:|\n| c1 | c2 |";
    assert_conversion(html, expected);
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_br_in_paragraph() {
    assert_conversion("<p>line1<br>line2</p>", "line1  \nline2");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_br_multiple_in_paragraph() {
    assert_conversion("<p>line1<br><br>line2</p>", "line1  \n  \nline2");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_br_standalone() {
    // Standalone <br> converted to block-level representation if that's how parser handles it.
    // Or it might be handled as an inline element that produces "  \n".
    // If convert_nodes_to_markdown's default case pushes children_content_str ("  \n"), this is the result.
    assert_conversion("<br>", "  \n");
}

// --- <hr> Tests ---

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_hr_simple() {
    assert_conversion("<hr>", "---");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_hr_with_attributes() {
    assert_conversion("<hr class=\"fancy\" id=\"divider\">", "---");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_hr_between_blocks() {
    assert_conversion("<h1>Title</h1><hr><p>Text</p>", "# Title\n\n---\n\nText");
}

// --- Inline <code> Tests ---

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_code_inline_simple() {
    assert_conversion("<code>my_code</code>", "`my_code`");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_code_inline_in_paragraph() {
    assert_conversion("<p>Before <code>some_code()</code> after.</p>", "Before `some_code()` after.");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_code_inline_empty() {
    assert_conversion("<code></code>", "``");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_code_inline_with_internal_spaces_trimmed() {
    // Current implementation of convert_children_to_string trims, so internal leading/trailing spaces are lost.
    assert_conversion("<code>  spaced code  </code>", "`spaced code`");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_code_inline_with_html_entities_as_text() {
    // Assumes parser decodes entities like &lt; to < before converter sees it.
    assert_conversion("<code>a < b > c & d</code>", "`a < b > c & d`");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_code_in_strong() {
    assert_conversion("<strong><code>important_code()</code></strong>", "**`important_code()`**");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_strong_in_code() {
    // Markdown typically doesn't support formatting inside inline code blocks.
    // So, <strong> inside <code> should be literal.
    // convert_children_to_string for <code>'s children will process <strong>.
    // This might result in "`**strong code**`" if not handled carefully.
    // For now, let's assume simple text content for <code>.
    // A more robust solution would treat <code> content as opaque.
    // The current implementation will produce: "`**strong code**`"
    // This is a known limitation for now.
    assert_conversion("<code><strong>strong code</strong></code>", "`**strong code**`");
}

// --- List Tests (ul, ol, li) ---

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_ul_simple() {
    assert_conversion("<ul><li>Item 1</li><li>Item 2</li></ul>", "* Item 1\n* Item 2");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_ol_simple() {
    assert_conversion("<ol><li>Item 1</li><li>Item 2</li></ol>", "1. Item 1\n2. Item 2");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_ol_with_start_attribute() {
    assert_conversion("<ol start=\"3\"><li>Item 3</li><li>Item 4</li></ol>", "3. Item 3\n4. Item 4");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_ul_empty() {
    assert_conversion("<ul></ul>", ""); // Or perhaps a single "*" if list cannot be empty? Current impl likely ""
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_ol_empty() {
    assert_conversion("<ol></ol>", ""); // Similar to ul_empty
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_ul_with_empty_li() {
    assert_conversion("<ul><li></li><li>Item 2</li></ul>", "* \n* Item 2");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_ol_with_empty_li() {
    assert_conversion("<ol><li>Item 1</li><li></li></ol>", "1. Item 1\n2. ");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_ul_nested() {
    let html = "<ul><li>Parent 1<ul><li>Child A</li><li>Child B</li></ul></li><li>Parent 2</li></ul>";
    let expected = "* Parent 1\n    * Child A\n    * Child B\n* Parent 2";
    assert_conversion(html, expected);
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_li_with_multiple_paragraphs() {
    let html = "<ul><li><p>First para.</p><p>Second para.</p></li></ul>";
    let expected = "* First para.\n  \n  Second para.";
    assert_conversion(html, expected);
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_li_with_text_then_nested_list() {
    let html = "<ul><li>Item text<ul><li>Nested 1</li><li>Nested 2</li></ul></li></ul>";
    let expected = "* Item text\n  \n  * Nested 1\n  * Nested 2";
    assert_conversion(html, expected);
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_li_with_paragraph_then_nested_list() {
    let html = "<ul><li><p>Item para</p><ul><li>Nested 1</li></ul></li></ul>";
    let expected = "* Item para\n  \n  * Nested 1";
    assert_conversion(html, expected);
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_li_with_blockquote() {
    let html = "<ul><li>Item text<blockquote><p>Quoted</p></blockquote></li></ul>";
    let expected = "* Item text\n  \n  > Quoted";
    assert_conversion(html, expected);
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_li_with_pre_code() {
    let html = "<ul><li>Item text<pre><code>code\nblock</code></pre></li></ul>";
    let expected = "* Item text\n  \n  ```\n  code\n  block\n  ```";
    assert_conversion(html, expected);
}

// --- Embedded Content Tests (iframe, video, audio, embed, object) ---

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_iframe_simple() {
    assert_conversion(
        "<iframe src=\"https://example.com/embed\" title=\"My Embed\"></iframe>",
        "[My Embed](https://example.com/embed \"My Embed\")",
    );
}

// --- Combination / Integration Tests ---

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_li_complex_content() {
    let html = concat!(
        "<ul>",
        "  <li>",
        "    <p>Paragraph 1 in li.</p>",
        "    <p>Paragraph 2 in li.</p>",
        "    <ul>",
        "      <li>Nested item</li>",
        "    </ul>",
        "    <blockquote>",
        "      <p>Quote in li.</p>",
        "    </blockquote>",
        "    <pre><code>Code in li.</code></pre>",
        "  </li>",
        "</ul>"
    );
    // Marker "* " (2 chars), continuation indent is 2 spaces.
    let expected = concat!(
        "* Paragraph 1 in li.\n",
        "  \n", // Blank line between paras, indented
        "  Paragraph 2 in li.\n",
        "  \n", // Blank line before nested list, indented
        "  * Nested item\n", // Nested list itself is further indented by its own logic on top of this continuation
        "  \n", // Blank line before blockquote, indented
        "  > Quote in li.\n",
        "  \n", // Blank line before pre, indented
        "  ```\n",
        "  Code in li.\n",
        "  ```"
    );
    assert_conversion(html, expected);
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_table_cell_various_content() {
    let html = concat!(
        "<table><thead><tr><th>Header</th></tr></thead><tbody>",
        "<tr><td>Cell with <strong>bold</strong>, <em>italic</em>,<br>and a <a href=\"#\">link</a>.</td></tr>",
        "<tr><td>Cell with list:<ul><li>L1</li><li>L2</li></ul> (list becomes inline)</td></tr>",
        "<tr><td>Cell with image: <img src=\"img.png\" alt=\"alt\"></td></tr>",
        "</tbody></table>"
    );
    // convert_children_to_string for <ul><li>L1</li><li>L2</li></ul> results in "L1L2" or similar.
    // Let's assume it becomes "L1 L2" if there were spaces, or just "L1L2". For simplicity, "L1L2".
    let expected = concat!(
        "| Header |\n",
        "| --- |\n",
        "| Cell with **bold**, *italic*,  \\nand a [link](<#>) |\n", // <br> becomes "  \n"
        "| Cell with list:L1L2 (list becomes inline) |\n",
        "| Cell with image: ![alt](<img.png>) |"
    );
    assert_conversion(html, expected);
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_blockquote_complex_content() {
    let html = concat!(
        "<blockquote>",
        "  <p>Quote text.</p>",
        "  <ul><li>List in quote</li></ul>",
        "  <pre><code>Code in quote</code></pre>",
        "  <blockquote><p>Nested quote</p></blockquote>",
        "</blockquote>"
    );
    let expected = concat!(
        "> Quote text.\n",
        ">\n", // Blank line before list
        "> * List in quote\n",
        ">\n", // Blank line before pre
        "> ```\n",
        "> Code in quote\n",
        "> ```\n",
        ">\n", // Blank line before nested quote
        "> > Nested quote"
    );
    assert_conversion(html, expected);
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_dd_complex_content() {
    let html = concat!(
        "<dl>",
        "  <dt>Term</dt>",
        "  <dd>",
        "    <p>Para in dd.</p>",
        "    <ul><li>List in dd</li></ul>",
        "    <pre><code>Code in dd</code></pre>",
        "  </dd>",
        "</dl>"
    );
    // <dd> content is processed by convert_nodes_to_markdown, then each line indented by "  "
    // convert_nodes_to_markdown for children of <dd> gives:
    // "Para in dd.\n\n* List in dd\n\n```\nCode in dd\n```"
    // Indenting this:
    let expected = concat!(
        "**Term**\n",
        "  Para in dd.\n",
        "  \n",
        "  * List in dd\n",
        "  \n",
        "  ```\n",
        "  Code in dd\n",
        "  ```"
    );
    assert_conversion(html, expected);
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_full_page_structure() {
    let html = concat!(
        "<h1>Title</h1>",
        "<p>Intro paragraph with <a href=\"#l\">link</a>.</p>",
        "<ul><li>Item 1</li><li>Item 2 <input type=\"checkbox\" checked> done</li></ul>",
        "<table><thead><tr><th align=\"center\">TH</th></tr></thead><tbody><tr><td>TD</td></tr></tbody></table>",
        "<blockquote><p>Quote</p></blockquote>",
        "<pre><code>code block here</code></pre>",
        "<dl><dt>DT</dt><dd>DD</dd></dl>",
        "<p>Final para.</p>"
    );
    let expected = concat!(
        "# Title\n\n",
        "Intro paragraph with [link](<#l>).\n\n",
        "* Item 1\n* [x] done\n\n", // List items, checkbox processed
        "| TH |\n|:---:|\n| TD |\n\n", // Table with alignment
        "> Quote\n\n", // Blockquote
        "```\ncode block here\n```\n\n", // Pre block
        "**DT**\n  DD\n\n", // Definition list
        "Final para."
    );
    assert_conversion(html, expected);
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_iframe_no_title() {
    assert_conversion(
        "<iframe src=\"https://example.com/embed\"></iframe>",
        "[Embedded Iframe](https://example.com/embed)",
    );
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_iframe_no_src() {
    assert_conversion("<iframe title=\"My Embed\"></iframe>", "");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_video_simple_src() {
    assert_conversion(
        "<video src=\"movie.mp4\" title=\"My Movie\"></video>",
        "[My Movie](movie.mp4 \"My Movie\")",
    );
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_video_with_source_tag() {
    assert_conversion(
        "<video title=\"Another Movie\"><source src=\"movie.ogg\" type=\"video/ogg\"><source src=\"movie.mp4\" type=\"video/mp4\"></video>",
        "[Another Movie](movie.ogg \"Another Movie\")", // Takes first source
    );
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_video_no_title_with_source() {
     assert_conversion(
        "<video><source src=\"movie.mp4\" type=\"video/mp4\"></video>",
        "[Video](movie.mp4)",
    );
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_video_with_poster() {
    assert_conversion(
        "<video src=\"movie.mp4\" title=\"My Movie\" poster=\"poster.jpg\"></video>",
        "[My Movie](movie.mp4 \"My Movie\") (Poster: poster.jpg)",
    );
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_video_no_src_no_source() {
    assert_conversion("<video title=\"My Movie\"></video>", "");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_audio_simple_src() {
    assert_conversion(
        "<audio src=\"sound.mp3\" title=\"My Sound\"></audio>",
        "[My Sound](sound.mp3 \"My Sound\")",
    );
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_audio_with_source_tag_no_title() {
     assert_conversion(
        "<audio><source src=\"sound.ogg\" type=\"audio/ogg\"></audio>",
        "[Audio](sound.ogg)",
    );
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_embed_simple() {
    assert_conversion(
        "<embed src=\"plugin.swf\" title=\"Flash Plugin\">",
        "[Flash Plugin](plugin.swf \"Flash Plugin\")",
    );
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_embed_no_title() {
    assert_conversion(
        "<embed src=\"plugin.swf\" type=\"application/x-shockwave-flash\">",
        "[Embedded Content](plugin.swf)", // Type is not used for description in current impl
    );
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_object_simple() {
    assert_conversion(
        "<object data=\"data.pdf\" title=\"PDF Document\"></object>",
        "[PDF Document](data.pdf \"PDF Document\")",
    );
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_object_no_title() {
    assert_conversion(
        "<object data=\"data.pdf\" type=\"application/pdf\"></object>",
        "[Embedded Object](data.pdf)", // Type is not used for description
    );
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_object_no_data() {
    assert_conversion("<object title=\"My Object\"></object>", "");
}

// --- SVG Tests ---

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_svg_with_title() {
    assert_conversion(
        "<svg><title>My Awesome Icon</title><circle cx=\"50\" cy=\"50\" r=\"40\" /></svg>",
        "[SVG: My Awesome Icon]",
    );
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_svg_no_title() {
    assert_conversion(
        "<svg><rect width=\"100\" height=\"100\" /></svg>",
        "[SVG Image]",
    );
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_svg_empty_title() {
    assert_conversion(
        "<svg><title></title><path d=\"...\" /></svg>",
        "[SVG Image]",
    );
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_svg_title_with_whitespace_only() {
    assert_conversion(
        "<svg><title>   </title><ellipse /></svg>",
        "[SVG Image]",
    );
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_svg_empty_tag() {
    assert_conversion("<svg></svg>", "[SVG Image]");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_svg_title_with_inline_markup() {
    // convert_children_to_string is used for title content, so markup might pass through
    assert_conversion(
        "<svg><title>An <em>important</em> icon</title><line /></svg>",
        "[SVG: An *important* icon]",
    );
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_svg_multiple_titles_uses_first() {
    assert_conversion(
        "<svg><title>First Title</title><title>Second Title</title><circle /></svg>",
        "[SVG: First Title]",
    );
}

// --- Checkbox (<input type="checkbox">) Tests ---

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_checkbox_unchecked() {
    assert_conversion("<input type=\"checkbox\">", "[ ] ");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_checkbox_checked() {
    assert_conversion("<input type=\"checkbox\" checked>", "[x] ");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_checkbox_checked_explicit_value() {
    // checked="checked", checked="", etc. should all be treated as checked.
    // The .contains_key("checked") handles this.
    assert_conversion("<input type=\"checkbox\" checked=\"checked\">", "[x] ");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_checkbox_with_label_suffix() {
    // Assuming the label text is simply adjacent and gets concatenated by parent's children processing
    assert_conversion("<p><input type=\"checkbox\"> Remember me</p>", "[ ] Remember me");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_checkbox_in_list_item() {
    assert_conversion("<ul><li><input type=\"checkbox\"> Task 1</li></ul>", "* [ ] Task 1");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_checkbox_checked_in_list_item() {
    assert_conversion("<ul><li><input type=\"checkbox\" checked> Done</li></ul>", "* [x] Done");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_input_other_type_ignored() {
    assert_conversion("<input type=\"text\" value=\"Hello\">", "");
}


// --- Definition List (<dl>, <dt>, <dd>) Tests ---

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_dl_simple() {
    let html = "<dl><dt>Term 1</dt><dd>Definition 1</dd></dl>";
    let expected = "**Term 1**\n  Definition 1";
    assert_conversion(html, expected);
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_dl_multiple_pairs() {
    let html = "<dl><dt>T1</dt><dd>D1</dd><dt>T2</dt><dd>D2</dd></dl>";
    let expected = "**T1**\n  D1\n**T2**\n  D2";
    assert_conversion(html, expected);
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_dl_one_dt_multiple_dd() {
    let html = "<dl><dt>Term</dt><dd>Def 1</dd><dd>Def 2</dd></dl>";
    let expected = "**Term**\n  Def 1\n  Def 2";
    assert_conversion(html, expected);
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_dl_with_inline_elements() {
    let html = "<dl><dt><strong>Term</strong></dt><dd><em>Definition</em></dd></dl>";
    let expected = "****Term****\n  *Definition*"; // convert_children_to_string handles inline, then dt wraps with **
    assert_conversion(html, expected);
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_dl_dd_with_paragraph() {
    // <dd><p>Para</p></dd> -> <dd>Para</dd> (p is stripped, content becomes inline for dd)
    // then indented.
    // If dd_markdown_block = convert_nodes_to_markdown(&dd_el.children,...) is used,
    // and children is [<p>Para</p>], then dd_markdown_block = "Para".
    // Then indented_dd_lines becomes "  Para".
    let html = "<dl><dt>Term</dt><dd><p>Paragraph in definition.</p></dd></dl>";
    let expected = "**Term**\n  Paragraph in definition.";
    assert_conversion(html, expected);
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_dl_dd_with_multiple_paragraphs() {
    let html = "<dl><dt>T</dt><dd><p>P1</p><p>P2</p></dd></dl>";
    // convert_nodes_to_markdown for <dd> children ([<p>P1</p>,<p>P2</p>]) gives "P1\n\nP2".
    // Indenting this: "  P1\n  \n  P2"
    let expected = "**T**\n  P1\n  \n  P2";
    assert_conversion(html, expected);
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_dl_empty() {
    assert_conversion("<dl></dl>", "");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_dl_dt_empty() {
    let html = "<dl><dt></dt><dd>Definition</dd></dl>";
    let expected = "****\n  Definition"; // **""** becomes ****
    assert_conversion(html, expected);
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_dl_dd_empty() {
    // Empty <dd> content from convert_nodes_to_markdown is "", so no line pushed for it.
    let html = "<dl><dt>Term</dt><dd></dd></dl>";
    let expected = "**Term**"; // Empty <dd> results in no output line for it.
                               // If we want "  ", need to change <dd> handling for empty content.
                               // Current: if !dd_markdown_block.is_empty() { ... }
    assert_conversion(html, expected);
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_dl_dd_empty_explicit() {
    // To get an indented empty line, we might need <dd>&nbsp;</dd> or similar,
    // or change logic for empty <dd>.
    // For now, empty <dd> produces nothing for the dd part.
    let html = "<dl><dt>Term</dt><dd> </dd></dl>"; // dd has only whitespace
    // convert_nodes_to_markdown for " " children returns "" (due to trim).
    // So, same as test_dl_dd_empty.
    let expected = "**Term**";
    assert_conversion(html, expected);
}


#[cfg(feature = "html-to-markdown")]
#[test]
fn test_dl_with_list_in_dd() {
    let html = "<dl><dt>Topic</dt><dd><ul><li>Point 1</li><li>Point 2</li></ul></dd></dl>";
    // convert_nodes_to_markdown for <ul> gives "* Point 1\n* Point 2"
    // Indenting this: "  * Point 1\n  * Point 2"
    let expected = "**Topic**\n  * Point 1\n  * Point 2";
    assert_conversion(html, expected);
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_dl_ignore_comments_and_whitespace_nodes() {
    let html = "<dl>\n  <!-- comment --> <dt>Term</dt> \n <dd>Def</dd> </dl>";
    let expected = "**Term**\n  Def";
    assert_conversion(html, expected);
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_ol_nested() {
    let html = "<ol><li>Parent 1<ol><li>Child A</li><li>Child B</li></ol></li><li>Parent 2</li></ol>";
    let expected = "1. Parent 1\n    1. Child A\n    2. Child B\n2. Parent 2";
    assert_conversion(html, expected);
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_ul_ol_mixed_nested() {
    let html = "<ul><li>Outer A<ol><li>Inner 1</li><li>Inner 2</li></ol></li><li>Outer B</li></ul>";
    let expected = "* Outer A\n    1. Inner 1\n    2. Inner 2\n* Outer B";
    assert_conversion(html, expected);
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_ol_ul_mixed_nested() {
    let html = "<ol><li>Outer 1<ul><li>Inner A</li><li>Inner B</li></ul></li><li>Outer 2</li></ol>";
    let expected = "1. Outer 1\n    * Inner A\n    * Inner B\n2. Outer 2";
    assert_conversion(html, expected);
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_li_with_inline_elements() {
    assert_conversion("<ul><li>Item with <strong>bold</strong> and <a href=\"#\">link</a></li></ul>", "* Item with **bold** and [link](#)");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_li_with_paragraph_inside_treated_as_inline() {
    // Current implementation treats <p> inside <li> as inline content
    assert_conversion("<ul><li><p>Paragraph text</p></li></ul>", "* Paragraph text");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_li_text_trimming() {
    assert_conversion("<ul><li>  Item with spaces  </li></ul>", "* Item with spaces");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_deeply_nested_lists() {
    let html = concat!(
        "<ul>",
        "<li>L1 A",
            "<ul>",
            "<li>L2 A",
                "<ol>",
                "<li>L3 A</li>",
                "<li>L3 B</li>",
                "</ol>",
            "</li>",
            "<li>L2 B</li>",
            "</ul>",
        "</li>",
        "<li>L1 B</li>",
        "</ul>"
    );
    let expected = concat!(
        "* L1 A\n",
        "    * L2 A\n",
        "        1. L3 A\n",
        "        2. L3 B\n",
        "    * L2 B\n",
        "* L1 B"
    );
    assert_conversion(html, expected);
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_list_item_starting_with_nested_list() {
    let html = "<ul><li><ul><li>Nested Item</li></ul></li><li>Next Item</li></ul>";
    // Expected: Parent li marker, then nested list.
    let expected = "* \n    * Nested Item\n* Next Item";
    assert_conversion(html, expected);
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_ordered_list_item_starting_with_nested_list() {
    let html = "<ol><li><ol><li>Nested Item</li></ol></li><li>Next Item</li></ol>";
    let expected = "1. \n    1. Nested Item\n2. Next Item";
    assert_conversion(html, expected);
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_list_with_text_nodes_between_li() {
    // Whitespace text nodes between <li> elements should generally be ignored.
    assert_conversion("<ul> <li>Item 1</li> \n <li>Item 2</li> </ul>", "* Item 1\n* Item 2");
}

// --- Image <img> Tests ---

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_img_simple() {
    assert_conversion("<img src=\"image.png\" alt=\"My Alt Text\">", "![My Alt Text](image.png)");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_img_with_title() {
    assert_conversion(
        "<img src=\"image.jpg\" alt=\"Alt\" title=\"My Title\">",
        "![Alt](image.jpg \"My Title\")",
    );
}

// --- HTML Table Tests ---

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_table_simple() {
    let html = "<table><thead><tr><th>H1</th><th>H2</th></tr></thead><tbody><tr><td>C1</td><td>C2</td></tr><tr><td>D1</td><td>D2</td></tr></tbody></table>";
    let expected = "| H1 | H2 |\n| --- | --- |\n| C1 | C2 |\n| D1 | D2 |";
    assert_conversion(html, expected);
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_table_no_thead_tbody_first_row_as_header() { // Renamed for clarity
    let html = "<table><tbody><tr><td>H1 by td</td><td>H2 by td</td></tr><tr><td>C1</td><td>C2</td></tr></tbody></table>";
    let expected = "| H1 by td | H2 by td |\n| --- | --- |\n| C1 | C2 |";
    assert_conversion(html, expected);
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_table_empty() {
    assert_conversion("<table></table>", "");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_table_thead_only() {
    let html = "<table><thead><tr><th>H1</th><th>H2</th></tr></thead></table>";
    let expected = "| H1 | H2 |\n| --- | --- |";
    assert_conversion(html, expected);
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_table_tbody_only_first_row_as_header_no_data() { // Renamed for clarity
    let html = "<table><tbody><tr><td>Head1</td><td>Head2</td></tr></tbody></table>";
    let expected = "| Head1 | Head2 |\n| --- | --- |"; // No data rows after header
    assert_conversion(html, expected);
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_table_tbody_empty() {
    let html = "<table><tbody></tbody></table>";
    assert_conversion(html, ""); // No header can be formed
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_table_tbody_with_empty_tr() {
    // First row is <tr></tr>, so no header cells.
    let html = "<table><tbody><tr></tr><tr><td>Data1</td><td>Data2</td></tr></tbody></table>";
    assert_conversion(html, ""); // No header can be formed
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_table_tbody_first_tr_empty_cells_as_header() {
    // First row has cells but they are empty.
    let html = "<table><tbody><tr><td></td><td></td></tr><tr><td>Data1</td><td>Data2</td></tr></tbody></table>";
    let expected = "|  |  |\n| --- | --- |\n| Data1 | Data2 |";
    assert_conversion(html, expected);
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_table_with_empty_cells() {
    let html = "<table><thead><tr><th>H1</th><th>H2</th></tr></thead><tbody><tr><td></td><td>C2</td></tr><tr><td>D1</td><td></td></tr></tbody></table>";
    let expected = "| H1 | H2 |\n| --- | --- |\n|  | C2 |\n| D1 |  |"; // Note: empty cells are "  " due to "| content |" formatting.
    assert_conversion(html, expected);
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_table_header_cell_empty() {
    let html = "<table><thead><tr><th></th><th>H2</th></tr></thead><tbody><tr><td>C1</td><td>C2</td></tr></tbody></table>";
    let expected = "|  | H2 |\n| --- | --- |\n| C1 | C2 |";
    assert_conversion(html, expected);
}


#[cfg(feature = "html-to-markdown")]
#[test]
fn test_table_with_inline_elements_in_cells() {
    let html = "<table><thead><tr><th><strong>H1</strong></th><th><em>H2</em></th></tr></thead><tbody><tr><td><a href=\"#\">L</a></td><td><code>C</code></td></tr></tbody></table>";
    let expected = "| **H1** | *H2* |\n| --- | --- |\n| [L](#) | `C` |";
    assert_conversion(html, expected);
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_table_cell_with_pipe_character() {
    let html = "<table><thead><tr><th>Header</th></tr></thead><tbody><tr><td>Content | with pipe</td></tr></tbody></table>";
    let expected = "| Header |\n| --- |\n| Content \\| with pipe |";
    assert_conversion(html, expected);
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_table_row_with_fewer_cells_than_header() {
    let html = "<table><thead><tr><th>H1</th><th>H2</th><th>H3</th></tr></thead><tbody><tr><td>C1</td><td>C2</td></tr></tbody></table>";
    let expected = "| H1 | H2 | H3 |\n| --- | --- | --- |\n| C1 | C2 |  |"; // Padded with empty cell
    assert_conversion(html, expected);
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_table_row_with_more_cells_than_header() {
    // Markdown table typically only renders columns defined by header. Extra cells might be ignored or create malformed table.
    // Current implementation will use header column_count.
    let html = "<table><thead><tr><th>H1</th><th>H2</th></tr></thead><tbody><tr><td>C1</td><td>C2</td><td>C3</td></tr></tbody></table>";
    let expected = "| H1 | H2 |\n| --- | --- |\n| C1 | C2 |"; // C3 is effectively ignored by Markdown structure
    assert_conversion(html, expected);
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_table_with_colspan_ignored() {
    let html = "<table><thead><tr><th colspan=\"2\">H1-2</th></tr></thead><tbody><tr><td>C1</td><td>C2</td></tr></tbody></table>";
    // colspan is ignored, H1-2 is treated as a single cell for the first column.
    let expected = "| H1-2 |\n| --- |\n| C1 |\n| C2 |"; // This expectation might be wrong.
                                                       // If header_cells has 1 cell, column_count is 1.
                                                       // Then body rows are formatted to 1 column.
                                                       // C1 becomes first row, C2 becomes second row if they are in separate <td>.
                                                       // If <tr><td>C1</td><td>C2</td></tr>, then C1 is cell 1, C2 is ignored.
                                                       // Let's assume: <thead><tr><th colspan="2">H</th></tr></thead><tbody><tr><td>A</td><td>B</td></tr></tbody>
                                                       // Expected: | H |\n|---|\n| A |  (B is ignored)
    let html_revised = "<table><thead><tr><th colspan=\"2\">H</th></tr></thead><tbody><tr><td>A</td><td>B</td></tr></tbody></table>";
    let expected_revised = "| H |\n| --- |\n| A |"; // B is ignored because column_count is 1 from header
    assert_conversion(html_revised, expected_revised);
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_table_with_rowspan_ignored() {
    // rowspan is ignored. Content of rowspan cell appears in its original row. Subsequent rows fill cells normally.
    let html = "<table><thead><tr><th>H1</th><th>H2</th></tr></thead><tbody><tr><td rowspan=\"2\">R1C1</td><td>R1C2</td></tr><tr><td>R2C2</td></tr></tbody></table>";
    let expected = "| H1 | H2 |\n| --- | --- |\n| R1C1 | R1C2 |\n| R2C2 |  |"; // R2C1 would be empty if R1C1 didn't exist.
                                                                            // Since R1C1 is there, R2C2 is the first cell of its row in the parsed model.
                                                                            // The first cell of the second body row is R2C2.
                                                                            // The code iterates cells in a tr.
                                                                            // 1st tr: [td(R1C1), td(R1C2)] -> | R1C1 | R1C2 |
                                                                            // 2nd tr: [td(R2C2)] -> | R2C2 |  | (padded)
    assert_conversion(html, expected);
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_table_thead_with_td_cells() {
    // <th> or <td> in <thead> should be treated as header cells
    let html = "<table><thead><tr><td>H1</td><td>H2</td></tr></thead><tbody><tr><td>C1</td><td>C2</td></tr></tbody></table>";
    let expected = "| H1 | H2 |\n| --- | --- |\n| C1 | C2 |";
    assert_conversion(html, expected);
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_table_tbody_with_th_cells() {
    // <th> in <tbody> should be treated as data cells
    let html = "<table><thead><tr><th>H1</th></tr></thead><tbody><tr><th>R1C1</th></tr></tbody></table>";
    let expected = "| H1 |\n| --- |\n| R1C1 |";
    assert_conversion(html, expected);
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_table_with_tfoot_ignored() {
    let html = "<table><thead><tr><th>H1</th></tr></thead><tbody><tr><td>C1</td></tr></tbody><tfoot><tr><td>F1</td></tr></tfoot></table>";
    let expected = "| H1 |\n| --- |\n| C1 |"; // tfoot content is ignored
    assert_conversion(html, expected);
}

// --- Blockquote Tests ---

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_blockquote_simple_paragraph() {
    assert_conversion("<blockquote><p>Quoted text.</p></blockquote>", "> Quoted text.");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_blockquote_multiple_paragraphs() {
    let html = "<blockquote><p>First paragraph.</p><p>Second paragraph.</p></blockquote>";
    let expected = "> First paragraph.\n>\n> Second paragraph.";
    assert_conversion(html, expected);
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_blockquote_nested() {
    let html = "<blockquote><p>Level 1</p><blockquote><p>Level 2</p></blockquote></blockquote>";
    let expected = "> Level 1\n>\n> > Level 2"; // Assuming convert_nodes_to_markdown handles block separation for ">" line
    assert_conversion(html, expected);
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_blockquote_empty() {
    assert_conversion("<blockquote></blockquote>", ">");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_blockquote_with_list() {
    let html = "<blockquote><ul><li>Item 1</li><li>Item 2</li></ul></blockquote>";
    let expected = "> * Item 1\n> * Item 2";
    assert_conversion(html, expected);
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_blockquote_with_heading() {
    assert_conversion("<blockquote><h1>Heading</h1></blockquote>", "> # Heading");
}


// --- Preformatted Text <pre> Tests ---

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_pre_code_simple() {
    assert_conversion("<pre><code>Hello World</code></pre>", "```\nHello World\n```");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_pre_code_with_language_class() {
    assert_conversion(
        "<pre><code class=\"language-rust\">fn main() {}</code></pre>",
        "```rust\nfn main() {}\n```",
    );
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_pre_code_with_lang_class() {
    assert_conversion(
        "<pre><code class=\"lang-js\">console.log('hi');</code></pre>",
        "```js\nconsole.log('hi');\n```",
    );
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_pre_plain_text() {
    assert_conversion("<pre>Plain text content.</pre>", "```\nPlain text content.\n```");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_pre_with_html_entities() {
    assert_conversion(
        "<pre><code>&lt;div&gt; &amp; &quot; &#39; &lt;/div&gt;</code></pre>",
        "```\n<div> & \" ' </div>\n```",
    );
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_pre_empty() {
    assert_conversion("<pre></pre>", "```\n\n```");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_pre_code_empty() {
    assert_conversion("<pre><code></code></pre>", "```\n\n```");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_pre_leading_newline_stripping() {
    // A single leading newline immediately after <pre> or <code> should be stripped.
    assert_conversion("<pre>\nCode here</pre>", "```\nCode here\n```");
    assert_conversion("<pre><code>\nCode here</code></pre>", "```\nCode here\n```");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_pre_trailing_newline_handling() {
    // Trailing newlines in the content are usually preserved up to the closing ```
    // Current impl uses trim_end_matches('\n') on content.
    assert_conversion("<pre>Code here\n</pre>", "```\nCode here\n```"); // trim_end_matches('\n') removes it
    assert_conversion("<pre>Code here\n\n</pre>", "```\nCode here\n```"); // trim_end_matches('\n') removes both
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_pre_with_br_tags() {
    // extract_text_from_pre_children converts <br> to \n
    assert_conversion("<pre>Line1<br>Line2</pre>", "```\nLine1\nLine2\n```");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_pre_code_with_multiple_classes() {
    assert_conversion(
        "<pre><code class=\"foo language-python bar\">print('hello')</code></pre>",
        "```python\nprint('hello')\n```",
    );
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_img_no_alt() {
    assert_conversion("<img src=\"foo.gif\">", "![](foo.gif)");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_img_empty_alt() {
    assert_conversion("<img src=\"bar.jpeg\" alt=\"\">", "![](bar.jpeg)");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_img_no_title() {
    // This is the same as test_img_simple, just for clarity
    assert_conversion("<img src=\"image.png\" alt=\"My Alt Text\">", "![My Alt Text](image.png)");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_img_empty_title() {
    assert_conversion(
        "<img src=\"image.png\" alt=\"Alt Text\" title=\"\">",
        "![Alt Text](image.png)",
    );
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_img_no_src() {
    // src is essential. If missing, the tag should be ignored or result in empty string.
    assert_conversion("<img alt=\"Alt Text\">", "");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_img_empty_src() {
    assert_conversion("<img src=\"\" alt=\"Alt Text\">", "");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_img_in_paragraph() {
    assert_conversion(
        "<p>Some text <img src=\"i.png\" alt=\"inline\"> and more text.</p>",
        "Some text ![inline](i.png) and more text.",
    );
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_img_title_with_quotes() {
    assert_conversion(
        "<img src=\"a.png\" alt=\"alt\" title=\"A title with &quot;quotes&quot; inside\">",
        "![alt](a.png \"A title with \\\"quotes\\\" inside\")",
    );
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_img_all_attributes_empty_except_src() {
    assert_conversion("<img src=\"b.png\" alt=\"\" title=\"\">", "![](b.png)");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_img_url_with_special_chars() {
    // Markdown spec doesn't require extensive URL encoding for link destinations if they don't break parsing.
    // Parentheses can be an issue. Spaces should be %20.
    // For now, assume src URL is passed as is.
    assert_conversion(
        "<img src=\"images/my image (new).jpg\" alt=\"special\">",
        "![special](</images/my%20image%20(new).jpg>)",
    );
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_h1_simple() {
    assert_conversion("<h1>Hello World</h1>", "# Hello World");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_h2_simple() {
    assert_conversion("<h2>Hello World</h2>", "## Hello World");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_h3_simple() {
    assert_conversion("<h3>Hello World</h3>", "### Hello World");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_h4_simple() {
    assert_conversion("<h4>Hello World</h4>", "#### Hello World");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_h5_simple() {
    assert_conversion("<h5>Hello World</h5>", "##### Hello World");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_h6_simple() {
    assert_conversion("<h6>Hello World</h6>", "###### Hello World");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_h1_with_attributes() {
    // Attributes on heading tags are generally ignored in Markdown conversion
    assert_conversion("<h1 id=\"main-title\" class=\"important\">Hello</h1>", "# Hello");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_h2_empty() {
    assert_conversion("<h2></h2>", "## "); // Or just "##" - common practice is a space after #
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_h3_with_whitespace() {
    assert_conversion("<h3>  Spaced Out  </h3>", "### Spaced Out");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_multiple_headings() {
    assert_conversion("<h1>First</h1><h2>Second</h2>", "# First\n\n## Second");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_heading_with_inline_strong() {
    // This test will initially fail as <strong> is not yet handled by parser/converter
    // We will implement <strong> and <em> handling later.
    // For now, the text content might be extracted, or it might fail parsing depending on implementation.
    // Let's assume for now it extracts text content, and we'll refine when strong/em is added.
    assert_conversion("<h1>Hello <strong>World</strong></h1>", "# Hello **World**");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_heading_with_inline_em() {
    // Similar to strong, this will be refined later.
    assert_conversion("<h2>Hello <em>World</em></h2>", "## Hello *World*");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_heading_mixed_content() {
    // Text, then strong, then text
    assert_conversion("<h3>Part1 <strong>bold</strong> Part2</h3>", "### Part1 **bold** Part2");
}

// TODO: Add tests for headings with links, images etc. once those elements are supported.

// Test for parsing error on malformed heading (illustrative, might need adjustment based on parser behavior)
// At this stage, the generic "parsing not yet fully implemented" error is expected for unhandled valid tags,
// but malformed tags might also trigger it or a more specific error once the parser is more developed.
#[cfg(feature = "html-to-markdown")]
#[test]
fn test_h1_malformed_open() {
    let result = convert_html_to_markdown("<h1>Hello World</h_oops>");
    match result {
        Err(HtmlToMarkdownError::ParseError { .. }) => { /* Expected for now */ }
        Ok(md) => panic!("Should have failed for malformed HTML, got: {}", md),
        Err(e) => panic!("Expected ParseError, got different error: {:?}", e),
    }
}

// --- Strong and Emphasis Tests ---

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_strong_simple() {
    assert_conversion("<strong>Hello</strong>", "**Hello**");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_em_simple() {
    assert_conversion("<em>World</em>", "*World*");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_strong_with_attributes() {
    assert_conversion("<strong class=\"bold\">Text</strong>", "**Text**");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_em_empty() {
    assert_conversion("<em></em>", ""); // Empty emphasis should probably result in empty string
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_strong_in_paragraph() {
    assert_conversion("<p>This is <strong>bold</strong> text.</p>", "This is **bold** text.");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_em_in_paragraph() {
    assert_conversion("<p>This is <em>italic</em> text.</p>", "This is *italic* text.");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_strong_and_em_in_paragraph() {
    assert_conversion("<p><strong>Bold</strong> and <em>italic</em>.</p>", "**Bold** and *italic*.");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_nested_strong_em() {
    assert_conversion("<strong><em>Bold Italic</em></strong>", "***Bold Italic***");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_nested_em_strong() {
    // Markdown doesn't distinguish em>strong vs strong>em, usually renders same (typically ***text***)
    assert_conversion("<em><strong>Italic Bold</strong></em>", "***Italic Bold***");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_strong_in_heading_now_correctly_formatted() {
    assert_conversion("<h1>Hello <strong>World</strong></h1>", "# Hello **World**");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_em_in_heading_now_correctly_formatted() {
    assert_conversion("<h2>Hello <em>World</em></h2>", "## Hello *World*");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_mixed_content_in_heading_correctly_formatted() {
    assert_conversion("<h3>Part1 <strong>bold</strong> and <em>italic</em> Part2</h3>", "### Part1 **bold** and *italic* Part2");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_strong_with_internal_whitespace() {
    assert_conversion("<strong>  spaced  </strong>", "**spaced**");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_em_around_strong() {
    assert_conversion("<em>Emphasis around <strong>bold</strong> text.</em>", "*Emphasis around **bold** text.*");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_strong_around_em() {
    assert_conversion("<strong>Bold around <em>emphasis</em> text.</strong>", "**Bold around *emphasis* text.**");
}

// --- Link (<a>) Tests ---

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_link_simple() {
    assert_conversion("<a href=\"https://example.com\">Example</a>", "[Example](https://example.com)");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_link_with_title() {
    assert_conversion("<a href=\"https://example.com\" title=\"Cool Site\">Example</a>", "[Example](https://example.com \"Cool Site\")");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_link_empty_text() {
    // Markdown doesn't have a standard for empty link text. Some renderers might use the URL.
    // We'll aim for [] which might be ignored or handled by specific renderers.
    // Or, consider [url](url) if that's more common GFM behavior. For now, `[]`.
    assert_conversion("<a href=\"https://example.com\"></a>", "[](https://example.com)");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_link_href_empty_processed() {
    assert_conversion("<a href=\"\">empty href</a>", "[empty href](<>)");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_link_no_href() {
    // Anchor link without href should just render the text content.
    assert_conversion("<a name=\"anchor\">Anchor Text</a>", "Anchor Text");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_link_with_emphasized_text() {
    assert_conversion("<a href=\"/foo\"><em>italic link</em></a>", "[*italic link*](/foo)");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_link_with_strong_text() {
    assert_conversion("<a href=\"/bar\"><strong>bold link</strong></a>", "[**bold link**](/bar)");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_link_with_mixed_emphasis_text() {
    assert_conversion("<a href=\"/baz\">normal <strong>bold</strong> <em>italic</em></a>", "[normal **bold** *italic*](/baz)");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_link_relative_url() {
    assert_conversion("<a href=\"../index.html\">Go Back</a>", "[Go Back](../index.html)");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_link_url_with_spaces_and_parentheses() {
    // HTML href usually has spaces URL-encoded as %20.
    // Markdown link destination can have spaces if URL-encoded, or sometimes if surrounded by <>.
    // Parentheses in URL for Markdown need to be balanced or URL enclosed in <>.
    // For simplicity, assume valid, possibly encoded URLs in href.
    // If href="foo bar.html", output "[text](<foo%20bar.html>)" is common.
    // If href="/path(with)parens", output "[text](</path(with)parens>)"
    // We'll aim for direct passthrough for now and refine if specific encoding/escaping is needed by Markdown spec.
    assert_conversion("<a href=\"/url%20with%20spaces(and%29parentheses.html\">Link</a>", "[Link](</url%20with%20spaces(and%29parentheses.html>)");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_link_url_with_unescaped_parentheses_in_href() {
    // Markdown requires parentheses in URL to be escaped or the URL enclosed in <>
    // For now, we will assume the parser provides the href as is, and converter might need to handle this.
    // Let's test a simple case. If href="/a(b)c", output could be "[text](</a(b)c>)" which is fine for many renderers,
    // or ideally "[text](</a(b)c>)" or "[text](/a\(b\)c)".
    assert_conversion("<a href=\"/a(b)c\">text</a>", "[text](</a(b)c>)");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_link_href_with_spaces_only() {
    assert_conversion("<a href=\"/url with spaces\">Link</a>", "[Link](</url%20with%20spaces>)");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_link_title_with_quotes() {
    // HTML: <a href="url" title="a &quot;quote&quot;">text</a>
    // Markdown: [text](url "a \"quote\"")
    // The parser should unescape HTML entities in attribute values.
    // The converter should then re-escape for Markdown if necessary (e.g., " becomes \").
    assert_conversion("<a href=\"/foo\" title=\"A &quot;quoted&quot; title\">QLink</a>", "[QLink](/foo \"A \\\"quoted\\\" title\")");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_link_in_paragraph() {
    assert_conversion("<p>Here is a <a href=\"#\">link</a>.</p>", "Here is a [link](#).");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_link_in_heading() {
    // assert_conversion("<h2>Heading with <a href=\"/s\">strong link</a></h2>", "## Heading with [strong link](/s)");
    // This will require strong to be handled correctly within link text if not already.
    // The test `test_link_with_strong_text` covers `[**bold link**](url)`
    // So, this should be "## Heading with [**strong link**](/s)" if strong is implemented in links.
    // Let's update this expectation once strong in link is confirmed.
    // For now, assume link text doesn't re-process for strong/em if parser is simple.
    // No, convert_children_to_string should handle this:
    assert_conversion("<h2>Heading with <a href=\"/s\"><strong>strong link</strong></a></h2>", "## Heading with [**strong link**](/s)");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_link_complex_content_and_title() {
    assert_conversion(
        "<a href=\"/path\" title=\"A 'single' & &quot;double&quot; title\"><em>Italic</em> and <strong>Bold</strong> Link Text</a>",
        "[*Italic* and **Bold** Link Text](/path \"A 'single' & \\\"double\\\" title\")"
    );
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_h1_not_closed() {
    // Behavior for unclosed tags can vary. Some parsers are lenient.
    // For now, expecting a ParseError as our simple parser likely won't handle this.
    let result = convert_html_to_markdown("<h1>Hello World");
     match result {
        Err(HtmlToMarkdownError::ParseError { .. }) => { /* Expected for now */ }
        Ok(md) => panic!("Should have failed for unclosed HTML tag, got: {}", md),
        Err(e) => panic!("Expected ParseError, got different error: {:?}", e),
    }
}
