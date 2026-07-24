use super::{Io, IoError, NativeIo};
use std::borrow::Cow;
use std::path::{Path, PathBuf};

/// Wraps an inner [`Io`] (typically [`NativeIo`]) with instance-owned
/// read/write/net permission flags, enforced by every operation. All three
/// flags default to `false` (fail safe).
///
/// Every operation enforces its own permission independently of any external
/// pre-check, so a caller cannot accidentally bypass sandboxing by forgetting
/// to check a flag first.
#[derive(Debug, Clone)]
pub struct SandboxedIo<Inner: Io = NativeIo> {
    inner: Inner,
    allow_read: bool,
    allow_write: bool,
    allow_net: bool,
}

impl<Inner: Io> SandboxedIo<Inner> {
    pub fn new(inner: Inner) -> Self {
        Self {
            inner,
            allow_read: false,
            allow_write: false,
            allow_net: false,
        }
    }

    pub fn allow_read(mut self, allow: bool) -> Self {
        self.allow_read = allow;
        self
    }

    pub fn allow_write(mut self, allow: bool) -> Self {
        self.allow_write = allow;
        self
    }

    pub fn allow_net(mut self, allow: bool) -> Self {
        self.allow_net = allow;
        self
    }

    pub fn is_read_allowed(&self) -> bool {
        self.allow_read
    }

    pub fn is_write_allowed(&self) -> bool {
        self.allow_write
    }

    pub fn is_net_allowed(&self) -> bool {
        self.allow_net
    }
}

fn denied(what: &'static str) -> IoError {
    IoError::PermissionDenied(Cow::Borrowed(what))
}

impl<Inner: Io> Io for SandboxedIo<Inner> {
    fn read_to_string(&self, path: &Path) -> Result<String, IoError> {
        if !self.allow_read {
            return Err(denied("filesystem reads are disabled"));
        }
        self.inner.read_to_string(path)
    }

    fn read_bytes(&self, path: &Path) -> Result<Vec<u8>, IoError> {
        if !self.allow_read {
            return Err(denied("filesystem reads are disabled"));
        }
        self.inner.read_bytes(path)
    }

    fn write(&self, path: &Path, content: &[u8]) -> Result<(), IoError> {
        if !self.allow_write {
            return Err(denied("filesystem writes are disabled"));
        }
        self.inner.write(path, content)
    }

    fn exists(&self, path: &Path) -> Result<bool, IoError> {
        if !self.allow_read {
            return Err(denied("filesystem reads are disabled"));
        }
        self.inner.exists(path)
    }

    fn read_dir(&self, path: &Path) -> Result<Vec<(PathBuf, bool)>, IoError> {
        if !self.allow_read {
            return Err(denied("filesystem reads are disabled"));
        }
        self.inner.read_dir(path)
    }

    fn canonicalize(&self, path: &Path) -> PathBuf {
        if self.allow_read {
            self.inner.canonicalize(path)
        } else {
            path.to_path_buf()
        }
    }

    fn env_var(&self, name: &str) -> Result<String, IoError> {
        self.inner.env_var(name)
    }

    fn home_dir(&self) -> Option<PathBuf> {
        self.inner.home_dir()
    }

    fn current_dir(&self) -> Option<PathBuf> {
        self.inner.current_dir()
    }

    fn fetch(&self, url: &str) -> Result<String, IoError> {
        if !self.allow_net {
            return Err(denied("network access is disabled"));
        }
        self.inner.fetch(url)
    }

    fn http_request(
        &self,
        method: &str,
        url: &str,
        body: Option<&str>,
        headers: &[(String, String)],
    ) -> Result<String, IoError> {
        if !self.allow_net {
            return Err(denied("network access is disabled"));
        }
        self.inner.http_request(method, url, body, headers)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::MemIo;

    #[test]
    fn test_read_denied_by_default() {
        let io = SandboxedIo::new(MemIo::default().with_file("/a.txt", "content"));
        assert!(matches!(
            io.read_to_string(Path::new("/a.txt")),
            Err(IoError::PermissionDenied(_))
        ));
        assert!(matches!(
            io.exists(Path::new("/a.txt")),
            Err(IoError::PermissionDenied(_))
        ));
    }

    #[test]
    fn test_read_allowed_delegates_to_inner() {
        let io = SandboxedIo::new(MemIo::default().with_file("/a.txt", "content")).allow_read(true);
        assert_eq!(io.read_to_string(Path::new("/a.txt")).unwrap(), "content");
        assert!(io.exists(Path::new("/a.txt")).unwrap());
    }

    #[test]
    fn test_write_denied_by_default_then_allowed() {
        let mut io = SandboxedIo::new(MemIo::default());
        assert!(matches!(
            io.write(Path::new("/a.txt"), b"x"),
            Err(IoError::PermissionDenied(_))
        ));

        io = io.allow_write(true);
        assert!(io.write(Path::new("/a.txt"), b"x").is_ok());
    }

    #[test]
    fn test_fetch_denied_by_default_then_allowed() {
        let io = SandboxedIo::new(MemIo::default().with_fetch_response("https://example.com", "body"));
        assert!(matches!(
            io.fetch("https://example.com"),
            Err(IoError::PermissionDenied(_))
        ));

        let io = io.allow_net(true);
        assert_eq!(io.fetch("https://example.com").unwrap(), "body");
    }

    #[test]
    fn test_env_var_and_home_current_dir_are_not_gated() {
        let io = SandboxedIo::new(
            MemIo::default()
                .with_env("FOO", "bar")
                .with_home("/home/x")
                .with_cwd("/proj"),
        );
        assert_eq!(io.env_var("FOO").unwrap(), "bar");
        assert_eq!(io.home_dir(), Some(PathBuf::from("/home/x")));
        assert_eq!(io.current_dir(), Some(PathBuf::from("/proj")));
    }

    #[test]
    fn test_http_request_denied_by_default_then_allowed() {
        let io = SandboxedIo::new(MemIo::default().with_fetch_response("https://example.com", "body"));
        assert!(matches!(
            io.http_request("GET", "https://example.com", None, &[]),
            Err(IoError::PermissionDenied(_))
        ));

        let io = io.allow_net(true);
        assert_eq!(
            io.http_request("GET", "https://example.com", None, &[]).unwrap(),
            "body"
        );
    }

    #[test]
    fn test_is_allowed_queries_reflect_builder_state() {
        let io = SandboxedIo::new(MemIo::default()).allow_read(true).allow_net(true);
        assert!(io.is_read_allowed());
        assert!(!io.is_write_allowed());
        assert!(io.is_net_allowed());
    }
}
