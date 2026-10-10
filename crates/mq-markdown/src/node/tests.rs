use super::*;
use rstest::rstest;

#[rstest]
#[case::dash('-', Some(ListMarker::Dash))]
#[case::plus('+', Some(ListMarker::Plus))]
#[case::star('*', Some(ListMarker::Star))]
#[case::period('.', Some(ListMarker::Period))]
#[case::paren(')', Some(ListMarker::Paren))]
#[case::underscore('_', None)]
#[case::digit('1', None)]
fn list_marker_from_char(#[case] char: char, #[case] expected: Option<ListMarker>) {
    assert_eq!(ListMarker::from_char(char), expected);
    assert_eq!(expected.map(ListMarker::as_char), expected.map(|_| char));
}

#[rstest]
#[case::dash(ListMarker::Dash, Some(ListStyle::Dash))]
#[case::plus(ListMarker::Plus, Some(ListStyle::Plus))]
#[case::star(ListMarker::Star, Some(ListStyle::Star))]
#[case::period(ListMarker::Period, None)]
#[case::paren(ListMarker::Paren, None)]
fn list_marker_style(#[case] marker: ListMarker, #[case] expected: Option<ListStyle>) {
    assert_eq!(marker.style(), expected);
}

#[rstest]
#[case::dash(ListStyle::Dash, ListMarker::Dash)]
#[case::plus(ListStyle::Plus, ListMarker::Plus)]
#[case::star(ListStyle::Star, ListMarker::Star)]
fn list_marker_from_style(#[case] style: ListStyle, #[case] expected: ListMarker) {
    assert_eq!(ListMarker::from(style), expected);
}

#[rstest]
#[case::star('*', Some(HorizontalRuleMarker::Star))]
#[case::dash('-', Some(HorizontalRuleMarker::Dash))]
#[case::underscore('_', Some(HorizontalRuleMarker::Underscore))]
#[case::plus('+', None)]
fn horizontal_rule_marker_from_char(#[case] char: char, #[case] expected: Option<HorizontalRuleMarker>) {
    assert_eq!(HorizontalRuleMarker::from_char(char), expected);
    assert_eq!(expected.map(HorizontalRuleMarker::as_char), expected.map(|_| char));
}

#[rstest]
#[case::bullet_dash(false, Some(ListMarker::Dash), "- a")]
#[case::bullet_plus(false, Some(ListMarker::Plus), "+ a")]
#[case::bullet_star(false, Some(ListMarker::Star), "* a")]
#[case::bullet_unknown(false, None, "- a")]
#[case::bullet_with_an_ordered_marker(false, Some(ListMarker::Paren), "- a")]
#[case::ordered_period(true, Some(ListMarker::Period), "1. a")]
#[case::ordered_paren(true, Some(ListMarker::Paren), "1) a")]
#[case::ordered_unknown(true, None, "1. a")]
#[case::ordered_with_a_bullet_marker(true, Some(ListMarker::Star), "1. a")]
fn list_renders_its_marker(#[case] ordered: bool, #[case] marker: Option<ListMarker>, #[case] expected: &str) {
    let node = Node::List(List {
        index: 0,
        level: 0,
        checked: None,
        values: vec![Node::Text(Text {
            value: "a".into(),
            position: None,
        })],
        ordered,
        start: None,
        spread: false,
        marker,
        position: None,
    });
    assert_eq!(
        node.render_with_theme(&RenderOptions::default(), &ColorTheme::PLAIN),
        expected
    );
}

#[rstest]
#[case::star(Some(HorizontalRuleMarker::Star), "***")]
#[case::dash(Some(HorizontalRuleMarker::Dash), "---")]
#[case::underscore(Some(HorizontalRuleMarker::Underscore), "___")]
#[case::unknown(None, "***")]
fn horizontal_rule_renders_its_marker(#[case] marker: Option<HorizontalRuleMarker>, #[case] expected: &str) {
    let node = Node::HorizontalRule(HorizontalRule { marker, position: None });
    assert_eq!(
        node.render_with_theme(&RenderOptions::default(), &ColorTheme::PLAIN),
        expected
    );
}

/// The JSON of the markers is the character they are written with, as it was when they were chars.
#[cfg(feature = "json")]
#[rstest]
#[case::dash(ListMarker::Dash, "\"-\"")]
#[case::plus(ListMarker::Plus, "\"+\"")]
#[case::star(ListMarker::Star, "\"*\"")]
#[case::period(ListMarker::Period, "\".\"")]
#[case::paren(ListMarker::Paren, "\")\"")]
fn list_marker_json(#[case] marker: ListMarker, #[case] expected: &str) {
    assert_eq!(serde_json::to_string(&marker).unwrap(), expected);
    assert_eq!(serde_json::from_str::<ListMarker>(expected).unwrap(), marker);
}

#[cfg(feature = "json")]
#[rstest]
#[case::star(HorizontalRuleMarker::Star, "\"*\"")]
#[case::dash(HorizontalRuleMarker::Dash, "\"-\"")]
#[case::underscore(HorizontalRuleMarker::Underscore, "\"_\"")]
fn horizontal_rule_marker_json(#[case] marker: HorizontalRuleMarker, #[case] expected: &str) {
    assert_eq!(serde_json::to_string(&marker).unwrap(), expected);
    assert_eq!(serde_json::from_str::<HorizontalRuleMarker>(expected).unwrap(), marker);
}

#[cfg(feature = "json")]
#[test]
fn list_json_keeps_the_marker_as_a_string() {
    let json =
        r#"{"type":"list","values":[],"index":0,"level":0,"ordered":false,"checked":null,"spread":false,"marker":"+"}"#;
    let list = serde_json::from_str::<List>(json).unwrap();
    assert_eq!(list.marker, Some(ListMarker::Plus));
    assert!(serde_json::to_string(&list).unwrap().contains(r#""marker":"+""#));
}

#[test]
fn map_values_into_transforms_owned_fragments() {
    let node = Node::Fragment(Fragment {
        values: vec![Node::Text(Text {
            value: "before".to_string(),
            position: None,
        })],
    });

    let mapped = node
        .map_values_into(&mut |node| -> Result<Node, std::io::Error> {
            Ok(match node {
                Node::Text(text) => Node::Text(Text {
                    value: text.value.to_uppercase(),
                    position: text.position.clone(),
                }),
                node => node.clone(),
            })
        })
        .unwrap();

    assert_eq!(mapped.to_string(), "BEFORE");
}

#[test]
fn map_values_into_owned_moves_nodes_through_the_callback() {
    let node = Node::Fragment(Fragment {
        values: vec![Node::Text(Text {
            value: "before".to_string(),
            position: None,
        })],
    });

    let mapped = node
        .map_values_into_owned(&mut |node| -> Result<Node, std::io::Error> {
            Ok(match node {
                Node::Text(mut text) => {
                    text.value.make_ascii_uppercase();
                    Node::Text(text)
                }
                node => node,
            })
        })
        .unwrap();

    assert_eq!(mapped.to_string(), "BEFORE");
}

#[rstest]
#[case::text(Node::Text(Text{value: "".to_string(), position: None}),
       "test".to_string(),
       Node::Text(Text{value: "test".to_string(), position: None }))]
#[case::blockquote(Node::Blockquote(Blockquote{values: vec!["test".to_string().into()], position: None }),
       "test".to_string(),
       Node::Blockquote(Blockquote{values: vec!["test".to_string().into()], position: None }))]
#[case::delete(Node::Delete(Delete{values: vec!["test".to_string().into()], position: None }),
       "test".to_string(),
       Node::Delete(Delete{values: vec!["test".to_string().into()], position: None }))]
#[case::emphasis(Node::Emphasis(Emphasis{values: vec!["test".to_string().into()], position: None }),
       "test".to_string(),
       Node::Emphasis(Emphasis{values: vec!["test".to_string().into()], position: None }))]
#[case::strong(Node::Strong(Strong{values: vec!["test".to_string().into()], position: None }),
       "test".to_string(),
       Node::Strong(Strong{values: vec!["test".to_string().into()], position: None }))]
#[case::heading(Node::Heading(Heading {depth: HeadingDepth::H1, values: vec!["test".to_string().into()], position: None }),
       "test".to_string(),
       Node::Heading(Heading{depth: HeadingDepth::H1, values: vec!["test".to_string().into()], position: None }))]
#[case::link(Node::Link(Link {url: Url::new("test".to_string()), values: Vec::new(), title: None, position: None }),
       "test".to_string(),
       Node::Link(Link{url: Url::new("test".to_string()), values: Vec::new(), title: None, position: None }))]
#[case::image(Node::Image(Image {alt: "test".to_string(), url: "test".to_string(), title: None, position: None }),
       "test".to_string(),
       Node::Image(Image{alt: "test".to_string(), url: "test".to_string(), title: None, position: None }))]
#[case::code(Node::Code(Code {value: "test".to_string(), lang: None, fence: true, meta: None, position: None }),
       "test".to_string(),
       Node::Code(Code{value: "test".to_string(), lang: None, fence: true, meta: None, position: None }))]
#[case::footnote_ref(Node::FootnoteRef(FootnoteRef {ident: "test".to_string(), label: None, position: None }),
       "test".to_string(),
       Node::FootnoteRef(FootnoteRef{ident: "test".to_string(), label: Some("test".to_string()), position: None }))]
#[case::footnote(Node::Footnote(Footnote {ident: "test".to_string(), values: Vec::new(), position: None }),
       "test".to_string(),
       Node::Footnote(Footnote{ident: "test".to_string(), values: Vec::new(), position: None }))]
#[case::list(Node::List(List{ marker: None,start: None, spread: false, index: 0, level: 0, checked: None, ordered: false, values: vec!["test".to_string().into()], position: None }),
       "test".to_string(),
       Node::List(List{ marker: None,start: None, spread: false, index: 0, level: 0, checked: None, ordered: false, values: vec!["test".to_string().into()], position: None }))]
#[case::list(Node::List(List{ marker: None,start: None, spread: false, index: 1, level: 1, checked: Some(true), ordered: false, values: vec!["test".to_string().into()], position: None }),
       "test".to_string(),
       Node::List(List{ marker: None,start: None, spread: false, index: 1, level: 1, checked: Some(true), ordered: false, values: vec!["test".to_string().into()], position: None }))]
#[case::list(Node::List(List{ marker: None,start: None, spread: false, index: 2, level: 2, checked: Some(false), ordered: false, values: vec!["test".to_string().into()], position: None }),
       "test".to_string(),
       Node::List(List{ marker: None,start: None, spread: false, index: 2, level: 2, checked: Some(false), ordered: false, values: vec!["test".to_string().into()], position: None }))]
#[case::code_inline(Node::CodeInline(CodeInline{ value: "t".into(), position: None }),
       "test".to_string(),
       Node::CodeInline(CodeInline{ value: "test".into(), position: None }))]
#[case::math_inline(Node::MathInline(MathInline{ value: "t".into(), position: None }),
       "test".to_string(),
       Node::MathInline(MathInline{ value: "test".into(), position: None }))]
#[case::toml(Node::Toml(Toml{ value: "t".to_string(), position: None }),
       "test".to_string(),
       Node::Toml(Toml{ value: "test".to_string(), position: None }))]
#[case::yaml(Node::Yaml(Yaml{ value: "t".to_string(), position: None }),
       "test".to_string(),
       Node::Yaml(Yaml{ value: "test".to_string(), position: None }))]
#[case::html(Node::Html(Html{ value: "t".to_string(), position: None }),
       "test".to_string(),
       Node::Html(Html{ value: "test".to_string(), position: None }))]
#[case::table_row(Node::TableRow(TableRow{ values: vec![
                    Node::TableCell(TableCell{values: vec!["test1".to_string().into()], row:0, column:1,  position: None}),
                    Node::TableCell(TableCell{values: vec!["test2".to_string().into()], row:0, column:2,  position: None})
                ]
                , position: None }),
       "test3,test4".to_string(),
       Node::TableRow(TableRow{ values: vec![
                    Node::TableCell(TableCell{values: vec!["test3".to_string().into()], row:0, column:1, position: None}),
                    Node::TableCell(TableCell{values: vec!["test4".to_string().into()], row:0, column:2, position: None})
                ]
                , position: None }))]
#[case::table_cell(Node::TableCell(TableCell{values: vec!["test1".to_string().into()], row:0, column:1, position: None}),
        "test2".to_string(),
        Node::TableCell(TableCell{values: vec!["test2".to_string().into()], row:0, column:1, position: None}),)]
#[case::link_ref(Node::LinkRef(LinkRef{ident: "test2".to_string(), values: vec![attr_keys::VALUE.to_string().into()], label: Some("test2".to_string()), position: None}),
        "test2".to_string(),
        Node::LinkRef(LinkRef{ident: "test2".to_string(), values: vec![attr_keys::VALUE.to_string().into()], label: Some("test2".to_string()), position: None}),)]
#[case::image_ref(Node::ImageRef(ImageRef{alt: attr_keys::ALT.to_string(), ident: "test1".to_string(), label: None, position: None}),
        "test2".to_string(),
        Node::ImageRef(ImageRef{alt: attr_keys::ALT.to_string(), ident: "test2".to_string(), label: Some("test2".to_string()), position: None}),)]
#[case::definition(Node::Definition(Definition{ url: Url::new(attr_keys::URL.to_string()), title: None, ident: "test1".to_string(), label: None, position: None}),
        "test2".to_string(),
        Node::Definition(Definition{url: Url::new("test2".to_string()), title: None, ident: "test1".to_string(), label: None, position: None}),)]
#[case::break_(Node::Break(Break{ position: None}),
        "test".to_string(),
        Node::Break(Break{position: None}))]
#[case::horizontal_rule(Node::HorizontalRule(HorizontalRule{ marker: None, position: None}),
        "test".to_string(),
        Node::HorizontalRule(HorizontalRule{ marker: None,position: None}))]
#[case::mdx_flow_expression(Node::MdxFlowExpression(MdxFlowExpression{value: "test".into(), position: None}),
       "updated".to_string(),
       Node::MdxFlowExpression(MdxFlowExpression{value: "updated".into(), position: None}))]
#[case::mdx_text_expression(Node::MdxTextExpression(MdxTextExpression{value: "test".into(), position: None}),
       "updated".to_string(),
       Node::MdxTextExpression(MdxTextExpression{value: "updated".into(), position: None}))]
#[case::mdx_js_esm(Node::MdxJsEsm(MdxJsEsm{value: "test".into(), position: None}),
       "updated".to_string(),
       Node::MdxJsEsm(MdxJsEsm{value: "updated".into(), position: None}))]
#[case(Node::MdxJsxFlowElement(MdxJsxFlowElement{
        name: Some("div".to_string()),
        attributes: Vec::new(),
        children: vec!["test".to_string().into()],
        position: None
    }),
    "updated".to_string(),
    Node::MdxJsxFlowElement(MdxJsxFlowElement{
        name: Some("div".to_string()),
        attributes: Vec::new(),
        children: vec!["updated".to_string().into()],
        position: None
    }))]
#[case::mdx_jsx_text_element(Node::MdxJsxTextElement(MdxJsxTextElement{
        name: Some("span".into()),
        attributes: Vec::new(),
        children: vec!["test".to_string().into()],
        position: None
    }),
    "updated".to_string(),
    Node::MdxJsxTextElement(MdxJsxTextElement{
        name: Some("span".into()),
        attributes: Vec::new(),
        children: vec!["updated".to_string().into()],
        position: None
    }))]
#[case(Node::Math(Math{ value: "x^2".to_string(), position: None }),
       "test".to_string(),
       Node::Math(Math{ value: "test".to_string(), position: None }))]
fn test_with_value(#[case] node: Node, #[case] input: String, #[case] expected: Node) {
    assert_eq!(node.clone().with_value(input.as_str()), expected);
    assert_eq!(node.into_with_value(input.as_str()), expected);
}

#[rstest]
#[case(Node::Blockquote(Blockquote{values: vec![
    Node::Text(Text{value: "first".to_string(), position: None}),
    Node::Text(Text{value: "second".to_string(), position: None})
], position: None}),
    "new",
    0,
    Node::Blockquote(Blockquote{values: vec![
        Node::Text(Text{value: "new".to_string(), position: None}),
        Node::Text(Text{value: "second".to_string(), position: None})
    ], position: None}))]
#[case(Node::Blockquote(Blockquote{values: vec![
    Node::Text(Text{value: "first".to_string(), position: None}),
    Node::Text(Text{value: "second".to_string(), position: None})
], position: None}),
    "new",
    1,
    Node::Blockquote(Blockquote{values: vec![
        Node::Text(Text{value: "first".to_string(), position: None}),
        Node::Text(Text{value: "new".to_string(), position: None})
    ], position: None}))]
#[case(Node::Delete(Delete{values: vec![
    Node::Text(Text{value: "first".to_string(), position: None}),
    Node::Text(Text{value: "second".to_string(), position: None})
], position: None}),
    "new",
    0,
    Node::Delete(Delete{values: vec![
        Node::Text(Text{value: "new".to_string(), position: None}),
        Node::Text(Text{value: "second".to_string(), position: None})
    ], position: None}))]
#[case(Node::Emphasis(Emphasis{values: vec![
    Node::Text(Text{value: "first".to_string(), position: None}),
    Node::Text(Text{value: "second".to_string(), position: None})
], position: None}),
    "new",
    1,
    Node::Emphasis(Emphasis{values: vec![
        Node::Text(Text{value: "first".to_string(), position: None}),
        Node::Text(Text{value: "new".to_string(), position: None})
    ], position: None}))]
#[case(Node::Strong(Strong{values: vec![
    Node::Text(Text{value: "first".to_string(), position: None}),
    Node::Text(Text{value: "second".to_string(), position: None})
], position: None}),
    "new",
    0,
    Node::Strong(Strong{values: vec![
        Node::Text(Text{value: "new".to_string(), position: None}),
        Node::Text(Text{value: "second".to_string(), position: None})
    ], position: None}))]
#[case(Node::Heading(Heading{depth: HeadingDepth::H1, values: vec![
    Node::Text(Text{value: "first".to_string(), position: None}),
    Node::Text(Text{value: "second".to_string(), position: None})
], position: None}),
    "new",
    1,
    Node::Heading(Heading{depth: HeadingDepth::H1, values: vec![
        Node::Text(Text{value: "first".to_string(), position: None}),
        Node::Text(Text{value: "new".to_string(), position: None})
    ], position: None}))]
#[case(Node::List(List{ marker: None,start: None, spread: false, index: 0, level: 0, checked: None, ordered: false, values: vec![
    Node::Text(Text{value: "first".to_string(), position: None}),
    Node::Text(Text{value: "second".to_string(), position: None})
], position: None}),
    "new",
    0,
    Node::List(List{ marker: None,start: None, spread: false, index: 0, level: 0, checked: None, ordered: false,  values: vec![
        Node::Text(Text{value: "new".to_string(), position: None}),
        Node::Text(Text{value: "second".to_string(), position: None})
    ], position: None}))]
#[case(Node::TableCell(TableCell{column: 0, row: 0, values: vec![
    Node::Text(Text{value: "first".to_string(), position: None}),
    Node::Text(Text{value: "second".to_string(), position: None})
], position: None}),
    "new",
    1,
    Node::TableCell(TableCell{column: 0, row: 0, values: vec![
        Node::Text(Text{value: "first".to_string(), position: None}),
        Node::Text(Text{value: "new".to_string(), position: None})
    ], position: None}))]
#[case(Node::Text(Text{value: "plain text".to_string(), position: None}),
    "new",
    0,
    Node::Text(Text{value: "plain text".to_string(), position: None}))]
#[case(Node::Code(Code{value: "code".to_string(), lang: None, fence: true, meta: None, position: None}),
    "new",
    0,
    Node::Code(Code{value: "code".to_string(), lang: None, fence: true, meta: None, position: None}))]
#[case(Node::List(List{ marker: None,start: None, spread: false, index: 0, level: 1, checked: Some(true), ordered: false, values: vec![
    Node::Text(Text{value: "first".to_string(), position: None})
], position: None}),
    "new",
    0,
    Node::List(List{ marker: None,start: None, spread: false, index: 0, level: 1, checked: Some(true), ordered: false, values: vec![
        Node::Text(Text{value: "new".to_string(), position: None})
    ], position: None}))]
#[case(Node::List(List{ marker: None,start: None, spread: false, index: 0, level: 1, checked: None, ordered: false, values: vec![
    Node::Text(Text{value: "first".to_string(), position: None})
], position: None}),
    "new",
    2,
    Node::List(List{ marker: None,start: None, spread: false, index: 0, level: 1, checked: None, ordered: false, values: vec![
        Node::Text(Text{value: "first".to_string(), position: None})
    ], position: None}))]
#[case::link_ref(Node::LinkRef(LinkRef{ident: "id".to_string(), values: vec![
        Node::Text(Text{value: "first".to_string(), position: None}),
        Node::Text(Text{value: "second".to_string(), position: None})
    ], label: None, position: None}), "new", 0, Node::LinkRef(LinkRef{ident: "id".to_string(), values: vec![
        Node::Text(Text{value: "new".to_string(), position: None}),
        Node::Text(Text{value: "second".to_string(), position: None})
    ], label: None, position: None}))]
#[case(Node::MdxJsxFlowElement(MdxJsxFlowElement{
        name: Some("div".to_string()),
        attributes: Vec::new(),
        children: vec![
            Node::Text(Text{value: "first".to_string(), position: None}),
            Node::Text(Text{value: "second".to_string(), position: None})
        ],
        position: None
    }),
    "new",
    0,
    Node::MdxJsxFlowElement(MdxJsxFlowElement{
        name: Some("div".to_string()),
        attributes: Vec::new(),
        children: vec![
            Node::Text(Text{value: "new".to_string(), position: None}),
            Node::Text(Text{value: "second".to_string(), position: None})
        ],
        position: None
    }))]
