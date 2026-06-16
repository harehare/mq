use std::{borrow::Cow, path::PathBuf};

use super::http_import::{
    extract_module_name, github_to_raw_url, is_allowed_url, is_github_url, is_remote_url, normalize_allowed_domain,
};
use crate::{ModuleError, ModuleResolver};

/// Pluggable HTTP fetch-and-cache backend for [`HttpModuleResolver`].
///
/// Implementations are responsible for both fetching remote content and managing
/// any associated caching (disk, memory, etc.).  The URL passed to [`fetch`] has
/// already been normalized and domain-checked by [`HttpModuleResolver`].
pub trait HttpFetcher: Clone + Default {
    /// Fetch the content at `url`, using any internal cache.
    ///
    /// `url` is always a fully-qualified `https://` URL that has already passed
    /// domain allow-list validation.
    fn fetch(&self, url: &str) -> Result<String, ModuleError>;
}

/// Resolves mq modules from HTTP/HTTPS URLs with optional domain allowlisting.
///
/// The actual HTTP request and caching strategy are delegated to the [`HttpFetcher`]
/// type parameter `F`, making it possible to swap in different backends (e.g. a
/// `ureq`-based native fetcher or a pre-populated in-memory cache for WASM).
///
/// # GitHub shorthand
///
/// In addition to plain `http(s)://` URLs, the resolver accepts shorthand GitHub paths:
///
/// ```text
/// github.com/{owner}/{path}[@{version}]
/// ```
///
/// See [`super::http_import::github_to_raw_url`] for details.
#[derive(Debug, Clone)]
pub struct HttpModuleResolver<F: HttpFetcher> {
    pub(crate) allowed_remote_domains: Vec<String>,
    fetcher: F,
}

impl<F: HttpFetcher> Default for HttpModuleResolver<F> {
    fn default() -> Self {
        Self {
            allowed_remote_domains: Vec::new(),
            fetcher: F::default(),
        }
    }
}

impl<F: HttpFetcher + Default> ModuleResolver for HttpModuleResolver<F> {
    fn canonical_name<'a>(&self, module_path: &'a str) -> &'a str {
        if is_github_url(module_path) || is_remote_url(module_path) {
            extract_module_name(module_path)
        } else {
            module_path
        }
    }

    fn resolve(&self, module_name: &str) -> Result<String, ModuleError> {
        let url = self.to_fetch_url(module_name)?;
        self.fetcher.fetch(&url)
    }

    fn get_path(&self, module_name: &str) -> Result<String, ModuleError> {
        self.to_fetch_url(module_name)
    }

    fn search_paths(&self) -> Vec<PathBuf> {
        Vec::new()
    }

    fn set_search_paths(&mut self, _paths: Vec<PathBuf>) {}
}

impl<F: HttpFetcher> HttpModuleResolver<F> {
    /// Creates a new resolver with the given domain allowlist and fetcher.
    ///
    /// Entries in the form `github.com/{user}/{repo}` are automatically expanded to
    /// `raw.githubusercontent.com/{user}/{repo}`.
    pub fn new(allowed_remote_domains: Vec<String>, fetcher: F) -> Self {
        Self {
            allowed_remote_domains: allowed_remote_domains
                .into_iter()
                .map(|d| normalize_allowed_domain(&d))
                .collect(),
            fetcher,
        }
    }

    /// Returns `true` if `url`'s host/path is permitted by the allowlist.
    pub fn is_allowed_domain(&self, url: &str) -> bool {
        is_allowed_url(url, &self.allowed_remote_domains)
    }

    /// Replaces the domain allowlist.
    ///
    /// Entries in the form `github.com/{user}/{repo}` are automatically normalized.
    pub fn set_allowed_domains(&mut self, domains: Vec<String>) {
        self.allowed_remote_domains = domains.into_iter().map(|d| normalize_allowed_domain(&d)).collect();
    }

    fn to_fetch_url(&self, module_name: &str) -> Result<String, ModuleError> {
        if is_github_url(module_name) {
            let url = github_to_raw_url(module_name)
                .ok_or_else(|| ModuleError::IOError(format!("Invalid GitHub URL: {}", module_name).into()))?;
            if !self.is_allowed_domain(&url) {
                return Err(ModuleError::IOError(format!("Domain not allowed: {}", url).into()));
            }
            return Ok(url);
        }

        if is_remote_url(module_name) {
            if !self.is_allowed_domain(module_name) {
                return Err(ModuleError::IOError(
                    format!("Domain not allowed: {}", module_name).into(),
                ));
            }
            return Ok(module_name.to_string());
        }

        Err(ModuleError::NotFound(Cow::Owned(module_name.to_string())))
    }
}

