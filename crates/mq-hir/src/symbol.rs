use std::fmt;

use crate::source::SourceInfo;
use crate::{SourceId, scope::ScopeId};
use compact_str::CompactString;
use itertools::Itertools;

slotmap::new_key_type! { pub struct SymbolId; }

type Params = Vec<CompactString>;
pub type Doc = (mq_lang::Range, String);

#[derive(Debug, Clone, PartialEq)]
pub struct Symbol {
    pub doc: Vec<Doc>,
    pub kind: SymbolKind,
    pub value: Option<CompactString>,
    pub scope: ScopeId,
    pub source: SourceInfo,
    pub parent: Option<SymbolId>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SymbolKind {
    Argument,
    Array,
    BinaryOp,
    Boolean,
    Call,
    Dict,
    Elif,
    Else,
    Foreach,
    Function(Params),
    If,
    Include(SourceId),
    Keyword,
    None,
    Number,
    Parameter,
    Ref,
    Selector,
    String,
    UnaryOp,
    Variable,
    While,
    Until,
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

    pub fn is_internal_function(&self) -> bool {
        if matches!(self.kind, SymbolKind::Function(_)) {
            self.value
                .as_ref()
                .is_some_and(|value| value.as_str().starts_with("_"))
        } else {
            false
        }
    }
}

impl fmt::Display for Symbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match &self.kind {
            SymbolKind::Function(args) => &format!("function({})", args.iter().join(", ")),
            _ => self.value.as_ref().map_or("", |value| value.as_str()),
        };
        write!(f, "{}", s)
    }
}
#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    fn create_test_symbol(kind: SymbolKind, value: Option<&str>) -> Symbol {
        Symbol {
            doc: Vec::new(),
            kind,
            value: value.map(CompactString::from),
            scope: ScopeId::default(),
            source: SourceInfo::new(None, None),
            parent: None,
        }
    }

    #[rstest]
    #[case(SymbolKind::Function(Vec::new()), true)]
    #[case(SymbolKind::Function(vec![CompactString::from("param")]), true)]
    #[case(SymbolKind::Variable, false)]
    #[case(SymbolKind::Call, false)]
    fn test_is_function(#[case] kind: SymbolKind, #[case] expected: bool) {
        let symbol = create_test_symbol(kind, Some("test"));
        assert_eq!(symbol.is_function(), expected);
    }

    #[rstest]
    #[case(SymbolKind::Variable, true)]
    #[case(SymbolKind::Function(Vec::new()), false)]
    #[case(SymbolKind::Parameter, false)]
    fn test_is_variable(#[case] kind: SymbolKind, #[case] expected: bool) {
        let symbol = create_test_symbol(kind, Some("test"));
        assert_eq!(symbol.is_variable(), expected);
    }

    #[rstest]
    #[case(SymbolKind::Parameter, true)]
    #[case(SymbolKind::Variable, false)]
    #[case(SymbolKind::Function(Vec::new()), false)]
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
