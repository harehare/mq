<h1 align="center">mq-markdown</h1>

Markdown parser, AST and writer for [mq](https://github.com/harehare/mq). It is the parser behind every `mq` query, and works as a standalone library.

- CommonMark and GFM, plus frontmatter, math and MDX
- Parses to a typed node tree with source positions, and writes it back to Markdown, HTML, plain text or JSON
- Optional Obsidian syntax (`[[wikilinks]]`, `![[embeds]]`, `> [!callouts]`) and HTML to Markdown conversion

## Spec conformance

Every example of each spec is rendered to HTML with `to_html` and compared with the expected HTML in the spec. The numbers below are the output of `just test-spec` (2026-10-25).

| Spec                                | Passed / Total | Rate  |
| ----------------------------------- | -------------- | ----- |
| GFM spec (based on CommonMark 0.29) | 624 / 648      | 96.3% |
| CommonMark 0.31.2                   | 636 / 652      | 97.5% |
| GFM extensions (`extensions.txt`)   | 26 / 30        | 86.7% |

With the `obsidian` feature the GFM and CommonMark counts drop by 3 (621 and 633), because `[[...]]` in a few spec examples is read as a wikilink or embed on purpose.

`just test-gfm-spec` checks the other direction: parsing each GFM example and writing it back as Markdown (what `mq -U` does) and parsing the result again gives the same tree. All 648 examples round-trip (0 known gaps).

The spec files are fetched over the network, so these tests are `#[ignore]`d in a normal `cargo test`. Set `SPEC_FILE`, `COMMONMARK_SPEC_FILE` or `EXTENSIONS_SPEC_FILE` to use a local copy. The expected list of differences is pinned in `src/parser.rs`, so a change that fixes or breaks an example fails the test.

### Where it differs from the spec

These are the examples that do not match, grouped by reason. None of them is a parse failure.

| Difference        | Why                                                                                                                                | GFM spec examples                           |
| ----------------- | ---------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------- |
| Frontmatter       | A document that starts with `---` (YAML) or `+++` (TOML) has frontmatter. Turn it off with `ParseOptions::with_frontmatter(false)` | 66, 68                                      |
| Nested `strong`   | Runs of `*` and `_` nest as in CommonMark 0.30 and later. The GFM spec is still on 0.29, which merged them                         | 388, 416, 424, 425, 426, 463, 464, 465, 467 |
| Link protocols    | A destination with a protocol other than `http`, `https`, `irc`, `ircs`, `mailto` or `xmpp` gets no `href`                         | 496, 594, 595, 597                          |
| Autolink literals | GFM extended autolinks (`www.example.com`) that the base spec does not have                                                        | 598, 604, 607, 608                          |
| Tag filter        | `<script>`, `<style>` and `<textarea>` are written as text                                                                         | 140, 141, 142, 145, 147                     |

The CommonMark 0.31.2 failures are the same five groups (frontmatter 96 and 98; link protocols 500, 598, 599 and 601; autolink literals 602, 608, 611 and 612; tag filter 170, 171, 172, 173, 176 and 178). The GFM extensions spec has four: example 20 is input that only has to not crash and has no expected output, and examples 23 to 25 are footnotes, which are rendered as current GitHub does rather than as the older spec text.

## Performance

Rough numbers from a `--release` build, median of 31 runs for the small inputs and 7 for the large one. The `parse` column is `str::parse::<Markdown>()`, `to_html` renders an already parsed document, and `to_string` writes it back as Markdown.

| Input                                 | Size   | Nodes   | parse             | to_html | to_string |
| ------------------------------------- | ------ | ------- | ----------------- | ------- | --------- |
| CommonMark spec                       | 205 KB | 2,098   | 1.4 ms (148 MB/s) | 2.5 ms  | 0.8 ms    |
| mq documentation (all `.md`)          | 298 KB | 5,528   | 2.6 ms (117 MB/s) | 5.7 ms  | 2.0 ms    |
| the same documentation, 20 times over | 5.9 MB | 110,560 | 55 ms (108 MB/s)  | 116 ms  | 42 ms     |

`to_html(&str)`, which parses and renders in one call, runs at 83 to 116 MB/s on the same inputs. Twenty times the input takes about 22 times as long, so the cost grows with the size of the document.

For reference, [`comrak`](https://github.com/kivikakk/comrak) (a Rust port of cmark-gfm) 0.40.0 is in the same range. Parse time on the same three inputs, GFM enabled, in a separate run: 1.8 / 2.9 / 61 ms for `comrak` against 2.0 / 2.6 / 57 ms for `mq-markdown`.

### Adversarial input

Input that is built to make a parser slow is a known problem for Markdown, so the cost of the worst cases is checked as well. Each input below was parsed, rendered to HTML and written back as Markdown, on a thread with an 8 MB stack. The number is how much longer it took when the input doubled; linear growth is 2.0.

| Input (n = 10,000 to 80,000 repeats)          | Time at the largest n | Growth per doubling |
| --------------------------------------------- | --------------------- | ------------------- |
| nested emphasis (`*a ` n times, then closers) | 23.6 ms               | 1.4 to 1.8          |
| unclosed `*a ` run                            | 7.2 ms                | 1.8 to 2.2          |
| unclosed `[a ` run                            | 5.2 ms                | 1.8 to 2.4          |
| unclosed backticks                            | 20.4 ms               | 2.0 to 2.1          |
| n link reference definitions                  | 90.7 ms               | 2.0 to 2.2          |
| n nested block quotes (`> > > ...`)           | 15.1 ms               | 1.8 to 2.0          |
| table with n rows                             | 118 ms                | 2.0 to 2.2          |
| n paragraphs of mixed inline markup           | 282 ms                | 1.9 to 2.1          |
| one line of n x 10 words                      | 31.9 ms               | 1.5 to 2.3          |

Nesting is bounded rather than slow: block containers (quotes and lists) and inline emphasis nest to 128 levels, and the rest is kept as text. For example, 1,000 `> ` markers produce 128 nested block quotes and the remaining `> ` characters stay in the text, so a hostile document cannot exhaust the stack. MDX elements nested deeper than 128 are an error. `tests/parser_limits_tests.rs` runs these cases, and others such as unclosed comments, runs of backticks of every length and tab-separated inlines, in the normal test suite.

The numbers are a rough guide from one run on one machine, not a benchmark suite. To check your own, parse your documents with `std::time::Instant` in a `--release` build.

## Install

```toml
[dependencies]
mq-markdown = "0.9"
```

## Usage

### Parse and render

```rust
use mq_markdown::{Markdown, to_html};

let doc = "# Title\n\nSome *emphasis*.".parse::<Markdown>().unwrap();

println!("{}", doc.to_html());   // <h1>Title</h1> ...
println!("{}", doc.to_text());   // plain text of every node
println!("{}", doc);             // back to Markdown

// One-shot Markdown to HTML
assert_eq!(to_html("# Hello"), "<h1>Hello</h1>");
```

### The node tree

`doc.nodes` is a flat `Vec<Node>` of the top-level blocks. A node has a `name()` (`"h1"`, `"code_inline"`, `"table_cell"`), a `value()`, `children()` and a `position()` with one-based line and column.

```rust
use mq_markdown::Markdown;

let doc = "# A\n\n[link](http://example.com)".parse::<Markdown>().unwrap();
for node in &doc.nodes {
    let start = node.position().unwrap().start;
    println!("{} at {}:{}", node.name(), start.line, start.column);
}
// h1 at 1:1
// link at 3:1
```

Frontmatter is a node of its own (`yaml` or `toml`). GFM tables are a sequence of `table_cell` and `table_align` nodes, and task list items are `list` nodes with a `checked` state.

`Node` is an enum with a struct per kind (`Heading`, `Code`, `Link`, `List`, `TableCell`, `Math`, `Footnote`, `MdxJsxFlowElement`, ...), so you can also `match` on it. `NodeKind` lists every kind with its attributes (`AttrSpec`), which is what the mq type checker uses.

### Frontmatter

```rust
use mq_markdown::{Markdown, ParseOptions};

let src = "---\ntitle: Hi\n---\n";

// Default: YAML between `---` lines (TOML between `+++`) is frontmatter.
let doc = src.parse::<Markdown>().unwrap();
assert_eq!(doc.nodes[0].name(), "yaml");

// Off: the lines are a rule and a heading, as in plain CommonMark.
let doc = Markdown::from_markdown_str_with(src, ParseOptions::default().with_frontmatter(false)).unwrap();
assert_eq!(doc.nodes[0].name(), "Horizontal_rule");
```

### Rendering options

`RenderOptions` controls how the document is written back as Markdown: the list bullet (`-`, `*` or `+`), how link destinations are wrapped (`<url>` or bare) and how titles are quoted. The default keeps what each element had in the source.

```rust
use mq_markdown::{Markdown, RenderOptions, ListStyle};

let mut doc = "* a\n* b".parse::<Markdown>().unwrap();
doc.set_options(RenderOptions {
    list_style: Some(ListStyle::Plus),
    ..Default::default()
});
assert_eq!(doc.to_string().trim(), "+ a\n+ b");
```

### MDX

`Markdown::from_mdx_str` parses JSX elements, `{expressions}` and `import`/`export`. A syntax error is a `miette` diagnostic with the message and position, for example an unclosed `<A>` reports ``Expected a closing tag for `<A>` (1:1) before the end of its container``.

```rust
use mq_markdown::Markdown;

let doc = Markdown::from_mdx_str("import A from 'a'\n\n<A x={1} />\n").unwrap();
// mdx_js_esm, mdx_jsx_flow_element
```

When writing an MDX document back, text that MDX would read as an expression or ESM is escaped.

## Features

| Feature            | Description                                                       |
| ------------------ | ----------------------------------------------------------------- |
| `json`             | `Markdown::to_json()` and `serde` support for the tree            |
| `html-to-markdown` | `convert_html_to_markdown` and `Markdown::from_html_str`          |
| `color`            | `to_colored_string()` renders with ANSI colors and a `ColorTheme` |
| `wikilink`         | `[[Page]]` and `[[Page\|alias]]` as `WikiLink` nodes              |
| `embed`            | `![[file.png]]` as `Embed` nodes                                  |
| `callout`          | `> [!note]- Title` as `Callout` nodes                             |
| `obsidian`         | `wikilink`, `embed` and `callout` together                        |

None is on by default. Wikilinks, embeds and callouts also render to HTML with Obsidian's classes (`internal-link`, `callout`). Link targets are used as written.

### HTML to Markdown

```rust
use mq_markdown::{convert_html_to_markdown, ConversionOptions};

let md = convert_html_to_markdown(
    "<h1>T</h1><p>Hello <strong>w</strong></p>",
    ConversionOptions::default(),
).unwrap();
assert_eq!(md, "# T\n\nHello **w**");
```

`ConversionOptions` can turn `<title>` into an `h1`, emit YAML frontmatter from `<head>`, keep `<script>` contents as code blocks, and resolve relative URLs against `base_url` (or `<base href>`).

## Documentation

- [API docs](https://docs.rs/mq-markdown)
- [mq book](https://mqlang.org/book/)

## License

MIT
