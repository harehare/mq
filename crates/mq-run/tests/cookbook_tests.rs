//! Runs the query/input/output examples from `docs/books/src/cookbook/*.md` through the
//! built `mq` binary, so the docs stay honest under both the tree-walking evaluator and the
//! `tarn` bytecode VM (this file runs under both via `just test-all`'s `--all-features` step).

use assert_cmd::cargo;
use base64::Engine as _;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

fn write_temp(name: &str, content: &str) -> PathBuf {
    let path = std::env::temp_dir().join(name);
    File::create(&path).unwrap().write_all(content.as_bytes()).unwrap();
    path
}

fn run(args: &[&str]) -> String {
    run_in(None, args)
}

fn run_in(dir: Option<&Path>, args: &[&str]) -> String {
    let mut cmd = cargo::cargo_bin_cmd!("mq");
    if let Some(dir) = dir {
        cmd.current_dir(dir);
    }
    let assert = cmd.args(args).assert().success();
    String::from_utf8(assert.get_output().stdout.clone()).unwrap()
}

#[test]
fn cookbook_add_row_to_table() {
    let path = write_temp(
        "cookbook_add_row_to_table.md",
        "| Name  | Age |\n| ----- | --- |\n| Alice | 30  |\n",
    );
    let out = run(&[
        "-A",
        r#"import "table" | table::tables | first | table::add_row(["Charlie", "35"])"#,
        path.to_str().unwrap(),
    ]);
    assert_eq!(
        out.trim(),
        "| Name    | Age |\n| ------- | --- |\n| Alice   | 30  |\n| Charlie | 35  |"
    );
}

#[test]
fn cookbook_convert_csv_to_markdown_table() {
    let path = write_temp(
        "cookbook_convert_csv_to_markdown_table.csv",
        "Name,Age,City\nAlice,30,NYC\nBob,25,LA\n",
    );
    let out = run(&["csv::csv_to_markdown_table", path.to_str().unwrap()]);
    assert_eq!(
        out.trim(),
        "| Name | Age | City |\n| --- | --- | --- |\n| Alice | 30 | NYC |\n| Bob | 25 | LA |"
    );
}

#[test]
fn cookbook_convert_table_to_csv() {
    let path = write_temp(
        "cookbook_convert_table_to_csv.md",
        "| Name  | Age |\n| ----- | --- |\n| Alice | 30  |\n",
    );
    let out = run(&[
        "-A",
        r#"import "table" | table::tables | first | table::to_csv"#,
        path.to_str().unwrap(),
    ]);
    assert_eq!(out.trim(), "Name,Age\nAlice,30");

    let out_tsv = run(&[
        "-A",
        r#"import "table" | table::tables | first | table::to_csv(self, "\t")"#,
        path.to_str().unwrap(),
    ]);
    assert_eq!(out_tsv.trim(), "Name\tAge\nAlice\t30");
}

#[test]
fn cookbook_count_words_in_document() {
    let path = write_temp(
        "cookbook_count_words_in_document.md",
        "# Title\n\nThis is a short paragraph with some words in it.\n\n## Section\n\nAnother paragraph here, with more words to count for the estimate.\n",
    );
    let out = run(&[
        "-A",
        "nodes | map(fn(n): to_text(n) | split(\" \") | len;) | fold(0, fn(acc, x): acc + x;)",
        path.to_str().unwrap(),
    ]);
    assert_eq!(out.trim(), "23");
}

#[test]
fn cookbook_define_custom_function() {
    let query = r#"def snake_to_camel(x):
  let words = split(x, "_")
  | foreach (word, words):
      let first_char = upcase(first(word))
      | let rest_str = downcase(slice(word, 1, len(word)))
      | s"${first_char}${rest_str}";
  | join("")
end
| snake_to_camel("hello_world")"#;
    let out = run(&["-I", "null", query]);
    assert_eq!(out.trim(), "HelloWorld");
}

#[test]
fn cookbook_delete_section_by_heading() {
    let path = write_temp(
        "cookbook_delete_section_by_heading.md",
        "# Introduction\n\nWelcome to the project.\n\n## Installation\n\nRun the following command.\n\n## Deprecated\n\nDo not use this anymore.\n",
    );
    let out = run(&[
        "-A",
        r#"section::filter_sections(fn(s): section::title(s) != "Deprecated";)"#,
        path.to_str().unwrap(),
    ]);
    assert_eq!(
        out.trim(),
        "# Introduction\n\nWelcome to the project.\n\n## Installation\n\nRun the following command."
    );
}

