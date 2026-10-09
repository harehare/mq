//! Wikilinks, embeds and callouts, the Obsidian syntax: what each construct becomes, and that it
//! survives being written out and read back.
#![cfg(feature = "obsidian")]

use mq_markdown::{Markdown, Node};
use proptest::prelude::*;
use rstest::rstest;

fn describe(node: &Node) -> String {
    let own = match node {
        Node::WikiLink(w) => format!("wikilink({}|{})", w.target, w.text.as_deref().unwrap_or("")),
        Node::Embed(e) => format!("embed({}|{})", e.target, e.display.as_deref().unwrap_or("")),
        Node::Callout(c) => format!(
            "callout({}|{}|{})",
            c.kind,
            c.fold.map(String::from).unwrap_or_default(),
            c.title.as_deref().unwrap_or("")
        ),
        Node::Text(t) => format!("text({})", t.value.replace('\n', "\\n")),
        Node::Blockquote(_) => "quote".to_string(),
        Node::CodeInline(c) => format!("code({})", c.value),
        Node::Code(_) => "codeblock".to_string(),
        Node::Emphasis(_) => "em".to_string(),
        Node::Strong(_) => "strong".to_string(),
        Node::Link(_) => "link".to_string(),
        Node::List(_) => "item".to_string(),
        Node::Heading(_) => "heading".to_string(),
        other => format!("{other:?}")
            .split(['(', ' '])
            .next()
            .unwrap_or_default()
            .to_string(),
    };
    let children = node.children().iter().map(describe).collect::<Vec<_>>().join(", ");
    if children.is_empty() {
        own
    } else {
        format!("{own}[{children}]")
    }
}

fn parse(input: &str) -> String {
    Markdown::from_markdown_str(input)
        .unwrap()
        .nodes
        .iter()
        .map(describe)
        .collect::<Vec<_>>()
        .join(" ")
}

#[rstest]
#[case::plain("[[Page]]", "wikilink(Page|)")]
#[case::alias("[[Page|Alias]]", "wikilink(Page|Alias)")]
#[case::heading("[[Page#Heading]]", "wikilink(Page#Heading|)")]
#[case::block("[[Page#^blk]]", "wikilink(Page#^blk|)")]
#[case::same_note_heading("[[#Heading]]", "wikilink(#Heading|)")]
#[case::nested_headings("[[Page#H#Sub]]", "wikilink(Page#H#Sub|)")]
#[case::more_than_one_pipe("[[a|b|c]]", "wikilink(a|b|c)")]
#[case::spaces_are_trimmed("[[ a | b ]]", "wikilink(a|b)")]
#[case::unicode("[[日本語|別名]]", "wikilink(日本語|別名)")]
#[case::around_text("x [[a]] y", "text(x ) wikilink(a|) text( y)")]
#[case::next_to_each_other("[[a]][[b]]", "wikilink(a|) wikilink(b|)")]
#[case::underscores_around("[[_a]] and [[b_]]", "wikilink(_a|) text( and ) wikilink(b_|)")]
#[case::asterisks_inside("[[a*b]] [[c*d]]", "wikilink(a*b|) text( ) wikilink(c*d|)")]
#[case::inside_emphasis("*[[a]]*", "em[wikilink(a|)]")]
#[case::inside_strong("**[[a]]**", "strong[wikilink(a|)]")]
#[case::in_a_list("- [[a]]", "item[wikilink(a|)]")]
#[case::in_a_heading("# [[a]]", "heading[wikilink(a|)]")]
#[case::empty_target_is_text("[[]]", "text([[]])")]
#[case::blank_target_is_text("[[ ]]", "text([[ ]])")]
#[case::unclosed_is_text("[[a]", "text([[a])")]
#[case::line_ending_is_text("[[a\nb]]", "text([[a\\nb]])")]
#[case::escaped_is_text("\\[[a]]", "text([[a]])")]
#[case::in_code_span("`[[a]]` [[b]]", "code([[a]]) text( ) wikilink(b|)")]
#[case::in_code_block("```\n[[a]]\n```", "codeblock")]
#[case::triple_bracket("[[[a]]]", "text([) wikilink(a|) text(])")]
fn wikilinks(#[case] input: &str, #[case] expected: &str) {
    assert_eq!(parse(input), expected);
}

