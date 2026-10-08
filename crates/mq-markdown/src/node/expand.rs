//! Turning Obsidian links and inline links written in text into nodes.

use super::*;

impl Node {
    /// Returns true if any `Text` descendant contains `[[`, covering both
    /// embed (`![[`) and wikilink (`[[`) patterns.
    #[cfg(any(feature = "embed", feature = "wikilink"))]
    fn values_contain_links(values: &[Node]) -> bool {
        values.iter().any(|n| match n {
            Node::Text(t) => t.value.contains("[["),
            Node::Heading(h) => Self::values_contain_links(&h.values),
            Node::Blockquote(b) => Self::values_contain_links(&b.values),
            #[cfg(feature = "callout")]
            Node::Callout(c) => Self::values_contain_links(&c.values),
            Node::List(l) => Self::values_contain_links(&l.values),
            Node::Strong(s) => Self::values_contain_links(&s.values),
            Node::Emphasis(e) => Self::values_contain_links(&e.values),
            Node::Delete(d) => Self::values_contain_links(&d.values),
            Node::TableCell(tc) => Self::values_contain_links(&tc.values),
            Node::TableRow(tr) => Self::values_contain_links(&tr.values),
            Node::Footnote(f) => Self::values_contain_links(&f.values),
            _ => false,
        })
    }

    /// Shared tree walker used by all expand functions. Recurses into container
    /// nodes and delegates `Text` node processing to `parse_into`.
    #[cfg(any(feature = "embed", feature = "wikilink"))]
    fn expand_with(nodes: Vec<Node>, parse_into: fn(&str, Option<Position>, &mut Vec<Node>)) -> Vec<Node> {
        let mut result = Vec::with_capacity(nodes.len());
        for node in nodes {
            Self::expand_with_into(node, &mut result, parse_into);
        }
        result
    }

    #[cfg(any(feature = "embed", feature = "wikilink"))]
    fn expand_with_into(node: Node, out: &mut Vec<Node>, parse_into: fn(&str, Option<Position>, &mut Vec<Node>)) {
        match node {
            Node::Text(Text { ref value, .. }) if !value.contains("[[") => out.push(node),
            Node::Text(Text { value, position }) => parse_into(&value, position, out),
            Node::Heading(mut h) => {
                if Self::values_contain_links(&h.values) {
                    h.values = Self::expand_with(h.values, parse_into);
                }
                out.push(Node::Heading(h));
            }
            Node::Blockquote(mut b) => {
                if Self::values_contain_links(&b.values) {
                    b.values = Self::expand_with(b.values, parse_into);
                }
                out.push(Node::Blockquote(b));
            }
            #[cfg(feature = "callout")]
            Node::Callout(mut c) => {
                if Self::values_contain_links(&c.values) {
                    c.values = Self::expand_with(c.values, parse_into);
                }
                out.push(Node::Callout(c));
            }
            Node::List(mut l) => {
                if Self::values_contain_links(&l.values) {
                    l.values = Self::expand_with(l.values, parse_into);
                }
                out.push(Node::List(l));
            }
            Node::Strong(mut s) => {
                if Self::values_contain_links(&s.values) {
                    s.values = Self::expand_with(s.values, parse_into);
                }
                out.push(Node::Strong(s));
            }
            Node::Emphasis(mut e) => {
                if Self::values_contain_links(&e.values) {
                    e.values = Self::expand_with(e.values, parse_into);
                }
                out.push(Node::Emphasis(e));
            }
            Node::Delete(mut d) => {
                if Self::values_contain_links(&d.values) {
                    d.values = Self::expand_with(d.values, parse_into);
                }
                out.push(Node::Delete(d));
            }
            Node::TableCell(mut tc) => {
                if Self::values_contain_links(&tc.values) {
                    tc.values = Self::expand_with(tc.values, parse_into);
                }
                out.push(Node::TableCell(tc));
            }
            Node::TableRow(mut tr) => {
                if Self::values_contain_links(&tr.values) {
                    tr.values = Self::expand_with(tr.values, parse_into);
                }
                out.push(Node::TableRow(tr));
            }
            Node::Footnote(mut f) => {
                if Self::values_contain_links(&f.values) {
                    f.values = Self::expand_with(f.values, parse_into);
                }
                out.push(Node::Footnote(f));
            }
            other => out.push(other),
        }
    }

    /// Recursively scans a node list and converts `![[target]]` / `![[target|display]]`
    /// patterns inside `Text` nodes into `Embed` nodes.
    #[cfg(feature = "embed")]
    pub fn expand_embeds(nodes: Vec<Node>) -> Vec<Node> {
        Self::expand_with(nodes, Self::parse_embeds_into)
    }

    /// Splits a text string on `![[...]]` patterns, pushing a mix of `Text`
    /// and `Embed` nodes into `out`.
    #[cfg(feature = "embed")]
    pub(super) fn parse_embeds_into(text: &str, position: Option<Position>, out: &mut Vec<Node>) {
        let mut last_end = 0;
        let mut search_from = 0;
        let mut found_any = false;

        while let Some(bang_rel) = text[search_from..].find("![[") {
            let bang = search_from + bang_rel;
            let inner_start = bang + 3;

            let Some(close_rel) = text[inner_start..].find("]]") else {
                search_from = bang + 1;
                continue;
            };
            let close = inner_start + close_rel;
            let content = &text[inner_start..close];

            if content.contains('[') || content.contains(']') {
                search_from = bang + 1;
                continue;
            }

            found_any = true;
            if last_end < bang {
                out.push(Node::Text(Text {
                    value: text[last_end..bang].to_string(),
                    position: position.clone(),
                }));
            }
            let (target, display) = if let Some(pipe) = content.find('|') {
                (
                    content[..pipe].trim().to_string(),
                    Some(content[pipe + 1..].trim().to_string()),
                )
            } else {
                (content.trim().to_string(), None)
            };
            out.push(Node::Embed(Embed {
                target,
                display,
                position: position.clone(),
            }));
            last_end = close + 2;
            search_from = close + 2;
        }

        if !found_any {
            out.push(Node::Text(Text {
                value: text.to_string(),
                position,
            }));
            return;
        }

        if last_end < text.len() {
            out.push(Node::Text(Text {
                value: text[last_end..].to_string(),
                position,
            }));
        }
    }