#[case(Node::MdxJsxFlowElement(MdxJsxFlowElement{
        name: Some("div".to_string()),
        attributes: Vec::new(),
        children: vec![
            Node::Text(Text{value: "first".to_string(), position: None}),
            Node::Text(Text{value: "second".to_string(), position: None})
        ],
        position: None
    }),
    "new",
    1,
    Node::MdxJsxFlowElement(MdxJsxFlowElement{
        name: Some("div".to_string()),
        attributes: Vec::new(),
        children: vec![
            Node::Text(Text{value: "first".to_string(), position: None}),
            Node::Text(Text{value: "new".to_string(), position: None})
        ],
        position: None
    }))]
#[case(Node::MdxJsxTextElement(MdxJsxTextElement{
        name: Some("span".into()),
        attributes: Vec::new(),
        children: vec![
            Node::Text(Text{value: "first".to_string(), position: None}),
            Node::Text(Text{value: "second".to_string(), position: None})
        ],
        position: None
    }),
    "new",
    0,
    Node::MdxJsxTextElement(MdxJsxTextElement{
        name: Some("span".into()),
        attributes: Vec::new(),
        children: vec![
            Node::Text(Text{value: "new".to_string(), position: None}),
            Node::Text(Text{value: "second".to_string(), position: None})
        ],
        position: None
    }))]
#[case(Node::MdxJsxTextElement(MdxJsxTextElement{
        name: Some("span".into()),
        attributes: Vec::new(),
        children: vec![
            Node::Text(Text{value: "first".to_string(), position: None}),
            Node::Text(Text{value: "second".to_string(), position: None})
        ],
        position: None
    }),
    "new",
    1,
    Node::MdxJsxTextElement(MdxJsxTextElement{
        name: Some("span".into()),
        attributes: Vec::new(),
        children: vec![
            Node::Text(Text{value: "first".to_string(), position: None}),
            Node::Text(Text{value: "new".to_string(), position: None})
        ],
        position: None
    }))]
fn test_with_children_value(#[case] node: Node, #[case] value: &str, #[case] index: usize, #[case] expected: Node) {
    assert_eq!(node.clone().with_children_value(value, index), expected);
    assert_eq!(node.into_with_children_value(value, index), expected);
}

#[rstest]
#[case(Node::Text(Text{value: "test".to_string(), position: None }),
       "test".to_string())]
#[case(Node::List(List{ marker: None,start: None, spread: false, index: 0, level: 2, checked: None, ordered: false, values: vec!["test".to_string().into()], position: None}),
       "    - test".to_string())]
fn test_display(#[case] node: Node, #[case] expected: String) {
    assert_eq!(node.to_string_with(&RenderOptions::default()), expected);
}

#[rstest]
#[case(Node::Text(Text{value: "test".to_string(), position: None}), true)]
#[case(Node::CodeInline(CodeInline{value: "test".into(), position: None}), false)]
#[case(Node::MathInline(MathInline{value: "test".into(), position: None}), false)]
fn test_is_text(#[case] node: Node, #[case] expected: bool) {
    assert_eq!(node.is_text(), expected);
}

#[rstest]
#[case(Node::CodeInline(CodeInline{value: "test".into(), position: None}), true)]
#[case(Node::Text(Text{value: "test".to_string(), position: None}), false)]
fn test_is_inline_code(#[case] node: Node, #[case] expected: bool) {
    assert_eq!(node.is_inline_code(), expected);
}

#[rstest]
#[case(Node::MathInline(MathInline{value: "test".into(), position: None}), true)]
#[case(Node::Text(Text{value: "test".to_string(), position: None}), false)]
fn test_is_inline_math(#[case] node: Node, #[case] expected: bool) {
    assert_eq!(node.is_inline_math(), expected);
}

#[rstest]
#[case(Node::Strong(Strong{values: vec!["test".to_string().into()], position: None}), true)]
#[case(Node::Text(Text{value: "test".to_string(), position: None}), false)]
fn test_is_strong(#[case] node: Node, #[case] expected: bool) {
    assert_eq!(node.is_strong(), expected);
}

#[rstest]
#[case(Node::Delete(Delete{values: vec!["test".to_string().into()], position: None}), true)]
#[case(Node::Text(Text{value: "test".to_string(), position: None}), false)]
fn test_is_delete(#[case] node: Node, #[case] expected: bool) {
    assert_eq!(node.is_delete(), expected);
}

#[rstest]
#[case(Node::Link(Link{url: Url::new("test".to_string()), values: Vec::new(), title: None, position: None}), true)]
#[case(Node::Text(Text{value: "test".to_string(), position: None}), false)]
fn test_is_link(#[case] node: Node, #[case] expected: bool) {
    assert_eq!(node.is_link(), expected);
}

#[rstest]
#[case(Node::LinkRef(LinkRef{ident: "test".to_string(), values: Vec::new(), label: None, position: None}), true)]
#[case(Node::Text(Text{value: "test".to_string(), position: None}), false)]
fn test_is_link_ref(#[case] node: Node, #[case] expected: bool) {
    assert_eq!(node.is_link_ref(), expected);
}

#[rstest]
#[case(Node::Image(Image{alt: attr_keys::ALT.to_string(), url: "test".to_string(), title: None, position: None}), true)]
#[case(Node::Text(Text{value: "test".to_string(), position: None}), false)]
fn test_is_image(#[case] node: Node, #[case] expected: bool) {
    assert_eq!(node.is_image(), expected);
}

#[rstest]
#[case(Node::ImageRef(ImageRef{alt: attr_keys::ALT.to_string(), ident: "test".to_string(), label: None, position: None}), true)]
#[case(Node::Text(Text{value: "test".to_string(), position: None}), false)]
fn test_is_image_ref(#[case] node: Node, #[case] expected: bool) {
    assert_eq!(node.is_image_ref(), expected);
}

#[rstest]
#[case(Node::Code(Code{value: "code".to_string(), lang: Some("rust".to_string()), fence: true, meta: None, position: None}), true, Some("rust".into()))]
#[case(Node::Code(Code{value: "code".to_string(), lang: Some("rust".to_string()), fence: true, meta: None, position: None}), false, Some("python".into()))]
#[case(Node::Code(Code{value: "code".to_string(), lang: None, fence: true, meta: None, position: None}), true, None)]
#[case(Node::Code(Code{value: "code".to_string(), lang: None, fence: false, meta: None, position: None}), true, None)]
#[case(Node::Text(Text{value: "test".to_string(), position: None}), false, None)]
fn test_is_code(#[case] node: Node, #[case] expected: bool, #[case] lang: Option<SmolStr>) {
    assert_eq!(node.is_code(lang), expected);
}

#[rstest]
#[case(Node::Heading(Heading{depth: HeadingDepth::H1, values: vec!["test".to_string().into()], position: None}), true, Some(1))]
#[case(Node::Heading(Heading{depth: HeadingDepth::H2, values: vec!["test".to_string().into()], position: None}), false, Some(1))]
#[case(Node::Heading(Heading{depth: HeadingDepth::H1, values: vec!["test".to_string().into()], position: None}), true, None)]
#[case(Node::Text(Text{value: "test".to_string(), position: None}), false, None)]
fn test_is_heading(#[case] node: Node, #[case] expected: bool, #[case] depth: Option<u8>) {
    assert_eq!(node.is_heading(depth), expected);
}

#[rstest]
#[case(Node::HorizontalRule(HorizontalRule{ marker: None,position: None}), true)]
#[case(Node::Text(Text{value: "test".to_string(), position: None}), false)]
fn test_is_horizontal_rule(#[case] node: Node, #[case] expected: bool) {
    assert_eq!(node.is_horizontal_rule(), expected);
}

#[rstest]
#[case(Node::Blockquote(Blockquote{values: vec!["test".to_string().into()], position: None}), true)]
#[case(Node::Text(Text{value: "test".to_string(), position: None}), false)]
fn test_is_blockquote(#[case] node: Node, #[case] expected: bool) {
    assert_eq!(node.is_blockquote(), expected);
}

#[cfg(feature = "wikilink")]
#[rstest]
#[case(Node::WikiLink(WikiLink{target: "target".to_string(), text: None, position: None}), true)]
#[case(Node::WikiLink(WikiLink{target: "Three laws of motion".to_string(), text: Some("Newton".to_string()), position: None}), true)]
#[case(Node::Link(Link{url: Url::new("https://example.com".to_string()), values: Vec::new(), title: None, position: None}), false)]
#[case(Node::Link(Link{url: Url::new("relative.md".to_string()), values: Vec::new(), title: None, position: None}), false)]
#[case(Node::Text(Text{value: "test".to_string(), position: None}), false)]
#[case(Node::Text(Text{value: "[[target]]".to_string(), position: None}), false)]
fn test_is_wikilink(#[case] node: Node, #[case] expected: bool) {
    assert_eq!(node.is_wikilink(), expected);
}

#[cfg(feature = "wikilink")]
#[rstest]
#[case(Node::WikiLink(WikiLink{target: "target".to_string(), text: None, position: None}), true)]
#[case(Node::WikiLink(WikiLink{target: "Three laws of motion".to_string(), text: Some("Newton".to_string()), position: None}), true)]
#[case(Node::Link(Link{url: Url::new("https://example.com".to_string()), values: Vec::new(), title: None, position: None}), true)]
#[case(Node::Link(Link{url: Url::new("relative.md".to_string()), values: Vec::new(), title: None, position: None}), true)]
#[case(Node::Text(Text{value: "test".to_string(), position: None}), false)]
fn test_is_link_includes_wikilink(#[case] node: Node, #[case] expected: bool) {
    assert_eq!(node.is_link(), expected);
}

#[cfg(feature = "wikilink")]
#[rstest]
// no wikilinks: returns original text node unchanged
#[case("plain text", vec![Node::Text(Text{value: "plain text".to_string(), position: None})])]
// only a wikilink
#[case("[[target]]", vec![Node::WikiLink(WikiLink{target: "target".to_string(), text: None, position: None})])]
// wikilink with display text
#[case("[[target|display]]", vec![Node::WikiLink(WikiLink{target: "target".to_string(), text: Some("display".to_string()), position: None})])]
// wikilink with spaces in target
#[case("[[Three laws of motion]]", vec![Node::WikiLink(WikiLink{target: "Three laws of motion".to_string(), text: None, position: None})])]
// wikilink with .md extension
#[case("[[target.md]]", vec![Node::WikiLink(WikiLink{target: "target.md".to_string(), text: None, position: None})])]
// wikilink at start
#[case("[[target]] after", vec![
    Node::WikiLink(WikiLink{target: "target".to_string(), text: None, position: None}),
    Node::Text(Text{value: " after".to_string(), position: None}),
])]
// wikilink at end
#[case("before [[target]]", vec![
    Node::Text(Text{value: "before ".to_string(), position: None}),
    Node::WikiLink(WikiLink{target: "target".to_string(), text: None, position: None}),
])]
// wikilink in middle
#[case("before [[target]] after", vec![
    Node::Text(Text{value: "before ".to_string(), position: None}),
    Node::WikiLink(WikiLink{target: "target".to_string(), text: None, position: None}),
    Node::Text(Text{value: " after".to_string(), position: None}),
])]
// multiple wikilinks
#[case("[[a]] and [[b]]", vec![
    Node::WikiLink(WikiLink{target: "a".to_string(), text: None, position: None}),
    Node::Text(Text{value: " and ".to_string(), position: None}),
    Node::WikiLink(WikiLink{target: "b".to_string(), text: None, position: None}),
])]
// unclosed [[ treated as plain text
#[case("[[unclosed", vec![Node::Text(Text{value: "[[unclosed".to_string(), position: None})])]
// nested brackets invalid: treated as plain text
#[case("[[in[ner]]]", vec![Node::Text(Text{value: "[[in[ner]]]".to_string(), position: None})])]
// multibyte characters in surrounding text
#[case("日本語 [[ターゲット]] テキスト", vec![
    Node::Text(Text{value: "日本語 ".to_string(), position: None}),
    Node::WikiLink(WikiLink{target: "ターゲット".to_string(), text: None, position: None}),
    Node::Text(Text{value: " テキスト".to_string(), position: None}),
])]
// multibyte characters inside wikilink target and display text
#[case("[[ページ|表示名]]", vec![
    Node::WikiLink(WikiLink{target: "ページ".to_string(), text: Some("表示名".to_string()), position: None}),
])]
fn test_parse_wikilinks_in_text(#[case] input: &str, #[case] expected: Vec<Node>) {
    let result = Node::parse_wikilinks_in_text(input, None);
    assert_eq!(result, expected);
}

#[cfg(feature = "wikilink")]
#[rstest]
#[case(Node::WikiLink(WikiLink{target: "target".to_string(), text: None, position: None}), "url", Some(AttrValue::String("target".to_string())))]
#[case(Node::WikiLink(WikiLink{target: "target".to_string(), text: None, position: None}), "value", Some(AttrValue::String("target".to_string())))]
#[case(Node::WikiLink(WikiLink{target: "target".to_string(), text: Some("display".to_string()), position: None}), "url", Some(AttrValue::String("target".to_string())))]
#[case(Node::WikiLink(WikiLink{target: "target".to_string(), text: Some("display".to_string()), position: None}), "value", Some(AttrValue::String("display".to_string())))]
#[case(Node::WikiLink(WikiLink{target: "target".to_string(), text: None, position: None}), "title", None)]
fn test_wikilink_attr(#[case] node: Node, #[case] attr: &str, #[case] expected: Option<AttrValue>) {
    assert_eq!(node.attr(attr), expected);
}

#[cfg(feature = "wikilink")]
#[rstest]
// no display text: with_value sets target
#[case(
    Node::WikiLink(WikiLink{target: "target".to_string(), text: None, position: None}),
    "new-target",
    Node::WikiLink(WikiLink{target: "new-target".to_string(), text: None, position: None})
)]
// with display text: with_value sets text, not target
#[case(
    Node::WikiLink(WikiLink{target: "page".to_string(), text: Some("Display Text".to_string()), position: None}),
    "DISPLAY TEXT",
    Node::WikiLink(WikiLink{target: "page".to_string(), text: Some("DISPLAY TEXT".to_string()), position: None})
)]
fn test_wikilink_with_value(#[case] node: Node, #[case] value: &str, #[case] expected: Node) {
    assert_eq!(node.clone().with_value(value), expected);
    assert_eq!(node.into_with_value(value), expected);
}

#[cfg(feature = "wikilink")]
#[rstest]
// footnote with no wikilinks: values unchanged
#[case(
    Node::Footnote(Footnote{ident: "1".to_string(), values: vec![Node::Text(Text{value: "plain text".to_string(), position: None})], position: None}),
    vec![Node::Footnote(Footnote{ident: "1".to_string(), values: vec![Node::Text(Text{value: "plain text".to_string(), position: None})], position: None})]
)]
// footnote with a wikilink in text: expanded to WikiLink node
#[case(
    Node::Footnote(Footnote{ident: "1".to_string(), values: vec![Node::Text(Text{value: "[[target]]".to_string(), position: None})], position: None}),
    vec![Node::Footnote(Footnote{ident: "1".to_string(), values: vec![Node::WikiLink(WikiLink{target: "target".to_string(), text: None, position: None})], position: None})]
)]
// footnote with wikilink and display text
#[case(
    Node::Footnote(Footnote{ident: "2".to_string(), values: vec![Node::Text(Text{value: "[[target|display]]".to_string(), position: None})], position: None}),
    vec![Node::Footnote(Footnote{ident: "2".to_string(), values: vec![Node::WikiLink(WikiLink{target: "target".to_string(), text: Some("display".to_string()), position: None})], position: None})]
)]
// footnote with mixed text and wikilink
#[case(
    Node::Footnote(Footnote{ident: "3".to_string(), values: vec![Node::Text(Text{value: "see [[note]]".to_string(), position: None})], position: None}),
    vec![Node::Footnote(Footnote{ident: "3".to_string(), values: vec![
        Node::Text(Text{value: "see ".to_string(), position: None}),
        Node::WikiLink(WikiLink{target: "note".to_string(), text: None, position: None}),
    ], position: None})]
)]
fn test_expand_wikilinks_footnote(#[case] input: Node, #[case] expected: Vec<Node>) {
    let result = Node::expand_wikilinks(vec![input]);
    assert_eq!(result, expected);
}

#[cfg(all(feature = "embed", feature = "wikilink"))]
#[rstest]
// embed only
#[case("![[note.md]]", vec![
    Node::Embed(Embed{target: "note.md".to_string(), display: None, position: None}),
])]
// wikilink only
#[case("[[target]]", vec![
    Node::WikiLink(WikiLink{target: "target".to_string(), text: None, position: None}),
])]
// embed and wikilink in same text
#[case("![[embed]] and [[link]]", vec![
    Node::Embed(Embed{target: "embed".to_string(), display: None, position: None}),
    Node::Text(Text{value: " and ".to_string(), position: None}),
    Node::WikiLink(WikiLink{target: "link".to_string(), text: None, position: None}),
])]
// wikilink before embed
#[case("[[link]] and ![[embed]]", vec![
    Node::WikiLink(WikiLink{target: "link".to_string(), text: None, position: None}),
    Node::Text(Text{value: " and ".to_string(), position: None}),
    Node::Embed(Embed{target: "embed".to_string(), display: None, position: None}),
])]
// embed with display hint
#[case("![[image.png|400]]", vec![
    Node::Embed(Embed{target: "image.png".to_string(), display: Some("400".to_string()), position: None}),
])]
// wikilink with display text
#[case("[[page|display]]", vec![
    Node::WikiLink(WikiLink{target: "page".to_string(), text: Some("display".to_string()), position: None}),
])]
// surrounding text
#[case("before ![[note]] after [[link]] end", vec![
    Node::Text(Text{value: "before ".to_string(), position: None}),
    Node::Embed(Embed{target: "note".to_string(), display: None, position: None}),
    Node::Text(Text{value: " after ".to_string(), position: None}),
    Node::WikiLink(WikiLink{target: "link".to_string(), text: None, position: None}),
    Node::Text(Text{value: " end".to_string(), position: None}),
])]
// plain text with no links
#[case("just plain text", vec![
    Node::Text(Text{value: "just plain text".to_string(), position: None}),
])]
// multibyte characters
#[case("日本語 ![[ファイル]] と [[ページ]]", vec![
    Node::Text(Text{value: "日本語 ".to_string(), position: None}),
    Node::Embed(Embed{target: "ファイル".to_string(), display: None, position: None}),
    Node::Text(Text{value: " と ".to_string(), position: None}),
    Node::WikiLink(WikiLink{target: "ページ".to_string(), text: None, position: None}),
])]
fn test_parse_inline_links_into(#[case] input: &str, #[case] expected: Vec<Node>) {
    let mut result = Vec::new();
    Node::parse_inline_links_into(input, None, &mut result);
    assert_eq!(result, expected);
}

#[rstest]
#[case(Node::Html(Html{value: "<div>test</div>".to_string(), position: None}), true)]
#[case(Node::Text(Text{value: "test".to_string(), position: None}), false)]
fn test_is_html(#[case] node: Node, #[case] expected: bool) {
    assert_eq!(node.is_html(), expected);
}

#[rstest]
#[case(Node::node_values(
       &Node::Strong(Strong{values: vec!["test".to_string().into()], position: None})),
       vec!["test".to_string().into()])]
#[case(Node::node_values(
       &Node::Text(Text{value: "test".to_string(), position: None})),
       vec!["test".to_string().into()])]
#[case(Node::node_values(
       &Node::Blockquote(Blockquote{values: vec!["test".to_string().into()], position: None})),
       vec!["test".to_string().into()])]
#[case(Node::node_values(
       &Node::Delete(Delete{values: vec!["test".to_string().into()], position: None})),
       vec!["test".to_string().into()])]
#[case(Node::node_values(
       &Node::Emphasis(Emphasis{values: vec!["test".to_string().into()], position: None})),
       vec!["test".to_string().into()])]
#[case(Node::node_values(
       &Node::Heading(Heading{depth: HeadingDepth::H1, values: vec!["test".to_string().into()], position: None})),
       vec!["test".to_string().into()])]
#[case(Node::node_values(
       &Node::List(List{ marker: None,values: vec!["test".to_string().into()], ordered: false, level: 1, checked: Some(false), index: 0, start: None, spread: false, position: None})),
       vec!["test".to_string().into()])]
fn test_node_value(#[case] actual: Vec<Node>, #[case] expected: Vec<Node>) {
    assert_eq!(actual, expected);
}

#[rstest]
#[case(Node::Footnote(Footnote{ident: "test".to_string(), values: Vec::new(), position: None}), true)]
#[case(Node::Text(Text{value: "test".to_string(), position: None}), false)]
fn test_is_footnote(#[case] node: Node, #[case] expected: bool) {
    assert_eq!(node.is_footnote(), expected);
}

#[rstest]
#[case(Node::FootnoteRef(FootnoteRef{ident: "test".to_string(), label: None, position: None}), true)]
#[case(Node::Text(Text{value: "test".to_string(), position: None}), false)]
fn test_is_footnote_ref(#[case] node: Node, #[case] expected: bool) {
    assert_eq!(node.is_footnote_ref(), expected);
}

#[rstest]
#[case(Node::Math(Math{value: "x^2".to_string(), position: None}), true)]
#[case(Node::Text(Text{value: "test".to_string(), position: None}), false)]
fn test_is_math(#[case] node: Node, #[case] expected: bool) {
    assert_eq!(node.is_math(), expected);
}

#[rstest]
#[case(Node::Break(Break{position: None}), true)]
#[case(Node::Text(Text{value: "test".to_string(), position: None}), false)]
fn test_is_break(#[case] node: Node, #[case] expected: bool) {
    assert_eq!(node.is_break(), expected);
}

#[rstest]
#[case(Node::Yaml(Yaml{value: "key: value".to_string(), position: None}), true)]
#[case(Node::Text(Text{value: "test".to_string(), position: None}), false)]
fn test_is_yaml(#[case] node: Node, #[case] expected: bool) {
    assert_eq!(node.is_yaml(), expected);
}

