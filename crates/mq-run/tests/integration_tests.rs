use assert_cmd::cargo;
use rstest::rstest;
use scopeguard::defer;
use std::io::Write;
use std::{fs::File, path::PathBuf};

pub fn create_file(name: &str, content: &str) -> (PathBuf, PathBuf) {
    let temp_dir = std::env::temp_dir();
    let temp_file_path = temp_dir.join(name);
    let mut file = File::create(&temp_file_path).expect("Failed to create temp file");
    file.write_all(content.as_bytes())
        .expect("Failed to write to temp file");

    (temp_dir, temp_file_path)
}

#[test]
fn test_cli_run_with_stdin() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = cargo::cargo_bin_cmd!("mq");

    let assert = cmd
        .arg("--unbuffered")
        .arg(r#".h | select(contains("title"))"#)
        .write_stdin("# **title**\n\n- test1\n- test2")
        .assert();
    assert.success().code(0).stdout("# **title**\n");

    Ok(())
}

#[rstest]
#[case::json(
    vec!["--unbuffered", "-F", "json", ".code_inline"],
    "`inline code`",
    Some(r#"[
  {
    "type": "CodeInline",
    "value": "inline code"
  }
]"#)
)]
#[case::args(
    vec!["--unbuffered", "--args", "val1", "test", "select(contains(val1))"],
    "# **title**\n\n- test1\n- test2",
    Some("- test1\n- test2\n")
)]
#[case::format(
    vec!["fmt"],
    "def test(x):\nadd(x,1);\n| map(array(1,2,3),test)",
    None
)]
#[case::docs(
    vec!["docs"],
    "",
    None
)]
#[case::update_file(
    vec!["--unbuffered", "--update", r#".h | select(contains("title")) | ltrimstr("titl")"#],
    "# **title**\n\n- test1\n- test2",
    Some("# **e**\n\n- test1\n- test2\n")
)]
#[case::update_nested(
    vec!["--unbuffered", "--update", r#".strong | select(contains("title")) | ltrimstr("titl")"#],
    "# [**title**](url)\n\n- test1\n- test2",
    Some("# [**e**](url)\n\n- test1\n- test2\n")
)]
#[case::null_input(
    vec!["--unbuffered", "-I", "null", "1 | add(2)"],
    "",
    Some("3\n")
)]
#[case::mdx_input(
    vec!["--unbuffered", "-I", "mdx", "select(is_mdx())"],
    r##"import {Chart} from './snowfall.js'
export const year = 2023

# Last year’s snowfall

In {year}, the snowfall was above average.

