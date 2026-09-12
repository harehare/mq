use smol_str::SmolStr;
use thiserror::Error;

use crate::{Hir, ScopeId, ScopeKind, Symbol, SymbolKind};

#[derive(Debug, Error)]
pub enum HirError {
    #[error(
        "Unresolved symbol: {}",
        similar_name
            .as_ref()
            .map(|name| format!("{symbol}. A name with a similar spelling exists: `{name}`."))
            .unwrap_or_else(|| symbol.to_string())
    )]
    UnresolvedSymbol {
        symbol: Symbol,
        similar_name: Option<SmolStr>,
    },
    #[error("Included module not found: {module_name}")]
    ModuleNotFound { symbol: Symbol, module_name: SmolStr },
    #[error("`yield` outside a function")]
    YieldOutsideFunction { symbol: Symbol },
}

#[derive(Debug, Error)]
pub enum HirWarning {
    #[error("Unreachable code after halt() function call")]
    UnreachableCode { symbol: Symbol },
}

impl Hir {
    pub fn errors(&self) -> Vec<HirError> {
        self.symbols
            .iter()
            .filter_map(|(symbol_id, symbol)| match symbol.kind {
                SymbolKind::Call | SymbolKind::Ref => {
                    if self.references.contains_key(&symbol_id) {
                        None
                    } else {
                        Some(HirError::UnresolvedSymbol {
                            symbol: symbol.clone(),
                            similar_name: self.find_similar_names(&symbol.clone().value.unwrap_or_default()),
                        })
                    }
                }
                SymbolKind::Include(_) => {
                    let module_name = symbol.clone().value.unwrap_or(SmolStr::new("unknown")).clone();
                    match self.module_loader.resolve(&module_name) {
                        Ok(_) => None,
                        Err(_) => Some(HirError::ModuleNotFound {
                            symbol: symbol.clone(),
                            module_name,
                        }),
                    }
                }
                SymbolKind::Keyword
                    if symbol.value.as_deref() == Some("yield")
                        && (self.is_outside_function(symbol.scope) || self.crosses_module_boundary(symbol)) =>
                {
                    Some(HirError::YieldOutsideFunction { symbol: symbol.clone() })
                }
                _ => None,
            })
            .collect::<Vec<_>>()
    }

    pub fn warnings(&self) -> Vec<HirWarning> {
        let mut warnings = Vec::new();

        // Find all halt() function calls
        let halt_calls: Vec<_> = self
            .symbols
            .iter()
            .filter(|(_, symbol)| matches!(symbol.kind, SymbolKind::Call) && symbol.value.as_deref() == Some("halt"))
            .collect();

        for (halt_symbol_id, halt_symbol) in halt_calls {
            // Find parent scope that contains this halt call
            if let Some(parent_id) = halt_symbol.parent {
                // Find all symbols that come after the halt call in the same parent
                let unreachable_symbols: Vec<_> = self.symbols
                    .iter()
                    .filter(|(_, other_symbol)| {
                        other_symbol.parent == Some(parent_id) &&
                        other_symbol.source.text_range.as_ref()
                            .zip(halt_symbol.source.text_range.as_ref())
                            .map(|(other_range, halt_range)| {
                                // Check if other symbol comes after halt call
                                other_range.start > halt_range.end
                            })
                            .unwrap_or(false) &&
                        // Don't warn about tokens, trivial symbols, or arguments/literals that are part of the halt call
                        !matches!(other_symbol.kind, SymbolKind::Keyword | SymbolKind::Argument | SymbolKind::Number | SymbolKind::String | SymbolKind::Bytes | SymbolKind::Boolean) &&
                        other_symbol.value.is_some() &&
                        other_symbol.parent != Some(halt_symbol_id)
                    })
                    .collect();

                // Add warnings for unreachable symbols
                for (_, unreachable_symbol) in unreachable_symbols {
                    warnings.push(HirWarning::UnreachableCode {
                        symbol: unreachable_symbol.clone(),
                    });
                }
            }
        }

        warnings
    }

