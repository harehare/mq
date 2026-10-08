//! Invalid MDX is reported as an `MdxDiagnostic` that says what is wrong and where.

use miette::Diagnostic;
use mq_markdown::{Markdown, MdxDiagnostic, MdxErrorKind, MdxFound, MdxPlace, Point};
use rstest::rstest;

fn diagnostic(input: &str) -> MdxDiagnostic {
    let report = Markdown::from_mdx_str(input).expect_err("the input is invalid MDX");
    let message = report.to_string();
    let diagnostic = report.downcast::<MdxDiagnostic>().expect("an MDX diagnostic");
    assert_eq!(diagnostic.to_string(), message);
    diagnostic
}

#[rstest]
#[case::unclosed_expression("a {b", MdxErrorKind::UnclosedExpression, 1, 3, 2)]
#[case::unclosed_attribute_value(
    "a <b c=\"d",
    MdxErrorKind::UnclosedAttributeValue { quote: '"' },
    1,
    8,
    7
)]
#[case::unexpected_after_name(
    "<a!>",
    MdxErrorKind::Unexpected { found: MdxFound::Char('!'), place: MdxPlace::AfterName },
    1,
    3,
    2
)]
#[case::unopened_closing_tag("</a>", MdxErrorKind::UnopenedClosingTag, 1, 1, 0)]
#[case::mismatched_flow_tag(
    "<a>\n\nb\n\n</c>",
    MdxErrorKind::MismatchedClosingTag {
        closing: Some("c".into()),
        opening: Some("a".into()),
        opened_at: Some(Point { line: 1, column: 1 }),
    },
    5,
    1,
    8
)]
#[case::mismatched_text_tag(
    "x <a>b</c> y",
    MdxErrorKind::MismatchedClosingTag {
        closing: Some("c".into()),
        opening: Some("a".into()),
        opened_at: None,
    },
    1,
    7,
    6
)]
#[case::unclosed_text_element("x <a>b", MdxErrorKind::UnclosedTextElement { name: Some("a".into()) }, 1, 3, 2)]
#[case::unclosed_flow_element(
    "- <a>\n\nb",
    MdxErrorKind::UnclosedFlowElement { name: Some("a".into()), opened_at: Point { line: 1, column: 3 } },
    1,
    3,
    2
)]
fn reports_kind_and_position(
    #[case] input: &str,
    #[case] kind: MdxErrorKind,
    #[case] line: usize,
    #[case] column: usize,
    #[case] offset: usize,
) {
    let diagnostic = diagnostic(input);
    assert_eq!(diagnostic.error().kind(), &kind);
    assert_eq!(diagnostic.error().position(), Some(&Point { line, column }));
    let labels = diagnostic.labels().expect("a label").collect::<Vec<_>>();
    assert_eq!(labels.len(), 1);
    assert_eq!(labels[0].offset(), offset);
}

#[test]
fn keeps_the_message() {
    assert_eq!(
        diagnostic("<a!>").to_string(),
        "Unexpected character `!` (U+0021) after name"
    );
}
