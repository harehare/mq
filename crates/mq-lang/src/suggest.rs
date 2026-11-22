//! "Did you mean" suggestions for unresolved builtin/selector names. Namespace-aware:
//! builtins are only compared against builtins, selectors only against selectors.

use crate::ast::constants::builtins::{BREAKPOINT, NEXT, SEND};
use crate::runtime::builtin::BUILTIN_FUNCTION_NAMES;
use mq_syntax::suggest::closest_match;

pub use mq_syntax::suggest::suggest_selector;

// Callable names the compiler handles itself, so they are absent from the native registry.
const COMPILER_BUILTINS: [&str; 3] = [BREAKPOINT, NEXT, SEND];

/// Closest name to `name` among builtin functions and the given extra candidates (e.g.
/// user-defined functions/variables visible in scope when the lookup failed).
#[cold]
pub fn suggest_name<'a>(name: &str, extra_candidates: impl IntoIterator<Item = &'a str>) -> Option<String> {
    closest_match(
        name,
        BUILTIN_FUNCTION_NAMES
            .iter()
            .copied()
            .chain(COMPILER_BUILTINS)
            .chain(extra_candidates),
    )
    .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::builtin::get_builtin_functions_by_str;

    #[test]
    fn test_suggest_name_finds_real_transposition_typo() {
        assert_eq!(suggest_name("slpit", []), Some("split".to_string()));
    }

    #[test]
    fn test_suggest_name_finds_hyphen_for_underscore_typo() {
        assert_eq!(suggest_name("date-add", []), Some("date_add".to_string()));
    }

    #[test]
    fn test_suggest_name_no_suggestion_for_unrelated_name() {
        assert_eq!(suggest_name("completely_unrelated_xyz", []), None);
    }

    #[test]
    fn test_suggest_name_considers_extra_candidates() {
        assert_eq!(suggest_name("my_fnc", ["my_func"]), Some("my_func".to_string()));
    }

    #[test]
    fn test_builtin_function_names_resolve_by_str() {
        for name in BUILTIN_FUNCTION_NAMES {
            assert!(
                get_builtin_functions_by_str(name).is_some(),
                "{name} is listed but not resolvable"
            );
        }
    }
}