#[rstest]
#[case::plain("![[Page]]", "embed(Page|)")]
#[case::image_size("![[img.png|100]]", "embed(img.png|100)")]
#[case::image_width_height("![[img.png|100x145]]", "embed(img.png|100x145)")]
#[case::alt_and_size("![[img.png|alt|200]]", "embed(img.png|alt|200)")]
#[case::heading("![[Page#Heading]]", "embed(Page#Heading|)")]
#[case::pdf_page("![[a.pdf#page=3]]", "embed(a.pdf#page=3|)")]
#[case::after_text("see ![[a.png]]", "text(see ) embed(a.png|)")]
#[case::empty_is_text("![[]]", "text(![[]])")]
#[case::with_wikilink("![[a.png]] [[b]]", "embed(a.png|) text( ) wikilink(b|)")]
fn embeds(#[case] input: &str, #[case] expected: &str) {
    assert_eq!(parse(input), expected);
}

#[rstest]
#[case::bare("> [!note]", "callout(note||)")]
#[case::title("> [!note] Title\n> body", "callout(note||Title)[text(body)]")]
#[case::upper_case("> [!NOTE]\n> a\n> b", "callout(NOTE||)[text(a\\nb)]")]
#[case::folded("> [!faq]- Folded\n> body", "callout(faq|-|Folded)[text(body)]")]
#[case::open("> [!tip]+ Open\n> body", "callout(tip|+|Open)[text(body)]")]
#[case::fold_without_title("> [!faq]-\n> body", "callout(faq|-|)[text(body)]")]
#[case::dash_title_after_a_space("> [!faq] - Title", "callout(faq||- Title)")]
#[case::custom_type("> [!my-custom_1] T", "callout(my-custom_1||T)")]
#[case::no_space_after_the_quote(">[!note] x", "callout(note||x)")]
#[case::title_with_emphasis("> [!note] *Title* more\n> b", "callout(note||*Title* more)[text(b)]")]
#[case::title_markup_then_paragraph("> [!note] *Title*\n>\n> Body", "callout(note||*Title*)[text(Body)]")]
#[case::title_markup_then_markup_paragraph(
    "> [!note] *Title*\n>\n> *Body* x",
    "callout(note||*Title*)[em[text(Body)], text( x)]"
)]
#[case::title_with_code("> [!note] a `c` b", "callout(note||a `c` b)")]
#[case::list_body("> [!note]\n> - a", "callout(note||)[item[text(a)]]")]
#[case::code_body("> [!note]\n> ```\n> x\n> ```", "callout(note||)[codeblock]")]
#[case::nested("> [!note]\n> > [!tip]\n> > inner", "callout(note||)[callout(tip||)[text(inner)]]")]
#[case::in_a_list("- > [!note]\n  > body", "item[callout(note||)[text(body)]]")]
#[case::empty_type_is_a_quote("> [!] x", "quote[text([!] x)]")]
#[case::space_in_type_is_a_quote("> [! note] x", "quote[text([! note] x)]")]
#[case::space_after_type_is_a_quote("> [!note x] y", "quote[text([!note x] y)]")]
#[case::not_at_the_start("> text [!note]", "quote[text(text [!note])]")]
fn callouts(#[case] input: &str, #[case] expected: &str) {
    assert_eq!(parse(input), expected);
}

#[rstest]
#[case("[[a|b]] ![[c.png|100]] [[p#h]]\n")]
#[case("> [!faq]- Folded\n> body\n")]
#[case("> [!note] *Title*\n> body\n")]
#[case("- [[a_]] and [[_b]]\n")]
#[case("> [!note]\n> > [!tip] T\n> > inner [[x]]\n")]
fn writing_and_reading_back_keeps_the_nodes(#[case] input: &str) {
    let written = Markdown::from_markdown_str(input).unwrap().to_string();
    assert_eq!(parse(&written), parse(input), "{written:?}");
    assert_eq!(written, input);
}

#[rstest]
#[case::after_wikilink("[[note]]\n- - -\n")]
#[case::after_embed("![[img.png]]\n- - -\n")]
fn a_dash_rule_after_inline_nodes_stays_a_rule(#[case] input: &str) {
    let written = Markdown::from_markdown_str(input).unwrap().to_string();
    assert_eq!(parse(&written), parse(input), "{written:?}");
    assert!(!parse(&written).contains("heading"), "{written:?}");
}

