//! MDX written out and read back keeps the kinds and values of its nodes.

use mq_markdown::{Markdown, Node};
use rstest::rstest;

fn describe(node: &Node) -> String {
    let children = node.children().iter().map(describe).collect::<Vec<_>>().join(", ");
    format!("{}({:?})[{children}]", node.name(), node.value())
}

fn describe_all(markdown: &Markdown) -> Vec<String> {
    markdown.nodes.iter().map(describe).collect()
}

#[rstest]
#[case::blocks("<A>\n\n# h\n\n- l\n\n</A>")]
#[case::image("<Zoom>\n\n![a](b)\n\n</Zoom>")]
#[case::nested("<A>\n  <B>\n    x\n  </B>\n</A>")]
#[case::nested_blocks("<A>\n\n<B>\n\n# h\n\n</B>\n\n- l\n\n</A>")]
#[case::code("<A>\n\n```js\nlet x = 1;\n\n  y();\n```\n\n</A>")]
#[case::list_item("- <A>\n  x\n  </A>")]
#[case::quote("> <A>\n>\n> # h\n>\n> </A>")]
fn flow_elements_survive_a_round_trip(#[case] input: &str) {
    let markdown = Markdown::from_mdx_str(input).unwrap();
    let written = markdown.to_string();
    let reparsed = Markdown::from_mdx_str(&written).unwrap_or_else(|error| panic!("{error}\n{written}"));
    assert_eq!(describe_all(&markdown), describe_all(&reparsed), "{written}");
}

#[rstest]
#[case::blocks("<A>\n\n# h\n\n- l\n\n</A>", "<A>\n  # h\n\n  - l\n</A>")]
#[case::image("<Zoom>\n\n![a](b)\n\n</Zoom>", "<Zoom>\n  ![a](b)\n</Zoom>")]
#[case::nested("<A>\n  <B>\n    x\n  </B>\n</A>", "<A>\n  <B>\n    x\n  </B>\n</A>")]
fn flow_elements_keep_tags_and_children_on_their_own_lines(#[case] input: &str, #[case] expected: &str) {
    assert_eq!(Markdown::from_mdx_str(input).unwrap().to_string().trim_end(), expected);
}
