use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use mq_lang::{Ident, RuntimeValue, Shared};
use similar::{ChangeTag, TextDiff};

use crate::html;

const MAX_INLINE_DIFF_LINES: usize = 40;

fn snapshot_dir(test_file: &Path) -> PathBuf {
    base_dir(test_file).join("__snapshots__").join(test_stem(test_file))
}

fn store_dir(test_file: &Path) -> PathBuf {
    base_dir(test_file).join(".mq-test-store").join(test_stem(test_file))
}

fn base_dir(test_file: &Path) -> PathBuf {
    match test_file.parent() {
        Some(parent) if parent != Path::new("") => parent.to_path_buf(),
        _ => PathBuf::from("."),
    }
}

fn test_stem(test_file: &Path) -> String {
    test_file
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "test".to_string())
}

fn sanitize_snapshot_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '[' | ']') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Compares `actual` against the golden snapshot named `name`. Real, unmocked disk I/O —
/// unlike every other mq-test builtin, golden files must survive across runs.
///
/// Returns the assert_eq-style convention `test.mq` expects: the passing value on
/// success, or a `{"error": true, "message": ...}` dict on failure.
pub(crate) fn check_snapshot(test_file: &Path, name: &str, actual: &str, update: bool) -> RuntimeValue {
    let safe_name = sanitize_snapshot_name(name);
    let snapshot_file = snapshot_dir(test_file).join(format!("{safe_name}.snap"));

    if update {
        return match write_snapshot(&snapshot_file, actual) {
            Ok(()) => RuntimeValue::String(actual.to_string()),
            Err(e) => fail(format!(
                "failed to write snapshot \"{name}\" to {}: {e}",
                snapshot_file.display()
            )),
        };
    }

    let expected = match fs::read_to_string(&snapshot_file) {
        Ok(content) => content,
        Err(_) => {
            let report = write_store(test_file, &safe_name, actual, &diff_lines("", actual));
            return fail(format!(
                "Assertion failed: snapshot \"{name}\" does not exist yet (expected at {}).\n\n\
                 Run with --update-snapshots to create it. The output that would have been \
                 saved is available for review at {}",
                snapshot_file.display(),
                report.display()
            ));
        }
    };

    if expected == actual {
        return RuntimeValue::String(actual.to_string());
    }

    let lines = diff_lines(&expected, actual);
    let report = write_store(test_file, &safe_name, actual, &lines);
    let (diff, truncated) = truncated_text_diff(&lines);
    let mut message = format!("Assertion failed: snapshot \"{name}\" does not match\n\n{diff}");
    if truncated {
        message.push_str(&format!("\n  … diff truncated, full report at {}", report.display()));
    } else {
        message.push_str(&format!("\n\nFull report: {}", report.display()));
    }

    fail(message)
}

fn fail(message: String) -> RuntimeValue {
    let mut map = BTreeMap::new();
    map.insert(Ident::new("error"), RuntimeValue::Boolean(true));
    map.insert(Ident::new("message"), RuntimeValue::String(message));
    RuntimeValue::Dict(Shared::new(map))
}

fn write_snapshot(path: &Path, content: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, content)
}

fn write_store(test_file: &Path, safe_name: &str, actual: &str, lines: &[DiffLine]) -> PathBuf {
    let dir = store_dir(test_file);
    let _ = fs::create_dir_all(&dir);
    let _ = fs::write(dir.join(format!("{safe_name}.actual.snap")), actual);

    let report_path = dir.join(format!("{safe_name}.diff.html"));
    let _ = fs::write(&report_path, format_diff_html(safe_name, lines));
    report_path
}

struct DiffLine {
    tag: ChangeTag,
    text: String,
}

fn diff_lines(expected: &str, actual: &str) -> Vec<DiffLine> {
    TextDiff::from_lines(expected, actual)
        .iter_all_changes()
        .map(|change| DiffLine {
            tag: change.tag(),
            text: change.value().trim_end_matches('\n').to_string(),
        })
        .collect()
}

