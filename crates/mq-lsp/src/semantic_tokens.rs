use std::sync::{Arc, RwLock};

use itertools::Itertools;
use tower_lsp_server::ls_types::{self, SemanticToken, SemanticTokenModifier, SemanticTokenType};
use url::Url;

pub fn response(hir: Arc<RwLock<mq_hir::Hir>>, url: Url) -> Vec<SemanticToken> {
    let mut pre_line = 0;
    let mut pre_start = 0;

    let source_id = hir.read().unwrap().source_by_url(&url);
    let symbols = source_id
        .map(|source_id| hir.read().unwrap().find_symbols_in_source(source_id))
        .unwrap_or_default();

    let mut semantic_tokens = Vec::with_capacity(symbols.len());

    for symbol in symbols
        .into_iter()
        .sorted_by_key(|symbol| symbol.source.text_range)
        .collect::<Vec<_>>()
    {
        for (range, _) in &symbol.doc {
            let line = range.start.line - 1_u32;
            let start = (range.start.column - 2) as u32;
            let length = ((range.end.column - 1) - (range.start.column - 2)) as u32;
            let token_type = token_type(ls_types::SemanticTokenType::COMMENT);

            if line.checked_sub(pre_line).is_none() {
                continue;
            }

            let delta_line = line - pre_line;
            let delta_start = if delta_line == 0 { start - pre_start } else { start };

            pre_line = line;
            pre_start = start;

            semantic_tokens.push(ls_types::SemanticToken {
                delta_line,
                delta_start,
                length,
                token_type,
                token_modifiers_bitset: token_modifier(ls_types::SemanticTokenModifier::DOCUMENTATION),
            });
        }

        if let Some(range) = &symbol.source.text_range {
            let line = range.start.line - 1_u32;
            let start = (range.start.column - 1) as u32;
            let length = if range.start.line == range.end.line {
                ((range.end.column - 1) - (range.start.column - 1)) as u32
            } else {
                symbol.value.as_ref().map(|name| name.len()).unwrap_or_default() as u32
            };
            let token_type = match symbol.kind {
                mq_hir::SymbolKind::Argument => token_type(ls_types::SemanticTokenType::PARAMETER),
                mq_hir::SymbolKind::BinaryOp | mq_hir::SymbolKind::UnaryOp => {
                    token_type(ls_types::SemanticTokenType::OPERATOR)
                }
                mq_hir::SymbolKind::Dict | mq_hir::SymbolKind::Boolean | mq_hir::SymbolKind::Array => {
                    token_type(ls_types::SemanticTokenType::TYPE)
                }
                mq_hir::SymbolKind::Call | mq_hir::SymbolKind::CallDynamic | mq_hir::SymbolKind::QualifiedAccess => {
                    token_type(ls_types::SemanticTokenType::METHOD)
                }

                mq_hir::SymbolKind::Else
                | mq_hir::SymbolKind::Elif
                | mq_hir::SymbolKind::Foreach
                | mq_hir::SymbolKind::If
                | mq_hir::SymbolKind::Include(_)
                | mq_hir::SymbolKind::Keyword
                | mq_hir::SymbolKind::Loop
                | mq_hir::SymbolKind::None
                | mq_hir::SymbolKind::Block
                | mq_hir::SymbolKind::Try
                | mq_hir::SymbolKind::Catch
                | mq_hir::SymbolKind::Match
                | mq_hir::SymbolKind::Import(_)
                | mq_hir::SymbolKind::Module(_)
                | mq_hir::SymbolKind::While => token_type(ls_types::SemanticTokenType::KEYWORD),
                mq_hir::SymbolKind::Function(_) | mq_hir::SymbolKind::Macro(_) => {
                    token_type(ls_types::SemanticTokenType::FUNCTION)
                }
                mq_hir::SymbolKind::Number => token_type(ls_types::SemanticTokenType::NUMBER),
                mq_hir::SymbolKind::Parameter => token_type(ls_types::SemanticTokenType::PARAMETER),
                mq_hir::SymbolKind::Ref => token_type(ls_types::SemanticTokenType::VARIABLE),
                mq_hir::SymbolKind::Selector => token_type(ls_types::SemanticTokenType::METHOD),
                mq_hir::SymbolKind::String => token_type(ls_types::SemanticTokenType::STRING),
                mq_hir::SymbolKind::Variable
                | mq_hir::SymbolKind::Symbol
                | mq_hir::SymbolKind::MatchArm
                | mq_hir::SymbolKind::Pattern
                | mq_hir::SymbolKind::PatternVariable
                | mq_hir::SymbolKind::Ident => token_type(ls_types::SemanticTokenType::VARIABLE),
            };

            let delta_line = line - pre_line;
            let delta_start = if delta_line == 0 { start - pre_start } else { start };

            pre_line = line;
            pre_start = start;

            let hir_guard = hir.read().unwrap();
            let mut modifiers_bitset = 0;
            if hir_guard.is_builtin_symbol(&symbol) {
                modifiers_bitset |= 1 << token_modifier(ls_types::SemanticTokenModifier::DEFAULT_LIBRARY);
            }
            if symbol.is_deprecated() {
                modifiers_bitset |= 1 << token_modifier(ls_types::SemanticTokenModifier::DEPRECATED);
            }
            drop(hir_guard);

            semantic_tokens.push(ls_types::SemanticToken {
                delta_line,
                delta_start,
                length,
                token_type,
                token_modifiers_bitset: modifiers_bitset,
            });
        }
    }

    semantic_tokens
}

