use std::fmt;

use crate::{SourceId, scope::ScopeId, source::SourceInfo};
use compact_str::CompactString;

slotmap::new_key_type! { pub struct SymbolId; }

type Params = Vec<CompactString>;
pub type Doc = (mq_lang::Range, String);

#[derive(Debug, Clone, PartialEq)]
pub struct Symbol {
    pub doc: Vec<Doc>,
    pub kind: SymbolKind,
    pub name: Option<CompactString>,
    pub scope: ScopeId,
    pub source: SourceInfo,
    pub parent: Option<SymbolId>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SymbolKind {
    Call,
    If,
    Elif,
    Else,
    Foreach,
    Function(Params),
    While,
    Until,
    Include(SourceId),
    Ref,
    Variable,
    Parameter,
    Argument,
    String,
    Boolean,
    Number,
    None,
    Keyword,
    Selector,
}

impl Symbol {
    pub fn is_function(&self) -> bool {
        matches!(self.kind, SymbolKind::Function(_))
    }

    pub fn is_variable(&self) -> bool {
        matches!(self.kind, SymbolKind::Variable)
    }

    pub fn is_parameter(&self) -> bool {
        matches!(self.kind, SymbolKind::Parameter)
    }
}

impl fmt::Display for Symbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name.clone().unwrap_or_default())
    }
}
