#[cfg(feature = "html-to-markdown")]
use crate::html_to_markdown;
#[cfg(feature = "html-to-markdown")]
use crate::html_to_markdown::ConversionOptions;
use markdown::Constructs;
use miette::miette;
use std::{fmt, str::FromStr};

use crate::node::{Node, Position, RenderOptions};

#[derive(Debug, Clone)]
pub struct Markdown {
    pub nodes: Vec<Node>,
    pub options: RenderOptions,
}

impl FromStr for Markdown {
    type Err = miette::Error;

    fn from_str(content: &str) -> Result<Self, Self::Err> {
        Self::from_markdown_str(content)
    }
}

impl fmt::Display for Markdown {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut pre_position: Option<Position> = None;
        let mut is_first = true;

        // Pre-allocate buffer to reduce allocations
        let mut buffer = String::with_capacity(self.nodes.len() * 50); // Reasonable estimate

        for node in &self.nodes {
            let value = node.to_string_with(&self.options);

            if value.is_empty() || value == "\n" {
                pre_position = None;
                continue;
            }

            if let Some(pos) = node.position() {
                let new_line_count = pre_position
                    .as_ref()
                    .map(|p| pos.start.line - p.end.line)
                    .unwrap_or_else(|| if is_first { 0 } else { 1 });

                pre_position = Some(pos.clone());

                // Write newlines directly to buffer instead of creating temp string
                for _ in 0..new_line_count {
                    buffer.push('\n');
                }
                buffer.push_str(&value);
            } else {
                pre_position = None;
                buffer.push_str(&value);
                buffer.push('\n');
            }

            if is_first {
                is_first = false;
            }
        }

        // Write final result to formatter
        if buffer.is_empty() || buffer.ends_with('\n') {
            write!(f, "{}", buffer)
        } else {
            writeln!(f, "{}", buffer)
        }
    }
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
        let root = markdown::to_mdast(content, &markdown::ParseOptions::mdx())
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
        let mut result = String::with_capacity(self.nodes.len() * 20); // Reasonable estimate
        for node in &self.nodes {
            result.push_str(&node.value());
            result.push('\n');
        }
        result
    }

    #[cfg(feature = "json")]
    pub fn to_json(&self) -> miette::Result<String> {
        let nodes = self
            .nodes
            .iter()
            .filter(|node| !node.is_empty() && !node.is_empty_fragment())
            .collect::<Vec<_>>();
        serde_json::to_string_pretty(&nodes)
            .map_err(|e| miette!("Failed to serialize to JSON: {}", e))
    }

    #[cfg(feature = "html-to-markdown")]
    pub fn from_html_str(content: &str) -> miette::Result<Self> {
        html_to_markdown::convert_html_to_markdown(content, ConversionOptions::default())
            .map_err(|e| miette!(e))
            .and_then(|md_string| Self::from_markdown_str(&md_string))
    }

    pub fn from_markdown_str(content: &str) -> miette::Result<Self> {
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

    use crate::{ListStyle, TitleSurroundStyle, UrlSurroundStyle};

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
    #[case::break_("a\\b", 1, "a\\b\n")]
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
        assert_eq!(md.options, RenderOptions::default());

        md.set_options(RenderOptions {
            list_style: ListStyle::Plus,
            ..RenderOptions::default()
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
            link_title_style: TitleSurroundStyle::default(),
            link_url_style: UrlSurroundStyle::default(),
        });

        let formatted = md.to_string();
        assert!(formatted.contains("* Item 1"));
        assert!(formatted.contains("* Item 2"));
    }

    #[test]
    fn test_display_with_ordered_list() {
        let md = "1. Item 1\n2. Item 2\n\n3. Item 2"
            .parse::<Markdown>()
            .unwrap();
        let formatted = md.to_string();

        assert!(formatted.contains("1. Item 1"));
        assert!(formatted.contains("2. Item 2"));
        assert!(formatted.contains("3. Item 2"));
    }
}

#[cfg(test)]
#[cfg(feature = "json")]
mod json_tests {
    use rstest::rstest;

    use super::*;

    #[test]
    fn test_to_json_simple() {
        let md = "# Hello".parse::<Markdown>().unwrap();
        let json = md.to_json().unwrap();
        assert!(json.contains("\"type\": \"Heading\""));
        assert!(json.contains("\"depth\": 1"));
        assert!(json.contains("\"values\":"));
    }

    #[test]
    fn test_to_json_complex() {
        let md = "# Header\n\n- Item 1\n- Item 2\n\n*Emphasis* and **Strong**"
            .parse::<Markdown>()
            .unwrap();
        let json = md.to_json().unwrap();

        assert!(json.contains("\"type\": \"Heading\""));
        assert!(json.contains("\"type\": \"List\""));
        assert!(json.contains("\"type\": \"Strong\""));
        assert!(json.contains("\"type\": \"Emphasis\""));
    }

    #[test]
    fn test_to_json_code_blocks() {
        let md = "```rust\nfn main() {\n    println!(\"Hello\");\n}\n```"
            .parse::<Markdown>()
            .unwrap();
        let json = md.to_json().unwrap();

        assert!(json.contains("\"type\": \"Code\""));
        assert!(json.contains("\"lang\": \"rust\""));
        assert!(json.contains("\"value\": \"fn main() {\\n    println!(\\\"Hello\\\");\\n}\""));
    }

    #[test]
    fn test_to_json_table() {
        let md = "| A | B |\n|---|---|\n| 1 | 2 |"
            .parse::<Markdown>()
            .unwrap();
        let json = md.to_json().unwrap();

        assert!(json.contains("\"type\": \"TableCell\""));
    }

    #[rstest]
    #[case("<h1>Hello</h1>", 1, "# Hello\n")]
    #[case("<p>Paragraph</p>", 1, "Paragraph\n")]
    #[case("<ul><li>Item 1</li><li>Item 2</li></ul>", 2, "- Item 1\n- Item 2\n")]
    #[case("<ol><li>First</li><li>Second</li></ol>", 2, "1. First\n2. Second\n")]
    #[case("<blockquote>Quote</blockquote>", 1, "> Quote\n")]
    #[case("<code>inline</code>", 1, "`inline`\n")]
    #[case("<pre><code>block</code></pre>", 1, "```\nblock\n```\n")]
    #[case(
        "<table><tr><td>A</td><td>B</td></tr></table>",
        3,
        "|A|B|\n|---|---|\n"
    )]
    #[cfg(feature = "html-to-markdown")]
    fn test_markdown_from_html(
        #[case] input: &str,
        #[case] expected_nodes: usize,
        #[case] expected_output: &str,
    ) {
        let md = Markdown::from_html_str(input).unwrap();
        assert_eq!(md.nodes.len(), expected_nodes);
        assert_eq!(md.to_string(), expected_output);
    }
}
