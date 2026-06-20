#[cfg(feature = "http-import")]
pub mod http_import;
#[cfg(feature = "http-import")]
pub mod http_resolver;
pub(crate) mod local_fs_resolver;
pub(crate) mod std_resolver;

use crate::module::error::ModuleError;
use std::path::PathBuf;

/// Core interface for resolving mq module source code by name.
pub trait ModuleResolver: Clone + Default {
    /// Returns the source content of `module_name`.
    fn resolve(&self, module_name: &str) -> Result<String, ModuleError>;
    /// Returns the canonical path string for `module_name` (for diagnostics / LSP).
    fn get_path(&self, module_name: &str) -> Result<String, ModuleError>;
    /// Returns the filesystem directories this resolver searches.
    fn search_paths(&self) -> Vec<PathBuf>;
    /// Replaces the filesystem search directories.
    fn set_search_paths(&mut self, paths: Vec<PathBuf>);
    /// Returns the short identifier to store the module under.
    ///
    /// For most resolvers this is `module_path` unchanged.  HTTP-based resolvers
    /// strip the URL prefix and `.mq` suffix so that, for example,
    /// `github.com/alice/mymod.mq@v1.0` becomes `"mymod"`.
    fn canonical_name<'a>(&self, module_path: &'a str) -> &'a str {
        module_path
    }
}

/// The default resolver, combining standard library, local filesystem, and (optionally) HTTP sources.
#[derive(Debug, Clone, Default)]
pub struct DefaultModuleResolver {
    local_fs_resolver: local_fs_resolver::LocalFsModuleResolver,
    std_resolver: std_resolver::StdModuleResolver,
    #[cfg(feature = "http-import-ureq")]
    http_resolver: http_resolver::HttpModuleResolver<http_resolver::UreqFetcher>,
}

impl ModuleResolver for DefaultModuleResolver {
    fn resolve(&self, module_name: &str) -> Result<String, ModuleError> {
        match self.std_resolver.resolve(module_name) {
            Ok(content) => return Ok(content),
            Err(ModuleError::NotFound(_)) => {}
            Err(e) => return Err(e),
        }

        match self.local_fs_resolver.resolve(module_name) {
            Ok(content) => return Ok(content),
            Err(ModuleError::NotFound(_)) => {}
            Err(e) => return Err(e),
        }

        #[cfg(feature = "http-import-ureq")]
        match self.http_resolver.resolve(module_name) {
            Ok(content) => return Ok(content),
            Err(ModuleError::NotFound(_)) => {}
            Err(e) => return Err(e),
        }

        Err(ModuleError::NotFound(format!("{}.mq", module_name).into()))
    }

    fn get_path(&self, module_name: &str) -> Result<String, ModuleError> {
        match self.std_resolver.get_path(module_name) {
            Ok(path) => return Ok(path),
            Err(ModuleError::NotFound(_)) => {}
            Err(e) => return Err(e),
        }

        match self.local_fs_resolver.get_path(module_name) {
            Ok(path) => return Ok(path),
            Err(ModuleError::NotFound(_)) => {}
            Err(e) => return Err(e),
        }

        #[cfg(feature = "http-import-ureq")]
        match self.http_resolver.get_path(module_name) {
            Ok(path) => return Ok(path),
            Err(ModuleError::NotFound(_)) => {}
            Err(e) => return Err(e),
        }

        Err(ModuleError::NotFound(format!("{}.mq", module_name).into()))
    }

    fn search_paths(&self) -> Vec<PathBuf> {
        self.local_fs_resolver.search_paths()
    }

    fn set_search_paths(&mut self, paths: Vec<PathBuf>) {
        self.local_fs_resolver.set_search_paths(paths)
    }

    fn canonical_name<'a>(&self, module_path: &'a str) -> &'a str {
        #[cfg(feature = "http-import-ureq")]
        {
            if http_import::is_github_url(module_path) || http_import::is_remote_url(module_path) {
                return self.http_resolver.canonical_name(module_path);
            }
        }
        module_path
    }
}

impl DefaultModuleResolver {
    /// Creates a new resolver with the given filesystem search paths.
    ///
    /// An empty `paths` slice falls back to the built-in default search directories.
    pub fn new(paths: Vec<PathBuf>) -> Self {
        Self {
            local_fs_resolver: local_fs_resolver::LocalFsModuleResolver::new(if paths.is_empty() {
                None
            } else {
                Some(paths)
            }),
            std_resolver: std_resolver::StdModuleResolver,
            #[cfg(feature = "http-import-ureq")]
            http_resolver: http_resolver::HttpModuleResolver::default(),
        }
    }

    /// Configures the HTTP resolver with a domain allowlist and request timeout.
    ///
    /// An empty `allowed_domains` list restricts access to the built-in default domain
    /// (`raw.githubusercontent.com/harehare`) only; it does not open up all URLs.
    /// Only available when the `http-import-ureq` feature is enabled.
    #[cfg(feature = "http-import-ureq")]
    pub fn with_http(mut self, allowed_domains: Vec<String>, timeout: Option<std::time::Duration>) -> Self {
        self.http_resolver = http_resolver::HttpModuleResolver::new(
            allowed_domains,
            http_resolver::UreqFetcher::new(timeout.unwrap_or(std::time::Duration::from_secs(10))),
        );
        self
    }

