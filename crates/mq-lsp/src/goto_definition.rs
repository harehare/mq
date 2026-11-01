use std::sync::{Arc, RwLock};

use tower_lsp_server::ls_types::{GotoDefinitionResponse, Location, Position, Range};
use url::Url;

use crate::virtual_document;

fn to_range(range: mq_lang::Range) -> Range {
    Range {
        start: Position {
            line: range.start.line - 1,
            character: (range.start.column - 1) as u32,
        },
        end: Position {
            line: range.end.line - 1,
            character: (range.end.column - 1) as u32,
        },
    }
}

pub(crate) fn response(hir: Arc<RwLock<mq_hir::Hir>>, url: Url, position: Position) -> Option<GotoDefinitionResponse> {
    let hir_guard = hir.read().unwrap();
    let source = hir_guard.source_by_url(&url)?;
    let (_, symbol) = hir_guard.find_symbol_in_position(
        source,
        mq_lang::Position::new(position.line + 1, (position.character + 1) as usize),
    )?;

    // `import "x"` jumps to the top of the module.
    if let mq_hir::SymbolKind::Import(module) | mq_hir::SymbolKind::Include(module) = symbol.kind {
        let uri = virtual_document::uri_for_source(&hir_guard, module).filter(virtual_document::is_openable)?;
        return Some(GotoDefinitionResponse::Scalar(Location {
            uri,
            range: Range::default(),
        }));
    }

    // Native builtins have no source text to show.
    let text_range = symbol.source.text_range?;
    let uri = virtual_document::uri_for_source(&hir_guard, symbol.source.source_id?)?;
    Some(GotoDefinitionResponse::Scalar(Location {
        uri,
        range: to_range(text_range),
    }))
}
#[cfg(test)]
mod tests {
    use super::*;
    use mq_hir::Hir;

    #[test]
    fn test_goto_definition_found() {
        let mut hir = Hir::default();
        let url = Url::parse("file:///test.mq").unwrap();
        hir.add_code(Some(url.clone()), "def func1(): 1; func1()");

        let result = response(Arc::new(RwLock::new(hir)), url.clone(), Position::new(0, 4));

        assert!(result.is_some());
        if let Some(GotoDefinitionResponse::Scalar(location)) = result {
            assert_eq!(location.uri.to_string(), url.to_string());
            assert_eq!(location.range.start, Position::new(0, 4));
            assert_eq!(location.range.end, Position::new(0, 9));
        } else {
            panic!("Expected Scalar response");
        }
    }

    #[test]
    fn test_goto_definition_not_found() {
        let hir = Arc::new(RwLock::new(mq_hir::Hir::default()));
        let url = Url::parse("file:///test.mq").unwrap();

        let result = response(hir, url, Position::new(0, 0));
        assert!(result.is_none());
    }

    #[test]
    fn test_goto_definition_no_text_range() {
        let mut hir = mq_hir::Hir::default();
        let url = Url::parse("file:///test.mq").unwrap();
        hir.add_code(Some(url.clone()), "let x = 42;");

        let result = response(Arc::new(RwLock::new(hir)), url, Position::new(0, 11));
        assert!(result.is_none());
    }

    #[test]
    fn test_goto_definition_call_to_definition() {
        let mut hir = Hir::default();
        let url = Url::parse("file:///test.mq").unwrap();
        // Define function then call it
        hir.add_code(Some(url.clone()), "def helper(): 42 end | helper()");

        // Position on helper() call (at character 24, after "| ")
        let result = response(Arc::new(RwLock::new(hir)), url.clone(), Position::new(0, 24));

        // Should navigate to the function definition in the same file
        assert!(result.is_some());
        if let Some(GotoDefinitionResponse::Scalar(location)) = result {
            assert_eq!(location.uri.to_string(), url.to_string());
            // Definition starts at "def helper"
            assert_eq!(location.range.start, Position::new(0, 4));
        } else {
            panic!("Expected Scalar response");
        }
    }

    fn goto(code: &str, position: Position) -> Option<Location> {
        let mut hir = Hir::default();
        let url = Url::parse("file:///test.mq").unwrap();
        hir.add_code(Some(url.clone()), code);
        match response(Arc::new(RwLock::new(hir)), url, position)? {
            GotoDefinitionResponse::Scalar(location) => Some(location),
            _ => None,
        }
    }

    #[test]
    fn test_goto_definition_builtin_function_opens_virtual_builtin() {
        // `is_dict` is defined in builtin.mq.
        let code = "{} | is_dict()";
        let location = goto(code, Position::new(0, 8)).expect("builtin definition");
        assert_eq!(location.uri.as_str(), "mq-std:/builtin.mq");
        let text = virtual_document::content(location.uri.as_str()).unwrap();
        let line = text.lines().nth(location.range.start.line as usize).unwrap();
        assert!(line.contains("def is_dict"), "{line}");
    }

    #[test]
    fn test_goto_definition_native_builtin_is_none() {
        assert!(goto("\"a\" | len()", Position::new(0, 7)).is_none());
    }

    #[test]
    fn test_goto_definition_import_opens_standard_module() {
        let location = goto("import \"section\"", Position::new(0, 9));
        assert_eq!(
            location.map(|l| l.uri.to_string()).as_deref(),
            Some("mq-std:/section.mq")
        );
    }

    #[test]
    fn test_goto_definition_import_of_missing_module_is_none() {
        assert!(goto("import \"nope\"", Position::new(0, 9)).is_none());
    }

    #[test]
    fn test_goto_definition_qualified_access_opens_standard_module() {
        let code = "import \"section\" | section::title(\"a\", 1)";
        let location = goto(code, Position::new(0, 29)).expect("module member definition");
        assert_eq!(location.uri.as_str(), "mq-std:/section.mq");
    }
}
