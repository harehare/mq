#[cfg(feature = "html-to-markdown")]
use super::node::HtmlNode;
#[cfg(feature = "html-to-markdown")]
use super::node::HtmlElement; // Added for convenience
#[cfg(feature = "html-to-markdown")]
use super::error::HtmlToMarkdownError;

#[cfg(feature = "html-to-markdown")]
fn extract_text_from_pre_children(nodes: &[HtmlNode]) -> String {
    let mut text_content = String::new();
    for node in nodes {
        match node {
            HtmlNode::Text(text) => text_content.push_str(text),
            HtmlNode::Element(el) if el.tag_name == "br" => text_content.push('\n'),
            HtmlNode::Element(el) if el.tag_name == "code" => { // If <code> is child of <pre> or another element
                text_content.push_str(&extract_text_from_pre_children(&el.children));
            }
            HtmlNode::Element(el) => { // For other elements, naively recurse.
                                       // This might not be what's desired for all cases inside <pre>.
                text_content.push_str(&extract_text_from_pre_children(&el.children));
            }
            HtmlNode::Comment(_) => {}
        }
    }
    text_content
}

#[cfg(feature = "html-to-markdown")]
#[derive(PartialEq, Debug, Clone, Copy)]
enum Alignment { Left, Center, Right, Default }

#[cfg(feature = "html-to-markdown")]
fn get_cell_alignment(element: &HtmlElement) -> Alignment {
    if let Some(Some(style_attr)) = element.attributes.get("style") {
        for part in style_attr.split(';') {
            let sub_parts: Vec<&str> = part.trim().splitn(2, ':').collect();
            if sub_parts.len() == 2 && sub_parts[0].trim() == "text-align" {
                match sub_parts[1].trim().to_lowercase().as_str() {
                    "left" => return Alignment::Left,
                    "center" => return Alignment::Center,
                    "right" => return Alignment::Right,
                    _ => {}
                }
            }
        }
    }
    if let Some(Some(align_attr)) = element.attributes.get("align") {
        match align_attr.to_lowercase().as_str() {
            "left" => return Alignment::Left,
            "center" => return Alignment::Center,
            "right" => return Alignment::Right,
            _ => {}
        }
    }
    Alignment::Default
}

#[cfg(feature = "html-to-markdown")]
fn escape_table_cell_content(content: &str) -> String {
    content.replace("|", "\\|")
}