<Chart color="#fcb32c" year={year} />
<Component />"##,
    Some(r##"{Chart}
{year}
<Chart color="#fcb32c" year={year} />
<Component />
"##)
)]
#[case::nested_item(
    vec!["--unbuffered", "--update" , r#"if (and(or(.link, .definition), matches_url("a/b/c.html"))): update("x/y/z.html")"#],
    "- another item\n\n  [another link]: a/b/c.html",
    Some("- another item\n\n  [another link]: x/y/z.html\n")
)]
#[case::nested_item(
    vec!["--unbuffered", "--update" , r#".code_inline | update("test")"#],
    "# `title`\n# `title`",
    Some("# `test`\n# `test`\n")
)]
#[case::nested_item(
    vec!["--unbuffered", "--update" , r#"if (and(or(.link, .link_ref, .definition), matches_url("a/b/c.html"))): update("x/y/z.html")"#],
    "- item\n\n  [another link]: <a/b/c.html> \"this\n  is a title\"\n\n<!-- -->\n\n    [link2](a/b/c.html)\n    test\n",
    Some("- item\n\n  [another link]: x/y/z.html \"this\n  is a title\"\n\n<!-- -->\n\n    [link2](a/b/c.html)\n    test\n"),
)]
#[case::nested_item(
    vec!["--unbuffered", "--update", "--link-title-style", "paren", "--link-url-style", "angle", r#"if (and(or(.link, .link_ref, .definition), matches_url("a/b/c.html"))): update("x/y/z.html")"#],
    "- item\n\n  [another link]: <a/b/c.html> (this  is a title)\n",
    Some("- item\n\n  [another link]: <x/y/z.html> (this  is a title)\n"),
)]
#[case::empty_results(
    vec!["--unbuffered", "--link-title-style", "paren", "--link-url-style", "angle", r#"select(or(.link, .definition)) | if (eq(get_url(), "a/b/c.html1")): "1234""#],
    "[link](a/b/c.html)\n[link](a/b/c.html)",
    Some(""),
)]
#[case::nodes(
    vec!["--unbuffered", r#".h | nodes"#],
    "# h1\n\nheader\n\n## h2\n\nheader\n\n# h3\n\nheader\n",
    Some("# h1\n## h2\n# h3\n"),
)]
#[case::parallel(
    vec!["--unbuffered", "-P", "0", r#"nodes | .h"#],
    "# h1\n\nheader\n\n## h2\n\nheader\n\n# h3\n\nheader\n",
    Some("# h1\n## h2\n# h3\n"),
)]
#[case::csv_output(
    vec!["--unbuffered", "-I", "raw", "--csv", "csv_parse(false)"],
    "type,value\nHeading,title1\nHeading,title2\n",
    Some(r#"[["type", "value"], ["Heading", "title1"], ["Heading", "title2"]]
"#)
)]
#[case::json_output(
    vec!["--unbuffered", "-I", "raw", "--json", "json_parse()"],
    r#"[
  {
    "type": "Heading",
    "value": "title1"
  },
  {
    "type": "Heading",
    "value": "title2"
  }
]"#,
    Some(r#"[{"type": "Heading", "value": "title1"}, {"type": "Heading", "value": "title2"}]
"#)
)]
#[case::yaml_output(
    vec!["--unbuffered", "-I", "raw", "--yaml", "yaml_parse()"],
    r#"- type: Heading
  value: title1
- type: Heading
  value: title2
"#,
    Some(
        r#"[{"value": "title1"}, {"value": "title2"}]
"#,
    )
)]
#[case::xml_output(
    vec!["--unbuffered", "-I", "raw", "--xml", "xml_parse() | xml_stringify()"],
    r#"<?xml version="1.0" encoding="UTF-8"?>
<root>
  <item>
    <type>Heading</type>
    <value>title1</value>
  </item>
  <item>
    <type>Heading</type>
    <value>title2</value>
  </item>
</root>
"#,
    Some(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>
<root><item><type>Heading</type><value>title1</value></item><item><type>Heading</type><value>title2</value></item></root>\n\n",
    )
)]
#[case::test_option_success(
    vec!["--unbuffered", "-I", "text", "--test", "assert(true)"],
    "ok",
    Some("ok\n")
)]
#[case::test_stream(
    vec!["--unbuffered", "-I", "text", "--stream", ".h"],
    "# title\n",
    None
)]
#[case::test_aggregate(
    vec!["--unbuffered", "-A", "section::split(2) | section::nth(0) | section::title()"],
    "# title\n\n## subtitle\n",
    Some("subtitle\n")
)]
#[case::capture_named_groups(
    vec!["--unbuffered", "-I", "text", r#"capture("(?P<year>\\d{4})-(?P<month>\\d{2})-(?P<day>\\d{2})")"#],
    "2024-01-15",
    Some("{\"year\": \"2024\", \"month\": \"01\", \"day\": \"15\"}\n")
)]
#[case::capture_no_match(
    vec!["--unbuffered", "-I", "text", r#"capture("(?P<year>\\d{4})-(?P<month>\\d{2})")"#],
    "no-match-here",
    Some("{}\n")
)]
#[case::capture_single_group(
    vec!["--unbuffered", "-I", "text", r#"capture("(?P<name>[a-z]+)")"#],
    "hello world",
    Some("{\"name\": \"hello\"}\n")
)]
#[case::capture_markdown_node(
    vec!["--unbuffered", r#".h | capture("(?P<level>\\w+)\\s+(?P<num>\\d+)")"#],
    "# title 42\n",
    Some("{\"level\": \"title\", \"num\": \"42\"}\n")
)]
fn test_cli_commands(
    #[case] args: Vec<&str>,
    #[case] input: &str,
    #[case] expected_output: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = cargo::cargo_bin_cmd!("mq");
    let mut assert = cmd.args(args);

    if !input.is_empty() {
        assert = assert.write_stdin(input);
    }

    let assert = assert.assert();

    if let Some(output) = expected_output {
        assert.success().code(0).stdout(output.to_owned());
    } else {
        assert.success().code(0);
    }

    Ok(())
}

