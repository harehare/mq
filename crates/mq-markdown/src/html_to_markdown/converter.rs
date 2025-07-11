use itertools::Itertools;
use miette::miette;

use super::node::HtmlElement;
use super::node::HtmlNode;
use super::options::ConversionOptions;

type MarkdownInline = bool;
type MarkdownBlock = (String, MarkdownInline);

fn extract_text_from_pre_children(nodes: &[HtmlNode]) -> String {
    let mut text_content = String::new();
    for node in nodes {
        match node {
            HtmlNode::Text(text) => text_content.push_str(text),
            HtmlNode::Element(el) if el.tag_name == "br" => text_content.push('\n'),
            HtmlNode::Element(el) if el.tag_name == "code" => {
                text_content.push_str(&extract_text_from_pre_children(&el.children));
            }
            HtmlNode::Element(el) => {
                text_content.push_str(&extract_text_from_pre_children(&el.children));
            }
            HtmlNode::Comment(_) => {}
        }
    }
    text_content
}

#[derive(PartialEq, Debug, Clone, Copy)]
enum Alignment {
    Left,
    Center,
    Right,
    Default,
}

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

fn escape_table_cell_content(content: &str) -> String {
    content.replace("|", "\\|")
}

fn convert_html_table_to_markdown(table_element: &HtmlElement) -> miette::Result<String> {
    let mut header_cells: Vec<String> = Vec::new();
    let mut header_alignments: Vec<Alignment> = Vec::new();
    let mut body_rows: Vec<Vec<String>> = Vec::new();
    let mut first_tbody_first_row_used_as_header = false;

    for node in &table_element.children {
        if let HtmlNode::Element(thead_element) = node {
            if thead_element.tag_name == "thead" {
                if let Some(HtmlNode::Element(tr_element)) = thead_element
                    .children
                    .iter()
                    .find(|n| matches!(n, HtmlNode::Element(el) if el.tag_name == "tr"))
                {
                    for cell_node in &tr_element.children {
                        if let HtmlNode::Element(cell_element) = cell_node {
                            if cell_element.tag_name == "th" || cell_element.tag_name == "td" {
                                let cell_content =
                                    convert_children_to_string(&cell_element.children)?;
                                header_cells.push(escape_table_cell_content(cell_content.trim()));
                                header_alignments.push(get_cell_alignment(cell_element));
                            }
                        }
                    }
                }
                break;
            }
        }
    }

    if header_cells.is_empty() {
        for node in &table_element.children {
            if let HtmlNode::Element(tbody_element) = node {
                if tbody_element.tag_name == "tbody" {
                    if let Some(HtmlNode::Element(tr_element)) = tbody_element
                        .children
                        .iter()
                        .find(|n| matches!(n, HtmlNode::Element(el) if el.tag_name == "tr"))
                    {
                        for cell_node in &tr_element.children {
                            if let HtmlNode::Element(cell_element) = cell_node {
                                if cell_element.tag_name == "td" || cell_element.tag_name == "th" {
                                    let cell_content =
                                        convert_children_to_string(&cell_element.children)?;
                                    header_cells
                                        .push(escape_table_cell_content(cell_content.trim()));
                                    header_alignments.push(get_cell_alignment(cell_element));
                                }
                            }
                        }
                        if !header_cells.is_empty() {
                            first_tbody_first_row_used_as_header = true;
                        }
                    }
                    break;
                }
            }
        }
    }

    if header_cells.is_empty() {
        return Ok("".to_string());
    }
    let column_count = header_cells.len();

    let mut first_tbody_processed_for_data = false;
    for node in &table_element.children {
        if let HtmlNode::Element(tbody_element) = node {
            if tbody_element.tag_name == "tbody" {
                let mut rows_to_iterate = tbody_element.children.iter();
                if first_tbody_first_row_used_as_header && !first_tbody_processed_for_data {
                    rows_to_iterate.next();
                    first_tbody_processed_for_data = true;
                }
                for tr_node in rows_to_iterate {
                    if let HtmlNode::Element(tr_element) = tr_node {
                        if tr_element.tag_name == "tr" {
                            let mut current_row_cells: Vec<String> = Vec::new();
                            for td_node in &tr_element.children {
                                if let HtmlNode::Element(td_element) = td_node {
                                    if td_element.tag_name == "td" || td_element.tag_name == "th" {
                                        let cell_content =
                                            convert_children_to_string(&td_element.children)?;
                                        current_row_cells
                                            .push(escape_table_cell_content(cell_content.trim()));
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

    let mut markdown_table = String::new();
    markdown_table.push_str("| ");
    markdown_table.push_str(&header_cells.join(" | "));
    markdown_table.push_str(" |\n");

    markdown_table.push('|');
    for i in 0..column_count {
        let align = header_alignments.get(i).unwrap_or(&Alignment::Default);
        let sep_str = match align {
            Alignment::Left => ":---",
            Alignment::Center => ":---:",
            Alignment::Right => "---:",
            Alignment::Default => "---",
        };
        markdown_table.push_str(sep_str);
        markdown_table.push('|');
    }
    markdown_table.push('\n');

    for row_cells in &body_rows {
        markdown_table.push_str("| ");
        for cell_idx in 0..column_count {
            if let Some(cell_content) = row_cells.get(cell_idx) {
                markdown_table.push_str(cell_content);
            }
            markdown_table.push_str(" | ");
        }
        if column_count > 0 {
            markdown_table.truncate(markdown_table.len() - 3);
        }
        markdown_table.push_str(" |\n");
    }
    Ok(markdown_table.trim_end_matches('\n').to_string())
}

fn process_url_for_markdown(url: &str) -> String {
    let processed_url = url.replace(" ", "%20");
    let needs_angle_brackets = url.is_empty()
        || url.contains(' ')
        || processed_url.contains('(')
        || processed_url.contains(')');
    if needs_angle_brackets {
        format!("<{}>", processed_url)
    } else {
        processed_url
    }
}

fn handle_heading_element(element: &HtmlElement) -> miette::Result<String> {
    let children_content_str = convert_children_to_string(&element.children)?;
    let marker_level = element.tag_name[1..].parse().unwrap_or(1);
    Ok(format!(
        "{} {}",
        "#".repeat(marker_level),
        children_content_str
    ))
}

fn handle_paragraph_element(element: &HtmlElement) -> miette::Result<String> {
    convert_children_to_string(&element.children)
}

fn handle_hr_element() -> miette::Result<String> {
    Ok("---".to_string())
}

fn handle_list_element(
    element: &HtmlElement,
    options: ConversionOptions,
) -> miette::Result<String> {
    convert_html_list_to_markdown(element, 0, options)
}

fn handle_blockquote_element(
    element: &HtmlElement,
    options: ConversionOptions,
) -> miette::Result<String> {
    let inner_markdown = convert_nodes_to_markdown(&element.children, options)?;
    if !inner_markdown.is_empty() {
        let quoted_lines: Vec<String> = inner_markdown
            .lines()
            .map(|line| format!("> {}", line))
            .collect();
        Ok(quoted_lines.join("\n"))
    } else {
        Ok(">".to_string())
    }
}

fn handle_pre_element(
    element: &HtmlElement,
    _options: ConversionOptions,
) -> miette::Result<String> {
    let mut lang_specifier = String::new();
    let mut content_nodes = &element.children;
    if let Some(HtmlNode::Element(code_element)) = element.children.first() {
        if code_element.tag_name == "code" {
            content_nodes = &code_element.children;
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
    if text_content.starts_with('\n') {
        text_content.remove(0);
    }
    Ok(format!(
        "```{}\n{}\n```",
        lang_specifier,
        text_content.trim_end_matches('\n')
    ))
}

fn handle_table_element(
    element: &HtmlElement,
    _options: ConversionOptions,
) -> miette::Result<String> {
    convert_html_table_to_markdown(element)
}

fn handle_dl_element(element: &HtmlElement, options: ConversionOptions) -> miette::Result<String> {
    let mut dl_content_parts = Vec::new();
    for child_node in &element.children {
        match child_node {
            HtmlNode::Element(dt_el) if dt_el.tag_name == "dt" => {
                let dt_text = convert_children_to_string(&dt_el.children)?;
                dl_content_parts.push(format!("**{}**", dt_text.trim()));
            }
            HtmlNode::Element(dd_el) if dd_el.tag_name == "dd" => {
                let dd_markdown_block = convert_nodes_to_markdown(&dd_el.children, options)?;
                if !dd_markdown_block.is_empty() {
                    let indented_dd_lines: Vec<String> = dd_markdown_block
                        .lines()
                        .map(|line| format!("  {}", line))
                        .collect();
                    dl_content_parts.push(indented_dd_lines.join("\n"));
                }
            }
            HtmlNode::Text(text) if text.trim().is_empty() => {}
            HtmlNode::Comment(_) => {}
            _ => {
                let unexpected_block = convert_nodes_to_markdown(&[child_node.clone()], options)?;
                if !unexpected_block.is_empty() {
                    dl_content_parts.push(unexpected_block);
                }
            }
        }
    }
    if !dl_content_parts.is_empty() {
        Ok(dl_content_parts.join("\n"))
    } else {
        Ok("".to_string())
    }
}

fn handle_script_element(
    element: &HtmlElement,
    options: ConversionOptions,
) -> miette::Result<Option<String>> {
    if options.extract_scripts_as_code_blocks {
        if element
            .attributes
            .get("src")
            .and_then(|opt| opt.as_ref())
            .is_none()
        {
            let type_attr = element
                .attributes
                .get("type")
                .and_then(|opt| opt.as_ref())
                .map(|s| s.to_lowercase());
            let lang_specifier = match type_attr.as_deref() {
                Some("text/javascript") | Some("application/javascript") | Some("module") => {
                    "javascript".to_string()
                }
                Some("application/json") | Some("application/ld+json") => "json".to_string(),
                _ => "".to_string(),
            };
            let mut script_content = extract_text_from_pre_children(&element.children);
            if script_content.starts_with('\n') {
                script_content.remove(0);
            }
            let final_content = script_content.trim_end_matches('\n');
            Ok(Some(format!(
                "```{}\n{}\n```",
                lang_specifier, final_content
            )))
        } else {
            Ok(None)
        }
    } else {
        Ok(None)
    }
}

fn handle_embedded_content_element(element: &HtmlElement) -> miette::Result<Option<String>> {
    let tag_name = element.tag_name.as_str();
    let mut src_url: Option<String> = None;
    let mut additional_info = String::new();
    match tag_name {
        "iframe" | "embed" => {
            src_url = element
                .attributes
                .get("src")
                .and_then(|opt| opt.as_ref().cloned())
        }
        "video" | "audio" => {
            src_url = element
                .attributes
                .get("src")
                .and_then(|opt| opt.as_ref().cloned());
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
            src_url = element
                .attributes
                .get("data")
                .and_then(|opt| opt.as_ref().cloned())
        }
        _ => {}
    }
    if let Some(url) = src_url {
        if !url.is_empty() {
            let title_val_opt = element.attributes.get("title").and_then(|opt| opt.as_ref());
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
            Ok(Some(format!(
                "[{}]({}{}){}",
                final_description_text, url, title_md_part, additional_info
            )))
        } else {
            Ok(None)
        }
    } else {
        Ok(None)
    }
}

fn handle_svg_element(element: &HtmlElement) -> miette::Result<String> {
    let mut title_text: Option<String> = None;
    for child_node in &element.children {
        if let HtmlNode::Element(title_el) = child_node {
            if title_el.tag_name == "title" {
                let extracted_title = convert_children_to_string(&title_el.children)?;
                let trimmed_title = extracted_title.trim();
                if !trimmed_title.is_empty() {
                    title_text = Some(trimmed_title.to_string());
                }
                break;
            }
        }
    }
    if let Some(title) = title_text {
        Ok(format!("[SVG: {}]", title))
    } else {
        Ok("[SVG Image]".to_string())
    }
}

fn convert_html_list_to_markdown(
    list_element: &HtmlElement,
    indent_level: usize,
    options: ConversionOptions,
) -> miette::Result<String> {
    let mut markdown_items = Vec::new();
    let base_indent = "    ".repeat(indent_level);
    let mut current_list_number = if list_element.tag_name == "ol" {
        list_element
            .attributes
            .get("start")
            .and_then(|opt_val| opt_val.as_ref())
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(1)
    } else {
        0
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
                    _ => {
                        return Err(miette!(
                            "Unexpected list tag name: {}",
                            list_element.tag_name,
                        ));
                    }
                };
                let li_content_markdown = convert_nodes_to_markdown(&li_element.children, options)?;
                if li_content_markdown.is_empty() {
                    markdown_items.push(format!("{}{}", base_indent, marker_prefix));
                } else {
                    let mut first_line_in_li = true;
                    for line in li_content_markdown.lines() {
                        if first_line_in_li {
                            markdown_items
                                .push(format!("{}{}{}", base_indent, marker_prefix, line));
                            first_line_in_li = false;
                        } else {
                            let continuation_indent = " ".repeat(marker_prefix.len());
                            markdown_items
                                .push(format!("{}{}{}", base_indent, continuation_indent, line));
                        }
                    }
                }
            }
        } else if let HtmlNode::Text(text_content) = node {
            if !text_content.trim().is_empty() {}
        }
    }
    Ok(markdown_items
        .iter()
        .filter(|item| !item.trim().is_empty())
        .join("\n"))
}

pub fn convert_children_to_string(nodes: &[HtmlNode]) -> miette::Result<String> {
    let mut parts = Vec::new();
    for node in nodes {
        match node {
            HtmlNode::Text(text) => {
                let trimmed = text.trim_start_matches('\n').trim_end_matches('\n');
                let trimmed = if trimmed.starts_with(" ") {
                    format!(" {}", trimmed.trim_start())
                } else {
                    trimmed.to_owned()
                };
                let trimmed = if trimmed.ends_with(" ") {
                    format!("{} ", trimmed.trim_end())
                } else {
                    trimmed.to_owned()
                };

                parts.push(trimmed.to_string());
            }
            HtmlNode::Element(element) => {
                let link_text = convert_children_to_string(&element.children)?;
                match element.tag_name.as_str() {
                    "strong" => {
                        if !link_text.is_empty() {
                            parts.push(format!("**{}**", link_text));
                        }
                    }
                    "em" => {
                        if !link_text.is_empty() {
                            parts.push(format!("*{}*", link_text));
                        }
                    }
                    "a" => {
                        if let Some(Some(href)) = element.attributes.get("href") {
                            let title_part = element
                                .attributes
                                .get("title")
                                .and_then(|opt_title| opt_title.as_ref())
                                .filter(|title_str| !title_str.is_empty())
                                .map(|title_str| format!(" \"{}\"", title_str.replace('"', "\\\"")))
                                .unwrap_or_default();
                            let processed_href = process_url_for_markdown(href);
                            parts.push(format!(
                                "[{}]({}{})",
                                link_text.replace("\n", "").trim(),
                                processed_href,
                                title_part
                            ));
                        } else if !link_text.is_empty() {
                            parts.push(link_text);
                        }
                    }
                    "code" => {
                        if !link_text.is_empty() {
                            parts.push(format!("`{}`", link_text));
                        } else {
                            parts.push("``".to_string());
                        }
                    }
                    "br" => parts.push("  \n".to_string()),
                    "img" => {
                        if let Some(Some(src_url)) = element.attributes.get("src") {
                            if !src_url.is_empty() {
                                let alt_text = element
                                    .attributes
                                    .get("alt")
                                    .and_then(|opt_alt| opt_alt.as_ref())
                                    .map(|s| s.as_str())
                                    .unwrap_or("");
                                let title_part = element
                                    .attributes
                                    .get("title")
                                    .and_then(|opt_title| opt_title.as_ref())
                                    .filter(|title_str| !title_str.is_empty())
                                    .map(|title_str| {
                                        format!(" \"{}\"", title_str.replace('"', "\\\""))
                                    })
                                    .unwrap_or_default();
                                let processed_src = process_url_for_markdown(src_url);
                                parts.push(format!(
                                    "![{}]({}{})",
                                    alt_text, processed_src, title_part
                                ));
                            }
                        }
                    }
                    "input" => {
                        if let Some(Some(type_attr)) = element.attributes.get("type") {
                            match type_attr.to_lowercase().as_str() {
                                "checkbox" | "radio" => {
                                    if element.attributes.contains_key("checked") {
                                        parts.push("[x] ".to_string());
                                    } else {
                                        parts.push("[ ] ".to_string());
                                    }
                                }
                                "text" | "number" | "button" | "url" | "email" => {
                                    if element.attributes.contains_key("value") {
                                        parts.push(
                                            element
                                                .attributes
                                                .get("value")
                                                .cloned()
                                                .unwrap()
                                                .unwrap_or_default(),
                                        );
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    "s" | "strike" | "del" => {
                        if !link_text.is_empty() {
                            parts.push(format!("~~{}~~", link_text));
                        }
                    }
                    "kbd" => parts.push(format!("<kbd>{}</kbd>", link_text)),
                    "u" => {
                        parts.push(format!("<u>{}</u>", link_text));
                    }
                    "span" => parts.push(link_text),
                    _ => parts.push(link_text),
                }
            }
            HtmlNode::Comment(_) => {}
        }
    }
    Ok(parts.join("").to_string())
}

pub fn convert_nodes_to_markdown(
    nodes: &[HtmlNode],
    options: ConversionOptions,
) -> miette::Result<String> {
    let mut markdown_blocks: Vec<MarkdownBlock> = Vec::new();
    for node in nodes {
        match node {
            HtmlNode::Text(text) => {
                if !text.trim().is_empty() {
                    markdown_blocks.push((text.to_string(), true));
                }
            }
            HtmlNode::Element(element) => {
                match element.tag_name.as_str() {
                    "html" | "head" | "header" | "footer" | "body" | "div" | "nav" | "main"
                    | "article" | "section" | "hgroup" => {
                        let markdown_block = convert_nodes_to_markdown(&element.children, options)?;

                        if !markdown_block.is_empty() {
                            markdown_blocks.push((markdown_block, false));
                        }
                    }
                    "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                        markdown_blocks.push((handle_heading_element(element)?, false))
                    }
                    "p" => markdown_blocks.push((handle_paragraph_element(element)?, false)),
                    "hr" => markdown_blocks.push((handle_hr_element()?, false)),
                    "ul" | "ol" => {
                        markdown_blocks.push((handle_list_element(element, options)?, false))
                    }
                    "title" if options.use_title_as_h1 => {
                        let children_content_str = convert_children_to_string(&element.children)?;
                        if !children_content_str.is_empty() {
                            markdown_blocks.push((format!("# {}", children_content_str), false));
                        }
                    }
                    "blockquote" => {
                        markdown_blocks.push((handle_blockquote_element(element, options)?, false))
                    }
                    "pre" => markdown_blocks.push((handle_pre_element(element, options)?, false)),
                    "table" => {
                        let table_md = handle_table_element(element, options)?;
                        if !table_md.is_empty() {
                            markdown_blocks.push((table_md, false));
                        }
                    }
                    "dl" => {
                        let dl_md = handle_dl_element(element, options)?;
                        if !dl_md.is_empty() {
                            markdown_blocks.push((dl_md, false));
                        }
                    }
                    "script" => {
                        if let Some(script_md) = handle_script_element(element, options)? {
                            markdown_blocks.push((script_md, false));
                        }
                    }
                    "style" => { /* Style tags are ignored */ }
                    "iframe" | "video" | "audio" | "embed" | "object" => {
                        if let Some(embed_md) = handle_embedded_content_element(element)? {
                            markdown_blocks.push((embed_md, false));
                        }
                    }
                    "svg" => markdown_blocks.push((handle_svg_element(element)?, false)),
                    "strong" | "em" | "a" | "code" | "span" | "img" | "br" | "input" | "s"
                    | "strike" | "del" | "kbd" => {
                        let inline_md =
                            convert_children_to_string(&[HtmlNode::Element(element.clone())])?;
                        if !inline_md.is_empty() {
                            markdown_blocks.push((inline_md.trim().to_string(), true));
                        }
                    }
                    _ => {
                        let children_content_str = convert_children_to_string(&element.children)?;
                        if !children_content_str.is_empty() {
                            markdown_blocks.push((children_content_str, false));
                        }
                    }
                }
            }
            HtmlNode::Comment(_) => {}
        }
    }

    let mut result = String::new();

    for (i, (block_content, is_inline)) in markdown_blocks.iter().enumerate() {
        if !is_inline
            && i > 0
            && !block_content.is_empty()
            && !result.ends_with("\n\n")
            && !result.ends_with("```\n")
            && !result.ends_with(">\n")
            && !result.ends_with("  \n")
        {
            if !(result.ends_with('\n') && block_content.starts_with('\n')) {
                // Avoid \n\n\n if prev ends with \n and current starts with \n
                result.push_str("\n\n");
            } else if !result.ends_with('\n') {
                result.push_str("\n\n");
            }
        }

        result.push_str(if *is_inline {
            block_content
        } else {
            block_content.trim_start()
        });
    }

    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;
    use rustc_hash::FxHashMap;
    fn text_node(text: &str) -> HtmlNode {
        HtmlNode::Text(text.to_string())
    }

    fn element_node(tag: &str, children: Vec<HtmlNode>) -> HtmlNode {
        HtmlNode::Element(HtmlElement {
            tag_name: tag.to_string(),
            attributes: FxHashMap::default(),
            children,
        })
    }

    #[rstest]
    #[case(
        vec![element_node("p", vec![text_node("Hello, world!")])],
        "Hello, world!"
    )]
    #[case(
        vec![element_node("h2", vec![text_node("Title")])],
        "## Title"
    )]
    #[case(
        vec![element_node(
            "p",
            vec![
                element_node("strong", vec![text_node("Bold")]),
                text_node(" and "),
                element_node("em", vec![text_node("Italic")]),
            ],
        )],
        "**Bold** and *Italic*"
    )]
    #[case(
        {
            let mut node = element_node("a", vec![text_node("link")]);
            if let HtmlNode::Element(ref mut el) = node {
                el.attributes.insert("href".to_string(), Some("https://example.com".to_string()));
            }
            vec![node]
        },
        "[link](https://example.com)"
    )]
    #[case(
        vec![element_node(
            "ul",
            vec![
                element_node("li", vec![text_node("Item 1")]),
                element_node("li", vec![text_node("Item 2")]),
            ],
        )],
        "* Item 1\n* Item 2"
    )]
    #[case(
        vec![element_node(
            "ol",
            vec![
                element_node("li", vec![text_node("First")]),
                element_node("li", vec![text_node("Second")]),
            ],
        )],
        "1. First\n2. Second"
    )]
    #[case(
        vec![element_node(
            "pre",
            vec![element_node("code", vec![text_node("let x = 1;")])],
        )],
        "```\nlet x = 1;\n```"
    )]
    #[case(
        {
            let th = element_node("th", vec![text_node("Header")]);
            let td = element_node("td", vec![text_node("Cell")]);
            let tr_head = element_node("tr", vec![th]);
            let tr_body = element_node("tr", vec![td]);
            let thead = element_node("thead", vec![tr_head]);
            let tbody = element_node("tbody", vec![tr_body]);
            let table = HtmlNode::Element(HtmlElement {
                tag_name: "table".to_string(),
                attributes: FxHashMap::default(),
                children: vec![thead, tbody],
            });
            vec![table]
        },
        "| Header |\n|---|\n| Cell |"
    )]
    #[case(
        vec![element_node(
            "blockquote",
            vec![element_node("p", vec![text_node("Quote")])],
        )],
        "> Quote"
    )]
    #[case(
        {
            let mut attrs = FxHashMap::default();
            attrs.insert("src".to_string(), Some("img.png".to_string()));
            attrs.insert("alt".to_string(), Some("alt text".to_string()));
            let img = HtmlNode::Element(HtmlElement {
                tag_name: "img".to_string(),
                attributes: attrs,
                children: vec![],
            });
            vec![img]
        },
        "![alt text](img.png)"
    )]
    fn test_convert_nodes_to_markdown_param(#[case] nodes: Vec<HtmlNode>, #[case] expected: &str) {
        let md = convert_nodes_to_markdown(&nodes, ConversionOptions::default()).unwrap();
        let md_trimmed = md.trim();
        assert_eq!(md_trimmed, expected);
    }
}
