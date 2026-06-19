//! Pure URL utility functions shared between the CLI HTTP resolver and the WASM resolver.
//!
//! These functions have no I/O dependencies and are not gated behind the `http-import` feature.

/// Default domain that is always permitted without an explicit allowlist entry.
pub const DEFAULT_ALLOWED_DOMAIN: &str = "raw.githubusercontent.com/harehare";

/// Returns `true` if `url` has an `http://` or `https://` scheme.
pub fn is_remote_url(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}

/// Returns `true` if `input` is a GitHub shorthand or full GitHub URL.
///
/// Recognized forms (with or without `https://` prefix):
/// - `github.com/{owner}/{path}[@{version}]`
pub fn is_github_url(input: &str) -> bool {
    let s = input
        .strip_prefix("https://")
        .or_else(|| input.strip_prefix("http://"))
        .unwrap_or(input);
    s.starts_with("github.com/")
}

/// Converts a GitHub shorthand into a `raw.githubusercontent.com` fetch URL.
///
/// # Format
/// `[https://]github.com/{owner}/{path}[@{version}]`
///
/// Where `{path}` is one of:
/// - `{repo}` → fetches `{repo}.mq` from the repo root at HEAD
/// - `{repo.mq}` → fetches `{repo.mq}` from the repo named `{repo.mq}` at HEAD
/// - `{repo}/{subpath}` → fetches `{subpath}` from the given repo
pub fn github_to_raw_url(input: &str) -> Option<String> {
    let without_scheme = input
        .strip_prefix("https://")
        .or_else(|| input.strip_prefix("http://"))
        .unwrap_or(input);

    let rest = without_scheme.strip_prefix("github.com/")?;

    let (path_part, version) = match rest.rfind('@') {
        Some(pos) => (&rest[..pos], &rest[pos + 1..]),
        None => (rest, "HEAD"),
    };

    let components: Vec<&str> = path_part.splitn(3, '/').collect();

    let (owner, repo, file) = match components.as_slice() {
        [owner, name] => {
            let repo = *name;
            let file = if name.ends_with(".mq") {
                name.to_string()
            } else {
                format!("{}.mq", name)
            };
            (owner.to_string(), repo.to_string(), file)
        }
        [owner, repo, subpath] => (owner.to_string(), repo.to_string(), subpath.to_string()),
        _ => return None,
    };

    Some(format!(
        "https://raw.githubusercontent.com/{}/{}/{}/{}",
        owner, repo, version, file
    ))
}

/// Returns `true` if `url` is pinned to a specific immutable version tag.
///
/// For `raw.githubusercontent.com` URLs the ref segment (the third path component after
/// `{owner}/{repo}/`) is checked: `HEAD`, `main`, and `master` are mutable; everything
/// else (e.g. `v0.1.0`) is treated as versioned/immutable.
///
/// All non-GitHub HTTP URLs are considered mutable.
pub fn is_versioned_url(url: &str) -> bool {
    const MUTABLE_REFS: &[&str] = &["HEAD", "main", "master"];
    let path = url
        .strip_prefix("https://raw.githubusercontent.com/")
        .or_else(|| url.strip_prefix("http://raw.githubusercontent.com/"));
    match path {
        Some(rest) => {
            let ref_segment = rest.split('/').nth(2).unwrap_or("HEAD");
            !MUTABLE_REFS.contains(&ref_segment)
        }
        None => false,
    }
}

/// Extracts a short module name from an HTTP URL or GitHub shorthand.
///
/// Strips the URL scheme, domain, and path prefix, then removes any `@version`
/// suffix and the `.mq` file extension from the last path segment.
pub fn extract_module_name(module_path: &str) -> &str {
    let path = module_path
        .strip_prefix("https://")
        .or_else(|| module_path.strip_prefix("http://"))
        .unwrap_or(module_path);

    let without_version = match path.rfind('@') {
        Some(pos) => &path[..pos],
        None => path,
    };

    let last_segment = without_version.rsplit('/').next().unwrap_or(without_version);
    last_segment.strip_suffix(".mq").unwrap_or(last_segment)
}

