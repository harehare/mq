use crate::{Hir, Symbol, SymbolId};

impl Hir {
    #[inline(always)]
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
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_references() {
        let mut hir = Hir::new();
        let _ = hir.add_code(None, "def func1(): 1; let val1 = func1()");

        assert_eq!(
            hir.references(hir.symbols().collect::<Vec<_>>().first().unwrap().0),
            Vec::new()
        );
    }
}