    /// Replaces the HTTP resolver's domain allowlist.
    ///
    /// An empty list restricts access to the built-in default domain only.
    /// Entries in the form `github.com/{user}/{repo}` are automatically expanded.
    #[cfg(feature = "http-import-ureq")]
    pub fn set_allowed_domains(&mut self, domains: Vec<String>) {
        self.http_resolver.set_allowed_domains(domains);
    }

    /// Clears all locally-cached HTTP module files (mutable/HEAD only).
    #[cfg(feature = "http-import-ureq")]
    pub fn clear_http_cache(&self) -> Result<(), crate::module::error::ModuleError> {
        self.http_resolver.clear_cache()
    }

    /// Clears all HTTP module cache including versioned modules.
    #[cfg(feature = "http-import-ureq")]
    pub fn clear_http_cache_all(&self) -> Result<(), crate::module::error::ModuleError> {
        self.http_resolver.clear_all_cache()
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;
    use tempfile::TempDir;

    use super::*;

    fn write_module(dir: &TempDir, name: &str, content: &str) {
        std::fs::write(dir.path().join(format!("{}.mq", name)), content).unwrap();
    }

    #[rstest]
    #[case("csv")]
    #[case("json")]
    #[case("yaml")]
    #[case("toml")]
    #[case("md")]
    fn test_resolve_standard_module(#[case] name: &str) {
        let resolver = DefaultModuleResolver::default();
        assert!(resolver.resolve(name).is_ok());
    }

    #[rstest]
    #[case("csv")]
    #[case("json")]
    #[case("md")]
    fn test_get_path_standard_module(#[case] name: &str) {
        let resolver = DefaultModuleResolver::default();
        assert!(resolver.get_path(name).is_ok());
    }

    #[rstest]
    #[case("nonexistent_xyz")]
    fn test_resolve_unknown_module_returns_error(#[case] name: &str) {
        let resolver = DefaultModuleResolver::new(vec![]);
        assert!(resolver.resolve(name).is_err());
    }

    #[test]
    fn test_resolve_local_module() {
        let dir = TempDir::new().unwrap();
        write_module(&dir, "mymod", "def foo(): 1;");

        let resolver = DefaultModuleResolver::new(vec![dir.path().to_path_buf()]);
        assert!(resolver.resolve("mymod").is_ok());
    }

    #[test]
    fn test_std_takes_priority_over_local() {
        let dir = TempDir::new().unwrap();
        write_module(&dir, "csv", "def foo(): 1;");

        let resolver = DefaultModuleResolver::new(vec![dir.path().to_path_buf()]);
        let content = resolver.resolve("csv").unwrap();
        assert!(!content.contains("def foo(): 1;"));
    }

    #[test]
    fn test_search_paths_empty_uses_defaults() {
        let resolver = DefaultModuleResolver::new(vec![]);
        assert!(!resolver.search_paths().is_empty());
    }

    #[test]
    fn test_search_paths_custom() {
        let paths = vec![PathBuf::from("/custom")];
        let resolver = DefaultModuleResolver::new(paths.clone());
        assert_eq!(resolver.search_paths(), paths);
    }

    #[test]
    fn test_set_search_paths() {
        let mut resolver = DefaultModuleResolver::new(vec![]);
        let paths = vec![PathBuf::from("/new")];
        resolver.set_search_paths(paths.clone());
        assert_eq!(resolver.search_paths(), paths);
    }

    #[cfg(feature = "http-import-ureq")]
    #[rstest]
    #[case("https://nonexistent.invalid/foo.mq")]
    fn test_http_url_not_in_local(#[case] url: &str) {
        let resolver = DefaultModuleResolver::new(vec![]);
        assert!(resolver.resolve(url).is_err());
    }

    #[cfg(feature = "http-import-ureq")]
    #[test]
    fn test_with_http_normalizes_github_domains() {
        let resolver = DefaultModuleResolver::new(vec![]).with_http(vec!["github.com/alice/myrepo".to_string()], None);
        assert!(
            resolver
                .http_resolver
                .is_allowed_domain("https://raw.githubusercontent.com/alice/myrepo/HEAD/mod.mq")
        );
        assert!(
            !resolver
                .http_resolver
                .is_allowed_domain("https://raw.githubusercontent.com/alice/other/HEAD/mod.mq")
        );
    }

    #[cfg(feature = "http-import-ureq")]
    #[test]
    fn test_set_allowed_domains_normalizes_github_domains() {
        let mut resolver = DefaultModuleResolver::new(vec![]);
        resolver.set_allowed_domains(vec!["github.com/bob/myrepo".to_string()]);
        assert!(
            resolver
                .http_resolver
                .is_allowed_domain("https://raw.githubusercontent.com/bob/myrepo/HEAD/mod.mq")
        );
        assert!(
            !resolver
                .http_resolver
                .is_allowed_domain("https://raw.githubusercontent.com/bob/other/HEAD/mod.mq")
        );
    }
}
