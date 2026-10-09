//! Rendering nodes back to Markdown.

use super::*;

impl Node {
    pub fn to_string_with(&self, options: &RenderOptions) -> String {
        self.render_with_theme(options, &ColorTheme::PLAIN)
    }

    /// Returns a colored string representation of this node using ANSI escape codes.
    #[cfg(feature = "color")]
    pub fn to_colored_string_with(&self, options: &RenderOptions) -> String {
        self.render_with_theme(options, &ColorTheme::COLORED)
    }

    pub(crate) fn render_with_theme(&self, options: &RenderOptions, theme: &ColorTheme<'_>) -> String {
        match self {
            Self::List(List {
                level,
                checked,
                values,
                ordered,
                index,
                start,
                marker,
                ..
            }) => {
                let marker = if *ordered {
                    let delimiter = if *marker == Some(ListMarker::Paren) { ')' } else { '.' };
                    format!("{}{}", start.unwrap_or(1) as usize + *index, delimiter)
                } else if let Some(style) = &options.list_style {
                    style.to_string()
                } else {
                    marker.and_then(ListMarker::style).unwrap_or_default().to_string()
                };
                let checkbox = (*checked).map(|it| if it { "[x] " } else { "[ ] " }).unwrap_or("");
                let prefix_width = *level as usize * 2 + list_own_prefix_width(*ordered, *index, *start);
                // Lines after the first are indented to the content of the item, whatever column they
                // had in the source, which is not reliable after an escape.
                let content = render_values_block(values, options, theme);
                let content = indent_continuation(&content, prefix_width);
                let (ms, me) = &theme.list_marker;
                format!(
                    "{}{}{}{} {}{}",
                    "  ".repeat(*level as usize),
                    ms,
                    marker,
                    me,
                    checkbox,
                    content
                )
            }
            Self::TableRow(TableRow { values, .. }) => {
                let (ts, te) = &theme.table_separator;
                let cells = values
                    .iter()
                    .map(|cell| cell.render_with_theme(options, theme))
                    .collect::<Vec<_>>()
                    .join("|");
                format!("{}|{}{}|", ts, te, cells)
            }
            Self::TableCell(TableCell { values, .. }) => render_cell_values(values, options, theme),
            Self::TableAlign(TableAlign { align, .. }) => {
                let (ts, te) = &theme.table_separator;
                format!("{}|{}|{}", ts, align.iter().map(|a| a.to_string()).join("|"), te)
            }
            Self::Blockquote(Blockquote { values, .. }) => {
                let (bs, be) = &theme.blockquote_marker;
                render_values_block(values, options, theme)
                    .split('\n')
                    .map(|line| format!("{}> {}{}", bs, be, line))
                    .join("\n")
            }
            #[cfg(feature = "callout")]
            Self::Callout(Callout {
                kind,
                fold,
                title,
                values,
                ..
            }) => {
                let (bs, be) = &theme.blockquote_marker;
                let fold = fold.map(String::from).unwrap_or_default();
                let header = match title.as_deref() {
                    Some(t) if !t.is_empty() => format!("[!{}]{} {}", kind, fold, t),
                    _ => format!("[!{}]{}", kind, fold),
                };
                let header_line = format!("{}> {}{}", bs, be, header);
                if values.is_empty() {
                    return header_line;
                }
                let body = render_values_block(values, options, theme);
                if body.trim().is_empty() {
                    header_line
                } else {
                    let body_lines = body
                        .split('\n')
                        .map(|line| format!("{}> {}{}", bs, be, line))
                        .join("\n");
                    format!("{}\n{}", header_line, body_lines)
                }
            }
            #[cfg(feature = "embed")]
            Self::Embed(Embed { target, display, .. }) => match display.as_deref() {
                Some(d) if !d.is_empty() => format!("![[{}|{}]]", target, d),
                _ => format!("![[{}]]", target),
            },
            Self::Code(Code {
                value,
                lang,
                fence,
                meta,
                position,
            }) => {
                let (cs, ce) = &theme.code;
                if lang.is_some() || *fence {
                    let meta = meta.as_deref().map(|meta| format!(" {}", meta)).unwrap_or_default();
                    let info = format!("{}{}", lang.as_deref().unwrap_or(""), meta);
                    // Empty body skips the content line so it doesn't gain a blank one.
                    let fence_str = code_fence(value, &info);
                    if value.is_empty() {
                        // Blank lines are not in the value, but the lines of the block tell of them.
                        let blank = position
                            .as_ref()
                            .map_or(0, |p| p.end.line.saturating_sub(p.start.line).saturating_sub(1));
                        format!("{}{}{}\n{}{}{}", cs, fence_str, info, "\n".repeat(blank), fence_str, ce)
                    } else {
                        format!("{}{}{}\n{}\n{}{}", cs, fence_str, info, value, fence_str, ce)
                    }
                } else {
                    value.lines().map(|line| format!("{}    {}{}", cs, line, ce)).join("\n")
                }
            }
            Self::Definition(Definition {
                ident,
                label,
                url,
                title,
                ..
            }) => {
                let (us, ue) = &theme.link_url;
                format!(
                    "[{}]: {}{}{}{}",
                    definition_label(label.as_deref(), ident),
                    us,
                    url.to_string_with(options),
                    ue,
                    title
                        .as_ref()
                        .map(|title| format!(" {}", title.to_string_with(options)))
                        .unwrap_or_default()
                )
            }
            Self::Delete(Delete { values, .. }) => {
                let (ds, de) = &theme.delete;
                format!("{}~~{}~~{}", ds, render_values(values, options, theme), de)
            }
            Self::Emphasis(Emphasis { values, .. }) => {
                // A lone nested Emphasis child needs the other delimiter, or adjacent
                // `*` `*` pairs fuse into `**` and reparse as Strong instead.
                let delim = if matches!(values.as_slice(), [Self::Emphasis(_)]) {
                    "_"
                } else {
                    "*"
                };
                render_emphasis(values, delim, options, theme)
            }
            Self::Footnote(Footnote { values, ident, .. }) => {
                format!(
                    "[^{}]: {}",
                    escape_brackets(ident),
                    render_values(values, options, theme)
                )
            }
            Self::FootnoteRef(FootnoteRef { label, ident, .. }) => {
                format!("[^{}]", reference_label(label.as_deref(), ident))
            }
            Self::Heading(Heading { depth, values, .. }) => {
                let (hs, he) = &theme.heading;
                let text = render_values(values, options, theme);
                // A line ending at either end is not part of the heading text, so it is written as a
                // character reference.
                let text = {
                    let inner = text.trim_matches('\n');
                    let leading = text.len() - text.trim_start_matches('\n').len();
                    let trailing = text.len() - text.trim_end_matches('\n').len();
                    if inner.is_empty() || (leading == 0 && trailing == 0) {
                        text.clone()
                    } else {
                        format!("{}{}{}", "&#10;".repeat(leading), inner, "&#10;".repeat(trailing))
                    }
                };
                // Multi-line content must stay setext for depths 1-2; ATX has no setext form.
                if text.contains('\n') && *depth <= HeadingDepth::H2 {
                    let underline = if *depth == HeadingDepth::H1 { "===" } else { "---" };
                    format!("{}{}\n{}{}", hs, text, underline, he)
                } else {
                    // A trailing `#` run reads back as an ATX closing sequence and gets
                    // stripped as syntax rather than kept as heading text.
                    let text = text.replace('\n', " ");
                    let text = match text.rfind(|c: char| c != '#') {
                        Some(i) => {
                            let hash_start = i + text[i..].chars().next().map_or(1, char::len_utf8);
                            if hash_start < text.len() {
                                format!("{}\\{}", &text[..hash_start], &text[hash_start..])
                            } else {
                                text
                            }
                        }
                        None if !text.is_empty() => format!("\\{text}"),
                        _ => text,
                    };
                    format!("{}{} {}{}", hs, "#".repeat(usize::from(depth.get())), text, he)
                }
            }
            Self::Html(Html { value, .. }) => {
                let (hs, he) = &theme.html;
                format!("{}{}{}", hs, value, he)
            }
            Self::Image(Image { alt, url, title, .. }) => {
                let (is, ie) = &theme.image;
                format!(
                    "{}![{}]({}{}){}",
                    is,
                    escape_label(alt),
                    render_link_destination(url, &options.link_url_style),
                    title
                        .as_deref()
                        .map(|it| format!(" {}", render_link_title(it, &options.link_title_style)))
                        .unwrap_or_default(),
                    ie
                )
            }
            // Shortcut is safe only if `alt` normalizes back to `ident`; `ident` is raw
            // (already correctly escaped), so escape_label would double it up.
            Self::ImageRef(ImageRef { alt, ident, label, .. }) => {
                let (is, ie) = &theme.image;
                let mismatched = crate::parser::normalize(alt) != ident.as_str();
                if mismatched || needs_broad_escaping(alt) {
                    format!(
                        "{}![{}][{}]{}",
                        is,
                        escape_label(alt),
                        reference_label(label.as_deref(), ident),
                        ie
                    )
                } else {
                    format!("{}![{}]{}", is, escape_label(alt), ie)
                }
            }
            Self::CodeInline(CodeInline { value, .. }) => {
                let (cs, ce) = &theme.code_inline;
                let fence = code_span_fence(value);
                // Padding avoids fusing with an edge backtick and protects a genuine
                // leading+trailing space from the parser's own space-stripping rule.
                let all_spaces = value.chars().all(|c| c == ' ');
                if value.starts_with('`')
                    || value.ends_with('`')
                    || (!all_spaces && value.starts_with(' ') && value.ends_with(' '))
                {
                    format!("{}{} {} {}{}", cs, fence, value, fence, ce)
                } else {
                    format!("{}{}{}{}{}", cs, fence, value, fence, ce)
                }
            }
            Self::MathInline(MathInline { value, .. }) => {
                let (ms, me) = &theme.math;
                let fence = math_span_fence(value);
                // Padding keeps a dollar sign at an end from fusing with the fence, and a genuine space at
                // both ends from being removed when it is read.
                let all_spaces = value.chars().all(|c| c == ' ');
                if value.starts_with('$')
                    || value.ends_with('$')
                    || (!all_spaces && value.starts_with(' ') && value.ends_with(' '))
                {
                    format!("{}{} {} {}{}", ms, fence, value, fence, me)
                } else {
                    format!("{}{}{}{}{}", ms, fence, value, fence, me)
                }
            }
            Self::Link(Link { url, title, values, .. }) => {
                let (ls, le) = &theme.link;
                if title.is_none()
                    && let Some(target) = autolink_target(url.as_str(), values)
                {
                    return format!("{ls}<{target}>{le}");
                }
                format!(
                    "{}[{}]({}{}){}",
                    ls,
                    render_values(values, options, theme),
                    url.to_string_with(options),
                    title
                        .as_ref()
                        .map(|title| format!(" {}", title.to_string_with(options)))
                        .unwrap_or_default(),
                    le
                )
            }
            #[cfg(feature = "wikilink")]
            Self::WikiLink(WikiLink { target, text, .. }) => {
                let (ls, le) = &theme.link;
                match text.as_deref() {
                    Some(t) if t != target.as_str() => format!("{}[[{}|{}]]{}", ls, target, t, le),
                    _ => format!("{}[[{}]]{}", ls, target, le),
                }
            }
            // Same reasoning as ImageRef, plus the same broad-escaping fallback.
            Self::LinkRef(LinkRef {
                values, ident, label, ..
            }) => {
                let (ls, le) = &theme.link;
                let rendered = render_values(values, options, theme);
                let plain = values_to_value(values);
                // The label of a shortcut reference is its text as written, markup included.
                let written = render_values(values, options, &ColorTheme::PLAIN);
                let mismatched = crate::parser::normalize(&written) != ident.as_str();

                if mismatched || needs_broad_escaping(&plain) {
                    format!(
                        "{}[{}][{}]{}",
                        ls,
                        rendered,
                        reference_label(label.as_deref(), ident),
                        le
                    )
                } else {
                    format!("{}[{}]{}", ls, rendered, le)
                }
            }
            Self::Math(Math { value, .. }) => {
                let (ms, me) = &theme.math;
                let fence = "$".repeat((longest_run(value, '$') + 1).max(2));
                format!("{}{}\n{}\n{}{}", ms, fence, value, fence, me)
            }
            // Only escape text parsed from real markdown (has a position); programmatic
            // values like JSON output or attr lookups must stay untouched.
            Self::Text(Text { value, position }) => {
                if position.is_some() {
                    escape_text_with(value.clone(), options.mdx)
                } else {
                    value.clone()
                }
            }
            Self::MdxFlowExpression(mdx_flow_expression) => {
                format!("{{{}}}", indent_expression(&mdx_flow_expression.value))
            }
            Self::MdxJsxFlowElement(mdx_jsx_flow_element) => {
                let name = mdx_jsx_flow_element.name.as_deref().unwrap_or_default();
                let attributes = if mdx_jsx_flow_element.attributes.is_empty() {
                    "".to_string()
                } else {
                    format!(
                        " {}",
                        mdx_jsx_flow_element
                            .attributes
                            .iter()
                            .map(Self::mdx_attribute_content_to_string)
                            .join(" ")
                    )
                };

                if mdx_jsx_flow_element.children.is_empty() {
                    // A fragment has no self-closing form.
                    if name.is_empty() {
                        format!("<{}></>", attributes)
                    } else {
                        format!("<{}{} />", name, attributes,)
                    }
                } else {
                    // Each tag and the children sit on their own lines, or the children would read as
                    // inline content of the tag.
                    let children = render_values_block(&mdx_jsx_flow_element.children, options, theme);
                    // The lines of an expression are indented as they are written, which indenting them
                    // again would add to each time they are read.
                    let children = if mdx_jsx_flow_element.children.iter().any(has_multiline_expression) {
                        children
                    } else {
                        indent_continuation(&format!("  {}", children), 2)
                    };
                    format!("<{}{}>\n{}\n</{}>", name, attributes, children, name)
                }
            }
            Self::MdxJsxTextElement(mdx_jsx_text_element) => {
                let name = mdx_jsx_text_element.name.as_deref().unwrap_or_default();
                let attributes = if mdx_jsx_text_element.attributes.is_empty() {
                    "".to_string()
                } else {
                    format!(
                        " {}",
                        mdx_jsx_text_element
                            .attributes
                            .iter()
                            .map(Self::mdx_attribute_content_to_string)
                            .join(" ")
                    )
                };

                if mdx_jsx_text_element.children.is_empty() {
                    // A fragment has no self-closing form.
                    if name.is_empty() {
                        format!("<{}></>", attributes)
                    } else {
                        format!("<{}{} />", name, attributes,)
                    }
                } else {
                    format!(
                        "<{}{}>{}</{}>",
                        name,
                        attributes,
                        render_values(&mdx_jsx_text_element.children, options, theme),
                        name
                    )
                }
            }
            Self::MdxTextExpression(mdx_text_expression) => {
                format!("{{{}}}", indent_expression(&mdx_text_expression.value))
            }
            Self::MdxJsEsm(mdxjs_esm) => mdxjs_esm.value.to_string(),
            Self::Strong(Strong { values, .. }) => {
                let (ss, se) = &theme.strong;
                let last = values.len().saturating_sub(1);
                let content = values
                    .iter()
                    .enumerate()
                    .map(|(index, value)| match value {
                        // Emphasis at an end uses the other delimiter, or `*` and `**` fuse into `***`.
                        // An underscore does not open or close next to a letter or digit, so those keep `*`.
                        Self::Emphasis(Emphasis { values: inner, .. })
                            if (index == 0 || index == last)
                                && !index
                                    .checked_sub(1)
                                    .is_some_and(|before| ends_with_word_char(&values[before]))
                                && !values.get(index + 1).is_some_and(starts_with_word_char) =>
                        {
                            render_emphasis(inner, "_", options, theme)
                        }
                        value => value.render_with_theme(options, theme),
                    })
                    .collect::<String>();
                format!("{}**{}**{}", ss, content, se)
            }
            Self::Yaml(Yaml { value, .. }) => {
                let (fs, fe) = &theme.frontmatter;
                format!("{}---\n{}\n---{}", fs, value, fe)
            }
            Self::Toml(Toml { value, .. }) => {
                let (fs, fe) = &theme.frontmatter;
                format!("{}+++\n{}\n+++{}", fs, value, fe)
            }
            Self::Break(_) => "\\\n".to_string(),
            Self::HorizontalRule(HorizontalRule { marker, .. }) => {
                let (hs, he) = &theme.horizontal_rule;
                let marker = match marker {
                    Some(HorizontalRuleMarker::Dash) => '-',
                    Some(HorizontalRuleMarker::Underscore) => '_',
                    _ => '*',
                };
                format!("{hs}{marker}{marker}{marker}{he}")
            }
            Self::Fragment(Fragment { values }) => values
                .iter()
                .filter(|value| !value.is_empty() && !value.is_empty_fragment())
                .map(|value| value.render_with_theme(options, theme))
                .join("\n"),
            Self::Empty => String::new(),
        }
    }

