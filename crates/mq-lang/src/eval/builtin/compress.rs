//! `token_compress` builtin: reduces a Markdown node list to a token budget in stages (ordered
//! least- to most-destructive) instead of a single blind char-truncate.

use mq_markdown::{Code, Node};

const TAIL_SHARE_DIVISOR: usize = 4;
const HARD_TRUNCATE_SEPARATOR: &str = "\n\n...\n\n";

/// Reduces `nodes` to fit within `budget` tokens, trying progressively more aggressive stages
/// and stopping as soon as the budget is met.
pub(super) fn token_compress(nodes: Vec<Node>, budget: usize, counter: &dyn Fn(&str) -> usize) -> Vec<Node> {
    if tokens_of(&nodes, counter) <= budget {
        return nodes;
    }

    let nodes = trim_paragraphs(nodes, budget, counter);
    if tokens_of(&nodes, counter) <= budget {
        return nodes;
    }

    let nodes = collapse_lists(nodes, budget, counter);
    if tokens_of(&nodes, counter) <= budget {
        return nodes;
    }

    let nodes = collapse_tables(nodes, budget, counter);
    if tokens_of(&nodes, counter) <= budget {
        return nodes;
    }

    let nodes = collapse_code(nodes);
    if tokens_of(&nodes, counter) <= budget {
        return nodes;
    }

    hard_truncate(&nodes, budget, counter)
}

fn tokens_of(nodes: &[Node], counter: &dyn Fn(&str) -> usize) -> usize {
    nodes.iter().map(|n| counter(&n.value())).sum()
}

