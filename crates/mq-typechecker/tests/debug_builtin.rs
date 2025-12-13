//! Debug test to understand HIR structure

use mq_hir::{Hir, SymbolId, SymbolKind};

fn get_children(hir: &Hir, parent_id: SymbolId) -> Vec<SymbolId> {
    hir.symbols()
        .filter_map(|(id, symbol)| {
            if symbol.parent == Some(parent_id) {
                Some(id)
            } else {
                None
            }
        })
        .collect()
}

#[test]
fn debug_abs_hir() {
    let mut hir = Hir::default();
    hir.builtin.disabled = false;
    hir.add_builtin();
    hir.add_code(None, "abs(42)");

    println!("\n===== HIR Symbols =====");
    for (id, symbol) in hir.symbols() {
        println!(
            "Symbol {:?}: kind={:?}, value={:?}, source={:?}",
            id, symbol.kind, symbol.value, symbol.source
        );
    }

    println!("\n===== Test Code Symbols =====");
    // Look for symbols related to abs(42) - they should have range info
    let mut call_id = None;
    for (id, symbol) in hir.symbols() {
        if let Some(range) = symbol.source.text_range {
            // Line 1 of the test code
            if range.start.line == 1 {
                println!(
                    "Symbol {:?}: kind={:?}, value={:?}, parent={:?}, range={:?}",
                    id, symbol.kind, symbol.value, symbol.parent, range
                );
                if symbol.kind == SymbolKind::Call && symbol.value.as_deref() == Some("abs") {
                    call_id = Some(id);
                }
            }
        }
    }

    if let Some(call_id) = call_id {
        println!("\n===== Call Node Children (using get_children) =====");
        let children = get_children(&hir, call_id);
        println!("Call {:?} has {} children: {:?}", call_id, children.len(), children);
        for child_id in children {
            if let Some(child) = hir.symbol(child_id) {
                println!("  Child {:?}: kind={:?}, value={:?}", child_id, child.kind, child.value);
            }
        }
    }

    println!("\n===== Looking for Argument symbols in test code =====");
    for (id, symbol) in hir.symbols() {
        if symbol.kind == SymbolKind::Argument
            && let Some(range) = symbol.source.text_range
            && range.start.line == 1
        {
            println!(
                "Argument {:?}: value={:?}, parent={:?}",
                id, symbol.value, symbol.parent
            );
        }
    }

    println!("\n===== Looking for Block symbols in test code =====");
    for (id, symbol) in hir.symbols() {
        if matches!(symbol.kind, SymbolKind::Block)
            && let Some(range) = symbol.source.text_range
            && range.start.line == 1
        {
            println!("Block {:?}: value={:?}, parent={:?}", id, symbol.value, symbol.parent);
            let block_children = get_children(&hir, id);
            for child_id in &block_children {
                if let Some(child) = hir.symbol(*child_id) {
                    println!("  Child {:?}: kind={:?}, value={:?}", child_id, child.kind, child.value);
                }
            }
        }
    }

    println!("\n===== Looking for Ref symbols in test code =====");
    for (id, symbol) in hir.symbols() {
        if symbol.kind == SymbolKind::Ref
            && let Some(range) = symbol.source.text_range
            && range.start.line == 1
        {
            println!("Ref {:?}: value={:?}, parent={:?}", id, symbol.value, symbol.parent);
            if let Some(def_id) = hir.resolve_reference_symbol(id)
                && let Some(def) = hir.symbol(def_id)
            {
                println!(
                    "  -> Resolves to {:?}: kind={:?}, value={:?}",
                    def_id, def.kind, def.value
                );
            }
        }
    }
}

#[test]
fn debug_pipe_hir() {
    let mut hir = Hir::default();
    hir.builtin.disabled = false;
    hir.add_builtin();
    hir.add_code(None, "42 | upcase");

    println!("\n===== Pipe HIR Symbols =====");
    for (id, symbol) in hir.symbols() {
        if let Some(range) = symbol.source.text_range {
            if range.start.line == 1 {
                println!(
                    "Symbol {:?}: kind={:?}, value={:?}, parent={:?}",
                    id, symbol.kind, symbol.value, symbol.parent
                );
                // Show children
                let children = get_children(&hir, id);
                if !children.is_empty() {
                    for child_id in &children {
                        if let Some(child) = hir.symbol(*child_id) {
                            println!("  Child {:?}: kind={:?}, value={:?}", child_id, child.kind, child.value);
                        }
                    }
                }
            }
        }
    }
}