    fn mdx_attribute_content_to_string(attr: &MdxAttributeContent) -> SmolStr {
        match attr {
            MdxAttributeContent::Expression(value) => format!("{{{}}}", indent_expression(value)).into(),
            MdxAttributeContent::Property(property) => match &property.value {
                Some(value) => match value {
                    MdxAttributeValue::Expression(value) => {
                        format!("{}={{{}}}", property.name, indent_expression(value)).into()
                    }
                    MdxAttributeValue::Literal(literal) => {
                        format!("{}={}", property.name, quote_attribute_value(literal)).into()
                    }
                },
                None => property.name.clone(),
            },
        }
    }
}

/// Whether `node` or a node in it is an expression of more than one line.
fn has_multiline_expression(node: &Node) -> bool {
    match node {
        Node::MdxFlowExpression(expression) => expression.value.contains('\n'),
        Node::MdxTextExpression(expression) => expression.value.contains('\n'),
        Node::MdxJsxTextElement(MdxJsxTextElement { attributes, .. })
        | Node::MdxJsxFlowElement(MdxJsxFlowElement { attributes, .. })
            if attributes.iter().any(|attribute| match attribute {
                MdxAttributeContent::Expression(value) => value.contains('\n'),
                MdxAttributeContent::Property(MdxJsxAttribute {
                    value: Some(MdxAttributeValue::Expression(value)),
                    ..
                }) => value.contains('\n'),
                _ => false,
            }) =>
        {
            true
        }
        other => other.children().iter().any(has_multiline_expression),
    }
}

