use std::str::FromStr;

use itertools::Itertools;
use mq_hir::{Hir, Symbol, SymbolId};
use url::Url;

fn main() {
    // Create a new HIR instance
    let mut hir = Hir::new();

    // Add some code to the HIR
    let code = r#"
        def main():
            let x = 42; | x;
    "#;
    hir.add_code(Url::from_str("file:///main.rs").unwrap(), code);

    // Retrieve symbols from the HIR
    let symbols: Vec<(SymbolId, &Symbol)> = hir.symbols().collect_vec();

    // Print the symbols
    for (symbol_id, symbol) in symbols {
        println!("{:?}, {:?}, {:?}", symbol_id, symbol.name, symbol.kind);
    }
}