#[test]
fn test_cli_run_with_raw_file_and_stdin() -> Result<(), Box<dyn std::error::Error>> {
    let (_, temp_file_path) = create_file("input.txt", "test");
    let temp_file_path_clone = temp_file_path.clone();

    defer! {
        if temp_file_path_clone.exists() {
            std::fs::remove_file(&temp_file_path_clone).expect("Failed to delete temp file");
        }
    }

    let mut cmd = cargo::cargo_bin_cmd!("mq");

    let assert = cmd
        .arg("--unbuffered")
        .arg("--rawfile")
        .arg("file1")
        .arg(temp_file_path.to_string_lossy().to_string())
        .arg("select(contains(file1))")
        .write_stdin("# **title**\n\n- test1\n- test2")
        .assert();
    assert.success().code(0).stdout("- test1\n- test2\n");

    Ok(())
}

#[test]
fn test_cli_run_with_file_input() -> Result<(), Box<dyn std::error::Error>> {
    let (_, temp_file_path) = create_file("test_cli_run_with_file_input.md", "# **title**\n\n- test1\n- test2");
    let temp_file_path_clone = temp_file_path.clone();

    defer! {
        if temp_file_path_clone.exists() {
            std::fs::remove_file(&temp_file_path_clone).expect("Failed to delete temp file");
        }
    }

    let mut cmd = cargo::cargo_bin_cmd!("mq");
    let assert = cmd
        .arg("--unbuffered")
        .arg(r#".h | select(contains("title"))"#)
        .arg(temp_file_path.to_string_lossy().to_string())
        .assert();

    assert.success().code(0).stdout("# **title**\n");
    Ok(())
}

#[test]
fn test_cli_run_with_query_from_file() -> Result<(), Box<dyn std::error::Error>> {
    let (_, temp_file_path) = create_file(
        "test_cli_run_with_query_from_file.mq",
        r#".h | select(contains("title"))"#,
    );
    let temp_file_path_clone = temp_file_path.clone();

    defer! {
        if temp_file_path_clone.exists() {
            std::fs::remove_file(&temp_file_path_clone).expect("Failed to delete temp file");
        }
    }

    let mut cmd = cargo::cargo_bin_cmd!("mq");
    let assert = cmd
        .arg("--unbuffered")
        .arg("--from-file")
        .arg(temp_file_path.to_string_lossy().to_string())
        .write_stdin("# **title**\n\n- test1\n- test2")
        .assert();

    assert.success().code(0).stdout("# **title**\n");
    Ok(())
}

#[test]
fn test_cli_run_with_csv_input() -> Result<(), Box<dyn std::error::Error>> {
    let csv_content = "name,age\nAlice,30\nBob,25";
    let (_, temp_file_path) = create_file("test_cli_run_with_csv_input.csv", csv_content);
    let temp_file_path_clone = temp_file_path.clone();

    defer! {
        if temp_file_path_clone.exists() {
            std::fs::remove_file(&temp_file_path_clone).expect("Failed to delete temp file");
        }
    }

    let mut cmd = cargo::cargo_bin_cmd!("mq");
    let assert = cmd
        .arg("--unbuffered")
        .arg("-I")
        .arg("raw")
        .arg(r#"include "csv" | csv_parse(false) | csv_to_markdown_table()"#)
        .arg(temp_file_path.to_string_lossy().to_string())
        .assert();

    assert.success().code(0).stdout(
        "| name | age |
| --- | --- |
| Alice | 30 |
| Bob | 25 |
",
    );
    Ok(())
}

#[test]
fn test_cli_run_with_html_input() -> Result<(), Box<dyn std::error::Error>> {
    let html_content = r#"
<!DOCTYPE html>
<html>
<head>
    <title>Test HTML</title>
</head>
<body>
    <h1>Sample Title</h1>
    <p>This is a <strong>test</strong> paragraph.</p>
</body>
</html>
"#;
    let (_, temp_file_path) = create_file("test_cli_run_with_html_input.html", html_content);
    let temp_file_path_clone = temp_file_path.clone();

    defer! {
        if temp_file_path_clone.exists() {
            std::fs::remove_file(&temp_file_path_clone).expect("Failed to delete temp file");
        }
    }

    let mut cmd = cargo::cargo_bin_cmd!("mq");
    let assert = cmd
        .arg("--unbuffered")
        .arg(".h1")
        .arg(temp_file_path.to_string_lossy().to_string())
        .assert();

    assert.success().code(0).stdout("# Sample Title\n");
    Ok(())
}

#[test]
fn test_cli_run_with_mdx_input_file() -> Result<(), Box<dyn std::error::Error>> {
    let mdx_content = r##"import {Chart} from './snowfall.js'
export const year = 2023

# Last year’s snowfall

In {year}, the snowfall was above average.

<Chart color="#fcb32c" year={year} />
<Component />"##;
    let (_, temp_file_path) = create_file("test_cli_run_with_mdx_input.mdx", mdx_content);
    let temp_file_path_clone = temp_file_path.clone();

    defer! {
        if temp_file_path_clone.exists() {
            std::fs::remove_file(&temp_file_path_clone).expect("Failed to delete temp file");
        }
    }

    let mut cmd = cargo::cargo_bin_cmd!("mq");
    let assert = cmd
        .arg("--unbuffered")
        .arg("-I")
        .arg("mdx")
        .arg("select(is_mdx())")
        .arg(temp_file_path.to_string_lossy().to_string())
        .assert();

    assert
        .success()
        .code(0)
        .stdout("{Chart}\n{year}\n<Chart color=\"#fcb32c\" year={year} />\n<Component />\n");
    Ok(())
}

#[test]
fn test_read_file() -> Result<(), Box<dyn std::error::Error>> {
    let (_, temp_file_path) = create_file("test_read_file.md", "test");
    let temp_file_path_clone = temp_file_path.clone();

    defer! {
        if temp_file_path_clone.exists() {
            std::fs::remove_file(&temp_file_path_clone).expect("Failed to delete temp file");
        }
    }

    let mut cmd = cargo::cargo_bin_cmd!("mq");
    let assert = cmd
        .arg("--unbuffered")
        .arg(format!(r#"read_file("{}")"#, temp_file_path.to_string_lossy(),))
        .arg(temp_file_path.to_string_lossy().to_string())
        .assert();

    assert.success().code(0).stdout("test\n");
    Ok(())
}

#[test]
fn test_def_argument_scope_with_let_and_do() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = cargo::cargo_bin_cmd!("mq");

    let assert = cmd
        .arg("--unbuffered")
        .arg("-I")
        .arg("null")
        .arg("let a = 10 | def c(cc): do cc + 3;; | c(1)")
        .assert();

    assert.success().code(0).stdout("4\n");

    Ok(())
}

#[test]
fn test_loop_with_break() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = cargo::cargo_bin_cmd!("mq");

    let assert = cmd
        .arg("--unbuffered")
        .arg("-I")
        .arg("null")
        .arg("let x = 0 | loop: let x = x + 1 | if(x > 3): break else: x;;")
        .assert();

    assert.success().code(0).stdout("3\n");

    Ok(())
}

#[test]
fn test_loop_with_counter() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = cargo::cargo_bin_cmd!("mq");

    let assert = cmd
        .arg("--unbuffered")
        .arg("-I")
        .arg("null")
        .arg("let x = 0 | loop: let x = x + 1 | if(x > 5): break else: x;;")
        .assert();

    assert.success().code(0).stdout("5\n");

    Ok(())
}

#[test]
fn test_loop_with_continue() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = cargo::cargo_bin_cmd!("mq");

    let assert = cmd
        .arg("--unbuffered")
        .arg("-I")
        .arg("null")
        .arg("let x = 0 | loop: let x = x + 1 | if(x < 3): continue elif(x > 5): break else: x;;")
        .assert();

    assert.success().code(0).stdout("5\n");

    Ok(())
}
