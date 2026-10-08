//! Wikilinks, embeds and callouts, the Obsidian syntax: what each construct becomes, and that it
//! survives being written out and read back.
#![cfg(feature = "obsidian")]

use mq_markdown::{Markdown, Node};
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
