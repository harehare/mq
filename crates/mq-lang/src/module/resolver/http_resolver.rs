use super::http_import::{
    extract_module_name, github_to_raw_url, is_allowed_url, is_github_url, is_remote_url, normalize_allowed_domain,
};
use crate::module::resolver::lockfile;
use crate::{ModuleError, ModuleResolver};
use std::{borrow::Cow, path::PathBuf};

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

/// A module read from the on-disk cache, paired with its content-verified SHA-256 hash.
#[cfg(feature = "http-import-ureq")]
struct CachedModule {
    content: String,
    hash: String,
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
///
/// Separately, every fetched URL's content hash is recorded in `mq.lock` (see
/// [`super::lockfile`]) to detect drift in mutable refs across fetches. Cache hits are
/// verified (and recorded) against `mq.lock` too, so a per-project lock file still
/// protects runs that are served entirely from the shared on-disk cache.
#[cfg(feature = "http-import-ureq")]
#[derive(Debug, Clone)]
pub struct UreqFetcher {
    timeout: std::time::Duration,
    cache_dir: std::path::PathBuf,
    lockfile_path: std::path::PathBuf,
    lockfile_enabled: bool,
    lockfile_cache: std::sync::Arc<std::sync::Mutex<Option<super::lockfile::ModuleLock>>>,
}

#[cfg(feature = "http-import-ureq")]
impl Default for UreqFetcher {
    fn default() -> Self {
        Self {
            timeout: std::time::Duration::from_secs(10),
            cache_dir: dirs::cache_dir().unwrap_or_default().join("mq"),
            lockfile_path: std::path::PathBuf::from(super::lockfile::LOCKFILE_NAME),
            lockfile_enabled: true,
            lockfile_cache: std::sync::Arc::new(std::sync::Mutex::new(None)),
        }
    }
}

#[cfg(feature = "http-import-ureq")]
impl UreqFetcher {
    /// Creates a new fetcher with the given request timeout.
    pub fn new(timeout: std::time::Duration) -> Self {
        Self {
            timeout,
            ..Self::default()
        }
    }

    /// Enables or disables the `mq.lock` integrity check/update. The path is unaffected.
    pub fn set_lockfile_enabled(&mut self, enabled: bool) {
        self.lockfile_enabled = enabled;
    }

    /// Sets the path used for `mq.lock`. Clears any in-memory cache of the previous path.
    pub fn set_lockfile_path(&mut self, path: std::path::PathBuf) {
        self.lockfile_path = path;
        *self.lockfile_cache.lock().unwrap() = None;
    }

    pub(crate) fn lockfile_path(&self) -> std::path::PathBuf {
        self.lockfile_path.clone()
    }

    pub(crate) fn lockfile_enabled(&self) -> bool {
        self.lockfile_enabled
    }

    /// Removes mutable-ref cached modules and their `mq.lock` entries, regardless of
    /// whether the lock check is currently enabled.
    pub fn clear_cache(&self) -> Result<(), ModuleError> {
        let mutable_dir = self.cache_dir.join("mutable");
        if mutable_dir.exists() {
            std::fs::remove_dir_all(&mutable_dir).map_err(|e| ModuleError::IOError(e.to_string().into()))?;
        }
        self.with_lockfile_guard(|| {
            if !self.lockfile_path.exists() {
                return Ok(());
            }
            let mut lock = Self::load_lock(&self.lockfile_path)?;
            lock.retain_versioned_only();
            Self::save_lock(&self.lockfile_path, &lock)?;
            *self.lockfile_cache.lock().unwrap() = Some(lock);
            Ok(())
        })
    }

