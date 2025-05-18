use crate::SourceInfo;
use crate::source::SourceId;
use crate::symbol::SymbolId;

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
#[cfg(test)]
mod tests {
    use crate::Hir;

    use super::*;

    #[test]
    fn test_add_child() {
        let mut hir = Hir::new();
        let (source_id, _) = hir.add_code(None, "let x = 5");
        let source = SourceInfo::new(Some(source_id), None);
        let mut scope = Scope::new(source, ScopeKind::Module(source_id), None);
        let child_id = ScopeId::default();

        scope.add_child(child_id);
        assert_eq!(scope.children.len(), 1);
        assert_eq!(scope.children[0], child_id);
    }

    #[test]
    fn test_symbol_id() {
        let mut hir = Hir::new();
        let (source_id, _) = hir.add_code(None, "let x = 5");
        let source = SourceInfo::new(Some(source_id), None);
        let symbol_id = hir.symbols().collect::<Vec<_>>().first().unwrap().0;

        let function_scope = Scope::new(source.clone(), ScopeKind::Function(symbol_id), None);
        assert_eq!(function_scope.symbol_id(), Some(symbol_id));

        let let_scope = Scope::new(source.clone(), ScopeKind::Let(symbol_id), None);
        assert_eq!(let_scope.symbol_id(), Some(symbol_id));

        let block_scope = Scope::new(source.clone(), ScopeKind::Block(symbol_id), None);
        assert_eq!(block_scope.symbol_id(), Some(symbol_id));

        let loop_scope = Scope::new(source.clone(), ScopeKind::Loop(symbol_id), None);
        assert_eq!(loop_scope.symbol_id(), Some(symbol_id));

        let module_scope = Scope::new(source.clone(), ScopeKind::Module(source_id), None);
        assert_eq!(module_scope.symbol_id(), None);
    }
}
