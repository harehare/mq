use crate::highlighter::SyntaxHighlighter;
use colored::*;
use mq_markdown::{Markdown, Node};
use std::io::{self, Write};
use std::path::Path;

/// Unicode header symbols (①②③④⑤⑥)
const HEADER_SYMBOLS: &[&str] = &["①", "②", "③", "④", "⑤", "⑥"];

/// Unicode bullet symbols for lists
const LIST_BULLETS: &[&str] = &["●", "○", "◆", "◇"];

/// GitHub-style callout definitions
#[derive(Debug, Clone)]
struct Callout {
    icon: &'static str,
    color: colored::Color,
    name: &'static str,
}

const CALLOUTS: &[(&str, Callout)] = &[
    (
        "NOTE",
        Callout {
            icon: "ℹ️",
            color: colored::Color::Blue,
            name: "Note",
        },
    ),
    (
        "TIP",
        Callout {
            icon: "💡",
            color: colored::Color::Green,
            name: "Tip",
        },
    ),
    (
        "IMPORTANT",
        Callout {
            icon: "❗",
            color: colored::Color::Magenta,
            name: "Important",
        },
    ),
    (
        "WARNING",
        Callout {
            icon: "⚠️",
            color: colored::Color::Yellow,
            name: "Warning",
        },
    ),
    (
        "CAUTION",
        Callout {
            icon: "🔥",
            color: colored::Color::Red,
            name: "Caution",
        },
    ),
];

/// Create a clickable link using ANSI escape sequences (OSC 8)
/// Format: ESC ] 8 ; params ; URI ST display_text ESC ] 8 ; ; ST
fn make_clickable_link(url: &str, display_text: &str) -> String {
    // Using ST (String Terminator) \x1b\\ instead of BEL \x07 for better compatibility
    format!("\x1b]8;;{}\x1b\\{}\x1b]8;;\x1b\\", url, display_text)
}

/// Render a Markdown document to a writer with syntax highlighting and rich text formatting.
///
/// # Examples
///
/// ```rust
/// use mq_viewer::render_markdown;
/// use mq_markdown::Markdown;
/// use std::io::BufWriter;
///
/// let markdown: Markdown = "# Hello\n\nWorld".parse().unwrap();
/// let mut output = Vec::new();
/// {
///     let mut writer = BufWriter::new(&mut output);
///     render_markdown(&markdown, &mut writer).unwrap();
/// }
/// ```
pub fn render_markdown<W: Write>(markdown: &Markdown, writer: &mut W) -> io::Result<()> {
    let mut highlighter = SyntaxHighlighter::new();
    let mut i = 0;
    while i < markdown.nodes.len() {
        let node = &markdown.nodes[i];
        // Check if this is the start of a table (TableCell nodes)
        if matches!(node, Node::TableCell(_)) {
            // Collect all table nodes (cells + header)
            let mut table_nodes = Vec::new();
            let mut j = i;
            while j < markdown.nodes.len() {
                match &markdown.nodes[j] {
                    Node::TableCell(_) | Node::TableHeader(_) | Node::TableRow(_) => {
                        table_nodes.push(&markdown.nodes[j]);
                        j += 1;
                    }
                    _ => break,
                }
            }
            // Render the complete table
            render_table(&table_nodes, &mut highlighter, writer)?;
            i = j;
        } else {
            render_node(node, 0, &mut highlighter, writer)?;
            i += 1;
        }
    }
    Ok(())
}

/// Render a Markdown document to a String with syntax highlighting and rich text formatting.
///
/// # Examples
///
/// ```rust
/// use mq_viewer::render_markdown_to_string;
/// use mq_markdown::Markdown;
///
/// let markdown: Markdown = "# Hello\n\nWorld".parse().unwrap();
/// let rendered = render_markdown_to_string(&markdown).unwrap();
/// println!("{}", rendered);
/// ```
pub fn render_markdown_to_string(markdown: &Markdown) -> io::Result<String> {
    let mut output = Vec::new();
    render_markdown(markdown, &mut output)?;
    String::from_utf8(output).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

fn detect_callout(text: &str) -> Option<&'static Callout> {
    let trimmed = text.trim();
    if trimmed.starts_with("[!") && trimmed.contains(']') {
        if let Some(end) = trimmed.find(']') {
            let callout_type = &trimmed[2..end];
            return CALLOUTS
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case(callout_type))
                .map(|(_, callout)| callout);
        }
    }
    None
}

