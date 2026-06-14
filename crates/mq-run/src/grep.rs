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
        mq_lang::RuntimeValue::Array(_) | mq_lang::RuntimeValue::Dict(_) => flatten(value)
            .into_iter()
            .map(|(i, v)| format!("{}: {}", i, v).into())
            .collect(),
        _ => vec![value.to_string().into()],
    }
}

fn join_key(prefix: &str, suffix: &str) -> String {
    if suffix.starts_with('[') {
        format!("{}{}", prefix, suffix)
    } else {
        format!("{}.{}", prefix, suffix)
    }
}

fn flatten(value: &mq_lang::RuntimeValue) -> Vec<(String, mq_lang::RuntimeValue)> {
    match value {
        mq_lang::RuntimeValue::Dict(map) => map
            .iter()
            .flat_map(|(k, v)| {
                let nested = flatten(v);
                if nested.is_empty() {
                    vec![(k.as_str(), v.clone())]
                } else {
                    nested
                        .into_iter()
                        .map(|(nk, nv)| (join_key(&k.as_str(), &nk), nv))
                        .collect()
                }
            })
            .collect(),
        mq_lang::RuntimeValue::Array(items) => items
            .iter()
            .enumerate()
            .flat_map(|(i, v)| {
                let prefix = format!("[{}]", i);
                let nested = flatten(v);
                if nested.is_empty() {
                    vec![(prefix, v.clone())]
                } else {
                    nested
                        .into_iter()
                        .map(|(nk, nv)| (join_key(&prefix, &nk), nv))
                        .collect()
                }
            })
            .collect(),
        _ => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;
    use std::collections::BTreeMap;

    #[rstest]
    #[case::with_filename_and_line(Some("file.md".to_string()), Some(5), "## Heading", ":", "file.md:5:## Heading\n")]
    #[case::context_separator(Some("file.md".to_string()), Some(4), "Some text.", "-", "file.md-4-Some text.\n")]
    #[case::without_filename(None, Some(3), "Paragraph.", ":", "3:Paragraph.\n")]
    #[case::without_line_number(Some("file.md".to_string()), None, "text", ":", "file.md:text\n")]
    #[case::no_filename_no_line(None, None, "plain", ":", "plain\n")]
    fn test_format_line(
        #[case] filename: Option<String>,
        #[case] line_num: Option<usize>,
        #[case] content: &str,
        #[case] sep: &str,
        #[case] expected: &str,
    ) {
        assert_eq!(format_line(&filename, line_num, content, sep), expected);
    }

    #[rstest]
    #[case::empty(vec![], vec![])]
    #[case::single(vec![(2, 5)], vec![(2, 5)])]
    #[case::overlapping(vec![(0, 3), (2, 5)], vec![(0, 5)])]
    #[case::adjacent(vec![(0, 1), (2, 3)], vec![(0, 3)])]
    #[case::non_adjacent(vec![(0, 1), (3, 4)], vec![(0, 1), (3, 4)])]
    #[case::adjacent_chain(vec![(0, 1), (2, 3), (4, 5)], vec![(0, 5)])]
    #[case::mixed(vec![(0, 1), (2, 3), (5, 6)], vec![(0, 3), (5, 6)])]
    #[case::already_merged(vec![(0, 10)], vec![(0, 10)])]
    fn test_merge_ranges(#[case] input: Vec<(usize, usize)>, #[case] expected: Vec<(usize, usize)>) {
        assert_eq!(merge_ranges(input), expected);
    }

    #[rstest]
    #[case::non_collection(
        mq_lang::RuntimeValue::String("hello".to_string()),
        vec![]
    )]
    #[case::empty_array(
        mq_lang::RuntimeValue::Array(vec![]),
        vec![]
    )]
    #[case::flat_array(
        mq_lang::RuntimeValue::Array(vec![
            mq_lang::RuntimeValue::String("x".to_string()),
            mq_lang::RuntimeValue::String("y".to_string()),
        ]),
        vec![
            ("[0]".to_string(), mq_lang::RuntimeValue::String("x".to_string())),
            ("[1]".to_string(), mq_lang::RuntimeValue::String("y".to_string())),
        ]
    )]
    #[case::nested_array(
        mq_lang::RuntimeValue::Array(vec![
            mq_lang::RuntimeValue::Array(vec![
                mq_lang::RuntimeValue::String("nested".to_string()),
            ]),
        ]),
        vec![("[0][0]".to_string(), mq_lang::RuntimeValue::String("nested".to_string()))]
    )]
    fn test_flatten(#[case] input: mq_lang::RuntimeValue, #[case] expected: Vec<(String, mq_lang::RuntimeValue)>) {
        assert_eq!(flatten(&input), expected);
    }

    #[test]
    fn test_flatten_flat_dict() {
        let mut m = BTreeMap::new();
        m.insert(
            mq_lang::Ident::new("key"),
            mq_lang::RuntimeValue::String("val".to_string()),
        );
        assert_eq!(
            flatten(&mq_lang::RuntimeValue::Dict(m)),
            vec![("key".to_string(), mq_lang::RuntimeValue::String("val".to_string()))]
        );
    }

    #[test]
    fn test_flatten_nested_dict() {
        let mut inner = BTreeMap::new();
        inner.insert(
            mq_lang::Ident::new("b"),
            mq_lang::RuntimeValue::String("deep".to_string()),
        );
        let mut outer = BTreeMap::new();
        outer.insert(mq_lang::Ident::new("a"), mq_lang::RuntimeValue::Dict(inner));
        assert_eq!(
            flatten(&mq_lang::RuntimeValue::Dict(outer)),
            vec![("a.b".to_string(), mq_lang::RuntimeValue::String("deep".to_string()))]
        );
    }

    #[test]
    fn test_flatten_dict_with_array() {
        // dict["key"][0] → "key[0]"
        let mut m = BTreeMap::new();
        m.insert(
            mq_lang::Ident::new("key"),
            mq_lang::RuntimeValue::Array(vec![mq_lang::RuntimeValue::String("val".to_string())]),
        );
        assert_eq!(
            flatten(&mq_lang::RuntimeValue::Dict(m)),
            vec![("key[0]".to_string(), mq_lang::RuntimeValue::String("val".to_string()))]
        );
    }

    #[test]
    fn test_flatten_array_with_dict() {
        // [0].key → "[0].key"
        let mut m = BTreeMap::new();
        m.insert(
            mq_lang::Ident::new("b"),
            mq_lang::RuntimeValue::String("val".to_string()),
        );
        let input = mq_lang::RuntimeValue::Array(vec![mq_lang::RuntimeValue::Dict(m)]);
        assert_eq!(
            flatten(&input),
            vec![("[0].b".to_string(), mq_lang::RuntimeValue::String("val".to_string()))]
        );
    }

    #[rstest]
    #[case::string(
        mq_lang::RuntimeValue::String("hello".to_string()),
        vec!["hello".to_string()]
    )]
    #[case::boolean(
        mq_lang::RuntimeValue::Boolean(true),
        vec!["true".to_string()]
    )]
    #[case::empty_array(
        mq_lang::RuntimeValue::Array(vec![]),
        vec![]
    )]
    #[case::flat_array(
        mq_lang::RuntimeValue::Array(vec![
            mq_lang::RuntimeValue::String("x".to_string()),
            mq_lang::RuntimeValue::String("y".to_string()),
        ]),
        vec!["[0]: x".to_string(), "[1]: y".to_string()]
    )]
    fn test_to_nodes(#[case] input: mq_lang::RuntimeValue, #[case] expected: Vec<String>) {
        let actual: Vec<String> = to_nodes(&input).into_iter().map(|n| n.to_string()).collect();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_to_nodes_dict() {
        let mut m = BTreeMap::new();
        m.insert(
            mq_lang::Ident::new("key"),
            mq_lang::RuntimeValue::String("val".to_string()),
        );
        let actual: Vec<String> = to_nodes(&mq_lang::RuntimeValue::Dict(m))
            .into_iter()
            .map(|n| n.to_string())
            .collect();
        assert_eq!(actual, vec!["key: val"]);
    }
}
