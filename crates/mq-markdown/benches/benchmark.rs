fn main() {
    divan::main();
}

// Build 100-paragraph document with wikilinks in each paragraph.
fn wikilink_doc() -> String {
    (0..100)
        .map(|i| format!("# Heading {i}\n\nText with [[target{i}]] and [[another{i}|Display {i}]].\n\n"))
        .collect()
}

// Same shape but no [[...]] patterns.
fn plain_doc() -> String {
    (0..100)
        .map(|i| format!("# Heading {i}\n\nText without any wikilink patterns here.\n\n"))
        .collect()
}

// Full end-to-end: mdast parse + expand_wikilinks (wikilinks present, early-exit skipped).
#[cfg(feature = "wikilink")]
#[divan::bench(name = "from_markdown_str/with_wikilinks")]
fn from_markdown_str_with_wikilinks() -> mq_markdown::Markdown {
    mq_markdown::Markdown::from_markdown_str(&wikilink_doc()).unwrap()
}

// Full end-to-end: mdast parse only (early-exit fires, expand_wikilinks skipped).
#[cfg(feature = "wikilink")]
#[divan::bench(name = "from_markdown_str/without_wikilinks")]
fn from_markdown_str_without_wikilinks() -> mq_markdown::Markdown {
    mq_markdown::Markdown::from_markdown_str(&plain_doc()).unwrap()
}

// Isolate expand_wikilinks alone (wikilinks present).
#[cfg(feature = "wikilink")]
#[divan::bench(name = "expand_wikilinks/with_wikilinks")]
fn expand_wikilinks_with(bencher: divan::Bencher) {
    let doc = wikilink_doc();
    let nodes = mq_markdown::Markdown::from_markdown_str_no_expand(&doc).unwrap();
    bencher.bench(|| mq_markdown::Node::expand_wikilinks(nodes.clone()));
}

// Isolate expand_wikilinks alone (no wikilinks — measures pure traversal cost before early-exit).
#[cfg(feature = "wikilink")]
#[divan::bench(name = "expand_wikilinks/without_wikilinks")]
fn expand_wikilinks_without(bencher: divan::Bencher) {
    let doc = plain_doc();
    let nodes = mq_markdown::Markdown::from_markdown_str_no_expand(&doc).unwrap();
    bencher.bench(|| mq_markdown::Node::expand_wikilinks(nodes.clone()));
}

// Isolate the early-exit check (contains("[[")) vs full expand_wikilinks for plain doc.
#[cfg(feature = "wikilink")]
#[divan::bench(name = "contains_check/with_wikilinks")]
fn contains_check_with() {
    let doc = wikilink_doc();
    divan::black_box(doc.contains("[["));
}

#[cfg(feature = "wikilink")]
#[divan::bench(name = "contains_check/without_wikilinks")]
fn contains_check_without() {
    let doc = plain_doc();
    divan::black_box(doc.contains("[["));
}
