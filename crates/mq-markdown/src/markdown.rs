use std::{fmt, str::FromStr};

use itertools::Itertools;
use markdown::Constructs;
use miette::miette;

use crate::node::{ListStyle, Node, Position};

#[derive(Debug, Clone)]
pub struct Markdown {
    pub nodes: Vec<Node>,
    pub options: RenderOptions,
}

impl FromStr for Markdown {
    type Err = miette::Error;

    fn from_str(content: &str) -> Result<Self, Self::Err> {
        Self::from_str(content)
    }
}

impl fmt::Display for Markdown {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut pre_position = None;
        let mut is_first = true;
        let text = self
            .nodes
            .iter()
            .filter_map(|node| {
                let value = node.to_string_with(&self.options.list_style);

                if value.is_empty() || value == "\n" {
                    pre_position = None;
                    return None;
                }

                let value = if let Some(pos) = node.position() {
                    let new_line_count = pre_position
                        .as_ref()
                        .map(|p: &Position| pos.start.line - p.end.line)
                        .unwrap_or_else(|| if is_first { 0 } else { 1 });

                    pre_position = Some(pos);
                    format!("{}{}", "\n".repeat(new_line_count), value)
                } else {
                    pre_position = None;
                    format!("{}{}", value, '\n')
                };

                is_first = false;
                Some(value)
            })
            .join("");

        write!(
            f,
            "{}",
            if text.ends_with('\n') {
                text
            } else {
                format!("{}\n", &text)
            }
        )
    }
}

#[derive(Debug, Clone, Default)]
pub struct RenderOptions {
    pub list_style: ListStyle,
}

impl Markdown {
    pub fn new(nodes: Vec<Node>) -> Self {
        Self {
            nodes,
            options: RenderOptions::default(),
        }
    }

    pub fn set_options(&mut self, options: RenderOptions) {
        self.options = options;
    }

    pub fn from_mdx_str(content: &str) -> miette::Result<Self> {
        let root = markdown::to_mdast(
            content,
            &markdown::ParseOptions {
                gfm_strikethrough_single_tilde: true,
                math_text_single_dollar: true,
                mdx_expression_parse: None,
                mdx_esm_parse: None,
                constructs: Constructs {
                    attention: true,
                    autolink: false,
                    block_quote: true,
                    character_escape: true,
                    character_reference: true,
                    code_indented: false,
                    code_fenced: true,
                    code_text: true,
                    definition: true,
                    frontmatter: true,
                    gfm_autolink_literal: true,
                    gfm_label_start_footnote: true,
                    gfm_footnote_definition: true,
                    gfm_strikethrough: true,
                    gfm_table: true,
                    gfm_task_list_item: true,
                    hard_break_escape: true,
                    hard_break_trailing: true,
                    heading_atx: true,
                    heading_setext: true,
                    html_flow: false,
                    html_text: false,
                    label_start_image: true,
                    label_start_link: true,
                    label_end: true,
                    list_item: true,
                    math_flow: true,
                    math_text: true,
                    mdx_esm: true,
                    mdx_expression_flow: true,
                    mdx_expression_text: true,
                    mdx_jsx_flow: true,
                    mdx_jsx_text: true,
                    thematic_break: true,
                },
            },
        )
        .map_err(|e| miette!(e.reason))?;
        let nodes = Node::from_mdast_node(root);

        Ok(Self {
            nodes,
            options: RenderOptions::default(),
        })
    }

    pub fn to_html(&self) -> String {
        markdown::to_html(self.to_string().as_str())
    }

    pub fn to_text(&self) -> String {
        self.nodes
            .iter()
            .map(|node| format!("{}\n", node.value()))
            .join("")
    }

    fn from_str(content: &str) -> miette::Result<Self> {
        let root = markdown::to_mdast(
            content,
            &markdown::ParseOptions {
                gfm_strikethrough_single_tilde: true,
                math_text_single_dollar: true,
                mdx_expression_parse: None,
                mdx_esm_parse: None,
                constructs: Constructs {
                    attention: true,
                    autolink: true,
                    block_quote: true,
                    character_escape: true,
                    character_reference: true,
                    code_indented: true,
                    code_fenced: true,
                    code_text: true,
                    definition: true,
                    frontmatter: true,
                    gfm_autolink_literal: true,
                    gfm_label_start_footnote: true,
                    gfm_footnote_definition: true,
                    gfm_strikethrough: true,
                    gfm_table: true,
                    gfm_task_list_item: true,
                    hard_break_escape: true,
                    hard_break_trailing: true,
                    heading_atx: true,
                    heading_setext: true,
                    html_flow: true,
                    html_text: true,
                    label_start_image: true,
                    label_start_link: true,
                    label_end: true,
                    list_item: true,
                    math_flow: true,
                    math_text: true,
                    mdx_esm: false,
                    mdx_expression_flow: false,
                    mdx_expression_text: false,
                    mdx_jsx_flow: false,
                    mdx_jsx_text: false,
                    thematic_break: true,
                },
            },
        )
        .map_err(|e| miette!(e.reason))?;
        let nodes = Node::from_mdast_node(root);

        Ok(Self {
            nodes,
            options: RenderOptions::default(),
        })
    }
}

