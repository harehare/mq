use std::sync::Arc;

use crate::{
    Hir, Scope, Symbol, SymbolKind,
    scope::{ScopeId, ScopeKind},
    source::SourceId,
    symbol::SymbolId,
};

impl Hir {
    pub fn find_symbol_in_position(
        &self,
        source_id: SourceId,
        position: mq_lang::Position,
    ) -> Option<(SymbolId, Symbol)> {
        let source = self.sources.get(source_id);

        source.and_then(|_| {
            self.symbols
                .iter()
                .find(|(_, symbol)| {
                    symbol.source.source_id.is_some()
                        && symbol.source.text_range.is_some()
                        && symbol.source.source_id.unwrap() == source_id
                        && symbol.source.text_range.as_ref().unwrap().contains(&position)
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

    pub fn find_scope_in_position(&self, source_id: SourceId, position: mq_lang::Position) -> Option<(ScopeId, Scope)> {
        let source = self.sources.get(source_id);

        source.and_then(|_| {
            self.scopes
                .iter()
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .find(|(_, scope)| {
                    scope.source.source_id.is_some()
                        && scope.source.text_range.is_some()
                        && scope.source.source_id.unwrap() == source_id
                        && scope.source.text_range.as_ref().unwrap().contains(&position)
                })
                .map(|(scope_id, scope)| (scope_id, scope.clone()))
        })
    }

    pub fn find_symbols_in_scope(&self, scope_id: ScopeId) -> Vec<Arc<Symbol>> {
        let mut symbols = Vec::with_capacity(self.symbols.len());

        self.symbols.iter().for_each(|(_, symbol)| {
            if symbol.scope == scope_id
                && (symbol.is_function()
                    || symbol.is_parameter()
                    || symbol.is_variable()
                    || symbol.is_module()
                    || symbol.is_argument()
                    || symbol.is_ident()
                    || symbol.is_macro())
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
            .collect::<Vec<_>>()
    }

    pub fn find_scope_by_source(&self, source_id: &SourceId) -> ScopeId {
        self.source_scopes[source_id]
    }

    /// Finds a module symbol by name in the given scope and its parent scopes
    pub fn find_module_by_name(&self, scope_id: ScopeId, module_name: &str) -> Option<(SymbolId, Symbol)> {
        let symbols = self.find_symbols_in_scope(scope_id);

        symbols
            .iter()
            .find(|symbol| symbol.is_module() && symbol.value.as_ref().map(|v| v.as_str()) == Some(module_name))
            .and_then(|_symbol| {
                self.symbols
                    .iter()
                    .find(|(_, s)| s.is_module() && s.value.as_ref().map(|v| v.as_str()) == Some(module_name))
                    .map(|(id, s)| (id, s.clone()))
            })
    }

    /// Finds symbols in a module by its source_id (only symbols directly in the module, not parent scopes)
    pub fn find_symbols_in_module(&self, module_source_id: SourceId) -> Vec<Arc<Symbol>> {
        // Find the scope for this module source
        if let Some(scope_id) = self.scopes.iter().find_map(|(scope_id, scope)| {
            if let ScopeKind::Module(source_id) = scope.kind
                && source_id == module_source_id
            {
                return Some(scope_id);
            }
            None
        }) {
            // Only return symbols directly in this scope, not parent scopes
            let mut symbols = Vec::new();
            self.symbols.iter().for_each(|(_, symbol)| {
                if symbol.scope == scope_id
                    && (symbol.is_function() || symbol.is_parameter() || symbol.is_variable() || symbol.is_argument())
                {
                    symbols.push(Arc::new(symbol.clone()));
                }
            });
            symbols
        } else {
            Vec::new()
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_symbol_in_position() {
        let mut hir = Hir::default();
        let (source_id, _) = hir.add_code(None, "let x = 5");
        let pos = mq_lang::Position::new(1, 4);

        assert!(hir.find_symbol_in_position(source_id, pos).is_some());
    }

    #[test]
    fn test_find_scope_in_position() {
        let mut hir = Hir::default();
        let (source_id, _) = hir.add_code(None, "def example(): 5;");
        let pos = mq_lang::Position::new(1, 18);

        assert!(hir.find_scope_in_position(source_id, pos).map(|(id, _)| id).is_some());
    }

    #[test]
    fn test_find_symbols_in_scope() {
        let mut hir = Hir::default();
        let (_, scope_id) = hir.add_code(None, "let x = 5");
        let symbols = hir.find_symbols_in_scope(scope_id);

        assert_eq!(symbols.len(), 1);
    }

    #[test]
    fn test_find_symbols_in_module_scope() {
        let mut hir = Hir::default();
        let (_, scope_id) = hir.add_code(None, "module mod1: def func1(): 1; end");
        let symbols = hir.find_symbols_in_scope(scope_id);

        // Symbols: mod1 (Ident), Module, func1 (Function)
        assert_eq!(symbols.len(), 3);
    }

    #[test]
    fn test_find_symbols_in_source() {
        let mut hir = Hir::default();
        let (source_id, _) = hir.add_code(None, "let x = 5");
        let symbols = hir.find_symbols_in_source(source_id);

        assert_eq!(symbols.len(), 3);
    }

    #[test]
    fn test_find_scope_by_source() {
        let mut hir = Hir::default();
        let (source_id, scope_id) = hir.add_code(None, "let x = 5");

        hir.source_scopes.insert(source_id, scope_id);
        assert_eq!(hir.find_scope_by_source(&source_id), scope_id);
    }
}
