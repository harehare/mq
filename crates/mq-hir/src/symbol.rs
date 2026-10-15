use std::fmt;

use crate::source::SourceInfo;
use crate::{SourceId, scope::ScopeId};
use itertools::Itertools;
use smol_str::SmolStr;

slotmap::new_key_type! {
    /// Identifier of a [`Symbol`].
    pub struct SymbolId;
}

/// Represents metadata about a function parameter
#[derive(Debug, Clone, PartialEq)]
pub struct ParamInfo {
    /// Parameter name.
    pub name: SmolStr,
    /// Whether the parameter has a default value.
    pub has_default: bool,
    /// Whether the parameter is variadic (`*name`).
    pub is_variadic: bool,
}

impl From<&str> for ParamInfo {
    fn from(name: &str) -> Self {
        ParamInfo {
            name: SmolStr::from(name),
            has_default: false,
            is_variadic: false,
        }
    }
}

impl std::fmt::Display for ParamInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_variadic {
            write!(f, "*{}", self.name)
        } else {
            write!(f, "{}", self.name)
        }
    }
}

type Params = Vec<ParamInfo>;
/// Doc comment text with its source range.
pub type Doc = (mq_lang::Range, String);

/// A named or structural element of the program, such as a function, variable or call.
#[derive(Debug, Clone, PartialEq)]
pub struct Symbol {
    /// Doc comments attached to the symbol.
    pub doc: Vec<Doc>,
    /// What the symbol is.
    pub kind: SymbolKind,
    /// Name or literal text of the symbol, if any.
    pub value: Option<SmolStr>,
    /// Scope the symbol belongs to.
    pub scope: ScopeId,
    /// Where the symbol appears.
    pub source: SourceInfo,
    /// Enclosing symbol, if any.
    pub parent: Option<SymbolId>,
    /// Monotonically increasing counter set at insertion time.
    /// Parent symbols always receive a lower number than their children because
    /// `add_expr` inserts parents before recursing into child nodes.
    pub(crate) insertion_order: u32,
}

/// The syntactic or semantic category of a [`Symbol`].
#[derive(Debug, Clone, PartialEq)]
pub enum SymbolKind {
    /// An argument passed in a call.
    Argument,
    /// An array literal.
    Array,
    /// An assignment.
    Assign,
    /// A binary operator expression.
    BinaryOp,
    /// A block expression.
    Block,
    /// A boolean literal.
    Boolean,
    /// A bytes literal.
    Bytes,
    /// A call to a named function.
    Call,
    /// A call whose callee is an expression.
    CallDynamic,
    /// Represents a destructuring binding (`let [a, b] = expr`).
    /// Owns PatternVariable children and the initializer expression,
    /// mirroring the structure of `Variable` for simple let bindings.
    DestructuringBinding,
    /// A dict literal.
    Dict,
    /// An `elif` branch.
    Elif,
    /// An `else` branch.
    Else,
    /// A `foreach` loop.
    Foreach,
    /// A function definition with its parameters.
    Function(Params),
    /// A bare identifier, such as a module or import alias name.
    Ident,
    /// An `if` expression.
    If,
    /// An `include` of the given source.
    Include(SourceId),
    /// A template string literal (`s"...${expr}..."`).
    InterpolatedString,
    /// A keyword token.
    Keyword,
    /// A `loop` expression.
    Loop,
    /// A `match` expression.
    Match,
    /// A match arm.
    MatchArm {
        /// Whether the arm has an `if` guard.
        has_guard: bool,
    },
    /// The `None` literal.
    None,
    /// A number literal.
    Number,
    /// A function parameter.
    Parameter,
    /// `is_dict` is `true` for dict patterns (`{a, b}` or `{x: y}`),
    /// `false` for array patterns (`[a, b]`) or match arm patterns.
    /// `is_or` is `true` for alternatives (`p1 || p2`), whose children are the alternatives.
    Pattern {
        /// `true` for dict patterns.
        is_dict: bool,
        /// `true` for alternatives.
        is_or: bool,
    },
    /// A variable introduced by a pattern binding.
    ///
    /// `is_rest` is `true` for the rest element in an array pattern (`..rest`),
    /// which has type `Array(elem_ty)` rather than `elem_ty`.
    PatternVariable {
        /// `true` for the rest element of an array pattern.
        is_rest: bool,
    },
    /// A reference to a variable or function.
    Ref,
    /// A selector such as `.h1`.
    Selector(mq_lang::Selector),
    /// A `...expr` spread element inside an array or dict literal.
    Spread,
    /// A string literal.
    String,
    /// A symbol literal (`:name`).
    Symbol,
    /// A unary operator expression.
    UnaryOp,
    /// An `unless` expression.
    Unless,
    /// An `until` loop.
    Until,
    /// A `let` or `var` binding.
    Variable,
    /// A `while` loop.
    While,
    /// A `try` expression.
    Try,
    /// The `catch` branch of a `try`.
    Catch,
    /// An `import` of the given source.
    Import(SourceId),
    /// A module definition for the given source.
    Module(SourceId),
    /// A qualified access such as `module::name`.
    QualifiedAccess,
}