#[test]
fn cookbook_extract_blockquotes_as_pull_quotes() {
    let path = write_temp(
        "cookbook_extract_blockquotes_as_pull_quotes.md",
        "# Article\n\nSome intro text.\n\n> This is a great pull quote worth sharing.\n\nMore text here.\n\n> Another quote.\n> Spanning two lines.\n",
    );
    let out = run(&[".blockquote | to_text", path.to_str().unwrap()]);
    assert_eq!(
        out.trim(),
        "This is a great pull quote worth sharing.\nAnother quote.\nSpanning two lines."
    );
}

#[test]
fn cookbook_extract_code_blocks_by_language() {
    let path = write_temp(
        "cookbook_extract_code_blocks_by_language.md",
        "```js\nconst x = 1;\n```\n\n```python\nx = 1\n```\n\n```js\nconst y = 2;\n```\n",
    );
    let out = run(&[r#"select(.code.lang == "js")"#, path.to_str().unwrap()]);
    assert_eq!(out.trim(), "```js\nconst x = 1;\n```\n```js\nconst y = 2;\n```");
}

#[test]
fn cookbook_extract_context_for_llm_prompts() {
    let path = write_temp(
        "cookbook_extract_context_for_llm_prompts.md",
        "# H1\n\n## H2\n\n### H3\n\n```rust\ncode\n```\n\n#### H4\n\n##### H5\n\nprose\n",
    );
    let out = run(&[
        "-A",
        "nodes | filter(fn(n): n | select(.h || .code) | !is_none();) | take(5)",
        path.to_str().unwrap(),
    ]);
    // Capped at the first 5 heading/code nodes, dropping H4/H5.
    assert_eq!(out.trim(), "# H1\n\n## H2\n\n### H3\n\n```rust\ncode\n```\n\n#### H4");
}

#[test]
fn cookbook_extract_footnote_definitions() {
    let path = write_temp(
        "cookbook_extract_footnote_definitions.md",
        "# Doc\n\nHere is a claim[^1] and another[^2].\n\n[^1]: First source.\n[^2]: Second source.\n",
    );
    let out = run(&[".footnote", path.to_str().unwrap()]);
    assert_eq!(out.trim(), "[^1]: First source.\n[^2]: Second source.");
}

#[test]
fn cookbook_extract_frontmatter() {
    let path = write_temp(
        "cookbook_extract_frontmatter.md",
        "---\ntitle: Hello\ntags: [a, b]\n---\n\n# Body\n",
    );
    let out = run(&[".yaml | frontmatter", path.to_str().unwrap()]);
    assert_eq!(out.trim(), r#"{"title": "Hello", "tags": ["a", "b"]}"#);
}

#[test]
fn cookbook_extract_link_urls() {
    let path = write_temp(
        "cookbook_extract_link_urls.md",
        "Check out [mq](https://mqlang.org) and [GitHub](https://github.com).\n",
    );
    let out = run(&[".link.url", path.to_str().unwrap()]);
    assert_eq!(out.trim(), "https://mqlang.org\nhttps://github.com");
}

#[test]
fn cookbook_extract_mdx_components() {
    let path = write_temp(
        "cookbook_extract_mdx_components.md",
        "Regular paragraph.\n\n<CustomComponent prop=\"value\" />\n\nAnother paragraph.\n\n<AnotherComponent>\n  Content\n</AnotherComponent>\n",
    );
    let out = run(&["-I", "mdx", "select(is_mdx())", path.to_str().unwrap()]);
    assert_eq!(
        out.trim(),
        "<CustomComponent prop=\"value\" />\n<AnotherComponent>Content</AnotherComponent>"
    );
}

#[test]
fn cookbook_extract_nth_list_item() {
    let path = write_temp(
        "cookbook_extract_nth_list_item.md",
        "- A1\n- A2\n- A3\n\n1. B1\n2. B2\n",
    );
    let out = run(&[".[1]", path.to_str().unwrap()]);
    assert_eq!(out.trim(), "- A2\n2. B2");
}

#[test]
fn cookbook_extract_section_by_heading() {
    let path = write_temp(
        "cookbook_extract_section_by_heading.md",
        "# Introduction\n\nWelcome to the project.\n\n## Installation\n\nRun the following command.\n\n## Usage\n\nUse the tool like this.\n",
    );
    let out = run(&["-A", r#"section::section("Installation")"#, path.to_str().unwrap()]);
    assert_eq!(out.trim(), "## Installation\n\nRun the following command.");
}

#[test]
fn cookbook_extract_table_row() {
    let path = write_temp(
        "cookbook_extract_table_row.md",
        "| Name  | Age | City |\n| ----- | --- | ---- |\n| Alice | 30  | NYC  |\n| Bob   | 25  | LA   |\n",
    );
    let out = run(&[".[2][]", path.to_str().unwrap()]);
    assert_eq!(out.trim(), "| Bob | 25 | LA |");
}

#[test]
fn cookbook_extract_tables() {
    let path = write_temp(
        "cookbook_extract_tables.md",
        "| Name  | Age |\n| ----- | --- |\n| Alice | 30  |\n",
    );
    let out = run(&["-A", r#"import "table" | table::tables"#, path.to_str().unwrap()]);
    assert_eq!(out.trim(), "| Name  | Age |\n| ----- | --- |\n| Alice | 30  |");
}

#[test]
fn cookbook_filter_empty_sections() {
    let path = write_temp(
        "cookbook_filter_empty_sections.md",
        "# Introduction\n\nWelcome to the project.\n\n## Empty Section\n\n## Usage\n\nUse the tool like this.\n",
    );
    let out = run(&[
        "-A",
        r#"section::sections | filter(fn(s): !section::has_body(s);) | section::titles"#,
        path.to_str().unwrap(),
    ]);
    assert_eq!(out.trim(), "Empty Section");
}

#[test]
fn cookbook_filter_sections_by_level() {
    let path = write_temp(
        "cookbook_filter_sections_by_level.md",
        "# Chapter 1\n\nIntro.\n\n## Section 1.1\n\nDetail.\n\n# Chapter 2\n\nContent.\n",
    );
    let by_level = run(&["-A", "section::sections | section::by_level(1)", path.to_str().unwrap()]);
    assert_eq!(by_level.trim(), "# Chapter 1\n\nIntro.\n\n# Chapter 2\n\nContent.");

    let by_range = run(&[
        "-A",
        "section::sections | section::by_level(1..2)",
        path.to_str().unwrap(),
    ]);
    assert_eq!(
        by_range.trim(),
        "# Chapter 1\n\nIntro.\n\n## Section 1.1\n\nDetail.\n\n# Chapter 2\n\nContent."
    );
}

#[test]
fn cookbook_find_images_missing_alt_text() {
    let path = write_temp(
        "cookbook_find_images_missing_alt_text.md",
        "![A cute cat](cat.png)\n\n![](missing-alt.png)\n\n![Team photo](team.jpg)\n",
    );
    let out = run(&[r#"select(.image.alt == "")"#, path.to_str().unwrap()]);
    assert_eq!(out.trim(), "![](missing-alt.png)");
}

#[test]
fn cookbook_find_raw_html_blocks() {
    let path = write_temp(
        "cookbook_find_raw_html_blocks.md",
        "# Doc\n\nSome text.\n\n<div class=\"callout\">\n  <strong>Note:</strong> important info.\n</div>\n\nMore text.\n",
    );
    let out = run(&[".html", path.to_str().unwrap()]);
    assert_eq!(
        out.trim(),
        "<div class=\"callout\">\n  <strong>Note:</strong> important info.\n</div>"
    );
}

#[test]
fn cookbook_generate_document_statistics() {
    let path = write_temp(
        "cookbook_generate_document_statistics.md",
        "# Title\n\nParagraph one.\n\n## Section\n\nParagraph two.\n\n```js\ncode block\n```\n\n[a link](http://example.com)\n",
    );
    let query = r#"let headers = count_by(fn(x): x | select(.h);)
| let paragraphs = count_by(fn(x): x | select(.text);)
| let code_blocks = count_by(fn(x): x | select(.code);)
| let links = count_by(fn(x): x | select(.link);)
| s"Headers: ${headers}, Paragraphs: ${paragraphs}, Code: ${code_blocks}, Links: ${links}""#;
    let out = run(&["-A", query, path.to_str().unwrap()]);
    assert_eq!(out.trim(), "Headers: 2, Paragraphs: 2, Code: 1, Links: 1");
}

#[test]
fn cookbook_generate_sitemap() {
    // Regression test: a `def` declared before `nodes` must stay visible after it. Tarn used
    // to compile the two halves of a `nodes` split as independent programs, dropping `sitemap`.
    let path_a = write_temp("cookbook_generate_sitemap_a.md", "# A\n");
    let path_b = write_temp("cookbook_generate_sitemap_b.md", "# B\n");
    let query = r#"def sitemap(item, base_url):
    let path = replace(to_text(item), ".md", ".html")
    | let loc = base_url + path
    | s"<url>
  <loc>${loc}</loc>
  <priority>1.0</priority>
  </url>"
end
| nodes
| first
| sitemap(__FILE__, "https://example.com/")"#;
    let out = run(&[query, path_a.to_str().unwrap(), path_b.to_str().unwrap()]);
    assert!(out.contains(&format!(
        "<loc>https://example.com/{}</loc>",
        path_a.with_extension("html").to_str().unwrap()
    )));
    assert!(out.contains(&format!(
        "<loc>https://example.com/{}</loc>",
        path_b.with_extension("html").to_str().unwrap()
    )));
}

#[test]
fn cookbook_generate_toc_from_headings() {
    let path = write_temp(
        "cookbook_generate_toc_from_headings.md",
        "# Introduction\n## Getting Started\n### Installation\n## Usage\n",
    );
    let out = run(&[
        r##".h | let text = to_text() | let anchor = downcase(replace(text, " ", "-")) | let link = to_link("#" + anchor, text, "") | let level = .h.depth | if (!is_none(level)): to_md_list(link, level - 1)"##,
        path.to_str().unwrap(),
    ]);
    assert_eq!(
        out.trim(),
        "- [Introduction](#introduction)\n  - [Getting Started](#getting-started)\n    - [Installation](#installation)\n  - [Usage](#usage)"
    );
}

#[test]
fn cookbook_inline_local_images_as_base64() {
    let dir = std::env::temp_dir().join("mq_cookbook_inline_images");
    fs::create_dir_all(&dir).unwrap();
    // 1x1 PNG, matches the doc's own worked example so the base64 output is checkable verbatim.
    let png = base64::engine::general_purpose::STANDARD
        .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGP4z8AAAAMBAQAY3Y2wAAAAAElFTkSuQmCC")
        .unwrap();
    File::create(dir.join("logo.png")).unwrap().write_all(&png).unwrap();
    File::create(dir.join("doc.md"))
        .unwrap()
        .write_all(b"![Logo](logo.png)\n\n![External](https://example.com/pic.png)\n")
        .unwrap();

    let out = run_in(
        Some(&dir),
        &["--allow-read=.", "select(.image) | embed_images(., \".\")", "doc.md"],
    );
    assert_eq!(
        out.trim(),
        "![Logo](data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGP4z8AAAAMBAQAY3Y2wAAAAAElFTkSuQmCC)\n\n![External](https://example.com/pic.png)"
    );
}

#[test]
fn cookbook_merge_multiple_files() {
    let dir = std::env::temp_dir().join("mq_cookbook_merge_multiple_files");
    fs::create_dir_all(dir.join("docs")).unwrap();
    File::create(dir.join("docs/intro.md"))
        .unwrap()
        .write_all(b"# Introduction\nWelcome.\n")
        .unwrap();
    File::create(dir.join("docs/usage.md"))
        .unwrap()
        .write_all(b"# Usage\nUse it like this.\n")
        .unwrap();

    let out = run_in(
        Some(&dir),
        &[
            "-S",
            r#"s"\n${__FILE__}\n""#,
            "identity",
            "docs/intro.md",
            "docs/usage.md",
        ],
    );
    assert_eq!(
        out.trim(),
        "docs/intro.md\n# Introduction\nWelcome.\n\ndocs/usage.md\n# Usage\nUse it like this."
    );
}

#[test]
fn cookbook_process_files_in_parallel() {
    let dir = std::env::temp_dir().join("mq_cookbook_process_files_in_parallel");
    fs::create_dir_all(&dir).unwrap();
    File::create(dir.join("a.md")).unwrap().write_all(b"# A\n").unwrap();
    File::create(dir.join("b.md")).unwrap().write_all(b"# B\n").unwrap();

    // -P is a parallelism *threshold*, not a worker count or output-order guarantee, so
    // compare sorted output across a forced-sequential and a forced-parallel run.
    let lines = |out: String| -> Vec<String> {
        let mut lines: Vec<String> = out.lines().map(str::to_owned).collect();
        lines.sort();
        lines
    };
    let sequential = lines(run_in(Some(&dir), &["-P", "50", ".h1", "a.md", "b.md"]));
    let parallel = lines(run_in(Some(&dir), &["-P", "1", ".h1", "a.md", "b.md"]));
    assert_eq!(sequential, vec!["# A", "# B"]);
    assert_eq!(parallel, vec!["# A", "# B"]);
}

#[test]
fn cookbook_reshape_table_pivot() {
    let wide = write_temp(
        "cookbook_reshape_table_pivot_wide.md",
        "| Name  | Q1 | Q2 | Q3 |\n| ----- | -- | -- | -- |\n| Alice | 10 | 20 | 30 |\n| Bob   | 5  | 15 | 25 |\n",
    );
    let longer = run(&[
        "-A",
        r#"import "table" | let t = first(table::tables()) | table::pivot_longer(t, [1, 2, 3], "quarter", "score")"#,
        wide.to_str().unwrap(),
    ]);
    assert_eq!(
        longer.trim(),
        "| Name  | quarter | score |\n| ----- | ------- | ----- |\n| Alice | Q1      | 10    |\n| Alice | Q2      | 20    |\n| Alice | Q3      | 30    |\n| Bob   | Q1      | 5     |\n| Bob   | Q2      | 15    |\n| Bob   | Q3      | 25    |"
    );

    let long = write_temp(
        "cookbook_reshape_table_pivot_long.md",
        "| Name  | quarter | score |\n| ----- | ------- | ----- |\n| Alice | Q1      | 10    |\n| Alice | Q2      | 20    |\n| Alice | Q3      | 30    |\n| Bob   | Q1      | 5     |\n| Bob   | Q2      | 15    |\n| Bob   | Q3      | 25    |\n",
    );
    let wider = run(&[
        "-A",
        r#"import "table" | table::tables | first | table::pivot_wider(1, 2)"#,
        long.to_str().unwrap(),
    ]);
    assert_eq!(
        wider.trim(),
        "| Name  | Q1 | Q2 | Q3 |\n| ----- | -- | -- | -- |\n| Alice | 10 | 20 | 30 |\n| Bob   | 5  | 15 | 25 |"
    );
}

#[test]
fn cookbook_split_document_by_heading() {
    let path = write_temp(
        "cookbook_split_document_by_heading.md",
        "# Chapter 1\n\nIntro.\n\n## Section 1.1\n\nDetail 1.1\n\n## Section 1.2\n\nDetail 1.2\n\n# Chapter 2\n\nContent.\n\n## Section 2.1\n\nDetail 2.1\n",
    );
    let out = run(&["-A", "section::split(2)", path.to_str().unwrap()]);
    assert_eq!(
        out.trim(),
        "## Section 1.1\n\nDetail 1.1\n\n## Section 1.2\n\nDetail 1.2\n\n# Chapter 2\n\nContent.\n\n## Section 2.1\n\nDetail 2.1"
    );
}

#[test]
fn cookbook_track_task_list_progress() {
    let path = write_temp(
        "cookbook_track_task_list_progress.md",
        "# TODO\n\n- [x] Write docs\n- [ ] Add tests\n- [x] Fix bug\n- [ ] Ship release\n",
    );
    let done_only = run(&["select(.list.checked == true)", path.to_str().unwrap()]);
    assert_eq!(done_only.trim(), "- [x] Write docs\n- [x] Fix bug");

    let summary = run(&[
        "-A",
        r#"let total = count_by(fn(x): x | select(.list);)
| let done = count_by(fn(x): x | select(.list.checked == true);)
| s"${done}/${total} done""#,
        path.to_str().unwrap(),
    ]);
    assert_eq!(summary.trim(), "2/4 done");
}

#[test]
fn cookbook_transform_arrays() {
    assert_eq!(
        run(&["-I", "null", "map([1, 2, 3, 4, 5], fn(x): x + 1;)"]).trim(),
        "[2, 3, 4, 5, 6]"
    );
    assert_eq!(
        run(&["-I", "null", "filter([5, 15, 8, 20, 3], fn(x): x > 10;)"]).trim(),
        "[15, 20]"
    );
    assert_eq!(
        run(&["-I", "null", "fold([1, 2, 3, 4], 0, fn(acc, x): acc + x;)"]).trim(),
        "10"
    );
}

#[test]
fn cookbook_update_text_in_place() {
    let path = write_temp(
        "cookbook_update_text_in_place.md",
        "# My Project v1.2.0\n\nInstall version 1.2.0 to get started.\n\nSee the changelog for details.\n",
    );
    let out = run(&[
        "-U",
        r#"select(contains("1.2.0")) | replace("1.2.0", "1.3.0")"#,
        path.to_str().unwrap(),
    ]);
    assert_eq!(
        out.trim(),
        "# My Project v1.3.0\n\nInstall version 1.3.0 to get started.\n\nSee the changelog for details."
    );
}