#[cfg(feature = "html-to-markdown")]
fn convert_html_table_to_markdown(
    table_element: &HtmlElement,
    html_input_for_error: &str,
) -> Result<String, HtmlToMarkdownError> {
    let mut header_cells: Vec<String> = Vec::new();
    let mut header_alignments: Vec<Alignment> = Vec::new(); // For storing alignments
    let mut body_rows: Vec<Vec<String>> = Vec::new();

    let mut first_tbody_first_row_used_as_header = false;

    // Attempt to find header from <thead>
    for node in &table_element.children {
        if let HtmlNode::Element(thead_element) = node {
            if thead_element.tag_name == "thead" {
                if let Some(tr_node) = thead_element.children.iter().find(|n| matches!(n, HtmlNode::Element(el) if el.tag_name == "tr")) {
                    if let HtmlNode::Element(tr_element) = tr_node {
                        for cell_node in &tr_element.children {
                            if let HtmlNode::Element(cell_element) = cell_node {
                                if cell_element.tag_name == "th" || cell_element.tag_name == "td" { // Changed from th || td to th only for thead priority
                                    let cell_content = convert_children_to_string(&cell_element.children, html_input_for_error)?;
                                    header_cells.push(escape_table_cell_content(cell_content.trim()));
                                    header_alignments.push(get_cell_alignment(cell_element));
                                }
                            }
                        }
                    }
                }
                break;
            }
        }
    }

    // If no header from <thead>, try first row of first <tbody>
    if header_cells.is_empty() {
        for node in &table_element.children {
            if let HtmlNode::Element(tbody_element) = node {
                if tbody_element.tag_name == "tbody" {
                    if let Some(tr_node) = tbody_element.children.iter().find(|n| matches!(n, HtmlNode::Element(el) if el.tag_name == "tr")) {
                         if let HtmlNode::Element(tr_element) = tr_node {
                            for cell_node in &tr_element.children {
                                if let HtmlNode::Element(cell_element) = cell_node {
                                    if cell_element.tag_name == "td" || cell_element.tag_name == "th" {
                                        let cell_content = convert_children_to_string(&cell_element.children, html_input_for_error)?;
                                        header_cells.push(escape_table_cell_content(cell_content.trim()));
                                    }
                                }
                            }
                            if !header_cells.is_empty() {
                                first_tbody_first_row_used_as_header = true;
                            }
                        }
                    }
                    break; // Only consider the first <tbody> for this fallback
                }
            }
        }
    }

    // If still no header, cannot create a GFM table
    if header_cells.is_empty() {
        return Ok("".to_string());
    }
    let column_count = header_cells.len();


    // Find and process tbody for body rows
    let mut first_tbody_processed_for_data = false;
    for node in &table_element.children {
        if let HtmlNode::Element(tbody_element) = node {
            if tbody_element.tag_name == "tbody" {
                let mut rows_to_iterate = tbody_element.children.iter();

                if first_tbody_first_row_used_as_header && !first_tbody_processed_for_data {
                    // Skip the first <tr> node if it was used as header
                    rows_to_iterate.next(); // Advance iterator once
                    first_tbody_processed_for_data = true;
                }

                for tr_node in rows_to_iterate {
                    if let HtmlNode::Element(tr_element) = tr_node {
                        if tr_element.tag_name == "tr" {
                            let mut current_row_cells: Vec<String> = Vec::new();
                            for td_node in &tr_element.children {
                                if let HtmlNode::Element(td_element) = td_node {
                                    if td_element.tag_name == "td" || td_element.tag_name == "th" {
                                        let cell_content = convert_children_to_string(&td_element.children, html_input_for_error)?;
                                        current_row_cells.push(escape_table_cell_content(cell_content.trim()));
                                    }
                                }
                            }
                            body_rows.push(current_row_cells);
                        }
                    }
                }
            }
        }
    }

    // column_count is now definitively set from header_cells.len()
    // If header_cells was not empty, column_count is at least 0 (e.g. for <tr></tr> in header).
    // Markdown construction logic can proceed.

    let mut markdown_table = String::new();

    markdown_table.push_str("| ");
    markdown_table.push_str(&header_cells.join(" | "));
    markdown_table.push_str(" |\n");

    markdown_table.push_str("|");
    for i in 0..column_count {
        let align = header_alignments.get(i).unwrap_or(&Alignment::Default);
        let sep_str = match align {
            Alignment::Left => ":---",
            Alignment::Center => ":---:",
            Alignment::Right => "---:",
            Alignment::Default => "---",
        };
        markdown_table.push_str(sep_str);
        markdown_table.push_str("|");
    }
    markdown_table.push_str("\n");

    for row_cells in &body_rows {
        markdown_table.push_str("| ");
        let mut current_col_idx = 0;
        for cell_idx in 0..column_count {
            if let Some(cell_content) = row_cells.get(cell_idx) {
                markdown_table.push_str(cell_content);
            }
            // else: cell is empty if row_cells.get(cell_idx) is None (row has fewer cells than header)
            markdown_table.push_str(" | ");
            current_col_idx +=1;
        }
        // Remove trailing " | "
        if column_count > 0 {
             markdown_table.truncate(markdown_table.len() - 3);
        }
        markdown_table.push_str(" |\n");
    }

    Ok(markdown_table.trim_end_matches('\n').to_string())
}

#[cfg(feature = "html-to-markdown")]
fn process_url_for_markdown(url: &str) -> String {
    let mut processed_url = url.replace(" ", "%20");

    let needs_angle_brackets = url.is_empty() ||
                               url.contains(' ') ||
                               processed_url.contains('(') ||
                               processed_url.contains(')');
    if needs_angle_brackets {
        format!("<{}>", processed_url)
    } else {
        processed_url
    }
}


