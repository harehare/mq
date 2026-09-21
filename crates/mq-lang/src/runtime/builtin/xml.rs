//! `_xml_parse` builtin: parses an XML string into nested `{tag, attributes, children, text}`
//! dicts, the same shape [`super::css`] produces for HTML.

use crate::DictMap;
use crate::runtime::runtime_value::RuntimeValue;
use crate::{Ident, Shared};

use quick_xml::XmlVersion;
use quick_xml::events::{BytesStart, Event};

use super::Error;

fn is_xml_whitespace(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\r' | '\n')
}

/// Text taken from [`PendingText`], remembering whether whitespace was trimmed at each edge.
///
/// A whitespace-only run has an empty `text` and `space_before` set.
struct TextSegment {
    text: String,
    space_before: bool,
    space_after: bool,
}

/// Character data and entity references collected until the next tag, comment or CDATA.
///
/// Only the outer edges of the collected text are trimmed (pretty-printing indentation), so
/// whitespace next to an entity reference (`x &amp; y`) is preserved.
#[derive(Default)]
struct PendingText {
    buf: String,
    tail_start: Option<usize>,
    space_before: bool,
}

impl PendingText {
    fn push_text(&mut self, text: &str) {
        let text = if self.buf.is_empty() {
            let trimmed = text.trim_start_matches(is_xml_whitespace);
            self.space_before |= trimmed.len() != text.len();
            trimmed
        } else {
            text
        };
        self.tail_start = Some(self.buf.len());
        self.buf.push_str(text);
    }

    fn push_resolved_ref(&mut self, resolved: &str) {
        self.tail_start = None;
        self.buf.push_str(resolved);
    }

    fn take(&mut self) -> Option<TextSegment> {
        let mut space_after = false;
        if let Some(start) = self.tail_start.take() {
            let tail = &self.buf[start..];
            let kept = tail.trim_end_matches(is_xml_whitespace).len();
            space_after = kept != tail.len();
            self.buf.truncate(start + kept);
        }
        let space_before = std::mem::take(&mut self.space_before);
        let text = std::mem::take(&mut self.buf);
        (!text.is_empty() || space_before).then_some(TextSegment {
            text,
            space_before,
            space_after,
        })
    }
}

/// An element whose end tag has not been seen yet.
struct OpenElement {
    tag: String,
    attributes: DictMap,
    children: Vec<RuntimeValue>,
    text: Option<String>,
    /// Whitespace was seen after the last text piece and becomes a single space if more text follows.
    space_pending: bool,
}

impl OpenElement {
    fn new(tag: String, attributes: DictMap) -> Self {
        Self {
            tag,
            attributes,
            children: Vec::new(),
            text: None,
            space_pending: false,
        }
    }

    /// Joins text split by child elements or comments, keeping one space where whitespace was.
    fn append_segment(&mut self, segment: TextSegment) {
        if segment.text.is_empty() {
            self.space_pending |= segment.space_before;
            return;
        }
        self.push_text(&segment.text, self.space_pending || segment.space_before);
        self.space_pending = segment.space_after;
    }

    /// CDATA is appended as is, only a pending space before it is kept.
    fn append_cdata(&mut self, text: &str) {
        self.push_text(text, self.space_pending);
        self.space_pending = false;
    }

    fn push_text(&mut self, text: &str, separate: bool) {
        match &mut self.text {
            Some(existing) => {
                if separate && !existing.ends_with(is_xml_whitespace) && !text.starts_with(is_xml_whitespace) {
                    existing.push(' ');
                }
                existing.push_str(text);
            }
            None => self.text = Some(text.to_string()),
        }
    }

    fn into_value(self) -> RuntimeValue {
        let mut dict = DictMap::default();
        dict.insert(Ident::new("tag"), RuntimeValue::String(self.tag.into()));
        dict.insert(
            Ident::new("attributes"),
            RuntimeValue::Dict(Shared::new(self.attributes)),
        );
        dict.insert(Ident::new("children"), RuntimeValue::Array(Shared::new(self.children)));
        dict.insert(
            Ident::new("text"),
            self.text
                .map(|s| RuntimeValue::String(s.into()))
                .unwrap_or(RuntimeValue::NONE),
        );
        RuntimeValue::Dict(Shared::new(dict))
    }
}

fn parse_attributes(e: &BytesStart<'_>) -> Result<DictMap, Error> {
    let mut attrs = DictMap::default();
    for attr in e.attributes() {
        let attr = attr.map_err(|e| Error::Runtime(format!("XML attribute error: {}", e)))?;
        let key = attr.key.as_ref().to_string();
        let value = attr
            .normalized_value(XmlVersion::default())
            .map_err(|e| Error::Runtime(format!("XML attribute value error: {}", e)))?
            .to_string();
        attrs.insert(Ident::new(&key), RuntimeValue::String(value.into()));
    }
    Ok(attrs)
}

