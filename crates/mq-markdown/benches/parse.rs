//! Regression benchmarks for Markdown parsing and rendering.
//!
//! The document is generated so the input stays stable, and mixes the block and inline
//! constructs found in real documents.

use mq_markdown::Markdown;
use std::fmt::Write;
use std::sync::LazyLock;

// Match the allocator used by the `mq` CLI.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() {
    divan::main();
}

fn section(out: &mut String, i: usize) {
    let _ = write!(
        out,
        "## Section {i}\n\n\
         Some *emphasis*, **strong** text, `inline code`, ~~strike~~ and a [link](https://example.com/{i} \"title\") \
         with an ![image](img/{i}.png) and a footnote[^{i}]. Plain words follow to give the scanner \
         something to skip over, including <span>inline html</span> and an autolink <https://example.com>.\n\n\
         - item one\n- item **two**\n  - nested item with `code`\n  - nested item\n- item three\n\n\
         1. first\n2. second\n3. third\n\n\
         > quoted *text* that spans\n> two lines\n\n\
         ```rust\nfn main() {{\n    println!(\"{i}\");\n}}\n```\n\n\
         | id | name | value |\n|----|:----:|------:|\n| {i} | `a` | 1.5 |\n| {i} | b | **2** |\n\n\
         ---\n\n\
         [^{i}]: Footnote text for section {i}.\n\n"
    );
}

fn document(sections: usize) -> String {
    let mut out = String::from("---\ntitle: bench\n---\n\n# Benchmark\n\n");
    for i in 0..sections {
        section(&mut out, i);
    }
    out
}

static SMALL: LazyLock<String> = LazyLock::new(|| document(5));
static MEDIUM: LazyLock<String> = LazyLock::new(|| document(100));
static LARGE: LazyLock<String> = LazyLock::new(|| document(2000));
static PLAIN_PARAGRAPHS: LazyLock<String> =
    LazyLock::new(|| "Plain text with no markup at all, only words and commas, repeated.\n\n".repeat(3000));
static HEADINGS: LazyLock<String> = LazyLock::new(|| (0..3000).map(|i| format!("# Heading {i}\n\n")).collect());

fn bench_parse(bencher: divan::Bencher, input: &str) {
    bencher
        .counter(divan::counter::BytesCount::of_str(input))
        .bench_local(|| Markdown::from_markdown_str(input).unwrap());
}

#[divan::bench]
fn parse_small(bencher: divan::Bencher) {
    bench_parse(bencher, &SMALL);
}

#[divan::bench]
fn parse_medium(bencher: divan::Bencher) {
    bench_parse(bencher, &MEDIUM);
}

#[divan::bench]
fn parse_large(bencher: divan::Bencher) {
    bench_parse(bencher, &LARGE);
}

#[divan::bench]
fn parse_plain_paragraphs(bencher: divan::Bencher) {
    bench_parse(bencher, &PLAIN_PARAGRAPHS);
}

#[divan::bench]
fn parse_headings_only(bencher: divan::Bencher) {
    bench_parse(bencher, &HEADINGS);
}

#[divan::bench]
fn render_medium(bencher: divan::Bencher) {
    let markdown = Markdown::from_markdown_str(&MEDIUM).unwrap();
    bencher.bench_local(|| markdown.to_string());
}

#[divan::bench]
fn parse_and_drop_large(bencher: divan::Bencher) {
    bencher.bench_local(|| drop(Markdown::from_markdown_str(&LARGE).unwrap()));
}
