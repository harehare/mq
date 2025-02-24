use crate::{Hir, Symbol, SymbolId};

impl Hir {
    pub fn references(&self, symbol_id: SymbolId) -> Vec<(SymbolId, Symbol)> {
        self.references
            .iter()
            .filter_map(|(k, v)| {
                if *v == symbol_id {
                    Some((*k, self.symbols[*k].clone()))
                } else {
                    None
                }
            })
            .collect()
    }
}
