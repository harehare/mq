use assert_cmd::Command;
use mq_test::defer;
use rstest::rstest;

#[test]
fn test_cli_run_with_stdin() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("mq")?;

    let assert = cmd
        .arg("--unbuffered")
        .arg(r#".h | select(contains("title")?)"#)
        .write_stdin("# **title**\n\n- test1\n- test2")
        .assert();
    assert.success().code(0).stdout("# **title**\n");

    Ok(())
}

// Tests for --output-ast-json and --execute-ast-json
// This module is added inside the existing tests module
#[cfg(feature = "ast-json")]
mod test_cli_ast_options {
    // No need for super::* if Command and NamedTempFile are brought into scope directly
    use std::fs;
    use assert_cmd::prelude::*;
    use predicates::prelude::*;
    use tempfile::NamedTempFile;
    use rstest::rstest;

    #[rstest]
    fn test_output_ast_json() {
        let mut cmd = std::process::Command::cargo_bin("mq").unwrap();
        let temp_ast_file = NamedTempFile::new().unwrap();
        let ast_file_path = temp_ast_file.path().to_str().unwrap();

        cmd.arg("'add(1, 2)'")
           .arg("--output-ast-json")
           .arg(ast_file_path);

        cmd.assert().success();

        let ast_json_content = fs::read_to_string(ast_file_path).unwrap();
        assert!(ast_json_content.contains("\"Call\""));
        assert!(ast_json_content.contains("\"name\":\"add\""));
        assert!(ast_json_content.contains("{\"Literal\":{\"Number\":1.0}}"));
        assert!(ast_json_content.contains("{\"Literal\":{\"Number\":2.0}}"));
    }

    #[rstest]
    fn test_execute_ast_json() {
        let mut cmd = std::process::Command::cargo_bin("mq").unwrap();

        let ast_content = r#"[
            {
                "expr": {
                    "Call": [
                        { "name": "add" },
                        [
                            { "expr": { "Literal": { "Number": 10.0 } } },
                            { "expr": { "Literal": { "Number": 5.0 } } }
                        ],
                        false
                    ]
                }
            }
        ]"#;
        let mut temp_ast_file = NamedTempFile::new().unwrap();
        use std::io::Write;
        writeln!(temp_ast_file, "{}", ast_content).unwrap();
        let ast_file_path = temp_ast_file.path().to_str().unwrap();

        let mut temp_input_file = NamedTempFile::new().unwrap();
        writeln!(temp_input_file, "dummy input line").unwrap();
        let input_file_path = temp_input_file.path().to_str().unwrap();

        cmd.arg("--execute-ast-json")
           .arg(ast_file_path)
           .arg(input_file_path);

        cmd.assert()
           .success()
           .stdout(predicate::str::contains("15"));
    }

    #[rstest]
    fn test_output_and_execute_ast_json_mutual_exclusion() {
        let mut cmd = std::process::Command::cargo_bin("mq").unwrap();
        let temp_file1 = NamedTempFile::new().unwrap();
        let temp_file2 = NamedTempFile::new().unwrap();

        cmd.arg("--output-ast-json")
           .arg(temp_file1.path())
           .arg("--execute-ast-json")
           .arg(temp_file2.path())
           .arg("'dummy query'");

        cmd.assert()
           .failure()
           .stderr(predicate::str::contains("cannot be used together"));
    }

    #[rstest]
    fn test_execute_ast_json_with_update_should_fail() {
        let mut cmd = std::process::Command::cargo_bin("mq").unwrap();

        let ast_content = r#"[ { "expr": { "Literal": { "String": "test" } } } ]"#;
        let mut temp_ast_file = NamedTempFile::new().unwrap();
        use std::io::Write;
        writeln!(temp_ast_file, "{}", ast_content).unwrap();
        let ast_file_path = temp_ast_file.path().to_str().unwrap();

        let mut temp_input_file = NamedTempFile::new().unwrap();
        writeln!(temp_input_file, "dummy input line").unwrap();
        let input_file_path = temp_input_file.path().to_str().unwrap();

        cmd.arg("--execute-ast-json")
           .arg(ast_file_path)
           .arg("--update")
           .arg(input_file_path);

        cmd.assert()
           .failure()
           .stderr(predicate::str::contains("--update is not supported with --execute-ast-json"));
    }
}

// Tests for --output-ast-json and --execute-ast-json
mod test_cli_ast_options {
    // use super::*; // Not strictly needed if we qualify types, but can be convenient.
    use std::fs;
    use assert_cmd::prelude::*; // For Command::cargo_bin etc.
    use predicates::prelude::*; // For predicate::str::contains etc.
    use tempfile::NamedTempFile;
    use rstest::rstest; // If rstest is used within this module

