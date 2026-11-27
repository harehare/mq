//! Markdown documents generated as a node tree, then rendered to source.
//!
//! The tree always renders to nested, well-formed blocks, so the fuzzer reaches list, quote and
//! table nesting that token soup rarely builds, and the round trip below has a document to check.

use arbitrary::{Arbitrary, Result, Unstructured};

const MAX_DEPTH: u32 = 4;
const MAX_WIDTH: usize = 4;
/// Characters for prose. Markup and punctuation are left out: how the writer escapes them depends on the
/// neighbouring text, so the round trip below could not tell a bug from a different valid output.
const TEXT_CHARS: &[char] = &['a', 'b', 'Z', '1', ' ', ' ', '日'];
/// Characters for code, where markup is literal.
const CODE_CHARS: &[char] = &[
    'a', ' ', '*', '_', '[', ']', '(', ')', '<', '>', '#', '|', '\\', '&', '~', '!', '$',
];

/// Prose never starts or ends with a space, which would stop a surrounding `*` or `~~` from opening.
fn text(u: &mut Unstructured) -> Result<String> {
    let len = u.int_in_range(1..=8)?;
    let text: String = (0..len).map(|_| Ok(*u.choose(TEXT_CHARS)?)).collect::<Result<_>>()?;
    Ok(match text.trim() {
        "" => "a".to_string(),
        trimmed => trimmed.to_string(),
    })
}

fn code(u: &mut Unstructured) -> Result<String> {
    let len = u.int_in_range(1..=8)?;
    (0..len).map(|_| Ok(*u.choose(CODE_CHARS)?)).collect()
}

#[derive(Debug, Clone)]
enum Inline {
    Text(String),
    Emphasis(Vec<Inline>),
    Strong(Vec<Inline>),
    Delete(Vec<Inline>),
    Code(String),
    Link(Vec<Inline>, String),
    Image(String, String),
    Break,
}

impl Inline {
    fn generate(u: &mut Unstructured, depth: u32) -> Result<Self> {
        // Nested emphasis can be read in several ways (`*a*b*c*`), so wrappers hold only leaves, with
        // text on both sides to keep delimiters apart.
        let list = |u: &mut Unstructured| {
            let mut items = vec![Inline::Text(text(u)?)];
            if u.arbitrary()? {
                items.push(if u.arbitrary()? {
                    Inline::Code(code(u)?)
                } else {
                    Inline::Image(text(u)?, text(u)?)
                });
            }
            items.push(Inline::Text(text(u)?));
            Ok::<_, arbitrary::Error>(items)
        };
        if depth >= MAX_DEPTH {
            return Ok(Inline::Text(text(u)?));
        }
        Ok(match u.int_in_range(0..=7)? {
            0 | 1 => Inline::Text(text(u)?),
            2 => Inline::Emphasis(list(u)?),
            3 => Inline::Strong(list(u)?),
            4 => Inline::Delete(list(u)?),
            5 => Inline::Code(code(u)?),
            6 => Inline::Link(list(u)?, text(u)?),
            _ => Inline::Image(text(u)?, text(u)?),
        })
    }

    fn render(&self, out: &mut String) {
        let wrap = |out: &mut String, mark: &str, children: &[Inline]| {
            out.push_str(mark);
            render_inlines(children, out);
            out.push_str(mark);
        };
        match self {
            Inline::Text(t) => out.push_str(t),
            Inline::Emphasis(c) => wrap(out, "*", c),
            Inline::Strong(c) => wrap(out, "**", c),
            Inline::Delete(c) => wrap(out, "~~", c),
            Inline::Code(t) => {
                out.push('`');
                out.push_str(&t.replace('`', "'"));
                out.push('`');
            }
            Inline::Link(c, url) => {
                out.push('[');
                render_inlines(c, out);
                out.push_str("](");
                out.push_str(&url.replace([' ', '(', ')', '\n'], ""));
                out.push(')');
            }
            Inline::Image(alt, url) => {
                out.push_str("![");
                out.push_str(alt);
                out.push_str("](");
                out.push_str(&url.replace([' ', '(', ')', '\n'], ""));
                out.push(')');
            }
            Inline::Break => out.push_str("  \n"),
        }
    }
}

fn inlines(u: &mut Unstructured, depth: u32) -> Result<Vec<Inline>> {
    let len = u.int_in_range(1..=MAX_WIDTH)?;
    let mut items = Vec::with_capacity(len * 2);
    for _ in 0..len {
        items.push(Inline::generate(u, depth)?);
        // Text between siblings, so that adjacent delimiters are not read as one run.
        items.push(Inline::Text(text(u)?));
    }
    Ok(items)
}

fn render_inlines(items: &[Inline], out: &mut String) {
    for item in items {
        item.render(out);
    }
}

#[derive(Debug, Clone)]
enum Block {
    Heading(u8, Vec<Inline>),
    Paragraph(Vec<Inline>),
    Quote(Vec<Block>),
    List {
        ordered: bool,
        task: bool,
        items: Vec<Vec<Block>>,
    },
    Code(Option<String>, String),
    Table(Vec<Vec<Vec<Inline>>>),
    Rule,
    Html(String),
}

