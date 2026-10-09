//! Property-based tests for the parser and the HTML renderer. They state what has to hold whatever the
//! document is, instead of what one output looks like: documents that are put together from blocks
//! parse as their blocks, line endings and containers change nothing else, other characters in place of
//! letters do not change the structure, every position is on a character boundary, and the HTML is
//! well-formed with safe URLs.

use mq_markdown::{Markdown, Node, ParseOptions, to_html};
use proptest::prelude::*;

fn parse(input: &str) -> Vec<Node> {
    input.parse::<Markdown>().unwrap().nodes
}

/// The kinds of the nodes with their nesting, without any value.
fn shape(nodes: &[Node]) -> String {
    fn one(node: &Node) -> String {
        let children = node.children();
        if children.is_empty() {
            node.name().to_string()
        } else {
            format!(
                "{}({})",
                node.name(),
                children.iter().map(one).collect::<Vec<_>>().join(",")
            )
        }
    }
    nodes.iter().map(one).collect::<Vec<_>>().join(" ")
}

/// Blocks that mean the same wherever they are and never run into the ones around them.
const BLOCKS: &[&str] = &[
    "a",
    "a\nb",
    "a  \nb",
    "a\\\nb",
    "# h",
    "## h ##",
    "a\n===",
    "a\n---",
    "***",
    "> q",
    "> a\n> b",
    "> > n",
    "```\ncode\n```",
    "```rust x\ncode\n\ncode\n```",
    "~~~\n~\n~~~",
    "    indented",
    "$$\nm\n$$",
    "<div>\nx\n</div>",
    "<!-- c -->",
    "| a | b |\n|:-|-:|\n| c | d |",
    "a | b\n-|-\nc | d",
    "*e* **s** `c` ~~d~~ $m$ [l](u \"t\") ![i](u) <http://a.b> www.a.com a@b.com &amp; \\*",
    "[r]\n\n[r]: /u",
    "あい\nう",
    "# 見出し",
    "> 引用\n> つづき",
    "```あ\n日本語\n```",
    "| あ | 🎉 |\n|-|-|\n| é | ß |",
    "*あ*。**い**です",
    "[あ](http://い.jp/う)",
    "http://あ.jp/パス あ@い.com www.う.com",
    "👨\u{200d}👩\u{200d}👧 e\u{301} ß ǅ",
];

fn block() -> impl Strategy<Value = &'static str> {
    prop::sample::select(BLOCKS)
}

/// Blocks put together with blank lines.
fn document() -> impl Strategy<Value = Vec<&'static str>> {
    prop::collection::vec(block(), 1..6)
}

/// Blocks without a footnote or a definition, whose HTML is only the HTML of its blocks.
fn plain_block() -> impl Strategy<Value = &'static str> {
    // Indented code that follows indented code is one block.
    block().prop_filter("definition or indented code", |block| {
        !block.contains("]:") && !block.starts_with("    ")
    })
}

/// What counts for a position: the columns are bytes, the lines end at `\n`, `\r\n` or `\r`.
fn byte_offset(input: &str, line: usize, column: usize) -> Option<usize> {
    let bytes = input.as_bytes();
    let mut start = 0;
    let mut current = 1;
    let mut index = 0;
    while current < line {
        match bytes.get(index)? {
            b'\r' if bytes.get(index + 1) == Some(&b'\n') => index += 2,
            b'\n' | b'\r' => index += 1,
            _ => {
                index += 1;
                continue;
            }
        }
        current += 1;
        start = index;
    }
    Some(start + column - 1)
}

fn walk(nodes: Vec<Node>) -> Vec<Node> {
    let mut all = Vec::new();
    let mut stack = nodes;
    while let Some(node) = stack.pop() {
        stack.extend(node.children());
        all.push(node);
    }
    all
}

