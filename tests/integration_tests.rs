use assert_cmd::Command;
use mq_test::defer;

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

#[test]
fn test_cli_run_with_args_and_stdin() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("mq")?;

    let assert = cmd
        .arg("--unbuffered")
        .arg("--args")
        .arg("val1")
        .arg("test")
        .arg("select(contains(val1))")
        .write_stdin("# **title**\n\n- test1\n- test2")
        .assert();
    assert.success().code(0).stdout("- test1\n- test2\n");

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
fn test_cli_format_with_stdin() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("mq")?;

    let assert = cmd
        .arg("fmt")
        .write_stdin(
            "def test(x):
add(x,1);
map(array(1,2,3),test)",
        )
        .assert();
    assert.success().code(0).stdout(
        "def test(x):
  add(x, 1);
map(array(1, 2, 3), test)
",
    );

    Ok(())
}

#[test]
fn test_cli_md_format_with_stdin() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("mq")?;

    let assert = cmd
        .arg("fmt")
        .arg("--target")
        .arg("markdown")
        .write_stdin(
            "# test
- item1
- item2 ",
        )
        .assert();
    dbg!(&assert);
    assert
        .success()
        .code(0)
        .stdout("# test\n\n- item1\n- item2\n\n");

    Ok(())
}

#[test]
fn test_cli_docs() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("mq")?;
    let assert = cmd.arg("docs").assert();
    assert.success().code(0);

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

#[test]
fn test_cli_run_with_update_file() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("mq")?;

    let assert = cmd
        .arg("--unbuffered")
        .arg("--update")
        .arg(".h | select(contains(\"title\")?) | ltrimstr(\"titl\")")
        .write_stdin("# **title**\n\n- test1\n- test2")
        .assert();
    assert
        .success()
        .code(0)
        .stdout("# **e**\n\n- test1\n- test2\n");

    Ok(())
}

#[test]
fn test_cli_run_with_null_input() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("mq")?;

    let assert = cmd
        .arg("--unbuffered")
        .arg("--null-input")
        .arg("1 | add(2)")
        .assert();
    assert.success().code(0).stdout("3\n");

    Ok(())
}