/// Adds a finished element to its parent, or hands it back when it is the root.
fn attach(stack: &mut [OpenElement], element: RuntimeValue) -> Option<RuntimeValue> {
    match stack.last_mut() {
        Some(parent) => {
            parent.children.push(element);
            None
        }
        None => Some(element),
    }
}

/// Parses the first root element of `xml`, or returns `NONE` when there is none.
///
/// Text split by child elements or comments is joined, with a single space where whitespace
/// separated the pieces. An element left open at the end of the input is an error.
pub(super) fn parse_xml(xml: &str) -> Result<RuntimeValue, Error> {
    let mut reader = quick_xml::Reader::from_str(xml);
    let mut buf = Vec::new();
    let mut stack: Vec<OpenElement> = Vec::new();
    let mut pending = PendingText::default();

    loop {
        let event = reader.read_event_into(&mut buf);

        if !matches!(event, Ok(Event::Text(_) | Event::GeneralRef(_)))
            && let (Some(segment), Some(parent)) = (pending.take(), stack.last_mut())
        {
            parent.append_segment(segment);
        }

        match event {
            Ok(Event::Start(e)) => {
                let tag = e.name().as_ref().to_string();
                stack.push(OpenElement::new(tag, parse_attributes(&e)?));
            }
            Ok(Event::End(e)) => {
                let end_tag = e.name().as_ref().to_string();
                let open = stack.pop().ok_or_else(|| {
                    Error::Runtime(format!(
                        "XML parse error at position {}: unexpected closing tag </{}>",
                        reader.buffer_position(),
                        end_tag
                    ))
                })?;

                if open.tag != end_tag {
                    return Err(Error::Runtime(format!(
                        "XML parse error at position {}: mismatched closing tag: expected </{}> but found </{}>",
                        reader.buffer_position(),
                        open.tag,
                        end_tag
                    )));
                }

                if let Some(root) = attach(&mut stack, open.into_value()) {
                    return Ok(root);
                }
            }
            Ok(Event::Empty(e)) => {
                let tag = e.name().as_ref().to_string();
                let element = OpenElement::new(tag, parse_attributes(&e)?).into_value();

                if let Some(root) = attach(&mut stack, element) {
                    return Ok(root);
                }
            }
            Ok(Event::Text(e)) => {
                if !stack.is_empty() {
                    pending.push_text(e.as_ref());
                }
            }
            Ok(Event::CData(e)) => {
                if let Some(parent) = stack.last_mut() {
                    parent.append_cdata(e.as_ref());
                }
            }
            Ok(Event::GeneralRef(e)) => {
                // quick-xml emits `&name;`/`&#NNN;` as their own event instead of leaving them
                // inside `Event::Text`, so they must be resolved here or they vanish from the text.
                if !stack.is_empty() {
                    let resolved = e
                        .resolve_char_ref()
                        .map_err(|e| {
                            Error::Runtime(format!(
                                "XML parse error at position {}: invalid character reference: {}",
                                reader.buffer_position(),
                                e
                            ))
                        })?
                        .map(String::from)
                        .or_else(|| quick_xml::escape::resolve_predefined_entity(e.as_ref()).map(String::from))
                        .ok_or_else(|| {
                            Error::Runtime(format!(
                                "XML parse error at position {}: unknown entity reference &{};",
                                reader.buffer_position(),
                                e.as_ref()
                            ))
                        })?;

                    pending.push_resolved_ref(&resolved);
                }
            }
            Ok(Event::Eof) => {
                return match stack.last() {
                    Some(open) => Err(Error::Runtime(format!(
                        "XML parse error at position {}: unexpected end of input: unclosed tag <{}>",
                        reader.buffer_position(),
                        open.tag
                    ))),
                    None => Ok(RuntimeValue::NONE),
                };
            }
            Err(e) => {
                return Err(Error::Runtime(format!(
                    "XML parse error at position {}: {}",
                    reader.buffer_position(),
                    e
                )));
            }
            _ => (),
        }

        buf.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use rstest::rstest;

    #[rstest]
    #[case::simple(
        "<root>hello</root>",
        {
            let mut root = DictMap::default();
            root.insert(Ident::new("tag"), RuntimeValue::String(Shared::new("root".to_string())));
            root.insert(Ident::new("attributes"), RuntimeValue::new_dict());
            root.insert(Ident::new("children"), RuntimeValue::empty_array());
            root.insert(Ident::new("text"), RuntimeValue::String(Shared::new("hello".to_string())));
            Ok(RuntimeValue::Dict(Shared::new(root)))
        }
    )]
    #[case::with_attributes(
        "<root id=\"1\" class=\"main\">hello</root>",
        {
            let mut root = DictMap::default();
            let mut attrs = DictMap::default();
            attrs.insert(Ident::new("id"), RuntimeValue::String(Shared::new("1".to_string())));
            attrs.insert(Ident::new("class"), RuntimeValue::String(Shared::new("main".to_string())));
            root.insert(Ident::new("tag"), RuntimeValue::String(Shared::new("root".to_string())));
            root.insert(Ident::new("attributes"), RuntimeValue::Dict(Shared::new(attrs)));
            root.insert(Ident::new("children"), RuntimeValue::empty_array());
            root.insert(Ident::new("text"), RuntimeValue::String(Shared::new("hello".to_string())));
            Ok(RuntimeValue::Dict(Shared::new(root)))
        }
    )]
    #[case::nested(
        "<root><child id=\"1\">hello</child><child id=\"2\">world</child></root>",
        {
            let mut root = DictMap::default();
            let mut child1 = DictMap::default();
            let mut attrs1 = DictMap::default();
            attrs1.insert(Ident::new("id"), RuntimeValue::String(Shared::new("1".to_string())));
            child1.insert(Ident::new("tag"), RuntimeValue::String(Shared::new("child".to_string())));
            child1.insert(Ident::new("attributes"), RuntimeValue::Dict(Shared::new(attrs1)));
            child1.insert(Ident::new("children"), RuntimeValue::empty_array());
            child1.insert(Ident::new("text"), RuntimeValue::String(Shared::new("hello".to_string())));

            let mut child2 = DictMap::default();
            let mut attrs2 = DictMap::default();
            attrs2.insert(Ident::new("id"), RuntimeValue::String(Shared::new("2".to_string())));
            child2.insert(Ident::new("tag"), RuntimeValue::String(Shared::new("child".to_string())));
            child2.insert(Ident::new("attributes"), RuntimeValue::Dict(Shared::new(attrs2)));
            child2.insert(Ident::new("children"), RuntimeValue::empty_array());
            child2.insert(Ident::new("text"), RuntimeValue::String(Shared::new("world".to_string())));

            root.insert(Ident::new("tag"), RuntimeValue::String(Shared::new("root".to_string())));
            root.insert(Ident::new("attributes"), RuntimeValue::new_dict());
            root.insert(Ident::new("children"), RuntimeValue::Array(Shared::new(vec![
                RuntimeValue::Dict(Shared::new(child1)),
                RuntimeValue::Dict(Shared::new(child2)),
            ])));
            root.insert(Ident::new("text"), RuntimeValue::NONE);
            Ok(RuntimeValue::Dict(Shared::new(root)))
        }
    )]
    #[case::self_closing(
        "<root><child id=\"1\"/></root>",
        {
            let mut root = DictMap::default();
            let mut child = DictMap::default();
            let mut attrs = DictMap::default();
            attrs.insert(Ident::new("id"), RuntimeValue::String(Shared::new("1".to_string())));
            child.insert(Ident::new("tag"), RuntimeValue::String(Shared::new("child".to_string())));
            child.insert(Ident::new("attributes"), RuntimeValue::Dict(Shared::new(attrs)));
            child.insert(Ident::new("children"), RuntimeValue::empty_array());
            child.insert(Ident::new("text"), RuntimeValue::NONE);

            root.insert(Ident::new("tag"), RuntimeValue::String(Shared::new("root".to_string())));
            root.insert(Ident::new("attributes"), RuntimeValue::new_dict());
            root.insert(Ident::new("children"), RuntimeValue::Array(Shared::new(vec![
                RuntimeValue::Dict(Shared::new(child)),
            ])));
            root.insert(Ident::new("text"), RuntimeValue::NONE);
            Ok(RuntimeValue::Dict(Shared::new(root)))
        }
    )]
    fn test_parse_xml_structure(#[case] xml: &str, #[case] expected: Result<RuntimeValue, Error>) {
        assert_eq!(parse_xml(xml), expected);
    }

    fn root_text(xml: &str) -> Option<String> {
        match parse_xml(xml) {
            Ok(RuntimeValue::Dict(root)) => match root.get(&Ident::new("text")) {
                Some(RuntimeValue::String(text)) => Some(text.to_string()),
                _ => None,
            },
            other => panic!("expected a root element, got {other:?}"),
        }
    }

    #[rstest]
    #[case::entity_references(
        "<root>&lt;b&gt;x&amp;x&quot;q&quot;x&apos;s&apos;x&#65;&#x42;</root>",
        Some("<b>x&x\"q\"x's'xAB")
    )]
    #[case::entity_with_surrounding_spaces("<a>x &amp; y</a>", Some("x & y"))]
    #[case::whitespace_only_between_entities("<a>&lt; &gt;</a>", Some("< >"))]
    #[case::entity_at_edges_with_inner_spaces("<a>&amp; x &amp;</a>", Some("& x &"))]
    #[case::char_ref_space_is_preserved("<a>&#32;x&#32;</a>", Some(" x "))]
    #[case::pretty_printed_text_is_trimmed("<a>\n  x &amp; y\n</a>", Some("x & y"))]
    #[case::pretty_printed_entity_only("<a>\n  &amp;\n</a>", Some("&"))]
    #[case::whitespace_only_text_is_dropped("<a>\n  \n</a>", None)]
    #[case::non_xml_whitespace_is_kept("<a>\u{a0}x\u{a0}</a>", Some("\u{a0}x\u{a0}"))]
    #[case::cdata_whitespace_is_preserved("<a><![CDATA[ x  y ]]></a>", Some(" x  y "))]
    #[case::entity_and_cdata("<a>x &amp; <![CDATA[ y ]]></a>", Some("x & y "))]
    #[case::space_before_cdata("<a>x <![CDATA[y]]></a>", Some("x y"))]
    #[case::space_after_cdata("<a><![CDATA[y]]> z</a>", Some("y z"))]
    #[case::mixed_content_keeps_word_space("<p>Hello <b>x</b> world</p>", Some("Hello world"))]
    #[case::mixed_content_without_space_stays_joined("<p>Hello<b>x</b>world</p>", Some("Helloworld"))]
    #[case::mixed_content_pretty_printed("<p>\n  Hello\n  <b>x</b>\n  world\n</p>", Some("Hello world"))]
    #[case::whitespace_between_children_separates_text("<a>x<b/> <c/>y</a>", Some("x y"))]
    #[case::space_before_child_is_dropped("<a>x <b/></a>", Some("x"))]
    #[case::space_after_child_is_dropped("<a><b/> x</a>", Some("x"))]
    #[case::comment_does_not_split_words("<a>x<!-- c -->y</a>", Some("xy"))]
    #[case::comment_between_spaced_words("<a>x <!-- c --> y</a>", Some("x y"))]
    #[case::content_after_root_is_ignored("<a>x</a><b>y</b>", Some("x"))]
    fn test_parse_xml_text(#[case] xml: &str, #[case] expected: Option<&str>) {
        assert_eq!(root_text(xml).as_deref(), expected);
    }

    #[rstest]
    #[case::empty("")]
    #[case::whitespace_only("  \n")]
    #[case::comment_only("<!-- c -->")]
    #[case::declaration_only(r#"<?xml version="1.0"?>"#)]
    fn test_parse_xml_without_root_is_none(#[case] xml: &str) {
        assert_eq!(parse_xml(xml), Ok(RuntimeValue::NONE));
    }

    #[rstest]
    #[case::unknown_entity("<a>&foo;</a>", "unknown entity reference &foo;")]
    #[case::invalid_char_ref("<a>&#xZZ;</a>", "invalid character reference")]
    #[case::unclosed_tag("<a>x", "unclosed tag <a>")]
    #[case::unclosed_nested_reports_innermost("<a><b>x</b><c>", "unclosed tag <c>")]
    #[case::mismatched_closing_tag("<a><b></a>", "XML parse error")]
    #[case::unexpected_closing_tag("</a>", "XML parse error")]
    #[case::unquoted_attribute("<a x=1>t</a>", "XML")]
    fn test_parse_xml_errors(#[case] xml: &str, #[case] expected: &str) {
        match parse_xml(xml) {
            Err(Error::Runtime(message)) => assert!(message.contains(expected), "unexpected message: {message}"),
            other => panic!("expected a runtime error, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_xml_decodes_entities_in_attributes() {
        let Ok(RuntimeValue::Dict(root)) = parse_xml(r#"<a x="1 &amp; 2 &lt;3">t</a>"#) else {
            panic!("expected a root element");
        };
        let Some(RuntimeValue::Dict(attributes)) = root.get(&Ident::new("attributes")) else {
            panic!("expected attributes");
        };
        assert_eq!(
            attributes.get(&Ident::new("x")),
            Some(&RuntimeValue::String(Shared::new("1 & 2 <3".to_string())))
        );
    }

    proptest! {
        #[test]
        fn prop_escaped_text_round_trips(text in "[a-z0-9<>&\"' \t\n\u{e9}\u{a0}]{0,30}") {
            let xml = format!("<a>{}</a>", quick_xml::escape::escape(&text));
            let expected = text.trim_matches(is_xml_whitespace);

            let actual = root_text(&xml);

            prop_assert_eq!(actual.as_deref(), (!expected.is_empty()).then_some(expected));
        }

        #[test]
        fn prop_indentation_does_not_change_text(text in "[a-z0-9<>&\"' \t\u{e9}\u{a0}]{0,30}") {
            let escaped = quick_xml::escape::escape(&text);

            prop_assert_eq!(
                root_text(&format!("<a>{escaped}</a>")),
                root_text(&format!("<a>\n  {escaped}\n</a>"))
            );
        }
    }
}
