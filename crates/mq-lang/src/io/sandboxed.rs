use super::{Io, IoError, NativeIo};
use std::borrow::Cow;
use std::path::{Path, PathBuf};

mod env_access;
mod net_access;
mod path_access;
mod run_access;

pub use env_access::EnvAccess;
pub use net_access::NetAccess;
pub use path_access::PathAccess;
pub use run_access::RunAccess;

/// Wraps an inner [`Io`] (typically [`NativeIo`]) with instance-owned
/// read/write/net/run/env permission grants, enforced by every operation. All
/// five default to fully denied (fail safe), and may additionally be
/// restricted to an allowlist via [`PathAccess::AllowedPaths`]/[`NetAccess::AllowedDomains`]/
/// [`RunAccess::AllowedCommands`]/[`EnvAccess::AllowedNames`].
///
/// Every operation enforces its own permission independently of any external
/// pre-check, so a caller cannot accidentally bypass sandboxing by forgetting
/// to check a flag first.
#[derive(Debug, Clone)]
pub struct SandboxedIo<Inner: Io = NativeIo> {
    inner: Inner,
    allow_read: PathAccess,
    allow_write: PathAccess,
    allow_net: NetAccess,
    allow_run: RunAccess,
    allow_env: EnvAccess,
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
            allow_net: NetAccess::Denied,
            allow_run: RunAccess::Denied,
            allow_env: EnvAccess::Denied,
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

    /// Grants network access, gating `fetch`/`http_request`. See [`Self::allow_read`] for
    /// the accepted forms; `Vec<String>`/`Option<Vec<String>>` restrict by domain (and any
    /// path under it) rather than by filesystem path.
    pub fn allow_net(mut self, allow: impl Into<NetAccess>) -> Self {
        self.allow_net = allow.into();
        self
    }

    /// Gates [`Io::execute`], the primitive backing the `system()` builtin — external process
    /// execution. See [`Self::allow_read`] for the accepted forms; `Vec<String>`/
    /// `Option<Vec<String>>` restrict by command name.
    pub fn allow_run(mut self, allow: impl Into<RunAccess>) -> Self {
        self.allow_run = allow.into();
        self
    }

    /// Grants environment-variable access, gating `$VAR`/`${$VAR}` resolution and debugger
    /// logpoint interpolation. See [`Self::allow_read`] for the accepted forms.
    pub fn allow_env(mut self, allow: impl Into<EnvAccess>) -> Self {
        self.allow_env = allow.into();
        self
    }

    /// Grants every permission at once (read/write/net/run/env), fully and
    /// unrestricted, matching `--allow-all`.
    pub fn allow_all(self) -> Self {
        self.allow_read(true)
            .allow_write(true)
            .allow_net(true)
            .allow_run(true)
            .allow_env(true)
    }

    /// Whether any read access is granted (fully or restricted to an allowlist).
    pub fn is_read_allowed(&self) -> bool {
        !self.allow_read.is_denied()
    }

    /// Whether any write access is granted (fully or restricted to an allowlist).
    pub fn is_write_allowed(&self) -> bool {
        !self.allow_write.is_denied()
    }

    /// Whether any network access is granted (fully or restricted to an allowlist).
    pub fn is_net_allowed(&self) -> bool {
        !self.allow_net.is_denied()
    }

    /// Whether any process-execution access is granted (fully or restricted to an allowlist).
    pub fn is_run_allowed(&self) -> bool {
        !self.allow_run.is_denied()
    }

