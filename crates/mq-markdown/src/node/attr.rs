//! Reading and writing node attributes by name.

use super::*;

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
                attr_keys::DEPTH | attr_keys::LEVEL => Some(AttrValue::Integer(*depth as i64)),
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

    /// Sets the value of the specified attribute for the node, if supported.
    pub fn set_attr(&mut self, attr: &str, value: impl Into<AttrValue>) {
        let value = value.into();
        let value_str = value.as_string();

        match self {
            Node::Footnote(f) => {
                if attr == attr_keys::IDENT {
                    f.ident = value_str;
                }
            }
            Node::Html(h) => {
                if attr == attr_keys::VALUE {
                    h.value = value_str;
                }
            }
            Node::Text(t) => {
                if attr == attr_keys::VALUE {
                    t.value = value_str;
                }
            }
            Node::Code(c) => match attr {
                attr_keys::VALUE => {
                    c.value = value_str;
                }
                attr_keys::LANG | "language" => {
                    c.lang = if value_str.is_empty() { None } else { Some(value_str) };
                }
                attr_keys::META => {
                    c.meta = if value_str.is_empty() { None } else { Some(value_str) };
                }
                attr_keys::FENCE => {
                    c.fence = match value {
                        AttrValue::Boolean(b) => b,
                        _ => value_str == "true",
                    };
                }
                _ => (),
            },
            Node::CodeInline(ci) => {
                if attr == attr_keys::VALUE {
                    ci.value = value_str.into();
                }
            }
            Node::MathInline(mi) => {
                if attr == attr_keys::VALUE {
                    mi.value = value_str.into();
                }
            }
            Node::Math(m) => {
                if attr == attr_keys::VALUE {
                    m.value = value_str;
                }
            }
            Node::Yaml(y) => {
                if attr == attr_keys::VALUE {
                    y.value = value_str;
                }
            }
            Node::Toml(t) => {
                if attr == attr_keys::VALUE {
                    t.value = value_str;
                }
            }
            Node::Image(i) => match attr {
                attr_keys::ALT => {
                    i.alt = value_str;
                }
                attr_keys::URL => {
                    i.url = value_str;
                }
                attr_keys::TITLE => {
                    i.title = if value_str.is_empty() { None } else { Some(value_str) };
                }
                _ => (),
            },
            Node::ImageRef(i) => match attr {
                attr_keys::ALT => {
                    i.alt = value_str;
                }
                attr_keys::IDENT => {
                    i.ident = value_str;
                }
                attr_keys::LABEL => {
                    i.label = if value_str.is_empty() { None } else { Some(value_str) };
                }
                _ => (),
            },
            Node::Link(l) => match attr {
                attr_keys::URL => {
                    l.url = Url::new(value_str);
                }
                attr_keys::TITLE => {
                    l.title = if value_str.is_empty() {
                        None
                    } else {
                        Some(Title::new(value_str))
                    };
                }
                _ => (),
            },
            Node::LinkRef(l) => match attr {
                attr_keys::IDENT => {
                    l.ident = value_str;
                }
                attr_keys::LABEL => {
                    l.label = if value_str.is_empty() { None } else { Some(value_str) };
                }
                _ => (),
            },
            Node::FootnoteRef(f) => match attr {
                attr_keys::IDENT => {
                    f.ident = value_str;
                }
                attr_keys::LABEL => {
                    f.label = if value_str.is_empty() { None } else { Some(value_str) };
                }
                _ => (),
            },
            Node::Definition(d) => match attr {
                attr_keys::IDENT => {
                    d.ident = value_str;
                }
                attr_keys::URL => {
                    d.url = Url::new(value_str);
                }
                attr_keys::TITLE => {
                    d.title = if value_str.is_empty() {
                        None
                    } else {
                        Some(Title::new(value_str))
                    };
                }
                attr_keys::LABEL => {
                    d.label = if value_str.is_empty() { None } else { Some(value_str) };
                }
                _ => (),
            },
            Node::Heading(h) => match attr {
                attr_keys::DEPTH | attr_keys::LEVEL => {
                    h.depth = match value {
                        AttrValue::Integer(i) => i as u8,
                        _ => value_str.parse::<u8>().unwrap_or(h.depth),
                    };
                }
                _ => (),
            },
            Node::List(l) => match attr {
                attr_keys::INDEX => {
                    l.index = match value {
                        AttrValue::Integer(i) => i as usize,
                        _ => value_str.parse::<usize>().unwrap_or(l.index),
                    };
                }
                attr_keys::LEVEL => {
                    l.level = match value {
                        AttrValue::Integer(i) => i as u8,
                        _ => value_str.parse::<u8>().unwrap_or(l.level),
                    };
                }
                attr_keys::ORDERED => {
                    l.ordered = match value {
                        AttrValue::Boolean(b) => b,
                        _ => value_str == "true",
                    };
                }
                attr_keys::CHECKED => {
                    l.checked = if value_str.is_empty() {
                        None
                    } else {
                        Some(match value {
                            AttrValue::Boolean(b) => b,
                            _ => value_str == "true",
                        })
                    };
                }
                _ => (),
            },
            Node::TableCell(c) => match attr {
                attr_keys::COLUMN => {
                    c.column = match value {
                        AttrValue::Integer(i) => i as usize,
                        _ => value_str.parse::<usize>().unwrap_or(c.column),
                    };
                }
                attr_keys::ROW => {
                    c.row = match value {
                        AttrValue::Integer(i) => i as usize,
                        _ => value_str.parse::<usize>().unwrap_or(c.row),
                    };
                }
                _ => (),
            },
            Node::TableAlign(th) => {
                if attr == attr_keys::ALIGN {
                    th.align = value_str.split(',').map(|s| s.trim().into()).collect();
                }
            }
            Node::MdxFlowExpression(m) => {
                if attr == attr_keys::VALUE {
                    m.value = value_str.into();
                }
            }
            Node::MdxTextExpression(m) => {
                if attr == attr_keys::VALUE {
                    m.value = value_str.into();
                }
            }
            Node::MdxJsEsm(m) => {
                if attr == attr_keys::VALUE {
                    m.value = value_str.into();
                }
            }
            Node::MdxJsxFlowElement(m) => {
                if attr == attr_keys::NAME {
                    m.name = if value_str.is_empty() { None } else { Some(value_str) };
                }
            }
            Node::MdxJsxTextElement(m) => {
                if attr == attr_keys::NAME {
                    m.name = if value_str.is_empty() {
                        None
                    } else {
                        Some(value_str.into())
                    };
                }
            }
            Node::Delete(_)
            | Node::Blockquote(_)
            | Node::Emphasis(_)
            | Node::Strong(_)
            | Node::TableRow(_)
            | Node::Break(_)
            | Node::HorizontalRule(_)
            | Node::Fragment(_)
            | Node::Empty => (),
            #[cfg(feature = "wikilink")]
            Node::WikiLink(w) => match attr {
                attr_keys::URL => w.target = value_str,
                attr_keys::VALUE => w.text = if value_str.is_empty() { None } else { Some(value_str) },
                _ => (),
            },
            #[cfg(feature = "callout")]
            Node::Callout(c) => match attr {
                attr_keys::KIND => c.kind = value_str,
                attr_keys::FOLD => c.fold = value_str.chars().next().filter(|c| matches!(c, '+' | '-')),
                attr_keys::TITLE => c.title = if value_str.is_empty() { None } else { Some(value_str) },
                _ => (),
            },
            #[cfg(feature = "embed")]
            Node::Embed(e) => match attr {
                attr_keys::URL => e.target = value_str,
                attr_keys::VALUE => e.display = if value_str.is_empty() { None } else { Some(value_str) },
                _ => (),
            },
        }
    }
}