    /// Removes all cached modules including versioned (tagged) ones, and deletes `mq.lock`,
    /// regardless of whether the lock check is currently enabled.
    pub fn clear_all_cache(&self) -> Result<(), ModuleError> {
        for subdir in &["mutable", "versioned"] {
            let dir = self.cache_dir.join(subdir);
            if dir.exists() {
                std::fs::remove_dir_all(&dir).map_err(|e| ModuleError::IOError(e.to_string().into()))?;
            }
        }
        self.with_lockfile_guard(|| {
            if self.lockfile_path.exists() {
                std::fs::remove_file(&self.lockfile_path).map_err(|e| ModuleError::IOError(e.to_string().into()))?;
            }
            let _ = std::fs::remove_file(self.lockfile_flock_path());
            *self.lockfile_cache.lock().unwrap() = None;
            Ok(())
        })
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
    ) -> Result<Option<CachedModule>, ModuleError> {
        if !cache_file.exists() || !hash_file.exists() {
            return Ok(None);
        }
        let content = std::fs::read_to_string(cache_file).map_err(|e| ModuleError::IOError(e.to_string().into()))?;
        let stored = std::fs::read_to_string(hash_file).map_err(|e| ModuleError::IOError(e.to_string().into()))?;
        if stored.trim() == Self::compute_hash(&content) {
            let hash = stored.trim().to_string();
            Ok(Some(CachedModule { content, hash }))
        } else {
            Ok(None)
        }
    }

    /// SHA-256 hex digest of `content`. Delegates to [`super::lockfile::compute_hash`], the
    /// single implementation shared with the WASM fetcher.
    pub fn compute_hash(content: &str) -> String {
        super::lockfile::compute_hash(content)
    }

    fn load_lock(path: &std::path::Path) -> Result<super::lockfile::ModuleLock, ModuleError> {
        if !path.exists() {
            return Ok(super::lockfile::ModuleLock::default());
        }
        let content = std::fs::read_to_string(path).map_err(|e| ModuleError::IOError(e.to_string().into()))?;
        super::lockfile::ModuleLock::parse(&content).map_err(|e| {
            ModuleError::IOError(
                format!(
                    "failed to parse {}: {e}. Delete the file to reset it, or pass --no-lockfile to skip the check.",
                    path.display()
                )
                .into(),
            )
        })
    }

    /// Writes `lock` to `path` atomically (write to a temp file, then rename) so a crash or
    /// concurrent write can't leave `path` truncated/corrupted.
    fn save_lock(path: &std::path::Path, lock: &super::lockfile::ModuleLock) -> Result<(), ModuleError> {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent).map_err(|e| ModuleError::IOError(e.to_string().into()))?;
        }
        let tmp_path = path.with_extension("lock.tmp");
        std::fs::write(&tmp_path, lock.to_json()).map_err(|e| ModuleError::IOError(e.to_string().into()))?;
        std::fs::rename(&tmp_path, path).map_err(|e| ModuleError::IOError(e.to_string().into()))
    }

    /// System temp dir, not next to `mq.lock`, so no stray sidecar file is left in the
    /// project; named by the absolute path's hash so distinct `mq.lock` paths never collide.
    fn lockfile_flock_path(&self) -> std::path::PathBuf {
        let absolute_path = std::path::absolute(&self.lockfile_path).unwrap_or_else(|_| self.lockfile_path.clone());
        let hash = super::lockfile::compute_hash(&absolute_path.to_string_lossy());
        std::env::temp_dir().join(format!("mq-{hash}.lock"))
    }

    /// Runs `f` while holding an OS-level advisory lock, so concurrent readers/writers of the
    /// same `mq.lock` (e.g. multiple `mq` processes) can't race on a lost update.
    fn with_lockfile_guard<T>(&self, f: impl FnOnce() -> Result<T, ModuleError>) -> Result<T, ModuleError> {
        let flock_file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(self.lockfile_flock_path())
            .map_err(|e| ModuleError::IOError(e.to_string().into()))?;
        flock_file
            .lock()
            .map_err(|e| ModuleError::IOError(e.to_string().into()))?;
        let result = f();
        drop(flock_file);
        result
    }

    /// Loads `mq.lock` into the cache if not already loaded. Call while holding the guard
    /// from [`Self::with_lockfile_guard`].
    fn ensure_lock_loaded(&self) -> Result<(), ModuleError> {
        let mut cache = self.lockfile_cache.lock().unwrap();
        if cache.is_none() {
            *cache = Some(Self::load_lock(&self.lockfile_path)?);
        }
        Ok(())
    }

    /// Checks a freshly-fetched `hash` for `url` against `mq.lock`, recording it if new.
    fn check_lock(&self, url: &str, hash: &str) -> Result<(), ModuleError> {
        if !self.lockfile_enabled {
            return Ok(());
        }
        self.with_lockfile_guard(|| {
            self.ensure_lock_loaded()?;
            let mut cache = self.lockfile_cache.lock().unwrap();
            let lock = cache.as_mut().unwrap();
            match lock.check(url, hash) {
                lockfile::LockCheck::Match => Ok(()),
                lockfile::LockCheck::NewEntry => {
                    lock.insert(url, hash);
                    Self::save_lock(&self.lockfile_path, lock)?;
                    Ok(())
                }
                lockfile::LockCheck::Mismatch { locked } => {
                    // --refresh-modules only clears mutable-ref cache and lock entries, so
                    // a drifted versioned (tagged) module needs the full reset instead.
                    let hint = if super::http_import::is_versioned_url(url) {
                        "re-run with --clear-cache to reset the module cache and the lock file"
                    } else {
                        "re-run with --refresh-modules to update the lock file"
                    };
                    Err(ModuleError::IOError(
                        format!(
                            "content for {url} does not match {} (expected sha256 {locked}, got sha256 {hash}). \
                             The module content may have changed since it was locked. If this is expected, {hint}.",
                            super::lockfile::LOCKFILE_NAME
                        )
                        .into(),
                    ))
                }
            }
        })
    }
}

