#[cfg(feature = "http")]
use super::HttpRequestSpec;
use super::{Io, IoError};
use std::borrow::Cow;
use std::path::{Path, PathBuf};

/// Real filesystem/environment/network-backed [`Io`] implementation. This is
/// what an embedding uses when it wants full host capability; wrap it in
/// [`SandboxedIo`](super::SandboxedIo) to gate read/write/net permissions.
///
/// `fetch` does not yet implement the on-disk caching/lockfile behavior that
/// `UreqFetcher` (the module-import HTTP fetcher) has today — that migration
/// is follow-up work. This is a plain, uncached HTTPS GET through the same
/// SSRF-hardened agent used elsewhere in this crate.
#[derive(Debug, Clone)]
#[cfg_attr(not(feature = "http-import-ureq"), derive(Default))]
pub struct NativeIo {
    #[cfg(feature = "http-import-ureq")]
    timeout: std::time::Duration,
}

#[cfg(feature = "http-import-ureq")]
impl Default for NativeIo {
    fn default() -> Self {
        Self {
            timeout: std::time::Duration::from_secs(10),
        }
    }
}

/// Matches the module-import fetcher's existing limit.
#[cfg(feature = "http-import-ureq")]
const MAX_FETCH_SIZE: u64 = 1024 * 1024;

fn io_err(err: std::io::Error, path: &Path) -> IoError {
    if err.kind() == std::io::ErrorKind::NotFound {
        IoError::NotFound(Cow::Owned(path.display().to_string()))
    } else {
        IoError::Other(Cow::Owned(err.to_string()))
    }
}

#[cfg(feature = "http")]
use crate::compression::{Algorithm, read_bounded_to_vec};

impl Io for NativeIo {
    fn read_to_string(&self, path: &Path) -> Result<String, IoError> {
        std::fs::read_to_string(path).map_err(|e| io_err(e, path))
    }

    fn read_bytes(&self, path: &Path) -> Result<Vec<u8>, IoError> {
        std::fs::read(path).map_err(|e| io_err(e, path))
    }

    fn write(&self, path: &Path, content: &[u8]) -> Result<(), IoError> {
        std::fs::write(path, content).map_err(|e| io_err(e, path))
    }

    fn exists(&self, path: &Path) -> Result<bool, IoError> {
        Ok(path.exists())
    }

    fn file_size(&self, path: &Path) -> Result<u64, IoError> {
        std::fs::metadata(path).map(|m| m.len()).map_err(|e| io_err(e, path))
    }

    fn read_dir(&self, path: &Path) -> Result<Vec<(PathBuf, bool)>, IoError> {
        std::fs::read_dir(path)
            .map_err(|e| io_err(e, path))?
            .filter_map(Result::ok)
            .map(|entry| {
                let path = entry.path();
                let is_dir = path.is_dir();
                (path, is_dir)
            })
            .map(Ok)
            .collect()
    }

    fn canonicalize(&self, path: &Path) -> PathBuf {
        std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
    }

    fn env_var(&self, name: &str) -> Result<String, IoError> {
        std::env::var(name).map_err(|_| IoError::NotFound(Cow::Owned(format!("env var `{name}`"))))
    }

    fn home_dir(&self) -> Option<PathBuf> {
        dirs::home_dir()
    }

    fn current_dir(&self) -> Option<PathBuf> {
        std::env::current_dir().ok()
    }

    #[cfg(feature = "http-import-ureq")]
    fn fetch(&self, url: &str) -> Result<String, IoError> {
        if !crate::module::resolver::ssrf::is_https(url) {
            return Err(IoError::Other(Cow::Owned(format!(
                "Only HTTPS URLs are allowed: {url}"
            ))));
        }

        let agent = crate::module::resolver::ssrf::ssrf_safe_agent(self.timeout, true);
        let mut response = agent
            .get(url)
            .call()
            .map_err(|e| IoError::Other(Cow::Owned(e.to_string())))?;

        if response.status() != 200 {
            return Err(IoError::Other(Cow::Owned(format!(
                "Failed to fetch {url} (status: {})",
                response.status()
            ))));
        }

        response
            .body_mut()
            .with_config()
            .limit(MAX_FETCH_SIZE)
            .read_to_string()
            .map_err(|e| IoError::Other(Cow::Owned(e.to_string())))
    }

    #[cfg(not(feature = "http-import-ureq"))]
    fn fetch(&self, _url: &str) -> Result<String, IoError> {
        Err(IoError::Other(Cow::Borrowed(
            "network access is not compiled into this build",
        )))
    }

