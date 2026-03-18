//! Symbol categorization for multi-pass constraint generation.

use mq_hir::{Hir, SymbolId, SymbolKind};
use rustc_hash::FxHashSet;

use super::helpers::is_module_symbol;

/// Pre-categorized symbols for efficient multi-pass constraint generation.
///
/// Built in a single pass over all HIR symbols, replacing 5 separate iterations.
pub(super) struct SymbolCategories {
    /// Source IDs from include/import/module symbols (for skipping module symbols)
    pub(super) module_source_ids: FxHashSet<mq_hir::SourceId>,
    /// Pass 1: literals, variables, parameters, function/macro definitions
    pub(super) pass1_symbols: Vec<(SymbolId, SymbolKind)>,
    /// Pass 2: root-level symbols (parent=None, non-builtin, non-module)
    pub(super) root_symbols: Vec<SymbolId>,
    /// Pass 2.5: Assign symbols (processed before other Pass 3 symbols
    /// so that variable types are updated before Refs and Calls are resolved)
    pub(super) assign_symbols: Vec<(SymbolId, SymbolKind)>,
    /// Pass 3: operators, calls, and other symbols (will be reversed for processing)
    pub(super) pass3_symbols: Vec<(SymbolId, SymbolKind)>,
    /// Pass 4: Block symbols
    pub(super) pass4_blocks: Vec<SymbolId>,
    /// Pass 4: Function/Macro symbols (for body pipe chains)
    pub(super) pass4_functions: Vec<SymbolId>,
}

/// Categorizes all HIR symbols into processing buckets in a single pass.
pub(super) fn categorize_symbols(hir: &Hir) -> SymbolCategories {
    let mut cats = SymbolCategories {
        module_source_ids: FxHashSet::default(),
        pass1_symbols: Vec::new(),
        root_symbols: Vec::new(),
        assign_symbols: Vec::new(),
        pass3_symbols: Vec::new(),
        pass4_blocks: Vec::new(),
        pass4_functions: Vec::new(),
    };

    // First pass: collect module source IDs (needed to filter subsequent symbols)
    for (_, symbol) in hir.symbols() {
        match symbol.kind {
            SymbolKind::Include(source_id) | SymbolKind::Import(source_id) | SymbolKind::Module(source_id) => {
                cats.module_source_ids.insert(source_id);
            }
            _ => {}
        }
    }

    // Second pass: categorize non-builtin, non-module symbols
    for (symbol_id, symbol) in hir.symbols() {
        if hir.is_builtin_symbol(symbol) || is_module_symbol(hir, symbol, &cats.module_source_ids) {
            continue;
        }

        // Root symbols (pass 2)
        if symbol.parent.is_none() {
            cats.root_symbols.push(symbol_id);
        }

        match &symbol.kind {
            // Pass 1: literals, variables, parameters, function/macro definitions
            SymbolKind::Number
            | SymbolKind::String
            | SymbolKind::Boolean
            | SymbolKind::Symbol
            | SymbolKind::None
            | SymbolKind::Variable
            | SymbolKind::DestructuringBinding
            | SymbolKind::Parameter
            | SymbolKind::PatternVariable { .. }
            | SymbolKind::Function(_)
            | SymbolKind::Macro(_) => {
                cats.pass1_symbols.push((symbol_id, symbol.kind.clone()));
            }
            // Pass 4: Block symbols
            SymbolKind::Block => {
                cats.pass4_blocks.push(symbol_id);
            }
            // Pass 2.5: Assign symbols (before other Pass 3 symbols)
            SymbolKind::Assign => {
                cats.assign_symbols.push((symbol_id, symbol.kind.clone()));
            }
            // Pass 3: everything else (operators, calls, etc.)
            _ => {
                cats.pass3_symbols.push((symbol_id, symbol.kind.clone()));
            }
        }

        // Pass 4: Function/Macro body pipe chains (also processed in pass 1)
        if matches!(symbol.kind, SymbolKind::Function(_) | SymbolKind::Macro(_)) {
            cats.pass4_functions.push(symbol_id);
        }
    }

    cats
}
