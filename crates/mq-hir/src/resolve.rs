use smol_str::SmolStr;

use crate::{Hir, ScopeId, SourceId, Symbol, SymbolId, SymbolKind};

impl Hir {
    pub fn resolve(&mut self) {
        // Extract only the fields we need instead of cloning the entire Symbol
        let symbols_to_resolve: Vec<_> = self
            .symbols
            .iter()
            .filter_map(|(ref_symbol_id, ref_symbol)| match &ref_symbol.kind {
                SymbolKind::Ref
                | SymbolKind::Call
                | SymbolKind::CallDynamic
                | SymbolKind::Argument
                | SymbolKind::Macro(_)
                | SymbolKind::QualifiedAccess => Some((ref_symbol_id, ref_symbol.scope, ref_symbol.value.clone())),
                _ => None,
            })
            .collect();

        for (ref_symbol_id, scope, ref_name) in symbols_to_resolve {
            if let Some(ref_name) = ref_name {
                let resolved = self
                    .resolve_ref_symbol_of_scope(scope, &ref_name, ref_symbol_id)
                    .or_else(|| self.resolve_ref_symbol_of_source(self.include_source_ids(), &ref_name));

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
            match symbol.kind {
                SymbolKind::Include(source_id) | SymbolKind::Import(source_id) | SymbolKind::Module(source_id) => {
                    source_ids.push(source_id);
                }
                _ => {}
            }
        }

        source_ids.push(self.builtin.source_id);

        source_ids
    }

    #[inline(always)]
    fn get_symbol_priority_for_cross_source(&self, symbol_kind: &SymbolKind) -> u8 {
        match symbol_kind {
            SymbolKind::Function(_) => 0,
            SymbolKind::Macro(_) => 0,
            SymbolKind::Variable | SymbolKind::DestructuringBinding => 1,
            SymbolKind::Parameter => 2,
            SymbolKind::PatternVariable { .. } => 2,
            SymbolKind::Ident => 2,
            SymbolKind::Argument => 3,
            _ => 4,
        }
    }

    fn resolve_ref_symbol_of_source(
        &self,
        source_ids: Vec<SourceId>,
        ref_name: &SmolStr,
    ) -> Option<(SymbolId, Symbol)> {
        let mut candidates = Vec::new();

        // Use the name index to avoid an O(n) full-symbol scan.
        for &symbol_id in self.name_index.get(ref_name).into_iter().flatten() {
            let Some(symbol) = self.symbols.get(symbol_id) else {
                continue;
            };
            if let Some(source_id) = symbol.source.source_id
                && source_ids.contains(&source_id)
                && (symbol.is_function()
                    || symbol.is_parameter()
                    || symbol.is_variable()
                    || symbol.is_argument()
                    || symbol.is_pattern_variable()
                    || symbol.is_macro()
                    || symbol.is_ident())
            {
                let priority = self.get_symbol_priority_for_cross_source(&symbol.kind);
                candidates.push((priority, symbol_id, symbol.clone()));
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
            SymbolKind::PatternVariable { .. } => 1,
            SymbolKind::Ident => 2,
            SymbolKind::Variable | SymbolKind::DestructuringBinding => 3,
            SymbolKind::Function(_) => 4,
            SymbolKind::Macro(_) => 4,
            _ => 5,
        }
    }

    fn resolve_ref_symbol_of_scope(
        &self,
        scope_id: ScopeId,
        ref_name: &SmolStr,
        ref_symbol_id: SymbolId,
    ) -> Option<(SymbolId, Symbol)> {
        // Get the Ref's source position for ordering checks.
        let ref_start_line = self
            .symbols
            .get(ref_symbol_id)
            .and_then(|s| s.source.text_range)
            .map(|r| r.start.line);

        // Find all matching symbols in current scope with priority order.
        // Use the name index to avoid an O(n) full-symbol scan.
        let mut candidates = Vec::new();

        for &symbol_id in self.name_index.get(ref_name).into_iter().flatten() {
            if symbol_id == ref_symbol_id {
                continue;
            }
            let Some(symbol) = self.symbols.get(symbol_id) else {
                continue;
            };
            if symbol.scope != scope_id {
                continue;
            }

            if !(symbol.is_function()
                || symbol.is_parameter()
                || symbol.is_variable()
                || symbol.is_argument()
                || symbol.is_pattern_variable()
                || symbol.is_macro()
                || symbol.is_ident())
            {
                continue;
            }

            // Variable (`let`) bindings must be declared before the use site.
            // Functions and macros allow forward references.
            if symbol.is_variable()
                && let (Some(ref_line), Some(def_range)) = (ref_start_line, symbol.source.text_range)
                && def_range.start.line > ref_line
            {
                continue;
            }

            let priority = self.get_symbol_priority_for_scope(&symbol.kind);
            candidates.push((priority, symbol_id, symbol.clone()));
        }

        // Sort by priority (lower number = higher priority).
        // For same-priority variables, prefer the one closest (highest line) before the ref.
        candidates.sort_by(|(p1, _, s1), (p2, _, s2)| {
            p1.cmp(p2).then_with(|| {
                let line1 = s1.source.text_range.map(|r| r.start.line).unwrap_or(0);
                let line2 = s2.source.text_range.map(|r| r.start.line).unwrap_or(0);
                // Prefer the definition closest to (but before) the Ref — i.e., higher line number first.
                line2.cmp(&line1)
            })
        });

        if let Some((_, symbol_id, symbol)) = candidates.first() {
            Some((*symbol_id, symbol.clone()))
        } else {
            // Search in parent scope
            self.scopes[scope_id]
                .parent_id
                .and_then(|parent_scope_id| self.resolve_ref_symbol_of_scope(parent_scope_id, ref_name, ref_symbol_id))
        }
    }
}