    #[rstest]
    fn test_output_ast_json() {
        let mut cmd = std::process::Command::cargo_bin("mq").unwrap();
        let temp_ast_file = NamedTempFile::new().unwrap();
        let ast_file_path = temp_ast_file.path().to_str().unwrap();

        cmd.arg("'add(1, 2)'") // Query that results in a known AST structure
           .arg("--output-ast-json")
           .arg(ast_file_path);

        cmd.assert().success();

        let ast_json_content = fs::read_to_string(ast_file_path).unwrap();
        // More specific checks for the AST of 'add(1, 2)'
        assert!(ast_json_content.contains("\"Call\""));
        assert!(ast_json_content.contains("\"name\":\"add\"")); // The function name
        assert!(ast_json_content.contains("{\"Literal\":{\"Number\":1.0}}")); // Argument 1
        assert!(ast_json_content.contains("{\"Literal\":{\"Number\":2.0}}")); // Argument 2
    }

    #[rstest]
    fn test_execute_ast_json() {
        let mut cmd = std::process::Command::cargo_bin("mq").unwrap();

        // AST for 'add(10, 5)'
        // Note: token_id fields are omitted as they are defaulted during deserialization.
        let ast_content = r#"[
            {
                "expr": {
                    "Call": [
                        { "name": "add" },
                        [
                            { "expr": { "Literal": { "Number": 10.0 } } },
                            { "expr": { "Literal": { "Number": 5.0 } } }
                        ],
                        false
                    ]
                }
            }
        ]"#;
        let mut temp_ast_file = NamedTempFile::new().unwrap();
        use std::io::Write;
        writeln!(temp_ast_file, "{}", ast_content).unwrap();
        let ast_file_path = temp_ast_file.path().to_str().unwrap();

        let mut temp_input_file = NamedTempFile::new().unwrap();
        writeln!(temp_input_file, "dummy input line").unwrap();
        let input_file_path = temp_input_file.path().to_str().unwrap();

        cmd.arg("--execute-ast-json")
           .arg(ast_file_path)
           .arg(input_file_path); // Provide an input file