/// Title lines, and the title they have to give: markup in a title spreads it over several nodes.
const TITLES: &[(&str, &str)] = &[
    ("Title", "Title"),
    ("Two words", "Two words"),
    ("*Title*", "*Title*"),
    ("**Title**", "**Title**"),
    ("`Title`", "`Title`"),
    ("~~Title~~", "~~Title~~"),
    ("[Title](u)", "[Title](u)"),
    ("![Title](u)", "![Title](u)"),
    ("[[Title]]", "[[Title]]"),
    ("![[Title]]", "![[Title]]"),
    ("*A* b", "*A* b"),
    ("a *B*", "a *B*"),
    ("*A* b *C*", "*A* b *C*"),
    ("*A* **B**", "*A* **B**"),
    ("**A** `b` [[c]]", "**A** `b` [[c]]"),
];

/// Bodies that mean the same alone as after a title, in any separator.
const BODIES: &[&str] = &[
    "Body",
    "Body words",
    "*Body*",
    "*Body* x",
    "x *Body*",
    "**Body**",
    "`Body`",
    "[Body](u)",
    "[[Body]]",
    "![[Body]]",
    "![Body](u)",
    "a\nb",
    "*a*\nb",
    "- a",
    "1. a",
    "# h",
    "```\nx\n```",
    "> q",
    "> [!tip] T\n> inner",
];

/// Bodies that are only right after a blank line, since after a title line they would run into it.
const PARAGRAPH_BODIES: &[&str] = &["Body\n\nSecond", "*Body*\n\n*Second*", "a\n\n- b", "---"];

