//! Checks that the parse+render round trip used by `-U` preserves rendered
//! meaning across the GFM spec's example corpus, the same methodology used
//! to find https://github.com/harehare/mq/issues/2148.
//!
//! Not run by `just test-all` because it fetches `spec.txt` over the network
//! on every run; run it explicitly via `just test-gfm-spec`.
//!
//! The spec text itself (CC-BY-SA 4.0, distinct from mq's own MIT license)
//! is intentionally not vendored into this repository. It's fetched fresh
//! from a pinned commit of `github/cmark-gfm` so nothing under that license
//! is stored in or distributed with mq, and so the example numbering behind
//! `KNOWN_FAILURES` below stays stable.

use mq_markdown::{Markdown, to_html};

const SPEC_URL: &str =
    "https://raw.githubusercontent.com/github/cmark-gfm/828322d1ee4facdab56f0d3edccb13e9af90dcd2/test/spec.txt";

struct SpecExample {
    number: usize,
    section: String,
    markdown: String,
}

/// Parses the CommonMark/GFM `spec.txt` example format: sections are `#`
/// headings, and each example is a fence of 32 backticks followed by
/// `example`, the markdown source, a lone `.` line, the expected HTML, then
/// a closing fence matching the opening one. The expected HTML is skipped;
/// this only needs the markdown source to run its own before/after check.
fn parse_spec_examples(text: &str) -> Vec<SpecExample> {
    let fence = "`".repeat(32);
    let open = format!("{fence} example");

    let mut examples = Vec::new();
    let mut section = String::new();
    let mut number = 0usize;
    let mut lines = text.lines();

    while let Some(line) = lines.next() {
        if let Some(rest) = line.strip_prefix('#') {
            section = rest.trim_start_matches('#').trim().to_string();
            continue;
        }
        if line == open {
            let mut markdown_lines: Vec<&str> = Vec::new();
            for l in lines.by_ref() {
                if l == "." {
                    break;
                }
                markdown_lines.push(l);
            }
            for l in lines.by_ref() {
                if l == fence {
                    break;
                }
            }
            number += 1;
            examples.push(SpecExample {
                number,
                section: section.clone(),
                // `→` is spec.txt's only escape, standing in for a literal tab.
                markdown: markdown_lines.join("\n").replace('\u{2192}', "\t") + "\n",
            });
        }
    }
    examples
}

/// Examples where the round trip currently changes rendered meaning, by
/// number in the spec revision pinned above. Remove an entry once it's
/// fixed; add one (with its section) if a new gap turns up.
const KNOWN_FAILURES: &[usize] = &[
    5, 6, 7, // Tabs
    229, 269, // List items
    271, 272, 295, // Lists
    // 300, 306, 346, 479, 480, 599: URL-shaped text disables backslash
    // escapes even in link text (GFM extended autolink scanning).
    300, 306, // Backslash escapes
    346, // Code spans
    479, 480, // Emphasis and strong emphasis
    522, 534, // Links
    570, 589, // Images
    599, 602, // Autolinks
];

#[test]
#[ignore = "fetches spec.txt over the network; run via `just test-gfm-spec`"]
fn gfm_round_trip_fidelity() {
    let body = ureq::get(SPEC_URL)
        .call()
        .expect("failed to fetch the GFM spec")
        .body_mut()
        .read_to_string()
        .expect("failed to read the GFM spec response body");

    let examples = parse_spec_examples(&body);
    assert!(
        examples.len() > 600,
        "parsed only {} examples from spec.txt, the parser is likely out of sync with its format",
        examples.len()
    );

    let mut new_failures = Vec::new();
    let mut newly_fixed = Vec::new();

    for example in &examples {
        let before = to_html(&example.markdown);
        let round_tripped = example
            .markdown
            .parse::<Markdown>()
            .unwrap_or_else(|e| panic!("example #{} [{}] failed to parse: {e}", example.number, example.section))
            .to_string();
        let after = to_html(&round_tripped);

        let changed = before != after;
        let known = KNOWN_FAILURES.contains(&example.number);

        if changed && !known {
            new_failures.push(format!("#{} [{}]", example.number, example.section));
        } else if !changed && known {
            newly_fixed.push(example.number);
        }
    }

    assert!(
        new_failures.is_empty(),
        "new round-trip fidelity regressions not in KNOWN_FAILURES: {new_failures:#?}"
    );
    assert!(
        newly_fixed.is_empty(),
        "these examples round-trip correctly now, remove them from KNOWN_FAILURES: {newly_fixed:?}"
    );

    // Passing only means no *new* regressions were found, not that
    // round-trip fidelity is complete: this many examples are still known
    // to be broken and tracked in KNOWN_FAILURES above.
    println!(
        "{} known round-trip fidelity gaps still open, out of {} examples checked",
        KNOWN_FAILURES.len(),
        examples.len()
    );
}