proptest! {
    #[test]
    fn blocks_separated_by_blank_lines_parse_as_their_blocks(blocks in prop::collection::vec(plain_block(), 1..6)) {
        let joined = parse(&blocks.join("\n\n"));
        let separate = blocks.iter().flat_map(|block| parse(block)).collect::<Vec<_>>();
        prop_assert_eq!(shape(&joined), shape(&separate));
        prop_assert_eq!(
            joined.iter().map(Node::value).collect::<Vec<_>>(),
            separate.iter().map(Node::value).collect::<Vec<_>>()
        );
    }

    #[test]
    fn html_of_blocks_separated_by_blank_lines_is_the_html_of_the_blocks(
        blocks in prop::collection::vec(plain_block(), 1..6)
    ) {
        let joined = to_html(&format!("{}\n", blocks.join("\n\n")));
        let separate = blocks.iter().map(|block| to_html(&format!("{block}\n"))).collect::<String>();
        prop_assert_eq!(joined, separate);
    }

    #[test]
    fn lines_of_a_block_are_after_the_lines_of_the_blocks_before_it(blocks in document()) {
        let source = blocks.join("\n\n");
        let mut last = 0;
        for node in parse(&source) {
            let line = node.position().map_or(last, |position| position.start.line);
            prop_assert!(line >= last, "{source:?}");
            last = line;
        }
    }

    #[test]
    fn crlf_documents_have_the_nodes_of_lf_documents(blocks in document()) {
        let lf = blocks.join("\n\n") + "\n";
        let crlf = lf.replace('\n', "\r\n");
        prop_assert_eq!(shape(&parse(&crlf)), shape(&parse(&lf)));
        let (lf_nodes, crlf_nodes) = (parse(&lf), parse(&crlf));
        for (a, b) in lf_nodes.iter().zip(&crlf_nodes) {
            prop_assert_eq!(a.position().map(|p| (p.start.line, p.end.line)), b.position().map(|p| (p.start.line, p.end.line)));
        }
    }

    #[test]
    fn crlf_documents_have_the_html_of_lf_documents_with_crlf(blocks in document()) {
        let lf = blocks.join("\n\n") + "\n";
        let crlf = lf.replace('\n', "\r\n");
        prop_assert_eq!(to_html(&crlf), to_html(&lf).replace('\n', "\r\n"));
    }

    #[test]
    fn cr_documents_have_the_nodes_of_lf_documents(blocks in document()) {
        let lf = blocks.join("\n\n") + "\n";
        let cr = lf.replace('\n', "\r");
        prop_assert_eq!(shape(&parse(&cr)), shape(&parse(&lf)));
    }

    #[test]
    fn a_block_quote_wraps_the_html_of_its_content(blocks in prop::collection::vec(plain_block(), 1..5)) {
        let content = blocks.join("\n\n") + "\n";
        let quoted = content
            .lines()
            .map(|line| if line.is_empty() { ">".to_string() } else { format!("> {line}") })
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        let html = to_html(&content);
        prop_assert_eq!(to_html(&quoted), format!("<blockquote>\n{html}</blockquote>\n"), "{:?}", quoted);
    }

    #[test]
    fn a_final_line_ending_changes_nothing_but_the_end_of_the_html(blocks in document()) {
        let without = blocks.join("\n\n");
        let with = format!("{without}\n");
        prop_assert_eq!(shape(&parse(&without)), shape(&parse(&with)));
        let (a, b) = (to_html(&without), to_html(&with));
        prop_assert_eq!(a.trim_end(), b.trim_end());
    }

    #[test]
    fn positions_are_on_character_boundaries(blocks in document()) {
        let source = blocks.join("\n\n") + "\n";
        for node in walk(parse(&source)) {
            let Some(position) = node.position() else { continue };
            let start = byte_offset(&source, position.start.line, position.start.column);
            let end = byte_offset(&source, position.end.line, position.end.column);
            prop_assert!(start.is_some() && end.is_some(), "{node:?}");
            let (start, end) = (start.unwrap(), end.unwrap());
            prop_assert!(start <= end && end <= source.len(), "{node:?} in {source:?}");
            prop_assert!(
                source.is_char_boundary(start) && source.is_char_boundary(end),
                "{node:?} is not on character boundaries in {source:?}"
            );
        }
    }

    #[test]
    fn every_prefix_of_a_document_parses(blocks in document()) {
        let source = blocks.join("\n\n") + "\n";
        for end in (0..=source.len()).filter(|end| source.is_char_boundary(*end)) {
            let prefix = &source[..end];
            parse(prefix);
            to_html(prefix);
        }
    }

    #[test]
    fn every_suffix_of_a_document_parses(blocks in document()) {
        let source = blocks.join("\n\n") + "\n";
        for start in (0..=source.len()).filter(|start| source.is_char_boundary(*start)) {
            let suffix = &source[start..];
            parse(suffix);
            to_html(suffix);
        }
    }

    #[test]
    fn rendering_a_document_and_parsing_it_again_keeps_its_shape(blocks in document()) {
        let source = blocks.join("\n\n") + "\n";
        let once = source.parse::<Markdown>().unwrap().to_string();
        let twice = once.parse::<Markdown>().unwrap().to_string();
        prop_assert_eq!(once, twice, "{:?}", source);
    }

    #[test]
    fn html_of_a_rendered_document_is_the_html_of_the_document(blocks in prop::collection::vec(plain_block(), 1..5)) {
        let source = blocks.join("\n\n") + "\n";
        let rendered = source.parse::<Markdown>().unwrap().to_string();
        prop_assert_eq!(to_html(&rendered), to_html(&source), "{:?}", rendered);
    }
}