/// Puts `prefix` before the first line and `rest` before the others.
fn indent(text: &str, first: &str, rest: &str) -> String {
    text.lines()
        .enumerate()
        .map(|(i, line)| {
            let prefix = if i == 0 { first } else { rest };
            if line.is_empty() && !prefix.trim().is_empty() && i > 0 {
                prefix.trim_end().to_string()
            } else {
                format!("{prefix}{line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// What `body` parses to as the content of a plain quote.
fn body_alone(body: &str) -> Vec<String> {
    let nodes = Markdown::from_markdown_str(&indent(body, "> ", "> ")).unwrap().nodes;
    match nodes.as_slice() {
        [Node::Blockquote(q)] => q.values.iter().map(describe).collect(),
        [Node::Callout(c)] => c.values.iter().map(describe).collect(),
        other => panic!("{body:?} is not a quote: {other:?}"),
    }
}

/// The callout at the top, or in the first list item.
fn find_callout(nodes: &[Node]) -> Option<&mq_markdown::Callout> {
    nodes.iter().find_map(|node| match node {
        Node::Callout(c) => Some(c),
        Node::List(l) => find_callout(&l.values),
        _ => None,
    })
}

/// Parses a callout made of a header and a body, as a top-level quote and inside a list item, and checks
/// that the title is the header line and the body is what the same text is in a plain quote.
fn check_callout(kind: &str, fold: &str, title_source: &str, separator: &str, body: &str) {
    let source = format!("> [!{kind}]{fold} {title_source}{separator}{}", indent(body, "", "> "));
    let title = title_source.trim();
    for source in [source.clone(), indent(&source, "- ", "  ")] {
        let nodes = Markdown::from_markdown_str(&source).unwrap().nodes;
        let callout = find_callout(&nodes).unwrap_or_else(|| panic!("no callout in {source:?}"));
        assert_eq!(
            callout.title.as_deref(),
            (!title.is_empty()).then_some(title),
            "title of {source:?}"
        );
        assert_eq!(callout.kind, kind, "type of {source:?}");
        assert_eq!(callout.fold, fold.chars().next(), "fold of {source:?}");
        let got: Vec<String> = callout.values.iter().map(describe).collect();
        assert_eq!(got, body_alone(body), "body of {source:?}");
    }
}

/// Whatever the title is, and however the body is separated from it, the title is the header line and
/// the body is what the same text is in a plain quote.
#[test]
fn the_title_ends_with_the_header_line() {
    let separators = [("blank", "\n>\n> "), ("direct", "\n> ")];
    let mut cases = 0;
    for kind in ["note", "NOTE", "my-custom_1"] {
        for fold in ["", "+", "-"] {
            for &(title_source, title) in TITLES {
                assert_eq!(title_source, title, "titles are written as they are expected");
                let bodies = BODIES
                    .iter()
                    .map(|b| (*b, true))
                    .chain(PARAGRAPH_BODIES.iter().map(|b| (*b, false)));
                for (body, runs_into_title) in bodies {
                    for (name, separator) in separators {
                        if name == "direct" && !runs_into_title {
                            continue;
                        }
                        check_callout(kind, fold, title_source, separator, body);
                        cases += 1;
                    }
                }
            }
        }
    }
    assert!(cases > 500, "only {cases} cases ran");
}

proptest! {
    /// The same for any type, any title made of characters that mean something in a title, and any number of
    /// blank lines between the header and the body.
    #[test]
    fn any_title_ends_with_the_header_line(
        kind in "[a-zA-Z0-9_-]{1,8}",
        fold in prop::sample::select(vec!["", "+", "-"]),
        title in "[a-zA-Z0-9 *_`~\\[\\]()!<>&#|.\\\\あ🎉-]{0,24}",
        body in prop::sample::select(BODIES.iter().chain(PARAGRAPH_BODIES).copied().collect::<Vec<_>>()),
        blank_lines in 1usize..4,
    ) {
        let separator = format!("\n{}> ", ">\n".repeat(blank_lines));
        check_callout(&kind, fold, &title, &separator, body);
    }

    /// Without a blank line, a body that is not a paragraph still starts on the line after the title.
    #[test]
    fn any_title_ends_at_the_line_ending(
        title in "[a-zA-Z0-9 *_`~\\[\\]()!<>&#|.\\\\あ🎉-]{0,24}",
        body in prop::sample::select(BODIES.to_vec()),
    ) {
        check_callout("note", "", &title, "\n> ", body);
    }
}

#[rstest]
#[case::wikilink(
    "[[foo]]\n",
    "<p><a class=\"internal-link\" href=\"foo\" data-href=\"foo\">foo</a></p>\n"
)]
#[case::wikilink_with_text(
    "a [[foo|bar]] b\n",
    "<p>a <a class=\"internal-link\" href=\"foo\" data-href=\"foo\">bar</a> b</p>\n"
)]
#[case::wikilink_with_spaces(
    "[[My Note]]\n",
    "<p><a class=\"internal-link\" href=\"My%20Note\" data-href=\"My Note\">My Note</a></p>\n"
)]
#[case::wikilink_with_heading(
    "[[note#Part one]]\n",
    "<p><a class=\"internal-link\" href=\"note#Part%20one\" data-href=\"note#Part one\">note &gt; Part one</a></p>\n"
)]
#[case::wikilink_to_a_heading_of_the_page(
    "[[#Part]]\n",
    "<p><a class=\"internal-link\" href=\"#Part\" data-href=\"#Part\">Part</a></p>\n"
)]
#[case::wikilink_is_escaped(
    "[[a&b|<c>]]\n",
    "<p><a class=\"internal-link\" href=\"a&amp;b\" data-href=\"a&amp;b\">&lt;c&gt;</a></p>\n"
)]
#[case::wikilink_with_a_script_protocol(
    "[[javascript:alert(1)]]\n",
    "<p><a class=\"internal-link\" href=\"\" data-href=\"javascript:alert(1)\">javascript:alert(1)</a></p>\n"
)]
#[case::wikilink_inside_a_link_stays_text("[a [[b]]](c)\n", "<p><a href=\"c\">a [[b]]</a></p>\n")]
#[case::wikilink_in_a_heading(
    "# [[a]]\n",
    "<h1><a class=\"internal-link\" href=\"a\" data-href=\"a\">a</a></h1>\n"
)]
#[case::image("![[img.png]]\n", "<p><img src=\"img.png\" alt=\"img.png\" /></p>\n")]
#[case::image_with_width(
    "![[img.png|100]]\n",
    "<p><img src=\"img.png\" alt=\"img.png\" width=\"100\" /></p>\n"
)]
#[case::image_with_size(
    "![[img.png|100x200]]\n",
    "<p><img src=\"img.png\" alt=\"img.png\" width=\"100\" height=\"200\" /></p>\n"
)]
#[case::image_with_alt("![[img.png|a photo]]\n", "<p><img src=\"img.png\" alt=\"a photo\" /></p>\n")]
#[case::image_extension_is_case_insensitive("![[IMG.PNG]]\n", "<p><img src=\"IMG.PNG\" alt=\"IMG.PNG\" /></p>\n")]
#[case::image_with_spaces("![[my img.png]]\n", "<p><img src=\"my%20img.png\" alt=\"my img.png\" /></p>\n")]
#[case::image_with_a_script_protocol("![[javascript:a.png]]\n", "<p><img src=\"\" alt=\"javascript:a.png\" /></p>\n")]
#[case::audio("![[a.mp3]]\n", "<p><audio controls src=\"a.mp3\"></audio></p>\n")]
#[case::video(
    "![[a.mp4|640x480]]\n",
    "<p><video controls src=\"a.mp4\" width=\"640\" height=\"480\"></video></p>\n"
)]
#[case::pdf("![[a.pdf]]\n", "<p><embed type=\"application/pdf\" src=\"a.pdf\" /></p>\n")]
#[case::note(
    "![[note]]\n",
    "<p><a class=\"internal-link embed\" href=\"note\" data-href=\"note\">note</a></p>\n"
)]
#[case::note_with_heading(
    "a ![[note#Part]] b\n",
    "<p>a <a class=\"internal-link embed\" href=\"note#Part\" data-href=\"note#Part\">note &gt; Part</a> b</p>\n"
)]
#[case::image_with_alt_and_width(
    "![[img.png|photo|200]]\n",
    "<p><img src=\"img.png\" alt=\"photo\" width=\"200\" /></p>\n"
)]
#[case::image_with_alt_and_size(
    "![[img.png|photo|200x100]]\n",
    "<p><img src=\"img.png\" alt=\"photo\" width=\"200\" height=\"100\" /></p>\n"
)]
#[case::image_with_alt_and_pipe_text("![[img.png|a|b]]\n", "<p><img src=\"img.png\" alt=\"a|b\" /></p>\n")]
#[case::note_with_label(
    "![[project-notes|Meeting notes]]\n",
    "<p><a class=\"internal-link embed\" href=\"project-notes\" data-href=\"project-notes\">Meeting notes</a></p>\n"
)]
fn html_renders_wikilinks_and_embeds(#[case] input: &str, #[case] expected: &str) {
    assert_eq!(mq_markdown::to_html(input), expected);
    assert_eq!(Markdown::from_markdown_str(input).unwrap().to_html(), expected);
}

