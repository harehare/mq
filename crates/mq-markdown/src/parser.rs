//! Markdown parser that builds [`Node`] values directly.
//!
//! Parsing runs in phases: [`block`] resolves the block structure line by line into a tree,
//! [`resolve`] collects the definitions and turns the tree into nodes, and [`inline`] parses the raw
//! text of paragraphs, headings and table cells along the way.
//!
//! It reads `CommonMark`, GFM, frontmatter, math and MDX (without a JavaScript parser, so expressions
//! only need balanced braces outside of strings and comments). HTML is rendered by [`render_html`].
mod block;
#[cfg(feature = "callout")]
mod callout;
mod code;
mod definition;
pub(crate) mod error;
mod flavor;
mod html_flow;
mod inline;
mod line;
mod mdx;
mod mdx_flow;
mod render_html;
mod resolve;
mod scan;
mod table;
mod tree;

use crate::node::Node;
use flavor::Flavor;

pub use error::{MdxDiagnostic, MdxError, MdxErrorKind, MdxFound, MdxPlace};
pub(crate) use inline::{is_autolink_email, normalize, unescape};

/// Parses `content` into a flat list of nodes.
#[cfg(test)]
pub(crate) fn parse(content: &str) -> miette::Result<Vec<Node>> {
    parse_with(content, true)
}

/// Parses `content` into a flat list of nodes, with frontmatter at the start when `frontmatter` is set.
pub(crate) fn parse_with(content: &str, frontmatter: bool) -> miette::Result<Vec<Node>> {
    resolve::resolve(block::parse(content, Flavor::Markdown, frontmatter), Flavor::Markdown)
        .map_err(|error| miette::Report::new(error.into_diagnostic(content)))
}

/// Renders `content` as HTML.
pub(crate) fn to_html(content: &str) -> String {
    render_html::render(content)
}

/// Parses `content` as MDX: no indented code, HTML, autolinks or GFM, but expressions and JSX.
#[cfg(test)]
pub(crate) fn parse_mdx(content: &str) -> miette::Result<Vec<Node>> {
    parse_mdx_with(content, true)
}

