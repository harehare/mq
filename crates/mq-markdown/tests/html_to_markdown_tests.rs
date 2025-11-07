#![recursion_limit = "256"]
#![cfg(feature = "html-to-markdown")]
use mq_markdown::{ConversionOptions, convert_html_to_markdown};
use rstest::rstest;

fn assert_conversion_with_options(html: &str, expected_markdown: &str, options: ConversionOptions) {
    match convert_html_to_markdown(html, options) {
        Ok(markdown) => assert_eq!(
            markdown.trim_end_matches('\n'),
            expected_markdown.trim_end_matches('\n')
        ),
        Err(e) => panic!("Conversion failed for HTML '{}': {:?}", html, e),
    }
}

#[rstest]
#[case::br_in_paragraph("<p>line1<br>line2</p>", ConversionOptions::default(), "line1  \nline2")]
#[case::br_multiple_in_paragraph("<p>line1<br><br>line2</p>", ConversionOptions::default(), "line1  \n  \nline2")]
#[case::br_standalone("<br>", ConversionOptions::default(), "\n")]
#[case::table_with_alignments(
    concat!(
        "<table><thead>",
        "<tr><th style=\"text-align:left\">Left</th>",
        "<th style=\"text-align:center\">Center</th>",
        "<th style=\"text-align:right\">Right</th>",
        "<th>Default</th></tr>",
        "</thead><tbody><tr><td>1</td><td>2</td><td>3</td><td>4</td></tr></tbody></table>"
    ),
    ConversionOptions::default(),
    "| Left | Center | Right | Default |\n|:---|:---:|---:|---|\n| 1 | 2 | 3 | 4 |"
)]
#[case::script_tag_ignored_when_option_is_false("<script>alert('ignored');</script>", ConversionOptions::default(), "")]
#[case::script_tag_ignored_when_option_is_false_external(
    "<script src=\"ext.js\"></script>",
    ConversionOptions::default(),
    ""
)]
#[case::script_tag_inline_javascript_default_type(
    "<script>alert('Hello');</script>",
    ConversionOptions {
        extract_scripts_as_code_blocks: true,
        ..ConversionOptions::default()
    },
    "```\nalert('Hello');\n```",
)]
#[case::script_tag_inline_javascript_text_javascript(
    "<script type=\"text/javascript\">console.log(1);</script>",
    ConversionOptions{
        extract_scripts_as_code_blocks: true,
        ..ConversionOptions::default()
    },
    "```javascript\nconsole.log(1);\n```"
)]
#[case::script_tag_inline_javascript_application_javascript(
    "<script type=\"application/javascript\">let a = 1;</script>",
    ConversionOptions{
        extract_scripts_as_code_blocks: true,
        ..ConversionOptions::default()
    },
    "```javascript\nlet a = 1;\n```"
)]
#[case::script_tag_inline_javascript_module(
    "<script type=\"module\">import { B } from './mod.js';</script>",
    ConversionOptions{
        extract_scripts_as_code_blocks: true,
        ..ConversionOptions::default()
    },
    "```javascript\nimport { B } from './mod.js';\n```"
)]
#[case::script_tag_inline_json_ld(
    "<script type=\"application/ld+json\">{\"@context\":\"schema.org\"}</script>",
    ConversionOptions {
        extract_scripts_as_code_blocks: true,
        ..ConversionOptions::default()
    },
    "```json\n{\"@context\":\"schema.org\"}\n```"
)]
#[case::script_tag_inline_json(
    "<script type=\"application/json\">{\"key\":\"value\"}</script>",
    ConversionOptions {
        extract_scripts_as_code_blocks: true,
        ..ConversionOptions::default()
    },
    "```json\n{\"key\":\"value\"}\n```"
)]
#[case::script_tag_unknown_type(
    "<script type=\"text/custom\">content</script>",
    ConversionOptions {
        extract_scripts_as_code_blocks: true,
        ..ConversionOptions::default()
    },
    "```\ncontent\n```"
)]
#[case::script_tag_empty_content(
    "<script></script>",
    ConversionOptions {
        extract_scripts_as_code_blocks: true,
        ..ConversionOptions::default()
    },
    "```\n\n```"
)]
#[case::script_tag_external_src_ignored_when_option_true(
    "<script src=\"app.js\"></script>",
    ConversionOptions {
        extract_scripts_as_code_blocks: true,
        ..ConversionOptions::default()
    },
    ""
)]
#[case::script_tag_with_html_comments_and_cdata(
    "<script><!-- alert(1); // --><![CDATA[\nalert(2);\n//]]></script>",
    ConversionOptions {
        extract_scripts_as_code_blocks: true,
        ..ConversionOptions::default()
    },
    &format!("```\n{}\n```", "<!-- alert(1); // --><![CDATA[\nalert(2);\n//]]>")
)]
#[case::front_matter_disabled(
    "<html><head><title>My Title</title></head><body><p>Body</p></body></html>",
    ConversionOptions::default(),
    "My Title\n\nBody"
)]
#[case::front_matter_title_only(
    "<html><head><title>My Title</title></head><body><p>Body</p></body></html>",
    ConversionOptions {
        generate_front_matter: true,
        ..ConversionOptions::default()
    },
    "---\ntitle: My Title\n---\n\nMy Title\n\nBody",
)]
#[case::front_matter_description(
    "<html><head><meta name=\"description\" content=\"Page description.\"></head><body><p>B</p></body></html>",
    ConversionOptions {
        generate_front_matter: true,
        ..ConversionOptions::default()
    },
    "---\ndescription: Page description.\n---\n\nB",
)]
#[case::front_matter_keywords_single(
    "<html><head><meta name=\"keywords\" content=\"rust\"></head><body><p>B</p></body></html>",
    ConversionOptions {
        generate_front_matter: true,
        ..ConversionOptions::default()
    },
    "---\nkeywords:\n- rust\n---\n\nB",
)]
#[case::front_matter_keywords_multiple_comma_separated(
    "<html><head><meta name=\"keywords\" content=\"rust, web, html\"></head><body><p>B</p></body></html>",
    ConversionOptions {
        generate_front_matter: true,
        ..ConversionOptions::default()
    },
    "---\nkeywords:\n- rust\n- web\n- html\n---\n\nB",
)]
#[case::front_matter_keywords_comma_space_separated(
    "<html><head><meta name=\"keywords\" content=\"rust, web,  html \"></head><body><p>B</p></body></html>",
    ConversionOptions {
        generate_front_matter: true,
        ..ConversionOptions::default()
    },
    "---\nkeywords:\n- rust\n- web\n- html\n---\n\nB",
)]
#[case::front_matter_author(
    "<html><head><meta name=\"author\" content=\"Jules Verne\"></head><body><p>B</p></body></html>",
    ConversionOptions {
        generate_front_matter: true,
        ..ConversionOptions::default()
    },
    "---\nauthor: Jules Verne\n---\n\nB",
)]
#[case::front_matter_all_present(
    "<html><head><title>Full Test</title><meta name=\"description\" content=\"Desc here\"><meta name=\"keywords\" content=\"key1,key2\"><meta name=\"author\" content=\"Author Name\"></head><body><p>Content</p></body></html>",
    ConversionOptions {
        generate_front_matter: true,
        ..ConversionOptions::default()
    },
    "---\nauthor: Author Name\ndescription: Desc here\nkeywords:\n- key1\n- key2\ntitle: Full Test\n---\n\nFull Test\n\nContent",
)]
#[case::front_matter_no_head_tag(
    "<html><body><p>Only body</p></body></html>",
    ConversionOptions {
        generate_front_matter: true,
        ..ConversionOptions::default()
    },
    "Only body",
)]
#[case::front_matter_empty_head(
    "<html><head></head><body><p>Body</p></body></html>",
    ConversionOptions {
        generate_front_matter: true,
        ..ConversionOptions::default()
    },
    "Body",
)]
#[case::front_matter_no_relevant_tags_in_head(
    "<html><head><meta name=\"viewport\" content=\"width=device-width\"></head><body><p>Body</p></body></html>",
    ConversionOptions {
        generate_front_matter: true,
        ..ConversionOptions::default()
    },
    "Body", // No relevant tags for front matter
)]
#[case::front_matter_with_script_extraction_option(
    "<html><head><title>Script Page</title></head><body><script>let x=1;</script><p>Text</p></body></html>",
    ConversionOptions {
        generate_front_matter: true,
        extract_scripts_as_code_blocks: true,
        ..ConversionOptions::default()
    },
    "---\ntitle: Script Page\n---\n\nScript Page\n\n```\nlet x=1;\n```\n\nText",
)]
#[case::front_matter_html_fragment_no_head(
    "<p>Just a paragraph</p><meta name=\"description\" content=\"Hidden\">",
    ConversionOptions {
        generate_front_matter: true,
        ..ConversionOptions::default()
    },
    "Just a paragraph", // Meta tag not in <head> context
)]
#[case::script_tag_leading_newline_stripping(
    "<script>\n  var x = 1;\n</script>",
    ConversionOptions {
        extract_scripts_as_code_blocks: true,
        ..ConversionOptions::default()
    },
    "```\n  var x = 1;\n```",
)]
#[case::table_with_align_attribute(
    "<table><thead><tr><th align=\"right\">H1</th><th align=\"center\">H2</th></tr></thead><tbody><tr><td>c1</td><td>c2</td></tr></tbody></table>",
    ConversionOptions::default(),
    "| H1 | H2 |\n|---:|:---:|\n| c1 | c2 |"
)]
#[case::br_in_paragraph("<p>line1<br>line2</p>", ConversionOptions::default(), "line1  \nline2")]
#[case::br_multiple_in_paragraph("<p>line1<br><br>line2</p>", ConversionOptions::default(), "line1  \n  \nline2")]
#[case::br_standalone("<br>", ConversionOptions::default(), "\n")]
#[case::hr_simple("<hr>", ConversionOptions::default(), "---")]
#[case::hr_with_attributes("<hr class=\"fancy\" id=\"divider\">", ConversionOptions::default(), "---")]
#[case::hr_between_blocks(
    "<h1>Title</h1><hr><p>Text</p>",
    ConversionOptions::default(),
    "# Title\n\n---\n\nText"
)]
#[case::code_inline_simple("<code>my_code</code>", ConversionOptions::default(), "`my_code`")]
#[case::code_inline_in_paragraph(
    "<p>Before <code>some_code()</code> after.</p>",
    ConversionOptions::default(),
    "Before `some_code()` after."
)]
#[case::code_inline_empty("<code></code>", ConversionOptions::default(), "``")]
#[case::code_inline_with_internal_spaces_trimmed(
    "<code>  spaced code  </code>",
    ConversionOptions::default(),
    "` spaced code `"
)]
#[case::code_inline_with_html_entities_as_text(
    "<code>a < b > c & d</code>",
    ConversionOptions::default(),
    "`a < b > c & d`"
)]
#[case::code_in_strong(
    "<strong><code>important_code()</code></strong>",
    ConversionOptions::default(),
    "**`important_code()`**"
)]
#[case::strong_in_code(
    "<code><strong>strong code</strong></code>",
    ConversionOptions::default(),
    "`**strong code**`"
)]
#[case::ul_simple(
    "<ul><li>Item 1</li><li>Item 2</li></ul>",
    ConversionOptions::default(),
    "* Item 1\n* Item 2"
)]
#[case::ol_simple(
    "<ol><li>Item 1</li><li>Item 2</li></ol>",
    ConversionOptions::default(),
    "1. Item 1\n2. Item 2"
)]
#[case::ol_with_start_attribute(
    "<ol start=\"3\"><li>Item 3</li><li>Item 4</li></ol>",
    ConversionOptions::default(),
    "3. Item 3\n4. Item 4"
)]
#[case::ul_empty("<ul></ul>", ConversionOptions::default(), "")]
#[case::ol_empty("<ol></ol>", ConversionOptions::default(), "")]
#[case::ul_with_empty_li("<ul><li></li><li>Item 2</li></ul>", ConversionOptions::default(), "* \n* Item 2")]
#[case::ol_with_empty_li("<ol><li>Item 1</li><li></li></ol>", ConversionOptions::default(), "1. Item 1\n2. ")]
#[case::ul_nested(
    "<ul><li>Parent 1<ul><li>Child A</li><li>Child B</li></ul></li><li>Parent 2</li></ul>",
    ConversionOptions::default(),
    "* Parent 1\n  * Child A\n  * Child B\n* Parent 2"
)]
#[case::li_with_multiple_paragraphs(
    "<ul><li><p>First para.</p><p>Second para.</p></li></ul>",
    ConversionOptions::default(),
    "* First para.\n  Second para."
)]
#[case::li_with_text_then_nested_list(
    "<ul><li>Item text<ul><li>Nested 1</li><li>Nested 2</li></ul></li></ul>",
    ConversionOptions::default(),
    "* Item text\n  * Nested 1\n  * Nested 2"
)]
#[case::li_with_paragraph_then_nested_list(
    "<ul><li><p>Item para</p><ul><li>Nested 1</li></ul></li></ul>",
    ConversionOptions::default(),
    "* Item para\n  * Nested 1"
)]
#[case::li_with_blockquote(
    "<ul><li>Item text<blockquote><p>Quoted</p></blockquote></li></ul>",
    ConversionOptions::default(),
    "* Item text\n  > Quoted"
)]
#[case::li_with_pre_code(
    "<ul><li>Item text<pre><code>code\nblock</code></pre></li></ul>",
    ConversionOptions::default(),
    "* Item text\n  ```\n  code\n  block\n  ```"
)]
#[case::iframe_simple(
    "<iframe src=\"https://example.com/embed\" title=\"My Embed\"></iframe>",
    ConversionOptions::default(),
    "[My Embed](https://example.com/embed \"My Embed\")"
)]
#[case::li_complex_content(
    concat!(
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
    ),
    ConversionOptions::default(),
    concat!(
        "* Paragraph 1 in li.\n",
        "  Paragraph 2 in li.\n",
        "  * Nested item\n",
        "  > Quote in li.\n",
        "  ```\n",
        "  Code in li.\n",
        "  ```"
    )
)]
#[case::table_cell_various_content(
    concat!(
        "<table><thead><tr><th>Header</th></tr></thead><tbody>",
        "<tr><td>Cell with <strong>bold</strong>, <em>italic</em>,<br>and a <a href=\"#\">link</a>.</td></tr>",
        "<tr><td>Cell with list:<ul><li>L1</li><li>L2</li></ul> (list becomes inline)</td></tr>",
        "<tr><td>Cell with image: <img src=\"img.png\" alt=\"alt\"></td></tr>",
        "</tbody></table>"
    ),
    ConversionOptions::default(),
    concat!(
        "| Header |\n",
        "|---|\n",
        "| Cell with **bold**, *italic*,  \nand a [link](#). |\n",
        "| Cell with list:L1L2 (list becomes inline) |\n",
        "| Cell with image: ![alt](img.png) |"
    )
)]
#[case::blockquote_complex_content(
    concat!(
        "<blockquote>",
        "  <p>Quote text.</p>",
        "  <ul><li>List in quote</li></ul>",
        "  <pre><code>Code in quote</code></pre>",
        "  <blockquote><p>Nested quote</p></blockquote>",
        "</blockquote>"
    ),
    ConversionOptions::default(),
    concat!(
        "> Quote text.\n",
        "> \n", // Blank line before list
        "> * List in quote\n",
        "> \n", // Blank line before pre
        "> ```\n",
        "> Code in quote\n",
        "> ```\n",
        "> \n", // Blank line before nested quote
        "> > Nested quote"
    )
)]
#[case::dd_complex_content(
    concat!(
        "<dl>",
        "  <dt>Term</dt>",
        "  <dd>",
        "    <p>Para in dd.</p>",
        "    <ul><li>List in dd</li></ul>",
        "    <pre><code>Code in dd</code></pre>",
        "  </dd>",
        "</dl>"
    ),
    ConversionOptions::default(),
    concat!(
        "**Term**\n",
        "  Para in dd.\n",
        "  \n",
        "  * List in dd\n",
        "  \n",
        "  ```\n",
        "  Code in dd\n",
        "  ```"
    )
)]
#[case::full_page_structure(
    concat!(
        "<h1>Title</h1>",
        "<p>Intro paragraph with <a href=\"#l\">link</a>.</p>",
        "<ul><li>Item 1</li><li>Item 2 <input type=\"checkbox\" checked> done</li></ul>",
        "<table><thead><tr><th align=\"center\">TH</th></tr></thead><tbody><tr><td>TD</td></tr></tbody></table>",
        "<blockquote><p>Quote</p></blockquote>",
        "<pre><code>code block here</code></pre>",
        "<dl><dt>DT</dt><dd>DD</dd></dl>",
        "<p>Final para.</p>"
    ),
    ConversionOptions::default(),
    concat!(
        "# Title\n\n",
        "Intro paragraph with [link](#l).\n\n",
        "* Item 1\n* Item 2 [x] done\n\n", // List items, checkbox processed
        "| TH |\n|:---:|\n| TD |\n\n",     // Table with alignment
        "> Quote\n\n",                     // Blockquote
        "```\ncode block here\n```\n\n",   // Pre block
        "**DT**\n  DD\n\n",                // Definition list
        "Final para."
    )
)]
#[case::iframe_no_title(
    "<iframe src=\"https://example.com/embed\"></iframe>",
    ConversionOptions::default(),
    "[Embedded Iframe](https://example.com/embed)"
)]
#[case::iframe_no_src("<iframe title=\"My Embed\"></iframe>", ConversionOptions::default(), "")]
#[case::video_simple_src(
    "<video src=\"movie.mp4\" title=\"My Movie\"></video>",
    ConversionOptions::default(),
    "[My Movie](movie.mp4 \"My Movie\")"
)]
#[case::video_with_source_tag(
    "<video title=\"Another Movie\"><source src=\"movie.ogg\" type=\"video/ogg\"><source src=\"movie.mp4\" type=\"video/mp4\"></video>",
    ConversionOptions::default(),
    "[Another Movie](movie.ogg \"Another Movie\")"
)]
#[case::video_no_title_with_source(
    "<video><source src=\"movie.mp4\" type=\"video/mp4\"></video>",
    ConversionOptions::default(),
    "[Video](movie.mp4)"
)]
#[case::video_with_poster(
    "<video src=\"movie.mp4\" title=\"My Movie\" poster=\"poster.jpg\"></video>",
    ConversionOptions::default(),
    "[My Movie](movie.mp4 \"My Movie\") (Poster: poster.jpg)"
)]
#[case::video_no_src_no_source("<video title=\"My Movie\"></video>", ConversionOptions::default(), "")]
#[case::audio_simple_src(
    "<audio src=\"sound.mp3\" title=\"My Sound\"></audio>",
    ConversionOptions::default(),
    "[My Sound](sound.mp3 \"My Sound\")"
)]
#[case::audio_with_source_tag_no_title(
    "<audio><source src=\"sound.ogg\" type=\"audio/ogg\"></audio>",
    ConversionOptions::default(),
    "[Audio](sound.ogg)"
)]
#[case::embed_simple(
    "<embed src=\"plugin.swf\" title=\"Flash Plugin\">",
    ConversionOptions::default(),
    "[Flash Plugin](plugin.swf \"Flash Plugin\")"
)]
#[case::embed_no_title(
    "<embed src=\"plugin.swf\" type=\"application/x-shockwave-flash\">",
    ConversionOptions::default(),
    "[Embedded Content](plugin.swf)"
)]
#[case::object_simple(
    "<object data=\"data.pdf\" title=\"PDF Document\"></object>",
    ConversionOptions::default(),
    "[PDF Document](data.pdf \"PDF Document\")"
)]
#[case::object_no_title(
    "<object data=\"data.pdf\" type=\"application/pdf\"></object>",
    ConversionOptions::default(),
    "[Embedded Object](data.pdf)"
)]
#[case::object_no_data("<object title=\"My Object\"></object>", ConversionOptions::default(), "")]
#[case::svg_with_title(
    "<svg><title>My Awesome Icon</title><circle cx=\"50\" cy=\"50\" r=\"40\" /></svg>",
    ConversionOptions::default(),
    "[SVG: My Awesome Icon]"
)]
#[case::svg_no_title(
    "<svg><rect width=\"100\" height=\"100\" /></svg>",
    ConversionOptions::default(),
    "[SVG Image]"
)]
#[case::svg_empty_title(
    "<svg><title></title><path d=\"...\" /></svg>",
    ConversionOptions::default(),
    "[SVG Image]"
)]
#[case::svg_title_with_whitespace_only(
    "<svg><title>   </title><ellipse /></svg>",
    ConversionOptions::default(),
    "[SVG Image]"
)]
#[case::svg_empty_tag("<svg></svg>", ConversionOptions::default(), "[SVG Image]")]
#[case::svg_title_with_inline_markup(
    "<svg><title>An <em>important</em> icon</title><line /></svg>",
    ConversionOptions::default(),
    "[SVG: An *important* icon]"
)]
#[case::svg_multiple_titles_uses_first(
    "<svg><title>First Title</title><title>Second Title</title><circle /></svg>",
    ConversionOptions::default(),
    "[SVG: First Title]"
)]
#[case::checkbox_unchecked("<input type=\"checkbox\">", ConversionOptions::default(), "[ ]")]
#[case::checkbox_checked("<input type=\"checkbox\" checked>", ConversionOptions::default(), "[x]")]
#[case::checkbox_checked_explicit_value(
    "<input type=\"checkbox\" checked=\"checked\">",
    ConversionOptions::default(),
    "[x]"
)]
#[case::checkbox_with_label_suffix(
    "<p><input type=\"checkbox\">Remember me</p>",
    ConversionOptions::default(),
    "[ ] Remember me"
)]
#[case::checkbox_in_list_item(
    "<ul><li><input type=\"checkbox\"> Task 1</li></ul>",
    ConversionOptions::default(),
    "* [ ] Task 1"
)]
#[case::checkbox_checked_in_list_item(
    "<ul><li><input type=\"checkbox\" checked> Done</li></ul>",
    ConversionOptions::default(),
    "* [x] Done"
)]
#[case::input_other_type_ignored("<input type=\"text\" value=\"Hello\">", ConversionOptions::default(), "Hello")]
#[case::dl_simple(
    "<dl><dt>Term 1</dt><dd>Definition 1</dd></dl>",
    ConversionOptions::default(),
    "**Term 1**\n  Definition 1"
)]
#[case::dl_multiple_pairs(
    "<dl><dt>T1</dt><dd>D1</dd><dt>T2</dt><dd>D2</dd></dl>",
    ConversionOptions::default(),
    "**T1**\n  D1\n**T2**\n  D2"
)]
#[case::dl_one_dt_multiple_dd(
    "<dl><dt>Term</dt><dd>Def 1</dd><dd>Def 2</dd></dl>",
    ConversionOptions::default(),
    "**Term**\n  Def 1\n  Def 2"
)]
#[case::dl_with_inline_elements(
    "<dl><dt><strong>Term</strong></dt><dd><em>Definition</em></dd></dl>",
    ConversionOptions::default(),
    "****Term****\n  *Definition*"
)]
#[case::dl_dd_with_paragraph(
    "<dl><dt>Term</dt><dd><p>Paragraph in definition.</p></dd></dl>",
    ConversionOptions::default(),
    "**Term**\n  Paragraph in definition."
)]
#[case::dl_dd_with_multiple_paragraphs(
    "<dl><dt>T</dt><dd><p>P1</p><p>P2</p></dd></dl>",
    ConversionOptions::default(),
    "**T**\n  P1\n  \n  P2"
)]
#[case::dl_empty("<dl></dl>", ConversionOptions::default(), "")]
#[case::dl_dt_empty(
    "<dl><dt></dt><dd>Definition</dd></dl>",
    ConversionOptions::default(),
    "****\n  Definition"
)]
#[case::dl_dd_empty("<dl><dt>Term</dt><dd></dd></dl>", ConversionOptions::default(), "**Term**")]
#[case::dl_dd_empty_explicit("<dl><dt>Term</dt><dd> </dd></dl>", ConversionOptions::default(), "**Term**")]
#[case::dl_with_list_in_dd(
    "<dl><dt>Topic</dt><dd><ul><li>Point 1</li><li>Point 2</li></ul></dd></dl>",
    ConversionOptions::default(),
    "**Topic**\n  * Point 1\n  * Point 2"
)]
#[case::dl_ignore_comments_and_whitespace_nodes(
    "<dl>\n  <!-- comment --> <dt>Term</dt> \n <dd>Def</dd> </dl>",
    ConversionOptions::default(),
    "**Term**\n  Def"
)]
#[case::ol_nested(
    "<ol><li>Parent 1<ol><li>Child A</li><li>Child B</li></ol></li><li>Parent 2</li></ol>",
    ConversionOptions::default(),
    "1. Parent 1\n   1. Child A\n   2. Child B\n2. Parent 2"
)]
#[case::ul_ol_mixed_nested(
    "<ul><li>Outer A<ol><li>Inner 1</li><li>Inner 2</li></ol></li><li>Outer B</li></ul>",
    ConversionOptions::default(),
    "* Outer A\n  1. Inner 1\n  2. Inner 2\n* Outer B"
)]
#[case::ol_ul_mixed_nested(
    "<ol><li>Outer 1<ul><li>Inner A</li><li>Inner B</li></ul></li><li>Outer 2</li></ol>",
    ConversionOptions::default(),
    "1. Outer 1\n   * Inner A\n   * Inner B\n2. Outer 2"
)]
#[case::li_with_inline_elements(
    "<ul><li>Item with <strong>bold</strong> and <a href=\"#\">link</a></li></ul>",
    ConversionOptions::default(),
    "* Item with **bold** and [link](#)"
)]
#[case::li_with_paragraph_inside_treated_as_inline(
    "<ul><li><p>Paragraph text</p></li></ul>",
    ConversionOptions::default(),
    "* Paragraph text"
)]
#[case::li_text_trimming(
    "<ul><li>  Item with spaces  </li></ul>",
    ConversionOptions::default(),
    "*   Item with spaces  "
)]
#[case::deeply_nested_lists(
    concat!(
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
    ),
    ConversionOptions::default(),
    concat!(
        "* L1 A\n",
        "  * L2 A\n",
        "    1. L3 A\n",
        "    2. L3 B\n",
        "  * L2 B\n",
        "* L1 B"
    )
)]
#[case::list_item_starting_with_nested_list(
    "<ul><li><ul><li>Nested Item</li></ul></li><li>Next Item</li></ul>",
    ConversionOptions::default(),
    "* * Nested Item\n* Next Item"
)]
#[case::ordered_list_item_starting_with_nested_list(
    "<ol><li><ol><li>Nested Item</li></ol></li><li>Next Item</li></ol>",
    ConversionOptions::default(),
    "1. 1. Nested Item\n2. Next Item"
)]
#[case::list_with_text_nodes_between_li(
    "<ul> <li>Item 1</li> \n <li>Item 2</li> </ul>",
    ConversionOptions::default(),
    "* Item 1\n* Item 2"
)]
#[case::img_simple(
    "<img src=\"image.png\" alt=\"My Alt Text\">",
    ConversionOptions::default(),
    "![My Alt Text](image.png)"
)]
#[case::img_with_title(
    "<img src=\"image.jpg\" alt=\"Alt\" title=\"My Title\">",
    ConversionOptions::default(),
    "![Alt](image.jpg \"My Title\")"
)]
#[case::table_no_thead_tbody_first_row_as_header(
    "<table><tbody><tr><td>H1 by td</td><td>H2 by td</td></tr><tr><td>C1</td><td>C2</td></tr></tbody></table>",
    ConversionOptions::default(),
    "| H1 by td | H2 by td |\n|---|---|\n| C1 | C2 |"
)]
#[case::table_empty("<table></table>", ConversionOptions::default(), "")]
#[case::table_thead_only(
    "<table><thead><tr><th>H1</th><th>H2</th></tr></thead></table>",
    ConversionOptions::default(),
    "| H1 | H2 |\n|---|---|"
)]
#[case::table_tbody_only_first_row_as_header_no_data(
    "<table><tbody><tr><td>Head1</td><td>Head2</td></tr></tbody></table>",
    ConversionOptions::default(),
    "| Head1 | Head2 |\n|---|---|"
)]
#[case::table_simple(
    "<table><thead><tr><th>H1</th><th>H2</th></tr></thead><tbody><tr><td>C1</td><td>C2</td></tr><tr><td>D1</td><td>D2</td></tr></tbody></table>",
    ConversionOptions::default(),
    "| H1 | H2 |\n|---|---|\n| C1 | C2 |\n| D1 | D2 |"
)]
#[case::table_tbody_empty(
    "<table><tbody></tbody></table>",
    ConversionOptions::default(),
    "" // No header can be formed
)]
#[case::table_tbody_with_empty_tr(
    "<table><tbody><tr></tr><tr><td>Data1</td><td>Data2</td></tr></tbody></table>",
    ConversionOptions::default(),
    "" // No header can be formed
)]
#[case::table_tbody_first_tr_empty_cells_as_header(
    "<table><tbody><tr><td></td><td></td></tr><tr><td>Data1</td><td>Data2</td></tr></tbody></table>",
    ConversionOptions::default(),
    "|  |  |\n|---|---|\n| Data1 | Data2 |"
)]
#[case::table_with_empty_cells(
    "<table><thead><tr><th>H1</th><th>H2</th></tr></thead><tbody><tr><td></td><td>C2</td></tr><tr><td>D1</td><td></td></tr></tbody></table>",
    ConversionOptions::default(),
    "| H1 | H2 |\n|---|---|\n|  | C2 |\n| D1 |  |"
)]
#[case::table_header_cell_empty(
    "<table><thead><tr><th></th><th>H2</th></tr></thead><tbody><tr><td>C1</td><td>C2</td></tr></tbody></table>",
    ConversionOptions::default(),
    "|  | H2 |\n|---|---|\n| C1 | C2 |"
)]
#[case::table_with_inline_elements_in_cells(
    "<table><thead><tr><th><strong>H1</strong></th><th><em>H2</em></th></tr></thead><tbody><tr><td><a href=\"#\">L</a></td><td><code>C</code></td></tr></tbody></table>",
    ConversionOptions::default(),
    "| **H1** | *H2* |\n|---|---|\n| [L](#) | `C` |"
)]
#[case::table_cell_with_pipe_character(
    "<table><thead><tr><th>Header</th></tr></thead><tbody><tr><td>Content | with pipe</td></tr></tbody></table>",
    ConversionOptions::default(),
    "| Header |\n|---|\n| Content \\| with pipe |"
)]
#[case::table_row_with_fewer_cells_than_header(
    "<table><thead><tr><th>H1</th><th>H2</th><th>H3</th></tr></thead><tbody><tr><td>C1</td><td>C2</td></tr></tbody></table>",
    ConversionOptions::default(),
    "| H1 | H2 | H3 |\n|---|---|---|\n| C1 | C2 |  |"
)]
#[case::table_row_with_more_cells_than_header(
    "<table><thead><tr><th>H1</th><th>H2</th></tr></thead><tbody><tr><td>C1</td><td>C2</td><td>C3</td></tr></tbody></table>",
    ConversionOptions::default(),
    "| H1 | H2 |\n|---|---|\n| C1 | C2 |"
)]
#[case::table_with_colspan_ignored(
    "<table><thead><tr><th colspan=\"2\">H</th></tr></thead><tbody><tr><td>A</td><td>B</td></tr></tbody></table>",
    ConversionOptions::default(),
    "| H |\n|---|\n| A |"
)]
#[case::table_with_rowspan_ignored(
    "<table><thead><tr><th>H1</th><th>H2</th></tr></thead><tbody><tr><td rowspan=\"2\">R1C1</td><td>R1C2</td></tr><tr><td>R2C2</td></tr></tbody></table>",
    ConversionOptions::default(),
    "| H1 | H2 |\n|---|---|\n| R1C1 | R1C2 |\n| R2C2 |  |"
)]
#[case::table_thead_with_td_cells(
    "<table><thead><tr><td>H1</td><td>H2</td></tr></thead><tbody><tr><td>C1</td><td>C2</td></tr></tbody></table>",
    ConversionOptions::default(),
    "| H1 | H2 |\n|---|---|\n| C1 | C2 |"
)]
#[case::table_tbody_with_th_cells(
    "<table><thead><tr><th>H1</th></tr></thead><tbody><tr><th>R1C1</th></tr></tbody></table>",
    ConversionOptions::default(),
    "| H1 |\n|---|\n| R1C1 |"
)]
#[case::table_with_tfoot_ignored(
    "<table><thead><tr><th>H1</th></tr></thead><tbody><tr><td>C1</td></tr></tbody><tfoot><tr><td>F1</td></tr></tfoot></table>",
    ConversionOptions::default(),
    "| H1 |\n|---|\n| C1 |"
)]
#[case::s_tag("<s>strike</s>", ConversionOptions::default(), "~~strike~~")]
#[case::strike_tag("<strike>strike</strike>", ConversionOptions::default(), "~~strike~~")]
#[case::del_tag("<del>deleted</del>", ConversionOptions::default(), "~~deleted~~")]
#[case::strikethrough_empty("<s></s>", ConversionOptions::default(), "")]
#[case::strikethrough_with_nested_inline(
    "<s><strong>bold strike</strong></s>",
    ConversionOptions::default(),
    "~~**bold strike**~~"
)]
#[case::kbd_simple("<kbd>Enter</kbd>", ConversionOptions::default(), "<kbd>Enter</kbd>")]
#[case::kbd_multiple_keys(
    "<p><kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>Del</kbd></p>",
    ConversionOptions::default(),
    "<kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>Del</kbd>"
)]
#[case::kbd_empty("<kbd></kbd>", ConversionOptions::default(), "<kbd></kbd>")]
#[case::kbd_with_nested_inline("<kbd><em>File</em></kbd>", ConversionOptions::default(), "<kbd>*File*</kbd>")]
#[case::style_tag_is_ignored(
    "<style>body { font-size: 16px; }</style><p>Text</p>",
    ConversionOptions::default(),
    "Text"
)]
#[case::style_tag_with_type_is_ignored(
    "<style type=\"text/css\">/* A comment */ h1 { color: blue; }</style>Next",
    ConversionOptions::default(),
    "Next"
)]
#[case::style_tag_empty_is_ignored("<style></style><p>Content</p>", ConversionOptions::default(), "Content")]
#[case::u_tag_simple("<u>underlined</u>", ConversionOptions::default(), "underlined")]
#[case::u_tag_empty("<u></u>", ConversionOptions::default(), "")]
#[case::u_tag_with_nested_inline(
    "<u><em>italic underline</em></u>",
    ConversionOptions::default(),
    "*italic underline*"
)]
#[case::u_tag_in_paragraph(
    "<p>This is <u>important</u>.</p>",
    ConversionOptions::default(),
    "This is <u>important</u>."
)]
#[case::blockquote_simple_paragraph(
    "<blockquote><p>Quoted text.</p></blockquote>",
    ConversionOptions::default(),
    "> Quoted text."
)]
#[case::blockquote_multiple_paragraphs(
    "<blockquote><p>First paragraph.</p><p>Second paragraph.</p></blockquote>",
    ConversionOptions::default(),
    "> First paragraph.\n> \n> Second paragraph."
)]
#[case::blockquote_nested(
    "<blockquote><p>Level 1</p><blockquote><p>Level 2</p></blockquote></blockquote>",
    ConversionOptions::default(),
    "> Level 1\n> \n> > Level 2"
)]
#[case::blockquote_empty("<blockquote></blockquote>", ConversionOptions::default(), ">")]
#[case::blockquote_with_list(
    "<blockquote><ul><li>Item 1</li><li>Item 2</li></ul></blockquote>",
    ConversionOptions::default(),
    "> * Item 1\n> * Item 2"
)]
#[case::blockquote_with_heading(
    "<blockquote><h1>Heading</h1></blockquote>",
    ConversionOptions::default(),
    "> # Heading"
)]
#[case::pre_code_simple(
    "<pre><code>Hello World</code></pre>",
    ConversionOptions::default(),
    "```\nHello World\n```"
)]
#[case::pre_code_with_language_class(
    "<pre><code class=\"language-rust\">fn main() {}</code></pre>",
    ConversionOptions::default(),
    "```rust\nfn main() {}\n```"
)]
#[case::pre_code_with_lang_class(
    "<pre><code class=\"lang-js\">console.log('hi');</code></pre>",
    ConversionOptions::default(),
    "```js\nconsole.log('hi');\n```"
)]
#[case::pre_plain_text(
    "<pre>Plain text content.</pre>",
    ConversionOptions::default(),
    "```\nPlain text content.\n```"
)]
#[case::pre_with_html_entities(
    "<pre><code>&lt;div&gt; &amp; &quot; &#39; &lt;/div&gt;</code></pre>",
    ConversionOptions::default(),
    "```\n<div> & \" ' </div>\n```"
)]
#[case::pre_empty("<pre></pre>", ConversionOptions::default(), "```\n\n```")]
#[case::pre_code_empty("<pre><code></code></pre>", ConversionOptions::default(), "```\n\n```")]
#[case::pre_leading_newline_stripping_1("<pre>\nCode here</pre>", ConversionOptions::default(), "```\nCode here\n```")]
#[case::pre_leading_newline_stripping_2(
    "<pre><code>\nCode here</code></pre>",
    ConversionOptions::default(),
    "```\nCode here\n```"
)]
#[case::pre_trailing_newline_handling_1("<pre>Code here\n</pre>", ConversionOptions::default(), "```\nCode here\n```")]
#[case::pre_trailing_newline_handling_2(
    "<pre>Code here\n\n</pre>",
    ConversionOptions::default(),
    "```\nCode here\n```"
)]
#[case::pre_with_br_tags("<pre>Line1<br>Line2</pre>", ConversionOptions::default(), "```\nLine1\nLine2\n```")]
#[case::pre_code_with_multiple_classes(
    "<pre><code class=\"foo language-python bar\">print('hello')</code></pre>",
    ConversionOptions::default(),
    "```python\nprint('hello')\n```"
)]
#[case::img_no_alt("<img src=\"foo.gif\">", ConversionOptions::default(), "![](foo.gif)")]
#[case::img_empty_alt("<img src=\"bar.jpeg\" alt=\"\">", ConversionOptions::default(), "![](bar.jpeg)")]
#[case::img_no_title(
    "<img src=\"image.png\" alt=\"My Alt Text\">",
    ConversionOptions::default(),
    "![My Alt Text](image.png)"
)]
#[case::img_empty_title(
    "<img src=\"image.png\" alt=\"Alt Text\" title=\"\">",
    ConversionOptions::default(),
    "![Alt Text](image.png)"
)]
#[case::img_no_src("<img alt=\"Alt Text\">", ConversionOptions::default(), "")]
#[case::img_empty_src("<img src=\"\" alt=\"Alt Text\">", ConversionOptions::default(), "")]
#[case::img_in_paragraph(
    "<p>Some text <img src=\"i.png\" alt=\"inline\"> and more text.</p>",
    ConversionOptions::default(),
    "Some text ![inline](i.png) and more text."
)]
#[case::img_title_with_quotes(
    "<img src=\"a.png\" alt=\"alt\" title=\"A title with &quot;quotes&quot; inside\">",
    ConversionOptions::default(),
    "![alt](a.png \"A title with \\\"quotes\\\" inside\")"
)]
#[case::img_all_attributes_empty_except_src(
    "<img src=\"b.png\" alt=\"\" title=\"\">",
    ConversionOptions::default(),
    "![](b.png)"
)]
#[case::img_url_with_special_chars(
    "<img src=\"images/my image (new).jpg\" alt=\"special\">",
    ConversionOptions::default(),
    "![special](<images/my%20image%20(new).jpg>)"
)]
#[case::h1_simple("<h1>Hello World</h1>", ConversionOptions::default(), "# Hello World")]
#[case::h2_simple("<h2>Hello World</h2>", ConversionOptions::default(), "## Hello World")]
#[case::h3_simple("<h3>Hello World</h3>", ConversionOptions::default(), "### Hello World")]
#[case::h4_simple("<h4>Hello World</h4>", ConversionOptions::default(), "#### Hello World")]
#[case::h5_simple("<h5>Hello World</h5>", ConversionOptions::default(), "##### Hello World")]
#[case::h6_simple("<h6>Hello World</h6>", ConversionOptions::default(), "###### Hello World")]
#[case::h1_with_attributes(
    "<h1 id=\"main-title\" class=\"important\">Hello</h1>",
    ConversionOptions::default(),
    "# Hello"
)]
#[case::h2_empty("<h2></h2>", ConversionOptions::default(), "## ")] // Or just "##" - common practice is a space after #
#[case::h3_with_whitespace("<h3>  Spaced Out  </h3>", ConversionOptions::default(), "###  Spaced Out ")]
#[case::multiple_headings(
    "<h1>First</h1><h2>Second</h2>",
    ConversionOptions::default(),
    "# First\n\n## Second"
)]
#[case::heading_with_inline_strong(
    "<h1>Hello <strong>World</strong></h1>",
    ConversionOptions::default(),
    "# Hello **World**"
)]
#[case::heading_with_inline_em("<h2>Hello <em>World</em></h2>", ConversionOptions::default(), "## Hello *World*")]
#[case::heading_mixed_content(
    "<h3>Part1 <strong>bold</strong> Part2</h3>",
    ConversionOptions::default(),
    "### Part1 **bold** Part2"
)]
#[case::h1_malformed_open("<h1>Hello World</h_oops>", ConversionOptions::default(), "# Hello World")]
#[case::strong_simple("<strong>Hello</strong>", ConversionOptions::default(), "**Hello**")]
#[case::em_simple("<em>World</em>", ConversionOptions::default(), "*World*")]
#[case::strong_with_attributes("<strong class=\"bold\">Text</strong>", ConversionOptions::default(), "**Text**")]
#[case::em_empty("<em></em>", ConversionOptions::default(), "")]
#[case::strong_in_paragraph(
    "<p>This is <strong>bold</strong> text.</p>",
    ConversionOptions::default(),
    "This is **bold** text."
)]
#[case::em_in_paragraph(
    "<p>This is <em>italic</em> text.</p>",
    ConversionOptions::default(),
    "This is *italic* text."
)]
#[case::strong_and_em_in_paragraph(
    "<p><strong>Bold</strong> and <em>italic</em>.</p>",
    ConversionOptions::default(),
    "**Bold** and *italic*."
)]
#[case::nested_strong_em(
    "<strong><em>Bold Italic</em></strong>",
    ConversionOptions::default(),
    "***Bold Italic***"
)]
#[case::nested_em_strong(
    "<em><strong>Italic Bold</strong></em>",
    ConversionOptions::default(),
    "***Italic Bold***"
)]
#[case::strong_in_heading_now_correctly_formatted(
    "<h1>Hello <strong>World</strong></h1>",
    ConversionOptions::default(),
    "# Hello **World**"
)]
#[case::em_in_heading_now_correctly_formatted(
    "<h2>Hello <em>World</em></h2>",
    ConversionOptions::default(),
    "## Hello *World*"
)]
#[case::mixed_content_in_heading_correctly_formatted(
    "<h3>Part1 <strong>bold</strong> and <em>italic</em> Part2</h3>",
    ConversionOptions::default(),
    "### Part1 **bold** and *italic* Part2"
)]
#[case::strong_with_internal_whitespace("<strong>  spaced  </strong>", ConversionOptions::default(), "** spaced **")]
#[case::em_around_strong(
    "<em>Emphasis around <strong>bold</strong> text.</em>",
    ConversionOptions::default(),
    "*Emphasis around **bold** text.*"
)]
#[case::strong_around_em(
    "<strong>Bold around <em>emphasis</em> text.</strong>",
    ConversionOptions::default(),
    "**Bold around *emphasis* text.**"
)]
// --- Link (<a>) Tests ---
#[case::link_simple(
    "<a href=\"https://example.com\">Example</a>",
    ConversionOptions::default(),
    "[Example](https://example.com)"
)]
#[case::link_with_title(
    "<a href=\"https://example.com\" title=\"Cool Site\">Example</a>",
    ConversionOptions::default(),
    "[Example](https://example.com \"Cool Site\")"
)]
#[case::link_empty_text(
    "<a href=\"https://example.com\"></a>",
    ConversionOptions::default(),
    "[](https://example.com)"
)]
#[case::link_href_empty_processed("<a href=\"\"></a>", ConversionOptions::default(), "[](<>)")]
#[case::link_no_href("<a name=\"anchor\">Anchor Text</a>", ConversionOptions::default(), "Anchor Text")]
#[case::link_with_emphasized_text(
    "<a href=\"/foo\"><em>italic link</em></a>",
    ConversionOptions::default(),
    "[*italic link*](/foo)"
)]
#[case::link_with_strong_text(
    "<a href=\"/bar\"><strong>bold link</strong></a>",
    ConversionOptions::default(),
    "[**bold link**](/bar)"
)]
#[case::link_with_mixed_emphasis_text(
    "<a href=\"/baz\">normal <strong>bold</strong> <em>italic</em></a>",
    ConversionOptions::default(),
    "[normal **bold** *italic*](/baz)"
)]
#[case::link_relative_url(
    "<a href=\"../index.html\">Go Back</a>",
    ConversionOptions::default(),
    "[Go Back](../index.html)"
)]
#[case::link_url_with_spaces_and_parentheses(
    "<a href=\"/url%20with%20spaces(and%29parentheses.html\">Link</a>",
    ConversionOptions::default(),
    "[Link](</url%20with%20spaces(and%29parentheses.html>)"
)]
#[case::link_url_with_unescaped_parentheses_in_href(
    "<a href=\"/a(b)c\">text</a>",
    ConversionOptions::default(),
    "[text](</a(b)c>)"
)]
#[case::link_href_with_spaces_only(
    "<a href=\"/url with spaces\">Link</a>",
    ConversionOptions::default(),
    "[Link](</url%20with%20spaces>)"
)]
#[case::link_title_with_quotes(
    "<a href=\"/foo\" title=\"A &quot;quoted&quot; title\">QLink</a>",
    ConversionOptions::default(),
    "[QLink](/foo \"A \\\"quoted\\\" title\")"
)]
#[case::link_in_paragraph(
    "<p>Here is a <a href=\"#\">link</a>.</p>",
    ConversionOptions::default(),
    "Here is a [link](#)."
)]
#[case::link_in_heading(
    "<h2>Heading with <a href=\"/s\"><strong>strong link</strong></a></h2>",
    ConversionOptions::default(),
    "## Heading with [**strong link**](/s)"
)]
#[case::link_complex_content_and_title(
    "<a href=\"/path\" title=\"A 'single' & &quot;double&quot; title\"><em>Italic</em> and <strong>Bold</strong> Link Text</a>",
    ConversionOptions::default(),
    "[*Italic* and **Bold** Link Text](/path \"A 'single' & \\\"double\\\" title\")"
)]
#[case::h1_not_closed("<h1>Hello World", ConversionOptions::default(), "# Hello World")]
#[case::use_title_as_h1_true(
    "<html><head><title>My Document</title></head><body><p>Body text</p></body></html>",
    ConversionOptions {
        use_title_as_h1: true,
        ..ConversionOptions::default()
    },
    "# My Document\n\nBody text"
)]
#[case::use_title_as_h1_true_with_no_body(
    "<html><head><title>Only Title</title></head><body></body></html>",
    ConversionOptions {
        use_title_as_h1: true,
        ..ConversionOptions::default()
    },
    "# Only Title"
)]
#[case::use_title_as_h1_true_with_no_title(
    "<html><head></head><body><p>Body only</p></body></html>",
    ConversionOptions {
        use_title_as_h1: true,
        ..ConversionOptions::default()
    },
    "Body only"
)]
#[case::use_title_as_h1_false(
    "<html><head><title>My Document</title></head><body><p>Body text</p></body></html>",
    ConversionOptions {
        use_title_as_h1: false,
        ..ConversionOptions::default()
    },
    "My Document\n\nBody text"
)]
fn test_html_to_markdown(#[case] html: &str, #[case] options: ConversionOptions, #[case] expected: &str) {
    assert_conversion_with_options(html, expected, options);
}

// TODO: Add tests for headings with links, images etc. once those elements are supported.

// Test for parsing error on malformed heading (illustrative, might need adjustment based on parser behavior)
// At this stage, the generic "parsing not yet fully implemented" error is expected for unhandled valid tags,
// but malformed tags might also trigger it or a more specific error once the parser is more developed.
