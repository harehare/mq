use super::bytecode::UpvalueSource;
use crate::Ident;

/// Always-active root lexical block.
const ROOT_BLOCK: u32 = 0;

pub(crate) struct FunctionScope {
    locals: Vec<Ident>,
    /// Block that declared each `locals` slot, parallel to `locals`.
    local_block: Vec<u32>,
    /// Currently open blocks, innermost last.
    active_blocks: Vec<u32>,
    next_block: u32,
    upvalues: Vec<(Ident, UpvalueSource)>,
    immutable: std::collections::HashSet<u16>,
    pub(crate) shadowed_builtin: Option<Ident>,
}

impl Default for FunctionScope {
    fn default() -> Self {
        Self {
            locals: Vec::new(),
            local_block: Vec::new(),
            active_blocks: vec![ROOT_BLOCK],
            next_block: ROOT_BLOCK + 1,
            upvalues: Vec::new(),
            immutable: std::collections::HashSet::new(),
            shadowed_builtin: None,
        }
    }
}

impl FunctionScope {
    /// Opens a lexical block (match arm, loop body); its names resolve only until `pop_scope`.
    pub(crate) fn push_scope(&mut self) {
        let block = self.next_block;
        self.next_block += 1;
        self.active_blocks.push(block);
    }

    /// Closes the innermost block; its slots keep their indices but stop resolving by name.
    pub(crate) fn pop_scope(&mut self) {
        debug_assert!(self.active_blocks.len() > 1, "pop_scope without matching push_scope");
        self.active_blocks.pop();
    }

    fn current_block(&self) -> u32 {
        *self.active_blocks.last().expect("root block is always active")
    }

    pub(crate) fn declare(&mut self, name: Ident) -> u16 {
        self.locals.push(name);
        self.local_block.push(self.current_block());
        (self.locals.len() - 1) as u16
    }

    pub(crate) fn mark_immutable(&mut self, slot: u16) {
        self.immutable.insert(slot);
    }

    pub(crate) fn is_immutable(&self, slot: u16) -> bool {
        self.immutable.contains(&slot)
    }

    pub(crate) fn unmark_immutable(&mut self, slot: u16) {
        self.immutable.remove(&slot);
    }

    /// Reuses `name`'s slot if one is currently visible (an existing loop counter, e.g.
    /// `let x = x + 1` inside a loop, must keep mutating the same slot every iteration),
    /// else declares fresh in the current block.
    pub(crate) fn declare_or_reuse(&mut self, name: Ident) -> u16 {
        self.resolve_local(name).unwrap_or_else(|| self.declare(name))
    }

    pub(crate) fn declare_synthetic(&mut self) -> u16 {
        self.declare(Ident::default())
    }

    pub(crate) fn set_local_name(&mut self, slot: u16, name: Ident) {
        self.locals[slot as usize] = name;
    }

    pub(crate) fn resolve_local(&self, name: Ident) -> Option<u16> {
        self.locals
            .iter()
            .copied()
            .enumerate()
            .rev()
            .find(|&(i, n)| n == name && self.active_blocks.contains(&self.local_block[i]))
            .map(|(i, _)| i as u16)
    }

    pub(crate) fn resolve_upvalue(&self, name: Ident) -> Option<u16> {
        self.upvalues.iter().position(|(n, _)| *n == name).map(|i| i as u16)
    }

    pub(crate) fn add_upvalue(&mut self, name: Ident, source: UpvalueSource) -> u16 {
        if let Some(idx) = self.resolve_upvalue(name) {
            return idx;
        }
        self.upvalues.push((name, source));
        (self.upvalues.len() - 1) as u16
    }

    /// Captures a synthetic slot, deduplicated by source.
    pub(crate) fn add_upvalue_for_source(&mut self, source: UpvalueSource) -> u16 {
        if let Some(idx) = self.upvalues.iter().position(|(_, s)| *s == source) {
            return idx as u16;
        }
        self.upvalues.push((Ident::default(), source));
        (self.upvalues.len() - 1) as u16
    }

    pub(crate) fn local_count(&self) -> u16 {
        self.locals.len() as u16
    }

    pub(crate) fn local_names(&self) -> Vec<Ident> {
        self.locals.clone()
    }

    pub(crate) fn local_mutable(&self) -> Vec<bool> {
        (0..self.locals.len() as u16)
            .map(|slot| !self.is_immutable(slot))
            .collect()
    }

    pub(crate) fn upvalue_names(&self) -> Vec<Ident> {
        self.upvalues.iter().map(|(name, _)| *name).collect()
    }

    pub(crate) fn upvalue_sources(&self) -> Vec<UpvalueSource> {
        self.upvalues.iter().map(|(_, s)| *s).collect()
    }

    #[cfg(feature = "debugger")]
    pub(crate) fn debug_locals(&self) -> Vec<(Ident, super::debug_symbols::DebugSlot)> {
        self.locals
            .iter()
            .copied()
            .enumerate()
            .filter(|(_, name)| *name != Ident::default())
            .map(|(slot, name)| (name, super::debug_symbols::DebugSlot::Local(slot as u16)))
            .collect()
    }

    #[cfg(feature = "debugger")]
    pub(crate) fn debug_upvalues(&self) -> Vec<(Ident, super::debug_symbols::DebugSlot)> {
        self.upvalues
            .iter()
            .enumerate()
            .map(|(slot, (name, _))| (*name, super::debug_symbols::DebugSlot::Upvalue(slot as u16)))
            .collect()
    }
}
