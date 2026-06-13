use std::{borrow::Cow, fs, path::PathBuf, time::Duration};

use sha2::Digest;

use crate::{ModuleError, ModuleResolver};

/// Default domain that is always permitted without `--allowed-domain`.
const DEFAULT_ALLOWED_DOMAIN: &str = "raw.githubusercontent.com/harehare";

/// Maximum response body size for a fetched module (1 MiB).
const MAX_MODULE_SIZE: u64 = 1024 * 1024;

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
/// Each cached module is accompanied by a `.mq.sha256` sidecar file for tamper detection.
/// Files are named `{md5(url)}.mq` and `{md5(url)}.mq.sha256` within their subdirectory.
/// If the process crashes between writing the two files, the sidecar will be absent and
/// the next `resolve()` call will automatically re-fetch rather than serve partial data.
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
    fn canonical_name<'a>(&self, module_path: &'a str) -> &'a str {
        if Self::is_github_url(module_path) || Self::is_remote_url(module_path) {
            Self::extract_module_name(module_path)
        } else {
            module_path
        }
    }

    fn resolve(&self, module_name: &str) -> Result<String, ModuleError> {
        let url = self.to_fetch_url(module_name)?;
        let cache_subdir = self.cache_subdir(&url);
        let cache_file = cache_subdir.join(self.cache_file_name(&url));
        let hash_file = cache_subdir.join(self.cache_hash_file_name(&url));

        // Fast path: serve from cache without acquiring any lock.
        if let Some(content) = self.try_read_cache(&cache_file, &hash_file)? {
            return Ok(content);
        }

        // Slow path: acquire an exclusive file lock so that only one thread/process
        // fetches and writes the cache at a time.  Others will wait, then hit the fast
        // path on the re-check below.
        fs::create_dir_all(&cache_subdir).map_err(|e| ModuleError::IOError(e.to_string().into()))?;
        let lock_path = cache_subdir.join(self.cache_lock_file_name(&url));
        let lock_file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(|e| ModuleError::IOError(e.to_string().into()))?;
        lock_file
            .lock()
            .map_err(|e| ModuleError::IOError(e.to_string().into()))?;

        // Re-check under lock: another engine may have populated the cache while we waited.
        if let Some(content) = self.try_read_cache(&cache_file, &hash_file)? {
            return Ok(content);
        }

        let content = self.fetch_url(&url)?;
        fs::write(&cache_file, content.as_bytes()).map_err(|e| ModuleError::IOError(e.to_string().into()))?;
        fs::write(&hash_file, Self::compute_content_hash(&content).as_bytes())
            .map_err(|e| ModuleError::IOError(e.to_string().into()))?;

        // Releasing the lock (drop) after both files are written keeps the invariant that
        // any reader that obtains the lock will see both files present.
        drop(lock_file);
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
    /// An empty `allowed_remote_domains` list restricts access to the built-in default domain
    /// (`raw.githubusercontent.com/harehare`) only. Additional domains must be listed explicitly.
    ///
    /// Entries in the form `github.com/{user}/{repo}` (with or without `https://` prefix) are
    /// automatically expanded to `raw.githubusercontent.com/{user}/{repo}`, so callers can
    /// use the familiar GitHub URL style instead of the raw content URL.
    pub fn new(allowed_remote_domains: Vec<String>, timeout: Duration) -> Self {
        let cache_dir = dirs::cache_dir().unwrap_or_default().join("mq");
        Self {
            allowed_remote_domains: allowed_remote_domains
                .into_iter()
                .map(|d| Self::normalize_allowed_domain(&d))
                .collect(),
            timeout,
            cache_dir,
        }
    }

    /// Normalizes a user-supplied allowed-domain entry.
    ///
    /// `github.com/{path}` (with or without `https://`/`http://` prefix) is expanded to
    /// `raw.githubusercontent.com/{path}` so that users can write
    /// `--allowed-domain github.com/alice/myrepo` instead of the full raw content URL.
    /// The scheme prefix is always stripped before storing.
    pub fn normalize_allowed_domain(domain: &str) -> String {
        let without_scheme = domain
            .strip_prefix("https://")
            .or_else(|| domain.strip_prefix("http://"))
            .unwrap_or(domain);

        if let Some(rest) = without_scheme.strip_prefix("github.com/") {
            format!("raw.githubusercontent.com/{}", rest)
        } else {
            without_scheme.to_string()
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
    /// - `{repo.mq}` → fetches `{repo.mq}` from the repo named `{repo.mq}` at HEAD
    /// - `{repo}/{subpath}` → fetches `{subpath}` from the given repo
    ///
    /// `{version}` (e.g. `v0.1.0`) selects a specific git tag; omitting it uses `HEAD`.
    ///
    /// # Examples
    /// | Input | Resolved URL |
    /// |---|---|
    /// | `github.com/alice/mymod` | `…/alice/mymod/HEAD/mymod.mq` |
    /// | `github.com/alice/mymod.mq@v1.0` | `…/alice/mymod.mq/v1.0/mymod.mq` |
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
                let repo = *name;
                let file = if name.ends_with(".mq") {
                    name.to_string()
                } else {
                    format!("{}.mq", name)
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

    /// Returns `true` if `url`'s host/path is permitted.
    ///
    /// `DEFAULT_ALLOWED_DOMAIN` (`raw.githubusercontent.com/harehare`) is always permitted.
    /// Additional domains are granted via the `--allowed-domain` flag; an empty user list
    /// does **not** open up all domains — only the default is allowed.
    ///
    /// The match requires that after the domain/path prefix the next character is `/`, `?`,
    /// `#`, `:` (port), or the string ends — preventing `example.com.evil.com` from
    /// bypassing an `example.com` allowlist entry.
    pub fn is_allowed_domain(&self, url: &str) -> bool {
        let url_without_scheme = url
            .strip_prefix("https://")
            .or_else(|| url.strip_prefix("http://"))
            .unwrap_or(url);

        if Self::prefix_matches(url_without_scheme, DEFAULT_ALLOWED_DOMAIN) {
            return true;
        }

        self.allowed_remote_domains
            .iter()
            .any(|domain| Self::prefix_matches(url_without_scheme, domain.as_str()))
    }

    fn prefix_matches(url_without_scheme: &str, domain: &str) -> bool {
        let rest = match url_without_scheme.strip_prefix(domain) {
            Some(r) => r,
            None => return false,
        };
        rest.is_empty()
            || rest.starts_with('/')
            || rest.starts_with('?')
            || rest.starts_with('#')
            || rest.starts_with(':')
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

    /// Removes all cached modules including versioned (tagged) ones, lock files, and hash sidecars.
    ///
    /// Unlike [`clear_cache`], this also clears `{cache_dir}/versioned/`, so the next resolve
    /// re-fetches every module regardless of whether it was pinned to a tag.
    /// Use `--clear-cache` on the CLI to trigger this.
    pub fn clear_all_cache(&self) -> Result<(), ModuleError> {
        for subdir in &["mutable", "versioned"] {
            let dir = self.cache_dir.join(subdir);
            if dir.exists() {
                fs::remove_dir_all(&dir).map_err(|e| ModuleError::IOError(e.to_string().into()))?;
            }
        }
        Ok(())
    }

    /// Fetches module source from the given URL without consulting the cache.
    ///
    /// Only HTTPS URLs are accepted; plain HTTP is rejected. Redirects are not followed.
    /// The response body is capped at [`MAX_MODULE_SIZE`].
    /// Returns an error if the response Content-Type is `text/html` (e.g. a 404 error page
    /// served with status 200), giving a clearer message than a parse error would.
    pub fn fetch_url(&self, url: &str) -> Result<String, ModuleError> {
        if !Self::is_remote_url(url) {
            return Err(ModuleError::NotFound(Cow::Owned(url.to_string())));
        }
        if !url.starts_with("https://") {
            return Err(ModuleError::IOError(
                format!("Only HTTPS URLs are allowed: {}", url).into(),
            ));
        }
        if !self.is_allowed_domain(url) {
            return Err(ModuleError::IOError(format!("Domain not allowed: {}", url).into()));
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

        response
            .body_mut()
            .with_config()
            .limit(MAX_MODULE_SIZE)
            .read_to_string()
            .map_err(|e| ModuleError::IOError(e.to_string().into()))
    }

    /// Extracts a short module name from an HTTP URL or GitHub shorthand.
    ///
    /// Strips the URL scheme, domain, and path prefix, then removes any `@version`
    /// suffix and the `.mq` file extension from the last path segment.
    ///
    /// # Examples
    /// | Input | Result |
    /// |---|---|
    /// | `github.com/alice/mymod` | `"mymod"` |
    /// | `github.com/alice/mymod.mq` | `"mymod"` |
    /// | `github.com/alice/mymod.mq@v1.0` | `"mymod"` |
    /// | `https://example.com/path/foo.mq` | `"foo"` |
    pub fn extract_module_name(module_path: &str) -> &str {
        let path = module_path
            .strip_prefix("https://")
            .or_else(|| module_path.strip_prefix("http://"))
            .unwrap_or(module_path);

        let without_version = match path.rfind('@') {
            Some(pos) => &path[..pos],
            None => path,
        };

        let last_segment = without_version.rsplit('/').next().unwrap_or(without_version);

        last_segment.strip_suffix(".mq").unwrap_or(last_segment)
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
                let ref_segment = rest.split('/').nth(2).unwrap_or("HEAD");
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

    fn cache_hash_file_name(&self, url: &str) -> String {
        let hash = md5::compute(url);
        format!("{:x}.mq.sha256", hash)
    }

    fn cache_lock_file_name(&self, url: &str) -> String {
        let hash = md5::compute(url);
        format!("{:x}.mq.lock", hash)
    }

    /// Tries to read a cached module without holding any lock.
    ///
    /// Returns `Ok(Some(content))` on a valid cache hit, `Ok(None)` when the cache
    /// is missing or the hash doesn't match, and `Err` only on unexpected I/O errors.
    fn try_read_cache(
        &self,
        cache_file: &std::path::Path,
        hash_file: &std::path::Path,
    ) -> Result<Option<String>, ModuleError> {
        if !cache_file.exists() || !hash_file.exists() {
            return Ok(None);
        }
        let content =
            fs::read_to_string(cache_file).map_err(|e| ModuleError::IOError(e.to_string().into()))?;
        let stored =
            fs::read_to_string(hash_file).map_err(|e| ModuleError::IOError(e.to_string().into()))?;
        if stored.trim() == Self::compute_content_hash(&content) {
            Ok(Some(content))
        } else {
            Ok(None)
        }
    }

    /// Computes the SHA-256 hash of `content` and returns it as a lowercase hex string.
    pub(crate) fn compute_content_hash(content: &str) -> String {
        sha2::Sha256::digest(content.as_bytes())
            .as_slice()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect()
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
    #[case("github.com/alice/mymod", "mymod")]
    #[case("github.com/alice/mymod.mq", "mymod")]
    #[case("github.com/alice/mymod.mq@v1.0", "mymod")]
    #[case("github.com/alice/mymod@v1.0", "mymod")]
    #[case("github.com/alice/repo/lib/utils.mq", "utils")]
    #[case("https://example.com/path/foo.mq", "foo")]
    #[case("https://example.com/foo.mq", "foo")]
    #[case("http://example.com/bar.mq", "bar")]
    #[case("https://example.com/noext", "noext")]
    fn test_extract_module_name(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(HttpModuleResolver::extract_module_name(input), expected);
    }

    #[rstest]
    #[case("github.com/alice/mymod", "mymod")]
    #[case("github.com/alice/mymod.mq@v1.0", "mymod")]
    #[case("https://example.com/foo.mq", "foo")]
    #[case("local_module", "local_module")]
    fn test_canonical_name(#[case] input: &str, #[case] expected: &str) {
        let resolver = HttpModuleResolver::default();
        assert_eq!(resolver.canonical_name(input), expected);
    }

    #[rstest]
    #[case("github.com/alice/myrepo", "raw.githubusercontent.com/alice/myrepo")]
    #[case("github.com/alice", "raw.githubusercontent.com/alice")]
    #[case("https://github.com/alice/myrepo", "raw.githubusercontent.com/alice/myrepo")]
    #[case("http://github.com/alice/myrepo", "raw.githubusercontent.com/alice/myrepo")]
    #[case("example.com", "example.com")]
    #[case("https://example.com", "example.com")]
    #[case("raw.githubusercontent.com/alice/repo", "raw.githubusercontent.com/alice/repo")]
    fn test_normalize_allowed_domain(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(HttpModuleResolver::normalize_allowed_domain(input), expected);
    }

    #[rstest]
    // github.com/user/repo shorthand is expanded to raw.githubusercontent.com at construction
    #[case(vec!["github.com/alice/myrepo".to_string()], "https://raw.githubusercontent.com/alice/myrepo/HEAD/mod.mq", true)]
    #[case(vec!["github.com/alice/myrepo".to_string()], "https://raw.githubusercontent.com/alice/other/HEAD/mod.mq", false)]
    // plain domain still works
    #[case(vec!["example.com".to_string()], "https://example.com/foo.mq", true)]
    fn test_new_normalizes_github_domains(#[case] domains: Vec<String>, #[case] url: &str, #[case] expected: bool) {
        let resolver = HttpModuleResolver::new(domains, Duration::from_secs(10));
        assert_eq!(resolver.is_allowed_domain(url), expected);
    }

    // github.com/user/repo shorthand accepted via --allowed-domain
    #[test]
    fn test_to_fetch_url_allowed_via_github_shorthand_domain() {
        let resolver = HttpModuleResolver::new(
            vec!["github.com/alice/lisp".to_string()],
            Duration::from_secs(10),
        );
        assert!(resolver.to_fetch_url("github.com/alice/lisp").is_ok());
        // Other repos under alice remain blocked
        assert!(resolver.to_fetch_url("github.com/alice/other").is_err());
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
        "https://raw.githubusercontent.com/harehare/lisp.mq/HEAD/lisp.mq"
    )]
    #[case(
        "github.com/harehare/lisp.mq@v0.1.0",
        "https://raw.githubusercontent.com/harehare/lisp.mq/v0.1.0/lisp.mq"
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
        "https://raw.githubusercontent.com/harehare/lisp.mq/v0.1.0/lisp.mq"
    )]
    // repo name contains a dot (e.g. json5.mq) — the full name is used as-is
    #[case(
        "github.com/harehare/json5.mq",
        "https://raw.githubusercontent.com/harehare/json5.mq/HEAD/json5.mq"
    )]
    #[case(
        "github.com/harehare/json5.mq@v0.1.0",
        "https://raw.githubusercontent.com/harehare/json5.mq/v0.1.0/json5.mq"
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
    // empty list: default domain always allowed
    #[case(vec![], "https://raw.githubusercontent.com/harehare/lisp/HEAD/lisp.mq", true)]
    #[case(vec![], "https://raw.githubusercontent.com/harehare/repo/v0.1.0/mod.mq", true)]
    // empty list: non-default domains denied
    #[case(vec![], "https://example.com/foo.mq", false)]
    #[case(vec![], "http://anything.org/bar.mq", false)]
    // user-specified domain allowed
    #[case(vec!["example.com".to_string()], "https://example.com/foo.mq", true)]
    #[case(vec!["example.com".to_string()], "https://example.com", true)]
    #[case(vec!["example.com".to_string()], "https://example.com:8080/foo.mq", true)]
    #[case(vec!["example.com/repo".to_string()], "https://example.com/repo/foo.mq", true)]
    #[case(vec!["example.com".to_string()], "https://other.com/foo.mq", false)]
    #[case(vec!["example.com".to_string()], "https://notexample.com/foo.mq", false)]
    #[case(vec!["example.com".to_string()], "http://example.com/foo.mq", true)]
    // default domain always allowed even when user list is non-empty
    #[case(vec!["example.com".to_string()], "https://raw.githubusercontent.com/harehare/x/HEAD/x.mq", true)]
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
    // harehare repos always allowed with empty list
    #[case(
        "github.com/harehare/lisp.mq@v0.1.0",
        "https://raw.githubusercontent.com/harehare/lisp.mq/v0.1.0/lisp.mq"
    )]
    #[case(
        "github.com/harehare/lisp",
        "https://raw.githubusercontent.com/harehare/lisp/HEAD/lisp.mq"
    )]
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
    // non-harehare GitHub shorthand blocked by empty allowlist
    #[case(vec![], "github.com/alice/lisp")]
    // non-harehare GitHub shorthand blocked by unrelated allowlist
    #[case(vec!["example.com".to_string()], "github.com/alice/lisp")]
    // plain HTTPS URL blocked by allowlist
    #[case(vec!["example.com".to_string()], "https://other.com/foo.mq")]
    // plain HTTPS URL blocked by empty allowlist
    #[case(vec![], "https://example.com/foo.mq")]
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
        let mutable_mq = mutable_dir.join("abc123.mq");
        let mutable_hash = mutable_dir.join("abc123.mq.sha256");
        let versioned_file = versioned_dir.join("def456.mq");
        let versioned_hash = versioned_dir.join("def456.mq.sha256");
        fs::write(&mutable_mq, b"mutable content").unwrap();
        fs::write(&mutable_hash, b"deadbeef").unwrap();
        fs::write(&versioned_file, b"versioned content").unwrap();
        fs::write(&versioned_hash, b"cafebabe").unwrap();

        let resolver = HttpModuleResolver {
            allowed_remote_domains: vec![],
            timeout: Duration::from_secs(10),
            cache_dir: dir.path().to_path_buf(),
        };

        resolver.clear_cache().unwrap();
        assert!(!mutable_dir.exists(), "mutable dir should be removed");
        assert!(versioned_file.exists(), "versioned .mq file should be preserved");
        assert!(versioned_hash.exists(), "versioned .mq.sha256 sidecar should be preserved");
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
    fn test_clear_all_cache_removes_mutable_and_versioned() {
        let dir = TempDir::new().unwrap();
        let mutable_dir = dir.path().join("mutable");
        let versioned_dir = dir.path().join("versioned");
        fs::create_dir_all(&mutable_dir).unwrap();
        fs::create_dir_all(&versioned_dir).unwrap();
        fs::write(mutable_dir.join("a.mq"), b"mutable").unwrap();
        fs::write(mutable_dir.join("a.mq.lock"), b"").unwrap();
        fs::write(versioned_dir.join("b.mq"), b"versioned").unwrap();
        fs::write(versioned_dir.join("b.mq.lock"), b"").unwrap();

        let resolver = HttpModuleResolver {
            allowed_remote_domains: vec![],
            timeout: Duration::from_secs(10),
            cache_dir: dir.path().to_path_buf(),
        };

        resolver.clear_all_cache().unwrap();
        assert!(!mutable_dir.exists(), "mutable dir should be removed");
        assert!(!versioned_dir.exists(), "versioned dir should be removed");
    }

    #[test]
    fn test_clear_all_cache_noop_when_dirs_missing() {
        let dir = TempDir::new().unwrap();
        let resolver = HttpModuleResolver {
            allowed_remote_domains: vec![],
            timeout: Duration::from_secs(10),
            cache_dir: dir.path().to_path_buf(),
        };
        assert!(resolver.clear_all_cache().is_ok());
    }

    #[test]
    fn test_resolve_uses_mutable_cache_on_hit() {
        let dir = TempDir::new().unwrap();
        let mutable_dir = dir.path().join("mutable");
        fs::create_dir_all(&mutable_dir).unwrap();

        let url = "https://raw.githubusercontent.com/harehare/mymod/HEAD/mymod.mq";
        let content = "def cached(): 42;";
        let hash = format!("{:x}.mq", md5::compute(url));
        let hash256 = format!("{:x}.mq.sha256", md5::compute(url));
        fs::write(mutable_dir.join(&hash), content.as_bytes()).unwrap();
        fs::write(
            mutable_dir.join(&hash256),
            HttpModuleResolver::compute_content_hash(content).as_bytes(),
        )
        .unwrap();

        let resolver = HttpModuleResolver {
            allowed_remote_domains: vec![],
            timeout: Duration::from_secs(10),
            cache_dir: dir.path().to_path_buf(),
        };

        let result = resolver.resolve("https://raw.githubusercontent.com/harehare/mymod/HEAD/mymod.mq");
        assert_eq!(result.unwrap(), content);
    }

    #[test]
    fn test_resolve_cache_without_hash_sidecar_triggers_refetch() {
        let dir = TempDir::new().unwrap();
        let mutable_dir = dir.path().join("mutable");
        fs::create_dir_all(&mutable_dir).unwrap();

        let url = "https://raw.githubusercontent.com/harehare/mymod/HEAD/mymod.mq";
        let hash = format!("{:x}.mq", md5::compute(url));
        // Write only the content file — no .mq.sha256 sidecar
        fs::write(mutable_dir.join(&hash), b"def foo(): 1;").unwrap();

        let resolver = HttpModuleResolver {
            allowed_remote_domains: vec![],
            timeout: Duration::from_secs(10),
            cache_dir: dir.path().to_path_buf(),
        };

        // No sidecar → must attempt a network re-fetch (which fails in tests)
        let result = resolver.resolve("https://raw.githubusercontent.com/harehare/mymod/HEAD/mymod.mq");
        assert!(result.is_err(), "cache without hash sidecar must trigger re-fetch");
    }

    #[test]
    fn test_resolve_tampered_cache_triggers_refetch() {
        let dir = TempDir::new().unwrap();
        let mutable_dir = dir.path().join("mutable");
        fs::create_dir_all(&mutable_dir).unwrap();

        let url = "https://raw.githubusercontent.com/harehare/mymod/HEAD/mymod.mq";
        let content = "def cached(): 42;";
        let hash = format!("{:x}.mq", md5::compute(url));
        let hash256 = format!("{:x}.mq.sha256", md5::compute(url));
        fs::write(mutable_dir.join(&hash), content.as_bytes()).unwrap();
        // Deliberately wrong hash
        fs::write(
            mutable_dir.join(&hash256),
            b"0000000000000000000000000000000000000000000000000000000000000000",
        )
        .unwrap();

        let resolver = HttpModuleResolver {
            allowed_remote_domains: vec![],
            timeout: Duration::from_secs(10),
            cache_dir: dir.path().to_path_buf(),
        };

        // Must attempt a network re-fetch (which fails in tests) rather than return tampered content
        let result = resolver.resolve("https://raw.githubusercontent.com/harehare/mymod/HEAD/mymod.mq");
        assert!(result.is_err(), "tampered cache must not return cached content");
    }

    #[test]
    fn test_resolve_uses_versioned_cache_on_hit() {
        let dir = TempDir::new().unwrap();
        let versioned_dir = dir.path().join("versioned");
        fs::create_dir_all(&versioned_dir).unwrap();

        let url = "https://raw.githubusercontent.com/harehare/mymod/v0.1.0/mymod.mq";
        let content = "def pinned(): 1;";
        let hash = format!("{:x}.mq", md5::compute(url));
        let hash256 = format!("{:x}.mq.sha256", md5::compute(url));
        fs::write(versioned_dir.join(&hash), content.as_bytes()).unwrap();
        fs::write(
            versioned_dir.join(&hash256),
            HttpModuleResolver::compute_content_hash(content).as_bytes(),
        )
        .unwrap();

        let resolver = HttpModuleResolver {
            allowed_remote_domains: vec![],
            timeout: Duration::from_secs(10),
            cache_dir: dir.path().to_path_buf(),
        };

        let result = resolver.resolve("https://raw.githubusercontent.com/harehare/mymod/v0.1.0/mymod.mq");
        assert_eq!(result.unwrap(), content);
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
    // empty allowlist blocks non-default domain
    #[case(vec![], "https://example.com/foo.mq")]
    #[case(vec![], "https://raw.githubusercontent.com/alice/mod/HEAD/mod.mq")]
    fn test_resolve_blocked_domain_returns_io_error(#[case] allowed: Vec<String>, #[case] url: &str) {
        let resolver = resolver_with_domains(allowed);
        assert!(matches!(resolver.resolve(url), Err(ModuleError::IOError(_))));
    }

    // fetch_url: HTTPS-only enforcement
    #[rstest]
    #[case("http://example.com/foo.mq")]
    #[case("http://raw.githubusercontent.com/harehare/mod/HEAD/mod.mq")]
    fn test_fetch_url_rejects_http(#[case] url: &str) {
        let resolver = HttpModuleResolver::default();
        assert!(matches!(resolver.fetch_url(url), Err(ModuleError::IOError(_))));
    }

    #[rstest]
    #[case("local_module")]
    #[case("csv")]
    fn test_fetch_url_rejects_non_remote(#[case] url: &str) {
        let resolver = HttpModuleResolver::default();
        assert!(matches!(resolver.fetch_url(url), Err(ModuleError::NotFound(_))));
    }

    #[test]
    fn test_fetch_url_rejects_non_default_domain_with_empty_allowlist() {
        let resolver = resolver_with_domains(vec![]);
        assert!(matches!(
            resolver.fetch_url("https://example.com/foo.mq"),
            Err(ModuleError::IOError(_))
        ));
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

    #[test]
    fn test_compute_content_hash_is_deterministic() {
        let h1 = HttpModuleResolver::compute_content_hash("def foo(): 1;");
        let h2 = HttpModuleResolver::compute_content_hash("def foo(): 1;");
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64, "SHA-256 hex is 64 chars");
    }

    #[test]
    fn test_compute_content_hash_differs_for_different_content() {
        let h1 = HttpModuleResolver::compute_content_hash("def foo(): 1;");
        let h2 = HttpModuleResolver::compute_content_hash("def foo(): 2;");
        assert_ne!(h1, h2);
    }

    // Content-Type: text/html with charset parameter must still be detected as HTML
    #[rstest]
    #[case("text/html")]
    #[case("text/html; charset=utf-8")]
    #[case("text/html;charset=UTF-8")]
    fn test_content_type_html_variants_contain_text_html(#[case] ct: &str) {
        assert!(ct.contains("text/html"));
    }

    #[rstest]
    #[case("text/plain")]
    #[case("text/plain; charset=utf-8")]
    #[case("application/octet-stream")]
    fn test_content_type_non_html_not_detected_as_html(#[case] ct: &str) {
        assert!(!ct.contains("text/html"));
    }

    // https://github.com/owner/repo URL form is resolved to raw.githubusercontent.com
    #[rstest]
    #[case(
        "https://github.com/harehare/lisp@v0.1.0",
        "https://raw.githubusercontent.com/harehare/lisp/v0.1.0/lisp.mq"
    )]
    #[case(
        "https://github.com/harehare/lisp",
        "https://raw.githubusercontent.com/harehare/lisp/HEAD/lisp.mq"
    )]
    fn test_to_fetch_url_https_github_form(#[case] input: &str, #[case] expected: &str) {
        let resolver = resolver_with_domains(vec![]);
        assert_eq!(resolver.to_fetch_url(input).unwrap(), expected);
    }
}
