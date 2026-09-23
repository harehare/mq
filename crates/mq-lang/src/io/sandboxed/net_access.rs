use crate::io::url_allowlist;

/// Network access granted to [`SandboxedIo`](super::SandboxedIo); same shape as
/// [`PathAccess`](super::PathAccess)/[`EnvAccess`](super::EnvAccess) but keyed by domain
/// (e.g. `mq-run`'s `--allow-net`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum NetAccess {
    /// No network access at all (fail-safe default).
    #[default]
    Denied,
    /// Unrestricted access, matching a bare `--allow-net`.
    Allowed,
    /// Access restricted to the given domains (and any path under them).
    AllowedDomains(Vec<String>),
}

impl From<bool> for NetAccess {
    fn from(allow: bool) -> Self {
        if allow { NetAccess::Allowed } else { NetAccess::Denied }
    }
}

impl From<Vec<String>> for NetAccess {
    fn from(domains: Vec<String>) -> Self {
        if domains.is_empty() {
            NetAccess::Allowed
        } else {
            NetAccess::AllowedDomains(domains)
        }
    }
}

impl From<Option<Vec<String>>> for NetAccess {
    fn from(domains: Option<Vec<String>>) -> Self {
        match domains {
            None => NetAccess::Denied,
            Some(domains) => domains.into(),
        }
    }
}

impl NetAccess {
    pub(super) fn permits(&self, url: &str) -> bool {
        match self {
            NetAccess::Denied => false,
            NetAccess::Allowed => true,
            NetAccess::AllowedDomains(allowed) => allowed.iter().any(|domain| url_allowlist::matches(url, domain)),
        }
    }

    pub(super) fn is_denied(&self) -> bool {
        matches!(self, NetAccess::Denied)
    }
}