#[rstest]
#[case(Node::Toml(Toml{value: "key = \"value\"".to_string(), position: None}), true)]
#[case(Node::Text(Text{value: "test".to_string(), position: None}), false)]
fn test_is_toml(#[case] node: Node, #[case] expected: bool) {
    assert_eq!(node.is_toml(), expected);
}

#[rstest]
#[case(Node::Definition(Definition{ident: attr_keys::IDENT.to_string(), url: Url::new(attr_keys::URL.to_string()), title: None, label: None, position: None}), true)]
#[case(Node::Text(Text{value: "test".to_string(), position: None}), false)]
fn test_is_definition(#[case] node: Node, #[case] expected: bool) {
    assert_eq!(node.is_definition(), expected);
}

#[rstest]
#[case(Node::Emphasis(Emphasis{values: vec!["test".to_string().into()], position: None}), true)]
#[case(Node::Text(Text{value: "test".to_string(), position: None}), false)]
fn test_is_emphasis(#[case] node: Node, #[case] expected: bool) {
    assert_eq!(node.is_emphasis(), expected);
}

#[rstest]
#[case(Node::MdxFlowExpression(MdxFlowExpression{value: "test".into(), position: None}), true)]
#[case(Node::Text(Text{value: "test".to_string(), position: None}), false)]
fn test_is_mdx_flow_expression(#[case] node: Node, #[case] expected: bool) {
    assert_eq!(node.is_mdx_flow_expression(), expected);
}

#[rstest]
#[case(Node::MdxTextExpression(MdxTextExpression{value: "test".into(), position: None}), true)]
#[case(Node::Text(Text{value: "test".to_string(), position: None}), false)]
fn test_is_mdx_text_expression(#[case] node: Node, #[case] expected: bool) {
    assert_eq!(node.is_mdx_text_expression(), expected);
}

#[rstest]
#[case(Node::MdxJsxFlowElement(MdxJsxFlowElement{name: None, attributes: Vec::new(), children: Vec::new(), position: None}), true)]
#[case(Node::Text(Text{value: "test".to_string(), position: None}), false)]
fn test_is_mdx_jsx_flow_element(#[case] node: Node, #[case] expected: bool) {
    assert_eq!(node.is_mdx_jsx_flow_element(), expected);
}

#[rstest]
#[case(Node::MdxJsxTextElement(MdxJsxTextElement{name: None, attributes: Vec::new(), children: Vec::new(), position: None}), true)]
#[case(Node::Text(Text{value: "test".to_string(), position: None}), false)]
fn test_is_mdx_jsx_text_element(#[case] node: Node, #[case] expected: bool) {
    assert_eq!(node.is_mdx_jsx_text_element(), expected);
}

#[rstest]
#[case(Node::MdxJsEsm(MdxJsEsm{value: "test".into(), position: None}), true)]
#[case(Node::Text(Text{value: "test".to_string(), position: None}), false)]
fn test_is_msx_js_esm(#[case] node: Node, #[case] expected: bool) {
    assert_eq!(node.is_mdx_js_esm(), expected);
}

#[rstest]
#[case::text(Node::Text(Text{value: "test".to_string(), position: None }), RenderOptions::default(), "test")]
#[case::list(Node::List(List{ marker: None,start: None, spread: false, index: 0, level: 2, checked: None, ordered: false, values: vec!["test".to_string().into()], position: None}), RenderOptions::default(), "    - test")]
#[case::list(Node::List(List{ marker: None,start: None, spread: false, index: 0, level: 1, checked: None, ordered: false, values: vec!["test".to_string().into()], position: None}), RenderOptions { list_style: Some(ListStyle::Plus), ..Default::default() }, "  + test")]
#[case::list(Node::List(List{ marker: None,start: None, spread: false, index: 0, level: 1, checked: Some(true), ordered: false, values: vec!["test".to_string().into()], position: None}), RenderOptions { list_style: Some(ListStyle::Star), ..Default::default() }, "  * [x] test")]
#[case::list(Node::List(List{ marker: None,start: None, spread: false, index: 0, level: 1, checked: Some(false), ordered: false, values: vec!["test".to_string().into()], position: None}), RenderOptions::default(), "  - [ ] test")]
#[case::list(Node::List(List{ marker: None,start: None, spread: false, index: 0, level: 1, checked: None, ordered: true, values: vec!["test".to_string().into()], position: None}), RenderOptions::default(), "  1. test")]
#[case::list(Node::List(List{ marker: None,start: None, spread: false, index: 0, level: 1, checked: Some(false), ordered: true, values: vec!["test".to_string().into()], position: None}), RenderOptions::default(), "  1. [ ] test")]
#[case::table_row(Node::TableRow(TableRow{values: vec![Node::TableCell(TableCell{column: 0, row: 0, values: vec!["test".to_string().into()], position: None})], position: None}), RenderOptions::default(), "|test|")]
#[case::table_row(Node::TableRow(TableRow{values: vec![Node::TableCell(TableCell{column: 0, row: 0, values: vec!["test".to_string().into()], position: None})], position: None}), RenderOptions::default(), "|test|")]
#[case::table_cell(Node::TableCell(TableCell{column: 0, row: 0, values: vec!["test".to_string().into()], position: None}), RenderOptions::default(), "test")]
#[case::table_cell(Node::TableCell(TableCell{column: 0, row: 0, values: vec!["test".to_string().into()], position: None}), RenderOptions::default(), "test")]
#[case::table_align(Node::TableAlign(TableAlign{align: vec![TableAlignKind::Left, TableAlignKind::Right, TableAlignKind::Center, TableAlignKind::None], position: None}), RenderOptions::default(), "|:---|---:|:---:|---|")]
#[case::block_quote(Node::Blockquote(Blockquote{values: vec!["test".to_string().into()], position: None}), RenderOptions::default(), "> test")]
#[case::block_quote(Node::Blockquote(Blockquote{values: vec!["test\ntest2".to_string().into()], position: None}), RenderOptions::default(), "> test\n> test2")]
#[case::code(Node::Code(Code{value: "code".to_string(), lang: Some("rust".to_string()), fence: true, meta: None, position: None}), RenderOptions::default(), "```rust\ncode\n```")]
#[case::code(Node::Code(Code{value: "code".to_string(), lang: None, fence: true, meta: None, position: None}), RenderOptions::default(), "```\ncode\n```")]
#[case::code(Node::Code(Code{value: "code".to_string(), lang: None, fence: false, meta: None, position: None}), RenderOptions::default(), "    code")]
#[case::code(Node::Code(Code{value: "code".to_string(), lang: Some("rust".to_string()), fence: true, meta: Some("meta".to_string()), position: None}), RenderOptions::default(), "```rust meta\ncode\n```")]
#[case::code_empty_body_no_blank_line(Node::Code(Code{value: "".to_string(), lang: None, fence: true, meta: None, position: None}), RenderOptions::default(), "```\n```")]
#[case::code_fence_escalates_past_backtick_run_in_body(Node::Code(Code{value: "aaa\n```".to_string(), lang: None, fence: true, meta: None, position: None}), RenderOptions::default(), "````\naaa\n```\n````")]
#[case::code_fence_escalates_past_longer_run(Node::Code(Code{value: "`````".to_string(), lang: None, fence: true, meta: None, position: None}), RenderOptions::default(), "``````\n`````\n``````")]
#[case::definition(Node::Definition(Definition{ident: "id".to_string(), url: Url::new(attr_keys::URL.to_string()), title: None, label: Some(attr_keys::LABEL.to_string()), position: None}), RenderOptions::default(), "[label]: url")]
#[case::definition(Node::Definition(Definition{ident: "id".to_string(), url: Url::new(attr_keys::URL.to_string()), title: Some(Title::new(attr_keys::TITLE.to_string())), label: Some(attr_keys::LABEL.to_string()), position: None}), RenderOptions::default(), "[label]: url \"title\"")]
#[case::definition(Node::Definition(Definition{ident: "id".to_string(), url: Url::new("".to_string()), title: None, label: Some(attr_keys::LABEL.to_string()), position: None}), RenderOptions::default(), "[label]: <>")]
#[case::delete(Node::Delete(Delete{values: vec!["test".to_string().into()], position: None}), RenderOptions::default(), "~~test~~")]
#[case::emphasis(Node::Emphasis(Emphasis{values: vec!["test".to_string().into()], position: None}), RenderOptions::default(), "*test*")]
#[case::emphasis_nested_in_emphasis_alternates_delimiter(Node::Emphasis(Emphasis{values: vec![Node::Emphasis(Emphasis{values: vec!["foo".to_string().into()], position: None})], position: None}), RenderOptions::default(), "_*foo*_")]
#[case::footnote(Node::Footnote(Footnote{ident: "id".to_string(), values: vec![attr_keys::LABEL.to_string().into()], position: None}), RenderOptions::default(), "[^id]: label")]
#[case::footnote_ref(Node::FootnoteRef(FootnoteRef{ident: attr_keys::LABEL.to_string(), label: Some(attr_keys::LABEL.to_string()), position: None}), RenderOptions::default(), "[^label]")]
#[case::heading(Node::Heading(Heading{depth: HeadingDepth::H1, values: vec!["test".to_string().into()], position: None}), RenderOptions::default(), "# test")]
#[case::heading(Node::Heading(Heading{depth: HeadingDepth::H3, values: vec!["test".to_string().into()], position: None}), RenderOptions::default(), "### test")]
#[case::heading_multiline_h1_stays_setext(Node::Heading(Heading{depth: HeadingDepth::H1, values: vec!["Foo\nBar".to_string().into()], position: None}), RenderOptions::default(), "Foo\nBar\n===")]
#[case::heading_multiline_h2_stays_setext(Node::Heading(Heading{depth: HeadingDepth::H2, values: vec!["Foo\nBar".to_string().into()], position: None}), RenderOptions::default(), "Foo\nBar\n---")]
#[case::heading_multiline_h3_joins_with_space(Node::Heading(Heading{depth: HeadingDepth::H3, values: vec!["Foo\nBar".to_string().into()], position: None}), RenderOptions::default(), "### Foo Bar")]
#[case::heading_trailing_hash_escaped(Node::Heading(Heading{depth: HeadingDepth::H1, values: vec!["foo #".to_string().into()], position: None}), RenderOptions::default(), "# foo \\#")]
#[case::heading_trailing_hash_run_escaped(Node::Heading(Heading{depth: HeadingDepth::H3, values: vec!["foo ###".to_string().into()], position: None}), RenderOptions::default(), "### foo \\###")]
#[case::heading_trailing_hash_after_multibyte_char(Node::Heading(Heading{depth: HeadingDepth::H1, values: vec!["foo あ#".to_string().into()], position: None}), RenderOptions::default(), "# foo あ\\#")]
#[case::heading_no_trailing_hash_unaffected(Node::Heading(Heading{depth: HeadingDepth::H1, values: vec!["foo bar".to_string().into()], position: None}), RenderOptions::default(), "# foo bar")]
#[case::html(Node::Html(Html{value: "<div>test</div>".to_string(), position: None}), RenderOptions::default(), "<div>test</div>")]
#[case::image(Node::Image(Image{alt: attr_keys::ALT.to_string(), url: attr_keys::URL.to_string(), title: None, position: None}), RenderOptions::default(), "![alt](url)")]
#[case::image(Node::Image(Image{alt: attr_keys::ALT.to_string(), url: "url with space".to_string(), title: Some(attr_keys::TITLE.to_string()), position: None}), RenderOptions::default(), "![alt](<url with space> \"title\")")]
#[case::image_ref(Node::ImageRef(ImageRef{alt: attr_keys::ALT.to_string(), ident: "id".to_string(), label: Some("id".to_string()), position: None}), RenderOptions::default(), "![alt][id]")]
#[case::image_ref(Node::ImageRef(ImageRef{alt: "id".to_string(), ident: "id".to_string(), label: Some("id".to_string()), position: None}), RenderOptions::default(), "![id]")]
#[case::code_inline(Node::CodeInline(CodeInline{value: "code".into(), position: None}), RenderOptions::default(), "`code`")]
#[case::code_inline_trailing_backtick_needs_padding(Node::CodeInline(CodeInline{value: "\\[\\`".into(), position: None}), RenderOptions::default(), "`` \\[\\` ``")]
#[case::code_inline_internal_backtick_escalates_fence(Node::CodeInline(CodeInline{value: "foo ` bar".into(), position: None}), RenderOptions::default(), "``foo ` bar``")]
#[case::code_inline_both_ends_backtick_needs_longer_padded_fence(Node::CodeInline(CodeInline{value: "``".into(), position: None}), RenderOptions::default(), "``` `` ```")]
#[case::code_inline_leading_trailing_space_needs_extra_padding(Node::CodeInline(CodeInline{value: " `` ".into(), position: None}), RenderOptions::default(), "```  ``  ```")]
#[case::code_inline_all_spaces_untouched(Node::CodeInline(CodeInline{value: "   ".into(), position: None}), RenderOptions::default(), "`   `")]
#[case::math_inline(Node::MathInline(MathInline{value: "x^2".into(), position: None}), RenderOptions::default(), "$x^2$")]
#[case::link(Node::Link(Link{url: Url::new(attr_keys::URL.to_string()), title: Some(Title::new(attr_keys::TITLE.to_string())), values: vec![attr_keys::VALUE.to_string().into()], position: None}), RenderOptions::default(), "[value](url \"title\")")]
#[case::link(Node::Link(Link{url: Url::new("".to_string()), title: None, values: vec![attr_keys::VALUE.to_string().into()], position: None}), RenderOptions::default(), "[value](<>)")]
#[case::link(Node::Link(Link{url: Url::new(attr_keys::URL.to_string()), title: None, values: vec![attr_keys::VALUE.to_string().into()], position: None}), RenderOptions::default(), "[value](url)")]
#[case::link_ref(Node::LinkRef(LinkRef{ident: "id".to_string(), values: vec!["id".to_string().into()], label: Some("id".to_string()), position: None}), RenderOptions::default(), "[id]")]
#[case::link_ref(Node::LinkRef(LinkRef{ident: "id".to_string(), values: vec!["open".to_string().into()], label: Some("id".to_string()), position: None}), RenderOptions::default(), "[open][id]")]
#[case::math(Node::Math(Math{value: "x^2".to_string(), position: None}), RenderOptions::default(), "$$\nx^2\n$$")]
#[case::strong(Node::Strong(Strong{values: vec!["test".to_string().into()], position: None}), RenderOptions::default(), "**test**")]
#[case::yaml(Node::Yaml(Yaml{value: "key: value".to_string(), position: None}), RenderOptions::default(), "---\nkey: value\n---")]
#[case::toml(Node::Toml(Toml{value: "key = \"value\"".to_string(), position: None}), RenderOptions::default(), "+++\nkey = \"value\"\n+++")]
#[case::break_(Node::Break(Break{position: None}), RenderOptions::default(), "\\\n")]
#[case::horizontal_rule(Node::HorizontalRule(HorizontalRule{ marker: None,position: None}), RenderOptions::default(), "***")]
#[case::mdx_jsx_flow_element(Node::MdxJsxFlowElement(MdxJsxFlowElement{
    name: Some("div".to_string()),
    attributes: vec![
        MdxAttributeContent::Property(MdxJsxAttribute {
            name: "className".into(),
            value: Some(MdxAttributeValue::Literal("container".into()))
        })
    ],
    children: vec![
        "content".to_string().into()
    ],
    position: None
}), RenderOptions::default(), "<div className=\"container\">\n  content\n</div>")]
#[case::mdx_jsx_flow_element(Node::MdxJsxFlowElement(MdxJsxFlowElement{
    name: Some("div".to_string()),
    attributes: vec![
        MdxAttributeContent::Property(MdxJsxAttribute {
            name: "className".into(),
            value: Some(MdxAttributeValue::Literal("container".into()))
        })
    ],
    children: Vec::new(),
    position: None
}), RenderOptions::default(), "<div className=\"container\" />")]
#[case::mdx_jsx_flow_element(Node::MdxJsxFlowElement(MdxJsxFlowElement{
    name: Some("div".to_string()),
    attributes: Vec::new(),
    children: Vec::new(),
    position: None
}), RenderOptions::default(), "<div />")]
#[case::mdx_jsx_text_element(Node::MdxJsxTextElement(MdxJsxTextElement{
    name: Some("span".into()),
    attributes: vec![
        MdxAttributeContent::Expression("...props".into())
    ],
    children: vec![
        "inline".to_string().into()
    ],
    position: None
}), RenderOptions::default(), "<span {...props}>inline</span>")]
#[case::mdx_jsx_text_element(Node::MdxJsxTextElement(MdxJsxTextElement{
    name: Some("span".into()),
    attributes: vec![
        MdxAttributeContent::Expression("...props".into())
    ],
    children: vec![
    ],
    position: None
}), RenderOptions::default(), "<span {...props} />")]
#[case::mdx_jsx_text_element(Node::MdxJsxTextElement(MdxJsxTextElement{
    name: Some("span".into()),
    attributes: vec![
    ],
    children: vec![
    ],
    position: None
}), RenderOptions::default(), "<span />")]
#[case(Node::MdxTextExpression(MdxTextExpression{
    value: "count + 1".into(),
    position: None,
}), RenderOptions::default(), "{count + 1}")]
#[case(Node::MdxJsEsm(MdxJsEsm{
    value: "import React from 'react'".into(),
    position: None,
}), RenderOptions::default(), "import React from 'react'")]
#[case::fragment_empty(Node::Fragment(Fragment{values: vec![]}), RenderOptions::default(), "")]
#[case::fragment_single(Node::Fragment(Fragment{values: vec![
    Node::Text(Text{value: "hello".to_string(), position: None})
]}), RenderOptions::default(), "hello")]
#[case::fragment_multiple(Node::Fragment(Fragment{values: vec![
    Node::Text(Text{value: "hello".to_string(), position: None}),
    Node::Text(Text{value: "world".to_string(), position: None})
]}), RenderOptions::default(), "hello\nworld")]
#[case::fragment_filters_empty(Node::Fragment(Fragment{values: vec![
    Node::Text(Text{value: "hello".to_string(), position: None}),
    Node::Empty,
    Node::Text(Text{value: "world".to_string(), position: None})
]}), RenderOptions::default(), "hello\nworld")]
#[case::fragment_all_empty(Node::Fragment(Fragment{values: vec![
    Node::Empty,
    Node::Empty,
]}), RenderOptions::default(), "")]
#[case::fragment_empty_text_as_blank_line(Node::Fragment(Fragment{values: vec![
    Node::Text(Text{value: "hello".to_string(), position: None}),
    Node::Text(Text{value: "".to_string(), position: None}),
    Node::Text(Text{value: "world".to_string(), position: None})
]}), RenderOptions::default(), "hello\n\nworld")]
#[case::fragment_filters_nested_empty_fragment(Node::Fragment(Fragment{values: vec![
    Node::Fragment(Fragment{values: vec![Node::Empty, Node::Empty]}),
    Node::Text(Text{value: "hello".to_string(), position: None}),
    Node::Fragment(Fragment{values: vec![Node::Empty]}),
]}), RenderOptions::default(), "hello")]
#[case::fragment_filters_deeply_nested_empty_fragment(Node::Fragment(Fragment{values: vec![
    Node::Fragment(Fragment{values: vec![
        Node::Fragment(Fragment{values: vec![Node::Empty]}),
        Node::Empty,
    ]}),
    Node::Text(Text{value: "hello".to_string(), position: None}),
]}), RenderOptions::default(), "hello")]
#[case::fragment_keeps_nested_fragment_with_blank_line_text(Node::Fragment(Fragment{values: vec![
    Node::Fragment(Fragment{values: vec![Node::Empty]}),
    Node::Text(Text{value: "hello".to_string(), position: None}),
    Node::Fragment(Fragment{values: vec![Node::Text(Text{value: "".to_string(), position: None})]}),
    Node::Text(Text{value: "world".to_string(), position: None}),
]}), RenderOptions::default(), "hello\n\nworld")]
#[cfg_attr(feature = "wikilink", case::wikilink(Node::WikiLink(WikiLink{target: "target".to_string(), text: None, position: None}), RenderOptions::default(), "[[target]]"))]
#[cfg_attr(feature = "wikilink", case::wikilink_with_text(Node::WikiLink(WikiLink{target: "target".to_string(), text: Some("display text".to_string()), position: None}), RenderOptions::default(), "[[target|display text]]"))]
#[cfg_attr(feature = "wikilink", case::wikilink_same_text(Node::WikiLink(WikiLink{target: "target".to_string(), text: Some("target".to_string()), position: None}), RenderOptions::default(), "[[target]]"))]
fn test_to_string_with(#[case] node: Node, #[case] options: RenderOptions, #[case] expected: &str) {
    assert_eq!(node.to_string_with(&options), expected);
}

#[test]
fn test_node_partial_ord() {
    let node1 = Node::Text(Text {
        value: "test1".to_string(),
        position: Some(Position {
            start: Point { line: 1, column: 1 },
            end: Point { line: 1, column: 5 },
        }),
    });

    let node2 = Node::Text(Text {
        value: "test2".to_string(),
        position: Some(Position {
            start: Point { line: 1, column: 6 },
            end: Point { line: 1, column: 10 },
        }),
    });

    let node3 = Node::Text(Text {
        value: "test3".to_string(),
        position: Some(Position {
            start: Point { line: 2, column: 1 },
            end: Point { line: 2, column: 5 },
        }),
    });

    assert_eq!(node1.partial_cmp(&node2), Some(std::cmp::Ordering::Less));
    assert_eq!(node2.partial_cmp(&node1), Some(std::cmp::Ordering::Greater));

    assert_eq!(node1.partial_cmp(&node3), Some(std::cmp::Ordering::Less));
    assert_eq!(node3.partial_cmp(&node1), Some(std::cmp::Ordering::Greater));

    let node4 = Node::Text(Text {
        value: "test4".to_string(),
        position: None,
    });

    assert_eq!(node1.partial_cmp(&node4), Some(std::cmp::Ordering::Less));
    assert_eq!(node4.partial_cmp(&node1), Some(std::cmp::Ordering::Greater));

    let node5 = Node::Text(Text {
        value: "test5".to_string(),
        position: None,
    });

    assert_eq!(node4.partial_cmp(&node5), Some(std::cmp::Ordering::Equal));

    let node6 = Node::Code(Code {
        value: "code".to_string(),
        lang: None,
        fence: true,
        meta: None,
        position: None,
    });

    assert_eq!(node6.partial_cmp(&node4), Some(std::cmp::Ordering::Less));
    assert_eq!(node4.partial_cmp(&node6), Some(std::cmp::Ordering::Greater));
}

