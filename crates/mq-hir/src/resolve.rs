use compact_str::CompactString;

use crate::{Hir, ScopeId, SourceId, Symbol, SymbolId, SymbolKind};

impl Hir {
    pub fn resolve(&mut self) {
        let symbols_to_resolve: Vec<_> = self
            .symbols
            .iter()
            .filter_map(|(ref_symbol_id, ref_symbol)| match &ref_symbol.kind {
                SymbolKind::Ref | SymbolKind::Call => Some((ref_symbol_id, ref_symbol.clone())),
                _ => None,
            })
            .collect();

        for (ref_symbol_id, ref_symbol) in symbols_to_resolve {
            if let Some(ref_name) = &ref_symbol.value {
                let resolved = self
                    .resolve_ref_symbol_of_scope(ref_symbol.scope, ref_name, ref_symbol_id)
                    .or_else(|| {
                        self.resolve_ref_symbol_of_source(self.include_source_ids(), ref_name)
                    });

                if let Some((symbol_id, _)) = resolved {
                    self.references.insert(ref_symbol_id, symbol_id);
                }
            }
        }
    }

    #[inline(always)]
    pub fn resolve_reference_symbol(&self, ref_symbol_id: SymbolId) -> Option<SymbolId> {
        self.references.get(&ref_symbol_id).copied()
    }

    #[inline(always)]
    fn include_source_ids(&self) -> Vec<SourceId> {
        let mut source_ids = Vec::new();

        for (_, symbol) in &self.symbols {
            if let SymbolKind::Include(source_id) = symbol.kind {
                source_ids.push(source_id);
            }
        }

        source_ids.push(self.builtin.source_id);

        source_ids
    }

    #[inline(always)]
    fn get_symbol_priority_for_cross_source(&self, symbol_kind: &SymbolKind) -> u8 {
        match symbol_kind {
            SymbolKind::Function(_) => 0,
            SymbolKind::Variable => 1,
            SymbolKind::Parameter => 2,
            SymbolKind::Argument => 3,
            _ => 4,
        }
    }

    fn resolve_ref_symbol_of_source(
        &self,
        source_ids: Vec<SourceId>,
        ref_name: &CompactString,
    ) -> Option<(SymbolId, Symbol)> {
        let mut candidates = Vec::new();

        for (symbol_id, symbol) in &self.symbols {
            if let Some(source_id) = symbol.source.source_id {
                if source_ids.contains(&source_id)
                    && symbol.value.as_ref() == Some(ref_name)
                    && (symbol.is_function()
                        || symbol.is_parameter()
                        || symbol.is_variable()
                        || symbol.is_argument())
                {
                    let priority = self.get_symbol_priority_for_cross_source(&symbol.kind);
                    candidates.push((priority, symbol_id, symbol.clone()));
                }
            }
        }

        // Sort by priority and return the best match
        candidates.sort_by_key(|(priority, _, _)| *priority);
        candidates
            .first()
            .map(|(_, symbol_id, symbol)| (*symbol_id, symbol.clone()))
    }

    #[inline(always)]
    fn get_symbol_priority_for_scope(&self, symbol_kind: &SymbolKind) -> u8 {
        match symbol_kind {
            SymbolKind::Argument => 0,
            SymbolKind::Parameter => 1,
            SymbolKind::Variable => 2,
            SymbolKind::Function(_) => 3,
            _ => 4,
        }
    }

    fn resolve_ref_symbol_of_scope(
        &self,
        scope_id: ScopeId,
        ref_name: &CompactString,
        ref_symbol_id: SymbolId,
    ) -> Option<(SymbolId, Symbol)> {
        // Find all matching symbols in current scope with priority order
        let mut candidates = Vec::new();

        for (symbol_id, symbol) in &self.symbols {
            if symbol_id != ref_symbol_id
                && symbol.scope == scope_id
                && symbol.value.as_ref() == Some(ref_name)
                && (symbol.is_function()
                    || symbol.is_parameter()
                    || symbol.is_variable()
                    || symbol.is_argument())
            {
                let priority = self.get_symbol_priority_for_scope(&symbol.kind);
                candidates.push((priority, symbol_id, symbol.clone()));
            }
        }

        // Sort by priority (lower number = higher priority)
        candidates.sort_by_key(|(priority, _, _)| *priority);

        if let Some((_, symbol_id, symbol)) = candidates.first() {
            Some((*symbol_id, symbol.clone()))
        } else {
            // Search in parent scope
            self.scopes[scope_id].parent_id.and_then(|parent_scope_id| {
                self.resolve_ref_symbol_of_scope(parent_scope_id, ref_name, ref_symbol_id)
            })
        }
    }
}
