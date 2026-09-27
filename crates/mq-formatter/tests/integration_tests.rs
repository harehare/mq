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

#[test]
fn test_bundled_mq_files_format_idempotently() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../mq-lang");
    let paths = ["builtin.mq", "builtin_tests.mq", "modules/*.mq"]
        .iter()
        .flat_map(|pattern| glob::glob(&format!("{root}/{pattern}")).unwrap())
        .map(Result::unwrap)
        .collect::<Vec<_>>();
    assert!(paths.len() > 2, "no bundled .mq files found under {root}");

    for max_width in [None, Some(60)] {
        let config = mq_formatter::FormatterConfig {
            max_width,
            ..Default::default()
        };
        for path in &paths {
            let code = std::fs::read_to_string(path).unwrap();
            let formatted = mq_formatter::Formatter::new(Some(config.clone()))
                .format(&code)
                .unwrap_or_else(|e| panic!("{}: {e:?}", path.display()));
            let (_, errors) = mq_lang::parse_recovery(&formatted);
            assert!(
                !errors.has_errors(),
                "{}: formatted output does not parse",
                path.display()
            );
            let again = mq_formatter::Formatter::new(Some(config.clone()))
                .format(&formatted)
                .unwrap();
            assert_eq!(
                again,
                formatted,
                "{}: not idempotent (max_width={max_width:?})",
                path.display()
            );
        }
    }
}
