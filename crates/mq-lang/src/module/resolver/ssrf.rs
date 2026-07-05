//! SSRF (server-side request forgery) defenses shared by every outbound HTTP path in this
//! crate: HTTP module imports (`http_resolver.rs`) and the `http` builtin
//! (`eval/builtin/http.rs`).
//!
//! [`is_global_ip`] has no I/O dependencies. [`SsrfSafeResolver`]/[`ssrf_safe_agent`] additionally
//! require a concrete `ureq` transport, so they're gated behind `http-import-ureq`.

/// Returns `true` if `url` uses the `https://` scheme.
pub fn is_https(url: &str) -> bool {
    url.starts_with("https://")
}

/// Returns `true` if `ip` is publicly routable.
///
/// Rejects loopback, private, link-local (which includes the `169.254.169.254` cloud metadata
/// endpoint), multicast, unspecified, and documentation/benchmark ranges (unique-local as well
/// for IPv6). Used to filter DNS resolution results so an allowlisted hostname can't be pointed
/// (directly, or via a later DNS rebind) at an internal address.
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

/// DNS resolver that drops any resolved address that isn't publicly routable.
///
/// Domain allowlisting only checks the *name* in the URL; without this, an
/// allowlisted domain could still resolve (at connect time, or via a later
/// DNS rebind) to a loopback/private/link-local address and reach internal
/// services. Filtering at the resolver level pins the connection to the
/// addresses validated here, so a later re-resolution can't smuggle in an
/// internal address.
#[cfg(feature = "http-import-ureq")]
#[derive(Debug, Default)]
pub(crate) struct SsrfSafeResolver(ureq::unversioned::resolver::DefaultResolver);

#[cfg(feature = "http-import-ureq")]
impl ureq::unversioned::resolver::Resolver for SsrfSafeResolver {
    fn resolve(
        &self,
        uri: &ureq::http::Uri,
        config: &ureq::config::Config,
        timeout: ureq::unversioned::transport::NextTimeout,
    ) -> Result<ureq::unversioned::resolver::ResolvedSocketAddrs, ureq::Error> {
        let resolved = self.0.resolve(uri, config, timeout)?;

        let mut safe = self.empty();
        for addr in resolved.iter() {
            if is_global_ip(addr.ip()) {
                safe.push(*addr);
            }
        }

        if safe.is_empty() {
            Err(ureq::Error::HostNotFound)
        } else {
            Ok(safe)
        }
    }
}

/// Builds a `ureq::Agent` hardened against SSRF: bounds every request to `timeout`, optionally
/// restricts to `https://` (`https_only`), disables automatic redirects (so a redirect to an
/// internal address can't bypass allowlist/IP checks), and resolves DNS through
/// [`SsrfSafeResolver`] so only publicly routable addresses are ever connected to.
///
/// Shared by the HTTP module-import fetcher and the `http` builtin.
#[cfg(feature = "http-import-ureq")]
pub(crate) fn ssrf_safe_agent(timeout: std::time::Duration, https_only: bool) -> ureq::Agent {
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .https_only(https_only)
        .max_redirects(0)
        .build();
    ureq::Agent::with_parts(
        config,
        ureq::unversioned::transport::DefaultConnector::default(),
        SsrfSafeResolver::default(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

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
}
