use crate::Ident;

/// A statically resolved variable location available while a VM chunk is paused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DebugSlot {
    /// A slot owned by the paused call frame.
    Local(u16),
    /// A cell captured from an enclosing call frame.
    Upvalue(u16),
}

/// Source-visible names resolved to VM slots for one compiled function.
#[derive(Debug, Clone, Default)]
pub(crate) struct DebugSymbolTable {
    bindings: Vec<(Ident, DebugSlot)>,
}

impl DebugSymbolTable {
    /// Builds a symbol table from the resolver's final local and upvalue layouts.
    pub(crate) fn new(locals: Vec<(Ident, u16)>, upvalues: Vec<(Ident, u16)>) -> Self {
        let mut bindings = Vec::with_capacity(locals.len() + upvalues.len());
        bindings.extend(locals.into_iter().map(|(name, slot)| (name, DebugSlot::Local(slot))));
        bindings.extend(
            upvalues
                .into_iter()
                .map(|(name, slot)| (name, DebugSlot::Upvalue(slot))),
        );
        Self { bindings }
    }

    /// Returns all source-visible bindings in declaration order.
    pub(crate) fn bindings(&self) -> &[(Ident, DebugSlot)] {
        &self.bindings
    }
}
