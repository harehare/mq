//! URL-aware matching for network and HTTP-import allowlists.

use url::Url;

/// Matches an HTTP(S) request URL against an allowlist entry containing a host
/// and, optionally, a path prefix. An entry without a port permits any port.
pub(crate) fn matches(url: &str, allowed: &str) -> bool {
    if url.contains('\\')
        || allowed.contains('\\')
        || url.bytes().any(|byte| byte.is_ascii_control())
        || allowed.bytes().any(|byte| byte.is_ascii_control())
        || url.trim() != url
        || allowed.trim() != allowed
    {
        return false;
    }
    let Ok(target) = Url::parse(url) else {
        return false;
    };
    if !matches!(target.scheme(), "http" | "https")
        || target.host().is_none()
        || !target.username().is_empty()
        || target.password().is_some()
    {
        return false;
    }

    let entry = allowed
        .strip_prefix("https://")
        .or_else(|| allowed.strip_prefix("http://"))
        .unwrap_or(allowed);
    let raw_path = entry.split_once('/').map(|(_, path)| path).unwrap_or("");
    if raw_path
        .split('/')
        .any(|segment| matches!(segment, "." | "..") || segment.contains('%'))
    {
        return false;
    }
    let Ok(grant) = Url::parse(&format!("{}://{entry}", target.scheme())) else {
        return false;
    };
    if grant.host().is_none()
        || !grant.username().is_empty()
        || grant.password().is_some()
        || grant.query().is_some()
        || grant.fragment().is_some()
        || target.host() != grant.host()
    {
        return false;
    }

    // A port in the grant narrows the permission; an omitted port permits any.
    let authority = entry.split('/').next().unwrap_or(entry);
    let has_port = authority
        .rsplit_once(':')
        .is_some_and(|(_, port)| port.parse::<u16>().is_ok());
    if has_port && target.port_or_known_default() != grant.port_or_known_default() {
        return false;
    }

    let prefix = grant.path().trim_end_matches('/');
    // A path-scoped grant cannot safely interpret an encoded separator the same
    // way every remote server does. Reject encoded paths instead of risking a
    // second decode that escapes the allowed repository/directory.
    if !prefix.is_empty() && target.path().contains('%') {
        return false;
    }
    prefix.is_empty()
        || target.path() == prefix
        || target
            .path()
            .strip_prefix(prefix)
            .is_some_and(|rest| rest.starts_with('/'))
}

#[cfg(test)]
mod tests {
    use super::matches;
    use rstest::rstest;

    #[rstest]
    #[case("https://example.invalid/path", "example.invalid", true)]
    #[case("https://EXAMPLE.INVALID/path", "example.invalid", true)]
    #[case("https://example.invalid:8443/path", "example.invalid", true)]
    #[case("https://example.invalid:8443/path", "example.invalid:443", false)]
    #[case("https://example.invalid:8443/path", "example.invalid:8443", true)]
    #[case("https://example.invalid/repo/file", "example.invalid/repo", true)]
    #[case("https://example.invalid/repository/file", "example.invalid/repo", false)]
    #[case("https://example.invalid/repo/../secret", "example.invalid/repo", false)]
    #[case("https://example.invalid/repo/%2e%2e/secret", "example.invalid/repo", false)]
    #[case("https://example.invalid/repo/%252fsecret", "example.invalid/repo", false)]
    #[case("https://example.invalid/secret", "example.invalid/repo/..", false)]
    #[case("https://example.invalid:443@evil.invalid/", "example.invalid", false)]
    #[case("https://example.invalid:443@evil.invalid/", "example.invalid:443", false)]
    #[case("https://example.invalid@evil.invalid/", "example.invalid", false)]
    #[case("https://evil.invalid/", "example.invalid@evil.invalid", false)]
    #[case("https://example.invalid.evil.invalid/", "example.invalid", false)]
    #[case("https://example.invalid\\@evil.invalid/", "example.invalid", false)]
    #[case("https://example.invalid\n@evil.invalid/", "example.invalid", false)]
    fn url_allowlist_matches_authority_and_path(#[case] url: &str, #[case] allowed: &str, #[case] expected: bool) {
        assert_eq!(matches(url, allowed), expected);
    }
}
