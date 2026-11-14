use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use tower_lsp_server::ls_types::{DocumentSymbol, DocumentSymbolResponse, SymbolKind, SymbolTag};
use url::Url;

use crate::position::LineIndex;

struct Node<'a> {
    symbol: &'a mq_hir::Symbol,
    kind: SymbolKind,
    /// Name range.
    selection: mq_lang::Range,
    /// Name range extended over everything the symbol owns.
    full: mq_lang::Range,
    parent: Option<mq_hir::SymbolId>,
}

fn kind_of(hir: &mq_hir::Hir, symbol: &mq_hir::Symbol) -> Option<SymbolKind> {
    match symbol.kind {
        mq_hir::SymbolKind::Function(_) => Some(SymbolKind::FUNCTION),
        mq_hir::SymbolKind::Module(_) => Some(SymbolKind::MODULE),
        mq_hir::SymbolKind::Variable => Some(SymbolKind::VARIABLE),
        // Pattern variables of match arms are local to the arm.
        mq_hir::SymbolKind::PatternVariable { .. } => {
            let mut parent = symbol.parent;
            while let Some(id) = parent {
                let ancestor = hir.symbol(id)?;
                if matches!(ancestor.kind, mq_hir::SymbolKind::DestructuringBinding) {
                    return Some(SymbolKind::VARIABLE);
                }
                parent = ancestor.parent;
            }
            None
        }
        _ => None,
    }
}

fn extend(range: &mut mq_lang::Range, other: mq_lang::Range) {
    range.start = range.start.min(other.start);
    range.end = range.end.max(other.end);
}