#[rstest]
#[case::note(
    "> [!note]\n> Body\n",
    "<div class=\"callout\" data-callout=\"note\">\n<div class=\"callout-title\">Note</div>\n<div class=\"callout-content\">\n<p>Body</p>\n</div>\n</div>\n"
)]
#[case::upper_case_kind(
    "> [!WARNING]\n> Careful\n",
    "<div class=\"callout\" data-callout=\"warning\">\n<div class=\"callout-title\">Warning</div>\n<div class=\"callout-content\">\n<p>Careful</p>\n</div>\n</div>\n"
)]
#[case::title(
    "> [!tip] A <b> title\n> Body\n",
    "<div class=\"callout\" data-callout=\"tip\">\n<div class=\"callout-title\">A &lt;b&gt; title</div>\n<div class=\"callout-content\">\n<p>Body</p>\n</div>\n</div>\n"
)]
#[case::without_body(
    "> [!note] Only a title\n",
    "<div class=\"callout\" data-callout=\"note\">\n<div class=\"callout-title\">Only a title</div>\n</div>\n"
)]
#[case::open_fold(
    "> [!faq]+ Question\n> Answer\n",
    "<details class=\"callout\" data-callout=\"faq\" open>\n<summary class=\"callout-title\">Question</summary>\n<div class=\"callout-content\">\n<p>Answer</p>\n</div>\n</details>\n"
)]
#[case::closed_fold(
    "> [!faq]- Question\n> Answer\n",
    "<details class=\"callout\" data-callout=\"faq\">\n<summary class=\"callout-title\">Question</summary>\n<div class=\"callout-content\">\n<p>Answer</p>\n</div>\n</details>\n"
)]
#[case::blocks_in_the_body(
    "> [!note]\n> a\n>\n> - b\n",
    "<div class=\"callout\" data-callout=\"note\">\n<div class=\"callout-title\">Note</div>\n<div class=\"callout-content\">\n<p>a</p>\n<ul>\n<li>b</li>\n</ul>\n</div>\n</div>\n"
)]
#[case::nested(
    "> [!note]\n> > [!tip]\n> > Inner\n",
    "<div class=\"callout\" data-callout=\"note\">\n<div class=\"callout-title\">Note</div>\n<div class=\"callout-content\">\n<div class=\"callout\" data-callout=\"tip\">\n<div class=\"callout-title\">Tip</div>\n<div class=\"callout-content\">\n<p>Inner</p>\n</div>\n</div>\n</div>\n</div>\n"
)]
#[case::wikilink_in_the_body(
    "> [!note]\n> [[a]]\n",
    "<div class=\"callout\" data-callout=\"note\">\n<div class=\"callout-title\">Note</div>\n<div class=\"callout-content\">\n<p><a class=\"internal-link\" href=\"a\" data-href=\"a\">a</a></p>\n</div>\n</div>\n"
)]
#[case::an_ordinary_quote("> [note]\n> Body\n", "<blockquote>\n<p>[note]\nBody</p>\n</blockquote>\n")]
#[case::after_a_paragraph(
    "a\n\n> [!note]\n> b\n\nc\n",
    "<p>a</p>\n<div class=\"callout\" data-callout=\"note\">\n<div class=\"callout-title\">Note</div>\n<div class=\"callout-content\">\n<p>b</p>\n</div>\n</div>\n<p>c</p>\n"
)]
fn html_renders_callouts(#[case] input: &str, #[case] expected: &str) {
    assert_eq!(mq_markdown::to_html(input), expected);
    assert_eq!(Markdown::from_markdown_str(input).unwrap().to_html(), expected);
}

