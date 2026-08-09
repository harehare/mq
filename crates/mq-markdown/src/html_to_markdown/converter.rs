use itertools::Itertools;
use miette::miette;
use rustc_hash::FxHashMap;

use super::iframe::detect_embed;
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

fn normalize_unicode_whitespace(text: &str) -> String {
    if text.chars().any(|c| matches!(c, '\u{00A0}' | '\u{202F}' | '\u{2009}')) {
        text.chars()
            .map(|c| match c {
                '\u{00A0}' | '\u{202F}' | '\u{2009}' => ' ',
                _ => c,
            })
            .collect()
    } else {
        text.to_owned()
    }
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

/// Escapes/sanitizes cell content for use inside a markdown table row.
///
/// Table rows are single lines, so a raw newline (from a `<br>`, which renders as
/// a "  \n" hard break in inline markdown) would otherwise split the row into
/// extra, malformed table rows. It's converted to a literal `<br>` tag instead,
/// which GFM table renderers support directly.
fn escape_table_cell_content(content: &str) -> String {
    content
        .replace("  \n", "<br>")
        .replace('\n', "<br>")
        .replace("|", "\\|")
}

/// Wraps raw text in a CommonMark code span, choosing a backtick fence longer than
/// any backtick run inside the content, and padding with a space when the content
/// starts or ends with a backtick, so the content survives verbatim.
fn wrap_code_span(raw: &str) -> String {
    if raw.is_empty() {
        return "``".to_string();
    }
    let max_backtick_run = raw.split(|c: char| c != '`').map(str::len).max().unwrap_or(0);
    let fence = "`".repeat(max_backtick_run + 1);
    if raw.starts_with('`') || raw.ends_with('`') {
        format!("{fence} {raw} {fence}")
    } else {
        format!("{fence}{raw}{fence}")
    }
}

/// Wraps `content` in a markdown delimiter (e.g. `**`, `*`, `~~`), moving any
/// leading/trailing whitespace outside the delimiters. CommonMark emphasis
/// requires the delimiter run to be immediately adjacent to non-whitespace
/// content, so `**  padded  **` is not valid emphasis and would round-trip as
/// literal asterisks; `  **padded**  ` is. Returns an empty string if `content`
/// is empty or entirely whitespace, so callers can skip pushing an empty wrap.
fn wrap_with_delimiter(content: &str, delimiter: &str) -> String {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let leading_ws = &content[..content.len() - content.trim_start().len()];
    let trailing_ws = &content[content.trim_end().len()..];
    format!("{leading_ws}{delimiter}{trimmed}{delimiter}{trailing_ws}")
}

/// Escapes markdown inline-syntax characters found in literal HTML text so they
/// survive as literal text instead of being reinterpreted as markdown syntax
/// (emphasis, code spans, links, raw HTML) when the generated markdown string is
/// re-parsed by a CommonMark parser.
fn escape_markdown_inline(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    for c in text.chars() {
        if matches!(c, '\\' | '`' | '*' | '_' | '[' | ']' | '<' | '~') {
            result.push('\\');
        }
        result.push(c);
    }
    result
}

/// Escapes markdown block-marker characters (`#`, `-`/`*`/`+`, ordered-list
/// numbers, `>`, thematic breaks) when they appear at the start of a line, so
/// literal text like "1. Not a list" or "# Not a heading" isn't reinterpreted
/// as block structure when the generated markdown is re-parsed.
fn escape_leading_block_markers(text: &str) -> String {
    text.lines()
        .map(escape_line_leading_marker)
        .collect::<Vec<_>>()
        .join("\n")
}

fn escape_line_leading_marker(line: &str) -> String {
    let trimmed_start = line.trim_start_matches(' ');
    let indent_len = line.len() - trimmed_start.len();
    if indent_len > 3 || trimmed_start.is_empty() {
        // 4+ leading spaces would already form an indented code block; leave as-is.
        return line.to_string();
    }
    let indent = &line[..indent_len];

    // ATX heading: 1-6 '#' followed by a space or end of line.
    let hashes = trimmed_start.chars().take_while(|&c| c == '#').count();
    if (1..=6).contains(&hashes) {
        let rest = &trimmed_start[hashes..];
        if rest.is_empty() || rest.starts_with(' ') {
            return format!("{}\\{}", indent, trimmed_start);
        }
    }

    // Bullet list marker: -, *, + followed by a space.
    let mut chars = trimmed_start.chars();
    if let Some(first) = chars.next()
        && matches!(first, '-' | '*' | '+')
        && chars.next() == Some(' ')
    {
        return format!("{}\\{}", indent, trimmed_start);
    }

    // Ordered list marker: digits followed by '.' or ')' then a space or end of line.
    let digit_count = trimmed_start.chars().take_while(|c| c.is_ascii_digit()).count();
    if digit_count > 0 {
        let after_digits = &trimmed_start[digit_count..];
        let mut rest_chars = after_digits.chars();
        if let Some(marker_char) = rest_chars.next()
            && matches!(marker_char, '.' | ')')
            && rest_chars.next().is_none_or(|c| c == ' ')
        {
            return format!(
                "{}{}\\{}{}",
                indent,
                &trimmed_start[..digit_count],
                marker_char,
                &after_digits[1..]
            );
        }
    }

    // Blockquote marker.
    if trimmed_start.starts_with('>') {
        return format!("{}\\{}", indent, trimmed_start);
    }

    // Thematic break: a line made up solely of 3+ of the same character among -, _, *.
    for marker in ['-', '_', '*'] {
        let stripped: String = trimmed_start.chars().filter(|&c| c != ' ').collect();
        if stripped.len() >= 3 && stripped.chars().all(|c| c == marker) {
            return format!("{}\\{}", indent, trimmed_start);
        }
    }

    line.to_string()
}

/// Parses a `colspan`/`rowspan` attribute, defaulting to 1 for missing/invalid/zero
/// values and clamping to a sane maximum so a pathological value (e.g. `colspan="99999999"`)
/// can't blow up table generation.
fn get_span_attr(element: &HtmlElement, attr: &str) -> usize {
    element
        .attributes
        .get(attr)
        .and_then(|opt| opt.as_ref())
        .and_then(|s| s.parse::<usize>().ok())
        .filter(|&n| n > 0)
        .unwrap_or(1)
        .min(64)
}

/// Extracts a header row's cells, honoring `colspan` by repeating the cell's content
/// (and alignment) across each spanned column. Without this, a `colspan` header
/// collapses to a single column while data rows below keep their real column count,
/// producing a table whose header and body widths don't match.
fn expand_header_row_cells(tr_element: &HtmlElement) -> miette::Result<(Vec<String>, Vec<Alignment>)> {
    let mut cells = Vec::new();
    let mut alignments = Vec::new();
    for cell_node in &tr_element.children {
        if let HtmlNode::Element(cell_element) = cell_node
            && (cell_element.tag_name == "th" || cell_element.tag_name == "td")
        {
            let cell_content = convert_children_to_string(&cell_element.children)?;
            let content = escape_table_cell_content(cell_content.trim());
            let alignment = get_cell_alignment(cell_element);
            let colspan = get_span_attr(cell_element, "colspan");
            for _ in 0..colspan {
                cells.push(content.clone());
                alignments.push(alignment);
            }
        }
    }
    Ok((cells, alignments))
}

/// Extracts one `<tr>`'s cells, honoring `colspan` (repeating content across spanned
/// columns) and `rowspan` (recording carry-over into `rowspan_carry`, keyed by column
/// index, so subsequent rows can pull in the repeated content at the right position).
fn expand_data_row_cells(
    tr_element: &HtmlElement,
    rowspan_carry: &mut FxHashMap<usize, (String, usize)>,
) -> miette::Result<Vec<String>> {
    let mut current_row_cells: Vec<String> = Vec::new();
    let mut real_cells = tr_element.children.iter().filter_map(|n| match n {
        HtmlNode::Element(el) if el.tag_name == "td" || el.tag_name == "th" => Some(el),
        _ => None,
    });
    let mut col = 0usize;
    loop {
        if let Some((content, rows_left)) = rowspan_carry.get_mut(&col) {
            current_row_cells.push(content.clone());
            *rows_left -= 1;
            if *rows_left == 0 {
                rowspan_carry.remove(&col);
            }
            col += 1;
            continue;
        }
        let Some(cell_element) = real_cells.next() else {
            break;
        };
        let cell_content = convert_children_to_string(&cell_element.children)?;
        let content = escape_table_cell_content(cell_content.trim());
        let colspan = get_span_attr(cell_element, "colspan");
        let rowspan = get_span_attr(cell_element, "rowspan");
        for i in 0..colspan {
            current_row_cells.push(content.clone());
            if rowspan > 1 {
                rowspan_carry.insert(col + i, (content.clone(), rowspan - 1));
            }
        }
        col += colspan;
    }
    Ok(current_row_cells)
}

fn convert_html_table_to_markdown(table_element: &HtmlElement) -> miette::Result<String> {
    let mut caption_text: Option<String> = None;
    for node in &table_element.children {
        if let HtmlNode::Element(el) = node
            && el.tag_name == "caption"
        {
            let text = convert_children_to_string(&el.children)?;
            let trimmed = text.trim().to_string();
            if !trimmed.is_empty() {
                caption_text = Some(trimmed);
            }
            break;
        }
    }

    let mut header_cells: Vec<String> = Vec::new();
    let mut header_alignments: Vec<Alignment> = Vec::new();
    let mut body_rows: Vec<Vec<String>> = Vec::new();
    let mut first_tbody_first_row_used_as_header = false;

    for node in &table_element.children {
        if let HtmlNode::Element(thead_element) = node
            && thead_element.tag_name == "thead"
            && let Some(HtmlNode::Element(tr_element)) = thead_element
                .children
                .iter()
                .find(|n| matches!(n, HtmlNode::Element(el) if el.tag_name == "tr"))
        {
            let (cells, alignments) = expand_header_row_cells(tr_element)?;
            header_cells = cells;
            header_alignments = alignments;

            break;
        }
    }

    if header_cells.is_empty() {
        for node in &table_element.children {
            if let HtmlNode::Element(tbody_element) = node {
                if tbody_element.tag_name == "tbody"
                    && let Some(HtmlNode::Element(tr_element)) = tbody_element
                        .children
                        .iter()
                        .find(|n| matches!(n, HtmlNode::Element(el) if el.tag_name == "tr"))
                {
                    let (cells, alignments) = expand_header_row_cells(tr_element)?;
                    header_cells = cells;
                    header_alignments = alignments;
                    if !header_cells.is_empty() {
                        first_tbody_first_row_used_as_header = true;
                    }
                }
                break;
            }
        }
    }

    if header_cells.is_empty() {
        return Ok("".to_string());
    }
    let column_count = header_cells.len();

    // Cells carried down from a `rowspan` in an earlier row: column index -> (content, rows left).
    let mut rowspan_carry: FxHashMap<usize, (String, usize)> = FxHashMap::default();
    let mut first_tbody_processed_for_data = false;
    for node in &table_element.children {
        if let HtmlNode::Element(tbody_element) = node
            && tbody_element.tag_name == "tbody"
        {
            let mut rows_to_iterate = tbody_element.children.iter();
            if first_tbody_first_row_used_as_header && !first_tbody_processed_for_data {
                rows_to_iterate.next();
                first_tbody_processed_for_data = true;
            }
            for tr_node in rows_to_iterate {
                if let HtmlNode::Element(tr_element) = tr_node
                    && tr_element.tag_name == "tr"
                {
                    body_rows.push(expand_data_row_cells(tr_element, &mut rowspan_carry)?);
                }
            }
        }
    }

    // `tfoot` rows have no separate representation in a markdown table; append them
    // as trailing body rows rather than dropping them.
    for node in &table_element.children {
        if let HtmlNode::Element(tfoot_element) = node
            && tfoot_element.tag_name == "tfoot"
        {
            for tr_node in &tfoot_element.children {
                if let HtmlNode::Element(tr_element) = tr_node
                    && tr_element.tag_name == "tr"
                {
                    body_rows.push(expand_data_row_cells(tr_element, &mut rowspan_carry)?);
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
    let table_md = markdown_table.trim_end_matches('\n').to_string();
    if let Some(caption) = caption_text {
        Ok(format!("{}\n\n{}", caption, table_md))
    } else {
        Ok(table_md)
    }
}

fn process_url_for_markdown(url: &str) -> String {
    let processed_url = url.replace(" ", "%20");
    let needs_angle_brackets =
        url.is_empty() || url.contains(' ') || processed_url.contains('(') || processed_url.contains(')');
    if needs_angle_brackets {
        format!("<{}>", processed_url)
    } else {
        processed_url
    }
}

fn handle_heading_element(element: &HtmlElement) -> miette::Result<String> {
    let children_content_str = convert_children_to_string(&element.children)?;
    let marker_level = element.tag_name[1..].parse().unwrap_or(1);
    Ok(format!("{} {}", "#".repeat(marker_level), children_content_str))
}

fn handle_paragraph_element(element: &HtmlElement) -> miette::Result<String> {
    let content = convert_children_to_string(&element.children)?;
    Ok(escape_leading_block_markers(&content))
}

fn handle_hr_element() -> miette::Result<String> {
    Ok("---".to_string())
}

fn handle_list_element(element: &HtmlElement, options: &ConversionOptions) -> miette::Result<String> {
    convert_html_list_to_markdown(element, 0, options)
}

fn handle_blockquote_element(element: &HtmlElement, options: &ConversionOptions) -> miette::Result<String> {
    let inner_markdown = convert_nodes_to_markdown(&element.children, options)?;
    if !inner_markdown.is_empty() {
        let quoted_lines: Vec<String> = inner_markdown.lines().map(|line| format!("> {}", line)).collect();
        Ok(quoted_lines.join("\n"))
    } else {
        Ok(">".to_string())
    }
}

/// Strips the common leading whitespace (dedent) from a multi-line string.
/// Lines that are entirely whitespace are ignored when computing the minimum indent.
fn dedent(text: &str) -> String {
    let min_indent = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start().len())
        .min()
        .unwrap_or(0);
    if min_indent == 0 {
        return text.to_owned();
    }
    text.lines()
        .map(|line| {
            if line.len() >= min_indent {
                &line[min_indent..]
            } else {
                line.trim_start()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn handle_pre_element(element: &HtmlElement, _options: &ConversionOptions) -> miette::Result<String> {
    let mut lang_specifier = String::new();
    // Find <code> among children, skipping leading whitespace-only text nodes.
    let code_child = element.children.iter().find_map(|n| {
        if let HtmlNode::Element(el) = n
            && el.tag_name == "code"
        {
            Some(el)
        } else {
            None
        }
    });
    let text_content = if let Some(code_element) = code_child {
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
        // Extract code content plus any sibling text nodes outside <code> (e.g. <pre><code>…</code>\nextra</pre>).
        let mut text = extract_text_from_pre_children(&code_element.children);
        let non_code: Vec<&HtmlNode> = element
            .children
            .iter()
            .filter(|n| !matches!(n, HtmlNode::Element(el) if el.tag_name == "code"))
            .collect();
        text.push_str(&extract_text_from_pre_children(
            non_code.iter().copied().cloned().collect::<Vec<_>>().as_slice(),
        ));
        text
    } else {
        extract_text_from_pre_children(&element.children)
    };

    let text_content = text_content.strip_prefix('\n').unwrap_or(&text_content);
    let text_content = dedent(text_content.trim_end_matches('\n'));
    Ok(format!("```{}\n{}\n```", lang_specifier, text_content))
}

/// GFM/CommonMark tables can't nest (a markdown table can't contain another table),
/// so a `<table>` with a `<table>` among its descendants can't be represented as a
/// markdown table without losing the inner table's structure entirely. Returns true
/// if `element` has a nested `<table>` anywhere below it.
fn contains_nested_table(element: &HtmlElement) -> bool {
    element.children.iter().any(|child| match child {
        HtmlNode::Element(el) => el.tag_name == "table" || contains_nested_table(el),
        _ => false,
    })
}

const VOID_HTML_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source", "track", "wbr",
];

fn escape_html_text(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn escape_html_attr_value(s: &str) -> String {
    escape_html_text(s).replace('"', "&quot;")
}

/// Serializes an [`HtmlElement`] tree back into an HTML string. Used as a fallback
/// for constructs markdown can't represent (e.g. nested tables), since CommonMark
/// passes raw HTML blocks through untouched.
fn html_element_to_html(element: &HtmlElement) -> String {
    let mut attr_keys: Vec<&String> = element.attributes.keys().collect();
    attr_keys.sort();
    let attrs_str = attr_keys
        .into_iter()
        .map(|key| match &element.attributes[key] {
            Some(value) => format!(" {}=\"{}\"", key, escape_html_attr_value(value)),
            None => format!(" {}", key),
        })
        .collect::<String>();

    if VOID_HTML_ELEMENTS.contains(&element.tag_name.as_str()) {
        return format!("<{}{}>", element.tag_name, attrs_str);
    }

    let inner = html_nodes_to_html(&element.children);
    format!("<{}{}>{}</{}>", element.tag_name, attrs_str, inner, element.tag_name)
}

fn html_nodes_to_html(nodes: &[HtmlNode]) -> String {
    nodes
        .iter()
        .map(|node| match node {
            HtmlNode::Text(text) => escape_html_text(text),
            HtmlNode::Element(el) => html_element_to_html(el),
            HtmlNode::Comment(text) => format!("<!--{}-->", text),
        })
        .collect::<String>()
}

fn handle_table_element(element: &HtmlElement, _options: &ConversionOptions) -> miette::Result<String> {
    if contains_nested_table(element) {
        return Ok(html_element_to_html(element));
    }
    convert_html_table_to_markdown(element)
}

fn handle_dl_element(element: &HtmlElement, options: &ConversionOptions) -> miette::Result<String> {
    let mut dl_content_parts = Vec::new();
    for child_node in &element.children {
        match child_node {
            HtmlNode::Element(dt_el) if dt_el.tag_name == "dt" => {
                let dt_text = convert_children_to_string(&dt_el.children)?;
                let dt_trimmed = dt_text.trim();
                // Avoid double-bold when <dt> already contains <strong>
                let dt_formatted = if dt_trimmed.starts_with("**") && dt_trimmed.ends_with("**") {
                    dt_trimmed.to_string()
                } else {
                    format!("**{}**", dt_trimmed)
                };
                dl_content_parts.push(dt_formatted);
            }
            HtmlNode::Element(dd_el) if dd_el.tag_name == "dd" => {
                let dd_markdown_block = convert_nodes_to_markdown(&dd_el.children, options)?;
                if !dd_markdown_block.is_empty() {
                    let indented_dd_lines: Vec<String> =
                        dd_markdown_block.lines().map(|line| format!("  {}", line)).collect();
                    dl_content_parts.push(indented_dd_lines.join("\n"));
                }
            }
            HtmlNode::Text(text) if text.trim().is_empty() => {}
            HtmlNode::Comment(_) => {}
            _ => {
                let unexpected_block = convert_nodes_to_markdown(std::slice::from_ref(child_node), options)?;
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

fn handle_script_element(element: &HtmlElement, options: &ConversionOptions) -> miette::Result<Option<String>> {
    if options.extract_scripts_as_code_blocks {
        if element.attributes.get("src").and_then(|opt| opt.as_ref()).is_none() {
            let type_attr = element
                .attributes
                .get("type")
                .and_then(|opt| opt.as_ref())
                .map(|s| s.to_lowercase());
            let lang_specifier = match type_attr.as_deref() {
                Some("text/javascript") | Some("application/javascript") | Some("module") => "javascript".to_string(),
                Some("application/json") | Some("application/ld+json") => "json".to_string(),
                _ => "".to_string(),
            };
            let mut script_content = extract_text_from_pre_children(&element.children);
            if script_content.starts_with('\n') {
                script_content.remove(0);
            }
            let final_content = script_content.trim_end_matches('\n');
            Ok(Some(format!("```{}\n{}\n```", lang_specifier, final_content)))
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
        "iframe" => {
            let src = element.attributes.get("src").and_then(|opt| opt.as_ref().cloned());
            if let Some(ref s) = src
                && let Some((description, canonical_url)) = detect_embed(s)
            {
                return Ok(Some(format!("[{}]({})", description, canonical_url)));
            }
            src_url = src;
        }
        "embed" => src_url = element.attributes.get("src").and_then(|opt| opt.as_ref().cloned()),
        "video" | "audio" => {
            src_url = element.attributes.get("src").and_then(|opt| opt.as_ref().cloned());
            if src_url.is_none() {
                for child_node in &element.children {
                    if let HtmlNode::Element(source_el) = child_node
                        && source_el.tag_name == "source"
                        && let Some(Some(s_src)) = source_el.attributes.get("src")
                    {
                        src_url = Some(s_src.clone());
                        break;
                    }
                }
            }
            if tag_name == "video"
                && let Some(Some(poster_url)) = element.attributes.get("poster")
                && !poster_url.is_empty()
            {
                additional_info = format!(" (Poster: {})", poster_url);
            }
        }
        "object" => src_url = element.attributes.get("data").and_then(|opt| opt.as_ref().cloned()),
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
        if let HtmlNode::Element(title_el) = child_node
            && title_el.tag_name == "title"
        {
            let extracted_title = convert_children_to_string(&title_el.children)?;
            let trimmed_title = extracted_title.trim();
            if !trimmed_title.is_empty() {
                title_text = Some(trimmed_title.to_string());
            }
            break;
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
    options: &ConversionOptions,
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
                        return Err(miette!("Unexpected list tag name: {}", list_element.tag_name,));
                    }
                };
                let li_content_markdown = convert_nodes_to_markdown(&li_element.children, options)?;
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
        } else if let HtmlNode::Text(text_content) = node
            && !text_content.trim().is_empty()
        {
        }
    }
    Ok(markdown_items.iter().filter(|item| !item.trim().is_empty()).join("\n"))
}

pub fn convert_children_to_string(nodes: &[HtmlNode]) -> miette::Result<String> {
    convert_children_to_string_impl(nodes, true)
}

/// Converts inline HTML nodes to a markdown string.
///
/// `escape_text` controls whether literal markdown-syntax characters in text nodes
/// are backslash-escaped. It is turned off while rendering the content of a `<code>`
/// element, since code span content is already verbatim and must not gain escape
/// backslashes (the code span fence itself protects the content from being
/// reinterpreted as markdown).
fn convert_children_to_string_impl(nodes: &[HtmlNode], escape_text: bool) -> miette::Result<String> {
    let mut parts = Vec::new();
    for node in nodes {
        match node {
            HtmlNode::Text(text) => {
                let escaped_owned;
                let text: &str = if escape_text {
                    escaped_owned = escape_markdown_inline(text);
                    &escaped_owned
                } else {
                    text
                };
                let normalized = normalize_unicode_whitespace(text);
                let trimmed = normalized.trim_start_matches('\n').trim_end_matches('\n');
                // Collapse internal newline+whitespace sequences (HTML whitespace collapsing).
                let collapsed = if trimmed.contains('\n') {
                    let leading_space = trimmed.starts_with(' ');
                    let trailing_space = trimmed.ends_with(' ');
                    let inner = trimmed
                        .split('\n')
                        .map(|s| s.trim())
                        .filter(|s| !s.is_empty())
                        .collect::<Vec<_>>()
                        .join(" ");
                    match (leading_space, trailing_space) {
                        (true, true) => format!(" {} ", inner),
                        (true, false) => format!(" {}", inner),
                        (false, true) => format!("{} ", inner),
                        (false, false) => inner,
                    }
                } else {
                    let trimmed = if trimmed.starts_with(' ') {
                        format!(" {}", trimmed.trim_start())
                    } else {
                        trimmed.to_owned()
                    };
                    if trimmed.ends_with(' ') {
                        format!("{} ", trimmed.trim_end())
                    } else {
                        trimmed
                    }
                };
                parts.push(collapsed);
            }
            HtmlNode::Element(element) => {
                let link_text = convert_children_to_string_impl(&element.children, escape_text)?;
                match element.tag_name.as_str() {
                    "strong" => {
                        let wrapped = wrap_with_delimiter(&link_text, "**");
                        if !wrapped.is_empty() {
                            parts.push(wrapped);
                        }
                    }
                    "em" => {
                        let wrapped = wrap_with_delimiter(&link_text, "*");
                        if !wrapped.is_empty() {
                            parts.push(wrapped);
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
                        let raw_text = convert_children_to_string_impl(&element.children, false)?;
                        parts.push(wrap_code_span(&raw_text));
                    }
                    "br" => parts.push("  \n".to_string()),
                    "img" => {
                        if let Some(Some(src_url)) = element.attributes.get("src")
                            && !src_url.is_empty()
                        {
                            let alt_text = element
                                .attributes
                                .get("alt")
                                .and_then(|opt_alt| opt_alt.as_ref())
                                .map(|s| escape_markdown_inline(s))
                                .unwrap_or_default();
                            let title_part = element
                                .attributes
                                .get("title")
                                .and_then(|opt_title| opt_title.as_ref())
                                .filter(|title_str| !title_str.is_empty())
                                .map(|title_str| format!(" \"{}\"", title_str.replace('"', "\\\"")))
                                .unwrap_or_default();
                            let processed_src = process_url_for_markdown(src_url);
                            parts.push(format!("![{}]({}{})", alt_text, processed_src, title_part));
                        }
                    }
                    "input" => {
                        // Skip inputs used as CSS toggle triggers (not actual form inputs)
                        let is_ui_toggle = element.attributes.get("role").and_then(|v| v.as_deref()) == Some("button")
                            || element.attributes.get("aria-haspopup").and_then(|v| v.as_deref()) == Some("true");
                        if is_ui_toggle {
                            // nothing
                        } else if let Some(Some(type_attr)) = element.attributes.get("type") {
                            match type_attr.to_lowercase().as_str() {
                                "checkbox" | "radio" => {
                                    if element.attributes.contains_key("checked") {
                                        parts.push("[x] ".to_string());
                                    } else {
                                        parts.push("[ ] ".to_string());
                                    }
                                }
                                "text" | "number" | "button" | "url" | "email"
                                    if element.attributes.contains_key("value") =>
                                {
                                    parts.push(element.attributes.get("value").cloned().unwrap().unwrap_or_default());
                                }
                                _ => {}
                            }
                        }
                    }
                    "s" | "strike" | "del" => {
                        let wrapped = wrap_with_delimiter(&link_text, "~~");
                        if !wrapped.is_empty() {
                            parts.push(wrapped);
                        }
                    }
                    "kbd" => parts.push(format!("<kbd>{}</kbd>", link_text)),
                    "u" => {
                        parts.push(format!("<u>{}</u>", link_text));
                    }
                    "sub" => parts.push(format!("<sub>{}</sub>", link_text)),
                    "sup" => parts.push(format!("<sup>{}</sup>", link_text)),
                    "q" => {
                        if !link_text.is_empty() {
                            parts.push(format!("\"{}\"", link_text));
                        }
                    }
                    "cite" => {
                        let wrapped = wrap_with_delimiter(&link_text, "*");
                        if !wrapped.is_empty() {
                            parts.push(wrapped);
                        }
                    }
                    "ins" => {
                        if !link_text.is_empty() {
                            parts.push(link_text);
                        }
                    }
                    "mark" => parts.push(format!("<mark>{}</mark>", link_text)),
                    "summary" => {
                        let wrapped = wrap_with_delimiter(&link_text, "**");
                        if !wrapped.is_empty() {
                            parts.push(wrapped);
                        }
                    }
                    "abbr" => {
                        if !link_text.is_empty() {
                            if let Some(Some(title)) = element.attributes.get("title")
                                && !title.is_empty()
                            {
                                parts.push(format!("{} ({})", link_text, title));
                            } else {
                                parts.push(link_text);
                            }
                        }
                    }
                    "picture" => parts.push(link_text),
                    "ruby" => {
                        let mut base = String::new();
                        let mut annotation = String::new();
                        for child in &element.children {
                            match child {
                                HtmlNode::Text(t) => base.push_str(&if escape_text {
                                    escape_markdown_inline(t)
                                } else {
                                    t.clone()
                                }),
                                HtmlNode::Element(el) if el.tag_name == "rt" => {
                                    annotation.push_str(&convert_children_to_string_impl(&el.children, escape_text)?);
                                }
                                HtmlNode::Element(el) if el.tag_name == "rp" => {}
                                HtmlNode::Element(el) => {
                                    base.push_str(&convert_children_to_string_impl(&el.children, escape_text)?);
                                }
                                HtmlNode::Comment(_) => {}
                            }
                        }
                        let base = base.trim();
                        let annotation = annotation.trim();
                        if !annotation.is_empty() {
                            parts.push(format!("{}({})", base, annotation));
                        } else if !base.is_empty() {
                            parts.push(base.to_string());
                        }
                    }
                    "dfn" => {
                        let wrapped = wrap_with_delimiter(&link_text, "*");
                        if !wrapped.is_empty() {
                            parts.push(wrapped);
                        }
                    }
                    "time" | "small" | "bdi" => {
                        if !link_text.is_empty() {
                            parts.push(link_text);
                        }
                    }
                    "span" => parts.push(link_text),
                    "nav" | "aside" | "noscript" => {} // skip
                    _ => parts.push(link_text),
                }
            }
            HtmlNode::Comment(_) => {}
        }
    }
    Ok(collapse_redundant_spaces(&parts.join("")))
}

/// Collapses runs of 2+ plain spaces into one, matching HTML's whitespace-collapse
/// rendering rules. This mainly cleans up doubled-up spaces that appear when an
/// inline element (e.g. `<strong> padded </strong>`) contributes its own boundary
/// space right next to a sibling text node that already ends/starts with one.
/// Runs immediately followed by a newline are left untouched since two trailing
/// spaces before `\n` are a meaningful CommonMark hard line break.
fn collapse_redundant_spaces(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut result = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == ' ' {
            let mut j = i;
            while j < chars.len() && chars[j] == ' ' {
                j += 1;
            }
            let run_len = j - i;
            if run_len >= 2 && chars.get(j) == Some(&'\n') {
                result.extend(std::iter::repeat_n(' ', run_len));
            } else {
                result.push(' ');
            }
            i = j;
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }
    result
}

/// Returns true if the element is a CSS-only UI widget (dropdown or tab switcher).
///
/// Two patterns are detected:
/// 1. A direct child `<input>` with `role="button"` or `aria-haspopup` — used for CSS dropdowns
///    (e.g., Wikipedia's language switcher).
/// 2. All direct children are `<input>` or `<label>` elements — used for CSS-only tabs/toggles
///    (e.g., tab bars built with radio inputs and labels).
fn is_css_dropdown_widget(element: &HtmlElement) -> bool {
    // Pattern 1: checkbox/radio used as dropdown toggle trigger
    let has_toggle_input = element.children.iter().any(|child| {
        matches!(child, HtmlNode::Element(el)
            if el.tag_name == "input"
            && (el.attributes.get("role").and_then(|v| v.as_deref()) == Some("button")
                || el.attributes.get("aria-haspopup").and_then(|v| v.as_deref()) == Some("true")))
    });
    if has_toggle_input {
        return true;
    }

    // Pattern 2: CSS tab/toggle widget built from alternating <input> and <label> siblings.
    // Both element types must be present; a lone <input> (e.g. standalone checkbox) is not a widget.
    let has_any_input = element
        .children
        .iter()
        .any(|child| matches!(child, HtmlNode::Element(el) if el.tag_name == "input"));
    let has_any_label = element
        .children
        .iter()
        .any(|child| matches!(child, HtmlNode::Element(el) if el.tag_name == "label"));
    if has_any_input && has_any_label {
        return element.children.iter().all(|child| match child {
            HtmlNode::Text(t) => t.trim().is_empty(),
            HtmlNode::Comment(_) => true,
            HtmlNode::Element(el) => matches!(el.tag_name.as_str(), "input" | "label"),
        });
    }

    false
}

/// Returns true if the element wraps a heading with only auxiliary UI siblings.
/// In this pattern (e.g., Wikipedia's `<div class="mw-heading">`), the heading is the
/// meaningful content and the siblings are UI chrome (edit links, toggle buttons, etc.).
fn is_heading_with_aux_siblings(element: &HtmlElement) -> bool {
    let has_heading = element.children.iter().any(|child| {
        matches!(child, HtmlNode::Element(el)
            if matches!(el.tag_name.as_str(), "h1" | "h2" | "h3" | "h4" | "h5" | "h6"))
    });
    if !has_heading {
        return false;
    }
    element.children.iter().all(|child| match child {
        HtmlNode::Text(t) => t.trim().is_empty(),
        HtmlNode::Comment(_) => true,
        HtmlNode::Element(el) => matches!(
            el.tag_name.as_str(),
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "span" | "input" | "label" | "button"
        ),
    })
}

pub fn convert_nodes_to_markdown(nodes: &[HtmlNode], options: &ConversionOptions) -> miette::Result<String> {
    let mut markdown_blocks: Vec<MarkdownBlock> = Vec::new();
    for node in nodes {
        match node {
            HtmlNode::Text(text) => {
                if !text.trim().is_empty() {
                    let escaped = escape_leading_block_markers(&escape_markdown_inline(text));
                    markdown_blocks.push((escaped, true));
                }
            }
            HtmlNode::Element(element) => {
                match element.tag_name.as_str() {
                    "nav" | "aside" | "noscript" => {
                        // Skip navigational/sidebar/noscript noise entirely
                    }
                    "html" | "head" | "header" | "footer" | "body" | "div" | "main" | "article" | "section"
                    | "hgroup" | "details" | "figure" => {
                        if is_css_dropdown_widget(element) {
                            // Skip CSS-only dropdown widgets (language switchers, nav menus, etc.)
                        } else if is_heading_with_aux_siblings(element) {
                            // Heading wrapper div: only extract the heading, drop UI chrome siblings
                            for child in &element.children {
                                if let HtmlNode::Element(el) = child
                                    && matches!(el.tag_name.as_str(), "h1" | "h2" | "h3" | "h4" | "h5" | "h6")
                                {
                                    markdown_blocks.push((handle_heading_element(el)?, false));
                                }
                            }
                        } else {
                            let markdown_block = convert_nodes_to_markdown(&element.children, options)?;
                            if !markdown_block.is_empty() {
                                markdown_blocks.push((markdown_block, false));
                            }
                        }
                    }
                    "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                        markdown_blocks.push((handle_heading_element(element)?, false))
                    }
                    "p" => markdown_blocks.push((handle_paragraph_element(element)?, false)),
                    "hr" => markdown_blocks.push((handle_hr_element()?, false)),
                    "ul" | "ol" => markdown_blocks.push((handle_list_element(element, options)?, false)),
                    "blockquote" => markdown_blocks.push((handle_blockquote_element(element, options)?, false)),
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
                    "summary" => {
                        let summary_text = convert_children_to_string(&element.children)?;
                        if !summary_text.is_empty() {
                            markdown_blocks.push((format!("**{}**", summary_text.trim()), false));
                        }
                    }
                    "figcaption" => {
                        let caption = handle_paragraph_element(element)?;
                        if !caption.is_empty() {
                            markdown_blocks.push((caption, false));
                        }
                    }
                    "address" => {
                        let content = convert_children_to_string(&element.children)?;
                        if !content.is_empty() {
                            markdown_blocks.push((format!("*{}*", content.trim()), false));
                        }
                    }
                    "script" => {
                        if let Some(script_md) = handle_script_element(element, options)? {
                            markdown_blocks.push((script_md, false));
                        }
                    }
                    "style" | "title" => { /* Metadata tags are ignored; title is handled at the top level */ }
                    "iframe" | "video" | "audio" | "embed" | "object" => {
                        if let Some(embed_md) = handle_embedded_content_element(element)? {
                            markdown_blocks.push((embed_md, false));
                        }
                    }
                    "svg" => markdown_blocks.push((handle_svg_element(element)?, false)),
                    "a" => {
                        // <a> without href is used as a transparent section container in some HTML.
                        // Treat it as a block pass-through in that case so block children are preserved.
                        if element.attributes.get("href").and_then(|v| v.as_deref()).is_some() {
                            let inline_md = convert_children_to_string(&[HtmlNode::Element(element.clone())])?;
                            if !inline_md.is_empty() {
                                markdown_blocks.push((inline_md.trim().to_string(), true));
                            }
                        } else {
                            let block_md = convert_nodes_to_markdown(&element.children, options)?;
                            if !block_md.is_empty() {
                                markdown_blocks.push((block_md, false));
                            }
                        }
                    }
                    "br" => {
                        // Hard line break — must not be trimmed or it becomes empty.
                        markdown_blocks.push(("  \n".to_string(), true));
                    }
                    "strong" | "em" | "code" | "span" | "img" | "input" | "s" | "strike" | "del" | "kbd" | "sub"
                    | "sup" | "q" | "cite" | "mark" | "abbr" | "picture" | "ruby" | "dfn" | "time" | "small"
                    | "bdi" => {
                        let inline_md = convert_children_to_string(&[HtmlNode::Element(element.clone())])?;
                        if !inline_md.is_empty() {
                            markdown_blocks.push((inline_md.trim().to_string(), true));
                        }
                    }
                    _ => {
                        // Unknown/custom elements (e.g. <astro-island>, <button>, web components):
                        // recurse as blocks so any block-level children are preserved correctly.
                        let block_md = convert_nodes_to_markdown(&element.children, options)?;
                        if !block_md.is_empty() {
                            markdown_blocks.push((block_md, false));
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
        let md = convert_nodes_to_markdown(&nodes, &ConversionOptions::default()).unwrap();
        let md_trimmed = md.trim();
        assert_eq!(md_trimmed, expected);
    }

    #[rstest]
    #[case(
        vec![element_node("nav", vec![element_node("a", vec![text_node("Home")])])],
        ""
    )]
    #[case(
        vec![element_node("aside", vec![text_node("Related")])],
        ""
    )]
    #[case(
        vec![element_node("noscript", vec![text_node("Enable JavaScript")])],
        ""
    )]
    fn test_noisy_elements_are_skipped(#[case] nodes: Vec<HtmlNode>, #[case] expected: &str) {
        let md = convert_nodes_to_markdown(&nodes, &ConversionOptions::default()).unwrap();
        assert_eq!(md.trim(), expected);
    }
}