/// Words of letters, and what to put in their place.
fn words() -> impl Strategy<Value = Vec<String>> {
    prop::collection::vec("[a-f]{1,4}", 1..5)
}

fn replace_letters(text: &str, replacements: &[char; 6]) -> String {
    text.chars()
        .map(|char| match char {
            'a'..='f' => replacements[(char as u8 - b'a') as usize],
            _ => char,
        })
        .collect()
}

/// Markup around words: the same markup with other characters for the letters has the same structure.
fn markup() -> impl Strategy<Value = String> {
    let wrap = prop::sample::select(vec![
        "{}",
        "*{}*",
        "**{}**",
        "_{}_",
        "~~{}~~",
        "`{}`",
        "[{}](u)",
        "![{}](u)",
        "# {}",
        "> {}",
        "- {}",
        "1. {}",
        "{}  \n{}",
        "| {} |\n|-|\n| {} |",
        "{}[^n]\n\n[^n]: {}",
        "<s>{}</s>",
        "`` {} ``",
        "$ {} $",
        "[{}][r]\n\n[r]: /u",
    ]);
    (words(), wrap).prop_map(|(words, wrap)| {
        let mut words = words.into_iter().cycle();
        let mut result = String::new();
        let mut rest = wrap;
        while let Some(index) = rest.find("{}") {
            result.push_str(&rest[..index]);
            result.push_str(&words.next().unwrap());
            rest = &rest[index + 2..];
        }
        result.push_str(rest);
        result
    })
}

proptest! {
    #[test]
    fn other_characters_in_place_of_letters_keep_the_structure(
        source in markup(),
        replacements in prop::sample::select(vec![
            ['あ', 'い', 'う', 'え', 'お', 'か'],
            ['é', 'ß', 'ñ', 'ø', 'ü', 'ç'],
            ['🎉', '🚀', '🔥', '✨', '🌟', '💡'],
            ['日', '本', '語', '文', '字', '列'],
            ['ａ', 'ｂ', 'ｃ', 'ｄ', 'ｅ', 'ｆ'],
            ['א', 'ب', 'क', 'ก', '한', 'ა'],
        ])
    ) {
        let replaced = replace_letters(&source, &replacements);
        prop_assert_eq!(shape(&parse(&replaced)), shape(&parse(&source)), "{:?} -> {:?}", source, replaced);
        // The tags of the HTML are the same too.
        prop_assert_eq!(tags(&to_html(&replaced)), tags(&to_html(&source)), "{:?} -> {:?}", source, replaced);
    }

    #[test]
    fn columns_count_bytes_of_the_characters_before(
        source in markup(),
        replacement in prop::sample::select(vec!['あ', 'é', '🎉', 'ａ'])
    ) {
        let replaced = replace_letters(&source, &[replacement; 6]);
        let lines = replaced.matches('\n').count() + 1;
        for node in walk(parse(&replaced)) {
            let Some(position) = node.position() else { continue };
            prop_assert!(position.end.line <= lines, "{node:?}");
            let line = replaced.split('\n').nth(position.end.line - 1).unwrap_or_default();
            // A column can be one past the end of its line, and no further.
            prop_assert!(position.end.column <= line.len() + 1, "{node:?} in {replaced:?}");
        }
    }
}

/// One HTML tag that the output of the renderer can have, after its name.
fn tags(html: &str) -> Vec<(bool, String)> {
    let mut result = Vec::new();
    let mut rest = html;
    while let Some(index) = rest.find('<') {
        rest = &rest[index + 1..];
        let end = rest.find('>').expect("a tag that is not closed");
        let tag = &rest[..end];
        // Comments and the like are no tags.
        if tag.starts_with(['!', '?']) {
            rest = &rest[end + 1..];
            continue;
        }
        let closing = tag.starts_with('/');
        let name = tag
            .trim_start_matches('/')
            .split([' ', '/'])
            .next()
            .unwrap()
            .to_string();
        let void = tag.ends_with('/') || matches!(name.as_str(), "br" | "hr" | "img" | "input");
        if !void {
            result.push((closing, name));
        }
        rest = &rest[end + 1..];
    }
    result
}

