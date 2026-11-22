//! End-to-end tests that run the `mq-test` binary.

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

const PASSING: &str = "def test_ok():\n  assert_eq(1, 1)\nend\n";
const FAILING: &str = "def test_broken():\n  assert_eq(1, 2)\nend\n";

struct Workspace(PathBuf);

impl Workspace {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("mq_test_cli_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn write(&self, path: &str, content: &str) {
        let path = self.0.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    fn path(&self, path: &str) -> PathBuf {
        self.0.join(path)
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_mq-test"))
            .args(args)
            .current_dir(&self.0)
            .output()
            .unwrap()
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn bare_subcommand_name_runs_the_directory_of_that_name() {
    let ws = Workspace::new("dir_collision");
    ws.write("list/failing.mq", FAILING);

    let output = ws.run(&["list"]);

    assert!(
        !output.status.success(),
        "the failing test in list/ must run: {}",
        stdout(&output)
    );
}

#[test]
fn explicit_dot_forces_the_subcommand() {
    let ws = Workspace::new("force_subcommand");
    ws.write("list/failing.mq", FAILING);

    let output = ws.run(&["list", "."]);

    assert!(output.status.success());
    assert!(stdout(&output).contains("broken"));
}

#[test]
fn bare_subcommand_without_directory_is_the_subcommand() {
    let ws = Workspace::new("no_collision");
    ws.write("failing.mq", FAILING);

    let output = ws.run(&["list"]);

    assert!(output.status.success());
    assert!(stdout(&output).contains("broken"));
}

#[test]
fn coverage_writes_artifact_and_prints_json_test_results() {
    let ws = Workspace::new("coverage_json");
    ws.write("module.mq", "def double(x): x * 2 end\n");
    ws.write(
        "tests.mq",
        "include \"module\"\ndef test_double():\n  assert_eq(double(2), 4)\nend\n",
    );

    let output = ws.run(&[
        "coverage",
        "--format",
        "lcov",
        "--output",
        "lcov.info",
        "--test-format",
        "json",
        "tests.mq",
    ]);

    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let lcov = fs::read_to_string(ws.path("lcov.info")).unwrap();
    assert!(lcov.contains("SF:"), "lcov artifact expected: {lcov}");
    let out = stdout(&output);
    assert!(
        out.trim_start().starts_with(['{', '[']),
        "JSON test results expected: {out}"
    );
    assert!(out.contains("\"name\": \"double\""));
}

#[test]
fn coverage_without_output_keeps_stdout_a_single_json_document() {
    let ws = Workspace::new("coverage_json_stdout");
    ws.write("tests.mq", PASSING);

    for coverage_format in ["text", "json"] {
        let output = ws.run(&[
            "coverage",
            "--format",
            coverage_format,
            "--test-format",
            "json",
            "tests.mq",
        ]);

        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        serde_json::from_str::<serde_json::Value>(&stdout(&output))
            .unwrap_or_else(|e| panic!("stdout must be one JSON document ({coverage_format}): {e}"));
    }
}

#[test]
fn coverage_format_only_changes_the_artifact() {
    let ws = Workspace::new("coverage_text");
    ws.write("tests.mq", PASSING);

    let output = ws.run(&["coverage", "--format", "lcov", "--output", "lcov.info", "tests.mq"]);

    assert!(output.status.success());
    assert!(Path::new(&ws.path("lcov.info")).exists());
    assert!(!stdout(&output).trim_start().starts_with(['{', '[']));
}

#[test]
fn legacy_coverage_flags_still_combine_both_formats() {
    let ws = Workspace::new("legacy");
    ws.write("tests.mq", PASSING);

    let output = ws.run(&[
        "--coverage",
        "--coverage-format",
        "lcov",
        "--coverage-output",
        "lcov.info",
        "--format",
        "json",
        "tests.mq",
    ]);

    assert!(output.status.success());
    assert!(ws.path("lcov.info").exists());
    assert!(stdout(&output).trim_start().starts_with(['{', '[']));
}
