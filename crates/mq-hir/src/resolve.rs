use compact_str::CompactString;

use crate::{Hir, ScopeId, SourceId, Symbol, SymbolId, SymbolKind};

impl Hir {
    pub fn resolve(&mut self) {
        self.symbols
            .clone()
            .iter()
            .for_each(|(ref_symbol_id, ref_symbol)| match &ref_symbol.kind {
                SymbolKind::Ref => {
                    self.resolve_ref_symbol_of_scope(
                        ref_symbol.scope,
                        ref_symbol.value.as_ref().unwrap(),
                        ref_symbol_id,
                    )
                    .map(|(symbol_id, _)| {
                        self.references.insert(ref_symbol_id, symbol_id);
                    })
                    .or(self
                        .resolve_ref_symbol_of_source(
                            self.include_source_ids(ref_symbol.scope),
                            ref_symbol.value.as_ref().unwrap(),
                        )
                        .map(|(symbol_id, _)| {
                            self.references.insert(ref_symbol_id, symbol_id);
                        }));
                }
                SymbolKind::Call => {
                    let _ = self
                        .resolve_ref_symbol_of_scope(
                            ref_symbol.scope,
                            ref_symbol.value.as_ref().unwrap(),
                            ref_symbol_id,
                        )
                        .map(|(symbol_id, _)| {
                            self.references.insert(ref_symbol_id, symbol_id);
                        })
                        .or(self
                            .resolve_ref_symbol_of_source(
                                self.include_source_ids(ref_symbol.scope),
                                ref_symbol.value.as_ref().unwrap(),
                            )
                            .map(|(symbol_id, _)| {
                                self.references.insert(ref_symbol_id, symbol_id);
                            }));
                }
                _ => {}
            });
    }

    pub fn resolve_reference_symbol(&self, ref_symbol_id: SymbolId) -> Option<SymbolId> {
        self.references.get(&ref_symbol_id).copied()
    }

    fn include_source_ids(&self, scope_id: ScopeId) -> Vec<SourceId> {
        let mut source_ids = Vec::new();

        for (_, symbol) in &self.symbols {
            if symbol.scope == scope_id {
                if let SymbolKind::Include(source_id) = symbol.kind {
                    source_ids.push(source_id);
                }
            }
        }

        source_ids.push(self.builtin.source_id);

        source_ids
    }

    fn resolve_ref_symbol_of_source(
        &self,
        source_ids: Vec<SourceId>,
        ref_name: &CompactString,
    ) -> Option<(SymbolId, Symbol)> {
        let mut symbols = Vec::new();

        for (symbol_id, symbol) in &self.symbols {
            if let Some(source_id) = symbol.source.source_id {
                if source_ids.contains(&source_id)
                    && symbol.value.as_ref() == Some(ref_name)
                    && (symbol.is_function() || symbol.is_parameter() || symbol.is_variable())
                {
                    symbols.push((symbol_id, symbol.clone()));
                    break;
                }
            }
        }

        symbols.first().cloned()
    }

    fn resolve_ref_symbol_of_scope(
        &self,
        scope_id: ScopeId,
        ref_name: &CompactString,
        ref_symbol_id: SymbolId,
    ) -> Option<(SymbolId, Symbol)> {
        let mut symbols = Vec::new();

        for (symbol_id, symbol) in &self.symbols {
            if symbol_id != ref_symbol_id
                && symbol.scope == scope_id
                && symbol.value.as_ref() == Some(ref_name)
                && (symbol.is_function() || symbol.is_parameter() || symbol.is_variable())
            {
                symbols.push((symbol_id, symbol.clone()));
                break;
            }
        }

        if symbols.is_empty() {
            let scope_id = self.scopes[scope_id].parent_id;
            scope_id.and_then(|scope_id| {
                self.resolve_ref_symbol_of_scope(scope_id, ref_name, ref_symbol_id)
            })
        } else {
            symbols.first().cloned()
        }
    }
}
