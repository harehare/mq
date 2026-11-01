use std::sync::{Arc, RwLock};

use tower_lsp_server::ls_types::{DocumentLink, Position, Range};
use url::Url;

use crate::{position::LineIndex, virtual_document};

/// Links the path string of each `import` / `include` to the module it loads.
pub(crate) fn response(hir: Arc<RwLock<mq_hir::Hir>>, url: Url, lines: &LineIndex) -> Option<Vec<DocumentLink>> {
    let hir = hir.read().unwrap();
    let source = hir.source_by_url(&url)?;

    let mut links = hir
        .symbols_for_source(source)
        .filter_map(|(_, symbol)| {
            let (mq_hir::SymbolKind::Import(module) | mq_hir::SymbolKind::Include(module)) = symbol.kind else {
                return None;
            };
            let target = virtual_document::uri_for_source(&hir, module).filter(virtual_document::is_openable)?;
            // The symbol ends with the quoted path.
            let end = symbol.source.text_range?.end;
            let width = symbol.value.as_ref()?.encode_utf16().count() + 2;
            let end = lines.to_lsp(end);
            let start = Position::new(end.line, end.character.checked_sub(width as u32)?);
            Some(DocumentLink {
                range: Range::new(start, end),
                target: Some(target),
                tooltip: None,
                data: None,
            })
        })
        .collect::<Vec<_>>();

    links.sort_by_key(|link| (link.range.start.line, link.range.start.character));
    Some(links)
}

#[cfg(test)]
mod tests {
    use mq_hir::Hir;

    use super::*;

    fn links(code: &str) -> Vec<DocumentLink> {
        let mut hir = Hir::default();
        let url = Url::parse("file:///test.mq").unwrap();
        hir.add_code(Some(url.clone()), code);
        let lines = LineIndex::new(Arc::new(code.to_string()));
        response(Arc::new(RwLock::new(hir)), url, &lines).unwrap()
    }

    #[test]
    fn test_links_cover_the_quoted_path() {
        let links = links("import \"section\"\n| include \"csv\"");
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].range, Range::new(Position::new(0, 7), Position::new(0, 16)));
        assert_eq!(links[0].target.as_ref().map(|u| u.as_str()), Some("mq-std:/section.mq"));
        assert_eq!(links[1].range, Range::new(Position::new(1, 10), Position::new(1, 15)));
    }

    #[test]
    fn test_links_use_utf16_columns_after_non_bmp_characters() {
        let links = links("\"😀\" | import \"csv\"");
        assert_eq!(links[0].range, Range::new(Position::new(0, 14), Position::new(0, 19)));
    }

    #[test]
    fn test_links_after_leading_bom() {
        let links = links("\u{FEFF}import \"csv\"");
        assert_eq!(links[0].range, Range::new(Position::new(0, 8), Position::new(0, 13)));
    }

    #[test]
    fn test_missing_module_has_no_link() {
        assert!(links("import \"nope\"").is_empty());
    }
}