        cmd.assert()
           .success()
           .stdout(predicate::str::contains("15"));
    }

    #[rstest]
    fn test_output_and_execute_ast_json_mutual_exclusion() {
        let mut cmd = std::process::Command::cargo_bin("mq").unwrap();
        let temp_file1 = NamedTempFile::new().unwrap();
        let temp_file2 = NamedTempFile::new().unwrap();

        cmd.arg("--output-ast-json")
           .arg(temp_file1.path())
           .arg("--execute-ast-json")
           .arg(temp_file2.path())
           .arg("'dummy query'");

        cmd.assert()
           .failure()
           .stderr(predicate::str::contains("cannot be used together"));
    }

    #[rstest]
    fn test_execute_ast_json_with_update_should_fail() {
        let mut cmd = std::process::Command::cargo_bin("mq").unwrap();

        let ast_content = r#"[ { "expr": { "Literal": { "String": "test" } } } ]"#;
        let mut temp_ast_file = NamedTempFile::new().unwrap();
        use std::io::Write;
        writeln!(temp_ast_file, "{}", ast_content).unwrap();
        let ast_file_path = temp_ast_file.path().to_str().unwrap();

        let mut temp_input_file = NamedTempFile::new().unwrap();
        writeln!(temp_input_file, "dummy input line").unwrap();
        let input_file_path = temp_input_file.path().to_str().unwrap();

        cmd.arg("--execute-ast-json")
           .arg(ast_file_path)
           .arg("--update")
           .arg(input_file_path);

        cmd.assert()
           .failure()
           .stderr(predicate::str::contains("--update is not supported with --execute-ast-json"));
    }
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
    Some("def test(x):\n  add(x, 1);\n| map(array(1, 2, 3), test)\n")
)]
#[case::docs(
    vec!["docs"],
    "",
    None
)]
#[case::update_file(
    vec!["--unbuffered", "--update", r#".h | select(contains("title")?) | ltrimstr("titl")"#],
    "# **title**\n\n- test1\n- test2",
    Some("# **e**\n\n- test1\n- test2\n")
)]
#[case::update_nested(
    vec!["--unbuffered", "--update", r#".strong | select(contains("title")?) | ltrimstr("titl")"#],
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
fn test_cli_commands(
    #[case] args: Vec<&str>,
    #[case] input: &str,
    #[case] expected_output: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("mq")?;
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
    let (_, temp_file_path) = mq_test::create_file("input.txt", "test");
    let temp_file_path_clone = temp_file_path.clone();

    defer! {
        if temp_file_path_clone.exists() {
            std::fs::remove_file(&temp_file_path_clone).expect("Failed to delete temp file");
        }
    }

    let mut cmd = Command::cargo_bin("mq")?;

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
    let (_, temp_file_path) = mq_test::create_file(
        "test_cli_run_with_file_input.md",
        "# **title**\n\n- test1\n- test2",
    );
    let temp_file_path_clone = temp_file_path.clone();

    defer! {
        if temp_file_path_clone.exists() {
            std::fs::remove_file(&temp_file_path_clone).expect("Failed to delete temp file");
        }
    }

    let mut cmd = Command::cargo_bin("mq")?;
    let assert = cmd
        .arg("--unbuffered")
        .arg(r#".h | select(contains("title")?)"#)
        .arg(temp_file_path.to_string_lossy().to_string())
        .assert();

    assert.success().code(0).stdout("# **title**\n");
    Ok(())
}

#[test]
fn test_cli_run_with_query_from_file() -> Result<(), Box<dyn std::error::Error>> {
    let (_, temp_file_path) = mq_test::create_file(
        "test_cli_run_with_query_from_file.mq",
        r#".h | select(contains("title")?)"#,
    );
    let temp_file_path_clone = temp_file_path.clone();

    defer! {
        if temp_file_path_clone.exists() {
            std::fs::remove_file(&temp_file_path_clone).expect("Failed to delete temp file");
        }
    }

    let mut cmd = Command::cargo_bin("mq")?;
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
    let (_, temp_file_path) = mq_test::create_file("test_cli_run_with_csv_input.csv", csv_content);
    let temp_file_path_clone = temp_file_path.clone();

    defer! {
        if temp_file_path_clone.exists() {
            std::fs::remove_file(&temp_file_path_clone).expect("Failed to delete temp file");
        }
    }

    let mut cmd = Command::cargo_bin("mq")?;
    let assert = cmd
        .arg("--unbuffered")
        .arg("nodes | csv2table_with_header()")
        .arg(temp_file_path.to_string_lossy().to_string())
        .assert();

    assert.success().code(0).stdout(
        "|name|age|
|---|---|
|Alice|30|
|Bob|25|
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
    let (_, temp_file_path) =
        mq_test::create_file("test_cli_run_with_html_input.html", html_content);
    let temp_file_path_clone = temp_file_path.clone();

    defer! {
        if temp_file_path_clone.exists() {
            std::fs::remove_file(&temp_file_path_clone).expect("Failed to delete temp file");
        }
    }

    let mut cmd = Command::cargo_bin("mq")?;
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
    let (_, temp_file_path) = mq_test::create_file("test_cli_run_with_mdx_input.mdx", mdx_content);
    let temp_file_path_clone = temp_file_path.clone();

    defer! {
        if temp_file_path_clone.exists() {
            std::fs::remove_file(&temp_file_path_clone).expect("Failed to delete temp file");
        }
    }

    let mut cmd = Command::cargo_bin("mq")?;
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
fn test_cli_sections_n_with_file_input() -> Result<(), Box<dyn std::error::Error>> {
    let markdown_content = r#"
# Section 1

Content of section 1.

## Subsection 1.1

Content of subsection 1.1.

## Subsection 1.2

Content of subsection 1.2.

# Section 2

Content of section 2.

# Section 3

Content of section 3.
"#;
    let (_, temp_file_path) =
        mq_test::create_file("test_cli_sections_n_with_file_input.md", markdown_content);
    let temp_file_path_clone = temp_file_path.clone();

    defer! {
        if temp_file_path_clone.exists() {
            std::fs::remove_file(&temp_file_path_clone).expect("Failed to delete temp file");
        }
    }

    // Test extracting top-level sections (n=1)
    let mut cmd = Command::cargo_bin("mq")?;
    let assert = cmd
        .arg("--unbuffered")
        .arg("nodes | sections(1)")
        .arg(temp_file_path.to_string_lossy().to_string())
        .assert();

    let expected =
        "[# Section 1, Content of section 1., ## Subsection 1.1, Content of subsection 1.1., ## Subsection 1.2, Content of subsection 1.2.]
[# Section 2, Content of section 2.]
[# Section 3, Content of section 3.]
";
    assert.success().code(0).stdout(expected);

    // Test extracting second-level sections (n=2)
    let mut cmd = Command::cargo_bin("mq")?;
    let assert = cmd
        .arg("--unbuffered")
        .arg("nodes | sections(2)")
        .arg(temp_file_path.to_string_lossy().to_string())
        .assert();

    let expected = "[## Subsection 1.1, Content of subsection 1.1.]
[## Subsection 1.2, Content of subsection 1.2., # Section 2, Content of section 2., # Section 3, Content of section 3.]
";
    assert.success().code(0).stdout(expected);

    Ok(())
}
