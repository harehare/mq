//! Query methods for the HIR — read-only accessors over symbols, scopes, and sources.

use rustc_hash::FxHashMap;
use smol_str::SmolStr;

use crate::{
    Hir, Symbol, SymbolKind,
    scope::{Scope, ScopeId},
    source::SourceId,
    symbol::SymbolId,
};

impl Hir {
    /// Returns `true` if the symbol belongs to the built-in source.
    pub fn is_builtin_symbol(&self, symbol: &Symbol) -> bool {
        symbol.source.source_id == Some(self.builtin.source_id)
    }

    /// Returns an iterator over symbols for a specific source using the index.
    /// This is much faster than iterating over all symbols and filtering.
    pub fn symbols_for_source(&self, source_id: SourceId) -> impl Iterator<Item = (SymbolId, &Symbol)> + '_ {
        self.source_symbols
            .get(&source_id)
            .into_iter()
            .flat_map(move |symbol_ids| {
                symbol_ids
                    .iter()
                    .filter_map(move |&symbol_id| self.symbols.get(symbol_id).map(|symbol| (symbol_id, symbol)))
            })
    }

    /// Returns a list of unused user-defined functions for a given source
    pub fn unused_functions(&self, source_id: SourceId) -> Vec<(SymbolId, &Symbol)> {
        // Build usage map and collect functions using the index for fast lookup
        // This only iterates over symbols from the specific source, not all symbols
        let mut usage_map: FxHashMap<&SmolStr, bool> = FxHashMap::default();
        let mut user_functions = Vec::new();

        for (symbol_id, symbol) in self.symbols_for_source(source_id) {
            // Collect user-defined functions from this source
            if symbol.is_function() && !self.is_builtin_symbol(symbol) && !symbol.is_internal_function() {
                if let Some(ref name) = symbol.value {
                    usage_map.entry(name).or_insert(false);
                    user_functions.push((symbol_id, symbol));
                }
            }
            // Mark functions as used if they're referenced
            else if let Some(ref name) = symbol.value
                && matches!(symbol.kind, SymbolKind::Call | SymbolKind::Ref | SymbolKind::Argument)
            {
                usage_map.entry(name).and_modify(|used| *used = true).or_insert(true);
            }
        }

        // Filter out used functions (O(m) where m = number of user functions in this source)
        user_functions
            .into_iter()
            .filter(|(_, symbol)| {
                symbol
                    .value
                    .as_ref()
                    .and_then(|name| usage_map.get(name))
                    .is_none_or(|&used| !used)
            })
            .collect()
    }

    #[inline(always)]
    pub fn builtins(&self) -> impl Iterator<Item = &mq_lang::BuiltinFunctionDoc> {
        self.builtin.functions.values()
    }

    #[inline(always)]
    pub fn symbol(&self, symbol_id: SymbolId) -> Option<&Symbol> {
        self.symbols.get(symbol_id)
    }

    #[inline(always)]
    pub fn symbols(&self) -> impl Iterator<Item = (SymbolId, &Symbol)> {
        self.symbols.iter()
    }

    #[inline(always)]
    pub fn scope(&self, scope_id: ScopeId) -> Option<&Scope> {
        self.scopes.get(scope_id)
    }

    #[inline(always)]
    pub fn scopes(&self) -> impl Iterator<Item = (ScopeId, &Scope)> {
        self.scopes.iter()
    }
}
