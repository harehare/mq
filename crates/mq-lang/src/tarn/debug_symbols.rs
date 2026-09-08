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
    pub(crate) fn new(bindings: impl IntoIterator<Item = (Ident, DebugSlot)>) -> Self {
        Self {
            bindings: bindings.into_iter().collect(),
        }
    }

    /// Returns all source-visible bindings in declaration order.
    pub(crate) fn bindings(&self) -> &[(Ident, DebugSlot)] {
        &self.bindings
    }
}
