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
pub struct NativeIo {
    #[cfg(feature = "http-import-ureq")]
    timeout: std::time::Duration,
}

impl Default for NativeIo {
    fn default() -> Self {
        Self {
            #[cfg(feature = "http-import-ureq")]
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

    fn exists(&self, path: &Path) -> bool {
        path.exists()
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
        assert!(io.exists(&path));
    }

    #[test]
    fn test_read_to_string_missing_file_is_not_found() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("missing.txt");
        let io = NativeIo::default();

        assert!(matches!(io.read_to_string(&path), Err(IoError::NotFound(_))));
        assert!(!io.exists(&path));
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
        assert!(matches!(io.fetch("http://example.com"), Err(IoError::Other(_))));
    }
}
