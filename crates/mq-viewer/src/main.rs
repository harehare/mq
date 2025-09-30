use clap::Parser;
use colored::*;
use miette::{IntoDiagnostic, Result};
use mq_markdown::{Markdown, Node};
use std::fs;
use std::io::{self, BufWriter, Write};
use std::io::{IsTerminal, Read};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "mqv")]
#[command(author = env!("CARGO_PKG_AUTHORS"))]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "A CLI markdown viewer with rich text rendering")]
pub struct Args {
    /// Markdown file to view
    #[arg(value_name = "FILE")]
    file: Option<PathBuf>,
}

/// Unicode header symbols (①②③④⑤⑥)
const HEADER_SYMBOLS: &[&str] = &["①", "②", "③", "④", "⑤", "⑥"];

/// Unicode bullet symbols for lists
const LIST_BULLETS: &[&str] = &["●", "○", "◆", "◇"];

/// Create a clickable link using ANSI escape sequences (OSC 8)
/// Format: ESC ] 8 ; params ; URI ST display_text ESC ] 8 ; ; ST
fn make_clickable_link(url: &str, display_text: &str) -> String {
    // Using ST (String Terminator) \x1b\\ instead of BEL \x07 for better compatibility
    format!("\x1b]8;;{}\x1b\\{}\x1b]8;;\x1b\\", url, display_text)
}

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

fn main() -> Result<()> {
    let args = Args::parse();
    let content = if io::stdin().is_terminal() {
        if let Some(file) = args.file {
            fs::read_to_string(&file).into_diagnostic()?
        } else {
            return Err(miette::miette!("No input file specified"));
        }
    } else {
        let mut buffer = String::new();
        io::stdin().read_to_string(&mut buffer).into_diagnostic()?;
        buffer
    };
    let markdown: Markdown = content.parse().map_err(|e| miette::miette!("{}", e))?;

    let stdout = io::stdout();
    let mut writer = BufWriter::new(stdout.lock());
    render_markdown(&markdown, &mut writer).into_diagnostic()?;
    writer.flush().into_diagnostic()?;

    Ok(())
}

fn render_markdown(markdown: &Markdown, writer: &mut BufWriter<io::StdoutLock>) -> io::Result<()> {
    for node in &markdown.nodes {
        render_node(node, 0, writer)?;
    }
    Ok(())
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

fn render_node(
    node: &Node,
    depth: usize,
    writer: &mut BufWriter<io::StdoutLock>,
) -> io::Result<()> {
    render_node_inline(node, depth, false, writer)
}

fn render_node_inline(
    node: &Node,
    depth: usize,
    inline: bool,
    writer: &mut BufWriter<io::StdoutLock>,
) -> io::Result<()> {
    match node {
        Node::Heading(heading) => {
            if !inline {
                writeln!(writer)?; // Add space before headers
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
            if inline || !text.value.trim().is_empty() {
                write!(writer, "{}", text.value)?;
            }
        }

        Node::List(list) => {
            render_list(list, depth, writer)?;
        }

        Node::Code(code) => {
            write!(writer, "{}", "```".bright_black())?;
            if let Some(lang) = &code.lang {
                write!(writer, "{}", lang.bright_black())?;
            }
            writeln!(writer)?;
            writeln!(writer, "{}", code.value.bright_white())?;
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
            if alt.trim().is_empty() {
                write!(
                    writer,
                    "{} {}",
                    "🖼️ ".bright_green(),
                    url.underline().bright_green()
                )?;
            } else {
                write!(
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
                render_callout_blockquote(blockquote, depth, writer)?;
            } else {
                render_regular_blockquote(blockquote, depth, writer)?;
            }

            writeln!(writer)?;
        }

        Node::Html(html) => {
            // Display HTML as-is as requested
            writeln!(writer, "{}", html.value)?;
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
                render_node_inline(child, depth, true, writer)?;
            }
            // Add newline after paragraph unless we're inline
            if !inline {
                writeln!(writer)?;
            }
        }

        // Handle other node types recursively if they have children
        _ => {
            if let Some(children) = get_node_children(node) {
                for child in children {
                    render_node_inline(child, depth, inline, writer)?;
                }
            }
        }
    }

    Ok(())
}

fn render_list(
    list: &mq_markdown::List,
    depth: usize,
    writer: &mut BufWriter<io::StdoutLock>,
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
                render_list(nested_list, depth + 1, writer)?;
            }
            Node::Fragment(fragment) => {
                // Handle paragraph content inline
                for child in &fragment.values {
                    render_node_inline(child, depth + 1, true, writer)?;
                }
                has_content = true;
            }
            _ => {
                render_node_inline(value, depth + 1, true, writer)?;
                has_content = true;
            }
        }
    }

    writeln!(writer)?; // Add line break after list item
    Ok(())
}

fn render_callout_blockquote(
    blockquote: &mq_markdown::Blockquote,
    _depth: usize,
    writer: &mut BufWriter<io::StdoutLock>,
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
                        render_node_inline(value, 0, false, writer)?;
                    }
                }
            }
        }

        writeln!(writer, "└─")?;
    }
    Ok(())
}

fn render_regular_blockquote(
    blockquote: &mq_markdown::Blockquote,
    depth: usize,
    writer: &mut BufWriter<io::StdoutLock>,
) -> io::Result<()> {
    for value in &blockquote.values {
        write!(writer, "{} ", "▌".bright_black())?;
        render_node_inline(value, depth, false, writer)?;
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