/// Normalizes a user-supplied allowed-domain entry.
///
/// `github.com/{path}` (with or without `https://`/`http://` prefix) is expanded to
/// `raw.githubusercontent.com/{path}` so that users can write
/// `--allowed-domain github.com/alice/myrepo` instead of the full raw content URL.
/// The scheme prefix is always stripped before storing.
pub fn normalize_allowed_domain(domain: &str) -> String {
    let without_scheme = domain
        .strip_prefix("https://")
        .or_else(|| domain.strip_prefix("http://"))
        .unwrap_or(domain);

    if let Some(rest) = without_scheme.strip_prefix("github.com/") {
        format!("raw.githubusercontent.com/{}", rest)
    } else {
        without_scheme.to_string()
    }
}

/// Returns `true` if `url`'s host/path matches `domain` as a strict prefix.
///
/// The match requires that after the prefix the next character is `/`, `?`, `#`, `:`, or
/// end of string — preventing `example.com.evil.com` from matching `example.com`.
pub fn prefix_matches(url_without_scheme: &str, domain: &str) -> bool {
    let rest = match url_without_scheme.strip_prefix(domain) {
        Some(r) => r,
        None => return false,
    };
    rest.is_empty() || rest.starts_with('/') || rest.starts_with('?') || rest.starts_with('#') || rest.starts_with(':')
}

/// Returns `true` if `ip` is publicly routable.
///
/// Used to reject DNS results that point at loopback, private, link-local
/// (which includes the `169.254.169.254` cloud metadata endpoint), multicast,
/// unspecified, or documentation/benchmark ranges. This guards against SSRF via
/// DNS rebinding: an allowlisted domain's name could otherwise resolve to an
/// internal address at connect time.
pub fn is_global_ip(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => {
            !(v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_multicast()
                || v4.is_broadcast()
                || v4.is_unspecified()
                || v4.is_documentation())
        }
        std::net::IpAddr::V6(v6) => {
            if let Some(mapped) = v6.to_ipv4_mapped() {
                return is_global_ip(std::net::IpAddr::V4(mapped));
            }
            !(v6.is_loopback()
                || v6.is_multicast()
                || v6.is_unspecified()
                || v6.is_unique_local()
                || v6.is_unicast_link_local())
        }
    }
}