    /// Recursively scans a node list and converts `[[target]]` / `[[target|text]]`
    /// patterns inside `Text` nodes into `WikiLink` nodes.
    #[cfg(feature = "wikilink")]
    pub fn expand_wikilinks(nodes: Vec<Node>) -> Vec<Node> {
        Self::expand_with(nodes, Self::parse_wikilinks_into)
    }

    /// Splits a text string on `[[...]]` patterns, pushing a mix of `Text`
    /// and `WikiLink` nodes into `out`.
    #[cfg(feature = "wikilink")]
    fn parse_wikilinks_into(text: &str, position: Option<Position>, out: &mut Vec<Node>) {
        let mut last_end = 0;
        let mut search_from = 0;
        let mut found_any = false;

        while let Some(open_rel) = text[search_from..].find("[[") {
            let open = search_from + open_rel;
            let inner_start = open + 2;

            let Some(close_rel) = text[inner_start..].find("]]") else {
                break;
            };
            let close = inner_start + close_rel;
            let content = &text[inner_start..close];

            if content.contains('[') || content.contains(']') {
                // nested brackets — skip past this '[' and retry
                search_from = open + 1;
                continue;
            }

            found_any = true;
            if last_end < open {
                out.push(Node::Text(Text {
                    value: text[last_end..open].to_string(),
                    position: position.clone(),
                }));
            }
            let (target, text_part) = if let Some(pipe) = content.find('|') {
                (content[..pipe].trim(), Some(content[pipe + 1..].trim()))
            } else {
                (content.trim(), None)
            };
            out.push(Node::WikiLink(WikiLink {
                target: target.to_string(),
                text: text_part.map(|t| t.to_string()),
                position: position.clone(),
            }));
            last_end = close + 2;
            search_from = close + 2;
        }

        if !found_any {
            out.push(Node::Text(Text {
                value: text.to_string(),
                position,
            }));
            return;
        }

        if last_end < text.len() {
            out.push(Node::Text(Text {
                value: text[last_end..].to_string(),
                position,
            }));
        }
    }

    /// Splits a text string on `[[...]]` patterns, returning a mix of `Text`
    /// and `WikiLink` nodes. Used only in tests.
    #[cfg(all(feature = "wikilink", test))]
    pub(super) fn parse_wikilinks_in_text(text: &str, position: Option<Position>) -> Vec<Node> {
        let mut result = Vec::new();
        Self::parse_wikilinks_into(text, position, &mut result);
        result
    }

    /// Recursively scans a node list and converts both `![[target]]` (embed) and
    /// `[[target]]` (wikilink) patterns in a single tree traversal. More efficient
    /// than running `expand_embeds` followed by `expand_wikilinks` separately, which
    /// traverses the tree and scans each `Text` node twice.
    #[cfg(all(feature = "embed", feature = "wikilink"))]
    pub fn expand_inline_links(nodes: Vec<Node>) -> Vec<Node> {
        Self::expand_with(nodes, Self::parse_inline_links_into)
    }

    /// Splits a text string on `![[...]]` and `[[...]]` patterns in a single pass,
    /// pushing a mix of `Text`, `Embed`, and `WikiLink` nodes into `out`.
    /// A `[[` immediately preceded by `!` is treated as an embed; otherwise as a wikilink.
    #[cfg(all(feature = "embed", feature = "wikilink"))]
    pub(super) fn parse_inline_links_into(text: &str, position: Option<Position>, out: &mut Vec<Node>) {
        let mut last_end = 0;
        let mut search_from = 0;
        let mut found_any = false;

        while let Some(open_rel) = text[search_from..].find("[[") {
            let open = search_from + open_rel;
            let inner_start = open + 2;

            let Some(close_rel) = text[inner_start..].find("]]") else {
                break;
            };
            let close = inner_start + close_rel;
            let content = &text[inner_start..close];

            if content.contains('[') || content.contains(']') {
                search_from = open + 1;
                continue;
            }

            let is_embed = text.as_bytes()[..open].last() == Some(&b'!');
            let token_start = if is_embed { open - 1 } else { open };

            found_any = true;
            if last_end < token_start {
                out.push(Node::Text(Text {
                    value: text[last_end..token_start].to_string(),
                    position: position.clone(),
                }));
            }

            let (target, opt_part) = if let Some(pipe) = content.find('|') {
                (content[..pipe].trim(), Some(content[pipe + 1..].trim()))
            } else {
                (content.trim(), None)
            };

            if is_embed {
                out.push(Node::Embed(Embed {
                    target: target.to_string(),
                    display: opt_part.map(|s| s.to_string()),
                    position: position.clone(),
                }));
            } else {
                out.push(Node::WikiLink(WikiLink {
                    target: target.to_string(),
                    text: opt_part.map(|s| s.to_string()),
                    position: position.clone(),
                }));
            }

            last_end = close + 2;
            search_from = close + 2;
        }

        if !found_any {
            out.push(Node::Text(Text {
                value: text.to_string(),
                position,
            }));
            return;
        }

        if last_end < text.len() {
            out.push(Node::Text(Text {
                value: text[last_end..].to_string(),
                position,
            }));
        }
    }
}
