use std::path::{Component, Path, PathBuf};

/// Filesystem access granted for one capability (read or write) of
/// [`SandboxedIo`](super::SandboxedIo).
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
    pub(super) fn permits(&self, path: &Path) -> bool {
        match self {
            PathAccess::Denied => false,
            PathAccess::Allowed => true,
            PathAccess::AllowedPaths(allowed) => {
                let target = normalize(path);
                allowed.iter().any(|root| target.starts_with(normalize(root)))
            }
        }
    }

    pub(super) fn is_denied(&self) -> bool {
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