#[cfg(feature = "html-to-markdown")]
fn convert_html_list_to_markdown(
    list_element: &HtmlElement, // This should be ul or ol
    indent_level: usize,
    html_input_for_error: &str,
    extract_scripts_as_code_blocks: bool, // New option
) -> Result<String, HtmlToMarkdownError> {
    let mut markdown_items = Vec::new();
    let base_indent = "    ".repeat(indent_level); // 4 spaces per indent level

    let mut current_list_number = if list_element.tag_name == "ol" {
        list_element.attributes.get("start")
            .and_then(|opt_val| opt_val.as_ref())
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(1)
    } else {
        0 // Not used for ul
    };

    for node in &list_element.children {
        if let HtmlNode::Element(li_element) = node {
            if li_element.tag_name == "li" {
                let marker_prefix = match list_element.tag_name.as_str() {
                    "ul" => "* ".to_string(),
                    "ol" => {
                        let m = format!("{}. ", current_list_number);
                        current_list_number += 1;
                        m
                    }
                    // This case should be unreachable if called correctly for 'ul' or 'ol'
                    _ => return Err(HtmlToMarkdownError::ParseError { html_snippet: format!("Unexpected list type: {}", list_element.tag_name), message: "Expected 'ul' or 'ol'".to_string() }),
                };

                let li_content_markdown = convert_nodes_to_markdown(&li_element.children, html_input_for_error, extract_scripts_as_code_blocks)?;

                if li_content_markdown.is_empty() {
                    markdown_items.push(format!("{}{}", base_indent, marker_prefix));
                } else {
                    let mut first_line_in_li = true;
                    for line in li_content_markdown.lines() {
                        if first_line_in_li {
                            markdown_items.push(format!("{}{}{}", base_indent, marker_prefix, line));
                            first_line_in_li = false;
                        } else {
                            let continuation_indent = " ".repeat(marker_prefix.len());
                            markdown_items.push(format!("{}{}{}", base_indent, continuation_indent, line));
                        }
                    }
                }
            }
        } else if let HtmlNode::Text(text_content) = node {
            if !text_content.trim().is_empty() {
                // Text nodes between <li> elements are usually ignored.
                // If they must be preserved, they break typical list formatting.
                // For now, they are effectively ignored.
            }
        }
        // Comments are also ignored here by omission.
    }
    Ok(markdown_items.join("\n"))
}


