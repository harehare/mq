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
    "value": "inline code",
    "position": {
      "start": {
        "line": 1,
        "column": 1
      },
      "end": {
        "line": 1,
        "column": 14
      }
    }
  }
]"#)
)]
#[case::args(
    vec!["--unbuffered", "--args", "val1", "test", "select(contains(val1))"],
    "# **title**\n\n- test1\n- test2",
    Some("- test1\n- test2\n")
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
#[case::section_auto_expand(
    vec!["--unbuffered", "-A", r#"section::section("Introduction")"#],
    "# Introduction\n\nBody text.\n\n# Conclusion\n\nConclusion text.\n",
    Some("# Introduction\n\nBody text.\n")
)]
#[case::section_no_match_empty_output(
    vec!["--unbuffered", "-A", r#"section::section("NonExistent")"#],
    "# Introduction\n\nBody text.\n",
    Some("")
)]
#[case::section_bodies_pipe(
    vec!["--unbuffered", "-A", r#"section::section("Introduction") | section::bodies() | first()"#],
    "# Introduction\n\nBody text.\n",
    Some("Body text.\n")
)]
#[case::table_auto_expand(
    vec!["--unbuffered", "-A", r#"import "table" | table::tables()"#],
    "| Name | Age |\n| ---- | --- |\n| Alice | 30 |\n| Bob | 25 |\n",
    Some("|Name|Age|\n|---|---|\n|Alice|30|\n|Bob|25|\n")
)]
#[case::table_auto_expand_first(
    vec!["--unbuffered", "-A", r#"import "table" | table::tables() | first()"#],
    "| Name | Age |\n| ---- | --- |\n| Alice | 30 |\n| Bob | 25 |\n",
    Some("|Name|Age|\n|---|---|\n|Alice|30|\n|Bob|25|\n")
)]
#[case::table_auto_expand_add_row(
    vec!["--unbuffered", "-A", r#"import "table" | table::tables() | first() | table::add_row(["Charlie", "35"])"#],
    "| Name | Age |\n| ---- | --- |\n| Alice | 30 |\n",
    Some("|Name|Age|\n|---|---|\n|Alice|30|\n|Charlie|35|\n")
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
#[case::grep_basic(
    vec!["--unbuffered", "-F", "grep", ".h"],
    "# title\n\nBody text.\n",
    Some("1:# title\n")
)]
#[case::grep_no_empty_lines(
    vec!["--unbuffered", "-F", "grep", "self"],
    "# title\n\nBody text.\n",
    Some("1:# title\n3:Body text.\n")
)]
#[case::grep_after_context(
    vec!["--unbuffered", "-F", "grep", "--after-context", "1", ".h"],
    "# title\n\nBody text.\n",
    Some("1:# title\n3-Body text.\n")
)]
#[case::grep_before_context(
    vec!["--unbuffered", "-F", "grep", "-B", "1", ".h"],
    "Intro.\n\n# title\n\nBody text.\n",
    Some("1-Intro.\n3:# title\n")
)]
#[case::grep_context_both(
    vec!["--unbuffered", "-F", "grep", "--context", "1", ".h"],
    "Intro.\n\n# title\n\nBody text.\n",
    Some("1-Intro.\n3:# title\n5-Body text.\n")
)]
#[case::grep_multiple_matches_no_separator_when_adjacent(
    vec!["--unbuffered", "-F", "grep", "--after-context", "1", ".h"],
    "# h1\n\nParagraph.\n\n# h2\n\nEnd.\n",
    Some("1:# h1\n3-Paragraph.\n5:# h2\n7-End.\n")
)]
#[case::grep_multiple_matches_separator(
    vec!["--unbuffered", "-F", "grep", "--after-context", "1", ".h"],
    "# h1\n\nP1.\n\nP2.\n\n# h2\n\nEnd.\n",
    Some("1:# h1\n3-P1.\n--\n7:# h2\n9-End.\n")
)]
#[case::input_format_json(
    vec!["--unbuffered", "-I", "json", "[.\"name\", .\"age\"]"],
    r#"{"name": "Alice", "age": 30}"#,
    Some("[\"Alice\", 30]\n")
)]
#[case::input_format_yaml(
    vec!["--unbuffered", "-I", "yaml", "[.\"name\", .\"age\"]"],
    "name: Alice\nage: 30\n",
    Some("[\"Alice\", 30]\n")
)]
#[case::input_format_toml(
    vec!["--unbuffered", "-I", "toml", ".[] | map(fn(x): [do x | .\"name\" end, do x | .\"age\" end];)"],
    "[person]\nname = \"Alice\"\nage = 30\n",
    Some("[[\"Alice\", 30]]\n")
)]
#[case::input_format_csv(
    vec!["--unbuffered", "-I", "csv", "map(fn(x): [do x | .\"name\" end, do x | .\"age\" end];)"],
    "name,age\nAlice,30\nBob,25\n",
    Some("[[\"Alice\", \"30\"], [\"Bob\", \"25\"]]\n")
)]
#[case::input_format_tsv(
    vec!["--unbuffered", "-I", "tsv", "map(fn(x): [do x | .\"name\" end, do x | .\"age\" end];)"],
    "name\tage\nAlice\t30\nBob\t25\n",
    Some("[[\"Alice\", \"30\"], [\"Bob\", \"25\"]]\n")
)]
#[case::input_format_psv(
    vec!["--unbuffered", "-I", "psv", "map(fn(x): [do x | .\"name\" end, do x | .\"age\" end];)"],
    "name|age\nAlice|30\nBob|25\n",
    Some("[[\"Alice\", \"30\"], [\"Bob\", \"25\"]]\n")
)]
#[case::input_format_xml(
    vec!["--unbuffered", "-I", "xml", "self"],
    "<root>text</root>",
    Some("{\"text\": \"text\", \"attributes\": {}, \"tag\": \"root\", \"children\": []}\n")
)]
#[case::select_skips_non_matching_wrapped_list_items(
    vec!["--unbuffered", r#"select(.code.lang == "bash") | to_text()"#],
    "- An [Amazon Bedrock Knowledge Base](https://aws.amazon.com/bedrock/knowledge-bases/)\n  indexes **one configurable [Confluence](https://www.atlassian.com/software/confluence)\n  space** (the space key is an OpenTofu variable)\n- [Confluence](https://www.atlassian.com/software/confluence) credentials are\n  rendered by OpenTofu into an [AWS Secrets Manager](https://aws.amazon.com/secrets-manager/)\n  secret - the only credential store the Bedrock connector accepts\n\n```bash\necho \"hello\"\n```\n",
    Some("echo \"hello\"\n")
)]
#[case::select_skips_non_matching_wrapped_list_items_markdown_format(
    vec!["--unbuffered", r#"select(.code.lang == "bash")"#],
    "- An [Amazon Bedrock Knowledge Base](https://aws.amazon.com/bedrock/knowledge-bases/)\n  indexes **one configurable [Confluence](https://www.atlassian.com/software/confluence)\n  space** (the space key is an OpenTofu variable)\n- [Confluence](https://www.atlassian.com/software/confluence) credentials are\n  rendered by OpenTofu into an [AWS Secrets Manager](https://aws.amazon.com/secrets-manager/)\n  secret - the only credential store the Bedrock connector accepts\n\n```bash\necho \"hello\"\n```\n",
    Some("```bash\necho \"hello\"\n```\n")
)]
#[case::select_skips_non_matching_container_nodes(
    vec!["--unbuffered", "select(.h1)"],
    "- [Link](http://a) **bold** item\n- another *em* ~~del~~ item\n\n> [Quoted link](http://b)\n\n| [Cell link](http://c) | **Cell bold** |\n| --- | --- |\n| a | b |\n\n[^1]: [Footnote link](http://d)\n\n# DONE\n",
    Some("# DONE\n")
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
fn test_auto_format_json_extension() -> Result<(), Box<dyn std::error::Error>> {
    let (_, temp_file_path) = create_file("test_auto_format.json", r#"{"name": "Alice"}"#);
    let temp_file_path_clone = temp_file_path.clone();
    defer! {
        if temp_file_path_clone.exists() {
            std::fs::remove_file(&temp_file_path_clone).expect("Failed to delete temp file");
        }
    }
    let assert = cargo::cargo_bin_cmd!("mq")
        .arg("--unbuffered")
        .arg(".[]")
        .arg(temp_file_path.to_string_lossy().to_string())
        .assert();
    assert.success().code(0).stdout("[\"Alice\"]\n");
    Ok(())
}

#[test]
fn test_auto_format_yaml_extension() -> Result<(), Box<dyn std::error::Error>> {
    let (_, temp_file_path) = create_file("test_auto_format.yaml", "name: Alice\n");
    let temp_file_path_clone = temp_file_path.clone();
    defer! {
        if temp_file_path_clone.exists() {
            std::fs::remove_file(&temp_file_path_clone).expect("Failed to delete temp file");
        }
    }
    let assert = cargo::cargo_bin_cmd!("mq")
        .arg("--unbuffered")
        .arg(".[]")
        .arg(temp_file_path.to_string_lossy().to_string())
        .assert();
    assert.success().code(0).stdout("[\"Alice\"]\n");
    Ok(())
}

#[test]
fn test_auto_format_toml_extension() -> Result<(), Box<dyn std::error::Error>> {
    let (_, temp_file_path) = create_file("test_auto_format.toml", "[person]\nname = \"Alice\"\n");
    let temp_file_path_clone = temp_file_path.clone();
    defer! {
        if temp_file_path_clone.exists() {
            std::fs::remove_file(&temp_file_path_clone).expect("Failed to delete temp file");
        }
    }
    let assert = cargo::cargo_bin_cmd!("mq")
        .arg("--unbuffered")
        .arg(".[]")
        .arg(temp_file_path.to_string_lossy().to_string())
        .assert();
    assert.success().code(0).stdout("[{\"name\": \"Alice\"}]\n");
    Ok(())
}

#[test]
fn test_auto_format_csv_extension() -> Result<(), Box<dyn std::error::Error>> {
    let (_, temp_file_path) = create_file("test_auto_format.csv", "name,age\nAlice,30\n");
    let temp_file_path_clone = temp_file_path.clone();
    defer! {
        if temp_file_path_clone.exists() {
            std::fs::remove_file(&temp_file_path_clone).expect("Failed to delete temp file");
        }
    }
    let assert = cargo::cargo_bin_cmd!("mq")
        .arg("--unbuffered")
        .arg(".[]")
        .arg(temp_file_path.to_string_lossy().to_string())
        .assert();
    assert
        .success()
        .code(0)
        .stdout("[{\"name\": \"Alice\", \"age\": \"30\"}]\n");
    Ok(())
}

#[test]
fn test_auto_format_tsv_extension() -> Result<(), Box<dyn std::error::Error>> {
    let (_, temp_file_path) = create_file("test_auto_format.tsv", "name\tage\nAlice\t30\n");
    let temp_file_path_clone = temp_file_path.clone();
    defer! {
        if temp_file_path_clone.exists() {
            std::fs::remove_file(&temp_file_path_clone).expect("Failed to delete temp file");
        }
    }
    let assert = cargo::cargo_bin_cmd!("mq")
        .arg("--unbuffered")
        .arg(".[]")
        .arg(temp_file_path.to_string_lossy().to_string())
        .assert();
    assert
        .success()
        .code(0)
        .stdout("[{\"name\": \"Alice\", \"age\": \"30\"}]\n");
    Ok(())
}

#[test]
fn test_auto_format_psv_extension() -> Result<(), Box<dyn std::error::Error>> {
    let (_, temp_file_path) = create_file("test_auto_format.psv", "name|age\nAlice|30\n");
    let temp_file_path_clone = temp_file_path.clone();
    defer! {
        if temp_file_path_clone.exists() {
            std::fs::remove_file(&temp_file_path_clone).expect("Failed to delete temp file");
        }
    }
    let assert = cargo::cargo_bin_cmd!("mq")
        .arg("--unbuffered")
        .arg(".[]")
        .arg(temp_file_path.to_string_lossy().to_string())
        .assert();
    assert
        .success()
        .code(0)
        .stdout("[{\"name\": \"Alice\", \"age\": \"30\"}]\n");
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
    #[cfg(unix)]
    let assert = cmd
        .arg("--unbuffered")
        .arg(format!(r#"read_file("{}")"#, temp_file_path.to_string_lossy()))
        .arg(temp_file_path.to_string_lossy().to_string())
        .assert();

    #[cfg(windows)]
    let assert = cmd
        .arg("--unbuffered")
        .arg(format!(
            r#"read_file("{}")"#,
            temp_file_path.to_string_lossy().replace("\\", "/")
        ))
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

#[test]
fn test_let_array_destructuring() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = cargo::cargo_bin_cmd!("mq");

    let assert = cmd
        .arg("--unbuffered")
        .arg("-I")
        .arg("null")
        .arg("let [a, b] = [1, 2] | a")
        .assert();

    assert.success().code(0).stdout("1\n");

    Ok(())
}

#[test]
fn test_let_array_rest_destructuring() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = cargo::cargo_bin_cmd!("mq");

    let assert = cmd
        .arg("--unbuffered")
        .arg("-I")
        .arg("null")
        .arg("let [x, ..r] = [1, 2, 3] | r")
        .assert();

    assert.success().code(0).stdout("[2, 3]\n");

    Ok(())
}

#[test]
fn test_let_dict_destructuring() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = cargo::cargo_bin_cmd!("mq");

    let assert = cmd
        .arg("--unbuffered")
        .arg("-I")
        .arg("null")
        .arg(r#"let {a} = {"a": 10} | a"#)
        .assert();

    assert.success().code(0).stdout("10\n");

    Ok(())
}

#[test]
fn test_let_dict_destructuring_explicit_key() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = cargo::cargo_bin_cmd!("mq");

    let assert = cmd
        .arg("--unbuffered")
        .arg("-I")
        .arg("null")
        .arg(r#"let {a: xxx} = {"a": 10, "b": 20} | xxx"#)
        .assert();

    assert.success().code(0).stdout("10\n");

    Ok(())
}

#[test]
fn test_var_array_destructuring_mutation() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = cargo::cargo_bin_cmd!("mq");

    let assert = cmd
        .arg("--unbuffered")
        .arg("-I")
        .arg("null")
        .arg("var [a, b] = [1, 2] | a = 99 | a")
        .assert();

    assert.success().code(0).stdout("99\n");

    Ok(())
}

#[test]
fn test_let_simple_regression() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = cargo::cargo_bin_cmd!("mq");

    let assert = cmd
        .arg("--unbuffered")
        .arg("-I")
        .arg("null")
        .arg("let a = 42 | a")
        .assert();

    assert.success().code(0).stdout("42\n");

    Ok(())
}

