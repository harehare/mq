//! Grep-like output format for the `-F grep` CLI option.
//!
//! Prints query results as `[file:]line:content`, with optional context nodes
//! before and after each match (controlled by `--before-context`, `--after-context`,
//! `--context`). Groups of context nodes are separated by `--`, matching the
//! behaviour of `grep -A/-B/-C`.

use miette::IntoDiagnostic;
use miette::miette;
use std::collections::HashSet;
use std::io::{self, BufWriter, Write};
use std::path::PathBuf;

/// Prints query results in grep-like format.
///
/// - `runtime_values`: the matched nodes returned by the query engine.
/// - `original_input`: all top-level nodes from the input document (used for context expansion).
/// - `file`: source file path, included as a prefix when present.
/// - `output_file`: redirect output to a file instead of stdout.
/// - `unbuffered`: skip buffering (flush after every write).
/// - `before` / `after`: number of sibling nodes to include before / after each match.
pub(crate) fn print_grep(
    runtime_values: mq_lang::RuntimeValues,
    original_input: &[mq_lang::RuntimeValue],
    file: &Option<PathBuf>,
    output_file: &Option<PathBuf>,
    unbuffered: bool,
    before: usize,
    after: usize,
) -> miette::Result<()> {
    let stdout = io::stdout();
    let mut handle: Box<dyn Write> = if let Some(path) = output_file {
        let f = std::fs::File::create(path).into_diagnostic()?;
        Box::new(BufWriter::new(f))
    } else if unbuffered {
        Box::new(stdout.lock())
    } else {
        Box::new(BufWriter::new(stdout.lock()))
    };

    let filename = file.as_ref().map(|p| p.to_string_lossy().into_owned());

    // Collect start lines of matched nodes for quick lookup.
    let match_lines: HashSet<usize> = runtime_values
        .values()
        .iter()
        .filter_map(|v| {
            if let mq_lang::RuntimeValue::Markdown(node, _) = v {
                node.position().map(|p| p.start.line)
            } else {
                None
            }
        })
        .collect();

    if before == 0 && after == 0 {
        for value in runtime_values.values() {
            for node in to_nodes(value) {
                let line_num = node.position().map(|p| p.start.line);
                let content = mq_markdown::Markdown::new(vec![node]).to_string();
                let content = content.trim_end_matches('\n');
                if content.is_empty() {
                    continue;
                }
                let line = format_line(&filename, line_num, content, ":");
                write_ignore_pipe(&mut handle, line.as_bytes())?;
            }
        }
    } else {
        let orig: Vec<&mq_lang::RuntimeValue> = original_input.iter().collect();

        // Build [start, end] index ranges (inclusive) for each matched node.
        let mut ranges: Vec<(usize, usize)> = match_lines
            .iter()
            .filter_map(|&line| {
                orig.iter().position(|v| {
                    if let mq_lang::RuntimeValue::Markdown(node, _) = v {
                        node.position().map(|p| p.start.line) == Some(line)
                    } else {
                        false
                    }
                })
            })
            .map(|idx| {
                let end = (idx + after).min(orig.len().saturating_sub(1));
                (idx.saturating_sub(before), end)
            })
            .collect();

        ranges.sort_unstable_by_key(|r| r.0);

        // Merge overlapping or adjacent ranges.
        let merged = merge_ranges(ranges);

        let mut first_group = true;
        for (start, end) in merged {
            if !first_group {
                write_ignore_pipe(&mut handle, b"--\n")?;
            }
            first_group = false;

            for idx in start..=end {
                if let Some(value) = orig.get(idx) {
                    for node in to_nodes(value) {
                        let line_num = node.position().map(|p| p.start.line);
                        let is_match = line_num.map(|l| match_lines.contains(&l)).unwrap_or(false);
                        let sep = if is_match { ":" } else { "-" };
                        let content = mq_markdown::Markdown::new(vec![node]).to_string();
                        let content = content.trim_end_matches('\n');
                        if content.is_empty() {
                            continue;
                        }
                        let line = format_line(&filename, line_num, content, sep);
                        write_ignore_pipe(&mut handle, line.as_bytes())?;
                    }
                }
            }
        }
    }

    if !unbuffered
        && let Err(e) = handle.flush()
        && e.kind() != std::io::ErrorKind::BrokenPipe
    {
        return Err(miette!(e));
    }

    Ok(())
}