pub fn to_html(s: &str) -> String {
    markdown::to_html(s)
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::header("# Title", 1, "# Title\n")]
    #[case::header("# Title\nParagraph", 2, "# Title\nParagraph\n")]
    #[case::header("# Title\n\nParagraph", 2, "# Title\n\nParagraph\n")]
    #[case::list("- Item 1\n- Item 2", 2, "- Item 1\n- Item 2\n")]
    #[case::quote("> Quote\n>Second line", 1, "> Quote\n> Second line\n")]
    #[case::code("```rust\nlet x = 1;\n```", 1, "```rust\nlet x = 1;\n```\n")]
    #[case::toml("+++\n[test]\ntest = 1\n+++", 1, "+++\n[test]\ntest = 1\n+++\n")]
    #[case::code_inline("`inline`", 1, "`inline`\n")]
    #[case::math_inline("$math$", 1, "$math$\n")]
    #[case::math("$$\nmath\n$$", 1, "$$\nmath\n$$\n")]
    #[case::html("<div>test</div>", 1, "<div>test</div>\n")]
    #[case::footnote("[^a]: b", 1, "[^a]: b\n")]
    #[case::definition("[a]: b", 1, "[a]: b\n")]
    #[case::footnote("[^a]: b", 1, "[^a]: b\n")]
    #[case::footnote_ref("[^a]: b\n\n[^a]", 2, "[^a]: b\n[^a]\n")]
    #[case::image("![a](b)", 1, "![a](b)\n")]
    #[case::image_with_title("![a](b \"c\")", 1, "![a](b \"c\")\n")]
    #[case::image_ref("[a]: b\n\n ![c][a]", 2, "[a]: b\n\n![c][a]\n")]
    #[case::yaml(
        "---\ntitle: Test\ndescription: YAML front matter\n---\n",
        1,
        "---\ntitle: Test\ndescription: YAML front matter\n---\n"
    )]
    #[case::link("[a](b)", 1, "[a](b)\n")]
    #[case::link_ref("[a]: b\n\n[c][a]", 2, "[a]: b\n\n[c][a]\n")]
    #[case::break_("a\\", 1, "a\\\n")]
    #[case::delete("~~a~~", 1, "~~a~~\n")]
    #[case::emphasis("*a*", 1, "*a*\n")]
    #[case::horizontal_rule("---", 1, "---\n")]
    #[case::table(
        "| Column1 | Column2 | Column3 |\n|:--------|:--------:|---------:|\n| Left    | Center  | Right   |\n",
        7,
        "|Column1|Column2|Column3|\n|:---|:---:|---:|\n|Left|Center|Right|\n"
    )]
    fn test_markdown_from_str(
        #[case] input: &str,
        #[case] expected_nodes: usize,
        #[case] expected_output: &str,
    ) {
        let md = input.parse::<Markdown>().unwrap();
        assert_eq!(md.nodes.len(), expected_nodes);
        assert_eq!(md.to_string(), expected_output);
    }

    #[rstest]
    #[case::mdx("{test}", 1, "{test}\n")]
    #[case::mdx("<a />", 1, "<a />\n")]
    #[case::mdx("<MyComponent {...props}/>", 1, "<MyComponent {...props} />\n")]
    #[case::mdx(
        "text<MyComponent {...props}/>text",
        3,
        "text<MyComponent {...props} />text\n"
    )]
    #[case::mdx(
        "<Chart color=\"#fcb32c\" year={year} />",
        1,
        "<Chart color=\"#fcb32c\" year={year} />\n"
    )]
    fn test_markdown_from_mdx_str(
        #[case] input: &str,
        #[case] expected_nodes: usize,
        #[case] expected_output: &str,
    ) {
        let md = Markdown::from_mdx_str(input).unwrap();
        assert_eq!(md.nodes.len(), expected_nodes);
        assert_eq!(md.to_string(), expected_output);
    }

    #[test]
    fn test_markdown_to_html() {
        let md = "# Hello".parse::<Markdown>().unwrap();
        let html = md.to_html();
        assert_eq!(html, "<h1>Hello</h1>\n");
    }

    #[test]
    fn test_markdown_to_text() {
        let md = "# Hello\n\nWorld".parse::<Markdown>().unwrap();
        let text = md.to_text();
        assert_eq!(text, "Hello\nWorld\n");
    }

    #[test]
    fn test_render_options() {
        let mut md = "- Item 1\n- Item 2".parse::<Markdown>().unwrap();
        assert_eq!(md.options.list_style, ListStyle::default());

        md.set_options(RenderOptions {
            list_style: ListStyle::Plus,
        });
        assert_eq!(md.options.list_style, ListStyle::Plus);

        let pretty = md.to_string();
        assert!(pretty.contains("+ Item 1"));
    }

    #[test]
    fn test_display_simple() {
        let md = "# Header\nParagraph".parse::<Markdown>().unwrap();
        assert_eq!(md.to_string(), "# Header\nParagraph\n");
    }

    #[test]
    fn test_display_with_empty_nodes() {
        let md = "# Header\nContent".parse::<Markdown>().unwrap();
        assert_eq!(md.to_string(), "# Header\nContent\n");
    }

    #[test]
    fn test_display_with_newlines() {
        let md = "# Header\n\nParagraph 1\n\nParagraph 2"
            .parse::<Markdown>()
            .unwrap();
        assert_eq!(md.to_string(), "# Header\n\nParagraph 1\n\nParagraph 2\n");
    }

    #[test]
    fn test_display_format_lists() {
        let md = "- Item 1\n- Item 2\n- Item 3".parse::<Markdown>().unwrap();
        assert_eq!(md.to_string(), "- Item 1\n- Item 2\n- Item 3\n");
    }

    #[test]
    fn test_display_with_different_list_styles() {
        let mut md = "- Item 1\n- Item 2".parse::<Markdown>().unwrap();

        md.set_options(RenderOptions {
            list_style: ListStyle::Star,
        });

        let formatted = md.to_string();
        assert!(formatted.contains("* Item 1"));
        assert!(formatted.contains("* Item 2"));
    }
}