#[rstest]
#[case(Node::Blockquote(Blockquote{values: Vec::new(), position: None}), "blockquote")]
#[case(Node::Break(Break{position: None}), "break")]
#[case(Node::Definition(Definition{ident: "".to_string(), url: Url::new("".to_string()), title: None, label: None, position: None}), "definition")]
#[case(Node::Delete(Delete{values: Vec::new(), position: None}), "delete")]
#[case(Node::Heading(Heading{depth: HeadingDepth::H1, values: Vec::new(), position: None}), "h1")]
#[case(Node::Heading(Heading{depth: HeadingDepth::H2, values: Vec::new(), position: None}), "h2")]
#[case(Node::Heading(Heading{depth: HeadingDepth::H3, values: Vec::new(), position: None}), "h3")]
#[case(Node::Heading(Heading{depth: HeadingDepth::H4, values: Vec::new(), position: None}), "h4")]
#[case(Node::Heading(Heading{depth: HeadingDepth::H5, values: Vec::new(), position: None}), "h5")]
#[case(Node::Heading(Heading{depth: HeadingDepth::H6, values: Vec::new(), position: None}), "h6")]
#[case(Node::Emphasis(Emphasis{values: Vec::new(), position: None}), "emphasis")]
#[case(Node::Footnote(Footnote{ident: "".to_string(), values: Vec::new(), position: None}), "footnote")]
#[case(Node::FootnoteRef(FootnoteRef{ident: "".to_string(), label: None, position: None}), "footnoteref")]
#[case(Node::Html(Html{value: "".to_string(), position: None}), "html")]
#[case(Node::Yaml(Yaml{value: "".to_string(), position: None}), "yaml")]
#[case(Node::Toml(Toml{value: "".to_string(), position: None}), "toml")]
#[case(Node::Image(Image{alt: "".to_string(), url: "".to_string(), title: None, position: None}), "image")]
#[case(Node::ImageRef(ImageRef{alt: "".to_string(), ident: "".to_string(), label: None, position: None}), "image_ref")]
#[case(Node::CodeInline(CodeInline{value: "".into(), position: None}), "code_inline")]
#[case(Node::MathInline(MathInline{value: "".into(), position: None}), "math_inline")]
#[case(Node::Link(Link{url: Url::new("".to_string()), title: None, values: Vec::new(), position: None}), "link")]
#[case(Node::LinkRef(LinkRef{ident: "".to_string(), values: Vec::new(), label: None, position: None}), "link_ref")]
#[case(Node::Math(Math{value: "".to_string(), position: None}), "math")]
#[case(Node::List(List{ marker: None,start: None, spread: false, index: 0, level: 0, checked: None, ordered: false, values: Vec::new(), position: None}), "list")]
#[case(Node::TableAlign(TableAlign{align: Vec::new(), position: None}), "table_align")]
#[case(Node::TableRow(TableRow{values: Vec::new(), position: None}), "table_row")]
#[case(Node::TableCell(TableCell{column: 0, row: 0, values: Vec::new(), position: None}), "table_cell")]
#[case(Node::Code(Code{value: "".to_string(), lang: None, fence: true, meta: None, position: None}), "code")]
#[case(Node::Strong(Strong{values: Vec::new(), position: None}), "strong")]
#[case(Node::HorizontalRule(HorizontalRule{ marker: None,position: None}), "Horizontal_rule")]
#[case(Node::MdxFlowExpression(MdxFlowExpression{value: "".into(), position: None}), "mdx_flow_expression")]
#[case(Node::MdxJsxFlowElement(MdxJsxFlowElement{name: None, attributes: Vec::new(), children: Vec::new(), position: None}), "mdx_jsx_flow_element")]
#[case(Node::MdxJsxTextElement(MdxJsxTextElement{name: None, attributes: Vec::new(), children: Vec::new(), position: None}), "mdx_jsx_text_element")]
#[case(Node::MdxTextExpression(MdxTextExpression{value: "".into(), position: None}), "mdx_text_expression")]
#[case(Node::MdxJsEsm(MdxJsEsm{value: "".into(), position: None}), "mdx_js_esm")]
#[case(Node::Text(Text{value: "".to_string(), position: None}), "text")]
fn test_name(#[case] node: Node, #[case] expected: &str) {
    assert_eq!(node.name(), expected);
}

#[rstest]
#[case(Node::Text(Text{value: "test".to_string(), position: None}), "test")]
#[case(Node::List(List{ marker: None,start: None, spread: false, index: 0, level: 0, checked: None, ordered: false, values: vec![Node::Text(Text{value: "test".to_string(), position: None})], position: None}), "test")]
#[case(Node::Blockquote(Blockquote{values: vec![Node::Text(Text{value: "test".to_string(), position: None})], position: None}), "test")]
#[case(Node::Delete(Delete{values: vec![Node::Text(Text{value: "test".to_string(), position: None})], position: None}), "test")]
#[case(Node::Heading(Heading{depth: HeadingDepth::H1, values: vec![Node::Text(Text{value: "test".to_string(), position: None})], position: None}), "test")]
#[case(Node::Emphasis(Emphasis{values: vec![Node::Text(Text{value: "test".to_string(), position: None})], position: None}), "test")]
#[case(Node::Footnote(Footnote{ident: "test".to_string(), values: vec![Node::Text(Text{value: "test".to_string(), position: None})], position: None}), "test")]
#[case(Node::FootnoteRef(FootnoteRef{ident: "test".to_string(), label: None, position: None}), "test")]
#[case(Node::Html(Html{value: "test".to_string(), position: None}), "test")]
#[case(Node::Yaml(Yaml{value: "test".to_string(), position: None}), "test")]
#[case(Node::Toml(Toml{value: "test".to_string(), position: None}), "test")]
#[case(Node::Image(Image{alt: attr_keys::ALT.to_string(), url: "test".to_string(), title: None, position: None}), "test")]
#[case(Node::ImageRef(ImageRef{alt: attr_keys::ALT.to_string(), ident: "test".to_string(), label: None, position: None}), "test")]
#[case(Node::CodeInline(CodeInline{value: "test".into(), position: None}), "test")]
#[case(Node::MathInline(MathInline{value: "test".into(), position: None}), "test")]
#[case(Node::Link(Link{url: Url::new("test".to_string()), title: None, values: Vec::new(), position: None}), "test")]
#[case(Node::LinkRef(LinkRef{ident: "test".to_string(), values: Vec::new(), label: None, position: None}), "test")]
#[case(Node::Math(Math{value: "test".to_string(), position: None}), "test")]
#[case(Node::Code(Code{value: "test".to_string(), lang: None, fence: true, meta: None, position: None}), "test")]
#[case(Node::Strong(Strong{values: vec![Node::Text(Text{value: "test".to_string(), position: None})], position: None}), "test")]
#[case(Node::TableCell(TableCell{column: 0, row: 0, values: vec![Node::Text(Text{value: "test".to_string(), position: None})], position: None}), "test")]
#[case(Node::TableRow(TableRow{values: vec![Node::TableCell(TableCell{column: 0, row: 0, values: vec![Node::Text(Text{value: "test".to_string(), position: None})], position: None})], position: None}), "test")]
#[case(Node::Break(Break{position: None}), "")]
#[case(Node::HorizontalRule(HorizontalRule{ marker: None,position: None}), "")]
#[case(Node::TableAlign(TableAlign{align: Vec::new(), position: None}), "")]
#[case(Node::MdxFlowExpression(MdxFlowExpression{value: "test".into(), position: None}), "test")]
#[case(Node::MdxTextExpression(MdxTextExpression{value: "test".into(), position: None}), "test")]
#[case(Node::MdxJsEsm(MdxJsEsm{value: "test".into(), position: None}), "test")]
#[case(Node::MdxJsxFlowElement(MdxJsxFlowElement{name: Some(attr_keys::NAME.to_string()), attributes: Vec::new(), children: vec![Node::Text(Text{value: "test".to_string(), position: None})],  position: None}), "test")]
#[case(Node::Definition(Definition{ident: "test".to_string(), url: Url::new(attr_keys::URL.to_string()), title: None, label: None, position: None}), attr_keys::URL)]
#[case(Node::Fragment(Fragment {values: vec![Node::Text(Text{value: "test".to_string(), position: None})]}), "test")]
fn test_value(#[case] node: Node, #[case] expected: &str) {
    assert_eq!(node.value(), expected);
}

#[rstest]
#[case(Node::Text(Text{value: "test".to_string(), position: None}), None)]
#[case(Node::Text(Text{value: "test".to_string(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
#[case(Node::List(List{ marker: None,start: None, spread: false, index: 0, level: 0, checked: None, ordered: false, values: Vec::new(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
#[case(Node::Blockquote(Blockquote{values: Vec::new(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
#[case(Node::Delete(Delete{values: Vec::new(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
#[case(Node::Heading(Heading{depth: HeadingDepth::H1, values: Vec::new(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
#[case(Node::Emphasis(Emphasis{values: Vec::new(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
#[case(Node::Footnote(Footnote{ident: "".to_string(), values: Vec::new(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
#[case(Node::FootnoteRef(FootnoteRef{ident: "".to_string(), label: None, position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
#[case(Node::Html(Html{value: "".to_string(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
#[case(Node::Yaml(Yaml{value: "".to_string(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
#[case(Node::Toml(Toml{value: "".to_string(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
#[case(Node::Image(Image{alt: "".to_string(), url: "".to_string(), title: None, position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
#[case(Node::ImageRef(ImageRef{alt: "".to_string(), ident: "".to_string(), label: None, position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
#[case(Node::CodeInline(CodeInline{value: "".into(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
#[case(Node::MathInline(MathInline{value: "".into(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
#[case(Node::Link(Link{url: Url("".to_string()), title: None, values: Vec::new(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
#[case(Node::LinkRef(LinkRef{ident: "".to_string(), values: Vec::new(), label: None, position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
#[case(Node::Math(Math{value: "".to_string(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
#[case(Node::Code(Code{value: "".to_string(), lang: None, fence: true, meta: None, position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
#[case(Node::Strong(Strong{values: Vec::new(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
#[case(Node::TableCell(TableCell{column: 0, row: 0, values: Vec::new(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
#[case(Node::TableRow(TableRow{values: Vec::new(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
#[case(Node::TableAlign(TableAlign{align: Vec::new(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
#[case(Node::Break(Break{position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
#[case(Node::HorizontalRule(HorizontalRule{ marker: None,position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
#[case(Node::MdxFlowExpression(MdxFlowExpression{value: "test".into(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
#[case(Node::MdxTextExpression(MdxTextExpression{value: "test".into(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
#[case(Node::MdxJsEsm(MdxJsEsm{value: "test".into(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
#[case(Node::MdxJsxFlowElement(MdxJsxFlowElement{name: Some("div".to_string()), attributes: Vec::new(), children: Vec::new(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
#[case(Node::MdxJsxTextElement(MdxJsxTextElement{name: Some("span".into()), attributes: Vec::new(), children: Vec::new(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
#[case(Node::Definition(Definition{ident: "".to_string(), url: Url("".to_string()), title: None, label: None, position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
#[case(Node::Fragment(Fragment{values: Vec::new()}), None)]
#[case(Node::Fragment(Fragment{values: vec![
    Node::Text(Text{value: "test1".to_string(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}),
    Node::Text(Text{value: "test2".to_string(), position: Some(Position{start: Point{line: 1, column: 6}, end: Point{line: 1, column: 10}})})
]}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 10}}))]
#[case(Node::Fragment(Fragment{values: vec![
    Node::Text(Text{value: "test".to_string(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}),
    Node::Text(Text{value: "test2".to_string(), position: None})
]}), Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}}))]
#[case(Node::Fragment(Fragment{values: vec![
    Node::Text(Text{value: "test".to_string(), position: None}),
    Node::Text(Text{value: "test2".to_string(), position: Some(Position{start: Point{line: 1, column: 6}, end: Point{line: 1, column: 10}})})
]}), Some(Position{start: Point{line: 1, column: 6}, end: Point{line: 1, column: 10}}))]
#[case(Node::Fragment(Fragment{values: vec![
    Node::Text(Text{value: "test2".to_string(), position: Some(Position{start: Point{line: 1, column: 6}, end: Point{line: 1, column: 10}})}),
    Node::Text(Text{value: "test".to_string(), position: None})
]}), Some(Position{start: Point{line: 1, column: 6}, end: Point{line: 1, column: 10}}))]
#[case(Node::Fragment(Fragment{values: vec![
    Node::Text(Text{value: "test".to_string(), position: None}),
    Node::Text(Text{value: "test2".to_string(), position: None})
]}), None)]
#[case(Node::Empty, None)]
fn test_position(#[case] node: Node, #[case] expected: Option<Position>) {
    assert_eq!(node.position(), expected);
}

#[rstest]
#[case(Node::Blockquote(Blockquote{values: vec![
    Node::Text(Text{value: "first".to_string(), position: None}),
    Node::Text(Text{value: "second".to_string(), position: None})
], position: None}), 0, Some(Node::Text(Text{value: "first".to_string(), position: None})))]
#[case(Node::Blockquote(Blockquote{values: vec![
    Node::Text(Text{value: "first".to_string(), position: None}),
    Node::Text(Text{value: "second".to_string(), position: None})
], position: None}), 1, Some(Node::Text(Text{value: "second".to_string(), position: None})))]
#[case(Node::Blockquote(Blockquote{values: vec![
    Node::Text(Text{value: "first".to_string(), position: None})
], position: None}), 1, None)]
#[case(Node::Delete(Delete{values: vec![
    Node::Text(Text{value: "first".to_string(), position: None}),
    Node::Text(Text{value: "second".to_string(), position: None})
], position: None}), 0, Some(Node::Text(Text{value: "first".to_string(), position: None})))]
#[case(Node::Emphasis(Emphasis{values: vec![
    Node::Text(Text{value: "first".to_string(), position: None}),
    Node::Text(Text{value: "second".to_string(), position: None})
], position: None}), 1, Some(Node::Text(Text{value: "second".to_string(), position: None})))]
#[case(Node::Strong(Strong{values: vec![
    Node::Text(Text{value: "first".to_string(), position: None})
], position: None}), 0, Some(Node::Text(Text{value: "first".to_string(), position: None})))]
#[case(Node::Heading(Heading{depth: HeadingDepth::H1, values: vec![
    Node::Text(Text{value: "first".to_string(), position: None}),
    Node::Text(Text{value: "second".to_string(), position: None})
], position: None}), 0, Some(Node::Text(Text{value: "first".to_string(), position: None})))]
#[case(Node::List(List{ marker: None,start: None, spread: false, index: 0, level: 0, checked: None, ordered: false, values: vec![
    Node::Text(Text{value: "first".to_string(), position: None}),
    Node::Text(Text{value: "second".to_string(), position: None})
], position: None}), 1, Some(Node::Text(Text{value: "second".to_string(), position: None})))]
#[case(Node::TableCell(TableCell{column: 0, row: 0, values: vec![
    Node::Text(Text{value: "cell content".to_string(), position: None})
], position: None}), 0, Some(Node::Text(Text{value: "cell content".to_string(), position: None})))]
#[case(Node::TableRow(TableRow{values: vec![
    Node::TableCell(TableCell{column: 0, row: 0, values: Vec::new(), position: None}),
    Node::TableCell(TableCell{column: 1, row: 0, values: Vec::new(), position: None})
], position: None}), 1, Some(Node::TableCell(TableCell{column: 1, row: 0, values: Vec::new(), position: None})))]
#[case(Node::Text(Text{value: "plain text".to_string(), position: None}), 0, None)]
#[case(Node::Code(Code{value: "code".to_string(), lang: None, fence: true, meta: None, position: None}), 0, None)]
#[case(Node::Html(Html{value: "<div>".to_string(), position: None}), 0, None)]
fn test_find_at_index(#[case] node: Node, #[case] index: usize, #[case] expected: Option<Node>) {
    assert_eq!(node.find_at_index(index), expected);
}

#[rstest]
#[case(Node::Blockquote(Blockquote{values: vec!["test".to_string().into()], position: None}),
       Node::Fragment(Fragment{values: vec!["test".to_string().into()]}))]
#[case(Node::Delete(Delete{values: vec!["test".to_string().into()], position: None}),
       Node::Fragment(Fragment{values: vec!["test".to_string().into()]}))]
#[case(Node::Heading(Heading{depth: HeadingDepth::H1, values: vec!["test".to_string().into()], position: None}),
       Node::Fragment(Fragment{values: vec!["test".to_string().into()]}))]
#[case(Node::Emphasis(Emphasis{values: vec!["test".to_string().into()], position: None}),
       Node::Fragment(Fragment{values: vec!["test".to_string().into()]}))]
#[case(Node::List(List{ marker: None,start: None, spread: false, index: 0, level: 0, checked: None, ordered: false, values: vec!["test".to_string().into()], position: None}),
       Node::Fragment(Fragment{values: vec!["test".to_string().into()]}))]
#[case(Node::Strong(Strong{values: vec!["test".to_string().into()], position: None}),
       Node::Fragment(Fragment{values: vec!["test".to_string().into()]}))]
#[case(Node::Link(Link{url: Url(attr_keys::URL.to_string()), title: None, values: vec!["test".to_string().into()], position: None}),
       Node::Fragment(Fragment{values: vec!["test".to_string().into()]}))]
#[case(Node::LinkRef(LinkRef{ident: "id".to_string(), values: vec!["test".to_string().into()], label: None, position: None}),
       Node::Fragment(Fragment{values: vec!["test".to_string().into()]}))]
#[case(Node::Footnote(Footnote{ident: "id".to_string(), values: vec!["test".to_string().into()], position: None}),
       Node::Fragment(Fragment{values: vec!["test".to_string().into()]}))]
#[case(Node::TableCell(TableCell{column: 0, row: 0, values: vec!["test".to_string().into()], position: None}),
       Node::Fragment(Fragment{values: vec!["test".to_string().into()]}))]
#[case(Node::TableRow(TableRow{values: vec!["test".to_string().into()], position: None}),
       Node::Fragment(Fragment{values: vec!["test".to_string().into()]}))]
#[case(Node::Fragment(Fragment{values: vec!["test".to_string().into()]}),
       Node::Fragment(Fragment{values: vec!["test".to_string().into()]}))]
#[case(Node::Text(Text{value: "test".to_string(), position: None}),
       Node::Empty)]
#[case(Node::Code(Code{value: "test".to_string(), lang: None, fence: true, meta: None, position: None}),
       Node::Empty)]
#[case(Node::Image(Image{alt: attr_keys::ALT.to_string(), url: attr_keys::URL.to_string(), title: None, position: None}),
       Node::Empty)]
#[case(Node::Empty, Node::Empty)]
fn test_to_fragment(#[case] node: Node, #[case] expected: Node) {
    assert_eq!(node.clone().to_fragment(), expected);
    assert_eq!(node.into_fragment(), expected);
}

// Regression coverage for the 0.6.2 blank-line bug: `eval_markdown_node`
// replaces a non-matching `select()` result with `child_node.to_fragment()`,
// not `Node::Empty` directly. For container nodes this yields a `Fragment`
// wrapping the (now `Empty`) children, which must still render to "" with
// no stray blank lines, while a partially-matching container must render
// only the surviving content.
#[rstest]
#[case::blockquote_all_empty(Node::Blockquote(Blockquote{values: vec![Node::Empty, Node::Empty], position: None}), "")]
#[case::blockquote_mixed(Node::Blockquote(Blockquote{values: vec![Node::Empty, Node::Text(Text{value: "kept".to_string(), position: None})], position: None}), "kept")]
#[case::delete_all_empty(Node::Delete(Delete{values: vec![Node::Empty, Node::Empty], position: None}), "")]
#[case::delete_mixed(Node::Delete(Delete{values: vec![Node::Text(Text{value: "kept".to_string(), position: None}), Node::Empty], position: None}), "kept")]
#[case::heading_all_empty(Node::Heading(Heading{depth: HeadingDepth::H1, values: vec![Node::Empty, Node::Empty], position: None}), "")]
#[case::heading_mixed(Node::Heading(Heading{depth: HeadingDepth::H1, values: vec![Node::Empty, Node::Text(Text{value: "kept".to_string(), position: None})], position: None}), "kept")]
#[case::emphasis_all_empty(Node::Emphasis(Emphasis{values: vec![Node::Empty, Node::Empty], position: None}), "")]
#[case::emphasis_mixed(Node::Emphasis(Emphasis{values: vec![Node::Empty, Node::Text(Text{value: "kept".to_string(), position: None})], position: None}), "kept")]
#[case::list_all_empty(Node::List(List{ marker: None,start: None, spread: false, index: 0, level: 0, checked: None, ordered: false, values: vec![Node::Empty, Node::Empty], position: None}), "")]
#[case::list_mixed(Node::List(List{ marker: None,start: None, spread: false, index: 0, level: 0, checked: None, ordered: false, values: vec![Node::Empty, Node::Text(Text{value: "kept".to_string(), position: None})], position: None}), "kept")]
#[case::strong_all_empty(Node::Strong(Strong{values: vec![Node::Empty, Node::Empty], position: None}), "")]
#[case::strong_mixed(Node::Strong(Strong{values: vec![Node::Text(Text{value: "kept".to_string(), position: None}), Node::Empty], position: None}), "kept")]
#[case::link_all_empty(Node::Link(Link{url: Url(attr_keys::URL.to_string()), title: None, values: vec![Node::Empty, Node::Empty], position: None}), "")]
#[case::link_mixed(Node::Link(Link{url: Url(attr_keys::URL.to_string()), title: None, values: vec![Node::Empty, Node::Text(Text{value: "kept".to_string(), position: None})], position: None}), "kept")]
#[case::link_ref_all_empty(Node::LinkRef(LinkRef{ident: "id".to_string(), values: vec![Node::Empty, Node::Empty], label: None, position: None}), "")]
#[case::link_ref_mixed(Node::LinkRef(LinkRef{ident: "id".to_string(), values: vec![Node::Empty, Node::Text(Text{value: "kept".to_string(), position: None})], label: None, position: None}), "kept")]
#[case::footnote_all_empty(Node::Footnote(Footnote{ident: "id".to_string(), values: vec![Node::Empty, Node::Empty], position: None}), "")]
#[case::footnote_mixed(Node::Footnote(Footnote{ident: "id".to_string(), values: vec![Node::Empty, Node::Text(Text{value: "kept".to_string(), position: None})], position: None}), "kept")]
#[case::table_cell_all_empty(Node::TableCell(TableCell{column: 0, row: 0, values: vec![Node::Empty, Node::Empty], position: None}), "")]
#[case::table_cell_mixed(Node::TableCell(TableCell{column: 0, row: 0, values: vec![Node::Empty, Node::Text(Text{value: "kept".to_string(), position: None})], position: None}), "kept")]
fn test_to_fragment_then_render_skips_non_matching(#[case] node: Node, #[case] expected: &str) {
    assert_eq!(node.to_fragment().to_string_with(&RenderOptions::default()), expected);
}

