//! Runs `# => expected` examples found in Markdown ` ```mq ` blocks and in fenced blocks inside
//! `.mq` comments, like a doc test.

use colored::Colorize;
use mq_lang::RuntimeValue;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Placed on the line before a fence (inside the comment for `.mq`) to exclude the block.
const SKIP_MARKER: &str = "<!-- doc-test: skip -->";

/// One `query` / `# => expected` pair, with the line of the query in its file.
#[derive(Debug, PartialEq)]
pub(crate) struct Example {
    pub line: usize,
    pub query: String,
    pub expected: String,
}

/// Whether a fence info string selects a block to run.
fn fence_selected(info: &str, is_markdown: bool) -> bool {
    match info.trim() {
        "mq" => true,
        "" => !is_markdown,
        _ => false,
    }
}

/// `# => x` or `#=> x`.
fn expectation(line: &str) -> Option<&str> {
    let rest = line.trim().strip_prefix('#')?.trim_start();
    rest.strip_prefix("=>").map(str::trim)
}

/// Splits a block into examples.
///
/// A query is the code lines since the last blank line or expectation, and the expectation is
/// the `# =>` line(s) that follow. A `def` or `module` segment without an expectation stays in
/// scope for the later examples of the block. With `bare_continuation` (`.mq` comments) the
/// lines after `# =>` up to the next blank line also belong to the expectation.
fn block_examples(lines: &[(usize, String)], bare_continuation: bool) -> Vec<Example> {
    let mut examples = Vec::new();
    let mut prelude: Vec<&str> = Vec::new();
    let mut query: Vec<&str> = Vec::new();
    let mut expected: Vec<String> = Vec::new();
    let mut start = 0;

    for (line_no, line) in lines.iter().chain(std::iter::once(&(0, String::new()))) {
        let trimmed = line.trim();
        if let Some(rest) = expectation(trimmed) {
            expected.push(rest.to_string());
            continue;
        }
        if !expected.is_empty() {
            if bare_continuation && !trimmed.is_empty() {
                expected.push(trimmed.to_string());
                continue;
            }
            examples.push(Example {
                line: start,
                query: prelude.iter().chain(&query).copied().collect::<Vec<_>>().join("\n"),
                expected: expected.join("\n"),
            });
            query.clear();
            expected.clear();
        }
        if trimmed.is_empty() {
            if query
                .first()
                .is_some_and(|l| l.starts_with("def ") || l.starts_with("module "))
            {
                prelude.append(&mut query);
            }
            query.clear();
        } else if !trimmed.starts_with('#') {
            if query.is_empty() {
                start = *line_no;
            }
            query.push(line);
        }
    }
    examples
}

/// Examples in Markdown ` ```mq ` blocks.
pub(crate) fn markdown_examples(content: &str) -> Vec<Example> {
    let mut examples = Vec::new();
    let mut block: Option<Vec<(usize, String)>> = None;
    let mut selected = false;
    let mut prev = "";
    for (i, line) in content.lines().enumerate() {
        if let Some(info) = line.trim_start().strip_prefix("```") {
            match block.take() {
                Some(lines) => {
                    if selected {
                        examples.extend(block_examples(&lines, false));
                    }
                }
                None => {
                    selected = fence_selected(info, true) && prev.trim() != SKIP_MARKER;
                    block = Some(Vec::new());
                }
            }
        } else if let Some(lines) = block.as_mut() {
            lines.push((i + 1, line.to_string()));
        }
        prev = line;
    }
    examples
}

/// Examples in fenced blocks inside `#` comments of a `.mq` file.
pub(crate) fn mq_comment_examples(content: &str) -> Vec<Example> {
    let mut examples = Vec::new();
    let mut block: Option<Vec<(usize, String)>> = None;
    let mut selected = false;
    let mut prev = String::new();
    for (i, line) in content.lines().enumerate() {
        let Some(comment) = line.trim_start().strip_prefix('#') else {
            block = None;
            prev.clear();
            continue;
        };
        let comment = comment.strip_prefix(' ').unwrap_or(comment);
        if let Some(info) = comment.trim_start().strip_prefix("```") {
            match block.take() {
                Some(lines) => {
                    if selected {
                        examples.extend(block_examples(&lines, true));
                    }
                }
                None => {
                    selected = fence_selected(info, false) && prev.trim() != SKIP_MARKER;
                    block = Some(Vec::new());
                }
            }
        } else if let Some(lines) = block.as_mut() {
            lines.push((i + 1, comment.to_string()));
        }
        prev = comment.to_string();
    }
    examples
}

fn eval_one(query: &str) -> Result<RuntimeValue, String> {
    let mut engine = mq_lang::Engine::with_io(
        crate::resolver::TestModuleResolver::default(),
        mq_lang::Shared::new(mq_lang::MemIo::default()),
    );
    engine.load_builtin_module();
    crate::runner::TestRunner::register_clock(&mut engine);
    let result = engine
        .eval(query, mq_lang::null_input().into_iter())
        .map_err(|e| e.to_string())?;
    Ok(result.values().last().cloned().unwrap_or(RuntimeValue::NONE))
}

/// Why an example failed.
enum Failure {
    Error(String),
    Mismatch { actual: String },
}

/// A failed example, with where it is and what it ran.
struct Failed {
    location: String,
    example: Example,
    failure: Failure,
}

