use std::{borrow::Cow, fs, path::PathBuf, time::Duration};

use crate::{ModuleError, ModuleResolver};

/// Resolves mq modules from HTTP/HTTPS URLs with optional domain allowlisting and local disk caching.
///
/// # Caching
///
/// Fetched modules are stored under `{system_cache_dir}/mq/` in one of two subdirectories:
///
/// - `versioned/` — URLs resolved to a specific tag (e.g. `@v0.1.0`); never cleared by
///   [`HttpModuleResolver::clear_cache`].
/// - `mutable/` — URLs resolved to `HEAD`, `main`, `master`, or any non-GitHub HTTP URL;
///   cleared by [`HttpModuleResolver::clear_cache`] (i.e. `--refresh-modules`).
///
/// Files are named `{md5(url)}.mq` within their subdirectory.
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
    pub(crate) timeout: Duration,
    cache_dir: PathBuf,
}

impl Default for HttpModuleResolver {
    fn default() -> Self {
        Self {
            allowed_remote_domains: Vec::new(),
            timeout: Duration::from_secs(10),
            cache_dir: dirs::cache_dir().unwrap_or_default().join("mq"),
        }
    }
}

impl ModuleResolver for HttpModuleResolver {
    fn resolve(&self, module_name: &str) -> Result<String, ModuleError> {
        let url = self.to_fetch_url(module_name)?;
        let cache_subdir = self.cache_subdir(&url);
        let cache_file = cache_subdir.join(self.cache_file_name(&url));

        if cache_file.exists() {
            return fs::read_to_string(&cache_file).map_err(|e| ModuleError::IOError(e.to_string().into()));
        }

        let content = self.fetch_url(&url)?;
        fs::create_dir_all(&cache_subdir).map_err(|e| ModuleError::IOError(e.to_string().into()))?;
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
            timeout,
            cache_dir,
        }
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

    /// Returns `true` if `url`'s host/path matches at least one entry in the allowlist.
    ///
    /// An empty allowlist means all URLs are permitted.
    ///
    /// The match requires that after the allowed domain (or domain/path prefix), the next
    /// character is `/`, `?`, `#`, `:` (port), or the string ends — preventing a domain like
    /// `example.com.evil.com` from bypassing an `example.com` allowlist entry.
    pub fn is_allowed_domain(&self, url: &str) -> bool {
        if self.allowed_remote_domains.is_empty() {
            return true;
        }
        let url_without_scheme = url
            .strip_prefix("https://")
            .or_else(|| url.strip_prefix("http://"))
            .unwrap_or(url);
        self.allowed_remote_domains.iter().any(|domain| {
            let rest = match url_without_scheme.strip_prefix(domain.as_str()) {
                Some(r) => r,
                None => return false,
            };
            rest.is_empty() || rest.starts_with('/') || rest.starts_with('?') || rest.starts_with('#') || rest.starts_with(':')
        })
    }