    pub fn error_ranges(&self) -> Vec<(String, mq_lang::Range)> {
        self.errors()
            .iter()
            .map(|e| {
                (
                    e.to_string(),
                    match e {
                        HirError::UnresolvedSymbol { symbol, .. } => symbol.source.text_range.unwrap_or_default(),
                        HirError::ModuleNotFound { symbol, .. } => symbol.source.text_range.unwrap_or_default(),
                        HirError::YieldOutsideFunction { symbol } => symbol.source.text_range.unwrap_or_default(),
                    },
                )
            })
            .collect::<Vec<_>>()
    }

    /// Whether `symbol`'s parent chain hits a `Module` before a `Function`. Inline modules share
    /// their enclosing scope, so `is_outside_function`'s scope walk misses this boundary.
    fn crosses_module_boundary(&self, symbol: &Symbol) -> bool {
        let mut parent_id = symbol.parent;
        while let Some(id) = parent_id {
            let Some(parent) = self.symbols.get(id) else {
                return false;
            };
            match &parent.kind {
                SymbolKind::Module(_) => return true,
                SymbolKind::Function(_) => return false,
                _ => parent_id = parent.parent,
            }
        }
        false
    }

    /// Walks up to the nearest `Function` scope (found) or `Module` scope (top-level, outside).
    fn is_outside_function(&self, mut scope_id: ScopeId) -> bool {
        loop {
            let Some(scope) = self.scopes.get(scope_id) else {
                return true;
            };
            match &scope.kind {
                ScopeKind::Function(_) => return false,
                ScopeKind::Module(_) | ScopeKind::DefaultParam(_) => return true,
                _ => match scope.parent_id {
                    Some(parent_id) => scope_id = parent_id,
                    None => return true,
                },
            }
        }
    }

    pub fn warning_ranges(&self) -> Vec<(String, mq_lang::Range)> {
        self.warnings()
            .iter()
            .map(|w| {
                (
                    w.to_string(),
                    match w {
                        HirWarning::UnreachableCode { symbol } => symbol.source.text_range.unwrap_or_default(),
                    },
                )
            })
            .collect::<Vec<_>>()
    }

