use super::bytecode::UpvalueSource;
use crate::Ident;

#[derive(Default)]
pub(crate) struct FunctionScope {
    locals: Vec<Ident>,
    upvalues: Vec<(Ident, UpvalueSource)>,
    immutable: std::collections::HashSet<u16>,
    pub(crate) shadowed_builtin: Option<Ident>,
}

impl FunctionScope {
    pub(crate) fn declare(&mut self, name: Ident) -> u16 {
        self.locals.push(name);
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
        self.locals.iter().rposition(|&n| n == name).map(|i| i as u16)
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

    /// Like `add_upvalue`, but for capturing a synthetic (name-hidden) slot — a qualified
    /// `import`/`module` binding — from an ancestor scope. Dedups by `source` instead of by
    /// name: every such capture shares the sentinel `Ident::default()` name, so name-based
    /// dedup (`add_upvalue`) would wrongly treat two different qualified bindings captured
    /// in the same scope as the same upvalue.
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