fn render_node<W: Write>(
    node: &Node,
    depth: usize,
    highlighter: &mut SyntaxHighlighter,
    writer: &mut W,
) -> io::Result<()> {
    render_node_inline(node, depth, false, highlighter, writer)
}

fn render_node_inline<W: Write>(
    node: &Node,
    depth: usize,
    inline: bool,
    highlighter: &mut SyntaxHighlighter,
    writer: &mut W,
) -> io::Result<()> {
    match node {
        Node::Heading(heading) => {
            if !inline {
                writeln!(writer)?;
            }

            let symbol = HEADER_SYMBOLS
                .get((heading.depth - 1) as usize)
                .unwrap_or(&"⑥");

            let text = render_inline_content(&heading.values);
            match heading.depth {
                1 => writeln!(
                    writer,
                    "{} {}",
                    symbol.bold().bright_blue(),
                    text.bold().bright_blue()
                )?,
                2 => writeln!(writer, "{} {}", symbol.bold().cyan(), text.bold().cyan())?,
                3 => writeln!(
                    writer,
                    "{} {}",
                    symbol.bold().yellow(),
                    text.bold().yellow()
                )?,
                4 => writeln!(writer, "{} {}", symbol.bold().green(), text.bold().green())?,
                5 => writeln!(
                    writer,
                    "{} {}",
                    symbol.bold().magenta(),
                    text.bold().magenta()
                )?,
                _ => writeln!(writer, "{} {}", symbol.bold().white(), text.bold().white())?,
            }
            writeln!(writer)?;
        }

        Node::Text(text) => {
            if !text.value.trim().is_empty() {
                if inline {
                    write!(writer, "{}", text.value)?;
                } else {
                    writeln!(writer, "{}", text.value)?;
                }
            }
        }

        Node::List(list) => {
            render_list(list, depth, highlighter, writer)?;
        }

        Node::Code(code) => {
            write!(writer, "{}", "```".bright_black())?;
            if let Some(lang) = &code.lang {
                write!(writer, "{}", lang.bright_black())?;
            }
            writeln!(writer)?;

            // Apply syntax highlighting if language is specified
            let highlighted = highlighter.highlight(&code.value, code.lang.as_deref());
            write!(writer, "{}", highlighted)?;

            writeln!(writer)?;
            writeln!(writer, "{}", "```".bright_black())?;
            writeln!(writer)?;
        }

        Node::CodeInline(code) => {
            write!(writer, "{}", format!("`{}`", code.value).bright_yellow())?;
        }

        Node::Strong(strong) => {
            write!(writer, "{}", render_inline_content(&strong.values).bold())?;
        }

        Node::Emphasis(emphasis) => {
            write!(
                writer,
                "{}",
                render_inline_content(&emphasis.values).italic()
            )?;
        }

        Node::Link(link) => {
            let text = render_inline_content(&link.values);
            let url = link.url.as_str();

            if text.trim().is_empty() {
                // If no link text, just make the URL clickable
                write!(
                    writer,
                    " {} {}",
                    "🔗".bright_blue(),
                    make_clickable_link(url, url)
                )?;
            } else {
                // Make the title clickable without showing URL
                write!(
                    writer,
                    " {} {}",
                    "🔗".bright_blue(),
                    make_clickable_link(url, &text).underline().bright_blue()
                )?;
            }
        }

        Node::Image(image) => {
            let alt = image.alt.as_str();
            let url = image.url.as_str();

            let _ = render_image_to_terminal(url);

            // Always show the text description as well
            if alt.trim().is_empty() {
                writeln!(
                    writer,
                    "{} {}",
                    "🖼️ ".bright_green(),
                    url.underline().bright_green()
                )?;
            } else {
                writeln!(
                    writer,
                    "{} {} ({})",
                    "🖼️ ".bright_green(),
                    alt.bright_green(),
                    url.bright_black()
                )?;
            }
        }

        Node::HorizontalRule(_) => {
            writeln!(writer, "{}", "─".repeat(80).bright_black())?;
            writeln!(writer)?;
        }

        Node::Blockquote(blockquote) => {
            if !inline {
                writeln!(writer)?;
            }

            // Check if this is a GitHub-style callout
            let is_callout = {
                let mut found_callout = false;
                // Check all nodes in blockquote for callout pattern
                for value in &blockquote.values {
                    match value {
                        Node::Fragment(para) => {
                            for child in &para.values {
                                if let Node::Text(text) = child {
                                    if detect_callout(&text.value).is_some() {
                                        found_callout = true;
                                        break;
                                    }
                                }
                            }
                        }
                        Node::Text(text) => {
                            if detect_callout(&text.value).is_some() {
                                found_callout = true;
                                break;
                            }
                        }
                        _ => {}
                    }
                    if found_callout {
                        break;
                    }
                }
                found_callout
            };

            if is_callout {
                render_callout_blockquote(blockquote, depth, highlighter, writer)?;
            } else {
                render_regular_blockquote(blockquote, depth, highlighter, writer)?;
            }

            writeln!(writer)?;
        }

        Node::Html(html) => {
            // Apply syntax highlighting to HTML
            let highlighted = highlighter.highlight(&html.value, Some("html"));
            writeln!(writer, "{}", highlighted)?;
        }

        Node::Break(_) => {
            if inline {
                write!(writer, " ")?;
            } else {
                writeln!(writer)?;
            }
        }

        Node::Fragment(fragment) => {
            // Render paragraph as inline content on one line
            for child in &fragment.values {
                render_node_inline(child, depth, true, highlighter, writer)?;
            }
            // Add newline after paragraph unless we're inline
            if !inline {
                writeln!(writer)?;
            }
        }

        Node::TableHeader(_) | Node::TableRow(_) => {
            // These should be handled by render_table in render_markdown
            // If we encounter them here, skip them
        }

        Node::TableCell(cell) => {
            // Individual table cells outside of tables
            // Calculate column widths for this cell
            let column_widths = calculate_column_widths(&[Node::TableCell(cell.clone())]);
            render_table_cell(cell, &column_widths, highlighter, writer)?;
        }

        // Handle other node types recursively if they have children
        _ => {
            if let Some(children) = get_node_children(node) {
                for child in children {
                    render_node_inline(child, depth, inline, highlighter, writer)?;
                }
            }
        }
    }

    Ok(())
}

