//! `_xml_parse` builtin: parses an XML string into nested `{tag, attributes, children, text}`
//! dicts, the same shape [`super::css`] produces for HTML.

use crate::DictMap;
use crate::runtime::runtime_value::RuntimeValue;
use crate::{Ident, Shared};

use quick_xml::XmlVersion;
use quick_xml::events::{BytesStart, Event};

use super::Error;

/// Character data and entity references collected until the next tag, comment or CDATA.
///
/// Only the outer edges of the collected text are trimmed (pretty-printing indentation), so
/// whitespace next to an entity reference (`x &amp; y`) is preserved.
#[derive(Default)]
struct PendingText {
    buf: String,
    tail_start: Option<usize>,
}

impl PendingText {
    fn is_xml_whitespace(c: char) -> bool {
        matches!(c, ' ' | '\t' | '\r' | '\n')
    }

    fn push_text(&mut self, text: &str) {
        let text = if self.buf.is_empty() {
            text.trim_start_matches(Self::is_xml_whitespace)
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

    fn take(&mut self) -> Option<String> {
        if let Some(start) = self.tail_start.take() {
            let kept = self.buf[start..].trim_end_matches(Self::is_xml_whitespace).len();
            self.buf.truncate(start + kept);
        }
        let text = std::mem::take(&mut self.buf);
        (!text.is_empty()).then_some(text)
    }
}

/// An element whose end tag has not been seen yet.
struct OpenElement {
    tag: String,
    attributes: DictMap,
    children: Vec<RuntimeValue>,
    text: Option<String>,
}

impl OpenElement {
    fn new(tag: String, attributes: DictMap) -> Self {
        Self {
            tag,
            attributes,
            children: Vec::new(),
            text: None,
        }
    }

    fn append_text(&mut self, text: &str) {
        match &mut self.text {
            Some(existing) => existing.push_str(text),
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
pub(super) fn parse_xml(xml: &str) -> Result<RuntimeValue, Error> {
    let mut reader = quick_xml::Reader::from_str(xml);
    let mut buf = Vec::new();
    let mut stack: Vec<OpenElement> = Vec::new();
    let mut pending = PendingText::default();

    loop {
        let event = reader.read_event_into(&mut buf);

        if !matches!(event, Ok(Event::Text(_) | Event::GeneralRef(_)))
            && let (Some(text), Some(parent)) = (pending.take(), stack.last_mut())
        {
            parent.append_text(&text);
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
                    parent.append_text(e.as_ref());
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
            Ok(Event::Eof) => return Ok(RuntimeValue::NONE),
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