/// Quotes the value of a JSX attribute with the quote it does not contain. Character references that the
/// value holds as text are written with their `&` as a reference.
fn quote_attribute_value(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for (index, c) in value.char_indices() {
        if c == '&' && starts_reference(&value[index + 1..]) {
            escaped.push_str("&amp;");
        } else {
            escaped.push(c);
        }
    }
    if !escaped.contains('"') {
        format!("\"{escaped}\"")
    } else if !escaped.contains('\'') {
        format!("'{escaped}'")
    } else {
        format!("\"{}\"", escaped.replace('"', "&quot;"))
    }
}

/// The content of an expression as it is written: the lines after the first are indented, as the
/// indentation of up to two characters is removed from them when they are read.
fn indent_expression(value: &str) -> String {
    value
        .split('\n')
        .enumerate()
        .map(|(index, line)| {
            if index == 0 || line.is_empty() || line == "\r" {
                line.to_string()
            } else {
                format!("  {line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn values_to_string(values: &[Node], options: &RenderOptions) -> String {
    render_values(values, options, &ColorTheme::PLAIN)
}

/// Indents every line, the first included, by `width` spaces, except empty ones.
pub(crate) fn indent_lines(content: &str, width: usize) -> String {
    let pad = " ".repeat(width);
    content
        .split('\n')
        .map(|line| {
            if line.is_empty() {
                line.to_string()
            } else {
                format!("{pad}{line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Indents every line after the first by `width` spaces, except empty ones.
fn indent_continuation(content: &str, width: usize) -> String {
    let Some((first, rest)) = content.split_once('\n') else {
        return content.to_string();
    };
    let pad = " ".repeat(width);
    let mut result = String::with_capacity(content.len() + width * 2);
    result.push_str(first);
    for line in rest.split('\n') {
        result.push('\n');
        if !line.is_empty() {
            result.push_str(&pad);
            result.push_str(line);
        }
    }
    result
}

fn render_emphasis(values: &[Node], delim: &str, options: &RenderOptions, theme: &ColorTheme<'_>) -> String {
    let (es, ee) = &theme.emphasis;
    format!("{es}{delim}{}{delim}{ee}", render_values(values, options, theme))
}

/// Renders the content of a table cell. A pipe that is not escaped yet, as in code, would end the cell.
pub(crate) fn render_cell_values(values: &[Node], options: &RenderOptions, theme: &ColorTheme<'_>) -> String {
    let rendered = render_values(values, options, theme);
    if !rendered.contains('|') {
        return rendered;
    }
    let mut result = String::with_capacity(rendered.len() + 4);
    let mut escaped = false;
    for c in rendered.chars() {
        if c == '|' && !escaped {
            result.push('\\');
        }
        escaped = c == '\\' && !escaped;
        result.push(c);
    }
    result
}

fn ends_with_word_char(node: &Node) -> bool {
    matches!(node, Node::Text(Text { value, .. }) if value.ends_with(|c: char| c.is_alphanumeric()))
}

fn starts_with_word_char(node: &Node) -> bool {
    matches!(node, Node::Text(Text { value, .. }) if value.starts_with(|c: char| c.is_alphanumeric()))
}

/// Renders `value`, escaping a `!` that ends its text when `next` starts a link, which would make an image.
pub(crate) fn render_before(
    value: &Node,
    next: Option<&Node>,
    options: &RenderOptions,
    theme: &ColorTheme<'_>,
) -> String {
    let mut rendered = value.render_with_theme(options, theme);
    let starts_link = match next {
        Some(Node::Link(_) | Node::LinkRef(_)) => true,
        #[cfg(feature = "wikilink")]
        Some(Node::WikiLink(_)) => true,
        _ => false,
    };
    if starts_link && matches!(value, Node::Text(Text { position: Some(_), .. })) && rendered.ends_with('!') {
        rendered.insert(rendered.len() - 1, '\\');
    }
    rendered
}

pub(crate) fn render_values(values: &[Node], options: &RenderOptions, theme: &ColorTheme<'_>) -> String {
    let mut pre_position: Option<Position> = None;
    values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let rendered = || render_before(value, values.get(index + 1), options, theme);
            if let Some(pos) = value.position() {
                let new_line_count = pre_position
                    .as_ref()
                    .map(|p: &Position| pos.start.line.saturating_sub(p.end.line))
                    .unwrap_or_default();

                let space = if new_line_count > 0
                    && pre_position
                        .as_ref()
                        .map(|p| pos.start.line > p.end.line)
                        .unwrap_or_default()
                {
                    " ".repeat(pos.start.column.saturating_sub(1))
                } else {
                    "".to_string()
                };

                pre_position = Some(pos);

                if space.is_empty() {
                    format!("{}{}", "\n".repeat(new_line_count), rendered())
                } else {
                    format!(
                        "{}{}",
                        "\n".repeat(new_line_count),
                        rendered().lines().map(|line| format!("{}{}", space, line)).join("\n")
                    )
                }
            } else {
                pre_position = None;
                rendered()
            }
        })
        .collect::<String>()
}

/// Like `render_values` but without column-offset–based indentation.
///
/// Inside blockquotes and callouts, node positions include the `> ` prefix offset,
/// so applying column-based spacing causes double-indentation. This variant preserves
/// blank lines between values (via newline counting) but ignores the column position.
pub(crate) fn render_values_block(values: &[Node], options: &RenderOptions, theme: &ColorTheme<'_>) -> String {
    let is_table_part = |node: &Node| matches!(node, Node::TableCell(_) | Node::TableAlign(_));
    let mut result = String::new();
    let mut pre_position: Option<Position> = None;
    let mut index = 0;
    // The last node written, to tell paragraphs from the inline nodes of one when there are no positions.
    let mut previous: Option<&Node> = None;

    while index < values.len() {
        // The cells of a table are laid out together, as at the top level.
        let (rendered, position, next, last) = if is_table_part(&values[index]) {
            let end = values[index..]
                .iter()
                .position(|node| !is_table_part(node))
                .map_or(values.len(), |offset| index + offset);
            let run = &values[index..end];
            let table = crate::Markdown {
                nodes: run.to_vec(),
                options: options.clone(),
            };
            let position = match (run[0].position(), run[run.len() - 1].position()) {
                (Some(first), Some(last)) => Some(Position {
                    start: first.start,
                    end: last.end,
                }),
                _ => None,
            };
            (
                table.render_with_theme(theme).trim_end_matches('\n').to_string(),
                position,
                end,
                &run[run.len() - 1],
            )
        } else {
            let value = &values[index];
            (
                render_before(value, values.get(index + 1), options, theme),
                value.position(),
                index + 1,
                value,
            )
        };
        let first = &values[index];
        index = next;

        if let Some(pos) = position {
            let new_line_count = pre_position
                .as_ref()
                .map(|p: &Position| pos.start.line.saturating_sub(p.end.line))
                .unwrap_or_default();
            pre_position = Some(pos);
            result.push_str(&"\n".repeat(new_line_count));
        } else {
            // Text never follows text inside one paragraph, so without positions that is a new one.
            if pre_position.is_none() && matches!((previous, first), (Some(Node::Text(_)), Node::Text(_))) {
                result.push_str("\n\n");
            }
            pre_position = None;
        }
        previous = Some(last);
        result.push_str(&rendered);
    }
    result
}

/// Renders a link/image destination, auto-upgrading to `<...>` (escaping `\ < >`)
/// when the bare form would be unsafe (empty, or contains whitespace/control/`< >`);
/// otherwise bare, escaping `\ ( )` since unescaped parens must balance.
pub(super) fn render_link_destination(url: &str, style: &UrlSurroundStyle) -> String {
    let needs_angle = url.is_empty()
        || url
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || c == '<' || c == '>');

    if matches!(style, UrlSurroundStyle::Angle) || needs_angle {
        let mut result = String::with_capacity(url.len() + 2);
        result.push('<');
        for c in url.chars() {
            if matches!(c, '\\' | '<' | '>') {
                result.push('\\');
            }
            result.push(c);
        }
        result.push('>');
        result
    } else {
        let mut result = String::with_capacity(url.len());
        for c in url.chars() {
            if matches!(c, '\\' | '(' | ')') {
                result.push('\\');
            }
            result.push(c);
        }
        result
    }
}

/// Renders a link/image title, escaping `\` and the style's delimiter(s) so the
/// title's own quote/paren characters can't end it early.
pub(super) fn render_link_title(title: &str, style: &TitleSurroundStyle) -> String {
    let (open, close, delims): (char, char, &[char]) = match style {
        TitleSurroundStyle::Double => ('"', '"', &['"']),
        TitleSurroundStyle::Single => ('\'', '\'', &['\'']),
        TitleSurroundStyle::Paren => ('(', ')', &['(', ')']),
    };
    let mut result = String::with_capacity(title.len() + 2);
    result.push(open);
    for c in title.chars() {
        if c == '\\' || delims.contains(&c) {
            result.push('\\');
        }
        result.push(c);
    }
    result.push(close);
    result
}

/// Escapes `\ [ ]` in reference labels/idents and image alt text: plain strings
/// (not walked as inline nodes) that still sit inside `[...]` syntax.
/// True if `s` needs escaping beyond `escape_label`'s narrow `\ [ ]` set — unsafe
/// for a reference's implicit shortcut form, since the definition's label won't match.
fn needs_broad_escaping(s: &str) -> bool {
    s.bytes().any(|b| {
        matches!(
            b,
            b'`' | b'*' | b'_' | b'|' | b'~' | b'$' | b'#' | b'-' | b'+' | b'>' | b'=' | b'.' | b')' | b'!'
        )
    })
}

/// Shifts every line's leading indentation by `delta` spaces (never below zero),
/// leaving each line's own content untouched.
pub(crate) fn reindent_all_lines(content: &str, delta: isize) -> String {
    if delta == 0 {
        return content.to_string();
    }
    content
        .split('\n')
        .map(|line| {
            if line.is_empty() {
                line.to_string()
            } else {
                let leading = line.bytes().take_while(|&b| b == b' ').count();
                let new_leading = (leading as isize + delta).max(0) as usize;
                format!("{}{}", " ".repeat(new_leading), &line[leading..])
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The width of a list item's own marker and trailing space (not including ancestor indentation),
/// e.g. 2 for `"- "`, 3 for `"1. "`, 4 for `"10. "`. A checkbox is not part of it, as the content of
/// the item starts at the checkbox, so children are indented to the marker alone.
pub(crate) fn list_own_prefix_width(ordered: bool, index: usize, start: Option<u32>) -> usize {
    let marker_len = if ordered {
        (start.unwrap_or(1) as usize + index).to_string().chars().count() + 1
    } else {
        1
    };
    marker_len + 1
}

/// The length of the longest run of `ch` in `s`.
fn longest_run(s: &str, ch: char) -> usize {
    s.split('\n')
        .flat_map(|line| line.split(|c| c != ch).map(str::len))
        .max()
        .unwrap_or(0)
}

/// Longest fence-char run in `value` + 1 (min 3); switches to tilde when `info`
/// has a backtick, since a backtick-fenced info string can't contain one.
fn code_fence(value: &str, info: &str) -> String {
    if info.contains('`') {
        let len = (longest_run(value, '~') + 1).max(3);
        "~".repeat(len)
    } else {
        let len = (longest_run(value, '`') + 1).max(3);
        "`".repeat(len)
    }
}

/// The label to write in a definition: the identifier as it was written when the label is only that
/// without its escapes and character references, which would not read back as the same identifier.
fn definition_label(label: Option<&str>, ident: &str) -> String {
    match label {
        Some(label) if crate::parser::normalize(label) == ident => escape_label(label),
        Some(label) if crate::parser::normalize(label) == crate::parser::normalize(&crate::parser::unescape(ident)) => {
            ident.to_string()
        }
        Some(label) => escape_label(label),
        None => ident.to_string(),
    }
}

/// The label to write in the second brackets of a full reference. The label as written is kept when it
/// still resolves to `ident`.
fn reference_label(label: Option<&str>, ident: &str) -> String {
    match label {
        Some(label) if crate::parser::normalize(label) == ident => escape_label(label),
        _ => ident.to_string(),
    }
}

/// Picks an inline code span delimiter one backtick longer than the longest
/// backtick run in `value`, so no run inside it can be mistaken for the closing.
fn code_span_fence(value: &str) -> String {
    let mut max_run = 0;
    let mut cur = 0;
    for c in value.chars() {
        if c == '`' {
            cur += 1;
            max_run = max_run.max(cur);
        } else {
            cur = 0;
        }
    }
    "`".repeat(max_run + 1)
}

/// The shortest run of dollar signs, from one, that is not the length of a run in `value`, so that no
/// run inside a math span can be mistaken for the closing.
fn math_span_fence(value: &str) -> String {
    let mut lengths = std::collections::HashSet::new();
    let mut current = 0;
    for c in value.chars() {
        if c == '$' {
            current += 1;
        } else if current > 0 {
            lengths.insert(current);
            current = 0;
        }
    }
    if current > 0 {
        lengths.insert(current);
    }
    "$".repeat((1..).find(|length| !lengths.contains(length)).unwrap_or(1))
}

/// Escapes the brackets that are not escaped yet, for an identifier that keeps the escapes it was written with.
fn escape_brackets(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if chars.peek().is_some_and(char::is_ascii_punctuation) => {
                result.push(c);
                result.extend(chars.next());
            }
            '[' | ']' => {
                result.push('\\');
                result.push(c);
            }
            _ => result.push(c),
        }
    }
    result
}

fn escape_label(s: &str) -> String {
    if !s.bytes().any(|b| matches!(b, b'\\' | b'[' | b']')) {
        return s.to_string();
    }
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, '\\' | '[' | ']') {
            result.push('\\');
        }
        result.push(c);
    }
    result
}

/// The text of an autolink for a link whose only content is its own destination, when writing that
/// text as a link label would need escapes. A URL in a label is read as it is, so `\_` would stay.
fn autolink_target(url: &str, values: &[Node]) -> Option<String> {
    let [Node::Text(Text { value, .. })] = values else {
        return None;
    };
    let target = if value == url {
        url
    } else if url.strip_prefix("mailto:") == Some(value.as_str()) && crate::parser::is_autolink_email(value) {
        value.as_str()
    } else {
        return None;
    };
    let plain = !target
        .chars()
        .any(|c| c.is_whitespace() || c.is_control() || matches!(c, '<' | '>'));
    let scheme = target.split_once(':').is_some_and(|(scheme, _)| {
        (2..=32).contains(&scheme.len())
            && scheme.starts_with(|c: char| c.is_ascii_alphabetic())
            && scheme
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-'))
    });
    let email = value != url;
    (plain && (scheme || email) && escape_text(target.to_string()) != target).then(|| target.to_string())
}

/// Whether `rest`, the text after an `&`, reads as the rest of a character reference.
fn starts_reference(rest: &str) -> bool {
    let name = match rest.strip_prefix('#') {
        Some(number) => match number.strip_prefix(['x', 'X']) {
            Some(hex) => hex
                .split_once(';')
                .map(|(digits, _)| (digits, char::is_ascii_hexdigit as fn(&char) -> bool)),
            None => number
                .split_once(';')
                .map(|(digits, _)| (digits, char::is_ascii_digit as fn(&char) -> bool)),
        },
        None => rest
            .split_once(';')
            .map(|(name, _)| (name, char::is_ascii_alphanumeric as fn(&char) -> bool)),
    };
    name.is_some_and(|(name, is_valid)| !name.is_empty() && name.chars().all(|c| is_valid(&c)))
}

fn is_email_local(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '+')
}

/// Whether `rest`, the text after an `@`, starts with a domain that makes an email address of the text
/// before it.
fn is_email_domain(rest: &str) -> bool {
    let domain = rest
        .split(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_')))
        .next()
        .unwrap_or_default();
    domain
        .split_once('.')
        .is_some_and(|(label, after)| !label.is_empty() && after.starts_with(|c: char| c.is_ascii_alphanumeric()))
}

/// Re-escapes markdown-significant characters so plain text can't be reinterpreted as
/// syntax on re-parse. `# - + > =` only unsafe at line start, `. )` only after leading digits;
/// a leading `\t` and runs of 2+ `\n` (only possible via decoded entities) use `&#N;` instead.
fn escape_text(value: String) -> String {
    escape_text_with(value, false)
}

/// Whether `text` starts with the `import ` or `export ` that makes a block ESM in MDX.
fn starts_esm(text: &str) -> bool {
    text.starts_with("import ") || text.starts_with("export ")
}

/// `escape_text`, and for MDX also the braces and the `import` or `export` that starts a line.
fn escape_text_with(value: String, mdx: bool) -> String {
    // Skip the copy when nothing needs escaping; byte scan avoids UTF-8 decoding.
    let needs_escaping = value.bytes().any(|b| {
        matches!(
            b,
            b'\\'
                | b'`'
                | b'*'
                | b'_'
                | b'['
                | b']'
                | b'|'
                | b'~'
                | b'$'
                | b'#'
                | b'-'
                | b'+'
                | b'>'
                | b'<'
                | b'='
                | b'.'
                | b')'
                | b'\n'
                | b'\t'
                | b'&'
                | b'@'
        )
    });
    let mdx_special = mdx && (value.contains('{') || value.contains("import ") || value.contains("export "));
    if !needs_escaping && !mdx_special {
        return value;
    }

    let mut result = String::with_capacity(value.len());
    let mut at_line_start = true;
    // True while in an unbroken digit run since line start (ordered-list marker number).
    let mut leading_digits = false;
    let mut chars = value
        .char_indices()
        .map(|(index, c)| (c, &value[index + c.len_utf8()..]))
        .peekable();
    let mut previous = None;

    while let Some((c, rest)) = chars.next() {
        match c {
            '\n' if chars.peek().is_some_and(|(next, _)| *next == '\n') => {
                result.push_str("&#10;");
                at_line_start = true;
                leading_digits = false;
                continue;
            }
            '\t' if at_line_start => {
                result.push_str("&#9;");
                at_line_start = false;
                leading_digits = false;
                continue;
            }
            '\\' | '`' | '*' | '_' | '[' | ']' | '|' | '~' | '$' | '<' => result.push('\\'),
            '{' if mdx => result.push('\\'),
            // The first letter as a character reference keeps the line from starting an ESM block.
            'i' | 'e' if mdx && at_line_start && starts_esm(&value[value.len() - rest.len() - 1..]) => {
                result.push_str(if c == 'i' { "&#105;" } else { "&#101;" });
                at_line_start = false;
                leading_digits = false;
                previous = Some(c);
                continue;
            }
            '#' | '-' | '+' | '>' | '=' if at_line_start => result.push('\\'),
            '.' | ')' if leading_digits => result.push('\\'),
            '&' if starts_reference(rest) => result.push('\\'),
            '@' if previous.is_some_and(is_email_local) && is_email_domain(rest) => result.push('\\'),
            _ => {}
        }
        result.push(c);
        previous = Some(c);

        let is_digit = c.is_ascii_digit();
        leading_digits = if at_line_start {
            is_digit
        } else {
            leading_digits && is_digit
        };
        at_line_start = c == '\n';
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use rstest::rstest;

    #[rstest]
    #[case::heading_marker("## not a heading", "\\## not a heading")]
    #[case::heading_seven_hashes_still_escaped("####### x", "\\####### x")]
    #[case::emphasis_star("*not emphasis*", "\\*not emphasis\\*")]
    #[case::emphasis_underscore("_not emphasis_", "\\_not emphasis\\_")]
    #[case::code_span("`not code`", "\\`not code\\`")]
    #[case::link_brackets("[not a link](x)", "\\[not a link\\](x)")]
    #[case::list_marker("- not a list item", "\\- not a list item")]
    #[case::blockquote_marker("> not a quote", "\\> not a quote")]
    #[case::table_pipe("| not a table |", "\\| not a table \\|")]
    #[case::strikethrough("~not strikethrough~", "\\~not strikethrough\\~")]
    #[case::math("$not math$", "\\$not math\\$")]
    #[case::plus_list_marker("+ not a list item", "\\+ not a list item")]
    #[case::ordered_list_dot("5. not a list", "5\\. not a list")]
    #[case::ordered_list_paren("5) not a list", "5\\) not a list")]
    #[case::setext_h1_underline("Foo\n===", "Foo\n\\===")]
    #[case::hash_not_at_line_start("a#b", "a#b")]
    #[case::dash_not_at_line_start("a-b", "a-b")]
    #[case::plus_not_at_line_start("a+b", "a+b")]
    #[case::gt_not_at_line_start("a>b", "a>b")]
    #[case::eq_not_at_line_start("a=b", "a=b")]
    #[case::dot_not_after_leading_digits("a5. b", "a5. b")]
    #[case::dot_after_broken_digit_run("5x3. b", "5x3. b")]
    #[case::escapes_after_embedded_newline("a\n# b", "a\n\\# b")]
    #[case::plain_text_unchanged("plain text", "plain text")]
    #[case::literal_backslash_is_doubled("a\\b", "a\\\\b")]
    #[case::empty_string("", "")]
    #[case::bang_before_bracket_only_bracket_escaped("![bar]", "!\\[bar\\]")]
    #[case::bang_at_value_end_not_escaped("foo!", "foo!")]
    #[case::consecutive_newlines_become_entity("foo\n\nbar", "foo&#10;\nbar")]
    #[case::three_consecutive_newlines("foo\n\n\nbar", "foo&#10;&#10;\nbar")]
    #[case::single_newline_unchanged("foo\nbar", "foo\nbar")]
    #[case::leading_tab_becomes_entity("\tfoo", "&#9;foo")]
    #[case::tab_not_at_line_start_unchanged("foo\tbar", "foo\tbar")]
    #[case::angle_bracket_escaped("<br/> not a tag", "\\<br/> not a tag")]
    // `&` is deliberately left unescaped: see KNOWN_FAILURES in gfm_roundtrip_fidelity.rs (#591).
    #[case::ampersand_not_escaped("a&b", "a&b")]
    fn test_escape_text(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(escape_text(input.to_string()), expected);
    }

    #[rstest]
    #[case::plain("plain", "plain")]
    #[case::brackets("[a][b]", "\\[a\\]\\[b\\]")]
    #[case::backslash("a\\b", "a\\\\b")]
    fn test_escape_label(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(escape_label(input), expected);
    }

    #[rstest]
    #[case::no_newline_unchanged("abc", 2, "abc")]
    #[case::continuation_indented("a\nb", 2, "a\n  b")]
    #[case::empty_lines_untouched("a\n\nb", 2, "a\n\n  b")]
    #[case::first_line_never_indented("  a\nb", 3, "  a\n   b")]
    fn test_indent_continuation(#[case] content: &str, #[case] width: usize, #[case] expected: &str) {
        assert_eq!(indent_continuation(content, width), expected);
    }

    #[rstest]
    #[case::no_backticks("", 1)]
    #[case::single_run("foo`bar", 2)]
    #[case::longer_run("foo``bar", 3)]
    fn test_code_span_fence(#[case] value: &str, #[case] expected_len: usize) {
        assert_eq!(code_span_fence(value), "`".repeat(expected_len));
    }

    #[rstest]
    #[case::no_backticks_minimum_three("plain", "", 3)]
    #[case::body_run_escalates("aaa\n```", "", 4)]
    fn test_code_fence(#[case] value: &str, #[case] info: &str, #[case] expected_len: usize) {
        assert_eq!(code_fence(value, info), "`".repeat(expected_len));
    }

    #[rstest]
    #[case::info_backtick_switches_to_tilde("plain", "a`b", 3)]
    #[case::info_backtick_run_switches_to_tilde("plain", "a``b", 3)]
    #[case::body_tilde_run_escalates_within_tilde_fence("aaa\n~~~", "a`b", 4)]
    fn test_code_fence_tilde(#[case] value: &str, #[case] info: &str, #[case] expected_len: usize) {
        assert_eq!(code_fence(value, info), "~".repeat(expected_len));
    }

    proptest! {
        // Every backslash escape_text inserts precedes exactly the character it
        // protects, and pre-existing backslashes are doubled, so blindly dropping
        // each backslash and keeping the character after it must recover the
        // original string, for any input. Tabs and repeated newlines go through a
        // separate entity-based path (see test_escape_text), excluded here.
        #[test]
        fn escape_text_is_losslessly_invertible(
            s in prop::collection::vec(any::<char>(), 0..60)
                .prop_map(|cs| cs.into_iter().collect::<String>())
                .prop_filter("excludes entity-escaped tab/newline-run cases", |s| {
                    !s.contains('\t') && !s.contains("\n\n")
                })
        ) {
            let escaped = escape_text(s.clone());
            prop_assert_eq!(naive_unescape(&escaped), s);
        }

        // Rendering must never panic, whatever the heading text (arbitrary Unicode,
        // possibly ending in a run of `#`) — this is a UTF-8 char-boundary regression
        // guard for the trailing-`#` escape logic.
        #[test]
        fn heading_render_never_panics_on_unicode_text(
            body in prop::collection::vec(any::<char>(), 0..20).prop_map(|cs| cs.into_iter().collect::<String>()),
            hashes in 0usize..5,
            depth in 1u8..=6,
        ) {
            let value = format!("{}{}", body, "#".repeat(hashes));
            let node = Node::Heading(Heading { depth: HeadingDepth::saturating(i64::from(depth)), values: vec![value.into()], position: None });
            let _ = node.to_string_with(&RenderOptions::default());
        }

        #[test]
        fn escape_label_is_losslessly_invertible(
            s in prop::collection::vec(any::<char>(), 0..40).prop_map(|cs| cs.into_iter().collect::<String>())
        ) {
            let escaped = escape_label(&s);
            prop_assert_eq!(naive_unescape(&escaped), s);
        }

        // A Link's destination, rendered through the real writer and re-parsed by the
        // real markdown parser, must recover the original URL string — the concrete
        // bug class from the GFM spec-suite fidelity check (spaces/parens/angle
        // brackets silently corrupting or breaking the link on round-trip).
        #[test]
        fn link_url_round_trips_through_parser(
            url in prop::collection::vec(
                prop::sample::select(vec!['a', 'b', '1', ' ', '(', ')', '<', '>', '\\']),
                1..12,
            ).prop_map(|cs| cs.into_iter().collect::<String>())
        ) {
            let doc = format!("[text]({})\n", render_link_destination(&url, &UrlSurroundStyle::None));
            let parsed = crate::Markdown::from_markdown_str(&doc).unwrap();
            let link = parsed.nodes.iter().find_map(|n| match n {
                Node::Link(l) => Some(l.url.as_str().to_string()),
                _ => None,
            });
            prop_assert_eq!(link, Some(url));
        }

    }

    fn naive_unescape(s: &str) -> String {
        let mut result = String::with_capacity(s.len());
        let mut chars = s.chars();
        while let Some(c) = chars.next() {
            if c == '\\'
                && let Some(next) = chars.next()
            {
                result.push(next);
                continue;
            }
            result.push(c);
        }
        result
    }
}
