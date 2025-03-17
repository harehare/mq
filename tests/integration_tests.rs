use assert_cmd::Command;

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