fn render_list<W: Write>(
    list: &mq_markdown::List,
    depth: usize,
    highlighter: &mut SyntaxHighlighter,
    writer: &mut W,
) -> io::Result<()> {
    let indent = "  ".repeat(depth);
    let bullet_index = depth % LIST_BULLETS.len();
    let bullet = if list.ordered {
        format!("{}.", list.index + 1)
    } else {
        LIST_BULLETS[bullet_index].to_string()
    };

    // Handle checkbox lists
    let checkbox = match list.checked {
        Some(true) => "☑️ ",
        Some(false) => "☐ ",
        None => "",
    };

    write!(writer, "{}{} {}", indent, bullet.bright_magenta(), checkbox)?;

    let mut has_content = false;
    for value in &list.values {
        match value {
            Node::List(nested_list) => {
                if has_content {
                    writeln!(writer)?; // New line before nested list only if we had content
                }
                render_list(nested_list, depth + 1, highlighter, writer)?;
            }
            Node::Fragment(fragment) => {
                // Handle paragraph content inline
                for child in &fragment.values {
                    render_node_inline(child, depth + 1, true, highlighter, writer)?;
                }
                has_content = true;
            }
            _ => {
                render_node_inline(value, depth + 1, true, highlighter, writer)?;
                has_content = true;
            }
        }
    }

    writeln!(writer)?; // Add line break after list item
    Ok(())
}