/// Binary-searches the largest `n` in `0..=len` such that `candidate(n)` stays within `budget`.
/// Assumes `candidate` is monotonically non-decreasing in `n`; doesn't verify `candidate(0)`
/// itself fits, so a `budget` too tight for anything can still overshoot slightly.
fn largest_fit(len: usize, budget: usize, mut candidate: impl FnMut(usize) -> usize) -> usize {
    let (mut lo, mut hi) = (0usize, len);
    while lo < hi {
        let mid = lo + (hi - lo).div_ceil(2);
        if candidate(mid) <= budget {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    lo
}

/// mq-markdown has no `Paragraph` node; a paragraph is an unmarked run of inline siblings
/// between these block markers.
fn is_block_marker(node: &Node) -> bool {
    node.is_heading(None)
        || node.is_list()
        || matches!(node, Node::Code(_))
        || matches!(node, Node::TableRow(_))
        || node.is_table_align()
        || node.is_blockquote()
        || node.is_horizontal_rule()
}

enum Segment {
    Marker(Node),
    Paragraph(Vec<Node>),
}

fn segment_tokens(segment: &Segment, counter: &dyn Fn(&str) -> usize) -> usize {
    match segment {
        Segment::Marker(node) => counter(&node.value()),
        Segment::Paragraph(nodes) => tokens_of(nodes, counter),
    }
}

fn split_into_segments(nodes: Vec<Node>) -> Vec<Segment> {
    let mut segments = Vec::new();
    let mut run: Vec<Node> = Vec::new();

    for node in nodes {
        if is_block_marker(&node) {
            if !run.is_empty() {
                segments.push(Segment::Paragraph(std::mem::take(&mut run)));
            }
            segments.push(Segment::Marker(node));
        } else {
            run.push(node);
        }
    }
    if !run.is_empty() {
        segments.push(Segment::Paragraph(run));
    }

    segments
}

/// Collapses paragraph runs down to their first sentence, largest run first, stopping as soon as
/// `budget` is met so smaller paragraphs can be left fully intact.
fn trim_paragraphs(nodes: Vec<Node>, budget: usize, counter: &dyn Fn(&str) -> usize) -> Vec<Node> {
    let mut segments = split_into_segments(nodes);
    let mut total: usize = segments.iter().map(|s| segment_tokens(s, counter)).sum();

    let mut run_indices: Vec<usize> = segments
        .iter()
        .enumerate()
        .filter(|(_, s)| matches!(s, Segment::Paragraph(_)))
        .map(|(i, _)| i)
        .collect();
    run_indices.sort_by_key(|&i| std::cmp::Reverse(segment_tokens(&segments[i], counter)));

    for i in run_indices {
        if total <= budget {
            break;
        }
        let Segment::Paragraph(run) = &segments[i] else {
            continue;
        };
        if let Some(sentence) = trimmed_paragraph(run) {
            let before = segment_tokens(&segments[i], counter);
            let after = counter(&sentence);
            if after < before {
                total -= before - after;
                segments[i] = Segment::Paragraph(vec![Node::from(sentence)]);
            }
        }
    }

    segments
        .into_iter()
        .flat_map(|s| match s {
            Segment::Marker(node) => vec![node],
            Segment::Paragraph(nodes) => nodes,
        })
        .collect()
}

fn trimmed_paragraph(run: &[Node]) -> Option<String> {
    let text: String = run.iter().map(|n| n.to_string()).collect();
    match first_sentence(&text) {
        Some(sentence) if sentence.len() < text.trim().len() => Some(sentence),
        _ => None,
    }
}

/// First sentence of `text`, or `None` if no boundary is found. CJK terminators (`。！？`) always
/// end a sentence (no space follows in CJK prose); Latin `.!?` only count when followed by
/// whitespace/EOF, so decimals and abbreviations (`3.14`) aren't mistaken for boundaries.
fn first_sentence(text: &str) -> Option<String> {
    let trimmed = text.trim();
    let chars: Vec<(usize, char)> = trimmed.char_indices().collect();

    for (i, (byte_idx, c)) in chars.iter().enumerate() {
        let is_boundary = match c {
            '。' | '！' | '？' => true,
            '.' | '!' | '?' => chars.get(i + 1).map(|(_, next)| next.is_whitespace()).unwrap_or(true),
            _ => false,
        };
        if is_boundary {
            return Some(trimmed[..byte_idx + c.len_utf8()].to_string());
        }
    }

    None
}

/// Caps each run of consecutive `List` siblings to whatever fits the remaining budget.
fn collapse_lists(nodes: Vec<Node>, budget: usize, counter: &dyn Fn(&str) -> usize) -> Vec<Node> {
    let mut result = Vec::with_capacity(nodes.len());
    let mut run: Vec<Node> = Vec::new();

    for node in nodes {
        if node.is_list() {
            run.push(node);
        } else {
            flush_capped_run(&mut run, &mut result, budget, counter, "list items");
            result.push(node);
        }
    }
    flush_capped_run(&mut run, &mut result, budget, counter, "list items");

    result
}

fn flush_capped_run(
    run: &mut Vec<Node>,
    result: &mut Vec<Node>,
    budget: usize,
    counter: &dyn Fn(&str) -> usize,
    label: &str,
) {
    if run.is_empty() {
        return;
    }

    let remaining = budget.saturating_sub(tokens_of(result, counter));
    result.extend(cap_by_budget(run, remaining, counter, label));
    run.clear();
}

/// Caps table body rows to whatever fits the remaining budget; the header row is positional
/// (whatever comes before `TableAlign` in the run), since `TableRow` itself has no header flag.
fn collapse_tables(nodes: Vec<Node>, budget: usize, counter: &dyn Fn(&str) -> usize) -> Vec<Node> {
    let mut result = Vec::with_capacity(nodes.len());
    let mut run: Vec<Node> = Vec::new();

    for node in nodes {
        if node.is_table_align() || matches!(node, Node::TableRow(_)) {
            run.push(node);
        } else {
            flush_table_run(&mut run, &mut result, budget, counter);
            result.push(node);
        }
    }
    flush_table_run(&mut run, &mut result, budget, counter);

    result
}

fn flush_table_run(run: &mut Vec<Node>, result: &mut Vec<Node>, budget: usize, counter: &dyn Fn(&str) -> usize) {
    if run.is_empty() {
        return;
    }

    if let Some(align_idx) = run.iter().position(Node::is_table_align) {
        let body_start = align_idx + 1;
        let header = &run[..body_start];
        let body = &run[body_start..];

        let remaining = budget
            .saturating_sub(tokens_of(result, counter))
            .saturating_sub(tokens_of(header, counter));

        result.extend_from_slice(header);
        result.extend(cap_by_budget(body, remaining, counter, "table rows"));
        run.clear();
        return;
    }

    result.append(run);
}

/// Largest prefix of `run` whose tokens, plus an omitted-count marker for the rest, fit within
/// `remaining`.
fn cap_by_budget(run: &[Node], remaining: usize, counter: &dyn Fn(&str) -> usize, label: &str) -> Vec<Node> {
    if tokens_of(run, counter) <= remaining {
        return run.to_vec();
    }

    let keep = largest_fit(run.len(), remaining, |n| {
        let omitted = run.len() - n;
        let marker_tokens = if omitted > 0 {
            counter(&omitted_marker(omitted, label))
        } else {
            0
        };
        tokens_of(&run[..n], counter) + marker_tokens
    });

    let mut result: Vec<Node> = run[..keep].to_vec();
    if keep < run.len() {
        result.push(Node::from(omitted_marker(run.len() - keep, label)));
    }
    result
}

fn omitted_marker(omitted: usize, label: &str) -> String {
    format!("_(+{omitted} more {label} omitted)_")
}

/// Replaces each code block's body with a `lang`/line-count placeholder.
fn collapse_code(nodes: Vec<Node>) -> Vec<Node> {
    nodes
        .into_iter()
        .map(|node| {
            if let Node::Code(Code {
                ref value, ref lang, ..
            }) = node
            {
                let lines = value.lines().count();
                let lang_label = lang.as_deref().unwrap_or("code");
                let placeholder = format!("... ({lines} lines of {lang_label} omitted)");
                node.with_value(&placeholder)
            } else {
                node
            }
        })
        .collect()
}

/// Last resort: keeps a word-boundary-snapped head and tail of the rendered text, joined by an
/// ellipsis, so the closing content survives too, not just the opening. Falls back to a
/// prefix-only truncation when `budget` is too tight to spare anything for a tail.
fn hard_truncate(nodes: &[Node], budget: usize, counter: &dyn Fn(&str) -> usize) -> Vec<Node> {
    let text: String = nodes.iter().map(|n| n.to_string()).collect::<Vec<_>>().join("\n\n");
    if counter(&text) <= budget {
        return vec![Node::from(text)];
    }
    let chars: Vec<char> = text.chars().collect();

    let separator_tokens = counter(HARD_TRUNCATE_SEPARATOR);
    let usable = budget.saturating_sub(separator_tokens);
    let tail_budget = usable / TAIL_SHARE_DIVISOR;
    let head_budget = usable - tail_budget;

    let tail_len = largest_fit(chars.len(), tail_budget, |n| {
        counter(&chars[chars.len() - n..].iter().collect::<String>())
    });
    if tail_len == 0 {
        return vec![Node::from(prefix_truncated(&chars, budget, counter))];
    }

    let head_len = largest_fit(chars.len() - tail_len, head_budget, |n| {
        counter(&chars[..n].iter().collect::<String>())
    });
    let head_end = snap_prefix_boundary(&chars, head_len);
    let tail_start = snap_suffix_start(&chars, chars.len() - tail_len).max(head_end);

    let head: String = chars[..head_end].iter().collect::<String>().trim_end().to_string();
    let tail: String = chars[tail_start..].iter().collect::<String>().trim_start().to_string();

    vec![Node::from(format!("{head}{HARD_TRUNCATE_SEPARATOR}{tail}"))]
}

fn prefix_truncated(chars: &[char], budget: usize, counter: &dyn Fn(&str) -> usize) -> String {
    let len = largest_fit(chars.len(), budget, |n| counter(&chars[..n].iter().collect::<String>()));
    let end = snap_prefix_boundary(chars, len);
    let mut truncated: String = chars[..end].iter().collect::<String>().trim_end().to_string();
    truncated.push_str("...");
    truncated
}

fn snap_prefix_boundary(chars: &[char], end: usize) -> usize {
    let mut end = end;
    if end > 0 && end < chars.len() && !chars[end - 1].is_whitespace() && !chars[end].is_whitespace() {
        while end > 0 && !chars[end - 1].is_whitespace() {
            end -= 1;
        }
    }
    end
}

fn snap_suffix_start(chars: &[char], start: usize) -> usize {
    let mut start = start;
    if start > 0 && start < chars.len() && !chars[start - 1].is_whitespace() && !chars[start].is_whitespace() {
        while start < chars.len() && !chars[start].is_whitespace() {
            start += 1;
        }
    }
    start
}

#[cfg(test)]
mod tests {
    use super::*;
    use mq_markdown::{Heading, List, TableAlign, TableRow, Text};
    use rstest::rstest;

    use super::super::tokenizer::token_count_estimate as count;

    fn text(value: &str) -> Node {
        Node::Text(Text {
            value: value.to_string(),
            position: None,
        })
    }

    fn heading(depth: u8, value: &str) -> Node {
        Node::Heading(Heading {
            depth,
            values: vec![text(value)],
            position: None,
        })
    }

    fn list_item(value: &str, index: usize) -> Node {
        Node::List(List {
            values: vec![text(value)],
            index,
            level: 0,
            ordered: false,
            checked: None,
            start: None,
            position: None,
        })
    }

    fn code(value: &str, lang: &str) -> Node {
        Node::Code(Code {
            value: value.to_string(),
            lang: Some(lang.to_string()),
            position: None,
            meta: None,
            fence: true,
        })
    }

    #[test]
    fn already_under_budget_is_untouched() {
        let nodes = vec![heading(1, "Title"), text("A short paragraph.")];
        let result = token_compress(nodes.clone(), 1000, &count);
        assert_eq!(result, nodes);
    }

    #[test]
    fn empty_input_stays_empty() {
        assert_eq!(token_compress(Vec::new(), 10, &count), Vec::new());
    }

    #[test]
    fn paragraph_is_cut_to_first_sentence() {
        let nodes = vec![
            heading(1, "Title"),
            text(
                "First sentence. Second sentence. Third sentence, much longer, to push the total well past any tiny budget.",
            ),
        ];
        // Big enough that trimming to "Title" + "First sentence." already fits (2 + 4 tokens),
        // but small enough that the untrimmed paragraph doesn't.
        let result = token_compress(nodes, 6, &count);
        assert_eq!(result.len(), 2);
        assert_eq!(result[1].value(), "First sentence.");
    }

    #[test]
    fn paragraph_without_sentence_boundary_is_left_alone() {
        let nodes = vec![text("no terminal punctuation here at all just words")];
        let result = trim_paragraphs(nodes.clone(), 0, &count);
        assert_eq!(result, nodes);
    }

    #[test]
    fn trim_paragraphs_prioritizes_largest_run_first() {
        let big = text("First sentence in big paragraph. Second sentence padding padding padding padding.");
        let small = text("Small one. More.");
        let nodes = vec![heading(1, "Title"), big.clone(), heading(2, "Sub"), small.clone()];

        let heading_tokens = tokens_of(&[nodes[0].clone(), nodes[2].clone()], &count);
        let big_trimmed = first_sentence(&big.to_string()).unwrap();
        let budget = heading_tokens + count(&big_trimmed) + count(&small.to_string());

        let result = trim_paragraphs(nodes, budget, &count);

        assert_eq!(result[1].value(), big_trimmed);
        assert_eq!(result[3].value(), small.to_string());
    }

    #[rstest]
    #[case("Hello world.", Some("Hello world.".to_string()))]
    #[case("Hi! Bye.", Some("Hi!".to_string()))]
    #[case("こんにちは。さようなら。", Some("こんにちは。".to_string()))]
    #[case("no boundary", None)]
    #[case("3.14 is pi", None)]
    fn test_first_sentence(#[case] input: &str, #[case] expected: Option<String>) {
        assert_eq!(first_sentence(input), expected);
    }

    #[test]
    fn list_run_within_budget_is_kept_in_full() {
        let nodes: Vec<Node> = (0..6).map(|i| list_item(&format!("item {i}"), i)).collect();
        let result = collapse_lists(nodes.clone(), 1000, &count);
        assert_eq!(result, nodes);
    }

    #[test]
    fn list_run_is_capped_by_remaining_budget() {
        let nodes: Vec<Node> = (0..6).map(|_| list_item("padding padding padding", 0)).collect();
        let per_item = count("padding padding padding");
        let marker = omitted_marker(4, "list items");
        let budget = per_item * 2 + count(&marker);

        let result = collapse_lists(nodes, budget, &count);

        assert_eq!(result.len(), 3);
        assert_eq!(result[0].value(), "padding padding padding");
        assert_eq!(result[1].value(), "padding padding padding");
        assert!(result[2].value().contains("+4 more list items omitted"));
    }

    #[test]
    fn table_body_rows_are_capped_by_remaining_budget() {
        let header = Node::TableRow(TableRow {
            values: vec![text("col")],
            position: None,
        });
        let align = Node::TableAlign(TableAlign {
            align: vec![],
            position: None,
        });
        let rows: Vec<Node> = (0..6)
            .map(|_| {
                Node::TableRow(TableRow {
                    values: vec![text("padding padding padding")],
                    position: None,
                })
            })
            .collect();
        let mut nodes = vec![header.clone(), align.clone()];
        nodes.extend(rows.clone());

        let header_tokens = tokens_of(&[header, align], &count);
        let per_row = count("padding padding padding");
        let marker = omitted_marker(4, "table rows");
        let budget = header_tokens + per_row * 2 + count(&marker);

        let result = collapse_tables(nodes, budget, &count);

        assert_eq!(result.len(), 2 + 2 + 1);
        assert!(result.last().unwrap().value().contains("+4 more table rows omitted"));
    }

    #[test]
    fn later_stage_is_skipped_once_earlier_stage_meets_budget() {
        let header = Node::TableRow(TableRow {
            values: vec![text("col")],
            position: None,
        });
        let align = Node::TableAlign(TableAlign {
            align: vec![],
            position: None,
        });
        let rows: Vec<Node> = (0..5)
            .map(|i| {
                Node::TableRow(TableRow {
                    values: vec![text(&format!("row {i}"))],
                    position: None,
                })
            })
            .collect();
        let list_items: Vec<Node> = (0..20).map(|_| list_item("padding padding padding", 0)).collect();

        let mut nodes = vec![header.clone(), align.clone()];
        nodes.extend(rows.clone());
        nodes.extend(list_items.clone());

        let table_tokens = tokens_of(&[header, align], &count) + tokens_of(&rows, &count);
        let per_item = count("padding padding padding");
        let marker = omitted_marker(17, "list items");
        // Enough for the table in full, plus only 3 list items -- forces list capping but should
        // leave the table alone once collapse_lists already brings the total under budget.
        let budget = table_tokens + per_item * 3 + count(&marker);

        let result = token_compress(nodes, budget, &count);

        for row in &rows {
            assert!(
                result.iter().any(|n| n.value() == row.value()),
                "table row {:?} should be untouched",
                row.value()
            );
        }
        assert!(
            result.iter().any(|n| n.value().contains("more list items omitted")),
            "list should have been capped"
        );
    }

    #[test]
    fn code_block_is_collapsed_to_placeholder() {
        let nodes = vec![code("line1\nline2\nline3", "rust")];
        let result = collapse_code(nodes);
        assert_eq!(result.len(), 1);
        let value = result[0].value();
        assert!(value.contains("3 lines of rust omitted"), "got: {value}");
    }

    #[test]
    fn hard_truncate_is_last_resort_and_fits_budget() {
        let nodes = vec![text("a".repeat(500).as_str())];
        let result = token_compress(nodes, 5, &count);
        assert_eq!(result.len(), 1);
        assert!(count(&result[0].value()) <= 5 + count("..."));
    }

    #[test]
    fn hard_truncate_keeps_head_and_tail_without_cutting_words() {
        let words: Vec<String> = (0..80).map(|i| format!("word{i}")).collect();
        let nodes = vec![text(&words.join(" "))];

        let result = hard_truncate(&nodes, 40, &count);

        assert_eq!(result.len(), 1);
        let value = result[0].value();
        assert!(value.contains("..."), "expected an ellipsis marker, got: {value}");
        assert!(value.starts_with("word0 "), "got: {value}");
        assert!(value.ends_with("word79"), "got: {value}");
        for token in value.split_whitespace() {
            if let Some(rest) = token.strip_prefix("word") {
                let rest = rest.trim_end_matches('.');
                assert!(
                    !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()),
                    "word cut mid-token: {token}"
                );
            }
        }
    }

    #[test]
    fn zero_budget_does_not_panic() {
        let nodes = vec![heading(1, "Title"), text("Some paragraph text here.")];
        let result = token_compress(nodes, 0, &count);
        assert!(tokens_of(&result, &count) <= count("..."));
    }
}