/// Fetcher backed by `ureq` with local filesystem caching.
///
/// Fetched modules are stored under `{system_cache_dir}/mq/` in one of two subdirectories:
///
/// - `versioned/` — URLs resolved to a specific tag (e.g. `@v0.1.0`); never cleared by
///   [`UreqFetcher::clear_cache`].
/// - `mutable/` — URLs resolved to `HEAD`, `main`, `master`, or any non-GitHub HTTP URL;
///   cleared by [`UreqFetcher::clear_cache`].
///
/// Each cached module is accompanied by a `.mq.sha256` sidecar for tamper detection.
#[cfg(feature = "http-import-ureq")]
#[derive(Debug, Clone)]
pub struct UreqFetcher {
    timeout: std::time::Duration,
    cache_dir: std::path::PathBuf,
}

#[cfg(feature = "http-import-ureq")]
impl Default for UreqFetcher {
    fn default() -> Self {
        Self {
            timeout: std::time::Duration::from_secs(10),
            cache_dir: dirs::cache_dir().unwrap_or_default().join("mq"),
        }
    }
}

#[cfg(feature = "http-import-ureq")]
impl UreqFetcher {
    /// Creates a new fetcher with the given request timeout.
    pub fn new(timeout: std::time::Duration) -> Self {
        Self {
            timeout,
            cache_dir: dirs::cache_dir().unwrap_or_default().join("mq"),
        }
    }

    /// Removes only mutable-ref cached modules (HEAD/branch/non-versioned URLs).
    pub fn clear_cache(&self) -> Result<(), ModuleError> {
        let mutable_dir = self.cache_dir.join("mutable");
        if mutable_dir.exists() {
            std::fs::remove_dir_all(&mutable_dir).map_err(|e| ModuleError::IOError(e.to_string().into()))?;
        }
        Ok(())
    }

    /// Removes all cached modules including versioned (tagged) ones.
    pub fn clear_all_cache(&self) -> Result<(), ModuleError> {
        for subdir in &["mutable", "versioned"] {
            let dir = self.cache_dir.join(subdir);
            if dir.exists() {
                std::fs::remove_dir_all(&dir).map_err(|e| ModuleError::IOError(e.to_string().into()))?;
            }
        }
        Ok(())
    }

    fn cache_subdir(&self, url: &str) -> std::path::PathBuf {
        use super::http_import::is_versioned_url;
        if is_versioned_url(url) {
            self.cache_dir.join("versioned")
        } else {
            self.cache_dir.join("mutable")
        }
    }

    fn cache_stem(url: &str) -> String {
        format!("{:x}", md5::compute(url))
    }

    fn try_read_cache(
        &self,
        cache_file: &std::path::Path,
        hash_file: &std::path::Path,
    ) -> Result<Option<String>, ModuleError> {
        if !cache_file.exists() || !hash_file.exists() {
            return Ok(None);
        }
        let content = std::fs::read_to_string(cache_file).map_err(|e| ModuleError::IOError(e.to_string().into()))?;
        let stored = std::fs::read_to_string(hash_file).map_err(|e| ModuleError::IOError(e.to_string().into()))?;
        if stored.trim() == Self::compute_hash(&content) {
            Ok(Some(content))
        } else {
            Ok(None)
        }
    }

    pub(crate) fn compute_hash(content: &str) -> String {
        use sha2::Digest;
        sha2::Sha256::digest(content.as_bytes())
            .as_slice()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect()
    }
}

/// Maximum response body size for a fetched module (1 MiB).
#[cfg(feature = "http-import-ureq")]
const MAX_MODULE_SIZE: u64 = 1024 * 1024;

