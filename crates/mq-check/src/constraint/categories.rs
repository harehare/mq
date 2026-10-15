//! Symbol categorization for multi-pass constraint generation.

use mq_hir::{Hir, Symbol, SymbolId, SymbolKind};
use rustc_hash::FxHashSet;

use super::helpers::{ChildrenIndex, get_children};

/// Pre-categorized symbols for efficient multi-pass constraint generation.
///
/// Built in a single pass over all HIR symbols, replacing 5 separate iterations.
pub(super) struct SymbolCategories {
    /// Pass 1: literals, variables, parameters, function definitions
    pub(super) pass1_symbols: Vec<(SymbolId, SymbolKind)>,
    /// Pass 2: root-level pipe stages (parent=None, non-builtin, non-module, not a `def`/`let`/
    /// `var`/`fn` keyword)
    pub(super) root_symbols: Vec<SymbolId>,
    /// Pass 2.5: Assign symbols (processed before other Pass 3 symbols
    /// so that variable types are updated before Refs and Calls are resolved)
    pub(super) assign_symbols: Vec<(SymbolId, SymbolKind)>,
    /// Pass 3: operators, calls, and other symbols (will be reversed for processing)
    pub(super) pass3_symbols: Vec<(SymbolId, SymbolKind)>,
    /// Pass 4: Function symbols (for body pipe chains)
    pub(super) pass4_functions: Vec<SymbolId>,
}

/// Module symbols reachable from the user's code: every symbol under a top-level module
/// definition that is referenced, directly or through other module definitions.
fn reachable_module_symbols(hir: &Hir, children_index: &ChildrenIndex) -> FxHashSet<SymbolId> {
    let mut reachable = FxHashSet::default();
    if !hir.has_module_sources() {
        return reachable;
    }

    let mut pending: Vec<SymbolId> = hir
        .symbols()
        .filter(|(_, symbol)| {
            matches!(symbol.kind, SymbolKind::Call | SymbolKind::Ref)
                && !hir.is_builtin_symbol(symbol)
                && !is_module(hir, symbol)
        })
        .filter_map(|(id, _)| referenced_module_def(hir, id))
        .collect();

    let mut stack: Vec<SymbolId> = Vec::new();
    while let Some(def) = pending.pop() {
        let owner = crate::walk_ancestors(hir, def).last().map_or(def, |(owner, _)| owner);
        if reachable.contains(&owner) {
            continue;
        }
        stack.push(owner);
        while let Some(id) = stack.pop() {
            reachable.insert(id);
            stack.extend_from_slice(get_children(children_index, id));
            if hir
                .symbol(id)
                .is_some_and(|s| matches!(s.kind, SymbolKind::Call | SymbolKind::Ref))
            {
                pending.extend(referenced_module_def(hir, id));
            }
        }
    }
    reachable
}

/// The module definition that the reference `id` resolves to, if any.
fn referenced_module_def(hir: &Hir, id: SymbolId) -> Option<SymbolId> {
    let def = hir.resolve_reference_symbol(id)?;
    hir.symbol(def).filter(|s| is_module(hir, s)).map(|_| def)
}

/// Whether `symbol` comes from an included or imported module (builtin.mq excluded).
fn is_module(hir: &Hir, symbol: &Symbol) -> bool {
    !hir.is_builtin_symbol(symbol) && symbol.source.source_id.is_some_and(|sid| hir.is_module_source(sid))
}

fn is_definition_keyword(symbol: &Symbol) -> bool {
    symbol.kind == SymbolKind::Keyword && matches!(symbol.value.as_deref(), Some("def" | "let" | "var" | "fn"))
}

/// Categorizes all HIR symbols into processing buckets in a single pass.
pub(super) fn categorize_symbols(hir: &Hir, children_index: &ChildrenIndex) -> SymbolCategories {
    let mut cats = SymbolCategories {
        pass1_symbols: Vec::new(),
        root_symbols: Vec::new(),
        assign_symbols: Vec::new(),
        pass3_symbols: Vec::new(),
        pass4_functions: Vec::new(),
    };

    let reachable = reachable_module_symbols(hir, children_index);

    // Categorize non-builtin symbols and the module symbols user code reaches
    for (symbol_id, symbol) in hir.symbols() {
        if hir.is_builtin_symbol(symbol) {
            continue;
        }
        let is_module = is_module(hir, symbol);
        if is_module && !reachable.contains(&symbol_id) {
            continue;
        }

        // Root symbols (pass 2). The keyword that introduces a definition is not a pipe stage:
        // counting it would hand the document to the keyword instead of to the next stage.
        if symbol.parent.is_none() && !is_definition_keyword(symbol) && !is_module {
            cats.root_symbols.push(symbol_id);
        }

        match &symbol.kind {
            // Pass 1: literals, variables, parameters, function definitions
            SymbolKind::Number
            | SymbolKind::String
            | SymbolKind::InterpolatedString
            | SymbolKind::Boolean
            | SymbolKind::Symbol
            | SymbolKind::None
            | SymbolKind::Variable
            | SymbolKind::DestructuringBinding
            | SymbolKind::Parameter
            | SymbolKind::PatternVariable { .. }
            | SymbolKind::Function(_) => {
                cats.pass1_symbols.push((symbol_id, symbol.kind.clone()));
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

        // Pass 4: Function body pipe chains (also processed in pass 1)
        if matches!(symbol.kind, SymbolKind::Function(_)) {
            cats.pass4_functions.push(symbol_id);
        }
    }

    cats
}
