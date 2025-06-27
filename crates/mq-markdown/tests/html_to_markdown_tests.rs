#[cfg(feature = "html-to-markdown")]
use mq_markdown::{convert_html_to_markdown, HtmlToMarkdownError};

#[cfg(feature = "html-to-markdown")]
fn assert_conversion(html: &str, expected_markdown: &str) {
    match convert_html_to_markdown(html) {
        Ok(markdown) => assert_eq!(markdown, expected_markdown),
        Err(e) => panic!("Conversion failed for HTML '{}': {:?}", html, e),
    }
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
fn test_link_href_empty() {
    assert_conversion("<a href=\"\">empty href</a>", "[empty href]()");
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
    // If href="foo bar.html", output "[text](foo%20bar.html)" is common.
    // If href="/path(with)parens", output "[text](/path(with)parens)" or "[text](</path(with)parens>)"
    // We'll aim for direct passthrough for now and refine if specific encoding/escaping is needed by Markdown spec.
    assert_conversion("<a href=\"/url%20with%20spaces(and%29parentheses.html\">Link</a>", "[Link](/url%20with%20spaces(and%29parentheses.html)");
}

#[cfg(feature = "html-to-markdown")]
#[test]
fn test_link_url_with_unescaped_parentheses_in_href() {
    // Markdown requires parentheses in URL to be escaped or the URL enclosed in <>
    // For now, we will assume the parser provides the href as is, and converter might need to handle this.
    // Let's test a simple case. If href="/a(b)c", output could be "[text](/a(b)c)" which is fine for many renderers,
    // or ideally "[text](</a(b)c>)" or "[text](/a\(b\)c)".
    // For now, direct passthrough:
    assert_conversion("<a href=\"/a(b)c\">text</a>", "[text](/a(b)c)");
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
    assert_conversion("<h2>Heading with <a href=\"/s\">strong link</a></h2>", "## Heading with [strong link](/s)");
    // This will require strong to be handled correctly within link text if not already.
    // The test `test_link_with_strong_text` covers `[**bold link**](url)`
    // So, this should be "## Heading with [**strong link**](/s)" if strong is implemented in links.
    // Let's update this expectation once strong in link is confirmed.
    // For now, assume link text doesn't re-process for strong/em if parser is simple.
    // No, convert_children_to_string should handle this:
    // assert_conversion("<h2>Heading with <a href=\"/s\"><strong>strong link</strong></a></h2>", "## Heading with [**strong link**](/s)");
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
