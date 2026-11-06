//! Module resolver that serves the modules bundled with `mq-test` (`gen`, `enumerate`, `test_runner`)
//! and defers every other module to the default resolver.

use std::path::PathBuf;

use mq_lang::{DefaultModuleResolver, ModuleError, ModuleResolver};

/// Modules compiled into `mq-test`, by import name.
const BUNDLED_MODULES: &[(&str, &str)] = &[
    ("test_runner", include_str!("../modules/test_runner.mq")),
    ("gen", include_str!("../modules/gen.mq")),
    ("enumerate", include_str!("../modules/enumerate.mq")),
];

fn bundled(module_name: &str) -> Option<&'static str> {
    BUNDLED_MODULES
        .iter()
        .find(|(name, _)| *name == module_name)
        .map(|(_, source)| *source)
}

/// Resolves `gen`, `enumerate` and `test_runner` from the sources bundled in `mq-test`.
#[derive(Debug, Clone, Default)]
pub(crate) struct TestModuleResolver {
    fallback: DefaultModuleResolver,
}

impl ModuleResolver for TestModuleResolver {
    fn resolve(&self, module_name: &str) -> Result<String, ModuleError> {
        match bundled(module_name) {
            Some(source) => Ok(source.to_string()),
            None => self.fallback.resolve(module_name),
        }
    }

    fn get_path(&self, module_name: &str) -> Result<String, ModuleError> {
        match bundled(module_name) {
            Some(_) => Ok(module_name.to_string()),
            None => self.fallback.get_path(module_name),
        }
    }

    fn search_paths(&self) -> Vec<PathBuf> {
        self.fallback.search_paths()
    }

    fn set_search_paths(&mut self, paths: Vec<PathBuf>) {
        self.fallback.set_search_paths(paths);
    }

    fn canonical_name<'a>(&self, module_path: &'a str) -> &'a str {
        self.fallback.canonical_name(module_path)
    }

    fn is_immutable_module(&self, module_path: &str) -> bool {
        bundled(module_path).is_some() || self.fallback.is_immutable_module(module_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case("test_runner", "def _small_scope_result(")]
    #[case("gen", "def tuple(")]
    #[case("enumerate", "def tuple(")]
    fn test_bundled_modules_resolve(#[case] name: &str, #[case] expected: &str) {
        let source = TestModuleResolver::default().resolve(name).unwrap();
        assert!(source.contains(expected));
        assert!(TestModuleResolver::default().is_immutable_module(name));
    }

    #[test]
    fn test_other_modules_fall_back_to_the_default_resolver() {
        let resolver = TestModuleResolver::default();
        assert!(resolver.resolve("csv").is_ok());
        assert!(matches!(
            resolver.resolve("no_such_module_xyz"),
            Err(ModuleError::NotFound(_))
        ));
    }
}