#[rstest]
// select() on markdown input replaces non-matching nodes with an empty
// value instead of dropping them, so --exit-status must treat that empty
// value as falsy, not just `None`/`false`.
#[case::truthy_match(vec!["--unbuffered", "--exit-status", "select(.h1)"], "# title\n\nbody", 0)]
#[case::no_match_is_falsy(vec!["--unbuffered", "-e", "select(.h2)"], "# title\n\nbody", 1)]
#[case::bare_false(vec!["--unbuffered", "-I", "null", "-e", "false"], "", 1)]
#[case::bare_null(vec!["--unbuffered", "-I", "null", "-e", "None"], "", 1)]
#[case::bare_truthy_number(vec!["--unbuffered", "-I", "null", "-e", "1"], "", 0)]
fn test_exit_status(
    #[case] args: Vec<&str>,
    #[case] input: &str,
    #[case] expected_code: i32,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = cargo::cargo_bin_cmd!("mq");
    let mut assert = cmd.args(args);

    if !input.is_empty() {
        assert = assert.write_stdin(input);
    }

    assert.assert().code(expected_code);

    Ok(())
}

#[rstest]
// With more files than `parallel_threshold`, batch processing fans out
// across rayon worker threads. --exit-status must aggregate truthiness
// across all of them, not just the main thread.
#[case::match_found(true, 0)]
#[case::no_match(false, 1)]
fn test_exit_status_parallel_batch(
    #[case] has_match: bool,
    #[case] expected_code: i32,
) -> Result<(), Box<dyn std::error::Error>> {
    let files: Vec<PathBuf> = (0..11)
        .map(|i| {
            let content = if has_match && i == 5 {
                "# YES\n"
            } else {
                "no heading here\n"
            };
            create_file(&format!("test_exit_status_parallel_batch_{has_match}_{i}.md"), content).1
        })
        .collect();
    let files_clone = files.clone();

    defer! {
        for file in &files_clone {
            if file.exists() {
                std::fs::remove_file(file).expect("Failed to delete temp file");
            }
        }
    }

    let mut cmd = cargo::cargo_bin_cmd!("mq");
    cmd.arg("--unbuffered").arg("--exit-status").arg("select(.h1)");
    for file in &files {
        cmd.arg(file.to_string_lossy().to_string());
    }
    cmd.assert().code(expected_code);

    Ok(())
}

#[rstest]
#[case::bash("bash", "_mq()")]
#[case::elvish("elvish", "edit:completion:arg-completer[mq]")]
#[case::fish("fish", "complete -c mq")]
#[case::nushell("nushell", "export extern mq")]
#[case::powershell("powershell", "Register-ArgumentCompleter")]
#[case::zsh("zsh", "#compdef mq")]
fn test_completion(#[case] shell: &str, #[case] expected_substring: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = cargo::cargo_bin_cmd!("mq");

    let assert = cmd.arg("completion").arg(shell).assert();
    let output = assert.success().code(0).get_output().stdout.clone();
    let output = String::from_utf8(output)?;

    assert!(
        output.contains(expected_substring),
        "expected completion script for {shell} to contain {expected_substring:?}, got:\n{output}"
    );

    Ok(())
}
