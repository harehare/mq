//! `token_compress` builtin: reduces a Markdown node list to a token budget in stages (ordered
//! least- to most-destructive) instead of a single blind char-truncate.

use mq_markdown::{Code, Node};

use super::tokenizer::token_count_estimate;

/// Max list items (and table body rows) kept per contiguous run once collection-trimming kicks in.
const MAX_COLLECTION_ITEMS: usize = 3;

/// Reduces `nodes` to fit within `budget` (estimated LLM tokens), trying progressively more
/// aggressive stages and stopping as soon as the budget is met.
pub(super) fn compress(nodes: Vec<Node>, budget: usize) -> Vec<Node> {
    if total_tokens(&nodes) <= budget {
        return nodes;
    }

    let nodes = trim_paragraphs(nodes);
    if total_tokens(&nodes) <= budget {
        return nodes;
    }

    let nodes = collapse_lists(nodes);
    let nodes = collapse_tables(nodes);
    let nodes = collapse_code(nodes);
    if total_tokens(&nodes) <= budget {
        return nodes;
    }

    hard_truncate(&nodes, budget)
}

fn total_tokens(nodes: &[Node]) -> usize {
    nodes.iter().map(|n| token_count_estimate(&n.value())).sum()
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

/// Collapses each paragraph run down to its first sentence; leaves runs with no sentence
/// boundary untouched.
fn trim_paragraphs(nodes: Vec<Node>) -> Vec<Node> {
    let mut result = Vec::with_capacity(nodes.len());
    let mut run: Vec<Node> = Vec::new();

    for node in nodes {
        if is_block_marker(&node) {
            flush_paragraph_run(&mut run, &mut result);
            result.push(node);
        } else {
            run.push(node);
        }
    }
    flush_paragraph_run(&mut run, &mut result);

    result
}

fn flush_paragraph_run(run: &mut Vec<Node>, result: &mut Vec<Node>) {
    if run.is_empty() {
        return;
    }

    let text: String = run.iter().map(|n| n.to_string()).collect();
    match first_sentence(&text) {
        Some(sentence) if sentence.len() < text.trim().len() => result.push(Node::from(sentence)),
        _ => result.append(run),
    }
    run.clear();
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

/// Caps each run of consecutive `List` siblings to [`MAX_COLLECTION_ITEMS`], noting how many
/// were omitted.
fn collapse_lists(nodes: Vec<Node>) -> Vec<Node> {
    let mut result = Vec::with_capacity(nodes.len());
    let mut run: Vec<Node> = Vec::new();

    for node in nodes {
        if node.is_list() {
            run.push(node);
        } else {
            flush_capped_run(&mut run, &mut result, MAX_COLLECTION_ITEMS, "list items");
            result.push(node);
        }
    }
    flush_capped_run(&mut run, &mut result, MAX_COLLECTION_ITEMS, "list items");

    result
}

/// Caps table body rows to [`MAX_COLLECTION_ITEMS`]; the header row is positional (whatever comes
/// before `TableAlign` in the run), since `TableRow` itself has no header flag.
fn collapse_tables(nodes: Vec<Node>) -> Vec<Node> {
    let mut result = Vec::with_capacity(nodes.len());
    let mut run: Vec<Node> = Vec::new();

    for node in nodes {
        if node.is_table_align() || matches!(node, Node::TableRow(_)) {
            run.push(node);
        } else {
            flush_table_run(&mut run, &mut result);
            result.push(node);
        }
    }
    flush_table_run(&mut run, &mut result);

    result
}

fn flush_table_run(run: &mut Vec<Node>, result: &mut Vec<Node>) {
    if run.is_empty() {
        return;
    }

    if let Some(align_idx) = run.iter().position(Node::is_table_align) {
        let body_start = align_idx + 1;
        let body_len = run.len() - body_start;
        if body_len > MAX_COLLECTION_ITEMS {
            let omitted = body_len - MAX_COLLECTION_ITEMS;
            result.extend(run.drain(..body_start + MAX_COLLECTION_ITEMS));
            result.push(Node::from(format!("_(+{omitted} more table rows omitted)_")));
            run.clear();
            return;
        }
    }

    result.append(run);
}

fn flush_capped_run(run: &mut Vec<Node>, result: &mut Vec<Node>, cap: usize, label: &str) {
    if run.len() > cap {
        let omitted = run.len() - cap;
        result.extend(run.drain(..cap));
        result.push(Node::from(format!("_(+{omitted} more {label} omitted)_")));
        run.clear();
    } else {
        result.append(run);
    }
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

/// Last resort: truncates the rendered text to the largest prefix fitting `budget`. Unlike the
/// earlier stages, this can cut off mid-sentence.
fn hard_truncate(nodes: &[Node], budget: usize) -> Vec<Node> {
    let text: String = nodes.iter().map(|n| n.to_string()).collect::<Vec<_>>().join("\n\n");

    if token_count_estimate(&text) <= budget {
        return vec![Node::from(text)];
    }

    let chars: Vec<char> = text.chars().collect();
    let (mut lo, mut hi) = (0usize, chars.len());
    while lo < hi {
        let mid = lo + (hi - lo).div_ceil(2);
        let prefix: String = chars[..mid].iter().collect();
        if token_count_estimate(&prefix) <= budget {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }

    let mut truncated: String = chars[..lo].iter().collect();
    truncated.push_str("...");
    vec![Node::from(truncated)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use mq_markdown::{Heading, List, TableAlign, TableRow, Text};
    use rstest::rstest;

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
        let result = compress(nodes.clone(), 1000);
        assert_eq!(result, nodes);
    }

    #[test]
    fn empty_input_stays_empty() {
        assert_eq!(compress(Vec::new(), 10), Vec::new());
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
        let result = compress(nodes, 6);
        assert_eq!(result.len(), 2);
        assert_eq!(result[1].value(), "First sentence.");
    }

    #[test]
    fn paragraph_without_sentence_boundary_is_left_alone() {
        let nodes = vec![text("no terminal punctuation here at all just words")];
        let result = trim_paragraphs(nodes.clone());
        assert_eq!(result, nodes);
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
    fn list_run_is_capped_with_omitted_count() {
        let nodes: Vec<Node> = (0..6).map(|i| list_item(&format!("item {i}"), i)).collect();
        let result = collapse_lists(nodes);
        assert_eq!(result.len(), MAX_COLLECTION_ITEMS + 1);
        assert!(result.last().unwrap().value().contains("+3 more list items omitted"));
    }

    #[test]
    fn short_list_run_is_untouched() {
        let nodes: Vec<Node> = (0..2).map(|i| list_item(&format!("item {i}"), i)).collect();
        let result = collapse_lists(nodes.clone());
        assert_eq!(result, nodes);
    }

    #[test]
    fn table_body_rows_are_capped_with_omitted_count() {
        let header = Node::TableRow(TableRow {
            values: vec![text("col")],
            position: None,
        });
        let align = Node::TableAlign(TableAlign {
            align: vec![],
            position: None,
        });
        let mut nodes = vec![header, align];
        nodes.extend((0..6).map(|i| {
            Node::TableRow(TableRow {
                values: vec![text(&format!("row {i}"))],
                position: None,
            })
        }));

        let result = collapse_tables(nodes);
        // header + align + MAX_COLLECTION_ITEMS body rows + omitted-count marker
        assert_eq!(result.len(), 2 + MAX_COLLECTION_ITEMS + 1);
        assert!(result.last().unwrap().value().contains("+3 more table rows omitted"));
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
        let result = compress(nodes, 5);
        assert_eq!(result.len(), 1);
        assert!(token_count_estimate(&result[0].value()) <= 5 + token_count_estimate("..."));
    }

    #[test]
    fn zero_budget_does_not_panic() {
        let nodes = vec![heading(1, "Title"), text("Some paragraph text here.")];
        let result = compress(nodes, 0);
        assert!(total_tokens(&result) <= token_count_estimate("..."));
    }
}
