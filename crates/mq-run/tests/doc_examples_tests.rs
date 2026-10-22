//! Runs the `# => expected` examples in the ```mq blocks of `docs/books/src/**/*.md` through
//! the built `mq` binary with `-I null`, so the reference docs stay honest against the VM.
//!
//! An example is the query lines since the last blank line or `# =>` run, plus the `# =>` lines
//! that follow. A `def` or `module` segment without `# =>` stays in scope for the later examples of its block.
//! Put `<!-- doc-test: skip -->` right before a fence to exclude a block that needs external input
//! or permissions.

use assert_cmd::cargo;
use std::fs;
use std::path::{Path, PathBuf};

const SKIP_MARKER: &str = "<!-- doc-test: skip -->";

struct Example {
    file: PathBuf,
    line: usize,
    query: String,
    expected: String,
}

fn markdown_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            markdown_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "md") {
            out.push(path);
        }
    }
}

/// Returns the first line number and body of every ```mq block that is not marked as skipped.
fn mq_blocks(text: &str) -> Vec<(usize, Vec<&str>)> {
    let mut blocks = Vec::new();
    let mut current: Option<(usize, Vec<&str>)> = None;
    let mut prev = "";
    for (i, line) in text.lines().enumerate() {
        match &mut current {
            None if line.trim() == "```mq" && prev.trim() != SKIP_MARKER => current = Some((i + 2, Vec::new())),
            None => {}
            Some(_) if line.trim() == "```" => blocks.extend(current.take()),
            Some((_, body)) => body.push(line),
        }
        prev = line;
    }
    blocks
}

fn examples_in(file: &Path) -> Vec<Example> {
    let text = fs::read_to_string(file).unwrap();
    let mut examples = Vec::new();

    for (first_line, body) in mq_blocks(&text) {
        let mut prelude: Vec<&str> = Vec::new();
        let mut query: Vec<&str> = Vec::new();
        let mut expected: Vec<&str> = Vec::new();
        let mut start = first_line;

        for (offset, line) in body.iter().chain(std::iter::once(&"")).enumerate() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("# =>") {
                expected.push(rest.trim());
                continue;
            }
            if !expected.is_empty() {
                examples.push(Example {
                    file: file.to_path_buf(),
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
                    start = first_line + offset;
                }
                query.push(line);
            }
        }
    }
    examples
}

fn run(query: &str) -> Result<String, String> {
    let output = cargo::cargo_bin_cmd!("mq")
        .args(["-I", "null", "--", query])
        .output()
        .unwrap();
    if output.status.success() {
        Ok(String::from_utf8(output.stdout).unwrap().trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

#[test]
fn doc_examples_match_documented_output() {
    let docs = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/books/src");
    let mut files = Vec::new();
    markdown_files(&docs, &mut files);

    let mut checked = 0;
    let mut failures = Vec::new();
    for example in files.iter().flat_map(|f| examples_in(f)) {
        checked += 1;
        // Strings print unquoted on the CLI but are quoted in the docs.
        let unquoted = example
            .expected
            .strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
            .unwrap_or(&example.expected);
        let location = format!(
            "{}:{}",
            example.file.strip_prefix(&docs).unwrap().display(),
            example.line
        );
        match run(&example.query) {
            Ok(got) if got == example.expected || got == unquoted => {}
            Ok(got) => failures.push(format!(
                "{location}\n  query: {}\n  want:  {}\n  got:   {got}",
                example.query, example.expected
            )),
            Err(err) => failures.push(format!(
                "{location}\n  query: {}\n  error: {}",
                example.query,
                err.lines().next().unwrap_or_default()
            )),
        }
    }

    assert!(checked > 0, "no doc examples found");
    assert!(
        failures.is_empty(),
        "{} of {checked} doc examples failed:\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}
