use std::fmt;

use crate::source::SourceInfo;
use crate::{SourceId, scope::ScopeId};
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
#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    fn create_test_symbol(kind: SymbolKind, name: Option<&str>) -> Symbol {
        Symbol {
            doc: Vec::new(),
            kind,
            name: name.map(CompactString::from),
            scope: ScopeId::default(),
            source: SourceInfo::new(None, None),
            parent: None,
        }
    }

    #[rstest]
    #[case(SymbolKind::Function(vec![]), true)]
    #[case(SymbolKind::Function(vec![CompactString::from("param")]), true)]
    #[case(SymbolKind::Variable, false)]
    #[case(SymbolKind::Call, false)]
    fn test_is_function(#[case] kind: SymbolKind, #[case] expected: bool) {
        let symbol = create_test_symbol(kind, Some("test"));
        assert_eq!(symbol.is_function(), expected);
    }

    #[rstest]
    #[case(SymbolKind::Variable, true)]
    #[case(SymbolKind::Function(vec![]), false)]
    #[case(SymbolKind::Parameter, false)]
    fn test_is_variable(#[case] kind: SymbolKind, #[case] expected: bool) {
        let symbol = create_test_symbol(kind, Some("test"));
        assert_eq!(symbol.is_variable(), expected);
    }

    #[rstest]
    #[case(SymbolKind::Parameter, true)]
    #[case(SymbolKind::Variable, false)]
    #[case(SymbolKind::Function(vec![]), false)]
    fn test_is_parameter(#[case] kind: SymbolKind, #[case] expected: bool) {
        let symbol = create_test_symbol(kind, Some("test"));
        assert_eq!(symbol.is_parameter(), expected);
    }

    #[test]
    fn test_display_with_name() {
        let symbol = create_test_symbol(SymbolKind::Variable, Some("test_var"));
        assert_eq!(format!("{}", symbol), "test_var");
    }

    #[test]
    fn test_display_without_name() {
        let symbol = create_test_symbol(SymbolKind::Variable, None);
        assert_eq!(format!("{}", symbol), "");
    }
}