#[inline(always)]
fn token_type(token_type: ls_types::SemanticTokenType) -> u32 {
    TOKEN_TYPE.iter().position(|t| t == &token_type).unwrap() as u32
}

#[inline(always)]
fn token_modifier(token_modifier: ls_types::SemanticTokenModifier) -> u32 {
    TOKEN_MODIFIER.iter().position(|t| t == &token_modifier).unwrap() as u32
}

pub const TOKEN_TYPE: &[ls_types::SemanticTokenType] = &[
    SemanticTokenType::COMMENT,
    SemanticTokenType::FUNCTION,
    SemanticTokenType::KEYWORD,
    SemanticTokenType::METHOD,
    SemanticTokenType::MODIFIER,
    SemanticTokenType::NUMBER,
    SemanticTokenType::OPERATOR,
    SemanticTokenType::PARAMETER,
    SemanticTokenType::PROPERTY,
    SemanticTokenType::STRING,
    SemanticTokenType::TYPE,
    SemanticTokenType::VARIABLE,
];

pub const TOKEN_MODIFIER: &[ls_types::SemanticTokenModifier] = &[
    SemanticTokenModifier::DEFINITION,
    SemanticTokenModifier::DEFAULT_LIBRARY,
    SemanticTokenModifier::DOCUMENTATION,
    SemanticTokenModifier::DEPRECATED,
];
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_type() {
        assert_eq!(token_type(SemanticTokenType::KEYWORD), 2);
        assert_eq!(token_type(SemanticTokenType::STRING), 9);
        assert_eq!(token_type(SemanticTokenType::FUNCTION), 1);
    }

    #[test]
    fn test_token_modifier() {
        assert_eq!(token_modifier(SemanticTokenModifier::DEFINITION), 0);
        assert_eq!(token_modifier(SemanticTokenModifier::DEFAULT_LIBRARY), 1);
        assert_eq!(token_modifier(SemanticTokenModifier::DOCUMENTATION), 2);
        assert_eq!(token_modifier(SemanticTokenModifier::DEPRECATED), 3);
    }

    #[test]
    fn test_response_empty() {
        let hir = Arc::new(RwLock::new(mq_hir::Hir::default()));
        let url = Url::parse("file:///test.mq").unwrap();

        let tokens = response(hir, url);
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_response_with_symbols() {
        let mut hir = mq_hir::Hir::default();
        let url = Url::parse("file:///test.mq").unwrap();

        hir.add_code(
            Some(url.clone()),
            "let val1 = 1 | def func1(): 1; def func2(): \"2\"; def func3(x): x; def func4(): false; | .h | func1() | func2() | func3(1) | func4()",
        );

        let hir = Arc::new(RwLock::new(hir));
        let tokens = response(hir, url);

        assert_eq!(tokens.len(), 22);
    }

    #[test]
    fn test_response_with_comment() {
        let mut hir = mq_hir::Hir::default();
        let url = Url::parse("file:///test.mq").unwrap();

        hir.add_code(Some(url.clone()), "# This is a comment\ndef func1(): 1;");

        let hir = Arc::new(RwLock::new(hir));
        let tokens = response(hir, url);

        assert_eq!(tokens.len(), 4);

        assert_eq!(tokens[0].token_type, token_type(SemanticTokenType::COMMENT));
        assert_eq!(
            tokens[0].token_modifiers_bitset,
            token_modifier(SemanticTokenModifier::DOCUMENTATION)
        );
    }

    #[test]
    fn test_response_with_deprecated_function() {
        let mut hir = mq_hir::Hir::default();
        let url = Url::parse("file:///test.mq").unwrap();

        // Create a deprecated function
        hir.add_code(
            Some(url.clone()),
            "# deprecated: This function is no longer supported\ndef old_func(): 1;",
        );

        let hir = Arc::new(RwLock::new(hir));
        let tokens = response(hir, url);

        // Should have tokens for comment and function definition
        assert!(tokens.len() >= 2, "Should have at least comment and function tokens");

        // Find the function token (should be the one with FUNCTION type)
        let func_token = tokens
            .iter()
            .find(|t| t.token_type == token_type(SemanticTokenType::FUNCTION));
        assert!(func_token.is_some(), "Should have a function token");

        // Check that the function token has the DEPRECATED modifier
        let func_token = func_token.unwrap();
        let deprecated_bit = 1 << token_modifier(SemanticTokenModifier::DEPRECATED);
        assert_ne!(
            func_token.token_modifiers_bitset & deprecated_bit,
            0,
            "Function token should have DEPRECATED modifier"
        );
    }
}
