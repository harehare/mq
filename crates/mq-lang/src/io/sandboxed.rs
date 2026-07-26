use super::{Io, IoError, NativeIo};
use std::borrow::Cow;
use std::path::{Component, Path, PathBuf};

/// Filesystem access granted for one capability (read or write) of
/// [`SandboxedIo`].
///
/// Built via `impl Into<PathAccess>` so existing call sites keep working
/// unchanged: `bool` maps to fully denied/allowed, and `Option<Vec<PathBuf>>`
/// maps directly to a CLI flag parsed as "absent = denied, present with no
/// paths = fully allowed, present with paths = restricted to those paths"
/// (e.g. `mq-run`'s `--allow-read`/`--allow-write`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum PathAccess {
    /// No filesystem access at all (fail-safe default).
    #[default]
    Denied,
    /// Unrestricted access, matching a bare `--allow-read`/`--allow-write`.
    Allowed,
    /// Access restricted to the given files/directories (and their
    /// descendants).
    AllowedPaths(Vec<PathBuf>),
}

impl From<bool> for PathAccess {
    fn from(allow: bool) -> Self {
        if allow { PathAccess::Allowed } else { PathAccess::Denied }
    }
}

impl From<Vec<PathBuf>> for PathAccess {
    fn from(paths: Vec<PathBuf>) -> Self {
        if paths.is_empty() {
            PathAccess::Allowed
        } else {
            PathAccess::AllowedPaths(paths)
        }
    }
}

impl From<Option<Vec<PathBuf>>> for PathAccess {
    fn from(paths: Option<Vec<PathBuf>>) -> Self {
        match paths {
            None => PathAccess::Denied,
            Some(paths) => paths.into(),
        }
    }
}

impl PathAccess {
    fn permits(&self, path: &Path) -> bool {
        match self {
            PathAccess::Denied => false,
            PathAccess::Allowed => true,
            PathAccess::AllowedPaths(allowed) => {
                let target = normalize(path);
                allowed.iter().any(|root| target.starts_with(normalize(root)))
            }
        }
    }

    fn is_denied(&self) -> bool {
        matches!(self, PathAccess::Denied)
    }
}

fn normalize(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(path)
    };

    let mut result = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::ParentDir => {
                result.pop();
            }
            Component::CurDir => {}
            other => result.push(other.as_os_str()),
        }
    }
    result
}

/// Wraps an inner [`Io`] (typically [`NativeIo`]) with instance-owned
/// read/write/net/run permission grants, enforced by every operation. All
/// four default to fully denied (fail safe); read/write may additionally be
/// restricted to an allowlist of paths via [`PathAccess::AllowedPaths`].
///
/// Every operation enforces its own permission independently of any external
/// pre-check, so a caller cannot accidentally bypass sandboxing by forgetting
/// to check a flag first.
#[derive(Debug, Clone)]
pub struct SandboxedIo<Inner: Io = NativeIo> {
    inner: Inner,
    allow_read: PathAccess,
    allow_write: PathAccess,
    allow_net: bool,
    allow_run: bool,
}

impl<Inner: Io + Default> Default for SandboxedIo<Inner> {
    /// All permission grants default to fully denied (fail safe), matching [`Inner::default()`](Default).
    fn default() -> Self {
        Self::new(Inner::default())
    }
}

impl<Inner: Io> SandboxedIo<Inner> {
    pub fn new(inner: Inner) -> Self {
        Self {
            inner,
            allow_read: PathAccess::Denied,
            allow_write: PathAccess::Denied,
            allow_net: false,
            allow_run: false,
        }
    }

    /// Grants read access. Accepts a `bool` (fully denied/allowed) or an
    /// `Option<Vec<PathBuf>>`/`Vec<PathBuf>` (`None`/omitted = denied, empty
    /// = fully allowed, non-empty = restricted to those paths), matching
    /// `--allow-read`'s clap parse.
    pub fn allow_read(mut self, allow: impl Into<PathAccess>) -> Self {
        self.allow_read = allow.into();
        self
    }

