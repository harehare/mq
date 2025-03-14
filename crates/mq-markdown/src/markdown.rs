use std::{fmt, str::FromStr};

use comrak::{
    Arena, ComrakOptions, ListStyleType, format_commonmark, markdown_to_html, parse_document,
};
use itertools::Itertools;
use miette::{IntoDiagnostic, miette};

use crate::node::{ListStyle, Node};

#[derive(Debug, Clone)]
pub struct Markdown {
    pub nodes: Vec<Node>,
    pub options: RenderOptions,
}

impl FromStr for Markdown {
    type Err = miette::Error;

    fn from_str(content: &str) -> Result<Self, Self::Err> {
        let root = markdown::to_mdast(content, &markdown::ParseOptions::gfm())
            .map_err(|e| miette!(e.reason))?;
        let nodes = Node::from_mdast_node(root);

        Ok(Self {
            nodes,
            options: RenderOptions::default(),
        })
    }
}

impl fmt::Display for Markdown {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut pre_line = 1;
        let mut pre_position = None;
        let text = self
            .nodes
            .iter()
            .filter_map(|node| {
                let value = node.to_string_with(&self.options.list_style);

                if value.is_empty() {
                    return None;
                }

                let value = if let Some(pos) = node.position() {
                    let value = if pre_line != pos.start.line {
                        if pre_position.is_some() {
                            format!("{}{}", '\n', value)
                        } else {
                            value
                        }
                    } else {
                        value
                    };

                    pre_line = pos.start.line;
                    pre_position = Some(pos);
                    value
                } else {
                    pre_position = None;
                    format!("{}{}", value, '\n')
                };

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

    pub fn to_pretty_markdown(&self) -> miette::Result<String> {
        pretty_markdown(&self.to_string(), &self.options)
    }

    pub fn to_html(&self) -> String {
        let options = ComrakOptions::default();
        let formatted_html = markdown_to_html(self.to_string().as_str(), &options);

        formatted_html
    }

    pub fn to_text(&self) -> String {
        self.nodes
            .iter()
            .map(|node| format!("{}\n", node.value()))
            .join("")
    }
}

pub fn pretty_markdown(s: &str, options: &RenderOptions) -> miette::Result<String> {
    let options = comrak::Options {
        extension: {
            comrak::ExtensionOptions {
                strikethrough: true,
                tagfilter: true,
                table: true,
                autolink: true,
                tasklist: true,
                superscript: true,
                footnotes: true,
                description_lists: true,
                multiline_block_quotes: true,
                math_dollars: true,
                math_code: true,
                wikilinks_title_after_pipe: true,
                wikilinks_title_before_pipe: true,
                underline: true,
                subscript: true,
                spoiler: true,
                greentext: true,
                ..comrak::ExtensionOptions::default()
            }
        },
        render: comrak::RenderOptions {
            list_style: match options.list_style.clone() {
                ListStyle::Dash => ListStyleType::Dash,
                ListStyle::Plus => ListStyleType::Plus,
                ListStyle::Star => ListStyleType::Star,
            },
            ..comrak::RenderOptions::default()
        },
        ..comrak::Options::default()
    };

    let arena = Arena::new();
    let root = parse_document(&arena, s, &options);
    let mut formatted_markdown = Vec::new();

    format_commonmark(root, &options, &mut formatted_markdown).unwrap();

    String::from_utf8(formatted_markdown).into_diagnostic()
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::header("# Title", 1, "# Title\n")]
    #[case::header("# Title\nParagraph", 2, "# Title\nParagraph\n")]
    #[case::header("# Title\n\nParagraph", 2, "# Title\nParagraph\n")]
    #[case::list("- Item 1\n- Item 2", 2, "- Item 1\n- Item 2\n")]
    #[case::quote("> Quote\n\n> Second line", 2, "> Quote\n> Second line\n")]
    #[case::code("```rust\nlet x = 1;\n```", 1, "\n```rust\nlet x = 1;\n```\n")]
    #[case::toml("[test]\ntest = 1", 1, "[test]\ntest = 1\n")]
    #[case::code("`inline`", 1, "`inline`\n")]
    #[case::math("$math$", 1, "$math$\n")]
    #[case::math("$$$\nmath\n$$$", 1, "$$$\nmath\n$$$\n")]
    #[case::html("<div>test</div>", 1, "\n<div>test</div>\n")]
    #[case::image(
        "![alt text](http://example.com/image.jpg)",
        1,
        "![alt text](http://example.com/image.jpg)\n"
    )]
    #[case::image_with_title(
        "![alt text](http://example.com/image.jpg \"title\")",
        1,
        "![alt text](http://example.com/image.jpg \"title\")\n"
    )]
    #[case::yaml(
        "title: Test\ndescription: YAML front matter\n",
        1,
        "title: Test\ndescription: YAML front matter\n"
    )]
    #[case::link("[title](http://example.com)", 1, "[title](http://example.com)\n")]
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

    #[test]
    fn test_markdown_to_pretty_markdown() {
        let md = "# Hello\n* Item 1\n* Item 2".parse::<Markdown>().unwrap();
        assert_eq!(
            md.to_pretty_markdown().unwrap(),
            "# Hello\n\n- Item 1\n- Item 2\n"
        );
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

        let pretty = md.to_pretty_markdown().unwrap();
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
        assert_eq!(md.to_string(), "# Header\nParagraph 1\nParagraph 2\n");
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

        let formatted = md.to_pretty_markdown().unwrap();
        assert!(formatted.contains("* Item 1"));
        assert!(formatted.contains("* Item 2"));
    }
}
