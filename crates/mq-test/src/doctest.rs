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

/// An opening code fence: its character, length and info string.
fn parse_fence(text: &str) -> Option<(char, usize, &str)> {
    let text = text.trim_start();
    let ch = text.chars().next().filter(|c| matches!(c, '`' | '~'))?;
    let len = text.chars().take_while(|c| *c == ch).count();
    (len >= 3).then(|| (ch, len, text[len..].trim()))
}

/// A fenced block being read.
struct OpenFence {
    ch: char,
    len: usize,
    selected: bool,
    lines: Vec<(usize, String)>,
}

/// Finds examples line by line. A fence closes only on the same character with at least the
/// opening length, so a longer outer fence keeps inner fences as literal content.
struct Scanner {
    is_markdown: bool,
    open: Option<OpenFence>,
    prev: String,
    examples: Vec<Example>,
}

impl Scanner {
    fn new(is_markdown: bool) -> Self {
        Self {
            is_markdown,
            open: None,
            prev: String::new(),
            examples: Vec::new(),
        }
    }

    /// Drops the open block, for a `.mq` line that is not a comment.
    fn reset(&mut self) {
        self.open = None;
        self.prev.clear();
    }

    fn feed(&mut self, line_no: usize, text: &str) {
        match (parse_fence(text), self.open.take()) {
            (Some((ch, len, info)), Some(open)) if ch == open.ch && len >= open.len && info.is_empty() => {
                if open.selected {
                    self.examples.extend(block_examples(&open.lines, !self.is_markdown));
                }
            }
            (Some((ch, len, info)), None) => {
                self.open = Some(OpenFence {
                    ch,
                    len,
                    selected: fence_selected(info, self.is_markdown) && self.prev.trim() != SKIP_MARKER,
                    lines: Vec::new(),
                });
            }
            (_, open) => {
                self.open = open;
                if let Some(open) = self.open.as_mut() {
                    open.lines.push((line_no, text.to_string()));
                }
            }
        }
        self.prev = text.to_string();
    }
}

/// Examples in Markdown ` ```mq ` blocks.
pub(crate) fn markdown_examples(content: &str) -> Vec<Example> {
    let mut scanner = Scanner::new(true);
    for (i, line) in content.lines().enumerate() {
        scanner.feed(i + 1, line);
    }
    scanner.examples
}

/// Examples in fenced blocks inside `#` comments of a `.mq` file.
pub(crate) fn mq_comment_examples(content: &str) -> Vec<Example> {
    let mut scanner = Scanner::new(false);
    for (i, line) in content.lines().enumerate() {
        match line.trim_start().strip_prefix('#') {
            Some(comment) => scanner.feed(i + 1, comment.strip_prefix(' ').unwrap_or(comment)),
            None => scanner.reset(),
        }
    }
    scanner.examples
}

/// Runs `query` against no input and returns every output value.
///
/// Files are read through an in-memory I/O, so only the standard modules resolve. With
/// `search_paths`, local modules are read from those directories on disk instead.
fn eval_all(query: &str, search_paths: Option<&[PathBuf]>) -> Result<Vec<RuntimeValue>, String> {
    let io = mq_lang::Shared::new(mq_lang::MemIo::default());
    let fallback = match search_paths {
        Some(paths) => mq_lang::DefaultModuleResolver::new(paths.to_vec()),
        None => mq_lang::DefaultModuleResolver::with_io(io.clone() as mq_lang::Shared<dyn mq_lang::Io>, Vec::new()),
    };
    let mut engine = mq_lang::Engine::with_io(crate::resolver::TestModuleResolver::new(fallback), io);
    engine.load_builtin_module();
    crate::runner::TestRunner::register_clock(&mut engine);
    let result = engine
        .eval(query, mq_lang::null_input().into_iter())
        .map_err(|e| e.to_string())?;
    Ok(result.values().to_vec())
}

/// Module search paths for examples in `file`: its directory, then the `-L` directories. `None`
/// when no `-L` is given, which keeps the examples off the disk.
fn search_paths(file: &Path, module_directories: &[PathBuf]) -> Option<Vec<PathBuf>> {
    if module_directories.is_empty() {
        return None;
    }
    let parent = file.parent().filter(|p| *p != Path::new("")).unwrap_or(Path::new("."));
    let mut paths = vec![parent.to_path_buf()];
    paths.extend(module_directories.iter().cloned());
    Some(paths)
}

/// The printed form of all output values, one per line.
fn render(values: &[RuntimeValue]) -> String {
    values.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n")
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

/// Whether the output `actual` matches the documented `expected`.
fn matches(expected: &str, actual: &[RuntimeValue], paths: Option<&[PathBuf]>) -> bool {
    if render(actual).trim() == expected {
        return true;
    }
    // A single value may also match an expectation written as an mq literal.
    let [actual] = actual else { return false };
    let Ok(values) = eval_all(expected, paths) else {
        return false;
    };
    matches!(values.as_slice(), [value]
        if value == actual || matches!(value, RuntimeValue::String(_)) && value.to_string() == actual.to_string())
}

/// Runs every example in `file` (`.md` as Markdown, anything else as `.mq` comments).
fn run_file(file: &Path, module_directories: &[PathBuf]) -> Result<FileReport, String> {
    let content = fs::read_to_string(file).map_err(|e| format!("failed to read {}: {e}", file.display()))?;
    let examples = if file.extension().is_some_and(|e| e == "md") {
        markdown_examples(&content)
    } else {
        mq_comment_examples(&content)
    };

    let paths = search_paths(file, module_directories);
    let total = examples.len();
    let mut failed = Vec::new();
    for example in examples {
        let failure = match eval_all(&example.query, paths.as_deref()) {
            Ok(actual) if matches(&example.expected, &actual, paths.as_deref()) => continue,
            Ok(actual) => Failure::Mismatch {
                actual: match actual.as_slice() {
                    [] => "(no output)".to_string(),
                    [RuntimeValue::String(s)] => format!("{s:?}"),
                    values => render(values),
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
pub(crate) fn run(files: &[PathBuf], module_directories: &[PathBuf]) -> bool {
    let start = Instant::now();
    let (mut total, mut checked_files) = (0, 0);
    let mut failed: Vec<Failed> = Vec::new();
    let mut unreadable: Vec<String> = Vec::new();

    for file in files {
        match run_file(file, module_directories) {
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
    if total == 0 {
        eprintln!("{}  no doc examples found", "❌".red());
    }
    total > 0 && failed.is_empty() && unreadable.is_empty()
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

    #[test]
    fn longer_outer_fence_keeps_inner_fence_literal() {
        let md = "````markdown\n```mq\n1\n# => 9\n```\n````\n\n```mq\n2\n# => 2\n```\n";
        let ex = markdown_examples(md);
        assert_eq!(ex.len(), 1);
        assert_eq!(ex[0].query, "2");
    }

    #[test]
    fn expectation_covers_all_output_values() {
        let out = [RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into())];
        assert!(matches("1\n2", &out, None));
        assert!(!matches("2", &out, None));
        assert!(!matches("None", &[], None));
    }
}
