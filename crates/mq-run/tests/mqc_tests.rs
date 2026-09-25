use assert_cmd::cargo;
use rstest::rstest;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn mq(dir: &Path) -> assert_cmd::Command {
    let mut cmd = cargo::cargo_bin_cmd!("mq");
    cmd.current_dir(dir);
    cmd
}

fn write(dir: &Path, name: &str, content: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, content).unwrap();
    path
}

fn stdout(assert: assert_cmd::assert::Assert) -> String {
    String::from_utf8(assert.success().get_output().stdout.clone()).unwrap()
}

fn stderr(assert: assert_cmd::assert::Assert) -> String {
    String::from_utf8(assert.failure().get_output().stderr.clone()).unwrap()
}

fn compile(dir: &Path, query: &str, flags: &[&str]) {
    write(dir, "query.mq", query);
    mq(dir)
        .args(flags)
        .args(["compile", "query.mq", "-o", "query.mqc"])
        .assert()
        .success();
}

#[rstest]
#[case::selector(".h2", "# a\n\n## b\n", &[])]
#[case::function("def shout(x): upcase(x); | .h | to_text() | shout()", "# a\n\n## b\n", &[])]
#[case::update(".h | upcase()", "# a\n\ntext\n", &["-U"])]
#[case::json_output(".h", "# a\n", &["-F", "json"])]
#[case::aggregate("len()", "# a\n\ntext\n", &["-A"])]
#[case::csv(r#"."a""#, "a,b\n1,2\n", &["-I", "csv"])]
#[case::text_stream("upcase()", "a\nb\n", &["-I", "text", "--stream"])]
fn test_run_matches_source_query(#[case] query: &str, #[case] input: &str, #[case] flags: &[&str]) {
    let dir = TempDir::new().unwrap();
    compile(dir.path(), query, flags);
    let input_file = write(dir.path(), "input.txt", input);

    let expected = stdout(mq(dir.path()).args(flags).arg(query).arg(&input_file).assert());
    let actual = stdout(
        mq(dir.path())
            .args(flags)
            .args(["run", "query.mqc"])
            .arg(&input_file)
            .assert(),
    );
    assert_eq!(actual, expected);
}

#[test]
fn test_run_reads_stdin_and_flags_after_program() {
    let dir = TempDir::new().unwrap();
    compile(dir.path(), ".h | to_text()", &[]);
    let output = stdout(
        mq(dir.path())
            .args(["run", "query.mqc", "-F", "json"])
            .write_stdin("# title\n")
            .assert(),
    );
    assert!(output.contains(r#""value": "title""#), "{output}");
}

#[test]
fn test_run_many_files_in_parallel() {
    let dir = TempDir::new().unwrap();
    compile(dir.path(), ".h | to_text()", &[]);
    let files: Vec<PathBuf> = (0..15)
        .map(|i| write(dir.path(), &format!("f{i:02}.md"), &format!("# f{i:02}\n")))
        .collect();
    let output = stdout(mq(dir.path()).args(["run", "query.mqc"]).args(&files).assert());
    let mut lines: Vec<&str> = output.lines().collect();
    lines.sort_unstable();
    let expected: Vec<String> = (0..15).map(|i| format!("f{i:02}")).collect();
    assert_eq!(lines, expected);
}

#[test]
fn test_run_needs_no_module_files() {
    let dir = TempDir::new().unwrap();
    let modules = dir.path().join("modules");
    fs::create_dir(&modules).unwrap();
    write(&modules, "util.mq", "def exclaim(s): s + \"!\";\nlet suffix = \"?\";\n");
    compile(
        dir.path(),
        r#"import "util" | .h | to_text() | util::exclaim() + util::suffix"#,
        &["-L", "modules"],
    );
    fs::remove_dir_all(&modules).unwrap();

    let output = stdout(mq(dir.path()).args(["run", "query.mqc"]).write_stdin("# hi\n").assert());
    assert_eq!(output.trim(), "hi!?");
}

#[test]
fn test_run_reads_args_at_run_time() {
    let dir = TempDir::new().unwrap();
    compile(dir.path(), r#"s"hello ${name}""#, &[]);
    let output = stdout(
        mq(dir.path())
            .args(["run", "query.mqc", "-I", "null", "--args", "name", "mq"])
            .assert(),
    );
    assert_eq!(output.trim(), "hello mq");
}

#[test]
fn test_runtime_error_shows_original_source() {
    let dir = TempDir::new().unwrap();
    compile(dir.path(), "def f(x): x / 0; | f(\"a\")", &[]);
    let error = stderr(mq(dir.path()).args(["run", "query.mqc", "-I", "null"]).assert());
    assert!(error.contains("def f(x): x / 0;"), "{error}");
}

#[rstest]
#[case::format_mismatch(&[], &["-I", "csv"], "compiled for markdown input")]
#[case::aggregate_mismatch(&["-A"], &[], "-A/--aggregate must match")]
#[case::module_flag_at_run(&[], &["-L", "modules"], "Pass it to `mq compile` instead")]
fn test_run_rejects_mismatched_flags(
    #[case] compile_flags: &[&str],
    #[case] run_flags: &[&str],
    #[case] message: &str,
) {
    let dir = TempDir::new().unwrap();
    compile(dir.path(), "self", compile_flags);
    let error = stderr(
        mq(dir.path())
            .args(["run", "query.mqc"])
            .args(run_flags)
            .write_stdin("a,b\n")
            .assert(),
    );
    assert!(error.contains(message), "{error}");
}

#[rstest]
#[case::compile_without_output(&["compile", "query.mq"], "requires -o/--output")]
#[case::compile_without_query(&["compile", "-o", "out.mqc"], "Usage: mq compile")]
#[case::run_without_program(&["run"], "Usage: mq run")]
#[case::run_source_file(&["run", "query.mq"], "not an mq bytecode file")]
fn test_usage_errors(#[case] args: &[&str], #[case] message: &str) {
    let dir = TempDir::new().unwrap();
    write(dir.path(), "query.mq", ".h");
    let error = stderr(mq(dir.path()).args(args).assert());
    assert!(error.contains(message), "{error}");
}

#[test]
fn test_run_rejects_corrupted_program() {
    let dir = TempDir::new().unwrap();
    compile(dir.path(), ".h", &[]);
    let path = dir.path().join("query.mqc");
    let mut bytes = fs::read(&path).unwrap();
    let middle = bytes.len() / 2;
    bytes[middle] ^= 0xFF;
    fs::write(&path, bytes).unwrap();

    let error = stderr(mq(dir.path()).args(["run", "query.mqc"]).write_stdin("# a\n").assert());
    assert!(error.contains("checksum mismatch"), "{error}");
}