/// Parses `content` as MDX, with frontmatter at the start when `frontmatter` is set.
pub(crate) fn parse_mdx_with(content: &str, frontmatter: bool) -> miette::Result<Vec<Node>> {
    resolve::resolve(block::parse(content, Flavor::Mdx, frontmatter), Flavor::Mdx)
        .map_err(|error| miette::Report::new(error.into_diagnostic(content)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use proptest::test_runner::{RngAlgorithm, TestRng, TestRunner};

    fn env_number<T: std::str::FromStr>(name: &str) -> Option<T> {
        std::env::var(name).ok()?.parse().ok()
    }

    /// Lines built from container prefixes and block-level bodies only, so inline syntax never
    /// appears. Constructs whose mdast positions depend on quirks of the following lines are left to the cases of `tests/parser_tree_tests.rs`: empty items and quotes,
    /// unclosed fences, indented code next to containers, ordered lists that do not start at 1, and
    /// lazy continuation inside nested containers, more than one container marker on a line, and runs
    /// of setext underlines (`===` is only used by those cases).
    fn block_input() -> impl Strategy<Value = String> {
        // At most one container marker per line; deeper nesting comes from indentation.
        let indent = prop::sample::select(vec!["", " ", "  ", "   "]);
        let marker = prop::sample::select(vec!["", "> ", ">", "- ", "* ", "+ ", "1. ", "1) "]);
        let body = prop::sample::select(vec![
            "a",
            "b",
            "# a",
            "#",
            "##  b ##",
            "---",
            "***",
            "| a | b |",
            "|-|:-:|",
            "|1|2|3|",
            "a|b",
            "[a]: /u",
            "[a]: /u 'T'",
            "[^a]: x",
            "<!-- c -->",
            "x [a] [b][a] [^a]",
            "*a* `b`",
            "a  ",
            "a\\",
            "+++",
        ]);
        let block = (indent, marker, body).prop_map(|(indent, marker, body)| format!("{indent}{marker}{body}"));
        let blank = prop::sample::select(vec!["", "  "]).prop_map(str::to_string);
        // Fences are always closed here.
        let fence = (
            prop::sample::select(vec!["", " ", "  ", "   "]),
            prop::sample::select(vec![
                ("```", "```"),
                ("```rust x", "````"),
                ("~~~", "~~~"),
                ("$$", "$$"),
                ("$$ m", "$$$"),
            ]),
            prop::sample::select(vec!["a", "", "> b", "- c"]),
        )
            .prop_map(|(indent, (open, close), content)| format!("{indent}{open}\n{indent}{content}\n{indent}{close}"));
        let line = prop_oneof![8 => block, 2 => blank, 1 => fence];
        (prop::collection::vec(line, 0..8), any::<bool>()).prop_map(|(lines, trailing_eol)| {
            let mut input = lines.join("\n");
            if trailing_eol {
                input.push('\n');
            }
            input
        })
    }

    /// Lines of inline syntax. Each line starts with `x` so that no line is a block on its own.
    fn inline_input() -> impl Strategy<Value = String> {
        let token = prop::sample::select(vec![
            "a",
            "b",
            "foo",
            " ",
            "  ",
            "*",
            "**",
            "***",
            "_",
            "__",
            "~",
            "~~",
            "`",
            "``",
            "[",
            "]",
            "(",
            ")",
            "![",
            "<",
            ">",
            "\\",
            "&",
            "#",
            "$",
            "$$",
            "http://a.b",
            "www.a.b",
            "a@b.co",
            "\"",
            "'",
            ".",
            ",",
            "!",
            "-",
            "/",
            "é",
            "あ",
            "\n",
            "](",
            "](x)",
            "][",
            "]:",
            "<a>",
            "</a>",
            "<x@y.z>",
            "<http://a>",
        ]);
        prop::collection::vec(token, 1..14).prop_map(|tokens| {
            let mut input = String::from("x");
            for token in tokens {
                input.push_str(token);
                if token == "\n" {
                    input.push('x');
                }
            }
            input
        })
    }

    /// Lines of MDX: tags, expressions and the Markdown around them.
    fn mdx_input() -> impl Strategy<Value = String> {
        let token = prop::sample::select(vec![
            "<",
            ">",
            "/",
            "{",
            "}",
            "a",
            "b",
            "c",
            " ",
            "  ",
            "=",
            "\"",
            "'",
            ".",
            ":",
            "-",
            "*",
            "_",
            "`",
            "[",
            "]",
            "(",
            ")",
            "&amp;",
            "<a>",
            "</a>",
            "<b/>",
            "<c d=\"e\">",
            "<>",
            "</>",
            "{x}",
            "{x {y}}",
            "# ",
            "- ",
            "> ",
            "\\",
            "!",
            "|",
            "\n",
            "\n\n",
            "\n  ",
            "<a.b>",
            "</a.b>",
            "<a:b/>",
            "{...c}",
            "é",
            "1. ",
        ]);
        prop::collection::vec(token, 1..16).prop_map(|tokens| tokens.concat())
    }

    /// Markdown syntax mixed with multibyte text: CJK, emoji, combining marks, `ß`, full-width and
    /// ideographic punctuation and spaces, and characters that change case or width.
    fn unicode_input() -> impl Strategy<Value = String> {
        let word = prop::sample::select(vec![
            "あ",
            "日本語",
            "テスト",
            "🎉",
            "👨‍👩‍👧",
            "é",
            "e\u{301}",
            "ß",
            "ǅ",
            "İ",
            "ﬁ",
            "ｗｗｗ",
            "www.あ.com",
            "wwß",
            "ww\u{301}",
            "http://あ.jp/パス",
            "https://例え.jp",
            "a@あ.com",
            "あ@b.com",
            "。",
            "、",
            "「",
            "」",
            "（",
            "）",
            "！",
            "？",
            "＊",
            "＿",
            "｀",
            "＃",
            "\u{3000}",
            "\u{a0}",
            "\u{200b}",
            "\u{2028}",
            "\u{feff}",
            "\u{fe0f}",
            "ا",
            "א",
            "𠮷",
            "\u{10ffff}",
            "\u{0}",
            "x",
            "foo",
        ]);
        let syntax = prop::sample::select(vec![
            " ",
            "  ",
            "\t",
            "*",
            "**",
            "_",
            "__",
            "~",
            "~~",
            "`",
            "``",
            "[",
            "]",
            "(",
            ")",
            "![",
            "<",
            ">",
            "&",
            "&amp;",
            "&#12354;",
            "&#x1F389;",
            "&あ;",
            "\\",
            "|",
            ":",
            "-",
            "#",
            "^",
            "\"",
            "'",
            "{",
            "}",
            "/",
            "=",
            "://",
            ".",
            "\n",
            "\n\n",
            "\n> ",
            "\n- ",
            "\n1. ",
            "\n# ",
            "\n| ",
            "\n|-|-|\n",
            "\n    ",
            "\n\t",
            "\r\n",
            "\n[a]: ",
            "\n[^a]: ",
        ]);
        let token = prop_oneof![3 => word, 2 => syntax];
        prop::collection::vec(token, 1..24).prop_map(|tokens| tokens.concat())
    }

    /// Any characters at all, with extra weight on the ones that matter to Markdown.
    fn arbitrary_input() -> impl Strategy<Value = String> {
        let ch = prop_oneof![
            4 => any::<char>(),
            3 => prop::sample::select(vec!['*', '_', '`', '[', ']', '(', ')', '<', '>', '&', '|', '\\', '#', '-', '~', ' ', '\t', '\n', '!', 'w', 'h', '@', ':', '/', '.', '{', '}']),
            3 => prop::sample::select(vec!['あ', 'é', 'ß', '🎉', '\u{301}', '\u{3000}', '\u{a0}', '。', '＊']),
        ];
        prop::collection::vec(ch, 0..40).prop_map(|chars| chars.into_iter().collect())
    }

    /// Closed fences whose info string and content are multibyte, alone or inside a container.
    fn unicode_fence_input() -> impl Strategy<Value = String> {
        let text = prop::sample::select(vec![
            "あ",
            "日本語 x",
            "🎉",
            "é",
            "ß",
            "ｗｗｗ",
            "a b",
            "",
            "> あ",
            "- 🎉",
            "\u{3000}",
        ]);
        (
            prop::sample::select(vec!["", "> ", "- ", "  ", "1. "]),
            prop::sample::select(vec![("```", "```"), ("~~~", "~~~"), ("````", "````"), ("$$", "$$")]),
            text.clone(),
            text.clone(),
            text,
        )
            .prop_map(|(container, (open, close), info, first, second)| {
                let pad = if container == "> " {
                    "> ".to_string()
                } else {
                    " ".repeat(container.len())
                };
                let lead = |line: &str, first: bool| {
                    if first {
                        format!("{container}{line}")
                    } else {
                        format!("{pad}{line}")
                    }
                };
                [
                    lead(&format!("{open}{info}"), true),
                    lead(first, false),
                    lead(second, false),
                    lead(close, false),
                ]
                .join("\n")
            })
    }

    /// Checks what has to hold for any input: nothing panics, and every position lies inside the
    /// document and does not end before it starts.
    fn assert_valid(input: &str) {
        let lines = 1 + input.matches(['\n', '\r']).count() - input.matches("\r\n").count();
        let mut stack = parse(input).expect("Markdown always parses");
        stack.extend(parse_mdx(input).unwrap_or_default());
        while let Some(node) = stack.pop() {
            if let Some(position) = node.position() {
                let (start, end) = (&position.start, &position.end);
                let ordered = (start.line, start.column) <= (end.line, end.column);
                assert!(
                    ordered && start.line >= 1 && start.column >= 1 && end.line <= lines,
                    "{node:?} has a position outside of {input:?}"
                );
            }
            stack.extend(node.children());
        }
        let _ = to_html(input);
    }

    /// Runs `strategy` with a fixed seed so the outcome never varies between runs. `PROPTEST_CASES`
    /// and `PARSER_TEST_SEED` widen the exploration.
    fn check(strategy: impl Strategy<Value = String>) {
        let config = ProptestConfig {
            cases: env_number("PROPTEST_CASES").unwrap_or(3000),
            failure_persistence: None,
            ..ProptestConfig::default()
        };
        let seed = env_number("PARSER_TEST_SEED").unwrap_or(7);
        let mut runner = TestRunner::new_with_rng(config, TestRng::from_seed(RngAlgorithm::ChaCha, &[seed; 32]));
        let result = runner.run(&strategy, |input| {
            assert_valid(&input);
            Ok(())
        });
        if let Err(error) = result {
            panic!("{error}");
        }
    }

    #[test]
    fn block_structure_is_valid() {
        check(block_input());
    }

    #[test]
    fn inline_syntax_is_valid() {
        check(inline_input());
    }

    #[test]
    fn mdx_syntax_is_valid() {
        check(mdx_input());
    }

    #[test]
    fn multibyte_syntax_is_valid() {
        check(unicode_input());
    }

    #[test]
    fn multibyte_fences_are_valid() {
        check(unicode_fence_input());
    }

    #[test]
    fn arbitrary_text_is_valid() {
        check(arbitrary_input());
    }

    /// Every control character and unusual space or separator at each syntax position.
    #[test]
    fn special_characters_are_valid() {
        let chars = (0u32..=0x9f)
            .chain([
                0xa0, 0xad, 0x1680, 0x180e, 0x2000, 0x200a, 0x200b, 0x200d, 0x2028, 0x2029, 0x202f, 0x205f, 0x2060,
                0x3000, 0xfeff, 0xfffd, 0xfe0f,
            ])
            .filter_map(char::from_u32);
        let templates = [
            "[a]: {}",
            "[a]: x{}y",
            "[a]: <{}>",
            "[a]: /u '{}'",
            "[a]: /u {}",
            "[a]\n\n[a]: {}x\n\n[a]",
            "[a]({})",
            "[a](x{}y)",
            "[a](<{}>)",
            "[a](/u '{}')",
            "[a](/u \"{}\")",
            "![{}](x)",
            "[{}](x)",
            "[x{}]: y",
            "<http://a{}b>",
            "<a@b{}c>",
            "<{}a>",
            "<a{}b>",
            "<a {}b=c>",
            "<a b={}c>",
            "<a b=\"{}\">",
            "http://a{}b.c",
            "www.a{}b.c",
            "www.a.b/{}x",
            "x@y{}z.com",
            "{}x@y.com",
            "a{}b",
            "{}a",
            "a{}",
            "a {}",
            "{} a",
            "# {}",
            "# a{}",
            "#{}a",
            "{}# a",
            "> {}",
            "- {}",
            "-{}a",
            "1.{}a",
            "1. {}",
            "```{}",
            "```a{}\nx\n```",
            "```\n{}\n```",
            "~~~{}",
            "$${}",
            "$$\n{}\n$$",
            "`{}`",
            "`a{}b`",
            "*{}*",
            "*a{}*",
            "{}*a*",
            "*a*{}",
            "**{}**",
            "_a{}_",
            "_{}a_",
            "~~a{}~~",
            "~~{}~~",
            "a\\{}",
            "&{};",
            "&a{};",
            "&#{};",
            "&#x{};",
            "&amp{}",
            "{}\n---",
            "a{}\n---",
            "a\n{}---",
            "---{}",
            "***{}",
            "- - {}",
            "| a{} | b |\n|-|-|",
            "| {} |\n|-|\n| {} |",
            "|{}|\n|{}|",
            "[^a]: {}",
            "[^a{}]: x",
            "[^a]\n\n[^a]: {}x",
            "x\n\n    {}",
            "x\n\n    a{}",
            "<!-- {} -->",
            "<div>{}",
            "<div {}>",
            "<?{}?>",
            "<![CDATA[{}]]>",
            "---\n{}\n---\na",
            "+++\n{}\n+++\na",
            "{}  \nb",
            "a{}\nb",
            "a\n{}b",
        ];
        for char in chars {
            for template in templates {
                assert_valid(&template.replace("{}", &char.to_string()));
            }
        }
    }

    const SPEC_URL: &str =
        "https://raw.githubusercontent.com/github/cmark-gfm/828322d1ee4facdab56f0d3edccb13e9af90dcd2/test/spec.txt";

    const EXTENSIONS_SPEC_URL: &str = "https://raw.githubusercontent.com/github/cmark-gfm/828322d1ee4facdab56f0d3edccb13e9af90dcd2/test/extensions.txt";

    const COMMONMARK_SPEC_URL: &str = "https://raw.githubusercontent.com/commonmark/commonmark-spec/0.31.2/spec.txt";

    /// The markdown and the HTML of each example of the `CommonMark` and GFM `spec.txt`, with its number.
    fn spec_examples(text: &str) -> Vec<(usize, String, String)> {
        let fence = "`".repeat(32);
        let open = format!("{fence} example");
        let mut examples = Vec::new();
        let mut lines = text.lines();

        while let Some(line) = lines.next() {
            if line != open {
                continue;
            }
            let markdown = lines.by_ref().take_while(|line| *line != ".").collect::<Vec<_>>();
            let html = lines.by_ref().take_while(|line| *line != fence).collect::<Vec<_>>();
            // `→` stands for a tab in the spec.
            let text = |lines: Vec<&str>| lines.join("\n").replace('\u{2192}', "\t");
            let html = text(html);
            let html = if html.is_empty() { html } else { html + "\n" };
            examples.push((examples.len() + 1, text(markdown) + "\n", html));
        }
        examples
    }

    /// Examples of the GFM spec, which is based on `CommonMark` 0.29, that are not rendered as in the
    /// spec, by number:
    ///
    /// - a document that starts with `---` has frontmatter (66, 68)
    /// - nested `strong` of runs of `*` and `_`, which `CommonMark` 0.30 and later render as nested
    ///   elements where 0.29 merged them (388, 416, 424, 425, 426, 463, 464, 465, 467)
    /// - links with a protocol other than http, https, irc, ircs, mailto and xmpp have no `href`, as
    ///   as mdast to HTML does by default (496, 594, 595, 597)
    /// - GFM autolink literals, which the spec does not have (598, 604, 607, 608)
    /// - the GFM tag filter, which writes the tags `script`, `style` and `textarea` as text (140, 141,
    ///   142, 145, 147)
    const KNOWN_DIFFERENCES: &[usize] = &[
        66, 68, 140, 141, 142, 145, 147, 388, 416, 424, 425, 426, 463, 464, 465, 467, 496, 594, 595, 597, 598, 604,
        607, 608,
    ];

    /// Examples of the GFM spec with `[[...]]` in them that the `wikilink` feature reads as a wikilink
    /// (544, 555), and the one that either the `wikilink` or the `embed` feature changes (586).
    const GFM_WIKILINK_DIFFERENCES: &[usize] = &[544, 555];
    const GFM_OBSIDIAN_DIFFERENCES: &[usize] = &[586];

    /// The same for the `CommonMark` 0.31.2 spec (548, 559 and 590).
    const COMMONMARK_WIKILINK_DIFFERENCES: &[usize] = &[548, 559];
    const COMMONMARK_OBSIDIAN_DIFFERENCES: &[usize] = &[590];

    /// The examples that differ from the spec: `known`, and those that the enabled features read as
    /// wikilinks and embeds.
    fn expected_differences(known: &[usize], wikilink: &[usize], either: &[usize]) -> Vec<usize> {
        let mut all = known.to_vec();
        if cfg!(feature = "wikilink") {
            all.extend_from_slice(wikilink);
        }
        if cfg!(any(feature = "wikilink", feature = "embed")) {
            all.extend_from_slice(either);
        }
        all.sort_unstable();
        all
    }

    /// Examples of the `CommonMark` 0.31.2 spec that are not rendered as in the spec, by number:
    ///
    /// - a document that starts with `---` has frontmatter (96, 98)
    /// - links with a protocol other than http, https, irc, ircs, mailto and xmpp have no `href` (500,
    ///   598, 599, 601)
    /// - GFM autolink literals (602, 608, 611, 612)
    /// - the GFM tag filter, which writes the tags `script`, `style` and `textarea` as text (170, 171, 172,
    ///   173, 176, 178)
    const COMMONMARK_KNOWN_DIFFERENCES: &[usize] = &[
        96, 98, 170, 171, 172, 173, 176, 178, 500, 598, 599, 601, 602, 608, 611, 612,
    ];

    /// Examples of the GFM extensions spec that are not rendered as in the spec, by number:
    ///
    /// - input that must only not crash, whose output the spec leaves out (20)
    /// - the HTML of footnotes, which is the one of the current micromark and GitHub (23, 24, 25)
    const EXTENSIONS_KNOWN_DIFFERENCES: &[usize] = &[20, 23, 24, 25];

    /// Fetches `url`, or reads the file in the environment variable `var`.
    fn spec_text(var: &str, url: &str) -> String {
        match std::env::var(var) {
            Ok(path) => std::fs::read_to_string(path).unwrap(),
            Err(_) => ureq::get(url).call().unwrap().into_body().read_to_string().unwrap(),
        }
    }

    /// The numbers of the examples of `text` that are not rendered as in the spec.
    fn differing_examples(text: &str, minimum: usize) -> Vec<usize> {
        let examples = spec_examples(text);
        assert!(examples.len() > minimum, "only {} examples were found", examples.len());
        examples
            .iter()
            .filter(|(_, markdown, html)| to_html(markdown) != *html)
            .map(|(number, ..)| *number)
            .collect()
    }

    /// Renders every example of the spec to HTML, from `SPEC_FILE` or fetched over the network.
    #[test]
    #[ignore = "fetches spec.txt over the network; set SPEC_FILE to use a local copy"]
    fn spec_examples_render_as_in_the_spec() {
        assert_eq!(
            differing_examples(&spec_text("SPEC_FILE", SPEC_URL), 600),
            expected_differences(KNOWN_DIFFERENCES, GFM_WIKILINK_DIFFERENCES, GFM_OBSIDIAN_DIFFERENCES),
            "examples that are not rendered as in the spec"
        );
    }

    /// Renders every example of the `CommonMark` 0.31.2 spec to HTML, from `COMMONMARK_SPEC_FILE` or
    /// fetched over the network.
    #[test]
    #[ignore = "fetches spec.txt over the network; set COMMONMARK_SPEC_FILE to use a local copy"]
    fn commonmark_spec_examples_render_as_in_the_spec() {
        assert_eq!(
            differing_examples(&spec_text("COMMONMARK_SPEC_FILE", COMMONMARK_SPEC_URL), 600),
            expected_differences(
                COMMONMARK_KNOWN_DIFFERENCES,
                COMMONMARK_WIKILINK_DIFFERENCES,
                COMMONMARK_OBSIDIAN_DIFFERENCES
            ),
            "examples that are not rendered as in the spec"
        );
    }

    /// Renders every example of the GFM extensions spec to HTML, from `EXTENSIONS_SPEC_FILE` or fetched
    /// over the network.
    #[test]
    #[ignore = "fetches extensions.txt over the network; set EXTENSIONS_SPEC_FILE to use a local copy"]
    fn extensions_spec_examples_render_as_in_the_spec() {
        assert_eq!(
            differing_examples(&spec_text("EXTENSIONS_SPEC_FILE", EXTENSIONS_SPEC_URL), 20),
            EXTENSIONS_KNOWN_DIFFERENCES,
            "examples that are not rendered as in the spec"
        );
    }
}