fn format_line(filename: &Option<String>, line_num: Option<usize>, content: &str, sep: &str) -> String {
    match (filename, line_num) {
        (Some(f), Some(l)) => format!("{}{}{}{}{}\n", f, sep, l, sep, content),
        (Some(f), None) => format!("{}{}{}\n", f, sep, content),
        (None, Some(l)) => format!("{}{}{}\n", l, sep, content),
        (None, None) => format!("{}\n", content),
    }
}

fn write_ignore_pipe<W: Write>(handle: &mut W, data: &[u8]) -> miette::Result<()> {
    match handle.write_all(data) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
        Err(e) => Err(miette!(e)),
    }
}

/// Merges a sorted list of `(start, end)` index ranges, combining ranges that
/// overlap or are directly adjacent (end + 1 == next start), matching `grep` behaviour.
fn merge_ranges(ranges: Vec<(usize, usize)>) -> Vec<(usize, usize)> {
    ranges
        .into_iter()
        .fold(Vec::<(usize, usize)>::new(), |mut acc, (s, e)| {
            if let Some(last) = acc.last_mut()
                && s <= last.1 + 1
            {
                last.1 = last.1.max(e);
                return acc;
            }
            acc.push((s, e));
            acc
        })
}

fn to_nodes(value: &mq_lang::RuntimeValue) -> Vec<mq_markdown::Node> {
    match value {
        mq_lang::RuntimeValue::Markdown(node, _) => vec![(**node).clone()],
        mq_lang::RuntimeValue::Array(items) => items.iter().flat_map(to_nodes).collect(),
        _ => vec![value.to_string().into()],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_line_with_filename_and_line() {
        let result = format_line(&Some("file.md".to_string()), Some(5), "## Heading", ":");
        assert_eq!(result, "file.md:5:## Heading\n");
    }

    #[test]
    fn test_format_line_context_separator() {
        let result = format_line(&Some("file.md".to_string()), Some(4), "Some text.", "-");
        assert_eq!(result, "file.md-4-Some text.\n");
    }

    #[test]
    fn test_format_line_without_filename() {
        let result = format_line(&None, Some(3), "Paragraph.", ":");
        assert_eq!(result, "3:Paragraph.\n");
    }

    #[test]
    fn test_format_line_without_line_number() {
        let result = format_line(&Some("file.md".to_string()), None, "text", ":");
        assert_eq!(result, "file.md:text\n");
    }

    #[test]
    fn test_format_line_no_filename_no_line() {
        let result = format_line(&None, None, "plain", ":");
        assert_eq!(result, "plain\n");
    }

    // --- merge_ranges tests ---

    #[test]
    fn test_merge_ranges_empty() {
        assert_eq!(merge_ranges(vec![]), vec![]);
    }

    #[test]
    fn test_merge_ranges_single() {
        assert_eq!(merge_ranges(vec![(2, 5)]), vec![(2, 5)]);
    }

    #[test]
    fn test_merge_ranges_overlapping() {
        // (0,3) and (2,5) overlap — should become (0,5)
        assert_eq!(merge_ranges(vec![(0, 3), (2, 5)]), vec![(0, 5)]);
    }

    #[test]
    fn test_merge_ranges_adjacent() {
        // (0,1) and (2,3) are adjacent (end+1 == next start) — should merge into (0,3)
        assert_eq!(merge_ranges(vec![(0, 1), (2, 3)]), vec![(0, 3)]);
    }

    #[test]
    fn test_merge_ranges_non_adjacent() {
        // (0,1) and (3,4) have a gap — should remain two separate groups
        assert_eq!(merge_ranges(vec![(0, 1), (3, 4)]), vec![(0, 1), (3, 4)]);
    }

    #[test]
    fn test_merge_ranges_multiple_adjacent_chain() {
        // Three adjacent ranges should collapse into one
        assert_eq!(merge_ranges(vec![(0, 1), (2, 3), (4, 5)]), vec![(0, 5)]);
    }

    #[test]
    fn test_merge_ranges_mixed() {
        // First two merge (adjacent), third is separate
        assert_eq!(merge_ranges(vec![(0, 1), (2, 3), (5, 6)]), vec![(0, 3), (5, 6)]);
    }

    #[test]
    fn test_merge_ranges_already_merged() {
        // No-op when ranges are already fully merged
        assert_eq!(merge_ranges(vec![(0, 10)]), vec![(0, 10)]);
    }
}
