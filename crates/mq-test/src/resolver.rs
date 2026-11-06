//! Module resolver that serves the modules bundled with `mq-test` (`gen`, `enumerate`, `test_runner`)
//! and defers every other module to the default resolver.

use std::path::PathBuf;

use mq_lang::{DefaultModuleResolver, ModuleError, ModuleResolver};

use crate::runner::TestRunner;

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
///
/// Every other module has its `@requires`/`@ensures` contracts wrapped like the test file's own,
/// so tests exercising a library function also check the contracts written on it.
#[derive(Debug, Clone, Default)]
pub(crate) struct TestModuleResolver {
    fallback: DefaultModuleResolver,
    label_nested: bool,
}

impl TestModuleResolver {
    /// Passes `label_nested` through to [`TestRunner::apply_contracts`].
    pub(crate) fn with_label_nested(mut self, label_nested: bool) -> Self {
        self.label_nested = label_nested;
        self
    }
}

impl ModuleResolver for TestModuleResolver {
    fn resolve(&self, module_name: &str) -> Result<String, ModuleError> {
        if let Some(source) = bundled(module_name) {
            return Ok(source.to_string());
        }
        let source = self.fallback.resolve(module_name)?;
        if !source.contains("@requires") && !source.contains("@ensures") {
            return Ok(source);
        }
        TestRunner::apply_contracts(&source, self.label_nested)
            .map_err(|message| ModuleError::IOError(format!("{module_name}: {message}").into()))
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

    #[test]
    fn test_modules_with_contracts_are_wrapped() {
        let dir = std::env::temp_dir().join(format!("mq_test_contract_module_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("contracted_mod.mq"),
            "# @ensures(result > 0)\ndef f(n): n;\ndef g(n): n;\n",
        )
        .unwrap();
        std::fs::write(dir.join("plain_mod.mq"), "def f(n): n;\n").unwrap();

        let mut resolver = TestModuleResolver::default();
        resolver.set_search_paths(vec![dir.clone()]);
        let wrapped = resolver.resolve("contracted_mod").unwrap();
        let plain = resolver.resolve("plain_mod").unwrap();
        std::fs::remove_dir_all(&dir).unwrap();

        assert!(wrapped.contains("def __orig_f(n)"));
        assert!(wrapped.contains("@ensures violated"));
        assert_eq!(plain, "def f(n): n;\n");
    }
}
