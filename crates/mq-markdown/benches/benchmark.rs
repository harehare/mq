fn main() {
    divan::main();
}

// Build 100-paragraph document with wikilinks in each paragraph.
#[cfg(feature = "wikilink")]
fn wikilink_doc() -> String {
    (0..100)
        .map(|i| format!("# Heading {i}\n\nText with [[target{i}]] and [[another{i}|Display {i}]].\n\n"))
        .collect()
}

// Same shape but no [[...]] patterns.
#[cfg(feature = "wikilink")]
fn plain_doc() -> String {
    (0..100)
        .map(|i| format!("# Heading {i}\n\nText without any wikilink patterns here.\n\n"))
        .collect()
}

// 100 callouts of various kinds.
#[cfg(feature = "callout")]
fn callout_doc() -> String {
    (0..100)
        .map(|i| {
            let kind = ["NOTE", "WARNING", "TIP", "INFO"][i % 4];
            format!("> [!{kind}] Title {i}\n> Body text for callout {i}.\n\n")
        })
        .collect()
}

// 100 plain blockquotes (no callout header).
#[cfg(feature = "callout")]
fn plain_blockquote_doc() -> String {
    (0..100)
        .map(|i| format!("> Plain blockquote {i} without any callout header.\n\n"))
        .collect()
}

// 100 embeds spread through paragraphs.
#[cfg(feature = "embed")]
fn embed_doc() -> String {
    (0..100)
        .map(|i| format!("# Heading {i}\n\nSee ![[note{i}.md]] for details.\n\n"))
        .collect()
}

fn table_doc() -> String {
    let rows = (0..100)
        .map(|i| format!("| Item {i} | {} | テーブル {i} |\n", i * 10))
        .collect::<String>();
    format!("| Name | Count | Label |\n| :--- | ---: | :-: |\n{rows}")
}

/// Measures Markdown table rendering separately from parsing. The renderer first scans cells
/// for display widths, then writes the aligned table into its final output buffer.
#[divan::bench(name = "render/table_100x3")]
fn render_table(bencher: divan::Bencher) {
    let markdown = mq_markdown::Markdown::from_markdown_str(&table_doc()).unwrap();
    bencher.bench(|| markdown.to_string());
}

// Full end-to-end parse of a document with wikilinks.
#[cfg(feature = "wikilink")]
#[divan::bench(name = "from_markdown_str/with_wikilinks")]
fn from_markdown_str_with_wikilinks() -> mq_markdown::Markdown {
    mq_markdown::Markdown::from_markdown_str(&wikilink_doc()).unwrap()
}

// Full end-to-end parse of a document without wikilinks.
#[cfg(feature = "wikilink")]
#[divan::bench(name = "from_markdown_str/without_wikilinks")]
fn from_markdown_str_without_wikilinks() -> mq_markdown::Markdown {
    mq_markdown::Markdown::from_markdown_str(&plain_doc()).unwrap()
}

// Callout: end-to-end parse with callout headers (mdast + try_parse_callout per blockquote).
#[cfg(feature = "callout")]
#[divan::bench(name = "from_markdown_str/with_callouts")]
fn from_markdown_str_with_callouts() -> mq_markdown::Markdown {
    mq_markdown::Markdown::from_markdown_str(&callout_doc()).unwrap()
}

// Callout: end-to-end parse with plain blockquotes — measures overhead of callout check on non-callout docs.
#[cfg(feature = "callout")]
#[divan::bench(name = "from_markdown_str/plain_blockquotes")]
fn from_markdown_str_plain_blockquotes() -> mq_markdown::Markdown {
    mq_markdown::Markdown::from_markdown_str(&plain_blockquote_doc()).unwrap()
}

// Embed: end-to-end parse with embeds.
#[cfg(feature = "embed")]
#[divan::bench(name = "from_markdown_str/with_embeds")]
fn from_markdown_str_with_embeds() -> mq_markdown::Markdown {
    mq_markdown::Markdown::from_markdown_str(&embed_doc()).unwrap()
}

// Real-world documents: the README and the book sources, concatenated.
fn real_doc() -> String {
    fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|ext| ext == "md") {
                out.push(path);
            }
        }
    }

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut paths = vec![root.join("README.md")];
    walk(&root.join("docs/books/src"), &mut paths);
    paths.sort();
    paths
        .iter()
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[divan::bench(name = "from_markdown_str/real_docs")]
fn from_markdown_str_real_docs(bencher: divan::Bencher) {
    let doc = real_doc();
    bencher
        .counter(divan::counter::BytesCount::of_str(&doc))
        .bench(|| mq_markdown::Markdown::from_markdown_str(&doc).unwrap());
}

// Many short lines, to stress line splitting.
#[divan::bench(name = "from_markdown_str/many_lines")]
fn from_markdown_str_many_lines(bencher: divan::Bencher) {
    let doc = "line of plain text\n".repeat(20_000);
    bencher
        .counter(divan::counter::BytesCount::of_str(&doc))
        .bench(|| mq_markdown::Markdown::from_markdown_str(&doc).unwrap());
}
