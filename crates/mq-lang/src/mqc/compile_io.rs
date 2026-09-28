//! Keeps environment variables from being baked into compiled programs.
use crate::Shared;
use crate::io::{FileMetadata, HttpRequestSpec, Io, IoError, IoReader};
use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Delegates to `inner`, but refuses and records every environment read.
#[derive(Debug)]
pub(crate) struct CompileTimeIo {
    inner: Shared<dyn Io>,
    env_read: Mutex<Option<String>>,
}

impl CompileTimeIo {
    pub(crate) fn new(inner: Shared<dyn Io>) -> Self {
        Self {
            inner,
            env_read: Mutex::new(None),
        }
    }

    /// The first environment variable read during compilation.
    pub(crate) fn env_read(&self) -> Option<String> {
        self.env_read.lock().map_or(None, |name| name.clone())
    }

    fn deny_env(&self, name: &str) -> IoError {
        if let Ok(mut env_read) = self.env_read.lock() {
            env_read.get_or_insert_with(|| name.to_string());
        }
        IoError::PermissionDenied(Cow::Borrowed("environment variables are not read at compile time"))
    }
}

impl Io for CompileTimeIo {
    fn read_to_string(&self, path: &Path) -> Result<String, IoError> {
        self.inner.read_to_string(path)
    }

    fn read_bytes(&self, path: &Path) -> Result<Vec<u8>, IoError> {
        self.inner.read_bytes(path)
    }

    fn open_read(&self, path: &Path) -> Result<Box<dyn IoReader>, IoError> {
        self.inner.open_read(path)
    }

    fn write(&self, path: &Path, content: &[u8]) -> Result<(), IoError> {
        self.inner.write(path, content)
    }

    fn exists(&self, path: &Path) -> Result<bool, IoError> {
        self.inner.exists(path)
    }

    fn file_size(&self, path: &Path) -> Result<u64, IoError> {
        self.inner.file_size(path)
    }

    fn metadata(&self, path: &Path) -> Result<FileMetadata, IoError> {
        self.inner.metadata(path)
    }

    fn read_dir(&self, path: &Path) -> Result<Vec<(PathBuf, bool)>, IoError> {
        self.inner.read_dir(path)
    }

    fn canonicalize(&self, path: &Path) -> PathBuf {
        self.inner.canonicalize(path)
    }

    fn env_var(&self, name: &str) -> Result<String, IoError> {
        Err(self.deny_env(name))
    }

    fn env_vars(&self) -> Result<Vec<(String, String)>, IoError> {
        Err(self.deny_env("*"))
    }

    fn fetch(&self, url: &str) -> Result<String, IoError> {
        self.inner.fetch(url)
    }

    fn http_request(
        &self,
        method: &str,
        url: &str,
        body: Option<&str>,
        headers: &[(String, String)],
    ) -> Result<String, IoError> {
        self.inner.http_request(method, url, body, headers)
    }

    fn http_request_stream(
        &self,
        method: &str,
        url: &str,
        body: Option<&str>,
        headers: &[(String, String)],
    ) -> Result<Box<dyn IoReader>, IoError> {
        self.inner.http_request_stream(method, url, body, headers)
    }

    fn http_request_all(&self, requests: &[HttpRequestSpec]) -> Result<Vec<String>, IoError> {
        self.inner.http_request_all(requests)
    }

    fn home_dir(&self) -> Option<PathBuf> {
        self.inner.home_dir()
    }

    fn current_dir(&self) -> Option<PathBuf> {
        self.inner.current_dir()
    }

    fn execute(&self, command: &str, args: &[String]) -> Result<String, IoError> {
        self.inner.execute(command, args)
    }
}
