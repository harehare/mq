use crate::{ModuleError, ModuleResolver, STANDARD_MODULES};
use std::{borrow::Cow, path::PathBuf};

/// Resolves built-in standard library modules (e.g. `csv`, `json`) that are compiled into the binary.
#[derive(Debug, Clone, Default)]
pub struct StdModuleResolver;

impl ModuleResolver for StdModuleResolver {
    fn resolve(&self, module_name: &str) -> Result<String, ModuleError> {
        STANDARD_MODULES
            .get(module_name)
            .map(|f| f().to_string())
            .ok_or_else(|| ModuleError::NotFound(Cow::Owned(format!("{}.mq", module_name))))
    }

    fn get_path(&self, module_name: &str) -> Result<String, ModuleError> {
        if STANDARD_MODULES.contains_key(module_name) {
            Ok(module_name.to_string())
        } else {
            Err(ModuleError::NotFound(Cow::Owned(format!("{}.mq", module_name))))
        }
    }

    fn search_paths(&self) -> Vec<PathBuf> {
        Vec::new()
    }

    fn set_search_paths(&mut self, _paths: Vec<PathBuf>) {}
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case("csv")]
    #[case("json")]
    #[case("yaml")]
    #[case("toml")]
    #[case("xml")]
    #[case("semver")]
    #[case("table")]
    #[case("test")]
    fn test_resolve_known_module(#[case] name: &str) {
        let resolver = StdModuleResolver;
        let result = resolver.resolve(name);
        assert!(result.is_ok(), "expected Ok for std module '{name}', got {result:?}");
        assert!(!result.unwrap().is_empty());
    }

    #[rstest]
    #[case("notamodule")]
    #[case("http")]
    #[case("")]
    fn test_resolve_unknown_module_returns_not_found(#[case] name: &str) {
        let resolver = StdModuleResolver;
        assert!(matches!(resolver.resolve(name), Err(ModuleError::NotFound(_))));
    }

    #[rstest]
    #[case("csv", "csv")]
    #[case("json", "json")]
    fn test_get_path_known_module(#[case] name: &str, #[case] expected: &str) {
        let resolver = StdModuleResolver;
        assert_eq!(resolver.get_path(name).unwrap(), expected);
    }

    #[rstest]
    #[case("notamodule")]
    fn test_get_path_unknown_returns_not_found(#[case] name: &str) {
        let resolver = StdModuleResolver;
        assert!(matches!(resolver.get_path(name), Err(ModuleError::NotFound(_))));
    }

    #[test]
    fn test_search_paths_empty() {
        let resolver = StdModuleResolver;
        assert!(resolver.search_paths().is_empty());
    }
}
