//! Input that nests or repeats constructs very deeply must parse and render without exhausting the
//! stack, and without spending time that grows faster than the input.

use mq_markdown::Markdown;
use rstest::rstest;

fn nested(open: &str, content: &str, close: &str, times: usize) -> String {
    format!("{}{content}{}", open.repeat(times), close.repeat(times))
}

/// Runs `check` on a thread with the stack of a main thread: the test threads have less, and a debug
/// build uses far more of it than a release build does.
fn on_main_sized_stack(check: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(check)
        .unwrap()
        .join()
        .unwrap();
}

/// Everything that walks the nodes, each of which recurses over the nesting.
fn exercise(markdown: Markdown) {
    let _ = markdown.to_string();
    let _ = markdown.to_text();
    let _ = markdown.to_html();
    #[cfg(feature = "json")]
    let _ = markdown.to_json();
    let copy = markdown.clone();
    drop(markdown);
    drop(copy);
}

#[rstest]
#[case::emphasis(nested("*a ", "b", " a*", 100_000))]
#[case::emphasis_and_strong(nested("*a **a ", "b", " a** a*", 50_000))]
#[case::underscore(nested("_a ", "b", " a_", 100_000))]
#[case::strikethrough(nested("~a ", "b", " a~", 100_000))]
#[case::links(nested("[a ", "![x](y)", "](z)", 50_000))]
#[case::images(nested("![a ", "b", "](z)", 50_000))]
#[case::link_labels(nested("[a ", "b", "]", 100_000))]
#[case::emphasis_in_links(nested("[*a ", "b", " a*](x)", 40_000))]
#[case::quotes(format!("{}a", "> ".repeat(10_000)))]
#[case::quotes_with_emphasis(nested(&"> ".repeat(100), "b", "", 1) + &nested("*a ", "b", " a*", 50_000))]
#[case::lists((0..300).map(|level| format!("{}- a\n", "  ".repeat(level))).collect::<String>())]
fn deep_nesting_does_not_exhaust_the_stack(#[case] input: String) {
    on_main_sized_stack(move || exercise(Markdown::from_markdown_str(&input).unwrap()));
}

#[rstest]
#[case::inline(nested("<a>", "x", "</a>", 20_000))]
#[case::flow(nested("<a>\n\n", "x\n\n", "</a>\n\n", 20_000))]
fn deeply_nested_elements_are_an_error(#[case] input: String) {
    let error = Markdown::from_mdx_str(&input).unwrap_err().to_string();
    assert!(error.contains("nested deeper"), "{error}");
}

#[test]
fn elements_nested_within_the_limit_parse() {
    let input = nested("<a>", "x", "</a>", 100);
    on_main_sized_stack(move || exercise(Markdown::from_mdx_str(&input).unwrap()));
}

#[rstest]
#[case::wikilink_openers("[[a ".repeat(100_000))]
#[case::unmatched_openers("[".repeat(100_000))]
#[case::open_links_then_links(format!("{}{}", "[".repeat(50_000), "[a](b) ".repeat(50_000)))]
#[case::unmatched_emphasis("*a ".repeat(100_000))]
#[case::emphasis_that_cannot_match("*a_".repeat(100_000))]
#[case::list_markers_on_a_line(format!("{}a\n", "- ".repeat(20_000)).repeat(20))]
#[case::list_markers_ending_like_a_rule(format!("{}a -\n", "- ".repeat(20_000)).repeat(20))]
#[case::star_markers_ending_like_a_rule(format!("{}a *\n", "* ".repeat(20_000)).repeat(20))]
#[case::quote_markers_on_a_line(format!("{}a\n", "> ".repeat(20_000)).repeat(20))]
#[case::numbered_markers_on_a_line(format!("{}a\n", "1. ".repeat(20_000)).repeat(20))]
#[case::long_labels(format!("[a]: /u\n{}", "[b ".repeat(100_000)))]
#[case::many_emphasis_pairs("*a* _b_ **c** __d__ ~~e~~ ".repeat(30_000))]
#[case::many_emphasis_pairs_and_an_email("*a* b@c.de ".repeat(30_000))]
#[case::backtick_runs_of_every_length((1..3_000).map(|length| format!("e{}", "`".repeat(length))).collect::<String>())]
#[case::dollar_runs_of_every_length((1..3_000).map(|length| format!("e{}", "$".repeat(length))).collect::<String>())]
#[case::tabs_between_inlines(format!("x{}\n", "\t*a* ".repeat(40_000)))]
#[case::tabs_between_inlines_in_a_quote(format!("> x{}\n", "\t*a* ".repeat(40_000)))]
#[case::tabs_between_inlines_in_a_table(format!("|a|\n|-|\n|x{}|\n", "\t*a* ".repeat(40_000)))]
#[case::unclosed_comments("a <!--".repeat(300_000))]
#[case::unclosed_comments_after_a_closer(format!("</{}", "<!--".repeat(300_000)))]
#[case::unclosed_instructions("a <?".repeat(300_000))]
#[case::unclosed_cdata("a <![CDATA[".repeat(300_000))]
#[case::unclosed_declarations("a <!A".repeat(300_000))]
fn repeated_constructs_parse(#[case] input: String) {
    exercise(Markdown::from_markdown_str(&input).unwrap());
}

