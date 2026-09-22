//! Unified diff rendering using the shared Myers-only engine.

use mq_lang::diff::TextDiff;
use similar::udiff::UnifiedHunkHeader;
use std::fmt::Write;

pub(crate) fn unified_diff(old: &str, new: &str, label: &str) -> String {
    let diff = TextDiff::from_lines(old, new);
    let mut output = String::new();
    for (index, ops) in diff.grouped_ops(3).iter().enumerate() {
        if index == 0 {
            let _ = writeln!(output, "--- {label}\n+++ {label}");
        }
        let _ = writeln!(output, "{}", UnifiedHunkHeader::new(ops));
        for change in ops.iter().flat_map(|op| diff.iter_changes(op)) {
            let _ = write!(output, "{}{}", change.tag(), change.value());
            if !change.value().ends_with(['\r', '\n']) {
                output.push_str("\n\\ No newline at end of file\n");
            }
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use similar::TextDiff as ReferenceTextDiff;

    #[test]
    fn unified_output_matches_similar() {
        let cases = ["", "a", "a\n", "a\r", "a\r\n", "a\nb\nc", "日本語🙂\n", "a\nb\na\n"];
        for old in cases {
            for new in cases {
                assert_eq!(
                    unified_diff(old, new, "input.md"),
                    ReferenceTextDiff::from_lines(old, new)
                        .unified_diff()
                        .header("input.md", "input.md")
                        .to_string(),
                    "{old:?} -> {new:?}"
                );
            }
        }
        let old = (0..200).map(|i| format!("line {i}\n")).collect::<String>();
        let new = old.replace("line 20\n", "changed\n").replace("line 150\n", "変更\n");
        assert_eq!(
            unified_diff(&old, &new, "input.md"),
            ReferenceTextDiff::from_lines(&old, &new)
                .unified_diff()
                .header("input.md", "input.md")
                .to_string()
        );
    }
}