/// Outcome of a file's examples.
struct FileReport {
    total: usize,
    failed: Vec<Failed>,
}

/// Whether `actual` matches the documented `expected`.
fn matches(expected: &str, actual: &RuntimeValue) -> bool {
    if actual.to_string().trim() == expected {
        return true;
    }
    // Fall back to comparing values when the expectation is written as an mq literal.
    eval_one(expected).is_ok_and(|value| {
        value == *actual || matches!(value, RuntimeValue::String(_)) && value.to_string() == actual.to_string()
    })
}

/// Runs every example in `file` (`.md` as Markdown, anything else as `.mq` comments).
fn run_file(file: &Path) -> Result<FileReport, String> {
    let content = fs::read_to_string(file).map_err(|e| format!("failed to read {}: {e}", file.display()))?;
    let examples = if file.extension().is_some_and(|e| e == "md") {
        markdown_examples(&content)
    } else {
        mq_comment_examples(&content)
    };

    let total = examples.len();
    let mut failed = Vec::new();
    for example in examples {
        let failure = match eval_one(&example.query) {
            Ok(actual) if matches(&example.expected, &actual) => continue,
            Ok(actual) => Failure::Mismatch {
                actual: match actual {
                    RuntimeValue::String(s) => format!("{s:?}"),
                    other => other.to_string(),
                },
            },
            Err(e) => Failure::Error(e),
        };
        failed.push(Failed {
            location: format!("{}:{}", file.display(), example.line),
            example,
            failure,
        });
    }
    Ok(FileReport { total, failed })
}

/// Prints `text` with every line prefixed, so multi-line values stay aligned.
fn indented(prefix: &str, text: &str) -> String {
    text.lines()
        .map(|l| format!("      {prefix} {l}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn print_failure(f: &Failed) {
    eprintln!("{}  {}", "❌".red(), f.location.bold());
    eprintln!("    {}", "query".dimmed());
    eprintln!("{}", indented(" ", &f.example.query));
    match &f.failure {
        Failure::Error(e) => {
            eprintln!("    {}", "error".red());
            eprintln!("{}", indented(" ", e).red());
        }
        Failure::Mismatch { actual } => {
            eprintln!("    {}", "diff".dimmed());
            eprintln!("{}", indented("-", &f.example.expected).red());
            eprintln!("{}", indented("+", actual).green());
        }
    }
    eprintln!();
}

/// Runs the examples of every file and reports per-file results, the failures and a summary.
/// Returns `true` when all pass.
pub(crate) fn run(files: &[PathBuf]) -> bool {
    let start = Instant::now();
    let (mut total, mut checked_files) = (0, 0);
    let mut failed: Vec<Failed> = Vec::new();
    let mut unreadable: Vec<String> = Vec::new();

    for file in files {
        match run_file(file) {
            Ok(report) if report.total == 0 => {}
            Ok(report) => {
                checked_files += 1;
                total += report.total;
                let name = file.display().to_string();
                if report.failed.is_empty() {
                    println!("{}  {name} {}", "✅".green(), format!("({})", report.total).dimmed());
                } else {
                    println!(
                        "{}  {name} {}",
                        "❌".red(),
                        format!("({}/{} passed)", report.total - report.failed.len(), report.total).red()
                    );
                    failed.extend(report.failed);
                }
            }
            Err(e) => {
                println!("{}  {}", "⚠".yellow(), e);
                unreadable.push(e);
            }
        }
    }

    if !failed.is_empty() {
        eprintln!();
        failed.iter().for_each(print_failure);
    }

    let passed = total - failed.len();
    let mut summary = vec![
        format!("✅  {passed} passed").green().to_string(),
        format!("❌  {} failed", failed.len())
            .color(if failed.is_empty() { "white" } else { "red" })
            .to_string(),
    ];
    if !unreadable.is_empty() {
        summary.push(format!("⚠  {} unreadable", unreadable.len()).yellow().to_string());
    }
    println!(
        "\n{}  {}",
        summary.join("  "),
        format!("{checked_files} files, {:.2}s", start.elapsed().as_secs_f64()).dimmed()
    );
    failed.is_empty() && unreadable.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_segments_prelude_and_skip() {
        let md = "```mq\ndef f(): 1;\n\n# note\nf() + 1\n# => 2\n\n2 + 2\n#=> 4\n```\n\n<!-- doc-test: skip -->\n```mq\n1\n# => 9\n```\n\n```bash\n1\n# => 9\n```\n";
        let ex = markdown_examples(md);
        assert_eq!(ex.len(), 2);
        assert_eq!(ex[0].query, "def f(): 1;\nf() + 1");
        assert_eq!(ex[0].expected, "2");
        assert_eq!(ex[0].line, 5);
        assert_eq!(ex[1].query, "def f(): 1;\n2 + 2");
    }

    #[test]
    fn mq_comment_blocks_with_continuation() {
        let src = "# Example:\n# ```\n# add(1, 2)\n# #=> a\n# b\n# ```\ndef f(): 1;\n";
        let ex = mq_comment_examples(src);
        assert_eq!(
            ex,
            vec![Example {
                line: 3,
                query: "add(1, 2)".into(),
                expected: "a\nb".into()
            }]
        );
    }
}