/// Every line of a list nested deeper than the limit is read again by each of the containers around it,
/// so the indentation of the line must not be scanned each time.
#[rstest]
#[case::bullets((0..500).map(|level| format!("{}- a\n", "  ".repeat(level))).collect::<String>())]
#[case::numbered((0..500).map(|level| format!("{}1. a\n", "   ".repeat(level))).collect::<String>())]
#[case::tabs((0..500).map(|level| format!("{}- a\n", "\t".repeat(level))).collect::<String>())]
#[case::quotes_and_bullets((0..500).map(|level| format!("{}{}- a\n", "> ".repeat(level / 2), "  ".repeat(level / 2))).collect::<String>())]
fn deeply_indented_nesting_parses(#[case] input: String) {
    on_main_sized_stack(move || exercise(Markdown::from_markdown_str(&input).unwrap()));
}

#[test]
fn repeated_unclosed_expressions_are_an_error() {
    let error = Markdown::from_mdx_str(&"a {".repeat(300_000)).unwrap_err().to_string();
    assert!(error.contains("closing brace"), "{error}");
}

#[rstest]
#[case::emphasis(
    "*a **a *a b a* a** a*\n",
    "<p><em>a <strong>a <em>a b a</em> a</strong> a</em></p>\n"
)]
#[case::rule_of_three("*foo**bar**baz*\n", "<p><em>foo<strong>bar</strong>baz</em></p>\n")]
#[case::partly_used_runs("***a** b*\n", "<p><em><strong>a</strong> b</em></p>\n")]
#[case::unmatched_stay_text("*a **b\n", "<p>*a **b</p>\n")]
#[case::closer_also_opens("*a*b*c*\n", "<p><em>a</em>b<em>c</em></p>\n")]
#[case::strikethrough_lengths("~a~ ~~b~~ ~~c~\n", "<p><del>a</del> <del>b</del> ~~c~</p>\n")]
fn emphasis_matches_as_before(#[case] input: &str, #[case] expected: &str) {
    assert_eq!(mq_markdown::to_html(input), expected);
}

#[rstest]
#[case::dashes("---\n", "<hr />\n")]
#[case::spaced_dashes("- - -\n", "<hr />\n")]
#[case::spaced_stars("*  *  *\n", "<hr />\n")]
#[case::underscores("_ _ _\n", "<hr />\n")]
#[case::trailing_whitespace("-- - \t \n", "<hr />\n")]
#[case::in_a_quote("> - - -\n", "<blockquote>\n<hr />\n</blockquote>\n")]
#[case::in_a_list_item("- ***\n", "<ul>\n<li>\n<hr />\n</li>\n</ul>\n")]
#[case::two_markers_are_not_enough("- -\n", "<ul>\n<li>\n<ul>\n<li></li>\n</ul>\n</li>\n</ul>\n")]
#[case::mixed_markers(
    "- * -\n",
    "<ul>\n<li>\n<ul>\n<li>\n<ul>\n<li></li>\n</ul>\n</li>\n</ul>\n</li>\n</ul>\n"
)]
#[case::text_after_the_markers(
    "- - - a\n",
    "<ul>\n<li>\n<ul>\n<li>\n<ul>\n<li>a</li>\n</ul>\n</li>\n</ul>\n</li>\n</ul>\n"
)]
#[case::text_before_the_end(
    "- - - a -\n",
    "<ul>\n<li>\n<ul>\n<li>\n<ul>\n<li>a -</li>\n</ul>\n</li>\n</ul>\n</li>\n</ul>\n"
)]
#[case::rule_after_text_in_an_item("- a\n  ---\n", "<ul>\n<li>\n<h2>a</h2>\n</li>\n</ul>\n")]
fn thematic_breaks_are_told_from_other_lines(#[case] input: &str, #[case] expected: &str) {
    assert_eq!(mq_markdown::to_html(input), expected);
}

/// A run of emphasis nodes that follow each other is written with alternating delimiters, which must
/// not rewrite every node before it: that took seconds for a few thousand nodes.
#[test]
fn adjacent_emphasis_renders_in_linear_time() {
    let markdown = Markdown::from_markdown_str(&"*_".repeat(20_000)).unwrap();
    let start = std::time::Instant::now();
    let _ = markdown.to_string();
    let _ = markdown.to_html();
    assert!(
        start.elapsed() < std::time::Duration::from_secs(5),
        "{:?}",
        start.elapsed()
    );
}

#[test]
fn adjacent_emphasis_alternates_its_delimiters() {
    use mq_markdown::{Emphasis, Heading, HeadingDepth, Node};

    let emphasis = |text: &str| {
        Node::Emphasis(Emphasis {
            values: vec![text.to_string().into()],
            position: None,
        })
    };
    let nodes = vec![emphasis("a"), emphasis("b"), emphasis("c")];
    assert_eq!(Markdown::new(nodes.clone()).to_string().trim_end(), "*a*\n_b_\n*c*");

    let heading = Node::Heading(Heading {
        depth: HeadingDepth::H1,
        values: nodes,
        position: None,
    });
    assert_eq!(Markdown::new(vec![heading]).to_string().trim_end(), "# *a*_b_*c*");
}
