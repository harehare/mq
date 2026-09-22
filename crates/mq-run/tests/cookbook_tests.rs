//! Runs the query/input/output examples from `docs/books/src/cookbook/*.md` through the
//! built `mq` binary, so the docs stay honest against the Tarn bytecode VM.

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
fn cookbook_compact_json_with_toon() {
    let path = write_temp(
        "cookbook_compact_json_with_toon.json",
        r#"{"users": [{"id": 1, "name": "Alice", "role": "admin"}, {"id": 2, "name": "Bob", "role": "dev"}, {"id": 3, "name": "Carol", "role": "dev"}]}"#,
    );
    let toon = run(&["-I", "json", "-F", "toon", ".", path.to_str().unwrap()]);
    assert_eq!(
        toon.trim(),
        "users[3]{id,name,role}:\n  1,Alice,admin\n  2,Bob,dev\n  3,Carol,dev"
    );

    let embedded = run(&[
        "-I",
        "json",
        r#"import "toon" | "Users:\n" + toon::toon_stringify(get("users"))"#,
        path.to_str().unwrap(),
    ]);
    assert_eq!(
        embedded.trim(),
        "Users:\n[3]{id,name,role}:\n  1,Alice,admin\n  2,Bob,dev\n  3,Carol,dev"
    );

    let toon_file = write_temp("cookbook_compact_json_with_toon.toon", &toon);
    let names = run(&[
        "-I",
        "toon",
        "-F",
        "json",
        r#"get("users") | map(fn(u): u["name"];)"#,
        toon_file.to_str().unwrap(),
    ]);
    assert_eq!(
        names.split_whitespace().collect::<String>(),
        r#"["Alice","Bob","Carol"]"#
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
fn cookbook_convert_xml_and_json() {
    let path = write_temp(
        "cookbook_convert_xml_and_json.xml",
        "<library>\n  <book id=\"b1\" lang=\"en\"><title>Dune</title><year>1965</year></book>\n  <book id=\"b2\" lang=\"ja\"><title>Kokoro</title><year>1914</year></book>\n</library>\n",
    );
    let books = run(&[
        "-I",
        "xml",
        "-F",
        "json",
        r#"import "xml" | xml::xml_find_all("book") | map(fn(b): {"id": xml::xml_attr(b, "id"), "title": xml::xml_text(xml::xml_find(b, "title")), "year": to_number(xml::xml_text(xml::xml_find(b, "year")))};)"#,
        path.to_str().unwrap(),
    ]);
    assert_eq!(
        books.split_whitespace().collect::<String>(),
        r#"[{"id":"b1","title":"Dune","year":1965},{"id":"b2","title":"Kokoro","year":1914}]"#
    );

    let tree = write_temp(
        "cookbook_convert_xml_and_json_tree.xml",
        r#"<book id="b1"><title>Dune</title><year>1965</year></book>"#,
    );
    let tree_json = run(&["-I", "xml", "-F", "json", ".", tree.to_str().unwrap()]);
    assert_eq!(
        tree_json.split_whitespace().collect::<String>(),
        r#"{"tag":"book","attributes":{"id":"b1"},"children":[{"tag":"title","attributes":{},"children":[],"text":"Dune"},{"tag":"year","attributes":{},"children":[],"text":"1965"}],"text":null}"#
    );

    let json = write_temp(
        "cookbook_convert_xml_and_json.json",
        r#"{"name": "web", "ports": [80, 443], "debug": false}"#,
    );
    let xml = run(&["-I", "json", "-F", "xml", ".", json.to_str().unwrap()]);
    assert_eq!(
        xml.trim(),
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<root>\n  <name>web</name>\n  <ports>\n    <item>80</item>\n    <item>443</item>\n  </ports>\n  <debug>false</debug>\n</root>"
    );

    let greeting = run(&[
        "-I",
        "null",
        r#"import "xml" | xml::xml_stringify({"tag": "greeting", "attributes": {"lang": "en"}, "children": [], "text": "hello"})"#,
    ]);
    assert_eq!(
        greeting.trim(),
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<greeting lang=\"en\">hello</greeting>"
    );
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
fn cookbook_decode_cbor_payload() {
    let expected = r#"{"id":7,"tags":["a","b"],"ok":true}"#;
    let compact = |out: String| out.split_whitespace().collect::<String>();

    let payload = std::env::temp_dir().join("cookbook_decode_cbor_payload.cbor");
    fs::write(
        &payload,
        [
            0xa3, 0x62, 0x69, 0x64, 0x07, 0x64, 0x74, 0x61, 0x67, 0x73, 0x82, 0x61, 0x61, 0x61, 0x62, 0x62, 0x6f, 0x6b,
            0xf5,
        ],
    )
    .unwrap();
    let from_file = run(&["-I", "cbor", "-F", "json", ".", payload.to_str().unwrap()]);
    assert_eq!(compact(from_file), expected);

    let base64 = write_temp("cookbook_decode_cbor_payload.b64", "o2JpZAdkdGFnc4JhYWFiYm9r9Q==\n");
    let from_base64 = run(&[
        "-I",
        "raw",
        "-F",
        "json",
        r#"import "cbor" | cbor::cbor_parse"#,
        base64.to_str().unwrap(),
    ]);
    assert_eq!(compact(from_base64), expected);

    let tags = run(&["-I", "cbor", r#"get("tags")"#, payload.to_str().unwrap()]);
    assert_eq!(tags.trim(), r#"["a", "b"]"#);

    let json = write_temp(
        "cookbook_decode_cbor_payload.json",
        r#"{"id": 7, "tags": ["a", "b"], "ok": true}"#,
    );
    let encoded = run(&[
        "-I",
        "json",
        r#"import "cbor" | cbor::cbor_stringify | base64"#,
        json.to_str().unwrap(),
    ]);
    assert_eq!(encoded.trim(), "o2JpZPlHAGR0YWdzgmFhYWJib2v1");
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
fn cookbook_fill_blank_csv_cells() {
    let path = write_temp(
        "cookbook_fill_blank_csv_cells.csv",
        "region,product,owner\nEast,apple,Kim\n,banana,\nWest,apple,\n,banana,Lee\n",
    );
    let path = path.to_str().unwrap();

    let out = run(&[r#"csv::forward_fill(["region"]) | csv::csv_stringify(",")"#, path]);
    assert_eq!(
        out.trim(),
        "region,product,owner\nEast,apple,Kim\nEast,banana,\nWest,apple,\nWest,banana,Lee"
    );

    let grouped =
        r#"csv::forward_fill(["region"]) | csv::forward_fill(self, ["owner"], ["region"]) | csv::csv_stringify(",")"#;
    assert_eq!(
        run(&[grouped, path]).trim(),
        "region,product,owner\nEast,apple,Kim\nEast,banana,Kim\nWest,apple,\nWest,banana,Lee"
    );

    let constant = r#"csv::forward_fill(["region"]) | csv::forward_fill(self, ["owner"], ["region"]) | csv::constant_fill(self, {"owner": "unassigned"}) | csv::csv_stringify(",")"#;
    assert_eq!(
        run(&[constant, path]).trim(),
        "region,product,owner\nEast,apple,Kim\nEast,banana,Kim\nWest,apple,unassigned\nWest,banana,Lee"
    );

    let limited = r#"csv::forward_fill(self, ["region"], [], "both", 1) | csv::csv_stringify(",")"#;
    assert_eq!(
        run(&[limited, path]).trim(),
        "region,product,owner\nEast,apple,Kim\nEast,banana,\nWest,apple,\nWest,banana,Lee"
    );
}

#[test]
fn cookbook_fill_blank_table_cells() {
    let path = write_temp(
        "cookbook_fill_blank_table_cells.md",
        "| Region | Product | Owner |\n| :----- | ------- | ----: |\n| East   | apple   | Kim   |\n|        | banana  |       |\n| West   | apple   |       |\n|        | banana  | Lee   |\n",
    );
    let path = path.to_str().unwrap();

    let out = run(&[
        "-A",
        r#"import "table" | table::tables | first | table::forward_fill(["Region"])"#,
        path,
    ]);
    assert_eq!(
        out.trim(),
        "| Region | Product | Owner |\n| :----- | ------- | ----: |\n| East   | apple   |   Kim |\n| East   | banana  |       |\n| West   | apple   |       |\n| West   | banana  |   Lee |"
    );

    let constant = r#"import "table" | table::tables | first | table::forward_fill(["Region"]) | table::forward_fill(self, ["Owner"], ["Region"]) | table::constant_fill(self, {"Owner": "unassigned"})"#;
    assert_eq!(
        run(&["-A", constant, path]).trim(),
        "| Region | Product |      Owner |\n| :----- | ------- | ---------: |\n| East   | apple   |        Kim |\n| East   | banana  |        Kim |\n| West   | apple   | unassigned |\n| West   | banana  |        Lee |"
    );
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
fn cookbook_find_section_containing_a_node() {
    let path = write_temp(
        "cookbook_find_section_containing_a_node.md",
        "# Introduction\n\nWelcome.\n\n## Installation\n\n```bash\nnpm install\n```\n\n## Usage\n\nRun it.\n",
    );
    let out = run(&[
        "-A",
        r#"let n = first(compact(.code)) | section::containing(n) | section::title"#,
        path.to_str().unwrap(),
    ]);
    assert_eq!(out.trim(), "Installation");
}

#[test]
fn cookbook_navigate_the_section_tree() {
    let path = write_temp(
        "cookbook_navigate_the_section_tree.md",
        "# Guide\n\nIntro.\n\n## Install\n\n### macOS\n\nbrew.\n\n### Linux\n\napt.\n\n## Usage\n\nRun it.\n",
    );
    let out = run(&[
        "-A",
        r#"let t = section::tree(self) | map(section::tree::flatten(t), fn(s): join(section::tree::breadcrumb(t, s), " > ");)"#,
        path.to_str().unwrap(),
    ]);
    assert_eq!(
        out.trim(),
        "Guide\nGuide > Install\nGuide > Install > macOS\nGuide > Install > Linux\nGuide > Usage"
    );

    let neighbours = run(&[
        "-A",
        r#"let t = section::tree(self) | let s = section::tree::at(t, [0, 0, 1]) | section::title(section::tree::prev_sibling(t, s))"#,
        path.to_str().unwrap(),
    ]);
    assert_eq!(neighbours.trim(), "macOS");
}

#[test]
fn cookbook_flatten_json_with_gron() {
    let path = write_temp(
        "cookbook_flatten_json_with_gron.json",
        r#"{"name": "demo", "version": "1.0.0", "scripts": {"build": "tsc", "test": "vitest"}, "dependencies": {"react": "^18.2.0", "zod": "^3.22.0"}, "keywords": ["a", "b"]}"#,
    );
    let gron = run(&["-I", "json", "-F", "gron", ".", path.to_str().unwrap()]);
    assert_eq!(
        gron.trim(),
        "json = {};\njson.name = \"demo\";\njson.version = \"1.0.0\";\njson.scripts = {};\njson.scripts.build = \"tsc\";\njson.scripts.test = \"vitest\";\njson.dependencies = {};\njson.dependencies.react = \"^18.2.0\";\njson.dependencies.zod = \"^3.22.0\";\njson.keywords = [];\njson.keywords[0] = \"a\";\njson.keywords[1] = \"b\";"
    );

    // The rebuild step reads gron lines back with `-I gron`; both the parent-inclusive and
    // leaf-only selections must recreate the same object.
    let compact = |out: String| out.split_whitespace().collect::<String>();
    let expected = r#"{"dependencies":{"react":"^18.2.0","zod":"^3.22.0"}}"#;
    for selection in [
        "json.dependencies = {};\njson.dependencies.react = \"^18.2.0\";\njson.dependencies.zod = \"^3.22.0\";\n",
        "json.dependencies.react = \"^18.2.0\";\njson.dependencies.zod = \"^3.22.0\";\n",
    ] {
        let lines = write_temp("cookbook_flatten_json_with_gron.gron", selection);
        let rebuilt = run(&["-I", "gron", "-F", "json", ".", lines.to_str().unwrap()]);
        assert_eq!(compact(rebuilt), expected);
    }

    let module = run(&[
        "-I",
        "json",
        r#"import "gron" | gron::gron_stringify | split("\n") | filter(fn(l): contains(l, "dependencies.");) | join("\n")"#,
        path.to_str().unwrap(),
    ]);
    assert_eq!(
        module.trim(),
        "json.dependencies.react = \"^18.2.0\";\njson.dependencies.zod = \"^3.22.0\";"
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
fn cookbook_generate_markdown_with_md_module() {
    let path = write_temp(
        "cookbook_generate_markdown_with_md_module.json",
        r#"{"users": [{"id": 1, "name": "Alice", "role": "admin"}, {"id": 2, "name": "Bob", "role": "dev"}, {"id": 3, "name": "Carol", "role": "dev"}]}"#,
    );
    let query = r#"import "md" | let users = get("users") | md::doc(md::h2("Team"), md::table(["Name", "Role"], map(users, fn(u): [u["name"], u["role"]];)), md::h3("Members"), map(users, fn(u): md::list(u["name"]);))"#;
    let out = run(&["-I", "json", query, path.to_str().unwrap()]);
    assert_eq!(
        out.trim(),
        "## Team\n|Name|Role|\n|---|---|\n|Alice|admin|\n|Bob|dev|\n|Carol|dev|\n### Members\n- Alice\n- Bob\n- Carol"
    );

    let html = run(&["-I", "json", "-F", "html", query, path.to_str().unwrap()]);
    assert!(
        html.starts_with("<h2>Team</h2>\n<table>\n<thead>"),
        "unexpected HTML: {html}"
    );

    let aligned = run(&[
        "-I",
        "null",
        r#"import "md" | md::doc(md::table(["Name", "Score"], [["a", "1"]], ["left", "right"]))"#,
    ]);
    assert_eq!(aligned.trim(), "|Name|Score|\n|:---|---:|\n|a|1|");

    let checkboxes = run(&[
        "-I",
        "null",
        r#"import "md" | md::doc(md::list("todo", 0, false, false), md::list("done", 0, false, true))"#,
    ]);
    assert_eq!(checkboxes.trim(), "- [ ] todo\n- [x] done");
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
fn cookbook_read_values_from_xml() {
    let pom = write_temp(
        "cookbook_read_values_from_xml_pom.xml",
        r#"<?xml version="1.0" encoding="UTF-8"?>
<project>
  <modelVersion>4.0.0</modelVersion>
  <artifactId>demo-app</artifactId>
  <version>1.2.0</version>
  <dependencies>
    <dependency>
      <groupId>org.slf4j</groupId>
      <artifactId>slf4j-api</artifactId>
      <version>2.0.9</version>
    </dependency>
    <dependency>
      <groupId>com.google.guava</groupId>
      <artifactId>guava</artifactId>
      <version>33.0.0-jre</version>
      <scope>test</scope>
    </dependency>
  </dependencies>
</project>
"#,
    );
    let table = run(&[
        "-I",
        "xml",
        r#"import "xml" | import "csv" | xml::xml_find_all("dependency") | map(fn(d): {"group": xml::xml_text(xml::xml_find(d, "groupId")), "artifact": xml::xml_text(xml::xml_find(d, "artifactId")), "version": xml::xml_text(xml::xml_find(d, "version"))};) | csv::csv_to_markdown_table()"#,
        pom.to_str().unwrap(),
    ]);
    assert_eq!(
        table.trim(),
        "| group | artifact | version |\n| --- | --- | --- |\n| org.slf4j | slf4j-api | 2.0.9 |\n| com.google.guava | guava | 33.0.0-jre |"
    );

    // The first `version` in document order is the project's own, not a dependency's.
    let version = run(&[
        "-I",
        "xml",
        r#"import "xml" | xml::xml_text(xml::xml_find("version"))"#,
        pom.to_str().unwrap(),
    ]);
    assert_eq!(version.trim(), "1.2.0");

    let test_scoped = run(&[
        "-I",
        "xml",
        r#"import "xml" | xml::xml_find_all("dependency") | filter(fn(d): xml::xml_text(xml::xml_find(d, "scope")) == "test";) | map(fn(d): xml::xml_text(xml::xml_find(d, "artifactId"));)"#,
        pom.to_str().unwrap(),
    ]);
    assert_eq!(test_scoped.trim(), r#"["guava"]"#);

    let users = write_temp(
        "cookbook_read_values_from_xml_users.xml",
        r#"<users><user id="1" role="admin"/><user id="2" role="dev"/><user id="3" role="dev"/></users>"#,
    );
    let dev_ids = run(&[
        "-I",
        "xml",
        r#"import "xml" | xml::xml_find_all("user") | filter(fn(u): xml::xml_attr(u, "role") == "dev";) | map(fn(u): xml::xml_attr(u, "id");)"#,
        users.to_str().unwrap(),
    ]);
    assert_eq!(dev_ids.trim(), r#"["2", "3"]"#);

    let feed = write_temp(
        "cookbook_read_values_from_xml_feed.xml",
        r#"<?xml version="1.0"?>
<rss version="2.0">
  <channel>
    <title>Release notes</title>
    <item><title>v0.8.5</title><link>https://example.com/v0.8.5</link></item>
    <item><title>v0.8.4</title><link>https://example.com/v0.8.4</link></item>
  </channel>
</rss>
"#,
    );
    let items = run(&[
        "-I",
        "xml",
        r#"import "xml" | xml::xml_find_all("item") | map(fn(i): "- [" + xml::xml_text(xml::xml_find(i, "title")) + "](" + xml::xml_text(xml::xml_find(i, "link")) + ")";) | join("\n")"#,
        feed.to_str().unwrap(),
    ]);
    assert_eq!(
        items.trim(),
        "- [v0.8.5](https://example.com/v0.8.5)\n- [v0.8.4](https://example.com/v0.8.4)"
    );
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