    /// Whether any environment-variable access is granted (fully or restricted to an allowlist).
    pub fn is_env_allowed(&self) -> bool {
        !self.allow_env.is_denied()
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

fn denied_env(name: &str) -> IoError {
    IoError::PermissionDenied(Cow::Owned(format!(
        "access to environment variable `{name}` is not allowed"
    )))
}

fn denied_domain(url: &str) -> IoError {
    IoError::PermissionDenied(Cow::Owned(format!(
        "network access to {url} is not allowed (outside the allowed domains)"
    )))
}

fn denied_command(command: &str) -> IoError {
    IoError::PermissionDenied(Cow::Owned(format!(
        "execution of {command} is not allowed (outside the allowed commands)"
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
        if self.allow_env.is_denied() {
            return Err(denied("environment variable access is disabled"));
        }
        if !self.allow_env.permits(name) {
            return Err(denied_env(name));
        }
        self.inner.env_var(name)
    }

    fn home_dir(&self) -> Option<PathBuf> {
        self.inner.home_dir()
    }

    fn current_dir(&self) -> Option<PathBuf> {
        self.inner.current_dir()
    }

    fn fetch(&self, url: &str) -> Result<String, IoError> {
        if self.allow_net.is_denied() {
            return Err(denied("network access is disabled"));
        }
        if !self.allow_net.permits(url) {
            return Err(denied_domain(url));
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
        if self.allow_net.is_denied() {
            return Err(denied("network access is disabled"));
        }
        if !self.allow_net.permits(url) {
            return Err(denied_domain(url));
        }
        self.inner.http_request(method, url, body, headers)
    }

    // Not gated by the domain allowlist: this seeds mock data for the `mock_fetch` builtin
    // rather than performing a real request, so there's no domain to restrict.
    fn set_fetch_response(&self, url: &str, body: &str) -> Result<(), IoError> {
        if self.allow_net.is_denied() {
            return Err(denied("network access is disabled"));
        }
        self.inner.set_fetch_response(url, body)
    }

    fn execute(&self, command: &str, args: &[String]) -> Result<String, IoError> {
        if self.allow_run.is_denied() {
            return Err(denied("process execution is disabled"));
        }
        if !self.allow_run.permits(command) {
            return Err(denied_command(command));
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

    /// `None` = `allow_net` never called (fail-safe default); `Some(vec![])` = the CLI flag
    /// passed with no value (fully allowed); `Some(domains)` = the flag restricted to an
    /// allowlist. Mirrors clap's parse of `--allow-net`.
    #[rstest]
    #[case::denied_by_default(None, "https://example.com", false)]
    #[case::bare_flag_allows_everywhere(Some(vec![]), "https://example.com", true)]
    #[case::within_allowlisted_domain(Some(vec!["example.com".to_string()]), "https://example.com/path", true)]
    #[case::outside_allowlisted_domain(Some(vec!["example.com".to_string()]), "https://other.com", false)]
    #[case::subdomain_prefix_escape_rejected(
        Some(vec!["example.com".to_string()]),
        "https://example.com.evil.com",
        false
    )]
    fn test_fetch_allowlist(#[case] allow: Option<Vec<String>>, #[case] target: &str, #[case] expected_ok: bool) {
        let mut io = SandboxedIo::new(
            MemIo::default()
                .with_fetch_response("https://example.com", "body")
                .with_fetch_response("https://example.com/path", "body")
                .with_fetch_response("https://other.com", "body")
                .with_fetch_response("https://example.com.evil.com", "body"),
        );
        if let Some(domains) = allow {
            io = io.allow_net(Some(domains));
        }

        assert_eq!(io.fetch(target).is_ok(), expected_ok);
    }

    #[test]
    fn test_env_var_denied_by_default_then_allowed() {
        let io = SandboxedIo::new(MemIo::default().with_env("FOO", "bar"));
        assert!(matches!(io.env_var("FOO"), Err(IoError::PermissionDenied(_))));

        let io = io.allow_env(true);
        assert_eq!(io.env_var("FOO").unwrap(), "bar");
    }

    /// `None` = `allow_env` never called (fail-safe default); `Some(vec![])` = the CLI flag
    /// passed with no value (fully allowed); `Some(names)` = the flag restricted to an
    /// allowlist. Mirrors clap's parse of `--allow-env`.
    #[rstest]
    #[case::denied_by_default(None, "FOO", false)]
    #[case::bare_flag_allows_everywhere(Some(vec![]), "FOO", true)]
    #[case::within_allowlist(Some(vec!["FOO".to_string()]), "FOO", true)]
    #[case::outside_allowlist(Some(vec!["FOO".to_string()]), "BAR", false)]
    fn test_env_var_allowlist(#[case] allow: Option<Vec<String>>, #[case] target: &str, #[case] expected_ok: bool) {
        let mut io = SandboxedIo::new(MemIo::default().with_env("FOO", "foo").with_env("BAR", "bar"));
        if let Some(names) = allow {
            io = io.allow_env(Some(names));
        }

        assert_eq!(io.env_var(target).is_ok(), expected_ok);
    }

    #[test]
    fn test_home_and_current_dir_are_not_gated() {
        let io = SandboxedIo::new(MemIo::default().with_home("/home/x").with_cwd("/proj"));
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

    /// `is_read_allowed`/`is_write_allowed`/`is_env_allowed` report whether *any* access was
    /// granted, including a restricted allowlist — they don't distinguish full vs. restricted
    /// access.
    #[rstest]
    #[case::nothing_granted(false, false, false, false, false, (false, false, false, false, false))]
    #[case::read_and_net_granted(true, false, true, false, false, (true, false, true, false, false))]
    #[case::all_granted(true, true, true, true, true, (true, true, true, true, true))]
    fn test_is_allowed_queries_reflect_builder_state(
        #[case] allow_read: bool,
        #[case] allow_write: bool,
        #[case] allow_net: bool,
        #[case] allow_run: bool,
        #[case] allow_env: bool,
        #[case] expected: (bool, bool, bool, bool, bool),
    ) {
        let io = SandboxedIo::new(MemIo::default())
            .allow_read(allow_read)
            .allow_write(allow_write)
            .allow_net(allow_net)
            .allow_run(allow_run)
            .allow_env(allow_env);

        assert_eq!(
            (
                io.is_read_allowed(),
                io.is_write_allowed(),
                io.is_net_allowed(),
                io.is_run_allowed(),
                io.is_env_allowed(),
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
    fn test_is_env_allowed_true_for_restricted_allowlist() {
        let io = SandboxedIo::new(MemIo::default()).allow_env(Some(vec!["FOO".to_string()]));
        assert!(io.is_env_allowed());
    }

    #[test]
    fn test_is_net_allowed_true_for_restricted_allowlist() {
        let io = SandboxedIo::new(MemIo::default()).allow_net(Some(vec!["example.com".to_string()]));
        assert!(io.is_net_allowed());
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

    /// `None` = `allow_run` never called (fail-safe default); `Some(vec![])` = the CLI flag
    /// passed with no value (fully allowed); `Some(commands)` = the flag restricted to an
    /// allowlist. Mirrors clap's parse of `--allow-run`.
    #[rstest]
    #[case::denied_by_default(None, "echo", false)]
    #[case::bare_flag_allows_everywhere(Some(vec![]), "echo", true)]
    #[case::within_allowlist(Some(vec!["echo".to_string()]), "echo", true)]
    #[case::outside_allowlist(Some(vec!["echo".to_string()]), "rm", false)]
    fn test_execute_allowlist(#[case] allow: Option<Vec<String>>, #[case] command: &str, #[case] expected_ok: bool) {
        let mut io = SandboxedIo::new(
            MemIo::default()
                .with_command_response("echo", &["hi".to_string()], "hi")
                .with_command_response("rm", &["hi".to_string()], "hi"),
        );
        if let Some(commands) = allow {
            io = io.allow_run(Some(commands));
        }

        assert_eq!(io.execute(command, &["hi".to_string()]).is_ok(), expected_ok);
    }

    #[test]
    fn test_is_run_allowed_true_for_restricted_allowlist() {
        let io = SandboxedIo::new(MemIo::default()).allow_run(Some(vec!["echo".to_string()]));
        assert!(io.is_run_allowed());
    }

    #[test]
    fn test_allow_all_grants_every_permission() {
        let io = SandboxedIo::new(MemIo::default()).allow_all();
        assert!(io.is_read_allowed());
        assert!(io.is_write_allowed());
        assert!(io.is_net_allowed());
        assert!(io.is_run_allowed());
        assert!(io.is_env_allowed());
    }
}
