//! Reading and writing node attributes by name.

use super::*;
use crate::NodeKind;

impl Node {
    /// Returns the value of the specified attribute, if present.
    ///
    /// `line`/`end_line` are handled here, not per-variant below, since `position` is common
    /// to every node.
    pub fn attr(&self, attr: &str) -> Option<AttrValue> {
        match attr {
            attr_keys::LINE => return self.position().map(|p| AttrValue::Integer(p.start.line as i64)),
            attr_keys::END_LINE => return self.position().map(|p| AttrValue::Integer(p.end.line as i64)),
            _ => {}
        }

        match self {
            Node::Footnote(Footnote { ident, values, .. }) => match attr {
                attr_keys::IDENT => Some(AttrValue::String(ident.clone())),
                attr_keys::VALUE => Some(AttrValue::String(values_to_string(values, &RenderOptions::default()))),
                attr_keys::VALUES | attr_keys::CHILDREN => Some(AttrValue::Array(values.clone())),
                _ => None,
            },
            Node::Html(Html { value, .. }) => match attr {
                attr_keys::VALUE => Some(AttrValue::String(value.clone())),
                _ => None,
            },
            Node::Text(Text { value, .. }) => match attr {
                attr_keys::VALUE => Some(AttrValue::String(value.clone())),
                _ => None,
            },
            Node::Code(Code {
                value,
                lang,
                meta,
                fence,
                ..
            }) => match attr {
                attr_keys::VALUE => Some(AttrValue::String(value.clone())),
                attr_keys::LANG => lang.clone().map(AttrValue::String),
                attr_keys::META => meta.clone().map(AttrValue::String),
                attr_keys::FENCE => Some(AttrValue::Boolean(*fence)),
                _ => None,
            },
            Node::CodeInline(CodeInline { value, .. }) => match attr {
                attr_keys::VALUE => Some(AttrValue::String(value.to_string())),
                _ => None,
            },
            Node::MathInline(MathInline { value, .. }) => match attr {
                attr_keys::VALUE => Some(AttrValue::String(value.to_string())),
                _ => None,
            },
            Node::Math(Math { value, .. }) => match attr {
                attr_keys::VALUE => Some(AttrValue::String(value.clone())),
                _ => None,
            },
            Node::Yaml(Yaml { value, .. }) => match attr {
                attr_keys::VALUE => Some(AttrValue::String(value.clone())),
                _ => None,
            },
            Node::Toml(Toml { value, .. }) => match attr {
                attr_keys::VALUE => Some(AttrValue::String(value.clone())),
                _ => None,
            },
            Node::Image(Image { alt, url, title, .. }) => match attr {
                attr_keys::ALT => Some(AttrValue::String(alt.clone())),
                attr_keys::URL => Some(AttrValue::String(url.clone())),
                attr_keys::TITLE => title.clone().map(AttrValue::String),
                _ => None,
            },
            Node::ImageRef(ImageRef { alt, ident, label, .. }) => match attr {
                attr_keys::ALT => Some(AttrValue::String(alt.clone())),
                attr_keys::IDENT => Some(AttrValue::String(ident.clone())),
                attr_keys::LABEL => label.clone().map(AttrValue::String),
                _ => None,
            },
            Node::Link(Link { url, title, values, .. }) => match attr {
                attr_keys::URL => Some(AttrValue::String(url.as_str().to_string())),
                attr_keys::TITLE => title.as_ref().map(|t| AttrValue::String(t.to_value())),
                attr_keys::VALUE => Some(AttrValue::String(values_to_string(values, &RenderOptions::default()))),
                attr_keys::VALUES | attr_keys::CHILDREN => Some(AttrValue::Array(values.clone())),
                _ => None,
            },
            #[cfg(feature = "wikilink")]
            Node::WikiLink(WikiLink { target, text, .. }) => match attr {
                attr_keys::URL => Some(AttrValue::String(target.clone())),
                attr_keys::VALUE => Some(AttrValue::String(text.clone().unwrap_or_else(|| target.clone()))),
                _ => None,
            },
            #[cfg(feature = "callout")]
            Node::Callout(Callout {
                kind,
                fold,
                title,
                values,
                ..
            }) => match attr {
                attr_keys::KIND => Some(AttrValue::String(kind.clone())),
                attr_keys::FOLD => fold.map(|fold| AttrValue::String(fold.to_string())),
                attr_keys::TITLE => title.clone().map(AttrValue::String),
                attr_keys::VALUE => Some(AttrValue::String(values_to_string(values, &RenderOptions::default()))),
                attr_keys::VALUES | attr_keys::CHILDREN => Some(AttrValue::Array(values.clone())),
                _ => None,
            },
            #[cfg(feature = "embed")]
            Node::Embed(Embed { target, display, .. }) => match attr {
                attr_keys::URL => Some(AttrValue::String(target.clone())),
                attr_keys::VALUE => Some(AttrValue::String(display.clone().unwrap_or_else(|| target.clone()))),
                _ => None,
            },
            Node::LinkRef(LinkRef { ident, label, .. }) => match attr {
                attr_keys::IDENT => Some(AttrValue::String(ident.clone())),
                attr_keys::LABEL => label.clone().map(AttrValue::String),
                _ => None,
            },
            Node::FootnoteRef(FootnoteRef { ident, label, .. }) => match attr {
                attr_keys::IDENT => Some(AttrValue::String(ident.clone())),
                attr_keys::LABEL => label.clone().map(AttrValue::String),
                _ => None,
            },
            Node::Definition(Definition {
                ident,
                url,
                title,
                label,
                ..
            }) => match attr {
                attr_keys::IDENT => Some(AttrValue::String(ident.clone())),
                attr_keys::URL => Some(AttrValue::String(url.as_str().to_string())),
                attr_keys::TITLE => title.as_ref().map(|t| AttrValue::String(t.to_value())),
                attr_keys::LABEL => label.clone().map(AttrValue::String),
                _ => None,
            },
            Node::Heading(Heading { depth, values, .. }) => match attr {
                attr_keys::DEPTH | attr_keys::LEVEL => Some(AttrValue::Integer(i64::from(depth.get()))),
                attr_keys::VALUE => Some(AttrValue::String(values_to_string(values, &RenderOptions::default()))),
                attr_keys::VALUES | attr_keys::CHILDREN => Some(AttrValue::Array(values.clone())),
                _ => None,
            },
            Node::List(List {
                index,
                level,
                ordered,
                checked,
                values,
                ..
            }) => match attr {
                attr_keys::INDEX => Some(AttrValue::Integer(*index as i64)),
                attr_keys::LEVEL => Some(AttrValue::Integer(*level as i64)),
                attr_keys::ORDERED => Some(AttrValue::Boolean(*ordered)),
                attr_keys::CHECKED => checked.map(AttrValue::Boolean),
                attr_keys::VALUE => Some(AttrValue::String(values_to_string(values, &RenderOptions::default()))),
                attr_keys::VALUES | attr_keys::CHILDREN => Some(AttrValue::Array(values.clone())),
                _ => None,
            },
            Node::TableCell(TableCell {
                column, row, values, ..
            }) => match attr {
                attr_keys::COLUMN => Some(AttrValue::Integer(*column as i64)),
                attr_keys::ROW => Some(AttrValue::Integer(*row as i64)),
                attr_keys::VALUE => Some(AttrValue::String(values_to_string(values, &RenderOptions::default()))),
                attr_keys::VALUES | attr_keys::CHILDREN => Some(AttrValue::Array(values.clone())),
                _ => None,
            },
            Node::TableAlign(TableAlign { align, .. }) => match attr {
                attr_keys::ALIGN => Some(AttrValue::String(
                    align.iter().map(|a| a.to_string()).collect::<Vec<_>>().join(","),
                )),
                _ => None,
            },
            Node::MdxFlowExpression(MdxFlowExpression { value, .. }) => match attr {
                attr_keys::VALUE => Some(AttrValue::String(value.to_string())),
                _ => None,
            },
            Node::MdxTextExpression(MdxTextExpression { value, .. }) => match attr {
                attr_keys::VALUE => Some(AttrValue::String(value.to_string())),
                _ => None,
            },
            Node::MdxJsEsm(MdxJsEsm { value, .. }) => match attr {
                attr_keys::VALUE => Some(AttrValue::String(value.to_string())),
                _ => None,
            },
            Node::MdxJsxFlowElement(MdxJsxFlowElement { name, children, .. }) => match attr {
                attr_keys::NAME => name.clone().map(AttrValue::String),
                attr_keys::VALUES | attr_keys::CHILDREN => Some(AttrValue::Array(children.clone())),
                _ => None,
            },
            Node::MdxJsxTextElement(MdxJsxTextElement { name, children, .. }) => match attr {
                attr_keys::NAME => name.as_ref().map(|n| AttrValue::String(n.to_string())),
                attr_keys::VALUES | attr_keys::CHILDREN => Some(AttrValue::Array(children.clone())),
                _ => None,
            },
            Node::Strong(Strong { values, .. })
            | Node::Blockquote(Blockquote { values, .. })
            | Node::Delete(Delete { values, .. })
            | Node::Emphasis(Emphasis { values, .. })
            | Node::TableRow(TableRow { values, .. })
            | Node::Fragment(Fragment { values, .. }) => match attr {
                attr_keys::VALUE => Some(AttrValue::String(values_to_string(values, &RenderOptions::default()))),
                attr_keys::VALUES | attr_keys::CHILDREN => Some(AttrValue::Array(values.clone())),
                _ => None,
            },
            Node::Break(_) | Node::HorizontalRule(_) | Node::Empty => None,
        }
    }