/// Returns `true` if `url` is permitted given `allowed_domains`.
///
/// `DEFAULT_ALLOWED_DOMAIN` is always allowed regardless of `allowed_domains`.
/// An empty `allowed_domains` slice restricts access to the default domain only.
pub fn is_allowed_url(url: &str, allowed_domains: &[String]) -> bool {
    let url_without_scheme = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);

    if prefix_matches(url_without_scheme, DEFAULT_ALLOWED_DOMAIN) {
        return true;
    }

    allowed_domains
        .iter()
        .any(|domain| prefix_matches(url_without_scheme, domain.as_str()))
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case("https://example.com/foo.mq", true)]
    #[case("http://example.com/foo.mq", true)]
    #[case("ftp://example.com/foo.mq", false)]
    #[case("example.com/foo.mq", false)]
    #[case("csv", false)]
    #[case("", false)]
    #[case("https://", true)]
    #[case("http://", true)]
    fn test_is_remote_url(#[case] url: &str, #[case] expected: bool) {
        assert_eq!(is_remote_url(url), expected);
    }

    proptest! {
        #[test]
        fn prop_is_remote_url_https_prefix(path in "[a-z0-9/._-]{1,30}") {
            let url = format!("https://{}", path);
            prop_assert!(is_remote_url(&url));
        }

        #[test]
        fn prop_is_remote_url_http_prefix(path in "[a-z0-9/._-]{1,30}") {
            let url = format!("http://{}", path);
            prop_assert!(is_remote_url(&url));
        }

        #[test]
        fn prop_is_remote_url_no_scheme_is_false(s in "[a-zA-Z0-9._/-]{1,40}") {
            // Strings without http(s):// prefix must not be treated as remote.
            prop_assume!(!s.starts_with("https://") && !s.starts_with("http://"));
            prop_assert!(!is_remote_url(&s));
        }
    }

    #[rstest]
    #[case("github.com/owner/repo", true)]
    #[case("https://github.com/owner/repo", true)]
    #[case("http://github.com/owner/repo", true)]
    #[case("https://example.com/foo.mq", false)]
    #[case("csv", false)]
    #[case("", false)]
    #[case("github.com/", true)]
    fn test_is_github_url(#[case] input: &str, #[case] expected: bool) {
        assert_eq!(is_github_url(input), expected);
    }

    proptest! {
        #[test]
        fn prop_is_github_url_bare_prefix(path in "[a-z0-9/_-]{1,30}") {
            let url = format!("github.com/{}", path);
            prop_assert!(is_github_url(&url));
        }

        #[test]
        fn prop_is_github_url_https_prefix(path in "[a-z0-9/_-]{1,30}") {
            let url = format!("https://github.com/{}", path);
            prop_assert!(is_github_url(&url));
        }

        #[test]
        fn prop_not_github_url_random(s in "[a-zA-Z0-9._/-]{1,40}") {
            prop_assume!(
                !s.starts_with("github.com/")
                    && !s.starts_with("https://github.com/")
                    && !s.starts_with("http://github.com/")
            );
            prop_assert!(!is_github_url(&s));
        }
    }

    #[rstest]
    #[case(
        "github.com/harehare/lisp",
        "https://raw.githubusercontent.com/harehare/lisp/HEAD/lisp.mq"
    )]
    #[case(
        "github.com/harehare/lisp@v0.1.0",
        "https://raw.githubusercontent.com/harehare/lisp/v0.1.0/lisp.mq"
    )]
    #[case(
        "github.com/harehare/repo/lib/utils.mq@v2.0",
        "https://raw.githubusercontent.com/harehare/repo/v2.0/lib/utils.mq"
    )]
    #[case(
        "https://github.com/alice/mod",
        "https://raw.githubusercontent.com/alice/mod/HEAD/mod.mq"
    )]
    #[case(
        "http://github.com/alice/mod",
        "https://raw.githubusercontent.com/alice/mod/HEAD/mod.mq"
    )]
    #[case(
        "github.com/alice/mod.mq",
        "https://raw.githubusercontent.com/alice/mod.mq/HEAD/mod.mq"
    )]
    fn test_github_to_raw_url(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(github_to_raw_url(input).unwrap(), expected);
    }

    #[rstest]
    // Single component after github.com/ — not enough to form owner/repo
    #[case("github.com/owner")]
    fn test_github_to_raw_url_returns_none(#[case] input: &str) {
        assert!(github_to_raw_url(input).is_none());
    }

    proptest! {
        #[test]
        fn prop_github_to_raw_url_always_https(
            owner in "[a-z][a-z0-9-]{0,10}",
            repo  in "[a-z][a-z0-9-]{0,10}",
        ) {
            let input = format!("github.com/{}/{}", owner, repo);
            let url = github_to_raw_url(&input).unwrap();
            prop_assert!(url.starts_with("https://raw.githubusercontent.com/"));
        }

        #[test]
        fn prop_github_to_raw_url_versioned_contains_version(
            owner   in "[a-z][a-z0-9-]{0,10}",
            repo    in "[a-z][a-z0-9-]{0,10}",
            version in "v[0-9]\\.[0-9]\\.[0-9]",
        ) {
            let input = format!("github.com/{}/{}@{}", owner, repo, version);
            let url = github_to_raw_url(&input).unwrap();
            prop_assert!(url.contains(&version));
        }
    }

    #[rstest]
    #[case("github.com/alice/mymod", "mymod")]
    #[case("github.com/alice/mymod.mq@v1.0", "mymod")]
    #[case("https://example.com/path/foo.mq", "foo")]
    #[case("https://example.com/bar", "bar")]
    #[case("https://example.com/a/b/c.mq@v2", "c")]
    fn test_extract_module_name(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(extract_module_name(input), expected);
    }

    proptest! {
        #[test]
        fn prop_extract_module_name_no_mq_suffix(
            owner in "[a-z][a-z0-9-]{0,10}",
            repo  in "[a-z][a-z0-9-]{0,10}",
        ) {
            let input = format!("github.com/{}/{}", owner, repo);
            let name = extract_module_name(&input);
            prop_assert!(!name.ends_with(".mq"));
        }

        #[test]
        fn prop_extract_module_name_no_at_suffix(
            owner   in "[a-z][a-z0-9-]{0,10}",
            repo    in "[a-z][a-z0-9-]{0,10}",
            version in "v[0-9]\\.[0-9]",
        ) {
            let input = format!("github.com/{}/{}@{}", owner, repo, version);
            let name = extract_module_name(&input);
            prop_assert!(!name.contains('@'));
        }
    }

    #[rstest]
    #[case("github.com/alice/myrepo", "raw.githubusercontent.com/alice/myrepo")]
    #[case("https://github.com/alice/myrepo", "raw.githubusercontent.com/alice/myrepo")]
    #[case("http://github.com/alice/myrepo", "raw.githubusercontent.com/alice/myrepo")]
    #[case("example.com", "example.com")]
    #[case("https://example.com", "example.com")]
    #[case("raw.githubusercontent.com/alice/repo", "raw.githubusercontent.com/alice/repo")]
    fn test_normalize_allowed_domain(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(normalize_allowed_domain(input), expected);
    }

    proptest! {
        #[test]
        fn prop_normalize_allowed_domain_no_scheme(domain in "[a-z0-9._/-]{2,30}") {
            let normalized = normalize_allowed_domain(&domain);
            prop_assert!(!normalized.starts_with("https://"));
            prop_assert!(!normalized.starts_with("http://"));
        }

        #[test]
        fn prop_normalize_strips_https_scheme(path in "[a-z0-9._/-]{2,30}") {
            let input = format!("https://{}", path);
            let normalized = normalize_allowed_domain(&input);
            prop_assert!(!normalized.starts_with("https://"));
        }
    }

    #[rstest]
    // default domain always allowed
    #[case(vec![], "https://raw.githubusercontent.com/harehare/lisp/HEAD/lisp.mq", true)]
    // non-default domain blocked by empty list
    #[case(vec![], "https://example.com/foo.mq", false)]
    // user-specified domain allowed
    #[case(vec!["example.com".to_string()], "https://example.com/foo.mq", true)]
    #[case(vec!["example.com".to_string()], "https://other.com/foo.mq", false)]
    // prefix-bypass prevention
    #[case(vec!["example.com".to_string()], "https://example.com.evil.com/foo.mq", false)]
    // multiple allowed domains
    #[case(vec!["a.com".to_string(), "b.com".to_string()], "https://a.com/x.mq", true)]
    #[case(vec!["a.com".to_string(), "b.com".to_string()], "https://b.com/x.mq", true)]
    #[case(vec!["a.com".to_string(), "b.com".to_string()], "https://c.com/x.mq", false)]
    fn test_is_allowed_url(#[case] allowed: Vec<String>, #[case] url: &str, #[case] expected: bool) {
        assert_eq!(is_allowed_url(url, &allowed), expected);
    }

    proptest! {
        #[test]
        fn prop_default_domain_always_allowed(path in "[a-z0-9/_.-]{1,40}") {
            let url = format!("https://raw.githubusercontent.com/harehare/{}", path);
            // Always allowed regardless of the allowlist.
            prop_assert!(is_allowed_url(&url, &[]));
        }

        #[test]
        fn prop_arbitrary_domain_blocked_by_empty_allowlist(
            host in "[a-z][a-z0-9-]{2,10}\\.[a-z]{2,4}",
            path in "[a-z0-9/_.-]{1,20}",
        ) {
            prop_assume!(host != "raw.githubusercontent.com");
            let url = format!("https://{}/{}", host, path);
            prop_assert!(!is_allowed_url(&url, &[]));
        }

        #[test]
        fn prop_own_domain_allowed_when_listed(
            host in "[a-z][a-z0-9-]{2,10}\\.[a-z]{2,4}",
            path in "[a-z0-9/_.-]{1,20}",
        ) {
            let url = format!("https://{}/{}", host, path);
            let allowed = vec![host.clone()];
            prop_assert!(is_allowed_url(&url, &allowed));
        }

        #[test]
        fn prop_prefix_attack_blocked(
            host in "[a-z][a-z0-9-]{2,10}\\.[a-z]{2,4}",
            path in "[a-z0-9/_.-]{1,20}",
        ) {
            // "example.com.evil.com" must not match "example.com".
            let allowed = vec![host.clone()];
            let attacker_url = format!("https://{}.evil.com/{}", host, path);
            prop_assert!(!is_allowed_url(&attacker_url, &allowed));
        }
    }

    #[rstest]
    #[case("127.0.0.1", false)]
    #[case("10.0.0.1", false)]
    #[case("172.16.0.1", false)]
    #[case("192.168.1.1", false)]
    #[case("169.254.169.254", false)] // cloud metadata endpoint
    #[case("0.0.0.0", false)]
    #[case("224.0.0.1", false)]
    #[case("255.255.255.255", false)]
    #[case("8.8.8.8", true)]
    #[case("93.184.216.34", true)]
    #[case("::1", false)]
    #[case("fc00::1", false)]
    #[case("fe80::1", false)]
    #[case("::ffff:127.0.0.1", false)] // IPv4-mapped loopback bypass
    #[case("::ffff:8.8.8.8", true)]
    #[case("2001:4860:4860::8888", true)]
    fn test_is_global_ip(#[case] ip: &str, #[case] expected: bool) {
        assert_eq!(is_global_ip(ip.parse().unwrap()), expected);
    }

    #[rstest]
    #[case("https://raw.githubusercontent.com/alice/mymod/v0.1.0/mymod.mq", true)]
    #[case("https://raw.githubusercontent.com/alice/mymod/HEAD/mymod.mq", false)]
    #[case("https://raw.githubusercontent.com/alice/mymod/main/mymod.mq", false)]
    #[case("https://raw.githubusercontent.com/alice/mymod/master/mymod.mq", false)]
    #[case("https://example.com/foo.mq", false)]
    #[case("https://raw.githubusercontent.com/a/b/feature-branch/f.mq", true)]
    fn test_is_versioned_url(#[case] url: &str, #[case] expected: bool) {
        assert_eq!(is_versioned_url(url), expected);
    }

    proptest! {
        #[test]
        fn prop_versioned_tag_is_immutable(
            owner   in "[a-z][a-z0-9-]{0,10}",
            repo    in "[a-z][a-z0-9-]{0,10}",
            version in "v[0-9]\\.[0-9]\\.[0-9]",
        ) {
            let url = format!(
                "https://raw.githubusercontent.com/{}/{}/{}/mod.mq",
                owner, repo, version
            );
            prop_assert!(is_versioned_url(&url));
        }

        #[test]
        fn prop_mutable_refs_are_not_versioned(
            owner in "[a-z][a-z0-9-]{0,10}",
            repo  in "[a-z][a-z0-9-]{0,10}",
            ref_  in prop::sample::select(vec!["HEAD", "main", "master"]),
        ) {
            let url = format!(
                "https://raw.githubusercontent.com/{}/{}/{}/mod.mq",
                owner, repo, ref_
            );
            prop_assert!(!is_versioned_url(&url));
        }
    }

    #[rstest]
    #[case("example.com/foo", "example.com", true)]
    #[case("example.com?q=1", "example.com", true)]
    #[case("example.com#anchor", "example.com", true)]
    #[case("example.com:8080/foo", "example.com", true)]
    #[case("example.com", "example.com", true)]
    #[case("example.com.evil.com/foo", "example.com", false)]
    #[case("other.com/foo", "example.com", false)]
    fn test_prefix_matches(#[case] url_without_scheme: &str, #[case] domain: &str, #[case] expected: bool) {
        assert_eq!(prefix_matches(url_without_scheme, domain), expected);
    }
}
