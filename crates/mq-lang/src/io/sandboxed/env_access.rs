/// Environment-variable access granted to [`SandboxedIo`](super::SandboxedIo); same shape as
/// [`PathAccess`](super::PathAccess) but keyed by variable name instead of filesystem path
/// (e.g. `mq-run`'s `--allow-env`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum EnvAccess {
    /// No environment variable access at all (fail-safe default).
    #[default]
    Denied,
    /// Unrestricted access, matching a bare `--allow-env`.
    Allowed,
    /// Access restricted to the given variable names.
    AllowedNames(Vec<String>),
}

impl From<bool> for EnvAccess {
    fn from(allow: bool) -> Self {
        if allow { EnvAccess::Allowed } else { EnvAccess::Denied }
    }
}

impl From<Vec<String>> for EnvAccess {
    fn from(names: Vec<String>) -> Self {
        if names.is_empty() {
            EnvAccess::Allowed
        } else {
            EnvAccess::AllowedNames(names)
        }
    }
}

impl From<Option<Vec<String>>> for EnvAccess {
    fn from(names: Option<Vec<String>>) -> Self {
        match names {
            None => EnvAccess::Denied,
            Some(names) => names.into(),
        }
    }
}

impl EnvAccess {
    pub(super) fn permits(&self, name: &str) -> bool {
        match self {
            EnvAccess::Denied => false,
            EnvAccess::Allowed => true,
            EnvAccess::AllowedNames(allowed) => allowed.iter().any(|allowed_name| allowed_name == name),
        }
    }

    pub(super) fn is_denied(&self) -> bool {
        matches!(self, EnvAccess::Denied)
    }
}
