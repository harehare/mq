use std::sync::{Arc, RwLock};

use itertools::Itertools;
use tower_lsp::lsp_types::{SemanticToken, SemanticTokenModifier, SemanticTokenType, Url};

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
        .sorted_by_key(|symbol| symbol.source.text_range.clone())
        .collect::<Vec<_>>()
    {
        for (range, _) in &symbol.doc {
            let line = range.start.line - 1_u32;
            let start = (range.start.column - 2) as u32;
            let length = ((range.end.column - 1) - (range.start.column - 2)) as u32;
            let token_type = token_type(tower_lsp::lsp_types::SemanticTokenType::COMMENT);

            if line.checked_sub(pre_line).is_none() {
                continue;
            }

            let delta_line = line - pre_line;
            let delta_start = if delta_line == 0 {
                start - pre_start
            } else {
                start
            };

            pre_line = line;
            pre_start = start;

            semantic_tokens.push(tower_lsp::lsp_types::SemanticToken {
                delta_line,
                delta_start,
                length,
                token_type,
                token_modifiers_bitset: token_modifier(
                    tower_lsp::lsp_types::SemanticTokenModifier::DOCUMENTATION,
                ),
            });
        }

        if let Some(range) = &symbol.source.text_range {
            let line = range.start.line - 1_u32;
            let start = (range.start.column - 1) as u32;
            let length = ((range.end.column - 1) - (range.start.column - 1)) as u32;
            let token_type = match symbol.kind {
                mq_hir::SymbolKind::Function(_) => {
                    token_type(tower_lsp::lsp_types::SemanticTokenType::FUNCTION)
                }
                mq_hir::SymbolKind::Call => {
                    token_type(tower_lsp::lsp_types::SemanticTokenType::METHOD)
                }
                mq_hir::SymbolKind::Variable => {
                    token_type(tower_lsp::lsp_types::SemanticTokenType::VARIABLE)
                }
                mq_hir::SymbolKind::Boolean => {
                    token_type(tower_lsp::lsp_types::SemanticTokenType::TYPE)
                }
                mq_hir::SymbolKind::Parameter => {
                    token_type(tower_lsp::lsp_types::SemanticTokenType::PARAMETER)
                }
                mq_hir::SymbolKind::Argument => {
                    token_type(tower_lsp::lsp_types::SemanticTokenType::PARAMETER)
                }
                mq_hir::SymbolKind::String => {
                    token_type(tower_lsp::lsp_types::SemanticTokenType::STRING)
                }
                mq_hir::SymbolKind::Number => {
                    token_type(tower_lsp::lsp_types::SemanticTokenType::NUMBER)
                }
                mq_hir::SymbolKind::Ref => {
                    token_type(tower_lsp::lsp_types::SemanticTokenType::VARIABLE)
                }
                mq_hir::SymbolKind::Selector => {
                    token_type(tower_lsp::lsp_types::SemanticTokenType::METHOD)
                }
                mq_hir::SymbolKind::If
                | mq_hir::SymbolKind::Else
                | mq_hir::SymbolKind::Elif
                | mq_hir::SymbolKind::Foreach
                | mq_hir::SymbolKind::Include(_)
                | mq_hir::SymbolKind::Keyword
                | mq_hir::SymbolKind::None
                | mq_hir::SymbolKind::Until
                | mq_hir::SymbolKind::While => {
                    token_type(tower_lsp::lsp_types::SemanticTokenType::KEYWORD)
                }
            };

            let delta_line = line - pre_line;
            let delta_start = if delta_line == 0 {
                start - pre_start
            } else {
                start
            };

            pre_line = line;
            pre_start = start;

            semantic_tokens.push(tower_lsp::lsp_types::SemanticToken {
                delta_line,
                delta_start,
                length,
                token_type,
                token_modifiers_bitset: if hir.read().unwrap().is_builtin_symbol(symbol) {
                    token_modifier(tower_lsp::lsp_types::SemanticTokenModifier::DEFAULT_LIBRARY)
                } else {
                    0
                },
            });
        }
    }

    semantic_tokens
}

#[inline(always)]
fn token_type(token_type: tower_lsp::lsp_types::SemanticTokenType) -> u32 {
    TOKEN_TYPE.iter().position(|t| t == &token_type).unwrap() as u32
}

#[inline(always)]
fn token_modifier(token_modifier: tower_lsp::lsp_types::SemanticTokenModifier) -> u32 {
    TOKEN_MODIFIER
        .iter()
        .position(|t| t == &token_modifier)
        .unwrap() as u32
}

pub const TOKEN_TYPE: &[tower_lsp::lsp_types::SemanticTokenType] = &[
    SemanticTokenType::KEYWORD,
    SemanticTokenType::STRING,
    SemanticTokenType::NUMBER,
    SemanticTokenType::COMMENT,
    SemanticTokenType::FUNCTION,
    SemanticTokenType::PARAMETER,
    SemanticTokenType::VARIABLE,
    SemanticTokenType::OPERATOR,
    SemanticTokenType::METHOD,
    SemanticTokenType::MODIFIER,
    SemanticTokenType::PROPERTY,
    SemanticTokenType::MODIFIER,
    SemanticTokenType::TYPE,
];

pub const TOKEN_MODIFIER: &[tower_lsp::lsp_types::SemanticTokenModifier] = &[
    SemanticTokenModifier::DEFINITION,
    SemanticTokenModifier::DEFAULT_LIBRARY,
    SemanticTokenModifier::DOCUMENTATION,
];
