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
#[cfg(test)]
mod tests {
    use url::Url;

    use super::*;

    #[test]
    fn test_references() {
        let mut hir = Hir::new();
        let url = Url::parse("file:///test").unwrap();
        let _ = hir.add_code(url.clone(), "def func1(): 1; let val1 = func1()");

        assert_eq!(
            hir.references(hir.symbols().collect::<Vec<_>>().first().unwrap().0),
            Vec::new()
        );
    }
}