    /// Sets the value of the specified attribute for the node.
    ///
    /// Fails when the node has no settable attribute `attr` or `value` does not fit it.
    /// A heading depth outside of 1 to 6 is clamped.
    pub fn set_attr(&mut self, attr: &str, value: impl Into<AttrValue>) -> Result<(), SetAttrError> {
        let value = value.into();
        let value_str = value.as_string();
        let kind = self.kind();
        let unknown = || SetAttrError::UnknownAttr {
            attr: attr.to_string(),
            kind,
        };
        let opt = |s: String| if s.is_empty() { None } else { Some(s) };

        match self {
            Node::Footnote(Footnote { ident: v, .. })
            | Node::FootnoteRef(FootnoteRef { ident: v, .. })
            | Node::LinkRef(LinkRef { ident: v, .. })
                if attr == attr_keys::IDENT =>
            {
                *v = value_str;
            }
            Node::Html(Html { value: v, .. })
            | Node::Text(Text { value: v, .. })
            | Node::Math(Math { value: v, .. })
            | Node::Yaml(Yaml { value: v, .. })
            | Node::Toml(Toml { value: v, .. })
            | Node::Code(Code { value: v, .. })
                if attr == attr_keys::VALUE =>
            {
                *v = value_str;
            }
            Node::CodeInline(CodeInline { value: v, .. })
            | Node::MathInline(MathInline { value: v, .. })
            | Node::MdxFlowExpression(MdxFlowExpression { value: v, .. })
            | Node::MdxTextExpression(MdxTextExpression { value: v, .. })
            | Node::MdxJsEsm(MdxJsEsm { value: v, .. })
                if attr == attr_keys::VALUE =>
            {
                *v = value_str.into();
            }
            Node::Code(c) => match attr {
                attr_keys::LANG | "language" => c.lang = opt(value_str),
                attr_keys::META => c.meta = opt(value_str),
                attr_keys::FENCE => c.fence = to_bool(attr, &value)?,
                _ => return Err(unknown()),
            },
            Node::Image(i) => match attr {
                attr_keys::ALT => i.alt = value_str,
                attr_keys::URL => i.url = value_str,
                attr_keys::TITLE => i.title = opt(value_str),
                _ => return Err(unknown()),
            },
            Node::ImageRef(i) => match attr {
                attr_keys::ALT => i.alt = value_str,
                attr_keys::IDENT => i.ident = value_str,
                attr_keys::LABEL => i.label = opt(value_str),
                _ => return Err(unknown()),
            },
            Node::Link(l) => match attr {
                attr_keys::URL => l.url = Url::new(value_str),
                attr_keys::TITLE => l.title = opt(value_str).map(Title::new),
                _ => return Err(unknown()),
            },
            Node::LinkRef(l) => match attr {
                attr_keys::LABEL => l.label = opt(value_str),
                _ => return Err(unknown()),
            },
            Node::FootnoteRef(f) => match attr {
                attr_keys::LABEL => f.label = opt(value_str),
                _ => return Err(unknown()),
            },
            Node::Definition(d) => match attr {
                attr_keys::IDENT => d.ident = value_str,
                attr_keys::URL => d.url = Url::new(value_str),
                attr_keys::TITLE => d.title = opt(value_str).map(Title::new),
                attr_keys::LABEL => d.label = opt(value_str),
                _ => return Err(unknown()),
            },
            Node::Heading(h) => match attr {
                attr_keys::DEPTH | attr_keys::LEVEL => {
                    h.depth = HeadingDepth::saturating(to_int(attr, &value)?);
                }
                _ => return Err(unknown()),
            },
            Node::List(l) => match attr {
                attr_keys::INDEX => l.index = to_uint(attr, &value)?,
                attr_keys::LEVEL => l.level = to_uint(attr, &value)?,
                attr_keys::ORDERED => l.ordered = to_bool(attr, &value)?,
                attr_keys::CHECKED => {
                    l.checked = if value_str.is_empty() {
                        None
                    } else {
                        Some(to_bool(attr, &value)?)
                    };
                }
                _ => return Err(unknown()),
            },
            Node::TableCell(c) => match attr {
                attr_keys::COLUMN => c.column = to_uint(attr, &value)?,
                attr_keys::ROW => c.row = to_uint(attr, &value)?,
                _ => return Err(unknown()),
            },
            Node::TableAlign(th) => match attr {
                attr_keys::ALIGN => th.align = value_str.split(',').map(|s| s.trim().into()).collect(),
                _ => return Err(unknown()),
            },
            Node::MdxJsxFlowElement(m) => match attr {
                attr_keys::NAME => m.name = opt(value_str),
                _ => return Err(unknown()),
            },
            Node::MdxJsxTextElement(m) => match attr {
                attr_keys::NAME => m.name = opt(value_str).map(Into::into),
                _ => return Err(unknown()),
            },
            #[cfg(feature = "wikilink")]
            Node::WikiLink(w) => match attr {
                attr_keys::URL => w.target = value_str,
                attr_keys::VALUE => w.text = opt(value_str),
                _ => return Err(unknown()),
            },
            #[cfg(feature = "callout")]
            Node::Callout(c) => match attr {
                attr_keys::KIND => c.kind = value_str,
                attr_keys::FOLD => c.fold = value_str.chars().next().filter(|c| matches!(c, '+' | '-')),
                attr_keys::TITLE => c.title = opt(value_str),
                _ => return Err(unknown()),
            },
            #[cfg(feature = "embed")]
            Node::Embed(e) => match attr {
                attr_keys::URL => e.target = value_str,
                attr_keys::VALUE => e.display = opt(value_str),
                _ => return Err(unknown()),
            },
            _ => return Err(unknown()),
        }
        Ok(())
    }
}

