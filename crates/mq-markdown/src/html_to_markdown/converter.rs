#[cfg(feature = "html-to-markdown")]
use super::node::HtmlNode;
#[cfg(feature = "html-to-markdown")]
use super::error::HtmlToMarkdownError;

// Helper function to convert child nodes into a single string, suitable for inline contexts.
#[cfg(feature = "html-to-markdown")]
fn convert_children_to_string(nodes: &[HtmlNode], html_input_for_error: &str) -> Result<String, HtmlToMarkdownError> {
    let mut parts = Vec::new();
    for node in nodes {
        match node {
            HtmlNode::Text(text) => parts.push(text.clone()), // Keep original spacing for now
            HtmlNode::Element(element) => {
                // Recursively convert child elements.
                let link_text = convert_children_to_string(&element.children, html_input_for_error)?;
                match element.tag_name.as_str() {
                    "strong" => {
                        if !link_text.is_empty() { parts.push(format!("**{}**", link_text)); }
                    }
                    "em" => {
                        if !link_text.is_empty() { parts.push(format!("*{}*", link_text)); }
                    }
                    "a" => {
                        if let Some(Some(href)) = element.attributes.get("href") {
                            let mut title_part = String::new();
                            if let Some(Some(title)) = element.attributes.get("title") {
                                if !title.is_empty() {
                                    // Escape double quotes in title for Markdown
                                    title_part = format!(" \"{}\"", title.replace('"', "\\\""));
                                }
                            }
                            // TODO: URL Escaping for characters like '(', ')', ' ' if not already handled or if using < >.
                            // For now, assume href is mostly okay or already encoded.
                            // Markdown common practice: URL encode spaces to %20. Parentheses can be problematic.
                            // For simplicity, not adding < > around URL yet unless tests show necessity.
                            parts.push(format!("[{}]({}{})", link_text, href, title_part));
                        } else {
                            // No href, treat as plain text (e.g., <a name="anchor">text</a>)
                            parts.push(link_text);
                        }
                    }
                    // TODO: Add "code" tag handling if desired: parts.push(format!("`{}`", link_text));
                    "span" => parts.push(link_text), // Spans are usually for styling, content passed through
                    _ => parts.push(link_text), // Unhandled inline tags, pass content through
                }
            }
            HtmlNode::Comment(_) => {}
        }
    }
    // Join parts and then trim the final string.
    // Trimming here helps remove leading/trailing whitespace from the combined content of a block element
    // and also handles cases like <em></em> becoming an empty string rather than "**".
    Ok(parts.join("").trim().to_string())
}

#[cfg(feature = "html-to-markdown")]
pub fn convert_nodes_to_markdown(nodes: &[HtmlNode], html_input_for_error: &str) -> Result<String, HtmlToMarkdownError> {
    let mut markdown_blocks = Vec::new();

    for node in nodes {
        match node {
            HtmlNode::Text(text) => {
                let trimmed_text = text.trim();
                if !trimmed_text.is_empty() {
                    // Top-level text (not inside any block element from the input) becomes a paragraph.
                    markdown_blocks.push(trimmed_text.to_string());
                }
            }
            HtmlNode::Element(element) => {
                // `convert_children_to_string` is designed to get the content of an element,
                // applying inline formatting like strong/em to its children recursively.
                let children_content_str = convert_children_to_string(&element.children, html_input_for_error)?;

                match element.tag_name.as_str() {
                    "h1" => markdown_blocks.push(format!("# {}", children_content_str)),
                    "h2" => markdown_blocks.push(format!("## {}", children_content_str)),
                    "h3" => markdown_blocks.push(format!("### {}", children_content_str)),
                    "h4" => markdown_blocks.push(format!("#### {}", children_content_str)),
                    "h5" => markdown_blocks.push(format!("##### {}", children_content_str)),
                    "h6" => markdown_blocks.push(format!("###### {}", children_content_str)),
                    "p" => {
                        markdown_blocks.push(children_content_str);
                    }
                    // These are inline elements. If they appear at the top-level (which is unusual for valid HTML structure
                    // but possible with fragments or simple inputs), we'll wrap their content.
                    // The `convert_children_to_string` already applies these, so this might be redundant if the
                    // input is just "<strong>text</strong>".
                    // The key is that `convert_children_to_string` should return "text" for the children of <strong>,
                    // and then here we wrap it.
                    // Let's test this carefully.
                    // If children_content_str is already "**text**" from convert_children_to_string (it shouldn't be,
                    // convert_children_to_string is for the *content* of the current element),
                    // then this would double-wrap.
                    // The current convert_children_to_string will apply formatting.
                    // So, if we call convert_children_to_string on <strong>'s children, we get "text".
                    // Then here, we format it. This seems correct.
                    "strong" => {
                        if !children_content_str.is_empty() {
                            markdown_blocks.push(format!("**{}**", children_content_str));
                        } else {
                            // If strong is empty, push nothing or an empty string if required by block structure
                            // For now, let the joining logic skip it if it becomes an empty part.
                        }
                    }
                    "em" => {
                        if !children_content_str.is_empty() {
                            markdown_blocks.push(format!("*{}*", children_content_str));
                        } else {
                            // Handle empty em similarly
                        }
                    }
                    _ => {
                        if !children_content_str.is_empty() {
                            markdown_blocks.push(children_content_str);
                        } else if ["ul", "ol", "blockquote", "pre", "hr"].contains(&element.tag_name.as_str()) {
                            markdown_blocks.push(children_content_str);
                        }
                    }
                }
            }
            HtmlNode::Comment(_) => {
                // Comments are ignored
            }
        }
    }

    // Join block-level markdown parts with two newlines
    let mut result = String::new();
    for (i, block_content) in markdown_blocks.iter().enumerate() {
        if i > 0 {
            result.push_str("\n\n");
        }
        result.push_str(block_content);
    }
    Ok(result)
}
