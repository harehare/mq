#[cfg(feature = "http-import")]
pub(crate) mod http_resolver;
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
}

/// The default resolver, combining standard library, local filesystem, and (optionally) HTTP sources.
#[derive(Debug, Clone, Default)]
pub struct DefaultModuleResolver {
    local_fs_resolver: local_fs_resolver::LocalFsModuleResolver,
    std_resolver: std_resolver::StdModuleResolver,
    #[cfg(feature = "http-import")]
    http_resolver: http_resolver::HttpModuleResolver,
}

impl ModuleResolver for DefaultModuleResolver {
    fn resolve(&self, module_name: &str) -> Result<String, ModuleError> {
        if let Ok(content) = self.std_resolver.resolve(module_name) {
            return Ok(content);
        }

        if let Ok(content) = self.local_fs_resolver.resolve(module_name) {
            return Ok(content);
        }

        #[cfg(feature = "http-import")]
        {
            if let Ok(content) = self.http_resolver.resolve(module_name) {
                return Ok(content);
            }
        }

        Err(ModuleError::NotFound(format!("{}.mq", module_name).into()))
    }

    fn get_path(&self, module_name: &str) -> Result<String, ModuleError> {
        if let Ok(path) = self.std_resolver.get_path(module_name) {
            return Ok(path);
        }

        if let Ok(path) = self.local_fs_resolver.get_path(module_name) {
            return Ok(path);
        }

        #[cfg(feature = "http-import")]
        {
            if let Ok(path) = self.http_resolver.get_path(module_name) {
                return Ok(path);
            }
        }

        Err(ModuleError::NotFound(format!("{}.mq", module_name).into()))
    }

    fn search_paths(&self) -> Vec<PathBuf> {
        self.local_fs_resolver.search_paths()
    }

    fn set_search_paths(&mut self, paths: Vec<PathBuf>) {
        self.local_fs_resolver.set_search_paths(paths)
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
            #[cfg(feature = "http-import")]
            http_resolver: http_resolver::HttpModuleResolver::default(),
        }
    }

    /// Configures the HTTP resolver with a domain allowlist and request timeout.
    ///
    /// An empty `allowed_domains` list permits any https/http URL.
    /// Only available when the `http-import` feature is enabled.
    #[cfg(feature = "http-import")]
    pub fn with_http(mut self, allowed_domains: Vec<String>, timeout: Option<std::time::Duration>) -> Self {
        self.http_resolver = http_resolver::HttpModuleResolver::new(
            allowed_domains,
            timeout.unwrap_or(std::time::Duration::from_secs(10)),
        );
        self
    }

    /// Replaces the HTTP resolver's domain allowlist.
    ///
    /// An empty list permits all URLs.
    #[cfg(feature = "http-import")]
    pub fn set_allowed_domains(&mut self, domains: Vec<String>) {
        self.http_resolver.allowed_remote_domains = domains;
    }

    /// Clears all locally-cached HTTP module files.
    ///
    /// Call this once before processing to force a re-fetch of all cached modules
    /// on the next resolve.
    #[cfg(feature = "http-import")]
    pub fn clear_http_cache(&self) -> Result<(), crate::module::error::ModuleError> {
        self.http_resolver.clear_cache()
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
    fn test_resolve_standard_module(#[case] name: &str) {
        let resolver = DefaultModuleResolver::default();
        assert!(resolver.resolve(name).is_ok());
    }

    #[rstest]
    #[case("csv")]
    #[case("json")]
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
        // standard module should win over local file with the same name
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

    #[cfg(feature = "http-import")]
    #[rstest]
    #[case("https://nonexistent.invalid/foo.mq")]
    fn test_http_url_not_in_local(#[case] url: &str) {
        // Without an HTTP resolver configured, should fall through to error
        let resolver = DefaultModuleResolver::new(vec![]);
        // Either network error or module-not-found; should not panic
        assert!(resolver.resolve(url).is_err());
    }
}