/// The error of [`Node::set_attr`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetAttrError {
    /// The node kind has no settable attribute with this name.
    UnknownAttr {
        /// The attribute name.
        attr: String,
        /// The kind of the node.
        kind: NodeKind,
    },
    /// The value does not fit the attribute.
    InvalidValue {
        /// The attribute name.
        attr: String,
        /// What the attribute accepts.
        expected: &'static str,
        /// The rejected value.
        value: String,
    },
}

impl std::fmt::Display for SetAttrError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SetAttrError::UnknownAttr { attr, kind } => {
                write!(f, "`{attr}` is not a settable attribute of {kind:?}")
            }
            SetAttrError::InvalidValue { attr, expected, value } => {
                write!(f, "`{value}` is not {expected} for `{attr}`")
            }
        }
    }
}

impl std::error::Error for SetAttrError {}

fn invalid(attr: &str, expected: &'static str, value: &AttrValue) -> SetAttrError {
    SetAttrError::InvalidValue {
        attr: attr.to_string(),
        expected,
        value: value.as_string(),
    }
}

fn to_bool(attr: &str, value: &AttrValue) -> Result<bool, SetAttrError> {
    match value {
        AttrValue::Boolean(b) => Ok(*b),
        AttrValue::String(s) if s == "true" => Ok(true),
        AttrValue::String(s) if s == "false" => Ok(false),
        _ => Err(invalid(attr, "a boolean", value)),
    }
}

fn to_int(attr: &str, value: &AttrValue) -> Result<i64, SetAttrError> {
    match value {
        AttrValue::Integer(i) => Some(*i),
        AttrValue::Number(n) if n.fract() == 0.0 => Some(*n as i64),
        AttrValue::String(s) => s.parse().ok(),
        _ => None,
    }
    .ok_or_else(|| invalid(attr, "an integer", value))
}

fn to_uint<T: TryFrom<i64>>(attr: &str, value: &AttrValue) -> Result<T, SetAttrError> {
    T::try_from(to_int(attr, value)?).map_err(|_| invalid(attr, "an integer in range", value))
}