/// Maximum response body size for a fetched module (1 MiB).
#[cfg(feature = "http-import-ureq")]
const MAX_MODULE_SIZE: u64 = 1024 * 1024;

#[cfg(feature = "http-import-ureq")]
impl HttpFetcher for UreqFetcher {
    fn fetch(&self, url: &str) -> Result<String, ModuleError> {
        if !super::ssrf::is_https(url) {
            return Err(ModuleError::IOError(
                format!("Only HTTPS URLs are allowed: {}", url).into(),
            ));
        }

        let cache_subdir = self.cache_subdir(url);
        let stem = Self::cache_stem(url);
        let cache_file = cache_subdir.join(format!("{}.mq", stem));
        let hash_file = cache_subdir.join(format!("{}.mq.sha256", stem));

        if let Some(cached) = self.try_read_cache(&cache_file, &hash_file)? {
            self.check_lock(url, &cached.hash)?;
            return Ok(cached.content);
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

        if let Some(cached) = self.try_read_cache(&cache_file, &hash_file)? {
            self.check_lock(url, &cached.hash)?;
            return Ok(cached.content);
        }

        let agent = super::ssrf::ssrf_safe_agent(self.timeout, true);

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
        let hash = Self::compute_hash(&content);

        self.check_lock(url, &hash)?;

        std::fs::write(&cache_file, content.as_bytes()).map_err(|e| ModuleError::IOError(e.to_string().into()))?;
        std::fs::write(&hash_file, hash.as_bytes()).map_err(|e| ModuleError::IOError(e.to_string().into()))?;

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

    /// Enables or disables the `mq.lock` integrity check/update. The path is unaffected.
    pub fn set_lockfile_enabled(&mut self, enabled: bool) {
        self.fetcher.set_lockfile_enabled(enabled);
    }

    /// Sets the path used for `mq.lock`.
    pub fn set_lockfile_path(&mut self, path: std::path::PathBuf) {
        self.fetcher.set_lockfile_path(path);
    }

    pub(crate) fn lockfile_path(&self) -> std::path::PathBuf {
        self.fetcher.lockfile_path()
    }

    pub(crate) fn lockfile_enabled(&self) -> bool {
        self.fetcher.lockfile_enabled()
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
    #[case("https://example.invalid/foo.mq", "foo")]
    #[case("local_module", "local_module")]
    #[cfg(feature = "http-import-ureq")]
    fn test_canonical_name(#[case] input: &str, #[case] expected: &str) {
        let resolver = HttpModuleResolver::<UreqFetcher>::default();
        assert_eq!(resolver.canonical_name(input), expected);
    }

    #[rstest]
    #[case(vec!["github.com/alice/myrepo".to_string()], "https://raw.githubusercontent.com/alice/myrepo/HEAD/mod.mq", true)]
    #[case(vec!["github.com/alice/myrepo".to_string()], "https://raw.githubusercontent.com/alice/other/HEAD/mod.mq", false)]
    #[case(vec!["example.invalid".to_string()], "https://example.invalid/foo.mq", true)]
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
    #[case(vec!["example.invalid".to_string()], "github.com/alice/lisp")]
    #[case(vec!["example.invalid".to_string()], "https://other.com/foo.mq")]
    #[case(vec![], "https://example.invalid/foo.mq")]
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
            lockfile_path: dir.path().join("mq.lock"),
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
            lockfile_path: dir.path().join("mq.lock"),
            ..UreqFetcher::default()
        };
        let resolver = HttpModuleResolver::new(vec![], fetcher);
        assert_eq!(resolver.resolve(url).unwrap(), content);
    }

    /// Writes `content` and its matching `.sha256` sidecar into the given cache subdirectory.
    #[cfg(feature = "http-import-ureq")]
    fn seed_cache(cache_dir: &std::path::Path, subdir: &str, url: &str, content: &str) {
        let dir = cache_dir.join(subdir);
        std::fs::create_dir_all(&dir).unwrap();
        let stem = UreqFetcher::cache_stem(url);
        std::fs::write(dir.join(format!("{}.mq", stem)), content.as_bytes()).unwrap();
        std::fs::write(
            dir.join(format!("{}.mq.sha256", stem)),
            UreqFetcher::compute_hash(content).as_bytes(),
        )
        .unwrap();
    }

    #[test]
    #[cfg(feature = "http-import-ureq")]
    fn test_resolve_cache_hit_errors_on_lock_mismatch() {
        let dir = TempDir::new().unwrap();
        let url = "https://raw.githubusercontent.com/harehare/mymod/HEAD/mymod.mq";
        seed_cache(dir.path(), "mutable", url, "def cached(): 42;");

        let lock_path = dir.path().join("mq.lock");
        let mut lock = super::super::lockfile::ModuleLock::default();
        lock.insert(url, "0000000000000000000000000000000000000000000000000000000000000000");
        std::fs::write(&lock_path, lock.to_json()).unwrap();

        let fetcher = UreqFetcher {
            cache_dir: dir.path().to_path_buf(),
            lockfile_path: lock_path,
            ..UreqFetcher::default()
        };
        let resolver = HttpModuleResolver::new(vec![], fetcher);

        let err = resolver.resolve(url).unwrap_err();
        assert!(err.to_string().contains("--refresh-modules"), "message was: {err}");
    }

    #[test]
    #[cfg(feature = "http-import-ureq")]
    fn test_resolve_cache_hit_versioned_lock_mismatch_mentions_clear_cache() {
        let dir = TempDir::new().unwrap();
        let url = "https://raw.githubusercontent.com/harehare/mymod/v0.1.0/mymod.mq";
        seed_cache(dir.path(), "versioned", url, "def pinned(): 1;");

        let lock_path = dir.path().join("mq.lock");
        let mut lock = super::super::lockfile::ModuleLock::default();
        lock.insert(url, "0000000000000000000000000000000000000000000000000000000000000000");
        std::fs::write(&lock_path, lock.to_json()).unwrap();

        let fetcher = UreqFetcher {
            cache_dir: dir.path().to_path_buf(),
            lockfile_path: lock_path,
            ..UreqFetcher::default()
        };
        let resolver = HttpModuleResolver::new(vec![], fetcher);

        let err = resolver.resolve(url).unwrap_err();
        assert!(err.to_string().contains("--clear-cache"), "message was: {err}");
    }

    #[test]
    #[cfg(feature = "http-import-ureq")]
    fn test_resolve_cache_hit_records_new_lock_entry() {
        let dir = TempDir::new().unwrap();
        let url = "https://raw.githubusercontent.com/harehare/mymod/HEAD/mymod.mq";
        let content = "def cached(): 42;";
        seed_cache(dir.path(), "mutable", url, content);

        let lock_path = dir.path().join("mq.lock");
        let fetcher = UreqFetcher {
            cache_dir: dir.path().to_path_buf(),
            lockfile_path: lock_path.clone(),
            ..UreqFetcher::default()
        };
        let resolver = HttpModuleResolver::new(vec![], fetcher);

        // A cache hit in a project with no mq.lock yet must still create the entry, so the
        // project gains lock protection even when the module never touches the network.
        assert_eq!(resolver.resolve(url).unwrap(), content);
        let lock = super::super::lockfile::ModuleLock::parse(&std::fs::read_to_string(&lock_path).unwrap()).unwrap();
        assert_eq!(
            lock.check(url, &UreqFetcher::compute_hash(content)),
            super::super::lockfile::LockCheck::Match
        );
    }

    #[test]
    #[cfg(feature = "http-import-ureq")]
    fn test_resolve_cache_hit_skips_lock_check_when_disabled() {
        let dir = TempDir::new().unwrap();
        let url = "https://raw.githubusercontent.com/harehare/mymod/HEAD/mymod.mq";
        let content = "def cached(): 42;";
        seed_cache(dir.path(), "mutable", url, content);

        let lock_path = dir.path().join("mq.lock");
        let mut lock = super::super::lockfile::ModuleLock::default();
        lock.insert(url, "0000000000000000000000000000000000000000000000000000000000000000");
        std::fs::write(&lock_path, lock.to_json()).unwrap();

        let mut fetcher = UreqFetcher {
            cache_dir: dir.path().to_path_buf(),
            lockfile_path: lock_path,
            ..UreqFetcher::default()
        };
        fetcher.set_lockfile_enabled(false);
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
    #[case(vec!["other.com".to_string()], "https://example.invalid/foo.mq")]
    #[case(vec![], "https://example.invalid/foo.mq")]
    #[case(vec![], "https://raw.githubusercontent.com/alice/mod/HEAD/mod.mq")]
    #[cfg(feature = "http-import-ureq")]
    fn test_resolve_blocked_domain_returns_io_error(#[case] allowed: Vec<String>, #[case] url: &str) {
        let resolver = resolver_with_domains(allowed);
        assert!(matches!(resolver.resolve(url), Err(ModuleError::IOError(_))));
    }

    #[rstest]
    #[case("http://example.invalid/foo.mq")]
    #[case("http://raw.githubusercontent.com/harehare/mod/HEAD/mod.mq")]
    #[cfg(feature = "http-import-ureq")]
    fn test_fetch_rejects_http(#[case] url: &str) {
        let fetcher = UreqFetcher::default();
        assert!(matches!(fetcher.fetch(url), Err(ModuleError::IOError(_))));
    }

    #[test]
    #[cfg(feature = "http-import-ureq")]
    fn test_fetch_rejects_loopback_address() {
        // "localhost" resolves to a loopback address via the OS hosts file
        // (no network access needed), so the SSRF-safe resolver must reject
        // it before any connection is attempted.
        let dir = TempDir::new().unwrap();
        let fetcher = UreqFetcher {
            cache_dir: dir.path().to_path_buf(),
            ..UreqFetcher::default()
        };
        assert!(matches!(
            fetcher.fetch("https://localhost/foo.mq"),
            Err(ModuleError::IOError(_))
        ));
    }

    #[test]
    #[cfg(feature = "http-import-ureq")]
    fn test_fetch_rejects_non_default_domain_with_empty_allowlist() {
        let resolver = resolver_with_domains(vec![]);
        assert!(matches!(
            resolver.resolve("https://example.invalid/foo.mq"),
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
            lockfile_path: dir.path().join("mq.lock"),
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
            lockfile_path: dir.path().join("mq.lock"),
            ..UreqFetcher::default()
        };
        let resolver = HttpModuleResolver::new(vec![], fetcher);
        resolver.clear_all_cache().unwrap();
        assert!(!mutable_dir.exists());
        assert!(!versioned_dir.exists());
    }

    #[test]
    #[cfg(feature = "http-import-ureq")]
    fn test_clear_cache_strips_only_mutable_lock_entries() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("mutable")).unwrap();
        let lock_path = dir.path().join("mq.lock");
        let mutable_url = "https://raw.githubusercontent.com/harehare/lisp/HEAD/lisp.mq";
        let versioned_url = "https://raw.githubusercontent.com/harehare/lisp/v0.1.0/lisp.mq";
        let mut lock = super::super::lockfile::ModuleLock::default();
        lock.insert(mutable_url, "mutable-hash");
        lock.insert(versioned_url, "versioned-hash");
        std::fs::write(&lock_path, lock.to_json()).unwrap();

        let fetcher = UreqFetcher {
            cache_dir: dir.path().to_path_buf(),
            lockfile_path: lock_path.clone(),
            ..UreqFetcher::default()
        };
        let resolver = HttpModuleResolver::new(vec![], fetcher);
        resolver.clear_cache().unwrap();

        let reloaded =
            super::super::lockfile::ModuleLock::parse(&std::fs::read_to_string(&lock_path).unwrap()).unwrap();
        assert_eq!(
            reloaded.check(mutable_url, "mutable-hash"),
            super::super::lockfile::LockCheck::NewEntry
        );
        assert_eq!(
            reloaded.check(versioned_url, "versioned-hash"),
            super::super::lockfile::LockCheck::Match
        );
    }

    #[test]
    #[cfg(feature = "http-import-ureq")]
    fn test_clear_all_cache_deletes_lockfile() {
        let dir = TempDir::new().unwrap();
        let lock_path = dir.path().join("mq.lock");
        std::fs::write(&lock_path, super::super::lockfile::ModuleLock::default().to_json()).unwrap();

        let fetcher = UreqFetcher {
            cache_dir: dir.path().to_path_buf(),
            lockfile_path: lock_path.clone(),
            ..UreqFetcher::default()
        };
        let resolver = HttpModuleResolver::new(vec![], fetcher);
        resolver.clear_all_cache().unwrap();

        assert!(!lock_path.exists());
    }

    #[test]
    #[cfg(feature = "http-import-ureq")]
    fn test_check_lock_records_new_entry() {
        let dir = TempDir::new().unwrap();
        let lock_path = dir.path().join("mq.lock");
        let fetcher = UreqFetcher {
            cache_dir: dir.path().to_path_buf(),
            lockfile_path: lock_path.clone(),
            ..UreqFetcher::default()
        };

        fetcher.check_lock("https://example.invalid/a.mq", "hash-a").unwrap();

        let lock = super::super::lockfile::ModuleLock::parse(&std::fs::read_to_string(&lock_path).unwrap()).unwrap();
        assert_eq!(
            lock.check("https://example.invalid/a.mq", "hash-a"),
            super::super::lockfile::LockCheck::Match
        );
    }

    #[test]
    #[cfg(feature = "http-import-ureq")]
    fn test_check_lock_creates_missing_parent_directories() {
        let dir = TempDir::new().unwrap();
        let lock_path = dir.path().join("nested").join("deeper").join("mq.lock");
        let fetcher = UreqFetcher {
            cache_dir: dir.path().to_path_buf(),
            lockfile_path: lock_path.clone(),
            ..UreqFetcher::default()
        };

        fetcher.check_lock("https://example.invalid/a.mq", "hash-a").unwrap();

        assert!(lock_path.exists());
    }

    #[test]
    #[cfg(feature = "http-import-ureq")]
    fn test_check_lock_passes_on_matching_hash() {
        let dir = TempDir::new().unwrap();
        let lock_path = dir.path().join("mq.lock");
        let mut lock = super::super::lockfile::ModuleLock::default();
        lock.insert("https://example.invalid/a.mq", "hash-a");
        std::fs::write(&lock_path, lock.to_json()).unwrap();

        let fetcher = UreqFetcher {
            cache_dir: dir.path().to_path_buf(),
            lockfile_path: lock_path,
            ..UreqFetcher::default()
        };

        assert!(fetcher.check_lock("https://example.invalid/a.mq", "hash-a").is_ok());
    }

    #[test]
    #[cfg(feature = "http-import-ureq")]
    fn test_check_lock_errors_on_mismatched_hash() {
        let dir = TempDir::new().unwrap();
        let lock_path = dir.path().join("mq.lock");
        let mut lock = super::super::lockfile::ModuleLock::default();
        lock.insert("https://example.invalid/a.mq", "hash-a");
        std::fs::write(&lock_path, lock.to_json()).unwrap();

        let fetcher = UreqFetcher {
            cache_dir: dir.path().to_path_buf(),
            lockfile_path: lock_path,
            ..UreqFetcher::default()
        };

        assert!(matches!(
            fetcher.check_lock("https://example.invalid/a.mq", "hash-b"),
            Err(ModuleError::IOError(_))
        ));
    }

    #[test]
    #[cfg(feature = "http-import-ureq")]
    fn test_check_lock_disabled_via_set_lockfile_enabled() {
        let mut fetcher = UreqFetcher::default();
        fetcher.set_lockfile_enabled(false);
        assert!(fetcher.check_lock("https://example.invalid/a.mq", "any-hash").is_ok());
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

    #[test]
    #[cfg(feature = "http-import-ureq")]
    fn test_check_lock_concurrent_new_entries_do_not_lose_updates() {
        let dir = TempDir::new().unwrap();
        let lock_path = dir.path().join("mq.lock");

        // Simulates separate UreqFetcher instances (e.g. one per rayon worker in mq-run's
        // parallel file processing) racing to lock two different new URLs at once. Without
        // the flock guard in `with_lockfile_guard`, whichever save_lock() ran last would
        // silently drop the other thread's entry.
        let handles: Vec<_> = (0..8)
            .map(|i| {
                let lock_path = lock_path.clone();
                let cache_dir = dir.path().to_path_buf();
                std::thread::spawn(move || {
                    let fetcher = UreqFetcher {
                        cache_dir,
                        lockfile_path: lock_path,
                        ..UreqFetcher::default()
                    };
                    fetcher
                        .check_lock(&format!("https://example.invalid/{i}.mq"), &format!("hash-{i}"))
                        .unwrap();
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }

        let lock = super::super::lockfile::ModuleLock::parse(&std::fs::read_to_string(&lock_path).unwrap()).unwrap();
        for i in 0..8 {
            assert_eq!(
                lock.check(&format!("https://example.invalid/{i}.mq"), &format!("hash-{i}")),
                super::super::lockfile::LockCheck::Match,
                "entry for URL {i} was lost to a concurrent write"
            );
        }
    }

    #[test]
    #[cfg(feature = "http-import-ureq")]
    fn test_save_lock_leaves_no_leftover_temp_file() {
        let dir = TempDir::new().unwrap();
        let lock_path = dir.path().join("mq.lock");
        let mut lock = super::super::lockfile::ModuleLock::default();
        lock.insert("https://example.invalid/a.mq", "hash-a");

        UreqFetcher::save_lock(&lock_path, &lock).unwrap();

        assert!(lock_path.exists());
        assert!(!lock_path.with_extension("lock.tmp").exists());
    }

    #[test]
    #[cfg(feature = "http-import-ureq")]
    fn test_load_lock_parse_error_mentions_no_lockfile_recovery() {
        let dir = TempDir::new().unwrap();
        let lock_path = dir.path().join("mq.lock");
        std::fs::write(&lock_path, "not json").unwrap();

        let fetcher = UreqFetcher {
            cache_dir: dir.path().to_path_buf(),
            lockfile_path: lock_path,
            ..UreqFetcher::default()
        };

        let err = fetcher
            .check_lock("https://example.invalid/a.mq", "hash-a")
            .unwrap_err();
        let message = err.to_string();
        assert!(message.contains("--no-lockfile"), "message was: {message}");
    }

    #[test]
    #[cfg(feature = "http-import-ureq")]
    fn test_check_lock_reuses_in_memory_cache_across_calls() {
        let dir = TempDir::new().unwrap();
        let lock_path = dir.path().join("mq.lock");
        let fetcher = UreqFetcher {
            cache_dir: dir.path().to_path_buf(),
            lockfile_path: lock_path.clone(),
            ..UreqFetcher::default()
        };

        fetcher.check_lock("https://example.invalid/a.mq", "hash-a").unwrap();
        // Removing the on-disk file doesn't affect a second call for a URL already known
        // in-memory, proving the second check didn't need to re-read the file.
        std::fs::remove_file(&lock_path).unwrap();
        assert!(fetcher.check_lock("https://example.invalid/a.mq", "hash-a").is_ok());
    }

    #[test]
    #[cfg(feature = "http-import-ureq")]
    fn test_set_lockfile_path_clears_in_memory_cache() {
        let dir = TempDir::new().unwrap();
        let mut fetcher = UreqFetcher {
            cache_dir: dir.path().to_path_buf(),
            lockfile_path: dir.path().join("a.lock"),
            ..UreqFetcher::default()
        };
        fetcher.check_lock("https://example.invalid/a.mq", "hash-a").unwrap();

        fetcher.set_lockfile_path(dir.path().join("b.lock"));

        // A URL that was only ever recorded under the old path is unknown under the new one.
        fetcher.check_lock("https://example.invalid/a.mq", "hash-b").unwrap();
        let lock =
            super::super::lockfile::ModuleLock::parse(&std::fs::read_to_string(dir.path().join("b.lock")).unwrap())
                .unwrap();
        assert_eq!(
            lock.check("https://example.invalid/a.mq", "hash-b"),
            super::super::lockfile::LockCheck::Match
        );
    }
}
