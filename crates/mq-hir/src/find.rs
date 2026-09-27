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
        self.sources.get(source_id)?;

        // Innermost scope: latest start, then earliest end.
        self.scopes
            .iter()
            .filter_map(|(scope_id, scope)| {
                let range = scope.source.text_range?;
                (scope.source.source_id == Some(source_id) && range.contains(&position)).then_some((scope_id, range))
            })
            .max_by(|(_, a), (_, b)| a.start.cmp(&b.start).then_with(|| b.end.cmp(&a.end)))
            .map(|(scope_id, _)| (scope_id, self.scopes[scope_id].clone()))
    }

    pub fn find_symbols_in_scope(&self, scope_id: ScopeId) -> Vec<Arc<Symbol>> {
        self.symbol_ids_in_scope(scope_id)
            .into_iter()
            .map(|symbol_id| Arc::new(self.symbols[symbol_id].clone()))
            .collect()
    }

    /// Like [`Self::find_symbols_in_scope`], limited to what code at `position` can see.
    /// Inside an inline module, that is only the module's own symbols.
    pub fn find_visible_symbols_in_scope(
        &self,
        scope_id: ScopeId,
        source_id: SourceId,
        position: mq_lang::Position,
    ) -> Vec<Arc<Symbol>> {
        let module = self.find_inline_module_in_position(source_id, position);
        self.symbol_ids_in_scope(scope_id)
            .into_iter()
            .filter(|symbol_id| module.is_none_or(|module| self.is_inside(*symbol_id, module)))
            .map(|symbol_id| Arc::new(self.symbols[symbol_id].clone()))
            .collect()
    }

    /// The innermost inline `module` whose body contains `position`.
    pub fn find_inline_module_in_position(&self, source_id: SourceId, position: mq_lang::Position) -> Option<SymbolId> {
        let in_source = |symbol: &Symbol| symbol.source.source_id == Some(source_id);
        self.symbols
            .iter()
            .filter(|(_, symbol)| matches!(symbol.kind, SymbolKind::Module(_)) && in_source(symbol))
            .filter_map(|(module_id, module)| {
                let start = module.source.text_range?.start;
                // The module symbol spans only its keyword, so its body ends where its last symbol does.
                let end = self
                    .symbols
                    .iter()
                    .filter(|(symbol_id, symbol)| in_source(symbol) && self.is_inside(*symbol_id, module_id))
                    .filter_map(|(_, symbol)| symbol.source.text_range.map(|range| range.end))
                    .max()?;
                (start <= position && position <= end).then_some((module_id, start))
            })
            .max_by_key(|(_, start)| *start)
            .map(|(module_id, _)| module_id)
    }

    fn symbol_ids_in_scope(&self, scope_id: ScopeId) -> Vec<SymbolId> {
        let mut symbol_ids = Vec::new();
        let mut scope_id = Some(scope_id);

        while let Some(id) = scope_id {
            symbol_ids.extend(self.symbols.iter().filter_map(|(symbol_id, symbol)| {
                (symbol.scope == id
                    && (symbol.is_function()
                        || symbol.is_parameter()
                        || symbol.is_variable()
                        || symbol.is_module()
                        || symbol.is_argument()
                        || symbol.is_ident()))
                .then_some(symbol_id)
            }));
            scope_id = self.scopes[id].parent_id;
        }

        symbol_ids
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
    fn test_find_symbol_in_position_resolves_descendant_chain_step() {
        // Regression: a synthetic `..` bridge node used to shadow `.code`'s range.
        let mut hir = Hir::default();
        let (source_id, _) = hir.add_code(None, ".blockquote .code");
        let pos = mq_lang::Position::new(1, 14);

        let (_, symbol) = hir
            .find_symbol_in_position(source_id, pos)
            .expect("symbol at .code position");
        assert_eq!(symbol.value.as_deref(), Some(".code"));
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