fn text_diff(lines: &[DiffLine]) -> String {
    lines
        .iter()
        .map(|line| {
            let sign = match line.tag {
                ChangeTag::Delete => "- ",
                ChangeTag::Insert => "+ ",
                ChangeTag::Equal => "  ",
            };
            format!("{sign}{}", line.text)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn truncated_text_diff(lines: &[DiffLine]) -> (String, bool) {
    let truncated = lines.len() > MAX_INLINE_DIFF_LINES;
    let shown = if truncated {
        &lines[..MAX_INLINE_DIFF_LINES]
    } else {
        lines
    };
    (text_diff(shown), truncated)
}

const DIFF_HTML_STYLE: &str = r#"
:root {
  color-scheme: light dark;
  --bg: #ffffff;
  --fg: #1a1a1a;
  --muted: #6b7280;
  --insert-bg: #d9f7e380;
  --insert-fg: #1a7f37;
  --delete-bg: #ffe3e380;
  --delete-fg: #c53030;
  --code-bg: #f8fafc;
}
@media (prefers-color-scheme: dark) {
  :root {
    --bg: #16181d;
    --fg: #e6e6e6;
    --muted: #9aa0a6;
    --insert-bg: #12382280;
    --insert-fg: #4ada91;
    --delete-bg: #3a161880;
    --delete-fg: #ff8080;
    --code-bg: #1e293b;
  }
}
* { box-sizing: border-box; }
body {
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
  margin: 2rem auto;
  max-width: 960px;
  color: var(--fg);
  background: var(--bg);
}
h1 { font-size: 1.4rem; }
table {
  border-collapse: collapse;
  width: 100%;
  font-family: ui-monospace, monospace;
  font-size: 0.85rem;
  background: var(--code-bg);
  border-radius: 6px;
  overflow: hidden;
}
td { padding: 0.1rem 0.6rem; white-space: pre-wrap; word-break: break-word; }
td.marker { width: 1.5rem; text-align: center; user-select: none; color: var(--muted); }
tr.insert td { background: var(--insert-bg); }
tr.insert td.marker { color: var(--insert-fg); }
tr.delete td { background: var(--delete-bg); }
tr.delete td.marker { color: var(--delete-fg); }
"#;

/// Renders a self-contained HTML diff report, in the spirit of typst's `store/` HTML
/// diff reports — meant for reviewing a large snapshot mismatch without scrolling a
/// terminal, and as a CI artifact.
fn format_diff_html(name: &str, lines: &[DiffLine]) -> String {
    let mut rows = String::new();

    for line in lines {
        let (class, marker) = match line.tag {
            ChangeTag::Delete => ("delete", "-"),
            ChangeTag::Insert => ("insert", "+"),
            ChangeTag::Equal => ("equal", " "),
        };
        rows.push_str(&format!(
            "      <tr class=\"{class}\"><td class=\"marker\">{marker}</td><td>{code}</td></tr>\n",
            code = html::escape(&line.text),
        ));
    }

    let body = format!(
        "\x20 <h1>Snapshot diff — {escaped_name}</h1>\n\
        \x20 <table>\n\
        \x20   <tbody>\n\
        {rows}\
        \x20   </tbody>\n\
        \x20 </table>\n",
        escaped_name = html::escape(name),
    );

    html::page(&format!("mq-test snapshot diff — {name}"), DIFF_HTML_STYLE, &body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    fn temp_test_file(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mq_test_snapshot_{name}_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir.join("tests.mq")
    }

    fn is_error_dict(value: &RuntimeValue) -> bool {
        matches!(value, RuntimeValue::Dict(map) if map.contains_key(&Ident::new("error")))
    }

    #[rstest]
    #[case("plain", "plain")]
    #[case("a-b_c.d[0]", "a-b_c.d[0]")]
    #[case("has spaces", "has_spaces")]
    #[case("weird/../name", "weird_.._name")]
    #[case("emoji🎉name", "emoji_name")]
    fn test_sanitize_snapshot_name(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(sanitize_snapshot_name(input), expected);
    }

    #[test]
    fn test_missing_snapshot_fails_and_writes_store() {
        let test_file = temp_test_file("missing");
        let result = check_snapshot(&test_file, "greeting", "hello world", false);

        assert!(is_error_dict(&result));
        let report = store_dir(&test_file).join("greeting.diff.html");
        assert!(report.exists(), "expected diff report at {}", report.display());
        assert!(!snapshot_dir(&test_file).join("greeting.snap").exists());

        fs::remove_dir_all(test_file.parent().unwrap()).ok();
    }

    #[test]
    fn test_update_creates_snapshot_and_passes() {
        let test_file = temp_test_file("create");
        let result = check_snapshot(&test_file, "greeting", "hello world", true);

        assert_eq!(result, RuntimeValue::String("hello world".to_string()));
        let saved = fs::read_to_string(snapshot_dir(&test_file).join("greeting.snap")).unwrap();
        assert_eq!(saved, "hello world");

        fs::remove_dir_all(test_file.parent().unwrap()).ok();
    }

    #[test]
    fn test_matching_snapshot_passes() {
        let test_file = temp_test_file("match");
        check_snapshot(&test_file, "greeting", "hello world", true);

        let result = check_snapshot(&test_file, "greeting", "hello world", false);
        assert_eq!(result, RuntimeValue::String("hello world".to_string()));

        fs::remove_dir_all(test_file.parent().unwrap()).ok();
    }

    #[test]
    fn test_mismatched_snapshot_fails_and_writes_store() {
        let test_file = temp_test_file("mismatch");
        check_snapshot(&test_file, "greeting", "hello world", true);

        let result = check_snapshot(&test_file, "greeting", "goodbye world", false);
        assert!(is_error_dict(&result));

        let actual_store = store_dir(&test_file).join("greeting.actual.snap");
        assert_eq!(fs::read_to_string(&actual_store).unwrap(), "goodbye world");
        let report = store_dir(&test_file).join("greeting.diff.html");
        let html = fs::read_to_string(&report).unwrap();
        assert!(html.contains("hello"));
        assert!(html.contains("goodbye"));

        // The golden file itself must be untouched by a non-update mismatch.
        assert_eq!(
            fs::read_to_string(snapshot_dir(&test_file).join("greeting.snap")).unwrap(),
            "hello world"
        );

        fs::remove_dir_all(test_file.parent().unwrap()).ok();
    }

    #[test]
    fn test_update_overwrites_an_existing_mismatched_snapshot() {
        let test_file = temp_test_file("overwrite");
        check_snapshot(&test_file, "greeting", "hello world", true);
        check_snapshot(&test_file, "greeting", "goodbye world", true);

        let result = check_snapshot(&test_file, "greeting", "goodbye world", false);
        assert_eq!(result, RuntimeValue::String("goodbye world".to_string()));

        fs::remove_dir_all(test_file.parent().unwrap()).ok();
    }

    #[test]
    fn test_large_diff_is_truncated_in_the_failure_message_but_full_report_is_written() {
        let test_file = temp_test_file("large");
        let expected: String = (0..200).map(|i| format!("line {i}\n")).collect();
        let actual: String = (0..200).map(|i| format!("line {i} changed\n")).collect();
        check_snapshot(&test_file, "big", &expected, true);
        fs::write(snapshot_dir(&test_file).join("big.snap"), &expected).unwrap();

        let result = check_snapshot(&test_file, "big", &actual, false);
        match &result {
            RuntimeValue::Dict(map) => {
                let message = match map.get(&Ident::new("message")).unwrap() {
                    RuntimeValue::String(s) => s.clone(),
                    _ => panic!("expected string message"),
                };
                assert!(
                    message.contains("truncated"),
                    "message should mention truncation: {message}"
                );
                assert!(
                    message.lines().count() < 200,
                    "inline message must not dump the whole 200-line diff: {} lines",
                    message.lines().count()
                );
            }
            other => panic!("expected error dict, got {other:?}"),
        }

        let report = fs::read_to_string(store_dir(&test_file).join("big.diff.html")).unwrap();
        assert!(
            report.contains("line 199 changed"),
            "full report must contain every changed line"
        );

        fs::remove_dir_all(test_file.parent().unwrap()).ok();
    }

    #[test]
    fn test_snapshot_dir_and_store_dir_are_scoped_per_test_file_stem() {
        let a = PathBuf::from("/proj/tests/a.mq");
        let b = PathBuf::from("/proj/tests/b.mq");
        assert_ne!(snapshot_dir(&a), snapshot_dir(&b));
        assert_ne!(store_dir(&a), store_dir(&b));
        assert!(snapshot_dir(&a).ends_with("__snapshots__/a"));
        assert!(store_dir(&a).ends_with(".mq-test-store/a"));
    }
}