#[rstest]
#[case(
    &mut Node::Blockquote(Blockquote{values: vec![
        Node::Text(Text{value: "old".to_string(), position: None})
    ], position: None}),
    Node::Fragment(Fragment{values: vec![
        Node::Text(Text{value: "new".to_string(), position: None})
    ]}),
    Node::Blockquote(Blockquote{values: vec![
        Node::Text(Text{value: "new".to_string(), position: None})
    ], position: None})
)]
#[case(
    &mut Node::Delete(Delete{values: vec![
        Node::Text(Text{value: "old".to_string(), position: None})
    ], position: None}),
    Node::Fragment(Fragment{values: vec![
        Node::Text(Text{value: "new".to_string(), position: None})
    ]}),
    Node::Delete(Delete{values: vec![
        Node::Text(Text{value: "new".to_string(), position: None})
    ], position: None})
)]
#[case(
    &mut Node::Emphasis(Emphasis{values: vec![
        Node::Text(Text{value: "old".to_string(), position: None})
    ], position: None}),
    Node::Fragment(Fragment{values: vec![
        Node::Text(Text{value: "new".to_string(), position: None})
    ]}),
    Node::Emphasis(Emphasis{values: vec![
        Node::Text(Text{value: "new".to_string(), position: None})
    ], position: None})
)]
#[case(
    &mut Node::Strong(Strong{values: vec![
        Node::Text(Text{value: "old".to_string(), position: None})
    ], position: None}),
    Node::Fragment(Fragment{values: vec![
        Node::Text(Text{value: "new".to_string(), position: None})
    ]}),
    Node::Strong(Strong{values: vec![
        Node::Text(Text{value: "new".to_string(), position: None})
    ], position: None})
)]
#[case(
    &mut Node::List(List{ marker: None,start: None, spread: false, index: 0, level: 0, checked: None, ordered: false, values: vec![
        Node::Text(Text{value: "old".to_string(), position: None})
    ], position: None}),
    Node::Fragment(Fragment{values: vec![
        Node::Text(Text{value: "new".to_string(), position: None})
    ]}),
    Node::List(List{ marker: None,start: None, spread: false, index: 0, level: 0, checked: None, ordered: false, values: vec![
        Node::Text(Text{value: "new".to_string(), position: None})
    ], position: None})
)]
#[case(
    &mut Node::Heading(Heading{depth: HeadingDepth::H1, values: vec![
        Node::Text(Text{value: "old".to_string(), position: None})
    ], position: None}),
    Node::Fragment(Fragment{values: vec![
        Node::Text(Text{value: "new".to_string(), position: None})
    ]}),
    Node::Heading(Heading{depth: HeadingDepth::H1, values: vec![
        Node::Text(Text{value: "new".to_string(), position: None})
    ], position: None})
)]
#[case(
    &mut Node::Link(Link{url: Url(attr_keys::URL.to_string()), title: None, values: vec![
        Node::Text(Text{value: "old".to_string(), position: None})
    ], position: None}),
    Node::Fragment(Fragment{values: vec![
        Node::Text(Text{value: "new".to_string(), position: None})
    ]}),
    Node::Link(Link{url: Url(attr_keys::URL.to_string()), title: None, values: vec![
        Node::Text(Text{value: "new".to_string(), position: None})
    ], position: None})
)]
#[case(
    &mut Node::LinkRef(LinkRef{ident: "id".to_string(), values: vec![
        Node::Text(Text{value: "old".to_string(), position: None})
    ], label: None, position: None}),
    Node::Fragment(Fragment{values: vec![
        Node::Text(Text{value: "new".to_string(), position: None})
    ]}),
    Node::LinkRef(LinkRef{ident: "id".to_string(), values: vec![
        Node::Text(Text{value: "new".to_string(), position: None})
    ], label: None, position: None})
)]
#[case(
    &mut Node::Footnote(Footnote{ident: "id".to_string(), values: vec![
        Node::Text(Text{value: "old".to_string(), position: None})
    ], position: None}),
    Node::Fragment(Fragment{values: vec![
        Node::Text(Text{value: "new".to_string(), position: None})
    ]}),
    Node::Footnote(Footnote{ident: "id".to_string(), values: vec![
        Node::Text(Text{value: "new".to_string(), position: None})
    ], position: None})
)]
#[case(
    &mut Node::TableCell(TableCell{column: 0, row: 0, values: vec![
        Node::Text(Text{value: "old".to_string(), position: None})
    ], position: None}),
    Node::Fragment(Fragment{values: vec![
        Node::Text(Text{value: "new".to_string(), position: None})
    ]}),
    Node::TableCell(TableCell{column: 0, row: 0, values: vec![
        Node::Text(Text{value: "new".to_string(), position: None})
    ], position: None})
)]
#[case(
    &mut Node::TableRow(TableRow{values: vec![
        Node::TableCell(TableCell{column: 0, row: 0, values: vec![
            Node::Text(Text{value: "old".to_string(), position: None})
        ], position: None})
    ], position: None}),
    Node::Fragment(Fragment{values: vec![
        Node::TableCell(TableCell{column: 0, row: 0, values: vec![
            Node::Text(Text{value: "new".to_string(), position: None})
        ], position: None})
    ]}),
    Node::TableRow(TableRow{values: vec![
        Node::TableCell(TableCell{column: 0, row: 0, values: vec![
            Node::Text(Text{value: "new".to_string(), position: None})
        ], position: None})
    ], position: None})
)]
#[case(
    &mut Node::Text(Text{value: "old".to_string(), position: None}),
    Node::Fragment(Fragment{values: vec![
        Node::Text(Text{value: "new".to_string(), position: None})
    ]}),
    Node::Text(Text{value: "old".to_string(), position: None})
)]
#[case(
    &mut Node::Blockquote(Blockquote{values: vec![
        Node::Text(Text{value: "text1".to_string(), position: None}),
        Node::Text(Text{value: "text2".to_string(), position: None})
    ], position: None}),
    Node::Fragment(Fragment{values: vec![
        Node::Text(Text{value: "new1".to_string(), position: None}),
        Node::Text(Text{value: "new2".to_string(), position: None})
    ]}),
    Node::Blockquote(Blockquote{values: vec![
        Node::Text(Text{value: "new1".to_string(), position: None}),
        Node::Text(Text{value: "new2".to_string(), position: None})
    ], position: None})
)]
#[case(
    &mut Node::Strong(Strong{values: vec![
        Node::Text(Text{value: "text1".to_string(), position: None}),
        Node::Text(Text{value: "text2".to_string(), position: None})
    ], position: None}),
    Node::Fragment(Fragment{values: vec![
        Node::Empty,
        Node::Text(Text{value: "new2".to_string(), position: None})
    ]}),
    Node::Strong(Strong{values: vec![
        Node::Text(Text{value: "text1".to_string(), position: None}),
        Node::Text(Text{value: "new2".to_string(), position: None})
    ], position: None})
)]
#[case(
    &mut Node::List(List{ marker: None,start: None, spread: false, index: 0, level: 0, checked: None, ordered: false, values: vec![
        Node::Text(Text{value: "text1".to_string(), position: None}),
        Node::Text(Text{value: "text2".to_string(), position: None})
    ], position: None}),
    Node::Fragment(Fragment{values: vec![
        Node::Text(Text{value: "new1".to_string(), position: None}),
        Node::Fragment(Fragment{values: Vec::new()})
    ]}),
    Node::List(List{ marker: None,start: None, spread: false, index: 0, level: 0, checked: None, ordered: false, values: vec![
        Node::Text(Text{value: "new1".to_string(), position: None}),
        Node::Text(Text{value: "text2".to_string(), position: None})
    ], position: None})
)]
fn test_apply_fragment(#[case] node: &mut Node, #[case] fragment: Node, #[case] expected: Node) {
    node.apply_fragment(fragment);
    assert_eq!(*node, expected);
}

#[rstest]
#[case(Node::Text(Text{value: "test".to_string(), position: None}),
   Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}},
   Node::Text(Text{value: "test".to_string(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}))]
#[case(Node::Code(Code{value: "code".to_string(), lang: None, fence: true, meta: None, position: None}),
   Position{start: Point{line: 1, column: 1}, end: Point{line: 3, column: 3}},
   Node::Code(Code{value: "code".to_string(), lang: None, fence: true, meta: None, position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 3, column: 3}})}))]
#[case(Node::List(List{ marker: None,start: None, spread: false, index: 0, level: 1, checked: None, ordered: false, values: vec![], position: None}),
   Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}},
   Node::List(List{ marker: None,start: None, spread: false, index: 0, level: 1, checked: None, ordered: false, values: vec![], position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}))]
#[case(Node::Definition(Definition{ident: "id".to_string(), url: Url::new(attr_keys::URL.to_string()), title: None, label: None, position: None}),
   Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 10}},
   Node::Definition(Definition{ident: "id".to_string(), url: Url::new(attr_keys::URL.to_string()), title: None, label: None, position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 10}})}))]
#[case(Node::Delete(Delete{values: vec![Node::Text(Text{value: "test".to_string(), position: None})], position: None}),
    Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}},
    Node::Delete(Delete{values: vec![Node::Text(Text{value: "test".to_string(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})})], position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}))]
#[case(Node::Emphasis(Emphasis{values: vec![Node::Text(Text{value: "test".to_string(), position: None})], position: None}),
    Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}},
    Node::Emphasis(Emphasis{values: vec![Node::Text(Text{value: "test".to_string(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})})], position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}))]
#[case(Node::Footnote(Footnote{ident: "id".to_string(), values: vec![Node::Text(Text{value: "test".to_string(), position: None})], position: None}),
    Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}},
    Node::Footnote(Footnote{ident: "id".to_string(), values: vec![Node::Text(Text{value: "test".to_string(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})})], position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}))]
#[case(Node::FootnoteRef(FootnoteRef{ident: "id".to_string(), label: Some(attr_keys::LABEL.to_string()), position: None}),
    Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}},
    Node::FootnoteRef(FootnoteRef{ident: "id".to_string(), label: Some(attr_keys::LABEL.to_string()), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}))]
#[case(Node::Html(Html{value: "<div>test</div>".to_string(), position: None}),
    Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 15}},
    Node::Html(Html{value: "<div>test</div>".to_string(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 15}})}))]
#[case(Node::Yaml(Yaml{value: "key: value".to_string(), position: None}),
    Position{start: Point{line: 1, column: 1}, end: Point{line: 3, column: 4}},
    Node::Yaml(Yaml{value: "key: value".to_string(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 3, column: 4}})}))]
#[case(Node::Toml(Toml{value: "key = \"value\"".to_string(), position: None}),
    Position{start: Point{line: 1, column: 1}, end: Point{line: 3, column: 4}},
    Node::Toml(Toml{value: "key = \"value\"".to_string(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 3, column: 4}})}))]
#[case(Node::Image(Image{alt: attr_keys::ALT.to_string(), url: attr_keys::URL.to_string(), title: None, position: None}),
    Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 12}},
    Node::Image(Image{alt: attr_keys::ALT.to_string(), url: attr_keys::URL.to_string(), title: None, position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 12}})}))]
#[case(Node::ImageRef(ImageRef{alt: attr_keys::ALT.to_string(), ident: "id".to_string(), label: None, position: None}),
    Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 10}},
    Node::ImageRef(ImageRef{alt: attr_keys::ALT.to_string(), ident: "id".to_string(), label: None, position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 10}})}))]
#[case(Node::CodeInline(CodeInline{value: "code".into(), position: None}),
    Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 7}},
    Node::CodeInline(CodeInline{value: "code".into(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 7}})}))]
#[case(Node::MathInline(MathInline{value: "x^2".into(), position: None}),
    Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}},
    Node::MathInline(MathInline{value: "x^2".into(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}))]
#[case(Node::Link(Link{url: Url::new(attr_keys::URL.to_string()), title: None, values: vec![Node::Text(Text{value: "text".to_string(), position: None})], position: None}),
    Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 10}},
    Node::Link(Link{url: Url::new(attr_keys::URL.to_string()), title: None, values: vec![Node::Text(Text{value: "text".to_string(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 10}})})], position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 10}})}))]
#[case(Node::LinkRef(LinkRef{ident: "id".to_string(), values: vec![Node::Text(Text{value: "text".to_string(), position: None})], label: None, position: None}),
    Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 10}},
    Node::LinkRef(LinkRef{ident: "id".to_string(), values: vec![Node::Text(Text{value: "text".to_string(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 10}})})], label: None, position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 10}})}))]
#[case(Node::Math(Math{value: "x^2".to_string(), position: None}),
    Position{start: Point{line: 1, column: 1}, end: Point{line: 3, column: 3}},
    Node::Math(Math{value: "x^2".to_string(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 3, column: 3}})}))]
#[case(Node::TableCell(TableCell{column: 0, row: 0, values: vec![Node::Text(Text{value: "cell".to_string(), position: None})], position: None}),
    Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 6}},
    Node::TableCell(TableCell{column: 0, row: 0, values: vec![Node::Text(Text{value: "cell".to_string(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 6}})})], position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 6}})}))]
#[case(Node::TableAlign(TableAlign{align: vec![TableAlignKind::Left, TableAlignKind::Right], position: None}),
    Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 15}},
    Node::TableAlign(TableAlign{align: vec![TableAlignKind::Left, TableAlignKind::Right], position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 15}})}))]
#[case(Node::MdxFlowExpression(MdxFlowExpression{value: "test".into(), position: None}),
    Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 7}},
    Node::MdxFlowExpression(MdxFlowExpression{value: "test".into(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 7}})}))]
#[case(Node::MdxTextExpression(MdxTextExpression{value: "test".into(), position: None}),
    Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 7}},
    Node::MdxTextExpression(MdxTextExpression{value: "test".into(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 7}})}))]
#[case(Node::MdxJsEsm(MdxJsEsm{value: "import React from 'react'".into(), position: None}),
    Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 25}},
    Node::MdxJsEsm(MdxJsEsm{value: "import React from 'react'".into(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 25}})}))]
#[case(Node::MdxJsxTextElement(MdxJsxTextElement{name: Some("span".into()), attributes: Vec::new(), children: vec![Node::Text(Text{value: "text".to_string(), position: None})], position: None}),
    Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 20}},
    Node::MdxJsxTextElement(MdxJsxTextElement{name: Some("span".into()), attributes: Vec::new(), children: vec![Node::Text(Text{value: "text".to_string(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 20}})})], position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 20}})}))]
#[case(Node::Break(Break{position: None}),
    Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 2}},
    Node::Break(Break{position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 2}})}))]
#[case(Node::Empty,
   Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}},
   Node::Empty)]
#[case(Node::Fragment(Fragment{values: vec![
       Node::Text(Text{value: "test1".to_string(), position: None}),
       Node::Text(Text{value: "test2".to_string(), position: None})
   ]}),
   Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 10}},
   Node::Fragment(Fragment{values: vec![
       Node::Text(Text{value: "test1".to_string(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 10}})}),
       Node::Text(Text{value: "test2".to_string(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 10}})})
   ]}))]
#[case(Node::Blockquote(Blockquote{values: vec![
    Node::Text(Text{value: "test".to_string(), position: None})], position: None}),
    Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}},
    Node::Blockquote(Blockquote{values: vec![
        Node::Text(Text{value: "test".to_string(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})})
    ], position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}))]
#[case(Node::Heading(Heading{depth: HeadingDepth::H1, values: vec![
        Node::Text(Text{value: "test".to_string(), position: None})], position: None}),
        Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}},
        Node::Heading(Heading{depth: HeadingDepth::H1, values: vec![
            Node::Text(Text{value: "test".to_string(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})})
        ], position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}))]
#[case(Node::Strong(Strong{values: vec![
        Node::Text(Text{value: "test".to_string(), position: None})], position: None}),
        Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}},
        Node::Strong(Strong{values: vec![
            Node::Text(Text{value: "test".to_string(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})})
        ], position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 5}})}))]
#[case(Node::TableRow(TableRow{values: vec![
        Node::TableCell(TableCell{column: 0, row: 0, values: vec![
            Node::Text(Text{value: "cell".to_string(), position: None})
        ], position: None})
    ], position: None}),
        Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 10}},
        Node::TableRow(TableRow{values: vec![
            Node::TableCell(TableCell{column: 0, row: 0, values: vec![
                Node::Text(Text{value: "cell".to_string(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 10}})})
            ], position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 10}})})
        ], position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 10}})}))]
#[case(Node::MdxJsxFlowElement(MdxJsxFlowElement{
        name: Some("div".to_string()),
        attributes: Vec::new(),
        children: vec![Node::Text(Text{value: "content".to_string(), position: None})],
        position: None
    }),
        Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 20}},
        Node::MdxJsxFlowElement(MdxJsxFlowElement{
            name: Some("div".to_string()),
            attributes: Vec::new(),
            children: vec![Node::Text(Text{value: "content".to_string(), position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 20}})})],
            position: Some(Position{start: Point{line: 1, column: 1}, end: Point{line: 1, column: 20}})
        }))]
fn test_set_position(#[case] mut node: Node, #[case] position: Position, #[case] expected: Node) {
    node.set_position(Some(position));
    assert_eq!(node, expected);
}

fn some_position() -> Option<Position> {
    Some(Position {
        start: Point { line: 1, column: 1 },
        end: Point { line: 1, column: 5 },
    })
}

#[test]
fn test_strip_positions_top_level() {
    let mut node = Node::Text(Text {
        value: "test".to_string(),
        position: some_position(),
    });
    node.strip_positions();
    assert_eq!(node.position(), None);
}

#[test]
fn test_strip_positions_recurses_into_children() {
    let mut node = Node::Blockquote(Blockquote {
        values: vec![Node::Strong(Strong {
            values: vec![Node::Text(Text {
                value: "test".to_string(),
                position: some_position(),
            })],
            position: some_position(),
        })],
        position: some_position(),
    });

    node.strip_positions();

    assert_eq!(node.position(), None);
    let strong = &node.children()[0];
    assert_eq!(strong.position(), None);
    assert_eq!(strong.children()[0].position(), None);
}

#[test]
fn test_clear_text_position_at_leaf_only() {
    let mut node = Node::Heading(Heading {
        depth: HeadingDepth::H1,
        values: vec![Node::Text(Text {
            value: "title".to_string(),
            position: some_position(),
        })],
        position: some_position(),
    });

    node.clear_text_position_at(0);

    assert_eq!(node.position(), some_position(), "outer node keeps its position");
    assert_eq!(node.children()[0].position(), None, "escaped leaf loses its position");
}

#[test]
fn test_clear_text_position_at_selected_index() {
    let mut node = Node::List(List {
        marker: None,
        start: None,
        spread: false,
        index: 0,
        level: 0,
        checked: None,
        ordered: false,
        values: vec![
            Node::Text(Text {
                value: "a".to_string(),
                position: some_position(),
            }),
            Node::Text(Text {
                value: "b".to_string(),
                position: some_position(),
            }),
        ],
        position: some_position(),
    });

    node.clear_text_position_at(1);

    assert_eq!(
        node.children()[0].position(),
        some_position(),
        "untouched sibling keeps its position"
    );
    assert_eq!(node.children()[1].position(), None, "selected leaf loses its position");
}

#[rstest]
#[case(Node::List(List{ marker: None,start: None, spread: false, index: 0, level: 0, checked: None, ordered: false, values: vec!["test".to_string().into()], position: None}), true)]
#[case(Node::List(List{ marker: None,start: None, spread: false, index: 1, level: 2, checked: Some(true), ordered: false, values: vec!["test".to_string().into()], position: None}), true)]
#[case(Node::Text(Text{value: "test".to_string(), position: None}), false)]
fn test_is_list(#[case] node: Node, #[case] expected: bool) {
    assert_eq!(node.is_list(), expected);
}

#[rstest]
#[case(Url::new("https://example.com".to_string()), RenderOptions{link_url_style: UrlSurroundStyle::None, ..Default::default()}, "https://example.com")]
#[case(Url::new("https://example.com".to_string()), RenderOptions{link_url_style: UrlSurroundStyle::Angle, ..Default::default()}, "<https://example.com>")]
#[case(Url::new("".to_string()), RenderOptions::default(), "<>")]
fn test_url_to_string_with(#[case] url: Url, #[case] options: RenderOptions, #[case] expected: &str) {
    assert_eq!(url.to_string_with(&options), expected);
}

