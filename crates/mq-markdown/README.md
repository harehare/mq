<h1 align="center">mq-markdown</h1>

Markdown parser, AST and writer for [mq](https://github.com/harehare/mq). It is the parser behind every `mq` query, and works as a standalone library.

- CommonMark and GFM, plus frontmatter, math and MDX
- Parses to a typed node tree with source positions, and writes it back to Markdown, HTML, plain text or JSON
- Optional Obsidian syntax (`[[wikilinks]]`, `![[embeds]]`, `> [!callouts]`) and HTML to Markdown conversion

## Spec conformance

Every example of each spec is rendered to HTML with `to_html` and compared with the expected HTML in the spec (`just test-spec`).

| Spec                                | Passed / Total | Rate  |
| ----------------------------------- | -------------- | ----- |
| GFM spec (based on CommonMark 0.29) | 624 / 648      | 96.3% |
| CommonMark 0.31.2                   | 636 / 652      | 97.5% |
| GFM extensions (`extensions.txt`)   | 26 / 30        | 86.7% |

The examples that do not match are intentional differences, not parse failures:

- Frontmatter: a document that starts with `---` (YAML) or `+++` (TOML) has frontmatter. Turn it off with `ParseOptions::with_frontmatter(false)`.
- Nested `strong`: runs of `*` and `_` nest as in CommonMark 0.30 and later.
- Link protocols: a destination with a protocol other than `http`, `https`, `irc`, `ircs`, `mailto` or `xmpp` gets no `href`.
- Autolink literals: GFM extended autolinks (`www.example.com`) that the base spec does not have.
- Tag filter: `<script>`, `<style>` and `<textarea>` are written as text.

Nesting is bounded (128 levels for block containers and inline emphasis), so a hostile document cannot exhaust the stack.

## Performance

Rough numbers from a `--release` build, median of 31 runs for the small inputs and 7 for the large one. The `parse` column is `str::parse::<Markdown>()`, `to_html` renders an already parsed document, and `to_string` writes it back as Markdown.

| Input                                 | Size   | Nodes   | parse             | to_html | to_string |
| ------------------------------------- | ------ | ------- | ----------------- | ------- | --------- |
| CommonMark spec                       | 205 KB | 2,098   | 1.4 ms (148 MB/s) | 2.5 ms  | 0.8 ms    |
| mq documentation (all `.md`)          | 298 KB | 5,528   | 2.6 ms (117 MB/s) | 5.7 ms  | 2.0 ms    |
| the same documentation, 20 times over | 5.9 MB | 110,560 | 55 ms (108 MB/s)  | 116 ms  | 42 ms     |

The cost grows linearly with the size of the document. Parse time is on par with [`comrak`](https://github.com/kivikakk/comrak).

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