    fn find_similar_names(&self, target: &str) -> Option<SmolStr> {
        let candidates: Vec<&str> = self
            .symbols
            .iter()
            .filter_map(|(_, symbol)| {
                if (matches!(&symbol.kind, SymbolKind::Function(_)) || matches!(&symbol.kind, SymbolKind::Variable))
                    && symbol.value.as_ref().is_some_and(|name| name != target)
                {
                    symbol.value.as_deref()
                } else {
                    None
                }
            })
            .collect();

        mq_lang::suggest_name(target, candidates).map(SmolStr::from)
    }
}
#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[test]
    fn test_find_similar_names() {
        let mut hir = Hir::default();
        hir.builtin.disabled = true;
        let _ = hir.add_code(None, "let test1 = 1");

        let similar = hir.find_similar_names("test");
        assert_eq!(similar, Some("test1".into()));

        let no_similar = hir.find_similar_names("xyz123");
        assert!(no_similar.is_none());
    }
    #[test]
    fn test_errors() {
        let mut hir = Hir::default();
        let _ = hir.add_code(None, "let abc = 1 | unknown_var | let xyz = 2");

        let errors = hir.errors();
        assert_eq!(errors.len(), 1);
        match &errors[0] {
            HirError::UnresolvedSymbol { symbol, similar_name } => {
                assert_eq!(symbol.value.as_deref(), Some("unknown_var"));
                assert!(similar_name.is_none());
            }
            _ => {
                panic!("Expected UnresolvedSymbol error");
            }
        }
    }

    #[test]
    fn test_error_ranges() {
        let mut hir = Hir::default();
        let _ = hir.add_code(None, "let abc = 1 | unknown_var | let xyz = 2");
        let error_ranges = hir.error_ranges();
        assert_eq!(error_ranges.len(), 1);
    }

    #[test]
    fn test_warnings_unreachable_after_halt() {
        let mut hir = Hir::default();
        hir.builtin.disabled = true;

        // Test case: halt() followed by unreachable code
        let code = "def test(): halt(1) | let x = 42";
        let _ = hir.add_code(None, code);

        let warnings = hir.warnings();
        assert_eq!(warnings.len(), 1);

        match &warnings[0] {
            HirWarning::UnreachableCode { symbol } => {
                assert_eq!(symbol.value.as_deref(), Some("x"));
                assert_eq!(symbol.kind, SymbolKind::Variable);
            }
        }
    }

    #[test]
    fn test_warnings_no_unreachable_without_halt() {
        let mut hir = Hir::default();
        hir.builtin.disabled = true;

        // Test case: no halt() call
        let code = "def test(): let x = 42 | let y = 24";
        let _ = hir.add_code(None, code);

        let warnings = hir.warnings();
        assert_eq!(warnings.len(), 0);
    }

    #[test]
    fn test_warnings_halt_at_end_no_warning() {
        let mut hir = Hir::default();
        hir.builtin.disabled = true;

        // Test case: halt() at the end, no unreachable code
        let code = "def test(): let x = 42 | halt(1)";
        let _ = hir.add_code(None, code);

        let warnings = hir.warnings();
        assert_eq!(warnings.len(), 0);
    }

    #[test]
    fn test_yield_inside_a_function_is_not_an_error() {
        let mut hir = Hir::default();
        hir.builtin.disabled = true;
        let _ = hir.add_code(None, "def g(): yield: 1;");

        assert!(hir.errors().is_empty());
    }

    #[test]
    fn test_yield_inside_nested_control_flow_is_not_an_error() {
        let mut hir = Hir::default();
        hir.builtin.disabled = true;
        let _ = hir.add_code(None, "def g(): while (true): yield: 1; end;");

        assert!(hir.errors().is_empty());
    }

    #[test]
    fn test_yield_outside_a_function_is_an_error() {
        let mut hir = Hir::default();
        hir.builtin.disabled = true;
        let _ = hir.add_code(None, "yield: 1");

        let errors = hir.errors();
        assert_eq!(errors.len(), 1);
        assert!(matches!(errors[0], HirError::YieldOutsideFunction { .. }));
    }

    #[rstest]
    #[case::def("def f(x = yield: 1): x;")]
    #[case::fn_("let f = fn(x = yield: 1): x; | f()")]
    fn test_yield_in_a_default_param_is_an_error(#[case] code: &str) {
        let mut hir = Hir::default();
        hir.builtin.disabled = true;
        let _ = hir.add_code(None, code);

        let errors = hir.errors();
        assert_eq!(errors.len(), 1);
        assert!(matches!(errors[0], HirError::YieldOutsideFunction { .. }));
    }

    #[test]
    fn test_yield_in_a_nested_fn_does_not_affect_the_outer_function() {
        let mut hir = Hir::default();
        hir.builtin.disabled = true;
        let _ = hir.add_code(None, "def outer(): let inner = fn(): yield: 1; | inner();");

        assert!(hir.errors().is_empty());
    }

    #[test]
    fn test_yield_in_an_inline_module_nested_in_a_function_is_an_error() {
        let mut hir = Hir::default();
        hir.builtin.disabled = true;
        let _ = hir.add_code(None, "def g(): module m: yield: 1 end; | g()");

        let errors = hir.errors();
        assert_eq!(errors.len(), 1);
        assert!(matches!(errors[0], HirError::YieldOutsideFunction { .. }));
    }

    #[test]
    fn test_yield_in_a_function_nested_in_a_module_is_not_an_error() {
        let mut hir = Hir::default();
        hir.builtin.disabled = true;
        let _ = hir.add_code(None, "module a: def b(): yield: 1; end");

        assert!(hir.errors().is_empty());
    }

    #[test]
    fn test_warning_ranges() {
        let mut hir = Hir::default();
        hir.builtin.disabled = true;

        // Test case: halt() followed by unreachable code
        let code = "def test(): halt(1) | let x = 42";
        let _ = hir.add_code(None, code);

        let warning_ranges = hir.warning_ranges();
        assert_eq!(warning_ranges.len(), 1);

        let (message, _) = &warning_ranges[0];
        assert_eq!(message, "Unreachable code after halt() function call");
    }
}