fn render_callout_blockquote<W: Write>(
    blockquote: &mq_markdown::Blockquote,
    _depth: usize,
    highlighter: &mut SyntaxHighlighter,
    writer: &mut W,
) -> io::Result<()> {
    // Find the callout type from any text node in the blockquote
    let mut callout_info = None;
    let mut callout_text = String::new();

    for value in &blockquote.values {
        match value {
            Node::Fragment(para) => {
                for child in &para.values {
                    if let Node::Text(text) = child {
                        if let Some(callout) = detect_callout(&text.value) {
                            callout_info = Some(callout);
                            // Extract content after the callout marker
                            if let Some(end) = text.value.find(']') {
                                callout_text = text.value[end + 1..].trim_start().to_string();
                            }
                            break;
                        }
                    }
                }
            }
            Node::Text(text) => {
                if let Some(callout) = detect_callout(&text.value) {
                    callout_info = Some(callout);
                    if let Some(end) = text.value.find(']') {
                        callout_text = text.value[end + 1..].trim_start().to_string();
                    }
                    break;
                }
            }
            _ => {}
        }
        if callout_info.is_some() {
            break;
        }
    }

    if let Some(callout) = callout_info {
        // Print the callout header
        let header = format!("{} {}", callout.icon, callout.name)
            .color(callout.color)
            .bold();
        writeln!(writer, "┌─ {}", header)?;

        // Print the content
        if !callout_text.is_empty() {
            writeln!(writer, "│ {}", callout_text)?;
        }

        // Print remaining content from blockquote
        let mut found_callout_marker = false;
        for value in &blockquote.values {
            match value {
                Node::Fragment(para) => {
                    let mut line_content = String::new();
                    for child in &para.values {
                        match child {
                            Node::Text(text) => {
                                if !found_callout_marker && detect_callout(&text.value).is_some() {
                                    found_callout_marker = true;
                                    // Skip the callout marker part
                                    if let Some(end) = text.value.find(']') {
                                        let remaining = text.value[end + 1..].trim_start();
                                        if !remaining.is_empty() {
                                            line_content.push_str(remaining);
                                        }
                                    }
                                } else {
                                    line_content.push_str(&text.value);
                                }
                            }
                            Node::Link(link) => {
                                let text = render_inline_content(&link.values);
                                let url = link.url.as_str();
                                if text.trim().is_empty() {
                                    line_content.push_str(&format!(
                                        " 🔗 {}",
                                        make_clickable_link(url, url)
                                    ));
                                } else {
                                    line_content.push_str(&format!(
                                        " 🔗 {}",
                                        make_clickable_link(url, &text)
                                    ));
                                }
                            }
                            _ => {
                                // Handle all other inline formatting
                                line_content.push_str(&render_inline_content(&[child.clone()]));
                            }
                        }
                    }
                    if !line_content.trim().is_empty() && found_callout_marker {
                        writeln!(writer, "│ {}", line_content)?;
                    }
                }
                _ => {
                    if found_callout_marker {
                        write!(writer, "│ ")?;
                        render_node_inline(value, 0, false, highlighter, writer)?;
                    }
                }
            }
        }

        writeln!(writer, "└─")?;
    }
    Ok(())
}

fn render_regular_blockquote<W: Write>(
    blockquote: &mq_markdown::Blockquote,
    depth: usize,
    highlighter: &mut SyntaxHighlighter,
    writer: &mut W,
) -> io::Result<()> {
    for value in &blockquote.values {
        write!(writer, "{} ", "▌".bright_black())?;
        render_node_inline(value, depth, false, highlighter, writer)?;
    }
    Ok(())
}

fn render_inline_content(nodes: &[Node]) -> String {
    let mut result = String::new();
    for (i, node) in nodes.iter().enumerate() {
        // Add space between inline elements if needed
        if i > 0 && needs_space_before(node) && !result.ends_with(' ') {
            result.push(' ');
        }

        match node {
            Node::Text(text) => result.push_str(&text.value),
            Node::CodeInline(code) => result.push_str(&format!("`{}`", code.value)),
            Node::Strong(strong) => result.push_str(&render_inline_content(&strong.values)),
            Node::Emphasis(emphasis) => result.push_str(&render_inline_content(&emphasis.values)),
            Node::Link(link) => {
                let text = render_inline_content(&link.values);
                let url = link.url.as_str();
                if text.trim().is_empty() {
                    result.push_str(&format!("🔗 {}", make_clickable_link(url, url)));
                } else {
                    result.push_str(&format!("🔗 {}", make_clickable_link(url, &text)));
                }
            }
            _ => {}
        }
    }
    result
}

fn needs_space_before(node: &Node) -> bool {
    matches!(
        node,
        Node::Link(_) | Node::Strong(_) | Node::Emphasis(_) | Node::CodeInline(_)
    )
}

fn get_node_children(node: &Node) -> Option<&Vec<Node>> {
    match node {
        Node::Fragment(fragment) => Some(&fragment.values),
        Node::TableRow(row) => Some(&row.values),
        Node::TableCell(cell) => Some(&cell.values),
        _ => None,
    }
}