impl Block {
    fn generate(u: &mut Unstructured, depth: u32) -> Result<Self> {
        if depth >= MAX_DEPTH {
            return Ok(Block::Paragraph(inlines(u, depth)?));
        }
        Ok(match u.int_in_range(0..=9)? {
            0 => Block::Heading(u.int_in_range(1..=6)?, inlines(u, depth)?),
            1 | 2 => {
                let mut content = inlines(u, depth)?;
                if u.arbitrary()? {
                    content.extend([Inline::Text(text(u)?), Inline::Break, Inline::Text(text(u)?)]);
                }
                Block::Paragraph(content)
            }
            3 => Block::Quote(blocks(u, depth + 1)?),
            4 | 5 => {
                let len = u.int_in_range(1..=MAX_WIDTH)?;
                let items = (0..len).map(|_| blocks(u, depth + 1)).collect::<Result<_>>()?;
                Block::List {
                    ordered: u.arbitrary()?,
                    task: u.arbitrary()?,
                    items,
                }
            }
            6 => Block::Code(u.arbitrary::<bool>()?.then(|| text(u)).transpose()?, code(u)?),
            7 => {
                let columns = u.int_in_range(1..=3)?;
                let rows = u.int_in_range(1..=3)?;
                let table = (0..rows)
                    .map(|_| (0..columns).map(|_| inlines(u, depth + 1)).collect::<Result<_>>())
                    .collect::<Result<_>>()?;
                Block::Table(table)
            }
            8 => Block::Rule,
            _ => Block::Html(text(u)?),
        })
    }

    /// Renders the block, ending in a blank line.
    fn render(&self, out: &mut String) {
        match self {
            Block::Heading(level, c) => {
                out.push_str(&"#".repeat(usize::from(*level)));
                out.push(' ');
                render_inlines(c, out);
                out.push_str("\n\n");
            }
            Block::Paragraph(c) => {
                render_inlines(c, out);
                out.push_str("\n\n");
            }
            Block::Quote(children) => prefix_lines(&render_blocks(children), "> ", "> ", out),
            Block::List { ordered, task, items } => {
                for (i, item) in items.iter().enumerate() {
                    // A task marker belongs to a paragraph, so an item that starts with another block has none.
                    let task = *task && matches!(item.first(), Some(Block::Paragraph(_)));
                    let marker = match (*ordered, task) {
                        (true, _) => format!("{}. ", i + 1),
                        (false, true) => "- [ ] ".to_string(),
                        (false, false) => "- ".to_string(),
                    };
                    let indent = " ".repeat(marker.len());
                    prefix_lines(render_blocks(item).trim_end(), &marker, &indent, out);
                    out.push('\n');
                }
                out.push('\n');
            }
            Block::Code(lang, body) => {
                out.push_str("```");
                if let Some(lang) = lang {
                    out.push_str(&lang.replace(['`', ' ', '\n'], ""));
                }
                out.push('\n');
                out.push_str(&body.replace("```", "'''"));
                out.push_str("\n```\n\n");
            }
            Block::Table(rows) => {
                for (i, row) in rows.iter().enumerate() {
                    out.push('|');
                    for cell in row {
                        out.push(' ');
                        let mut rendered = String::new();
                        render_inlines(cell, &mut rendered);
                        out.push_str(&rendered.replace(['\n', '|'], " "));
                        out.push_str(" |");
                    }
                    out.push('\n');
                    if i == 0 {
                        out.push('|');
                        for _ in row {
                            out.push_str(" --- |");
                        }
                        out.push('\n');
                    }
                }
                out.push('\n');
            }
            Block::Rule => out.push_str("---\n\n"),
            Block::Html(t) => {
                out.push_str("<div>");
                out.push_str(&t.replace('\n', " "));
                out.push_str("</div>\n\n");
            }
        }
    }
}

fn blocks(u: &mut Unstructured, depth: u32) -> Result<Vec<Block>> {
    let len = u.int_in_range(1..=MAX_WIDTH)?;
    (0..len).map(|_| Block::generate(u, depth)).collect()
}

fn render_blocks(blocks: &[Block]) -> String {
    let mut out = String::new();
    for block in blocks {
        block.render(&mut out);
    }
    out
}

/// Writes `text` with `first` before its first line and `rest` before the others.
fn prefix_lines(text: &str, first: &str, rest: &str, out: &mut String) {
    for (i, line) in text.trim_end().split('\n').enumerate() {
        out.push_str(if i == 0 { first } else { rest });
        out.push_str(line);
        out.push('\n');
    }
    if first.starts_with('>') {
        out.push('\n');
    }
}

/// A Markdown document built from a node tree.
#[derive(Debug, Clone)]
pub struct Document {
    blocks: Vec<Block>,
}

impl Arbitrary<'_> for Document {
    fn arbitrary(u: &mut Unstructured<'_>) -> Result<Self> {
        Ok(Self { blocks: blocks(u, 0)? })
    }
}

impl Document {
    /// The document source.
    pub fn source(&self) -> String {
        render_blocks(&self.blocks)
    }
}

/// Reads the document, writes it, and reads the output again.
///
/// Writing must reach a fixed point after one read: the text written from a second read equals
/// the first. The generated source is well-formed, so a read error is also a finding.
pub fn check_round_trip(document: &Document) {
    use mq_markdown::Markdown;

    let source = document.source();
    let first = Markdown::from_markdown_str(&source)
        .unwrap_or_else(|err| panic!("generated Markdown is rejected: {source:?}\n{err:?}"))
        .to_string();
    let second = Markdown::from_markdown_str(&first)
        .unwrap_or_else(|err| panic!("written Markdown is rejected: {first:?}\n{err:?}"))
        .to_string();
    assert_eq!(first, second, "write is not stable for {source:?}");
}