#[rstest]
#[case(Title::new(attr_keys::TITLE.to_string()), RenderOptions::default(), "\"title\"")]
#[case(Title::new(r#"title with "quotes""#.to_string()), RenderOptions::default(), r#""title with \"quotes\"""#)]
#[case(Title::new("title with spaces".to_string()), RenderOptions::default(), "\"title with spaces\"")]
#[case(Title::new("".to_string()), RenderOptions::default(), "\"\"")]
#[case(Title::new(attr_keys::TITLE.to_string()), RenderOptions{link_title_style: TitleSurroundStyle::Single, ..Default::default()}, "'title'")]
#[case(Title::new("title with 'quotes'".to_string()), RenderOptions{link_title_style: TitleSurroundStyle::Double, ..Default::default()}, "\"title with 'quotes'\"")]
#[case(Title::new(attr_keys::TITLE.to_string()), RenderOptions{link_title_style: TitleSurroundStyle::Paren, ..Default::default()}, "(title)")]
fn test_title_to_string_with(#[case] title: Title, #[case] options: RenderOptions, #[case] expected: &str) {
    assert_eq!(title.to_string_with(&options), expected);
}

#[rstest]
#[case(Node::Fragment(Fragment{values: vec![]}), true)]
#[case(Node::Fragment(Fragment{values: vec![
    Node::Text(Text{value: "not_empty".to_string(), position: None})
]}), false)]
#[case(Node::Fragment(Fragment{values: vec![
    Node::Fragment(Fragment{values: vec![]}),
    Node::Fragment(Fragment{values: vec![]})
]}), true)]
#[case(Node::Fragment(Fragment{values: vec![
    Node::Fragment(Fragment{values: vec![]}),
    Node::Text(Text{value: "not_empty".to_string(), position: None})
]}), false)]
#[case(Node::Text(Text{value: "not_fragment".to_string(), position: None}), false)]
fn test_is_empty_fragment(#[case] node: Node, #[case] expected: bool) {
    assert_eq!(node.is_empty_fragment(), expected);
}

#[rstest]
#[case::footnote(Node::Footnote(Footnote{ident: "id".to_string(), values: Vec::new(), position: None}), attr_keys::IDENT, Some(AttrValue::String("id".to_string())))]
#[case::footnote(Node::Footnote(Footnote{ident: "id".to_string(), values: Vec::new(), position: None}), "unknown", None)]
#[case::html(Node::Html(Html{value: "<div>test</div>".to_string(), position: None}), attr_keys::VALUE, Some(AttrValue::String("<div>test</div>".to_string())))]
#[case::html(Node::Html(Html{value: "<div>test</div>".to_string(), position: None}), "unknown", None)]
#[case::text(Node::Text(Text{value: "text".to_string(), position: None}), attr_keys::VALUE, Some(AttrValue::String("text".to_string())))]
#[case::text(Node::Text(Text{value: "text".to_string(), position: None}), "unknown", None)]
#[case::code(Node::Code(Code{value: "code".to_string(), lang: Some("rust".to_string()), meta: Some("meta".to_string()), fence: true, position: None}), attr_keys::VALUE, Some(AttrValue::String("code".to_string())))]
#[case::code(Node::Code(Code{value: "code".to_string(), lang: Some("rust".to_string()), meta: Some("meta".to_string()), fence: true, position: None}), attr_keys::LANG, Some(AttrValue::String("rust".to_string())))]
#[case::code(Node::Code(Code{value: "code".to_string(), lang: Some("rust".to_string()), meta: Some("meta".to_string()), fence: true, position: None}), "meta", Some(AttrValue::String("meta".to_string())))]
#[case::code(Node::Code(Code{value: "code".to_string(), lang: Some("rust".to_string()), meta: Some("meta".to_string()), fence: true, position: None}), attr_keys::FENCE, Some(AttrValue::Boolean(true)))]
#[case::code(Node::Code(Code{value: "code".to_string(), lang: None, meta: None, fence: false, position: None}), attr_keys::FENCE, Some(AttrValue::Boolean(false)))]
#[case::code_inline(Node::CodeInline(CodeInline{value: "inline".into(), position: None}), attr_keys::VALUE, Some(AttrValue::String("inline".to_string())))]
#[case::math_inline(Node::MathInline(MathInline{value: "math".into(), position: None}), attr_keys::VALUE, Some(AttrValue::String("math".to_string())))]
#[case::math(Node::Math(Math{value: "math".to_string(), position: None}), attr_keys::VALUE, Some(AttrValue::String("math".to_string())))]
#[case::yaml(Node::Yaml(Yaml{value: "yaml".to_string(), position: None}), attr_keys::VALUE, Some(AttrValue::String("yaml".to_string())))]
#[case::toml(Node::Toml(Toml{value: "toml".to_string(), position: None}), attr_keys::VALUE, Some(AttrValue::String("toml".to_string())))]
#[case::image(Node::Image(Image{alt: attr_keys::ALT.to_string(), url: attr_keys::URL.to_string(), title: Some(attr_keys::TITLE.to_string()), position: None}), attr_keys::ALT, Some(AttrValue::String(attr_keys::ALT.to_string())))]
#[case::image(Node::Image(Image{alt: attr_keys::ALT.to_string(), url: attr_keys::URL.to_string(), title: Some(attr_keys::TITLE.to_string()), position: None}), attr_keys::URL, Some(AttrValue::String(attr_keys::URL.to_string())))]
#[case::image(Node::Image(Image{alt: attr_keys::ALT.to_string(), url: attr_keys::URL.to_string(), title: Some(attr_keys::TITLE.to_string()), position: None}), attr_keys::TITLE, Some(AttrValue::String(attr_keys::TITLE.to_string())))]
#[case::image_ref(Node::ImageRef(ImageRef{alt: attr_keys::ALT.to_string(), ident: "id".to_string(), label: Some(attr_keys::LABEL.to_string()), position: None}), attr_keys::ALT, Some(AttrValue::String(attr_keys::ALT.to_string())))]
#[case::image_ref(Node::ImageRef(ImageRef{alt: attr_keys::ALT.to_string(), ident: "id".to_string(), label: Some(attr_keys::LABEL.to_string()), position: None}), attr_keys::IDENT, Some(AttrValue::String("id".to_string())))]
#[case::image_ref(Node::ImageRef(ImageRef{alt: attr_keys::ALT.to_string(), ident: "id".to_string(), label: Some(attr_keys::LABEL.to_string()), position: None}), attr_keys::LABEL, Some(AttrValue::String(attr_keys::LABEL.to_string())))]
#[case::link(Node::Link(Link{url: Url::new(attr_keys::URL.to_string()), title: Some(Title::new(attr_keys::TITLE.to_string())), values: Vec::new(), position: None}), attr_keys::URL, Some(AttrValue::String(attr_keys::URL.to_string())))]
#[case::link(Node::Link(Link{url: Url::new(attr_keys::URL.to_string()), title: Some(Title::new(attr_keys::TITLE.to_string())), values: Vec::new(), position: None}), attr_keys::TITLE, Some(AttrValue::String(attr_keys::TITLE.to_string())))]
#[case::link_ref(Node::LinkRef(LinkRef{ident: "id".to_string(), values: Vec::new(), label: Some(attr_keys::LABEL.to_string()), position: None}), attr_keys::IDENT, Some(AttrValue::String("id".to_string())))]
#[case::link_ref(Node::LinkRef(LinkRef{ident: "id".to_string(), values: Vec::new(), label: Some(attr_keys::LABEL.to_string()), position: None}), attr_keys::LABEL, Some(AttrValue::String(attr_keys::LABEL.to_string())))]
#[case::footnote_ref(Node::FootnoteRef(FootnoteRef{ident: "id".to_string(), label: Some(attr_keys::LABEL.to_string()), position: None}), attr_keys::IDENT, Some(AttrValue::String("id".to_string())))]
#[case::footnote_ref(Node::FootnoteRef(FootnoteRef{ident: "id".to_string(), label: Some(attr_keys::LABEL.to_string()), position: None}), attr_keys::LABEL, Some(AttrValue::String(attr_keys::LABEL.to_string())))]
#[case::definition(Node::Definition(Definition{ident: "id".to_string(), url: Url::new(attr_keys::URL.to_string()), title: Some(Title::new(attr_keys::TITLE.to_string())), label: Some(attr_keys::LABEL.to_string()), position: None}), attr_keys::IDENT, Some(AttrValue::String("id".to_string())))]
#[case::definition(Node::Definition(Definition{ident: "id".to_string(), url: Url::new(attr_keys::URL.to_string()), title: Some(Title::new(attr_keys::TITLE.to_string())), label: Some(attr_keys::LABEL.to_string()), position: None}), attr_keys::URL, Some(AttrValue::String(attr_keys::URL.to_string())))]
#[case::definition(Node::Definition(Definition{ident: "id".to_string(), url: Url::new(attr_keys::URL.to_string()), title: Some(Title::new(attr_keys::TITLE.to_string())), label: Some(attr_keys::LABEL.to_string()), position: None}), attr_keys::TITLE, Some(AttrValue::String(attr_keys::TITLE.to_string())))]
#[case::definition(Node::Definition(Definition{ident: "id".to_string(), url: Url::new(attr_keys::URL.to_string()), title: Some(Title::new(attr_keys::TITLE.to_string())), label: Some(attr_keys::LABEL.to_string()), position: None}), attr_keys::LABEL, Some(AttrValue::String(attr_keys::LABEL.to_string())))]
#[case::heading(Node::Heading(Heading{depth: HeadingDepth::H3, values: Vec::new(), position: None}), "depth", Some(AttrValue::Integer(3)))]
#[case::list(Node::List(List{ marker: None,start: None, spread: false, index: 2, level: 1, checked: Some(true), ordered: true, values: Vec::new(), position: None}), "index", Some(AttrValue::Integer(2)))]
#[case::list(Node::List(List{ marker: None,start: None, spread: false, index: 2, level: 1, checked: Some(true), ordered: true, values: Vec::new(), position: None}), "level", Some(AttrValue::Integer(1)))]
#[case::list(Node::List(List{ marker: None,start: None, spread: false, index: 2, level: 1, checked: Some(true), ordered: true, values: Vec::new(), position: None}), "ordered", Some(AttrValue::Boolean(true)))]
#[case::list(Node::List(List{ marker: None,start: None, spread: false, index: 2, level: 1, checked: Some(true), ordered: true, values: Vec::new(), position: None}), attr_keys::CHECKED, Some(AttrValue::Boolean(true)))]
#[case::table_cell(Node::TableCell(TableCell{column: 1, row: 2, values: Vec::new(), position: None}), "column", Some(AttrValue::Integer(1)))]
#[case::table_cell(Node::TableCell(TableCell{column: 1, row: 2, values: Vec::new(), position: None}), "row", Some(AttrValue::Integer(2)))]
#[case::table_align(Node::TableAlign(TableAlign{align: vec![TableAlignKind::Left, TableAlignKind::Right], position: None}), "align", Some(AttrValue::String(":---,---:".to_string())))]
#[case::mdx_flow_expression(Node::MdxFlowExpression(MdxFlowExpression{value: "expr".into(), position: None}), attr_keys::VALUE, Some(AttrValue::String("expr".to_string())))]
#[case::mdx_flow_expression(Node::MdxTextExpression(MdxTextExpression{value: "expr".into(), position: None}), attr_keys::VALUE, Some(AttrValue::String("expr".to_string())))]
#[case::mdx_js_esm(Node::MdxJsEsm(MdxJsEsm{value: "esm".into(), position: None}), attr_keys::VALUE, Some(AttrValue::String("esm".to_string())))]
#[case::mdx_jsx_flow_element(Node::MdxJsxFlowElement(MdxJsxFlowElement{name: Some("div".to_string()), attributes: Vec::new(), children: Vec::new(), position: None}), attr_keys::NAME, Some(AttrValue::String("div".to_string())))]
#[case::mdx_jsx_flow_element(Node::MdxJsxTextElement(MdxJsxTextElement{name: Some("span".into()), attributes: Vec::new(), children: Vec::new(), position: None}), attr_keys::NAME, Some(AttrValue::String("span".to_string())))]
#[case::break_(Node::Break(Break{position: None}), attr_keys::VALUE, None)]
#[case::horizontal_rule(Node::HorizontalRule(HorizontalRule{ marker: None,position: None}), attr_keys::VALUE, None)]
#[case::fragment(Node::Fragment(Fragment{values: Vec::new()}), attr_keys::VALUE, Some(AttrValue::String("".to_string())))]
#[case::heading(Node::Heading(Heading{depth: HeadingDepth::H1, values: vec![Node::Text(Text{value: "heading text".to_string(), position: None})], position: None}), attr_keys::VALUE, Some(AttrValue::String("heading text".to_string())))]
#[case::heading(Node::Heading(Heading{depth: HeadingDepth::H2, values: vec![], position: None}), attr_keys::VALUE, Some(AttrValue::String("".to_string())))]
#[case::heading(Node::Heading(Heading{depth: HeadingDepth::H3, values: vec![
    Node::Text(Text{value: "first".to_string(), position: None}),
    Node::Text(Text{value: "second".to_string(), position: None}),
], position: None}), attr_keys::VALUE, Some(AttrValue::String("firstsecond".to_string())))]
#[case(
    Node::List(List { marker: None,
        index: 0,
        level: 1,
        checked: None,
        ordered: false,
        start: None, spread: false,
        values: vec![
            Node::Text(Text { value: "item1".to_string(), position: None }),
            Node::Text(Text { value: "item2".to_string(), position: None }),
        ],
        position: None,
    }),
    attr_keys::VALUE,
    Some(AttrValue::String("item1item2".to_string()))
)]
#[case(
    Node::TableCell(TableCell {
        column: 1,
        row: 2,
        values: vec![Node::Text(Text {
            value: "cell_value".to_string(),
            position: None,
        })],
        position: None,
    }),
    attr_keys::VALUE,
    Some(AttrValue::String("cell_value".to_string()))
)]
#[case::footnote(
    Node::Footnote(Footnote {
        ident: "id".to_string(),
        values: vec![Node::Text(Text {
            value: "footnote value".to_string(),
            position: None,
        })],
        position: None,
    }),
    attr_keys::VALUE,
    Some(AttrValue::String("footnote value".to_string()))
)]
#[case::link(
    Node::Link(Link {
        url: Url::new("https://example.com".to_string()),
        title: Some(Title::new("Example".to_string())),
        values: vec![Node::Text(Text {
            value: "link text".to_string(),
            position: None,
        })],
        position: None,
    }),
    attr_keys::VALUE,
    Some(AttrValue::String("link text".to_string()))
)]
#[case::empty(Node::Empty, attr_keys::VALUE, None)]
#[case::heading(
    Node::Heading(Heading {
        depth: HeadingDepth::H1,
        values: vec![
        Node::Text(Text {
            value: "child1".to_string(),
            position: None,
        }),
        Node::Text(Text {
            value: "child2".to_string(),
            position: None,
        }),
        ],
        position: None,
    }),
    attr_keys::CHILDREN,
    Some(AttrValue::Array(vec![
        Node::Text(Text {
        value: "child1".to_string(),
        position: None,
        }),
        Node::Text(Text {
        value: "child2".to_string(),
        position: None,
        }),
    ]))
    )]
#[case::list(
    Node::List(List { marker: None,
        index: 0,
        level: 1,
        checked: None,
        ordered: false,
        start: None, spread: false,
        values: vec![
        Node::Text(Text {
            value: "item1".to_string(),
            position: None,
        }),
        ],
        position: None,
    }),
    attr_keys::CHILDREN,
    Some(AttrValue::Array(vec![
        Node::Text(Text {
        value: "item1".to_string(),
        position: None,
        }),
    ]))
    )]
#[case::blockquote(
    Node::Blockquote(Blockquote {
        values: vec![
        Node::Text(Text {
            value: "quote".to_string(),
            position: None,
        }),
        ],
        position: None,
    }),
    attr_keys::VALUES,
    Some(AttrValue::Array(vec![
        Node::Text(Text {
        value: "quote".to_string(),
        position: None,
        }),
    ]))
    )]
#[case::link(
    Node::Link(Link {
        url: Url::new(attr_keys::URL.to_string()),
        title: None,
        values: vec![
        Node::Text(Text {
            value: "link".to_string(),
            position: None,
        }),
        ],
        position: None,
    }),
    attr_keys::VALUES,
    Some(AttrValue::Array(vec![
        Node::Text(Text {
        value: "link".to_string(),
        position: None,
        }),
    ]))
    )]
#[case::table_cell(
    Node::TableCell(TableCell {
        column: 0,
        row: 0,
        values: vec![
        Node::Text(Text {
            value: "cell".to_string(),
            position: None,
        }),
        ],
        position: None,
    }),
    attr_keys::CHILDREN,
    Some(AttrValue::Array(vec![
        Node::Text(Text {
        value: "cell".to_string(),
        position: None,
        }),
    ]))
    )]
#[case::strong(
    Node::Strong(Strong {
        values: vec![
        Node::Text(Text {
            value: "bold".to_string(),
            position: None,
        }),
        ],
        position: None,
    }),
    attr_keys::CHILDREN,
    Some(AttrValue::Array(vec![
        Node::Text(Text {
        value: "bold".to_string(),
        position: None,
        }),
    ]))
    )]
#[case::em(
    Node::Emphasis(Emphasis {
        values: vec![],
        position: None,
    }),
    attr_keys::CHILDREN,
    Some(AttrValue::Array(vec![]))
    )]
fn test_attr(#[case] node: Node, #[case] attr: &str, #[case] expected: Option<AttrValue>) {
    assert_eq!(node.attr(attr), expected);
}

#[rstest]
#[case::heading_with_position(
    Node::Heading(Heading {
        depth: HeadingDepth::H1,
        values: vec![],
        position: Some(Position {
            start: Point { line: 3, column: 1 },
            end: Point { line: 5, column: 4 },
        }),
    }),
    attr_keys::LINE,
    Some(AttrValue::Integer(3))
)]
#[case::code_end_line(
    Node::Code(Code {
        value: "x".to_string(),
        lang: None,
        meta: None,
        fence: true,
        position: Some(Position {
            start: Point { line: 3, column: 1 },
            end: Point { line: 5, column: 4 },
        }),
    }),
    attr_keys::END_LINE,
    Some(AttrValue::Integer(5))
)]
#[case::synthetic_node_has_no_line(
    Node::Heading(Heading {
        depth: HeadingDepth::H1,
        values: vec![],
        position: None,
    }),
    attr_keys::LINE,
    None
)]
#[case::synthetic_node_has_no_end_line(
    Node::Heading(Heading {
        depth: HeadingDepth::H1,
        values: vec![],
        position: None,
    }),
    attr_keys::END_LINE,
    None
)]
fn test_line_attr(#[case] node: Node, #[case] attr: &str, #[case] expected: Option<AttrValue>) {
    assert_eq!(node.attr(attr), expected);
}

#[rstest]
#[case::heading(
    Node::Heading(Heading{depth: HeadingDepth::H1, values: vec![], position: None}),
    vec![Node::Text(Text{value: "child".to_string(), position: None})],
    Node::Heading(Heading{depth: HeadingDepth::H1, values: vec![Node::Text(Text{value: "child".to_string(), position: None})], position: None})
)]
#[case::list(
    Node::List(List{ marker: None,start: None, spread: false, index: 0, level: 0, checked: None, ordered: false, values: vec![], position: None}),
    vec![Node::Text(Text{value: "item".to_string(), position: None})],
    Node::List(List{ marker: None,start: None, spread: false, index: 0, level: 0, checked: None, ordered: false, values: vec![Node::Text(Text{value: "item".to_string(), position: None})], position: None})
)]
#[case::blockquote(
    Node::Blockquote(Blockquote{values: vec![], position: None}),
    vec![Node::Text(Text{value: "quote".to_string(), position: None})],
    Node::Blockquote(Blockquote{values: vec![Node::Text(Text{value: "quote".to_string(), position: None})], position: None})
)]
#[case::link(
    Node::Link(Link{url: Url::new(attr_keys::URL.to_string()), title: None, values: vec![], position: None}),
    vec![Node::Text(Text{value: "link".to_string(), position: None})],
    Node::Link(Link{url: Url::new(attr_keys::URL.to_string()), title: None, values: vec![Node::Text(Text{value: "link".to_string(), position: None})], position: None})
)]
#[case::footnote(
    Node::Footnote(Footnote{ident: "1".to_string(), values: vec![], position: None}),
    vec![Node::Text(Text{value: "note".to_string(), position: None})],
    Node::Footnote(Footnote{ident: "1".to_string(), values: vec![Node::Text(Text{value: "note".to_string(), position: None})], position: None})
)]
#[case::table_cell(
    Node::TableCell(TableCell{column: 0, row: 0, values: vec![], position: None}),
    vec![Node::Text(Text{value: "cell".to_string(), position: None})],
    Node::TableCell(TableCell{column: 0, row: 0, values: vec![Node::Text(Text{value: "cell".to_string(), position: None})], position: None})
)]
#[case::table_row(
    Node::TableRow(TableRow{values: vec![], position: None}),
    vec![Node::Text(Text{value: "row".to_string(), position: None})],
    Node::TableRow(TableRow{values: vec![Node::Text(Text{value: "row".to_string(), position: None})], position: None})
)]
#[case::strong(
    Node::Strong(Strong{values: vec![], position: None}),
    vec![Node::Text(Text{value: "bold".to_string(), position: None})],
    Node::Strong(Strong{values: vec![Node::Text(Text{value: "bold".to_string(), position: None})], position: None})
)]
#[case::delete(
    Node::Delete(Delete{values: vec![], position: None}),
    vec![Node::Text(Text{value: "del".to_string(), position: None})],
    Node::Delete(Delete{values: vec![Node::Text(Text{value: "del".to_string(), position: None})], position: None})
)]
#[case::emphasis(
    Node::Emphasis(Emphasis{values: vec![], position: None}),
    vec![Node::Text(Text{value: "em".to_string(), position: None})],
    Node::Emphasis(Emphasis{values: vec![Node::Text(Text{value: "em".to_string(), position: None})], position: None})
)]
#[case::fragment(
    Node::Fragment(Fragment{values: vec![]}),
    vec![Node::Text(Text{value: "frag".to_string(), position: None})],
    Node::Fragment(Fragment{values: vec![Node::Text(Text{value: "frag".to_string(), position: None})]})
)]
#[case::mdx_jsx_flow_element(
    Node::MdxJsxFlowElement(MdxJsxFlowElement{name: Some("div".to_string()), attributes: Vec::new(), children: vec![], position: None}),
    vec![Node::Text(Text{value: "mdx".to_string(), position: None})],
    Node::MdxJsxFlowElement(MdxJsxFlowElement{name: Some("div".to_string()), attributes: Vec::new(), children: vec![Node::Text(Text{value: "mdx".to_string(), position: None})], position: None})
)]
#[case::mdx_jsx_text_element(
    Node::MdxJsxTextElement(MdxJsxTextElement{name: Some("span".into()), attributes: Vec::new(), children: vec![], position: None}),
    vec![Node::Text(Text{value: "mdx".to_string(), position: None})],
    Node::MdxJsxTextElement(MdxJsxTextElement{name: Some("span".into()), attributes: Vec::new(), children: vec![Node::Text(Text{value: "mdx".to_string(), position: None})], position: None})
)]
#[case::leaf_node_is_noop(
    Node::Text(Text{value: "leaf".to_string(), position: None}),
    vec![Node::Text(Text{value: "ignored".to_string(), position: None})],
    Node::Text(Text{value: "leaf".to_string(), position: None})
)]
fn test_set_children(#[case] mut node: Node, #[case] children: Vec<Node>, #[case] expected: Node) {
    node.set_children(children.clone());
    assert_eq!(node, expected);
    assert_eq!(
        node.children(),
        if matches!(expected, Node::Text(_)) {
            Vec::new()
        } else {
            children
        }
    );
}

