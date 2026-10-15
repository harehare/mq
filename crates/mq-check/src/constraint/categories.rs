//! Symbol categorization for multi-pass constraint generation.

use mq_hir::{Hir, Symbol, SymbolId, SymbolKind};
use rustc_hash::{FxHashMap, FxHashSet};

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
fn reachable_module_symbols(hir: &Hir) -> FxHashSet<SymbolId> {
    let owner_of = |id: SymbolId| crate::walk_ancestors(hir, id).last().map_or(id, |(owner, _)| owner);
    let is_module = |symbol: &Symbol| is_module(hir, symbol);

    let mut members: FxHashMap<SymbolId, Vec<SymbolId>> = FxHashMap::default();
    let mut pending: Vec<SymbolId> = Vec::new();
    for (id, symbol) in hir.symbols() {
        if hir.is_builtin_symbol(symbol) {
            continue;
        }
        let from_module = is_module(symbol);
        if from_module {
            members.entry(owner_of(id)).or_default().push(id);
        }
        if !from_module && matches!(symbol.kind, SymbolKind::Call | SymbolKind::Ref) {
            pending.extend(referenced_module_owner(hir, id, &is_module, &owner_of));
        }
    }

    let mut seen: FxHashSet<SymbolId> = FxHashSet::default();
    let mut out = FxHashSet::default();
    while let Some(owner) = pending.pop() {
        if !seen.insert(owner) {
            continue;
        }
        for &id in members.get(&owner).into_iter().flatten() {
            out.insert(id);
            if hir
                .symbol(id)
                .is_some_and(|s| matches!(s.kind, SymbolKind::Call | SymbolKind::Ref))
            {
                pending.extend(referenced_module_owner(hir, id, &is_module, &owner_of));
            }
        }
    }
    out
}

fn referenced_module_owner(
    hir: &Hir,
    id: SymbolId,
    is_module: &impl Fn(&Symbol) -> bool,
    owner_of: &impl Fn(SymbolId) -> SymbolId,
) -> Option<SymbolId> {
    let def = hir.resolve_reference_symbol(id)?;
    hir.symbol(def).filter(|s| is_module(s)).map(|_| owner_of(def))
}

/// Whether `symbol` comes from an included or imported module (builtin.mq excluded).
fn is_module(hir: &Hir, symbol: &Symbol) -> bool {
    !hir.is_builtin_symbol(symbol) && symbol.source.source_id.is_some_and(|sid| hir.is_module_source(sid))
}

fn is_definition_keyword(symbol: &Symbol) -> bool {
    symbol.kind == SymbolKind::Keyword && matches!(symbol.value.as_deref(), Some("def" | "let" | "var" | "fn"))
}

/// Categorizes all HIR symbols into processing buckets in a single pass.
pub(super) fn categorize_symbols(hir: &Hir) -> SymbolCategories {
    let mut cats = SymbolCategories {
        pass1_symbols: Vec::new(),
        root_symbols: Vec::new(),
        assign_symbols: Vec::new(),
        pass3_symbols: Vec::new(),
        pass4_functions: Vec::new(),
    };

    let reachable = reachable_module_symbols(hir);

    // Second pass: categorize non-builtin symbols and the module symbols user code reaches
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