impl Symbol {
    /// Whether the symbol is a function definition.
    #[inline(always)]
    pub fn is_function(&self) -> bool {
        matches!(self.kind, SymbolKind::Function(_))
    }

    /// Whether the symbol is a variable binding.
    #[inline(always)]
    pub fn is_variable(&self) -> bool {
        matches!(self.kind, SymbolKind::Variable | SymbolKind::DestructuringBinding)
    }

    /// Whether the symbol is a module definition.
    #[inline(always)]
    pub fn is_module(&self) -> bool {
        matches!(self.kind, SymbolKind::Module(_))
    }

    /// Whether the symbol is an import.
    #[inline(always)]
    pub fn is_import(&self) -> bool {
        matches!(self.kind, SymbolKind::Import(_))
    }

    /// Whether the symbol is a function parameter.
    #[inline(always)]
    pub fn is_parameter(&self) -> bool {
        matches!(self.kind, SymbolKind::Parameter)
    }

    /// Whether the symbol is a call argument.
    #[inline(always)]
    pub fn is_argument(&self) -> bool {
        matches!(self.kind, SymbolKind::Argument)
    }

    /// Whether the symbol is a pattern variable.
    #[inline(always)]
    pub fn is_pattern_variable(&self) -> bool {
        matches!(self.kind, SymbolKind::PatternVariable { .. })
    }

    /// Whether the symbol is a bare identifier.
    #[inline(always)]
    pub fn is_ident(&self) -> bool {
        matches!(self.kind, SymbolKind::Ident)
    }

    /// Whether the symbol is a function whose name starts with `_`.
    #[inline(always)]
    pub fn is_internal_function(&self) -> bool {
        if matches!(self.kind, SymbolKind::Function(_)) {
            self.value.as_ref().is_some_and(|value| value.as_str().starts_with("_"))
        } else {
            false
        }
    }

    /// Check if documentation contains deprecated marker
    #[inline(always)]
    pub fn is_deprecated(&self) -> bool {
        self.doc
            .iter()
            .any(|(_, text)| text.contains("deprecated") || text.contains("Deprecated") || text.contains("DEPRECATED"))
    }
}

impl fmt::Display for Symbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match &self.kind {
            SymbolKind::Function(args) => &format!("function({})", args.iter().map(|p| p.to_string()).join(", ")),
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
            value: value.map(SmolStr::from),
            scope: ScopeId::default(),
            source: SourceInfo::new(None, None),
            parent: None,
            insertion_order: 0,
        }
    }

    #[rstest]
    #[case(SymbolKind::Function(Vec::new()), true)]
    #[case(SymbolKind::Function(vec![ParamInfo { name: SmolStr::from("param"), has_default: false, is_variadic: false }]), true)]
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
