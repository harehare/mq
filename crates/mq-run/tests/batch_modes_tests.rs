//! Batch, parallel, count, stream, and `.mqc` runs must match running each file alone.
use assert_cmd::cargo;
use rstest::rstest;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

const MARKDOWN: &[&str] = &["a0.md", "a1.md", "a2.md", "a3.md", "a4.md", "a5.md", "a6.md", "a7.md"];
const CSV: &[&str] = &["c0.csv", "c1.csv", "c2.csv"];
const TEXT: &[&str] = &["t0.txt", "t1.txt", "t2.txt"];

fn mq(dir: &Path) -> assert_cmd::Command {
    let mut cmd = cargo::cargo_bin_cmd!("mq");
    cmd.current_dir(dir).arg("--unbuffered");
    cmd
}

fn fixture() -> TempDir {
    let dir = TempDir::new().unwrap();
    for (i, name) in MARKDOWN.iter().enumerate() {
        fs::write(dir.path().join(name), format!("# h{i}\n\ntext {i}\n\n## sub{i}\n")).unwrap();
    }
    for (i, name) in CSV.iter().enumerate() {
        fs::write(dir.path().join(name), format!("x,y\n{i},a\n{i},b\n")).unwrap();
    }
    for (i, name) in TEXT.iter().enumerate() {
        fs::write(dir.path().join(name), format!("line{i}a\nline{i}b\n")).unwrap();
    }
    dir
}

fn stdout(dir: &Path, args: &[&str], files: &[&str]) -> String {
    let assert = mq(dir).args(args).args(files).assert().success();
    String::from_utf8(assert.get_output().stdout.clone()).unwrap()
}

fn sorted_lines(output: &str) -> Vec<String> {
    let mut lines: Vec<String> = output.lines().map(str::to_string).collect();
    lines.sort_unstable();
    lines
}

/// Output of running `args` on each file alone, one process per file.
fn single_file_runs(dir: &Path, args: &[&str], files: &[&str]) -> String {
    files.iter().map(|file| stdout(dir, args, &[file])).collect()
}

fn files(groups: &[&[&'static str]]) -> Vec<&'static str> {
    groups.iter().flat_map(|group| group.iter().copied()).collect()
}

#[rstest]
fn test_batch_matches_single_file_runs(
    #[values(&["-P", "1000"], &["-P", "0"])] mode: &[&str],
    #[values(&[], &["-S", r#""---""#])] flags: &[&str],
    #[values(
        (r#"s"${__FILE_STEM__}: ${to_string(self)}""#, true),
        (r#"import "csv" | s"${__FILE_STEM__}""#, true),
        ("module m: let n = __FILE_STEM__ end | m::n", true),
        (".h | to_text()", false)
    )]
    case: (&str, bool),
) {
    let (query, with_csv) = case;
    let dir = fixture();
    let files = if with_csv {
        files(&[MARKDOWN, CSV, TEXT])
    } else {
        files(&[MARKDOWN, TEXT])
    };
    let args: Vec<&str> = flags.iter().copied().chain([query]).collect();

    let expected = single_file_runs(dir.path(), &args, &files);
    let batch_args: Vec<&str> = mode.iter().copied().chain(args.iter().copied()).collect();
    let actual = stdout(dir.path(), &batch_args, &files);

    assert_eq!(sorted_lines(&actual), sorted_lines(&expected), "{mode:?} {args:?}");
}

#[rstest]
fn test_update_matches_single_file_runs(#[values(&["-P", "1000"], &["-P", "0"])] mode: &[&str]) {
    let dir = fixture();
    let args = ["-U", ".h | upcase()"];
    let expected = single_file_runs(dir.path(), &args, MARKDOWN);
    let batch_args: Vec<&str> = mode.iter().copied().chain(args).collect();
    let actual = stdout(dir.path(), &batch_args, MARKDOWN);
    assert_eq!(sorted_lines(&actual), sorted_lines(&expected));
}

#[rstest]
#[case::mixed_formats(r#"s"${__FILE_STEM__}""#, true)]
#[case::import(r#"import "csv" | s"${__FILE_STEM__}""#, true)]
#[case::module_let_reads_file_global("module m: let n = __FILE_STEM__ end | m::n", true)]
fn test_count_matches_single_file_counts(#[case] query: &str, #[case] with_csv: bool) {
    let dir = fixture();
    let files = if with_csv {
        files(&[MARKDOWN, CSV, TEXT])
    } else {
        files(&[MARKDOWN, TEXT])
    };
    let counts: Vec<usize> = files
        .iter()
        .map(|file| stdout(dir.path(), &["-c", query], &[file]).trim().parse().unwrap())
        .collect();
    let mut expected: String = files
        .iter()
        .zip(&counts)
        .map(|(file, count)| format!("{file}: {count}\n"))
        .collect();
    expected.push_str(&format!("total: {}\n", counts.iter().sum::<usize>()));

    assert_eq!(stdout(dir.path(), &["-c", query], &files), expected);
}

#[rstest]
#[case::file_global(r#"s"${__FILE_STEM__}: ${self}""#)]
#[case::module_let_reads_file_global("module m: let n = __FILE_STEM__ end | s\"${m::n}: ${self}\"")]
fn test_stream_matches_single_file_runs(#[case] query: &str) {
    let dir = fixture();
    let args = ["--stream", "-I", "text", query];
    let expected = single_file_runs(dir.path(), &args, TEXT);
    assert_eq!(stdout(dir.path(), &args, TEXT), expected);
}

#[rstest]
fn test_mqc_matches_single_file_runs(
    #[values(&["-P", "1000"], &["-P", "0"])] mode: &[&str],
    #[values(&[], &["-S", r#""---""#])] flags: &[&str],
) {
    let dir = fixture();
    mq(dir.path())
        .args(["compile", r#".h | s"${__FILE_STEM__} ${to_text()}""#, "-o", "query.mqc"])
        .assert()
        .success();
    let args: Vec<&str> = flags.iter().copied().chain(["query.mqc"]).collect();

    let expected = single_file_runs(dir.path(), &args, MARKDOWN);
    let batch_args: Vec<&str> = mode.iter().copied().chain(args.iter().copied()).collect();
    let actual = stdout(dir.path(), &batch_args, MARKDOWN);

    assert_eq!(sorted_lines(&actual), sorted_lines(&expected), "{mode:?} {flags:?}");
}

#[test]
fn test_mqc_count_matches_single_file_counts() {
    let dir = fixture();
    mq(dir.path())
        .args(["compile", ".h", "-o", "query.mqc"])
        .assert()
        .success();
    let output = stdout(dir.path(), &["-c", "query.mqc"], MARKDOWN);
    let mut expected: String = MARKDOWN.iter().map(|file| format!("{file}: 2\n")).collect();
    expected.push_str(&format!("total: {}\n", MARKDOWN.len() * 2));
    assert_eq!(output, expected);
}

#[rstest]
fn test_invalid_query_fails_in_every_mode(
    #[values(&["-P", "1000"], &["-P", "0"], &["-c"], &["-S", r#""---""#])] mode: &[&str],
) {
    let dir = fixture();
    let assert = mq(dir.path())
        .args(mode)
        .arg("def f(:")
        .args(MARKDOWN)
        .assert()
        .failure();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(stderr.contains("Unexpected token"), "{mode:?}: {stderr}");
}