/// Document outline: functions, modules and `let`/`var` bindings, nested by definition.
pub(crate) fn response(hir: Arc<RwLock<mq_hir::Hir>>, url: Url, lines: &LineIndex) -> Option<DocumentSymbolResponse> {
    let hir = hir.read().unwrap();
    let source = hir.source_by_url(&url)?;
    let in_source = hir
        .symbols()
        .filter(|(_, symbol)| symbol.source.source_id == Some(source))
        .collect::<Vec<_>>();

    let mut nodes: HashMap<mq_hir::SymbolId, Node> = HashMap::new();
    for (id, symbol) in &in_source {
        let (Some(kind), Some(name), Some(range)) = (kind_of(&hir, symbol), &symbol.value, symbol.source.text_range)
        else {
            continue;
        };
        if name.is_empty() {
            continue;
        }
        // A module symbol spans only its keyword. Its name is the identifier after it.
        let selection = if matches!(symbol.kind, mq_hir::SymbolKind::Module(_)) {
            in_source
                .iter()
                .filter(|(_, s)| matches!(s.kind, mq_hir::SymbolKind::Ident) && s.value.as_ref() == Some(name))
                .filter_map(|(_, s)| s.source.text_range)
                .filter(|r| r.start >= range.end)
                .min_by_key(|r| r.start)
                .unwrap_or(range)
        } else {
            range
        };
        let mut full = range;
        extend(&mut full, selection);
        nodes.insert(
            *id,
            Node {
                symbol,
                kind,
                selection,
                full,
                parent: None,
            },
        );
    }

    // Attach each node to its nearest enclosing node and grow ranges over all descendants.
    let ids = nodes.keys().copied().collect::<Vec<_>>();
    for id in ids {
        let mut parent = nodes[&id].symbol.parent;
        while let Some(pid) = parent {
            if nodes.contains_key(&pid) {
                break;
            }
            parent = hir.symbol(pid).and_then(|s| s.parent);
        }
        nodes.get_mut(&id).unwrap().parent = parent;
    }
    for (_, symbol) in &in_source {
        let Some(range) = symbol.source.text_range else {
            continue;
        };
        let mut parent = symbol.parent;
        while let Some(pid) = parent {
            if let Some(node) = nodes.get_mut(&pid) {
                extend(&mut node.full, range);
            }
            parent = hir.symbol(pid).and_then(|s| s.parent);
        }
    }
    // Ancestors cover their children's full ranges, which may exceed raw descendant symbols.
    let mut order = nodes.keys().copied().collect::<Vec<_>>();
    order.sort_by_key(|id| std::cmp::Reverse(nodes[id].full.start));
    for id in order {
        if let Some(pid) = nodes[&id].parent {
            let full = nodes[&id].full;
            if let Some(parent) = nodes.get_mut(&pid) {
                extend(&mut parent.full, full);
            }
        }
    }

    let mut children: HashMap<Option<mq_hir::SymbolId>, Vec<mq_hir::SymbolId>> = HashMap::new();
    for (id, node) in &nodes {
        children.entry(node.parent).or_default().push(*id);
    }
    for list in children.values_mut() {
        list.sort_by_key(|id| nodes[id].selection.start);
    }

    fn build(
        parent: Option<mq_hir::SymbolId>,
        nodes: &HashMap<mq_hir::SymbolId, Node>,
        children: &HashMap<Option<mq_hir::SymbolId>, Vec<mq_hir::SymbolId>>,
        lines: &LineIndex,
    ) -> Vec<DocumentSymbol> {
        children
            .get(&parent)
            .into_iter()
            .flatten()
            .map(|id| {
                let node = &nodes[id];
                let is_deprecated = node.symbol.is_deprecated();
                let nested = build(Some(*id), nodes, children, lines);
                // `deprecated` is superseded by `tags`, but still set for
                // clients that predate tag support (LSP < 3.15).
                #[allow(deprecated)]
                DocumentSymbol {
                    name: node.symbol.value.as_ref().map(|n| n.to_string()).unwrap_or_default(),
                    detail: None,
                    kind: node.kind,
                    tags: is_deprecated.then(|| vec![SymbolTag::DEPRECATED]),
                    range: lines.to_lsp_range(node.full),
                    selection_range: lines.to_lsp_range(node.selection),
                    children: (!nested.is_empty()).then_some(nested),
                    deprecated: Some(is_deprecated),
                }
            })
            .collect()
    }

    Some(DocumentSymbolResponse::Nested(build(None, &nodes, &children, lines)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn symbols(code: &str) -> Vec<DocumentSymbol> {
        let mut hir = mq_hir::Hir::default();
        let url = Url::parse("file:///test.mq").unwrap();
        hir.add_code(Some(url.clone()), code);
        let lines = LineIndex::new(Arc::new(code.to_string()));
        match response(Arc::new(RwLock::new(hir)), url, &lines).unwrap() {
            DocumentSymbolResponse::Nested(symbols) => symbols,
            _ => panic!("Expected Nested response"),
        }
    }

    /// `name:Kind@range` per symbol, children indented.
    fn outline(symbols: &[DocumentSymbol], depth: usize, out: &mut String) {
        for s in symbols {
            let (r, n) = (s.range, s.selection_range);
            out.push_str(&format!(
                "{}{} {:?} {}:{}-{}:{} sel {}:{}-{}:{}\n",
                "  ".repeat(depth),
                s.name,
                s.kind,
                r.start.line,
                r.start.character,
                r.end.line,
                r.end.character,
                n.start.line,
                n.start.character,
                n.end.line,
                n.end.character,
            ));
            outline(s.children.as_deref().unwrap_or(&[]), depth + 1, out);
        }
    }

    #[test]
    fn test_response_with_empty_source() {
        let hir = Arc::new(RwLock::new(mq_hir::Hir::default()));
        let url = Url::parse("file:///test.mq").unwrap();
        let lines = LineIndex::new(Arc::new(String::new()));
        assert!(response(hir, url, &lines).is_none());
    }

    #[test]
    fn test_outline() {
        let code =
            "module m:\n  def f(x):\n    let y = \"s\" | y\n  end\nend\nlet [a, b] = [1, 2] | let z = 3\ndef g(): 1;";
        let mut out = String::new();
        outline(&symbols(code), 0, &mut out);
        assert_eq!(
            out,
            "m Module 0:0-4:3 sel 0:7-0:8
  f Function 1:6-3:5 sel 1:6-1:7
    y Variable 2:8-2:15 sel 2:8-2:9
a Variable 5:5-5:6 sel 5:5-5:6
b Variable 5:8-5:9 sel 5:8-5:9
z Variable 5:26-5:31 sel 5:26-5:27
g Function 6:4-6:10 sel 6:4-6:5
"
        );
    }

    #[test]
    fn test_literals_are_not_symbols() {
        let symbols = symbols("let x = \"s\" | let y = true | let z = None");
        let names = symbols.iter().map(|s| s.name.as_str()).collect::<Vec<_>>();
        assert_eq!(names, ["x", "y", "z"]);
        assert!(symbols.iter().all(|s| s.kind == SymbolKind::VARIABLE));
    }

    #[test]
    fn test_selection_range_is_the_name() {
        let symbols = symbols("def func1(): 1;");
        assert_eq!(symbols[0].selection_range.start.character, 4);
        assert_eq!(symbols[0].selection_range.end.character, 9);
        assert!(symbols[0].range.end.character > symbols[0].selection_range.end.character);
    }

    #[test]
    fn test_columns_are_utf16() {
        let symbols = symbols("let a = \"😀\" | let b = 1");
        assert_eq!(symbols[1].selection_range.start.character, 19);
    }

    #[test]
    fn test_deprecated_symbol() {
        let symbols =
            symbols("# deprecated: This function is no longer supported\ndef old_func(): 1;\ndef new_func(): 2;");
        assert_eq!(symbols[0].tags.as_deref(), Some(&[SymbolTag::DEPRECATED][..]));
        assert!(symbols[1].tags.is_none());
    }
}