#[cfg(feature = "http-import-ureq")]
impl HttpFetcher for UreqFetcher {
    fn fetch(&self, url: &str) -> Result<String, ModuleError> {
        if !url.starts_with("https://") {
            return Err(ModuleError::IOError(
                format!("Only HTTPS URLs are allowed: {}", url).into(),
            ));
        }

        let cache_subdir = self.cache_subdir(url);
        let stem = Self::cache_stem(url);
        let cache_file = cache_subdir.join(format!("{}.mq", stem));
        let hash_file = cache_subdir.join(format!("{}.mq.sha256", stem));

        if let Some(content) = self.try_read_cache(&cache_file, &hash_file)? {
            return Ok(content);
        }

        std::fs::create_dir_all(&cache_subdir).map_err(|e| ModuleError::IOError(e.to_string().into()))?;
        let lock_path = cache_subdir.join(format!("{}.mq.lock", stem));
        let lock_file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(|e| ModuleError::IOError(e.to_string().into()))?;
        lock_file
            .lock()
            .map_err(|e| ModuleError::IOError(e.to_string().into()))?;

        if let Some(content) = self.try_read_cache(&cache_file, &hash_file)? {
            return Ok(content);
        }

        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(self.timeout))
            .https_only(true)
            .max_redirects(0)
            .build()
            .into();

        let mut response = agent
            .get(url)
            .call()
            .map_err(|e| ModuleError::IOError(e.to_string().into()))?;

        if response.status() != 200 {
            return Err(ModuleError::IOError(
                format!("Failed to fetch module: {} (status: {})", url, response.status()).into(),
            ));
        }

        let is_html = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.contains("text/html"))
            .unwrap_or(false);
        if is_html {
            return Err(ModuleError::IOError(
                format!(
                    "URL returned HTML instead of mq source code: {}. Check that the URL points directly to a .mq file.",
                    url
                )
                .into(),
            ));
        }

        let content = response
            .body_mut()
            .with_config()
            .limit(MAX_MODULE_SIZE)
            .read_to_string()
            .map_err(|e| ModuleError::IOError(e.to_string().into()))?;

        std::fs::write(&cache_file, content.as_bytes()).map_err(|e| ModuleError::IOError(e.to_string().into()))?;
        std::fs::write(&hash_file, Self::compute_hash(&content).as_bytes())
            .map_err(|e| ModuleError::IOError(e.to_string().into()))?;

        drop(lock_file);
        Ok(content)
    }
}

/// Cache management methods specific to the `ureq`-backed resolver.
#[cfg(feature = "http-import-ureq")]
impl HttpModuleResolver<UreqFetcher> {
    /// Removes only mutable-ref cached modules.
    pub fn clear_cache(&self) -> Result<(), ModuleError> {
        self.fetcher.clear_cache()
    }