    #[cfg(feature = "http")]
    fn http_request(
        &self,
        method: &str,
        url: &str,
        body: Option<&str>,
        headers: &[(String, String)],
    ) -> Result<String, IoError> {
        /// Applied to the decompressed body, to guard against decompression bombs.
        const MAX_RESPONSE_SIZE: u64 = 10 * 1024 * 1024;
        const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
        static AGENT: std::sync::LazyLock<ureq::Agent> =
            std::sync::LazyLock::new(|| crate::module::resolver::ssrf::ssrf_safe_agent(TIMEOUT, true));

        if !crate::module::resolver::ssrf::is_https(url) {
            return Err(IoError::Other(Cow::Owned(format!(
                "only https:// URLs are allowed, got {url:?}"
            ))));
        }

        let method: ureq::http::Method = method
            .parse()
            .map_err(|_| IoError::Other(Cow::Owned(format!("invalid HTTP method {method:?}"))))?;

        let has_accept_encoding = headers
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case("accept-encoding"));
        let mut builder = ureq::http::Request::builder().method(method).uri(url);
        if !has_accept_encoding {
            // ureq only advertises encodings it decodes itself (gzip); deflate/zstd are ours below.
            builder = builder.header("accept-encoding", "gzip, deflate, zstd");
        }
        for (name, value) in headers {
            builder = builder.header(name, value);
        }

        let mut response = match body {
            Some(body) => {
                let request = builder
                    .body(body.to_string())
                    .map_err(|e| IoError::Other(Cow::Owned(e.to_string())))?;
                AGENT.run(request)
            }
            None => {
                let request = builder
                    .body(())
                    .map_err(|e| IoError::Other(Cow::Owned(e.to_string())))?;
                AGENT.run(request)
            }
        }
        .map_err(|e| IoError::Other(Cow::Owned(e.to_string())))?;

        let status = response.status();
        if !status.is_success() {
            return Err(IoError::Other(Cow::Owned(format!(
                "request failed with status {status}"
            ))));
        }