#[rstest]
#[case(
    Node::Text(Text{value: "old".to_string(), position: None}),
    attr_keys::VALUE,
    "new",
    Node::Text(Text{value: "new".to_string(), position: None})
)]
#[case(
    Node::Code(Code{value: "old".to_string(), lang: Some("rust".to_string()), fence: true, meta: None, position: None}),
    attr_keys::VALUE,
    "new_code",
    Node::Code(Code{value: "new_code".to_string(), lang: Some("rust".to_string()), fence: true, meta: None, position: None})
)]
#[case(
    Node::Code(Code{value: "code".to_string(), lang: Some("rust".to_string()), fence: true, meta: None, position: None}),
    attr_keys::LANG,
    "python",
    Node::Code(Code{value: "code".to_string(), lang: Some("python".to_string()), fence: true, meta: None, position: None})
)]
#[case(
    Node::Code(Code{value: "code".to_string(), lang: None, fence: false, meta: None, position: None}),
    attr_keys::FENCE,
    "true",
    Node::Code(Code{value: "code".to_string(), lang: None, fence: true, meta: None, position: None})
)]
#[case(
    Node::Image(Image{alt: attr_keys::ALT.to_string(), url: attr_keys::URL.to_string(), title: None, position: None}),
    attr_keys::ALT,
    "new_alt",
    Node::Image(Image{alt: "new_alt".to_string(), url: attr_keys::URL.to_string(), title: None, position: None})
)]
#[case(
    Node::Image(Image{alt: attr_keys::ALT.to_string(), url: attr_keys::URL.to_string(), title: None, position: None}),
    attr_keys::URL,
    "new_url",
    Node::Image(Image{alt: attr_keys::ALT.to_string(), url: "new_url".to_string(), title: None, position: None})
)]
#[case(
    Node::Image(Image{alt: attr_keys::ALT.to_string(), url: attr_keys::URL.to_string(), title: Some(attr_keys::TITLE.to_string()), position: None}),
    attr_keys::TITLE,
    "new_title",
    Node::Image(Image{alt: attr_keys::ALT.to_string(), url: attr_keys::URL.to_string(), title: Some("new_title".to_string()), position: None})
)]
#[case(
    Node::Heading(Heading{depth: HeadingDepth::H2, values: vec![], position: None}),
    "depth",
    "3",
    Node::Heading(Heading{depth: HeadingDepth::H3, values: vec![], position: None})
)]
#[case(
    Node::List(List{ marker: None,start: None, spread: false, index: 1, level: 2, checked: Some(true), ordered: false, values: vec![], position: None}),
    attr_keys::CHECKED,
    "false",
    Node::List(List{ marker: None,start: None, spread: false, index: 1, level: 2, checked: Some(false), ordered: false, values: vec![], position: None})
)]
#[case(
    Node::List(List{ marker: None,start: None, spread: false, index: 1, level: 2, checked: Some(true), ordered: false, values: vec![], position: None}),
    "ordered",
    "true",
    Node::List(List{ marker: None,start: None, spread: false, index: 1, level: 2, checked: Some(true), ordered: true, values: vec![], position: None})
)]
#[case(
    Node::TableCell(TableCell{column: 1, row: 2, values: vec![], position: None}),
    "column",
    "3",
    Node::TableCell(TableCell{column: 3, row: 2, values: vec![], position: None})
)]
#[case(
    Node::TableCell(TableCell{column: 1, row: 2, values: vec![], position: None}),
    "row",
    "5",
    Node::TableCell(TableCell{column: 1, row: 5, values: vec![], position: None})
)]
#[case(
    Node::Definition(Definition{ident: "id".to_string(), url: Url::new(attr_keys::URL.to_string()), title: None, label: None, position: None}),
    attr_keys::IDENT,
    "new_id",
    Node::Definition(Definition{ident: "new_id".to_string(), url: Url::new(attr_keys::URL.to_string()), title: None, label: None, position: None})
)]
#[case(
    Node::Definition(Definition{ident: "id".to_string(), url: Url::new(attr_keys::URL.to_string()), title: None, label: None, position: None}),
    attr_keys::URL,
    "new_url",
    Node::Definition(Definition{ident: "id".to_string(), url: Url::new("new_url".to_string()), title: None, label: None, position: None})
)]
#[case(
    Node::Definition(Definition{ident: "id".to_string(), url: Url::new(attr_keys::URL.to_string()), title: None, label: None, position: None}),
    attr_keys::LABEL,
    "new_label",
    Node::Definition(Definition{ident: "id".to_string(), url: Url::new(attr_keys::URL.to_string()), title: None, label: Some("new_label".to_string()), position: None})
)]
#[case(
    Node::Definition(Definition{ident: "id".to_string(), url: Url::new(attr_keys::URL.to_string()), title: None, label: None, position: None}),
    attr_keys::TITLE,
    "new_title",
    Node::Definition(Definition{ident: "id".to_string(), url: Url::new(attr_keys::URL.to_string()), title: Some(Title::new("new_title".to_string())), label: None, position: None})
)]
#[case(
    Node::ImageRef(ImageRef{alt: attr_keys::ALT.to_string(), ident: "id".to_string(), label: Some(attr_keys::LABEL.to_string()), position: None}),
    attr_keys::ALT,
    "new_alt",
    Node::ImageRef(ImageRef{alt: "new_alt".to_string(), ident: "id".to_string(), label: Some(attr_keys::LABEL.to_string()), position: None})
)]
#[case(
    Node::ImageRef(ImageRef{alt: attr_keys::ALT.to_string(), ident: "id".to_string(), label: Some(attr_keys::LABEL.to_string()), position: None}),
    attr_keys::IDENT,
    "new_id",
    Node::ImageRef(ImageRef{alt: attr_keys::ALT.to_string(), ident: "new_id".to_string(), label: Some(attr_keys::LABEL.to_string()), position: None})
)]
#[case(
    Node::ImageRef(ImageRef{alt: attr_keys::ALT.to_string(), ident: "id".to_string(), label: Some(attr_keys::LABEL.to_string()), position: None}),
    attr_keys::LABEL,
    "new_label",
    Node::ImageRef(ImageRef{alt: attr_keys::ALT.to_string(), ident: "id".to_string(), label: Some("new_label".to_string()), position: None})
)]
#[case(
    Node::ImageRef(ImageRef{alt: attr_keys::ALT.to_string(), ident: "id".to_string(), label: None, position: None}),
    attr_keys::LABEL,
    "new_label",
    Node::ImageRef(ImageRef{alt: attr_keys::ALT.to_string(), ident: "id".to_string(), label: Some("new_label".to_string()), position: None})
)]
#[case(
    Node::LinkRef(LinkRef{ident: "id".to_string(), values: vec![], label: Some(attr_keys::LABEL.to_string()), position: None}),
    attr_keys::IDENT,
    "new_id",
    Node::LinkRef(LinkRef{ident: "new_id".to_string(), values: vec![], label: Some(attr_keys::LABEL.to_string()), position: None})
)]
#[case(
    Node::LinkRef(LinkRef{ident: "id".to_string(), values: vec![], label: Some(attr_keys::LABEL.to_string()), position: None}),
    attr_keys::LABEL,
    "new_label",
    Node::LinkRef(LinkRef{ident: "id".to_string(), values: vec![], label: Some("new_label".to_string()), position: None})
)]
#[case(
    Node::LinkRef(LinkRef{ident: "id".to_string(), values: vec![], label: None, position: None}),
    attr_keys::LABEL,
    "new_label",
    Node::LinkRef(LinkRef{ident: "id".to_string(), values: vec![], label: Some("new_label".to_string()), position: None})
)]
#[case(
    Node::LinkRef(LinkRef{ident: "id".to_string(), values: vec![], label: Some(attr_keys::LABEL.to_string()), position: None}),
    "unknown",
    "ignored",
    Node::LinkRef(LinkRef{ident: "id".to_string(), values: vec![], label: Some(attr_keys::LABEL.to_string()), position: None})
)]
#[case(
    Node::FootnoteRef(FootnoteRef{ident: "id".to_string(), label: Some(attr_keys::LABEL.to_string()), position: None}),
    attr_keys::IDENT,
    "new_id",
    Node::FootnoteRef(FootnoteRef{ident: "new_id".to_string(), label: Some(attr_keys::LABEL.to_string()), position: None})
)]
#[case(
    Node::FootnoteRef(FootnoteRef{ident: "id".to_string(), label: Some(attr_keys::LABEL.to_string()), position: None}),
    attr_keys::LABEL,
    "new_label",
    Node::FootnoteRef(FootnoteRef{ident: "id".to_string(), label: Some("new_label".to_string()), position: None})
)]
#[case(
    Node::FootnoteRef(FootnoteRef{ident: "id".to_string(), label: None, position: None}),
    attr_keys::LABEL,
    "new_label",
    Node::FootnoteRef(FootnoteRef{ident: "id".to_string(), label: Some("new_label".to_string()), position: None})
)]
#[case(
    Node::FootnoteRef(FootnoteRef{ident: "id".to_string(), label: Some(attr_keys::LABEL.to_string()), position: None}),
    "unknown",
    "ignored",
    Node::FootnoteRef(FootnoteRef{ident: "id".to_string(), label: Some(attr_keys::LABEL.to_string()), position: None})
)]
#[case(Node::Empty, attr_keys::VALUE, "ignored", Node::Empty)]
#[case(
    Node::TableAlign(TableAlign{align: vec![TableAlignKind::Left, TableAlignKind::Right], position: None}),
    "align",
    "---,:---:",
    Node::TableAlign(TableAlign{align: vec![TableAlignKind::None, TableAlignKind::Center], position: None})
)]
#[case(
    Node::TableAlign(TableAlign{align: vec![], position: None}),
    "align",
    ":---,---:",
    Node::TableAlign(TableAlign{align: vec![TableAlignKind::Left, TableAlignKind::Right], position: None})
)]
#[case(
    Node::TableAlign(TableAlign{align: vec![TableAlignKind::Left], position: None}),
    "unknown",
    "ignored",
    Node::TableAlign(TableAlign{align: vec![TableAlignKind::Left], position: None})
)]
#[case(
    Node::MdxFlowExpression(MdxFlowExpression{value: "old".into(), position: None}),
    attr_keys::VALUE,
    "new_expr",
    Node::MdxFlowExpression(MdxFlowExpression{value: "new_expr".into(), position: None})
)]
#[case(
    Node::MdxFlowExpression(MdxFlowExpression{value: "expr".into(), position: None}),
    "unknown",
    "ignored",
    Node::MdxFlowExpression(MdxFlowExpression{value: "expr".into(), position: None})
)]
#[case(
    Node::MdxTextExpression(MdxTextExpression{value: "old".into(), position: None}),
    attr_keys::VALUE,
    "new_expr",
    Node::MdxTextExpression(MdxTextExpression{value: "new_expr".into(), position: None})
)]
#[case(
    Node::MdxTextExpression(MdxTextExpression{value: "expr".into(), position: None}),
    "unknown",
    "ignored",
    Node::MdxTextExpression(MdxTextExpression{value: "expr".into(), position: None})
)]
#[case(
    Node::MdxJsEsm(MdxJsEsm{value: "import x".into(), position: None}),
    attr_keys::VALUE,
    "import y",
    Node::MdxJsEsm(MdxJsEsm{value: "import y".into(), position: None})
)]
#[case(
    Node::MdxJsEsm(MdxJsEsm{value: "import x".into(), position: None}),
    "unknown",
    "ignored",
    Node::MdxJsEsm(MdxJsEsm{value: "import x".into(), position: None})
)]
#[case(
    Node::MdxJsxFlowElement(MdxJsxFlowElement{name: Some("div".to_string()), attributes: Vec::new(), children: Vec::new(), position: None}),
    attr_keys::NAME,
    "section",
    Node::MdxJsxFlowElement(MdxJsxFlowElement{name: Some("section".to_string()), attributes: Vec::new(), children: Vec::new(), position: None})
)]
#[case(
    Node::MdxJsxFlowElement(MdxJsxFlowElement{name: None, attributes: Vec::new(), children: Vec::new(), position: None}),
    attr_keys::NAME,
    "main",
    Node::MdxJsxFlowElement(MdxJsxFlowElement{name: Some("main".to_string()), attributes: Vec::new(), children: Vec::new(), position: None})
)]
#[case(
    Node::MdxJsxFlowElement(MdxJsxFlowElement{name: Some("div".to_string()), attributes: Vec::new(), children: Vec::new(), position: None}),
    "unknown",
    "ignored",
    Node::MdxJsxFlowElement(MdxJsxFlowElement{name: Some("div".to_string()), attributes: Vec::new(), children: Vec::new(), position: None})
)]
#[case(
    Node::MdxJsxTextElement(MdxJsxTextElement{name: Some("span".into()), attributes: Vec::new(), children: Vec::new(), position: None}),
    attr_keys::NAME,
    "b",
    Node::MdxJsxTextElement(MdxJsxTextElement{name: Some("b".into()), attributes: Vec::new(), children: Vec::new(), position: None})
)]
#[case(
    Node::MdxJsxTextElement(MdxJsxTextElement{name: None, attributes: Vec::new(), children: Vec::new(), position: None}),
    attr_keys::NAME,
    "i",
    Node::MdxJsxTextElement(MdxJsxTextElement{name: Some("i".into()), attributes: Vec::new(), children: Vec::new(), position: None})
)]
#[case(
    Node::MdxJsxTextElement(MdxJsxTextElement{name: Some("span".into()), attributes: Vec::new(), children: Vec::new(), position: None}),
    "unknown",
    "ignored",
    Node::MdxJsxTextElement(MdxJsxTextElement{name: Some("span".into()), attributes: Vec::new(), children: Vec::new(), position: None})
)]
fn test_set_attr(#[case] mut node: Node, #[case] attr: &str, #[case] value: &str, #[case] expected: Node) {
    node.set_attr(attr, value);
    assert_eq!(node, expected);
}

#[rstest]
#[case(AttrValue::String("test".to_string()), AttrValue::String("test".to_string()), true)]
#[case(AttrValue::String("test".to_string()), AttrValue::String("other".to_string()), false)]
#[case(AttrValue::Integer(42), AttrValue::Integer(42), true)]
#[case(AttrValue::Integer(42), AttrValue::Integer(0), false)]
#[case(AttrValue::Boolean(true), AttrValue::Boolean(true), true)]
#[case(AttrValue::Boolean(true), AttrValue::Boolean(false), false)]
#[case(AttrValue::String("42".to_string()), AttrValue::Integer(42), false)]
#[case(AttrValue::Boolean(false), AttrValue::Integer(0), false)]
fn test_attr_value_eq(#[case] a: AttrValue, #[case] b: AttrValue, #[case] expected: bool) {
    assert_eq!(a == b, expected);
}

#[rstest]
#[case(AttrValue::String("test".to_string()), "test")]
#[case(AttrValue::Integer(42), "42")]
#[case(AttrValue::Boolean(true), "true")]
fn test_attr_value_as_str(#[case] value: AttrValue, #[case] expected: &str) {
    assert_eq!(&value.as_string(), expected);
}

#[rstest]
#[case(AttrValue::Integer(42), Some(42))]
#[case(AttrValue::String("42".to_string()), Some(42))]
#[case(AttrValue::Boolean(false), Some(0))]
fn test_attr_value_as_i64(#[case] value: AttrValue, #[case] expected: Option<i64>) {
    assert_eq!(value.as_i64(), expected);
}

// --- Callout tests ---

#[cfg(feature = "callout")]
#[rstest]
// basic render
#[case(
    Node::Callout(Callout { fold: None, kind: "NOTE".to_string(), title: None,
        values: vec![Node::Text(Text { value: "content".to_string(), position: None })],
        position: None }),
    "> [!NOTE]\n> content"
)]
// with title
#[case(
    Node::Callout(Callout { fold: None, kind: "WARNING".to_string(), title: Some("Heads up".to_string()),
        values: vec![Node::Text(Text { value: "watch out".to_string(), position: None })],
        position: None }),
    "> [!WARNING] Heads up\n> watch out"
)]
// empty body
#[case(
    Node::Callout(Callout { fold: None, kind: "TIP".to_string(), title: None, values: vec![], position: None }),
    "> [!TIP]"
)]
// multiline body: single Text with embedded '\n': each line prefixed with "> "
#[case(
    Node::Callout(Callout { fold: None, kind: "INFO".to_string(), title: None,
        values: vec![Node::Text(Text { value: "line one\nline two".to_string(), position: None })],
        position: None }),
    "> [!INFO]\n> line one\n> line two"
)]
// two separate position-less Text values are paragraphs: the inline nodes of one never are two Texts
#[case(
    Node::Callout(Callout { fold: None, kind: "INFO".to_string(), title: None,
        values: vec![
            Node::Text(Text { value: "part a".to_string(), position: None }),
            Node::Text(Text { value: "part b".to_string(), position: None }),
        ],
        position: None }),
    "> [!INFO]\n> part a\n> \n> part b"
)]
fn test_callout_render(#[case] node: Node, #[case] expected: &str) {
    assert_eq!(node.to_string_with(&RenderOptions::default()), expected);
}

#[cfg(feature = "callout")]
#[rstest]
#[case(Node::Callout(Callout { fold: None, kind: "NOTE".to_string(), title: None, values: vec![], position: None }), "kind", Some(AttrValue::String("NOTE".to_string())))]
#[case(Node::Callout(Callout { fold: None, kind: "WARNING".to_string(), title: Some("Title".to_string()), values: vec![], position: None }), "title", Some(AttrValue::String("Title".to_string())))]
#[case(Node::Callout(Callout { fold: None, kind: "TIP".to_string(), title: None, values: vec![], position: None }), "title", None)]
#[case(Node::Callout(Callout { fold: None, kind: "NOTE".to_string(), title: None, values: vec![Node::Text(Text { value: "body".to_string(), position: None })], position: None }), "value", Some(AttrValue::String("body".to_string())))]
#[case(Node::Callout(Callout { fold: None, kind: "NOTE".to_string(), title: None, values: vec![Node::Text(Text { value: "body".to_string(), position: None })], position: None }), "children", Some(AttrValue::Array(vec![Node::Text(Text { value: "body".to_string(), position: None })])))]
fn test_callout_attr(#[case] node: Node, #[case] attr: &str, #[case] expected: Option<AttrValue>) {
    assert_eq!(node.attr(attr), expected);
}

#[cfg(feature = "callout")]
#[rstest]
#[case(
    Node::Callout(Callout { fold: None, kind: "NOTE".to_string(), title: None, values: vec![], position: None }),
    "kind", "WARNING",
    Node::Callout(Callout { fold: None, kind: "WARNING".to_string(), title: None, values: vec![], position: None })
)]
// set_attr stores kind as-is without case conversion
#[case(
    Node::Callout(Callout { fold: None, kind: "NOTE".to_string(), title: None, values: vec![], position: None }),
    "kind", "tip",
    Node::Callout(Callout { fold: None, kind: "tip".to_string(), title: None, values: vec![], position: None })
)]
#[case(
    Node::Callout(Callout { fold: None, kind: "NOTE".to_string(), title: None, values: vec![], position: None }),
    "title", "My title",
    Node::Callout(Callout { fold: None, kind: "NOTE".to_string(), title: Some("My title".to_string()), values: vec![], position: None })
)]
// empty string clears title
#[case(
    Node::Callout(Callout { fold: None, kind: "NOTE".to_string(), title: Some("old".to_string()), values: vec![], position: None }),
    "title", "",
    Node::Callout(Callout { fold: None, kind: "NOTE".to_string(), title: None, values: vec![], position: None })
)]
fn test_callout_set_attr(#[case] mut node: Node, #[case] attr: &str, #[case] value: &str, #[case] expected: Node) {
    node.set_attr(attr, value);
    assert_eq!(node, expected);
}