/// The HTML and the nodes are made from one reading of the source: what the callout node says is what
/// the HTML shows.
#[rstest]
#[case::plain("> [!NOTE]\n> body\n")]
#[case::title("> [!warning] Heads *up*\n> body\n")]
#[case::folded("> [!tip]- Fold me\n> body\n")]
#[case::open("> [!faq]+\n> body\n")]
#[case::kind_with_dash("> [!my-type_1] T\n")]
#[case::not_a_callout("> [! NOTE]\n> body\n")]
fn html_shows_what_the_callout_node_holds(#[case] input: &str) {
    let nodes = input.parse::<Markdown>().unwrap().nodes;
    let html = mq_markdown::to_html(input);
    let Some(callout) = find_callout(&nodes) else {
        assert!(!html.contains("callout"), "{html}");
        return;
    };
    assert!(
        html.contains(&format!("data-callout=\"{}\"", callout.kind.to_lowercase())),
        "{html}"
    );
    let tag = if callout.fold.is_some() {
        "<details"
    } else {
        "<div class=\"callout\""
    };
    assert!(html.starts_with(tag), "{html}");
    assert_eq!(html.contains(" open>"), callout.fold == Some('+'), "{html}");
    if let Some(title) = &callout.title {
        assert!(html.contains(&title.replace('<', "&lt;")), "{title} in {html}");
    }
}

#[rstest]
#[case::target("[[Page]]")]
#[case::text("[[Page|shown]]")]
#[case::heading("[[Page#Heading]]")]
#[case::embed_note("![[Note]]")]
#[case::embed_image("![[pic.png|alt|100]]")]
fn html_shows_what_the_link_node_holds(#[case] input: &str) {
    let nodes = input.parse::<Markdown>().unwrap().nodes;
    let html = mq_markdown::to_html(input);
    let target = nodes
        .iter()
        .find_map(|node| match node {
            Node::WikiLink(link) => Some(link.target.clone()),
            Node::Embed(embed) => Some(embed.target.clone()),
            _ => None,
        })
        .unwrap();
    assert!(html.contains(&target), "{target} in {html}");
}