    /// Grants write access. See [`Self::allow_read`] for the accepted forms.
    pub fn allow_write(mut self, allow: impl Into<PathAccess>) -> Self {
        self.allow_write = allow.into();
        self
    }

    pub fn allow_net(mut self, allow: bool) -> Self {
        self.allow_net = allow;
        self
    }

    /// Gates [`Io::execute`], the primitive backing the `system()` builtin — external process
    /// execution, akin to Deno's `--allow-run`.
    pub fn allow_run(mut self, allow: bool) -> Self {
        self.allow_run = allow;
        self
    }

    /// Whether any read access is granted (fully or restricted to an allowlist).
    pub fn is_read_allowed(&self) -> bool {
        !self.allow_read.is_denied()
    }

    /// Whether any write access is granted (fully or restricted to an allowlist).
    pub fn is_write_allowed(&self) -> bool {
        !self.allow_write.is_denied()
    }

    pub fn is_net_allowed(&self) -> bool {
        self.allow_net
    }

    pub fn is_run_allowed(&self) -> bool {
        self.allow_run
    }
}

fn denied(what: &'static str) -> IoError {
    IoError::PermissionDenied(Cow::Borrowed(what))
}

fn denied_path(action: &str, path: &Path) -> IoError {
    IoError::PermissionDenied(Cow::Owned(format!(
        "{action} of {} is not allowed (outside the allowed paths)",
        path.display()
    )))
}

impl<Inner: Io> Io for SandboxedIo<Inner> {
    fn read_to_string(&self, path: &Path) -> Result<String, IoError> {
        if self.allow_read.is_denied() {
            return Err(denied("filesystem reads are disabled"));
        }
        if !self.allow_read.permits(path) {
            return Err(denied_path("read", path));
        }
        self.inner.read_to_string(path)
    }

    fn read_bytes(&self, path: &Path) -> Result<Vec<u8>, IoError> {
        if self.allow_read.is_denied() {
            return Err(denied("filesystem reads are disabled"));
        }
        if !self.allow_read.permits(path) {
            return Err(denied_path("read", path));
        }
        self.inner.read_bytes(path)
    }

    fn write(&self, path: &Path, content: &[u8]) -> Result<(), IoError> {
        if self.allow_write.is_denied() {
            return Err(denied("filesystem writes are disabled"));
        }
        if !self.allow_write.permits(path) {
            return Err(denied_path("write", path));
        }
        self.inner.write(path, content)
    }

    fn exists(&self, path: &Path) -> Result<bool, IoError> {
        if self.allow_read.is_denied() {
            return Err(denied("filesystem reads are disabled"));
        }
        if !self.allow_read.permits(path) {
            return Err(denied_path("read", path));
        }
        self.inner.exists(path)
    }

    fn read_dir(&self, path: &Path) -> Result<Vec<(PathBuf, bool)>, IoError> {
        if self.allow_read.is_denied() {
            return Err(denied("filesystem reads are disabled"));
        }
        if !self.allow_read.permits(path) {
            return Err(denied_path("read", path));
        }
        self.inner.read_dir(path)
    }

