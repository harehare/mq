use crate::{SourceInfo, source::SourceId, symbol::SymbolId};

slotmap::new_key_type! { pub struct ScopeId; }

#[derive(Debug, Clone)]
pub struct Scope {
    pub source: SourceInfo,
    pub kind: ScopeKind,
    pub parent_id: Option<ScopeId>,
    pub children: Vec<ScopeId>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ScopeKind {
    Module(SourceId),
    Function(SymbolId),
    Let(SymbolId),
    Block(SymbolId),
    Loop(SymbolId),
}

impl Scope {
    pub fn new(source: SourceInfo, kind: ScopeKind, parent_id: Option<ScopeId>) -> Self {
        Self {
            source,
            kind,
            parent_id,
            children: Vec::new(),
        }
    }

    pub fn add_child(&mut self, child_id: ScopeId) {
        self.children.push(child_id);
    }

    pub fn symbol_id(&self) -> Option<SymbolId> {
        match self.kind {
            ScopeKind::Function(symbol_id) => Some(symbol_id),
            ScopeKind::Let(symbol_id) => Some(symbol_id),
            ScopeKind::Block(symbol_id) => Some(symbol_id),
            ScopeKind::Loop(symbol_id) => Some(symbol_id),
            ScopeKind::Module(_) => None,
        }
    }
}
