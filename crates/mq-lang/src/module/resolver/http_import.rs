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
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case("https://example.com/foo.mq", true)]
    #[case("http://example.com/foo.mq", true)]
    #[case("ftp://example.com/foo.mq", false)]
    #[case("example.com/foo.mq", false)]
    #[case("csv", false)]
    fn test_is_remote_url(#[case] url: &str, #[case] expected: bool) {
        assert_eq!(is_remote_url(url), expected);
    }

    #[rstest]
    #[case("github.com/owner/repo", true)]
    #[case("https://github.com/owner/repo", true)]
    #[case("http://github.com/owner/repo", true)]
    #[case("https://example.com/foo.mq", false)]
    #[case("csv", false)]
    fn test_is_github_url(#[case] input: &str, #[case] expected: bool) {
        assert_eq!(is_github_url(input), expected);
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
    fn test_github_to_raw_url(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(github_to_raw_url(input).unwrap(), expected);
    }

    #[rstest]
    #[case("github.com/alice/mymod", "mymod")]
    #[case("github.com/alice/mymod.mq@v1.0", "mymod")]
    #[case("https://example.com/path/foo.mq", "foo")]
    fn test_extract_module_name(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(extract_module_name(input), expected);
    }

    #[rstest]
    #[case("github.com/alice/myrepo", "raw.githubusercontent.com/alice/myrepo")]
    #[case("https://github.com/alice/myrepo", "raw.githubusercontent.com/alice/myrepo")]
    #[case("example.com", "example.com")]
    fn test_normalize_allowed_domain(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(normalize_allowed_domain(input), expected);
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
    fn test_is_allowed_url(#[case] allowed: Vec<String>, #[case] url: &str, #[case] expected: bool) {
        assert_eq!(is_allowed_url(url, &allowed), expected);
    }

    #[rstest]
    #[case("https://raw.githubusercontent.com/alice/mymod/v0.1.0/mymod.mq", true)]
    #[case("https://raw.githubusercontent.com/alice/mymod/HEAD/mymod.mq", false)]
    #[case("https://raw.githubusercontent.com/alice/mymod/main/mymod.mq", false)]
    #[case("https://example.com/foo.mq", false)]
    fn test_is_versioned_url(#[case] url: &str, #[case] expected: bool) {
        assert_eq!(is_versioned_url(url), expected);
    }
}
