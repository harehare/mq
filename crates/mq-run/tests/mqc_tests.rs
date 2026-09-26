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
        .arg("compile")
        .args(flags)
        .args(["-f", "query.mq", "-o", "query.mqc"])
        .assert()
        .success();
}

#[rstest]
#[case::selector(".h2", "# a\n\n## b\n", &[], &[])]
#[case::function("def shout(x): upcase(x); | .h | to_text() | shout()", "# a\n\n## b\n", &[], &[])]
#[case::update(".h | upcase()", "# a\n\ntext\n", &[], &["-U"])]
#[case::json_output(".h", "# a\n", &[], &["-F", "json"])]
#[case::aggregate("len()", "# a\n\ntext\n", &["-A"], &["-A"])]
#[case::csv(r#"."a""#, "a,b\n1,2\n", &["-I", "csv"], &["-I", "csv"])]
#[case::text_stream("upcase()", "a\nb\n", &["-I", "text"], &["-I", "text", "--stream"])]
fn test_run_matches_source_query(
    #[case] query: &str,
    #[case] input: &str,
    #[case] compile_flags: &[&str],
    #[case] run_flags: &[&str],
) {
    let dir = TempDir::new().unwrap();
    compile(dir.path(), query, compile_flags);
    let input_file = write(dir.path(), "input.txt", input);

    let expected = stdout(mq(dir.path()).args(run_flags).arg(query).arg(&input_file).assert());
    let actual = stdout(
        mq(dir.path())
            .args(run_flags)
            .args(["query.mqc"])
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
            .args(["query.mqc", "-F", "json"])
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
    let output = stdout(mq(dir.path()).args(["query.mqc"]).args(&files).assert());
    let mut lines: Vec<&str> = output.lines().collect();
    lines.sort_unstable();
    let expected: Vec<String> = (0..15).map(|i| format!("f{i:02}")).collect();
    assert_eq!(lines, expected);
}

#[test]
fn test_compile_permissions_apply_to_module_level_lets() {
    let dir = TempDir::new().unwrap();
    let modules = dir.path().join("modules");
    fs::create_dir(&modules).unwrap();
    write(
        &modules,
        "m.mq",
        "def get(): data;\nlet data = read_file(\"data.txt\");\n",
    );
    write(dir.path(), "data.txt", "hello");
    write(dir.path(), "query.mq", r#"import "m" | m::get()"#);

    let error = stderr(
        mq(dir.path())
            .args(["compile", "-L", "modules", "-f", "query.mq"])
            .assert(),
    );
    assert!(error.contains("filesystem reads are disabled"), "{error}");

    mq(dir.path())
        .args(["compile", "-L", "modules", "-R", "-f", "query.mq"])
        .assert()
        .success();
    fs::remove_file(dir.path().join("data.txt")).unwrap();
    let output = stdout(mq(dir.path()).args(["query.mqc", "-I", "null"]).assert());
    assert_eq!(output.trim(), "hello");
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

    let output = stdout(mq(dir.path()).args(["query.mqc"]).write_stdin("# hi\n").assert());
    assert_eq!(output.trim(), "hi!?");
}

#[test]
fn test_run_reads_args_at_run_time() {
    let dir = TempDir::new().unwrap();
    compile(dir.path(), r#"s"hello ${name}""#, &[]);
    let output = stdout(
        mq(dir.path())
            .args(["query.mqc", "-I", "null", "--args", "name", "mq"])
            .assert(),
    );
    assert_eq!(output.trim(), "hello mq");
}

#[test]
fn test_runtime_error_shows_original_source() {
    let dir = TempDir::new().unwrap();
    compile(dir.path(), "def f(x): x / 0; | f(\"a\")", &[]);
    let error = stderr(mq(dir.path()).args(["query.mqc", "-I", "null"]).assert());
    assert!(error.contains("def f(x): x / 0;"), "{error}");
}

#[rstest]
#[case::format_mismatch(&[], &["-I", "csv"], "compiled for markdown input")]
#[case::aggregate_mismatch(&["-A"], &[], "-A/--aggregate must match")]
#[case::module_flag_at_run(&[], &["-L", "modules"], "Pass them to `mq compile` instead")]
fn test_run_rejects_mismatched_flags(
    #[case] compile_flags: &[&str],
    #[case] run_flags: &[&str],
    #[case] message: &str,
) {
    let dir = TempDir::new().unwrap();
    compile(dir.path(), "self", compile_flags);
    let error = stderr(
        mq(dir.path())
            .args(["query.mqc"])
            .args(run_flags)
            .write_stdin("a,b\n")
            .assert(),
    );
    assert!(error.contains(message), "{error}");
}

#[rstest]
#[case::compile_mqc_without_output(&["compile", "-f", "query.mqc"], "pass -o/--output")]
#[case::compile_query_string_without_output(&["compile", ".h"], "output file for a query string")]
#[case::compile_without_query(&["compile", "-o", "out.mqc"], "<QUERY OR FILE>")]
#[case::compile_flags_before_subcommand(&["-A", "compile", "-f", "query.mq"], "Pass compile options after `compile`")]
#[case::compile_from_file_before_subcommand(&["-f", "compile", "query.mq"], "Pass compile options after `compile`")]
#[case::compile_global_format(&["-T", "csv", "compile", "-f", "query.mq"], "-T/--format has no effect on `mq compile`")]
#[case::compile_output_is_query(&["compile", "-f", "query.mq", "-o", "query.mq"], "is the query file")]
#[case::compile_output_is_query_alias(&["compile", "-f", "query.mq", "-o", "./sub/../query.mq"], "is the query file")]
#[case::compile_runtime_flag(&["compile", "--stream", "-f", "query.mq"], "unexpected argument '--stream'")]
#[case::mqc_extension_not_bytecode(&["source.mqc"], "not an mq bytecode file")]
#[case::from_file_mqc(&["-f", "source.mqc"], "-f does not accept .mqc files")]
fn test_usage_errors(#[case] args: &[&str], #[case] message: &str) {
    let dir = TempDir::new().unwrap();
    write(dir.path(), "query.mq", ".h");
    write(dir.path(), "source.mqc", ".h");
    fs::create_dir(dir.path().join("sub")).unwrap();
    let error = stderr(mq(dir.path()).args(args).assert());
    assert!(error.contains(message), "{error}");
    assert_eq!(fs::read_to_string(dir.path().join("query.mq")).unwrap(), ".h");
}

#[rstest]
#[case::same_directory("query.mq", "query.mqc")]
#[case::no_extension("query", "query.mqc")]
#[case::subdirectory("sub/query.mq", "sub/query.mqc")]
fn test_compile_defaults_output_to_mqc_extension(#[case] query_file: &str, #[case] expected: &str) {
    let dir = TempDir::new().unwrap();
    fs::create_dir(dir.path().join("sub")).unwrap();
    write(dir.path(), query_file, ".h");
    mq(dir.path()).args(["compile", "-f", query_file]).assert().success();
    let output = stdout(mq(dir.path()).args([expected]).write_stdin("# a\n").assert());
    assert_eq!(output, "# a\n");
}

#[test]
fn test_compile_query_string() {
    let dir = TempDir::new().unwrap();
    mq(dir.path())
        .args(["compile", ".h | upcase()", "-o", "query.mqc"])
        .assert()
        .success();
    let output = stdout(mq(dir.path()).args(["query.mqc"]).write_stdin("# a\n").assert());
    assert_eq!(output, "# A\n");
}

#[test]
fn test_runs_mqc_given_as_query() {
    let dir = TempDir::new().unwrap();
    compile(dir.path(), ".h | upcase()", &[]);
    let output = stdout(mq(dir.path()).args(["query.mqc"]).write_stdin("# a\n").assert());
    assert_eq!(output, "# A\n");
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

    let error = stderr(mq(dir.path()).args(["query.mqc"]).write_stdin("# a\n").assert());
    assert!(error.contains("checksum mismatch"), "{error}");
}

#[test]
fn test_run_rejects_watch() {
    let dir = TempDir::new().unwrap();
    compile(dir.path(), "self", &[]);
    let input = write(dir.path(), "input.md", "# a\n");

    let error = stderr(mq(dir.path()).args(["query.mqc", "--watch"]).arg(&input).assert());
    assert!(error.contains("--watch does not support"), "{error}");
}

/// Joins miette's wrapped lines so a message matches regardless of where it was wrapped.
fn message(error: &str) -> String {
    error
        .split_whitespace()
        .filter(|word| *word != "│")
        .collect::<Vec<_>>()
        .join(" ")
}

fn source_output(dir: &Path, query: &str, args: &[&str], stdin: &str) -> String {
    stdout(mq(dir).args(args).arg(query).write_stdin(stdin).assert())
}

fn run_output(dir: &Path, args: &[&str], stdin: &str) -> String {
    stdout(mq(dir).args(["query.mqc"]).args(args).write_stdin(stdin).assert())
}

#[rstest]
fn test_run_checks_native_input_format(
    #[values("markdown", "mdx", "html", "text", "raw", "null", "bytes")] compiled: &str,
    #[values("markdown", "mdx", "html", "text", "raw", "null", "bytes")] run: &str,
) {
    let dir = TempDir::new().unwrap();
    compile(dir.path(), "self", &["-I", compiled]);
    if compiled == run {
        assert_eq!(
            run_output(dir.path(), &["-I", run], "# a\n"),
            source_output(dir.path(), "self", &["-I", run], "# a\n")
        );
        return;
    }
    let error = message(&stderr(
        mq(dir.path())
            .args(["query.mqc", "-I", run])
            .write_stdin("# a\n")
            .assert(),
    ));
    let expected = format!("compiled for {compiled} input, but stdin is read as {run} input");
    assert!(error.contains(&expected), "{error}");
}

#[rstest]
fn test_run_checks_parsed_input_format(
    #[values("csv", "tsv", "json", "yaml", "toml", "xml")] compiled: &str,
    #[values("csv", "tsv", "json", "yaml", "toml", "xml", "markdown", "raw")] run: &str,
) {
    if compiled == run {
        return;
    }
    let dir = TempDir::new().unwrap();
    compile(dir.path(), "self", &["-I", compiled]);
    let error = message(&stderr(
        mq(dir.path())
            .args(["query.mqc", "-I", run])
            .write_stdin("a\n")
            .assert(),
    ));
    let expected = format!("compiled for {compiled} input, but stdin is read as {run} input");
    assert!(error.contains(&expected), "{error}");
}

#[rstest]
fn test_run_without_compile_time_format_accepts_native_formats(
    #[values("markdown", "mdx", "html", "text", "raw", "null", "bytes")] run: &str,
) {
    let dir = TempDir::new().unwrap();
    compile(dir.path(), "self", &[]);
    assert_eq!(
        run_output(dir.path(), &["-I", run], "# a\n"),
        source_output(dir.path(), "self", &["-I", run], "# a\n")
    );
}

#[rstest]
fn test_run_without_compile_time_format_rejects_parsed_formats(
    #[values("csv", "tsv", "psv", "json", "yaml", "toml", "xml", "gron", "toon", "cbor")] run: &str,
) {
    let dir = TempDir::new().unwrap();
    compile(dir.path(), "self", &[]);
    let error = message(&stderr(
        mq(dir.path())
            .args(["query.mqc", "-I", run])
            .write_stdin("a\n")
            .assert(),
    ));
    let expected = format!("compiled for markdown input, but stdin is read as {run} input");
    assert!(error.contains(&expected), "{error}");
}

#[rstest]
#[case::txt_is_raw("raw", Some("input.txt"), None)]
#[case::log_is_raw("raw", Some("input.log"), None)]
#[case::jsonl_is_text("text", Some("input.jsonl"), None)]
#[case::html("html", Some("input.html"), None)]
#[case::md_is_markdown("markdown", Some("input.md"), None)]
#[case::unknown_extension_is_markdown("markdown", Some("input.unknown"), None)]
#[case::stdin_is_markdown("markdown", None, None)]
#[case::md_is_not_raw("raw", Some("input.md"), Some("markdown"))]
#[case::txt_is_not_markdown("markdown", Some("input.txt"), Some("raw"))]
#[case::html_is_not_markdown("markdown", Some("input.html"), Some("html"))]
#[case::stdin_is_not_raw("raw", None, Some("markdown"))]
fn test_run_checks_format_inferred_from_extension(
    #[case] compiled: &str,
    #[case] file: Option<&str>,
    #[case] inferred_mismatch: Option<&str>,
) {
    let dir = TempDir::new().unwrap();
    compile(dir.path(), "self", &["-I", compiled]);
    let mut command = mq(dir.path());
    command.args(["query.mqc"]).write_stdin("# a\n");
    if let Some(file) = file {
        command.arg(write(dir.path(), file, "# a\n"));
    }
    match inferred_mismatch {
        None => {
            command.assert().success();
        }
        Some(inferred) => {
            let target = file.unwrap_or("stdin");
            let error = message(&stderr(command.assert()));
            let expected = format!("compiled for {compiled} input, but");
            assert!(error.contains(&expected), "{error}");
            assert!(
                error.contains(&format!("{target} is read as {inferred} input")),
                "{error}"
            );
        }
    }
}

#[rstest]
#[case::sequential(&[], "c.txt is read as raw input")]
#[case::parallel(&["-P", "1"], "c.txt is read as raw input")]
#[case::stream(&["--stream"], "c.txt is read as raw input")]
#[case::eval_all(&["--eval-all"], "--eval-all requires all input files to use the same input format")]
fn test_run_checks_every_input_file(#[case] mode: &[&str], #[case] expected: &str) {
    let dir = TempDir::new().unwrap();
    compile(dir.path(), "self", &["-I", "markdown"]);
    let files = [
        write(dir.path(), "a.md", "# a\n"),
        write(dir.path(), "b.md", "# b\n"),
        write(dir.path(), "c.txt", "# c\n"),
    ];
    let error = message(&stderr(
        mq(dir.path()).args(["query.mqc"]).args(mode).args(&files).assert(),
    ));
    assert!(error.contains(expected), "{error}");
}

#[rstest]
#[case::sequential(&[])]
#[case::parallel(&["-P", "1"])]
#[case::stream(&["--stream"])]
#[case::eval_all(&["--eval-all"])]
fn test_run_accepts_files_matching_compiled_format(#[case] mode: &[&str]) {
    let dir = TempDir::new().unwrap();
    compile(dir.path(), ".h | to_text()", &["-I", "markdown"]);
    let files: Vec<PathBuf> = ["a.md", "b.markdown", "c.unknown"]
        .iter()
        .map(|name| write(dir.path(), name, &format!("# {name}\n")))
        .collect();
    let sorted_lines = |output: String| {
        let mut lines: Vec<String> = output.lines().map(str::to_string).collect();
        lines.sort_unstable();
        lines
    };
    let expected = stdout(mq(dir.path()).args(mode).arg(".h | to_text()").args(&files).assert());
    let actual = stdout(mq(dir.path()).args(["query.mqc"]).args(mode).args(&files).assert());
    assert_eq!(sorted_lines(actual), sorted_lines(expected));
}

#[rstest]
#[case::inline_module("module m: let label = arg end | m::label")]
#[case::imported_module(r#"import "labels" | labels::label"#)]
#[case::included_module(r#"include "labels" | label"#)]
fn test_compile_rejects_runtime_names_in_module_let(#[case] query: &str) {
    let dir = TempDir::new().unwrap();
    let modules = dir.path().join("modules");
    fs::create_dir(&modules).unwrap();
    write(&modules, "labels.mq", "let label = arg;\n");
    write(dir.path(), "query.mq", query);

    let error = message(&stderr(
        mq(dir.path())
            .args(["compile", "-L", "modules", "-f", "query.mq", "-o", "query.mqc"])
            .assert(),
    ));
    assert!(
        error.contains(r#""arg" is not defined when module-level `let`s are computed"#),
        "{error}"
    );
    assert!(!dir.path().join("query.mqc").exists());
}

#[rstest]
#[case::top_level_let("let label = arg | label")]
#[case::inline_module_function("module m: def label(): arg; end | m::label()")]
#[case::imported_module_function(r#"import "labels" | labels::label()"#)]
#[case::included_module_function(r#"include "labels" | label()"#)]
fn test_run_reads_runtime_names_outside_module_let(#[case] query: &str) {
    let dir = TempDir::new().unwrap();
    let modules = dir.path().join("modules");
    fs::create_dir(&modules).unwrap();
    write(&modules, "labels.mq", "def label(): arg;\n");
    compile(dir.path(), query, &["-L", "modules"]);
    fs::remove_dir_all(&modules).unwrap();

    for value in ["hello", "world"] {
        let output = stdout(
            mq(dir.path())
                .args(["query.mqc", "-I", "null", "--args", "arg", value])
                .assert(),
        );
        assert_eq!(output.trim(), value);
    }
}

#[cfg(feature = "debug-trace")]
#[rstest]
#[case::builtin_call("upcase()", &["phase: main", "CallBuiltin upcase"])]
#[case::nodes_split(".h | nodes | len()", &["phase: per-input", "phase: nodes aggregate"])]
#[case::imported_module(r#"import "csv" | csv::csv_parse("a,b\n1,2", true)"#, &["CallBuiltin _csv_parse"])]
fn test_run_dumps_loaded_bytecode(#[case] query: &str, #[case] expected: &[&str]) {
    let dir = TempDir::new().unwrap();
    compile(dir.path(), query, &[]);
    let output = mq(dir.path())
        .args(["--dump-bytecode", "query.mqc"])
        .write_stdin("# a\n")
        .assert()
        .success();
    let dump = String::from_utf8(output.get_output().stderr.clone()).unwrap();
    for text in expected {
        assert!(dump.contains(text), "missing {text:?} in:\n{dump}");
    }
    assert!(!dump.contains("StmtBoundary"), "{dump}");
}

#[rstest]
#[case::imported_module(r#"import "csv" | csv::csv_parse(true)"#)]
#[case::included_module(r#"include "csv" | csv_parse(true)"#)]
fn test_runtime_error_in_standard_module_matches_source(#[case] query: &str) {
    let dir = TempDir::new().unwrap();
    compile(dir.path(), query, &[]);
    let expected = stderr(mq(dir.path()).arg(query).write_stdin("# a\n").assert());
    let actual = stderr(mq(dir.path()).args(["query.mqc"]).write_stdin("# a\n").assert());
    assert_eq!(actual, expected);
}