    /// Removes only mutable-ref cached modules (HEAD/branch/non-versioned URLs).
    ///
    /// Versioned (tagged) modules in `{cache_dir}/versioned/` are preserved.
    /// Call this when `--refresh-modules` is passed to force a re-fetch of HEAD/branch imports.
    pub fn clear_cache(&self) -> Result<(), ModuleError> {
        let mutable_dir = self.cache_dir.join("mutable");
        if mutable_dir.exists() {
            fs::remove_dir_all(&mutable_dir).map_err(|e| ModuleError::IOError(e.to_string().into()))?;
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

        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(self.timeout))
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

        response
            .body_mut()
            .read_to_string()
            .map_err(|e| ModuleError::IOError(e.to_string().into()))
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

    /// Returns `true` if `url` is pinned to a specific immutable version tag.
    ///
    /// For `raw.githubusercontent.com` URLs the ref segment (the third path component after
    /// `{owner}/{repo}/`) is checked: `HEAD`, `main`, and `master` are mutable; everything
    /// else (e.g. `v0.1.0`) is treated as versioned/immutable.
    ///
    /// All non-GitHub HTTP URLs are considered mutable.
    pub fn is_versioned_url(url: &str) -> bool {
        const MUTABLE_REFS: &[&str] = &["HEAD", "main", "master"];
        let path = url
            .strip_prefix("https://raw.githubusercontent.com/")
            .or_else(|| url.strip_prefix("http://raw.githubusercontent.com/"));
        match path {
            Some(rest) => {
                // layout: {owner}/{repo}/{ref}/{file}
                let ref_segment = rest.splitn(4, '/').nth(2).unwrap_or("HEAD");
                !MUTABLE_REFS.contains(&ref_segment)
            }
            None => false,
        }
    }

    /// Returns the cache subdirectory for `url`:
    /// `{cache_dir}/versioned/` for pinned tags, `{cache_dir}/mutable/` otherwise.
    fn cache_subdir(&self, url: &str) -> PathBuf {
        if Self::is_versioned_url(url) {
            self.cache_dir.join("versioned")
        } else {
            self.cache_dir.join("mutable")
        }
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
    // only owner, no repo component
    #[case("github.com/owner")]
    fn test_github_to_raw_url_returns_none_for_non_github(#[case] input: &str) {
        assert!(HttpModuleResolver::github_to_raw_url(input).is_none());
    }

    #[rstest]
    // explicit mutable-ref version tags expand correctly
    #[case(
        "github.com/harehare/lisp@HEAD",
        "https://raw.githubusercontent.com/harehare/lisp/HEAD/lisp.mq"
    )]
    #[case(
        "github.com/harehare/lisp@main",
        "https://raw.githubusercontent.com/harehare/lisp/main/lisp.mq"
    )]
    #[case(
        "github.com/harehare/lisp@master",
        "https://raw.githubusercontent.com/harehare/lisp/master/lisp.mq"
    )]
    fn test_github_to_raw_url_explicit_mutable_refs(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(HttpModuleResolver::github_to_raw_url(input).unwrap(), expected);
    }

    #[rstest]
    #[case(vec![], "https://example.com/foo.mq", true)]
    #[case(vec![], "http://anything.org/bar.mq", true)]
    #[case(vec!["example.com".to_string()], "https://example.com/foo.mq", true)]
    #[case(vec!["example.com".to_string()], "https://example.com", true)]
    #[case(vec!["example.com".to_string()], "https://example.com:8080/foo.mq", true)]
    #[case(vec!["example.com/repo".to_string()], "https://example.com/repo/foo.mq", true)]
    #[case(vec!["example.com".to_string()], "https://other.com/foo.mq", false)]
    #[case(vec!["example.com".to_string()], "https://notexample.com/foo.mq", false)]
    #[case(vec!["example.com".to_string()], "http://example.com/foo.mq", true)]
    // prefix-bypass: example.com.evil.com must NOT match allowlist entry "example.com"
    #[case(vec!["example.com".to_string()], "https://example.com.evil.com/foo.mq", false)]
    #[case(vec!["example.com".to_string()], "https://example.com.evil.com", false)]
    #[case(vec!["example".to_string()], "https://example.com/foo.mq", false)]
    // multiple allowlist entries: second entry matches
    #[case(vec!["other.com".to_string(), "example.com".to_string()], "https://example.com/foo.mq", true)]
    // multiple allowlist entries: none match
    #[case(vec!["other.com".to_string(), "another.org".to_string()], "https://example.com/foo.mq", false)]
    // URL with query string
    #[case(vec!["example.com".to_string()], "https://example.com/foo.mq?v=1", true)]
    // URL with fragment
    #[case(vec!["example.com".to_string()], "https://example.com/foo.mq#section", true)]
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
    // https:// GitHub URL is also expanded to raw.githubusercontent.com
    #[case(
        "https://github.com/harehare/lisp@v0.1.0",
        "https://raw.githubusercontent.com/harehare/lisp/v0.1.0/lisp.mq"
    )]
    fn test_to_fetch_url_with_empty_allowlist(#[case] input: &str, #[case] expected: &str) {
        let resolver = resolver_with_domains(vec![]);
        assert_eq!(resolver.to_fetch_url(input).unwrap(), expected);
    }

    #[rstest]
    // GitHub shorthand blocked by allowlist
    #[case(vec!["example.com".to_string()], "github.com/harehare/lisp")]
    // plain HTTPS URL blocked by allowlist
    #[case(vec!["example.com".to_string()], "https://other.com/foo.mq")]
    fn test_to_fetch_url_blocked_by_allowlist(#[case] allowed: Vec<String>, #[case] input: &str) {
        let resolver = resolver_with_domains(allowed);
        assert!(matches!(resolver.to_fetch_url(input), Err(ModuleError::IOError(_))));
    }

    #[rstest]
    // non-URL, non-GitHub local name
    #[case("local_module")]
    fn test_to_fetch_url_returns_not_found(#[case] input: &str) {
        let resolver = resolver_with_domains(vec![]);
        assert!(matches!(resolver.to_fetch_url(input), Err(ModuleError::NotFound(_))));
    }

    #[test]
    fn test_to_fetch_url_invalid_github_shorthand_returns_io_error() {
        // "github.com/owner" has no repo component so github_to_raw_url returns None
        let resolver = resolver_with_domains(vec![]);
        assert!(matches!(
            resolver.to_fetch_url("github.com/owner"),
            Err(ModuleError::IOError(_))
        ));
    }

    #[rstest]
    // versioned: tag that is not HEAD/main/master
    #[case("https://raw.githubusercontent.com/alice/mymod/v0.1.0/mymod.mq", true)]
    #[case("https://raw.githubusercontent.com/alice/mymod/v2.0/lib/util.mq", true)]
    #[case("https://raw.githubusercontent.com/alice/mymod/release-1.0/mymod.mq", true)]
    // mutable: HEAD/main/master
    #[case("https://raw.githubusercontent.com/alice/mymod/HEAD/mymod.mq", false)]
    #[case("https://raw.githubusercontent.com/alice/mymod/main/mymod.mq", false)]
    #[case("https://raw.githubusercontent.com/alice/mymod/master/mymod.mq", false)]
    // http:// scheme variant of raw.githubusercontent.com
    #[case("http://raw.githubusercontent.com/alice/mymod/v0.1.0/mymod.mq", true)]
    #[case("http://raw.githubusercontent.com/alice/mymod/HEAD/mymod.mq", false)]
    // non-GitHub URLs are always mutable
    #[case("https://example.com/foo.mq", false)]
    #[case("http://example.com/foo.mq", false)]
    // URL with insufficient path segments defaults to mutable
    #[case("https://raw.githubusercontent.com/alice/mymod", false)]
    fn test_is_versioned_url(#[case] url: &str, #[case] expected: bool) {
        assert_eq!(HttpModuleResolver::is_versioned_url(url), expected);
    }

    #[test]
    fn test_cache_subdir_versioned() {
        let dir = TempDir::new().unwrap();
        let resolver = HttpModuleResolver {
            allowed_remote_domains: vec![],
            timeout: Duration::from_secs(10),
            cache_dir: dir.path().to_path_buf(),
        };
        let subdir = resolver.cache_subdir("https://raw.githubusercontent.com/alice/mymod/v0.1.0/mymod.mq");
        assert_eq!(subdir, dir.path().join("versioned"));
    }

    #[test]
    fn test_cache_subdir_mutable() {
        let dir = TempDir::new().unwrap();
        let resolver = HttpModuleResolver {
            allowed_remote_domains: vec![],
            timeout: Duration::from_secs(10),
            cache_dir: dir.path().to_path_buf(),
        };
        let subdir = resolver.cache_subdir("https://raw.githubusercontent.com/alice/mymod/HEAD/mymod.mq");
        assert_eq!(subdir, dir.path().join("mutable"));
    }

    #[test]
    fn test_cache_valid_when_file_exists() {
        let dir = TempDir::new().unwrap();
        let mutable_dir = dir.path().join("mutable");
        fs::create_dir_all(&mutable_dir).unwrap();
        let cache_file = mutable_dir.join("cached.mq");
        fs::write(&cache_file, b"content").unwrap();

        let resolver = HttpModuleResolver {
            allowed_remote_domains: vec![],
            timeout: Duration::from_secs(10),
            cache_dir: dir.path().to_path_buf(),
        };

        assert!(cache_file.exists());
        assert!(resolver.cache_dir.join("mutable").join("cached.mq").exists());
    }

    #[test]
    fn test_clear_cache_removes_only_mutable() {
        let dir = TempDir::new().unwrap();
        let mutable_dir = dir.path().join("mutable");
        let versioned_dir = dir.path().join("versioned");
        fs::create_dir_all(&mutable_dir).unwrap();
        fs::create_dir_all(&versioned_dir).unwrap();
        let mutable_file = mutable_dir.join("abc123.mq");
        let versioned_file = versioned_dir.join("def456.mq");
        fs::write(&mutable_file, b"mutable content").unwrap();
        fs::write(&versioned_file, b"versioned content").unwrap();

        let resolver = HttpModuleResolver {
            allowed_remote_domains: vec![],
            timeout: Duration::from_secs(10),
            cache_dir: dir.path().to_path_buf(),
        };

        resolver.clear_cache().unwrap();
        assert!(!mutable_dir.exists(), "mutable dir should be removed");
        assert!(versioned_file.exists(), "versioned file should be preserved");
    }

    #[test]
    fn test_clear_cache_noop_when_dir_missing() {
        let dir = TempDir::new().unwrap();
        let nonexistent = dir.path().join("nonexistent");

        let resolver = HttpModuleResolver {
            allowed_remote_domains: vec![],
            timeout: Duration::from_secs(10),
            cache_dir: nonexistent,
        };

        assert!(resolver.clear_cache().is_ok());
    }

    #[test]
    fn test_clear_cache_noop_when_only_versioned_exists() {
        let dir = TempDir::new().unwrap();
        let versioned_dir = dir.path().join("versioned");
        fs::create_dir_all(&versioned_dir).unwrap();
        let versioned_file = versioned_dir.join("v1.mq");
        fs::write(&versioned_file, b"pinned").unwrap();

        let resolver = HttpModuleResolver {
            allowed_remote_domains: vec![],
            timeout: Duration::from_secs(10),
            cache_dir: dir.path().to_path_buf(),
        };

        resolver.clear_cache().unwrap();
        assert!(versioned_file.exists(), "versioned file must survive clear_cache");
    }

    #[test]
    fn test_clear_cache_removes_multiple_mutable_files() {
        let dir = TempDir::new().unwrap();
        let mutable_dir = dir.path().join("mutable");
        fs::create_dir_all(&mutable_dir).unwrap();
        for name in &["a.mq", "b.mq", "c.mq"] {
            fs::write(mutable_dir.join(name), b"data").unwrap();
        }

        let resolver = HttpModuleResolver {
            allowed_remote_domains: vec![],
            timeout: Duration::from_secs(10),
            cache_dir: dir.path().to_path_buf(),
        };

        resolver.clear_cache().unwrap();
        assert!(!mutable_dir.exists());
    }

    #[test]
    fn test_resolve_uses_mutable_cache_on_hit() {
        let dir = TempDir::new().unwrap();
        let mutable_dir = dir.path().join("mutable");
        fs::create_dir_all(&mutable_dir).unwrap();

        // Pre-populate the cache with a known URL's hash
        let url = "https://raw.githubusercontent.com/alice/mymod/HEAD/mymod.mq";
        let hash = format!("{:x}.mq", md5::compute(url));
        fs::write(mutable_dir.join(&hash), b"def cached(): 42;").unwrap();

        let resolver = HttpModuleResolver {
            allowed_remote_domains: vec![],
            timeout: Duration::from_secs(10),
            cache_dir: dir.path().to_path_buf(),
        };

        let result = resolver.resolve("https://raw.githubusercontent.com/alice/mymod/HEAD/mymod.mq");
        assert_eq!(result.unwrap(), "def cached(): 42;");
    }

    #[test]
    fn test_resolve_uses_versioned_cache_on_hit() {
        let dir = TempDir::new().unwrap();
        let versioned_dir = dir.path().join("versioned");
        fs::create_dir_all(&versioned_dir).unwrap();

        let url = "https://raw.githubusercontent.com/alice/mymod/v0.1.0/mymod.mq";
        let hash = format!("{:x}.mq", md5::compute(url));
        fs::write(versioned_dir.join(&hash), b"def pinned(): 1;").unwrap();

        let resolver = HttpModuleResolver {
            allowed_remote_domains: vec![],
            timeout: Duration::from_secs(10),
            cache_dir: dir.path().to_path_buf(),
        };

        let result = resolver.resolve("https://raw.githubusercontent.com/alice/mymod/v0.1.0/mymod.mq");
        assert_eq!(result.unwrap(), "def pinned(): 1;");
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
    fn test_new_applies_parameters() {
        let domains = vec!["example.com".to_string()];
        let timeout = Duration::from_secs(5);
        let resolver = HttpModuleResolver::new(domains.clone(), timeout);
        assert_eq!(resolver.allowed_remote_domains, domains);
        assert_eq!(resolver.timeout, timeout);
    }
}