/// Render a complete table with proper column alignment
fn render_table<W: Write>(
    table_nodes: &[&Node],
    highlighter: &mut SyntaxHighlighter,
    writer: &mut W,
) -> io::Result<()> {
    if table_nodes.is_empty() {
        return Ok(());
    }

    // Calculate column widths from all cells
    let all_nodes: Vec<Node> = table_nodes.iter().map(|n| (*n).clone()).collect();
    let column_widths = calculate_column_widths(&all_nodes);

    // Find table header to determine column count
    let col_count = table_nodes
        .iter()
        .find_map(|node| {
            if let Node::TableHeader(header) = node {
                Some(header.align.len())
            } else {
                None
            }
        })
        .unwrap_or(column_widths.len());

    writeln!(writer)?;

    // Render top border
    render_table_top_border(&column_widths, col_count, writer)?;

    // Render cells row by row
    write!(writer, "{}", "│ ".bright_cyan())?;

    for (i, node) in table_nodes.iter().enumerate() {
        match node {
            Node::TableCell(cell) => {
                let content = render_inline_content(&cell.values);
                let width = column_widths.get(cell.column).copied().unwrap_or(0);

                for value in &cell.values {
                    render_node_inline(value, 0, true, highlighter, writer)?;
                }

                // Pad with spaces to align columns
                let content_width = content.chars().count();
                if content_width < width {
                    write!(writer, "{}", " ".repeat(width - content_width))?;
                }

                write!(writer, " {}", "│ ".bright_cyan())?;

                if cell.last_cell_in_row {
                    writeln!(writer)?;
                    // Check if next node is the header separator or another cell
                    if i + 1 < table_nodes.len() {
                        if let Some(Node::TableHeader(header)) = table_nodes.get(i + 1) {
                            render_table_header(header, &column_widths, writer)?;
                            // After header, if there's another cell, start a new row
                            if i + 2 < table_nodes.len()
                                && matches!(table_nodes.get(i + 2), Some(Node::TableCell(_)))
                            {
                                write!(writer, "{}", "│ ".bright_cyan())?;
                            }
                        } else if matches!(table_nodes.get(i + 1), Some(Node::TableCell(_))) {
                            // Start new row
                            write!(writer, "{}", "│ ".bright_cyan())?;
                        }
                    }
                }
            }
            Node::TableHeader(_) => {
                // Already handled in the TableCell last_cell_in_row logic
            }
            Node::TableRow(row) => {
                render_table_row(row, &column_widths, highlighter, writer)?;
            }
            _ => {}
        }
    }

    // Render bottom border
    render_table_bottom_border(&column_widths, col_count, writer)?;

    writeln!(writer)?;
    Ok(())
}

/// Calculate column widths for a table
fn calculate_column_widths(nodes: &[Node]) -> Vec<usize> {
    let mut column_widths: Vec<usize> = Vec::new();

    for node in nodes {
        match node {
            Node::TableRow(row) => {
                for (col_idx, cell_node) in row.values.iter().enumerate() {
                    if let Node::TableCell(cell) = cell_node {
                        let content = render_inline_content(&cell.values);
                        let width = content.chars().count();

                        if col_idx >= column_widths.len() {
                            column_widths.resize(col_idx + 1, 0);
                        }
                        column_widths[col_idx] = column_widths[col_idx].max(width);
                    }
                }
            }
            Node::TableCell(cell) => {
                let content = render_inline_content(&cell.values);
                let width = content.chars().count();

                if cell.column >= column_widths.len() {
                    column_widths.resize(cell.column + 1, 0);
                }
                column_widths[cell.column] = column_widths[cell.column].max(width);
            }
            _ => {}
        }
    }

    column_widths
}

/// Render table top border
fn render_table_top_border<W: Write>(
    column_widths: &[usize],
    col_count: usize,
    writer: &mut W,
) -> io::Result<()> {
    write!(writer, "{}", "┌".bright_black())?;
    for i in 0..col_count {
        let width = column_widths.get(i).copied().unwrap_or(4);
        write!(writer, "{}", "─".repeat(width + 2).bright_black())?;
        if i < col_count - 1 {
            write!(writer, "{}", "┬".bright_black())?;
        }
    }
    writeln!(writer, "{}", "┐".bright_black())?;
    Ok(())
}

