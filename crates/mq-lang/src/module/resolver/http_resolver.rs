use std::{borrow::Cow, fs, path::PathBuf, time::Duration};

use crate::{ModuleError, ModuleResolver};

/// Resolves mq modules from HTTP/HTTPS URLs with optional domain allowlisting and local disk caching.
///
/// # Caching
///
/// Fetched modules are stored in `{system_cache_dir}/mq/` as `{md5(url)}.mq` files.
///
/// - **Versioned URLs** (tag ≠ `HEAD`/`main`/`master`): cached indefinitely — tag content is immutable.
/// - **Mutable refs** (`HEAD`, `main`, `master`, or no version): cached on first fetch and reused
///   on subsequent resolves. Call [`HttpModuleResolver::clear_cache`] (e.g. via `--refresh-modules`)
///   to discard all cached entries and force a re-fetch on the next resolve.
///
/// # GitHub shorthand
///
/// In addition to plain http(s) URLs, the resolver accepts a shorthand form that omits
/// the `https://` scheme and maps GitHub paths to `raw.githubusercontent.com`:
///
/// ```text
/// github.com/{owner}/{path}[@{version}]
/// ```
///
/// See [`HttpModuleResolver::github_to_raw_url`] for details.
#[derive(Debug, Clone)]
pub struct HttpModuleResolver {
    pub(crate) allowed_remote_domains: Vec<String>,
    pub(crate) client: reqwest::blocking::Client,
    pub(crate) timeout: Duration,
    cache_dir: PathBuf,
}

impl Default for HttpModuleResolver {
    fn default() -> Self {
        Self {
            allowed_remote_domains: Vec::new(),
            client: reqwest::blocking::Client::new(),
            timeout: Duration::from_secs(10),
            cache_dir: dirs::cache_dir().unwrap_or_default().join("mq"),
        }
    }
}

impl ModuleResolver for HttpModuleResolver {
    fn resolve(&self, module_name: &str) -> Result<String, ModuleError> {
        let url = self.to_fetch_url(module_name)?;
        let cache_file = self.cache_dir.join(self.cache_file_name(&url));

        if cache_file.exists() {
            return fs::read_to_string(&cache_file).map_err(|e| ModuleError::IOError(e.to_string().into()));
        }

        let content = self.fetch_url(&url)?;
        fs::create_dir_all(&self.cache_dir).map_err(|e| ModuleError::IOError(e.to_string().into()))?;
        fs::write(&cache_file, content.as_bytes()).map_err(|e| ModuleError::IOError(e.to_string().into()))?;
        Ok(content)
    }

    fn get_path(&self, module_name: &str) -> Result<String, ModuleError> {
        self.to_fetch_url(module_name)
    }

    fn search_paths(&self) -> Vec<PathBuf> {
        Vec::new()
    }

    fn set_search_paths(&mut self, _paths: Vec<PathBuf>) {}
}

impl HttpModuleResolver {
    /// Creates a new resolver with the given domain allowlist and request timeout.
    ///
    /// An empty `allowed_remote_domains` list permits all http(s) URLs.
    pub fn new(allowed_remote_domains: Vec<String>, timeout: Duration) -> Self {
        let cache_dir = dirs::cache_dir().unwrap_or_default().join("mq");
        Self {
            allowed_remote_domains,
            client: reqwest::blocking::Client::new(),
            timeout,
            cache_dir,
        }
    }