    fn canonicalize(&self, path: &Path) -> PathBuf {
        if self.allow_read.permits(path) {
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

    fn set_fetch_response(&self, url: &str, body: &str) -> Result<(), IoError> {
        if !self.allow_net {
            return Err(denied("network access is disabled"));
        }
        self.inner.set_fetch_response(url, body)
    }

    fn execute(&self, command: &str, args: &[String]) -> Result<String, IoError> {
        if !self.allow_run {
            return Err(denied("process execution is disabled"));
        }
        self.inner.execute(command, args)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::MemIo;
    use rstest::rstest;

    #[test]
    fn test_exists_denied_by_default_then_allowed() {
        let io = SandboxedIo::new(MemIo::default().with_file("/a.txt", "content"));
        assert!(matches!(
            io.exists(Path::new("/a.txt")),
            Err(IoError::PermissionDenied(_))
        ));

        let io = io.allow_read(true);
        assert!(io.exists(Path::new("/a.txt")).unwrap());
    }

    /// `None` = `allow_read`/`allow_write` never called (fail-safe default); `Some(vec![])` =
    /// the CLI flag passed with no value (fully allowed); `Some(paths)` = the flag restricted
    /// to an allowlist. Mirrors clap's parse of `--allow-read`/`--allow-write`.
    #[rstest]
    #[case::denied_by_default(None, "/a.txt", false)]
    #[case::bare_flag_allows_everywhere(Some(vec![]), "/a.txt", true)]
    #[case::within_allowlisted_dir(Some(vec![PathBuf::from("/allowed")]), "/allowed/a.txt", true)]
    #[case::outside_allowlisted_dir(Some(vec![PathBuf::from("/allowed")]), "/other/b.txt", false)]
    #[case::exact_file_match(Some(vec![PathBuf::from("/allowed/a.txt")]), "/allowed/a.txt", true)]
    #[case::dot_dot_escape_rejected(Some(vec![PathBuf::from("/allowed")]), "/allowed/../other/b.txt", false)]
    fn test_read_allowlist(#[case] allow: Option<Vec<PathBuf>>, #[case] target: &str, #[case] expected_ok: bool) {
        let mut io = SandboxedIo::new(
            MemIo::default()
                .with_file("/a.txt", "content")
                .with_file("/allowed/a.txt", "yes")
                .with_file("/other/b.txt", "no"),
        );
        if let Some(paths) = allow {
            io = io.allow_read(Some(paths));
        }

        assert_eq!(io.read_to_string(Path::new(target)).is_ok(), expected_ok);
    }

    #[rstest]
    #[case::denied_by_default(None, "/out.txt", false)]
    #[case::bare_flag_allows_everywhere(Some(vec![]), "/out.txt", true)]
    #[case::within_allowlisted_dir(Some(vec![PathBuf::from("/allowed")]), "/allowed/out.txt", true)]
    #[case::outside_allowlisted_dir(Some(vec![PathBuf::from("/allowed")]), "/other/out.txt", false)]
    fn test_write_allowlist(#[case] allow: Option<Vec<PathBuf>>, #[case] target: &str, #[case] expected_ok: bool) {
        let mut io = SandboxedIo::new(MemIo::default());
        if let Some(paths) = allow {
            io = io.allow_write(Some(paths));
        }

        assert_eq!(io.write(Path::new(target), b"x").is_ok(), expected_ok);
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

    /// `is_read_allowed`/`is_write_allowed` report whether *any* access was granted,
    /// including a restricted allowlist — they don't distinguish full vs. restricted access.
    #[rstest]
    #[case::nothing_granted(false, false, false, false, (false, false, false, false))]
    #[case::read_and_net_granted(true, false, true, false, (true, false, true, false))]
    #[case::all_granted(true, true, true, true, (true, true, true, true))]
    fn test_is_allowed_queries_reflect_builder_state(
        #[case] allow_read: bool,
        #[case] allow_write: bool,
        #[case] allow_net: bool,
        #[case] allow_run: bool,
        #[case] expected: (bool, bool, bool, bool),
    ) {
        let io = SandboxedIo::new(MemIo::default())
            .allow_read(allow_read)
            .allow_write(allow_write)
            .allow_net(allow_net)
            .allow_run(allow_run);

        assert_eq!(
            (
                io.is_read_allowed(),
                io.is_write_allowed(),
                io.is_net_allowed(),
                io.is_run_allowed()
            ),
            expected
        );
    }

    #[test]
    fn test_is_read_allowed_true_for_restricted_allowlist() {
        let io = SandboxedIo::new(MemIo::default()).allow_read(Some(vec![PathBuf::from("/allowed")]));
        assert!(io.is_read_allowed());
    }

    #[test]
    fn test_execute_denied_by_default_then_allowed() {
        let io = SandboxedIo::new(MemIo::default().with_command_response("echo", &["hi".to_string()], "hi"));
        assert!(matches!(
            io.execute("echo", &["hi".to_string()]),
            Err(IoError::PermissionDenied(_))
        ));

        let io = io.allow_run(true);
        assert_eq!(io.execute("echo", &["hi".to_string()]).unwrap(), "hi");
    }
}