// Helper function to convert child nodes into a single string, suitable for inline contexts.
#[cfg(feature = "html-to-markdown")]
pub fn convert_children_to_string(nodes: &[HtmlNode], html_input_for_error: &str) -> Result<String, HtmlToMarkdownError> {
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
                            let title_part = element.attributes.get("title")
                                .and_then(|opt_title| opt_title.as_ref())
                                .filter(|title_str| !title_str.is_empty())
                                .map(|title_str| format!(" \"{}\"", title_str.replace('"', "\\\"")))
                                .unwrap_or_default();

                            let processed_href = process_url_for_markdown(href);
                            parts.push(format!("[{}]({}{})", link_text, processed_href, title_part));
                        } else {
                            // No href, treat as plain text (e.g., <a name="anchor">text</a>)
                            if !link_text.is_empty() { parts.push(link_text); }
                        }
                    }
                    "code" => {
                        // For inline code, content is usually text.
                        // link_text is the result of convert_children_to_string on <code>'s children.
                        if !link_text.is_empty() {
                            parts.push(format!("`{}`", link_text));
                        } else {
                            parts.push("``".to_string()); // Markdown for empty inline code
                        }
                    }
                    "br" => {
                        // HTML <br> typically means a hard line break.
                        parts.push("  \n".to_string());
                    }
                    "img" => {
                        // <img> is an empty element, so link_text (children content) is not relevant.
                        if let Some(Some(src_url)) = element.attributes.get("src") {
                            if !src_url.is_empty() {
                                let alt_text = element.attributes.get("alt")
                                    .and_then(|opt_alt| opt_alt.as_ref())
                                    .map(|s| s.as_str()) // No complex escaping needed for alt text itself in Markdown spec
                                    .unwrap_or("");

                                let title_part = element.attributes.get("title")
                                    .and_then(|opt_title| opt_title.as_ref())
                                    .filter(|title_str| !title_str.is_empty())
                                    .map(|title_str| format!(" \"{}\"", title_str.replace('"', "\\\"")))
                                    .unwrap_or_default();

                                let processed_src = process_url_for_markdown(src_url);
                                parts.push(format!("![{}]({}{})", alt_text, processed_src, title_part));
                            }
                            // If src_url is empty, effectively ignore the tag by not pushing anything.
                        }
                        // If src attribute is missing, also ignore the tag.
                    }
                    "input" => {
                       if let Some(Some(type_attr)) = element.attributes.get("type") {
                           if type_attr.to_lowercase() == "checkbox" {
                               let checked = element.attributes.contains_key("checked");
                               if checked {
                                   parts.push("[x] ".to_string());
                               } else {
                                   parts.push("[ ] ".to_string());
                               }
                           }
                           // Other input types are ignored and produce no output.
                       }
                       // If type attribute is missing, ignore.
                    }
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
pub fn convert_nodes_to_markdown(
    nodes: &[HtmlNode],
    html_input_for_error: &str,
    extract_scripts_as_code_blocks: bool, // New option
) -> Result<String, HtmlToMarkdownError> {
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
                    "hr" => {
                        markdown_blocks.push("---".to_string());
                    }
                    "ul" | "ol" => {
                        // Initial indent level for top-level list is 0
                        let list_md = convert_html_list_to_markdown(element, 0, html_input_for_error, extract_scripts_as_code_blocks)?;
                        markdown_blocks.push(list_md);
                    }
                    "blockquote" => {
                        // Recursively call convert_nodes_to_markdown for the children.
                        // The content generated by the recursive call will be plain Markdown.
                        let inner_markdown = convert_nodes_to_markdown(&element.children, html_input_for_error, extract_scripts_as_code_blocks)?;
                        if !inner_markdown.is_empty() {
                            let quoted_lines: Vec<String> = inner_markdown
                                .lines()
                                .map(|line| format!("> {}", line)) // Add "> " to each line
                                .collect();
                            markdown_blocks.push(quoted_lines.join("\n"));
                        } else {
                            markdown_blocks.push(">".to_string()); // Empty blockquote is just ">"
                        }
                    }
                    "pre" => {
                        let mut lang_specifier = String::new();
                        let mut content_nodes = &element.children; // Default to <pre>'s children

                        // Check if the first child of <pre> is a <code> element
                        if let Some(HtmlNode::Element(code_element)) = element.children.get(0) {
                            if code_element.tag_name == "code" {
                                // If so, content is from <code>'s children
                                content_nodes = &code_element.children;
                                // And check for language class on the <code> element
                                if let Some(Some(class_attr)) = code_element.attributes.get("class") {
                                    for class_name in class_attr.split_whitespace() {
                                        if let Some(lang) = class_name.strip_prefix("language-") {
                                            lang_specifier = lang.to_string();
                                            break;
                                        } else if let Some(lang) = class_name.strip_prefix("lang-") {
                                            lang_specifier = lang.to_string();
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                        let mut text_content = extract_text_from_pre_children(content_nodes);
                        if text_content.starts_with('\n') { text_content.remove(0); } // Remove one leading newline
                        // Trailing newlines: Markdown ``` usually implies one. Let's trim all from content.
                        markdown_blocks.push(format!("```{}\n{}\n```", lang_specifier, text_content.trim_end_matches('\n')));
                    }
                    "table" => {
                       let table_md = convert_html_table_to_markdown(element, html_input_for_error)?;
                       if !table_md.is_empty() {
                           markdown_blocks.push(table_md);
                       }
                       // If table_md is empty (e.g. no header), nothing is added.
                    }
                    "dl" => {
                       let mut dl_content_parts = Vec::new();
                       for child_node in &element.children {
                           match child_node {
                               HtmlNode::Element(dt_el) if dt_el.tag_name == "dt" => {
                                   let dt_text = convert_children_to_string(&dt_el.children, html_input_for_error)?;
                                   dl_content_parts.push(format!("**{}**", dt_text.trim()));
                               }
                               HtmlNode::Element(dd_el) if dd_el.tag_name == "dd" => {
                                   let dd_markdown_block = convert_nodes_to_markdown(&dd_el.children, html_input_for_error, extract_scripts_as_code_blocks)?;
                                   if !dd_markdown_block.is_empty() {
                                       let indented_dd_lines: Vec<String> = dd_markdown_block
                                           .lines()
                                           .map(|line| format!("  {}", line)) // Indent by 2 spaces
                                           .collect();
                                       dl_content_parts.push(indented_dd_lines.join("\n"));
                                   }
                               }
                               HtmlNode::Text(text) if text.trim().is_empty() => {}
                               HtmlNode::Comment(_) => {}
                               _ => { // Unexpected elements within dl
                                   let unexpected_block = convert_nodes_to_markdown(&[child_node.clone()], html_input_for_error, extract_scripts_as_code_blocks)?;
                                   if !unexpected_block.is_empty() {
                                       dl_content_parts.push(unexpected_block);
                                   }
                               }
                           }
                       }
                       if !dl_content_parts.is_empty() {
                           markdown_blocks.push(dl_content_parts.join("\n"));
                       }
                    }
                    "script" => {
                        if extract_scripts_as_code_blocks {
                            if element.attributes.get("src").and_then(|opt| opt.as_ref()).is_none() { // Inline script
                                let type_attr = element.attributes.get("type").and_then(|opt| opt.as_ref()).map(|s| s.to_lowercase());
                                let lang_specifier = match type_attr.as_deref() {
                                    Some("text/javascript") | Some("application/javascript") | Some("module") => "javascript".to_string(),
                                    Some("application/json") | Some("application/ld+json") => "json".to_string(),
                                    _ => "".to_string(),
                                };
                                let mut script_content = extract_text_from_pre_children(&element.children);
                                if script_content.starts_with('\n') { script_content.remove(0); }
                                let final_content = script_content.trim_end_matches('\n');
                                markdown_blocks.push(format!("```{}\n{}\n```", lang_specifier, final_content));
                            }
                        }
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
                    "iframe" | "video" | "audio" | "embed" | "object" => {
                        let tag_name = element.tag_name.as_str();
                        let mut src_url: Option<String> = None;
                        let mut additional_info = String::new();

                        // Extract src/data URL
                        match tag_name {
                            "iframe" | "embed" => {
                                src_url = element.attributes.get("src").and_then(|opt| opt.as_ref().cloned());
                            }
                            "video" | "audio" => {
                                src_url = element.attributes.get("src").and_then(|opt| opt.as_ref().cloned());
                                if src_url.is_none() {
                                    for child_node in &element.children {
                                        if let HtmlNode::Element(source_el) = child_node {
                                            if source_el.tag_name == "source" {
                                                if let Some(Some(s_src)) = source_el.attributes.get("src") {
                                                    src_url = Some(s_src.clone());
                                                    break;
                                                }
                                            }
                                        }
                                    }
                                }
                                if tag_name == "video" {
                                    if let Some(Some(poster_url)) = element.attributes.get("poster") {
                                        if !poster_url.is_empty() {
                                            additional_info = format!(" (Poster: {})", poster_url);
                                        }
                                    }
                                }
                            }
                            "object" => {
                                src_url = element.attributes.get("data").and_then(|opt| opt.as_ref().cloned());
                            }
                            _ => {} // Unreachable due to outer match
                        }

                        // Extract title attribute for Markdown link title
                        let title_val_opt = element.attributes.get("title").and_then(|opt| opt.as_ref());

                        // Determine Description Text (title attribute or default label)
                        let final_description_text = match title_val_opt {
                            Some(title_str) if !title_str.is_empty() => title_str.clone(),
                            _ => match tag_name {
                                "iframe" => "Embedded Iframe".to_string(),
                                "video" => "Video".to_string(),
                                "audio" => "Audio".to_string(),
                                "embed" => "Embedded Content".to_string(),
                                "object" => "Embedded Object".to_string(),
                                _ => "Embedded Resource".to_string(),
                            },
                        };

                        let title_md_part = title_val_opt
                            .filter(|t_str| !t_str.is_empty())
                            .map(|t_str| format!(" \"{}\"", t_str.replace('"', "\\\"")))
                            .unwrap_or_default();

                        if let Some(url) = src_url {
                            if !url.is_empty() {
                                markdown_blocks.push(format!("[{}]({}{}){}", final_description_text, url, title_md_part, additional_info));
                            }
                        }
                    }
                    "svg" => {
                        let mut title_text: Option<String> = None;
                        // Find the first <title> child element
                        for child_node in &element.children {
                            if let HtmlNode::Element(title_el) = child_node {
                                if title_el.tag_name == "title" {
                                    let extracted_title = convert_children_to_string(&title_el.children, html_input_for_error)?;
                                    let trimmed_title = extracted_title.trim();
                                    if !trimmed_title.is_empty() {
                                        title_text = Some(trimmed_title.to_string());
                                    }
                                    break;
                                }
                            }
                        }

                        if let Some(title) = title_text {
                            markdown_blocks.push(format!("[SVG: {}]", title));
                        } else {
                            markdown_blocks.push("[SVG Image]".to_string());
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
                        } else if element.tag_name.is_empty() { // Should not happen for valid element
                            // No specific handling for other empty known block tags like blockquote/pre if children_content_str is empty
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