#[cfg(feature = "callout")]
#[rstest]
// with_value changes first body node
#[case(
    Node::Callout(Callout { fold: None, kind: "NOTE".to_string(), title: None,
        values: vec![Node::Text(Text { value: "old".to_string(), position: None })],
        position: None }),
    "new",
    Node::Callout(Callout { fold: None, kind: "NOTE".to_string(), title: None,
        values: vec![Node::Text(Text { value: "new".to_string(), position: None })],
        position: None })
)]
fn test_callout_with_value(#[case] node: Node, #[case] value: &str, #[case] expected: Node) {
    assert_eq!(node.clone().with_value(value), expected);
    assert_eq!(node.into_with_value(value), expected);
}

#[cfg(feature = "callout")]
#[rstest]
#[case(Node::Callout(Callout { fold: None, kind: "NOTE".to_string(), title: None, values: vec![], position: None }), "callout")]
fn test_callout_name(#[case] node: Node, #[case] expected: &str) {
    assert_eq!(node.name(), expected);
}

#[cfg(feature = "callout")]
#[rstest]
#[case(
    Node::Callout(Callout { fold: None, kind: "NOTE".to_string(), title: None,
        values: vec![Node::Text(Text { value: "body".to_string(), position: None })],
        position: None }),
    "body"
)]
fn test_callout_value(#[case] node: Node, #[case] expected: &str) {
    assert_eq!(node.value(), expected);
}

#[cfg(feature = "callout")]
#[rstest]
#[case(Node::Callout(Callout { fold: None, kind: "NOTE".to_string(), title: None, values: vec![], position: None }), true)]
#[case(Node::Blockquote(Blockquote { values: vec![], position: None }), false)]
#[case(Node::Text(Text { value: "test".to_string(), position: None }), false)]
fn test_is_callout(#[case] node: Node, #[case] expected: bool) {
    assert_eq!(node.is_callout(), expected);
}

#[cfg(feature = "callout")]
#[test]
fn test_callout_end_to_end() {
    use crate::Markdown;
    // plain blockquote stays as Blockquote
    let plain = Markdown::from_markdown_str("> just a quote").unwrap();
    assert!(plain.nodes[0].is_blockquote());

    // [!NOTE] is parsed as Callout
    let note = Markdown::from_markdown_str("> [!NOTE]\n> body").unwrap();
    assert!(note.nodes[0].is_callout());
    assert_eq!(note.nodes[0].attr("kind"), Some(AttrValue::String("NOTE".to_string())));

    // [!WARNING] with title
    let warn = Markdown::from_markdown_str("> [!WARNING] Watch out\n> content").unwrap();
    assert!(warn.nodes[0].is_callout());
    assert_eq!(
        warn.nodes[0].attr("title"),
        Some(AttrValue::String("Watch out".to_string()))
    );
}

// --- Embed tests ---

#[cfg(feature = "embed")]
#[rstest]
// plain embed
#[case("![[target]]", vec![Node::Embed(Embed { target: "target".to_string(), display: None, position: None })])]
// embed with display hint (image size)
#[case("![[image.png|400]]", vec![Node::Embed(Embed { target: "image.png".to_string(), display: Some("400".to_string()), position: None })])]
// embed with section reference in target
#[case("![[note#Heading]]", vec![Node::Embed(Embed { target: "note#Heading".to_string(), display: None, position: None })])]
// embed at start of text
#[case("![[note]] rest", vec![
    Node::Embed(Embed { target: "note".to_string(), display: None, position: None }),
    Node::Text(Text { value: " rest".to_string(), position: None }),
])]
// embed in middle
#[case("before ![[note]] after", vec![
    Node::Text(Text { value: "before ".to_string(), position: None }),
    Node::Embed(Embed { target: "note".to_string(), display: None, position: None }),
    Node::Text(Text { value: " after".to_string(), position: None }),
])]
// multiple embeds
#[case("![[a]] and ![[b]]", vec![
    Node::Embed(Embed { target: "a".to_string(), display: None, position: None }),
    Node::Text(Text { value: " and ".to_string(), position: None }),
    Node::Embed(Embed { target: "b".to_string(), display: None, position: None }),
])]
// multibyte target
#[case("![[ノート]]", vec![Node::Embed(Embed { target: "ノート".to_string(), display: None, position: None })])]
// no embed: plain text unchanged
#[case("plain text", vec![Node::Text(Text { value: "plain text".to_string(), position: None })])]
// unclosed embed: treated as plain text
#[case("![[unclosed", vec![Node::Text(Text { value: "![[unclosed".to_string(), position: None })])]
// lone ! before non-embed: treated as plain text
#[case("! not an embed", vec![Node::Text(Text { value: "! not an embed".to_string(), position: None })])]
fn test_parse_embeds_in_text(#[case] input: &str, #[case] expected: Vec<Node>) {
    let mut result = Vec::new();
    Node::parse_embeds_into(input, None, &mut result);
    assert_eq!(result, expected);
}

#[cfg(feature = "embed")]
#[rstest]
// plain embed renders without display
#[case(
    Node::Embed(Embed { target: "note.md".to_string(), display: None, position: None }),
    "![[note.md]]"
)]
// embed with display hint
#[case(
    Node::Embed(Embed { target: "image.png".to_string(), display: Some("400".to_string()), position: None }),
    "![[image.png|400]]"
)]
// embed with section reference
#[case(
    Node::Embed(Embed { target: "note#Intro".to_string(), display: None, position: None }),
    "![[note#Intro]]"
)]
fn test_embed_render(#[case] node: Node, #[case] expected: &str) {
    assert_eq!(node.to_string_with(&RenderOptions::default()), expected);
}

#[cfg(feature = "embed")]
#[rstest]
#[case(Node::Embed(Embed { target: "note".to_string(), display: None, position: None }), "url", Some(AttrValue::String("note".to_string())))]
// value returns target when no display
#[case(Node::Embed(Embed { target: "note".to_string(), display: None, position: None }), "value", Some(AttrValue::String("note".to_string())))]
// url always returns target regardless of display
#[case(Node::Embed(Embed { target: "note".to_string(), display: Some("400".to_string()), position: None }), "url", Some(AttrValue::String("note".to_string())))]
// value returns display when present
#[case(Node::Embed(Embed { target: "note".to_string(), display: Some("400".to_string()), position: None }), "value", Some(AttrValue::String("400".to_string())))]
// unknown attr returns None
#[case(Node::Embed(Embed { target: "note".to_string(), display: None, position: None }), "unknown", None)]
fn test_embed_attr(#[case] node: Node, #[case] attr: &str, #[case] expected: Option<AttrValue>) {
    assert_eq!(node.attr(attr), expected);
}

#[cfg(feature = "embed")]
#[rstest]
// set url changes target
#[case(
    Node::Embed(Embed { target: "old".to_string(), display: None, position: None }),
    "url", "new",
    Node::Embed(Embed { target: "new".to_string(), display: None, position: None })
)]
// set value changes display
#[case(
    Node::Embed(Embed { target: "note".to_string(), display: None, position: None }),
    "value", "800",
    Node::Embed(Embed { target: "note".to_string(), display: Some("800".to_string()), position: None })
)]
// empty value clears display
#[case(
    Node::Embed(Embed { target: "note".to_string(), display: Some("400".to_string()), position: None }),
    "value", "",
    Node::Embed(Embed { target: "note".to_string(), display: None, position: None })
)]
fn test_embed_set_attr(#[case] mut node: Node, #[case] attr: &str, #[case] value: &str, #[case] expected: Node) {
    node.set_attr(attr, value);
    assert_eq!(node, expected);
}

#[cfg(feature = "embed")]
#[rstest]
// no display: with_value sets target
#[case(
    Node::Embed(Embed { target: "old".to_string(), display: None, position: None }),
    "new",
    Node::Embed(Embed { target: "new".to_string(), display: None, position: None })
)]
// with display: with_value sets display
#[case(
    Node::Embed(Embed { target: "note".to_string(), display: Some("400".to_string()), position: None }),
    "800",
    Node::Embed(Embed { target: "note".to_string(), display: Some("800".to_string()), position: None })
)]
fn test_embed_with_value(#[case] node: Node, #[case] value: &str, #[case] expected: Node) {
    assert_eq!(node.clone().with_value(value), expected);
    assert_eq!(node.into_with_value(value), expected);
}

#[cfg(feature = "embed")]
#[rstest]
#[case(Node::Embed(Embed { target: "note".to_string(), display: None, position: None }), "embed")]
fn test_embed_name(#[case] node: Node, #[case] expected: &str) {
    assert_eq!(node.name(), expected);
}

#[cfg(feature = "embed")]
#[rstest]
// value returns display when present
#[case(Node::Embed(Embed { target: "note".to_string(), display: Some("400".to_string()), position: None }), "400")]
// value returns target when no display
#[case(Node::Embed(Embed { target: "note".to_string(), display: None, position: None }), "note")]
fn test_embed_value(#[case] node: Node, #[case] expected: &str) {
    assert_eq!(node.value(), expected);
}

#[cfg(feature = "embed")]
#[rstest]
#[case(Node::Embed(Embed { target: "note".to_string(), display: None, position: None }), true)]
#[case(Node::Text(Text { value: "test".to_string(), position: None }), false)]
#[case(Node::Image(Image { alt: "".to_string(), url: "img.png".to_string(), title: None, position: None }), false)]
fn test_is_embed(#[case] node: Node, #[case] expected: bool) {
    assert_eq!(node.is_embed(), expected);
}

#[cfg(feature = "embed")]
#[test]
fn test_embed_end_to_end() {
    use crate::Markdown;
    let md = Markdown::from_markdown_str("See ![[note.md]] for details.").unwrap();
    let embed = md.nodes.iter().find(|n| n.is_embed());
    assert!(embed.is_some());
    assert_eq!(
        embed.unwrap().attr("url"),
        Some(AttrValue::String("note.md".to_string()))
    );
}

#[cfg(all(feature = "callout", feature = "wikilink"))]
#[rstest]
// callout body containing a wikilink
#[case("> [!NOTE]\n> See [[note]]", 1, "> [!NOTE]\n> See [[note]]\n")]
fn test_callout_with_wikilink(#[case] input: &str, #[case] expected_nodes: usize, #[case] expected_output: &str) {
    use crate::Markdown;
    let md = Markdown::from_markdown_str(input).unwrap();
    assert_eq!(md.nodes.len(), expected_nodes);
    assert_eq!(md.to_string(), expected_output);
}

#[cfg(feature = "callout")]
#[rstest]
// callout body with bold
#[case("> [!NOTE]\n> **important**", 1, "> [!NOTE]\n> **important**\n")]
// callout body with inline code
#[case("> [!NOTE]\n> Use `code`", 1, "> [!NOTE]\n> Use `code`\n")]
// multiple callouts in one document
#[case(
    "> [!NOTE]\n> first\n\n> [!WARNING]\n> second",
    2,
    "> [!NOTE]\n> first\n\n> [!WARNING]\n> second\n"
)]
// callout after heading
#[case("# Title\n\n> [!NOTE]\n> body", 2, "# Title\n\n> [!NOTE]\n> body\n")]
// callout body with a multi-item list: second item must not be over-indented
#[case("> [!NOTE]\n> - item 1\n> - item 2", 1, "> [!NOTE]\n> - item 1\n> - item 2\n")]
fn test_callout_combined_with_other_nodes(
    #[case] input: &str,
    #[case] expected_nodes: usize,
    #[case] expected_output: &str,
) {
    use crate::Markdown;
    let md = Markdown::from_markdown_str(input).unwrap();
    assert_eq!(md.nodes.len(), expected_nodes);
    assert_eq!(md.to_string(), expected_output);
}

#[cfg(feature = "embed")]
#[rstest]
// embed inside list item
#[case("- ![[note.md]]", 1, "- ![[note.md]]\n")]
// embed inside heading
#[case("# See ![[note]]", 1, "# See ![[note]]\n")]
// embed inside bold
#[case("**![[note]]**", 1, "**![[note]]**\n")]
fn test_embed_combined_with_other_nodes(
    #[case] input: &str,
    #[case] expected_nodes: usize,
    #[case] expected_output: &str,
) {
    use crate::Markdown;
    let md = Markdown::from_markdown_str(input).unwrap();
    assert_eq!(md.nodes.len(), expected_nodes);
    assert_eq!(md.to_string(), expected_output);
}

#[cfg(all(feature = "callout", feature = "embed"))]
#[rstest]
// wikilink inside callout body + embed as separate node
#[case("> [!NOTE]\n> [[link]]\n\n![[embed]]", 2, "> [!NOTE]\n> [[link]]\n\n![[embed]]\n")]
fn test_callout_and_embed_together(#[case] input: &str, #[case] expected_nodes: usize, #[case] expected_output: &str) {
    use crate::Markdown;
    let md = Markdown::from_markdown_str(input).unwrap();
    assert_eq!(md.nodes.len(), expected_nodes);
    assert_eq!(md.to_string(), expected_output);
}

#[cfg(all(feature = "embed", feature = "wikilink"))]
#[test]
fn test_embed_does_not_conflict_with_wikilink() {
    use crate::Markdown;
    // ![[embed]] must become Embed, not WikiLink with preceding "!"
    let md = Markdown::from_markdown_str("![[embed]] and [[link]]").unwrap();
    let embed = md.nodes.iter().find(|n| n.is_embed());
    let wikilink = md.nodes.iter().find(|n| n.is_wikilink());
    assert!(embed.is_some(), "expected Embed node");
    assert!(wikilink.is_some(), "expected WikiLink node");
}

#[rstest]
#[case::escaped_when_parsed_from_source(
    Node::Text(Text {
        value: "*a*".to_string(),
        position: Some(Position {
            start: Point { line: 1, column: 1 },
            end: Point { line: 1, column: 4 },
        }),
    }),
    "\\*a\\*"
)]
#[case::untouched_when_built_programmatically(
    Node::Text(Text { value: "*a*".to_string(), position: None }),
    "*a*"
)]
fn test_text_escaping_gated_by_position(#[case] node: Node, #[case] expected: &str) {
    assert_eq!(node.to_string_with(&RenderOptions::default()), expected);
}

#[rstest]
#[case::bare_safe("https://example.com/x", UrlSurroundStyle::None, "https://example.com/x")]
#[case::space_forces_angle("my uri", UrlSurroundStyle::None, "<my uri>")]
#[case::empty_forces_angle("", UrlSurroundStyle::None, "<>")]
#[case::unbalanced_paren_escaped("b)c", UrlSurroundStyle::None, "b\\)c")]
#[case::both_parens_escaped("foo(and(bar)", UrlSurroundStyle::None, "foo\\(and\\(bar\\)")]
#[case::forced_angle_style("plain", UrlSurroundStyle::Angle, "<plain>")]
#[case::angle_escapes_inner_angle_and_backslash("foo\\>", UrlSurroundStyle::None, "<foo\\\\\\>>")]
fn test_render_link_destination(#[case] url: &str, #[case] style: UrlSurroundStyle, #[case] expected: &str) {
    assert_eq!(render_link_destination(url, &style), expected);
}

#[rstest]
#[case::double_quote_escaped(r#"a "b" c"#, TitleSurroundStyle::Double, r#""a \"b\" c""#)]
#[case::single_quote_escaped("a 'b' c", TitleSurroundStyle::Single, "'a \\'b\\' c'")]
#[case::paren_escapes_both("a (b) c", TitleSurroundStyle::Paren, "(a \\(b\\) c)")]
#[case::backslash_escaped("a\\b", TitleSurroundStyle::Double, "\"a\\\\b\"")]
fn test_render_link_title(#[case] title: &str, #[case] style: TitleSurroundStyle, #[case] expected: &str) {
    assert_eq!(render_link_title(title, &style), expected);
}

#[rstest]
#[case::shortcut_kept_when_ident_matches_label_present("foo", "foo", Some("foo".to_string()), "[foo]")]
#[case::shortcut_kept_when_ident_matches_label_absent("foo", "foo", None, "[foo]")]
#[case::full_form_when_ident_differs("foo", "bar", Some("bar".to_string()), "[foo][bar]")]
#[case::full_form_when_plain_needs_broad_escaping("foo*bar", "foo*bar", Some("foo*bar".to_string()), "[foo*bar][foo*bar]")]
fn test_link_ref_shortcut_vs_full(
    #[case] text: &str,
    #[case] ident: &str,
    #[case] label: Option<String>,
    #[case] expected: &str,
) {
    let node = Node::LinkRef(LinkRef {
        ident: ident.to_string(),
        values: vec![text.to_string().into()],
        label,
        position: None,
    });
    assert_eq!(node.to_string_with(&RenderOptions::default()), expected);
}

#[test]
fn test_link_ref_display_escaping_does_not_leak_into_matching_label() {
    // The display text (first bracket) is escaped for safety; the matching key
    // (second bracket) must stay the definition-compatible, narrowly-escaped label.
    let node = Node::LinkRef(LinkRef {
        ident: "foo*bar".to_string(),
        values: vec![Node::Text(Text {
            value: "foo*bar".to_string(),
            position: Some(Position {
                start: Point { line: 1, column: 1 },
                end: Point { line: 1, column: 8 },
            }),
        })],
        label: Some("foo*bar".to_string()),
        position: None,
    });
    assert_eq!(node.to_string_with(&RenderOptions::default()), "[foo\\*bar][foo*bar]");
}

#[rstest]
#[case::shortcut_kept_when_ident_matches("foo bar", "foo bar", "![foo bar]")]
#[case::full_form_when_ident_differs("foo bar", "foo*bar*", "![foo bar][foo*bar*]")]
fn test_image_ref_shortcut_vs_full(#[case] alt: &str, #[case] ident: &str, #[case] expected: &str) {
    let node = Node::ImageRef(ImageRef {
        alt: alt.to_string(),
        ident: ident.to_string(),
        label: None,
        position: None,
    });
    assert_eq!(node.to_string_with(&RenderOptions::default()), expected);
}

#[rstest]
#[case::no_newline_still_shifted("abc", 3, "   abc")]
#[case::zero_delta_unchanged("a\n  b", 0, "a\n  b")]
#[case::first_line_also_shifted("a\nb", 2, "  a\n  b")]
#[case::negative_delta_clamped_at_zero("  a\n  b", -5, "a\nb")]
fn test_reindent_all_lines(#[case] content: &str, #[case] delta: isize, #[case] expected: &str) {
    assert_eq!(reindent_all_lines(content, delta), expected);
}

#[rstest]
#[case::unordered_bullet(false, 0, None, 2)]
#[case::ordered_single_digit(true, 0, None, 3)]
#[case::ordered_double_digit(true, 9, None, 4)]
#[case::ordered_with_start(true, 0, Some(10), 4)]
fn test_list_own_prefix_width(
    #[case] ordered: bool,
    #[case] index: usize,
    #[case] start: Option<u32>,
    #[case] expected: usize,
) {
    assert_eq!(list_own_prefix_width(ordered, index, start), expected);
}

#[rstest]
#[case(0, None)]
#[case(1, Some(HeadingDepth::H1))]
#[case(6, Some(HeadingDepth::H6))]
#[case(7, None)]
fn heading_depth_new(#[case] depth: u8, #[case] expected: Option<HeadingDepth>) {
    assert_eq!(HeadingDepth::new(depth), expected);
    assert_eq!(HeadingDepth::try_from(depth).ok(), expected);
}

#[rstest]
#[case(i64::MIN, HeadingDepth::H1)]
#[case(0, HeadingDepth::H1)]
#[case(3, HeadingDepth::H3)]
#[case(257, HeadingDepth::H6)]
fn heading_depth_saturating(#[case] depth: i64, #[case] expected: HeadingDepth) {
    assert_eq!(HeadingDepth::saturating(depth), expected);
}

#[rstest]
#[case(HeadingDepth::H2, 3, Some(HeadingDepth::H5))]
#[case(HeadingDepth::H6, 1, None)]
#[case(HeadingDepth::H6, u8::MAX, None)]
fn heading_depth_checked_add(#[case] depth: HeadingDepth, #[case] levels: u8, #[case] expected: Option<HeadingDepth>) {
    assert_eq!(depth.checked_add(levels), expected);
}

#[rstest]
#[case(HeadingDepth::H4, 2, HeadingDepth::H2)]
#[case(HeadingDepth::H2, u8::MAX, HeadingDepth::H1)]
fn heading_depth_saturating_sub(#[case] depth: HeadingDepth, #[case] levels: u8, #[case] expected: HeadingDepth) {
    assert_eq!(depth.saturating_sub(levels), expected);
}

#[rstest]
#[case::integer(AttrValue::Integer(3), HeadingDepth::H3)]
#[case::string(AttrValue::String("4".to_string()), HeadingDepth::H4)]
#[case::too_deep(AttrValue::Integer(7), HeadingDepth::H2)]
#[case::wraps_as_byte(AttrValue::Integer(257), HeadingDepth::H2)]
#[case::negative(AttrValue::Integer(-1), HeadingDepth::H2)]
#[case::zero_string(AttrValue::String("0".to_string()), HeadingDepth::H2)]
fn set_attr_keeps_heading_depth_in_range(#[case] value: AttrValue, #[case] expected: HeadingDepth) {
    let mut node = Node::Heading(Heading {
        depth: HeadingDepth::H2,
        values: vec!["a".to_string().into()],
        position: None,
    });
    node.set_attr(attr_keys::DEPTH, value);
    assert!(matches!(node, Node::Heading(Heading { depth, .. }) if depth == expected));
}

#[cfg(feature = "json")]
#[test]
fn heading_depth_is_a_number_in_json() {
    let heading = Heading {
        depth: HeadingDepth::H3,
        values: Vec::new(),
        position: None,
    };
    let json = serde_json::to_string(&heading).unwrap();
    assert!(json.contains("\"depth\":3"), "{json}");
    assert_eq!(serde_json::from_str::<Heading>(&json).unwrap(), heading);
    assert!(serde_json::from_str::<Heading>(&json.replace("\"depth\":3", "\"depth\":7")).is_err());
}