        let content_encoding = response
            .headers()
            .get("content-encoding")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.trim().to_ascii_lowercase());

        let bytes = match content_encoding.as_deref().and_then(Algorithm::parse) {
            // Gzip is decoded by ureq itself; unknown/absent encodings pass through as-is.
            Some(Algorithm::Gzip) | None => read_bounded_to_vec(response.body_mut().as_reader(), MAX_RESPONSE_SIZE)
                .map_err(|e| IoError::Other(Cow::Owned(format!("failed to read response body: {e}"))))?,
            Some(algorithm) => {
                let compressed = read_bounded_to_vec(response.body_mut().as_reader(), MAX_RESPONSE_SIZE)
                    .map_err(|e| IoError::Other(Cow::Owned(format!("failed to read response body: {e}"))))?;
                algorithm.decode(&compressed, MAX_RESPONSE_SIZE).map_err(|e| {
                    IoError::Other(Cow::Owned(format!(
                        "failed to decompress ({algorithm:?}) response body: {e}"
                    )))
                })?
            }
        };

        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }

    #[cfg(not(feature = "http"))]
    fn http_request(
        &self,
        _method: &str,
        _url: &str,
        _body: Option<&str>,
        _headers: &[(String, String)],
    ) -> Result<String, IoError> {
        Err(IoError::Other(Cow::Borrowed(
            "network access is not compiled into this build",
        )))
    }

    #[cfg(feature = "http")]
    fn http_request_all(&self, requests: &[HttpRequestSpec]) -> Result<Vec<String>, IoError> {
        // Each request runs on its own thread; NativeIo is a thin handle
        // (a timeout Duration) around the shared SSRF-hardened agent, so
        // concurrent fan-out is safe and bounded by the caller's batch size.
        std::thread::scope(|scope| {
            let handles: Vec<_> = requests
                .iter()
                .map(|spec| {
                    let method = spec.method.clone();
                    let url = spec.url.clone();
                    let body = spec.body.clone();
                    let headers = spec.headers.clone();
                    scope.spawn(move || self.http_request(&method, &url, body.as_deref(), &headers))
                })
                .collect();
            handles
                .into_iter()
                .map(|handle| {
                    handle
                        .join()
                        .unwrap_or(Err(IoError::Other(Cow::Borrowed("http_all worker thread panicked"))))
                })
                .collect()
        })
    }

    #[cfg(feature = "process-io")]
    fn execute(&self, command: &str, args: &[String]) -> Result<String, IoError> {
        let output = std::process::Command::new(command)
            .args(args)
            .output()
            .map_err(|e| IoError::Other(Cow::Owned(format!("failed to execute `{command}`: {e}"))))?;

        if !output.status.success() {
            return Err(IoError::Other(Cow::Owned(format!(
                "`{command}` exited with {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            ))));
        }

        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    #[cfg(not(feature = "process-io"))]
    fn execute(&self, _command: &str, _args: &[String]) -> Result<String, IoError> {
        Err(IoError::Other(Cow::Borrowed(
            "process execution is not compiled into this build",
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_write_round_trip() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("file.txt");
        let io = NativeIo::default();

        io.write(&path, b"hello world").unwrap();
        assert_eq!(io.read_to_string(&path).unwrap(), "hello world");
        assert_eq!(io.read_bytes(&path).unwrap(), b"hello world");
        assert!(io.exists(&path).unwrap());
    }

    #[test]
    fn test_read_to_string_missing_file_is_not_found() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("missing.txt");
        let io = NativeIo::default();

        assert!(matches!(io.read_to_string(&path), Err(IoError::NotFound(_))));
        assert!(!io.exists(&path).unwrap());
    }

    #[test]
    fn test_read_dir_lists_entries() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("a.txt"), "a").unwrap();
        std::fs::create_dir(dir.path().join("subdir")).unwrap();
        let io = NativeIo::default();

        let mut entries = io.read_dir(dir.path()).unwrap();
        entries.sort();
        assert_eq!(
            entries,
            vec![(dir.path().join("a.txt"), false), (dir.path().join("subdir"), true),]
        );
    }

    #[test]
    fn test_canonicalize_falls_back_on_missing_path() {
        let io = NativeIo::default();
        let missing = PathBuf::from("/definitely/does/not/exist/anywhere");
        assert_eq!(io.canonicalize(&missing), missing);
    }

    #[test]
    fn test_env_var_round_trip() {
        let io = NativeIo::default();
        // SAFETY: test-only, single-threaded within this test's execution.
        unsafe { std::env::set_var("MQ_IO_TEST_ENV_VAR", "test-value") };
        assert_eq!(io.env_var("MQ_IO_TEST_ENV_VAR").unwrap(), "test-value");
        unsafe { std::env::remove_var("MQ_IO_TEST_ENV_VAR") };
        assert!(matches!(io.env_var("MQ_IO_TEST_ENV_VAR"), Err(IoError::NotFound(_))));
    }

    #[test]
    fn test_home_dir_and_current_dir_are_resolvable() {
        let io = NativeIo::default();
        assert_eq!(io.home_dir(), dirs::home_dir());
        assert_eq!(io.current_dir(), std::env::current_dir().ok());
    }

    #[cfg(feature = "http-import-ureq")]
    #[test]
    fn test_fetch_rejects_non_https() {
        let io = NativeIo::default();
        assert!(matches!(io.fetch("http://example.invalid"), Err(IoError::Other(_))));
    }

    #[cfg(all(feature = "process-io", not(windows)))]
    #[test]
    fn test_execute_captures_stdout() {
        let io = NativeIo::default();
        let output = io.execute("echo", &["hello".to_string()]).unwrap();
        assert_eq!(output.trim(), "hello");
    }

    #[cfg(all(feature = "process-io", windows))]
    #[test]
    fn test_execute_captures_stdout() {
        let io = NativeIo::default();
        let output = io
            .execute("cmd", &["/C".to_string(), "echo hello".to_string()])
            .unwrap();
        assert_eq!(output.trim(), "hello");
    }

    #[cfg(all(feature = "process-io", not(windows)))]
    #[test]
    fn test_execute_reports_non_zero_exit_status() {
        let io = NativeIo::default();
        assert!(matches!(
            io.execute("sh", &["-c".to_string(), "exit 1".to_string()]),
            Err(IoError::Other(_))
        ));
    }

    #[cfg(all(feature = "process-io", windows))]
    #[test]
    fn test_execute_reports_non_zero_exit_status() {
        let io = NativeIo::default();
        assert!(matches!(
            io.execute("cmd", &["/C".to_string(), "exit 1".to_string()]),
            Err(IoError::Other(_))
        ));
    }

    #[cfg(feature = "process-io")]
    #[test]
    fn test_execute_reports_missing_command() {
        let io = NativeIo::default();
        assert!(matches!(
            io.execute("mq-this-command-should-not-exist", &[]),
            Err(IoError::Other(_))
        ));
    }

    #[cfg(feature = "http")]
    #[test]
    fn test_http_request_rejects_non_https() {
        let io = NativeIo::default();
        assert!(matches!(
            io.http_request("GET", "http://example.invalid", None, &[]),
            Err(IoError::Other(_))
        ));
    }
}