/// Render table bottom border
fn render_table_bottom_border<W: Write>(
    column_widths: &[usize],
    col_count: usize,
    writer: &mut W,
) -> io::Result<()> {
    write!(writer, "{}", "└".bright_black())?;
    for i in 0..col_count {
        let width = column_widths.get(i).copied().unwrap_or(4);
        write!(writer, "{}", "─".repeat(width + 2).bright_black())?;
        if i < col_count - 1 {
            write!(writer, "{}", "┴".bright_black())?;
        }
    }
    writeln!(writer, "{}", "┘".bright_black())?;
    Ok(())
}

/// Render table header with alignment and column widths
fn render_table_header<W: Write>(
    header: &mq_markdown::TableHeader,
    column_widths: &[usize],
    writer: &mut W,
) -> io::Result<()> {
    write!(writer, "{}", "├".bright_black())?;
    for (i, align) in header.align.iter().enumerate() {
        let width = column_widths.get(i).copied().unwrap_or(4);
        let (left, right) = match align {
            mq_markdown::TableAlignKind::Left => (":", "─"),
            mq_markdown::TableAlignKind::Right => ("─", ":"),
            mq_markdown::TableAlignKind::Center => (":", ":"),
            mq_markdown::TableAlignKind::None => ("─", "─"),
        };

        write!(writer, "{}", left.bright_black())?;
        write!(writer, "{}", "─".repeat(width).bright_black())?;
        write!(writer, "{}", right.bright_black())?;

        if i < header.align.len() - 1 {
            write!(writer, "{}", "┼".bright_black())?;
        }
    }
    writeln!(writer, "{}", "┤".bright_black())?;
    Ok(())
}

/// Render table row with column widths
fn render_table_row<W: Write>(
    row: &mq_markdown::TableRow,
    column_widths: &[usize],
    highlighter: &mut SyntaxHighlighter,
    writer: &mut W,
) -> io::Result<()> {
    write!(writer, "{}", "│ ".bright_cyan())?;
    for (col_idx, cell_node) in row.values.iter().enumerate() {
        if let Node::TableCell(cell) = cell_node {
            let content = render_inline_content(&cell.values);
            let width = column_widths.get(col_idx).copied().unwrap_or(0);

            for value in &cell.values {
                render_node_inline(value, 0, true, highlighter, writer)?;
            }

            // Pad with spaces to align columns
            let content_width = content.chars().count();
            if content_width < width {
                write!(writer, "{}", " ".repeat(width - content_width))?;
            }

            write!(writer, " {}", "│ ".bright_cyan())?;
        }
    }
    writeln!(writer)?;
    Ok(())
}

/// Render table cell with column width
fn render_table_cell<W: Write>(
    cell: &mq_markdown::TableCell,
    column_widths: &[usize],
    highlighter: &mut SyntaxHighlighter,
    writer: &mut W,
) -> io::Result<()> {
    write!(writer, "{}", "│ ".bright_cyan())?;

    let content = render_inline_content(&cell.values);
    let width = column_widths.get(cell.column).copied().unwrap_or(0);

    for value in &cell.values {
        render_node_inline(value, 0, true, highlighter, writer)?;
    }

    // Pad with spaces to align columns
    let content_width = content.chars().count();
    if content_width < width {
        write!(writer, "{}", " ".repeat(width - content_width))?;
    }

    write!(writer, " ")?;
    if cell.last_cell_in_row {
        writeln!(writer, "{}", "│".bright_cyan())?;
    }
    Ok(())
}

/// Render an image to the terminal if possible
fn render_image_to_terminal(path: &str) -> io::Result<()> {
    // Check if the path is a local file
    if path.starts_with("http://") || path.starts_with("https://") {
        // For remote images, we would need to download them first
        // For now, skip rendering remote images
        return Ok(());
    }

    let image_path = Path::new(path);
    if !image_path.exists() {
        return Ok(());
    }

    // Use viuer to display the image with default configuration
    // This will auto-detect the best protocol (Kitty, iTerm2, Sixel, or blocks)
    let conf = viuer::Config {
        width: Some(60),
        height: None,
        absolute_offset: false,
        ..Default::default()
    };

    // Try to open and display the image
    if let Ok(img) = image::open(path) {
        let _ = viuer::print(&img, &conf);
    }

    Ok(())
}
