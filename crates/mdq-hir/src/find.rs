use std::sync::Arc;

use itertools::Itertools;

use crate::{Hir, Scope, Symbol, SymbolKind, scope::ScopeId, source::SourceId, symbol::SymbolId};

impl Hir {
    pub fn find_symbol_in_position(
        &self,
        source_id: SourceId,
        position: mdq_lang::Position,
    ) -> Option<(SymbolId, Symbol)> {
        let source = self.sources.get(source_id);

        source.and_then(|_| {
            self.symbols
                .iter()
                .find(|(_, symbol)| {
                    symbol.source.source_id.is_some()
                        && symbol.source.text_range.is_some()
                        && symbol.source.source_id.unwrap() == source_id
                        && symbol
                            .source
                            .text_range
                            .clone()
                            .unwrap()
                            .contains(&position)
                })
                .and_then(|(symbol_id, symbol)| match symbol.kind {
                    SymbolKind::Ref | SymbolKind::Call => {
                        let target_symbol_id = self.references.get(&symbol_id);
                        target_symbol_id.and_then(|target_symbol_id| {
                            self.symbols
                                .get(*target_symbol_id)
                                .map(|symbol| (*target_symbol_id, symbol.clone()))
                        })
                    }
                    _ => Some((symbol_id, symbol.clone())),
                })
        })
    }

    pub fn find_scope_in_position(
        &self,
        source_id: SourceId,
        position: mdq_lang::Position,
    ) -> Option<(ScopeId, Scope)> {
        let source = self.sources.get(source_id);

        source.and_then(|_| {
            self.scopes
                .iter()
                .collect_vec()
                .into_iter()
                .rev()
                .find(|(_, scope)| {
                    scope.source.source_id.is_some()
                        && scope.source.text_range.is_some()
                        && scope.source.source_id.unwrap() == source_id
                        && scope.source.text_range.clone().unwrap().contains(&position)
                })
                .map(|(scope_id, scope)| (scope_id, scope.clone()))
        })
    }

    pub fn find_symbols_in_scope(&self, scope_id: ScopeId) -> Vec<Arc<Symbol>> {
        let mut symbols = Vec::with_capacity(self.symbols.len());

        self.symbols.iter().for_each(|(_, symbol)| {
            if symbol.scope == scope_id
                && (symbol.is_function() || symbol.is_parameter() || symbol.is_variable())
            {
                symbols.push(Arc::new(symbol.clone()));
            }
        });

        let scope_id = self.scopes[scope_id].parent_id;

        symbols.extend(
            scope_id
                .map(|scope_id| self.find_symbols_in_scope(scope_id))
                .unwrap_or_default(),
        );

        symbols
    }

    pub fn find_symbols_in_source(&self, source_id: SourceId) -> Vec<Arc<Symbol>> {
        self.symbols
            .iter()
            .filter_map(|(_, symbol)| {
                symbol.source.source_id.and_then(|symbol_source_id| {
                    if symbol_source_id == source_id {
                        Some(Arc::new(symbol.clone()))
                    } else {
                        None
                    }
                })
            })
            .collect_vec()
    }

    pub fn find_scope_by_source(&self, source_id: &SourceId) -> ScopeId {
        self.source_scopes[source_id]
    }
}
