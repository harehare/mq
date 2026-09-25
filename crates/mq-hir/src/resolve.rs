use smol_str::SmolStr;

use crate::{Hir, ScopeId, SourceId, Symbol, SymbolId, SymbolKind};

impl Hir {
    /// Resolves references that are unresolved or were resolved across sources.
    ///
    /// Scope-chain resolutions depend only on their own source, so they stay valid until
    /// that source is replaced, which drops them from `references`.
    pub fn resolve(&mut self) {
        let symbols_to_resolve: Vec<_> = self
            .symbols
            .iter()
            .filter_map(|(ref_symbol_id, ref_symbol)| match &ref_symbol.kind {
                SymbolKind::Ref
                | SymbolKind::Call
                | SymbolKind::CallDynamic
                | SymbolKind::Argument
                | SymbolKind::QualifiedAccess
                    if !self.references.contains_key(&ref_symbol_id)
                        || self.fallback_references.contains(&ref_symbol_id) =>
                {
                    ref_symbol
                        .value
                        .clone()
                        .map(|name| (ref_symbol_id, ref_symbol.scope, name))
                }
                _ => None,
            })
            .collect();

        let mut include_source_ids = None;

        for (ref_symbol_id, scope, ref_name) in symbols_to_resolve {
            if let Some(symbol_id) = self.resolve_ref_symbol_of_scope(scope, &ref_name, ref_symbol_id) {
                self.references.insert(ref_symbol_id, symbol_id);
                self.fallback_references.remove(&ref_symbol_id);
                continue;
            }

            let source_ids = include_source_ids.get_or_insert_with(|| self.include_source_ids());
            if let Some(symbol_id) = self.resolve_ref_symbol_of_source(source_ids, &ref_name) {
                self.references.insert(ref_symbol_id, symbol_id);
                self.fallback_references.insert(ref_symbol_id);
            } else {
                self.references.remove(&ref_symbol_id);
                self.fallback_references.remove(&ref_symbol_id);
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
            SymbolKind::Variable | SymbolKind::DestructuringBinding => 1,
            SymbolKind::Parameter => 2,
            SymbolKind::PatternVariable { .. } => 2,
            SymbolKind::Ident => 2,
            SymbolKind::Argument => 3,
            _ => 4,
        }
    }

    fn resolve_ref_symbol_of_source(&self, source_ids: &[SourceId], ref_name: &SmolStr) -> Option<SymbolId> {
        self.name_index
            .get(ref_name)
            .into_iter()
            .flatten()
            .filter_map(|&symbol_id| {
                let symbol = self.symbols.get(symbol_id)?;
                let source_id = symbol.source.source_id?;
                (source_ids.contains(&source_id) && Self::is_resolvable_target(symbol))
                    .then(|| (self.get_symbol_priority_for_cross_source(&symbol.kind), symbol_id))
            })
            .min_by_key(|(priority, _)| *priority)
            .map(|(_, symbol_id)| symbol_id)
    }

    #[inline(always)]
    fn is_resolvable_target(symbol: &Symbol) -> bool {
        symbol.is_function()
            || symbol.is_parameter()
            || symbol.is_variable()
            || symbol.is_argument()
            || symbol.is_pattern_variable()
            || symbol.is_ident()
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
            _ => 5,
        }
    }

    fn resolve_ref_symbol_of_scope(
        &self,
        scope_id: ScopeId,
        ref_name: &SmolStr,
        ref_symbol_id: SymbolId,
    ) -> Option<SymbolId> {
        let ref_start_line = self
            .symbols
            .get(ref_symbol_id)
            .and_then(|s| s.source.text_range)
            .map(|r| r.start.line);
        let candidates = self.name_index.get(ref_name).map(Vec::as_slice).unwrap_or_default();
        let mut scope_id = Some(scope_id);

        while let Some(current_scope_id) = scope_id {
            // Lowest priority wins; among equal priorities, prefer the definition closest
            // to (but before) the ref, i.e. the highest line.
            let best = candidates
                .iter()
                .filter_map(|&symbol_id| {
                    if symbol_id == ref_symbol_id {
                        return None;
                    }
                    let symbol = self.symbols.get(symbol_id)?;
                    if symbol.scope != current_scope_id || !Self::is_resolvable_target(symbol) {
                        return None;
                    }
                    // `let` bindings must be declared before the use site; functions allow forward references.
                    let line = symbol.source.text_range.map(|r| r.start.line);
                    if symbol.is_variable()
                        && let (Some(ref_line), Some(def_line)) = (ref_start_line, line)
                        && def_line > ref_line
                    {
                        return None;
                    }
                    Some((
                        self.get_symbol_priority_for_scope(&symbol.kind),
                        std::cmp::Reverse(line.unwrap_or(0)),
                        symbol_id,
                    ))
                })
                .min_by_key(|(priority, line, _)| (*priority, *line));

            if let Some((_, _, symbol_id)) = best {
                return Some(symbol_id);
            }

            scope_id = self.scopes.get(current_scope_id).and_then(|scope| scope.parent_id);
        }

        None
    }
}