    /// Removes all cached modules including versioned ones.
    pub fn clear_all_cache(&self) -> Result<(), ModuleError> {
        self.fetcher.clear_all_cache()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "http-import-ureq")]
    use {rstest::rstest, std::time::Duration, tempfile::TempDir};

    #[cfg(feature = "http-import-ureq")]
    fn resolver_with_domains(domains: Vec<String>) -> HttpModuleResolver<UreqFetcher> {
        HttpModuleResolver::new(
            domains,
            UreqFetcher {
                cache_dir: std::path::PathBuf::from("/tmp/mq-test-cache"),
                ..UreqFetcher::default()
            },
        )
    }

    #[rstest]
    #[case("github.com/alice/mymod", "mymod")]
    #[case("github.com/alice/mymod.mq@v1.0", "mymod")]
    #[case("https://example.com/foo.mq", "foo")]
    #[case("local_module", "local_module")]
    #[cfg(feature = "http-import-ureq")]
    fn test_canonical_name(#[case] input: &str, #[case] expected: &str) {
        let resolver = HttpModuleResolver::<UreqFetcher>::default();
        assert_eq!(resolver.canonical_name(input), expected);
    }

    #[rstest]
    #[case(vec!["github.com/alice/myrepo".to_string()], "https://raw.githubusercontent.com/alice/myrepo/HEAD/mod.mq", true)]
    #[case(vec!["github.com/alice/myrepo".to_string()], "https://raw.githubusercontent.com/alice/other/HEAD/mod.mq", false)]
    #[case(vec!["example.com".to_string()], "https://example.com/foo.mq", true)]
    #[cfg(feature = "http-import-ureq")]
    fn test_new_normalizes_github_domains(#[case] domains: Vec<String>, #[case] url: &str, #[case] expected: bool) {
        let resolver = HttpModuleResolver::new(domains, UreqFetcher::new(Duration::from_secs(10)));
        assert_eq!(resolver.is_allowed_domain(url), expected);
    }

    #[test]
    #[cfg(feature = "http-import-ureq")]
    fn test_to_fetch_url_allowed_via_github_shorthand_domain() {
        let resolver = HttpModuleResolver::new(
            vec!["github.com/alice/lisp".to_string()],
            UreqFetcher::new(Duration::from_secs(10)),
        );
        assert!(resolver.to_fetch_url("github.com/alice/lisp").is_ok());
        assert!(resolver.to_fetch_url("github.com/alice/other").is_err());
    }

    #[rstest]
    #[case(
        "github.com/harehare/lisp.mq@v0.1.0",
        "https://raw.githubusercontent.com/harehare/lisp.mq/v0.1.0/lisp.mq"
    )]
    #[case(
        "github.com/harehare/lisp",
        "https://raw.githubusercontent.com/harehare/lisp/HEAD/lisp.mq"
    )]
    #[case(
        "https://github.com/harehare/lisp@v0.1.0",
        "https://raw.githubusercontent.com/harehare/lisp/v0.1.0/lisp.mq"
    )]
    #[cfg(feature = "http-import-ureq")]
    fn test_to_fetch_url_with_empty_allowlist(#[case] input: &str, #[case] expected: &str) {
        let resolver = resolver_with_domains(vec![]);
        assert_eq!(resolver.to_fetch_url(input).unwrap(), expected);
    }

    #[rstest]
    #[case(vec![], "github.com/alice/lisp")]
    #[case(vec!["example.com".to_string()], "github.com/alice/lisp")]
    #[case(vec!["example.com".to_string()], "https://other.com/foo.mq")]
    #[case(vec![], "https://example.com/foo.mq")]
    #[cfg(feature = "http-import-ureq")]
    fn test_to_fetch_url_blocked_by_allowlist(#[case] allowed: Vec<String>, #[case] input: &str) {
        let resolver = resolver_with_domains(allowed);
        assert!(matches!(resolver.to_fetch_url(input), Err(ModuleError::IOError(_))));
    }

    #[rstest]
    #[case("local_module")]
    #[cfg(feature = "http-import-ureq")]
    fn test_to_fetch_url_returns_not_found(#[case] input: &str) {
        let resolver = resolver_with_domains(vec![]);
        assert!(matches!(resolver.to_fetch_url(input), Err(ModuleError::NotFound(_))));
    }

    #[test]
    #[cfg(feature = "http-import-ureq")]
    fn test_to_fetch_url_invalid_github_shorthand_returns_io_error() {
        let resolver = resolver_with_domains(vec![]);
        assert!(matches!(
            resolver.to_fetch_url("github.com/owner"),
            Err(ModuleError::IOError(_))
        ));
    }

    #[test]
    #[cfg(feature = "http-import-ureq")]
    fn test_cache_subdir_versioned() {
        let dir = TempDir::new().unwrap();
        let fetcher = UreqFetcher {
            cache_dir: dir.path().to_path_buf(),
            ..UreqFetcher::default()
        };
        let resolver = HttpModuleResolver::new(vec![], fetcher);
        let subdir = resolver
            .fetcher
            .cache_subdir("https://raw.githubusercontent.com/alice/mymod/v0.1.0/mymod.mq");
        assert_eq!(subdir, dir.path().join("versioned"));
    }

    #[test]
    #[cfg(feature = "http-import-ureq")]
    fn test_cache_subdir_mutable() {
        let dir = TempDir::new().unwrap();
        let fetcher = UreqFetcher {
            cache_dir: dir.path().to_path_buf(),
            ..UreqFetcher::default()
        };
        let resolver = HttpModuleResolver::new(vec![], fetcher);
        let subdir = resolver
            .fetcher
            .cache_subdir("https://raw.githubusercontent.com/alice/mymod/HEAD/mymod.mq");
        assert_eq!(subdir, dir.path().join("mutable"));
    }

    #[test]
    #[cfg(feature = "http-import-ureq")]
    fn test_resolve_uses_mutable_cache_on_hit() {
        let dir = TempDir::new().unwrap();
        let mutable_dir = dir.path().join("mutable");
        std::fs::create_dir_all(&mutable_dir).unwrap();

        let url = "https://raw.githubusercontent.com/harehare/mymod/HEAD/mymod.mq";
        let content = "def cached(): 42;";
        let stem = UreqFetcher::cache_stem(url);
        std::fs::write(mutable_dir.join(format!("{}.mq", stem)), content.as_bytes()).unwrap();
        std::fs::write(
            mutable_dir.join(format!("{}.mq.sha256", stem)),
            UreqFetcher::compute_hash(content).as_bytes(),
        )
        .unwrap();

        let fetcher = UreqFetcher {
            cache_dir: dir.path().to_path_buf(),
            ..UreqFetcher::default()
        };
        let resolver = HttpModuleResolver::new(vec![], fetcher);
        assert_eq!(resolver.resolve(url).unwrap(), content);
    }

    #[test]
    #[cfg(feature = "http-import-ureq")]
    fn test_resolve_cache_without_hash_sidecar_triggers_refetch() {
        let dir = TempDir::new().unwrap();
        let mutable_dir = dir.path().join("mutable");
        std::fs::create_dir_all(&mutable_dir).unwrap();

        let url = "https://raw.githubusercontent.com/harehare/mymod/HEAD/mymod.mq";
        let stem = UreqFetcher::cache_stem(url);
        std::fs::write(mutable_dir.join(format!("{}.mq", stem)), b"def foo(): 1;").unwrap();

        let fetcher = UreqFetcher {
            cache_dir: dir.path().to_path_buf(),
            ..UreqFetcher::default()
        };
        let resolver = HttpModuleResolver::new(vec![], fetcher);
        assert!(resolver.resolve(url).is_err());
    }

    #[test]
    #[cfg(feature = "http-import-ureq")]
    fn test_resolve_tampered_cache_triggers_refetch() {
        let dir = TempDir::new().unwrap();
        let mutable_dir = dir.path().join("mutable");
        std::fs::create_dir_all(&mutable_dir).unwrap();

        let url = "https://raw.githubusercontent.com/harehare/mymod/HEAD/mymod.mq";
        let content = "def cached(): 42;";
        let stem = UreqFetcher::cache_stem(url);
        std::fs::write(mutable_dir.join(format!("{}.mq", stem)), content.as_bytes()).unwrap();
        std::fs::write(
            mutable_dir.join(format!("{}.mq.sha256", stem)),
            b"0000000000000000000000000000000000000000000000000000000000000000",
        )
        .unwrap();

        let fetcher = UreqFetcher {
            cache_dir: dir.path().to_path_buf(),
            ..UreqFetcher::default()
        };
        let resolver = HttpModuleResolver::new(vec![], fetcher);
        assert!(resolver.resolve(url).is_err());
    }

    #[test]
    #[cfg(feature = "http-import-ureq")]
    fn test_resolve_uses_versioned_cache_on_hit() {
        let dir = TempDir::new().unwrap();
        let versioned_dir = dir.path().join("versioned");
        std::fs::create_dir_all(&versioned_dir).unwrap();

        let url = "https://raw.githubusercontent.com/harehare/mymod/v0.1.0/mymod.mq";
        let content = "def pinned(): 1;";
        let stem = UreqFetcher::cache_stem(url);
        std::fs::write(versioned_dir.join(format!("{}.mq", stem)), content.as_bytes()).unwrap();
        std::fs::write(
            versioned_dir.join(format!("{}.mq.sha256", stem)),
            UreqFetcher::compute_hash(content).as_bytes(),
        )
        .unwrap();

        let fetcher = UreqFetcher {
            cache_dir: dir.path().to_path_buf(),
            ..UreqFetcher::default()
        };
        let resolver = HttpModuleResolver::new(vec![], fetcher);
        assert_eq!(resolver.resolve(url).unwrap(), content);
    }

    #[rstest]
    #[case("not_a_url")]
    #[case("local/module")]
    #[case("csv")]
    #[case("")]
    #[cfg(feature = "http-import-ureq")]
    fn test_resolve_non_url_returns_not_found(#[case] module_name: &str) {
        let resolver = HttpModuleResolver::<UreqFetcher>::default();
        assert!(matches!(resolver.resolve(module_name), Err(ModuleError::NotFound(_))));
    }

    #[rstest]
    #[case("not_a_url")]
    #[case("csv")]
    #[cfg(feature = "http-import-ureq")]
    fn test_get_path_non_url_returns_not_found(#[case] module_name: &str) {
        let resolver = HttpModuleResolver::<UreqFetcher>::default();
        assert!(matches!(resolver.get_path(module_name), Err(ModuleError::NotFound(_))));
    }

    #[rstest]
    #[case(vec!["other.com".to_string()], "https://example.com/foo.mq")]
    #[case(vec![], "https://example.com/foo.mq")]
    #[case(vec![], "https://raw.githubusercontent.com/alice/mod/HEAD/mod.mq")]
    #[cfg(feature = "http-import-ureq")]
    fn test_resolve_blocked_domain_returns_io_error(#[case] allowed: Vec<String>, #[case] url: &str) {
        let resolver = resolver_with_domains(allowed);
        assert!(matches!(resolver.resolve(url), Err(ModuleError::IOError(_))));
    }

    #[rstest]
    #[case("http://example.com/foo.mq")]
    #[case("http://raw.githubusercontent.com/harehare/mod/HEAD/mod.mq")]
    #[cfg(feature = "http-import-ureq")]
    fn test_fetch_rejects_http(#[case] url: &str) {
        let fetcher = UreqFetcher::default();
        assert!(matches!(fetcher.fetch(url), Err(ModuleError::IOError(_))));
    }

    #[test]
    #[cfg(feature = "http-import-ureq")]
    fn test_fetch_rejects_non_default_domain_with_empty_allowlist() {
        let resolver = resolver_with_domains(vec![]);
        assert!(matches!(
            resolver.resolve("https://example.com/foo.mq"),
            Err(ModuleError::IOError(_))
        ));
    }

    #[test]
    #[cfg(feature = "http-import-ureq")]
    fn test_search_paths_empty() {
        assert!(HttpModuleResolver::<UreqFetcher>::default().search_paths().is_empty());
    }

    #[test]
    #[cfg(feature = "http-import-ureq")]
    fn test_compute_hash_is_deterministic() {
        let h1 = UreqFetcher::compute_hash("def foo(): 1;");
        let h2 = UreqFetcher::compute_hash("def foo(): 1;");
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64);
    }

    #[test]
    #[cfg(feature = "http-import-ureq")]
    fn test_clear_cache_removes_only_mutable() {
        let dir = TempDir::new().unwrap();
        let mutable_dir = dir.path().join("mutable");
        let versioned_dir = dir.path().join("versioned");
        std::fs::create_dir_all(&mutable_dir).unwrap();
        std::fs::create_dir_all(&versioned_dir).unwrap();
        std::fs::write(mutable_dir.join("a.mq"), b"mutable").unwrap();
        std::fs::write(versioned_dir.join("b.mq"), b"versioned").unwrap();

        let fetcher = UreqFetcher {
            cache_dir: dir.path().to_path_buf(),
            ..UreqFetcher::default()
        };
        let resolver = HttpModuleResolver::new(vec![], fetcher);
        resolver.clear_cache().unwrap();
        assert!(!mutable_dir.exists());
        assert!(versioned_dir.join("b.mq").exists());
    }

    #[test]
    #[cfg(feature = "http-import-ureq")]
    fn test_clear_all_cache_removes_both() {
        let dir = TempDir::new().unwrap();
        let mutable_dir = dir.path().join("mutable");
        let versioned_dir = dir.path().join("versioned");
        std::fs::create_dir_all(&mutable_dir).unwrap();
        std::fs::create_dir_all(&versioned_dir).unwrap();
        std::fs::write(mutable_dir.join("a.mq"), b"mutable").unwrap();
        std::fs::write(versioned_dir.join("b.mq"), b"versioned").unwrap();

        let fetcher = UreqFetcher {
            cache_dir: dir.path().to_path_buf(),
            ..UreqFetcher::default()
        };
        let resolver = HttpModuleResolver::new(vec![], fetcher);
        resolver.clear_all_cache().unwrap();
        assert!(!mutable_dir.exists());
        assert!(!versioned_dir.exists());
    }

    #[rstest]
    #[case(
        "https://github.com/harehare/lisp@v0.1.0",
        "https://raw.githubusercontent.com/harehare/lisp/v0.1.0/lisp.mq"
    )]
    #[case(
        "https://github.com/harehare/lisp",
        "https://raw.githubusercontent.com/harehare/lisp/HEAD/lisp.mq"
    )]
    #[cfg(feature = "http-import-ureq")]
    fn test_to_fetch_url_https_github_form(#[case] input: &str, #[case] expected: &str) {
        let resolver = resolver_with_domains(vec![]);
        assert_eq!(resolver.to_fetch_url(input).unwrap(), expected);
    }
}
