//! Tree-sitter grammar for mq language

use tree_sitter_language::LanguageFn;

extern "C" {
    fn tree_sitter_mq() -> *const ();
}

/// The tree-sitter language function for mq
pub const LANGUAGE: LanguageFn = unsafe { LanguageFn::from_raw(tree_sitter_mq) };

/// The content of the node-types.json file for this grammar
pub const NODE_TYPES: &str = include_str!("../../src/node-types.json");

/// The syntax highlighting queries for this grammar
pub const HIGHLIGHTS_QUERY: &str = include_str!("../../queries/highlights.scm");

#[cfg(test)]
mod tests {
    #[test]
    fn test_can_load_grammar() {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&super::LANGUAGE.into())
            .expect("Error loading mq parser");
    }
}