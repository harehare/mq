use assert_cmd::cargo;

#[test]
fn test_stdin_stdout_formats_and_prints_to_stdout() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = cargo::cargo_bin_cmd!("mq-fmt");

    cmd.arg("-")
        .write_stdin("def foo():1;")
        .assert()
        .success()
        .stdout("def foo(): 1;");

    Ok(())
}

#[test]
fn test_stdin_check_passes_when_already_formatted() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = cargo::cargo_bin_cmd!("mq-fmt");

    cmd.arg("--check")
        .arg("-")
        .write_stdin("def foo(): 1;\n")
        .assert()
        .success()
        .stdout("");

    Ok(())
}

#[test]
fn test_stdin_check_fails_when_not_formatted() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = cargo::cargo_bin_cmd!("mq-fmt");

    cmd.arg("--check")
        .arg("-")
        .write_stdin("def foo():1;")
        .assert()
        .failure()
        .code(1);

    Ok(())
}