    /// Updates the request timeout.
    pub fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = timeout;
    }

    /// Adds a domain prefix to the allowlist (e.g. `"example.com/myrepo"`).
    pub fn add_allowed_domain(&mut self, domain: String) {
        self.allowed_remote_domains.push(domain);
    }

    /// Returns `true` if `url` has an `http://` or `https://` scheme.
    pub fn is_remote_url(url: &str) -> bool {
        url.starts_with("http://") || url.starts_with("https://")
    }

    /// Returns `true` if `input` is a GitHub shorthand or full GitHub URL.
    ///
    /// Recognized forms (with or without `https://` prefix):
    /// - `github.com/{owner}/{path}[@{version}]`
    pub fn is_github_url(input: &str) -> bool {
        let s = input
            .strip_prefix("https://")
            .or_else(|| input.strip_prefix("http://"))
            .unwrap_or(input);
        s.starts_with("github.com/")
    }

    /// Converts a GitHub shorthand into a `raw.githubusercontent.com` fetch URL.
    ///
    /// # Format
    /// `[https://]github.com/{owner}/{path}[@{version}]`
    ///
    /// Where `{path}` is one of:
    /// - `{repo}` → fetches `{repo}.mq` from the repo root at HEAD
    /// - `{file.mq}` → fetches `{file.mq}` from the repo whose name equals the file stem
    /// - `{repo}/{subpath}` → fetches `{subpath}` from the given repo
    ///
    /// `{version}` (e.g. `v0.1.0`) selects a specific git tag; omitting it uses `HEAD`.
    ///
    /// # Examples
    /// | Input | Resolved URL |
    /// |---|---|
    /// | `github.com/alice/mymod` | `…/alice/mymod/HEAD/mymod.mq` |
    /// | `github.com/alice/mymod.mq@v1.0` | `…/alice/mymod/v1.0/mymod.mq` |
    /// | `github.com/alice/repo/lib/util.mq@v2.0` | `…/alice/repo/v2.0/lib/util.mq` |
    pub fn github_to_raw_url(input: &str) -> Option<String> {
        let without_scheme = input
            .strip_prefix("https://")
            .or_else(|| input.strip_prefix("http://"))
            .unwrap_or(input);

        let rest = without_scheme.strip_prefix("github.com/")?;

        let (path_part, version) = match rest.rfind('@') {
            Some(pos) => (&rest[..pos], &rest[pos + 1..]),
            None => (rest, "HEAD"),
        };

        let components: Vec<&str> = path_part.splitn(3, '/').collect();

        let (owner, repo, file) = match components.as_slice() {
            [owner, name] => {
                let (repo, file) = if let Some(stem) = name.strip_suffix(".mq") {
                    (stem, *name)
                } else {
                    (*name, *name)
                };
                let file = if file.ends_with(".mq") {
                    file.to_string()
                } else {
                    format!("{}.mq", file)
                };
                (owner.to_string(), repo.to_string(), file)
            }
            [owner, repo, subpath] => (owner.to_string(), repo.to_string(), subpath.to_string()),
            _ => return None,
        };

        Some(format!(
            "https://raw.githubusercontent.com/{}/{}/{}/{}",
            owner, repo, version, file
        ))
    }

    /// Returns `true` if `url` is pinned to an immutable git tag (not HEAD/main/master).
    pub fn is_versioned_url(url: &str) -> bool {
        if let Some(path) = url.strip_prefix("https://raw.githubusercontent.com/") {
            let parts: Vec<&str> = path.splitn(4, '/').collect();
            if parts.len() >= 3 {
                let git_ref = parts[2];
                return git_ref != "HEAD" && git_ref != "main" && git_ref != "master";
            }
        }
        true
    }

    /// Returns `true` if `url`'s host/path matches at least one entry in the allowlist.
    ///
    /// An empty allowlist means all URLs are permitted.
    pub fn is_allowed_domain(&self, url: &str) -> bool {
        if self.allowed_remote_domains.is_empty() {
            return true;
        }
        let url_without_scheme = url
            .strip_prefix("https://")
            .or_else(|| url.strip_prefix("http://"))
            .unwrap_or(url);
        self.allowed_remote_domains
            .iter()
            .any(|domain| url_without_scheme.starts_with(domain.as_str()))
    }

    /// Removes all locally-cached module files.
    ///
    /// Call this once before processing to force a re-fetch of all cached modules
    /// on the next resolve (e.g. when `--refresh-modules` is passed on the CLI).
    pub fn clear_cache(&self) -> Result<(), ModuleError> {
        if self.cache_dir.exists() {
            fs::remove_dir_all(&self.cache_dir).map_err(|e| ModuleError::IOError(e.to_string().into()))?;
        }
        Ok(())
    }

    /// Fetches module source from the given URL without consulting the cache.
    pub fn fetch_url(&self, url: &str) -> Result<String, ModuleError> {
        if !Self::is_remote_url(url) {
            return Err(ModuleError::NotFound(Cow::Owned(url.to_string())));
        }
        if !self.is_allowed_domain(url) {
            return Err(ModuleError::IOError(format!("Domain not allowed: {}", url).into()));
        }

        let response = self
            .client
            .get(url)
            .timeout(self.timeout)
            .send()
            .map_err(|e| ModuleError::IOError(e.to_string().into()))?;

        if !response.status().is_success() {
            return Err(ModuleError::IOError(
                format!("Failed to fetch module: {} (status: {})", url, response.status()).into(),
            ));
        }

        response.text().map_err(|e| ModuleError::IOError(e.to_string().into()))
    }

    fn to_fetch_url(&self, module_name: &str) -> Result<String, ModuleError> {
        if Self::is_github_url(module_name) && !Self::is_remote_url(module_name) {
            let url = Self::github_to_raw_url(module_name)
                .ok_or_else(|| ModuleError::IOError(format!("Invalid GitHub URL: {}", module_name).into()))?;
            if !self.is_allowed_domain(&url) {
                return Err(ModuleError::IOError(format!("Domain not allowed: {}", url).into()));
            }
            return Ok(url);
        }

        if Self::is_github_url(module_name)
            && let Some(raw_url) = Self::github_to_raw_url(module_name)
        {
            if !self.is_allowed_domain(&raw_url) {
                return Err(ModuleError::IOError(format!("Domain not allowed: {}", raw_url).into()));
            }
            return Ok(raw_url);
        }

        if Self::is_remote_url(module_name) {
            if !self.is_allowed_domain(module_name) {
                return Err(ModuleError::IOError(
                    format!("Domain not allowed: {}", module_name).into(),
                ));
            }
            return Ok(module_name.to_string());
        }

        Err(ModuleError::NotFound(Cow::Owned(module_name.to_string())))
    }

    fn cache_file_name(&self, url: &str) -> String {
        let hash = md5::compute(url);
        format!("{:x}.mq", hash)
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;
    use tempfile::TempDir;

    use super::*;

    fn resolver_with_domains(domains: Vec<String>) -> HttpModuleResolver {
        HttpModuleResolver {
            allowed_remote_domains: domains,
            client: reqwest::blocking::Client::new(),
            timeout: Duration::from_secs(10),
            cache_dir: PathBuf::from("/tmp/mq-test-cache"),
        }
    }

    #[rstest]
    #[case("https://example.com/foo.mq", true)]
    #[case("http://example.com/foo.mq", true)]
    #[case("ftp://example.com/foo.mq", false)]
    #[case("example.com/foo.mq", false)]
    #[case("csv", false)]
    #[case("", false)]
    fn test_is_remote_url(#[case] url: &str, #[case] expected: bool) {
        assert_eq!(HttpModuleResolver::is_remote_url(url), expected);
    }

    #[rstest]
    #[case("github.com/owner/repo", true)]
    #[case("github.com/owner/repo.mq@v1.0", true)]
    #[case("https://github.com/owner/repo", true)]
    #[case("http://github.com/owner/repo", true)]
    #[case("https://example.com/foo.mq", false)]
    #[case("example.com/foo.mq", false)]
    #[case("csv", false)]
    fn test_is_github_url(#[case] input: &str, #[case] expected: bool) {
        assert_eq!(HttpModuleResolver::is_github_url(input), expected);
    }

    #[rstest]
    #[case(
        "github.com/harehare/lisp",
        "https://raw.githubusercontent.com/harehare/lisp/HEAD/lisp.mq"
    )]
    #[case(
        "github.com/harehare/lisp.mq",
        "https://raw.githubusercontent.com/harehare/lisp/HEAD/lisp.mq"
    )]
    #[case(
        "github.com/harehare/lisp.mq@v0.1.0",
        "https://raw.githubusercontent.com/harehare/lisp/v0.1.0/lisp.mq"
    )]
    #[case(
        "github.com/harehare/lisp@v0.1.0",
        "https://raw.githubusercontent.com/harehare/lisp/v0.1.0/lisp.mq"
    )]
    #[case(
        "github.com/harehare/repo/lib/utils.mq@v2.0",
        "https://raw.githubusercontent.com/harehare/repo/v2.0/lib/utils.mq"
    )]
    #[case(
        "https://github.com/harehare/lisp.mq@v0.1.0",
        "https://raw.githubusercontent.com/harehare/lisp/v0.1.0/lisp.mq"
    )]
    fn test_github_to_raw_url(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(HttpModuleResolver::github_to_raw_url(input).unwrap(), expected);
    }

    #[rstest]
    #[case("example.com/foo")]
    #[case("notgithub.com/owner/repo")]
    fn test_github_to_raw_url_returns_none_for_non_github(#[case] input: &str) {
        assert!(HttpModuleResolver::github_to_raw_url(input).is_none());
    }

    #[rstest]
    #[case("https://raw.githubusercontent.com/owner/repo/v0.1.0/file.mq", true)]
    #[case("https://raw.githubusercontent.com/owner/repo/1.2.3/file.mq", true)]
    #[case("https://raw.githubusercontent.com/owner/repo/HEAD/file.mq", false)]
    #[case("https://raw.githubusercontent.com/owner/repo/main/file.mq", false)]
    #[case("https://raw.githubusercontent.com/owner/repo/master/file.mq", false)]
    #[case("https://example.com/foo.mq", true)]
    fn test_is_versioned_url(#[case] url: &str, #[case] expected: bool) {
        assert_eq!(HttpModuleResolver::is_versioned_url(url), expected);
    }

    #[rstest]
    #[case(vec![], "https://example.com/foo.mq", true)]
    #[case(vec![], "http://anything.org/bar.mq", true)]
    #[case(vec!["example.com".to_string()], "https://example.com/foo.mq", true)]
    #[case(vec!["example.com/repo".to_string()], "https://example.com/repo/foo.mq", true)]
    #[case(vec!["example.com".to_string()], "https://other.com/foo.mq", false)]
    #[case(vec!["example.com".to_string()], "https://notexample.com/foo.mq", false)]
    #[case(vec!["example.com".to_string()], "http://example.com/foo.mq", true)]
    fn test_is_allowed_domain(#[case] allowed_domains: Vec<String>, #[case] url: &str, #[case] expected: bool) {
        let resolver = resolver_with_domains(allowed_domains);
        assert_eq!(resolver.is_allowed_domain(url), expected);
    }

    #[rstest]
    #[case(
        "github.com/harehare/lisp.mq@v0.1.0",
        "https://raw.githubusercontent.com/harehare/lisp/v0.1.0/lisp.mq"
    )]
    #[case(
        "github.com/harehare/lisp",
        "https://raw.githubusercontent.com/harehare/lisp/HEAD/lisp.mq"
    )]
    #[case("https://example.com/foo.mq", "https://example.com/foo.mq")]
    fn test_to_fetch_url_with_empty_allowlist(#[case] input: &str, #[case] expected: &str) {
        let resolver = resolver_with_domains(vec![]);
        assert_eq!(resolver.to_fetch_url(input).unwrap(), expected);
    }

    #[test]
    fn test_cache_valid_when_file_exists() {
        let dir = TempDir::new().unwrap();
        let cache_file = dir.path().join("cached.mq");
        fs::write(&cache_file, b"content").unwrap();

        let resolver = HttpModuleResolver {
            allowed_remote_domains: vec![],
            client: reqwest::blocking::Client::new(),
            timeout: Duration::from_secs(10),
            cache_dir: dir.path().to_path_buf(),
        };

        assert!(cache_file.exists());
        // Any cached file (versioned or mutable-ref) is valid as long as it exists
        assert!(resolver.cache_dir.join("cached.mq").exists());
    }

    #[test]
    fn test_clear_cache_removes_files() {
        let dir = TempDir::new().unwrap();
        let cache_file = dir.path().join("abc123.mq");
        fs::write(&cache_file, b"cached content").unwrap();

        let resolver = HttpModuleResolver {
            allowed_remote_domains: vec![],
            client: reqwest::blocking::Client::new(),
            timeout: Duration::from_secs(10),
            cache_dir: dir.path().to_path_buf(),
        };

        assert!(cache_file.exists());
        resolver.clear_cache().unwrap();
        assert!(!dir.path().exists());
    }

    #[test]
    fn test_clear_cache_noop_when_dir_missing() {
        let dir = TempDir::new().unwrap();
        let nonexistent = dir.path().join("nonexistent");

        let resolver = HttpModuleResolver {
            allowed_remote_domains: vec![],
            client: reqwest::blocking::Client::new(),
            timeout: Duration::from_secs(10),
            cache_dir: nonexistent,
        };

        assert!(resolver.clear_cache().is_ok());
    }

    #[rstest]
    #[case("not_a_url")]
    #[case("local/module")]
    #[case("csv")]
    #[case("")]
    fn test_resolve_non_url_returns_not_found(#[case] module_name: &str) {
        let resolver = HttpModuleResolver::default();
        assert!(matches!(resolver.resolve(module_name), Err(ModuleError::NotFound(_))));
    }

    #[rstest]
    #[case("not_a_url")]
    #[case("csv")]
    fn test_get_path_non_url_returns_not_found(#[case] module_name: &str) {
        let resolver = HttpModuleResolver::default();
        assert!(matches!(resolver.get_path(module_name), Err(ModuleError::NotFound(_))));
    }

    #[rstest]
    #[case(vec!["other.com".to_string()], "https://example.com/foo.mq")]
    #[case(vec!["example.com/private".to_string()], "https://example.com/public/foo.mq")]
    fn test_resolve_blocked_domain_returns_io_error(#[case] allowed: Vec<String>, #[case] url: &str) {
        let resolver = resolver_with_domains(allowed);
        assert!(matches!(resolver.resolve(url), Err(ModuleError::IOError(_))));
    }

    #[test]
    fn test_search_paths_empty() {
        assert!(HttpModuleResolver::default().search_paths().is_empty());
    }

    #[test]
    fn test_add_allowed_domain() {
        let mut resolver = HttpModuleResolver::default();
        resolver.add_allowed_domain("example.com".to_string());
        assert_eq!(resolver.allowed_remote_domains, vec!["example.com"]);
    }

    #[test]
    fn test_set_timeout() {
        let mut resolver = HttpModuleResolver::default();
        resolver.set_timeout(Duration::from_secs(30));
        assert_eq!(resolver.timeout, Duration::from_secs(30));
    }

    #[test]
    fn test_new_applies_parameters() {
        let domains = vec!["example.com".to_string()];
        let timeout = Duration::from_secs(5);
        let resolver = HttpModuleResolver::new(domains.clone(), timeout);
        assert_eq!(resolver.allowed_remote_domains, domains);
        assert_eq!(resolver.timeout, timeout);
    }
}