fn assert_well_formed(html: &str) -> Result<(), TestCaseError> {
    let mut open: Vec<String> = Vec::new();
    for (closing, name) in tags(html) {
        if closing {
            prop_assert_eq!(open.pop(), Some(name.clone()), "{} in {}", name, html);
        } else {
            match name.as_str() {
                // These cannot be inside of themselves.
                "a" | "p" => prop_assert!(!open.contains(&name), "nested <{}> in {}", name, html),
                "li" => prop_assert!(matches!(open.last().map(String::as_str), Some("ul" | "ol")), "{}", html),
                "td" | "th" => prop_assert_eq!(open.last().map(String::as_str), Some("tr"), "{}", html),
                _ => {}
            }
            open.push(name);
        }
    }
    prop_assert!(open.is_empty(), "unclosed {open:?} in {html}");
    Ok(())
}

proptest! {
    #[test]
    fn html_of_a_document_is_well_formed(blocks in document()) {
        let source = blocks.join("\n\n") + "\n";
        assert_well_formed(&to_html(&source))?;
    }

    #[test]
    fn html_of_nested_blocks_is_well_formed(
        blocks in prop::collection::vec(plain_block(), 1..4),
        // The prefix of the first line, and the one of the lines that follow.
        (first, rest) in prop::sample::select(vec![
            ("> ", "> "),
            ("- ", "  "),
            ("1. ", "   "),
            ("> - ", ">   "),
            ("- > ", "  > "),
            ("- - ", "    "),
        ])
    ) {
        let source = blocks
            .join("\n\n")
            .lines()
            .enumerate()
            .map(|(index, line)| match (index, line.is_empty()) {
                (0, _) => format!("{first}{line}"),
                (_, true) => rest.trim_end().to_string(),
                _ => format!("{rest}{line}"),
            })
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        assert_well_formed(&to_html(&source))?;
    }

    #[test]
    // Raw HTML is passed through as it is, so it is left out.
    fn html_of_arbitrary_text_is_well_formed(source in "[^<\\p{C}]{0,60}") {
        assert_well_formed(&to_html(&source))?;
    }
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

proptest! {
    /// Text without markup is a paragraph with the text escaped.
    #[test]
    fn plain_text_is_an_escaped_paragraph(
        words in prop::collection::vec("[a-zA-Z0-9あ-んé🎉,;'\"]{1,8}", 1..6)
    ) {
        let text = words.join(" ");
        prop_assert_eq!(to_html(&text), format!("<p>{}</p>", escape(&text)));
    }

    #[test]
    fn special_characters_in_text_are_escaped(
        words in prop::collection::vec(("[a-z]{1,4}", prop::sample::select(vec!["<", ">", "&", "\"", "&&", "<>"])), 1..5)
    ) {
        // A space after each keeps `<` from starting a tag and `&` from starting a reference.
        let text = words.iter().map(|(word, special)| format!("{word}{special} ")).collect::<String>();
        let html = to_html(&text);
        let inner = html.strip_prefix("<p>").and_then(|html| html.strip_suffix("</p>")).expect("a paragraph");
        prop_assert!(!inner.contains(['<', '>', '"']), "{}", html);
        let after_ampersand = inner.split('&').skip(1).all(|rest| {
            ["amp;", "lt;", "gt;", "quot;"].iter().any(|entity| rest.starts_with(entity))
        });
        prop_assert!(after_ampersand, "{}", html);
        // What the HTML says is the text that was written.
        let decoded = inner.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&amp;", "&");
        prop_assert_eq!(decoded, text.trim_end());
    }

    /// The `href` of a link has the characters of a URL and nothing that could end the attribute, and
    /// has no protocol that is not safe.
    #[test]
    fn href_is_safe(
        scheme in prop::sample::select(vec!["", "http:", "https:", "mailto:", "javascript:", "JAVASCRIPT:", "data:", "vbscript:", "file:", "ftp:", "x-y:"]),
        path in "[a-zA-Z0-9/._~%?#=&:;,+@!$*'あé🎉 ]{0,12}"
    ) {
        let url = format!("{scheme}{path}");
        let html = to_html(&format!("[a](<{url}>)"));
        let href = html
            .strip_prefix("<p><a href=\"")
            .and_then(|rest| rest.split('"').next())
            .expect("a link");
        prop_assert!(
            href.chars().all(|char| char.is_ascii() && !char.is_ascii_control() && !matches!(char, ' ' | '"' | '<' | '>' | '\\' | '`' | '{' | '}' | '|' | '^' | '[' | ']')),
            "{href:?} from {url:?}"
        );
        let lowered = href.to_ascii_lowercase();
        let protocol = lowered.split([':', '/', '?', '#']).next().filter(|_| {
            lowered.find(':').is_some_and(|colon| lowered.find(['/', '?', '#']).is_none_or(|end| colon < end))
        });
        if let Some(protocol) = protocol {
            prop_assert!(["http", "https", "irc", "ircs", "mailto", "xmpp"].contains(&protocol), "{href:?}");
        }
    }

    #[test]
    fn image_source_only_has_http_protocols(
        scheme in prop::sample::select(vec!["", "http:", "https:", "mailto:", "javascript:", "data:", "irc:"]),
        path in "[a-z/.]{0,8}"
    ) {
        let html = to_html(&format!("![a]({scheme}{path})"));
        let src = html
            .strip_prefix("<p><img src=\"")
            .and_then(|rest| rest.split('"').next())
            .expect("an image");
        if !src.is_empty() {
            let lowered = src.to_ascii_lowercase();
            let has_protocol = lowered.find(':').is_some_and(|colon| lowered.find(['/', '?', '#']).is_none_or(|end| colon < end));
            prop_assert!(!has_protocol || lowered.starts_with("http:") || lowered.starts_with("https:"), "{src:?}");
        }
    }

    #[test]
    fn code_blocks_keep_their_text(
        lines in prop::collection::vec("[a-zA-Z0-9 あé🎉<>&\"']{0,10}", 1..5),
        fence in prop::sample::select(vec!["```", "~~~", "````"])
    ) {
        prop_assume!(lines.iter().all(|line| !line.trim_start().starts_with(['`', '~'])));
        let source = format!("{fence}\n{}\n{fence}\n", lines.join("\n"));
        let html = to_html(&source);
        let expected = format!("<pre><code>{}\n</code></pre>\n", escape(&lines.join("\n")));
        prop_assert_eq!(html, expected);
    }

    #[test]
    fn code_span_text_is_kept_and_escaped(text in "[a-zA-Z0-9あé🎉<>&\"']{1,10}") {
        prop_assert_eq!(to_html(&format!("`{text}`")), format!("<p><code>{}</code></p>", escape(&text)));
    }

    #[test]
    fn heading_depth_is_the_number_of_hashes(depth in 1usize..=6, text in "[a-zあé🎉]{1,8}") {
        prop_assert_eq!(
            to_html(&format!("{} {text}\n", "#".repeat(depth))),
            format!("<h{depth}>{text}</h{depth}>\n")
        );
    }

    #[test]
    fn list_has_an_item_for_each_line(count in 1usize..8, ordered in any::<bool>(), start in 0u32..20) {
        let source = (0..count)
            .map(|index| if ordered { format!("{}. item", start + index as u32) } else { "- item".to_string() })
            .collect::<Vec<_>>()
            .join("\n");
        let html = to_html(&source);
        prop_assert_eq!(html.matches("<li>").count(), count);
        if ordered && start != 1 {
            let expected_start = format!("<ol start=\"{start}\">");
            prop_assert!(html.starts_with(&expected_start), "{}", html);
        }
    }

    #[test]
    fn table_has_a_cell_for_each_column_of_each_row(columns in 1usize..5, rows in 0usize..4) {
        let row = |text: &str| format!("| {} |", vec![text; columns].join(" | "));
        let mut lines = vec![row("h"), format!("|{}", "-|".repeat(columns))];
        lines.extend((0..rows).map(|_| row("c")));
        let html = to_html(&(lines.join("\n") + "\n"));
        prop_assert_eq!(html.matches("<th>").count(), columns);
        prop_assert_eq!(html.matches("<td>").count(), columns * rows);
        prop_assert_eq!(html.contains("<tbody>"), rows > 0);
    }

    #[test]
    fn footnotes_are_numbered_from_one_in_the_order_of_their_references(order in Just(vec![0usize, 1, 2, 3]).prop_shuffle()) {
        let references = order.iter().map(|id| format!("r[^n{id}]")).collect::<Vec<_>>().join(" ");
        let definitions = (0..4).map(|id| format!("[^n{id}]: d{id}")).collect::<Vec<_>>().join("\n");
        let html = to_html(&format!("{references}\n\n{definitions}\n"));
        for (number, id) in order.iter().enumerate() {
            let call = format!("href=\"#user-content-fn-n{id}\" id=\"user-content-fnref-n{id}\" data-footnote-ref=\"\" aria-describedby=\"footnote-label\">{}</a>", number + 1);
            prop_assert!(html.contains(&call), "{html}");
        }
        let positions = order.iter().map(|id| html.find(&format!("<li id=\"user-content-fn-n{id}\">")).unwrap()).collect::<Vec<_>>();
        prop_assert!(positions.windows(2).all(|pair| pair[0] < pair[1]), "{html}");
    }
}

/// Lines that start, continue and end the blocks that decide whether a paragraph is open. Footnote
/// definitions are left out: a quote hoists them, so their text is not found in the same place.
/// Definitions are left out too: a quote does not know that a paragraph is only definitions, so it reads a line of `=`
/// under one as a setext underline, where outside a quote it is text. Indented code and an
/// empty item are left out for the same reason as blank lines after an item.
const LAZY_LINES: &[&str] = &[
    "a", "b c", "", "# h", "***", "---", "===", "> q", "- i", "1. i", "2. i", "  more", "```", "~~~", "$$", "<div>",
    "</div>", "<!-- c", "-->", "<?x", "?>", "<p>", "<span>",
];

/// The lines of MDX that start, continue and end flow content. A fence is left out of the lines with
/// them: it is text inside an expression that spans lines, which a quote cannot tell line by line.
const LAZY_MDX_LINES: &[&str] = &[
    "<A>",
    "</A>",
    "<A />",
    "<A",
    "b>",
    "<A b={",
    "}>",
    "{x}",
    "{",
    "{x} <B />",
    "<A> t",
    "t {x}",
    "<a b='",
];

/// Whether the text `lazy` has been joined to the paragraph before it.
fn lazy_is_joined(nodes: Vec<Node>) -> bool {
    walk(nodes)
        .iter()
        .any(|node| matches!(node, Node::Text(text) if text.value.contains("\nlazy")))
}

fn quoted(lines: &[&str]) -> String {
    lines
        .iter()
        .map(|line| format!(">{}{line}", if line.is_empty() { "" } else { " " }))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Block parsing tracks which paragraph is open in a quote on its own, to tell lazy continuation. It has
/// to decide as parsing the lines without the quote does.
fn lazy_line_agrees(lines: &[&str], parse: impl Fn(&str) -> Option<Vec<Node>>) -> Result<(), TestCaseError> {
    if lines.last().is_none_or(|line| line.is_empty()) {
        return Ok(());
    }
    // A quote does not track the indentation of the items that are open, so it reads a line that an
    // item holds after a blank line as code, or as text where the item has a setext underline.
    let first_item = lines
        .iter()
        .position(|line| line.starts_with("- ") || line.starts_with(char::is_numeric));
    if first_item.is_some_and(|item| lines[item..].contains(&"")) {
        return Ok(());
    }
    let plain = format!("{}\nlazy", lines.join("\n"));
    let in_quote = format!("{}\nlazy", quoted(lines));
    // MDX does not allow a lazy line inside flow content, which is an error only in a quote.
    let (Some(in_quote_nodes), Some(plain_nodes)) = (parse(&in_quote), parse(&plain)) else {
        return Ok(());
    };
    prop_assert_eq!(
        lazy_is_joined(in_quote_nodes),
        lazy_is_joined(plain_nodes),
        "{:?} against {:?}",
        in_quote,
        plain
    );
    Ok(())
}

proptest! {
    #[test]
    fn lazy_continuation_in_a_quote_agrees_with_a_paragraph_outside(
        lines in prop::collection::vec(prop::sample::select(LAZY_LINES), 1..6)
    ) {
        lazy_line_agrees(&lines, |input| {
            Markdown::from_markdown_str_with(input, ParseOptions::default().with_frontmatter(false)).ok().map(|md| md.nodes)
        })?;
    }

    #[test]
    fn lazy_continuation_in_a_quote_agrees_with_a_paragraph_outside_in_mdx(
        lines in prop::collection::vec(
            prop_oneof![
                prop::sample::select(LAZY_LINES).prop_filter("a fence", |line| !matches!(*line, "```" | "~~~" | "$$")),
                prop::sample::select(LAZY_MDX_LINES)
            ],
            1..6
        )
    ) {
        lazy_line_agrees(&lines, |input| {
            Markdown::from_mdx_str_with(input, ParseOptions::default().with_frontmatter(false)).ok().map(|md| md.nodes)
        })?;
    }
}
