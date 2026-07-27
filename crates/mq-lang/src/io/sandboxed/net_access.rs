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
            NetAccess::AllowedDomains(allowed) => {
                let without_scheme = url
                    .strip_prefix("https://")
                    .or_else(|| url.strip_prefix("http://"))
                    .unwrap_or(url);
                allowed.iter().any(|domain| prefix_matches(without_scheme, domain))
            }
        }
    }

    pub(super) fn is_denied(&self) -> bool {
        matches!(self, NetAccess::Denied)
    }
}

/// Returns `true` if `url_without_scheme`'s host/path matches `domain` as a strict prefix.
///
/// The match requires that after the prefix the next character is `/`, `?`, `#`, `:`, or
/// end of string — preventing `example.com.evil.com` from matching `example.com`.
fn prefix_matches(url_without_scheme: &str, domain: &str) -> bool {
    let rest = match url_without_scheme.strip_prefix(domain) {
        Some(r) => r,
        None => return false,
    };
    rest.is_empty() || rest.starts_with('/') || rest.starts_with('?') || rest.starts_with('#') || rest.starts_with(':')
}
