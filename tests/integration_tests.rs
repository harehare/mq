use assert_cmd::Command;
use mq_test::defer;
use rstest::rstest;

#[test]
fn test_cli_run_with_stdin() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("mq")?;

    let assert = cmd
        .arg("--unbuffered")
        .arg(".h | select(contains(\"title\")?)")
        .write_stdin("# **title**\n\n- test1\n- test2")
        .assert();
    assert.success().code(0).stdout("# **title**\n");

    Ok(())
}

#[rstest]
#[case::json(
    vec!["--unbuffered", "-F", "json", ".code_inline"],
    "`inline code`",
    r#"[
  {
    "type": "CodeInline",
    "value": "inline code"
  }
]"#
)]
#[case::args(
    vec!["--unbuffered", "--args", "val1", "test", "select(contains(val1))"],
    "# **title**\n\n- test1\n- test2",
    "- test1\n- test2\n"
)]
#[case::completion(
    vec!["completion", "--shell", "zsh"],
    "",
    ""
)]
#[case::format(
    vec!["fmt"],
    "def test(x):\nadd(x,1);\n| map(array(1,2,3),test)",
    "def test(x):\n  add(x, 1);\n| map(array(1, 2, 3), test)\n"
)]
#[case::docs(
    vec!["docs"],
    "",
    ""
)]
#[case::update_file(
    vec!["--unbuffered", "--update", ".h | select(contains(\"title\")?) | ltrimstr(\"titl\")"],
    "# **title**\n\n- test1\n- test2",
    "# **e**\n\n- test1\n- test2\n"
)]
#[case::update_nested(
    vec!["--unbuffered", "--update", ".strong | select(contains(\"title\")?) | ltrimstr(\"titl\")"],
    "# [**title**](url)\n\n- test1\n- test2",
    "# [**e**](url)\n\n- test1\n- test2\n"
)]
#[case::null_input(
    vec!["--unbuffered", "--null-input", "1 | add(2)"],
    "",
    "3\n"
)]
#[case::nested_item(
    vec!["--unbuffered", "--update" , "if (and(or(.link, .definition), matches_url(\"a/b/c.html\"))): update(\"x/y/z.html\")"],
    "- another item\n\n  [another link]: a/b/c.html",
    "- another item\n\n  [another link]: x/y/z.html\n"
)]
#[case::nested_item(
    vec!["--unbuffered", "--update" , ".code_inline | update(\"test\")"],
    "# `title`\n# `title`",
    "# `test`\n# `test`\n"
)]
#[case::nested_item(
    vec!["--unbuffered", "--update" , "if (and(or(.link, .link_ref, .definition), matches_url(\"a/b/c.html\"))):\nupdate(\"x/y/z.html\")"],
    "- item\n\n  [another link]: <a/b/c.html> \"this\n  is a title\"\n\n<!-- -->\n\n    [link2](a/b/c.html)\n    test\n",
    "- item\n\n  [another link]: x/y/z.html \"this\n  is a title\"\n\n<!-- -->\n\n    [link2](a/b/c.html)\n    test\n",
)]
#[case::nested_item(
    vec!["--unbuffered", "--update", "--link-title-style", "paren", "--link-url-style", "angle", "if (and(or(.link, .link_ref, .definition), matches_url(\"a/b/c.html\"))):\nupdate(\"x/y/z.html\")"],
    "- item\n\n  [another link]: <a/b/c.html> (this  is a title)\n",
    "- item\n\n  [another link]: <x/y/z.html> (this  is a title)\n",
)]
fn test_cli_commands(
    #[case] args: Vec<&str>,
    #[case] input: &str,
    #[case] expected_output: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("mq")?;
    let mut assert = cmd.args(args);

    if !input.is_empty() {
        assert = assert.write_stdin(input);
    }

    let assert = assert.assert();

    if !expected_output.is_empty() {
        assert.success().code(0).stdout(expected_output.to_owned());
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
        .arg(".h | select(contains(\"title\")?)")
        .arg(temp_file_path.to_string_lossy().to_string())
        .assert();

    assert.success().code(0).stdout("# **title**\n");
    Ok(())
}

#[test]
fn test_cli_run_with_query_from_file() -> Result<(), Box<dyn std::error::Error>> {
    let (_, temp_file_path) = mq_test::create_file(
        "test_cli_run_with_query_from_file.mq",
        ".h | select(contains(\"title\")?)",
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
