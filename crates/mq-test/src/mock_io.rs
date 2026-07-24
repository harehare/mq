use mq_lang::{Io, IoError};
use std::borrow::Cow;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// In-memory [`Io`] used by the test runner's engine so that `.mq` test files exercising
/// `read_file`/`write_file`/`http`/environment-variable builtins run hermetically — no real
/// disk or network access — instead of failing outright under the engine's all-denied
/// default (a real, sandboxed [`NativeIo`](mq_lang::NativeIo)).
///
/// A fresh instance is used per test file, so state (files written, nothing else) does not
/// leak across files. Seed fixtures upfront with [`Self::with_file`], [`Self::with_env`], and
/// [`Self::with_fetch_response`] before running a test that expects to read them back.
#[derive(Debug, Default)]
pub struct MockIo {
    files: Mutex<HashMap<PathBuf, Vec<u8>>>,
    env: Mutex<HashMap<String, String>>,
    fetch_responses: Mutex<HashMap<String, String>>,
    home: Option<PathBuf>,
    cwd: Option<PathBuf>,
}

impl MockIo {
    /// Seeds a file so `read_file`/`read_file_bytes`/`exists` see it without a prior `write_file`.
    pub fn with_file(self, path: impl Into<PathBuf>, content: impl Into<Vec<u8>>) -> Self {
        self.files.lock().unwrap().insert(path.into(), content.into());
        self
    }

    /// Seeds an environment variable value.
    pub fn with_env(self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.lock().unwrap().insert(key.into(), value.into());
        self
    }

    /// Seeds the body returned for `fetch`/`http` requests to `url`.
    pub fn with_fetch_response(self, url: impl Into<String>, body: impl Into<String>) -> Self {
        self.fetch_responses.lock().unwrap().insert(url.into(), body.into());
        self
    }

    /// Seeds the value returned by `home_dir`.
    pub fn with_home(mut self, path: impl Into<PathBuf>) -> Self {
        self.home = Some(path.into());
        self
    }

    /// Seeds the value returned by `current_dir`.
    pub fn with_cwd(mut self, path: impl Into<PathBuf>) -> Self {
        self.cwd = Some(path.into());
        self
    }
}

impl Io for MockIo {
    fn read_to_string(&self, path: &Path) -> Result<String, IoError> {
        self.files
            .lock()
            .unwrap()
            .get(path)
            .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
            .ok_or_else(|| IoError::NotFound(Cow::Owned(path.display().to_string())))
    }

    fn read_bytes(&self, path: &Path) -> Result<Vec<u8>, IoError> {
        self.files
            .lock()
            .unwrap()
            .get(path)
            .cloned()
            .ok_or_else(|| IoError::NotFound(Cow::Owned(path.display().to_string())))
    }

    fn write(&self, path: &Path, content: &[u8]) -> Result<(), IoError> {
        self.files.lock().unwrap().insert(path.to_path_buf(), content.to_vec());
        Ok(())
    }

    fn exists(&self, path: &Path) -> Result<bool, IoError> {
        Ok(self.files.lock().unwrap().contains_key(path))
    }

    fn read_dir(&self, path: &Path) -> Result<Vec<(PathBuf, bool)>, IoError> {
        let files = self.files.lock().unwrap();
        Ok(files
            .keys()
            .filter(|p| p.parent() == Some(path))
            .map(|p| (p.clone(), false))
            .collect())
    }

    fn canonicalize(&self, path: &Path) -> PathBuf {
        path.to_path_buf()
    }

    fn env_var(&self, name: &str) -> Result<String, IoError> {
        self.env
            .lock()
            .unwrap()
            .get(name)
            .cloned()
            .ok_or_else(|| IoError::NotFound(Cow::Owned(format!("env var `{name}`"))))
    }

    fn fetch(&self, url: &str) -> Result<String, IoError> {
        self.fetch_responses
            .lock()
            .unwrap()
            .get(url)
            .cloned()
            .ok_or_else(|| IoError::NotFound(Cow::Owned(url.to_string())))
    }

    fn http_request(
        &self,
        _method: &str,
        url: &str,
        _body: Option<&str>,
        _headers: &[(String, String)],
    ) -> Result<String, IoError> {
        self.fetch(url)
    }

    fn home_dir(&self) -> Option<PathBuf> {
        self.home.clone()
    }

    fn current_dir(&self) -> Option<PathBuf> {
        self.cwd.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_round_trip() {
        let io = MockIo::default().with_file("/a.txt", "hello");
        assert_eq!(io.read_to_string(Path::new("/a.txt")).unwrap(), "hello");
        assert_eq!(io.read_bytes(Path::new("/a.txt")).unwrap(), b"hello");
        assert!(io.exists(Path::new("/a.txt")).unwrap());
    }

    #[test]
    fn test_missing_file_is_not_found() {
        let io = MockIo::default();
        assert!(matches!(
            io.read_to_string(Path::new("/missing.txt")),
            Err(IoError::NotFound(_))
        ));
        assert!(!io.exists(Path::new("/missing.txt")).unwrap());
    }

    #[test]
    fn test_write_then_read() {
        let io = MockIo::default();
        io.write(Path::new("/a.txt"), b"written").unwrap();
        assert_eq!(io.read_to_string(Path::new("/a.txt")).unwrap(), "written");
    }

    #[test]
    fn test_env_var_and_fetch() {
        let io = MockIo::default()
            .with_env("FOO", "bar")
            .with_fetch_response("https://example.com", "body");
        assert_eq!(io.env_var("FOO").unwrap(), "bar");
        assert!(matches!(io.env_var("MISSING"), Err(IoError::NotFound(_))));
        assert_eq!(io.fetch("https://example.com").unwrap(), "body");
        assert!(matches!(
            io.fetch("https://missing.example.com"),
            Err(IoError::NotFound(_))
        ));
    }
}
